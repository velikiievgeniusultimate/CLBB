//! Linux libc adapter. No dependencies and no resolver implementation of our own.
//! The declarations below are Linux libc ABI, deliberately separate from wire.rs.
use crate::wire::*;
use std::ffi::{c_char, c_int, c_void, CStr};
use std::ptr;

#[repr(C)]
struct AddrInfo {
    flags: c_int,
    family: c_int,
    socktype: c_int,
    protocol: c_int,
    addrlen: u32,
    addr: *mut c_void,
    canonname: *mut c_char,
    next: *mut AddrInfo,
}

#[repr(C)]
struct Sockaddr4 {
    family: u16,
    port: u16,
    address: [u8; 4],
    zero: [u8; 8],
}
#[repr(C)]
struct Sockaddr6 {
    family: u16,
    port: u16,
    flowinfo: u32,
    address: [u8; 16],
    scope_id: u32,
}

extern "C" {
    fn getaddrinfo(
        node: *const c_char,
        service: *const c_char,
        hints: *const AddrInfo,
        result: *mut *mut AddrInfo,
    ) -> c_int;
    fn freeaddrinfo(result: *mut AddrInfo);
    fn __errno_location() -> *mut c_int;
}

struct OwnedList(*mut AddrInfo);
impl Drop for OwnedList {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { freeaddrinfo(self.0) };
        }
    }
}

fn host_flags(flags: u32) -> c_int {
    let mut out = 0;
    for (wire, native) in [
        (PASSIVE, 0x1),
        (CANONNAME, 0x2),
        (NUMERICHOST, 0x4),
        (NUMERICSERV, 0x400),
        (ADDRCONFIG, 0x20),
        (V4MAPPED, 0x8),
        (ALL, 0x10),
    ] {
        if flags & wire != 0 {
            out |= native;
        }
    }
    out
}

fn status(native: c_int) -> i32 {
    match native {
        -1 => BADFLAGS,
        -2 => NONAME,
        -3 => AGAIN,
        -4 => FAIL,
        -5 => NODATA,
        -6 => FAMILY,
        -7 => SOCKTYPE,
        -8 => SERVICE,
        -9 => ADDRFAMILY,
        -10 => MEMORY,
        -11 => SYSTEM,
        -12 => OVERFLOW,
        _ => FAIL,
    }
}

// Caller has already validated the C strings and pointer lifetimes.
pub unsafe fn resolve(
    node: *const c_char,
    service: *const c_char,
    hints: Hints,
    response: &mut Response,
) -> i32 {
    if hints.version != VERSION {
        return BADHINTS;
    }
    if hints.flags & !FLAG_MASK != 0 {
        return BADFLAGS;
    }
    let family = match hints.family {
        0 => 0,
        4 => 2,
        6 => 10,
        _ => return FAMILY,
    };
    if hints.socktype > 2 {
        return SOCKTYPE;
    }
    if !matches!(hints.protocol, 0 | 6 | 17) {
        return PROTOCOL;
    }
    if (hints.socktype == 1 && hints.protocol == 17) || (hints.socktype == 2 && hints.protocol == 6)
    {
        return BADHINTS;
    }
    let native_hints = AddrInfo {
        flags: host_flags(hints.flags),
        family,
        socktype: hints.socktype as c_int,
        protocol: hints.protocol as c_int,
        addrlen: 0,
        addr: ptr::null_mut(),
        canonname: ptr::null_mut(),
        next: ptr::null_mut(),
    };
    let mut head = ptr::null_mut();
    let rc = getaddrinfo(node, service, &native_hints, &mut head);
    if rc != 0 {
        let result = status(rc);
        if result == SYSTEM {
            response.system_errno = *__errno_location();
        }
        return result;
    }
    let _list = OwnedList(head);
    let mut current = head;
    while !current.is_null() {
        let ai = &*current;
        // getaddrinfo with unspecified socktype can additionally return raw
        // socket records. This adapter explicitly supports TCP and UDP only.
        if !matches!(ai.socktype, 1 | 2) || !matches!(ai.protocol, 6 | 17) {
            current = ai.next;
            continue;
        }
        let mut record = Record {
            socktype: ai.socktype as u32,
            protocol: ai.protocol as u32,
            ..Record::default()
        };
        if ai.addr.is_null() {
            return FAIL;
        }
        match ai.family {
            2 if ai.addrlen as usize >= std::mem::size_of::<Sockaddr4>() => {
                let sa = ptr::read_unaligned(ai.addr.cast::<Sockaddr4>());
                if sa.family != 2 {
                    return FAIL;
                }
                record.family = 4;
                record.port = u16::from_be(sa.port);
                record.address[..4].copy_from_slice(&sa.address);
            }
            10 if ai.addrlen as usize >= std::mem::size_of::<Sockaddr6>() => {
                let sa = ptr::read_unaligned(ai.addr.cast::<Sockaddr6>());
                if sa.family != 10 {
                    return FAIL;
                }
                record.family = 6;
                record.port = u16::from_be(sa.port);
                record.flowinfo = u32::from_be(sa.flowinfo);
                record.scope_id = sa.scope_id;
                record.address = sa.address;
            }
            _ => return FAMILY,
        }
        if response.canonical_len == 0 && !ai.canonname.is_null() {
            let name = CStr::from_ptr(ai.canonname).to_bytes();
            if name.len() >= CANON_CAPACITY {
                return OVERFLOW;
            }
            response.canonical[..name.len()].copy_from_slice(name);
            response.canonical_len = name.len() as u32;
        }
        let count = response.count as usize;
        if count == MAX_RECORDS {
            return OVERFLOW;
        }
        response.records[count] = record;
        response.count += 1;
        current = ai.next;
    }
    if response.count == 0 {
        NODATA
    } else {
        0
    }
}
