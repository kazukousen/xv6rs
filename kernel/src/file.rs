//! A cool aspect of the Unix interface is that most resources in Unix are represented as files,
//! including devices such as the console, pipes, and of course, real files. The file descriptor
//! layer is the layer that archives this uniformity.
use core::{cell::UnsafeCell, panic};

use alloc::{boxed::Box, sync::Arc};

use crate::{
    console,
    cpu::CPU_TABLE,
    fs::{FileStat, Inode, InodeType, INODE_TABLE},
    log::LOG,
    net::{self, Socket},
    process::PROCESS_TABLE,
    spinlock::SpinLock,
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
    pub readable: bool,
    pub writable: bool,
    inner: FileInner,
}

impl File {
    pub fn open(path: &[u8], o_mode: i32) -> Option<Arc<Self>> {
        LOG.begin_op();
        let inode = if o_mode & O_CREATE > 0 {
            Some(INODE_TABLE.create(&path, InodeType::File, 0, 0))
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

    pub fn alloc_pipe() -> (Arc<File>, Arc<File>) {
        let p = Arc::new(SpinLock::new(FilePipe::new(), "pipe"));
        let rf = Arc::new(Self {
            readable: true,
            writable: false,
            inner: FileInner::Pipe(p.clone()),
        });
        let wf = Arc::new(Self {
            readable: false,
            writable: true,
            inner: FileInner::Pipe(p.clone()),
        });
        (rf, wf)
    }

    pub fn alloc_socket(domain: u16, typ: u8, protocol: u8) -> Result<Arc<Self>, &'static str> {
        let s = net::Socket::new(typ)?;

        let f = Self {
            readable: true,
            writable: true,
            inner: FileInner::Socket(Box::new(s)),
        };

        Ok(Arc::new(f))
    }

    pub fn write(&self, addr: usize, n: usize) -> Result<usize, &'static str> {
        if !self.writable {
            return Err("write: not writable");
        }

        match &self.inner {
            FileInner::Device(ref f) => {
                if f.major != 1 {
                    panic!("device_write not implemented");
                }
                console::write(true, addr as *const u8, n);
                Ok(n)
            }
            FileInner::Pipe(ref f) => {
                let p = unsafe { CPU_TABLE.my_proc() };
                let mut guard = f.lock();
                let mut i = 0;
                while i < n {
                    if !guard.read_open {
                        // TODO killed
                        drop(guard);
                        return Err("pipe_write: no read open");
                    }

                    if guard.n_write == guard.n_read + PIPE_SIZE {
                        // reached full size
                        unsafe { PROCESS_TABLE.wakeup(&guard.n_read as *const _ as usize) };
                        guard = p.sleep(&guard.n_write as *const _ as usize, guard);
                    } else {
                        let mut ch = 0u8;
                        if p.data
                            .get_mut()
                            .copy_in(&mut ch as *mut _, addr + i as usize, 1)
                            .is_err()
                        {
                            break;
                        }
                        let n_write = guard.n_write + 1;
                        guard.data[n_write % PIPE_SIZE] = ch;
                        guard.n_write = n_write;
                        i += 1;
                    }
                }

                unsafe { PROCESS_TABLE.wakeup(&guard.n_read as *const _ as usize) };
                drop(guard);
                Ok(i)
            }
            FileInner::Socket(ref s) => s.write(addr, n),
            FileInner::Inode(ref f) => {
                LOG.begin_op();
                let mut idata = f.inode.as_ref().unwrap().ilock();
                let offset = unsafe { &mut (*f.offset.get()) };
                idata
                    .writei(true, addr as *const u8, *offset, n)
                    .or_else(|()| Err("cannot write the file"))?;
                *offset += n;
                drop(idata);
                LOG.end_op();
                Ok(n)
            }
        }
    }

    pub fn read(&self, addr: usize, n: usize) -> Result<usize, &'static str> {
        if !self.readable {
            return Err("read: not readable");
        }

