//! Only the C boundary is unsafe. Qt guarantees serialized, non-reentrant calls
//! per handle; each callback receives a distinct host context, never a Rust
//! state reference. Typed frames/events are copied before invoking callbacks.
use super::{
    Controller,
    host::{Binding as SafeBinding, Host, Timer},
    snapshot::{Frame, Text},
    view::{Binding, ButtonView, View, text},
};
use crate::Point;
use std::ffi::c_void;
#[repr(C)]
pub struct Api {
    pub context: *mut c_void,
    pub begin: Option<unsafe extern "C" fn(*mut c_void, Point) -> i32>,
    pub move_touch: Option<unsafe extern "C" fn(*mut c_void, i32, Point) -> u8>,
    pub end: Option<unsafe extern "C" fn(*mut c_void, i32, Point)>,
    pub project: Option<unsafe extern "C" fn(*mut c_void, i32, Point, f64, *mut Point) -> u8>,
    pub capture: Option<unsafe extern "C" fn(*mut c_void, Point) -> u64>,
    pub resolve: Option<unsafe extern "C" fn(*mut c_void, u64, *mut Point) -> u8>,
    pub release_resolver: Option<unsafe extern "C" fn(*mut c_void, u64)>,
    pub navigation: Option<
        unsafe extern "C" fn(*mut c_void, Point, Point, u8, *mut Point, *mut u32, *mut Text) -> u8,
    >,
    pub binding:
        Option<unsafe extern "C" fn(*mut c_void, i32, *const ButtonView, *const Text) -> Binding>,
    pub cancel_key: Option<unsafe extern "C" fn(*mut c_void, i32) -> i32>,
    pub timer: Option<unsafe extern "C" fn(*mut c_void, i32, i64)>,
    pub trace: Option<unsafe extern "C" fn(*mut c_void, *const View, *const Text, *const Text)>,
    pub clock: Option<unsafe extern "C" fn(*mut c_void) -> i64>,
}
struct ForeignHost<'a>(&'a Api);
impl ForeignHost<'_> {
    fn project(&mut self, kind: i32, p: Point, plane: f64) -> Option<Point> {
        let mut out = Point::default();
        let f = self.0.project?;
        if unsafe { f(self.0.context, kind, p, plane, &mut out) } != 0 {
            Some(out)
        } else {
            None
        }
    }
}
impl Host for ForeignHost<'_> {
    fn clock_ns(&mut self) -> Option<i64> {
        self.0.clock.map(|f| unsafe { f(self.0.context) })
    }
    fn begin(&mut self, p: Point) -> i32 {
        self.0
            .begin
            .map(|f| unsafe { f(self.0.context, p) })
            .unwrap_or(-1)
    }
    fn move_touch(&mut self, id: i32, p: Point) -> bool {
        self.0
            .move_touch
            .is_some_and(|f| unsafe { f(self.0.context, id, p) } != 0)
    }
    fn end(&mut self, id: i32, p: Point) {
        if let Some(f) = self.0.end {
            unsafe { f(self.0.context, id, p) }
        }
    }
    fn pointer_world(&mut self, p: Point) -> Option<Point> {
        self.project(0, p, 0.)
    }
    fn ground_world(&mut self, p: Point) -> Option<Point> {
        self.project(1, p, 0.)
    }
    fn movement_direction(&mut self, p: Point) -> Option<Point> {
        self.project(2, p, 0.)
    }
    fn hero_world(&mut self) -> Option<Point> {
        self.project(3, Point::default(), 0.)
    }
    fn pointer_world_at_plane(&mut self, p: Point, plane: f64) -> Option<Point> {
        self.project(4, p, plane)
    }
    fn capture_pointer_world(&mut self, p: Point) -> u64 {
        self.0
            .capture
            .map(|f| unsafe { f(self.0.context, p) })
            .unwrap_or(0)
    }
    fn resolve_world(&mut self, id: u64) -> Option<Point> {
        let f = self.0.resolve?;
        let mut out = Point::default();
        if unsafe { f(self.0.context, id, &mut out) } != 0 {
            Some(out)
        } else {
            None
        }
    }
    fn release_resolver(&mut self, id: u64) {
        if let Some(f) = self.0.release_resolver {
            unsafe { f(self.0.context, id) }
        }
    }
    fn navigation(
        &mut self,
        start: Point,
        target: Point,
        avoid: bool,
    ) -> Result<Vec<Point>, String> {
        let Some(f) = self.0.navigation else {
            return Err(String::new());
        };
        let mut points = vec![Point::default(); 4096];
        let mut count = 4096;
        let mut reason = Text::default();
        if unsafe {
            f(
                self.0.context,
                start,
                target,
                avoid as u8,
                points.as_mut_ptr(),
                &mut count,
                &mut reason,
            )
        } == 0
        {
            return Err(reason.owned());
        }
        if count > 4096 {
            return Err("Нет безопасного маршрута".into());
        }
        points.truncate(count as usize);
        Ok(points)
    }
    fn binding(&mut self, hero: i32, b: &super::Button, config: &str) -> SafeBinding {
        let Some(f) = self.0.binding else {
            return SafeBinding::default();
        };
        let button = ButtonView::from(b);
        let config = text(config);
        let binding = unsafe { f(self.0.context, hero, &button, &config) };
        SafeBinding {
            key: binding.key,
            cast_mode: binding.cast_mode,
            aim_type: 0,
            stop_movement: binding.stop_movement != 0,
        }
    }
    fn cancel_key(&mut self, hero: i32) -> i32 {
        self.0
            .cancel_key
            .map(|f| unsafe { f(self.0.context, hero) })
            .unwrap_or(84)
    }
    fn timer(&mut self, timer: Timer, delay: Option<i64>) {
        if let Some(f) = self.0.timer {
            let id = match timer {
                Timer::Main => 0,
                Timer::Pointer => 1,
                Timer::Tap => 2,
                Timer::Movement => 3,
                Timer::Auxiliary => 4,
            };
            unsafe { f(self.0.context, id, delay.unwrap_or(-1)) }
        }
    }
    fn trace(&mut self, state: &Controller, event: &str, reason: &str) {
        if let Some(f) = self.0.trace {
            let view = View::of(state);
            let event = text(event);
            let reason = text(reason);
            unsafe { f(self.0.context, &view, &event, &reason) }
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Event {
    pub kind: u32,
    pub first: i32,
    pub second: i32,
    pub third: i32,
    pub point: Point,
    pub text: Text,
}
#[unsafe(no_mangle)]
pub extern "C" fn ewm_mlbb_controller_new() -> *mut c_void {
    Box::into_raw(Box::new(Controller::new())).cast()
}
/// The handle must be uniquely owned and idle, obtained from new().
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ewm_mlbb_controller_free(raw: *mut c_void) {
    if !raw.is_null() {
        drop(unsafe { Box::from_raw(raw.cast::<Controller>()) })
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ewm_mlbb_controller_dispatch(
    raw: *mut c_void,
    api: *const Api,
    event: *const Event,
    now: i64,
) -> u8 {
    let (Some(state), Some(api), Some(event)) = (
        unsafe { raw.cast::<Controller>().as_mut() },
        unsafe { api.as_ref() },
        unsafe { event.as_ref() },
    ) else {
        return 0;
    };
    let event = *event;
    state.now_ = now;
    state.changed_ = false;
    let host = &mut ForeignHost(api);
    let handled = match event.kind {
        1 => {
            state.set_enabled(host, event.first != 0);
            true
        }
        2 => {
            state.set_android_size(host, event.point);
            true
        }
        3 => {
            if super::finite(event.point) {
                state.heroScreen_ = super::bounded(event.point)
            }
            true
        }
        4 => {
            state.observe_aim(event.first != 0, event.point, event.second);
            true
        }
        5 => {
            state.update_pointer(host, event.point);
            true
        }
        6 => state.handle_key(host, event.first, event.second != 0, event.third != 0),
        7 => state.handle_mouse(host, event.first, event.second != 0, event.point),
        8 => {
            state.release_all(host);
            true
        }
        9 => {
            state.invalidate(host, event.text.as_str());
            true
        }
        10 => {
            state.tick(host);
            true
        }
        11 => {
            state.flush_pointer(host);
            true
        }
        12 => {
            if state.ready() && state.movementReady_ {
                state.update_movement(host, true)
            }
            true
        }
        13 => {
            state.expire_auxiliary_taps(host);
            true
        }
        14 => {
            state.set_navigation_state(host, event.first != 0, event.text.owned());
            true
        }
        15 => {
            state.refresh_bindings(host);
            true
        }
        16 => {
            state.cancelKey_ = event.first;
            true
        }
        17 => {
            if (0..=1).contains(&event.first) && state.movementClickMode_ != event.first {
                if state.movementAuto_ {
                    state.finish_movement(host, "")
                }
                state.movementClickMode_ = event.first;
                state.changed_ = true
            }
            true
        }
        18 => {
            if (40..=600).contains(&event.first) {
                state.movementHoldMs_ = event.first;
                state.changed_ = true
            }
            true
        }
        19 => {
            state.traceEnabled_ = event.first != 0;
            state.changed_ = true;
            true
        }
        20 => {
            state.finish_skill(host, true);
            true
        }
        21 => {
            state.finish_movement(host, "");
            true
        }
        _ => false,
    };
    handled as u8
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ewm_mlbb_controller_frame(
    raw: *mut c_void,
    api: *const Api,
    frame: *const Frame,
    now: i64,
) {
    let (Some(state), Some(api), Some(frame)) = (
        unsafe { raw.cast::<Controller>().as_mut() },
        unsafe { api.as_ref() },
        unsafe { frame.as_ref() },
    ) else {
        return;
    };
    let frame = *frame;
    state.now_ = now;
    state.ingest_frame(&mut ForeignHost(api), &frame);
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ewm_mlbb_controller_view(raw: *const c_void, out: *mut View) {
    let (Some(state), Some(out)) = (unsafe { raw.cast::<Controller>().as_ref() }, unsafe {
        out.as_mut()
    }) else {
        return;
    };
    *out = View::of(state);
}
#[unsafe(no_mangle)]
pub extern "C" fn ewm_mlbb_controller_layout(kind: u32) -> u64 {
    (match kind {
        0 => std::mem::size_of::<Frame>(),
        1 => std::mem::size_of::<View>(),
        2 => std::mem::size_of::<Api>(),
        3 => std::mem::size_of::<Event>(),
        4 => std::mem::size_of::<ButtonView>(),
        _ => 0,
    }) as u64
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ewm_mlbb_controller_view_scalars(raw: *const c_void, out: *mut View) {
    let (Some(state), Some(out)) = (unsafe { raw.cast::<Controller>().as_ref() }, unsafe {
        out.as_mut()
    }) else {
        return;
    };
    out.refresh_scalars(state);
}
