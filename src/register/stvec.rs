use core::arch::asm;

#[inline]
pub unsafe fn write(v: usize) {
    asm!("csrw stvec, {}", in(reg) v);
}
