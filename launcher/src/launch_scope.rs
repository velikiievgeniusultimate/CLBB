// SPDX-License-Identifier: MIT
//! Bind crash evidence to one launcher-owned process, never directory ordering.
use serde_json::{json, Value};
use std::{env, fs, path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};

#[derive(Clone, Debug)]
pub struct Context {
    pub run: PathBuf,
    pub record: Value,
}

#[derive(Debug)]
pub struct Evidence {
    pub context: Option<Context>,
    pub provenance: Value,
}

pub fn process_birth(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse().ok()
}

pub fn boot_id() -> String {
    fs::read_to_string("/proc/sys/kernel/random/boot_id").unwrap_or_default().trim().into()
}

pub fn invocation_id() -> Option<String> {
    env::var("CLBB_DIAGNOSTIC_INVOCATION").ok().filter(|v| {
        !v.is_empty() && v.len() <= 100 && v.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
    })
}

pub fn new_context(root: &Path, run: &Path, pid: u32, start_ns: u64) -> Context {
    Context {
        run: run.to_path_buf(),
        record: json!({
            "schema":"clbb-launch-scope-v2", "backend":"qemu", "launcher_pid":std::process::id(),
            "pid":pid, "start_monotonic_ns":start_ns, "fresh_process":true,
            "start_unix_ns":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() as u64,
            "pid_start_ticks":process_birth(pid), "launcher_start_ticks":process_birth(std::process::id()),
            "boot_id":boot_id(), "runtime_root":root, "launch_directory":run,
            "runtime_version":fs::read_to_string(root.join("VERSION")).unwrap_or_default().trim(),
            "runtime_revision":fs::read_to_string(root.join("REVISION")).unwrap_or_default().trim(),
            "invocation_id":invocation_id(),
        }),
    }
}

fn unknown(source: &str, reason: String, expected: Option<&Value>) -> Evidence {
    Evidence { context: None, provenance: json!({"status":"unknown", "source":source,
        "reason":reason,"expected":expected.map(identity_fields),"runtime_log_attached":false}) }
}

fn identity_fields(record: &Value) -> Value {
    let mut value=json!({});
    for key in ["schema","backend","pid","launcher_pid","pid_start_ticks","launcher_start_ticks","start_monotonic_ns","start_unix_ns","boot_id","runtime_root","runtime_version","runtime_revision","launch_directory","invocation_id"] {
        if let Some(field)=record.get(key){value[key]=field.clone();}
    }
    value
}

pub fn exit_record(context: &Context) -> Value {
    let record=fs::read(context.run.join("exit.json")).ok().and_then(|b|serde_json::from_slice::<Value>(&b).ok());
    match record {
        Some(value) if ["pid","launcher_pid","start_monotonic_ns"].iter().all(|key|value[*key]==context.record[*key])=>value,
        _=>Value::Null,
    }
}

pub fn unavailable(reason: &str) -> Evidence {
    unknown("explicit_launcher_context",reason.into(),None)
}

