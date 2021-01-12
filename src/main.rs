// SPDX-License-Identifier: Apache-2.0

//! The Enarx Keep runtime binary.
//!
//! It can be used to run a Wasm file with given command-line
//! arguments and environment variables.
//!
//! Now requires compilation with cargo nightly (`cargo +nightly build`)
//!
//! ## Example invocation
//!
//! ```console
//! $ wat2wasm fixtures/return_1.wat
//! $ RUST_LOG=enarx_wasmldr=info RUST_BACKTRACE=1 cargo run return_1.wasm
//!     Finished dev [unoptimized + debuginfo] target(s) in 0.07s
//!      Running `target/x86_64-unknown-linux-musl/debug/enarx-wasmldr target/x86_64-unknown-linux-musl/debug/build/enarx-wasmldr-c374d181f6abdda0/out/fixtures/return_1.wasm`
//! [2020-09-10T17:56:18Z INFO  enarx_wasmldr] got result: [
//!         I32(
//!             1,
//!         ),
//!     ]
//! ```

//!
//! On Unix platforms, the command can also read the workload from the
//! file descriptor (3):
//! ```console
//! $ RUST_LOG=enarx_wasmldr=info RUST_BACKTRACE=1 cargo run 3< return_1.wasm
//! ```
//!

//#![deny(missing_docs)]
#![deny(clippy::all)]
#![feature(asm)]
//#![feature(proc_macro_hygiene, decl_macro)]

mod attestation;
mod bundle;
mod config;
mod handle;
mod socket;
mod virtfs;
mod workload;

use koine::*;
use log::info;
use openssl::asn1::Asn1Time;
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::pkey::Private;
use openssl::rsa::*;
use serde_cbor::{de, to_vec};
use std::error::Error;
use std::fmt;
use std::net::{IpAddr, SocketAddr};
//#[cfg(unix)]
use sys_info::*;
use warp::Filter;

pub const KEY_SOURCE: &str = "generate";
#[cfg(unix)]
#[tokio::main(basic_scheduler)]
async fn main() {
    //This required when calling from Rust std::process::command.  Recorded
    // to allow debugging.
    //    let args: Vec<String> = std::env::args().skip(1).collect();
    let _args: Vec<String> = std::env::args().collect();

    //TODO - the mechanism for binding to an IP address is currently undefined.
    // It is expected that a new bridge will be created, to which this process
    //  will then bind.

    //FIXME - hard-coding for now
    //    let listen_address: &str = "127.0.0.1";
    //let listen_address: &str = "192.168.1.203";
    //This is the IP address of rome.sev.lab.enarx.dev (2021-01-07)
    let listen_address: &str = "147.75.68.181";
    //    let listen_address: &str = &args[0];
    //FIXME - hard-coding for now
    let listen_port: &str = "3040";
    //    let listen_port: &str = &args[1];

    let listen_socketaddr = SocketAddr::new(
        listen_address.parse::<IpAddr>().unwrap(),
        listen_port.parse().unwrap(),
    );
    let (server_key, server_cert) = get_credentials_bytes(listen_address);

    // POST /workload
    let workload = warp::post()
        .and(warp::path("workload"))
        .and(warp::body::bytes())
        .and_then(payload_launch);

    let routes = workload;
    warp::serve(routes)
        .tls()
        .cert(&server_cert)
        .key(&server_key)
        .run(listen_socketaddr)
        .await;
}

fn create_new_runtime(recvd_data: &[u8]) -> Result<bool, String> {
    //println!("About to attempt new runtime creation");
    let _ = env_logger::try_init_from_env(env_logger::Env::default());
    //TODO - get args these from main() if required
    //    let args = std::env::args().skip(1);
    let dummy_arr: [&str; 1] = [""];
    let vars = std::env::vars();

    let result = workload::run(recvd_data, &dummy_arr, vars).expect("Failed to run workload");
    //println!("Got result (println) {:#?}", result);
    info!("got result: {:#?}", result);
    //TODO - some error checking
    Ok(true)
}

async fn payload_launch<B: warp::Buf>(bytes: B) -> Result<impl warp::Reply, warp::Rejection> {
    //println!(
    //    "payload_launch bytes.bytes().len() = {}",
    //    bytes.bytes().len()
    //);
    let wbytes: &[u8] = bytes.bytes();
    //println!("payload_launch received {} bytes", wbytes.len());
    let workload_bytes = wbytes.as_ref();

    //deserialise the Vector into a Payload (and handle errors)
    let workload: Workload;
    match de::from_slice(&workload_bytes) {
        Ok(wl) => {
            workload = wl;

            //println!("Received a workload: {}", workload.human_readable_info);

            //Exit after completion
            std::process::exit(match create_new_runtime(&workload.wasm_binary) {
                Ok(_) => {
                    //println!("Success - exiting");
                    0
                }
                Err(err) => {
                    eprintln!("error: {:?}", err);
                    1
                }
            });

            //TODO - does this code need to be here?
            #[allow(unreachable_code)]
            {
                let comms_complete = CommsComplete::Success;
                let cbor_reply_body: Vec<u8> = to_vec(&comms_complete).unwrap();
                //let cbor_reply: CborReply = CborReply {
                //    msg: cbor_reply_body,
                //};
                //Ok(cbor_reply)
                Ok(cbor_reply_body)
            }
        }
        Err(_) => {
            println!("Payload parsing problem");
            let cbore = LocalCborErr::new("Payload parsing problem");
            Err(warp::reject::custom(cbore))
        }
    }
}

