//! Thin legacy-field bridge: CausalInput owns the sole command ring and all
//! admission/provenance state; Controller retains its public trace/goal scalars.
use crate::Point;
use crate::controller::causal::{CausalContext, MotionSample};
use crate::controller::{Controller, ResponseModel, distance, host::Host, snapshot::Frame};

impl Controller {
    pub(crate) fn reset_accepted_input(&mut self) {
        self.causal_.reset();
        self.touchSequence_ = 0;
        self.lastInputPacketNs_ = 0;
        self.acceptedSampleNs_ = 0;
        self.consumedSampleNs_ = 0;
        self.pendingGoalSequence_ = 0;
        self.directionGoalSequence_ = 0;
        self.directionAccepted_ = false;
        self.directionLastEchoNs_ = 0;
        self.acceptedInput_ = false;
        self.acceptedAdapt_ = false;
    }
    pub(crate) fn record_skill_touch(&mut self, point: Point, echo: bool, final_goal: bool) {
        self.causal_
            .record(point, self.androidSize_, self.now_, echo, final_goal);
        self.touchSequence_ = self.causal_.sequence();
    }
    pub(crate) fn sample_accepted_input(&mut self, frame: &Frame, host: &mut impl Host) {
        let sample = if self.logicalMode_ {
            let v = &frame.logical;
            MotionSample {
                source_verified: true,
                available: true,
                bracket_stable: true,
                raw_shape_valid: v.raw_touch_xyz.count == 3,
                raw_numbers_valid: v.raw_touch_xyz.values.iter().all(|n| n.is_number()),
                raw: v.raw_touch_xyz.values.map(|n| n.get(0.)),
                timestamps_numeric: v.sample_start_ns.is_number() && v.sample_end_ns.is_number(),
                start_ns: v.sample_start_ns.get(-1.),
                end_ns: v.sample_end_ns.get(-1.),
            }
        } else {
            let v = &frame.motion;
            MotionSample {
                source_verified: v.source.as_str() == "IndicatorMotionBase",
                available: v.available.get(),
                bracket_stable: v.bracket_stable.get(),
                raw_shape_valid: v.drag_touch_raw_xyz.count == 3,
                raw_numbers_valid: v.drag_touch_raw_xyz.values.iter().all(|n| n.is_number()),
                raw: v.drag_touch_raw_xyz.values.map(|n| n.get(0.)),
                timestamps_numeric: v.sample_start_ns.is_number() && v.sample_end_ns.is_number(),
                start_ns: v.sample_start_ns.get(-1.),
                end_ns: v.sample_end_ns.get(-1.),
            }
        };
        let context = CausalContext {
            now_ns: self.now_,
            cast_start_ns: self.castStartedNs_,
            has_touch: self.skillTouch_ >= 0,
            aim_ready: self.aimReady_,
            slots_match: self.frameActiveSlot_ == self.activeSlot_,
            core_skill_matches: frame.skill_id.int() == self.aimSkillId_,
            aim_skill_id: self.aimSkillId_,
            q_drag_arming: self.qDragArming_,
            logical_mode: self.logicalMode_,
            logical_fresh: self.logicalSampleFresh_,
            verified_direct_class: self.verified_direct_class(),
            direct_geometry_fresh: self.directGeometryFresh_,
            resolution: self.androidSize_,
            center: if self.joystickReady_ {
                self.pixels(self.joystickCenter_)
            } else {
                self.solverCenter_
            },
            radius: if self.joystickReady_ {
                self.radius_pixels(self.joystickRadius_)
            } else {
                self.solverRadius_
            },
            solver_center: self.solverCenter_,
            observed: self.solver_observed(),
            hero: host.hero_world(),
            solver: &self.solver_,
            direct: &self.direct_,
        };
        let result = self.causal_.sample(&context, &sample);
        self.acceptedInput_ = result.admitted;
        self.acceptedAdapt_ = result.adapt;
        self.inputReason_ = result.reason.into();
        self.traceMatchedSequence_ = result.trace.matched_sequence;
        self.traceSampleStart_ = result.trace.sample_start_ns;
        self.traceSampleEnd_ = result.trace.sample_end_ns;
        self.traceRaw_ = result.trace.raw_top_down;
        self.traceDecoded_ = result.trace.decoded;
        if result.admitted {
            self.acceptedPoint_ = result.point;
            self.acceptedSampleNs_ = result.sample_ns;
        }
        self.trace(
            host,
            if result.admitted {
                "input_admitted"
            } else {
                "input_rejected"
            },
            result.reason,
        );
    }
    pub(crate) fn find_model(
        &mut self,
        skill_id: i32,
        center: Point,
        radius: f64,
    ) -> Option<usize> {
        let source = if self.logicalMode_ {
            self.logicalSource_.as_str()
        } else {
            ""
        };
        for i in 0..self.responseModels_.len() {
            let m = &self.responseModels_[i];
            if m.heroId != self.heroId_
                || m.slot != self.activeSlot_
                || m.skillId != skill_id
                || m.resolution != self.androidSize_
                || m.relative != self.logicalMode_
                || m.source != source
            {
                continue;
            }
            if distance(m.center, center) > 3. || (m.radius - radius).abs() > radius * 0.03 {
                self.responseModels_.remove(i);
                return None;
            }
            self.modelClock_ += 1;
            self.responseModels_[i].used = self.modelClock_;
            return Some(i);
        }
        None
    }
    pub(crate) fn discard_active_models(&mut self) {
        self.responseModels_
            .retain(|m| m.heroId != self.heroId_ || m.slot != self.activeSlot_);
    }
    pub(crate) fn save_model(&mut self, skill_id: i32, center: Point, radius: f64) {
        if let Some(i) = self.find_model(skill_id, center, radius) {
            self.responseModels_[i].solver.resume_from(&self.solver_);
            return;
        }
        if self.responseModels_.len() >= 32 {
            let oldest = self
                .responseModels_
                .iter()
                .enumerate()
                .min_by_key(|(_, m)| m.used)
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.responseModels_.remove(oldest);
        }
        self.modelClock_ += 1;
        let mut model = ResponseModel {
            heroId: self.heroId_,
            slot: self.activeSlot_.clone(),
            skillId: skill_id,
            relative: self.logicalMode_,
            source: if self.logicalMode_ {
                self.logicalSource_.clone()
            } else {
                String::new()
            },
            resolution: self.androidSize_,
            center,
            radius,
            used: self.modelClock_,
            ..ResponseModel::default()
        };
        if model.solver.resume_from(&self.solver_) {
            self.responseModels_.push(model);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AimSolver;
    fn ready_model() -> AimSolver {
        AimSolver {
            stage: 5,
            anchor_valid: 1,
            j: [0.1, 0., 0., 0.1],
            initial_j: [0.1, 0., 0., 0.1],
            ..AimSolver::default()
        }
    }
    fn controller() -> Controller {
        let mut c = Controller::new();
        c.heroId_ = 25;
        c.activeSlot_ = "skill:0".into();
        c.logicalMode_ = true;
        c.logicalSource_ = "IndicatorMotionFaTiaoQiu2.m_vDragResult".into();
        c.androidSize_ = Point::new(4000., 3111.);
        c.solver_ = ready_model();
        c
    }
    #[test]
    fn response_cache_source_resolution_and_geometry_are_exact_identity() {
        let mut c = controller();
        let center = Point::new(3075., 2718.);
        c.save_model(2511, center, 370.);
        assert!(c.find_model(2511, center, 370.).is_some());
        c.logicalSource_ = "IndicatorMotionRotAndPosRot.m_vDragResult".into();
        assert!(c.find_model(2511, center, 370.).is_none());
        c.logicalSource_ = "IndicatorMotionFaTiaoQiu2.m_vDragResult".into();
        c.androidSize_ = Point::new(2000., 1555.);
        assert!(c.find_model(2511, center, 370.).is_none());
        c.androidSize_ = Point::new(4000., 3111.);
        assert!(
            c.find_model(2511, center + Point::new(4., 0.), 370.)
                .is_none()
        );
        assert!(c.responseModels_.is_empty());
    }
    #[test]
    fn response_cache_remains_bounded_and_cold_models_do_not_resume() {
        let mut c = controller();
        let center = Point::new(3075., 2718.);
        for id in 0..40 {
            c.save_model(id, center, 370.);
        }
        assert_eq!(c.responseModels_.len(), 32);
        assert!(c.find_model(0, center, 370.).is_none());
        assert!(c.find_model(39, center, 370.).is_some());
        c.discard_active_models();
        assert!(c.responseModels_.is_empty());
        c.solver_.reset();
        c.save_model(2511, center, 370.);
        assert!(c.responseModels_.is_empty());
    }
    #[test]
    fn accepted_reset_clears_pending_goal_references_and_history() {
        let mut c = controller();
        c.now_ = 1_000_000_000;
        c.record_skill_touch(Point::new(0.8, 0.8), false, false);
        c.pendingGoalSequence_ = c.touchSequence_;
        c.directionGoalSequence_ = c.touchSequence_;
        c.acceptedInput_ = true;
        c.acceptedAdapt_ = true;
        c.reset_accepted_input();
        assert_eq!(c.touchSequence_, 0);
        assert_eq!(c.causal_.history_len(), 0);
        assert_eq!(c.pendingGoalSequence_, 0);
        assert_eq!(c.directionGoalSequence_, 0);
        assert!(!c.acceptedInput_);
        assert!(!c.acceptedAdapt_);
    }
}
