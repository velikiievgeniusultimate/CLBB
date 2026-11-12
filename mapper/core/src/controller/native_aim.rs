//! Native logical coordinates, authoritative radial models, and guarded input receipts.
//! Port of the existing controller behavior; no transport or game-memory access.
use super::snapshot::{Frame, Number, Vector2, Vector3};
use super::{
    Button, Controller, DirectRange, DirectStartup, Host, SkillBehavior, SkillVariant, distance,
    finite, MAX_SKILL_ID,
};
use crate::{AngularRadialSolver, Point};
use std::collections::BTreeSet;

const NS100: f64 = 100_000_000.;
const NS250: f64 = 250_000_000.;
const POINT_SOURCE: &str = "IndicatorMotionPos.m_vDragResult";
fn direct_source(source:&str)->bool { matches!(source,Q_SOURCE_A | Q_SOURCE_B | POINT_SOURCE) }
const Q_SOURCE_A: &str = "IndicatorMotionFaTiaoQiu2.m_vDragResult";
const Q_SOURCE_B: &str = "IndicatorMotionRotAndPosRot.m_vDragResult";
const RANGE_SOURCE: &str = "CData_Skill_Element.m_ShowRectPara[1]";
fn known_direction_source(hero:i32,skill:i32,source:&str)->bool {
    source=="IndicatorMotionRot.m_vDragResult" || (hero==84&&skill==8410&&source=="KeySkilleCheckMesh.currentDirect")
}
const ANGLE_COS: f64 = 0.9999904807207345; // cos(0.25 degrees)

fn uv(value: &Vector2, radius: bool) -> Option<Point> {
    if !value.valid() {
        return None;
    }
    let p = Point::new(value.values[0].value, value.values[1].value);
    let lower = if radius { 0.0001 } else { 0. };
    if p.x < lower || p.y < lower || p.x > 1. || p.y > 1. {
        None
    } else {
        Some(p)
    }
}
fn vector3(value: &Vector3) -> bool {
    value.valid() && value.values.iter().all(|n| n.value.abs() < 1e6)
}
fn vector2(value: &Vector2) -> bool {
    value.valid() && value.values.iter().all(|n| n.value.abs() < 1e6)
}
fn same_number(a: Number, b: Number) -> bool {
    a.kind == b.kind && a.value == b.value
}
fn xy(value: &Vector2) -> Point {
    Point::new(value.values[0].value, value.values[1].value)
}
fn xz(value: &Vector3) -> Point {
    Point::new(value.values[0].value, value.values[2].value)
}
fn raw_command(value: &Vector3, resolution: Point) -> Point {
    Point::new(
        value.values[0].value * resolution.x / (resolution.x - 1.),
        (resolution.y - value.values[1].value) * resolution.y / (resolution.y - 1.),
    )
}
fn cell(value: Point) -> Point {
    Point::new(value.x.floor(), value.y.floor())
}

