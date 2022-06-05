#![no_std]
#![no_main]

use xv6rs_user::syscall::sys_exit;

#[no_mangle]
pub unsafe extern "C" fn _start() -> ! {
    sys_exit(42);
}