fn get_credentials_bytes(listen_addr: &str) -> (Vec<u8>, Vec<u8>) {
    let (key, cert) = match KEY_SOURCE {
        "generate" => (generate_credentials(&listen_addr)),
        //no match!
        _ => panic!("No match for credentials source"),
    };
    (key, cert)
}

fn retrieve_existing_key() -> Option<Rsa<Private>> {
    //This function retrieves an existing key from the pre-launch
    // attestation in the case of AMD SEV
    let input_bytes: &[u8] = &Vec::new();
    //let mut output_bytes = &Vec::new();
    let mut output_bytes = vec![0];
    let expected_key_length: usize = match attestation::attest(&input_bytes, &mut output_bytes) {
        Ok(attestation) => {
            let expected_key_length = match attestation {
                attestation::Attestation::Sev(expected_key_length) => expected_key_length,
                attestation::Attestation::Sgx(_) => 0,
                attestation::Attestation::None => 0,
            };
            expected_key_length
        }
        Err(_) => 0,
    };
    if expected_key_length > 0 {
        //this cast makes sense as the longest key that can be created has length u8
        let ekl_as_u8: u8 = expected_key_length as u8;
        let mut key_bytes = vec![0, ekl_as_u8];
        let attempted_attestation_result =
            attestation::attest(&input_bytes, &mut key_bytes).unwrap();
        //TODO - error checking
        let key_result = openssl::rsa::Rsa::private_key_from_der(&key_bytes);
        let key: Option<Rsa<Private>> = match key_result {
            Ok(key) => Some(key),
            Err(_) => None,
        };
        key
    } else {
        None
    }
}

//TODO - this is vital code, and needs to be carefully audited!
fn generate_credentials(listen_addr: &str) -> (Vec<u8>, Vec<u8>) {
    //TODO - parameterise key_length?
    let key_length = 2048;
    let key_opt = retrieve_existing_key();
    let key: Rsa<Private> = match key_opt {
        Some(key) => key,
        None => Rsa::generate(key_length).unwrap(),
    };

    let pkey = PKey::from_rsa(key.clone()).unwrap();

    let myhostname = hostname().unwrap();
    //println!(
    //    "Create a certificate for {} ({})",
    //    &listen_addr, &myhostname
    //);

    let mut x509_name = openssl::x509::X509NameBuilder::new().unwrap();
    x509_name.append_entry_by_text("C", "GB").unwrap();
    x509_name.append_entry_by_text("O", "enarx-test").unwrap();
    //FIXME - we should use &listen-addr, but this fails
    x509_name
        .append_entry_by_text("subjectAltName", &listen_addr)
        .unwrap();
    //x509_name.append_entry_by_text("CN", &listen_addr).unwrap();
    //x509_name.append_entry_by_text("CN", "nail").unwrap();
    x509_name.append_entry_by_text("CN", &myhostname).unwrap();
    //TODO - include SGX case, where we're adding public key (?) information
    //       to this cert
    let x509_name = x509_name.build();

    let mut x509_builder = openssl::x509::X509::builder().unwrap();
    if let Err(e) = x509_builder.set_not_before(&Asn1Time::days_from_now(0).unwrap()) {
        panic!("Problem creating cert {}", e)
    }
    if let Err(e) = x509_builder.set_not_after(&Asn1Time::days_from_now(7).unwrap()) {
        panic!("Problem creating cert {}", e)
    }

    x509_builder.set_subject_name(&x509_name).unwrap();
    x509_builder.set_pubkey(&pkey).unwrap();
    x509_builder.sign(&pkey, MessageDigest::sha256()).unwrap();
    let certificate = x509_builder.build();

    (
        //TODO - move to der
        key.private_key_to_pem().unwrap(),
        certificate.to_pem().unwrap(),
    )
}

#[derive(Debug)]
struct LocalCborErr {
    details: String,
}

impl LocalCborErr {
    fn new(msg: &str) -> LocalCborErr {
        LocalCborErr {
            details: msg.to_string(),
        }
    }
}

impl fmt::Display for LocalCborErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}

impl Error for LocalCborErr {
    fn description(&self) -> &str {
        &self.details
    }
}

impl warp::reject::Reject for LocalCborErr {}
