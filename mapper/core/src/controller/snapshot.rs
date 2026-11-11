//! Typed snapshots copied at the FFI boundary; no JSON or serde in control.
#![allow(non_snake_case)]
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Number {
    pub value: f64,
    pub kind: i32,
}
impl Number {
    pub fn is_number(self) -> bool {
        self.kind == 1
    }
    pub fn get(self, default: f64) -> f64 {
        if self.kind == 1 { self.value } else { default }
    }
    pub fn int(self) -> i32 {
        let n = self.get(0.);
        if n.is_finite() && n.fract() == 0. && n >= i32::MIN as f64 && n <= i32::MAX as f64 {
            n as i32
        } else {
            0
        }
    }
    pub fn missing(self) -> bool {
        self.kind == 0
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Flag {
    pub value: i32,
}
impl Default for Flag {
    fn default() -> Self {
        Self { value: -1 }
    }
}
impl Flag {
    pub fn get(self) -> bool {
        self.value == 1
    }
    pub fn is_bool(self) -> bool {
        self.value == 0 || self.value == 1
    }
    pub fn missing(self) -> bool {
        self.value == -1
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Text {
    pub bytes: [u8; 256],
    pub size: u32,
    pub present: u32,
}
impl Default for Text {
    fn default() -> Self {
        Self {
            bytes: [0; 256],
            size: 0,
            present: 0,
        }
    }
}
impl Text {
    pub fn as_str(&self) -> &str {
        if self.present != 1 || self.size > 256 {
            return "";
        }
        std::str::from_utf8(&self.bytes[..self.size as usize]).unwrap_or("")
    }
    pub fn owned(&self) -> String {
        self.as_str().to_owned()
    }
    pub fn missing(self) -> bool {
        self.present == 0
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Vector2 {
    pub values: [Number; 2],
    pub count: u32,
}
impl Vector2 {
    pub fn valid(&self) -> bool {
        self.count == 2
            && self
                .values
                .iter()
                .all(|n| n.is_number() && n.value.is_finite())
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Vector3 {
    pub values: [Number; 3],
    pub count: u32,
}
impl Vector3 {
    pub fn valid(&self) -> bool {
        self.count == 3
            && self
                .values
                .iter()
                .all(|n| n.is_number() && n.value.is_finite())
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Vector16 {
    pub values: [Number; 16],
    pub count: u32,
}
impl Vector16 {
    pub fn valid(&self) -> bool {
        self.count == 16
            && self
                .values
                .iter()
                .all(|n| n.is_number() && n.value.is_finite())
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Button {
    pub slot: Text,
    pub kind: Text,
    pub geometry_source: Text,
    pub index: Number,
    pub skill_id: Number,
    pub active: Flag,
    pub press_uv: Vector2,
    pub center_uv: Vector2,
    pub radius_uv: Vector2,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Geometry {
    pub ready: Flag,
    pub source: Text,
    pub slot: Text,
    pub skill_id: Number,
    pub sample_start_ns: Number,
    pub sample_end_ns: Number,
    pub center_uv: Vector2,
    pub radius_uv: Vector2,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Variant {
    pub slot: Text,
    pub base_skill_id: Number,
    pub active_skill_id: Number,
    pub cast_locked: Flag,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VariantState {
    pub ready: Flag,
    pub source: Text,
    pub token: Text,
    pub hero_id: Number,
    pub sample_start_ns: Number,
    pub sample_end_ns: Number,
    pub buttons: [Variant; 32],
    pub count: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CooldownRow {
    pub slot: Text,
    pub base_skill_id: Number,
    pub active_skill_id: Number,
    pub ready: Flag,
    pub on_cooldown: Flag,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CooldownState {
    pub present: Flag,
    pub ready: Flag,
    pub coherent: Flag,
    pub source: Text,
    pub token: Text,
    pub hero_id: Number,
    pub sample_start_ns: Number,
    pub sample_end_ns: Number,
    pub buttons: [CooldownRow; 32],
    pub count: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct MovementGeometry {
    pub ready: Flag,
    pub press_uv: Vector2,
    pub center_uv: Vector2,
    pub radius_uv: Vector2,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Ui {
    pub ready: Flag,
    pub hero_id: Number,
    pub time_ns: Number,
    pub resolution_px: Vector2,
    pub buttons: [Button; 32],
    pub button_count: u32,
    pub capability_controls: [Button; 32],
    pub capability_count: u32,
    pub active_slot: Text,
    pub joystick: Geometry,
    pub cancel_uv: Vector2,
    pub movement: MovementGeometry,
    pub cooldown_state: CooldownState,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Motion {
    pub source: Text,
    pub available: Flag,
    pub bracket_stable: Flag,
    pub drag_touch_raw_xyz: Vector3,
    pub sample_start_ns: Number,
    pub sample_end_ns: Number,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Logical {
    pub ready: Flag,
    pub input_ready: Flag,
    pub coherent: Flag,
    pub source: Text,
    pub skill_id: Number,
    pub sample_start_ns: Number,
    pub sample_end_ns: Number,
    pub origin_render_xyz: Vector3,
    pub raw_touch_xyz: Vector3,
    pub offset_xz: Vector2,
    pub target_render_xyz: Vector3,
    pub joystick: Geometry,
    pub drag_ratio: Number,
    pub world_range: Number,
    pub world_range_source: Text,
    pub world_range_source_present: Flag,
    pub visual_plane_y: Number,
    pub valid_drag: Flag,
    pub first_drag: Flag,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Direction {
    pub ready: Flag,
    pub coherent: Flag,
    pub source: Text,
    pub token: Text,
    pub hero_id: Number,
    pub slot: Text,
    pub skill_id: Number,
    pub sample_start_ns: Number,
    pub sample_end_ns: Number,
    pub raw_touch_xyz: Vector3,
    pub direction_xz: Vector2,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CancelState {
    pub ready: Flag,
    pub coherent: Flag,
    pub source: Text,
    pub token: Text,
    pub hero_id: Number,
    pub slot: Text,
    pub base_skill_id: Number,
    pub cancelled: Flag,
    pub sample_start_ns: Number,
    pub sample_end_ns: Number,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Frame {
    pub ready: Flag,
    pub hero_id: Number,
    pub time_ns: Number,
    pub sample_duration_ms: Number,
    pub skill_id: Number,
    pub token: Text,
    pub token_nonempty: Flag,
    pub token_string: Flag,
    pub view_matrix: Vector16,
    pub hero_render_xyz: Vector3,
    pub hero_screen_uv: Vector2,
    pub ui: Ui,
    pub skill_state: VariantState,
    pub active_joystick: Geometry,
    pub direction_input: Direction,
    pub cancel_state: CancelState,
    pub logical: Logical,
    pub motion: Motion,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn qt_integer_conversion_rejects_fractions_and_saturation() {
        for value in [
            1.5,
            -1.5,
            f64::NAN,
            f64::INFINITY,
            2147483648.,
            -2147483649.,
        ] {
            assert_eq!(Number { value, kind: 1 }.int(), 0)
        }
        assert_eq!(
            Number {
                value: 2510.,
                kind: 1
            }
            .int(),
            2510
        );
        assert_eq!(
            Number {
                value: -1.,
                kind: 1
            }
            .int(),
            -1
        );
        assert_eq!(
            Number {
                value: 2510.,
                kind: 2
            }
            .int(),
            0
        );
    }
}
