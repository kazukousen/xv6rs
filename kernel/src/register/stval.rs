use core::arch::asm;

#[inline]
pub unsafe fn read() -> usize {
    let ret: usize;
    asm!("csrr {}, stval", out(reg) ret);
    ret
}
