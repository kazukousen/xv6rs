use core::mem;

use alloc::boxed::Box;
use array_macro::array;

use crate::{
    file::File,
    fs::{FileStat, InodeType, INODE_TABLE},
    log::LOG,
    net::SockAddr,
    page_table::{align_down, PteFlag},
    param::PAGESIZE,
    process::PROCESS_TABLE,
};

use super::{elf, MapFlag, Proc, MAXARG, MAXARGLEN, VMA};

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

    /// int pipe(int p[])
    /// Create a pipe, put read/write file descriptors in p[0] and o[1].
    fn sys_pipe(&mut self) -> SysResult; // 4

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

    /// int fstat(int fd, struct stat *st)
    /// Place info about an open file into *st.
    fn sys_fstat(&mut self) -> SysResult; // 8

    /// int chdir(char *dir)
    /// Change the current directory.
    fn sys_chdir(&mut self) -> SysResult; // 9

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
    fn sys_mknod(&mut self) -> SysResult; // 17

    /// int unlink(char *file)
    /// Remove a file.
    fn sys_unlink(&mut self) -> SysResult; // 18

    /// TODO
    /// int link(char *file1, char *file2)
    /// Create another name (file2) for the file file1.
    // 19

    /// int mkdir(char *dir)
    /// Create a new directory.
    fn sys_mkdir(&mut self) -> SysResult; // 20

    /// int close(int fd)
    /// Release open file fd.
    fn sys_close(&mut self) -> SysResult; // 21

    /// int socket(int domain, int type, int protocol)
    /// Create a new socket.
    fn sys_socket(&mut self) -> SysResult; // 22

    /// int bind(int sockfd, const struct sockaddr *addr, socklen_t addrlen)
    /// Bind a socket to an address. Usually, a server employs this call to bind its socket to a
    /// well-known address so that clients can locate the socket.
    fn sys_bind(&mut self) -> SysResult; // 23

    /// int listen(int sockfd, int backlog)
    /// Allow a stream socket to accept incoming connections from other sockets.
    fn sys_listen(&mut self) -> SysResult; // 24

    /// int accept(int sockfd, struct sockaddr *addr, socklen_t *addrlen)
    /// Accept a coonection from a peer application on a listening stream socket, and optionally
    /// returns the address of the peer socket.
    fn sys_accept(&mut self) -> SysResult; // 25

    /// int connect(int sockfd, const struct sockaddr *addr, socklen_t addrlen)
    /// Establish a connection with another socket.
    fn sys_connect(&mut self) -> SysResult; // 26

    /// void *mmap(void *addr, size_t length, int prot, int flags, int fd, off_t offset)
    /// returns that address, or 0xffff_ffff_ffff_ffff if it fails.
    ///
    /// A file mapping maps a region of a file directly into the calling process's virtual memory.
    /// Once a file is mapped, its contents can be accessed by operations on the bytes in the
    /// corresponding memory region.
    fn sys_mmap(&mut self) -> SysResult; // 27
}

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

    /// 4
    fn sys_pipe(&mut self) -> SysResult {
        // array of two integers.
        let addr = self.arg_raw(0)?;

        let (rf, wf) = File::alloc_pipe();

        let rfd = self
            .alloc_fd()
            .or_else(|_| Err("sys_pipe: cannot allocate fd to read the pipe"))?;
        self.data.get_mut().o_files[rfd].replace(rf);

        let wfd = self
            .alloc_fd()
            .or_else(|_| Err("sys_pipe: cannot allocate fd to write the pipe"))?;
        self.data.get_mut().o_files[wfd].replace(wf);

        let pdata = self.data.get_mut();

        pdata.copy_out(
            addr,
            &rfd as *const usize as *const u8,
            mem::size_of::<usize>(),
        )?;
        pdata.copy_out(
            addr + mem::size_of::<usize>(),
            &wfd as *const usize as *const u8,
            mem::size_of::<usize>(),
        )?;

        Ok(0)
    }

    /// 5
    fn sys_read(&mut self) -> SysResult {
        let fd = self.arg_fd(0)?;
        let addr = self.arg_raw(1)?;
        let n = self.arg_i32(2)?;

        match self.data.get_mut().o_files[fd as usize].as_ref() {
            None => Err("sys_read"),
            Some(f) => f.read(addr, n as usize),
        }
    }

    /// 7
    fn sys_exec(&mut self) -> SysResult {
        let mut path: [u8; 128] = unsafe { mem::MaybeUninit::uninit().assume_init() };
        self.arg_str(0, &mut path)?;

        // argv: a pointer of null-terminated string, ..., 0
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

    /// 8
    fn sys_fstat(&mut self) -> SysResult {
        let fd = self.arg_fd(0)?;
        // `addr` is a user virtual address, pointing to a struct stat.
        let addr = self.arg_raw(1)?;

        // lookup the open file.
        let f = self.data.get_mut().o_files[fd as usize]
            .as_ref()
            .ok_or_else(|| "file not found")?;

        let mut st = FileStat::uninit();
        f.stat(&mut st);

        // copy data of struct stat from kernel to user.
        self.data.get_mut().copy_out(
            addr,
            &st as *const _ as *const u8,
            mem::size_of::<FileStat>(),
        )?;

        Ok(0)
    }

    /// 9
    fn sys_chdir(&mut self) -> SysResult {
        LOG.begin_op();
        let mut path: [u8; 128] = unsafe { mem::MaybeUninit::uninit().assume_init() };
        let null_pos = self.arg_str(0, &mut path).or_else(|msg| {
            LOG.end_op();
            Err(msg)
        })?;
        let path = &path[0..=null_pos];
        let inode = INODE_TABLE.namei(&path).ok_or_else(|| {
            LOG.end_op();
            "cannot find path"
        })?;

        let idata = inode.ilock();
        if idata.get_type() != InodeType::Directory {
            drop(idata);
            drop(inode);
            LOG.end_op();
            return Err("target path is not directory");
        }

        drop(idata);
        let old = self.data.get_mut().cwd.replace(inode).unwrap();
        drop(old);
        LOG.end_op();

        Ok(0)
    }

    /// 10
    fn sys_dup(&mut self) -> SysResult {
        let old_fd = self.arg_fd(0)?;
        let new_fd = self
            .alloc_fd()
            .or_else(|_| Err("sys_dup: cannot allocate new fd"))?;

        let old_f = self.data.get_mut().o_files[old_fd as usize]
            .as_ref()
            .unwrap();
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
        let path = &path[0..=null_pos];
        let o_mode = self.arg_i32(1)?;

        let f = File::open(&path, o_mode).ok_or_else(|| "sys_open: cannot open file")?;
        let fd = self
            .alloc_fd()
            .or_else(|_| Err("sys_open: cannot allocate fd"))?;
        self.data.get_mut().o_files[fd].replace(f);

        Ok(fd)
    }

    /// 16
    fn sys_write(&mut self) -> SysResult {
        let fd = self.arg_fd(0)?;
        let addr = self.arg_raw(1)?;
        let n = self.arg_i32(2)?;

        match self.data.get_mut().o_files[fd as usize].as_ref() {
            None => Err("sys_write"),
            Some(f) => f.write(addr, n as usize),
        }
    }

    /// 17
    fn sys_mknod(&mut self) -> SysResult {
        LOG.begin_op();
        let mut path: [u8; 128] = unsafe { mem::MaybeUninit::uninit().assume_init() };
        let null_pos = self.arg_str(0, &mut path).or_else(|msg| {
            LOG.end_op();
            Err(msg)
        })?;
        let path = &path[0..=null_pos];
        let major: u16 = self
            .arg_i32(1)
            .or_else(|msg| {
                LOG.end_op();
                Err(msg)
            })?
            .try_into()
            .unwrap();
        let minor: u16 = self
            .arg_i32(2)
            .or_else(|msg| {
                LOG.end_op();
                Err(msg)
            })?
            .try_into()
            .unwrap();

        let inode = INODE_TABLE.create(&path, InodeType::Device, major, minor);
        drop(inode);
        LOG.end_op();
        Ok(0)
    }

    /// 18
    fn sys_unlink(&mut self) -> SysResult {
        let mut path: [u8; 128] = unsafe { mem::MaybeUninit::uninit().assume_init() };
        let null_pos = self.arg_str(0, &mut path).or_else(|msg| Err(msg))?;
        let path = &path[0..=null_pos];

        LOG.begin_op();
        INODE_TABLE.unlink(&path).or_else(|msg| {
            LOG.end_op();
            Err(msg)
        })?;

        Ok(0)
    }

    /// 20
    fn sys_mkdir(&mut self) -> SysResult {
        LOG.begin_op();
        let mut path: [u8; 128] = unsafe { mem::MaybeUninit::uninit().assume_init() };
        let null_pos = self.arg_str(0, &mut path).or_else(|msg| {
            LOG.end_op();
            Err(msg)
        })?;
        let path = &path[0..=null_pos];

        let inode = INODE_TABLE.create(&path, InodeType::Directory, 0, 0);
        drop(inode);
        LOG.end_op();
        Ok(0)
    }

    /// 21
    fn sys_close(&mut self) -> SysResult {
        let fd = self.arg_fd(0)?;
        drop(self.data.get_mut().o_files[fd as usize].take());
        Ok(0)
    }

    /// 22
    fn sys_socket(&mut self) -> SysResult {
        let domain = self.arg_i32(0)? as u16;
        let typ = self.arg_i32(1)? as u8;
        let protocol = self.arg_i32(2)? as u8;

        let fd = self
            .alloc_fd()
            .or_else(|_| Err("sys_socket: cannot allocate fd"))?;

        let f = File::alloc_socket(domain, typ, protocol)?;

        self.data.get_mut().o_files[fd].replace(f);

        Ok(fd)
    }

    /// 23
    fn sys_bind(&mut self) -> SysResult {
        let fd = self.arg_fd(0)?;
        let addr = self.arg_raw(1)?;
        let addr_len = self.arg_raw(2)?;
        if addr_len != mem::size_of::<SockAddr>() {
            return Err("addr_len invalid");
        }
        let mut sock_addr = SockAddr::uninit();
        self.data
            .get_mut()
            .copy_in(&mut sock_addr as *mut _ as *mut u8, addr, addr_len)?;

        let f = self.data.get_mut().o_files[fd as usize]
            .as_ref()
            .ok_or("sys_bind: file not found")?;
        let soc = f.get_socket().ok_or("sys_bind: file type must be socket")?;
        soc.bind(&sock_addr)?;

        Ok(0)
    }

    /// 24
    fn sys_listen(&mut self) -> SysResult {
        Ok(0)
    }

    /// 25
    fn sys_accept(&mut self) -> SysResult {
        Ok(0)
    }

    /// 26
    fn sys_connect(&mut self) -> SysResult {
        let fd = self.arg_fd(0)?;
        let addr = self.arg_raw(1)?;
        let addr_len = self.arg_raw(2)?;
        if addr_len != mem::size_of::<SockAddr>() {
            return Err("addr_len invalid");
        }
        let mut sock_addr = SockAddr::uninit();
        self.data
            .get_mut()
            .copy_in(&mut sock_addr as *mut _ as *mut u8, addr, addr_len)?;

        let f = self.data.get_mut().o_files[fd as usize]
            .as_ref()
            .ok_or("sys_connect: file not found")?;
        let soc = f
            .get_socket()
            .ok_or("sys_connect: file type must be socket")?;
        soc.connect(&sock_addr)?;

        Ok(0)
    }

    /// 27
    /// This syscall func does not allocate physical memory or read the file, just add new VMA
    /// entry. Instead, do that in page fault handler.
    fn sys_mmap(&mut self) -> SysResult {
        // args
        // arg 0 `addr`
        let size = self.arg_i32(1)? as usize;
        let prot = self.arg_i32(2)? as usize;
        let prot = PteFlag::from_bits(prot).ok_or("sys_mmap: cannot parse prot")?;
        let flags = self.arg_i32(3)? as usize;
        let flags = MapFlag::from_bits(flags).ok_or("sys_mmap: cannot parse flags")?;
        let fd = self.arg_i32(4)?;

        let pdata = unsafe { &mut *self.data.get() };

        if fd != -1 {
            let f = pdata.o_files[fd as usize]
                .as_ref()
                .ok_or("sys_mmap: file not found")?;

            if (PteFlag::WRITE.bits() & prot.bits() > 0) && !f.writable {
                return Err("sys_mmap: file is read-only, but mmap has write permission and flag");
            }
        }

        let addr_end = pdata.cur_max;
        let addr_start = align_down(addr_end - size, PAGESIZE);

        pdata
            .vm_area
            .iter_mut()
            .find(|vm| {
                return vm.is_none();
            })
            .ok_or("cannot find unused vma")?
            .replace(VMA {
                addr_start,
                addr_end,
                size,
                prot,
                flags,
                fd,
            });
        pdata.cur_max = addr_start;

        return Ok(addr_start);
    }
}
