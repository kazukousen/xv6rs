use core::arch::asm;

#[inline]
pub unsafe fn write(v: usize) {
    asm!("csrw satp, {}", in(reg) v);
}

#[inline]
pub unsafe fn read() -> usize {
    let ret: usize;
    asm!("csrr {}, satp", out(reg) ret);
    ret
}
