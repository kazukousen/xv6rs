use core::arch::asm;

use crate::{register, param::NCPU};


#[no_mangle]
static STACK0: [u8; 4096 * NCPU] = [0; 4096 * NCPU];

#[no_mangle]
static TIMER_SCRATCH: [[usize; 5]; NCPU] = [[0; 5]; NCPU];

#[no_mangle]
unsafe fn start() -> ! {
    // set MPP mode to supervisor, for mret.
    register::mstatus::set_mpp(register::mstatus::MPPMode::Supervisor);

    // set mepc to main, for mret.
    extern "Rust" {
        fn main();
}
    register::mepc::write(main as usize);

    // diable paging for now.
    register::satp::write(0);

    // delegate all interrupts and exceptions to supervisor mode.
    register::medeleg::write(0xffff);
    register::mideleg::write(0xffff);
    register::sie::intr_on();

    // configure PMP to give supervisor mode access to all of physical memory.
    register::pmp::write_address0(!(0) >> 10);
    register::pmp::set_config0();

    // ask for clock interrupts.
    timerinit();

    // keep each CPU's hartid in its tp register, for cpu_id().
    let id = register::mhartid::read();
    register::tp::write(id);

    // switch to suervisor mode and jump to main().
    asm!("mret");

    loop {}
}

unsafe fn timerinit() {
    let id = register::mhartid::read();

    // ask the CLINT for a timer interrupt.
    let interval = 1000000; // cycles; about 1/10th second in qemu.
    register::clint::add_mtimecmp(id, interval);

    let mut arr = TIMER_SCRATCH[id];
    arr[3] = register::clint::CLINT_MTIMECMP + 8 * id;
    arr[4] = interval as usize;
    register::mscratch::write(arr.as_ptr() as u64);

    // Set the machine-mode trap handler.
    extern "C" {
        fn timervec();
    }
    register::mtvec::write(timervec as usize);

    // Enable machine interrupt.
    register::mstatus::intr_on(register::mstatus::MPPMode::Machine);

    // Enable machine-mode timer interrupt.
    register::mie::enable_machine_timer_interrupt();
}