        match &self.inner {
            FileInner::Device(ref f) => {
                if f.major != 1 {
                    panic!("device_read not implemented");
                }
                console::read(true, addr as *mut u8, n).or_else(|_| Err("read: cannot read"))
            }
            FileInner::Inode(ref f) => {
                let mut idata = f.inode.as_ref().unwrap().ilock();

                let offset = unsafe { &mut (*f.offset.get()) };
                let read_n = idata
                    .readi(true, addr as *mut u8, *offset, n)
                    .or_else(|()| Err("cannot read the file"))?;
                *offset += read_n;
                drop(idata);
                Ok(read_n)
            }
            FileInner::Pipe(ref f) => {
                let p = unsafe { CPU_TABLE.my_proc() };
                let mut guard = f.lock();
                while guard.n_read == guard.n_write && guard.write_open {
                    // TODO: killed
                    // pipe is still empty. sleep.
                    guard = p.sleep(&guard.n_read as *const _ as usize, guard);
                }

                for i in 0..n as isize {
                    if guard.n_read == guard.n_write {
                        break;
                    }
                    // copy into addr
                    guard.n_read += 1;
                    let ch = &guard.data[guard.n_read % PIPE_SIZE];
                    if p.data
                        .get_mut()
                        .copy_out(addr + i as usize, ch as *const _, 1)
                        .is_err()
                    {
                        break;
                    }
                }
                // wakeup writer
                unsafe { PROCESS_TABLE.wakeup(&guard.n_write as *const _ as usize) };
                drop(guard);
                Ok(0)
            }
            FileInner::Socket(ref s) => s.read(addr, n),
        }
    }

    /// Get metadata about the file.
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
            FileInner::Pipe(ref f) => {
                let guard = f.lock();
                drop(guard);
            }
            FileInner::Socket(ref s) => {}
        }
    }

    pub fn get_socket(&self) -> Option<&Socket> {
        match &self.inner {
            FileInner::Socket(s) => Some(s),
            _ => None,
        }
    }

    pub fn seek(&self, offset: usize) {
        match &self.inner {
            FileInner::Inode(ref f) => {
                // When updating atomically the read and write offsets, the inode must be locked.
                let idata = f.inode.as_ref().unwrap().ilock();
                let current = unsafe { &mut (*f.offset.get()) };
                *current = offset;
                drop(idata);
            }
            _ => panic!("file: seek() is only available on File"),
        }
    }
}

impl Drop for File {
    fn drop(&mut self) {
        match self.inner {
            FileInner::Inode(ref mut f) => {
                LOG.begin_op();
                drop(f.inode.take());
                LOG.end_op();
            }
            FileInner::Device(ref mut f) => {
                LOG.begin_op();
                drop(f.inode.take());
                LOG.end_op();
            }
            FileInner::Pipe(ref mut f) => {
                let mut guard = f.lock();
                if self.writable {
                    guard.write_open = false;
                    unsafe { PROCESS_TABLE.wakeup(&guard.n_read as *const _ as usize) };
                } else {
                    guard.read_open = false;
                    unsafe { PROCESS_TABLE.wakeup(&guard.n_write as *const _ as usize) };
                }
                if !guard.read_open && !guard.write_open {
                    drop(guard);
                    drop(f);
                } else {
                    drop(guard);
                }
            }
            FileInner::Socket(ref s) => {
                drop(s);
            }
        }
    }
}

enum FileInner {
    Inode(FileInode),
    Device(FileDevice),
    Pipe(Arc<SpinLock<FilePipe>>),
    Socket(Box<Socket>),
}

struct FileInode {
    inode: Option<Inode>,
    offset: UnsafeCell<usize>,
}

struct FileDevice {
    inode: Option<Inode>,
    major: u16,
}

const PIPE_SIZE: usize = 512;

struct FilePipe {
    data: [u8; PIPE_SIZE],
    read_open: bool,
    write_open: bool,
    n_read: usize,
    n_write: usize,
}

impl FilePipe {
    fn new() -> Self {
        Self {
            data: [0; PIPE_SIZE],
            read_open: true,
            write_open: true,
            n_read: 0,
            n_write: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::proc::INITCODE;

    use super::*;

    #[test_case]
    fn share_pipe() {
        let (r, w) = File::alloc_pipe();
        assert_eq!(true, r.readable);
        assert_eq!(false, r.writable);
        assert_eq!(false, w.readable);
        assert_eq!(true, w.writable);

        if let FileInner::Pipe(ref f) = &r.inner {
            // is sharing the same file pointer with the writer
            assert_eq!(2, Arc::strong_count(f));
        } else {
            panic!("read pipe");
        }

        if let FileInner::Pipe(ref f) = &w.inner {
            // is sharing the same file pointer with the reader
            assert_eq!(2, Arc::strong_count(f));
        } else {
            panic!("read pipe");
        }
    }

    #[test_case]
    fn write_read() {
        // remap
        let p = unsafe { CPU_TABLE.my_proc() };
        let pdata = p.data.get_mut();
        let pgt = pdata.page_table.as_mut().unwrap();
        pgt.unmap_pages(0, 1, true).expect("cannot unmap initcode");
        pgt.uvm_init(&[0, 0, 0, 0, 0, 1, 2, 3, 4, 5])
            .expect("cannot map into the page");

        let (r, w) = File::alloc_pipe();

        w.write(5, 5).expect("cannot write");
        r.read(0, 5).expect("cannot read");

        let pa = pgt.walk_addr(0).expect("cannot walk") as *const u8;
        let actual = unsafe { (pa as *const [u8; 10]).as_ref() }.unwrap();
        assert_eq!(&[1, 2, 3, 4, 5, 1, 2, 3, 4, 5], actual);

        // restore
        pgt.unmap_pages(0, 1, true).expect("cannot unmap test code");
        pgt.uvm_init(&INITCODE)
            .expect("cannot map the initcode into the page");
    }
}
