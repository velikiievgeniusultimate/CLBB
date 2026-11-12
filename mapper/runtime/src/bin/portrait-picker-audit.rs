// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded read-only watcher; no input, native calls, focus or cursor changes.
use clbb_mapper::{
    platform::linux::{ProcessMemory, now_ns},
    source::Source,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
fn run() -> Result<(), String> {
    let a = std::env::args().collect::<Vec<_>>();
    if a.len() != 3 {
        return Err("usage: portrait-picker-audit PID OUTPUT".into());
    }
    let pid: u32 = a[1].parse().map_err(|_| "invalid pid")?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    let status=std::process::Command::new("python3").args(["-c","import sys;from pathlib import Path;sys.path.insert(0,sys.argv[1]);from runtime_identity import runtime_executable_is_owned;sys.exit(0 if runtime_executable_is_owned(int(sys.argv[2]),Path(sys.argv[3])) else 1)"]).arg(root.join("tools")).arg(pid.to_string()).arg(&root).status().map_err(|e|e.to_string())?;
    if !status.success() {
        return Err("not selected owned runtime".into());
    }
    let mut source = Source::new(
        ProcessMemory::diagnostic(pid)?,
        format!("portrait-watch:{pid}"),
    )?;
    let end = Instant::now() + Duration::from_secs(180);
    let mut ready = false;
    let mut maximum = 0;
    let mut print_at = Instant::now();
    while Instant::now() < end {
        if let Ok(s) = source.sample(now_ns(), [4000., 3111.]) {
            if s.frame.hero_id.int() != 104 {
                return Err("hero changed".into());
            }
            if !ready {
                std::fs::write(format!("{}.ready", a[2]), b"ready").map_err(|e| e.to_string())?;
                ready = true;
            }
            if print_at.elapsed() > Duration::from_secs(2) {
                eprintln!("native={:?} error={}", s.portrait, source.portrait_error);
                print_at = Instant::now();
            }
            if let Some(p) = s.portrait.filter(|p| p.held && !p.portraits.is_empty()) {
                if p.portraits.len() >= maximum {
                    maximum = p.portraits.len();
                    let value = serde_json::json!({"pid":pid,"time_ns":now_ns(),"epoch":s.frame.token.as_str(),"portrait":p});
                    std::fs::write(&a[2], serde_json::to_vec_pretty(&value).unwrap())
                        .map_err(|e| e.to_string())?;
                }
                if maximum == 5 {
                    println!("captured all five portraits: {}", a[2]);
                    return Ok(());
                }
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Err(format!("watch ended; maximum verified portraits={maximum}"))
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
