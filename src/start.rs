
#[no_mangle]
static STACK0: [u8; 4096 * 4] = [0; 4096 * 4];

#[no_mangle]
fn start() -> ! {
    loop {}
}
