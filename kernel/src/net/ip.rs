use alloc::boxed::Box;
use core::mem;

use crate::{mbuf::MBuf, println};

use super::{
    arp, ethernet, toggle_endian16, toggle_endian32, udp, ETHERNET_MAC_ADDR_BROADCAST,
    GATEWAY_MAC_ADDR, LOCAL_IP_ADDR,
};

#[repr(u8)]
#[derive(PartialEq, Debug)]
pub enum Protocol {
    TCP = 6,
    UDP = 17,
}

#[repr(C, packed)]
struct Header {
    hdr_ver: u8,
    tos: u8,
    len: u16,
    id: u16,
    fragment: u16,
    ttl: u8,
    protocol: Protocol,
    checksum: u16,
    src_ip_addr: u32,
    dst_ip_addr: u32,
}

pub const HEADER_SIZE: usize = mem::size_of::<Header>();

impl Header {
    fn checksum(&mut self, init: u32) {
        let mut nleft = mem::size_of::<Self>();
        let mut w = self as *const _ as usize as u16;
        let mut sum = init;

        while nleft > 1 {
            w += 1;
            sum += w as u32;
            nleft -= 2;
        }

        if nleft != 0 {
            sum += w as u32 & 0xff;
        }

        while sum >> 16 != 0 {
            sum = (sum >> 16) + (sum & 0xffff);
        }

        self.checksum = (sum ^ 0xffff) as u16
    }
}

pub fn tx(mut m: Box<MBuf>, dst_ip_addr: u32, proto: Protocol) {
    let mut hdr = m.prepend::<Header>(HEADER_SIZE);
    hdr.hdr_ver = (4 << 4) | 5; // hdr.hdr = 5, hdr.ver = 4
    hdr.tos = 0;
    hdr.len = toggle_endian16(m.get_len() as u16);
    hdr.ttl = 100;
    hdr.protocol = proto;
    hdr.src_ip_addr = toggle_endian32(LOCAL_IP_ADDR);
    hdr.dst_ip_addr = toggle_endian32(dst_ip_addr);
    hdr.checksum(0);

    if dst_ip_addr == 0 {
        // broadcast
        ethernet::tx(m, ethernet::Type::IPv4, &ETHERNET_MAC_ADDR_BROADCAST);
        return;
    }

    // resolve the mac address
    let mut dst_mac = [0u8; 6];
    match arp::resolve(dst_ip_addr, &mut dst_mac) {
        Ok(_) => {
            // resolved
            ethernet::tx(m, ethernet::Type::IPv4, &dst_mac);
            return;
        }
        Err(_) => {
            ethernet::tx(m, ethernet::Type::IPv4, &GATEWAY_MAC_ADDR);
            return;
        }
    }
}

pub fn rx(mut m: Box<MBuf>) {
    let hdr = m.pop::<Header>(HEADER_SIZE);
    // TODO: is_ip_packet_valid

    let len = toggle_endian16(hdr.len) - HEADER_SIZE as u16;

    m.trim(m.get_len() - len as usize);

    match hdr.protocol {
        Protocol::TCP => {
            panic!("ip_rx: handling tcp is unimpelemented yet");
        }
        Protocol::UDP => match udp::rx(m, toggle_endian32(hdr.src_ip_addr)) {
            Err(msg) => {
                println!("ip_rx: udp failed: {}", msg);
            }
            Ok(_) => return,
        },
    }
}
