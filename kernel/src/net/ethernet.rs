use alloc::boxed::Box;

use core::{
    convert::{TryFrom, TryInto},
    mem, ptr,
};

use crate::{e1000::E1000, mbuf::MBuf, println};

use super::{arp, ip, toggle_endian16, LOCAL_MAC_ADDR};

#[repr(u16)]
#[derive(PartialEq, Debug)]
pub enum Type {
    IPv4 = 0x800,
    ARP = 0x806,
}

impl TryFrom<u16> for Type {
    type Error = &'static str;
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0x800 => Ok(Type::IPv4),
            0x806 => Ok(Type::ARP),
            _ => Err("undefined ethernet type"),
        }
    }
}

pub const HEADER_SIZE: usize = mem::size_of::<Header>();

#[repr(C, packed)]
struct Header {
    dst_mac: [u8; 6],
    src_mac: [u8; 6],
    typ: u16,
}

pub fn tx(mut m: Box<MBuf>, typ: Type, dst_mac: &[u8; 6]) {
    let mut hdr = m.prepend::<Header>(HEADER_SIZE);
    unsafe { ptr::copy(dst_mac.as_ptr(), hdr.dst_mac.as_mut_ptr(), 6) };
    unsafe { ptr::copy(LOCAL_MAC_ADDR.as_ptr(), hdr.src_mac.as_mut_ptr(), 6) };
    hdr.typ = toggle_endian16(typ as u16);

    match E1000.lock().send(m) {
        Ok(_) => {}
        Err(msg) => {
            println!("failed to send packet: {}", msg);
        }
    }
}

pub fn rx(mut m: Box<MBuf>) {
    let hdr = m.pop::<Header>(HEADER_SIZE);

    let typ = toggle_endian16(hdr.typ);
    match typ.try_into().unwrap() {
        Type::IPv4 => {
            ip::rx(m);
            return;
        }
        Type::ARP => {
            arp::rx(m);
            return;
        }
    }
}
