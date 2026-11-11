//! Same-cast causal input admission. Coordinates are continuous command pixels;
//! history proves provenance, never substitutes a SENT point for measured input.
use crate::{AimSolver, AngularRadialSolver, Point};

const CAPACITY: usize = 512;
const HISTORY_NS: i64 = 500_000_000;

#[derive(Clone, Copy, Debug, Default)]
pub struct SentTouch {
    pub pixels: Point,
    pub time_ns: i64,
    pub sequence: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Provenance {
    pub sequence: u64,
    pub time_ns: i64,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct InputTrace {
    pub matched_sequence: u64,
    pub sample_start_ns: i64,
    pub sample_end_ns: i64,
    pub raw_top_down: Point,
    pub decoded: Point,
}
#[derive(Clone, Copy, Debug)]
pub struct Admission {
    pub admitted: bool,
    pub adapt: bool,
    pub reason: &'static str,
    pub point: Point,
    pub sample_ns: i64,
    pub trace: InputTrace,
}
impl Default for Admission {
    fn default() -> Self {
        Self {
            admitted: false,
            adapt: false,
            reason: "not_ready",
            point: Point::default(),
            sample_ns: 0,
            trace: InputTrace::default(),
        }
    }
}

/// Shape/type flags preserve Qt JSON rejection distinctions. `source_verified`
/// means exact IndicatorMotionBase, or the already-validated logical adapter.
#[derive(Clone, Copy, Debug)]
pub struct MotionSample {
    pub source_verified: bool,
    pub available: bool,
    pub bracket_stable: bool,
    pub raw_shape_valid: bool,
    pub raw_numbers_valid: bool,
    pub raw: [f64; 3],
    pub timestamps_numeric: bool,
    pub start_ns: f64,
    pub end_ns: f64,
}
impl Default for MotionSample {
    fn default() -> Self {
        Self {
            source_verified: false,
            available: false,
            bracket_stable: false,
            raw_shape_valid: false,
            raw_numbers_valid: false,
            raw: [0.; 3],
            timestamps_numeric: false,
            start_ns: -1.,
            end_ns: -1.,
        }
    }
}

pub struct CausalContext<'a> {
    pub now_ns: i64,
    pub cast_start_ns: i64,
    pub has_touch: bool,
    pub aim_ready: bool,
    pub slots_match: bool,
    pub core_skill_matches: bool,
    pub aim_skill_id: i32,
    pub q_drag_arming: bool,
    pub logical_mode: bool,
    pub logical_fresh: bool,
    pub verified_direct_class: bool,
    pub direct_geometry_fresh: bool,
    pub resolution: Point,
    pub center: Point,
    pub radius: f64,
    pub solver_center: Point,
    pub observed: Point,
    pub hero: Option<Point>,
    pub solver: &'a AimSolver,
    pub direct: &'a AngularRadialSolver,
}

pub struct CausalInput {
    history: [SentTouch; CAPACITY],
    head: usize,
    len: usize,
    sequence: u64,
    trajectory: Option<(SentTouch, SentTouch)>,
    last_packet_ns: i64,
    consumed_sample_ns: i64,
    candidate: Option<(Point, Point, i32)>,
    candidate_since_ns: i64,
    last_accepted_hero: Option<Point>,
    verified_transport: Option<(Point, u64)>,
    last: Admission,
}
impl Default for CausalInput {
    fn default() -> Self {
        Self {
            history: [SentTouch::default(); CAPACITY],
            head: 0,
            len: 0,
            sequence: 0,
            trajectory: None,
            last_packet_ns: 0,
            consumed_sample_ns: 0,
            candidate: None,
            candidate_since_ns: 0,
            last_accepted_hero: None,
            verified_transport: None,
            last: Admission::default(),
        }
    }
}
fn distance(a: Point, b: Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}
fn finite(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}
fn segment_matches(previous: SentTouch, current: SentTouch, actual: Point, start: f64) -> bool {
    if current.time_ns as f64 > start {
        return false;
    }
    let delta = current.pixels - previous.pixels;
    let square = delta.x * delta.x + delta.y * delta.y;
    if square <= 0.01 || current.time_ns <= previous.time_ns {
        return false;
    }
    let offset = actual - current.pixels;
    let alpha = (offset.x * delta.x + offset.y * delta.y) / square;
    (-1.00001..=0.50001).contains(&alpha)
        && (alpha <= 0. || alpha * (current.time_ns - previous.time_ns) as f64 <= 8e6 + 100000.)
        && distance(actual, current.pixels + delta * alpha) < 0.02
}
impl CausalInput {
    /// Controller must also reset its goal/direction sequence references.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn history_len(&self) -> usize {
        self.len
    }
    pub fn last(&self) -> Admission {
        self.last
    }
    pub fn consumed_sample_ns(&self) -> i64 {
        self.consumed_sample_ns
    }
    pub fn mark_consumed(&mut self, sample_ns: i64) {
        self.consumed_sample_ns = sample_ns;
    }
    pub fn history(&self) -> impl DoubleEndedIterator<Item = &SentTouch> {
        (0..self.len).map(|i| &self.history[(self.head + i) % CAPACITY])
    }
    fn at(&self, index: usize) -> SentTouch {
        self.history[(self.head + index) % CAPACITY]
    }
    pub fn latest(&self) -> Option<SentTouch> {
        if self.len == 0 {
            None
        } else {
            Some(self.at(self.len - 1))
        }
    }
    fn pop_front(&mut self) {
        self.head = (self.head + 1) % CAPACITY;
        self.len -= 1;
    }
    /// Returns false only for an unchanged non-echo/non-goal command.
    pub fn record(
        &mut self,
        uv: Point,
        resolution: Point,
        now_ns: i64,
        echo: bool,
        final_goal: bool,
    ) -> bool {
        let px = Point::new(
            (uv.x * resolution.x + 1e-7).floor(),
            (uv.y * resolution.y + 1e-7).floor(),
        );
        let last = self.latest();
        let same = last.is_some_and(|s| distance(s.pixels, px) < 0.05);
        if same && !echo && !final_goal {
            return false;
        }
        if !same || final_goal {
            self.sequence += 1;
        }
        let event = SentTouch {
            pixels: px,
            time_ns: now_ns,
            sequence: self.sequence,
        };
        if !same && let Some(previous) = last {
            self.trajectory = Some((previous, event));
        }
        if self.len == CAPACITY {
            self.pop_front();
        }
        let tail = (self.head + self.len) % CAPACITY;
        self.history[tail] = event;
        self.len += 1;
        while self.len > 1 && self.at(0).time_ns < now_ns - HISTORY_NS {
            self.pop_front();
        }
        true
    }
    /// Direction acknowledges only a preceding, recent physical command or its
    /// bounded Android interpolation/extrapolation. No target/model state mutates.
    pub fn match_direction(&self, actual: Point, start_ns: i64) -> Option<Provenance> {
        self.match_history(actual, start_ns as f64)
    }
    fn match_history(&self, actual: Point, start: f64) -> Option<Provenance> {
        for i in (0..self.len).rev() {
            let current = self.at(i);
            if current.time_ns as f64 > start || start - current.time_ns as f64 > HISTORY_NS as f64
            {
                continue;
            }
            if distance(actual, current.pixels) < 0.02
                || (i > 0 && segment_matches(self.at(i - 1), current, actual, start))
            {
                return Some(Provenance {
                    sequence: current.sequence,
                    time_ns: current.time_ns,
                });
            }
        }
        None
    }
    fn reject(&mut self, reason: &'static str) -> Admission {
        self.last.reason = reason;
        self.last
    }
    pub fn sample(&mut self, c: &CausalContext<'_>, s: &MotionSample) -> Admission {
        self.last = Admission::default();
        if !c.has_touch || !c.aim_ready || !c.slots_match || self.len == 0 || !c.core_skill_matches
        {
            return self.reject("slot_or_skill_unready");
        }
        if c.q_drag_arming {
            return self.reject("await_native_valid_drag");
        }
        if c.logical_mode && !c.logical_fresh {
            return self.reject("logical_not_coherent");
        }
        if c.verified_direct_class && !c.direct_geometry_fresh {
            return self.reject("logical_geometry_not_coherent");
        }
        let (start, end) = (s.start_ns, s.end_ns);
        if start.is_finite() {
            self.last.trace.sample_start_ns = start as i64;
        }
        if end.is_finite() {
            self.last.trace.sample_end_ns = end as i64;
        }
        if !s.source_verified
            || !s.available
            || !s.bracket_stable
            || !s.raw_shape_valid
            || !s.timestamps_numeric
            || !start.is_finite()
            || !end.is_finite()
            || end < start
            || end - start > 100e6
            || start < c.cast_start_ns as f64
            || end > c.now_ns as f64 + 10e6
            || c.now_ns as f64 - end > 250e6
            || end <= self.last_packet_ns as f64
        {
            return self.reject("source_bracket_or_stale");
        }
        if !s.raw_numbers_valid || s.raw.iter().any(|v| !v.is_finite()) {
            return self.reject("invalid_raw");
        }
        let [x, y, z] = s.raw;
        if (x == 0. && y == 0.)
            || x.abs() > c.resolution.x * 2.
            || y.abs() > c.resolution.y * 2.
            || z.abs() > 1.
        {
            return self.reject("zero_or_outside_raw");
        }
        self.last.trace.raw_top_down = Point::new(x, c.resolution.y - y);
        let raw = self.last.trace.raw_top_down;
        let actual = Point::new(
            raw.x * c.resolution.x / (c.resolution.x - 1.),
            raw.y * c.resolution.y / (c.resolution.y - 1.),
        );
        self.last.trace.decoded = actual;
        if c.radius < 4. || distance(actual, c.center) > c.radius * 2. + 4. {
            return self.reject("outside_resampled_joystick");
        }
        let verified = self
            .verified_transport
            .filter(|(point, seq)| *seq == self.sequence && distance(actual, *point) < 0.02);
        let mut matched = verified
            .map(|(_, seq)| seq)
            .or_else(|| self.match_history(actual, start).map(|p| p.sequence));
        if matched.is_none()
            && let Some((previous, current)) = self.trajectory
        {
            if current.sequence == self.sequence
                && segment_matches(previous, current, actual, start)
            {
                matched = Some(current.sequence);
            }
        }
        let Some(matched) = matched else {
            return self.reject("no_resampling_provenance");
        };
        self.last.trace.matched_sequence = matched;
        let latest = matched == self.sequence;
        if c.direct.ready == 0 && !c.solver.learned() && !latest {
            return self.reject("not_latest_cold_probe");
        }
        if latest {
            self.verified_transport = Some((actual, self.sequence));
        }
        let same = self.candidate.is_some_and(|(point, target, skill)| {
            skill == c.aim_skill_id
                && distance(point, actual) < 0.01
                && distance(target, c.observed) <= 0.1
        });
        if !same {
            self.candidate_since_ns = end as i64;
        }
        let confirmed = same && end - self.candidate_since_ns as f64 >= 16e6;
        let consistent = if c.verified_direct_class {
            c.direct
                .forward(actual)
                .is_some_and(|p| distance(p, c.observed) <= 0.03)
        } else {
            c.solver
                .observation_consistent(actual, c.observed, c.solver_center, 1.)
        };
        let hero = c.hero.filter(|p| finite(*p));
        let hero_changed = !c.logical_mode
            && hero
                .zip(self.last_accepted_hero)
                .is_some_and(|(a, b)| distance(a, b) > 0.01);
        let old_target = !c.verified_direct_class
            && c.solver.learned()
            && !consistent
            && !hero_changed
            && self.history().any(|sent| {
                sent.time_ns as f64 <= start
                    && distance(sent.pixels, actual) > 6.
                    && c.solver
                        .observation_consistent(sent.pixels, c.observed, c.solver_center, 2.)
            });
        self.candidate = Some((actual, c.observed, c.aim_skill_id));
        self.last_packet_ns = end as i64;
        let stable_alias = old_target && same && end - self.candidate_since_ns as f64 >= 64e6;
        if (old_target && !stable_alias) || (!old_target && !confirmed && !consistent) {
            return self.reject(if old_target {
                "old_target_new_input"
            } else {
                "await_stable_pair"
            });
        }
        self.last.admitted = true;
        self.last.point = actual;
        self.last.sample_ns = end as i64;
        self.last.adapt = !c.verified_direct_class
            && !stable_alias
            && (!c.solver.learned() || consistent)
            && !hero_changed;
        self.last.reason = if stable_alias {
            "stable_reanchor_after_alias"
        } else if consistent {
            if latest {
                "accepted_latest"
            } else {
                "accepted_history"
            }
        } else {
            "stable_reanchor"
        };
        if let Some(hero) = hero {
            self.last_accepted_hero = Some(hero);
        }
        self.last
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const T: i64 = 1_000_000_000;
    const RES: Point = Point::new(4000., 3111.);
    const CENTER: Point = Point::new(3000., 2500.);
    fn context<'a>(solver: &'a AimSolver, direct: &'a AngularRadialSolver) -> CausalContext<'a> {
        CausalContext {
            now_ns: T + 50_000_000,
            cast_start_ns: T,
            has_touch: true,
            aim_ready: true,
            slots_match: true,
            core_skill_matches: true,
            aim_skill_id: 2511,
            q_drag_arming: false,
            logical_mode: true,
            logical_fresh: true,
            verified_direct_class: false,
            direct_geometry_fresh: true,
            resolution: RES,
            center: CENTER,
            radius: 400.,
            solver_center: CENTER,
            observed: Point::default(),
            hero: None,
            solver,
            direct,
        }
    }
    fn sample(point: Point, ns: i64) -> MotionSample {
        MotionSample {
            source_verified: true,
            available: true,
            bracket_stable: true,
            raw_shape_valid: true,
            raw_numbers_valid: true,
            raw: [
                point.x * (RES.x - 1.) / RES.x,
                RES.y - point.y * (RES.y - 1.) / RES.y,
                0.,
            ],
            timestamps_numeric: true,
            start_ns: ns as f64,
            end_ns: ns as f64,
        }
    }
    fn record(input: &mut CausalInput, point: Point, ns: i64) {
        input.record(
            Point::new(point.x / RES.x, point.y / RES.y),
            RES,
            ns,
            false,
            false,
        );
    }
    fn learned() -> AimSolver {
        AimSolver {
            stage: 5,
            anchor_valid: 1,
            last_touch: Point::default(),
            last_target: Point::default(),
            j: [0.1, 0., 0., 0.1],
            initial_j: [0.1, 0., 0., 0.1],
            ..AimSolver::default()
        }
    }
    fn observed(point: Point) -> Point {
        (point - CENTER) * 0.1
    }
    #[test]
    fn raw_decode_keeps_continuous_measured_point_not_sent_cell() {
        let mut input = CausalInput::default();
        let solver = learned();
        let direct = AngularRadialSolver::default();
        let mut c = context(&solver, &direct);
        record(&mut input, Point::new(3000., 2500.), T + 1_000_000);
        record(&mut input, Point::new(3040., 2520.), T + 5_000_000);
        let actual = Point::new(3054.6328, 2527.3164);
        c.observed = observed(actual);
        let result = input.sample(&c, &sample(actual, T + 10_000_000));
        assert!(result.admitted, "{}", result.reason);
        assert!(result.adapt);
        assert!(distance(result.point, actual) < 1e-9);
        assert_eq!(result.trace.matched_sequence, 2);
        assert_eq!(result.reason, "accepted_latest");
    }
    #[test]
    fn resampling_provenance_is_spatial_temporal_and_same_cast() {
        let mut input = CausalInput::default();
        record(&mut input, Point::new(3000., 2500.), T);
        record(&mut input, Point::new(3040., 2500.), T + 20_000_000);
        assert!(
            input
                .match_direction(Point::new(3056., 2500.), T + 30_000_000)
                .is_some()
        ); // .4 *20ms=8ms
        assert!(
            input
                .match_direction(Point::new(3060., 2500.), T + 30_000_000)
                .is_none()
        ); // .5 *20ms=10ms
        assert!(
            input
                .match_direction(Point::new(3056., 2500.021), T + 30_000_000)
                .is_none()
        );
        assert!(
            input
                .match_direction(Point::new(3040., 2500.), T + 19_000_000)
                .is_none()
        );
        assert!(
            input
                .match_direction(Point::new(3040., 2500.), T + 521_000_000)
                .is_none()
        );
        input.reset();
        assert!(
            input
                .match_direction(Point::new(3040., 2500.), T + 30_000_000)
                .is_none()
        );
    }
    #[test]
    fn cold_probe_requires_latest_and_stable_pair() {
        let mut input = CausalInput::default();
        let solver = AimSolver::default();
        let direct = AngularRadialSolver::default();
        let c = context(&solver, &direct);
        record(&mut input, CENTER, T);
        record(&mut input, CENTER + Point::new(30., 0.), T + 8_000_000);
        assert_eq!(
            input.sample(&c, &sample(CENTER, T + 5_000_000)).reason,
            "not_latest_cold_probe"
        );
        let p = CENTER + Point::new(30., 0.);
        assert_eq!(
            input.sample(&c, &sample(p, T + 10_000_000)).reason,
            "await_stable_pair"
        );
        let result = input.sample(&c, &sample(p, T + 26_000_000));
        assert!(result.admitted);
        assert!(result.adapt);
        assert_eq!(result.reason, "stable_reanchor");
    }
    #[test]
    fn delayed_reversal_does_not_reanchor_or_adapt_old_target() {
        let mut input = CausalInput::default();
        let solver = learned();
        let direct = AngularRadialSolver::default();
        let mut c = context(&solver, &direct);
        let left = CENTER + Point::new(-100., 0.);
        let right = CENTER + Point::new(100., 0.);
        record(&mut input, left, T);
        c.observed = observed(left);
        assert!(input.sample(&c, &sample(left, T + 1_000_000)).admitted);
        record(&mut input, right, T + 2_000_000);
        for ms in [4, 20, 39] {
            let r = input.sample(&c, &sample(right, T + ms * 1_000_000));
            assert!(!r.admitted);
            assert_eq!(r.reason, "old_target_new_input");
        }
        c.observed = observed(right);
        let r = input.sample(&c, &sample(right, T + 45_000_000));
        assert!(r.admitted);
        assert!(r.adapt);
    }
    #[test]
    fn stable_alias_can_reanchor_but_cannot_learn_false_jacobian() {
        let mut input = CausalInput::default();
        let solver = learned();
        let direct = AngularRadialSolver::default();
        let mut c = context(&solver, &direct);
        let left = CENTER + Point::new(-100., 0.);
        let right = CENTER + Point::new(100., 0.);
        record(&mut input, left, T);
        record(&mut input, right, T + 1_000_000);
        c.observed = observed(left);
        assert_eq!(
            input.sample(&c, &sample(right, T + 2_000_000)).reason,
            "old_target_new_input"
        );
        c.now_ns = T + 100_000_000;
        let r = input.sample(&c, &sample(right, T + 66_000_000));
        assert!(r.admitted);
        assert!(!r.adapt);
        assert_eq!(r.reason, "stable_reanchor_after_alias");
    }
    #[test]
    fn visual_hero_translation_disables_j_learning_and_allows_reanchor() {
        let mut input = CausalInput::default();
        let solver = learned();
        let direct = AngularRadialSolver::default();
        let mut c = context(&solver, &direct);
        c.logical_mode = false;
        c.hero = Some(Point::new(10., 20.));
        record(&mut input, CENTER, T);
        assert!(input.sample(&c, &sample(CENTER, T + 1_000_000)).admitted);
        c.hero = Some(Point::new(11., 22.));
        c.observed = Point::new(700., -450.);
        assert_eq!(
            input.sample(&c, &sample(CENTER, T + 2_000_000)).reason,
            "await_stable_pair"
        );
        let r = input.sample(&c, &sample(CENTER, T + 18_000_000));
        assert!(r.admitted);
        assert!(!r.adapt);
        assert_eq!(r.reason, "stable_reanchor");
    }
    #[test]
    fn stationary_provenance_survives_expiry_but_not_new_command_or_reset() {
        let mut input = CausalInput::default();
        let solver = learned();
        let direct = AngularRadialSolver::default();
        let mut c = context(&solver, &direct);
        record(&mut input, CENTER, T);
        record(&mut input, CENTER + Point::new(40., 0.), T + 4_000_000);
        let actual = CENTER + Point::new(54.6328, 0.);
        c.observed = observed(actual);
        assert!(input.sample(&c, &sample(actual, T + 10_000_000)).admitted);
        c.now_ns = T + 2_000_000_000;
        input.record(
            Point::new(3040. / RES.x, 2500. / RES.y),
            RES,
            c.now_ns,
            true,
            false,
        );
        assert_eq!(input.history_len(), 1);
        assert!(input.sample(&c, &sample(actual, c.now_ns)).admitted);
        record(
            &mut input,
            CENTER + Point::new(40., 200.),
            c.now_ns + 1_000_000,
        );
        c.now_ns += 2_000_000;
        assert_eq!(
            input.sample(&c, &sample(actual, c.now_ns)).reason,
            "no_resampling_provenance"
        );
        input.reset();
        assert_eq!(input.sequence(), 0);
        assert_eq!(input.history_len(), 0);
        assert_eq!(input.consumed_sample_ns(), 0);
    }
    #[test]
    fn persistent_trajectory_proves_first_stationary_sample_after_expiry() {
        let mut input = CausalInput::default();
        let solver = learned();
        let direct = AngularRadialSolver::default();
        let mut c = context(&solver, &direct);
        record(&mut input, CENTER, T);
        record(&mut input, CENTER + Point::new(40., 0.), T + 4_000_000);
        c.now_ns = T + 2_000_000_000;
        input.record(
            Point::new(3040. / RES.x, 2500. / RES.y),
            RES,
            c.now_ns,
            true,
            false,
        );
        let actual = CENTER + Point::new(54.6328, 0.);
        c.observed = observed(actual);
        assert!(input.sample(&c, &sample(actual, c.now_ns)).admitted);
    }
    #[test]
    fn ring_bounds_and_echo_goal_sequences() {
        let mut input = CausalInput::default();
        record(&mut input, CENTER, T);
        let seq = input.sequence();
        assert!(!input.record(
            Point::new(CENTER.x / RES.x, CENTER.y / RES.y),
            RES,
            T + 1,
            false,
            false
        ));
        assert!(input.record(
            Point::new(CENTER.x / RES.x, CENTER.y / RES.y),
            RES,
            T + 2,
            true,
            false
        ));
        assert_eq!(input.sequence(), seq);
        input.record(
            Point::new(CENTER.x / RES.x, CENTER.y / RES.y),
            RES,
            T + 3,
            false,
            true,
        );
        assert_eq!(input.sequence(), seq + 1);
        for n in 0..2000 {
            record(
                &mut input,
                CENTER + Point::new(n as f64, 0.),
                T + 4 + n * 1000,
            );
        }
        assert_eq!(input.history_len(), 512);
        assert_eq!(input.history().count(), 512);
        input.mark_consumed(42);
        assert_eq!(input.consumed_sample_ns(), 42);
    }
    #[test]
    fn exact_type_stale_geometry_and_bracket_rejections_remain_distinct() {
        let mut input = CausalInput::default();
        let solver = learned();
        let direct = AngularRadialSolver::default();
        let mut c = context(&solver, &direct);
        record(&mut input, CENTER, T);
        c.logical_fresh = false;
        assert_eq!(
            input.sample(&c, &sample(CENTER, T + 1)).reason,
            "logical_not_coherent"
        );
        c.logical_fresh = true;
        c.verified_direct_class = true;
        c.direct_geometry_fresh = false;
        assert_eq!(
            input.sample(&c, &sample(CENTER, T + 1)).reason,
            "logical_geometry_not_coherent"
        );
        c.verified_direct_class = false;
        let mut s = sample(CENTER, T + 1);
        s.raw_shape_valid = false;
        assert_eq!(input.sample(&c, &s).reason, "source_bracket_or_stale");
        s.raw_shape_valid = true;
        s.raw_numbers_valid = false;
        assert_eq!(input.sample(&c, &s).reason, "invalid_raw");
        s.raw_numbers_valid = true;
        s.raw[0] = f64::NAN;
        assert_eq!(input.sample(&c, &s).reason, "invalid_raw");
        s = sample(CENTER, T - 1);
        assert_eq!(input.sample(&c, &s).reason, "source_bracket_or_stale");
        s = sample(CENTER, T + 1);
        s.bracket_stable = false;
        assert_eq!(input.sample(&c, &s).reason, "source_bracket_or_stale");
    }
    #[test]
    fn verified_direct_class_uses_continuous_actual_forward_without_j_learning() {
        let mut input = CausalInput::default();
        let solver = AimSolver::default();
        let mut direct = AngularRadialSolver::default();
        assert!(direct.configure(CENTER, 400., [1., 0.2, -0.1, 1.], 6.2, RES));
        let mut c = context(&solver, &direct);
        c.verified_direct_class = true;
        let p = Point::new(3100., 2600.);
        record(&mut input, p, T);
        c.observed = direct.forward(p).unwrap();
        let result = input.sample(&c, &sample(p, T + 1_000_000));
        assert!(result.admitted);
        assert!(!result.adapt);
        assert_eq!(result.reason, "accepted_latest");
        assert_eq!(
            input.sample(&c, &sample(p, T + 1_000_000)).reason,
            "source_bracket_or_stale"
        );
    }
    #[test]
    fn offscreen_continuation_requires_exact_native_provenance_and_full_range() {
        let center = Point::new(3073.278801, 2843.123579);
        let mut direct = AngularRadialSolver::default();
        assert!(direct.configure(center, 384.58, [1., 0., 0., 1.], 6.2, RES));
        let solver = AimSolver::default();
        let mut c = context(&solver, &direct);
        c.center = center;
        c.radius = 384.58;
        c.verified_direct_class = true;
        let p = Point::new(3074., 3229.);
        c.observed = direct.forward(p).unwrap();
        let mut input = CausalInput::default();
        record(&mut input, p, T);
        let receipt = sample(p, T + 1_000_000);
        assert!(receipt.raw[1] < 0., "offscreen Android feedback has negative bottom-up Y");
        let admitted = input.sample(&c, &receipt);
        assert!(admitted.admitted);
        assert!((c.observed.length() - 6.2).abs() < 1e-9);
        let mut clipped = CausalInput::default();
        record(&mut clipped, Point::new(p.x, RES.y - 1.), T);
        assert!(!clipped.sample(&c, &receipt).admitted,
                "an offscreen receipt must not acknowledge a clipped earlier command");
    }
}