/// Automatic failures supply their in-memory context. Manual checks may only use
/// the canonical pointer and its exact matching launch.json, with process birth.
pub fn resolve(state: &Path, explicit: Option<&Context>) -> Evidence {
    let source = if explicit.is_some() { "explicit_launcher_context" } else { "current_pointer" };
    let expected = match explicit {
        Some(context) => context.record.clone(),
        None => match fs::read(state.join("logs/current-atl-pid.json")).ok().and_then(|b|serde_json::from_slice::<Value>(&b).ok()) {
            Some(value) => value,
            None => return unknown(source,"Canonical current launch pointer is missing or invalid".into(),None),
        },
    };
    // Legacy records lack birth/boot/manifest evidence. Never silently attach an
    // older run to a new failure, even when its filename sorts last.
    if expected["schema"] != "clbb-launch-scope-v2" {
        return unknown(source,"Legacy launch record lacks verified process birth and runtime manifest".into(),Some(&expected));
    }
    let number = |key:&str| expected[key].as_u64().filter(|v|*v>0);
    let (Some(pid),Some(launcher),Some(start),Some(birth),Some(_)) =
        (number("pid"),number("launcher_pid"),number("start_monotonic_ns"),number("pid_start_ticks"),number("launcher_start_ticks")) else {
            return unknown(source,"Launch identity is incomplete".into(),Some(&expected));
        };
    if pid > u32::MAX as u64 || launcher > u32::MAX as u64 || expected["boot_id"].as_str().filter(|v|!v.is_empty()) != Some(boot_id().as_str()) {
        return unknown(source,"Launch boot or process identity does not match this host boot".into(),Some(&expected));
    }
    for key in ["runtime_root","runtime_version","runtime_revision"] {
        if expected[key].as_str().filter(|v|!v.is_empty()).is_none() {
            return unknown(source,format!("Missing launch manifest field {key}"),Some(&expected));
        }
    }
    if number("start_unix_ns").is_none() || !Path::new(expected["runtime_root"].as_str().unwrap()).is_absolute() {
        return unknown(source,"Launch manifest lacks an absolute runtime root or wall-clock start".into(),Some(&expected));
    }
    let logs = match state.join("logs").canonicalize() { Ok(p)=>p,Err(e)=>return unknown(source,e.to_string(),Some(&expected)) };
    let run = logs.join(format!("qemu-launch-{start}-{launcher}"));
    if explicit.is_some_and(|c|c.run.canonicalize().ok().as_ref()!=Some(&run))
        || expected["launch_directory"].as_str().map(Path::new).and_then(|p|p.canonicalize().ok()).as_ref()!=Some(&run) {
        return unknown(source,"Launch directory does not match its monotonic start and launcher PID".into(),Some(&expected));
    }
    let record = match fs::read(run.join("launch.json")).ok().and_then(|b|serde_json::from_slice::<Value>(&b).ok()) {
        Some(v)=>v,None=>return unknown(source,"Exact launch.json is missing or invalid".into(),Some(&expected)),
    };
    for key in ["schema","backend","pid","launcher_pid","start_monotonic_ns","start_unix_ns","pid_start_ticks","launcher_start_ticks","boot_id","runtime_root","runtime_version","runtime_revision","launch_directory","invocation_id"] {
        if record[key] != expected[key] {
            return unknown(source,format!("Exact launch.json identity mismatch: {key}"),Some(&expected));
        }
    }
    if process_birth(pid as u32).is_some_and(|current|current!=birth) {
        return unknown(source,"Runtime PID has been reused by a different process birth".into(),Some(&expected));
    }
    if process_birth(launcher as u32).is_some_and(|current|Some(current)!=number("launcher_start_ticks")) {
        return unknown(source,"Launcher PID has been reused by a different process birth".into(),Some(&expected));
    }
    Evidence { context:Some(Context{run,record:record.clone()}), provenance:json!({
        "status":"verified_launch", "source":source,"identity":identity_fields(&record),
        "runtime_log_attached":true,"version_source":"launch_manifest",
        "process_presence":"A missing exited process is allowed; any observed PID birth must match the recorded birth.",
    }) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT:AtomicU64=AtomicU64::new(0);
    struct Fixture { state:PathBuf, context:Context }
    impl Fixture {
        fn new(start:u64)->Self {
            let state=env::temp_dir().join(format!("clbb-launch-scope-test-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
            let run=state.join("logs").join(format!("qemu-launch-{start}-4294967294"));fs::create_dir_all(&run).unwrap();
            let record=json!({"schema":"clbb-launch-scope-v2","backend":"qemu","pid":4294967293u32,"launcher_pid":4294967294u32,
                "start_monotonic_ns":start,"start_unix_ns":100_000_000_000u64,"pid_start_ticks":7,"launcher_start_ticks":6,"boot_id":boot_id(),"runtime_root":"/immutable/selected32",
                "runtime_version":"v32","runtime_revision":"revision32","launch_directory":run,"invocation_id":"fixture-current"});
            fs::write(run.join("launch.json"),serde_json::to_vec(&record).unwrap()).unwrap();
            fs::write(state.join("logs/current-atl-pid.json"),serde_json::to_vec(&record).unwrap()).unwrap();
            Self{state,context:Context{run,record}}
        }
    }
    impl Drop for Fixture {fn drop(&mut self){let _=fs::remove_dir_all(&self.state);}}
    #[test]fn current_pointer_beats_lexicographically_later_old_directory(){
        let f=Fixture::new(215845032545626);let old=f.state.join("logs/qemu-launch-99567360239857-1534559");fs::create_dir(&old).unwrap();
        fs::write(old.join("runtime.log"),"OLD ERROR").unwrap();
        let e=resolve(&f.state,None);assert_eq!(e.context.unwrap().run,f.context.run);assert_eq!(e.provenance["status"],"verified_launch");
    }
    #[test]fn explicit_failure_keeps_its_scope_after_current_pointer_changes(){
        let f=Fixture::new(22);fs::write(f.state.join("logs/current-atl-pid.json"),b"{\"pid\":123}").unwrap();
        assert_eq!(resolve(&f.state,Some(&f.context)).context.unwrap().record["runtime_version"],"v32");
    }
    #[test]fn mismatched_pid_or_birth_never_attaches_a_log(){
        for key in ["pid","pid_start_ticks","start_monotonic_ns","start_unix_ns","runtime_revision"] {
            let f=Fixture::new(23);let mut record=f.context.record.clone();record[key]=json!(123);
            fs::write(f.context.run.join("launch.json"),serde_json::to_vec(&record).unwrap()).unwrap();
            assert!(resolve(&f.state,Some(&f.context)).context.is_none(),"{key}");
        }
    }
    #[test]fn legacy_pointer_and_old_boot_are_explicitly_unknown(){
        let f=Fixture::new(24);fs::write(f.state.join("logs/current-atl-pid.json"),b"{\"pid\":4}").unwrap();assert!(resolve(&f.state,None).context.is_none());
        let mut context=f.context.clone();context.record["boot_id"]=json!("previous-boot");assert!(resolve(&f.state,Some(&context)).context.is_none());
    }
    #[test]fn reused_runtime_pid_is_rejected(){
        let f=Fixture::new(25);let mut context=f.context.clone();context.record["pid"]=json!(std::process::id());context.record["pid_start_ticks"]=json!(process_birth(std::process::id()).unwrap()+1);
        fs::write(context.run.join("launch.json"),serde_json::to_vec(&context.record).unwrap()).unwrap();
        assert!(resolve(&f.state,Some(&context)).context.is_none());
    }
}
