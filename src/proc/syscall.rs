use core::mem;

use alloc::boxed::Box;
use array_macro::array;

use super::ProcData;

type SysResult = Result<usize, &'static str>;

pub trait Syscall {
    fn sys_exec(&mut self) -> SysResult; // 7
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

        Ok(0)
    }
}
