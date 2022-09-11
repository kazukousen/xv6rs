#[repr(u16)]
pub enum SAFamily {
    UNSPEC = 0,
    LOCAL = 1,
    INET = 2,
}

#[repr(C)]
pub struct SockAddr {
    pub family: SAFamily,
    pub port: u16,
    pub addr: u32,
}
