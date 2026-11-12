//! Cancel the currently aimed common button without moving other fingers.
use super::*;
impl Controller {
    pub(crate) fn request_auxiliary_cancel(&mut self, host:&mut impl Host)->bool {
        if self.auxiliaryTaps_.iter().any(|t|t.cancelRequested) {return true;}
        let Some(i)=self.auxiliaryTaps_.iter().position(|t| {
            t.until==i64::MAX && matches!(t.cooldownSlot.as_str(),"extra:0"|"extra:1")
                && t.cooldownSlot==self.frameActiveSlot_
                && t.hero==self.heroId_ && t.epoch==self.frameEpoch_
        }) else{return false;};
        self.auxiliaryTaps_[i].cancelRequested=true;
        self.trace(host,"auxiliary_cancel",&format!("requested slot={} finger={}",
            self.auxiliaryTaps_[i].cooldownSlot,self.auxiliaryTaps_[i].id));
        self.tick_auxiliary_cancel(host);true
    }
    pub(crate) fn tick_auxiliary_cancel(&mut self, host:&mut impl Host) {
        if !self.cancelReady_ {return;}
        for i in 0..self.auxiliaryTaps_.len() {
            let tap=&self.auxiliaryTaps_[i];
            if !tap.cancelRequested || tap.cancelSentNs!=0 || tap.cooldownSlot!=self.frameActiveSlot_ {continue;}
            let id=tap.id;let point=self.cancelPoint_;
            if host.move_touch(id,point) {
                self.sync_clock(host);
                let tap=&mut self.auxiliaryTaps_[i];tap.point=point;tap.cancelSentNs=self.now_;
                self.trace(host,"auxiliary_cancel",&format!("move_to_cancel finger={id}"));
            }
        }
    }
    pub(crate) fn sample_auxiliary_cancel(&mut self, host:&mut impl Host, f:&snapshot::Frame) {
        let s=&f.cancel_state;let start=s.sample_start_ns.get(-1.);let end=s.sample_end_ns.get(-1.);
        if !s.ready.get() || !s.coherent.get() || !s.cancelled.is_bool() || !s.cancelled.get()
            || s.source.as_str()!="UIKeySkill.bCancelSkill" || !f.token_string.get() || !f.token_nonempty.get()
            || !s.sample_start_ns.is_number() || !s.sample_end_ns.is_number()
            || !start.is_finite() || !end.is_finite() || end<start || end-start>100e6
            || self.now_ as f64-end>100e6 || end>self.now_ as f64+10e6 {return;}
        for i in (0..self.auxiliaryTaps_.len()).rev() {
            let tap=&self.auxiliaryTaps_[i];
            if !tap.cancelRequested || tap.cancelSentNs==0 || start<tap.cancelSentNs as f64
                || s.slot.as_str()!=tap.cooldownSlot || s.base_skill_id.int()!=tap.cooldownVariant
                || s.hero_id.int()!=tap.hero || tap.hero!=self.heroId_
                || s.token.as_str()!=tap.epoch || tap.epoch!=self.frameEpoch_ {continue;}
            let tap=self.auxiliaryTaps_.remove(i);
            host.end(tap.id,tap.point);self.sync_clock(host);
            // Cancellation is not a cast: never mark the cooldown here.
            self.trace(host,"auxiliary_cancel",&format!("native_ack_and_up slot={} finger={}",tap.cooldownSlot,tap.id));
        }
    }
}
