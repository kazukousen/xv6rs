use core::arch::asm;

#[inline]
pub unsafe fn write(v: usize) {
    asm!("csrw sepc, {}", in(reg) v);
}

#[inline]
pub unsafe fn read() -> usize {
    let ret: usize;
    asm!("csrr {}, sepc", out(reg) ret);
    ret
}
