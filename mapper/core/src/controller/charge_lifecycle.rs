//! Native charge gestures can finish while their physical key is still held.
//! Retire only a witnessed form lifecycle; UI absence alone is never an ACK.
use super::*;

#[derive(Clone, Debug)]
pub(crate) struct ChargeGesture {
    epoch: String,
    hero: i32,
    slot: String,
    pub(crate) base: i32,
    pub(crate) future: i32,
    // 1 armed, 2 owned base, 3 owned future, 4 returned base,
    // 5 first empty game tick, 6 confirmed native gesture end.
    pub(crate) stage: u8,
    pub(crate) end_clock: Option<u32>,
    pub(crate) end_sample_ns: i64,
    pub(crate) last_sample_ns: i64,
    cancel_after_end_candidate: bool,
}
fn game_clock(value: snapshot::Number) -> Option<u32> {
    (value.is_number() && value.value.is_finite() && value.value.fract() == 0.
        && (0. ..=u32::MAX as f64).contains(&value.value)).then_some(value.value as u32)
}
impl Controller {
    fn fresh_charge_variant(&self, button: &Button, minimum: i64) -> Option<&SkillVariant> {
        if button.kind != "skill" || self.skillVariantSampleNs_ <= 0
            || self.skillVariantSampleStartNs_ < minimum
            || self.skillVariantSampleNs_ < self.skillVariantSampleStartNs_
            || self.now_ - self.skillVariantSampleNs_ > 100_000_000
        { return None; }
        self.skillVariants_.iter().find(|v| v.slot == button.slot && v.base == button.skillId)
    }
    pub(crate) fn arm_charge_gesture(&mut self, button: &Button) {
        self.chargeGesture_ = self.fresh_charge_variant(button, 0)
            .filter(|v| v.charge_skill == Some(true) && v.active == button.skillId)
            .filter(|_| self.coreSkill_ == 0 && self.frameActiveSlot_.is_empty() && !self.frameEpoch_.is_empty())
            .map(|_| ChargeGesture {
                epoch: self.frameEpoch_.clone(), hero: self.heroId_, slot: button.slot.clone(),
                base: button.skillId, future: 0, stage: 1, end_clock: None,
                end_sample_ns: 0, last_sample_ns: 0, cancel_after_end_candidate: false,
            });
    }
    pub(crate) fn charge_cancel_requested(&mut self) {
        if let Some(proof) = self.chargeGesture_.as_mut() {
            // Only an end already observed before T may later be classified as
            // native completion. An ordinary T cancellation has its own ACK.
            proof.cancel_after_end_candidate = proof.stage == 5
                && proof.end_sample_ns > 0 && proof.end_sample_ns <= self.now_;
        }
    }
    fn trace_charge_revoked(&mut self, host: &mut impl Host, proof: &ChargeGesture, reason: &str) {
        // Record the rejected proof before dropping it; derived View fields
        // would otherwise already be empty because observation takes ownership.
        self.chargeGesture_ = Some(proof.clone());
        self.trace(host, "charge_tracking_revoked", reason);
        self.chargeGesture_ = None;
    }
    pub(crate) fn observe_charge_gesture(&mut self, host: &mut impl Host, frame: &snapshot::Frame) {
        let Some(mut proof) = self.chargeGesture_.take() else { return };
        // The source worker can publish a pre-DOWN sample just after the
        // transport returns. It is not evidence about this gesture yet.
        if self.frameCoreSampleStartNs_ < self.castStartedNs_ {
            self.chargeGesture_ = Some(proof);
            return;
        }
        let Some(button) = self.find_button(&proof.slot) else { return };
        let Some(variant) = self.fresh_charge_variant(button, self.castStartedNs_) else {
            self.trace_charge_revoked(host, &proof, "missing_current_charge_or_variant");
            return;
        };
        if self.skillTouch_ < 0 || self.heroId_ != proof.hero || self.frameEpoch_ != proof.epoch
            || self.activeSlot_ != proof.slot || button.skillId != proof.base
            || frame.token.as_str() != proof.epoch || frame.hero_id.int() != proof.hero
            || !frame.token_string.get() || !frame.token_nonempty.get()
            || variant.charge_skill != Some(true) || variant.active <= 0
            || self.frameCoreSampleStartNs_ < self.castStartedNs_
            || !frame.time_ns.is_number() || !frame.time_ns.value.is_finite()
            || !frame.skill_id.is_number()
        {
            self.trace_charge_revoked(host, &proof, "charge_owner_or_policy_changed");
            return;
        }
        let form = variant.active;
        let Some(clock) = game_clock(frame.game_clock_ms) else {
            self.trace_charge_revoked(host, &proof, "native_game_clock_unavailable");
            return;
        };
        let end = frame.time_ns.value as i64;
        if end <= proof.last_sample_ns {
            if end == proof.last_sample_ns { self.chargeGesture_ = Some(proof); }
            else { self.trace_charge_revoked(host, &proof, "source_sample_order_reversed"); }
            return;
        }
        proof.last_sample_ns = end;
        let g = &frame.ui.joystick;
        let owned = self.frameActiveSlot_ == proof.slot
            && g.ready.get() && g.source.as_str() == "active_widget_transform"
            && g.slot.as_str() == proof.slot && g.skill_id.int() == proof.base
            && g.sample_start_ns.is_number()
            && g.sample_start_ns.value >= self.castStartedNs_ as f64
            && (self.coreSkill_ == proof.base || self.coreSkill_ == form);
        if owned {
            if proof.stage == 5 {
                self.trace_charge_revoked(host, &proof, "ui_reclaimed_after_end_candidate");
                return;
            }
            if form == proof.base {
                proof.stage = if proof.future == 0 { 2 } else { 4 };
            } else if proof.future == 0 {
                proof.future = form;
                proof.stage = 3;
                self.chargeGesture_ = Some(proof.clone());
                self.trace(host, "charge_transition", "owned_future_form_observed");
            } else if proof.future != form || proof.stage >= 4 {
                self.trace_charge_revoked(host, &proof, "charge_future_form_changed");
                return;
            }
            self.chargeGesture_ = Some(proof);
            return;
        }
        if self.coreSkill_ != 0 || !self.frameActiveSlot_.is_empty() {
            self.trace_charge_revoked(host, &proof, "another_or_unverified_ui_owner");
            return;
        }
        if proof.future == 0 {
            // Neither an unaccepted DOWN nor a temporarily hidden first preview
            // proves that the game completed a charge lifecycle.
            self.chargeGesture_ = Some(proof);
            return;
        }
        if form != proof.base {
            if form != proof.future || proof.stage >= 4 {
                self.trace_charge_revoked(host, &proof, "charge_form_not_returned");
                return;
            }
            self.chargeGesture_ = Some(proof);
            return;
        }
        if self.skillCancelPending_ && !proof.cancel_after_end_candidate {
            self.trace_charge_revoked(host, &proof, "cancel_preceded_native_end_evidence");
            return;
        }
        if let Some(first_clock) = proof.end_clock {
            let advance = clock.wrapping_sub(first_clock);
            if advance == 0 {
                self.chargeGesture_ = Some(proof);
                return;
            }
            if advance >= 0x8000_0000 || self.frameCoreSampleStartNs_ < proof.end_sample_ns {
                self.trace_charge_revoked(host, &proof, "game_clock_or_sample_order_changed");
                return;
            }
            proof.stage = 6;
            self.chargeGesture_ = Some(proof);
            self.trace(host, "charge_transition", "native_form_cycle_and_two_empty_game_ticks");
            self.finish_auto_completed_charge(host);
        } else {
            proof.stage = 5;
            proof.end_clock = Some(clock);
            proof.end_sample_ns = end;
            self.chargeGesture_ = Some(proof);
            self.trace(host, "charge_transition", "await_second_empty_native_game_tick");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::host::{Binding, Timer};
    use super::super::snapshot::{Flag, Frame, Number, Text, Variant, VariantState};
    #[derive(Default)]
    struct TestHost { next: i32, edges: Vec<(&'static str, i32)>, traces: Vec<(String, String)>, world: Option<Point> }
    impl Host for TestHost {
        fn begin(&mut self, _: Point) -> i32 { self.next += 1; self.edges.push(("down", self.next)); self.next }
        fn move_touch(&mut self, id: i32, _: Point) -> bool { self.edges.push(("move", id)); true }
        fn end(&mut self, id: i32, _: Point) { self.edges.push(("up", id)); }
        fn pointer_world(&mut self, _: Point) -> Option<Point> { self.world }
        fn ground_world(&mut self, _: Point) -> Option<Point> { self.world }
        fn movement_direction(&mut self, _: Point) -> Option<Point> { None }
        fn hero_world(&mut self) -> Option<Point> { None }
        fn pointer_world_at_plane(&mut self, _: Point, _: f64) -> Option<Point> { self.world }
        fn capture_pointer_world(&mut self, _: Point) -> u64 { 0 }
        fn resolve_world(&mut self, _: u64) -> Option<Point> { None }
        fn release_resolver(&mut self, _: u64) {}
        fn navigation(&mut self, _: Point, _: Point, _: bool) -> Result<Vec<Point>, String> { Err("unused".into()) }
        fn binding(&mut self, _: i32, _: &Button, _: &str) -> Binding { Binding { key: 87, aim_type: 2, ..Binding::default() } }
        fn cancel_key(&mut self, _: i32) -> i32 { 84 }
        fn timer(&mut self, _: Timer, _: Option<i64>) {}
        fn trace(&mut self, _: &Controller, event: &str, reason: &str) { self.traces.push((event.into(), reason.into())); }
    }
    fn n(v: f64) -> Number { Number { kind: 1, value: v } }
    fn flag(v: bool) -> Flag { Flag { value: v as i32 } }
    fn text(v: &str) -> Text { let mut s = Text::default(); s.bytes[..v.len()].copy_from_slice(v.as_bytes()); s.size = v.len() as u32; s.present = 1; s }
    fn v2(x: f64, y: f64) -> snapshot::Vector2 { snapshot::Vector2 { count: 2, values: [n(x), n(y)] } }
    fn frame(now: i64, form: i32, owned: bool, charge: Option<bool>, clock: u32) -> Frame {
        let mut f = Frame::default();
        f.ready = flag(true); f.hero_id = n(126.); f.time_ns = n(now as f64); f.sample_duration_ms = n(0.5);
        f.game_clock_ms = n(clock as f64); f.skill_id = n(if owned { 12630. } else { 0. });
        f.token = text("current-charge-match"); f.token_string = flag(true); f.token_nonempty = flag(true);
        f.ui.ready = flag(true); f.ui.hero_id = f.hero_id; f.ui.time_ns = f.time_ns;
        f.ui.resolution_px = v2(1000., 1000.); f.ui.button_count = 1;
        f.ui.buttons[0] = snapshot::Button {
            slot: text("skill:1"), kind: text("skill"), index: n(1.), skill_id: n(12630.), active: flag(true),
            press_uv: v2(0.8, 0.8), center_uv: v2(0.8, 0.8), radius_uv: v2(0.1, 0.1), ..Default::default()
        };
        if owned {
            f.ui.active_slot = text("skill:1");
            f.ui.joystick = snapshot::Geometry {
                ready: flag(true), source: text("active_widget_transform"), slot: text("skill:1"), skill_id: n(12630.),
                center_uv: v2(0.8, 0.8), radius_uv: v2(0.1, 0.1),
                sample_start_ns: n(now as f64 - 500_000.), sample_end_ns: f.time_ns,
            };
        }
        f.skill_state = VariantState {
            ready: flag(true), source: text("ShowSkillData.m_TranID"), token: f.token, hero_id: f.hero_id,
            sample_start_ns: n(now as f64 - 500_000.), sample_end_ns: f.time_ns, count: 1, ..Default::default()
        };
        f.skill_state.buttons[0] = Variant {
            slot: text("skill:1"), base_skill_id: n(12630.), active_skill_id: n(form as f64),
            charge_skill: charge.map(flag).unwrap_or_default(), ..Default::default()
        };
        f
    }
    fn setup(charge: Option<bool>) -> (Controller, TestHost, i32) {
        let mut c = Controller::new(); c.enabled_ = true; c.traceEnabled_ = true;
        let mut h = TestHost { next: 9, ..Default::default() };
        c.apply_snapshot(&mut h, &frame(1_000_000_000, 12630, false, charge, 1000), 1_000_000_000);
        assert!(c.handle_key(&mut h, 87, true, false));
        let touch = c.skillTouch_;
        (c, h, touch)
    }
    fn sample(c: &mut Controller, h: &mut TestHost, elapsed_ms: i64, form: i32, owned: bool, charge: Option<bool>, clock: u32) {
        let at = 1_000_000_000 + elapsed_ms * 1_000_000;
        c.apply_snapshot(h, &frame(at, form, owned, charge, clock), at);
    }
    fn owned_future(c: &mut Controller, h: &mut TestHost) {
        sample(c, h, 10, 12630, true, Some(true), 1001);
        sample(c, h, 20, 12631, true, Some(true), 1002);
        assert_eq!(c.chargeGesture_.as_ref().unwrap().stage, 3);
    }
    fn auto_completed(h: &TestHost) -> bool { h.traces.iter().any(|(e, r)| e == "release" && r == "auto_completed") }
    #[test]
    fn native_charge_form_cycle_and_two_distinct_empty_ticks_retire_only_old_contact() {
        let (mut c, mut h, touch) = setup(Some(true));
        c.movementTouch_ = 1; c.movementHeld_ = true;
        c.manualTouch_ = 2; c.manualPoint_ = c.pointer_;
        c.auxiliaryTaps_.push(AuxiliaryTap { id: 3, until: 5_000_000_000, ..Default::default() });
        c.queuedSkills_.push(QueuedSkill { slot: "skill:1".into(), key: 87, held: true, requestedNs: 1_000_000_000, ..Default::default() });
        owned_future(&mut c, &mut h);
        sample(&mut c, &mut h, 30, 12630, false, Some(true), 1003);
        assert_eq!(c.skillTouch_, touch);
        assert_eq!(c.chargeGesture_.as_ref().unwrap().stage, 5);
        sample(&mut c, &mut h, 40, 12630, false, Some(true), 1003);
        assert_eq!(c.skillTouch_, touch, "new host reads of the same game tick do not prove completion");
        h.edges.clear();
        sample(&mut c, &mut h, 50, 12630, false, Some(true), 1004);
        assert_eq!(c.skillTouch_, -1);
        assert_eq!(h.edges, vec![("up", touch)], "no cancel MOVE, pointer flush, or unrelated contact retirement");
        assert!(auto_completed(&h));
        assert!(!h.traces.iter().any(|(e, r)| e == "release" && (r == "cast" || r == "cancel")));
        assert!(c.cooldownCasts_.is_empty(), "native lifecycle completion is not mapper cast confirmation");
        assert_eq!(c.movementTouch_, 1); assert!(c.movementHeld_);
        assert_eq!(c.manualTouch_, 2); assert_eq!(c.auxiliaryTaps_[0].id, 3);
        assert_eq!(c.queuedSkills_.len(), 1, "unrelated queued intent survives an already-ended predecessor");
        assert!(c.handle_key(&mut h, 87, false, false));
        assert_eq!(h.edges, vec![("up", touch)], "late physical UP cannot release the retired contact twice");
    }
    #[test]
    fn late_cancel_after_first_native_end_tick_is_cleanup_not_cancel_ack() {
        let (mut c, mut h, touch) = setup(Some(true));
        owned_future(&mut c, &mut h);
        sample(&mut c, &mut h, 30, 12630, false, Some(true), 1003);
        c.request_skill_cancel(&mut h);
        assert!(c.skillCancelPending_);
        sample(&mut c, &mut h, 40, 12630, false, Some(true), 1004);
        assert_eq!(c.skillTouch_, -1); assert!(!c.skillCancelPending_);
        assert_eq!(h.edges.iter().filter(|e| **e == ("up", touch)).count(), 1);
        assert!(auto_completed(&h)); assert!(c.cooldownCasts_.is_empty());
    }
    #[test]
    fn non_charge_unknown_or_missing_form_cycle_cannot_auto_complete() {
        for charge in [Some(false), None] {
            let (mut c, mut h, touch) = setup(charge);
            for (ms, form, owned, clock) in [(10,12630,true,1001),(20,12631,true,1002),(30,12630,false,1003),(40,12630,false,1004)] {
                sample(&mut c, &mut h, ms, form, owned, charge, clock);
            }
            assert_eq!(c.skillTouch_, touch); assert!(!auto_completed(&h));
        }
        let (mut c, mut h, touch) = setup(Some(true));
        sample(&mut c, &mut h, 10, 12630, true, Some(true), 1001);
        sample(&mut c, &mut h, 20, 12630, false, Some(true), 1002);
        sample(&mut c, &mut h, 30, 12630, false, Some(true), 1003);
        assert_eq!(c.skillTouch_, touch, "UI loss without an owned future form is insufficient");
        assert!(!auto_completed(&h));
    }
    #[test]
    fn charge_requires_post_down_ui_capture_not_just_a_future_form_and_absence() {
        let (mut c, mut h, touch) = setup(Some(true));
        sample(&mut c, &mut h, 10, 12631, false, Some(true), 1001);
        sample(&mut c, &mut h, 20, 12630, false, Some(true), 1002);
        sample(&mut c, &mut h, 30, 12630, false, Some(true), 1003);
        assert_eq!(c.skillTouch_, touch); assert!(!auto_completed(&h));
    }
    #[test]
    fn native_charge_evidence_is_revoked_by_unknown_policy_rebinding_or_clock_change() {
        for case in 0..9 {
            let (mut c, mut h, touch) = setup(Some(true));
            owned_future(&mut c, &mut h);
            sample(&mut c, &mut h, 30, 12630, false, Some(true), 1003);
            let mut f = frame(1_040_000_000, 12630, false, Some(true), 1004);
            match case {
                0 => f.skill_state.buttons[0].charge_skill = Flag::default(),
                1 => f.skill_state.buttons[0].charge_skill = flag(false),
                2 => f.skill_state.ready = flag(false),
                3 => f.game_clock_ms = Number::default(),
                4 => f.game_clock_ms = n(1002.),
                5 => f = frame(1_040_000_000, 12630, true, Some(true), 1004),
                6 => f.skill_state.buttons[0].active_skill_id = n(12699.),
                7 => { f.token = text("different-match"); f.skill_state.token = f.token; }
                _ => { f.hero_id = n(84.); f.ui.hero_id = f.hero_id; f.skill_state.hero_id = f.hero_id; }
            }
            c.apply_snapshot(&mut h, &f, 1_040_000_000);
            assert!(!auto_completed(&h), "case {case}: uncertain ownership/lifecycle cannot confirm auto completion");
            if case < 7 { assert_eq!(c.skillTouch_, touch); }
            assert!(c.chargeGesture_.is_none(), "case {case}: unknown or rebound proof must not survive for a later false completion");
        }
    }
    #[test]
    fn user_cancel_before_native_end_keeps_ordinary_cancel_ack_path() {
        let (mut c, mut h, touch) = setup(Some(true));
        owned_future(&mut c, &mut h);
        c.request_skill_cancel(&mut h);
        sample(&mut c, &mut h, 30, 12630, false, Some(true), 1003);
        sample(&mut c, &mut h, 40, 12630, false, Some(true), 1004);
        assert_eq!(c.skillTouch_, touch); assert!(c.skillCancelPending_);
        assert!(!auto_completed(&h));
    }
    #[test]
    fn ordinary_physical_up_before_native_auto_end_keeps_the_existing_commit_path() {
        let (mut c, mut h, touch) = setup(Some(true));
        owned_future(&mut c, &mut h);
        h.world = Some(Point::new(1., 0.));
        c.frameCameraReady_ = true; c.frameCamera_ = [1., 0., 0., 1.];
        assert!(c.handle_key(&mut h, 87, false, false));
        assert_eq!(c.skillTouch_, -1);
        assert!(!auto_completed(&h));
        assert!(h.traces.iter().any(|(e, r)| e == "release" && r == "cast"));
        sample(&mut c, &mut h, 30, 12630, false, Some(true), 1003);
        sample(&mut c, &mut h, 40, 12630, false, Some(true), 1004);
        assert_eq!(h.edges.iter().filter(|e| **e == ("up", touch)).count(), 1);
        assert!(c.chargeGesture_.is_none());
    }
    #[test]
    fn duplicate_source_packet_is_not_a_second_game_receipt_and_reversed_packet_revokes() {
        for reversed in [false, true] {
            let (mut c, mut h, touch) = setup(Some(true));
            owned_future(&mut c, &mut h);
            sample(&mut c, &mut h, 30, 12630, false, Some(true), 1003);
            let same_or_old = if reversed { 1_029_000_000 } else { 1_030_000_000 };
            let f = frame(same_or_old, 12630, false, Some(true), 1004);
            c.apply_snapshot(&mut h, &f, 1_040_000_000);
            assert_eq!(c.skillTouch_, touch);
            assert!(!auto_completed(&h));
            if reversed { assert!(c.chargeGesture_.is_none()); }
            else {
                assert_eq!(c.chargeGesture_.as_ref().unwrap().stage, 5);
                sample(&mut c, &mut h, 50, 12630, false, Some(true), 1004);
                assert_eq!(c.skillTouch_, -1); assert!(auto_completed(&h));
            }
        }
    }
    #[test]
    fn source_sample_started_before_down_does_not_revoke_new_charge_proof() {
        let (mut c, mut h, touch) = setup(Some(true));
        let f = frame(1_000_000_000, 12630, false, Some(true), 1000);
        c.apply_snapshot(&mut h, &f, 1_000_200_000);
        assert!(c.chargeGesture_.is_some());
        owned_future(&mut c, &mut h);
        sample(&mut c, &mut h, 30, 12630, false, Some(true), 1003);
        sample(&mut c, &mut h, 40, 12630, false, Some(true), 1004);
        assert_eq!(c.skillTouch_, -1); assert!(auto_completed(&h));
        assert_eq!(h.edges.iter().filter(|e| **e == ("up", touch)).count(), 1);
    }
}