impl Controller {
    pub(crate) fn ingest_skill_variants(&mut self, frame: &Frame) {
        self.skillVariants_.clear();
        self.skillVariantSampleNs_ = 0;
        self.skillVariantSampleStartNs_ = 0;
        let state = &frame.skill_state;
        let first = state.sample_start_ns.get(-1.);
        let last = state.sample_end_ns.get(-1.);
        if self.frameEpoch_.is_empty()
            || !frame.token_string.get()
            || !frame.token_nonempty.get()
            || !state.ready.get()
            || state.source.as_str() != "ShowSkillData.m_TranID"
            || state.hero_id.int() != self.heroId_
            || state.token.as_str() != self.frameEpoch_
            || !state.sample_start_ns.is_number()
            || !state.sample_end_ns.is_number()
            || !first.is_finite()
            || !last.is_finite()
            || first < 0.
            || last < first
            || last - first > NS100
            || last > self.now_ as f64 + 10e6
            || self.now_ as f64 - last > NS100
            || state.count > 32
        {
            return;
        }
        let mut variants = Vec::new();
        let mut seen = BTreeSet::new();
        for entry in &state.buttons[..state.count as usize] {
            let slot = entry.slot.as_str();
            let base = entry.base_skill_id;
            let active = entry.active_skill_id;
            if slot.is_empty()
                || !seen.insert(slot)
                || !base.is_number()
                || !active.is_number()
                || base.value != base.int() as f64
                || active.value != active.int() as f64
                || base.int() <= 0
                || base.int() > MAX_SKILL_ID
                || active.int() < 0
                || active.int() > MAX_SKILL_ID
            {
                return;
            }
            variants.push(SkillVariant {
                slot: slot.into(),
                base: base.int(),
                active: active.int(),
                cast_locked: entry.cast_locked.is_bool().then(|| entry.cast_locked.get()),
                state_forbidden: entry.state_forbidden.is_bool().then(|| entry.state_forbidden.get()),
                charge_skill: entry.charge_skill.is_bool().then(|| entry.charge_skill.get()),
            });
        }
        self.skillVariants_ = variants;
        self.skillVariantSampleNs_ = last as i64;
        self.skillVariantSampleStartNs_ = first as i64;
    }
    pub(crate) fn classify_skill(&self, button: &Button) -> SkillBehavior {
        if Self::tap_only(button) {
            return SkillBehavior::Tap;
        }
        if Self::is_retribution(button) {
            return if matches!(button.castMode, 2 | 3) {
                SkillBehavior::Tap
            } else {
                SkillBehavior::Direction
            };
        }
        match button.aimType {
            1 => return SkillBehavior::Tap,
            2 => return SkillBehavior::Direction,
            3 => return SkillBehavior::Range,
            _ => {},
        }
        if self.heroId_ != 25 || (button.skillId != 2520 && button.skillId != 2530) {
            return SkillBehavior::Range;
        }
        if button.skillId == 2530 {
            return SkillBehavior::Tap;
        }
        if self.skillVariantSampleNs_ == 0 || self.now_ - self.skillVariantSampleNs_ > 100_000_000 {
            return SkillBehavior::AwaitVariant;
        }
        for variant in &self.skillVariants_ {
            if variant.slot == button.slot && variant.base == button.skillId {
                return match variant.active {
                    2520 => SkillBehavior::Direction,
                    2521 => SkillBehavior::Tap,
                    _ => SkillBehavior::AwaitVariant,
                };
            }
        }
        SkillBehavior::AwaitVariant
    }
    /// A held button can switch its future cast/release variant while its
    /// owned aiming indicator still belongs to the base skill. Geometry may
    /// authorize the first MOVE before native unit-direction feedback exists;
    /// sample_direction_input separately requires the full receipt contract.
    pub(crate) fn direction_geometry_skill_id(&self, button: &Button, frame: &Frame) -> i32 {
        if button.aimType != 2 || button.kind != "skill" {
            return self.measured_direction_skill_id(button);
        }
        let actual = frame.skill_id;
        let variant = self.active_variant(button);
        if self.skillTouch_ < 0
            || self.skillBehavior_ != SkillBehavior::Direction
            || self.frameActiveSlot_ != self.activeSlot_
            || frame.ui.active_slot.as_str() != self.activeSlot_
            || button.slot != self.activeSlot_
            || variant <= 0
            || !actual.is_number()
            || actual.value != actual.int() as f64
            || actual.int() <= 0
            || actual.int() != self.coreSkill_
            || (actual.int() != button.skillId && actual.int() != variant)
            || frame.token.as_str() != self.frameEpoch_
            || frame.hero_id.int() != self.heroId_
            || frame.ui.hero_id.int() != self.heroId_
            || self.frameCoreSampleStartNs_ < self.castStartedNs_
        {
            return 0;
        }
        actual.int()
    }
    pub(crate) fn direction_point(&mut self, host: &mut impl Host) -> Option<Point> {
        if !self.frameCameraReady_ {
            return None;
        }
        let (center,radius)=if self.directionGeometryFresh_ {
            (self.directionCenter_,self.directionRadius_)
        } else {
            // A DOWN can race a cooldown reset/buffered cast and never open an
            // indicator. Physical UP must still finish that contact. Use the
            // fresh, owned button layout for the final angle, not a previous
            // cast's indicator; never promote this fallback to a native ACK.
            if !self.pendingRelease_ || self.skillCastMode_!=0 || self.skillKeyHeld_
                || self.lastFrameNs_<=0 || self.now_-self.lastFrameNs_>100_000_000 {
                return None;
            }
            let b=self.find_button(&self.activeSlot_)?;
            if !b.available || self.classify_skill(b)!=SkillBehavior::Direction
                || !finite(b.center) || !finite(b.radius) || b.radius.x<=0. || b.radius.y<=0. {
                return None;
            }
            let geometry=(b.center,b.radius);
            self.trace(host,"direction_goal","final_geometry_from_fresh_button");
            geometry
        };
        if self.directionGeometryFresh_ && self.directionGoalSequence_ != 0 {
            let rotation_change = self
                .frameCamera_
                .iter()
                .zip(self.directionGoalCamera_)
                .fold(0f64, |m, (a, b)| m.max((a - b).abs()));
            if distance(
                self.pixels(self.directionCenter_),
                self.directionGoalCenter_,
            ) > 1.
                || rotation_change > 1e-5
                || (self.radius_pixels(self.directionRadius_) - self.directionGoalRadius_).abs()
                    > self.directionGoalRadius_ * 0.03
            {
                self.directionGoalSequence_ = 0;
            } else if self.pendingRelease_ {
                return Some(self.directionGoalPoint_);
            }
        }
        let direction = if self.pendingRelease_ {
            if !self.pendingDirectionValid_ {
                return None;
            }
            self.pendingDirection_
        } else {
            let world = host.skill_direction_world(self.pointer_,self.coreSkill_)?;
            if !finite(world) {
                return None;
            }
            world - self.frameHero_
        };
        let length = direction.length();
        if length < 1e-8 {
            return None;
        }
        let mut angular = AngularRadialSolver::default();
        if !angular.configure(
            self.pixels(center),
            self.radius_pixels(radius),
            self.frameCamera_,
            1.,
            self.androidSize_,
        ) {
            return None;
        }
        angular.predict(direction / length)
    }
    pub(crate) fn freeze_direction_intent(&mut self, host: &mut impl Host) {
        self.pendingRelease_ = true;
        self.pendingPointer_ = self.pointer_;
        if self.pendingWorldResolver_ != 0 {
            host.release_resolver(self.pendingWorldResolver_);
        }
        self.pendingWorldResolver_ = 0;
        self.pendingDirectionValid_ = false;
        self.pendingWorldValid_ = false;
        let projection_skill=if self.coreSkill_>0 {self.coreSkill_} else {
            self.find_button(&self.activeSlot_).map_or(0,|b|self.measured_direction_skill_id(b))
        };
        if let Some(world) = host.skill_direction_world(self.pointer_,projection_skill).filter(|p| finite(*p)) {
            self.pendingWorld_ = world;
            self.pendingWorldValid_ = true;
        }
        if self.pendingWorldValid_ && self.frameCameraReady_ {
            let direction = self.pendingWorld_ - self.frameHero_;
            let size = direction.length();
            if size > 1e-8 {
                self.pendingDirection_ = direction / size;
                self.pendingDirectionValid_ = true;
            }
        }
        self.directionGoalSequence_ = 0;
        self.releaseRequested_.restart(self.now_);
    }
    pub(crate) fn send_direction(&mut self, host: &mut impl Host, point: Point) -> bool {
        if !self.move_skill(host, self.skill_normalized(point), true) {
            return false;
        }
        if self.pendingRelease_ && self.directionGoalSequence_ == 0 {
            let point = cell(self.pixels(self.skillPoint_));
            // A stationary release can freeze the command already observed
            // during this cast. A redundant MOVE may be coalesced by the game,
            // so do not demand a second receipt for the identical command.
            let observed_goal = self.causal_.latest().and_then(|latest| {
                if !self.pendingDirectionValid_
                    || !self.directionAccepted_
                    || !self.directionGeometryFresh_
                    || self.now_ - self.directionSampleEnd_ > NS100 as i64
                    || self.directionMatchedSequence_ != latest.sequence
                    || distance(latest.pixels, point) >= 0.02
                    || distance(self.directionAcceptedPoint_, point) >= 0.02
                    || self.directionObserved_.dot(self.pendingDirection_) < ANGLE_COS
                {
                    return None;
                }
                self.causal_
                    .match_direction(self.directionAcceptedPoint_, self.directionSampleStart_)
                    .filter(|receipt| receipt.sequence == latest.sequence)
            });
            if let Some(receipt) = observed_goal {
                self.directionGoalSequence_ = receipt.sequence;
                self.directionGoalTime_ = receipt.time_ns;
            } else {
                if !host.move_touch(self.skillTouch_, self.skillPoint_) {
                    return false;
                }
                self.sync_clock(host);
                self.record_skill_touch(self.skillPoint_, false, true);
                self.directionGoalSequence_ = self.touchSequence_;
                self.directionGoalTime_ = self.causal_.latest().map_or(self.now_, |v| v.time_ns);
            }
            self.directionLastEchoNs_ = self.directionGoalTime_;
            self.directionGoalPoint_ = point;
            self.directionGoalCenter_ = self.pixels(self.directionCenter_);
            self.directionGoalRadius_ = self.radius_pixels(self.directionRadius_);
            self.directionGoalCamera_ = self.frameCamera_;
            let reason = if observed_goal.is_some() {
                "reuse_exact_direction_ack"
            } else {
                "await_native_direction_ack"
            };
            self.trace(host, "direction_goal", reason);
        }
        true
    }
    pub(crate) fn direction_acknowledged(&self) -> bool {
        if !self.pendingRelease_
            || !self.pendingDirectionValid_
            || self.directionGoalSequence_ == 0
            || !self.directionAccepted_
            || !self.directionGeometryFresh_
            || self.directionSampleStart_ < self.directionGoalTime_
            || self.directionMatchedSequence_ < self.directionGoalSequence_
            || self.now_ - self.directionSampleEnd_ > 100_000_000
        {
            return false;
        }
        self.directionObserved_.dot(self.pendingDirection_) >= ANGLE_COS
    }
    pub(crate) fn sample_direction_input(&mut self, host: &mut impl Host, frame: &Frame) {
        macro_rules! reject {($reason:expr)=>{{self.trace(host,"direction_feedback_rejected",$reason);return;}}}
        self.directionAccepted_ = false;
        self.directionMatchedSequence_ = 0;
        let expected = self
            .find_button(&self.activeSlot_)
            .map_or(0, |button| self.direction_geometry_skill_id(button, frame));
        if self.skillTouch_ < 0 {return;}
        if !self.directionGeometryFresh_ {reject!("geometry_not_fresh");}
        if expected<=0 {reject!("no_measured_direction_contract");}
        if self.coreSkill_!=expected {reject!("active_skill_mismatch");}
        let input = &frame.direction_input;
        let raw = &input.raw_touch_xyz;
        let world = &input.direction_xz;
        let start = input.sample_start_ns.get(-1.);
        let end = input.sample_end_ns.get(-1.);
        let reject_reason=if !input.ready.get(){Some("native_direction_not_ready")}
            else if !input.coherent.get(){Some("native_direction_incoherent")}
            else if !known_direction_source(self.heroId_,expected,input.source.as_str()){Some("unrecognized_direction_source")}
            else if input.token.as_str()!=self.frameEpoch_{Some("epoch_mismatch")}
            else if input.hero_id.int()!=self.heroId_{Some("hero_mismatch")}
            else if input.slot.as_str()!=self.activeSlot_{Some("slot_mismatch")}
            else if input.skill_id.int()!=expected{Some("native_skill_mismatch")}
            else if !raw.valid()||!world.valid(){Some("native_vector_invalid")}
            else if !input.sample_start_ns.is_number()||!input.sample_end_ns.is_number()||!start.is_finite()||!end.is_finite(){Some("sample_clock_invalid")}
            else if start<self.castStartedNs_ as f64 {Some("sample_before_current_cast")}
            else if end<start||end-start>NS100{Some("sample_window_invalid")}
            else if self.now_ as f64-end>NS100{Some("sample_stale")}
            else if end>self.now_ as f64+10e6{Some("sample_from_future")}
            else {None};
        if let Some(reason)=reject_reason{reject!(reason);}
        let geometry = &frame.active_joystick;
        if !same_number(input.sample_start_ns, geometry.sample_start_ns)
            || !same_number(input.sample_end_ns, geometry.sample_end_ns)
        {
            reject!("geometry_clock_mismatch");
        }
        if raw.values[2].value.abs() > 1.
            || (raw.values[0].value == 0. && raw.values[1].value == 0.)
        {
            reject!("native_touch_zero_or_invalid");
        }
        let actual = raw_command(raw, self.androidSize_);
        if distance(actual, self.pixels(self.directionCenter_))
            > self.radius_pixels(self.directionRadius_) * 2. + 4.
        {
            reject!("native_touch_outside_joystick");
        }
        let Some(provenance) = self.causal_.match_direction(actual, start as i64) else {
            reject!("command_provenance_not_matched");
        };
        let matched = provenance.sequence;
        let observed = xy(world);
        let norm = observed.length();
        if norm < 0.9 || norm > 1.1 {
            reject!("native_direction_not_unit");
        }
        let observed = observed / norm;
        let mut model = AngularRadialSolver::default();
        if !model.configure(
            self.pixels(self.directionCenter_),
            self.radius_pixels(self.directionRadius_),
            self.frameCamera_,
            1.,
            self.androidSize_,
        ) {
            reject!("angular_model_unavailable");
        }
        let Some(predicted) = model.forward(actual) else {
            reject!("angular_prediction_unavailable");
        };
        let predicted_norm = predicted.length();
        if predicted_norm < 1e-4 || (predicted / predicted_norm).dot(observed) < ANGLE_COS {
            reject!("native_direction_model_mismatch");
        }
        self.directionAccepted_ = true;
        self.directionAcceptedPoint_ = actual;
        self.directionObserved_ = observed;
        self.directionMatchedSequence_ = matched;
        self.directionSampleStart_ = start as i64;
        self.directionSampleEnd_ = end as i64;
        self.trace(
            host,
            "direction_observed",
            "coherent_raw_and_unit_direction",
        );
    }
    pub(crate) fn select_logical_coordinates(&mut self, host: &mut impl Host, frame: &Frame) {
        let logical = &frame.logical;
        let source = logical.source.as_str();
        let known = direct_source(source);
        let frame_skill = frame.skill_id.int();
        let same_skill = self.skillTouch_ >= 0
            && self.frameActiveSlot_ == self.activeSlot_
            && frame_skill > 0
            && logical.skill_id.int() == frame_skill
            && (frame_skill == self.aimSkillId_
                || (!self.aimReady_ && self.logicalMode_ && frame_skill == self.observedSkillId_));
        let start = logical.sample_start_ns.get(-1.);
        let end = logical.sample_end_ns.get(-1.);
        let protected_input = known
            && logical.input_ready.get()
            && logical.valid_drag.is_bool()
            && logical.first_drag.is_bool();
        self.logicalInputFresh_ = known
            && same_skill
            && (logical.ready.get() || protected_input)
            && logical.coherent.get()
            && logical.sample_start_ns.is_number()
            && logical.sample_end_ns.is_number()
            && start.is_finite()
            && end.is_finite()
            && end >= start
            && end - start <= NS100
            && self.now_ as f64 - end <= NS250
            && end <= self.now_ as f64 + 10e6
            && start >= self.castStartedNs_ as f64
            && vector3(&logical.origin_render_xyz)
            && vector3(&logical.raw_touch_xyz);
        let drag_ready = (logical.valid_drag.missing()
            || (logical.valid_drag.is_bool() && logical.valid_drag.get()))
            && (logical.first_drag.missing()
                || (logical.first_drag.is_bool() && !logical.first_drag.get()));
        let valid = self.logicalInputFresh_
            && logical.ready.get()
            && drag_ready
            && vector2(&logical.offset_xz)
            && vector3(&logical.target_render_xyz)
            && (logical.target_render_xyz.values[0].value
                - logical.origin_render_xyz.values[0].value
                - logical.offset_xz.values[0].value)
                .abs()
                < 0.001
            && (logical.target_render_xyz.values[2].value
                - logical.origin_render_xyz.values[2].value
                - logical.offset_xz.values[1].value)
                .abs()
                < 0.001;
        let relative = known && same_skill;
        if relative != self.logicalMode_ || (relative && self.logicalSource_ != source) {
            self.trace(
                host,
                "coordinate_reset",
                if relative {
                    "logical_relative"
                } else {
                    "visual_absolute"
                },
            );
            self.logicalMode_ = relative;
            self.logicalSource_ = if relative {
                source.into()
            } else {
                String::new()
            };
            self.logicalSampleNs_ = 0;
            if self.skillTouch_ >= 0 {
                self.solver_.reset();
                self.warmAttempted_ = false;
                self.skillNudged_ = self.provisionalStartup_;
                self.armingDistance_ = 0.;
                self.reset_accepted_input();
                self.record_skill_touch(self.skillPoint_, false, false);
            }
        }
        self.logicalSampleFresh_ = valid;
        if self.logicalInputFresh_ {
            self.logicalOrigin_ = xz(&logical.origin_render_xyz);
        }
        if valid {
            self.logicalObserved_ = xy(&logical.offset_xz);
            self.logicalSampleNs_ = end as i64;
        }
    }
    pub(crate) fn verified_direct_class(&self) -> bool {
        self.logicalMode_
            && direct_source(&self.logicalSource_)
    }
    pub(crate) fn configure_direct(&mut self, host: &mut impl Host, frame: &Frame) {
        self.direct_.reset();
        self.directGeometryFresh_ = false;
        self.directRangeSource_.clear();
        if !self.verified_direct_class()
            || !self.logicalInputFresh_
            || self.frameActiveSlot_ != self.activeSlot_
        {
            return;
        }
        let logical = &frame.logical;
        let geometry = &logical.joystick;
        let (Some(center), Some(radius)) = (
            uv(&geometry.center_uv, false),
            uv(&geometry.radius_uv, true),
        ) else {
            return;
        };
        if !geometry.ready.get()
            || geometry.source.as_str() != "active_widget_transform"
            || geometry.slot.as_str() != self.activeSlot_
            || !geometry.sample_start_ns.is_number()
            || !geometry.sample_end_ns.is_number()
            || !same_number(geometry.sample_start_ns, logical.sample_start_ns)
            || !same_number(geometry.sample_end_ns, logical.sample_end_ns)
        {
            return;
        }
        self.directGeometryFresh_ = true;
        self.joystickCenter_ = center;
        self.joystickRadius_ = radius;
        self.joystickReady_ = true;
        self.joystickMeasured_ = true;
        self.observedSkillId_ = self.aimSkillId_;
        let ratio = logical.drag_ratio.get(-1.);
        let mut range = 0.;
        let mut measured_range = false;
        let mut native_fresh = false;
        let native_q = self.heroId_ == 25
            && self.activeSlot_ == "skill:0"
            && (self.aimSkillId_ == 2510 || self.aimSkillId_ == 2511)
            || (self.heroId_ == 84 && self.aimSkillId_ == 8425
                && self.logicalSource_ == POINT_SOURCE);
        // Newly authorized fixed ranges are witnesses for this coherent
        // sample only. Charge policy can change on the next sample, so a
        // generic native range must never survive as an unqualified cache or
        // provisional startup prediction. Historical fixed Kagura/Ling paths
        // keep their already validated cache behavior.
        if !native_q {
            self.directRanges_.retain(|entry| {
                !(entry.hero == self.heroId_
                    && entry.skill == self.aimSkillId_
                    && entry.slot == self.activeSlot_
                    && entry.source == self.logicalSource_
                    && entry.resolution == self.androidSize_
                    && entry.native)
            });
            self.directStartups_.retain(|entry| entry.slot != self.activeSlot_);
        }
        let cached = self.directRanges_.iter().position(|entry| {
            entry.hero == self.heroId_
                && entry.skill == self.aimSkillId_
                && entry.slot == self.activeSlot_
                && entry.source == self.logicalSource_
                && entry.resolution == self.androidSize_
                && entry.native == native_q
        });
        if native_q {
            if let Some(i) = cached {
                range = self.directRanges_[i].range;
            }
        }
        let witness_fields = !logical.world_range.missing()
            || logical.world_range_source_present.get()
            || !logical.world_range_source.missing();
        if !logical.world_range_source_present.missing()
            && !logical.world_range_source_present.is_bool()
        {
            self.trace(host, "direct_metadata_rejected", "invalid_configuration_range_presence");
            return;
        }
        if native_q || witness_fields {
            if witness_fields {
                if logical.world_range_source.as_str() != RANGE_SOURCE
                    || (!native_q && !logical.world_range_source_present.get())
                    || !logical.world_range.is_number()
                    || !logical.world_range.value.is_finite()
                    || logical.world_range.value < 1e-4
                    || logical.world_range.value > 1e4
                {
                    self.trace(host, "direct_metadata_rejected", "invalid_configuration_range");
                    return;
                }
                range = logical.world_range.value;
                native_fresh = true;
                if native_q {
                    if let Some(i) = cached {
                        self.directRanges_[i].range = range;
                    } else {
                        if self.directRanges_.len() >= 32 {
                            self.directRanges_.remove(0);
                        }
                        self.directRanges_.push(DirectRange {
                            hero: self.heroId_, skill: self.aimSkillId_,
                            slot: self.activeSlot_.clone(), source: self.logicalSource_.clone(),
                            resolution: self.androidSize_, range, native: true,
                        });
                    }
                }
            }
            if range <= 0. {
                self.trace(host, "direct_metadata_rejected", "await_native_configuration_range");
                return;
            }
            self.directRangeSource_ = RANGE_SOURCE.into();
        } else {
            let observed = self.logicalObserved_.length();
            // A protected/first-drag sample may retain an older processed
            // offset. It can bootstrap from a fresh authorized native range,
            // but cannot estimate today's range from yesterday's offset.
            if self.logicalSampleFresh_
                && logical.drag_ratio.is_number()
                && ratio.is_finite()
                && ratio >= 1e-4
                && ratio <= 1.001
                && observed > 1e-6
            {
                let measured = observed / ratio;
                if measured.is_finite() && measured >= 1e-4 && measured <= 1e6 {
                    range = measured;
                    measured_range = true;
                    if let Some(i) = cached {
                        self.directRanges_[i].range = range;
                    } else {
                        if self.directRanges_.len() >= 32 {
                            self.directRanges_.remove(0);
                        }
                        self.directRanges_.push(DirectRange {
                            hero: self.heroId_, skill: self.aimSkillId_,
                            slot: self.activeSlot_.clone(), source: self.logicalSource_.clone(),
                            resolution: self.androidSize_, range, native: false,
                        });
                    }
                }
            }
            self.directRangeSource_ = "offset_over_drag_ratio".into();
        }
        if !frame.view_matrix.valid() || range <= 0. {
            return;
        }
        let view = &frame.view_matrix.values;
        let a = [view[0].value, -view[1].value, view[8].value, -view[9].value];
        if self.direct_.configure(
            self.pixels(self.joystickCenter_),
            self.radius_pixels(self.joystickRadius_),
            a,
            range,
            self.androidSize_,
        ) {
            self.provisionalStartup_ = false;
            self.startupArmed_ = false;
            if self.pendingGoalSequence_ != 0
                && (self.pendingGoalSource_ != self.logicalSource_
                    || self.pendingGoalSkill_ != self.aimSkillId_
                    || distance(self.pendingGoalCenter_, self.pixels(self.joystickCenter_)) > 3.
                    || (self.pendingGoalRadius_ - self.radius_pixels(self.joystickRadius_)).abs()
                        > self.pendingGoalRadius_ * 0.03
                    || (self.pendingGoalRange_ - range).abs() > range * 0.001)
            {
                self.pendingGoalSequence_ = 0;
            }
            let plane = logical.visual_plane_y;
            if let Some(button) = self.find_button(&self.activeSlot_).cloned() {
                if (native_q || !native_fresh)
                    && !self.frameEpoch_.is_empty()
                    && frame.token_nonempty.get()
                    && plane.is_number()
                    && plane.value.is_finite()
                    && plane.value.abs() < 1e6
                {
                    self.directStartups_
                        .retain(|cached| cached.slot != self.activeSlot_);
                    if self.directStartups_.len() >= 32 {
                        self.directStartups_.remove(0);
                    }
                    self.directStartups_.push(DirectStartup {
                        epoch: self.frameEpoch_.clone(),
                        hero: self.heroId_,
                        buttonSkill: button.skillId,
                        activeSkill: self.aimSkillId_,
                        slot: self.activeSlot_.clone(),
                        source: self.logicalSource_.clone(),
                        resolution: self.androidSize_,
                        press: button.press,
                        center: self.joystickCenter_,
                        radius: self.joystickRadius_,
                        range,
                        plane: plane.value,
                    });
                }
            }
            self.trace(
                host,
                "direct_metadata",
                if native_q || native_fresh {
                    if native_fresh {
                        "native_configuration_range"
                    } else {
                        "cached_configuration_range"
                    }
                } else if measured_range {
                    "range_from_drag_ratio"
                } else {
                    "cached_world_range"
                },
            );
        }
    }
    pub(crate) fn update_range_drag_arming(&mut self, host: &mut impl Host, frame: &Frame) {
        if self.skillTouch_ < 0
            || self.skillCancelPending_
            || !self.verified_direct_class()
            || !self.logicalInputFresh_
            || !self.directGeometryFresh_
        {
            return;
        }
        let logical = &frame.logical;
        let skill = logical.skill_id.int();
        if skill != self.aimSkillId_
            || !logical.valid_drag.is_bool()
            || !logical.first_drag.is_bool()
        {
            return;
        }
        let protected_drag = !logical.valid_drag.get() || logical.first_drag.get();
        if self.qDragArming_ {
            if protected_drag
                || !self.logicalSampleFresh_
                || skill != self.qDragArmSkill_
                || self.logicalSource_ != self.qDragArmSource_
                || logical.sample_start_ns.get(0.) < self.qDragArmSentNs_ as f64
                || logical.raw_touch_xyz.count != 3
            {
                return;
            }
            let actual = raw_command(&logical.raw_touch_xyz, self.androidSize_);
            let sent = cell(self.pixels(self.qDragArmPoint_));
            if !finite(actual) || distance(actual, sent) > 1.5 {
                return;
            }
            self.qDragArming_ = false;
            self.qDragMismatchSinceNs_ = 0;
            self.reset_accepted_input();
            self.record_skill_touch(self.skillPoint_, false, false);
            self.trace(
                host,
                "drag_bootstrap",
                if self.qDragRecompute_ {
                    "native_geometry_recompute_receipt"
                } else {
                    "native_valid_drag_receipt"
                },
            );
            self.qDragRecompute_ = false;
            self.request_pointer_flush(host);
            return;
        }
        if !protected_drag {
            if !self.pendingRelease_
                || self.qDragRecomputeUsed_
                || !self.logicalSampleFresh_
                || self.direct_.ready == 0
            {
                self.qDragMismatchSinceNs_ = 0;
                return;
            }
            if logical.raw_touch_xyz.count != 3 {
                return;
            }
            let actual = raw_command(&logical.raw_touch_xyz, self.androidSize_);
            let sent = cell(self.pixels(self.skillPoint_));
            let expected = self.direct_.forward(actual);
            if !finite(actual)
                || distance(actual, sent) > 0.05
                || expected.is_none()
                || distance(expected.unwrap(), self.logicalObserved_) <= 0.03
            {
                self.qDragMismatchSinceNs_ = 0;
                return;
            }
            let sample = logical.sample_end_ns.get(0.) as i64;
            if self.qDragMismatchSinceNs_ == 0
                || distance(self.qDragMismatchCell_, sent) > 0.05
                || distance(self.qDragMismatchOffset_, self.logicalObserved_) > 0.001
            {
                self.qDragMismatchSinceNs_ = sample;
                self.qDragMismatchCell_ = sent;
                self.qDragMismatchOffset_ = self.logicalObserved_;
                return;
            }
            if sample - self.qDragMismatchSinceNs_ < 32_000_000 {
                return;
            }
            let mut pulse = None;
            let mut closest = f64::INFINITY;
            for delta in [
                Point::new(-2., 0.),
                Point::new(2., 0.),
                Point::new(0., -2.),
                Point::new(0., 2.),
            ] {
                let p = sent + delta;
                if p.x < 1.
                    || p.y < 1.
                    || p.x > self.androidSize_.x - 2.
                    || p.y > self.androidSize_.y - 2.
                {
                    continue;
                }
                let d = distance(p, self.pixels(self.joystickCenter_));
                if d < closest {
                    closest = d;
                    pulse = Some(p);
                }
            }
            let Some(pulse) = pulse else { return };
            self.qDragArming_ = true;
            self.qDragRecompute_ = true;
            self.qDragRecomputeUsed_ = true;
            self.pendingGoalSequence_ = 0;
            self.qDragArmSkill_ = skill;
            self.qDragArmSource_ = self.logicalSource_.clone();
            if !self.move_skill(host, self.skill_normalized(pulse), true) {
                self.finish_skill(host, true);
                return;
            }
            self.qDragArmPoint_ = self.skillPoint_;
            self.qDragArmSentNs_ = self.now_;
            self.trace(host, "drag_bootstrap", "await_geometry_recompute_receipt");
            self.request_pointer_flush(host);
            return;
        }
        self.qDragMismatchSinceNs_ = 0;
        let center = self.pixels(self.joystickCenter_);
        let initial = self.pixels(self.skillDownPoint_);
        let radius = self.radius_pixels(self.joystickRadius_) * 0.9;
        if !finite(center) || !finite(initial) || !radius.is_finite() || radius < 4. {
            return;
        }
        let mut chosen = Point::default();
        let mut best = -1.;
        for i in 0..32 {
            let angle = 2. * std::f64::consts::PI * i as f64 / 32.;
            let p = center + Point::new(angle.cos(), angle.sin()) * radius;
            if p.x < 1.
                || p.y < 1.
                || p.x > self.androidSize_.x - 2.
                || p.y > self.androidSize_.y - 2.
            {
                continue;
            }
            if self.cancelReady_
                && distance(p, self.pixels(self.cancelPoint_)) < 16f64.max(radius * 0.2)
            {
                continue;
            }
            let d = distance(p, initial);
            if d > best {
                best = d;
                chosen = p;
            }
        }
        if best < 4. {
            return;
        }
        self.qDragArming_ = true;
        self.pendingGoalSequence_ = 0;
        self.provisionalStartup_ = false;
        self.startupArmed_ = false;
        self.qDragArmSkill_ = skill;
        self.qDragArmSource_ = self.logicalSource_.clone();
        let previous_sequence = self.touchSequence_;
        if !self.move_skill(host, self.skill_normalized(chosen), true) {
            self.finish_skill(host, true);
            return;
        }
        if self.touchSequence_ == previous_sequence {
            if !host.move_touch(self.skillTouch_, self.skillPoint_) {
                self.finish_skill(host, true);
                return;
            }
            self.sync_clock(host);
            self.record_skill_touch(self.skillPoint_, true, false);
            self.skillEchoPending_ = true;
        }
        self.qDragArmPoint_ = self.skillPoint_;
        self.qDragArmSentNs_ = self.now_;
        self.trace(host, "drag_bootstrap", "await_native_valid_drag");
        self.request_pointer_flush(host);
    }
    pub(crate) fn predict_startup(
        &mut self,
        host: &mut impl Host,
        button: &Button,
        active_skill: i32,
    ) {
        if self.qDragArming_
            || !self.frameCameraReady_
            || self.lastFrameNs_ <= 0
            || self.now_ - self.lastFrameNs_ > 100_000_000
        {
            return;
        }
        let cached = self
            .directStartups_
            .iter()
            .find(|cached| {
                cached.epoch == self.frameEpoch_
                    && cached.hero == self.heroId_
                    && cached.slot == button.slot
                    && cached.buttonSkill == button.skillId
                    && cached.resolution == self.androidSize_
                    && distance(self.pixels(cached.press), self.pixels(button.press)) <= 1.
            })
            .cloned();
        if let Some(cached) = cached {
            self.startupArmed_ = false;
            if cached.activeSkill != active_skill {
                return;
            }
            let target = if self.pendingRelease_ && self.pendingWorldValid_ {
                Some(self.pendingWorld_)
            } else {
                host.pointer_world_at_plane(
                    if self.pendingRelease_ {
                        self.pendingPointer_
                    } else {
                        self.pointer_
                    },
                    cached.plane,
                )
            };
            let Some(target) = target.filter(|p| finite(*p)) else {
                return;
            };
            let mut provisional = AngularRadialSolver::default();
            if !provisional.configure(
                self.pixels(cached.center),
                self.radius_pixels(cached.radius),
                self.frameCamera_,
                cached.range,
                self.androidSize_,
            ) {
                return;
            }
            if let Some(goal) = provisional.predict(target - self.frameHero_) {
                if self.move_skill(host, self.skill_normalized(goal), true) {
                    self.provisionalStartup_ = true;
                    self.provisionalSkill_ = cached.activeSkill;
                    self.skillNudged_ = true;
                    self.skillMoved_.restart(self.now_);
                    self.trace(
                        host,
                        "startup_prediction",
                        "cached_geometry_provisional_no_release",
                    );
                }
            }
            return;
        }
        self.startupArmed_ = false;
    }
    pub(crate) fn direct_converged(&self, desired: Point) -> bool {
        if self.direct_.ready == 0
            || !self.acceptedInput_
            || self.acceptedSampleNs_ <= 0
            || self.now_ - self.acceptedSampleNs_ > 100_000_000
        {
            return false;
        }
        self.direct_.predict(desired).is_some_and(|predicted| {
            distance(self.solver_observed(), self.direct_.clamped_world(desired)) <= 0.03
                && distance(self.acceptedPoint_, predicted) <= 1.
        })
    }
    pub(crate) fn pending_direct_acknowledged(&self) -> bool {
        if !self.pendingRelease_
            || !self.pendingWorldValid_
            || self.pendingGoalSequence_ == 0
            || !self.verified_direct_class()
            || self.direct_.ready == 0
            || !self.directGeometryFresh_
            || !self.logicalSampleFresh_
            || !self.acceptedInput_
            || self.pendingGoalSource_ != self.logicalSource_
            || self.pendingGoalSkill_ != self.aimSkillId_
            || self.traceSampleStart_ < self.pendingGoalTime_
            || self.traceMatchedSequence_ < self.pendingGoalSequence_
            || self.acceptedSampleNs_ <= 0
            || self.now_ - self.acceptedSampleNs_ > 100_000_000
        {
            return false;
        }
        self.direct_
            .forward(self.acceptedPoint_)
            .is_some_and(|actual| distance(actual, self.logicalObserved_) <= 0.03)
    }
    pub(crate) fn desired_solver_world(&mut self, host: &mut impl Host) -> Option<Point> {
        if self.logicalMode_
            && (!self.logicalSampleFresh_
                || self.logicalSampleNs_ == 0
                || self.now_ - self.logicalSampleNs_ > 250_000_000)
        {
            return None;
        }
        self.desired_skill_world(host).map(|world| {
            if self.logicalMode_ {
                world - self.logicalOrigin_
            } else {
                world
            }
        })
    }
    pub(crate) fn desired_skill_world(&mut self, host: &mut impl Host) -> Option<Point> {
        if self.pendingRelease_ && self.pendingWorldValid_ {
            return Some(self.pendingWorld_);
        }
        if self.pendingRelease_ && self.pendingWorldResolver_ != 0 {
            let world = host
                .resolve_world(self.pendingWorldResolver_)
                .filter(|p| finite(*p))?;
            self.pendingWorld_ = world;
            self.pendingWorldValid_ = true;
            host.release_resolver(self.pendingWorldResolver_);
            self.pendingWorldResolver_ = 0;
            return Some(world);
        }
        let pointer = if self.pendingRelease_ {
            self.pendingPointer_
        } else {
            self.pointer_
        };
        let world = host.pointer_world(pointer).filter(|p| finite(*p))?;
        if self.pendingRelease_ {
            self.pendingWorld_ = world;
            self.pendingWorldValid_ = true;
        }
        Some(world)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        host::{Binding, Timer},
        snapshot::{Flag, Text, Vector16},
    };
    use super::*;
    use super::super::QueuedSkill;
    #[derive(Default)]
    struct TestHost {
        moves: Vec<(i32, Point)>,
        edges: Vec<(&'static str,i32,Point)>,
        fail_move: bool,
        traces: Vec<(String,String)>,
        world: Option<Point>,
        plane_world: Option<Point>,
        skill_world: Option<Point>,
        released: Vec<u64>,
        binding_aim_type: u8,
    }
    impl Host for TestHost {
        fn begin(&mut self, _: Point) -> i32 {
            1
        }
        fn move_touch(&mut self, id: i32, p: Point) -> bool {
            self.moves.push((id, p));
            self.edges.push(("move",id,p));
            !self.fail_move
        }
        fn end(&mut self, id: i32, p: Point) {self.edges.push(("up",id,p));}
        fn pointer_world(&mut self, _: Point) -> Option<Point> {
            self.world
        }
        fn ground_world(&mut self, _: Point) -> Option<Point> {
            self.world
        }
        fn skill_direction_world(&mut self,_:Point,_skill:i32)->Option<Point>{self.skill_world.or(self.world)}
        fn movement_direction(&mut self, _: Point) -> Option<Point> {
            None
        }
        fn hero_world(&mut self) -> Option<Point> {
            None
        }
        fn pointer_world_at_plane(&mut self, _: Point, _: f64) -> Option<Point> {
            self.plane_world
        }
        fn capture_pointer_world(&mut self, _: Point) -> u64 {
            0
        }
        fn resolve_world(&mut self, _: u64) -> Option<Point> {
            self.world
        }
        fn release_resolver(&mut self, id: u64) {
            self.released.push(id)
        }
        fn navigation(&mut self, _: Point, _: Point, _: bool) -> Result<Vec<Point>, String> {
            Ok(vec![])
        }
        fn binding(&mut self, _: i32, button: &Button, _: &str) -> Binding {
            Binding { key: button.key, aim_type: self.binding_aim_type, ..Binding::default() }
        }
        fn cancel_key(&mut self, _: i32) -> i32 {
            84
        }
        fn timer(&mut self, _: Timer, _: Option<i64>) {}
        fn trace(&mut self, _: &Controller, event: &str, reason: &str) {self.traces.push((event.into(),reason.into()));}
    }
    fn n(value: f64) -> Number {
        Number { value, kind: 1 }
    }
    fn flag(value: bool) -> Flag {
        Flag {
            value: value as i32,
        }
    }
    fn text(value: &str) -> Text {
        let mut t = Text::default();
        t.bytes[..value.len()].copy_from_slice(value.as_bytes());
        t.size = value.len() as u32;
        t.present = 1;
        t
    }
    fn v2(x: f64, y: f64) -> Vector2 {
        Vector2 {
            values: [n(x), n(y)],
            count: 2,
        }
    }
    fn v3(x: f64, y: f64, z: f64) -> Vector3 {
        Vector3 {
            values: [n(x), n(y), n(z)],
            count: 3,
        }
    }
    fn setup() -> (Controller, Frame, TestHost) {
        let mut c = Controller::new();
        c.now_ = 2_000_000_000;
        c.lastFrameNs_ = c.now_;
        c.castStartedNs_ = 1_900_000_000;
        c.heroId_ = 25;
        c.skillTouch_ = 3;
        c.activeSlot_ = "skill:0".into();
        c.frameActiveSlot_ = c.activeSlot_.clone();
        c.frameEpoch_ = "token".into();
        c.aimSkillId_ = 2511;
        c.coreSkill_ = 2511;
        c.aimReady_ = true;
        c.androidSize_ = Point::new(4000., 3111.);
        c.skillPoint_ = Point::new(0.75, 0.85);
        c.skillDownPoint_ = c.skillPoint_;
        c.buttons_.push(Button {
            slot: "skill:0".into(),
            skillId: 2510,
            press: c.skillPoint_,
            ..Button::default()
        });
        let mut f = Frame::default();
        f.token = text("token");
        f.token_string = flag(true);
        f.token_nonempty = flag(true);
        f.skill_id = n(2511.);
        f.view_matrix = Vector16 {
            values: [n(0.); 16],
            count: 16,
        };
        f.view_matrix.values[0] = n(1.);
        f.view_matrix.values[9] = n(-1.);
        f.logical.ready = flag(true);
        f.logical.coherent = flag(true);
        f.logical.source = text(Q_SOURCE_A);
        f.logical.skill_id = n(2511.);
        f.logical.sample_start_ns = n(1_990_000_000.);
        f.logical.sample_end_ns = n(1_995_000_000.);
        f.logical.origin_render_xyz = v3(10., 0., 20.);
        f.logical.raw_touch_xyz = v3(3000., 500., 0.);
        f.logical.offset_xz = v2(1., 2.);
        f.logical.target_render_xyz = v3(11., 0.1, 22.);
        f.logical.joystick.ready = flag(true);
        f.logical.joystick.source = text("active_widget_transform");
        f.logical.joystick.slot = text("skill:0");
        f.logical.joystick.sample_start_ns = f.logical.sample_start_ns;
        f.logical.joystick.sample_end_ns = f.logical.sample_end_ns;
        f.logical.joystick.center_uv = v2(0.75, 0.85);
        f.logical.joystick.radius_uv = v2(0.1, 400. / 3111.);
        f.logical.drag_ratio = n(0.1);
        f.logical.world_range = n(6.2);
        f.logical.world_range_source = text(RANGE_SOURCE);
        f.logical.world_range_source_present = flag(true);
        f.logical.visual_plane_y = n(0.1);
        f.logical.valid_drag = flag(true);
        f.logical.first_drag = flag(false);
        (c, f, TestHost::default())
    }
    #[test]
    fn physical_wall_query_receipt_cannot_be_used_by_another_hero_or_skill() {
        assert!(known_direction_source(84,8410,"KeySkilleCheckMesh.currentDirect"));
        assert!(!known_direction_source(25,8410,"KeySkilleCheckMesh.currentDirect"));
        assert!(!known_direction_source(84,8425,"KeySkilleCheckMesh.currentDirect"));
        assert!(!known_direction_source(84,8410,"UnverifiedDirection"));
    }
    #[test]
    fn directional_release_freezes_visual_intent_independently_of_movement_plane() {
        let (mut c,_,mut h)=setup();c.heroId_=84;c.coreSkill_=8410;c.frameHero_=Point::new(10.,20.);
        c.frameCameraReady_=true;h.world=Some(Point::new(99.,99.));h.skill_world=Some(Point::new(11.,22.));
        c.freeze_direction_intent(&mut h);
        assert!(c.pendingDirectionValid_);assert_eq!(c.pendingWorld_,Point::new(11.,22.));
        assert!((c.pendingDirection_.x-1./5f64.sqrt()).abs()<1e-10);
        assert!((c.pendingDirection_.y-2./5f64.sqrt()).abs()<1e-10);
    }
    #[test]
    fn first_wall_w_arms_from_protected_input_without_acknowledging_stale_target() {
        let (mut c,mut f,mut h)=setup();
        c.heroId_=84;c.activeSlot_="skill:1".into();c.frameActiveSlot_="skill:1".into();c.aimSkillId_=8425;c.coreSkill_=8425;
        f.skill_id=n(8425.);f.logical.skill_id=n(8425.);f.logical.source=text(POINT_SOURCE);f.ui.active_slot=text("skill:1");
        f.logical.joystick.slot=text("skill:1");f.logical.world_range=n(7.5);
        f.logical.ready=flag(false);f.logical.input_ready=flag(true);f.logical.valid_drag=flag(false);f.logical.first_drag=flag(true);
        c.select_logical_coordinates(&mut h,&f);
        assert!(c.logicalInputFresh_);assert!(!c.logicalSampleFresh_);
        c.configure_direct(&mut h,&f);assert_eq!(c.direct_.world_range,7.5);
        c.update_range_drag_arming(&mut h,&f);assert!(c.qDragArming_);assert_eq!(h.moves.len(),1);
        c.update_range_drag_arming(&mut h,&f);assert_eq!(h.moves.len(),1);
        assert!(!c.pending_direct_acknowledged());
    }
    #[test]
    fn ling_range_forms_use_own_current_native_range_and_coherent_receipts() {
        for (skill,slot,source,range) in [(8425,"skill:1",POINT_SOURCE,7.5)] {
            let (mut c,mut f,mut h)=setup();c.heroId_=84;c.activeSlot_=slot.into();c.frameActiveSlot_=slot.into();
            c.aimSkillId_=skill;c.coreSkill_=skill;f.skill_id=n(skill as f64);f.ui.active_slot=text(slot);
            f.logical.source=text(source);f.logical.skill_id=n(skill as f64);f.logical.joystick.slot=text(slot);
            f.logical.world_range=n(range);
            c.select_logical_coordinates(&mut h,&f);c.select_logical_coordinates(&mut h,&f);
            assert!(c.verified_direct_class());assert!(c.logicalSampleFresh_);
            c.configure_direct(&mut h,&f);assert_eq!(c.direct_.world_range,range);
            f.logical.skill_id=n(999.);c.select_logical_coordinates(&mut h,&f);assert!(!c.logicalSampleFresh_);
        }
    }
    #[test]
    fn panorama_bottom_and_corner_goals_preserve_full_native_joystick_radius() {
        let (mut c, _, mut h) = setup();
        let centers = [
            Point::new(0.7683197003091601, 0.9138937894708801),
            Point::new(0.04, 0.04), Point::new(0.96, 0.04),
            Point::new(0.04, 0.96), Point::new(0.96, 0.96),
        ];
        // First two are cursor(.5,.95) and(.5,1) world offsets captured
        // while the game's own aiming panorama translated its camera.
        let desired = [Point::new(-4.3057296115, -4.2442832933),
                       Point::new(-4.7531061897, -4.6922410220),
                       Point::new(-6.2, -6.2), Point::new(6.2, -6.2),
                       Point::new(-6.2, 6.2), Point::new(6.2, 6.2)];
        let camera = [0.7077527046, -0.4810055494, -0.7064598203, -0.4809567332];
        let mut overflow_seen = false;
        for center in centers {
            let mut model = AngularRadialSolver::default();
            assert!(model.configure(c.pixels(center), 384.5804280084, camera, 6.2, c.androidSize_));
            for target in desired {
                let command = model.predict(target).unwrap();
                assert!(c.move_skill(&mut h, c.skill_normalized(command), true));
                let sent = c.pixels(c.skillPoint_);
                overflow_seen |= sent.x < 0. || sent.y < 0. || sent.x > 4000. || sent.y > 3111.;
                let actual = model.forward(cell(sent)).unwrap();
                assert!(distance(actual, model.clamped_world(target)) < 0.015,
                        "screen clipping shortened or rotated a generated joystick command");
                let receipt = c.causal_.latest().unwrap();
                assert_eq!(receipt.pixels, cell(sent));
            }
        }
        assert!(overflow_seen);
        // Real pointer/manual/movement coordinates still follow screen bounds.
        c.update_pointer(&mut h, Point::new(-0.2, 1.3));
        assert_eq!(c.pointer_, Point::new(0., 1.));
        assert_eq!(c.normalized(Point::new(-80., 3300.)), Point::new(0., 1.));
        assert!(c.move_skill(&mut h, Point::new(-0.2, 1.3), false));
        assert_eq!(c.skillPoint_, Point::new(0., 1.));
    }
    #[test]
    fn directional_w_keeps_offscreen_angle_and_does_not_expand_cursor_bounds() {
        let (mut c, _, mut h) = setup();
        c.directionGeometryFresh_ = true;
        c.frameCameraReady_ = true;
        c.frameCamera_ = [1., 0., 0., 1.];
        c.directionCenter_ = Point::new(0.96, 0.96);
        c.directionRadius_ = Point::new(0.1, 400. / 3111.);
        c.frameHero_ = Point::new(10., 20.);
        for offset in [Point::new(6., 0.), Point::new(0., 6.), Point::new(-6., -6.)] {
            h.world = Some(c.frameHero_ + offset);
            let command = c.direction_point(&mut h).unwrap();
            assert!(c.send_direction(&mut h, command));
            let mut model = AngularRadialSolver::default();
            assert!(model.configure(c.pixels(c.directionCenter_), 400., c.frameCamera_, 1., c.androidSize_));
            let actual = model.forward(cell(c.pixels(c.skillPoint_))).unwrap();
            assert!(distance(actual, offset / offset.length()) < 0.004);
        }
    }
    #[test]
    fn protected_raw_can_arm_but_never_becomes_logical_feedback() {
        let (mut c, mut f, mut h) = setup();
        f.logical.ready = flag(false);
        f.logical.input_ready = flag(true);
        f.logical.valid_drag = flag(false);
        f.logical.first_drag = flag(true);
        c.select_logical_coordinates(&mut h, &f);
        assert!(c.logicalMode_ && c.logicalInputFresh_);
        assert!(!c.logicalSampleFresh_);
        c.configure_direct(&mut h, &f);
        assert_eq!(c.direct_.ready, 1);
        assert_eq!(c.direct_.world_range, 6.2);
        c.update_range_drag_arming(&mut h, &f);
        assert!(c.qDragArming_);
        assert!(!h.moves.is_empty());
        let count = h.moves.len();
        c.update_range_drag_arming(&mut h, &f);
        assert_eq!(
            h.moves.len(),
            count,
            "protected snapshots must not issue repeated arming throws"
        );
        c.pendingRelease_ = true;
        c.pendingWorldValid_ = true;
        c.acceptedInput_ = true;
        assert!(!c.pending_direct_acknowledged());
        let arm = cell(c.pixels(c.qDragArmPoint_));
        f.logical.valid_drag = flag(true);
        f.logical.first_drag = flag(false);
        f.logical.raw_touch_xyz = v3(
            arm.x * (c.androidSize_.x - 1.) / c.androidSize_.x,
            c.androidSize_.y - arm.y * (c.androidSize_.y - 1.) / c.androidSize_.y,
            0.,
        );
        f.logical.sample_start_ns = n(c.qDragArmSentNs_ as f64);
        c.logicalSampleFresh_ = true;
        c.update_range_drag_arming(&mut h, &f);
        assert!(
            !c.qDragArming_,
            "only native valid raw receipt ends the bootstrap"
        );
    }
    fn generic_point_range_fixture(hero: i32, skill: i32) -> (Controller, Frame, TestHost) {
        let (mut c, mut f, h) = setup();
        c.heroId_ = hero;
        c.coreSkill_ = skill;
        c.aimSkillId_ = skill;
        c.aimReady_ = false;
        c.buttons_[0].skillId = skill;
        f.hero_id = n(hero as f64);
        f.skill_id = n(skill as f64);
        f.logical.skill_id = f.skill_id;
        f.logical.source = text(POINT_SOURCE);
        f.logical.ready = flag(false);
        f.logical.input_ready = flag(true);
        f.logical.valid_drag = flag(false);
        f.logical.first_drag = flag(true);
        f.logical.offset_xz = Default::default();
        f.logical.target_render_xyz = Default::default();
        f.logical.drag_ratio = Number::default();
        f.logical.world_range = n(3.);
        (c, f, h)
    }
    #[test]
    fn generic_authorized_native_range_bootstraps_before_processed_offset_exists() {
        for (hero, skill) in [(118, 11810), (937, 93760)] {
            let (mut c, f, mut h) = generic_point_range_fixture(hero, skill);
            c.select_logical_coordinates(&mut h, &f);
            assert!(c.logicalInputFresh_ && !c.logicalSampleFresh_);
            c.configure_direct(&mut h, &f);
            assert!(c.directGeometryFresh_);
            assert_eq!(c.direct_.ready, 1);
            assert_eq!(c.direct_.world_range, 3.);
            assert_eq!(c.directRangeSource_, RANGE_SOURCE);
            assert!(c.directRanges_.is_empty(), "generic fixed metadata is a current witness, not a reusable fixed-range cache");
            assert!(c.directStartups_.is_empty(), "a later unknown charge policy must not reuse provisional fixed metadata");
        }
    }
    #[test]
    fn generic_native_range_malformed_or_unauthorized_witness_never_falls_back() {
        for case in 0..10 {
            let (mut c, mut f, mut h) = generic_point_range_fixture(118, 11810);
            // Even a plausible current observed range must not conceal an
            // explicitly malformed or contradictory native witness.
            f.logical.ready = flag(true);
            f.logical.valid_drag = flag(true);
            f.logical.first_drag = flag(false);
            f.logical.offset_xz = v2(1., 2.);
            f.logical.target_render_xyz = v3(11., 0.1, 22.);
            f.logical.drag_ratio = n(0.5);
            match case {
                0 => f.logical.world_range = Number { kind: 2, value: 3. },
                1 => f.logical.world_range = n(f64::NAN),
                2 => f.logical.world_range = n(-1.),
                3 => f.logical.world_range = n(0.),
                4 => f.logical.world_range = n(10_001.),
                5 => f.logical.world_range_source = text("UnknownRangeSource"),
                6 => f.logical.world_range_source_present = flag(false),
                7 => f.logical.world_range_source_present = Flag::default(),
                8 => f.logical.world_range_source_present.value = 2,
                _ => f.logical.world_range_source = Text::default(),
            }
            c.select_logical_coordinates(&mut h, &f);
            assert!(c.logicalSampleFresh_);
            c.configure_direct(&mut h, &f);
            assert_eq!(c.direct_.ready, 0, "case {case}: malformed fixed metadata cannot silently become a ratio/cache model");
            assert!(c.directRanges_.is_empty());
        }
    }
    #[test]
    fn revoked_generic_fixed_range_cannot_reuse_native_or_stale_observed_cache() {
        for permission in [Flag::default(), flag(false)] {
            let (mut c, mut f, mut h) = generic_point_range_fixture(118, 11810);
            c.select_logical_coordinates(&mut h, &f);
            c.configure_direct(&mut h, &f);
            assert_eq!(c.direct_.world_range, 3.);
            for (native, range) in [(true, 3.), (false, 9.)] {
                c.directRanges_.push(DirectRange {
                    hero: c.heroId_, skill: c.aimSkillId_, slot: c.activeSlot_.clone(),
                    source: c.logicalSource_.clone(), resolution: c.androidSize_, range, native,
                });
            }
            c.directStartups_.push(DirectStartup { slot: c.activeSlot_.clone(), range: 3., ..Default::default() });
            f.logical.world_range = Number::default();
            f.logical.world_range_source = Text::default();
            f.logical.world_range_source_present = permission;
            f.logical.drag_ratio = n(0.5);
            c.logicalObserved_ = Point::new(300., 400.);
            c.configure_direct(&mut h, &f);
            assert_eq!(c.direct_.ready, 0, "protected input cannot turn stale processed offsets or old fixed metadata into today's range");
            assert!(c.directRanges_.iter().all(|entry| !entry.native));
            assert!(c.directStartups_.is_empty());
            f.logical.ready = flag(true);
            f.logical.valid_drag = flag(true);
            f.logical.first_drag = flag(false);
            f.logical.offset_xz = v2(1., 2.);
            f.logical.target_render_xyz = v3(11., 0.1, 22.);
            c.select_logical_coordinates(&mut h, &f);
            assert!(c.logicalSampleFresh_);
            c.configure_direct(&mut h, &f);
            assert_eq!(c.direct_.ready, 1);
            assert!((c.direct_.world_range - 5f64.sqrt() / 0.5).abs() < 1e-12);
            assert_eq!(c.directRangeSource_, "offset_over_drag_ratio");
            assert!(c.directRanges_.iter().all(|entry| !entry.native));
        }
    }
    #[test]
    fn generic_native_range_requires_a_fresh_same_cast_logical_witness() {
        let (mut c, mut f, mut h) = generic_point_range_fixture(118, 11810);
        f.logical.sample_start_ns = n(1_500_000_000.);
        f.logical.sample_end_ns = n(1_501_000_000.);
        f.logical.joystick.sample_start_ns = f.logical.sample_start_ns;
        f.logical.joystick.sample_end_ns = f.logical.sample_end_ns;
        c.select_logical_coordinates(&mut h, &f);
        assert!(!c.logicalInputFresh_);
        c.configure_direct(&mut h, &f);
        assert_eq!(c.direct_.ready, 0);
    }
    #[test]
    fn native_q_never_learns_poisoned_processed_offset_ratio() {
        let (mut c, mut f, mut h) = setup();
        c.select_logical_coordinates(&mut h, &f);
        f.logical.world_range = Number::default();
        f.logical.world_range_source = Text::default();
        f.logical.world_range_source_present = Flag::default();
        c.configure_direct(&mut h, &f);
        assert_eq!(c.direct_.ready, 0);
        assert!(c.directRanges_.is_empty());
        f.logical.world_range = n(6.2);
        f.logical.world_range_source = text(RANGE_SOURCE);
        f.logical.world_range_source_present = flag(true);
        c.configure_direct(&mut h, &f);
        assert_eq!(c.direct_.ready, 1);
        assert_eq!(c.direct_.world_range, 6.2);
        assert_eq!(c.directRanges_.len(), 1);
        f.logical.world_range = Number::default();
        f.logical.world_range_source = Text::default();
        f.logical.world_range_source_present = Flag::default();
        c.logicalObserved_ = Point::new(500., 800.);
        f.logical.drag_ratio = n(0.0001);
        c.configure_direct(&mut h, &f);
        assert_eq!(c.direct_.ready, 1);
        assert_eq!(c.direct_.world_range, 6.2);
        f.logical.world_range = n(6.2);
        f.logical.world_range_source = text("unknown");
        c.configure_direct(&mut h, &f);
        assert_eq!(
            c.direct_.ready, 0,
            "explicit invalid metadata must not silently use cache"
        );
    }
    #[test]
    fn unavailable_same_motion_preserves_relative_mode_but_changed_source_resets() {
        let (mut c, mut f, mut h) = setup();
        c.select_logical_coordinates(&mut h, &f);
        assert!(c.logicalSampleFresh_);
        f.logical.ready = flag(false);
        f.logical.coherent = flag(false);
        c.select_logical_coordinates(&mut h, &f);
        assert!(c.logicalMode_);
        assert!(!c.logicalSampleFresh_);
        assert!(!c.logicalInputFresh_);
        f.logical.source = text("UnsupportedMotion.m_vDragResult");
        c.select_logical_coordinates(&mut h, &f);
        assert!(!c.logicalMode_);
    }
    #[test]
    fn same_bracket_geometry_and_current_native_range_are_mandatory() {
        let (mut c, mut f, mut h) = setup();
        c.select_logical_coordinates(&mut h, &f);
        f.logical.joystick.sample_end_ns = n(1_994_000_000.);
        c.configure_direct(&mut h, &f);
        assert_eq!(c.direct_.ready, 0);
        assert!(!c.directGeometryFresh_);
        f.logical.joystick.sample_end_ns = f.logical.sample_end_ns;
        c.configure_direct(&mut h, &f);
        assert_eq!(c.direct_.ready, 1);
        f.logical.joystick.slot = text("skill:1");
        c.configure_direct(&mut h, &f);
        assert_eq!(c.direct_.ready, 0);
        assert!(!c.directGeometryFresh_);
    }
    #[test]
    fn variant_zero_duplicates_and_epoch_are_fail_closed() {
        let (mut c, mut f, _) = setup();
        let button = Button {
            slot: "skill:1".into(),
            skillId: 2520,
            ..Button::default()
        };
        let s = &mut f.skill_state;
        s.ready = flag(true);
        s.source = text("ShowSkillData.m_TranID");
        s.token = text("token");
        s.hero_id = n(25.);
        s.sample_start_ns = n(1_990_000_000.);
        s.sample_end_ns = n(1_995_000_000.);
        s.count = 1;
        s.buttons[0].slot = text("skill:1");
        s.buttons[0].base_skill_id = n(2520.);
        s.buttons[0].active_skill_id = n(0.);
        c.ingest_skill_variants(&f);
        assert_eq!(c.classify_skill(&button), SkillBehavior::AwaitVariant);
        f.skill_state.buttons[0].active_skill_id = n(2520.);
        c.ingest_skill_variants(&f);
        assert_eq!(c.classify_skill(&button), SkillBehavior::Direction);
        f.skill_state.buttons[0].active_skill_id = n(2521.);
        c.ingest_skill_variants(&f);
        assert_eq!(c.classify_skill(&button), SkillBehavior::Tap);
        f.skill_state.count = 2;
        f.skill_state.buttons[1] = f.skill_state.buttons[0];
        c.ingest_skill_variants(&f);
        assert!(c.skillVariants_.is_empty());
        f.skill_state.count = 1;
        f.skill_state.token = text("other");
        c.ingest_skill_variants(&f);
        assert_eq!(c.classify_skill(&button), SkillBehavior::AwaitVariant);
        c.frameEpoch_ = "\"\"".into();
        f.token_nonempty = flag(false);
        f.skill_state.token = text("\"\"");
        c.ingest_skill_variants(&f);
        assert!(c.skillVariants_.is_empty());
    }
    #[test]
    fn seven_digit_native_variants_preserve_integer_bounds_and_match_ownership() {
        let (mut c,mut f,_)=setup();c.heroId_=131;
        let mut state=super::super::snapshot::VariantState {
            ready:flag(true),source:text("ShowSkillData.m_TranID"),token:text("token"),hero_id:n(131.),
            sample_start_ns:n(1_990_000_000.),sample_end_ns:n(1_995_000_000.),count:1,..Default::default()
        };
        state.buttons[0].slot=text("skill:0");
        for (base,active,accepted) in [(1_013_110.,2_013_110.,true),(10_000_000.,10_000_000.,true),
            (1_013_110.,0.,true),(0.,2_013_110.,false),(-1.,2_013_110.,false),
            (10_000_001.,2_013_110.,false),(1_013_110.,10_000_001.,false),
            (1_013_110.,-1.,false),(1_013_110.5,2_013_110.,false),(1_013_110.,f64::NAN,false)] {
            state.buttons[0].base_skill_id=n(base);state.buttons[0].active_skill_id=n(active);
            f.skill_state=state;c.ingest_skill_variants(&f);
            assert_eq!(c.skillVariants_.len(),usize::from(accepted),"base={base}, active={active}");
            if accepted {assert_eq!(c.skillVariants_[0].active,active as i32);}
        }
        state.buttons[0].base_skill_id=n(1_013_110.);state.buttons[0].active_skill_id=n(2_013_110.);
        for case in ["base_type","active_type","hero","epoch","duplicate"] {
            f.skill_state=state;
            match case {
                "base_type"=>f.skill_state.buttons[0].base_skill_id.kind=2,
                "active_type"=>f.skill_state.buttons[0].active_skill_id.kind=0,
                "hero"=>f.skill_state.hero_id=n(130.),
                "epoch"=>f.skill_state.token=text("different-match"),
                "duplicate"=>{f.skill_state.count=2;f.skill_state.buttons[1]=f.skill_state.buttons[0];},
                _=>unreachable!(),
            }
            c.ingest_skill_variants(&f);assert!(c.skillVariants_.is_empty(),"{case}");
        }
    }
    #[test]
    fn seven_digit_cooldown_ids_use_the_same_bound_without_bypassing_typed_owner_checks() {
        let (mut c,_,_)=setup();c.heroId_=131;c.cooldownSeen_=true;
        let button=Button {slot:"skill:0".into(),kind:"skill".into(),skillId:1_013_110,..Default::default()};
        let mut state=super::super::snapshot::CooldownState {
            present:flag(true),ready:flag(true),coherent:flag(true),source:text("VirtualButton.m_CdCoolDown"),
            token:text("token"),hero_id:n(131.),sample_start_ns:n(1_990_000_000.),sample_end_ns:n(1_995_000_000.),count:1,..Default::default()
        };
        state.buttons[0]=super::super::snapshot::CooldownRow {
            slot:text("skill:0"),base_skill_id:n(1_013_110.),active_skill_id:n(2_013_110.),ready:flag(true),on_cooldown:flag(false)
        };
        for active in [1_013_110.,2_013_110.,10_000_000.] {
            state.buttons[0].active_skill_id=n(active);c.cooldownState_=state;
            assert_eq!(c.cooldown_block(&button,0),"");
            c.cooldownState_.buttons[0].on_cooldown=flag(true);
            assert_eq!(c.cooldown_block(&button,0),"active");
        }
        state.buttons[0].active_skill_id=n(2_013_110.);
        for case in ["zero","negative","over_bound","fraction","type","base","hero","epoch","duplicate"] {
            c.cooldownState_=state;
            match case {
                "zero"=>c.cooldownState_.buttons[0].active_skill_id=n(0.),
                "negative"=>c.cooldownState_.buttons[0].active_skill_id=n(-1.),
                "over_bound"=>c.cooldownState_.buttons[0].active_skill_id=n(10_000_001.),
                "fraction"=>c.cooldownState_.buttons[0].active_skill_id=n(2_013_110.5),
                "type"=>c.cooldownState_.buttons[0].active_skill_id.kind=2,
                "base"=>c.cooldownState_.buttons[0].base_skill_id=n(0.),
                "hero"=>c.cooldownState_.hero_id=n(130.),
                "epoch"=>c.cooldownState_.token=text("different-match"),
                "duplicate"=>{c.cooldownState_.count=2;c.cooldownState_.buttons[1]=c.cooldownState_.buttons[0];},
                _=>unreachable!(),
            }
            assert_eq!(c.cooldown_block(&button,0),"unknown","{case}");
        }
    }
    #[test]
    fn empty_process_identity_never_warms_startup_geometry() {
        let (mut c, mut f, mut h) = setup();
        f.token_nonempty = flag(false);
        c.select_logical_coordinates(&mut h, &f);
        c.configure_direct(&mut h, &f);
        assert!(c.directStartups_.is_empty());
        assert_eq!(c.direct_.ready, 1);
    }
    #[test]
    fn startup_prediction_is_provisional_and_checks_form_and_epoch() {
        let (mut c, mut f, mut h) = setup();
        c.frameCameraReady_ = true;
        c.frameCamera_ = [1., 0., 0., 1.];
        c.select_logical_coordinates(&mut h, &f);
        c.configure_direct(&mut h, &f);
        assert_eq!(c.directStartups_.len(), 1);
        h.plane_world = Some(Point::new(2., 3.));
        let button = c.buttons_[0].clone();
        c.predict_startup(&mut h, &button, 2510);
        assert!(h.moves.is_empty());
        c.predict_startup(&mut h, &button, 2511);
        assert!(!h.moves.is_empty());
        assert!(c.provisionalStartup_);
        assert!(!c.pendingRelease_);
        h.moves.clear();
        c.frameEpoch_ = "new".into();
        c.predict_startup(&mut h, &button, 2511);
        assert!(h.moves.is_empty());
        f.logical.world_range = n(f64::NAN);
        c.configure_direct(&mut h, &f);
        assert_eq!(c.direct_.ready, 0);
    }

