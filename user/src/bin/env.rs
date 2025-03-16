#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(xv6rs_user::test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::{ptr, slice, str::from_utf8_unchecked};

use xv6rs_user::{
    entry_point,
    println,
    syscall::{sys_listenv, sys_mmap},
    Args,
};

entry_point!(main);

fn main(_args: &mut Args) -> Result<i32, &'static str> {
    // Define buffer size
    const BUF_SIZE: usize = 4096;
    
    // Use mmap to allocate buffer
    // PROT_READ | PROT_WRITE = 3
    // MAP_PRIVATE | MAP_ANONYMOUS = 6
    let buf_addr = sys_mmap(ptr::null(), BUF_SIZE, 3, 6, -1, 0);
    if buf_addr == usize::MAX {
        println!("Failed to allocate buffer with mmap");
        return Ok(1);
    }
    
    // Create a slice from the mmap'd memory
    let buf = unsafe { slice::from_raw_parts_mut(buf_addr as *mut u8, BUF_SIZE) };
    
    // Get environment variables list
    let len = sys_listenv(buf);
    
    if len <= 0 {
        if len == 0 {
            println!("No environment variables set.");
        } else {
            println!("Error listing environment variables.");
        }
        return Ok(0);
    }
    
    // Parse and display environment variables list
    let mut pos = 0;
    while pos < len as usize {
        // Find the end of current environment variable (null-terminated)
        let mut end = pos;
        while end < len as usize && buf[end] != 0 {
            end += 1;
        }
        
        // Get and display the environment variable
        let env_var = unsafe { from_utf8_unchecked(&buf[pos..end]) };
        println!("{}", env_var);
        
        // Move to next environment variable
        pos = end + 1;
    }
    
    // Note: We don't need to munmap as the process will exit and clean up resources
    
    Ok(0)
}
