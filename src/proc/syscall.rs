use core::mem;

use alloc::boxed::Box;
use array_macro::array;

use crate::{file::File, process::PROCESS_TABLE};

use super::{elf, Proc};

type SysResult = Result<usize, &'static str>;

pub trait Syscall {
    /// int fork()
    /// Create a process, return child's PID.
    fn sys_fork(&mut self) -> SysResult; // 1

    /// int exit(int status)
    /// Terminate the current process; status reported to wait(). No return.
    fn sys_exit(&mut self) -> SysResult; // 2

    /// int wait(int *status)
    /// Wait for a child to exit; exit status in *status; returns child PID.
    fn sys_wait(&mut self) -> SysResult; // 3

    /// TODO
    /// int pipe(int p[])
    /// Create a pipe, put read/write file descriptors in p[0] and o[1].
    // 4

    /// int read(int fd, char *buf, int n)
    /// Read n bytes into buf; returns number read; or 0 if end of file.
    fn sys_read(&mut self) -> SysResult; // 5

    /// TODO
    /// int kill(int pid)
    /// Terminate process PID. Returns 0, or -1 for error.
    // 6

    /// int exec(char *file, char *argv[])
    /// Load a file and execute it with arguments; only returns if error.
    fn sys_exec(&mut self) -> SysResult; // 7

    /// TODO
    /// int fstat(int fd, struct stat *st)
    /// Place info about an open file into *st.
    // 8

    /// TODO
    /// int chdir(char *dir)
    /// Change the current directory.
    // 9

    /// int dup(int fd)
    /// Return a new file descriptor referring to the same file as fd.
    fn sys_dup(&mut self) -> SysResult; // 10

    /// TODO
    /// int getpid()
    /// Return the current process's PID.
    // 11

    /// char *sbrk(int n)
    /// Grow process's memory by n bytes. Returns start of new memory.
    fn sys_sbrk(&mut self) -> SysResult; // 12

    /// TODO
    /// int sleep(int n)
    /// Pause for n clock ticks.
    // 13

    /// TODO
    /// int uptime()
    /// Return how many clock tick interrupts have occurred since start.
    // 14

    /// int open(char *file, int flags)
    /// Open a file; flags indicate read/write; returns an fd(file descriptor).
    fn sys_open(&mut self) -> SysResult; // 15

    /// int write(int fd, char *buf, int n)
    /// Write n bytes from buf to file descriptor fd; returns n.
    fn sys_write(&mut self) -> SysResult; // 16

    /// TODO
    /// int mknod(char *file, int, int)
    /// Create a device file.
    // 17

    /// TODO
    /// int unlink(char *file)
    /// Remove a file.
    // 18

    /// TODO
    /// int link(char *file1, char *file2)
    /// Create another name (file2) for the file file1.
    // 19

    /// TODO
    /// int mkdir(char *dir)
    /// Create a new directory.
    // 20

    /// int close(int fd)
    /// Release open file fd.
    fn sys_close(&mut self) -> SysResult; // 21
}

pub const MAXARG: usize = 16;
pub const MAXARGLEN: usize = 64;

impl Syscall for Proc {
    /// 1
    fn sys_fork(&mut self) -> SysResult {
        self.fork()
    }

    /// 2
    fn sys_exit(&mut self) -> SysResult {
        let n = self.arg_i32(0)?;
        unsafe { PROCESS_TABLE.exit(self, n) };
        unreachable!();
    }

    /// 3
    fn sys_wait(&mut self) -> SysResult {
        let addr = self.arg_raw(0)?;
        unsafe { PROCESS_TABLE.wait(self, addr) }
    }

    /// 5
    fn sys_read(&mut self) -> SysResult {
        let fd = 0;
        self.arg_fd(fd)?;
        let addr = self.arg_raw(1)?;
        let n = self.arg_i32(2)?;

        match self.data.get_mut().o_files[fd as usize].as_ref() {
            None => Err("sys_read"),
            Some(f) => f.read(addr as *mut u8, n as usize),
        }
    }

    /// 7
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

        elf::load(self.data.get_mut(), &path, &argv)
    }

    /// 10
    fn sys_dup(&mut self) -> SysResult {
        let old_fd = 0;
        self.arg_fd(old_fd)?;
        let new_fd = self
            .alloc_fd()
            .or_else(|_| Err("sys_dup: cannot allocate new fd"))?;

        let old_f = self.data.get_mut().o_files[0].as_ref().unwrap();
        let new_f = old_f.clone();
        self.data.get_mut().o_files[new_fd].replace(new_f);

        Ok(new_fd)
    }

    /// 12
    fn sys_sbrk(&mut self) -> SysResult {
        let n = self.arg_i32(0)?;
        let pdata = self.data.get_mut();
        let sz = pdata.sz;
        if n > 0 {
            pdata.sz = pdata
                .page_table
                .as_mut()
                .unwrap()
                .uvm_alloc(sz, sz + n as usize)?;
        } else if n < 0 {
            pdata.sz = pdata
                .page_table
                .as_mut()
                .unwrap()
                .uvm_dealloc(sz, sz + n as usize)?;
        }
        Ok(0)
    }

    /// 15
    fn sys_open(&mut self) -> SysResult {
        let mut path: [u8; 128] = unsafe { mem::MaybeUninit::uninit().assume_init() };
        let null_pos = self.arg_str(0, &mut path)?;
        let o_mode = self.arg_i32(1)?;
        let path = &path[0..=null_pos];

        let f = File::open(&path, o_mode).ok_or_else(|| "sys_open: cannot open file")?;
        let fd = self
            .alloc_fd()
            .or_else(|_| Err("sys_open: cannot allocate fd"))?;
        self.data.get_mut().o_files[fd].replace(f);

        Ok(fd)
    }

    /// 16
    fn sys_write(&mut self) -> SysResult {
        let fd = 0;
        self.arg_fd(fd)?;
        let addr = self.arg_raw(1)?;
        let n = self.arg_i32(2)?;

        match self.data.get_mut().o_files[fd as usize].as_ref() {
            None => Err("sys_write"),
            Some(f) => f.write(addr as *const u8, n as usize),
        }
    }

    /// 21
    fn sys_close(&mut self) -> SysResult {
        let fd = self.arg_fd(0)?;
        drop(self.data.get_mut().o_files[fd as usize].take());
        Ok(0)
    }
}
