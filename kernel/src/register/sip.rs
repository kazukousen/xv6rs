use core::arch::asm;

#[inline]
unsafe fn read() -> usize {
    let ret: usize;
    asm!("csrr {}, sip", out(reg) ret);
    ret
}

#[inline]
unsafe fn write(v: usize) {
    asm!("csrw sip, {}", in(reg) v);
}

// Supervisor Interrupt Pending
const SSIP: usize = 1 << 1;

#[inline]
pub fn clear_ssip() {
    unsafe {
        write(read() & !SSIP);
    }
}
