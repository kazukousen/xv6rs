use crate::{fstat::FileStat, net::SockAddr};
use core::mem;

extern "C" {
    /// 1
    /// int fork()
    fn __fork() -> i32;
    /// 2
    /// int exit(int status)
    fn __exit(status: i32) -> !;
    /// 3
    /// int wait(int *status)
    fn __wait(addr: *mut i32) -> i32;
    /// 5
    /// int read(int fd, char *buf, int n)
    fn __read(fd: i32, addr: *const u8, n: i32) -> i32;
    /// 7
    /// int exec(char *file, char *argv[])
    fn __exec(addr: *const u8, argv: *const *const u8) -> i32;
    /// 8
    /// int fstat(int fd, struct stat *st)
    fn __fstat(fd: i32, st: *mut FileStat) -> i32;
    /// 9
    /// int chdir(char *dir)
    fn __chdir(addr: *const u8) -> i32;
    /// 10
    /// int dup(int fd)
    fn __dup(fd: i32);
    /// 12
    /// char *sbrk(int n)
    /// Grow process's memory by n bytes. Returns start of new memory.
    fn __sbrk(n: i32) -> *mut u8;
    /// 15
    /// int open(char *file, int flags)
    fn __open(addr: *const u8, mode: i32) -> i32;
    /// 16
    /// int write(int fd, char *buf, int n)
    fn __write(fd: i32, addr: *const u8, n: i32) -> i32;
    /// 17
    /// int mknod(char *file, int, int)
    fn __mknod(addr: *const u8, major: i32, minor: i32) -> i32;
    /// 18
    /// int unlink(char *file)
    fn __unlink(addr: *const u8) -> i32;
    /// 20
    /// int mkdir(char *dir)
    fn __mkdir(addr: *const u8) -> i32;
    /// 21
    /// int close(int fd)
    fn __close(fd: i32) -> i32;
    /// 22
    /// int socket(int domain, int type, int protocol)
    fn __socket(domain: i32, typ: i32, protocol: i32) -> i32; // 22
    /// 23
    /// int bind(int sockfd, const struct sockaddr *addr, socklen_t addrlen)
    fn __bind(sockfd: i32, addr: *const u8, addr_len: usize) -> i32;
    /// 26
    /// int connect(int sockfd, const struct sockaddr *addr, socklen_t addrlen)
    fn __connect(sockfd: i32, addr: *const u8, addr_len: usize) -> i32;
    /// 27
    /// void *mmap(void *addr, size_t length, int prot, int flags, int fd, off_t offset)
    fn __mmap(
        addr: *const u8,
        size: usize,
        prot: usize,
        flags: usize,
        fd: i32,
        offset: usize,
    ) -> usize;
    /// 28
    /// int getenv(const char *name, char *value, size_t size)
    fn __getenv(name: *const u8, value: *mut u8, size: usize) -> i32;
    /// 29
    /// int setenv(const char *name, const char *value, int overwrite)
    fn __setenv(name: *const u8, value: *const u8, overwrite: i32) -> i32;
    /// 30
    /// int unsetenv(const char *name)
    fn __unsetenv(name: *const u8) -> i32;
    /// 31
    /// int listenv(char *buf, size_t size)
    fn __listenv(buf: *mut u8, size: usize) -> i32;
}

// 1
pub fn sys_fork() -> i32 {
    unsafe { __fork() }
}

// 2
pub fn sys_exit(status: i32) -> ! {
    unsafe { __exit(status) }
}

// 3
pub fn sys_wait(status: &mut i32) -> i32 {
    unsafe { __wait(status as *mut _) }
}

// 5
pub fn sys_read(fd: i32, buf: &mut [u8]) -> i32 {
    unsafe { __read(fd, buf.as_mut_ptr(), buf.len() as i32) }
}

// 7
pub fn sys_exec(argv: &[*const u8]) -> i32 {
    unsafe { __exec(argv[0], argv.as_ptr()) }
}

// 8
pub fn sys_fstat(fd: i32, st: &mut FileStat) -> i32 {
    unsafe { __fstat(fd, st as *mut _) }
}

// 9
pub fn sys_chdir(path: &str) -> i32 {
    unsafe { __chdir(path.as_ptr()) }
}

// 10
pub fn sys_dup(fd: i32) {
    unsafe { __dup(fd) }
}

// 12
pub fn sys_sbrk(n: i32) -> *mut u8 {
    unsafe { __sbrk(n) }
}

// 15
pub fn sys_open(path: &str, mode: i32) -> i32 {
    unsafe { __open(path.as_ptr(), mode) }
}

// 16
pub fn sys_write(fd: i32, buf: &[u8]) -> i32 {
    unsafe { __write(fd, buf.as_ptr(), buf.len() as i32) }
}

// 17
pub fn sys_mknod(path: &str, major: i32, minor: i32) -> i32 {
    unsafe { __mknod(path.as_ptr(), major, minor) }
}

// 18
pub fn sys_unlink(path: &str) -> i32 {
    unsafe { __unlink(path.as_ptr()) }
}

// 20
pub fn sys_mkdir(path: &str) -> i32 {
    unsafe { __mkdir(path.as_ptr()) }
}

// 21
pub fn sys_close(fd: i32) -> i32 {
    unsafe { __close(fd) }
}

// 22
pub fn sys_socket(domain: i32, typ: i32, protocol: i32) -> i32 {
    unsafe { __socket(domain, typ, protocol) }
}

// 23
pub fn sys_bind(sockfd: i32, sock_addr: &SockAddr) -> i32 {
    unsafe {
        __bind(
            sockfd,
            sock_addr as *const _ as *const u8,
            mem::size_of::<SockAddr>(),
        )
    }
}

// 26
pub fn sys_connect(sockfd: i32, sock_addr: &SockAddr) -> i32 {
    unsafe {
        __connect(
            sockfd,
            sock_addr as *const _ as *const u8,
            mem::size_of::<SockAddr>(),
        )
    }
}

// 27
pub fn sys_mmap(
    addr: *const u8,
    size: usize,
    prot: usize,
    flags: usize,
    fd: i32,
    offset: usize,
) -> usize {
    unsafe { __mmap(addr, size, prot, flags, fd, offset) }
}

// 28
pub fn sys_getenv(name: &str, value: &mut [u8]) -> i32 {
    unsafe { __getenv(name.as_ptr(), value.as_mut_ptr(), value.len()) }
}

// 29
pub fn sys_setenv(name: &str, value: &str, overwrite: bool) -> i32 {
    unsafe { __setenv(name.as_ptr(), value.as_ptr(), if overwrite { 1 } else { 0 }) }
}

// 30
pub fn sys_unsetenv(name: &str) -> i32 {
    unsafe { __unsetenv(name.as_ptr()) }
}

// 31
pub fn sys_listenv(buf: &mut [u8]) -> i32 {
    unsafe { __listenv(buf.as_mut_ptr(), buf.len()) }
}
