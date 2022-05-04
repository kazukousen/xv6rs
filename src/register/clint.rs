use core::ptr;

const CLINT_MTIME: usize = 0x200bff8;
pub const CLINT_MTIMECMP: usize = 0x2004000;

#[inline]
unsafe fn read_mtime() -> u64 {
    ptr::read_volatile(CLINT_MTIME as *const u64)
}

#[inline]
unsafe fn write_mtimecmp(mhartid: usize, v: u64) {
    let offset = CLINT_MTIMECMP + 8 * mhartid;
    ptr::write_volatile(offset as *mut u64, v);
}

pub unsafe fn add_mtimecmp(mhartid: usize, interval: u64) {
    let v = read_mtime();
    write_mtimecmp(mhartid, v + interval);
}
