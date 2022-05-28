use core::arch::asm;

#[inline]
pub unsafe fn write(v: usize) {
    asm!("mv tp, {}", in(reg) v);
}

#[inline]
pub unsafe fn read() -> usize {
    let ret: usize;
    asm!("mv {}, tp", out(reg) ret);
    ret
}
