use super::{Button, Controller};
use crate::Point;
#[derive(Clone, Copy, Debug)]
pub enum Timer {
    Main,
    Pointer,
    Tap,
    Movement,
    Auxiliary,
}
/// Qt supplies transport and camera projection only. Implementations must defer
/// all reentrant events until the current Rust dispatch returns.
pub trait Host {
    /// Refresh causal boundaries after synchronous transport returns. Tests may
    /// retain their injected deterministic clock by leaving this unavailable.
    fn clock_ns(&mut self) -> Option<i64> {
        None
    }
    fn begin(&mut self, p: Point) -> i32;
    fn move_touch(&mut self, id: i32, p: Point) -> bool;
    fn end(&mut self, id: i32, p: Point);
    fn cancel_touch(&mut self, _id: i32, _p: Point) -> bool {
        false
    }
    fn pointer_world(&mut self, p: Point) -> Option<Point>;
    fn ground_world(&mut self, p: Point) -> Option<Point>;
    fn skill_direction_world(&mut self, p:Point, _skill:i32)->Option<Point> {self.ground_world(p)}
    fn movement_direction(&mut self, p: Point) -> Option<Point>;
    fn hero_world(&mut self) -> Option<Point>;
    fn pointer_world_at_plane(&mut self, p: Point, plane: f64) -> Option<Point>;
    fn capture_pointer_world(&mut self, p: Point) -> u64;
    fn resolve_world(&mut self, id: u64) -> Option<Point>;
    fn release_resolver(&mut self, id: u64);
    fn navigation(
        &mut self,
        start: Point,
        target: Point,
        avoid: bool,
    ) -> Result<Vec<Point>, String>;
    fn binding(&mut self, hero: i32, button: &Button, config: &str) -> Binding;
    fn cancel_key(&mut self, hero: i32) -> i32;
    fn timer(&mut self, timer: Timer, delay_ms: Option<i64>);
    fn trace(&mut self, state: &Controller, event: &str, reason: &str);
}
#[derive(Clone, Copy, Default, Debug)]
pub struct Binding {
    pub key: i32,
    pub cast_mode: i32,
    /// Verified native template: 0 unknown, 1 tap, 2 direction, 3 range.
    pub aim_type: u8,
    pub stop_movement: bool,
}
