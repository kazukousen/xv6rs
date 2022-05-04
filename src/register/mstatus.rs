use core::arch::asm;

#[inline]
unsafe fn read() -> usize {
    let ret: usize;
    asm!("csrr {}, mstatus", out(reg) ret);
    ret
}

#[inline]
unsafe fn write(v: usize) {
    asm!("csrw mstatus, {}", in(reg) v);
}

pub enum MPPMode {
    User = 0,
    Supervisor = 1,
    Machine = 3,
}

#[inline]
pub unsafe fn set_mpp(mode: MPPMode) {
    let mut mstatus = read();
    mstatus &= !(3 << 11);
    mstatus |= (mode as usize) << 11;
    write(mstatus);
}

#[inline]
pub unsafe fn intr_on(mode: MPPMode) {
    let mut mstatus = read();
    mstatus |= 1 << (mode as usize);
    write(mstatus);
}
