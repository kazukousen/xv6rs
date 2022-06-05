#![no_std]
#![no_main]

use xv6rs_user::{
    fcntl::O_RDWR,
    println,
    syscall::{sys_dup, sys_exec, sys_exit, sys_fork, sys_mknod, sys_open, sys_wait},
};

#[no_mangle]
extern "C" fn _start() {
    if sys_open("console", O_RDWR) < 0 {
        sys_mknod("console", 1, 1);
        sys_open("console", O_RDWR);
    }

    sys_dup(0); // stdio
    sys_dup(0); // stderr
    loop {
        println!("init: starting sh");
        let pid = sys_fork();
        if pid < 0 {
            println!("init: fork failed");
            sys_exit(1);
        }
        if pid == 0 {
            let argv = [];
            sys_exec("sh\0", &argv);
            println!("init: exec failed");
            sys_exit(1);
        }

        loop {
            let mut status = 0i32;
            let wpid = sys_wait(&mut status);
            if wpid == pid {
                // the shell exited; restart it.
                break;
            } else if wpid < 0 {
                println!("init: wait returned an error");
                sys_exit(1);
            }
            // it was a parentless process; do nothing.
        }
    }
}
