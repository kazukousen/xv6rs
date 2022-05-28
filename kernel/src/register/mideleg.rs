use core::arch::asm;

#[inline]
pub unsafe fn write(v: usize) {
    asm!("csrw mideleg, {}", in(reg) v);
}