    fn held_variant_direction_fixture() -> (Controller, Frame, TestHost) {
        let mut c = Controller::new();
        c.enabled_ = true;
        c.ready_ = true;
        c.now_ = 1_980_000_000;
        c.castStartedNs_ = 1_900_000_000;
        c.heroId_ = 126;
        c.traceEnabled_ = true;
        c.frameEpoch_ = "token".into();
        c.skillTouch_ = 3;
        c.skillKey_ = 81;
        c.skillKeyHeld_ = true;
        c.skillBehavior_ = SkillBehavior::Direction;
        c.activeSlot_ = "skill:0".into();
        c.androidSize_ = Point::new(1000., 1000.);
        c.skillPoint_ = Point::new(0.6005, 0.5005);
        c.record_skill_touch(c.skillPoint_, false, false);
        c.buttons_.push(Button { slot: "skill:0".into(), kind: "skill".into(),
            skillId: 12610, key: 81, aimType: 2, available: true, ..Button::default() });
        let mut f = Frame::default();
        f.ready = flag(true);
        f.token = text("token");
        f.token_string = flag(true);
        f.token_nonempty = flag(true);
        f.hero_id = n(126.);
        f.time_ns = n(2_000_000_000.);
        f.sample_duration_ms = n(1.);
        f.skill_id = n(12610.);
        f.hero_render_xyz = v3(0., 0., 0.);
        f.view_matrix = Vector16 { values: [n(0.); 16], count: 16 };
        f.view_matrix.values[0] = n(1.);
        f.view_matrix.values[9] = n(-1.);
        f.ui.ready = flag(true);
        f.ui.hero_id = n(126.);
        f.ui.time_ns = f.time_ns;
        f.ui.active_slot = text("skill:0");
        f.ui.resolution_px = v2(1000., 1000.);
        f.ui.button_count = 1;
        f.ui.buttons[0] = super::super::snapshot::Button {
            slot: text("skill:0"), kind: text("skill"), skill_id: n(12610.),
            index: n(0.), active: flag(true), press_uv: v2(0.5, 0.5),
            center_uv: v2(0.5, 0.5), radius_uv: v2(0.1, 0.1),
            ..Default::default()
        };
        f.skill_state.ready = flag(true);
        f.skill_state.source = text("ShowSkillData.m_TranID");
        f.skill_state.token = f.token;
        f.skill_state.hero_id = f.hero_id;
        f.skill_state.sample_start_ns = n(1_998_000_000.);
        f.skill_state.sample_end_ns = n(1_999_000_000.);
        f.skill_state.count = 1;
        f.skill_state.buttons[0].slot = text("skill:0");
        f.skill_state.buttons[0].base_skill_id = n(12610.);
        f.skill_state.buttons[0].active_skill_id = n(12610.);
        f.active_joystick = super::super::snapshot::Geometry {
            ready: flag(true), source: text("active_widget_transform"),
            slot: text("skill:0"), skill_id: n(12610.),
            sample_start_ns: n(1_998_000_000.), sample_end_ns: n(1_999_000_000.),
            center_uv: v2(0.5, 0.5), radius_uv: v2(0.1, 0.1),
        };
        f.direction_input.ready = flag(true);
        f.direction_input.coherent = flag(true);
        f.direction_input.source = text("IndicatorMotionRot.m_vDragResult");
        f.direction_input.token = f.token;
        f.direction_input.hero_id = f.hero_id;
        f.direction_input.slot = text("skill:0");
        f.direction_input.skill_id = f.skill_id;
        f.direction_input.sample_start_ns = f.active_joystick.sample_start_ns;
        f.direction_input.sample_end_ns = f.active_joystick.sample_end_ns;
        f.direction_input.raw_touch_xyz = v3(599.4, 500.5, 0.);
        let mut model = AngularRadialSolver::default();
        assert!(model.configure(Point::new(500., 500.), 100., [1., 0., 0., 1.], 1., c.androidSize_));
        let observed = model.forward(Point::new(600., 500.)).unwrap();
        let observed = observed / observed.length();
        f.direction_input.direction_xz = v2(observed.x, observed.y);
        c.now_ = 2_000_000_000;
        (c, f, TestHost { world: Some(Point::new(1., 0.)), binding_aim_type: 2, ..Default::default() })
    }
    #[test]
    fn held_direction_rotates_when_release_variant_changes_but_indicator_stays_base() {
        let (mut c, mut f, mut h) = held_variant_direction_fixture();
        for (step, variant, desired) in [
            (0, 12610, Point::new(1., 0.)),
            (1, 12611, Point::new(0., 1.)),
            (2, 12611, Point::new(-1., 0.)),
        ] {
            let now = 2_000_000_000 + step * 30_000_000;
            c.now_ = now;
            f.time_ns = n(now as f64);
            f.ui.time_ns = f.time_ns;
            f.skill_state.sample_start_ns = n((now - 2_000_000) as f64);
            f.skill_state.sample_end_ns = n((now - 1_000_000) as f64);
            f.skill_state.buttons[0].active_skill_id = n(variant as f64);
            f.active_joystick.sample_start_ns = f.skill_state.sample_start_ns;
            f.active_joystick.sample_end_ns = f.skill_state.sample_end_ns;
            f.direction_input.sample_start_ns = f.active_joystick.sample_start_ns;
            f.direction_input.sample_end_ns = f.active_joystick.sample_end_ns;
            let actual = cell(c.pixels(c.skillPoint_));
            f.direction_input.raw_touch_xyz = v3(actual.x * 999. / 1000., 1000. - actual.y * 999. / 1000., 0.);
            let mut model = AngularRadialSolver::default();
            assert!(model.configure(Point::new(500., 500.), 100., [1., 0., 0., 1.], 1., c.androidSize_));
            let observed = model.forward(actual).unwrap();
            let observed = observed / observed.length();
            f.direction_input.direction_xz = v2(observed.x, observed.y);
            h.world = Some(desired);
            c.ingest_frame(&mut h, &f);
            assert!(c.directionGeometryFresh_, "current base indicator must remain live after variant switch");
            assert!(c.directionAccepted_, "the same current-cast raw receipt remains valid");
            assert_eq!(c.skillTouch_, 3);
            assert!(c.skillKeyHeld_);
        }
        assert!(h.moves.len() >= 2, "held cursor changes must reach the joystick after the charge transition");
        assert!(h.edges.iter().all(|edge| edge.0 != "up"));
    }
    #[test]
    fn direction_indicator_may_belong_to_fresh_active_variant() {
        let (mut c, mut f, mut h) = held_variant_direction_fixture();
        f.skill_state.buttons[0].active_skill_id = n(12611.);
        f.skill_id = n(12611.);
        f.active_joystick.skill_id = f.skill_id;
        f.direction_input.skill_id = f.skill_id;
        c.ingest_frame(&mut h, &f);
        assert!(c.directionGeometryFresh_, "geometry rejected: {:?}", h.traces);
        assert!(c.directionAccepted_, "receipt rejected: {:?}", h.traces);
    }
    #[test]
    fn direction_identity_rejects_unrelated_or_stale_owned_geometry() {
        for case in 0..9 {
            let (mut c, mut f, mut h) = held_variant_direction_fixture();
            f.skill_state.buttons[0].active_skill_id = n(12611.);
            match case {
                0 => { f.skill_id = n(777.); f.active_joystick.skill_id = f.skill_id; f.direction_input.skill_id = f.skill_id; }
                1 => f.ui.active_slot = text("skill:1"),
                2 => { f.skill_state.buttons[0].active_skill_id = n(12612.); f.skill_id = n(12611.); f.active_joystick.skill_id = f.skill_id; f.direction_input.skill_id = f.skill_id; }
                3 => f.skill_state.ready = flag(false),
                4 => h.binding_aim_type = 3,
                5 => f.active_joystick.slot = text("skill:1"),
                6 => f.active_joystick.source = text("UnverifiedGeometry"),
                7 => { f.active_joystick.sample_start_ns = n(1_799_000_000.); f.active_joystick.sample_end_ns = n(1_800_000_000.); }
                _ => f.token = text("different-owner-epoch"),
            }
            c.ingest_frame(&mut h, &f);
            assert!(!c.directionGeometryFresh_, "case {case} must not borrow another direction contract");
            assert!(!c.directionAccepted_, "case {case} must not acknowledge a cast");
            assert!(h.moves.is_empty(), "case {case} must not send held-direction input from invalid geometry");
        }
    }
    #[test]
    fn direction_feedback_cannot_ack_foreign_or_missing_input_but_owned_geometry_can_bootstrap() {
        for case in 0..6 {
            let (mut c, mut f, mut h) = held_variant_direction_fixture();
            f.skill_state.buttons[0].active_skill_id = n(12611.);
            h.world = Some(Point::new(0., 1.));
            match case {
                0 => f.direction_input.slot = text("skill:1"),
                1 => f.direction_input.token = text("previous-match"),
                2 => f.direction_input.hero_id = n(84.),
                3 => f.direction_input.source = text("UnverifiedDirection"),
                4 => { f.direction_input.sample_end_ns = n(1_800_000_000.); f.direction_input.sample_start_ns = n(1_799_000_000.); }
                _ => f.direction_input = Default::default(),
            }
            c.ingest_frame(&mut h, &f);
            assert!(c.directionGeometryFresh_, "case {case}: feedback availability must not revoke independently owned geometry");
            assert!(!c.directionAccepted_, "case {case}: geometry identity alone must not acknowledge a cast");
            assert!(!h.moves.is_empty(), "case {case}: the first MOVE must not require the ACK it creates");
            assert!(h.edges.iter().all(|edge| edge.0 != "up"));
        }
    }
    #[test]
    fn cold_direction_first_move_precedes_unit_feedback_then_exact_receipt_is_accepted() {
        let (mut c, mut f, mut h) = held_variant_direction_fixture();
        f.skill_state.buttons[0].active_skill_id = n(12611.);
        let contract = f.direction_input;
        f.direction_input = Default::default();
        h.world = Some(Point::new(0., 1.));
        c.ingest_frame(&mut h, &f);
        assert!(c.directionGeometryFresh_);
        assert!(!c.directionAccepted_);
        assert!(!h.moves.is_empty());
        let sent_sequence = c.touchSequence_;
        assert!(sent_sequence > 1);
        c.now_ += 30_000_000;
        f.time_ns = n(c.now_ as f64);
        f.ui.time_ns = f.time_ns;
        f.skill_state.sample_start_ns = n((c.now_ - 2_000_000) as f64);
        f.skill_state.sample_end_ns = n((c.now_ - 1_000_000) as f64);
        f.active_joystick.sample_start_ns = f.skill_state.sample_start_ns;
        f.active_joystick.sample_end_ns = f.skill_state.sample_end_ns;
        f.direction_input = contract;
        f.direction_input.sample_start_ns = f.active_joystick.sample_start_ns;
        f.direction_input.sample_end_ns = f.active_joystick.sample_end_ns;
        let actual = cell(c.pixels(c.skillPoint_));
        f.direction_input.raw_touch_xyz = v3(actual.x * 999. / 1000., 1000. - actual.y * 999. / 1000., 0.);
        let mut model = AngularRadialSolver::default();
        assert!(model.configure(Point::new(500., 500.), 100., [1., 0., 0., 1.], 1., c.androidSize_));
        let observed = model.forward(actual).unwrap();
        let observed = observed / observed.length();
        f.direction_input.direction_xz = v2(observed.x, observed.y);
        c.ingest_frame(&mut h, &f);
        assert!(c.directionGeometryFresh_ && c.directionAccepted_);
        assert_eq!(c.directionMatchedSequence_, sent_sequence);
        assert_eq!(c.skillTouch_, 3);
        assert!(h.edges.iter().all(|edge| edge.0 != "up"));
    }
    #[test]
    fn charge_variant_base_indicator_still_requires_exact_sent_raw_provenance() {
        let (mut c, mut f, mut h) = held_variant_direction_fixture();
        f.skill_state.buttons[0].active_skill_id = n(12611.);
        f.direction_input.raw_touch_xyz = v3(619.4, 500.5, 0.);
        c.ingest_frame(&mut h, &f);
        assert!(c.directionGeometryFresh_);
        assert!(!c.directionAccepted_, "identity alone must not turn unrelated raw input into a receipt");
    }
    fn buffered_direction_fixture() -> (Controller,TestHost) {
        let (mut c,_,h)=setup();
        c.enabled_=true;c.ready_=true;c.traceEnabled_=true;c.heroId_=84;
        c.activeSlot_="skill:1".into();c.frameActiveSlot_=c.activeSlot_.clone();
        c.coreSkill_=8420;c.skillKey_=87;c.skillKeyHeld_=false;c.skillBehavior_=SkillBehavior::Direction;
        c.buttons_[0].slot=c.activeSlot_.clone();c.buttons_[0].skillId=8420;c.buttons_[0].aimType=2;
        c.frameCameraReady_=true;c.frameCamera_=[1.,0.,0.,1.];
        c.directionGeometryFresh_=true;c.directionCenter_=Point::new(0.8,0.8);c.directionRadius_=Point::new(0.1,0.1);
        c.pendingRelease_=true;c.pendingDirectionValid_=true;c.pendingDirection_=Point::new(1.,0.);
        c.releaseRequested_.restart(c.now_);c.directionAccepted_=false;c.directionMatchedSequence_=0;
        (c,h)
    }
    #[test]
    fn buffered_direction_physical_up_sends_final_move_then_up_without_native_echo() {
        let (mut c,mut h)=buffered_direction_fixture();
        let contact=c.skillTouch_;c.movementTouch_=9;c.movementHeld_=false;
        c.queuedSkills_.push(QueuedSkill {slot:"skill:1".into(),key:87,held:true,requestedNs:c.now_,..QueuedSkill::default()});
        c.tick(&mut h);
        assert_eq!(c.queuedSkills_.len(),1,"release must retain the next physical press");
        assert!(c.queuedSkills_[0].held);
        assert_eq!(c.skillTouch_,-1,"physical UP must not wait for the buffered indicator");
        assert_eq!(c.movementTouch_,9,"independent movement contact survives");
        assert_eq!(h.edges.last().unwrap().0,"up");
        assert_eq!(h.edges.last().unwrap().1,contact);
        assert!(h.edges.iter().any(|e|e.0=="move"&&e.1==contact));
        let up=h.edges.last().unwrap().2;
        assert_eq!(h.moves.last().unwrap().1,up,"UP carries the delivered final direction");
    }
    #[test]
    fn buffered_direction_keeps_hold_and_requires_fresh_geometry() {
        let (mut c,mut h)=buffered_direction_fixture();c.skillKeyHeld_=true;c.pendingRelease_=false;
        c.tick(&mut h);assert!(h.edges.iter().all(|e|e.0!="up"));
        c.skillKeyHeld_=false;c.pendingRelease_=true;c.directionGeometryFresh_=false;
        c.tick(&mut h);assert!(h.edges.iter().all(|e|e.0!="up"));
    }
    #[test]
    fn physical_up_with_no_indicator_uses_current_button_and_does_not_block_next_press() {
        let (mut c,mut h)=buffered_direction_fixture();
        c.coreSkill_=0;c.directionGeometryFresh_=false;
        let b=&mut c.buttons_[0];b.available=true;b.center=Point::new(0.82589750665,0.7794586664);
        b.radius=Point::new(0.1072924943,0.1379524169);
        c.queuedSkills_.push(QueuedSkill{slot:b.slot.clone(),key:87,held:true,requestedNs:c.now_,..QueuedSkill::default()});
        c.tick(&mut h);
        assert_eq!(c.skillTouch_,-1);assert_eq!(c.queuedSkills_.len(),1);
        assert!(h.traces.iter().any(|(_,reason)|reason=="final_geometry_from_fresh_button"));
        assert!(h.traces.iter().any(|(event,reason)|event=="release"&&reason=="cast"));
        assert_eq!(h.edges.last().unwrap().0,"up");
    }
    #[test]
    fn absent_indicator_fallback_rejects_stale_layout_and_changed_range_form() {
        for changed_form in [false,true] {
            let (mut c,mut h)=buffered_direction_fixture();c.coreSkill_=0;c.directionGeometryFresh_=false;
            c.buttons_[0].available=true;c.buttons_[0].center=Point::new(0.8,0.8);c.buttons_[0].radius=Point::new(0.1,0.1);
            if changed_form {c.buttons_[0].aimType=3;}else{c.lastFrameNs_=c.now_-101_000_000;}
            c.tick(&mut h);assert!(h.edges.iter().all(|edge|edge.0!="up"));
        }
    }
    #[test]
    fn direction_transport_failure_does_not_commit_a_cast() {
        let (mut c,mut h)=buffered_direction_fixture();h.fail_move=true;
        c.tick(&mut h);
        assert!(!h.traces.iter().any(|(event,reason)|event=="release"&&reason=="cast"));
        assert!(!h.traces.iter().any(|(_,reason)|reason=="physical_up_after_final_direction_move"));
    }
    #[test]
    fn instant_direction_still_requires_a_game_receipt() {
        let (mut c,mut h)=buffered_direction_fixture();c.skillCastMode_=1;
        c.tick(&mut h);assert!(h.edges.iter().all(|e|e.0!="up"));
    }
    #[test]
    fn direction_ack_requires_current_raw_provenance_and_post_goal_snapshot() {
        let (mut c, mut f, mut h) = setup();
        c.androidSize_ = Point::new(1000., 1000.);
        c.activeSlot_ = "skill:1".into();
        c.buttons_.push(Button {
            slot: c.activeSlot_.clone(),
            skillId: 2520,
            ..Button::default()
        });
        c.coreSkill_ = 2520;
        c.directionGeometryFresh_ = true;
        c.directionCenter_ = Point::new(0.5, 0.5);
        c.directionRadius_ = Point::new(0.1, 0.1);
        c.frameCamera_ = [1., 0., 0., 1.];
        c.now_ = 1_980_000_000;
        c.record_skill_touch(Point::new(0.6005, 0.5005), false, true);
        c.directionGoalSequence_ = c.touchSequence_;
        c.directionGoalTime_ = c.now_;
        c.now_ = 2_000_000_000;
        let mut model = AngularRadialSolver::default();
        assert!(model.configure(
            Point::new(500., 500.),
            100.,
            c.frameCamera_,
            1.,
            c.androidSize_
        ));
        let observed = model.forward(Point::new(600., 500.)).unwrap();
        let observed = observed / observed.length();
        c.pendingRelease_ = true;
        c.pendingDirectionValid_ = true;
        c.pendingDirection_ = observed;
        let input = &mut f.direction_input;
        input.ready = flag(true);
        input.coherent = flag(true);
        input.source = text("IndicatorMotionRot.m_vDragResult");
        input.token = text("token");
        input.hero_id = n(25.);
        input.slot = text("skill:1");
        input.skill_id = n(2520.);
        input.sample_start_ns = n(1_990_000_000.);
        input.sample_end_ns = n(1_995_000_000.);
        input.raw_touch_xyz = v3(599.4, 500.5, 0.);
        input.direction_xz = v2(observed.x, observed.y);
        f.active_joystick.sample_start_ns = input.sample_start_ns;
        f.active_joystick.sample_end_ns = input.sample_end_ns;
        c.sample_direction_input(&mut h, &f);
        assert!(c.directionAccepted_);
        assert!(c.direction_acknowledged());
        c.skillPoint_ = Point::new(0.6005, 0.5005);
        c.directionGoalSequence_ = 0;
        c.record_skill_touch(c.skillPoint_, true, false); // Echo after the receipt.
        let acknowledged_sequence = c.touchSequence_;
        assert!(c.send_direction(&mut h, Point::new(600., 500.)));
        assert!(h.moves.is_empty(), "identical release must not enqueue another MOVE");
        assert_eq!(c.directionGoalSequence_, acknowledged_sequence);
        assert!(c.direction_acknowledged());
        c.directionGoalTime_ = 1_996_000_000;
        assert!(
            !c.direction_acknowledged(),
            "pre-goal feedback cannot release"
        );
        c.directionGoalTime_ = 1_980_000_000;
        f.direction_input.token = text("other");
        c.sample_direction_input(&mut h, &f);
        assert!(!c.directionAccepted_);
        f.direction_input.token = text("token");
        f.direction_input.raw_touch_xyz = v3(620., 500.5, 0.);
        c.sample_direction_input(&mut h, &f);
        assert!(
            !c.directionAccepted_,
            "plausible direction without sent-input receipt is not an ACK"
        );
        f.direction_input.raw_touch_xyz = v3(599.4, 500.5, 0.);
        c.sample_direction_input(&mut h, &f);
        assert!(c.directionAccepted_);
        c.directionGoalSequence_ = 0;
        assert!(c.send_direction(&mut h, Point::new(601., 500.)));
        assert!(!h.moves.is_empty());
        assert!(c.directionGoalSequence_ > acknowledged_sequence);
        assert!(!c.direction_acknowledged(), "new command still requires its own receipt");
    }

