//! One desktop playback device, shared by OpenSL ES and Android AudioTrack.
//! Resampling uses the SDL2 runtime already required by OpenSL ES. No host mixer
//! defaults or per-user virtual sinks are created. Capture remains independent.
use std::{
    collections::BTreeMap,
    ffi::{c_char, c_int, c_void},
    sync::{Arc, Condvar, Mutex, OnceLock},
    time::Duration,
};
const RATE: u32 = 48000;
const FORMAT: u16 = 0x8010; // AUDIO_S16LSB (supported Linux host is little-endian)
type Render = unsafe extern "C" fn(*mut c_void, *mut u8, c_int);
#[repr(C)]
struct Spec {
    freq: c_int,
    format: u16,
    channels: u8,
    silence: u8,
    samples: u16,
    padding: u16,
    size: u32,
    callback: Option<Render>,
    userdata: *mut c_void,
}
#[link(name = "SDL2")]
unsafe extern "C" {
    fn SDL_InitSubSystem(flags: u32) -> c_int;
    fn SDL_OpenAudioDevice(
        name: *const c_char,
        capture: c_int,
        want: *const Spec,
        got: *mut Spec,
        changes: c_int,
    ) -> u32;
    fn SDL_PauseAudioDevice(device: u32, pause: c_int);
    fn SDL_NewAudioStream(
        src: u16,
        channels: u8,
        rate: c_int,
        dst: u16,
        dst_channels: u8,
        dst_rate: c_int,
    ) -> *mut c_void;
    fn SDL_AudioStreamPut(stream: *mut c_void, buf: *const c_void, len: c_int) -> c_int;
    fn SDL_AudioStreamGet(stream: *mut c_void, buf: *mut c_void, len: c_int) -> c_int;
    fn SDL_AudioStreamClear(stream: *mut c_void);
    fn SDL_FreeAudioStream(stream: *mut c_void);
}
struct Inner {
    converter: usize,
    callback: Option<Render>,
    user: usize,
    scratch: Vec<i16>,
    rate: u32,
    channels: u32,
    phase: u32,
    queued: f64,
    capacity: u32,
    active: bool,
    closed: bool,
}
// The SDL converter and callback lifetime are protected by this source's mutex.
impl Drop for Inner {
    fn drop(&mut self) {
        unsafe { SDL_FreeAudioStream(self.converter as *mut c_void) }
    }
}
struct Source {
    inner: Mutex<Inner>,
    wake: Condvar,
}
#[derive(Default)]
struct Mixer {
    device: u32,
    next: u32,
    sources: BTreeMap<u32, Arc<Source>>,
}
static MIXER: OnceLock<Mutex<Mixer>> = OnceLock::new();
fn mixer() -> &'static Mutex<Mixer> {
    MIXER.get_or_init(|| Mutex::new(Mixer::default()))
}
fn source(id: u32) -> Option<Arc<Source>> {
    mixer().lock().unwrap().sources.get(&id).cloned()
}
unsafe extern "C" fn render(_: *mut c_void, bytes: *mut u8, len: c_int) {
    if len <= 0 || bytes.is_null() {
        return;
    }
    let output = unsafe { std::slice::from_raw_parts_mut(bytes.cast::<i16>(), len as usize / 2) };
    output.fill(0);
    // Never hold the registry while invoking game callbacks or waiting for a close.
    let sources: Vec<_> = mixer().lock().unwrap().sources.values().cloned().collect();
    for chunk in output.chunks_mut(2048) {
        let mut sum = [0i32; 2048];
        let mut scratch = [0i16; 2048];
        for source in &sources {
            let mut s = source.inner.lock().unwrap();
            if s.closed || !s.active {
                continue;
            }
            let converter = s.converter as *mut c_void;
            if let Some(callback) = s.callback {
                let numerator = (chunk.len() / 2) as u32 * s.rate + s.phase;
                let frames = numerator / RATE;
                s.phase = numerator % RATE;
                let samples = frames as usize * s.channels as usize;
                s.scratch[..samples].fill(0);
                unsafe {
                    callback(
                        s.user as *mut c_void,
                        s.scratch.as_mut_ptr().cast(),
                        (samples * 2) as c_int,
                    )
                };
                unsafe {
                    SDL_AudioStreamPut(converter, s.scratch.as_ptr().cast(), (samples * 2) as c_int)
                };
            }
            let got = unsafe {
                SDL_AudioStreamGet(
                    converter,
                    scratch.as_mut_ptr().cast(),
                    (chunk.len() * 2) as c_int,
                )
            };
            if got > 0 {
                let count = got as usize / 2;
                for (a, b) in sum.iter_mut().zip(&scratch[..count]) {
                    *a += i32::from(*b)
                }
                s.queued = (s.queued - (count / 2) as f64 * s.rate as f64 / RATE as f64).max(0.);
            }
            source.wake.notify_all();
        }
        for (dst, sum) in chunk.iter_mut().zip(sum) {
            *dst = sum.clamp(i16::MIN as i32, i16::MAX as i32) as i16
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn clbb_mixer_open(
    rate: u32,
    channels: u32,
    capacity: u32,
    callback: Option<Render>,
    user: *mut c_void,
) -> u32 {
    if !(8000..=192000).contains(&rate) || !(1..=2).contains(&channels) {
        return 0;
    }
    let mut m = mixer().lock().unwrap();
    if m.sources.len() >= 64 {
        return 0;
    }
    if m.device == 0 {
        if unsafe { SDL_InitSubSystem(0x10) } < 0 {
            return 0;
        }
        let want = Spec {
            freq: RATE as c_int,
            format: FORMAT,
            channels: 2,
            silence: 0,
            samples: 512,
            padding: 0,
            size: 0,
            callback: Some(render),
            userdata: std::ptr::null_mut(),
        };
        m.device =
            unsafe { SDL_OpenAudioDevice(std::ptr::null(), 0, &want, std::ptr::null_mut(), 0) };
        if m.device == 0 {
            eprintln!("[CLBB_AUDIO] shared output open failed");
            return 0;
        }
        // SDL opens paused; release the registry before its first callback.
    }
    let converter = unsafe {
        SDL_NewAudioStream(
            FORMAT,
            channels as u8,
            rate as c_int,
            FORMAT,
            2,
            RATE as c_int,
        )
    };
    if converter.is_null() {
        return 0;
    }
    m.next = m.next.wrapping_add(1).max(1);
    let id = m.next;
    let src = Arc::new(Source {
        inner: Mutex::new(Inner {
            converter: converter as usize,
            callback,
            user: user as usize,
            scratch: vec![0; ((1024 * rate / RATE + 2) * channels) as usize],
            rate,
            channels,
            phase: 0,
            queued: 0.,
            capacity: capacity.clamp(rate / 50, rate / 5),
            active: callback.is_some(),
            closed: false,
        }),
        wake: Condvar::new(),
    });
    m.sources.insert(id, src);
    let device = m.device;
    drop(m);
    unsafe { SDL_PauseAudioDevice(device, 0) };
    eprintln!(
        "[CLBB_AUDIO] shared source opened id={id} rate={rate} channels={channels} callback={}",
        callback.is_some()
    );
    id
}
#[unsafe(no_mangle)]
pub extern "C" fn clbb_mixer_close(id: u32) {
    let src = mixer().lock().unwrap().sources.remove(&id);
    if let Some(src) = src {
        let mut s = src.inner.lock().unwrap();
        s.closed = true;
        s.active = false;
        src.wake.notify_all();
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn clbb_mixer_active(id: u32, active: c_int) {
    if let Some(src) = source(id) {
        let mut s = src.inner.lock().unwrap();
        s.active = active != 0;
        src.wake.notify_all();
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn clbb_mixer_clear(id: u32) {
    if let Some(src) = source(id) {
        let mut s = src.inner.lock().unwrap();
        unsafe { SDL_AudioStreamClear(s.converter as *mut c_void) };
        s.queued = 0.;
        src.wake.notify_all();
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn clbb_mixer_available(id: u32) -> c_int {
    source(id)
        .map(|src| {
            let s = src.inner.lock().unwrap();
            if s.closed {
                -3
            } else {
                (s.capacity as f64 - s.queued).max(0.) as c_int
            }
        })
        .unwrap_or(-3)
}
#[unsafe(no_mangle)]
pub extern "C" fn clbb_mixer_pending(id: u32) -> c_int {
    source(id)
        .map(|src| src.inner.lock().unwrap().queued.ceil() as c_int)
        .unwrap_or(0)
}
/// Interleaved PCM16, bounded backpressure. Never keeps caller memory after return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clbb_mixer_write(id: u32, data: *const i16, frames: c_int) -> c_int {
    if frames < 0 || frames > 1048576 || data.is_null() {
        return -2;
    }
    let Some(src) = source(id) else { return -3 };
    let mut s = src.inner.lock().unwrap();
    let mut written = 0;
    while written < frames {
        if s.closed {
            break;
        }
        let free = (s.capacity as f64 - s.queued).max(0.) as c_int;
        if free == 0 {
            if !s.active {
                break;
            }
            let (guard, timeout) = src
                .wake
                .wait_timeout(s, Duration::from_millis(250))
                .unwrap();
            s = guard;
            if timeout.timed_out() {
                break;
            }
            continue;
        }
        let count = free.min(frames - written);
        let start = unsafe { data.add(written as usize * s.channels as usize) };
        if unsafe {
            SDL_AudioStreamPut(
                s.converter as *mut c_void,
                start.cast(),
                count * s.channels as c_int * 2,
            )
        } < 0
        {
            break;
        }
        written += count;
        s.queued += count as f64;
    }
    written
}

pub fn status() -> String {
    let (device, sources) = {
        let m = mixer().lock().unwrap();
        (
            m.device,
            m.sources
                .iter()
                .map(|(id, src)| (*id, src.clone()))
                .collect::<Vec<_>>(),
        )
    };
    serde_json::json!({"device":device,"desktop_outputs":if device==0{0}else{1},"rate":RATE,"channels":2,"sources":sources.iter().map(|(id,src)|{let s=src.inner.lock().unwrap();serde_json::json!({"id":id,"rate":s.rate,"channels":s.channels,"callback":s.callback.is_some(),"active":s.active,"queued_frames":s.queued})}).collect::<Vec<_>>()}).to_string()
}
