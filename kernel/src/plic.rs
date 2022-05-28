use core::ptr;

use crate::{cpu, param};

pub unsafe fn init() {
    write(param::UART0_IRQ * 4, 1);
    write(param::VIRTIO0_IRQ * 4, 1);
}

pub unsafe fn init_hart(hart: usize) {
    write(
        SENABLE + SENABLE_HART * hart,
        (1 << param::UART0_IRQ) | (1 << param::VIRTIO0_IRQ),
    );
    write(SPRIORITY + SPRIORITY_HART * hart, 0);
}

pub unsafe fn complete(irq: u32) {
    let hart: usize = cpu::CpuTable::cpu_id();
    write(SCLAIM + SCLAIM_HART * hart, irq);
}

pub unsafe fn claim() -> u32 {
    let hart = cpu::CpuTable::cpu_id();
    read(SCLAIM + SCLAIM_HART * hart)
}

#[inline]
unsafe fn write(offset: usize, v: u32) {
    let dst = (param::PLIC + offset) as *mut u32;
    ptr::write_volatile(dst, v);
}

#[inline]
unsafe fn read(offset: usize) -> u32 {
    let src = (param::PLIC + offset) as *const u32;
    ptr::read_volatile(src)
}

const SENABLE: usize = 0x2080;
const SENABLE_HART: usize = 0x100;
const SPRIORITY: usize = 0x201000;
const SPRIORITY_HART: usize = 0x2000;
const SCLAIM: usize = 0x201004;
const SCLAIM_HART: usize = 0x2000;
