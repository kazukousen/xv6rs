use core::arch::asm;

#[inline]
pub unsafe fn write(v: usize) {
    asm!("csrw medeleg, {}", in(reg) v);
}
