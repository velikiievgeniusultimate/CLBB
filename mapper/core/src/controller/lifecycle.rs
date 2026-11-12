use super::*;
impl Controller {
    pub(crate) fn tap_only(b: &Button) -> bool {
        (b.kind == "upgrade"
            && (0..4).contains(&b.index)
            && b.slot == format!("upgrade:{}", b.index))
            || (b.kind == "quick_buy" && b.slot == "shop:quick")
            || (b.kind == "attack"
                && ["attack", "attack:minion", "attack:tower"].contains(&b.slot.as_str())
                && !(b.slot=="attack" && b.aimType==3))
            || (b.kind == "extra" && ["extra:0", "extra:1"].contains(&b.slot.as_str()))
    }
    pub(crate) fn is_retribution(b: &Button) -> bool {
        b.kind == "extra" && b.slot == "extra:2" && matches!(b.skillId, 20020 | 20150)
    }
    pub(crate) fn measured_direction_skill_id(&self, b: &Button) -> i32 {
        if b.aimType == 2 {
            let variant = self.active_variant(b);
            if variant > 0 { variant } else { b.skillId }
        } else if Self::is_retribution(b) {
            b.skillId
        } else if self.heroId_ == 25 && b.skillId == 2520 {
            2520
        } else {
            0
        }
    }
    pub(crate) fn active_variant(&self, b: &Button) -> i32 {
        if self.now_ - self.skillVariantSampleNs_ > 100_000_000 {
            return 0;
        }
        self.skillVariants_
            .iter()
            .find(|v| v.slot == b.slot && v.base == b.skillId)
            .map(|v| v.active)
            .unwrap_or(0)
    }
    pub(crate) fn current_config_slot(&self, b: &Button) -> String {
        let variant = self.active_variant(b);
        if b.kind == "skill" && variant > 0 && variant != b.skillId {
            format!("{}:{variant}", b.slot)
        } else { b.slot.clone() }
    }
    pub(crate) fn refresh_bindings(&mut self, host: &mut impl Host) {
        for i in 0..self.buttons_.len() {
            let config = self.current_config_slot(&self.buttons_[i]);
            let binding = host.binding(self.heroId_, &self.buttons_[i], &config);
            let b = &mut self.buttons_[i];
            b.key = binding.key;
            b.castMode = binding.cast_mode;
            b.stopMovement = binding.stop_movement;
        }
    }
    pub(crate) fn request_pointer_flush(&mut self, host: &mut impl Host) {
        if self.manualTouch_ >= 0
            || (self.ready() && (self.skillTouch_ >= 0 || self.movementHeld_ || self.movementAuto_))
        {
            host.timer(Timer::Pointer, Some(1));
        }
    }
    pub(crate) fn update_pointer(&mut self, host: &mut impl Host, p: Point) {
        if !finite(p) {
            return;
        }
        self.pointer_ = bounded(p);
        self.pointerUpdates_ += 1;
        self.request_pointer_flush(host)
    }
    pub(crate) fn move_skill(
        &mut self,
        host: &mut impl Host,
        mut next: Point,
        command_cell: bool,
    ) -> bool {
        if self.skillTouch_ < 0 || !finite(next) {
            return false;
        }
        if command_cell {
            // Only generated joystick commands can continue outside the view.
            // Native geometry is at most a fraction of a screen; reject a
            // corrupt producer rather than silently changing its direction.
            if !(-1. ..=2.).contains(&next.x) || !(-1. ..=2.).contains(&next.y) {
                return false;
            }
            let p = self.pixels(next);
            next = self.skill_normalized(Point {
                x: p.x.round() + 0.5,
                y: p.y.round() + 0.5,
            });
        } else {
            next = bounded(next);
        }
        if distance(self.skillPoint_, next) < 1e-8 {
            return true;
        }
        if !host.move_touch(self.skillTouch_, next) {
            return false;
        }
        self.sync_clock(host);
        self.skillPoint_ = next;
        self.record_skill_touch(next, false, false);
        self.trace(host, "touch_sent", "move");
        if command_cell {
            self.skillEchoPending_ = true;
            self.request_pointer_flush(host)
        }
        true
    }
    pub(crate) fn flush_pointer(&mut self, host: &mut impl Host) {
        host.timer(Timer::Pointer, None);
        if self.manualTouch_ >= 0
            && distance(self.manualPoint_, self.pointer_) >= 1e-8
            && host.move_touch(self.manualTouch_, self.pointer_)
        {
            self.manualPoint_ = self.pointer_;
        }
        if !self.ready() {
            return;
        }
        self.pointerFlushes_ += 1;
        let before = self.touchSequence_;
        if self.skillTouch_ >= 0
            && !self.skillCancelPending_
            && self.skillBehavior_ == SkillBehavior::Direction
        {
            if let Some(p) = self.direction_point(host) {
                if !self.send_direction(host, p) {
                    self.finish_skill(host, true);
                    return;
                }
            }
        }
        if self.skillTouch_>=0&&!self.skillCancelPending_&&self.skillBehavior_==SkillBehavior::Selection {
            if !self.send_selection(host){self.selection_transport_failed(host,"selection_pointer_move_delivery_failed");return;}
        }
        if self.skillTouch_ >= 0
            && !self.skillCancelPending_
            && !self.qDragArming_
            && self.skillBehavior_ == SkillBehavior::Range
            && self.aimReady_
            && self.frameActiveSlot_ == self.activeSlot_
        {
            if let Some(desired) = self.desired_solver_world(host) {
                self.traceDesired_ = desired;
                self.traceDesiredValid_ = true;
                let predicted = if self.verified_direct_class() {
                    self.direct_.predict(desired)
                } else {
                    self.solver_
                        .predict(desired, self.solverCenter_, self.solverRadius_)
                };
                if let Some(p) = predicted {
                    if self.move_skill(host, self.skill_normalized(p), true) {
                        self.skillMoved_.restart(self.now_);
                        if self.pendingRelease_
                            && self.verified_direct_class()
                            && self.direct_.ready != 0
                            && self.pendingGoalSequence_ == 0
                        {
                            if host.move_touch(self.skillTouch_, self.skillPoint_) {
                                self.sync_clock(host);
                                self.record_skill_touch(self.skillPoint_, false, true);
                                self.pendingGoalSequence_ = self.touchSequence_;
                                self.pendingGoalTime_ = self
                                    .causal_
                                    .latest()
                                    .map(|s| s.time_ns)
                                    .unwrap_or(self.now_);
                                self.pendingGoalSource_ = self.logicalSource_.clone();
                                self.pendingGoalSkill_ = self.aimSkillId_;
                                self.pendingGoalCenter_ = self.pixels(self.joystickCenter_);
                                self.pendingGoalRadius_ = self.radius_pixels(self.joystickRadius_);
                                self.pendingGoalRange_ = self.direct_.world_range;
                                self.trace(host, "pending_goal", "await_same_cast_command_ack");
                            } else {
                                self.finish_skill(host, true);
                                return;
                            }
                        }
                    }
                }
            }
        }
        if self.movementHeld_ || self.movementAuto_ {
            self.update_movement(host, false)
        }
        if self.skillTouch_ >= 0
            && !self.skillCancelPending_
            && self.skillEchoPending_
            &&(self.skillBehavior_!=SkillBehavior::Selection||self.selection_echo_allowed())
            && before == self.touchSequence_
        {
            self.skillEchoPending_ = false;
            if !host.move_touch(self.skillTouch_, self.skillPoint_) {
                if self.skillBehavior_ == SkillBehavior::Selection {
                    self.selection_transport_failed(host, "selection_pointer_echo_delivery_failed");
                } else {
                    self.finish_skill(host, true);
                }
                return;
            }
            self.sync_clock(host);
            self.record_skill_touch(self.skillPoint_, true, false);
            self.trace(host, "touch_echo", "zero_velocity");
        }
        self.trace(host, "pointer_flush", "latest_value");
    }
    pub(crate) fn cooldown_variant(&self, b: &Button) -> i32 {
        let variant = self.active_variant(b);
        if variant > 0 {
            return variant;
        }
        self.cooldownState_
            .buttons
            .iter()
            .take(self.cooldownState_.count.min(32) as usize)
            .find(|r| {
                r.slot.as_str() == b.slot
                    && r.base_skill_id.int() == b.skillId
                    && r.active_skill_id.int() > 0
            })
            .map(|r| r.active_skill_id.int())
            .unwrap_or(b.skillId)
    }
    pub(crate) fn cooldown_block(&self, b: &Button, minimum: i64) -> &'static str {
        if (b.kind != "skill" && b.kind != "extra") || !self.cooldownSeen_ {
            return "";
        }
        let s = &self.cooldownState_;
        let start = s.sample_start_ns.get(-1.);
        let end = s.sample_end_ns.get(-1.);
        let now = self.now_ as f64;
        if !s.ready.get()
            || !s.coherent.get()
            || s.source.as_str() != "VirtualButton.m_CdCoolDown"
            || s.token.as_str() != self.frameEpoch_
            || s.hero_id.int() != self.heroId_
            || !s.sample_start_ns.is_number()
            || !s.sample_end_ns.is_number()
            || !start.is_finite()
            || !end.is_finite()
            || end < start
            || end - start > 100e6
            || end > now + 10e6
        {
            return "unknown";
        }
        if minimum != 0 && start < (minimum as f64) {
            return "await_post_release_snapshot";
        }
        if now - end > 100e6 {
            return "stale";
        }
        let mut rows = s
            .buttons
            .iter()
            .take(s.count.min(32) as usize)
            .filter(|r| r.slot.as_str() == b.slot);
        let Some(r) = rows.next() else {
            return "unknown";
        };
        if rows.next().is_some()
            || !r.ready.get()
            || !r.on_cooldown.is_bool()
            || !r.base_skill_id.is_number()
            || r.base_skill_id.value != b.skillId as f64
            || !r.active_skill_id.is_number()
            || r.active_skill_id.value != r.active_skill_id.int() as f64
            || r.active_skill_id.int() <= 0
            || r.active_skill_id.int() > MAX_SKILL_ID
        {
            return "unknown";
        }
        let variant = self.active_variant(b);
        if variant > 0 && variant != r.active_skill_id.int() {
            return "variant_unknown";
        }
        if start
            < *self
                .cooldownCasts_
                .get(&format!("{}:{}", b.slot, r.active_skill_id.int()))
                .unwrap_or(&0) as f64
        {
            return "pre_cast_snapshot";
        }
        if r.on_cooldown.get() { "active" } else { "" }
    }
    pub(crate) fn trace_cooldown_ignored(
        &mut self,
        host: &mut impl Host,
        b: &Button,
        key: i32,
        reason: &str,
    ) {
        self.cooldownTraceSlot_ = b.slot.clone();
        self.cooldownTraceKey_ = key;
        self.cooldownTraceVariant_ = self.cooldown_variant(b);
        self.trace(host, "cooldown_ignored", reason)
    }
    pub(crate) fn mark_cooldown_cast(&mut self, slot: &str, variant: i32) {
        if !slot.is_empty() && variant > 0 {
            self.cooldownCasts_
                .insert(format!("{slot}:{variant}"), self.now_);
        }
    }
    pub(crate) fn capture_skill_intent(&self, host: &mut impl Host) -> SkillIntent {
        let mut r = SkillIntent {
            pointer: self.pointer_,
            worldResolver: host.capture_pointer_world(self.pointer_),
            ..SkillIntent::default()
        };
        if r.worldResolver == 0 {
            if let Some(w) = host.pointer_world(self.pointer_).filter(|p| finite(*p)) {
                r.world = w;
                r.worldValid = true
            }
        }
        if self.frameCameraReady_ {
            if let Some(w) = host.ground_world(self.pointer_).filter(|p| finite(*p)) {
                let delta = w - self.frameHero_;
                let length = delta.length();
                if length > 1e-8 {
                    r.direction = delta / length;
                    r.directionValid = true
                }
            }
        }
        if self.selectionHeroSampleNs_>0&&self.now_-self.selectionHeroSampleNs_<=100_000_000 {
            let d=self.pixels(self.pointer_-self.selectionHeroScreen_);let n=d.length();
            if finite(d)&&n>1e-8 {r.screenDirection=d/n;r.screenDirectionValid=true;}
        }
        r
    }
    pub(crate) fn drop_intent(host: &mut impl Host, intent: &SkillIntent) {
        if intent.worldResolver != 0 {
            host.release_resolver(intent.worldResolver)
        }
    }
    pub(crate) fn apply_skill_intent(&mut self, host: &mut impl Host, intent: SkillIntent) {
        if self.skillBehavior_ == SkillBehavior::Tap {
            Self::drop_intent(host, &intent);
            return;
        }
        if self.skillBehavior_==SkillBehavior::Selection {
            let pointer=self.pointer_;self.pointer_=intent.pointer;
            self.freeze_selection_intent(host);self.pointer_=pointer;
            self.selection_.frozen_direction=intent.screenDirectionValid.then_some(intent.screenDirection);
            Self::drop_intent(host,&intent);
            return;
        }
        self.pendingRelease_ = true;
        self.pendingPointer_ = intent.pointer;
        self.pendingGoalSequence_ = 0;
        self.directionGoalSequence_ = 0;
        self.pendingWorld_ = intent.world;
        self.pendingWorldValid_ = intent.worldValid;
        if self.pendingWorldResolver_ != 0 {
            host.release_resolver(self.pendingWorldResolver_)
        }
        self.pendingWorldResolver_ = intent.worldResolver;
        self.pendingDirection_ = intent.direction;
        self.pendingDirectionValid_ = intent.directionValid;
        self.releaseRequested_.restart(self.now_);
    }
    pub(crate) fn clear_queued_skills(&mut self, host: &mut impl Host, reason: &str) {
        if self.queuedSkills_.is_empty() {
            return;
        }
        self.trace(host, "skill_queue", reason);
        for request in self.queuedSkills_.drain(..) {
            Self::drop_intent(host, &request.down);
            Self::drop_intent(host, &request.up);
        }
    }
    fn kagura_w_cast_lock(&self) -> Option<bool> {
        if self.skillVariantSampleNs_ == 0 || self.now_ - self.skillVariantSampleNs_ > 100_000_000 {
            return None;
        }
        self.skillVariants_
            .iter()
            .find(|v| v.slot == "skill:1" && v.base == 2520 && matches!(v.active, 2520 | 2521))
            .and_then(|v| v.cast_locked)
    }
    pub(crate) fn waiting_for_kagura_w(&self, b: &Button) -> bool {
        self.heroId_ == 25 && b.slot == "skill:1" && b.skillId == 2520 && self.kaguraThrow_.is_some()
    }
    pub(crate) fn observe_kagura_throw(&mut self, host: &mut impl Host) {
        let Some(mut transition) = self.kaguraThrow_ else { return };
        if self.skillVariantSampleStartNs_ < transition.released_ns {
            return;
        }
        match self.kagura_w_cast_lock() {
            Some(true) => {
                transition.observed_lock = true;
                self.kaguraThrow_ = Some(transition);
            }
            Some(false) if transition.observed_lock => {
                self.kaguraThrow_ = None;
                self.trace(host, "skill_queue", "kagura_w_unlocked");
            }
            // A transported Q may fail in-game or its short lock may go
            // unobserved. Require fresh unlocked data even on this path.
            Some(false) if self.skillVariantSampleStartNs_ - transition.released_ns >= 220_000_000 => {
                self.kaguraThrow_ = None;
                self.trace(host, "skill_queue", "kagura_w_unlocked_without_observed_lock");
            }
            _ => {}
        }
    }
    pub(crate) fn queue_skill(&mut self, host: &mut impl Host, b: &Button) {
        if self.queuedSkills_.len() >= 4 {
            self.trace(host, "skill_queue", "capacity_rejected_newest");
            self.set_status("F7: очередь навыков заполнена — новое нажатие пропущено");
            return;
        }
        let request = QueuedSkill {
            slot: b.slot.clone(),
            key: b.key,
            held: true,
            requestedNs: self.now_,
            down: self.capture_skill_intent(host),
            ..QueuedSkill::default()
        };
        self.queuedSkills_.push(request);
        let reason = if self.waiting_for_kagura_w(b) { "queued_for_kagura_w_unlock" } else { "queued_after_active_gesture" };
        self.trace(host, "skill_queue", reason);
        if self.skillTouch_ >= 0
            && !self.skillCancelPending_
            && !self.pendingRelease_
            && self.skillCastMode_ == 0
            && self.skillKeyHeld_
        {
            self.request_skill_cancel(host)
        }
    }
    pub(crate) fn dispatch_queued_skill(&mut self, host: &mut impl Host) {
        if self.queuedSkills_.is_empty() {
            return;
        }
        if self.now_ - self.queuedSkills_[0].requestedNs > 1_500_000_000 {
            self.clear_queued_skills(host, "expired");
            self.set_status("F7: очередь навыков устарела — отложенные нажатия пропущены");
            return;
        }
        if self.skillTouch_ >= 0
            || self.skillCancelPending_
            || self.coreSkill_ != 0
            || self.frameCoreSampleStartNs_ < self.lastSkillEndNs_
        {
            return;
        }
        while !self.queuedSkills_.is_empty() {
            let Some(b) = self.find_button(&self.queuedSkills_[0].slot).cloned() else {
                return;
            };
            if !b.available {
                return;
            }
            if self.waiting_for_kagura_w(&b) {
                return;
            }
            if self.classify_skill(&b) == SkillBehavior::AwaitVariant {
                return;
            }
            if self.heroId_ == 25
                && b.slot == "skill:1"
                && b.skillId == 2520
                && self.skillVariantSampleStartNs_ < self.lastSkillEndNs_
            {
                return;
            }
            if b.key != self.queuedSkills_[0].key {
                self.trace(host, "skill_queue", "binding_or_form_changed");
                let q = self.queuedSkills_.remove(0);
                Self::drop_intent(host, &q.down);
                Self::drop_intent(host, &q.up);
                self.set_status(
                    "F7: отложенная клавиша не соответствует текущему навыку — нажатие пропущено",
                );
                continue;
            }
            let cd = self.cooldown_block(&b, self.lastSkillEndNs_);
            if cd == "await_post_release_snapshot" {
                return;
            }
            if !cd.is_empty() {
                let q = self.queuedSkills_.remove(0);
                self.trace_cooldown_ignored(host, &b, q.key, cd);
                if q.held {
                    self.cooldownIgnoredKeys_.insert(q.key);
                }
                Self::drop_intent(host, &q.down);
                Self::drop_intent(host, &q.up);
                continue;
            }
            let q = self.queuedSkills_.remove(0);
            if !self.begin_skill(host, &b) {
                Self::drop_intent(host, &q.down);
                Self::drop_intent(host, &q.up);
                self.clear_queued_skills(host, "dispatch_failed");
                return;
            }
            self.skillKeyHeld_ = q.held;
            if self.skillCastMode_ == 1 {
                Self::drop_intent(host, &q.up);
                self.apply_skill_intent(host, q.down)
            } else if !q.held {
                Self::drop_intent(host, &q.down);
                self.apply_skill_intent(host, q.up)
            } else {
                Self::drop_intent(host, &q.down);
                Self::drop_intent(host, &q.up)
            }
            self.trace(host, "skill_queue", "dispatched_with_fresh_form");
            return;
        }
    }
    pub(crate) fn begin_skill(&mut self, host: &mut impl Host, b: &Button) -> bool {
        if self.skillCancelPending_ || self.skillTouch_ >= 0 {
            return false;
        }
        if self.classify_skill(b)==SkillBehavior::Selection {
            if let Some(reason)=self.selection_start_rejection(b) {
                if self.physicalKeys_.contains(&b.key) { self.auxiliaryHeldKeys_.insert(b.key); }
                self.trace(host,"selection_start_rejected",reason);
                return false;
            }
        }
        let state_forbidden = self.state_forbidden(b, self.cooldown_variant(b), 0) == Some(true);
        self.flush_pointer(host);
        if b.stopMovement && self.classify_skill(b) != SkillBehavior::AwaitVariant {
            self.finish_movement(host, "")
        }
        let id = host.begin(b.press);
        if id < 0 {
            self.set_status("F7: не удалось начать касание навыка");
            return false;
        }
        self.sync_clock(host);
        self.qDragArming_ = false;
        self.qDragRecompute_ = false;
        self.qDragRecomputeUsed_ = false;
        self.qDragMismatchSinceNs_ = 0;
        self.qDragArmSentNs_ = 0;
        self.qDragArmSkill_ = 0;
        self.qDragArmSource_.clear();
        self.skillDownPoint_ = b.press;
        self.reset_accepted_input();
        self.castStartedNs_ = self.now_;
        self.skillTouch_ = id;
        self.skillPoint_ = b.press;
        self.record_skill_touch(b.press, false, false);
        self.activeSlot_ = b.slot.clone();
        self.skillKey_ = b.key;
        self.skillCastMode_ = match b.castMode {
            2 if Self::is_retribution(b) => 1,
            3 if Self::is_retribution(b) => 0,
            mode => mode,
        };
        self.skillKeyHeld_ = true;
        self.skillCooldownVariant_ = self.cooldown_variant(b);
        self.activeConfigSlot_ = self.current_config_slot(b);
        self.skillBindingVariant_ = if self.heroId_ == 25 && b.skillId == 2520 {
            self.active_variant(b)
        } else {
            0
        };
        self.skillBehavior_ = self.classify_skill(b);
        self.directionGeometryFresh_ = false;
        self.pendingDirectionValid_ = false;
        self.skillStarted_.restart(self.now_);
        self.skillMoved_.invalidate();
        self.solver_.reset();
        self.direct_.reset();
        self.pendingGoalSequence_ = 0;
        self.pendingRelease_ = false;
        self.pendingWorldValid_ = false;
        if self.pendingWorldResolver_ != 0 {
            host.release_resolver(self.pendingWorldResolver_)
        }
        self.pendingWorldResolver_ = 0;
        self.releaseRequested_.invalidate();
        self.skillNudged_ = false;
        self.skillExpectedAim_ = false;
        self.skillActivationObserved_ = false;
        self.skillStateForbiddenAtDown_ = state_forbidden;
        self.traceDesiredValid_ = false;
        self.observedSkillId_ = 0;
        self.solverRadius_ = 0.;
        self.armingDistance_ = 0.;
        self.warmAttempted_ = false;
        self.startupArmed_ = self.skillBehavior_ == SkillBehavior::Range;
        self.startupMayActivate_ = self.coreSkill_ == 0;
        self.arm_selection(b);
        if self.skillBehavior_ == SkillBehavior::Tap && self.skillCastMode_ == 1 {
            host.timer(Timer::Tap, Some(16))
        }
        if self.skillCastMode_ == 1
            && (self.skillBehavior_ == SkillBehavior::Direction
                || self.skillBehavior_ == SkillBehavior::AwaitVariant)
        {
            self.freeze_direction_intent(host)
        }
        if self.skillCastMode_==1&&self.skillBehavior_==SkillBehavior::Selection {
            self.freeze_selection_intent(host);
        }
        self.arm_charge_gesture(b);
        self.changed_ = true;
        true
    }
    fn state_forbidden(&self, b: &Button, variant: i32, minimum: i64) -> Option<bool> {
        if b.kind != "skill"
            || variant <= 0
            || self.skillVariantSampleNs_ <= 0
            || self.skillVariantSampleStartNs_ < minimum
            || self.now_ - self.skillVariantSampleNs_ > 100_000_000
        {
            return None;
        }
        self.skillVariants_.iter()
            .find(|v| v.slot == b.slot && v.base == b.skillId && v.active == variant)
            .and_then(|v| v.state_forbidden)
    }
    pub(crate) fn finish_rejected_range(&mut self, host: &mut impl Host) -> bool {
        // StateForbid alone is not a generic admission gate: buffered skills
        // may legitimately be pressed during another animation (e.g. Ling R).
        // Require an ordinary range release, a forbid already present at DOWN,
        // and fresh proof that this exact form never claimed any game UI/input.
        if self.skillTouch_ < 0
            || self.skillKeyHeld_
            || self.skillCancelPending_
            || self.skillCastMode_ != 0
            || self.skillBehavior_ != SkillBehavior::Range
            || !self.skillStateForbiddenAtDown_
            || self.skillActivationObserved_
            || self.touchSequence_ != 1
            || self.skillNudged_
            || self.qDragArming_
            || self.provisionalStartup_
            || self.coreSkill_ != 0
            || !self.frameActiveSlot_.is_empty()
            || self.frameCoreSampleStartNs_ < self.castStartedNs_
            || self.lastFrameNs_ <= 0
            || self.now_ - self.lastFrameNs_ > 100_000_000
        {
            return false;
        }
        let Some(button) = self.find_button(&self.activeSlot_) else { return false };
        if self.state_forbidden(button, self.skillCooldownVariant_, self.castStartedNs_) != Some(true) {
            return false;
        }
        self.trace(host, "range_rejected", "state_forbidden_without_ui_capture");
        self.finish_skill(host, true);
        true
    }
    pub(crate) fn finish_skill(&mut self, host: &mut impl Host, cancel: bool) {
        self.finish_skill_inner(host, cancel, false);
    }
    pub(crate) fn finish_auto_completed_charge(&mut self, host: &mut impl Host) {
        self.finish_skill_inner(host, false, true);
    }
    pub(crate) fn finish_auto_completed_selection(&mut self, host: &mut impl Host) {
        self.finish_skill_inner(host, false, true);
    }
    fn finish_skill_inner(&mut self, host: &mut impl Host, cancel: bool, auto_completed: bool) {
        host.timer(Timer::Tap, None);
        if cancel && !(self.skillCancelPending_ && self.skillCancelAcknowledged_) {
            self.clear_queued_skills(host, "predecessor_aborted")
        }
        if cancel || auto_completed {
            self.skillEchoPending_ = false
        }
        if self.skillTouch_ >= 0 {
            if !cancel && !auto_completed {
                let finishing = self.skillTouch_;
                self.flush_pointer(host);
                if self.skillTouch_ != finishing
                    || (self.skillBehavior_ == SkillBehavior::Selection && self.skillCancelPending_)
                {
                    return;
                }
            }
            self.trace(host, "release", if auto_completed { "auto_completed" } else if cancel { "cancel" }
                else if self.skillBehavior_==SkillBehavior::Selection {"selection_up_after_ui_ack"} else { "cast" });
            if cancel && !self.skillCancelPending_ {
                if let Some(point) = self.selection_cancel_point() {
                    self.move_skill(host, point, false);
                }
            }
            host.end(self.skillTouch_, self.skillPoint_);
            self.sync_clock(host);
            self.lastSkillEndNs_ = self.now_;
            if self.heroId_ == 25 && self.activeSlot_ == "skill:0" {
                self.kaguraThrow_ = (!cancel && !auto_completed
                    && self.skillCooldownVariant_ == 2510
                    && self.kagura_w_cast_lock().is_some())
                .then_some(KaguraThrowTransition {
                    released_ns: self.now_,
                    observed_lock: false,
                });
            }
            if !cancel && !auto_completed {
                let slot = self.activeSlot_.clone();
                self.mark_cooldown_cast(&slot, self.skillCooldownVariant_)
            }
        }
        self.skillCooldownVariant_ = 0;
        self.selection_=Default::default();
        self.chargeGesture_ = None;
        self.skillActivationObserved_ = false;
        self.skillStateForbiddenAtDown_ = false;
        self.skillCancelPending_ = false;
        self.skillCancelAcknowledged_ = false;
        self.skillCancelSentNs_ = 0;
        self.qDragArming_ = false;
        self.qDragRecompute_ = false;
        self.qDragRecomputeUsed_ = false;
        self.qDragMismatchSinceNs_ = 0;
        self.qDragArmSentNs_ = 0;
        self.qDragArmSkill_ = 0;
        self.qDragArmSource_.clear();
        self.skillEchoPending_ = false;
        self.skillTouch_ = -1;
        self.direct_.reset();
        self.pendingGoalSequence_ = 0;
        self.provisionalStartup_ = false;
        self.startupArmed_ = false;
        self.directionGeometryFresh_ = false;
        self.pendingDirectionValid_ = false;
        self.skillBehavior_ = SkillBehavior::Range;
        self.reset_accepted_input();
        self.pendingRelease_ = false;
        self.pendingWorldValid_ = false;
        if self.pendingWorldResolver_ != 0 {
            host.release_resolver(self.pendingWorldResolver_)
        }
        self.pendingWorldResolver_ = 0;
        self.releaseRequested_.invalidate();
        self.skillKey_ = 0;
        self.skillKeyHeld_ = false;
        self.activeSlot_.clear();
        self.activeConfigSlot_.clear();
        self.skillBindingVariant_ = 0;
        self.solver_.reset();
        self.skillStarted_.invalidate();
        self.skillMoved_.invalidate();
    }
    pub(crate) fn request_skill_cancel(&mut self, host: &mut impl Host) {
        // Aimed basic attacks have no native cancel gesture. Never enter the
        // cancel-ACK wait or move their contact onto a skill cancel button.
        if self.activeSlot_ == "attack" || self.skillTouch_ < 0 || self.skillCancelPending_ {
            return;
        }
        self.charge_cancel_requested();
        self.skillCancelPending_ = true;
        self.skillCancelAcknowledged_ = false;
        self.skillCancelSentNs_ = 0;
        host.timer(Timer::Tap, None);
        self.skillEchoPending_ = false;
        self.selection_.goal_sequence=0;
        self.startupArmed_ = false;
        self.pendingRelease_ = false;
        self.pendingWorldValid_ = false;
        if self.pendingWorldResolver_ != 0 {
            host.release_resolver(self.pendingWorldResolver_)
        }
        self.pendingWorldResolver_ = 0;
        self.releaseRequested_.invalidate();
        // Retribution and Execute also require this native cancel gesture:
        // ACTION_CANCEL on their touch can be interpreted as a normal cast.
        if let Some(point) = self.selection_cancel_point() {
            if !self.send_skill_cancel_point(host, point) {
                if self.skillBehavior_ == SkillBehavior::Selection {
                    self.selection_transport_failed(host, "selection_cancel_move_delivery_failed");
                } else {
                    self.finish_skill(host, true);
                }
                return;
            }
            self.skillCancelSentNs_ = self.now_;
        }
        self.trace(host, "cancel_requested", "await_native_cancel_flag");
        self.set_status("F7: ожидаю подтверждение отмены навыка")
    }
    pub(crate) fn sample_skill_cancel(&mut self, host: &mut impl Host, f: &snapshot::Frame) {
        self.skillCancelAcknowledged_ = false;
        if !self.skillCancelPending_ || self.skillCancelSentNs_ == 0 || self.skillTouch_ < 0 {
            return;
        }
        if self.skillBehavior_ == SkillBehavior::Selection && self.selection_.kind == 2 {
            if self.selection_target_cancel_ack(f) {
                self.skillCancelAcknowledged_ = true;
                self.trace(host, "cancel_acknowledged", "owned_target_cancel_flag_and_exact_native_touch");
            }
            return;
        }
        let Some(b) = self.find_button(&self.activeSlot_) else {
            return;
        };
        let s = &f.cancel_state;
        if self.skillBehavior_==SkillBehavior::Selection&&!self.selection_cancel_owner(b,f){return;}
        let start = s.sample_start_ns.get(-1.);
        let end = s.sample_end_ns.get(-1.);
        let now = self.now_ as f64;
        if !s.ready.get()
            || !s.coherent.get()
            || s.source.as_str() != self.selection_cancel_source()
            || !f.token_string.get()
            || !f.token_nonempty.get()
            || s.token.as_str() != self.frameEpoch_
            || s.hero_id.int() != self.heroId_
            || s.slot.as_str() != self.activeSlot_
            || s.base_skill_id.int() != b.skillId
            || !s.cancelled.is_bool()
            || !s.cancelled.get()
            || !s.sample_start_ns.is_number()
            || !s.sample_end_ns.is_number()
            || !start.is_finite()
            || !end.is_finite()
            || start < self.skillCancelSentNs_ as f64
            || end < start
            || end - start > 100e6
            || now - end > 100e6
            || end > now + 10e6
        {
            return;
        }
        self.skillCancelAcknowledged_ = true;
        self.trace(host, "cancel_acknowledged", "native_cancel_flag_after_move");
    }
    pub(crate) fn finish_manual(&mut self, host: &mut impl Host) {
        if self.manualTouch_ >= 0 {
            host.end(self.manualTouch_, self.manualPoint_)
        }
        self.manualTouch_ = -1
    }
    pub(crate) fn begin_auxiliary_tap(&mut self, host: &mut impl Host, b: &Button) {
        if !b.available || self.auxiliaryTaps_.len() + self.auxiliaryHolds_.len() >= 8 {
            return;
        }
        let id = host.begin(b.press);
        if id < 0 {
            return;
        }
        let variant = if b.kind == "extra" {
            self.cooldown_variant(b)
        } else {
            0
        };
        self.sync_clock(host);
        self.auxiliaryTaps_.push(AuxiliaryTap {
            id,
            key: b.key,
            point: b.press,
            until: if b.castMode == 1 { self.now_ + 16_000_000 } else { i64::MAX },
            cooldownSlot: if b.kind == "extra" {
                b.slot.clone()
            } else {
                String::new()
            },
            cooldownVariant: variant,
        });
        if b.castMode == 1 { host.timer(Timer::Auxiliary, Some(16)); }
    }
    pub(crate) fn expire_auxiliary_taps(&mut self, host: &mut impl Host) {
        for i in (0..self.auxiliaryTaps_.len()).rev() {
            if self.auxiliaryTaps_[i].until <= self.now_ {
                let tap = self.auxiliaryTaps_.remove(i);
                host.end(tap.id, tap.point);
                self.sync_clock(host);
                self.mark_cooldown_cast(&tap.cooldownSlot, tap.cooldownVariant)
            }
        }
        if let Some(next) = self.auxiliaryTaps_.iter().filter(|t| t.until != i64::MAX).map(|t| t.until).min() {
            host.timer(
                Timer::Auxiliary,
                Some(((next - self.now_ + 999_999) / 1_000_000).max(1)),
            )
        }
    }
    pub(crate) fn finish_auxiliary_taps(&mut self, host: &mut impl Host) {
        host.timer(Timer::Auxiliary, None);
        let taps = std::mem::take(&mut self.auxiliaryTaps_);
        for tap in taps {
            host.end(tap.id, tap.point);
            self.sync_clock(host);
            self.mark_cooldown_cast(&tap.cooldownSlot, tap.cooldownVariant)
        }
        self.auxiliaryHeldKeys_.clear();
    }
    pub(crate) fn release_all(&mut self, host: &mut impl Host) {
        self.tombstone_physical_inputs();
        self.abort_owned_context(host);
        host.timer(Timer::Pointer, None);
        self.clear_queued_skills(host, "lifecycle_reset");
        self.cooldownIgnoredKeys_.clear();
        self.finish_skill(host, true);
        self.kaguraThrow_ = None;
        self.finish_movement(host, "");
        self.finish_manual(host);
        self.finish_auxiliary_taps(host);
        self.finish_auxiliary_holds(host);
    }
    pub(crate) fn invalidate(&mut self, host: &mut impl Host, reason: &str) {
        self.tombstone_physical_inputs();
        self.abort_owned_context(host);
        self.clear_queued_skills(host, "telemetry_invalidated");
        self.cooldownIgnoredKeys_.clear();
        let was = self.ready_;
        self.finish_skill(host, true);
        self.kaguraThrow_ = None;
        self.finish_movement(host, "");
        self.finish_auxiliary_taps(host);
        self.finish_auxiliary_holds(host);
        self.ready_ = false;
        self.aimReady_ = false;
        self.directStartups_.clear();
        self.directRanges_.clear();
        self.frameCameraReady_ = false;
        self.skillVariants_.clear();
        self.skillVariantSampleNs_ = 0;
        self.set_status(if self.enabled_ {
            reason
        } else {
            "F7: автоматическое управление выключено"
        });
        if was {
            self.changed_ = true
        }
    }
}
#[cfg(test)]
mod transport_clock_tests {
    use super::*;
    struct ClockHost {
        now: i64,
        pointer_deadline: Option<i64>,
    }
    impl Host for ClockHost {
        fn clock_ns(&mut self) -> Option<i64> {
            Some(self.now)
        }
        fn begin(&mut self, _: Point) -> i32 {
            self.now = 200;
            7
        }
        fn move_touch(&mut self, _: i32, _: Point) -> bool {
            self.now = 300;
            true
        }
        fn end(&mut self, _: i32, _: Point) {
            self.now = 400
        }
        fn pointer_world(&mut self, _: Point) -> Option<Point> {
            None
        }
        fn ground_world(&mut self, _: Point) -> Option<Point> {
            None
        }
        fn movement_direction(&mut self, _: Point) -> Option<Point> {
            None
        }
        fn hero_world(&mut self) -> Option<Point> {
            None
        }
        fn pointer_world_at_plane(&mut self, _: Point, _: f64) -> Option<Point> {
            None
        }
        fn capture_pointer_world(&mut self, _: Point) -> u64 {
            0
        }
        fn resolve_world(&mut self, _: u64) -> Option<Point> {
            None
        }
        fn release_resolver(&mut self, _: u64) {}
        fn navigation(&mut self, _: Point, _: Point, _: bool) -> Result<Vec<Point>, String> {
            Err(String::new())
        }
        fn binding(&mut self, _: i32, _: &Button, _: &str) -> host::Binding {
            host::Binding::default()
        }
        fn cancel_key(&mut self, _: i32) -> i32 {
            84
        }
        fn timer(&mut self, timer: Timer, delay: Option<i64>) {
            if let Timer::Pointer = timer {
                match delay {
                    None => self.pointer_deadline = None,
                    Some(ms) if self.pointer_deadline.is_none() => {
                        self.pointer_deadline = Some(self.now + ms * 1_000_000)
                    }
                    _ => {}
                }
            }
        }
        fn trace(&mut self, _: &Controller, _: &str, _: &str) {}
    }
    #[test]
    fn causal_commands_and_cooldown_boundaries_follow_actual_transport_return() {
        let mut c = Controller::new();
        c.now_ = 100;
        c.androidSize_ = Point::new(1000., 1000.);
        let mut host = ClockHost {
            now: 100,
            pointer_deadline: None,
        };
        let button = Button {
            slot: "skill:0".into(),
            kind: "skill".into(),
            skillId: 2510,
            press: Point::new(0.6, 0.8),
            available: true,
            ..Button::default()
        };
        assert!(c.begin_skill(&mut host, &button));
        assert_eq!(c.castStartedNs_, 200);
        assert_eq!(c.causal_.latest().unwrap().time_ns, 200);
        assert!(c.move_skill(&mut host, Point::new(0.7, 0.7), true));
        let command = c.causal_.latest().unwrap();
        assert_eq!(command.time_ns, 300);
        assert!(c.causal_.match_direction(command.pixels, 299).is_none());
        assert!(c.causal_.match_direction(command.pixels, 300).is_some());
        c.finish_skill(&mut host, false);
        assert_eq!(c.lastSkillEndNs_, 400);
        assert_eq!(c.cooldownCasts_.get("skill:0:2510"), Some(&400));
    }
    #[test]
    fn pointer_batch_uses_one_millisecond_oldest_deadline_and_latest_value() {
        let mut c = Controller::new();
        c.manualTouch_ = 7;
        c.now_ = 100_000_000;
        let mut host = ClockHost {
            now: c.now_,
            pointer_deadline: None,
        };
        c.update_pointer(&mut host, Point::new(0.2, 0.3));
        assert_eq!(host.pointer_deadline, Some(101_000_000));
        host.now = 100_800_000;
        c.now_ = host.now;
        c.update_pointer(&mut host, Point::new(0.8, 0.7));
        assert_eq!(host.pointer_deadline, Some(101_000_000));
        assert_eq!(c.pointer_, Point::new(0.8, 0.7));
    }
}
