use super::host::CableReceipt;
use super::{snapshot, Button, Controller, SkillBehavior};

// A missing impact must not keep a physical press in the queue.
const MAX_HANDOFF_NS: i64 = 250_000_000;

struct ReleasedCable {
    at: i64,
    uses: u32,
    previous_impact: Option<u32>,
    impact_clock: Option<u32>,
}

#[derive(Default)]
pub(super) struct CableChain {
    last_impact: Option<u32>,
    last_count: Option<(u32, i64)>,
    release: Option<ReleasedCable>,
}

impl CableChain {
    pub(super) fn observe(&mut self, sample: CableReceipt, clock: u32, sample_start: i64) {
        self.last_count = Some((sample.uses, sample_start));
        if let Some(guid) = sample.impact {
            self.last_impact = Some(guid);
        }
        let Some(prev) = &mut self.release else {
            return;
        };
        if sample_start <= prev.at {
            return;
        }
        if sample_start - prev.at >= MAX_HANDOFF_NS
            || sample.uses < prev.uses
            || sample.uses - prev.uses > 1
        {
            self.release = None;
            return;
        }
        if sample.uses == prev.uses {
            if let Some(guid) = sample.impact {
                prev.previous_impact = Some(guid);
            }
        } else if let Some(impact_clock) = prev.impact_clock {
            // The previous cable can still retire the UI during its impact update.
            if clock != impact_clock {
                self.release = None;
            }
        } else if sample.impact.is_some() && sample.impact != prev.previous_impact {
            prev.impact_clock = Some(clock);
        }
    }

    pub(super) fn finish(&mut self, chained: bool, now: i64) {
        self.release = self.last_count.and_then(|(uses, at)| {
            (chained && (0..100_000_000).contains(&(now - at))).then_some(ReleasedCable {
                at: now,
                uses,
                previous_impact: self.last_impact,
                impact_clock: None,
            })
        });
    }

    fn pending(&self, now: i64) -> bool {
        self.release
            .as_ref()
            .is_some_and(|prev| (0..MAX_HANDOFF_NS).contains(&(now - prev.at)))
    }
}

impl Controller {
    pub(super) fn waiting_for_cable(&self, button: &Button) -> bool {
        self.heroId_ == 17
            && button.slot == "skill:1"
            && button.skillId == 1720
            && button.castMode == 0
            && self.active_variant(button) == 1723
            && self.cableChain_.pending(self.now_)
    }

    pub(super) fn holding_cable_contact(&self) -> bool {
        self.heroId_ == 17
            && self.activeSlot_ == "skill:1"
            && self.skillCastMode_ == 0
            && matches!(self.skillCooldownVariant_, 1720 | 1723)
    }

    pub(super) fn awaiting_cable_direction(&self) -> bool {
        self.pendingRelease_ && self.holding_cable_contact()
    }

    // The game closes the chained cable's reticle when its recast window
    // expires. Its cancel flag can no longer acknowledge a subsequent T.
    // Require positive evidence of the base form in the same post-MOVE frame.
    pub(super) fn cancelled_cable_form_expired(&self, b: &Button, f: &snapshot::Frame) -> bool {
        self.holding_cable_contact()
            && self.skillCooldownVariant_ == 1723
            && self.skillBehavior_ == SkillBehavior::Direction
            && self.skillActivationObserved_
            && b.skillId == 1720
            && self.frameCoreSampleStartNs_ >= self.skillCancelSentNs_
            && self.skillVariantSampleStartNs_ >= self.skillCancelSentNs_
            && self.active_variant(b) == 1720
            && f.skill_id.get(-1.) == 0.
            && f.ui.active_slot.missing()
            && f.cancel_state.slot.missing()
            && !f.cancel_state.cancelled.get()
            && !f.active_joystick.ready.get()
            && !f.direction_input.ready.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_new_impact_after_one_cast_and_a_subsequent_update_opens_the_gate() {
        let mut chain = CableChain::default();
        let read = |uses, impact| CableReceipt { uses, impact };
        chain.observe(read(12, Some(100)), 66, 1);
        chain.finish(true, 10);
        chain.observe(read(12, Some(101)), 132, 20);
        chain.observe(read(13, Some(101)), 198, 30);
        chain.observe(read(13, Some(101)), 264, 31);
        assert!(chain.pending(31));
        chain.observe(read(13, Some(102)), 264, 40);
        chain.observe(read(13, None), 264, 50);
        assert!(chain.pending(51));
        chain.observe(read(13, None), 330, 60);
        assert!(!chain.pending(61));
    }

    #[test]
    fn missing_feedback_and_unrelated_releases_do_not_stall_the_next_press() {
        let mut chain = CableChain::default();
        chain.finish(true, 1);
        assert!(!chain.pending(2));
        chain.observe(
            CableReceipt {
                uses: 4,
                impact: None,
            },
            1,
            2,
        );
        chain.finish(true, 3);
        assert!(!chain.pending(3 + MAX_HANDOFF_NS));
        chain.finish(false, 4);
        assert!(!chain.pending(5));
    }
}
