//! A cool aspect of the Unix interface is that most resources in Unix are represented as files,
//! including devices such as the console, pipes, and of course, real files. The file descriptor
//! layer is the layer that archives this uniformity.
use core::cell::UnsafeCell;

use alloc::sync::Arc;

use crate::{
    console,
    fs::{FileStat, Inode, InodeType, INODE_TABLE},
    log::LOG,
};

pub const O_RDONLY: i32 = 0x000;
pub const O_WRONLY: i32 = 0x001;
pub const O_RDWR: i32 = 0x002;
pub const O_CREATE: i32 = 0x200;
pub const O_TRUNC: i32 = 0x400;

/// Each open file is represented by a `struct File`, which is a wrapper around either an inode or
/// a pipe, plus an I/O offset.
/// each call to `open` creates a new open file (a new `struct File`):
///     if multiple processes open the same file independently, the different instances will have
///     different I/O offsets.
pub struct File {
    // A file can be open for reading or writing or both. The `readable` and `writable` fields
    // track this.
    readable: bool,
    writable: bool,
    inner: FileInner,
}

impl File {
    pub fn open(path: &[u8], o_mode: i32) -> Option<Arc<Self>> {
        LOG.begin_op();
        let inode = if o_mode & O_CREATE > 0 {
            panic!("create file not implemented yet");
        } else {
            INODE_TABLE.namei(&path)
        }
        .or_else(|| {
            LOG.end_op();
            None
        })?;

        let readable = o_mode & O_WRONLY == 0;
        let writable = (o_mode & O_WRONLY > 0) || (o_mode & O_RDWR > 0);

        let mut idata = inode.ilock();
        let inner = match idata.get_type() {
            InodeType::Empty => panic!("create: inode empty"),
            InodeType::Directory => {
                if o_mode != O_RDONLY {
                    drop(idata);
                    drop(inode);
                    LOG.end_op();
                    return None;
                }
                drop(idata);
                FileInner::Inode(FileInode {
                    inode: Some(inode),
                    offset: UnsafeCell::new(0),
                })
            }
            InodeType::File => {
                if o_mode & O_TRUNC > 0 {
                    idata.itrunc();
                }
                drop(idata);
                FileInner::Inode(FileInode {
                    inode: Some(inode),
                    offset: UnsafeCell::new(0),
                })
            }
            InodeType::Device => {
                let major = idata.get_major();
                drop(idata);
                FileInner::Device(FileDevice {
                    inode: Some(inode),
                    major,
                })
            }
        };
        LOG.end_op();

        Some(Arc::new(Self {
            readable,
            writable,
            inner,
        }))
    }

    pub fn write(&self, addr: *const u8, n: usize) -> Result<usize, &'static str> {
        if !self.writable {
            return Err("write: not writable");
        }

        match &self.inner {
            FileInner::Device(ref f) => {
                if f.major != 1 {
                    panic!("device_write not implemented");
                }
                console::write(true, addr, n);
                Ok(n)
            }
            _ => panic!("unimplemented yet"),
        }
    }

    pub fn read(&self, addr: *mut u8, n: usize) -> Result<usize, &'static str> {
        if !self.readable {
            return Err("read: not readable");
        }

        match &self.inner {
            FileInner::Device(ref f) => {
                if f.major != 1 {
                    panic!("device_read not implemented");
                }
                console::read(true, addr, n).or_else(|_| Err("read: cannot read"))
            }
            FileInner::Inode(ref f) => {
                let mut idata = f.inode.as_ref().unwrap().ilock();

                let offset = unsafe { &mut (*f.offset.get()) };
                let read_n = idata
                    .readi(true, addr, *offset, n)
                    .or_else(|()| Err("cannot read the file"))?;
                *offset += read_n;
                drop(idata);
                Ok(read_n)
            }
        }
    }

    /// Get metadata about the file.
    /// `addr` is a user virtual address, pointing to a struct stat.
    pub fn stat(&self, st: &mut FileStat) {
        match &self.inner {
            FileInner::Inode(ref f) => {
                let idata = f.inode.as_ref().unwrap().ilock();
                idata.stati(st);
                drop(idata);
            }
            FileInner::Device(ref f) => {
                let idata = f.inode.as_ref().unwrap().ilock();
                idata.stati(st);
                drop(idata);
            }
        }
    }
}

enum FileInner {
    Inode(FileInode),
    Device(FileDevice),
}

struct FileInode {
    inode: Option<Inode>,
    offset: UnsafeCell<usize>,
}

struct FileDevice {
    inode: Option<Inode>,
    major: u16,
}
