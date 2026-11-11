//! Linux runtime adapter. The common controller and source decoding have no /proc dependency.
use super::{MAX_READ_SPANS, Memory, Region, validate_read_layout};
use std::{ffi::c_void, fs};

#[repr(C)]
#[derive(Clone, Copy)]
struct IoVec {
    base: *mut c_void,
    length: usize,
}
unsafe extern "C" {
    fn process_vm_readv(
        pid: i32,
        local: *const IoVec,
        local_count: usize,
        remote: *const IoVec,
        remote_count: usize,
        flags: usize,
    ) -> isize;
    fn clock_gettime(clock: i32, time: *mut Timespec) -> i32;
}
#[repr(C)]
struct Timespec {
    seconds: i64,
    nanoseconds: i64,
}
pub fn now_ns() -> i64 {
    let mut t = Timespec {
        seconds: 0,
        nanoseconds: 0,
    };
    unsafe { clock_gettime(1, &mut t) };
    t.seconds * 1_000_000_000 + t.nanoseconds
}

pub struct ProcessMemory {
    pid: u32,
}
impl ProcessMemory {
    pub fn current() -> Self {
        Self {
            pid: std::process::id(),
        }
    }
    pub fn diagnostic(pid: u32) -> Result<Self, String> {
        let exe = fs::read_link(format!("/proc/{pid}/exe")).map_err(|e| e.to_string())?;
        if exe.file_name().and_then(|p| p.to_str()) != Some("android-translation-layer") {
            return Err("Diagnostic source is not an ATL runtime".into());
        }
        Ok(Self { pid })
    }
}
impl Memory for ProcessMemory {
    fn regions(&self) -> Result<Vec<Region>, String> {
        let maps =
            fs::read_to_string(format!("/proc/{}/maps", self.pid)).map_err(|e| e.to_string())?;
        let mut regions = Vec::new();
        for line in maps.lines() {
            let mut fields = line.split_whitespace();
            let (start, end) = fields
                .next()
                .and_then(|v| v.split_once('-'))
                .ok_or("Invalid memory map")?;
            let perms = fields.next().ok_or("Invalid memory map permissions")?;
            for _ in 0..3 {
                fields.next().ok_or("Invalid memory map fields")?;
            }
            regions.push(Region {
                start: u64::from_str_radix(start, 16).map_err(|e| e.to_string())?,
                end: u64::from_str_radix(end, 16).map_err(|e| e.to_string())?,
                readable: perms.starts_with('r'),
                writable: perms.as_bytes().get(1) == Some(&b'w'),
                name: fields.collect::<Vec<_>>().join(" "),
            });
        }
        Ok(regions)
    }
    fn read(&self, address: u64, length: usize) -> Result<Vec<u8>, String> {
        if length > 1024 * 1024 {
            return Err("Read exceeds one bounded span".into());
        }
        let mut bytes = vec![0; length];
        self.read_into(address, &mut bytes)?;
        Ok(bytes)
    }
    fn read_vectored(&self, spans: &[(u64, usize)], bytes: &mut [u8]) -> Result<(), String> {
        validate_read_layout(spans, bytes.len())?;
        if bytes.is_empty() {
            return Ok(());
        }
        let mut remote = [IoVec {
            base: std::ptr::null_mut(),
            length: 0,
        }; MAX_READ_SPANS];
        for (i, &(address, length)) in spans.iter().enumerate() {
            if length > 0 && address < 4096 {
                return Err("Invalid memory vector address".into());
            }
            remote[i] = IoVec {
                base: address as *mut c_void,
                length,
            };
        }
        let local = IoVec {
            base: bytes.as_mut_ptr().cast(),
            length: bytes.len(),
        };
        // The kernel still validates every remote span. A short/faulting read
        // rejects the entire sample, never the successfully copied prefix.
        let copied = unsafe {
            process_vm_readv(self.pid as i32, &local, 1, remote.as_ptr(), spans.len(), 0)
        };
        if copied != bytes.len() as isize {
            return Err("Memory owner changed during vector sample".into());
        }
        Ok(())
    }
    fn read_into(&self, address: u64, bytes: &mut [u8]) -> Result<(), String> {
        if bytes.is_empty() {
            return Ok(());
        }
        if address < 4096
            || address.checked_add(bytes.len() as u64).is_none()
            || bytes.len() > 1024 * 1024
        {
            return Err("Invalid memory span".into());
        }
        let local = IoVec {
            base: bytes.as_mut_ptr().cast(),
            length: bytes.len(),
        };
        let remote = IoVec {
            base: address as *mut c_void,
            length: bytes.len(),
        };
        // The kernel validates the remote span; a destroyed object cannot turn
        // this read into an unsafe Rust dereference, including self-process reads.
        let copied = unsafe { process_vm_readv(self.pid as i32, &local, 1, &remote, 1, 0) };
        if copied != bytes.len() as isize {
            return Err("Memory owner changed during sample".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod vector_tests {
    use super::*;
    #[test]
    fn reads_disjoint_owned_spans_in_order_without_extra_bytes() {
        let memory = ProcessMemory::current();
        let a = [1u8, 2, 3, 4];
        let b = [7u8, 8, 9];
        let spans = [
            (b.as_ptr() as u64, b.len()),
            (a.as_ptr() as u64, a.len()),
            (b.as_ptr() as u64, b.len()),
        ];
        let mut out = [0u8; 10];
        memory.read_vectored(&spans, &mut out).unwrap();
        assert_eq!(out, [7, 8, 9, 1, 2, 3, 4, 7, 8, 9]);
        assert!(memory.read_vectored(&[(1, 8)], &mut [0; 8]).is_err());
        assert!(memory.read_vectored(&spans, &mut [0; 9]).is_err());
    }
    #[test]
    fn protected_later_span_rejects_entire_kernel_partial_read() {
        unsafe extern "C" {
            fn mmap(
                address: *mut c_void,
                length: usize,
                prot: i32,
                flags: i32,
                fd: i32,
                offset: isize,
            ) -> *mut c_void;
            fn mprotect(address: *mut c_void, length: usize, prot: i32) -> i32;
            fn munmap(address: *mut c_void, length: usize) -> i32;
            fn getpagesize() -> i32;
        }
        struct Mapping(*mut c_void, usize);
        impl Drop for Mapping {
            fn drop(&mut self) {
                unsafe {
                    munmap(self.0, self.1);
                }
            }
        }
        let page = unsafe { getpagesize() } as usize;
        let pointer = unsafe { mmap(std::ptr::null_mut(), page * 2, 3, 0x22, -1, 0) };
        assert_ne!(pointer as isize, -1);
        let map = Mapping(pointer, page * 2);
        unsafe {
            std::ptr::write_bytes(pointer.cast::<u8>(), 91, 8);
        }
        let protected = unsafe { pointer.cast::<u8>().add(page) };
        assert_eq!(unsafe { mprotect(protected.cast(), page, 0) }, 0);
        let mut out = [0u8; 16];
        assert!(
            ProcessMemory::current()
                .read_vectored(&[(pointer as u64, 8), (protected as u64, 8)], &mut out)
                .is_err()
        );
        // Linux may copy a complete earlier iovec before the fault. It must not
        // be mistaken for a valid coherent observation by the caller.
        assert_eq!(&out[..8], &[91; 8]);
        drop(map);
    }
}
