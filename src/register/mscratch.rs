use core::arch::asm;

#[inline]
pub unsafe fn write(v: u64) {
    asm!("csrw mscratch, {}", in(reg) v);
}
