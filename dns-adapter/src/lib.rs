//! Experimental host resolver; not integrated into nb-qemu or the game runtime.
pub mod wire;

#[cfg(not(all(
    target_os = "linux",
    target_endian = "little",
    target_pointer_width = "64"
)))]
compile_error!("Only 64-bit little-endian Linux is implemented; Windows needs a separate adapter");
mod linux;
use std::ffi::c_char;
use wire::*;

/// Resolve host C strings into a bounded, pointer-free payload.
///
/// # Safety
/// `node`/`service` must be NULL or valid NUL-terminated strings throughout
/// this call. `hints` and `response` must be aligned and valid for their full
/// sizes, must not overlap each other or the strings, and response must be
/// exclusively writable. All pointers here belong to the HOST address space.
/// Guest addresses require validation and copying by the SVC transport first.
#[no_mangle]
pub unsafe extern "C" fn clbb_dns_resolve_v1(
    node: *const c_char,
    service: *const c_char,
    hints: *const Hints,
    response: *mut Response,
) -> i32 {
    if response.is_null() {
        return BADHINTS;
    }
    // No heap allocation or Rust unwinding crosses this C ABI.
    let mut output = Response::default();
    let result = if hints.is_null() {
        BADHINTS
    } else {
        unsafe { linux::resolve(node, service, *hints, &mut output) }
    };
    if result != 0 {
        let errno = output.system_errno;
        output = Response::default();
        output.system_errno = errno;
    }
    unsafe { response.write(output) };
    result
}
