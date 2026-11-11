use super::*;
impl Controller {
    pub(crate) fn handle_key(
        &mut self,
        host: &mut impl Host,
        key: i32,
        pressed: bool,
        auto_repeat: bool,
    ) -> bool {
        if !self.ready() {
            return false;
        }
        if self.cooldownIgnoredKeys_.contains(&key) {
            if !pressed && !auto_repeat {
                self.cooldownIgnoredKeys_.remove(&key);
            }
            if pressed || auto_repeat || key != self.skillKey_ || !self.skillKeyHeld_ {
                return true;
            }
        }
        if self.auxiliaryHeldKeys_.contains(&key) {
            if !pressed && !auto_repeat {
                self.auxiliaryHeldKeys_.remove(&key);
            }
            return true;
        }
        if key != 0 && key == self.cancelKey_ {
            if pressed && !auto_repeat {
                self.clear_queued_skills(host, "explicit_user_cancel");
                self.request_skill_cancel(host)
            }
            return true;
        }
        if let Some(i) = self
            .queuedSkills_
            .iter()
            .rposition(|q| q.key == key && q.held)
        {
            if !pressed && !auto_repeat {
                let intent = self.capture_skill_intent(host);
                self.queuedSkills_[i].held = false;
                Self::drop_intent(host, &self.queuedSkills_[i].up);
                self.queuedSkills_[i].up = intent;
                self.trace(host, "skill_queue", "queued_physical_up")
            }
            return true;
        }
        if pressed && key == self.skillKey_ && self.skillKeyHeld_ {
            return true;
        }
        if self.skillCancelPending_ && key == self.skillKey_ {
            if !pressed && !auto_repeat {
                self.skillKeyHeld_ = false
            }
            return true;
        }
        if !pressed && key == self.skillKey_ {
            if auto_repeat {
                return true;
            }
            self.skillKeyHeld_ = false;
            if self.skillBehavior_ == SkillBehavior::Tap {
                self.tick(host);
                return true;
            }
            if self.skillBehavior_ == SkillBehavior::Direction
                || self.skillBehavior_ == SkillBehavior::AwaitVariant
            {
                if !self.pendingRelease_ {
                    self.freeze_direction_intent(host)
                }
                self.tick(host);
                return true;
            }
            self.flush_pointer(host);
            if self.pendingRelease_ {
                return true;
            }
            if self.skillCastMode_ == 0 {
                let expected = self.skillExpectedAim_
                    || self
                        .find_button(&self.activeSlot_)
                        .is_some_and(|b| b.kind == "skill");
                if expected {
                    self.pendingRelease_ = true;
                    self.pendingPointer_ = self.pointer_;
                    self.pendingWorldValid_ = false;
                    if self.pendingWorldResolver_ != 0 {
                        host.release_resolver(self.pendingWorldResolver_)
                    }
                    self.pendingWorldResolver_ = 0;
                    if let Some(world) = host.pointer_world(self.pendingPointer_) {
                        self.pendingWorld_ = world;
                        self.pendingWorldValid_ = finite(world)
                    } else {
                        self.pendingWorldResolver_ =
                            host.capture_pointer_world(self.pendingPointer_)
                    }
                    self.releaseRequested_.restart(self.now_);
                    self.pendingGoalSequence_ = 0;
                    let desired = self.pendingWorld_
                        - if self.logicalMode_ {
                            self.logicalOrigin_
                        } else {
                            Point::default()
                        };
                    if !self.verified_direct_class()
                        && self.pendingWorldValid_
                        && self.solver_.learned()
                        && self.aimReady_
                        && self.frameActiveSlot_ == self.activeSlot_
                        && self.acceptedSampleNs_ > 0
                        && self.now_ - self.acceptedSampleNs_ < 100_000_000
                        && distance(self.solver_observed(), desired) <= 0.03
                    {
                        self.finish_skill(host, false)
                    } else {
                        self.tick(host)
                    }
                } else {
                    self.finish_skill(host, false)
                }
            }
            return true;
        }
        let mut unavailable = false;
        for b in self.buttons_.clone() {
            let mut matches = key != 0 && b.key == key;
            if self.heroId_ == 25
                && b.skillId == 2520
                && self.classify_skill(&b) == SkillBehavior::AwaitVariant
            {
                let present = host.binding(self.heroId_, &b, "skill:1");
                let away = host.binding(self.heroId_, &b, "skill:1:2521");
                matches = key != 0 && (present.key == key || away.key == key)
            }
            if matches {
                if !b.available {
                    unavailable = true;
                    continue;
                }
                if pressed && !auto_repeat {
                    let cd = self.cooldown_block(&b, 0);
                    if !cd.is_empty() {
                        self.cooldownIgnoredKeys_.insert(key);
                        self.trace_cooldown_ignored(host, &b, key, cd);
                        return true;
                    }
                }
                if Self::tap_only(&b) {
                    if pressed && !auto_repeat {
                        self.auxiliaryHeldKeys_.insert(key);
                        self.begin_auxiliary_tap(host, &b)
                    }
                } else if pressed && !auto_repeat {
                    let mut requested = b;
                    requested.key = key;
                    if self.skillTouch_ >= 0
                        || !self.queuedSkills_.is_empty()
                        || self.waiting_for_kagura_w(&requested)
                    {
                        self.queue_skill(host, &requested)
                    } else {
                        self.begin_skill(host, &requested);
                    }
                }
                return true;
            }
        }
        if unavailable && pressed && !auto_repeat {
            self.auxiliaryHeldKeys_.insert(key);
        }
        unavailable
    }
    pub(crate) fn observe_aim(&mut self, ready: bool, world: Point, skill: i32) {
        self.aimReady_ = ready && finite(world) && skill > 0;
        self.aimWorld_ = world;
        self.aimSkillId_ = skill;
    }
    pub(crate) fn set_android_size(&mut self, host: &mut impl Host, size: Point) {
        if !finite(size) || size.x < 32. || size.y < 32. || size.x > 32768. || size.y > 32768. {
            return;
        }
        if self.androidSize_.x > 0. && self.androidSize_.y > 0. && self.androidSize_ != size {
            self.release_all(host);
            self.responseModels_.clear();
            self.directStartups_.clear();
            self.directRanges_.clear()
        }
        self.androidSize_ = size;
    }
    pub(crate) fn set_enabled(&mut self, host: &mut impl Host, enabled: bool) {
        if self.enabled_ == enabled {
            return;
        }
        self.release_all(host);
        self.enabled_ = enabled;
        if enabled {
            host.timer(Timer::Main, Some(16));
            self.set_status(if self.ready_ {
                "F7: автоматическое управление готово"
            } else {
                "F7: ожидаю матч и кнопки героя"
            })
        } else {
            host.timer(Timer::Main, None);
            self.set_status("F7: автоматическое управление выключено")
        }
        self.changed_ = true;
    }
    pub(crate) fn set_navigation_state(
        &mut self,
        host: &mut impl Host,
        available: bool,
        status: String,
    ) {
        if self.navigationAvailable_ == available && self.navigationStatus_ == status {
            return;
        }
        self.navigationAvailable_ = available;
        self.navigationStatus_ = status;
        if !available && self.movementAuto_ && self.movementClickMode_ == 1 {
            self.finish_movement(host, "Обход остановлен: карта недоступна")
        }
        self.changed_ = true;
    }
}
