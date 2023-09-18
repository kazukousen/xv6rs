#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(xv6rs_user::test_runner)]
#![reexport_test_harness_main = "test_main"]

use xv6rs_user::{
    entry_point,
    fcntl::O_RDWR,
    println,
    syscall::{sys_dup, sys_exec, sys_fork, sys_mknod, sys_open, sys_wait},
    Args,
};

entry_point!(main);

fn main(_: &mut Args) -> Result<i32, &'static str> {
    if sys_open("console\0", O_RDWR) < 0 {
        sys_mknod("console\0", 1, 1);
        sys_open("console\0", O_RDWR);
    }

    sys_dup(0); // stdout
    sys_dup(0); // stderr
    loop {
        println!("init: starting sh");
        let pid = sys_fork();
        if pid < 0 {
            return Err("fork failed");
        }
        if pid == 0 {
            let cmd = "sh\0";
            sys_exec(cmd);
            return Err("exec failed");
        }

        loop {
            let mut status = 0i32;
            let wpid = sys_wait(&mut status);
            if wpid == pid {
                // the shell exited; restart it.
                break;
            } else if wpid < 0 {
                return Err("wait returned an error");
            }
            // it was a parentless process; do nothing.
        }
    }
}
