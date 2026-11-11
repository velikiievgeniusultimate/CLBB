//! Allocation-free MLBB input mathematics. Qt, transport and game reading stay
//! outside this crate. All numerical/state-machine operations are safe Rust;
//! `ffi` is the small C pointer boundary used by the Qt adapter.
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
pub mod controller;
#[allow(unsafe_code)]
mod ffi;
use std::ops::{Add, Div, Mul, Sub};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    fn finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
    fn length(self) -> f64 {
        self.x.hypot(self.y)
    }
    fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y
    }
}
impl Add for Point {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        Self::new(self.x + b.x, self.y + b.y)
    }
}
impl Sub for Point {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        Self::new(self.x - b.x, self.y - b.y)
    }
}
impl Mul<f64> for Point {
    type Output = Self;
    fn mul(self, b: f64) -> Self {
        Self::new(self.x * b, self.y * b)
    }
}
impl Div<f64> for Point {
    type Output = Self;
    fn div(self, b: f64) -> Self {
        Self::new(self.x / b, self.y / b)
    }
}
// Match std::min/max comparison order, including the legacy non-finite cases.
fn min(a: f64, b: f64) -> f64 {
    if b < a { b } else { a }
}
fn max(a: f64, b: f64) -> f64 {
    if a < b { b } else { a }
}
fn bounded(point: Point, center: Point, radius: f64) -> Point {
    let offset = point - center;
    let distance = offset.length();
    if distance > radius {
        center + offset * (radius / distance)
    } else {
        point
    }
}

