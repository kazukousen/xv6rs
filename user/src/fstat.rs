use crate::syscall::{sys_close, sys_fstat, sys_open};

#[repr(u16)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InodeType {
    Empty = 0,
    Directory = 1,
    File = 2,
    Device = 3,
}

#[repr(C)]
pub struct FileStat {
    pub dev: i32,
    pub inum: u32,
    pub typ: InodeType,
    pub nlink: u16,
    pub size: u64,
}

impl FileStat {
    pub fn uninit() -> Self {
        Self {
            dev: 0,
            inum: 0,
            typ: InodeType::Empty,
            nlink: 0,
            size: 0,
        }
    }
}

pub const DIRSIZ: usize = 30;

#[repr(C)]
pub struct DirEnt {
    pub inum: u16,
    pub name: [u8; DIRSIZ],
}

impl DirEnt {
    pub fn empty() -> Self {
        Self {
            inum: 0,
            name: [0; DIRSIZ],
        }
    }
}

pub fn stat(path: &str, st: &mut FileStat) -> Result<(), &'static str> {
    let fd = sys_open(path, 0);
    if fd < 0 {
        return Err("stat: cannot open");
    }

    if sys_fstat(fd, st) < 0 {
        sys_close(fd);
        return Err("stat: cannot stat");
    }
    sys_close(fd);

    Ok(())
}
