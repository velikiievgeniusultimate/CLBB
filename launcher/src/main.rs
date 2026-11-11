// SPDX-License-Identifier: MIT
//! Normal CLBB entry point. Never starts or resumes a runtime before invocation.
mod shader_cache;
use std::{env, fs::{self, File, OpenOptions}, io, path::PathBuf};
#[cfg(target_os = "linux")]
use std::{os::{fd::AsRawFd, unix::process::CommandExt}, process::Command};

#[cfg(target_os = "linux")]
fn launch() -> io::Result<()> {
    #[repr(C)]
    struct Timespec { seconds: i64, nanoseconds: i64 }
    unsafe extern "C" {
        fn clock_gettime(clock: i32, value: *mut Timespec) -> i32;
        fn flock(fd: i32, operation: i32) -> i32;
        fn prctl(option: i32, arg2: usize, arg3: usize, arg4: usize, arg5: usize) -> i32;
    }
    // Clock starts before filesystem, environment setup, shell helpers or runtime.
    let mut timestamp = Timespec { seconds: 0, nanoseconds: 0 };
    if unsafe { clock_gettime(1, &mut timestamp) } != 0 { return Err(io::Error::last_os_error()); }
    let start_ns = timestamp.seconds as u64 * 1_000_000_000 + timestamp.nanoseconds as u64;
    let root = runtime_root()?;
    let state = env::var_os("CLBB_STATE_ROOT").map(PathBuf::from).unwrap_or_else(||root.clone());
    if !state.join("game/base.apk").is_file() {
        let mut command=Command::new(root.join("bin/clbb-manager"));
        let status=command.arg("--import-game-dialog").env("CLBB_STATE_ROOT",&state).status()?;
        if !status.success(){return Err(io::Error::other("Game import cancelled or failed"));}
    }
    let logs = state.join("logs");
    fs::create_dir_all(&logs)?;
    let lock = OpenOptions::new().create(true).truncate(false).read(true).write(true).open(logs.join("clbb-launch.lock"))?;
    if unsafe { flock(lock.as_raw_fd(), 2 | 4) } != 0 {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "CLBB is already running"));
    }
    // Keep an owning parent so the CLOEXEC lock remains held for the whole child
    // lifetime. A lock accidentally released at exec would admit duplicate games.
    let run = logs.join(format!("qemu-launch-{}-{}", start_ns, std::process::id()));
    fs::create_dir(&run)?;
    let mut output = File::create(run.join("runtime.log"))?;
    // Imported compiled artifacts stay on disk; launch clock already started.
    use std::io::Write;
    let migration = std::time::Instant::now();
    match shader_cache::import(std::path::Path::new("/tmp/atl_cache/com.mobile.legends/UnityShaderCache"), &state.join("data/base.apk_/cache/com.mobile.legends")) {
        Ok(value) => { let _ = writeln!(output,"[CLBB shader cache migration] files={} bytes={} elapsed_us={}",value.files,value.bytes,migration.elapsed().as_micros()); }
        Err(error) => { let _ = writeln!(output,"[CLBB shader cache migration] skipped: {error}"); }
    }
    let mut command = Command::new(root.join("run-mlbb.sh"));
    command.current_dir(&root).args(env::args_os().skip(1)).env("GDK_BACKEND", "x11");
    if env::var_os("DISPLAY").is_none() { return Err(io::Error::other("X11 or XWayland DISPLAY is required")); }
    for (name, _) in env::vars_os() {
        let text = name.to_string_lossy();
        if text.starts_with("CLBB_ETA_") || text.starts_with("CLBB_ARM64_")
            || text.starts_with("CLBB_TCG_") || text.starts_with("CLBB_TB_")
            || text.starts_with("CLBB_BOOT_") || text.starts_with("CLBB_STARTUP_")
            || text.starts_with("NB_QEMU_DIAG_") || text.starts_with("NB_QEMU_TRACE_")
            || text == "ATL_INPUT_TRACE" || text == "ATL_BOOT_TRACE"
            || text == "ATL_WEBVIEW_NETWORK_TRACE" || text == "WEBKIT_INSPECTOR_HTTP_SERVER"
            || text == "LD_LIBRARY_PATH" || text == "LD_PRELOAD" {
            command.env_remove(name);
        }
    }
    command.stdout(output.try_clone()?).stderr(output);
    // Diagnostic access is per owned runtime, without changing system policy.
    unsafe { command.pre_exec(|| {
        if prctl(0x59616d61, usize::MAX, 0, 0, 0) != 0 { return Err(io::Error::last_os_error()); }
        Ok(())
    }); }
    let mut child = command.spawn()?;
    let pid = child.id();
    let metadata = format!("{{\"backend\":\"qemu\",\"launcher_pid\":{},\"pid\":{},\"start_monotonic_ns\":{},\"fresh_process\":true}}\n", std::process::id(), pid, start_ns);
    // Atomic current pointer; individual launch evidence is never overwritten.
    let publish = || -> io::Result<()> {
        fs::write(run.join("launch.json"), &metadata)?;
        let pending = logs.join(format!(".current-atl-pid-{}.json", std::process::id()));
        fs::write(&pending, &metadata)?;
        fs::rename(pending, logs.join("current-atl-pid.json"))
    };
    if let Err(error) = publish() {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    println!("{{\"runtime_pid\":{pid},\"backend\":\"qemu\",\"start_monotonic_ns\":{start_ns}}}");
    let status = child.wait()?;
    fs::write(run.join("exit.json"), format!("{{\"success\":{},\"code\":{}}}\n", status.success(), status.code().map_or("null".to_owned(), |x| x.to_string())))?;
    drop(lock);
    if status.success() { Ok(()) } else { Err(io::Error::other(format!("CLBB runtime exited: {status}"))) }
}

fn runtime_root() -> io::Result<PathBuf> {
    if let Some(path)=env::var_os("CLBB_ROOT") {
        let root=PathBuf::from(path).canonicalize()?;
        if root.join("run-mlbb.sh").is_file(){return Ok(root);}
        return Err(io::Error::other("CLBB_ROOT is not a runtime directory"));
    }
    let exe=env::current_exe()?.canonicalize()?;
    exe.ancestors().skip(1).take(6).find(|p|p.join("run-mlbb.sh").is_file()&&p.join("env.sh").is_file())
        .map(PathBuf::from).ok_or_else(||io::Error::other("CLBB runtime directory not found next to launcher"))
}
fn main() {
    let args:Vec<_>=env::args().skip(1).collect();
    if args.first().map(String::as_str)==Some("--version") {
        match runtime_root().and_then(|r|fs::read_to_string(r.join("VERSION"))) {Ok(v)=>print!("CLBB {}",v),Err(_)=>println!("CLBB dev")};return;
    }
    if matches!(args.first().map(String::as_str),Some("--update"|"--check-updates"|"--import-game")) {
        let command=if args[0]=="--check-updates"{"--check"}else{&args[0]};
        let result=runtime_root().and_then(|root|std::process::Command::new(root.join("bin/clbb-manager")).arg(command).args(&args[1..]).status());
        match result {Ok(status)=>std::process::exit(status.code().unwrap_or(1)),Err(e)=>{eprintln!("CLBB: {e}");std::process::exit(1);}}
    }
    #[cfg(target_os = "linux")]
    let result = launch();
    #[cfg(not(target_os = "linux"))]
    let result: io::Result<()> = Err(io::Error::new(io::ErrorKind::Unsupported, "This runtime is currently available for Linux"));
    if let Err(error) = result { eprintln!("CLBB: {error}"); std::process::exit(1); }
}
