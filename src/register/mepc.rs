use core::arch::asm;

#[inline]
pub unsafe fn write(v: usize) {
    asm!("csrw mepc, {}", in(reg) v);
}