/// POD layout is shared with app/MlbbCoreFfi.h. No handles/allocations are needed
/// when the controller creates, copies or caches a model on the hot path.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AimSolver {
    pub stage: i32,
    pub failed: u8,
    pub centre_sample: u8,
    pub warm: u8,
    pub rebase: u8,
    pub anchor_valid: u8,
    pub adapt_anchor_valid: u8,
    pub probe: f64,
    pub second_probe: f64,
    pub deadzone: f64,
    pub dx: f64,
    pub dy: f64,
    pub origin_touch: Point,
    pub origin_target: Point,
    pub x_touch: Point,
    pub x_target: Point,
    pub y_touch: Point,
    pub y_target: Point,
    pub last_touch: Point,
    pub last_target: Point,
    pub adapt_touch: Point,
    pub adapt_target: Point,
    pub j: [f64; 4],
    pub initial_j: [f64; 4],
}
impl Default for AimSolver {
    fn default() -> Self {
        Self {
            stage: 0,
            failed: 0,
            centre_sample: 0,
            warm: 0,
            rebase: 0,
            anchor_valid: 0,
            adapt_anchor_valid: 0,
            probe: 0.,
            second_probe: 0.,
            deadzone: 0.,
            dx: -1.,
            dy: -1.,
            origin_touch: Point::default(),
            origin_target: Point::default(),
            x_touch: Point::default(),
            x_target: Point::default(),
            y_touch: Point::default(),
            y_target: Point::default(),
            last_touch: Point::default(),
            last_target: Point::default(),
            adapt_touch: Point::default(),
            adapt_target: Point::default(),
            j: [0.; 4],
            initial_j: [0.; 4],
        }
    }
}
impl AimSolver {
    pub fn reset(&mut self) {
        self.stage = 0;
        self.failed = 0;
        self.deadzone = 0.;
        self.dx = -1.;
        self.dy = -1.;
        self.warm = 0;
        self.rebase = 0;
        self.anchor_valid = 0;
        self.adapt_anchor_valid = 0;
    }
    pub fn learned(&self) -> bool {
        self.stage == 5 && self.failed == 0
    }
    pub fn resume_from(&mut self, model: &Self) -> bool {
        if !model.learned() || !model.deadzone.is_finite() || model.deadzone < 0. {
            return false;
        }
        if model
            .j
            .iter()
            .chain(model.initial_j.iter())
            .any(|v| !v.is_finite())
        {
            return false;
        }
        let det = model.j[0] * model.j[3] - model.j[1] * model.j[2];
        if det.abs() < 1e-8 || !det.is_finite() {
            return false;
        }
        self.reset();
        self.j = model.j;
        self.initial_j = model.initial_j;
        self.deadzone = model.deadzone;
        self.centre_sample = model.centre_sample;
        self.stage = 5;
        self.warm = 1;
        self.rebase = 1;
        true
    }
    fn virtual_point(&self, physical: Point, center: Point) -> Point {
        let offset = physical - center;
        let distance = offset.length();
        if distance > self.deadzone && distance > 1e-9 {
            offset * (1. - self.deadzone / distance)
        } else {
            Point::default()
        }
    }
    pub fn predict(&self, desired: Point, center: Point, radius: f64) -> Option<Point> {
        if !self.learned()
            || self.anchor_valid == 0
            || !desired.finite()
            || !center.finite()
            || !radius.is_finite()
            || radius < 4.
        {
            return None;
        }
        let det = self.j[0] * self.j[3] - self.j[1] * self.j[2];
        if !det.is_finite() || det.abs() < 1e-8 {
            return None;
        }
        let error = desired - self.last_target;
        let mut offset = self.last_touch
            + Point::new(
                (self.j[3] * error.x - self.j[1] * error.y) / det,
                (-self.j[2] * error.x + self.j[0] * error.y) / det,
            );
        let size = offset.length();
        if !size.is_finite() {
            return None;
        }
        if size > 1e-9 {
            offset = offset * (1. + self.deadzone / size);
        }
        Some(bounded(center + offset, center, radius))
    }
    pub fn observation_consistent(
        &self,
        touch: Point,
        observed: Point,
        center: Point,
        noise_pixels: f64,
    ) -> bool {
        if !self.learned() || self.anchor_valid == 0 || !touch.finite() || !observed.finite() {
            return false;
        }
        let delta = self.virtual_point(touch, center) - self.last_touch;
        let expected = self.last_target
            + Point::new(
                self.j[0] * delta.x + self.j[1] * delta.y,
                self.j[2] * delta.x + self.j[3] * delta.y,
            );
        let mut norm = 0.;
        for value in self.j {
            norm += value * value;
        }
        (expected - observed).length() <= max(0.03, norm.sqrt() * noise_pixels)
    }
    fn estimate_deadzone(r1: f64, f1: Point, r2: f64, f2: Point, radius: f64) -> f64 {
        if r2 - r1 < 1. {
            return -1.;
        }
        let slope = (f2 - f1) / (r2 - r1);
        let size = slope.dot(slope);
        if size < 1e-10 {
            return -1.;
        }
        let d = r1 - f1.dot(slope) / size;
        let residual = f1 - slope * (r1 - d);
        if !d.is_finite()
            || d < -radius * 0.02
            || d > radius * 0.8
            || residual.length() > max(1e-4, f1.length() * 0.02)
        {
            return -1.;
        }
        max(0., d)
    }
    fn probe_point(&self, y: bool, distance: f64, center: Point, radius: f64) -> Point {
        if self.centre_sample != 0 {
            return bounded(
                center
                    + if y {
                        Point::new(0., distance)
                    } else {
                        Point::new(distance, 0.)
                    },
                center,
                radius,
            );
        }
        let offset = self.origin_touch - center;
        let sign = if (if y { offset.y } else { offset.x }) > 0. {
            -1.
        } else {
            1.
        };
        bounded(
            self.origin_touch
                + if y {
                    Point::new(0., sign * distance)
                } else {
                    Point::new(sign * distance, 0.)
                },
            center,
            radius,
        )
    }
    pub fn step(
        &mut self,
        touch: Point,
        observed: Point,
        desired: Point,
        center: Point,
        radius: f64,
        coherent_observation: bool,
        adapt_jacobian: bool,
    ) -> Option<Point> {
        if !coherent_observation {
            return self.predict(desired, center, radius);
        }
        if !touch.finite()
            || !observed.finite()
            || !desired.finite()
            || !center.finite()
            || !radius.is_finite()
            || radius < 4.
            || self.failed != 0
        {
            return None;
        }
        let limit = radius * 0.9;
        let base = min(limit, max(4., radius * 0.08));
        if self.stage == 0 {
            self.origin_touch = touch;
            self.origin_target = observed;
            self.centre_sample = u8::from((touch - center).length() <= max(2., radius * 0.01));
            self.probe = base;
            self.stage = 1;
            return Some(self.probe_point(false, self.probe, center, radius));
        }
        if self.stage == 1 {
            if (observed - self.origin_target).length() <= 1e-4 {
                if self.probe >= limit - 0.01 {
                    self.failed = 1;
                    return None;
                }
                self.probe = min(limit, self.probe * 2.);
                return Some(self.probe_point(false, self.probe, center, radius));
            }
            self.x_touch = touch;
            self.x_target = observed;
            self.second_probe = min(limit, self.probe + base);
            if self.centre_sample != 0 && self.second_probe - self.probe >= 1. {
                self.stage = 2;
                return Some(self.probe_point(false, self.second_probe, center, radius));
            }
            self.stage = 3;
            return Some(self.probe_point(true, self.probe, center, radius));
        }
        if self.stage == 2 {
            self.dx = Self::estimate_deadzone(
                (self.x_touch - center).length(),
                self.x_target - self.origin_target,
                (touch - center).length(),
                observed - self.origin_target,
                radius,
            );
            self.stage = 3;
            return Some(self.probe_point(true, self.probe, center, radius));
        }
        if self.stage == 3 {
            if (observed - self.origin_target).length() <= 1e-4 {
                if self.probe >= limit - 0.01 {
                    self.failed = 1;
                    return None;
                }
                self.probe = min(limit, self.probe * 2.);
                return Some(self.probe_point(true, self.probe, center, radius));
            }
            self.y_touch = touch;
            self.y_target = observed;
            self.second_probe = min(limit, self.probe + base);
            if self.centre_sample != 0 && self.second_probe - self.probe >= 1. {
                self.stage = 4;
                return Some(self.probe_point(true, self.second_probe, center, radius));
            }
            self.stage = 4;
        }
        if self.stage == 4 {
            if self.centre_sample != 0 {
                self.dy = Self::estimate_deadzone(
                    (self.y_touch - center).length(),
                    self.y_target - self.origin_target,
                    (touch - center).length(),
                    observed - self.origin_target,
                    radius,
                );
                if self.dx >= 0. && self.dy >= 0. && (self.dx - self.dy).abs() < radius * 0.08 {
                    self.deadzone = min(
                        (self.dx + self.dy) * 0.5,
                        min(
                            (self.x_touch - center).length(),
                            (self.y_touch - center).length(),
                        ) * 0.95,
                    );
                }
            }
            let origin = self.virtual_point(self.origin_touch, center);
            let x = self.virtual_point(self.x_touch, center) - origin;
            let y = self.virtual_point(self.y_touch, center) - origin;
            let u = self.x_target - self.origin_target;
            let v = self.y_target - self.origin_target;
            let det = x.x * y.y - y.x * x.y;
            if det.abs() < 1e-4 {
                self.failed = 1;
                return None;
            }
            self.j = [
                (u.x * y.y - v.x * x.y) / det,
                (-u.x * y.x + v.x * x.x) / det,
                (u.y * y.y - v.y * x.y) / det,
                (-u.y * y.x + v.y * x.x) / det,
            ];
            self.initial_j = self.j;
            self.stage = 5;
        } else if self.stage == 5 {
            let delta = self.virtual_point(touch, center) - self.adapt_touch;
            let change = observed - self.adapt_target;
            let size = delta.dot(delta);
            let resolvable = max(2., radius * 0.02);
            if self.rebase != 0 || self.adapt_anchor_valid == 0 || !adapt_jacobian {
                self.rebase = 0;
                self.adapt_touch = self.virtual_point(touch, center);
                self.adapt_target = observed;
                self.adapt_anchor_valid = 1;
            } else if size >= resolvable * resolvable {
                let predicted = Point::new(
                    self.j[0] * delta.x + self.j[1] * delta.y,
                    self.j[2] * delta.x + self.j[3] * delta.y,
                );
                let residual = change - predicted;
                if self.warm != 0
                    && predicted.length() > 1e-4
                    && (predicted.dot(change) < 0.
                        || residual.length() > max(predicted.length(), change.length()) * 0.85)
                {
                    self.reset();
                    return Some(center);
                }
                if predicted.length() > 1e-4
                    && predicted.dot(change) > predicted.dot(predicted) * 0.1
                {
                    let mut candidate = self.j;
                    let damping = 0.5;
                    candidate[0] += damping * residual.x * delta.x / size;
                    candidate[1] += damping * residual.x * delta.y / size;
                    candidate[2] += damping * residual.y * delta.x / size;
                    candidate[3] += damping * residual.y * delta.y / size;
                    let initial_det = self.initial_j[0] * self.initial_j[3]
                        - self.initial_j[1] * self.initial_j[2];
                    let candidate_det = candidate[0] * candidate[3] - candidate[1] * candidate[2];
                    let mut initial_norm = 0.;
                    let mut candidate_norm = 0.;
                    for i in 0..4 {
                        initial_norm += self.initial_j[i] * self.initial_j[i];
                        candidate_norm += candidate[i] * candidate[i];
                    }
                    let h00 = (self.initial_j[3] * candidate[0] - self.initial_j[1] * candidate[2])
                        / initial_det;
                    let h01 = (self.initial_j[3] * candidate[1] - self.initial_j[1] * candidate[3])
                        / initial_det;
                    let h10 = (-self.initial_j[2] * candidate[0]
                        + self.initial_j[0] * candidate[2])
                        / initial_det;
                    let h11 = (-self.initial_j[2] * candidate[1]
                        + self.initial_j[0] * candidate[3])
                        / initial_det;
                    let rotation = (h10 - h01).atan2(h00 + h11);
                    if candidate_norm.is_finite()
                        && candidate_det * initial_det > 0.
                        && rotation.abs() <= 0.2617993877991494
                        && h00 > 0.
                        && h11 > 0.
                        && (self.centre_sample == 0
                            || max(h01.abs(), h10.abs()) <= min(h00, h11) * 0.2679491924311227)
                        && candidate_norm >= initial_norm * 0.09
                        && candidate_norm <= initial_norm * 9.
                        && candidate_det.abs() >= initial_det.abs() * 0.1
                        && candidate_det.abs() / candidate_norm
                            >= initial_det.abs() / initial_norm * 0.2
                    {
                        self.j = candidate;
                    }
                }
                self.adapt_touch = self.virtual_point(touch, center);
                self.adapt_target = observed;
            }
        }
        let current = self.virtual_point(touch, center);
        self.last_touch = current;
        self.last_target = observed;
        self.anchor_valid = 1;
        if self.adapt_anchor_valid == 0 {
            self.adapt_touch = current;
            self.adapt_target = observed;
            self.adapt_anchor_valid = 1;
        }
        let mut det = self.j[0] * self.j[3] - self.j[1] * self.j[2];
        let initial_det =
            self.initial_j[0] * self.initial_j[3] - self.initial_j[1] * self.initial_j[2];
        if !det.is_finite() || det.abs() < initial_det.abs() * 0.02 {
            self.j = self.initial_j;
            det = initial_det;
        }
        if !det.is_finite() || det.abs() < 1e-8 {
            self.failed = 1;
            return None;
        }
        let error = desired - observed;
        let mut correction = Point::new(
            (self.j[3] * error.x - self.j[1] * error.y) / det,
            (-self.j[2] * error.x + self.j[0] * error.y) / det,
        );
        let correction_length = correction.length();
        if !correction_length.is_finite() {
            self.failed = 1;
            return None;
        }
        if correction_length > radius * 0.2 {
            correction = correction * (radius * 0.2 / correction_length);
        }
        let mut offset = current + correction * 0.85;
        let distance = offset.length();
        if distance > 1e-9 {
            offset = offset * (1. + self.deadzone / distance);
        }
        Some(bounded(center + offset, center, radius))
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct AngularRadialSolver {
    pub ready: u8,
    pub center: Point,
    pub resolution: Point,
    pub radius: f64,
    pub world_range: f64,
    pub a: [f64; 4],
}
impl AngularRadialSolver {
    pub fn reset(&mut self) {
        self.ready = 0;
    }
    pub fn configure(
        &mut self,
        center: Point,
        radius: f64,
        a: [f64; 4],
        range: f64,
        resolution: Point,
    ) -> bool {
        self.reset();
        if !center.finite()
            || !resolution.finite()
            || resolution.x < 32.
            || resolution.y < 32.
            || !radius.is_finite()
            || radius < 4.
            || !range.is_finite()
            || range < 1e-4
            || range > 1e6
            || a.iter().any(|x| !x.is_finite())
        {
            return false;
        }
        let det = a[0] * a[3] - a[1] * a[2];
        if !det.is_finite() || det.abs() < 1e-5 {
            return false;
        }
        self.center = center;
        self.radius = radius;
        self.a = a;
        self.world_range = range;
        self.resolution = resolution;
        self.ready = 1;
        true
    }
    pub fn clamped_world(&self, desired: Point) -> Point {
        let r = desired.length();
        if r > self.world_range && r > 0. {
            desired * (self.world_range / r)
        } else {
            desired
        }
    }
    pub fn predict(&self, desired: Point) -> Option<Point> {
        if self.ready == 0 || !desired.finite() {
            return None;
        }
        let distance = desired.length();
        let mut unity = self.center;
        if distance > 1e-10 {
            let det = self.a[0] * self.a[3] - self.a[1] * self.a[2];
            let v = Point::new(
                (self.a[3] * desired.x - self.a[1] * desired.y) / det,
                (-self.a[2] * desired.x + self.a[0] * desired.y) / det,
            );
            let length = v.length();
            if !length.is_finite() || length < 1e-10 {
                return None;
            }
            unity = unity + v * (self.radius * min(1., distance / self.world_range) / length);
        }
        Some(Point::new(
            unity.x * self.resolution.x / (self.resolution.x - 1.),
            unity.y * self.resolution.y / (self.resolution.y - 1.),
        ))
    }
    pub fn forward(&self, command: Point) -> Option<Point> {
        if self.ready == 0 || !command.finite() {
            return None;
        }
        let unity = Point::new(
            command.x * (self.resolution.x - 1.) / self.resolution.x,
            command.y * (self.resolution.y - 1.) / self.resolution.y,
        );
        let delta = unity - self.center;
        let radius = delta.length();
        if radius < 1e-10 {
            return Some(Point::default());
        }
        let v = Point::new(
            self.a[0] * delta.x + self.a[1] * delta.y,
            self.a[2] * delta.x + self.a[3] * delta.y,
        );
        let length = v.length();
        if !length.is_finite() || length < 1e-10 {
            return None;
        }
        Some(v * (self.world_range * min(1., radius / self.radius) / length))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recorded_native_angular_observations() {
        let mut solver = AngularRadialSolver::default();
        assert!(solver.configure(
            Point::new(3074.5809678885944, 2716.9705373318106),
            369.71659298914706,
            [
                0.7077527046203613,
                -0.48100554943084717,
                -0.7064598202705383,
                -0.4809567332267761
            ],
            6.19921875,
            Point::new(4000., 3111.)
        ));
        for [x, y, wx, wz] in [
            [
                3076.00020122218,
                2717.00030584305,
                0.0178249813616276,
                -0.00110539270099252,
            ],
            [
                3104.00012112403,
                2717.00030584305,
                0.346791595220566,
                -0.332561403512955,
            ],
            [
                2901.00015238185,
                2692.00032648902,
                -1.87117874622345,
                2.28656220436096,
            ],
            [
                3027.00015824269,
                2449.00034156137,
                2.30239462852478,
                3.95770049095154,
            ],
        ] {
            let world = solver.forward(Point::new(x, y)).unwrap();
            assert!((world - Point::new(wx, wz)).length() < 0.00002);
            assert!((solver.predict(world).unwrap() - Point::new(x, y)).length() < 0.000001);
        }
        assert!(!solver.configure(
            Point::default(),
            100.,
            [1., 1., 1., 1.],
            6.,
            Point::new(1000., 1000.)
        ));
        assert!(solver.predict(Point::default()).is_none());
    }
    #[test]
    fn measures_large_deadzone_and_rejects_unpaired_anchor() {
        for fraction in [0.3, 0.45] {
            let mut solver = AimSolver::default();
            let center = Point::new(1000., 600.);
            let radius = 300.;
            let response = |p: Point| {
                let delta = p - center;
                let n = delta.length();
                let v = if n > radius * fraction {
                    delta * ((n - radius * fraction) / n)
                } else {
                    Point::default()
                };
                Point::new(23., -17.)
                    + Point::new(0.02 * v.x + 0.008 * v.y, -0.006 * v.x + 0.017 * v.y)
            };
            let desired = Point::new(24., -16.5);
            let mut touch = center;
            for _ in 0..60 {
                touch = solver
                    .step(touch, response(touch), desired, center, radius, true, true)
                    .unwrap();
            }
            assert!(solver.learned());
            assert!((response(touch) - desired).length() < 0.0001);
            let anchor = solver.last_target;
            let j = solver.j;
            let prediction = solver.predict(desired, center, radius);
            assert_eq!(
                solver.step(
                    center + Point::new(100., 0.),
                    Point::new(-90., 50.),
                    desired,
                    center,
                    radius,
                    false,
                    true
                ),
                prediction
            );
            assert_eq!(solver.last_target, anchor);
            assert_eq!(solver.j, j);
        }
    }
}
