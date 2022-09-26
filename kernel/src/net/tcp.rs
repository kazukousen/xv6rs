use core::mem;

#[repr(C, packed)]
struct Header {
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    off: u8,
    flg: u8,
    win: u16,
    checksum: u16,
    urg: u16,
}

pub const HEADER_SIZE: usize = mem::size_of::<Header>();

struct ControlBlock {
    used: bool,
    state: bool,
    port: u16,
    peer_addr: u32,
    peer_port: u16,
    iss: u32,
    irs: u32,
    buf: [u8; 8192],
}
