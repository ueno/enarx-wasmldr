use std::any::Any;
use std::io;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, RwLock};
use wasi_common::wasi::types::{Advice, Fdflags, Filesize, Filestat, Filetype, Oflags, Rights};
use wasi_common::{Error, Result};
use wasi_common::{Handle, HandleRights};

pub struct SocketHandle {
    rights: RwLock<HandleRights>,
    stream: Arc<RwLock<TcpStream>>,
}

impl SocketHandle {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            rights: RwLock::new(HandleRights::from_base(
                Rights::FD_DATASYNC
                    | Rights::FD_FDSTAT_SET_FLAGS
                    | Rights::FD_READ
                    | Rights::FD_SYNC
                    | Rights::FD_WRITE
                    | Rights::FD_FILESTAT_GET
                    | Rights::POLL_FD_READWRITE,
            )),
            stream: Arc::new(RwLock::new(stream)),
        }
    }
}

impl Clone for SocketHandle {
    fn clone(&self) -> Self {
        Self {
            rights: RwLock::new(*self.rights.read().unwrap()),
            stream: self.stream.clone(),
        }
    }
}

impl Handle for SocketHandle {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn try_clone(&self) -> io::Result<Box<dyn Handle>> {
        Ok(Box::new(self.clone()))
    }

    fn get_file_type(&self) -> Filetype {
        Filetype::Unknown
    }

    fn get_rights(&self) -> HandleRights {
        *self.rights.read().unwrap()
    }

    fn set_rights(&self, rights: HandleRights) {
        *self.rights.write().unwrap() = rights;
    }

    fn advise(&self, _advice: Advice, _offset: Filesize, _len: Filesize) -> Result<()> {
        Err(Error::Spipe)
    }

    fn allocate(&self, _offset: Filesize, _len: Filesize) -> Result<()> {
        Err(Error::Spipe)
    }

    fn fdstat_set_flags(&self, _fdflags: Fdflags) -> Result<()> {
        // do nothing for now
        Ok(())
    }

    fn filestat_get(&self) -> Result<Filestat> {
        let stat = Filestat {
            dev: 0,
            ino: 0,
            nlink: 0,
            size: 0,
            atim: 0,
            ctim: 0,
            mtim: 0,
            filetype: self.get_file_type(),
        };
        Ok(stat)
    }

    fn filestat_set_size(&self, _st_size: Filesize) -> Result<()> {
        Err(Error::Spipe)
    }

    fn preadv(&self, buf: &mut [io::IoSliceMut], offset: Filesize) -> Result<usize> {
        if offset != 0 {
            return Err(Error::Spipe);
        }
        self.read_vectored(buf)
    }

    fn pwritev(&self, buf: &[io::IoSlice], offset: Filesize) -> Result<usize> {
        if offset != 0 {
            return Err(Error::Spipe);
        }
        self.write_vectored(buf)
    }

    fn seek(&self, _offset: io::SeekFrom) -> Result<Filesize> {
        Err(Error::Spipe)
    }

    fn read_vectored(&self, iovs: &mut [io::IoSliceMut]) -> Result<usize> {
        let mut stream = self.stream.write().unwrap();
        stream.read_vectored(iovs).map_err(Into::into)
    }

    fn write_vectored(&self, iovs: &[io::IoSlice]) -> Result<usize> {
        let mut stream = self.stream.write().unwrap();
        stream.write_vectored(iovs).map_err(Into::into)
    }

    fn create_directory(&self, _path: &str) -> Result<()> {
        Err(Error::Notdir)
    }

    fn openat(
        &self,
        _path: &str,
        _read: bool,
        _write: bool,
        _oflags: Oflags,
        _fd_flags: Fdflags,
    ) -> Result<Box<dyn Handle>> {
        Err(Error::Notdir)
    }

    fn link(
        &self,
        _old_path: &str,
        _new_handle: Box<dyn Handle>,
        _new_path: &str,
        _follow: bool,
    ) -> Result<()> {
        Err(Error::Notdir)
    }

    fn readlink(&self, _path: &str, _buf: &mut [u8]) -> Result<usize> {
        Err(Error::Notdir)
    }

    fn readlinkat(&self, _path: &str) -> Result<String> {
        Err(Error::Notdir)
    }

    fn rename(&self, _old_path: &str, _new_handle: Box<dyn Handle>, _new_path: &str) -> Result<()> {
        Err(Error::Notdir)
    }

    fn remove_directory(&self, _path: &str) -> Result<()> {
        Err(Error::Notdir)
    }

    fn symlink(&self, _old_path: &str, _new_path: &str) -> Result<()> {
        Err(Error::Notdir)
    }

    fn unlink_file(&self, _path: &str) -> Result<()> {
        Err(Error::Notdir)
    }
}
