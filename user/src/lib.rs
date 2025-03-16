#![no_std] // Do not use the Rust standard library, suitable for bare-metal or OS development
#![cfg_attr(test, no_main)] // Use a custom main function for tests
#![feature(custom_test_frameworks)] // Enable custom test frameworks
#![test_runner(crate::test_runner)] // Define the test runner function
#![reexport_test_harness_main = "test_main"] // Re-export the test harness main function
#![feature(alloc_error_handler)] // Enable custom allocation error handling

// External crates
extern crate alloc; // Use the alloc crate for heap allocation

pub mod allocator;
pub mod fcntl;
pub mod fstat;
pub mod net;
pub mod printf;
pub mod syscall;

use core::{panic::PanicInfo, slice::from_raw_parts, str::from_utf8_unchecked};

use crate::syscall::sys_exit;

#[panic_handler] // Custom panic handler for the kernel
pub fn panic(info: &PanicInfo<'_>) -> ! {
    #[cfg(test)]
    test_panic_handler(info);

    println!("panic: {}", info);
    sys_exit(1)
}

#[no_mangle] // Ensure the function name is not mangled, used for abort
fn abort() -> ! {
    panic!("abort");
}

/// Struct to handle command-line arguments for user programs
pub struct Args {
    length: usize,
    args: &'static [*const u8],
    pos: usize,
}

impl Args {
    /// Create a new Args instance from argc and argv
    pub fn new(argc: i32, argv: *const *const u8) -> Self {
        Self {
            length: argc as usize,
            args: unsafe { from_raw_parts(argv, argc as usize) },
            pos: 0,
        }
    }
}

impl Iterator for Args {
    type Item = &'static str;
    /// Retrieve the next argument as a string, if available
    ///
    /// This method advances the position in the argument list and returns
    /// the current argument as a string slice. It uses unsafe operations
    /// to convert raw C-style strings to Rust string slices.
    ///
    /// # Safety
    /// The method assumes that the argument pointers are valid and that
    /// the strings are null-terminated. It uses `from_raw_parts` to create
    /// a byte slice and `from_utf8_unchecked` to convert it to a string slice.
    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.length {
            return None;
        }

        let arg: &*const u8 = unsafe { self.args.get_unchecked(self.pos) };

        self.pos += 1;

        let s: &[u8] = unsafe { from_raw_parts(*arg, strlen(*arg)) };
        let s: &str = unsafe { from_utf8_unchecked(s) };
        Some(s)
    }
}

/// Calculate the length of a C-style string
pub fn strlen(mut c: *const u8) -> usize {
    let mut pos = 0;
    unsafe {
        while *c != 0 {
            pos += 1;
            c = c.offset(1);
        }
    }
    pos
}

/// wrapper so that it's ok main() does not call sys_exit()
#[macro_export] // Export the macro for use in other modules
macro_rules! entry_point {
    ($path:path) => {
        #[export_name = "_start"]
        pub extern "C" fn __impl_start(argc: i32, argv: *const *const u8) {
            #[cfg(test)]
            crate::test_main();

            let mut args = $crate::Args::new(argc, argv);

            let f: fn(&mut $crate::Args) -> Result<i32, &'static str> = $path;

            match f(&mut args) {
                Ok(c) => {
                    $crate::syscall::sys_exit(c);
                }
                Err(msg) => {
                    $crate::println!("fatal: {}", msg);
                    $crate::syscall::sys_exit(1);
                }
            }
        }
    };
}

#[cfg(test)]
fn test_panic_handler(info: &PanicInfo<'_>) {
    println!("failed: {}", info);
    sys_exit(1);
}

/// Custom test runner for executing tests
pub fn test_runner(tests: &[&dyn Testable]) {
    println!("Running {} tests", tests.len());
    for test in tests {
        test.run();
    }

    println!("\x1b[0;32mall tests finished!\x1b[0m");
    sys_exit(0);
}

pub trait Testable {
    fn run(&self) -> ();
}

impl<T> Testable for T
where
    T: Fn(),
{
    fn run(&self) {
        print!("{}...\t", core::any::type_name::<T>());
        self();
        println!("\x1b[0;32m[ok]\x1b[0m");
    }
}

#[cfg(test)]
#[no_mangle]
extern "C" fn _start() {
    test_main();
}

#[cfg(test)]
mod tests {
    use core::{ptr, str::from_utf8_unchecked};

    use crate::{
        fcntl::{O_CREATE, O_RDWR, O_WRONLY},
        syscall::{
            sys_chdir, sys_close, sys_getenv, sys_listenv, sys_mkdir, sys_open, sys_setenv,
            sys_unlink, sys_unsetenv, sys_write,
        },
    };

    use super::*;

    #[test_case]
    fn test_iput() {
        assert!(sys_mkdir("iputdir\0") >= 0);
        assert!(sys_chdir("iputdir\0") >= 0);
        assert!(sys_unlink("../iputdir\0") >= 0);
        assert!(sys_chdir("/\0") >= 0);
    }

    #[test_case]
    fn write_bytes() {
        let mut buf: [u8; 4] = [0; 4];
        buf[0] = b'a';
        buf[1] = b'b';
        buf[2] = b'c';
        buf[3] = b'd';

        assert_eq!(&buf, &[b'a', b'b', b'c', b'd']);

        unsafe {
            ptr::write_bytes(buf.as_mut_ptr(), 0, 2);
        }

        assert_eq!(&buf, &[0, 0, b'c', b'd']);
    }

    #[test_case]
    fn test_env_basic() {
        // Simple test for environment variables

        // Set a variable
        let result = sys_setenv("TEST_ENV", "test_value", true);
        assert!(result >= 0, "sys_setenv failed");

        // Get the variable
        let mut buf = [0u8; 64];
        let len = sys_getenv("TEST_ENV", &mut buf);
        assert!(len >= 0, "sys_getenv failed");

        // List environment variables
        let mut list_buf = [0u8; 128];
        let list_len = sys_listenv(&mut list_buf);
        assert!(list_len >= 0, "sys_listenv failed");

        // Unset the variable
        let unset_result = sys_unsetenv("TEST_ENV");
        assert!(unset_result >= 0, "sys_unsetenv failed");
    }
}
