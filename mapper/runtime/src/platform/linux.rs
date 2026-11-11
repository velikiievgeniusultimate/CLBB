//! Linux runtime adapter. The common controller and source decoding have no /proc dependency.
use super::{Memory, Region};
use std::{ffi::c_void, fs};

#[repr(C)]
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
