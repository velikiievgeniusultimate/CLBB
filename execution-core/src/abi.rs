// SPDX-License-Identifier: GPL-2.0-or-later
//! Backend-neutral host-to-ARM64 call boundary; no QEMU or OS internals.
#![deny(unsafe_op_in_unsafe_fn)]
#[cfg(not(target_pointer_width = "64"))]
compile_error!("CLBB execution ABI v1 requires a 64-bit host");
use core::ffi::{c_char, c_void};

pub const ABI: u32 = 1;
pub const OK: i32 = 0;
pub const BAD_ABI: i32 = -1;
pub const BAD_ARGUMENT: i32 = -2;
pub const NO_CPU: i32 = -3;

#[repr(C)]
pub struct CallV1 {
    pub address: isize,
    pub args: [isize; 8],
    pub stack: *mut u8,
    pub stack_size: i32,
}

#[repr(C)]
pub struct BackendV1 {
    pub abi: u32,
    pub size: u32,
    pub initialize: Option<unsafe extern "C" fn(*const c_char, *const c_char) -> i32>,
    pub lookup: Option<unsafe extern "C" fn(*const c_char) -> isize>,
    pub acquire: Option<unsafe extern "C" fn(*mut u32) -> *mut c_void>,
    pub release: Option<unsafe extern "C" fn(*mut c_void)>,
    pub call_word: Option<unsafe extern "C" fn(*mut c_void, *const CallV1) -> isize>,
    pub call_u64: Option<unsafe extern "C" fn(*mut c_void, *const CallV1) -> u64>,
}

impl BackendV1 {
    fn valid(&self) -> bool {
        self.abi == ABI && self.size as usize == core::mem::size_of::<Self>()
            && self.initialize.is_some() && self.lookup.is_some()
            && self.acquire.is_some() && self.release.is_some()
            && self.call_word.is_some() && self.call_u64.is_some()
    }
}

// These exports use pointers only at a trusted embedding ABI boundary. The
// caller guarantees valid, aligned storage; null/version/shape checks do not
// validate arbitrary host addresses. The descriptor is immutable and lives at
// least as long as every acquired CPU. Callbacks may re-enter this interface:
// no global mutable backend, allocation, mutex or Rust borrow spans a callback.
unsafe fn backend<'a>(p: *const BackendV1) -> Result<&'a BackendV1, i32> {
    if p.is_null() { return Err(BAD_ARGUMENT); }
    // SAFETY: embedding contract supplies a readable full V1 descriptor.
    let b = unsafe { &*p };
    if b.valid() { Ok(b) } else { Err(BAD_ABI) }
}

#[no_mangle]
pub unsafe extern "C" fn clbb_execution_initialize_v1(
    b: *const BackendV1, process: *const c_char, temp: *const c_char, out: *mut i32,
) -> i32 {
    if process.is_null() || temp.is_null() || out.is_null() { return BAD_ARGUMENT; }
    let b = match unsafe { backend(b) } { Ok(b) => b, Err(e) => return e };
    // SAFETY: validated callback; caller owns strings and output, callback has
    // its own error convention preserved separately from interface status.
    unsafe { *out = (b.initialize.unwrap_unchecked())(process, temp); }
    OK
}

#[no_mangle]
pub unsafe extern "C" fn clbb_execution_lookup_v1(
    b: *const BackendV1, name: *const c_char, out: *mut isize,
) -> i32 {
    if name.is_null() || out.is_null() { return BAD_ARGUMENT; }
    let b = match unsafe { backend(b) } { Ok(b) => b, Err(e) => return e };
    unsafe { *out = (b.lookup.unwrap_unchecked())(name); }
    OK
}

#[no_mangle]
pub unsafe extern "C" fn clbb_execution_acquire_v1(
    b: *const BackendV1, cpu: *mut *mut c_void, owned: *mut u32,
) -> i32 {
    if cpu.is_null() || owned.is_null() { return BAD_ARGUMENT; }
    let b = match unsafe { backend(b) } { Ok(b) => b, Err(e) => return e };
    // Backend must return owned=0 for a borrowed current CPU, owned=1 for a
    // newly created CPU. Only owned CPUs are released by the embedding TLS.
    let mut ownership = 0;
    let handle = unsafe { (b.acquire.unwrap_unchecked())(&mut ownership) };
    if handle.is_null() { return NO_CPU; }
    if ownership > 1 { return BAD_ABI; }
    unsafe { *cpu = handle; *owned = ownership; }
    OK
}

#[no_mangle]
pub unsafe extern "C" fn clbb_execution_release_v1(
    b: *const BackendV1, cpu: *mut c_void, owned: u32,
) -> i32 {
    if cpu.is_null() || owned > 1 { return BAD_ARGUMENT; }
    let b = match unsafe { backend(b) } { Ok(b) => b, Err(e) => return e };
    if owned == 1 { unsafe { (b.release.unwrap_unchecked())(cpu); } }
    OK
}

unsafe fn validate_call<'a>(cpu: *mut c_void, call: *const CallV1) -> Result<&'a CallV1, i32> {
    if cpu.is_null() || call.is_null() { return Err(BAD_ARGUMENT); }
    let c = unsafe { &*call };
    if c.address == 0 || c.stack_size < 0 || (c.stack_size > 0 && c.stack.is_null()) {
        return Err(BAD_ARGUMENT);
    }
    Ok(c)
}

#[no_mangle]
pub unsafe extern "C" fn clbb_execution_call_word_v1(
    b: *const BackendV1, cpu: *mut c_void, call: *const CallV1, out: *mut isize,
) -> i32 {
    if out.is_null() { return BAD_ARGUMENT; }
    let b = match unsafe { backend(b) } { Ok(b) => b, Err(e) => return e };
    if let Err(e) = unsafe { validate_call(cpu, call) } { return e; }
    unsafe { *out = (b.call_word.unwrap_unchecked())(cpu, call); }
    OK
}

#[no_mangle]
pub unsafe extern "C" fn clbb_execution_call_u64_v1(
    b: *const BackendV1, cpu: *mut c_void, call: *const CallV1, out: *mut u64,
) -> i32 {
    if out.is_null() { return BAD_ARGUMENT; }
    let b = match unsafe { backend(b) } { Ok(b) => b, Err(e) => return e };
    if let Err(e) = unsafe { validate_call(cpu, call) } { return e; }
    unsafe { *out = (b.call_u64.unwrap_unchecked())(cpu, call); }
    OK
}
