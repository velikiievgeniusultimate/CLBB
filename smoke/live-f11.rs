//! Temporary F11 fullscreen adapter for one existing X11 window.
//! Build: rustc --edition=2024 -O smoke/live-f11.rs -o smoke/live-f11
use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void};
use std::ptr;
use std::sync::atomic::{AtomicU8, Ordering};

type Display = c_void;
type Window = c_ulong;
type Atom = c_ulong;

const KEY_PRESS: c_int = 2;
const KEY_RELEASE: c_int = 3;
const DESTROY_NOTIFY: c_int = 17;
const CLIENT_MESSAGE: c_int = 33;
const BAD_WINDOW: u8 = 3;
const BAD_ACCESS: u8 = 10;
const ANY_MODIFIER: c_uint = 1 << 15;
const GRAB_MODE_ASYNC: c_int = 1;
const STRUCTURE_NOTIFY_MASK: c_long = 1 << 17;
const SUBSTRUCTURE_NOTIFY_MASK: c_long = 1 << 19;
const SUBSTRUCTURE_REDIRECT_MASK: c_long = 1 << 20;
const XK_F11: c_ulong = 0xffc8;
static X_ERROR: AtomicU8 = AtomicU8::new(0);

#[repr(C)]
#[derive(Clone, Copy)]
struct XKeyEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut Display,
    window: Window,
    root: Window,
    subwindow: Window,
    time: c_ulong,
    x: c_int,
    y: c_int,
    x_root: c_int,
    y_root: c_int,
    state: c_uint,
    keycode: c_uint,
    same_screen: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct XDestroyWindowEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut Display,
    event: Window,
    window: Window,
}

#[repr(C)]
#[derive(Clone, Copy)]
union ClientData {
    bytes: [c_char; 20],
    shorts: [i16; 10],
    longs: [c_long; 5],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct XClientMessageEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut Display,
    window: Window,
    message_type: Atom,
    format: c_int,
    data: ClientData,
}

#[repr(C)]
union XEvent {
    type_: c_int,
    key: XKeyEvent,
    destroy: XDestroyWindowEvent,
    client: XClientMessageEvent,
    padding: [c_long; 24],
}

#[repr(C)]
struct XErrorEvent {
    type_: c_int,
    display: *mut Display,
    resource_id: c_ulong,
    serial: c_ulong,
    error_code: u8,
    request_code: u8,
    minor_code: u8,
}

type ErrorHandler = Option<unsafe extern "C" fn(*mut Display, *mut XErrorEvent) -> c_int>;

#[link(name = "X11")]
unsafe extern "C" {
    fn XOpenDisplay(name: *const c_char) -> *mut Display;
    fn XCloseDisplay(display: *mut Display) -> c_int;
    fn XSetErrorHandler(handler: ErrorHandler) -> ErrorHandler;
    fn XGetGeometry(display: *mut Display, drawable: Window, root: *mut Window,
                    x: *mut c_int, y: *mut c_int, width: *mut c_uint,
                    height: *mut c_uint, border: *mut c_uint, depth: *mut c_uint) -> c_int;
    fn XSelectInput(display: *mut Display, window: Window, event_mask: c_long) -> c_int;
    fn XKeysymToKeycode(display: *mut Display, keysym: c_ulong) -> u8;
    fn XkbSetDetectableAutoRepeat(display: *mut Display, detectable: c_int,
                                  supported: *mut c_int) -> c_int;
    fn XGrabKey(display: *mut Display, keycode: c_int, modifiers: c_uint,
                window: Window, owner_events: c_int, pointer_mode: c_int,
                keyboard_mode: c_int) -> c_int;
    fn XInternAtom(display: *mut Display, name: *const c_char, only_if_exists: c_int) -> Atom;
    fn XSendEvent(display: *mut Display, window: Window, propagate: c_int,
                  event_mask: c_long, event: *mut XEvent) -> c_int;
    fn XSync(display: *mut Display, discard: c_int) -> c_int;
    fn XNextEvent(display: *mut Display, event: *mut XEvent) -> c_int;
}

struct Connection(*mut Display);
impl Drop for Connection {
    fn drop(&mut self) {
        // Closing this client releases its per-window passive grab.
        unsafe { XCloseDisplay(self.0); }
    }
}

unsafe extern "C" fn record_x_error(_: *mut Display, error: *mut XErrorEvent) -> c_int {
    if !error.is_null() {
        let code = unsafe { (*error).error_code };
        let _ = X_ERROR.compare_exchange(0, code, Ordering::Relaxed, Ordering::Relaxed);
    }
    0
}

