//! Pointer-free v1 payload. Integers use little endian on the supported hosts.
//! Transport must copy these values; it must never pass a native addrinfo list.
pub const VERSION: u32 = 1;
pub const MAX_RECORDS: usize = 64;
pub const CANON_CAPACITY: usize = 1024;
pub const PASSIVE: u32 = 1;
pub const CANONNAME: u32 = 2;
pub const NUMERICHOST: u32 = 4;
pub const NUMERICSERV: u32 = 8;
pub const ADDRCONFIG: u32 = 16;
pub const V4MAPPED: u32 = 32;
pub const ALL: u32 = 64;
pub const FLAG_MASK: u32 = 127;

// Stable statuses equal this guest bionic's positive EAI_* values, not glibc's.
pub const ADDRFAMILY: i32 = 1;
pub const AGAIN: i32 = 2;
pub const BADFLAGS: i32 = 3;
pub const FAIL: i32 = 4;
pub const FAMILY: i32 = 5;
pub const MEMORY: i32 = 6;
pub const NODATA: i32 = 7;
pub const NONAME: i32 = 8;
pub const SERVICE: i32 = 9;
pub const SOCKTYPE: i32 = 10;
pub const SYSTEM: i32 = 11;
pub const BADHINTS: i32 = 12;
pub const PROTOCOL: i32 = 13;
pub const OVERFLOW: i32 = 14;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Hints {
    pub version: u32,
    pub flags: u32,
    /// 0 = unspecified, 4 = IPv4, 6 = IPv6; never native AF_* values.
    pub family: u32,
    /// 0 = unspecified, 1 = stream, 2 = datagram.
    pub socktype: u32,
    /// 0 = unspecified, 6 = TCP, 17 = UDP.
    pub protocol: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Record {
    pub family: u32,
    pub socktype: u32,
    pub protocol: u32,
    /// Host-order value, not the sockaddr's network-order port bytes.
    pub port: u16,
    pub reserved: u16,
    pub scope_id: u32,
    /// Network-order IPv6 flowinfo decoded into an integer.
    pub flowinfo: u32,
    /// Address bytes in network order; IPv4 occupies the first four bytes.
    pub address: [u8; 16],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Response {
    pub version: u32,
    pub count: u32,
    /// Length excluding the final NUL; zero means absent.
    pub canonical_len: u32,
    /// Linux/Android errno for SYSTEM; zero otherwise. Windows needs mapping.
    pub system_errno: i32,
    pub canonical: [u8; CANON_CAPACITY],
    pub records: [Record; MAX_RECORDS],
}

impl Default for Response {
    fn default() -> Self {
        Self {
            version: VERSION,
            count: 0,
            canonical_len: 0,
            system_errno: 0,
            canonical: [0; CANON_CAPACITY],
            records: [Record::default(); MAX_RECORDS],
        }
    }
}

const _: () = {
    assert!(std::mem::size_of::<Hints>() == 20);
    assert!(std::mem::size_of::<Record>() == 40);
    assert!(std::mem::offset_of!(Record, address) == 24);
    assert!(std::mem::offset_of!(Response, records) == 1040);
    assert!(std::mem::size_of::<Response>() == 3600);
};
