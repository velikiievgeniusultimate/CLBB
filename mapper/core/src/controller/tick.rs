use super::*;
impl Controller {
    pub(crate) fn tick(&mut self, host: &mut impl Host) {
        if !self.enabled_ || !self.ready_ {
            return;
        }
        if self.lastFrameNs_ <= 0 || self.now_ - self.lastFrameNs_ > 250_000_000 {
            self.invalidate(host, "F7: данные игры устарели — касания отпущены");
            return;
        }
        self.activate_movement_hold(host);
        if self.movementHeld_ || self.movementAuto_ {
            self.request_pointer_flush(host)
        }
        self.dispatch_queued_skill(host);
        if self.skillTouch_ < 0 {
            return;
        }
        if self.skillCancelPending_ {
            if self.skillCancelAcknowledged_ {
                self.finish_skill(host, true);
                return;
            }
            if self.abort_retribution_touch(host) {
                self.finish_skill(host, true);
                return;
            }
            if self.skillCancelSentNs_ == 0 && self.cancelReady_ {
                if !self.move_skill(host, self.cancelPoint_, false) {
                    self.finish_skill(host, true);
                    return;
                }
                self.skillCancelSentNs_ = self.now_;
            }
            if self.skillCancelSentNs_ != 0 && self.now_ - self.skillCancelSentNs_ > 1_500_000_000 {
                self.set_status(
                    "F7: игра ещё не подтвердила отмену — палец остаётся на кнопке отмены",
                )
            }
            return;
        }
        if self.pendingRelease_
            && self.releaseRequested_.valid()
            && self.releaseRequested_.elapsed(self.now_) >= 1500
        {
            self.finish_skill(host, true);
            self.set_status(
                "F7: не удалось подтвердить наведение — отложенное применение отменено",
            );
            return;
        }
        let Some(b) = self.find_button(&self.activeSlot_).cloned() else {
            self.finish_skill(host, true);
            return;
        };
        if self.skillBehavior_ == SkillBehavior::AwaitVariant {
            self.skillBehavior_ = self.classify_skill(&b);
            if self.skillBehavior_ == SkillBehavior::AwaitVariant {
                if self.skillStarted_.elapsed(self.now_) >= 500 {
                    self.finish_skill(host, true);
                    self.set_status("F7: вариант навыка не подтверждён — действие отменено")
                }
                return;
            }
            if b.key != self.skillKey_ {
                self.finish_skill(host, true);
                return;
            }
            self.skillCastMode_ = b.castMode;
            self.activeConfigSlot_ = self.current_config_slot(&b);
            self.skillBindingVariant_ = self.active_variant(&b);
            self.skillCooldownVariant_ = self.cooldown_variant(&b);
            if b.stopMovement {
                self.finish_movement(host, "")
            }
            if self.skillBehavior_ == SkillBehavior::Direction
                && self.skillCastMode_ == 1
                && !self.pendingRelease_
            {
                self.freeze_direction_intent(host)
            }
            if self.skillBehavior_ == SkillBehavior::Tap {
                self.pendingRelease_ = false
            }
        }
        let variant = self.active_variant(&b);
        if self.heroId_ == 25
            && b.skillId == 2520
            && self.skillBindingVariant_ != 0
            && variant != 0
            && variant != self.skillBindingVariant_
        {
            self.finish_skill(host, true);
            return;
        }
        if self.skillBehavior_ == SkillBehavior::Tap {
            if self.skillCastMode_ == 0 && self.skillKeyHeld_ {
                return;
            }
            let elapsed = self.skillStarted_.elapsed(self.now_);
            if elapsed >= 16 {
                self.finish_skill(host, false)
            } else {
                host.timer(Timer::Tap, Some(16 - elapsed))
            }
            return;
        }
        if self.skillBehavior_ == SkillBehavior::Direction {
            self.skillExpectedAim_ = true;
            if let Some(p) = self.direction_point(host) {
                if !self.send_direction(host, p) {
                    self.finish_skill(host, true);
                    return;
                }
                if self.direction_acknowledged() {
                    self.trace(host, "direction_goal", "native_direction_ack_release");
                    self.finish_skill(host, false);
                    return;
                }
                if self.pendingRelease_
                    && self.directionGoalSequence_ != 0
                    && self.now_ - self.directionLastEchoNs_ >= 32_000_000
                {
                    if !host.move_touch(self.skillTouch_, self.skillPoint_) {
                        self.finish_skill(host, true);
                        return;
                    }
                    self.sync_clock(host);
                    self.record_skill_touch(self.skillPoint_, true, false);
                    self.directionLastEchoNs_ = self
                        .causal_
                        .latest()
                        .map(|s| s.time_ns)
                        .unwrap_or(self.now_);
                    self.trace(host, "touch_echo", "direction_goal_keepalive")
                }
            }
            return;
        }
        let matching = self.frameActiveSlot_ == self.activeSlot_;
        let center = if matching && self.joystickReady_ {
            self.joystickCenter_
        } else {
            b.center
        };
        let radius = if matching && self.joystickReady_ {
            self.joystickRadius_
        } else {
            b.radius
        };
        let elapsed = self.skillStarted_.elapsed(self.now_);
        self.skillExpectedAim_ = self.skillExpectedAim_
            || (matching
                && (self.aimReady_
                    || self.joystickMeasured_
                    || (b.kind == "skill" && self.joystickReady_)));
        let center_px = self.pixels(center);
        let radius_px = self.radius_pixels(radius);
        if self.provisionalStartup_ && !self.verified_direct_class() {
            if elapsed < 150
                && !(matching && self.aimReady_ && self.aimSkillId_ != self.provisionalSkill_)
            {
                return;
            }
            self.provisionalStartup_ = false;
            self.skillNudged_ = false;
        }
        if self.verified_direct_class() {
            if self.qDragArming_ {
                return;
            }
            if self.direct_.ready != 0 {
                self.skillNudged_ = true;
                if let Some(desired) = self.desired_solver_world(host) {
                    if self.pending_direct_acknowledged() {
                        let p = self.direct_.predict(desired);
                        if p.is_none() || !self.move_skill(host, self.skill_normalized(p.unwrap()), true)
                        {
                            self.finish_skill(host, true);
                            return;
                        }
                        self.trace(host, "pending_goal", "acknowledged_reproject_and_release");
                        self.finish_skill(host, false);
                        return;
                    }
                    self.traceDesired_ = desired;
                    self.traceDesiredValid_ = true;
                    self.request_pointer_flush(host);
                    self.trace(
                        host,
                        "direct_prediction",
                        "camera_angle_and_radial_fraction",
                    );
                    if self.skillCastMode_ == 1 && self.direct_converged(desired) {
                        self.finish_skill(host, false);
                        return;
                    }
                }
            } else if !self.skillNudged_ && matching && self.directGeometryFresh_ {
                let unity = self.pixels(center);
                let physical = Point::new(
                    unity.x * self.androidSize_.x / (self.androidSize_.x - 1.),
                    unity.y * self.androidSize_.y / (self.androidSize_.y - 1.),
                );
                if !self.move_skill(host, self.skill_normalized(physical + Point::new(1., 0.)), true) {
                    self.finish_skill(host, true);
                    return;
                }
                self.skillNudged_ = true;
                self.skillMoved_.restart(self.now_);
                self.set_status("F7: ожидаю измеренную дальность навыка")
            }
            if self.skillCastMode_ == 1 && elapsed >= 900 {
                self.finish_skill(host, true);
                self.set_status("F7: наведение не подтверждено — навык отменён")
            }
            return;
        }
        if self.skillNudged_
            && self.solverRadius_ > 0.
            && (distance(center_px, self.solverCenter_) > 3.
                || (radius_px - self.solverRadius_).abs() > self.solverRadius_ * 0.03)
        {
            self.trace(host, "geometry_reset", "center_or_radius_changed");
            self.discard_active_models();
            self.reset_accepted_input();
            self.record_skill_touch(self.skillPoint_, false, false);
            self.castStartedNs_ = self.now_;
            self.solver_.reset();
            self.skillNudged_ = false;
            self.armingDistance_ = 0.;
            self.warmAttempted_ = false;
        }
        if !self.skillNudged_
            && matching
            && (b.kind == "skill" || self.aimReady_ || self.joystickMeasured_)
        {
            if !self.move_skill(
                host,
                self.skill_normalized(self.pixels(center) + Point::new(1., 0.)),
                true,
            ) {
                self.finish_skill(host, true);
                return;
            }
            self.skillNudged_ = true;
            self.armingDistance_ = 1.;
            self.solverCenter_ = center_px;
            self.solverRadius_ = radius_px;
            self.skillMoved_.restart(self.now_);
            return;
        }
        if self.skillNudged_
            && matching
            && !self.aimReady_
            && !self.solver_.learned()
            && self.skillMoved_.valid()
            && self.skillMoved_.elapsed(self.now_) >= 60
            && self.armingDistance_ < radius_px * 0.9
        {
            let pointer = if self.pendingRelease_ {
                self.pendingPointer_
            } else {
                self.pointer_
            };
            let mut direction = host.movement_direction(pointer).unwrap_or_default();
            let mut size = direction.length();
            if !finite(direction) || size < 1e-7 {
                direction = self.pixels(pointer - self.heroScreen_);
                size = direction.length()
            }
            if !finite(direction) || size < 1e-7 {
                direction = Point::new(1., 0.);
                size = 1.
            }
            self.armingDistance_ =
                (radius_px * 0.9).min((radius_px * 0.08).max(self.armingDistance_ * 2.));
            if self.move_skill(
                host,
                self.skill_normalized(center_px + direction * (self.armingDistance_ / size)),
                true,
            ) {
                self.solver_.reset();
                self.warmAttempted_ = false;
                self.skillMoved_.restart(self.now_);
            }
        }
        if self.skillNudged_ && self.aimReady_ && matching {
            if self.observedSkillId_ != 0 && self.observedSkillId_ != self.aimSkillId_ {
                self.reset_accepted_input();
                self.castStartedNs_ = self.now_;
                self.solver_.reset();
                self.warmAttempted_ = false;
                self.skillNudged_ = false;
                self.observedSkillId_ = self.aimSkillId_;
                return;
            }
            self.observedSkillId_ = self.aimSkillId_;
            if let Some(desired) = self.desired_solver_world(host) {
                self.traceDesired_ = desired;
                self.traceDesiredValid_ = true;
                let observed = self.solver_observed();
                if !self.acceptedInput_ || self.acceptedSampleNs_ <= self.consumedSampleNs_ {
                    self.solver_.step(
                        self.pixels(self.skillPoint_),
                        observed,
                        desired,
                        center_px,
                        radius_px,
                        false,
                        false,
                    );
                    self.trace(host, "feedback_rejected", "unconfirmed_pair");
                    if self.solver_.failed != 0 {
                        self.set_status("F7: отклик навыка не распознан — удержание сохранено")
                    } else if !self.solver_.learned() {
                        self.set_status("F7: ожидаю подтверждённую пару касания и прицела")
                    }
                    if self.skillCastMode_ == 1 && elapsed >= 900 {
                        self.finish_skill(host, true);
                        self.set_status("F7: прицел не совпал с курсором — навык отменён")
                    }
                    return;
                }
                self.consumedSampleNs_ = self.acceptedSampleNs_;
                if !self.warmAttempted_ && self.joystickMeasured_ {
                    self.warmAttempted_ = true;
                    let cached = self.find_model(self.aimSkillId_, center_px, radius_px);
                    if let Some(i) = cached {
                        self.solver_.resume_from(&self.responseModels_[i].solver);
                    }
                    self.trace(
                        host,
                        "model_cache",
                        if cached.is_some() { "hit" } else { "miss" },
                    )
                }
                let was_warm = self.solver_.warm != 0;
                self.trace(host, "feedback_before", "accepted_pair");
                let mut next = self.solver_.step(
                    self.acceptedPoint_,
                    observed,
                    desired,
                    center_px,
                    radius_px,
                    true,
                    self.acceptedAdapt_,
                );
                if self.solver_.learned() {
                    if let Some(p) = self.solver_.predict(desired, center_px, radius_px) {
                        next = Some(p)
                    }
                }
                if was_warm && self.solver_.warm == 0 {
                    self.discard_active_models()
                }
                self.trace(host, "feedback_after", "accepted_pair");
                if self.solver_.learned() {
                    self.request_pointer_flush(host)
                } else if let Some(p) = next {
                    if self.move_skill(host, self.skill_normalized(p), true) {
                        self.skillMoved_.restart(self.now_)
                    }
                }
                if self.solver_.learned()
                    && self.joystickMeasured_
                    && distance(observed, desired) <= 0.03
                {
                    self.save_model(self.aimSkillId_, center_px, radius_px)
                }
                if self.solver_.failed != 0 {
                    self.set_status("F7: отклик навыка не распознан — удержание сохранено")
                }
                if self.pendingRelease_
                    && self.solver_.learned()
                    && distance(observed, desired) <= 0.03
                {
                    self.finish_skill(host, false);
                    return;
                }
                if self.skillCastMode_ == 1
                    && self.solver_.learned()
                    && distance(observed, desired) < 0.03
                {
                    self.finish_skill(host, false);
                    return;
                }
            }
        }
        let active_aim = matching && self.aimReady_;
        if self.skillCastMode_ == 1
            && elapsed >= 180
            && !self.skillExpectedAim_
            && !active_aim
            && !self.joystickMeasured_
            && (b.kind != "skill" || !self.joystickReady_)
        {
            self.finish_skill(host, false);
            return;
        }
        if self.skillCastMode_ == 1 && elapsed >= 900 {
            self.finish_skill(host, true);
            self.set_status("F7: прицел не совпал с курсором — навык отменён")
        }
    }
}