fn check_x_error() -> Result<bool, String> {
    match X_ERROR.swap(0, Ordering::Relaxed) {
        0 => Ok(true),
        BAD_WINDOW => Ok(false),
        BAD_ACCESS => Err("F11 is already grabbed for this window/modifier combination".into()),
        code => Err(format!("X11 error {code}")),
    }
}

fn window_argument() -> Result<Option<Window>, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        return Ok(None);
    }
    if args.len() != 1 {
        return Err("expected one hexadecimal X11 window ID".into());
    }
    let text = args[0].strip_prefix("0x").or_else(|| args[0].strip_prefix("0X"))
        .unwrap_or(&args[0]);
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("window ID must be hexadecimal, optionally prefixed with 0x".into());
    }
    let value = u64::from_str_radix(text, 16).map_err(|_| "window ID is out of range")?;
    let window = Window::try_from(value).map_err(|_| "window ID is out of range")?;
    if window == 0 || value > u32::MAX as u64 {
        return Err("window ID must be a nonzero 32-bit X11 resource ID".into());
    }
    Ok(Some(window))
}

unsafe fn run(window: Window) -> Result<(), String> {
    unsafe {
        let display = XOpenDisplay(ptr::null()); // Reads DISPLAY; never changes focus.
        if display.is_null() {
            return Err("cannot open X11 display from DISPLAY".into());
        }
        let _connection = Connection(display);
        XSetErrorHandler(Some(record_x_error));
        let mut root = 0;
        let (mut x, mut y) = (0, 0);
        let (mut width, mut height, mut border, mut depth) = (0, 0, 0, 0);
        let exists = XGetGeometry(display, window, &mut root, &mut x, &mut y,
                                  &mut width, &mut height, &mut border, &mut depth);
        XSync(display, 0);
        if !check_x_error()? || exists == 0 {
            return Ok(());
        }
        let keycode = XKeysymToKeycode(display, XK_F11);
        if keycode == 0 {
            return Err("the X11 keymap has no F11 keycode".into());
        }
        let mut supported = 0;
        let enabled = XkbSetDetectableAutoRepeat(display, 1, &mut supported);
        if enabled == 0 || supported == 0 {
            return Err("X11 detectable autorepeat is unavailable; refusing an unsafe repeat fallback".into());
        }
        XSelectInput(display, window, STRUCTURE_NOTIFY_MASK);
        XGrabKey(display, keycode as c_int, ANY_MODIFIER, window, 0,
                  GRAB_MODE_ASYNC, GRAB_MODE_ASYNC);
        let state = XInternAtom(display, c"_NET_WM_STATE".as_ptr(), 0);
        let fullscreen = XInternAtom(display, c"_NET_WM_STATE_FULLSCREEN".as_ptr(), 0);
        XSync(display, 0);
        if !check_x_error()? {
            return Ok(());
        }
        eprintln!("F11 fullscreen adapter attached to window 0x{window:x}");
        let mut held = false;
        loop {
            let mut event = XEvent { padding: [0; 24] };
            XNextEvent(display, &mut event);
            if !check_x_error()? {
                return Ok(());
            }
            match event.type_ {
                DESTROY_NOTIFY if event.destroy.window == window => return Ok(()),
                KEY_RELEASE if event.key.window == window && event.key.keycode == keycode as c_uint => {
                    held = false; // Both press and release are consumed by this client.
                }
                KEY_PRESS if event.key.window == window && event.key.keycode == keycode as c_uint => {
                    if held {
                        continue;
                    }
                    held = true;
                    let mut message = XEvent {
                        client: XClientMessageEvent {
                            type_: CLIENT_MESSAGE,
                            serial: 0,
                            send_event: 1,
                            display,
                            window,
                            message_type: state,
                            format: 32,
                            data: ClientData { longs: [2, fullscreen as c_long, 0, 2, 0] },
                        },
                    };
                    if XSendEvent(display, root, 0,
                                  SUBSTRUCTURE_REDIRECT_MASK | SUBSTRUCTURE_NOTIFY_MASK,
                                  &mut message) == 0 {
                        return Err("X11 rejected the fullscreen request".into());
                    }
                    XSync(display, 0);
                    if !check_x_error()? {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
    }
}

fn main() {
    let window = match window_argument() {
        Ok(Some(window)) => window,
        Ok(None) => {
            println!("Usage: live-f11 HEX_WINDOW_ID\n\nUses DISPLAY. Consumes F11 only for the specified window and toggles\n_NET_WM_STATE_FULLSCREEN once per physical press. Exits when the window\nis destroyed. Does not activate the window or grab pointer input.");
            return;
        }
        Err(error) => {
            eprintln!("live-f11: {error}\nUsage: live-f11 HEX_WINDOW_ID");
            std::process::exit(2);
        }
    };
    if let Err(error) = unsafe { run(window) } {
        eprintln!("live-f11: {error}");
        std::process::exit(1);
    }
}
