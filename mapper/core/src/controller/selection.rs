//! Native UI selectors use owned UI feedback, never a world direction.
use super::*;
use snapshot::{Frame, SelectionInput};
#[cfg(test)]
#[path = "target_selection_tests.rs"]
mod target_selection_tests;

const SOURCE: &str = "KeySkillMultiChoice.m_lastDragDir";
const TARGET_SOURCE: &str = "UILockHeroSkill.UICamera.activeTouches";
const FRESH: i64 = 100_000_000;
const COS: f64 = 0.9999904807207345;
#[derive(Clone, Default)]
pub(crate) struct SelectionState {
    pub(crate) kind: u8,
    pub(crate) owner: u64,
    pub(crate) backend_owner: u64,
    pub(crate) contact_owner: u64,
    pub(crate) target_guid: u32,
    pub(crate) target_owner: u64,
    pub(crate) target_self: u64,
    pub(crate) target_invalid: bool,
    active: i32,
    pub(crate) revoked: bool,
    pub(crate) geometry_fresh: bool,
    pub(crate) center: Point,
    pub(crate) radius: Point,
    pub(crate) accepted: bool,
    pub(crate) matched_sequence: u64,
    pub(crate) sample_start_ns: i64,
    pub(crate) sample_end_ns: i64,
    pub(crate) option: i32,
    pub(crate) captured: bool,
    pub(crate) end_clock: Option<u32>,
    pub(crate) end_sample_ns: i64,
    end_active: i32,
    end_core: i32,
    accepted_point: Point,
    pub(crate) frozen_direction: Option<Point>,
    pub(crate) goal_sequence: u64,
    goal_time_ns: i64,
    goal_point: Point,
    goal_center: Point,
    goal_radius: f64,
    last_echo_ns: i64,
    pub(crate) goal_target_guid: u32,
    pub(crate) goal_target_owner: u64,
    pub(crate) goal_target_self: u64,
    target_last_point: Point,
    target_last_valid_ns: i64,
}
fn selection_kind(s: &SelectionInput) -> Option<u8> {
    if integer(s.kind, 1, 2) {
        match (s.kind.int(), s.source.as_str()) {
            (1, SOURCE) => Some(1),
            (2, TARGET_SOURCE) => Some(2),
            _ => None,
        }
    } else {
        None
    }
}
fn target_guid(n: snapshot::Number) -> Option<u32> {
    (n.is_number()
        && n.value.is_finite()
        && n.value.fract() == 0.
        && (0. ..=u32::MAX as f64).contains(&n.value))
    .then_some(n.value as u32)
}
fn integer(n: snapshot::Number, low: i32, high: i32) -> bool {
    n.is_number() && n.value == n.int() as f64 && (low..=high).contains(&n.int())
}
fn same(a: snapshot::Number, b: snapshot::Number) -> bool {
    a.kind == b.kind && a.value == b.value
}
impl Controller {
    pub(crate) fn selection_start_rejection(&self, b: &Button) -> Option<&'static str> {
        if !matches!(b.castMode, 0 | 1) {
            Some("selection_plain_or_center_mode_unproven")
        } else if !self.selection_phase_known(b) {
            Some("selector_phase_unconfirmed")
        } else {
            None
        }
    }
    pub(crate) fn selection_phase_known(&self, b: &Button) -> bool {
        b.kind == "skill"
            && (1..=MAX_SKILL_ID).contains(&b.skillId)
            && self.active_variant(b) == b.skillId
    }
    fn selection_scope(&self, b: &Button, s: &SelectionInput, minimum: i64) -> bool {
        let (start, end) = (s.sample_start_ns.get(-1.), s.sample_end_ns.get(-1.));
        s.ready.get()
            && s.coherent.get()
            && selection_kind(s).is_some()
            && (self.selection_.kind == 0 || selection_kind(s) == Some(self.selection_.kind))
            && s.owner_id != 0
            && s.token.as_str() == self.frameEpoch_
            && integer(s.hero_id, 1, 10000)
            && s.hero_id.int() == self.heroId_
            && s.slot.as_str() == b.slot
            && integer(s.base_skill_id, 1, MAX_SKILL_ID)
            && s.base_skill_id.int() == b.skillId
            && integer(s.active_skill_id, 1, MAX_SKILL_ID)
            && s.active_skill_id.int() == self.active_variant(b)
            && integer(s.native_skill_id, 0, MAX_SKILL_ID)
            && s.native_skill_id.int() == self.coreSkill_
            && (selection_kind(s) == Some(2)
                || s.native_skill_id.int() == 0
                || s.native_skill_id.int() == b.skillId
                || s.native_skill_id.int() == s.active_skill_id.int())
            && s.sample_start_ns.is_number()
            && s.sample_end_ns.is_number()
            && start.is_finite()
            && end.is_finite()
            && start >= minimum as f64
            && end >= start
            && end - start <= FRESH as f64
            && self.now_ as f64 - end <= FRESH as f64
            && end <= self.now_ as f64 + 10_000_000.
    }
    pub(crate) fn arm_selection(&mut self, b: &Button) {
        self.selection_ = Default::default();
        self.selection_.active = self.active_variant(b);
        if self.skillBehavior_ == SkillBehavior::Selection
            && self.selection_scope(b, &self.selectionInput_, 0)
        {
            self.selection_.owner = self.selectionInput_.owner_id;
            self.selection_.kind = selection_kind(&self.selectionInput_).unwrap();
            if self.selection_.kind == 2 {
                self.selection_.backend_owner = self.selectionInput_.backend_owner_id;
            }
        }
    }
    pub(crate) fn sample_selection_input(&mut self, host: &mut impl Host, f: &Frame) {
        self.selectionInput_ = f.selection_input;
        self.selectionHeroSampleNs_ = 0;
        if f.hero_screen_uv.valid()
            && f.time_ns.is_number()
            && f.time_ns.value.is_finite()
            && self.now_ as f64 - f.time_ns.value <= FRESH as f64
            && f.time_ns.value <= self.now_ as f64 + 10_000_000.
        {
            self.selectionHeroScreen_ = Point::new(
                f.hero_screen_uv.values[0].value,
                f.hero_screen_uv.values[1].value,
            );
            self.selectionHeroSampleNs_ = f.time_ns.value as i64;
        }
        self.selection_.geometry_fresh = false;
        self.selection_.accepted = false;
        self.selection_.matched_sequence = 0;
        self.selection_.target_invalid = false;
        if self.skillTouch_ < 0 || self.skillBehavior_ != SkillBehavior::Selection {
            return;
        }
        macro_rules! reject {
            ($r:expr) => {{
                self.trace(host, "selection_feedback_rejected", $r);
                return;
            }};
        }
        let Some(b) = self.find_button(&self.activeSlot_) else {
            return;
        };
        let s = &f.selection_input;
        if !f.token_string.get()
            || !f.token_nonempty.get()
            || !self.selection_scope(b, s, self.castStartedNs_)
        {
            self.selection_.end_clock = None;
            self.selection_.end_sample_ns = 0;
            reject!("owner_receipt_unavailable");
        }
        if self.selection_.owner != 0 && self.selection_.owner != s.owner_id {
            self.selection_.revoked = true;
            reject!("choice_data_owner_rebound");
        }
        let kind = selection_kind(s).unwrap();
        if kind == 2 {
            if s.backend_owner_id == 0 {
                reject!("target_backend_owner_unavailable");
            }
            if self.selection_.backend_owner != 0
                && self.selection_.backend_owner != s.backend_owner_id
            {
                self.selection_.revoked = true;
                reject!("target_backend_owner_rebound");
            }
            self.selection_.backend_owner = s.backend_owner_id;
        }
        self.selection_.kind = kind;
        if self.observe_selection_end(host, f) {
            return;
        }
        let b = self.find_button(&self.activeSlot_).unwrap();
        if self.selection_.active != 0 && self.selection_.active != s.active_skill_id.int() {
            if s.active_skill_id.int() != b.skillId && matches!(b.aimType, 1..=3) {
                // A verified follow-up form can publish before the previous
                // pressed slot clears. It authorizes no further selection
                // input; wait for the independent native-end proof instead.
                reject!("verified_future_phase_await_native_end");
            }
            self.selection_.revoked = true;
            reject!("choice_active_form_changed");
        }
        if self.selection_.revoked {
            reject!("choice_owner_revoked");
        }
        if kind == 2 {
            self.sample_target_input(host, f);
            return;
        }
        if f.ui.active_slot.as_str() != self.activeSlot_ {
            reject!("choice_not_captured");
        }
        let g = &f.ui.joystick;
        if !g.ready.get()
            || g.source.as_str() != "active_widget_transform"
            || g.slot.as_str() != self.activeSlot_
            || !integer(g.skill_id, 1, MAX_SKILL_ID)
            || g.skill_id.int() != b.skillId
            || !same(g.sample_start_ns, s.sample_start_ns)
            || !same(g.sample_end_ns, s.sample_end_ns)
            || !g.center_uv.valid()
            || !g.radius_uv.valid()
            || !f.hero_screen_uv.valid()
        {
            reject!("choice_geometry_unavailable");
        }
        let center = Point::new(g.center_uv.values[0].value, g.center_uv.values[1].value);
        let radius = Point::new(g.radius_uv.values[0].value, g.radius_uv.values[1].value);
        if !finite(center)
            || !finite(radius)
            || !(0. ..=1.).contains(&center.x)
            || !(0. ..=1.).contains(&center.y)
            || radius.x <= 0.
            || radius.y <= 0.
            || radius.x > 1.
            || radius.y > 1.
        {
            reject!("choice_geometry_invalid");
        }
        self.selection_.owner = s.owner_id;
        self.selection_.active = s.active_skill_id.int();
        self.selection_.center = center;
        self.selection_.radius = radius;
        self.selection_.geometry_fresh = true;
        if s.panel_visible.is_bool() && s.panel_visible.get() {
            self.selection_.captured = true;
        }
        if self.skillCancelPending_ {
            return;
        }
        if !s.input_ready.get()
            || !s.panel_visible.get()
            || !s.has_drag.get()
            || !s.selected_option_valid.get()
            || !integer(s.hover_index, 0, 127)
            || !integer(s.selected_option, 1, 2)
            || s.native_skill_id.int() <= 0
        {
            reject!("choice_hover_not_ready");
        }
        let raw = &s.raw_touch_xyz;
        let v = &s.drag_direction_xy;
        if !raw.valid()
            || !v.valid()
            || raw
                .values
                .iter()
                .chain(v.values.iter())
                .any(|n| !n.value.is_finite())
            || raw.values[2].value.abs() > 1.
        {
            reject!("choice_native_vector_invalid");
        }
        let observed = Point::new(v.values[0].value, v.values[1].value);
        if (observed.length() - 1.).abs() > 0.01 {
            reject!("choice_ui_direction_not_unit");
        }
        // Raw feedback uses Android's inclusive device domain. Decode only for
        // causal command matching; UI bearing is measured in native UI pixels.
        let actual = Point::new(
            raw.values[0].value * self.androidSize_.x / (self.androidSize_.x - 1.),
            (self.androidSize_.y - raw.values[1].value) * self.androidSize_.y
                / (self.androidSize_.y - 1.),
        );
        let delta = Point::new(
            raw.values[0].value - center.x * self.androidSize_.x,
            raw.values[1].value - (1. - center.y) * self.androidSize_.y,
        );
        let length = delta.length();
        let r = self.radius_pixels(radius);
        if !finite(actual)
            || !length.is_finite()
            || length < 1e-6
            || length > r * 2. + 4.
            || (observed / observed.length()).dot(delta / length) < COS
        {
            reject!("choice_ui_direction_raw_mismatch");
        }
        let Some(sent) = self
            .causal_
            .match_direction(actual, s.sample_start_ns.value as i64)
        else {
            reject!("choice_command_not_matched");
        };
        self.selection_.accepted = true;
        self.selection_.matched_sequence = sent.sequence;
        self.selection_.sample_start_ns = s.sample_start_ns.value as i64;
        self.selection_.sample_end_ns = s.sample_end_ns.value as i64;
        self.selection_.accepted_point = actual;
        self.selection_.option = s.selected_option.int();
        self.skillActivationObserved_ = true;
        self.trace(
            host,
            "selection_feedback",
            "owned_ui_hover_and_exact_native_touch",
        );
    }
    fn target_receipt(&self, s: &SelectionInput) -> Option<(Point, u64, i64)> {
        if !s.input_ready.get()
            || !s.panel_visible.get()
            || !s.has_drag.get()
            || !s.contact_present.get()
            || s.contact_owner_id == 0
            || !integer(s.native_contact_id, i32::MIN, i32::MAX)
            || !s.raw_touch_xyz.valid()
            || s.raw_touch_xyz.values[2].value.abs() > 1.
        {
            return None;
        }
        let raw = &s.raw_touch_xyz.values;
        let point = Point::new(
            raw[0].value * self.androidSize_.x / (self.androidSize_.x - 1.),
            (self.androidSize_.y - raw[1].value) * self.androidSize_.y / (self.androidSize_.y - 1.),
        );
        let sent = self.causal_.latest()?;
        let start = s.sample_start_ns.get(-1.);
        if !finite(point)
            || sent.time_ns < self.castStartedNs_
            || sent.time_ns as f64 > start
            || start - sent.time_ns as f64 > 500_000_000.
            || distance(point, sent.pixels) >= 0.02
        {
            return None;
        }
        Some((point, sent.sequence, sent.time_ns))
    }
    fn sample_target_input(&mut self, host: &mut impl Host, f: &Frame) {
        let s = &f.selection_input;
        self.selection_.owner = s.owner_id;
        // Contact identity is learned only from this gesture's actual delivered
        // MOVE. Native IDs/list indexes are never compared with a host finger.
        if self.selection_.contact_owner != 0
            && s.contact_present.get()
            && s.contact_owner_id != 0
            && self.selection_.contact_owner != s.contact_owner_id
        {
            self.selection_.revoked = true;
            self.trace(
                host,
                "selection_feedback_rejected",
                "target_contact_owner_rebound",
            );
            return;
        }
        self.selection_.geometry_fresh = s.panel_visible.get()
            && s.contact_present.is_bool()
            && (self.selection_.contact_owner == 0
                || (s.contact_present.get()
                    && s.contact_owner_id == self.selection_.contact_owner));
        if !self.selection_.geometry_fresh {
            self.trace(
                host,
                "selection_feedback_rejected",
                "target_input_plane_unavailable",
            );
            return;
        }
        let Some((point, sequence, _)) = self.target_receipt(s) else {
            self.trace(
                host,
                "selection_feedback_rejected",
                "target_touch_not_exact_current_command",
            );
            return;
        };
        self.selection_.contact_owner = s.contact_owner_id;
        self.selection_.captured = true;
        self.selection_.accepted_point = point;
        self.selection_.matched_sequence = sequence;
        self.selection_.sample_start_ns = s.sample_start_ns.value as i64;
        self.selection_.sample_end_ns = s.sample_end_ns.value as i64;
        self.skillActivationObserved_ = true;
        let valid = s.target_valid.get()
            && target_guid(s.target_guid).is_some_and(|v| v > 0)
            && s.target_owner_id != 0
            && s.target_self_id != 0
            && !s.target_cancel_hit.get();
        self.selection_.target_invalid =
            (s.target_valid.is_bool() && !s.target_valid.get()) || s.target_cancel_hit.get();
        if valid {
            self.selection_.target_guid = target_guid(s.target_guid).unwrap();
            self.selection_.target_owner = s.target_owner_id;
            self.selection_.target_self = s.target_self_id;
            self.selection_.target_last_point = point;
            self.selection_.target_last_valid_ns = s.sample_end_ns.value as i64;
            self.selection_.accepted = true;
            self.trace(
                host,
                "selection_feedback",
                "owned_native_target_and_exact_ui_point",
            );
        } else {
            self.trace(
                host,
                "selection_feedback_rejected",
                if self.selection_.target_invalid {
                    "target_recipient_positively_invalid"
                } else {
                    "target_recipient_unknown"
                },
            );
        }
    }
    fn observe_selection_end(&mut self, host: &mut impl Host, f: &Frame) -> bool {
        let s = &f.selection_input;
        let b = self.find_button(&self.activeSlot_).unwrap();
        let future = s.active_skill_id.int() != b.skillId && matches!(b.aimType, 1..=3);
        let same_phase = s.active_skill_id.int() == self.selection_.active;
        let clock = f.game_clock_ms;
        let clock_valid = clock.is_number()
            && clock.value.is_finite()
            && clock.value.fract() == 0.
            && (0. ..=u32::MAX as f64).contains(&clock.value);
        let ui_empty = if self.selection_.kind == 2 {
            s.backend_owner_id == self.selection_.backend_owner
                && s.contact_present.is_bool()
                && !s.contact_present.get()
                && integer(s.backend_state, 3, 3)
        } else {
            f.ui.active_slot.as_str().is_empty()
                && (self.coreSkill_ == 0 || (future && self.coreSkill_ == s.active_skill_id.int()))
        };
        let empty = self.selection_.captured
            && !self.selection_.revoked
            && self.selection_.owner == s.owner_id
            && s.panel_visible.is_bool()
            && !s.panel_visible.get()
            && s.has_drag.is_bool()
            && !s.has_drag.get()
            && ui_empty
            && (same_phase || future)
            && clock_valid
            && self.frameCoreSampleStartNs_ >= self.castStartedNs_
            && same(s.sample_end_ns, f.time_ns);
        if !empty {
            self.selection_.end_clock = None;
            self.selection_.end_sample_ns = 0;
            return false;
        }
        let clock = clock.value as u32;
        let end_core = if self.selection_.kind == 2 {
            0
        } else {
            self.coreSkill_
        };
        if let Some(first) = self.selection_.end_clock {
            if self.selection_.end_active != s.active_skill_id.int()
                || self.selection_.end_core != end_core
            {
                self.selection_.end_clock = Some(clock);
                self.selection_.end_sample_ns = s.sample_end_ns.value as i64;
                self.selection_.end_active = s.active_skill_id.int();
                self.selection_.end_core = end_core;
                return true;
            }
            let advance = clock.wrapping_sub(first);
            if advance == 0 {
                return true;
            }
            if advance >= 0x8000_0000
                || s.sample_start_ns.value < self.selection_.end_sample_ns as f64
            {
                self.selection_.end_clock = None;
                self.selection_.end_sample_ns = 0;
                return true;
            }
            self.trace(
                host,
                "selection_native_end",
                "owned_panel_capture_and_two_empty_native_ticks",
            );
            self.finish_auto_completed_selection(host);
        } else {
            self.selection_.end_clock = Some(clock);
            self.selection_.end_sample_ns = s.sample_end_ns.value as i64;
            self.selection_.end_active = s.active_skill_id.int();
            self.selection_.end_core = end_core;
            self.trace(
                host,
                "selection_native_end",
                "await_second_empty_native_tick",
            );
        }
        true
    }
    pub(crate) fn freeze_selection_intent(&mut self, host: &mut impl Host) {
        self.pendingRelease_ = true;
        self.pendingPointer_ = self.pointer_;
        self.pendingWorldValid_ = false;
        self.pendingDirectionValid_ = false;
        if self.pendingWorldResolver_ != 0 {
            host.release_resolver(self.pendingWorldResolver_);
        }
        self.pendingWorldResolver_ = 0;
        let d = self.pixels(self.pendingPointer_ - self.selectionHeroScreen_);
        let n = d.length();
        self.selection_.frozen_direction = (self.selectionHeroSampleNs_ > 0
            && self.now_ - self.selectionHeroSampleNs_ <= FRESH
            && finite(d)
            && n > 1e-8)
            .then(|| d / n);
        self.selection_.goal_sequence = 0;
        self.selection_.goal_target_guid = 0;
        self.selection_.goal_target_owner = 0;
        self.selection_.goal_target_self = 0;
        if self.selection_.kind == 2
            && !self.selection_.revoked
            && self.selection_.target_guid > 0
            && self.now_ - self.selection_.target_last_valid_ns <= FRESH
        {
            let point = self.target_point(self.pendingPointer_);
            let cell = Point::new(point.x.round(), point.y.round());
            if distance(cell, self.selection_.target_last_point) < 0.02 {
                self.selection_.goal_target_guid = self.selection_.target_guid;
                self.selection_.goal_target_owner = self.selection_.target_owner;
                self.selection_.goal_target_self = self.selection_.target_self;
            }
        }
        self.releaseRequested_.restart(self.now_);
    }
    pub(crate) fn selection_cancel_source(&self) -> &'static str {
        if self.selection_.kind == 2 {
            "UILockHeroSkill.m_bCancelSkill"
        } else {
            "UIKeySkill.bCancelSkill"
        }
    }
    pub(crate) fn selection_cancel_point(&self) -> Option<Point> {
        if self.skillBehavior_ != SkillBehavior::Selection {
            return self.cancelReady_.then_some(self.cancelPoint_);
        }
        // A cold/unknown selector cannot borrow another active contact's
        // cancellation widget. Choice keeps its established UI cancel path.
        if self.selection_.kind == 1 {
            return self.cancelReady_.then_some(self.cancelPoint_);
        }
        if self.selection_.kind != 2
            || self.selection_.revoked
            || self.selection_.owner == 0
            || self.selection_.backend_owner == 0
        {
            return None;
        }
        let b = self.find_button(&self.activeSlot_)?;
        let s = &self.selectionInput_;
        if !self.selection_scope(b, s, self.castStartedNs_)
            || s.owner_id != self.selection_.owner
            || s.backend_owner_id != self.selection_.backend_owner
            || !s.panel_visible.get()
            || !s.target_cancel_ready.get()
            || !s.target_cancel_uv.valid()
        {
            return None;
        }
        let point = Point::new(
            s.target_cancel_uv.values[0].value,
            s.target_cancel_uv.values[1].value,
        );
        (finite(point) && (0. ..=1.).contains(&point.x) && (0. ..=1.).contains(&point.y))
            .then_some(point)
    }
    pub(crate) fn selection_target_cancel_ack(&self, f: &Frame) -> bool {
        if self.skillBehavior_ != SkillBehavior::Selection
            || self.selection_.kind != 2
            || !self.skillCancelPending_
            || self.skillCancelSentNs_ == 0
            || self.skillTouch_ < 0
        {
            return false;
        }
        self.find_button(&self.activeSlot_)
            .is_some_and(|b| self.selection_cancel_owner(b, f))
    }
    pub(crate) fn send_skill_cancel_point(&mut self, host: &mut impl Host, point: Point) -> bool {
        let before = self.touchSequence_;
        if !self.move_skill(host, point, false) {
            return false;
        }
        // The target-panel ACK names an actual delivered command. Even when
        // already stationary over its cancel widget, issue one fresh MOVE
        // rather than assigning a new request time to an older receipt.
        if self.skillBehavior_ == SkillBehavior::Selection
            && self.selection_.kind == 2
            && before == self.touchSequence_
        {
            if !host.move_touch(self.skillTouch_, self.skillPoint_) {
                return false;
            }
            self.sync_clock(host);
            self.record_skill_touch(self.skillPoint_, false, true);
        }
        true
    }
    pub(crate) fn selection_cancel_owner(&self, b: &Button, f: &Frame) -> bool {
        let owner = self.selection_.owner != 0
            && !self.selection_.revoked
            && self.selection_scope(b, &f.selection_input, self.skillCancelSentNs_)
            && f.selection_input.owner_id == self.selection_.owner
            && (self.selection_.active == 0
                || f.selection_input.active_skill_id.int() == self.selection_.active);
        if !owner {
            return false;
        }
        if self.selection_.kind != 2 {
            return true;
        }
        let s = &f.selection_input;
        s.backend_owner_id == self.selection_.backend_owner
            && self.selection_.contact_owner != 0
            && s.contact_owner_id == self.selection_.contact_owner
            && s.target_cancel_hit.get()
            && s.native_cancel.get()
            && self.target_receipt(s).is_some_and(|(point, _, time)| {
                time >= self.skillCancelSentNs_
                    && distance(
                        point,
                        Point::new(
                            self.pixels(self.skillPoint_).x.floor(),
                            self.pixels(self.skillPoint_).y.floor(),
                        ),
                    ) < 0.02
            })
    }
    fn target_point(&self, uv: Point) -> Point {
        Point::new(
            uv.x * self.androidSize_.x * self.androidSize_.x / (self.androidSize_.x - 1.),
            uv.y * self.androidSize_.y * self.androidSize_.y / (self.androidSize_.y - 1.),
        )
    }
    fn selection_point(&mut self) -> Option<Point> {
        if !self.selection_.geometry_fresh
            || self.selection_.revoked
            || self.now_ as f64 - self.selectionInput_.sample_end_ns.get(-1.) > FRESH as f64
        {
            return None;
        }
        if self.selection_.kind == 2 {
            return Some(
                if self.pendingRelease_ && self.selection_.goal_sequence != 0 {
                    self.selection_.goal_point
                } else {
                    self.target_point(if self.pendingRelease_ {
                        self.pendingPointer_
                    } else {
                        self.pointer_
                    })
                },
            );
        }
        let center = self.pixels(self.selection_.center);
        let radius = self.radius_pixels(self.selection_.radius);
        if radius <= 0. || !radius.is_finite() {
            return None;
        }
        if self.pendingRelease_ && self.selection_.goal_sequence != 0 {
            if distance(center, self.selection_.goal_center) <= 1.
                && (radius - self.selection_.goal_radius).abs()
                    <= self.selection_.goal_radius * 0.03
            {
                return Some(self.selection_.goal_point);
            }
            self.selection_.goal_sequence = 0;
        }
        let direction = if self.pendingRelease_ {
            self.selection_.frozen_direction?
        } else {
            if self.selectionHeroSampleNs_ <= 0 || self.now_ - self.selectionHeroSampleNs_ > FRESH {
                return None;
            }
            let d = self.pixels(self.pointer_ - self.selectionHeroScreen_);
            let n = d.length();
            if !finite(d) || n < 1e-8 {
                return None;
            }
            d / n
        };
        let native = center + direction * radius;
        Some(Point::new(
            native.x * self.androidSize_.x / (self.androidSize_.x - 1.),
            native.y * self.androidSize_.y / (self.androidSize_.y - 1.),
        ))
    }
    pub(crate) fn send_selection(&mut self, host: &mut impl Host) -> bool {
        let Some(point) = self.selection_point() else {
            return true;
        };
        if !self.move_skill(host, self.skill_normalized(point), true) {
            return false;
        }
        if self.pendingRelease_ && self.selection_.goal_sequence == 0 {
            let point = Point::new(
                self.pixels(self.skillPoint_).x.floor(),
                self.pixels(self.skillPoint_).y.floor(),
            );
            let reuse = self
                .causal_
                .latest()
                .filter(|last| {
                    self.selection_.accepted
                        && self.selection_.matched_sequence == last.sequence
                        && distance(last.pixels, point) < 0.02
                        && distance(self.selection_.accepted_point, point) < 0.02
                        && self.now_ - self.selection_.sample_end_ns <= FRESH
                })
                .map(|last| (last.sequence, last.time_ns));
            let (seq, time) = if let Some(v) = reuse {
                v
            } else {
                if !host.move_touch(self.skillTouch_, self.skillPoint_) {
                    return false;
                }
                self.sync_clock(host);
                self.record_skill_touch(self.skillPoint_, false, true);
                (
                    self.touchSequence_,
                    self.causal_.latest().map_or(self.now_, |v| v.time_ns),
                )
            };
            self.selection_.goal_sequence = seq;
            self.selection_.goal_time_ns = time;
            self.selection_.goal_point = point;
            self.selection_.goal_center = self.pixels(self.selection_.center);
            self.selection_.goal_radius = self.radius_pixels(self.selection_.radius);
            self.selection_.last_echo_ns = time;
            self.trace(
                host,
                "selection_goal",
                if reuse.is_some() {
                    "reuse_exact_ui_choice_ack"
                } else {
                    "await_exact_ui_choice_ack"
                },
            );
        }
        true
    }
    pub(crate) fn selection_echo_allowed(&self) -> bool {
        self.selection_.geometry_fresh
            && !self.selection_.revoked
            && self.selection_.owner != 0
            && self.selectionInput_.owner_id == self.selection_.owner
            && self.selectionInput_.ready.get()
            && self.selectionInput_.coherent.get()
            && self.now_ as f64 - self.selectionInput_.sample_end_ns.get(-1.) <= FRESH as f64
    }
    pub(crate) fn selection_transport_failed(&mut self, host: &mut impl Host, reason: &str) {
        // A failed delivery is neither a native cancellation receipt nor proof
        // that a held UI selector disappeared. Keep the contact until the
        // owned cancellation path or an explicit all-context reset retires it.
        if !self.skillCancelPending_ {
            self.request_skill_cancel(host);
        }
        self.trace(host, "selection_transport_failed", reason);
        self.set_status("F7: передача выбора не удалась — ожидаю отмену игры");
    }
    pub(crate) fn tick_selection(&mut self, host: &mut impl Host) {
        self.skillExpectedAim_ = true;
        if !self.send_selection(host) {
            self.selection_transport_failed(host, "selection_move_delivery_failed");
            return;
        }
        let final_receipt = self.pendingRelease_
            && self.selection_.goal_sequence != 0
            && self.selection_.geometry_fresh
            && !self.selection_.revoked
            && self.selection_.sample_start_ns >= self.selection_.goal_time_ns
            && self.selection_.matched_sequence >= self.selection_.goal_sequence
            && self.now_ - self.selection_.sample_end_ns <= FRESH
            && distance(self.selection_.accepted_point, self.selection_.goal_point) < 0.02;
        if self.selection_.kind == 2 && final_receipt {
            let changed = self.selection_.goal_target_guid != 0
                && (self.selection_.goal_target_guid != self.selection_.target_guid
                    || self.selection_.goal_target_owner != self.selection_.target_owner
                    || self.selection_.goal_target_self != self.selection_.target_self);
            if self.selection_.target_invalid || (self.selection_.accepted && changed) {
                self.trace(
                    host,
                    "selection_goal",
                    if changed {
                        "target_identity_changed_after_up"
                    } else {
                        "invalid_target_await_native_cancel"
                    },
                );
                self.request_skill_cancel(host);
                return;
            }
        }
        if self.pendingRelease_
            && self.selection_.goal_sequence != 0
            && self.selection_.accepted
            && self.selection_.geometry_fresh
            && !self.selection_.revoked
            && self.selection_.sample_start_ns >= self.selection_.goal_time_ns
            && self.selection_.matched_sequence >= self.selection_.goal_sequence
            && self.now_ - self.selection_.sample_end_ns <= FRESH
            && distance(self.selection_.accepted_point, self.selection_.goal_point) < 0.02
        {
            self.skillEchoPending_ = false;
            self.trace(host, "selection_goal", "native_ui_hover_ack_release");
            self.finish_skill(host, false);
            return;
        }
        if self.pendingRelease_
            && self.selection_.goal_sequence != 0
            && self.selection_echo_allowed()
            && self.now_ - self.selection_.last_echo_ns >= 32_000_000
        {
            if !host.move_touch(self.skillTouch_, self.skillPoint_) {
                self.selection_transport_failed(host, "selection_keepalive_delivery_failed");
                return;
            }
            self.sync_clock(host);
            self.record_skill_touch(self.skillPoint_, true, false);
            self.selection_.last_echo_ns = self.causal_.latest().map_or(self.now_, |v| v.time_ns);
            self.trace(host, "touch_echo", "selection_goal_keepalive");
        }
    }
}
