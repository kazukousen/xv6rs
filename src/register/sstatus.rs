use core::arch::asm;

#[inline]
unsafe fn read() -> usize {
    let ret: usize;
    asm!("csrr {}, sstatus", out(reg) ret);
    ret
}

#[inline]
unsafe fn write(v: usize) {
    asm!("csrw sstatus, {}", in(reg) v);
}

pub enum Mode {
    SIE = 1,
    SPIE = 5,
    SPP = 8,
}

#[inline]
pub unsafe fn intr_on() {
    let mut v = read();
    v |= 1 << (Mode::SIE as usize);
    write(v);
}

#[inline]
pub unsafe fn intr_off() {
    let mut v = read();
    v &= !(1 << (Mode::SIE as usize));
    write(v);
}

// are device interrupts enabled?
#[inline]
pub fn intr_get() -> bool {
    let x = unsafe { read() };
    (x & Mode::SIE as usize) != 0
}
