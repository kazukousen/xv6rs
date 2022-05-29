extern "C" {
    /// int write(int fd, char *buf, int n)
    fn __write(fd: i32, addr: *const u8, n: i32) -> i32;
    /// int exit(int status)
    fn __exit(status: i32) -> !;
}

pub fn sys_write(fd: i32, buf: &[u8]) -> i32 {
    unsafe { __write(fd, buf.as_ptr(), buf.len() as i32) }
}

pub fn sys_exit(status: i32) -> ! {
    unsafe { __exit(status) }
}
