use crate::uart;


pub fn init() {
    uart::init();
}

pub fn putc(c: u8) {
    uart::putc_sync(c);
}