    #[test]
    fn stationary_native_geometry_mismatch_pulses_once_after_stable_evidence() {
        let (mut c, mut f, mut h) = setup();
        c.select_logical_coordinates(&mut h, &f);
        c.configure_direct(&mut h, &f);
        c.pendingRelease_ = true;
        let sent = cell(c.pixels(c.skillPoint_));
        f.logical.raw_touch_xyz = v3(
            sent.x * (c.androidSize_.x - 1.) / c.androidSize_.x,
            c.androidSize_.y - sent.y * (c.androidSize_.y - 1.) / c.androidSize_.y,
            0.,
        );
        c.update_range_drag_arming(&mut h, &f);
        assert!(h.moves.is_empty());
        c.now_ += 33_000_000;
        f.logical.sample_end_ns = n(f.logical.sample_end_ns.value + 33e6);
        c.update_range_drag_arming(&mut h, &f);
        assert!(c.qDragArming_ && c.qDragRecomputeUsed_ && c.qDragRecompute_);
        assert_eq!(h.moves.len(), 1);
        let arm = cell(c.pixels(c.qDragArmPoint_));
        f.logical.raw_touch_xyz = v3(
            arm.x * (c.androidSize_.x - 1.) / c.androidSize_.x,
            c.androidSize_.y - arm.y * (c.androidSize_.y - 1.) / c.androidSize_.y,
            0.,
        );
        f.logical.sample_start_ns = n(c.qDragArmSentNs_ as f64);
        c.update_range_drag_arming(&mut h, &f);
        assert!(!c.qDragArming_);
        c.now_ += 100_000_000;
        c.update_range_drag_arming(&mut h, &f);
        assert_eq!(
            h.moves.len(),
            1,
            "recompute is bounded to one pulse per cast"
        );
    }

