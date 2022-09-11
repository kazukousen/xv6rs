use alloc::boxed::Box;
use array_macro::array;

use crate::mbuf::MBuf;
use crate::println;
use crate::spinlock::SpinLock;
use core::convert::TryFrom;
use core::convert::TryInto;
use core::mem;
use core::ptr;

use super::ethernet;
use super::toggle_endian16;
use super::toggle_endian32;
use super::ETHERNET_MAC_ADDR_ANY;
use super::ETHERNET_MAC_ADDR_BROADCAST;
use super::GATEWAY_MAC_ADDR;
use super::LOCAL_IP_ADDR;
use super::LOCAL_MAC_ADDR;

const ETH_HTYPE: u16 = 1;
const IPV4_PTYPE: u16 = 0x800;
const ETH_HLEN: u8 = 6;
const IPV4_PLEN: u8 = 4;

#[repr(u16)]
#[derive(PartialEq, Debug)]
pub enum Operand {
    Request = 1,
    Reply = 2,
}

impl TryFrom<u16> for Operand {
    type Error = &'static str;
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Operand::Request),
            2 => Ok(Operand::Reply),
            _ => Err("undefined arp operand"),
        }
    }
}

#[repr(C, packed)]
struct Header {
    htype: u16,
    ptype: u16,
    hlen: u8,
    plen: u8,
    oper: u16,
    sha: [u8; 6],
    spa: u32,
    tha: [u8; 6],
    tpa: u32,
}

struct Entry {
    used: bool,
    ip_addr: u32,
    mac_addr: [u8; 6],
}

impl Entry {
    const fn new() -> Self {
        Self {
            used: false,
            ip_addr: 0,
            mac_addr: [0; 6],
        }
    }
}

const TABLE_SIZE: usize = 2048;

static mut TABLE: SpinLock<Table> = SpinLock::new(Table::new(), "arp_table");

struct Table {
    table: [Entry; TABLE_SIZE],
}

impl Table {
    const fn new() -> Self {
        Self {
            table: array![_ => Entry::new(); TABLE_SIZE],
        }
    }

    fn select(&mut self, ip_addr: u32) -> Option<&mut Entry> {
        for entry in self.table.iter_mut() {
            if entry.used && entry.ip_addr == ip_addr {
                return Some(entry);
            }
        }
        None
    }

    fn get_unused(&mut self) -> Option<&mut Entry> {
        for entry in self.table.iter_mut() {
            if !entry.used {
                return Some(entry);
            }
        }
        None
    }
}

pub fn tx(op: Operand, hw_addr: &[u8; 6], dst_mac: &[u8; 6], dst_ip: u32) {
    let mut m = MBuf::alloc(128);
    let mut hdr = unsafe { (m.append(mem::size_of::<Header>()) as *mut Header).as_mut() }.unwrap();
    hdr.htype = toggle_endian16(ETH_HTYPE);
    hdr.ptype = toggle_endian16(IPV4_PTYPE);
    hdr.hlen = ETH_HLEN;
    hdr.plen = IPV4_PLEN;
    hdr.oper = toggle_endian16(op as u16);
    unsafe { ptr::copy_nonoverlapping(LOCAL_MAC_ADDR.as_ptr(), hdr.sha.as_mut_ptr(), 6) };
    hdr.spa = toggle_endian32(LOCAL_IP_ADDR);
    unsafe { ptr::copy_nonoverlapping(hw_addr.as_ptr(), hdr.tha.as_mut_ptr(), 6) };
    hdr.tpa = toggle_endian32(dst_ip);

    ethernet::tx(m, ethernet::Type::ARP, dst_mac);
}

pub fn rx(mut m: Box<MBuf>) {
    let hdr = m.pop::<Header>(mem::size_of::<Header>());
    let op = toggle_endian16(hdr.oper);
    match op.try_into().unwrap() {
        Operand::Request => {
            tx(Operand::Reply, &hdr.sha, &hdr.sha, toggle_endian32(hdr.spa));
        }
        Operand::Reply => {
            // println!("arp_rx: reply op received");
            let mut guard = unsafe { TABLE.lock() };
            let entry = guard.get_unused().unwrap();
            entry.used = true;
            entry.ip_addr = toggle_endian32(hdr.spa);
            unsafe { ptr::copy_nonoverlapping(hdr.sha.as_ptr(), entry.mac_addr.as_mut_ptr(), 6) };
            drop(guard);
            return;
        }
    }
}

pub fn resolve(ip_addr: u32, mac_addr: &mut [u8; 6]) -> Result<(), &'static str> {
    let mut guard = unsafe { TABLE.lock() };
    let mut entry = guard.select(ip_addr);
    match entry {
        Some(entry) => {
            if entry.mac_addr == ETHERNET_MAC_ADDR_ANY {
                tx(
                    Operand::Request,
                    &ETHERNET_MAC_ADDR_ANY,
                    &ETHERNET_MAC_ADDR_BROADCAST,
                    ip_addr,
                );
                return Err("same");
            }
            unsafe { ptr::copy_nonoverlapping(entry.mac_addr.as_ptr(), mac_addr.as_mut_ptr(), 6) };
            return Ok(());
        }
        None => {
            entry = guard.get_unused();
        }
    }

    match entry {
        Some(_) => {
            tx(
                Operand::Request,
                &GATEWAY_MAC_ADDR,
                &ETHERNET_MAC_ADDR_BROADCAST,
                ip_addr,
            );
            return Err("cannot resolved");
        }
        None => return Err("full arp table"),
    }
}
