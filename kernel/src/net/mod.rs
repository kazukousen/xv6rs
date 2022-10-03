use core::convert::{TryFrom, TryInto};

use alloc::boxed::Box;

use crate::{cpu::CPU_TABLE, mbuf::MBuf};

mod arp;
mod ethernet;
mod ip;
mod tcp;
mod udp;

const USABLE_PORT_MIN: u16 = 49152;
const USABLE_PORT_MAX: u16 = 65535;

const LOCAL_IP_ADDR: u32 = build_ip_addr(10, 0, 2, 15);
const LOCAL_MAC_ADDR: [u8; 6] = [0x52, 0x54, 0x0, 0x12, 0x34, 0x56];
const GATEWAY_IP_ADDR: u32 = build_ip_addr(10, 0, 2, 2);
const GATEWAY_MAC_ADDR: [u8; 6] = [0x52, 0x55, 0xa, 0x0, 0x2, 0x2];

const ETHERNET_MAC_ADDR_ANY: [u8; 6] = [0, 0, 0, 0, 0, 0];
const ETHERNET_MAC_ADDR_BROADCAST: [u8; 6] = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff];

fn toggle_endian16(v: u16) -> u16 {
    ((0xff & v) << 8) | ((0xff00 & v) >> 8)
}

fn toggle_endian32(v: u32) -> u32 {
    ((v & 0xff) << 24) | ((v & 0xff00) << 8) | ((v & 0xff0000) >> 8) | ((v & 0xff000000) >> 24)
}

const fn build_ip_addr(v1: u32, v2: u32, v3: u32, v4: u32) -> u32 {
    (v1 << 24) | (v2 << 16) | (v3 << 8) | (v4 << 0)
}

pub fn rx(m: Box<MBuf>) {
    ethernet::rx(m);
}

#[repr(u16)]
pub enum SAFamily {
    UNSPEC = 0,
    LOCAL = 1,
    INET = 2,
}

#[repr(C)]
pub struct SockAddr {
    family: SAFamily,
    port: u16,
    addr: u32,
}

impl SockAddr {
    pub fn uninit() -> Self {
        Self {
            family: SAFamily::UNSPEC,
            port: 0,
            addr: 0,
        }
    }
}

pub enum SocketType {
    TCP,
    UDP,
}

impl TryFrom<u8> for SocketType {
    type Error = &'static str;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::TCP),
            1 => Ok(Self::UDP),
            _ => Err("undefined socket type"),
        }
    }
}

pub struct Socket {
    cb_idx: usize,
    typ: SocketType,
}

impl Socket {
    pub fn new(typ: u8) -> Result<Self, &'static str> {
        let typ = typ.try_into().unwrap();
        let cb_idx = match &typ {
            SocketType::TCP => panic!("unimplemented"),
            SocketType::UDP => {
                let cdp_idx = udp::open()?;
                cdp_idx
            }
        };

        Ok(Self { cb_idx, typ })
    }

    pub fn bind(&self, addr: &SockAddr) -> Result<(), &'static str> {
        match &self.typ {
            SocketType::TCP => panic!("unimplemented"),
            SocketType::UDP => udp::bind(self.cb_idx, &addr),
        }
    }

    pub fn connect(&self, addr: &SockAddr) -> Result<(), &'static str> {
        match &self.typ {
            SocketType::TCP => panic!("unimplemented"),
            SocketType::UDP => udp::connect(self.cb_idx, addr),
        }
    }

    fn close(&self) -> Result<(), &'static str> {
        match &self.typ {
            SocketType::TCP => panic!("unimplemented"),
            SocketType::UDP => udp::close(self.cb_idx),
        }
    }

    pub fn read(&self, addr: usize, len: usize) -> Result<usize, &'static str> {
        match &self.typ {
            SocketType::TCP => panic!("unimplemented"),
            SocketType::UDP => udp::read(self.cb_idx, addr, len),
        }
    }

    pub fn write(&self, addr: usize, len: usize) -> Result<usize, &'static str> {
        match &self.typ {
            SocketType::TCP => panic!("unimplemented"),
            SocketType::UDP => {
                // setup a mbuf and copy data from user space
                let mut m = MBuf::alloc(ethernet::HEADER_SIZE + ip::HEADER_SIZE + udp::HEADER_SIZE);
                m.append(len);
                let p = unsafe { CPU_TABLE.my_proc() };
                p.data.get_mut().copy_in(m.get_buf_head(), addr, len)?;
                udp::send(m, self.cb_idx)?;
            }
        }

        Ok(len)
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        self.close().expect("cannot close");
    }
}
