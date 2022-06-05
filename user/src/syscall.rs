extern "C" {
    /// int exit(int status)
    fn __exit(status: i32) -> !;
    /// int read(int fd, char *buf, int n)
    fn __read(fd: i32, addr: *const u8, n: i32) -> i32;
    /// int open(char *file, int flags)
    fn __open(addr: *const u8, mode: i32) -> i32;
    /// int write(int fd, char *buf, int n)
    fn __write(fd: i32, addr: *const u8, n: i32) -> i32;
    /// int close(int fd)
    fn __close(fd: i32);
}

pub fn sys_exit(status: i32) -> ! {
    unsafe { __exit(status) }
}

pub fn sys_read(fd: i32, buf: &mut [u8]) -> i32 {
    unsafe { __read(fd, buf.as_mut_ptr(), buf.len() as i32) }
}

pub fn sys_open(path: &str, mode: i32) -> i32 {
    unsafe { __open(path.as_ptr(), mode) }
}

pub fn sys_write(fd: i32, buf: &[u8]) -> i32 {
    unsafe { __write(fd, buf.as_ptr(), buf.len() as i32) }
}

pub fn sys_close(fd: i32) {
    unsafe { __close(fd) }
}
