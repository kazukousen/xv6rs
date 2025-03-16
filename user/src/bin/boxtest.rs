#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(xv6rs_user::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use alloc::boxed::Box;
use xv6rs_user::{entry_point, println, Args};

entry_point!(main);
fn main(_: &mut Args) -> Result<i32, &'static str> {
    // Allocate a Box with an integer
    let x = Box::new(42);
    println!("Box value: {}", *x);
    
    // Allocate another Box
    let y = Box::new(100);
    println!("Another Box value: {}", *y);
    
    // Free the first Box
    drop(x);
    
    // Allocate a Box with a string
    let z = Box::new("Hello, Box!");
    println!("String Box: {}", z);
    
    // Allocate a larger Box with an array
    let arr = Box::new([1, 2, 3, 4, 5]);
    println!("Array Box: {:?}", arr);
    
    println!("All Box tests passed!");
    
    Ok(0)
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::vec::Vec;
    use xv6rs_user::syscall::{sys_exec, sys_fork, sys_wait};

    #[test_case]
    fn test_box_integer() {
        let x = Box::new(42);
        assert_eq!(*x, 42);
    }

    #[test_case]
    fn test_box_drop() {
        // Allocate and drop multiple boxes to test memory management
        for i in 0..100 {
            let x = Box::new(i);
            assert_eq!(*x, i);
            // Box is dropped here
        }
    }

    #[test_case]
    fn test_box_array() {
        let arr = Box::new([1, 2, 3, 4, 5]);
        assert_eq!(arr[0], 1);
        assert_eq!(arr[4], 5);
    }

    #[test_case]
    fn test_box_large_allocation() {
        // Test allocating a larger object
        let large_vec = Box::new([0u8; 1024]);
        assert_eq!(large_vec[0], 0);
        assert_eq!(large_vec[1023], 0);
    }

    #[test_case]
    fn test_box_in_process() {
        // Test Box in a child process
        let pid = sys_fork();
        assert!(pid >= 0);
        if pid == 0 {
            // Child process
            let x = Box::new(42);
            assert_eq!(*x, 42);
            xv6rs_user::syscall::sys_exit(123); // Exit with a specific code
        }
        
        // Parent process
        let mut status = 0i32;
        let wpid = sys_wait(&mut status);
        assert_eq!(pid, wpid);
        assert_eq!(123i32, status); // Check if child exited with the expected code
    }
}
