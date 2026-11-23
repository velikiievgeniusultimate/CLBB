//! Recover a contact the game left unclaimed while its exact skill was locked.
use super::{
    Button, Controller, Host, SkillBehavior, charge_lifecycle::game_clock, snapshot::Frame,
};

#[derive(Clone, Copy)]
pub(super) struct NativeLock {
    cast_locked: bool,
    state_forbidden: bool,
    last_tick: u32,
    idle_ticks: u8,
}

impl Controller {
    fn native_lock_flags(&self, button: &Button, minimum: i64) -> (bool, bool) {
        if self.skillVariantSampleStartNs_ <= 0
            || self.skillVariantSampleStartNs_ < minimum
            || !(0..=100_000_000).contains(&(self.now_ - self.skillVariantSampleNs_))
        {
            return (false, false);
        }
        self.skillVariants_
            .iter()
            .find(|row| {
                row.slot == button.slot
                    && row.base == button.skillId
                    && row.active == self.skillCooldownVariant_
                    && row.charge_skill != Some(true)
            })
            .map_or((false, false), |row| {
                (
                    row.cast_locked == Some(true),
                    row.state_forbidden == Some(true),
                )
            })
    }

    pub(super) fn arm_native_lock(&mut self, button: &Button) {
        if button.kind != "skill"
            || self.skillCooldownVariant_ <= 0
            || self.chargeGesture_.is_some()
            || self.holding_cable_contact()
            || matches!(
                self.skillBehavior_,
                SkillBehavior::Selection | SkillBehavior::AwaitVariant
            )
        {
            return;
        }
        let Some(last_tick) = self.frameGameClock_ else {
            return;
        };
        let (cast_locked, state_forbidden) = self.native_lock_flags(button, 0);
        if cast_locked || state_forbidden {
            self.nativeLock_ = Some(NativeLock {
                cast_locked,
                state_forbidden,
                last_tick,
                idle_ticks: 0,
            });
        }
    }

    pub(super) fn observe_native_lock(&mut self, frame: &Frame) {
        self.frameGameClock_ = game_clock(frame.game_clock_ms);
        let Some(mut proof) = self.nativeLock_ else {
            return;
        };
        if self.skillActivationObserved_ {
            self.nativeLock_ = None;
            return;
        }
        if self.frameCoreSampleStartNs_ < self.castStartedNs_ {
            return;
        }
        // Never reinterpret an empty logical aim as a rejected input. The HUD
        // itself must report idle and the same lock must survive game ticks.
        self.nativeLock_ = None;
        let Some(clock) = self.frameGameClock_ else {
            return;
        };
        if clock < proof.last_tick
            || frame.ui.active_slot.present != 1
            || frame.ui.active_slot.size != 0
            || !frame.skill_id.is_number()
            || frame.skill_id.value != 0.
        {
            return;
        }
        let Some(button) = self.find_button(&self.activeSlot_) else {
            return;
        };
        let (cast_locked, state_forbidden) = self.native_lock_flags(button, self.castStartedNs_);
        proof.cast_locked &= cast_locked;
        proof.state_forbidden &= state_forbidden;
        if proof.cast_locked || proof.state_forbidden {
            if clock > proof.last_tick {
                proof.last_tick = clock;
                proof.idle_ticks = (proof.idle_ticks + 1).min(2);
            }
            self.nativeLock_ = Some(proof);
        }
    }

    pub(super) fn finish_native_lock(&mut self, host: &mut impl Host) -> bool {
        let Some(proof) = self.nativeLock_ else {
            return false;
        };
        if proof.idle_ticks < 2
            || self.skillActivationObserved_
            || !(0..=100_000_000).contains(&(self.now_ - self.lastFrameNs_))
            || self.touchSequence_ != if self.skillCancelSentNs_ == 0 { 1 } else { 2 }
            || !self.skillCancelPending_
                && (!proof.cast_locked
                    || self.skillKeyHeld_
                        && (self.skillCastMode_ != 1 || self.skillBehavior_ == SkillBehavior::Tap))
        {
            return false;
        }
        if self.skillCancelSentNs_ == 0 {
            let Some(point) = self.selection_cancel_point() else {
                return false;
            };
            if !self.send_skill_cancel_point(host, point) {
                return false;
            }
        }
        if self.skillKeyHeld_
            && !self
                .queuedSkills_
                .iter()
                .any(|q| q.key == self.skillKey_ && q.held)
        {
            self.cooldownIgnoredKeys_.insert(self.skillKey_);
        }
        // Lift only this finger at the cancel point. Preserve newer intents
        // and avoid a synthetic cooldown or an Android-wide ACTION_CANCEL.
        let reason = if proof.cast_locked {
            "native_cast_lock_without_capture"
        } else {
            "native_state_forbid_without_capture"
        };
        self.finish_skill_inner(host, false, Some(reason));
        true
    }
}
