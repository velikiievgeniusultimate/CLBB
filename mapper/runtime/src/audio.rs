//! Per-application PulseAudio/PipeWire routing. Never changes desktop defaults.
use serde_json::Value;
use std::{
    collections::HashMap,
    ffi::{CString, c_char, c_int, c_void},
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
#[derive(Default)]
struct State {
    input: String,
    output: String,
    inputs: String,
    outputs: String,
    status: String,
    refresh: bool,
}
static STATE: OnceLock<Arc<Mutex<State>>> = OnceLock::new();
fn state() -> &'static Arc<Mutex<State>> {
    STATE.get_or_init(|| {
        let state = Arc::new(Mutex::new(State {
            input: "default".into(),
            output: "default".into(),
            refresh: true,
            ..Default::default()
        }));
        let worker = state.clone();
        std::thread::Builder::new()
            .name("clbb-audio".into())
            .spawn(move || routing(worker))
            .expect("audio worker");
        state
    })
}
pub fn configure(input: String, output: String) {
    let mut s = state().lock().unwrap();
    s.input = input;
    s.output = output;
}
pub fn read(key: &str) -> String {
    if key == "audio.capture_status" {
        let map = captures().lock().unwrap();
        return serde_json::json!({"streams":map.iter().map(|(id,c)|{let q=c.queue.0.lock().unwrap();serde_json::json!({"id":id,"active":q.active,"failed":q.failed,"samples":q.total,"queued":q.samples.len(),"channels":c.channels})}).collect::<Vec<_>>()}).to_string();
    }
    let s = state().lock().unwrap();
    match key {
        "audio.input_devices" => s.inputs.clone(),
        "audio.output_devices" => s.outputs.clone(),
        _ => s.status.clone(),
    }
}
pub fn refresh() {
    state().lock().unwrap().refresh = true;
}
fn command(args: &[&str]) -> Result<String, String> {
    // pactl is local IPC; limit hung servers so audio discovery cannot accumulate children.
    let output = std::process::Command::new("timeout")
        .args(["2", "pactl"])
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
fn list(kind: &str) -> Result<Vec<Value>, String> {
    serde_json::from_str(&command(&["-f", "json", "list", kind])?).map_err(|e| e.to_string())
}
fn devices(items: &[Value], input: bool) -> String {
    let mut out = String::from("default\tСистемное устройство по умолчанию\n");
    for item in items {
        let Some(name) = item["name"].as_str() else {
            continue;
        };
        if input && name.ends_with(".monitor") {
            continue;
        }
        let label = [
            item["description"].as_str(),
            item["properties"]["device.description"].as_str(),
            item["properties"]["node.nick"].as_str(),
            item["properties"]["alsa.card_name"].as_str(),
        ]
        .into_iter()
        .flatten()
        .find(|s| !s.is_empty() && *s != "(null)")
        .unwrap_or(name)
        .replace(['\t', '\n', '\r'], " ");
        out.push_str(&format!("{name}\t{label}\n"));
    }
    out
}
fn object_id(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
}
fn with_client_owners(mut streams: Vec<Value>, clients: &[Value]) -> Vec<Value> {
    for stream in &mut streams {
        if stream["properties"]["application.process.id"]
            .as_str()
            .is_some()
        {
            continue;
        }
        let Some(client_id) = object_id(&stream["client"]) else {
            continue;
        };
        if let Some(client) = clients
            .iter()
            .find(|c| object_id(&c["index"]) == Some(client_id))
        {
            if let Some(pid) = client["properties"]["pipewire.sec.pid"]
                .as_str()
                .or_else(|| client["properties"]["application.process.id"].as_str())
            {
                stream["properties"]["application.process.id"] = Value::String(pid.into());
            }
        }
    }
    streams
}
fn route(streams: &[Value], devices: &[Value], target: &str, input: bool) -> Result<(), String> {
    let Some(index) = devices
        .iter()
        .find(|v| v["name"].as_str() == Some(target))
        .and_then(|v| v["index"].as_u64())
    else {
        return Err(format!("Устройство недоступно: {target}"));
    };
    for stream in streams {
        if stream["properties"]["application.process.id"].as_str()
            != Some(&std::process::id().to_string())
        {
            continue;
        }
        if stream[if input { "source" } else { "sink" }].as_u64() == Some(index) {
            continue;
        }
        if let Some(id) = stream["index"].as_u64() {
            command(&[
                if input {
                    "move-source-output"
                } else {
                    "move-sink-input"
                },
                &id.to_string(),
                target,
            ])?;
        }
    }
    Ok(())
}
fn routing(state: Arc<Mutex<State>>) {
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut default_in = String::new();
    let mut default_out = String::new();
    let mut tick = 0;
    loop {
        let (input, output, refresh) = {
            let mut s = state.lock().unwrap();
            let refresh = s.refresh;
            s.refresh = false;
            (s.input.clone(), s.output.clone(), refresh)
        };
        let result = (|| {
            if tick % 10 == 0 || refresh {
                inputs = list("sources")?;
                outputs = list("sinks")?;
                default_in = command(&["get-default-source"])?.trim().into();
                default_out = command(&["get-default-sink"])?.trim().into();
                let mut s = state.lock().unwrap();
                s.inputs = devices(&inputs, true);
                s.outputs = devices(&outputs, false);
            }
            let clients = list("clients")?;
            route(
                &with_client_owners(list("sink-inputs")?, &clients),
                &outputs,
                if output == "default" {
                    &default_out
                } else {
                    &output
                },
                false,
            )?;
            route(
                &with_client_owners(list("source-outputs")?, &clients),
                &inputs,
                if input == "default" {
                    &default_in
                } else {
                    &input
                },
                true,
            )?;
            Ok::<_, String>(())
        })();
        state.lock().unwrap().status = match result {
            Ok(()) => "Устройства подключены · микрофон включается войсчатом игры".into(),
            Err(e) => format!("Аудио: {e}"),
        };
        tick += 1;
        std::thread::sleep(Duration::from_millis(500));
    }
}
// Capture runs in the game's recording thread. Handles are registry IDs, never guest pointers.
#[repr(C)]
struct SampleSpec {
    format: c_int,
    rate: u32,
    channels: u8,
}
#[repr(C)]
struct BufferAttr {
    maxlength: u32,
    tlength: u32,
    prebuf: u32,
    minreq: u32,
    fragsize: u32,
}
#[link(name = "pulse-simple")]
unsafe extern "C" {
    fn pa_simple_new(
        server: *const c_char,
        name: *const c_char,
        dir: c_int,
        dev: *const c_char,
        stream: *const c_char,
        spec: *const SampleSpec,
        map: *const c_void,
        attr: *const BufferAttr,
        error: *mut c_int,
    ) -> *mut c_void;
    fn pa_simple_read(s: *mut c_void, data: *mut c_void, bytes: usize, error: *mut c_int) -> c_int;
    fn pa_simple_free(s: *mut c_void);
}
#[derive(Default)]
struct Samples {
    samples: std::collections::VecDeque<i16>,
    active: bool,
    closed: bool,
    failed: bool,
    total: u64,
}
struct Capture {
    channels: usize,
    queue: Arc<(Mutex<Samples>, std::sync::Condvar)>,
}
static CAPTURES: OnceLock<Mutex<HashMap<u64, Capture>>> = OnceLock::new();
fn captures() -> &'static Mutex<HashMap<u64, Capture>> {
    CAPTURES.get_or_init(Default::default)
}
#[unsafe(no_mangle)]
pub extern "C" fn clbb_capture_open(rate: u32, channels: u32) -> u64 {
    if !(8000..=192000).contains(&rate) || !(1..=2).contains(&channels) {
        return 0;
    }
    let input = state().lock().unwrap().input.clone();
    let Ok(device) = CString::new(input.as_str()) else {
        return 0;
    };
    let mut error = 0;
    let attr = BufferAttr {
        maxlength: (rate / 5) * channels * 2,
        tlength: u32::MAX,
        prebuf: u32::MAX,
        minreq: u32::MAX,
        fragsize: (rate / 100) * channels * 2,
    };
    let spec = SampleSpec {
        format: 3,
        rate,
        channels: channels as u8,
    }; // PA_SAMPLE_S16LE
    let ptr = unsafe {
        pa_simple_new(
            std::ptr::null(),
            c"CLBB".as_ptr(),
            2,
            if input == "default" {
                std::ptr::null()
            } else {
                device.as_ptr()
            },
            c"Mobile Legends microphone".as_ptr(),
            &spec,
            std::ptr::null(),
            &attr,
            &mut error,
        )
    };
    if ptr.is_null() {
        eprintln!("[CLBB_AUDIO] capture open failed pulse={error}");
        return 0;
    }
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let queue = Arc::new((Mutex::new(Samples::default()), std::sync::Condvar::new()));
    let worker = queue.clone();
    let address = ptr as usize;
    let spawned = std::thread::Builder::new()
        .name("clbb-microphone".into())
        .spawn(move || {
            // This thread exclusively owns the Pulse connection, including destruction.
            let ptr = address as *mut c_void;
            let mut samples = vec![0i16; (rate as usize / 100) * channels as usize];
            loop {
                if worker.0.lock().unwrap().closed {
                    break;
                }
                let mut error = 0;
                let result = unsafe {
                    pa_simple_read(
                        ptr,
                        samples.as_mut_ptr().cast(),
                        samples.len() * 2,
                        &mut error,
                    )
                };
                let mut q = worker.0.lock().unwrap();
                if result < 0 {
                    q.failed = true;
                    worker.1.notify_all();
                    eprintln!("[CLBB_AUDIO] capture read failed pulse={error}");
                    break;
                }
                if q.active {
                    q.total += samples.len() as u64;
                    q.samples.extend(samples.iter().copied());
                    let excess = q
                        .samples
                        .len()
                        .saturating_sub((rate as usize / 5) * channels as usize);
                    q.samples.drain(..excess);
                    worker.1.notify_all();
                }
            }
            unsafe { pa_simple_free(ptr) }
        });
    if spawned.is_err() {
        unsafe { pa_simple_free(ptr) };
        return 0;
    }
    captures().lock().unwrap().insert(
        id,
        Capture {
            channels: channels as usize,
            queue,
        },
    );
    eprintln!("[CLBB_AUDIO] capture opened id={id} rate={rate} channels={channels} device={input}");
    id
}
#[unsafe(no_mangle)]
pub extern "C" fn clbb_capture_active(id: u64, active: c_int) -> c_int {
    let map = captures().lock().unwrap();
    let Some(c) = map.get(&id) else {
        return -3;
    };
    let mut q = c.queue.0.lock().unwrap();
    if q.failed {
        return -1;
    }
    q.active = active != 0;
    q.samples.clear();
    c.queue.1.notify_all();
    0
}
/// # Safety
/// `data` points to writable storage for `frames * channels` i16 samples.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clbb_capture_read(
    id: u64,
    data: *mut i16,
    frames: u32,
    blocking: c_int,
) -> c_int {
    if data.is_null() || frames > 8192 {
        return -2;
    }
    if frames == 0 {
        return 0;
    }
    let (channels, queue) = {
        let map = captures().lock().unwrap();
        let Some(c) = map.get(&id) else {
            return -3;
        };
        (c.channels, c.queue.clone())
    };
    let mut q = queue.0.lock().unwrap();
    if !q.active || q.closed {
        return -3;
    }
    if q.failed {
        return -1;
    }
    if blocking != 0 {
        let result = queue
            .1
            .wait_timeout_while(q, Duration::from_millis(100), |q| {
                q.samples.len() < channels && q.active && !q.closed && !q.failed
            })
            .unwrap();
        q = result.0;
    }
    if !q.active || q.closed {
        return -3;
    }
    if q.failed {
        return -1;
    }
    let count = (q.samples.len() / channels).min(frames as usize);
    for i in 0..count * channels {
        unsafe { data.add(i).write(q.samples.pop_front().unwrap()) };
    }
    count as c_int
}
#[unsafe(no_mangle)]
pub extern "C" fn clbb_capture_close(id: u64) {
    if let Some(c) = captures().lock().unwrap().remove(&id) {
        let mut q = c.queue.0.lock().unwrap();
        q.closed = true;
        q.active = false;
        q.samples.clear();
        c.queue.1.notify_all();
        eprintln!("[CLBB_AUDIO] capture closed id={id} samples={}", q.total);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inventories_hide_loopbacks() {
        let v = serde_json::json!([{"name":"headset","description":"Microphone","index":1},{"name":"speaker.monitor","index":2}]);
        let rows = devices(v.as_array().unwrap(), true);
        assert!(rows.contains("headset\tMicrophone"));
        assert!(!rows.contains("monitor"));
    }
    #[test]
    fn foreign_streams_untouched() {
        assert!(
            route(
                &[serde_json::json!({"index":5,"properties":{"application.process.id":"0"}})],
                &[serde_json::json!({"name":"test","index":1})],
                "test",
                true
            )
            .is_ok()
        );
    }
}

#[cfg(test)]
mod capture_tests {
    use super::*;
    #[test]
    #[ignore = "requires CLBB_TEST_AUDIO_SOURCE synthetic monitor and a generated tone"]
    fn synthetic_capture() {
        let device = std::env::var("CLBB_TEST_AUDIO_SOURCE").unwrap();
        configure(device, "default".into());
        let id = clbb_capture_open(48000, 1);
        assert_ne!(id, 0);
        assert_eq!(clbb_capture_active(id, 1), 0);
        let mut data = vec![0i16; 480];
        let mut energy = 0u64;
        let mut total = 0;
        for _ in 0..150 {
            let n = unsafe { clbb_capture_read(id, data.as_mut_ptr(), 480, 1) };
            assert!(n >= 0);
            for v in data.iter().take(n as usize) {
                energy += i64::from(*v).unsigned_abs();
            }
            total += n;
        }
        assert!(total > 4800, "not enough PCM frames {total}");
        assert!(energy > 100000, "synthetic tone missing {energy}");
        assert_eq!(clbb_capture_active(id, 0), 0);
        assert_eq!(
            unsafe { clbb_capture_read(id, data.as_mut_ptr(), 480, 0) },
            -3
        );
        clbb_capture_close(id);
        assert_eq!(clbb_capture_active(id, 1), -3);
        eprintln!("synthetic microphone: {total} frames, energy={energy}; stop/release verified");
    }
}

#[cfg(test)]
mod routing_tests {
    use super::*;
    #[test]
    #[ignore = "requires two owned synthetic sinks"]
    fn own_streams_move_without_changing_defaults() {
        let a = std::env::var("CLBB_TEST_AUDIO_SINK_A").unwrap();
        let b = std::env::var("CLBB_TEST_AUDIO_SINK_B").unwrap();
        let before_in = command(&["get-default-source"]).unwrap();
        let before_out = command(&["get-default-sink"]).unwrap();
        let spec = SampleSpec {
            format: 3,
            rate: 48000,
            channels: 1,
        };
        let mut error = 0;
        for (input, kind, devices_kind) in [
            (false, "sink-inputs", "sinks"),
            (true, "source-outputs", "sources"),
        ] {
            let origin = if input {
                format!("{a}.monitor")
            } else {
                a.clone()
            };
            let target = if input {
                format!("{b}.monitor")
            } else {
                b.clone()
            };
            let dev = CString::new(origin).unwrap();
            let ptr = unsafe {
                pa_simple_new(
                    std::ptr::null(),
                    c"CLBB routing test".as_ptr(),
                    if input { 2 } else { 1 },
                    dev.as_ptr(),
                    c"owned synthetic test".as_ptr(),
                    &spec,
                    std::ptr::null(),
                    std::ptr::null(),
                    &mut error,
                )
            };
            assert!(!ptr.is_null(), "Pulse open failed {error}");
            let devices = list(devices_kind).unwrap();
            let streams = list(kind).unwrap();
            assert!(
                streams
                    .iter()
                    .any(|s| s["properties"]["application.process.id"].as_str()
                        == Some(&std::process::id().to_string()))
            );
            route(&streams, &devices, &target, input).unwrap();
            let target_index = devices
                .iter()
                .find(|d| d["name"].as_str() == Some(&target))
                .unwrap()["index"]
                .as_u64()
                .unwrap();
            let mut moved = false;
            for _ in 0..30 {
                let owned: Vec<_> = list(kind)
                    .unwrap()
                    .into_iter()
                    .filter(|s| {
                        s["properties"]["application.process.id"].as_str()
                            == Some(&std::process::id().to_string())
                    })
                    .collect();
                moved = !owned.is_empty()
                    && owned.iter().all(|s| {
                        s[if input { "source" } else { "sink" }].as_u64() == Some(target_index)
                    });
                if moved {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            unsafe { pa_simple_free(ptr) };
            assert!(moved, "{kind} did not move to {target}");
        }
        assert_eq!(command(&["get-default-source"]).unwrap(), before_in);
        assert_eq!(command(&["get-default-sink"]).unwrap(), before_out);
    }
}

#[cfg(test)]
mod ownership_tests {
    use super::*;
    #[test]
    fn native_pipewire_stream_resolves_via_client() {
        let streams =
            vec![serde_json::json!({"client":"15345","properties":{"application.name":"CLBB"}})];
        let clients =
            vec![serde_json::json!({"index":15345,"properties":{"pipewire.sec.pid":"12345"}})];
        let resolved = with_client_owners(streams, &clients);
        assert_eq!(resolved[0]["properties"]["application.process.id"], "12345");
    }
    #[test]
    fn name_alone_is_never_ownership() {
        let s = with_client_owners(
            vec![serde_json::json!({"client":"missing","properties":{"application.name":"CLBB"}})],
            &[],
        );
        assert!(s[0]["properties"]["application.process.id"].is_null());
    }
}
