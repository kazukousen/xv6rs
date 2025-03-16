#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(xv6rs_user::test_runner)]
#![reexport_test_harness_main = "test_main"]

use xv6rs_user::{entry_point, Args};

entry_point!(main);
fn main(args: &mut Args) -> Result<i32, &'static str> {
    let mut n = 1;
    for arg in args.skip(1) {
        n += 1;
    }
    Ok(42 * n)
}

#[cfg(test)]
mod tests {
    use core::ptr;

    use xv6rs_user::syscall::{sys_exec, sys_fork, sys_wait};

    use super::*;

    #[test_case]
    fn test_exit42() {
        let pid = sys_fork();
        assert!(pid >= 0);
        if pid == 0 {
            assert!(sys_exec(&["exit42\0".as_ptr(), ptr::null()]) > 0);
        }
        let mut status = 0i32;
        let wpid = sys_wait(&mut status);
        assert_eq!(pid, wpid);
        assert_eq!(42i32, status);
    }

    #[test_case]
    fn test_exit42_x2() {
        let pid = sys_fork();
        assert!(pid >= 0);
        if pid == 0 {
            assert!(sys_exec(&["exit42\0".as_ptr(), "foo\0".as_ptr(), ptr::null()]) > 0);
        }
        let mut status = 0i32;
        let wpid = sys_wait(&mut status);
        assert_eq!(pid, wpid);
        assert_eq!(84i32, status);
    }
}
