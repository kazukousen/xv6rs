use core::arch::asm;

#[inline]
pub unsafe fn read() -> usize {
    let ret: usize;
    asm!("csrr {}, mhartid", out(reg) ret);
    ret
}
