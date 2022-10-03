#![no_std]
#![cfg_attr(test, no_main)]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(alloc_error_handler)]
#![feature(const_mut_refs)]
#![feature(allocator_api)]
#![feature(new_uninit)]

extern crate alloc;

use core::{
    panic::PanicInfo,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};

use cpu::CPU_TABLE;

use crate::{
    bio::BCACHE,
    cpu::CpuTable,
    e1000::E1000,
    param::{QEMU_EXIT_FAIL, QEMU_TEST0},
    process::PROCESS_TABLE,
    virtio::DISK,
};

mod bio;
mod bmap;
mod console;
mod cpu;
mod e1000;
mod file;
mod fs;
mod kalloc;
mod kvm;
mod log;
mod mbuf;
mod net;
mod page_table;
mod param;
mod pci;
mod plic;
pub mod printf;
mod proc;
mod process;
mod register;
mod sleeplock;
mod spinlock;
mod start;
mod superblock;
mod trap;
mod uart;
mod virtio;

pub static PANICKED: AtomicBool = AtomicBool::new(false);
static STARTED: AtomicBool = AtomicBool::new(false);

pub unsafe fn bootstrap() -> ! {
    let cpu_id = CpuTable::cpu_id();
    if cpu_id == 0 {
        console::init();
        println!("Hello, xv6 in Rust!");
        kalloc::heap_init(); // physical memory allocator
        kvm::init(); // create the kernel page table
        kvm::init_hart(); // turn on paging
        PROCESS_TABLE.init(); // process table
        trap::init_hart(); // install kernel trap vector
        plic::init(); // set up interrupt controller
        plic::init_hart(cpu_id); // ask PLIC for device interrupts
        BCACHE.init(); // buffer cache
        DISK.lock().init(); // emulated hard disk
        pci::init(); // pci

        PROCESS_TABLE.user_init(); // first user process
        STARTED.store(true, Ordering::SeqCst);
    } else {
        while !STARTED.load(Ordering::SeqCst) {}
        println!("hart {} starting...", cpu_id);
        kvm::init_hart(); // turn on paging
        trap::init_hart(); // install kernel trap vector
        plic::init_hart(cpu_id); // ask PLIC for device interrupts
    }

    CPU_TABLE.scheduler();
}

#[no_mangle]
fn abort() -> ! {
    panic!("abort");
}

#[cfg(test)]
#[panic_handler]
pub fn panic(info: &PanicInfo<'_>) -> ! {
    test_panic_handler(info)
}

pub fn test_panic_handler(info: &PanicInfo<'_>) -> ! {
    println!("failed: {}", info);
    PANICKED.store(true, Ordering::Relaxed);
    unsafe { ptr::write_volatile(QEMU_TEST0 as *mut u32, QEMU_EXIT_FAIL) };
    loop {}
}

#[cfg(test)]
pub fn test_runner(tests: &[&dyn Testable]) {
    println!("Running {} kernel tests", tests.len());
    for test in tests {
        test.run();
    }

    println!("\x1b[0;32mall kernel tests finished!\x1b[0m");

    crate::proc::usertests();

    unsafe { ptr::write_volatile(QEMU_TEST0 as *mut u32, crate::param::QEMU_EXIT_SUCCESS) };
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
unsafe fn main() -> ! {
    bootstrap();
}

#[cfg(test)]
mod tests {
    #[test_case]
    fn trivial_assertion() {
        assert_eq!(1, 1);
    }
}
