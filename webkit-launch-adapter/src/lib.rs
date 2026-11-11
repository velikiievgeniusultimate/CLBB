//! Exact-path Linux WebKit helper relocation. Hooks must also work after fork:
//! no Rust allocation, synchronization, formatting, or lazy symbol lookup here.
#![cfg(target_os = "linux")]
use std::ffi::{c_char, c_int, c_void};
use std::sync::atomic::{AtomicPtr, Ordering};

const PATH_CAPACITY: usize = 4096;
const EINVAL: c_int = 22;
const ENAMETOOLONG: c_int = 36;
const ENOSYS: c_int = 38;
const E2BIG: c_int = 7;
const ARGV_CAPACITY: usize = 2048;
type Argv = *const *mut c_char;
type Exec = unsafe extern "C" fn(*const c_char, Argv) -> c_int;
type Execve = unsafe extern "C" fn(*const c_char, Argv, Argv) -> c_int;
type Spawn = unsafe extern "C" fn(
    *mut c_int, *const c_char, *const c_void, *const c_void, Argv, Argv,
) -> c_int;

#[link(name = "dl")]
extern "C" {
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
    fn getenv(name: *const c_char) -> *mut c_char;
    fn __errno_location() -> *mut c_int;
}

static EXECV: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static EXECVE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static EXECVP: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static SPAWN: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static SPAWNP: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

unsafe extern "C" fn initialize() {
    // Resolve before any GLib/WebKit fork. In particular, do not call dlsym
    // from an exec hook in the post-fork child.
    for (slot, name) in [
        (&EXECV, b"execv\0".as_slice()),
        (&EXECVE, b"execve\0".as_slice()),
        (&EXECVP, b"execvp\0".as_slice()),
        (&SPAWN, b"posix_spawn\0".as_slice()),
        (&SPAWNP, b"posix_spawnp\0".as_slice()),
    ] {
        slot.store(dlsym((-1isize) as *mut c_void, name.as_ptr().cast()), Ordering::Release);
    }
}

#[used]
#[link_section = ".init_array"]
static INITIALIZE: unsafe extern "C" fn() = initialize;

unsafe fn equals(path: *const c_char, expected: &[u8]) -> bool {
    if path.is_null() { return false; }
    for (index, byte) in expected.iter().enumerate() {
        if *path.add(index) as u8 != *byte { return false; }
    }
    true
}

unsafe fn relocated(path: *const c_char, buffer: &mut [u8; PATH_CAPACITY])
    -> Result<*const c_char, c_int>
{
    let name: &[u8] = if equals(path, b"/usr/lib/webkitgtk-6.0/WebKitNetworkProcess\0") {
        b"WebKitNetworkProcess\0"
    } else if equals(path, b"/usr/lib/webkitgtk-6.0/WebKitWebProcess\0") {
        b"WebKitWebProcess\0"
    } else if equals(path, b"/usr/lib/webkitgtk-6.0/WebKitGPUProcess\0") {
        b"WebKitGPUProcess\0"
    } else { return Ok(path); };
    let directory = getenv(b"CLBB_WEBKIT_PROCESS_DIR\0".as_ptr().cast());
    if directory.is_null() || *directory == 0 { return Ok(path); }
    if *directory as u8 != b'/' { return Err(EINVAL); }
    let mut length = 0;
    while length < PATH_CAPACITY {
        let byte = *directory.add(length) as u8;
        if byte == 0 { break; }
        buffer[length] = byte;
        length += 1;
    }
    if length == PATH_CAPACITY { return Err(ENAMETOOLONG); }
    while length > 1 && buffer[length - 1] == b'/' { length -= 1; }
    let separator = usize::from(buffer[length - 1] != b'/');
    if length + separator + name.len() > PATH_CAPACITY { return Err(ENAMETOOLONG); }
    if separator != 0 { buffer[length] = b'/'; length += 1; }
    for byte in name { buffer[length] = *byte; length += 1; }
    Ok(buffer.as_ptr().cast())
}

unsafe fn exec_error(error: c_int) -> c_int { *__errno_location() = error; -1 }

struct LaunchBuffers {
    path: [u8; PATH_CAPACITY],
    arguments: [[u8; PATH_CAPACITY]; 3],
    pointers: [*mut c_char; ARGV_CAPACITY],
}
impl LaunchBuffers {
    fn new() -> Self {
        Self { path: [0; PATH_CAPACITY], arguments: [[0; PATH_CAPACITY]; 3],
            pointers: [std::ptr::null_mut(); ARGV_CAPACITY] }
    }
}

