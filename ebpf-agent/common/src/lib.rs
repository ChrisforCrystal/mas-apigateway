#![no_std]

#[derive(Copy, Clone)]
#[repr(C)]
pub struct PacketLog {
    pub ipv4_src: u32,
    pub ipv4_dst: u32,
    pub port_src: u16,
    pub port_dst: u16,
    pub len: u32,
}

#[cfg(feature = "user")]
unsafe impl aya::Pod for PacketLog {}
