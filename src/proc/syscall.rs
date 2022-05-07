use core::mem;

use alloc::boxed::Box;
use array_macro::array;

use crate::file::File;

use super::{elf, ProcData};

type SysResult = Result<usize, &'static str>;

pub trait Syscall {
    fn sys_exec(&mut self) -> SysResult; // 7
    fn sys_dup(&mut self) -> SysResult; // 10
    fn sys_open(&mut self) -> SysResult; // 15
    fn sys_write(&mut self) -> SysResult; // 16
}

pub const MAXARG: usize = 16;
pub const MAXARGLEN: usize = 64;

impl Syscall for ProcData {
    fn sys_exec(&mut self) -> SysResult {
        let mut path: [u8; 128] = unsafe { mem::MaybeUninit::uninit().assume_init() };
        self.arg_str(0, &mut path)?;

        let arg_base_addr = self.arg_raw(1)?;
        let mut argv: [Option<Box<[u8; MAXARGLEN]>>; MAXARG] = array![_ => None; MAXARG];
        for i in 0..MAXARG {
            let uarg = self.fetch_addr(arg_base_addr + i * mem::size_of::<usize>())?;
            if uarg == 0 {
                break;
            }

            match Box::<[u8; MAXARGLEN]>::try_new_zeroed() {
                Ok(b) => unsafe { argv[i] = Some(b.assume_init()) },
                Err(_) => {
                    return Err("sys_exec: cannot allocate kernel space to copy arg");
                }
            }

            // copy arg to kernel space
            self.fetch_str(uarg, argv[i].as_deref_mut().unwrap())?;
        }

        elf::load(self, &path, &argv)
    }

    fn sys_dup(&mut self) -> SysResult {
        let old_fd = 0;
        self.arg_fd(old_fd)?;
        let new_fd = self
            .alloc_fd()
            .or_else(|_| Err("sys_dup: cannot allocate new fd"))?;

        let old_f = self.o_files[0].as_ref().unwrap();
        let new_f = old_f.clone();
        self.o_files[new_fd].replace(new_f);

        Ok(new_fd)
    }

    fn sys_open(&mut self) -> SysResult {
        let mut path: [u8; 128] = unsafe { mem::MaybeUninit::uninit().assume_init() };
        let null_pos = self.arg_str(0, &mut path)?;
        let o_mode = self.arg_i32(1)?;
        let path = &path[0..=null_pos];

        let f = File::open(&path, o_mode).ok_or_else(|| "sys_open: cannot open file")?;
        let fd = self
            .alloc_fd()
            .or_else(|_| Err("sys_open: cannot allocate fd"))?;
        self.o_files[fd].replace(f);

        Ok(fd)
    }

    fn sys_write(&mut self) -> SysResult {
        let fd = 0;
        self.arg_fd(fd)?;
        let addr = self.arg_raw(1)?;
        let n = self.arg_i32(2)?;

        match self.o_files[fd as usize].as_ref() {
            None => Err("sys_write"),
            Some(f) => {
                f.write(addr as *const u8, n as usize)
            }
        }
    }
}
