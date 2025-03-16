# Userland Memory Allocation in xv6rs

This document describes the memory allocation system for userland programs in xv6rs, including how to use heap allocation features like `Box`, `Vec`, and `String`.

## Overview

xv6rs now supports heap allocation in userland programs through a custom memory allocator that uses the `sys_sbrk` system call to manage memory. This enables the use of Rust's standard collection types like `Box`, `Vec`, and `String` in userland programs.

## System Architecture

The memory allocation system consists of the following components:

1. **Kernel-side `sys_sbrk` system call**: Allocates or deallocates memory for a process
2. **Userland `sys_sbrk` interface**: Provides a Rust interface to the system call
3. **UserAllocator**: Implements the `GlobalAlloc` trait to manage memory allocation
4. **alloc crate integration**: Enables the use of standard collection types

### Kernel-side `sys_sbrk` System Call

The `sys_sbrk` system call in the kernel is responsible for growing or shrinking a process's memory space. It takes a single parameter `n` which specifies the number of bytes to add (if positive) or remove (if negative) from the process's memory space.

The implementation in `kernel/src/proc/syscall.rs` looks like this:

```rust
fn sys_sbrk(&mut self) -> SysResult {
    let n = self.arg_i32(0)?;
    let pdata = self.data.get_mut();
    let old_sz = pdata.sz; // Save the old size
    if n > 0 {
        pdata.sz = pdata
            .page_table
            .as_mut()
            .unwrap()
            .uvm_alloc(old_sz, old_sz + n as usize)?;
    } else if n < 0 {
        pdata.sz = pdata
            .page_table
            .as_mut()
            .unwrap()
            .uvm_dealloc(old_sz, old_sz + n as usize)?;
    }
    Ok(old_sz) // Return the old size (start address of the new memory)
}
```

The key aspect of this implementation is that it returns the old size of the process's memory space, which serves as the starting address of the newly allocated memory. This allows the userland allocator to know where the new memory begins.

### Userland `sys_sbrk` Interface

The userland interface to the `sys_sbrk` system call is defined in `user/src/syscall.rs`:

```rust
extern "C" {
    /// 12
    /// char *sbrk(int n)
    /// Grow process's memory by n bytes. Returns start of new memory.
    fn __sbrk(n: i32) -> *mut u8;
}

// 12
pub fn sys_sbrk(n: i32) -> *mut u8 {
    unsafe { __sbrk(n) }
}
```

This provides a safe Rust interface to the system call, allowing userland code to request memory from the kernel.

### UserAllocator

The `UserAllocator` in `user/src/allocator.rs` implements the `GlobalAlloc` trait, which is required for Rust's standard allocation types to work. It uses a linked list algorithm to manage memory blocks:

```rust
pub struct UserAllocator {
    inner: UnsafeCell<UserAllocatorInner>,
}

unsafe impl Sync for UserAllocator {}

struct UserAllocatorInner {
    head: ListNode,
    initialized: bool,
}

// ... implementation details ...

unsafe impl GlobalAlloc for UserAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // ... allocation logic ...
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // ... deallocation logic ...
    }
}

#[global_allocator]
pub static ALLOCATOR: UserAllocator = UserAllocator::new();
```

The allocator uses a linked list of free memory blocks. When memory is requested, it searches for a suitable block in the free list. If none is found, it requests more memory from the kernel using `sys_sbrk`.

### alloc Crate Integration

To use the standard collection types, the `alloc` crate is enabled in `user/src/lib.rs`:

```rust
extern crate alloc;

pub mod allocator;
```

This makes types like `Box`, `Vec`, and `String` available to userland programs.

## Using Heap Allocation in Userland Programs

To use heap allocation in a userland program, you need to include the `alloc` crate and import the types you want to use:

```rust
#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use alloc::string::String;
use xv6rs_user::{entry_point, println, Args};

entry_point!(main);
fn main(_: &mut Args) -> Result<i32, &'static str> {
    // Using Box
    let x = Box::new(42);
    println!("Box value: {}", *x);
    
    // Using Vec
    let mut v = Vec::new();
    v.push(1);
    v.push(2);
    v.push(3);
    println!("Vec: {:?}", v);
    
    // Using String
    let mut s = String::from("Hello");
    s.push_str(", world!");
    println!("String: {}", s);
    
    Ok(0)
}
```

## Memory Management Best Practices

When using heap allocation in userland programs, it's important to follow these best practices to avoid memory leaks and other issues:

1. **Use Rust's ownership system**: Let Rust's ownership and borrowing rules manage memory for you. When a value goes out of scope, Rust will automatically drop it and free the memory.

2. **Avoid circular references**: Circular references can lead to memory leaks. Use weak references (`Weak<T>`) when appropriate to break cycles.

3. **Be mindful of memory usage**: Userland programs have limited memory available. Avoid allocating large amounts of memory unnecessarily.

4. **Free memory when done**: While Rust's drop system will automatically free memory when values go out of scope, it's still a good practice to explicitly drop values when you're done with them, especially for long-running programs.

5. **Use appropriate collection types**: Choose the right collection type for your needs. For example, use `Vec` for dynamic arrays, `HashMap` for key-value pairs, and `String` for text.

## Example: Box Test Program

Here's an example program that tests various aspects of `Box` allocation:

```rust
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
```

## Limitations and Future Work

While the current implementation provides basic heap allocation functionality, there are some limitations and areas for future improvement:

1. **Performance**: The current allocator uses a simple linked list algorithm, which may not be optimal for all use cases. More sophisticated algorithms like buddy allocation or slab allocation could be implemented in the future.

2. **Memory fragmentation**: The current allocator does not have advanced defragmentation capabilities, which could lead to memory fragmentation over time.

3. **Memory statistics**: There is currently no way to query memory usage statistics from userland programs. This could be added in the future.

4. **Thread safety**: The current allocator is not designed for multi-threaded programs. If thread support is added to xv6rs in the future, the allocator would need to be updated to handle concurrent allocations.

## Conclusion

The addition of heap allocation support in userland programs greatly enhances the capabilities of xv6rs, allowing for more complex and flexible programs to be written. By leveraging Rust's ownership system and the `alloc` crate, userland programs can now use standard collection types like `Box`, `Vec`, and `String` while maintaining memory safety.