unsafe fn prepare(path: *const c_char, argv: Argv, buffers: &mut LaunchBuffers)
    -> Result<(*const c_char, Argv), c_int>
{
    let mapped_path = relocated(path, &mut buffers.path)?;
    if !equals(path, b"/usr/bin/bwrap\0") || argv.is_null() { return Ok((mapped_path, argv)); }
    let directory = getenv(b"CLBB_WEBKIT_PROCESS_DIR\0".as_ptr().cast());
    if directory.is_null() || *directory == 0 { return Ok((path, argv)); }
    let mut after_separator = false;
    let mut changed = 0;
    for index in 0..ARGV_CAPACITY {
        let arg = *argv.add(index);
        buffers.pointers[index] = arg;
        if arg.is_null() {
            return Ok((path, if changed == 0 { argv } else { buffers.pointers.as_ptr() }));
        }
        if index == ARGV_CAPACITY - 1 { return Err(E2BIG); }
        if after_separator {
            if changed == 3 {
                if equals(arg, b"/usr/lib/webkitgtk-6.0/WebKitNetworkProcess\0") ||
                    equals(arg, b"/usr/lib/webkitgtk-6.0/WebKitWebProcess\0") ||
                    equals(arg, b"/usr/lib/webkitgtk-6.0/WebKitGPUProcess\0") { return Err(E2BIG); }
                continue;
            }
            let mapped = relocated(arg, &mut buffers.arguments[changed])?;
            if mapped != arg.cast_const() {
                if changed == 3 { return Err(E2BIG); }
                buffers.pointers[index] = mapped.cast_mut();
                changed += 1;
            }
        } else if equals(arg, b"--\0") { after_separator = true; }
    }
    Err(E2BIG)
}

unsafe fn exec_common(slot: &AtomicPtr<c_void>, path: *const c_char, argv: Argv) -> c_int {
    let function = slot.load(Ordering::Acquire);
    if function.is_null() { return exec_error(ENOSYS); }
    let mut buffers = LaunchBuffers::new();
    match prepare(path, argv, &mut buffers) {
        Ok((path, argv)) => std::mem::transmute::<*mut c_void, Exec>(function)(path, argv),
        Err(error) => exec_error(error),
    }
}

#[no_mangle]
pub unsafe extern "C" fn execv(path: *const c_char, argv: Argv) -> c_int {
    exec_common(&EXECV, path, argv)
}
#[no_mangle]
pub unsafe extern "C" fn execvp(path: *const c_char, argv: Argv) -> c_int {
    exec_common(&EXECVP, path, argv)
}
#[no_mangle]
pub unsafe extern "C" fn execve(path: *const c_char, argv: Argv, envp: Argv) -> c_int {
    let function = EXECVE.load(Ordering::Acquire);
    if function.is_null() { return exec_error(ENOSYS); }
    let mut buffers = LaunchBuffers::new();
    match prepare(path, argv, &mut buffers) {
        Ok((path, argv)) => std::mem::transmute::<*mut c_void, Execve>(function)(path, argv, envp),
        Err(error) => exec_error(error),
    }
}

unsafe fn spawn_common(slot: &AtomicPtr<c_void>, pid: *mut c_int, path: *const c_char,
    actions: *const c_void, attrs: *const c_void, argv: Argv, envp: Argv) -> c_int
{
    let function = slot.load(Ordering::Acquire);
    if function.is_null() { return ENOSYS; }
    let mut buffers = LaunchBuffers::new();
    match prepare(path, argv, &mut buffers) {
        Ok((path, argv)) => std::mem::transmute::<*mut c_void, Spawn>(function)(pid, path, actions, attrs, argv, envp),
        Err(error) => error,
    }
}
#[no_mangle]
pub unsafe extern "C" fn posix_spawn(pid: *mut c_int, path: *const c_char,
    actions: *const c_void, attrs: *const c_void, argv: Argv, envp: Argv) -> c_int
{ spawn_common(&SPAWN, pid, path, actions, attrs, argv, envp) }
#[no_mangle]
pub unsafe extern "C" fn posix_spawnp(pid: *mut c_int, path: *const c_char,
    actions: *const c_void, attrs: *const c_void, argv: Argv, envp: Argv) -> c_int
{ spawn_common(&SPAWNP, pid, path, actions, attrs, argv, envp) }
