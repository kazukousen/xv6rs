use core::arch::asm;

#[inline]
pub fn write_address0(addr: u64) {
    unsafe {
        asm!(
            "csrw pmpaddr0, {}",
            in(reg) addr,
        )
    }
}

const READ: usize = 1 << 0;
const WRITE: usize = 1 << 1;
const EXEC: usize = 1 << 2;

#[inline]
unsafe fn write_config(v: usize) {
    asm!("csrw pmpcfg0, {}", in(reg) v);
}

#[inline]
unsafe fn read_config() -> usize {
    let ret: usize;

    asm!("csrr {}, pmpcfg0", out(reg) ret);

    ret
}

#[inline]
pub fn set_config0() {
    unsafe {
        let mut v = read_config();
        v |= READ | WRITE | EXEC;
        v &= !(3 << 3);
        v |= 1 << 3; // TOR
        write_config(v);
    }
}
