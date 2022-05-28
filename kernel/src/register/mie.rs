use core::arch::asm;

#[inline]
unsafe fn read() -> usize {
    let ret: usize;
    asm!("csrr {}, mie", out(reg) ret);
    ret
}

#[inline]
unsafe fn write(v: usize) {
    asm!("csrw mie, {}", in(reg) v);
}

pub unsafe fn enable_machine_timer_interrupt() {
    let mut mie = read();
    mie |= 1 << 7;
    write(mie);
}
