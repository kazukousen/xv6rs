//! A cool aspect of the Unix interface is that most resources in Unix are represented as files,
//! including devices such as the console, pipes, and of course, real files. The file descriptor
//! layer is the layer that archives this uniformity.
use alloc::sync::Arc;

use crate::{
    console,
    fs::{InodeType, INODE_TABLE},
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

        let idata = inode.ilock();
        let inner = match idata.get_type() {
            InodeType::Empty => panic!("create: inode empty"),
            InodeType::Directory => panic!(),
            InodeType::File => panic!("create: file type not implemented yet"),
            InodeType::Device => FileInner::Device,
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
            FileInner::Device => {
                console::write(true, addr, n);
                Ok(n)
            }
        }
    }
}

enum FileInner {
    Device,
}
