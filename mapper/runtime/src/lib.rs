#![deny(unsafe_op_in_unsafe_fn)]
#![recursion_limit="512"]
pub mod aim;
mod hero_names;
mod control_audit;
mod target_contacts;
mod target_cancel_geometry;
mod target_selection;
pub mod portrait_picker;
mod portrait_gesture;
pub mod linked_gesture;
pub mod math;
pub mod navigation;
pub mod optional_ui;
pub mod pacing;
pub mod platform;
pub mod projection;
pub mod reflection;
#[cfg(target_os="linux")]
mod native_debug;
pub mod flight_schema;
pub mod recorder;
pub mod render_metrics;
#[cfg(target_os = "linux")]
pub mod runtime;
pub mod settings;
pub mod source;
pub mod unity;
pub mod updater;

#[cfg(target_os = "linux")]
pub mod audio;
pub mod playback;

mod attack;
