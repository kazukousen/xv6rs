use crate::{fstat::FileStat, net::SockAddr};
use core::{mem, ptr};

extern "C" {
    /// int fork()
    fn __fork() -> i32;
    /// int exit(int status)
    fn __exit(status: i32) -> !;
    /// int wait(int *status)
    fn __wait(addr: *mut i32) -> i32;
    /// int read(int fd, char *buf, int n)
    fn __read(fd: i32, addr: *const u8, n: i32) -> i32;
    /// int exec(char *file, char *argv[])
    fn __exec(addr: *const u8, argv: *const *const u8) -> i32;
    /// int fstat(int fd, struct stat *st)
    fn __fstat(fd: i32, st: *mut FileStat) -> i32;
    /// int dup(int fd)
    fn __dup(fd: i32);
    /// int open(char *file, int flags)
    fn __open(addr: *const u8, mode: i32) -> i32;
    /// int write(int fd, char *buf, int n)
    fn __write(fd: i32, addr: *const u8, n: i32) -> i32;
    /// int mknod(char *file, int, int)
    fn __mknod(addr: *const u8, major: i32, minor: i32) -> i32;
    /// int close(int fd)
    fn __close(fd: i32);
    /// int socket(int domain, int type, int protocol)
    fn __socket(domain: i32, typ: i32, protocol: i32) -> i32; // 22
    /// int bind(int sockfd, const struct sockaddr *addr, socklen_t addrlen)
    fn __bind(sockfd: i32, addr: *const u8, addr_len: usize) -> i32;
    /// int connect(int sockfd, const struct sockaddr *addr, socklen_t addrlen)
    fn __connect(sockfd: i32, addr: *const u8, addr_len: usize) -> i32;
}

pub fn sys_fork() -> i32 {
    unsafe { __fork() }
}

pub fn sys_exit(status: i32) -> ! {
    unsafe { __exit(status) }
}

pub fn sys_wait(status: &mut i32) -> i32 {
    unsafe { __wait(status as *mut _) }
}

pub fn sys_read(fd: i32, buf: &mut [u8]) -> i32 {
    unsafe { __read(fd, buf.as_mut_ptr(), buf.len() as i32) }
}

pub fn sys_exec(path: &str) -> i32 {
    let argv: [*const u8; 2] = [path.as_ptr(), ptr::null()];
    unsafe { __exec(path.as_ptr(), argv.as_ptr()) }
}

pub fn sys_fstat(fd: i32, st: &mut FileStat) -> i32 {
    unsafe { __fstat(fd, st as *mut _) }
}

pub fn sys_dup(fd: i32) {
    unsafe { __dup(fd) }
}

pub fn sys_open(path: &str, mode: i32) -> i32 {
    unsafe { __open(path.as_ptr(), mode) }
}

pub fn sys_write(fd: i32, buf: &[u8]) -> i32 {
    unsafe { __write(fd, buf.as_ptr(), buf.len() as i32) }
}

pub fn sys_mknod(path: &str, major: i32, minor: i32) -> i32 {
    unsafe { __mknod(path.as_ptr(), major, minor) }
}

pub fn sys_close(fd: i32) {
    unsafe { __close(fd) }
}

pub fn sys_socket(domain: i32, typ: i32, protocol: i32) -> i32 {
    unsafe { __socket(domain, typ, protocol) }
}

pub fn sys_bind(sockfd: i32, sock_addr: &SockAddr) -> i32 {
    unsafe {
        __bind(
            sockfd,
            sock_addr as *const _ as *const u8,
            mem::size_of::<SockAddr>(),
        )
    }
}

pub fn sys_connect(sockfd: i32, sock_addr: &SockAddr) -> i32 {
    unsafe {
        __connect(
            sockfd,
            sock_addr as *const _ as *const u8,
            mem::size_of::<SockAddr>(),
        )
    }
}