    #[test]
    fn direct_release_requires_same_goal_receipt_and_fresh_observation() {
        let (mut c, f, mut h) = setup();
        c.select_logical_coordinates(&mut h, &f);
        c.configure_direct(&mut h, &f);
        c.acceptedInput_ = true;
        c.acceptedSampleNs_ = 1_995_000_000;
        c.acceptedPoint_ = c.direct_.predict(c.logicalObserved_).unwrap();
        assert!(c.direct_converged(c.logicalObserved_));
        c.pendingRelease_ = true;
        c.pendingWorldValid_ = true;
        c.pendingGoalSequence_ = 2;
        c.traceMatchedSequence_ = 2;
        c.pendingGoalTime_ = 1_980_000_000;
        c.traceSampleStart_ = 1_990_000_000;
        c.pendingGoalSource_ = c.logicalSource_.clone();
        c.pendingGoalSkill_ = c.aimSkillId_;
        assert!(c.pending_direct_acknowledged());
        c.traceMatchedSequence_ = 1;
        assert!(!c.pending_direct_acknowledged());
        c.traceMatchedSequence_ = 2;
        c.logicalSampleFresh_ = false;
        assert!(!c.pending_direct_acknowledged());
        c.logicalSampleFresh_ = true;
        c.now_ += 101_000_000;
        assert!(!c.pending_direct_acknowledged());
        assert!(!c.direct_converged(c.logicalObserved_));
    }

