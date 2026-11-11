//! Optional native input observations. Never reborrow Runtime during synchronous JNI callbacks.
use crate::{platform::linux::now_ns, recorder::Recorder};
use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicBool, Ordering},
};
static AUTO_MARKS: AtomicBool = AtomicBool::new(false);
pub fn automatic_marks(enabled: bool) {
    AUTO_MARKS.store(enabled, Ordering::Relaxed);
}
static ENABLED: AtomicBool = AtomicBool::new(false);
static RECORDER: Mutex<Option<Weak<Recorder>>> = Mutex::new(None);
pub fn install(recorder: &Arc<Recorder>) {
    *RECORDER.lock().unwrap() = Some(Arc::downgrade(recorder));
}
pub fn enable(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}
#[unsafe(no_mangle)]
pub extern "C" fn clbb_debug_recording() -> i32 {
    i32::from(ENABLED.load(Ordering::Relaxed))
}
/// # Safety
/// ids points to count i32s; coordinates points to count*4 f32s for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clbb_debug_motion(
    packet: u64,
    stage: i32,
    action: i32,
    count: i32,
    ids: *const i32,
    coordinates: *const f32,
    event_ms: i64,
    down_ms: i64,
    handled: i32,
    physical: i32,
) {
    if clbb_debug_recording() == 0
        || !(0..=50).contains(&count)
        || ids.is_null()
        || coordinates.is_null()
    {
        return;
    }
    let recorder = RECORDER.lock().unwrap().as_ref().and_then(Weak::upgrade);
    let Some(recorder) = recorder else { return };
    if !recorder.is_enabled() {
        return;
    }
    let ids = unsafe { std::slice::from_raw_parts(ids, count as usize) };
    let coords = unsafe { std::slice::from_raw_parts(coordinates, count as usize * 4) };
    let pointers: Vec<_> = ids
        .iter()
        .zip(coords.chunks_exact(4))
        .map(|(id, p)| serde_json::json!({"android_id":id,"local":[p[0],p[1]],"raw":[p[2],p[3]]}))
        .collect();
    if stage == 1
        && handled == 0
        && physical == 0
        && (action & 255) == 0
        && AUTO_MARKS.load(Ordering::Relaxed)
    {
        recorder.mark(now_ns(), "android_down_dispatch_rejected", true);
    }
    recorder.push(now_ns(),"android_motion",serde_json::json!({"packet":packet,"thread":format!("{:?}",std::thread::current().id()),"stage":match stage{0=>"before_dispatch",1=>"after_dispatch",_=>"motion_event_reader"},"action":action,"action_masked":action&255,"action_index":(action>>8)&255,"event_ms":event_ms,"down_ms":down_ms,"handled":if stage==2{None}else{Some(handled)},"reader_thread_java":if stage==2{Some(handled)}else{None},"physical":if physical<0{None}else{Some(physical!=0)},"pointers":pointers}));
}
#[unsafe(no_mangle)]
pub extern "C" fn clbb_debug_host_event(kind: i32, a: i64, b: i64, c: i64) {
    if clbb_debug_recording() == 0 {
        return;
    }
    let recorder = RECORDER.lock().unwrap().as_ref().and_then(Weak::upgrade);
    if let Some(recorder) = recorder {
        recorder.push(now_ns(),"host_event",serde_json::json!({"kind":match kind{0=>"window_active",1=>"settings_visible",2=>"input_before_host_routing",3=>"cursor_lock",_=>"unknown"},"a":a,"b":b,"c":c}));
    }
}
/// # Safety
/// status is a NUL-terminated, adapter-owned static diagnostic string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clbb_debug_transport_status(
    action: i32,
    id: i32,
    accepted: i32,
    status: *const std::ffi::c_char,
    x: f64,
    y: f64,
) {
    if clbb_debug_recording() == 0 || status.is_null() {
        return;
    }
    let recorder = RECORDER.lock().unwrap().as_ref().and_then(Weak::upgrade);
    if let Some(recorder) = recorder {
        let reason = unsafe { std::ffi::CStr::from_ptr(status) }.to_string_lossy();
        recorder.push(now_ns(),"native_transport",serde_json::json!({"action":action,"id":id,"accepted":accepted!=0,"status":reason,"packet_uv":[x,y]}));
        if accepted == 0 && AUTO_MARKS.load(Ordering::Relaxed) {
            recorder.mark(now_ns(), "native_transport_rejected", true);
        }
    }
}
