//! Cross-button gestures: fixed UI endpoints, independent of the aim cursor.
//! Only current, owned native choice receipts acknowledge the transition.
use clbb_moba_core::Point;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Observation {
    pub endpoints: [String; 2],
    pub source_slot: Option<String>,
    pub pressed_slot: Option<String>,
    pub enough_energy: bool,
    pub choices: [i32; 2],
    pub base_skills: [i32; 2],
    pub on_cooldown: [bool; 2],
}
#[derive(Clone, Debug)]
pub struct Geometry {
    pub epoch: String,
    pub endpoints: [Point; 2],
    pub cancel: Option<Point>,
}
pub trait Transport {
    fn down(&mut self, point: Point) -> i32;
    fn motion(&mut self, contact: i32, point: Point) -> bool;
    fn up(&mut self, contact: i32, point: Point);
    fn abort(&mut self, _contact: i32, _point: Point) -> bool { false }
    // Context reset only; individual user cancellation must preserve movement.
    fn abort_all(&mut self) -> bool { false }
}
pub struct CancelFeedback<'a> {
    pub slot: &'a str,
    pub sample_start_ns: i64,
    pub point: Option<Point>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Held {
    pub key: i32,
    pub contact: i32,
    pub origin: usize,
    pub pressed: bool,
    pub acknowledged: bool,
    pub down_observed: bool,
    pub chosen_skill: i32,
    probe_move_ns: Option<i64>,
    pub started_ns: i64,
    pub cancel_requested_ns: Option<i64>,
    #[serde(skip)]
    cancel_move_ns: Option<i64>,
    #[serde(skip)]
    cancel_first_move_ns: Option<i64>,
    #[serde(skip)]
    geometry: Geometry,
    #[serde(skip)]
    slots: [String; 2],
}
#[derive(Default)]
pub struct Gestures {
    pub active: Vec<Held>,
    ignored: Vec<i32>,
}
impl Gestures {
    pub fn ignored_keys(&self) -> &[i32] { &self.ignored }
    pub fn contains(&self, key: i32) -> bool {
        self.active.iter().any(|h|h.key==key) || self.ignored.contains(&key)
    }
    pub fn begin(&mut self, transport: &mut impl Transport, key: i32, origin: usize,
        geometry: Geometry, observed: &Observation, now: i64) -> bool {
        if self.contains(key) {return true;}
        // These bindings request a combo, not a replacement base cast.
        // Parent cooldowns do not determine native curve availability.
        if origin>1 || !observed.enough_energy || self.active.len()>=2 {
            self.ignored.push(key);return true;
        }
        let contact=transport.down(geometry.endpoints[origin]);
        if contact<0 {self.ignored.push(key);return true;}
        self.active.push(Held {key,contact,origin,pressed:true,acknowledged:false,down_observed:false,
            chosen_skill:0,probe_move_ns:None,started_ns:now,cancel_requested_ns:None,cancel_move_ns:None,cancel_first_move_ns:None,geometry,slots:observed.endpoints.clone()});
        true
    }
    pub fn release(&mut self, transport: &mut impl Transport, key: i32) -> bool {
        if let Some(i)=self.ignored.iter().position(|v|*v==key) {
            self.ignored.remove(i);return true;
        }
        let Some(i)=self.active.iter().position(|h|h.key==key) else{return false;};
        self.active[i].pressed=false;
        if self.active[i].acknowledged && self.active[i].cancel_requested_ns.is_none() {
            let h=self.active.remove(i);
            transport.motion(h.contact,h.geometry.endpoints[1-h.origin]);
            transport.up(h.contact,h.geometry.endpoints[1-h.origin]);
        }
        true
    }
    pub fn poll(&mut self, transport:&mut impl Transport, epoch:&str,
        observed:Option<&Observation>, _native_skill:i32, sample_start_ns:i64, now:i64,
        cancel_feedback: Option<CancelFeedback<'_>>) {
        let mut i=0;
        while i<self.active.len() {
            let h=&mut self.active[i];
            let valid=epoch==h.geometry.epoch && observed.is_some_and(|o|o.endpoints==h.slots);
            if valid && h.geometry.cancel.is_none() {
                h.geometry.cancel=cancel_feedback.as_ref().and_then(|s|s.point);
            }
            if !valid {
                if h.geometry.cancel.is_none() {transport.abort_all();}
                let h=self.active.remove(i);
                Self::retire_cancelled(transport, &h);
                if h.pressed {self.ignored.push(h.key);}
                continue;
            }
            if !h.acknowledged && now-h.started_ns>500_000_000 {
                h.cancel_requested_ns.get_or_insert(now);
            }
            if h.cancel_requested_ns.is_some() {
                if let Some(point) = cancel_feedback.as_ref().and_then(|sample| sample.point) {
                    if Some(point) != h.geometry.cancel {
                        h.geometry.cancel = Some(point);
                        h.cancel_move_ns = None;
                        h.cancel_first_move_ns = None;
                    }
                }
                let Some(cancel_point) = h.geometry.cancel else {
                    // Hidden cancel UI is normal before DOWN. Never synthesize
                    // an UP on the skill when its cancel location is unknown.
                    // A sole contact can be canceled natively; a shared gesture
                    // waits for the fresh cancel widget without dropping motion.
                    if transport.abort(h.contact,h.geometry.endpoints[h.origin]) {
                        let h=self.active.remove(i);
                        if h.pressed {self.ignored.push(h.key);}
                        continue;
                    }
                    i+=1;continue;
                };
                let acknowledged = h.cancel_move_ns.is_some_and(|moved| cancel_feedback.as_ref().is_some_and(|sample|
                    sample.sample_start_ns >= moved && h.slots.iter().any(|slot| slot == sample.slot)));
                if acknowledged || h.cancel_first_move_ns.is_some_and(|first|now-first>500_000_000) {
                    let h = self.active.remove(i);
                    if acknowledged || !transport.abort(h.contact, cancel_point) {
                        transport.up(h.contact, cancel_point);
                    }
                    if h.pressed {self.ignored.push(h.key);}
                    continue;
                }
                if h.cancel_move_ns.is_none_or(|moved| now-moved >= 32_000_000)
                    && transport.motion(h.contact,cancel_point) {
                    h.cancel_move_ns = Some(now);
                    h.cancel_first_move_ns.get_or_insert(now);
                }
                i += 1;
                continue;
            }
            let o=observed.unwrap();
            if !h.down_observed && sample_start_ns>=h.started_ns
                && (o.pressed_slot.as_deref()==Some(h.slots[h.origin].as_str())
                    || o.source_slot.as_deref()==Some(h.slots[h.origin].as_str())) {
                h.down_observed=true;
            }
            // Do not MOVE before the native DOWN is visible. Otherwise the
            // input consumer may coalesce DOWN at the probe point and see no
            // subsequent delta, so its drag owner never starts.
            if !h.down_observed {i+=1;continue;}

            // Energy gates DOWN only; a selected combo may consume it while
            // held. The owned native release choice is authoritative even if
            // the generic aim ID still describes an unavailable parent.
            if o.source_slot.as_deref()==Some(h.slots[h.origin].as_str()) {
                let choice=o.choices[h.origin];
                if choice>0 && sample_start_ns>=h.started_ns {
                    h.acknowledged=true;h.chosen_skill=choice;
                }
            }
            // Start dragging inside the origin before crossing into the other
            // button. A jump straight outside the origin did not acquire the
            // native curve owner in live tests. The short first leg is spatial,
            // not a timed sleep; the native drag-owner receipt advances it.
            let origin=h.geometry.endpoints[h.origin];
            let destination=h.geometry.endpoints[1-h.origin];
            let point=if o.source_slot.as_deref()==Some(h.slots[h.origin].as_str()) {
                destination
            } else {origin+(destination-origin)*0.05};
            if !h.acknowledged && !transport.motion(h.contact,point) {
                let h=self.active.remove(i);
                Self::retire_cancelled(transport, &h);
                if h.pressed {self.ignored.push(h.key);}
                continue;
            }
            if !h.acknowledged && h.probe_move_ns.is_none() {h.probe_move_ns=Some(now);}
            if h.acknowledged && !h.pressed {
                let h=self.active.remove(i);
                transport.motion(h.contact,h.geometry.endpoints[1-h.origin]);
            transport.up(h.contact,h.geometry.endpoints[1-h.origin]);continue;
            }
            i+=1;
        }
    }
    pub fn reset(&mut self, transport:&mut impl Transport) {
        if self.active.iter().any(|h|h.geometry.cancel.is_none()) && transport.abort_all() {
            self.active.clear();self.ignored.clear();return;
        }
        for h in self.active.drain(..) {
            Self::retire_cancelled(transport, &h);
        }
        self.ignored.clear();
    }
    fn retire_cancelled(transport: &mut impl Transport, h: &Held) {
        if let Some(point)=h.geometry.cancel {
            transport.motion(h.contact,point);
            if !transport.abort(h.contact,point) {transport.up(h.contact,point);}
        } else {
            transport.abort(h.contact,h.geometry.endpoints[h.origin]);
        }
    }
    pub fn cancel(&mut self, now: i64) {
        for h in &mut self.active {
            h.cancel_requested_ns.get_or_insert(now);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]struct Sink(Vec<(u8,i32,Point)>);
    impl Transport for Sink {
        fn down(&mut self,p:Point)->i32{let id=7;self.0.push((0,id,p));id}
        fn motion(&mut self,id:i32,p:Point)->bool{self.0.push((1,id,p));true}
        fn up(&mut self,id:i32,p:Point){self.0.push((2,id,p));}
    }
    fn geometry()->Geometry{Geometry {epoch:"match".into(),endpoints:[Point::new(0.7,0.8),Point::new(0.9,0.6)],cancel:Some(Point::new(0.9,0.2))}}
    fn observed()->Observation{Observation {endpoints:["skill:0".into(),"skill:1".into()],source_slot:None,pressed_slot:None,enough_energy:true,choices:[0,0],base_skills:[1234,5678],on_cooldown:[false,false]}}
    #[test]
    fn unknown_cancel_geometry_uses_solo_native_cancel_without_an_up() {
        struct Solo(Sink);
        impl Transport for Solo {
            fn down(&mut self,p:Point)->i32{self.0.down(p)}
            fn motion(&mut self,id:i32,p:Point)->bool{self.0.motion(id,p)}
            fn up(&mut self,id:i32,p:Point){self.0.up(id,p)}
            fn abort(&mut self,id:i32,p:Point)->bool{self.0.0.push((3,id,p));true}
        }
        let(mut s,mut g,o)=(Solo(Sink::default()),Gestures::default(),observed());
        let mut geo=geometry();geo.cancel=None;g.begin(&mut s,87,0,geo,&o,1);
        g.cancel(2);g.release(&mut s,87);g.poll(&mut s,"match",Some(&o),0,3,3,None);
        assert!(g.active.is_empty());assert_eq!(s.0.0.last().unwrap().0,3);
        assert!(!s.0.0.iter().any(|v|v.0==2));
    }
    #[test]
    fn hidden_cancel_widget_does_not_block_a_combo_or_normal_release() {
        for origin in 0..2 {
            let(mut s,mut g,mut o)=(Sink::default(),Gestures::default(),observed());
            let mut geo=geometry();geo.cancel=None;o.on_cooldown=[true,true];
            g.begin(&mut s,87,origin,geo,&o,1);assert_eq!(g.active.len(),1);
            o.pressed_slot=Some(o.endpoints[origin].clone());
            g.poll(&mut s,"match",Some(&o),0,2,2,None);
            o.source_slot=Some(o.endpoints[origin].clone());
            g.poll(&mut s,"match",Some(&o),0,3,3,None);
            o.choices[origin]=9000+origin as i32;
            g.poll(&mut s,"match",Some(&o),0,4,4,None);
            assert!(g.active[0].acknowledged);
            g.release(&mut s,87);assert!(g.active.is_empty());
            assert_eq!(s.0.last().unwrap(),&(2,7,geometry().endpoints[1-origin]));
        }
    }
    #[test]
    fn cancel_waits_for_real_geometry_without_releasing_at_the_skill_even_after_a_timeout() {
        let(mut s,mut g,o)=(Sink::default(),Gestures::default(),observed());
        let mut geo=geometry();geo.cancel=None;g.begin(&mut s,87,0,geo,&o,1);
        g.cancel(2);g.release(&mut s,87);
        for now in [3,1_000_000_000,2_000_000_000] {
            g.poll(&mut s,"match",Some(&o),0,now,now,None);
            assert_eq!(s.0.len(),1);assert_eq!(g.active.len(),1);
        }
        let point=geometry().cancel.unwrap();
        let feedback=|sample_start_ns|Some(CancelFeedback {slot:"skill:0",sample_start_ns,point:Some(point)});
        g.poll(&mut s,"match",Some(&o),0,3_000_000_000,3_000_000_000,feedback(3_000_000_000));
        assert_eq!(s.0.last().unwrap(),&(1,7,point));assert_eq!(g.active.len(),1);
        g.poll(&mut s,"match",Some(&o),0,3_000_000_001,3_000_000_001,feedback(3_000_000_001));
        assert_eq!(s.0.last().unwrap(),&(2,7,point));assert!(g.active.is_empty());
    }
    #[test]
    fn context_reset_with_unknown_cancel_geometry_uses_native_cancel_not_a_skill_up() {
        struct ResetSink(Sink);
        impl Transport for ResetSink {
            fn down(&mut self,p:Point)->i32{self.0.down(p)}
            fn motion(&mut self,id:i32,p:Point)->bool{self.0.motion(id,p)}
            fn up(&mut self,id:i32,p:Point){self.0.up(id,p)}
            fn abort_all(&mut self)->bool{self.0.0.push((4,-1,Point::default()));true}
        }
        let(mut s,mut g,o)=(ResetSink(Sink::default()),Gestures::default(),observed());
        let mut geo=geometry();geo.cancel=None;g.begin(&mut s,87,0,geo,&o,1);
        g.reset(&mut s);assert!(g.active.is_empty());
        assert_eq!(s.0.0.last().unwrap().0,4);
        assert!(!s.0.0.iter().any(|v|v.0==2));
    }
    #[test]
    fn combo_ignores_parent_cooldown_and_bootstraps_without_parent_aim_receipt() {
        let(mut s,mut g,mut o)=(Sink::default(),Gestures::default(),observed());
        o.on_cooldown=[true,true];
        g.begin(&mut s,69,0,geometry(),&o,1);
        assert_eq!(g.active.len(),1);
        g.poll(&mut s,"match",Some(&o),0,2,2,None);
        assert_eq!(s.0.len(),1);
        o.pressed_slot=Some("skill:0".into());
        g.poll(&mut s,"match",Some(&o),0,3,3,None);
        assert_eq!(s.0.last().unwrap().0,1);
        assert_ne!(s.0.last().unwrap().2,geometry().endpoints[1]);
        o.source_slot=Some("skill:0".into());
        g.poll(&mut s,"match",Some(&o),0,3,3,None);
        assert_eq!(s.0.last().unwrap().2,geometry().endpoints[1]);
        o.choices[0]=9876;
        g.poll(&mut s,"match",Some(&o),0,4,4,None);
        assert!(g.active[0].acknowledged);assert_eq!(g.active[0].chosen_skill,9876);
        g.release(&mut s,69);
        assert!(g.active.is_empty());
        assert_eq!(&s.0[s.0.len()-2..],&[(1,7,geometry().endpoints[1]),(2,7,geometry().endpoints[1])]);
    }
    #[test]
    fn combo_without_passive_never_substitutes_an_ordinary_skill() {
        let(mut s,mut g,mut o)=(Sink::default(),Gestures::default(),observed());
        o.enough_energy=false;g.begin(&mut s,69,0,geometry(),&o,1);
        assert!(s.0.is_empty());assert!(g.active.is_empty());
        o.enough_energy=true;g.begin(&mut s,69,0,geometry(),&o,2);
        assert!(s.0.is_empty());g.release(&mut s,69);
        g.begin(&mut s,69,0,geometry(),&o,3);assert_eq!(s.0.len(),1);
    }
    #[test] fn acknowledged_combo_holds_same_contact_without_any_cursor_input() {
        let(mut s,mut g,mut o)=(Sink::default(),Gestures::default(),observed());
        g.begin(&mut s,69,0,geometry(),&o,1);
        g.poll(&mut s,"match",Some(&o),0,2,2,None);assert_eq!(s.0.len(),1);
        o.pressed_slot=Some("skill:0".into());
        g.poll(&mut s,"match",Some(&o),0,0,2,None);assert_eq!(s.0.len(),1);
        g.poll(&mut s,"match",Some(&o),0,2,2,None);assert!(s.0.len()>=2);
        assert_ne!(s.0.last().unwrap().2,geometry().endpoints[1]);
        o.source_slot=Some("skill:0".into());g.poll(&mut s,"match",Some(&o),0,3,3,None);
        assert_eq!(s.0.last().unwrap().2,geometry().endpoints[1]);
        o.choices[0]=1234;g.poll(&mut s,"match",Some(&o),1234,4,4,None);
        let count=s.0.len();o.enough_energy=false;
        for now in [5,500_000_000,10_000_000_000] {g.poll(&mut s,"match",Some(&o),1234,now,now,None);}
        assert_eq!(s.0.len(),count);assert_eq!(g.active.len(),1);
        assert!(g.release(&mut s,69));assert!(g.active.is_empty());
        assert_eq!(s.0.last().unwrap().0,2);assert!(s.0.iter().all(|e|e.1==7));
    }
    #[test] fn early_up_waits_for_current_choice_not_a_fixed_delay_or_foreign_source() {
        let(mut s,mut g,mut o)=(Sink::default(),Gestures::default(),observed());
        g.begin(&mut s,82,1,geometry(),&o,1);g.release(&mut s,82);
        o.choices[1]=5678;o.pressed_slot=Some("skill:1".into());o.source_slot=Some("skill:0".into());
        g.poll(&mut s,"match",Some(&o),5678,2,2,None);assert!(!g.active[0].acknowledged);
        o.source_slot=Some("skill:1".into());
        g.poll(&mut s,"match",Some(&o),1234,3,3,None);assert!(g.active.is_empty());
        assert_eq!(s.0.last().unwrap().2,geometry().endpoints[0]);
    }
    #[test] fn unavailable_press_requires_new_down_and_lost_match_cancels_only_owned_contact() {
        let(mut s,mut g,mut o)=(Sink::default(),Gestures::default(),observed());o.enough_energy=false;
        g.begin(&mut s,69,0,geometry(),&o,1);o.enough_energy=true;
        g.begin(&mut s,69,0,geometry(),&o,2);assert!(s.0.is_empty());g.release(&mut s,69);
        g.begin(&mut s,69,0,geometry(),&o,3);g.poll(&mut s,"another_match",Some(&o),0,4,4,None);
        assert!(g.active.is_empty());assert_eq!(s.0.last().unwrap().2,geometry().cancel.unwrap());
        assert!(g.contains(69));g.release(&mut s,69);assert!(!g.contains(69));
    }
    #[test]
    fn cancelling_an_acknowledged_combo_blocks_normal_up_and_waits_for_fresh_receipt() {
        let (mut sink, mut gestures, observation) = (Sink::default(), Gestures::default(), observed());
        gestures.begin(&mut sink, 69, 0, geometry(), &observation, 1);
        gestures.active[0].acknowledged = true;
        gestures.cancel(5);
        gestures.release(&mut sink, 69);
        let feedback = |sample_start_ns| Some(CancelFeedback {
            slot: "skill:0", sample_start_ns, point: Some(geometry().cancel.unwrap()),
        });
        gestures.poll(&mut sink, "match", Some(&observation), 1234, 4, 6, feedback(4));
        assert_eq!(sink.0.last().unwrap().0, 1);
        assert_eq!(sink.0.last().unwrap().2, geometry().cancel.unwrap());
        gestures.poll(&mut sink, "match", Some(&observation), 1234, 5, 7, feedback(5));
        assert!(!gestures.active.is_empty());
        gestures.poll(&mut sink, "match", Some(&observation), 1234, 7, 8, feedback(7));
        assert!(gestures.active.is_empty());
        assert_eq!(sink.0.last().unwrap(), &(2, 7, geometry().cancel.unwrap()));
    }
}