    #[test]
    fn frozen_world_resolver_is_used_once_and_relative_feedback_can_expire() {
        let (mut c, f, mut h) = setup();
        c.select_logical_coordinates(&mut h, &f);
        c.pendingRelease_ = true;
        c.pendingWorldResolver_ = 7;
        h.world = Some(Point::new(12., 23.));
        assert_eq!(c.desired_solver_world(&mut h), Some(Point::new(2., 3.)));
        assert_eq!(h.released, [7]);
        h.world = Some(Point::new(100., 200.));
        assert_eq!(c.desired_solver_world(&mut h), Some(Point::new(2., 3.)));
        assert_eq!(h.released, [7]);
        c.now_ += 251_000_000;
        assert!(c.desired_solver_world(&mut h).is_none());
    }
    #[test]
    fn verified_native_templates_classify_unrelated_heroes_without_id_branches() {
        let mut c=Controller::default();
        for hero in [2,39,999] {
            c.heroId_=hero;
            let mut b=Button {slot:"skill:1".into(),kind:"skill".into(),skillId:12345,aimType:1,..Default::default()};
            assert_eq!(c.classify_skill(&b),SkillBehavior::Tap);
            b.aimType=2;
            assert_eq!(c.classify_skill(&b),SkillBehavior::Direction);
            assert_eq!(c.measured_direction_skill_id(&b),12345);
            b.aimType=3;
            assert_eq!(c.classify_skill(&b),SkillBehavior::Range);
            b.aimType=0;
            assert_eq!(c.measured_direction_skill_id(&b),0);
        }
    }

}
