#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(xv6rs_user::test_runner)]
#![reexport_test_harness_main = "test_main"]

use xv6rs_user::syscall::sys_exit;

#[no_mangle]
pub unsafe extern "C" fn _start() -> ! {
    #[cfg(test)]
    crate::test_main();

    sys_exit(42);
}

#[cfg(test)]
mod tests {
    use xv6rs_user::syscall::{sys_exec, sys_fork, sys_wait};

    use super::*;

    #[test_case]
    fn test_exit42() {
        let pid = sys_fork();
        assert!(pid >= 0);
        if pid == 0 {
            assert!(sys_exec("exit42\0") > 0);
        }
        let mut status = 0i32;
        let wpid = sys_wait(&mut status);
        assert_eq!(pid, wpid);
        assert_eq!(42i32, status);
    }
}
