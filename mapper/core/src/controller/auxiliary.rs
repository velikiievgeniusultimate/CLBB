//! Auxiliary native holds never own the primary skill or movement lifecycle.
use super::*;

pub(crate) const AUXILIARY_SOURCE: &str = "UIKeySkill.auxiliary_controls";
const FRESH_NS: i64 = 100_000_000;

#[derive(Clone, Debug)]
pub(crate) struct AuxiliaryHold {
    pub(crate) id: i32,
    pub(crate) key: i32,
    pub(crate) slot: String,
    pub(crate) point: Point,
    pub(crate) owner_id: u64,
    pub(crate) down_ns: i64,
    pub(crate) cancel_supported: snapshot::Flag,
    hero: i32,
    epoch: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{api::Input, host::Binding};
    const T: i64 = 4_000_000_000;
    const MS: i64 = 1_000_000;
    const OWNER: u64 = 0xffff_ffff_ffff_fffb;
    #[derive(Default)]
    struct TestHost {
        next: i32,
        active: BTreeMap<i32, Point>,
        ended: Vec<i32>,
        fail_down: bool,
        attack_key: i32,
        attack_mode: i32,
        attack_aim: u8,
        clock: Option<i64>,
        abort_ok: bool,
        abort_calls: usize,
        generation: u64,
    }
    impl Host for TestHost {
        fn clock_ns(&mut self) -> Option<i64> { self.clock }
        fn begin(&mut self, p: Point) -> i32 {
            if self.fail_down { return -1; }
            let id = self.next; self.next += 1;
            self.active.insert(id, p); id
        }
        fn move_touch(&mut self, id: i32, p: Point) -> bool {
            let Some(old) = self.active.get_mut(&id) else { return false };
            *old = p; true
        }
        fn end(&mut self, id: i32, _: Point) {
            assert!(self.active.remove(&id).is_some(), "ended unknown contact {id}");
            self.ended.push(id);
        }
        fn abort_all_touches(&mut self) -> bool {
            self.abort_calls += 1;
            if !self.abort_ok { return false; }
            self.active.clear();
            self.generation = self.generation.wrapping_add(1);
            true
        }
        fn touch_reset_generation(&self) -> u64 { self.generation }
        fn pointer_world(&mut self, _: Point) -> Option<Point> { None }
        fn ground_world(&mut self, p: Point) -> Option<Point> { Some(p * 10.) }
        fn movement_direction(&mut self, _: Point) -> Option<Point> { None }
        fn hero_world(&mut self) -> Option<Point> { Some(Point::default()) }
        fn pointer_world_at_plane(&mut self, _: Point, _: f64) -> Option<Point> { None }
        fn capture_pointer_world(&mut self, _: Point) -> u64 { 0 }
        fn resolve_world(&mut self, _: u64) -> Option<Point> { None }
        fn release_resolver(&mut self, _: u64) {}
        fn navigation(&mut self, _: Point, p: Point, _: bool) -> Result<Vec<Point>, String> { Ok(vec![p]) }
        fn binding(&mut self, _: i32, b: &Button, _: &str) -> Binding {
            Binding { key: if b.kind == "attack" { self.attack_key } else { 81 },
                aim_type: if b.kind=="attack" { self.attack_aim } else {2}, cast_mode: if b.kind == "attack" { self.attack_mode } else { 0 }, stop_movement: false }
        }
        fn cancel_key(&mut self, _: i32) -> i32 { 84 }
        fn timer(&mut self, _: Timer, _: Option<i64>) {}
        fn trace(&mut self, _: &Controller, _: &str, _: &str) {}
    }
    fn n(value: f64) -> snapshot::Number { snapshot::Number { value, kind: 1 } }
    fn flag(value: bool) -> snapshot::Flag { snapshot::Flag { value: value as i32 } }
    fn text(s: &str) -> snapshot::Text { super::super::view::text(s) }
    fn v2(x: f64, y: f64) -> snapshot::Vector2 { snapshot::Vector2 { values: [n(x), n(y)], count: 2 } }
    fn frame(ms: i64, hold: Option<bool>) -> snapshot::Frame {
        let at = T + ms * MS;
        let mut f = snapshot::Frame {
            ready: flag(true), hero_id: n(97.), time_ns: n(at as f64), sample_duration_ms: n(0.25),
            skill_id: n(0.), token: text("auxiliary-match"), token_string: flag(true), token_nonempty: flag(true),
            hero_screen_uv: v2(0.5, 0.5), ..Default::default()
        };
        f.ui.ready = flag(true); f.ui.hero_id = f.hero_id; f.ui.time_ns = f.time_ns;
        f.ui.resolution_px = v2(1000., 1000.); f.ui.cancel_uv = v2(0.95, 0.2);
        f.ui.movement = snapshot::MovementGeometry {
            ready: flag(true), press_uv: v2(0.15, 0.85), center_uv: v2(0.15, 0.85), radius_uv: v2(0.1, 0.1),
        };
        f.ui.button_count = 2;
        for (i, (slot, kind, skill, x)) in [("attack", "attack", 9700, 0.92), ("skill:0", "skill", 9710, 0.8)].into_iter().enumerate() {
            f.ui.buttons[i] = snapshot::Button {
                slot: text(slot), kind: text(kind), index: n(i as f64), skill_id: n(skill as f64), active: flag(true),
                press_uv: v2(x, 0.89), center_uv: v2(x, 0.89), radius_uv: v2(0.1, 0.1), ..Default::default()
            };
        }
        f.auxiliary_controls = snapshot::AuxiliaryControlState {
            ready: flag(true), coherent: flag(true), source: text(AUXILIARY_SOURCE), token: f.token,
            hero_id: f.hero_id, sample_start_ns: n(at as f64 - 250_000.), sample_end_ns: f.time_ns,
            count: 1, ..Default::default()
        };
        f.auxiliary_controls.controls[0] = snapshot::AuxiliaryControlRow {
            slot: text("attack"), owner_present: flag(true), owner_id: OWNER,
            hold_to_release: hold.map(flag).unwrap_or_default(), cancel_supported: Default::default(),
        };
        f
    }
    fn send(c: &mut Controller, h: &mut TestHost, f: snapshot::Frame) {
        c.apply_snapshot(h, &f, f.time_ns.value as i64);
    }
    fn key(c: &mut Controller, h: &mut TestHost, ms: i64, code: i32, down: bool) {
        assert!(c.apply_input(h, Input::Key { code, down, repeat: false }, T + ms * MS));
    }
    fn setup(hold: Option<bool>) -> (Controller, TestHost) {
        let mut c = Controller::new();
        let mut h = TestHost { attack_key: 32, ..Default::default() };
        c.apply_input(&mut h, Input::Enabled(true), T);
        c.apply_input(&mut h, Input::HoldThreshold(40), T);
        send(&mut c, &mut h, frame(0, hold));
        c.apply_input(&mut h, Input::Mouse { button: 2, down: true, point: Point::new(0.7, 0.5) }, T + MS);
        send(&mut c, &mut h, frame(50, hold));
        assert!(c.movementHeld_);
        (c, h)
    }
    fn three_contacts() -> (Controller, TestHost, i32, i32, i32) {
        let (mut c, mut h) = setup(Some(true));
        key(&mut c, &mut h, 60, 32, true);
        let aux = c.auxiliaryHolds_[0].id;
        key(&mut c, &mut h, 61, 81, true);
        let primary = c.skillTouch_; let movement = c.movementTouch_;
        assert_eq!(h.active.len(), 3);
        assert!(aux != primary && aux != movement && primary != movement);
        (c, h, aux, primary, movement)
    }
    fn others(h: &TestHost, primary: i32, movement: i32) {
        assert!(h.active.contains_key(&primary) && h.active.contains_key(&movement));
        assert!(!h.ended.contains(&primary) && !h.ended.contains(&movement));
    }

    #[test]
    fn native_hold_survives_timers_and_data_switches_then_physical_up_precedes_stale_source() {
        let (mut c, mut h, aux, primary, movement) = three_contacts();
        for (ms, skill, next_hold) in [(100, 9700., Some(true)), (300, 9750., None), (500, 9753., Some(false))] {
            let mut f = frame(ms, next_hold);
            f.ui.buttons[0].skill_id = n(skill);
            send(&mut c, &mut h, f);
            c.apply_input(&mut h, Input::Timer(Timer::Auxiliary), T + (ms + 1) * MS);
            assert!(h.active.contains_key(&aux)); others(&h, primary, movement);
        }
        let view = c.inspect();
        assert_eq!(view.auxiliaryHoldCount_, 1);
        assert_eq!(view.auxiliaryHolds_[0].owner_id, OWNER);
        assert_eq!(view.auxiliaryHolds_[0].cancel_supported.value, -1);
        // No fresh frame for >250ms: UP must still end the old owned finger.
        key(&mut c, &mut h, 900, 32, false);
        assert_eq!(h.ended, vec![aux]);
        others(&h, primary, movement);
        assert_eq!(c.inspect().auxiliaryHoldCount_, 0);
    }

    #[test]
    fn unknown_optional_owner_never_releases_an_existing_hold() {
        for case in 0..10 {
            let (mut c, mut h, aux, primary, movement) = three_contacts();
            let mut f = frame(100, Some(true));
            match case {
                0 => f.auxiliary_controls = Default::default(),
                1 => f.auxiliary_controls.count = 0,
                2 => f.auxiliary_controls.controls[0].owner_present = Default::default(),
                3 => f.auxiliary_controls.token = text("foreign-match"),
                4 => f.auxiliary_controls.hero_id = n(25.),
                5 => f.auxiliary_controls.source = text("filtered_ui_buttons"),
                6 => { f.auxiliary_controls.controls[0].owner_present = flag(false);
                    f.auxiliary_controls.sample_start_ns = n((T + 59 * MS) as f64); }
                7 => { f.auxiliary_controls.controls[0].owner_present = flag(false);
                    f.auxiliary_controls.sample_end_ns = n((T + 101 * MS) as f64); }
                8 => { f.auxiliary_controls.controls[0].owner_present = flag(false);
                    f.auxiliary_controls.sample_start_ns.kind = 2; }
                9 => { f.auxiliary_controls.count = 2;
                    f.auxiliary_controls.controls[1] = f.auxiliary_controls.controls[0];
                    f.auxiliary_controls.controls[1].owner_present = flag(false); }
                _ => unreachable!(),
            }
            send(&mut c, &mut h, f);
            assert!(h.active.contains_key(&aux), "unknown owner case {case}");
            others(&h, primary, movement);
            key(&mut c, &mut h, 101, 32, false);
            assert_eq!(h.ended, vec![aux]);
        }
    }

    #[test]
    fn post_down_absence_or_owner_rebind_retires_only_aux_and_tombstones_until_real_up() {
        for absent in [false, true] {
            let (mut c, mut h, aux, primary, movement) = three_contacts();
            let mut f = frame(100, Some(true));
            f.auxiliary_controls.controls[0].owner_id -= 1;
            f.auxiliary_controls.controls[0].owner_present = flag(!absent);
            send(&mut c, &mut h, f);
            assert_eq!(h.ended, vec![aux]); others(&h, primary, movement);
            let count = h.next;
            key(&mut c, &mut h, 101, 32, true);
            assert_eq!(h.next, count);
            key(&mut c, &mut h, 102, 32, false);
            send(&mut c, &mut h, frame(103, Some(true)));
            key(&mut c, &mut h, 104, 32, true);
            assert_eq!(c.auxiliaryHolds_.len(), 1);
            assert!(c.auxiliaryHolds_[0].id != aux);
            others(&h, primary, movement);
        }
    }

    #[test]
    fn hold_admission_does_not_turn_unknown_known_family_into_tap_and_proven_normal_restores_tap() {
        let (mut c, mut h) = setup(Some(true));
        let mut f = frame(55, Some(true)); f.auxiliary_controls = Default::default();
        send(&mut c, &mut h, f);
        let count = h.next;
        key(&mut c, &mut h, 60, 32, true);
        assert_eq!(h.next, count); assert!(c.auxiliaryTaps_.is_empty());
        send(&mut c, &mut h, frame(65, Some(true)));
        key(&mut c, &mut h, 66, 32, true);
        assert_eq!(h.next, count, "recovery cannot automatically rearm a held physical key");
        key(&mut c, &mut h, 67, 32, false);
        send(&mut c, &mut h, frame(68, Some(false)));
        key(&mut c, &mut h, 69, 32, true);
        assert!(c.auxiliaryHolds_.is_empty()); assert_eq!(c.auxiliaryTaps_.len(), 1);
    }

    #[test]
    fn explicitly_instant_auxiliary_early_up_and_rapid_repress_keep_separate_sixteen_ms_deadlines() {
        for initial in [Some(false), None] {
            let (mut c, mut h) = setup(initial);
            h.attack_mode = 1;
            send(&mut c, &mut h, frame(55, initial));
            key(&mut c, &mut h, 60, 32, true);
            let first = c.auxiliaryTaps_[0].id;
            key(&mut c, &mut h, 61, 32, false);
            assert!(!h.ended.contains(&first));
            key(&mut c, &mut h, 68, 32, true);
            let second = c.auxiliaryTaps_[1].id;
            c.apply_input(&mut h, Input::Timer(Timer::Auxiliary), T + 76 * MS);
            assert_eq!(h.ended, vec![first]); assert!(h.active.contains_key(&second));
            key(&mut c, &mut h, 77, 32, false);
            c.apply_input(&mut h, Input::Timer(Timer::Auxiliary), T + 84 * MS);
            assert_eq!(h.ended, vec![first, second]);
        }
    }

    #[test]
    fn attack_aim_is_opt_in_and_mode_revocation_retires_only_the_skill_contact() {
        let(mut c,mut h)=setup(Some(false));let movement=c.movementTouch_;
        key(&mut c,&mut h,60,32,true);
        assert_eq!(c.skillTouch_,-1);assert_eq!(c.auxiliaryTaps_.len(),1);
        key(&mut c,&mut h,61,32,false);
        h.attack_aim=3;send(&mut c,&mut h,frame(65,Some(false)));
        key(&mut c,&mut h,66,32,true);
        let aim=c.skillTouch_;assert!(aim>=0);assert!(c.auxiliaryTaps_.is_empty());
        h.attack_aim=1;send(&mut c,&mut h,frame(70,Some(false)));
        assert_eq!(c.skillTouch_,-1);assert!(!h.active.contains_key(&aim));assert!(h.active.contains_key(&movement));
        let count=h.next;key(&mut c,&mut h,71,32,true);assert_eq!(h.next,count);
        key(&mut c,&mut h,72,32,false);key(&mut c,&mut h,73,32,true);
        assert_eq!(c.skillTouch_,-1);assert_eq!(c.auxiliaryTaps_.len(),1);
    }
    #[test]
    fn keyboard_movement_is_immediate_diagonal_normalized_and_independent_of_pointer_and_skills() {
        let (mut c, mut h) = setup(Some(false));
        c.apply_input(&mut h, Input::MovementMode(2), T + 55 * MS);
        assert!(c.apply_input(&mut h, Input::MovementKey { direction:0, down:true }, T + 56 * MS));
        let finger=c.movementTouch_;
        let up=h.active[&finger];
        assert!(up.y<c.movementCenter_.y && (up.x-c.movementCenter_.x).abs()<1e-8);
        c.apply_input(&mut h, Input::MovementKey { direction:3, down:true }, T + 57 * MS);
        let diagonal=h.active[&finger];
        assert!(diagonal.x>up.x && diagonal.y>up.y);
        let radius=distance(c.pixels(diagonal),c.pixels(c.movementCenter_));
        assert!((radius-c.radius_pixels(c.movementRadius_)).abs()<1e-6);
        c.apply_input(&mut h, Input::Pointer(Point::new(0.1,0.9)), T + 58 * MS);
        assert_eq!(h.active[&finger],diagonal);
        key(&mut c,&mut h,59,81,true);
        let primary=c.skillTouch_;
        assert!(primary>=0 && primary!=finger);
        c.apply_input(&mut h, Input::MovementKey { direction:0, down:false }, T + 60 * MS);
        assert!((h.active[&finger].y-c.movementCenter_.y).abs()<1e-8);
        c.apply_input(&mut h, Input::MovementKey { direction:3, down:false }, T + 61 * MS);
        assert!(!h.active.contains_key(&finger));
        assert!(h.active.contains_key(&primary));
    }
    #[test]
    fn opposite_keyboard_keys_neutralize_and_release_resumes_remaining_direction() {
        let (mut c,mut h)=setup(Some(false));
        c.apply_input(&mut h,Input::MovementMode(2),T+55*MS);
        c.apply_input(&mut h,Input::MovementKey {direction:2,down:true},T+56*MS);
        c.apply_input(&mut h,Input::MovementKey {direction:3,down:true},T+57*MS);
        assert_eq!(c.movementTouch_,-1);
        c.apply_input(&mut h,Input::MovementKey {direction:3,down:false},T+58*MS);
        assert!(c.movementTouch_>=0);
        assert!(h.active[&c.movementTouch_].x<c.movementCenter_.x);
        c.apply_input(&mut h,Input::Release,T+59*MS);
        assert_eq!(c.movementKeys_,[false;4]);
        assert!(h.active.is_empty());
    }
    #[test]
    fn every_verified_alternate_skill_id_selects_its_own_mode_slot() {
        let mut c=Controller::new();
        for (hero,slot,base,active) in [(25,"skill:1",2520,2521),(25,"skill:2",2530,2531),(84,"skill:1",8420,8425),(131,"skill:2",13130,13131)] {
            c.heroId_=hero;
            let b=Button {slot:slot.into(),kind:"skill".into(),skillId:base,..Default::default()};
            c.skillVariants_=vec![SkillVariant {slot:slot.into(),base,active,..Default::default()}];
            assert_eq!(c.current_config_slot(&b),format!("{slot}:{active}"));
            c.skillVariants_[0].active=base;
            assert_eq!(c.current_config_slot(&b),slot);
        }
    }

    #[test]
    fn default_auxiliary_holds_until_physical_up_and_preserves_other_fingers() {
        let (mut c, mut h) = setup(Some(false));
        let movement = c.movementTouch_;
        key(&mut c, &mut h, 60, 32, true);
        let aux = c.auxiliaryTaps_[0].id;
        key(&mut c, &mut h, 61, 81, true);
        let primary = c.skillTouch_;
        c.apply_input(&mut h, Input::Timer(Timer::Auxiliary), T + 90 * MS);
        assert!(h.active.contains_key(&aux));
        others(&h, primary, movement);
        key(&mut c, &mut h, 91, 32, false);
        assert_eq!(h.ended, vec![aux]);
        others(&h, primary, movement);
        assert!(c.auxiliaryTaps_.is_empty());
        key(&mut c, &mut h, 92, 32, true);
        let second = c.auxiliaryTaps_[0].id;
        key(&mut c, &mut h, 93, 32, false);
        assert_eq!(h.ended, vec![aux, second]);
    }

    #[test]
    fn failed_hold_down_repeat_and_changed_binding_cannot_create_or_orphan_a_contact() {
        let (mut c, mut h) = setup(Some(true));
        h.fail_down = true;
        key(&mut c, &mut h, 60, 32, true);
        assert!(c.auxiliaryHolds_.is_empty());
        h.fail_down = false;
        let count = h.next;
        key(&mut c, &mut h, 61, 32, true);
        assert_eq!(h.next, count);
        key(&mut c, &mut h, 62, 32, false);
        key(&mut c, &mut h, 63, 32, true);
        let aux = c.auxiliaryHolds_[0].id;
        h.attack_key = 88;
        send(&mut c, &mut h, frame(70, Some(true)));
        c.apply_input(&mut h, Input::Key { code: 32, down: false, repeat: true }, T + 71 * MS);
        assert!(h.active.contains_key(&aux));
        key(&mut c, &mut h, 72, 32, false);
        assert_eq!(h.ended, vec![aux]);
        key(&mut c, &mut h, 73, 88, true);
        assert_eq!(c.auxiliaryHolds_.len(), 1);
    }

    #[test]
    fn user_cancel_only_addresses_primary_and_never_fakes_auxiliary_charge_cancel() {
        let (mut c, mut h, aux, primary, movement) = three_contacts();
        key(&mut c, &mut h, 70, 84, true);
        assert!(c.skillCancelPending_);
        assert!(h.active.contains_key(&aux)); others(&h, primary, movement);
        assert_eq!(h.active.len(), 3);
        key(&mut c, &mut h, 71, 32, false);
        assert_eq!(h.ended, vec![aux]); others(&h, primary, movement);
    }

    #[test]
    fn owner_revocation_uses_actual_transport_down_timestamp_not_input_request_time() {
        let (mut c, mut h) = setup(Some(true));
        h.clock = Some(T + 70 * MS);
        key(&mut c, &mut h, 60, 32, true);
        let aux = c.auxiliaryHolds_[0].id;
        assert_eq!(c.auxiliaryHolds_[0].down_ns, T + 70 * MS);
        h.clock = None;
        let mut f = frame(80, Some(true));
        f.auxiliary_controls.sample_start_ns = n((T + 65 * MS) as f64);
        f.auxiliary_controls.controls[0].owner_present = flag(false);
        send(&mut c, &mut h, f);
        assert!(h.active.contains_key(&aux), "overlapping pre-DOWN source read is not revocation");
        let mut f = frame(90, Some(true));
        f.auxiliary_controls.controls[0].owner_present = flag(false);
        send(&mut c, &mut h, f);
        assert_eq!(h.ended, vec![aux]);
    }

    #[test]
    fn global_context_cancel_forgets_all_contacts_before_cleanup_without_up_or_resend() {
        for cause in 0..6 {
            let (mut c, mut h, _, _, _) = three_contacts();
            h.abort_ok = true;
            match cause {
                0 => { c.apply_input(&mut h, Input::Release, T + 70 * MS); }
                1 => { c.apply_input(&mut h, Input::Invalid("lost source"), T + 70 * MS); }
                2 => { let mut f = frame(70, Some(true)); f.ready = flag(false); send(&mut c, &mut h, f); }
                3 => { let mut f = frame(70, Some(true)); f.hero_id = n(98.); f.ui.hero_id = f.hero_id;
                    f.auxiliary_controls.hero_id = f.hero_id; send(&mut c, &mut h, f); }
                4 => { let mut f = frame(70, Some(true)); f.token = text("new-epoch");
                    f.auxiliary_controls.token = f.token; send(&mut c, &mut h, f); }
                5 => { let mut f = frame(70, Some(true)); f.ui.resolution_px = v2(1200., 1000.);
                    send(&mut c, &mut h, f); }
                _ => unreachable!(),
            }
            assert_eq!(h.abort_calls, 1, "context cause {cause}");
            assert!(h.active.is_empty() && h.ended.is_empty(), "UP after global CANCEL, cause {cause}");
            assert_eq!(c.skillTouch_, -1); assert_eq!(c.movementTouch_, -1);
            assert!(c.auxiliaryHolds_.is_empty() && c.auxiliaryTaps_.is_empty());
            assert!(c.queuedSkills_.is_empty() && c.auxiliaryHeldKeys_.is_empty());
            let before = h.next;
            c.apply_input(&mut h, Input::Timer(Timer::Pointer), T + 71 * MS);
            c.apply_input(&mut h, Input::Timer(Timer::Auxiliary), T + 72 * MS);
            c.apply_input(&mut h, Input::Timer(Timer::Main), T + 73 * MS);
            assert!(h.active.is_empty() && h.ended.is_empty());
            assert_eq!(h.next, before, "context cancellation cannot rearm logical contacts");
        }
    }

    #[test]
    fn context_abort_failure_keeps_legacy_cleanup_and_raw_only_context_is_not_cancelled() {
        let (mut c, mut h, aux, primary, movement) = three_contacts();
        c.apply_input(&mut h, Input::Release, T + 70 * MS);
        assert_eq!(h.abort_calls, 1);
        assert!(h.active.is_empty());
        assert_eq!(h.ended.len(), 3);
        assert!(h.ended.contains(&aux) && h.ended.contains(&primary) && h.ended.contains(&movement));

        let mut c = Controller::new();
        let mut h = TestHost { attack_key: 32, abort_ok: true, ..Default::default() };
        c.apply_input(&mut h, Input::Enabled(true), T);
        send(&mut c, &mut h, frame(0, Some(true)));
        h.active.insert(99, Point::new(0.2, 0.2)); // foreign diagnostic raw UI touch
        let mut invalid = frame(10, Some(true)); invalid.ready = flag(false);
        send(&mut c, &mut h, invalid);
        c.apply_input(&mut h, Input::Release, T + 11 * MS);
        assert_eq!(h.abort_calls, 0, "no Core-owned contact means no global cancel request");
        assert!(h.active.contains_key(&99));
    }

    #[test]
    fn individual_up_owner_cleanup_and_user_cancel_never_request_global_abort() {
        let (mut c, mut h, aux, primary, movement) = three_contacts();
        h.abort_ok = true;
        key(&mut c, &mut h, 70, 84, true);
        assert_eq!(h.abort_calls, 0); others(&h, primary, movement);
        key(&mut c, &mut h, 71, 32, false);
        assert_eq!(h.ended, vec![aux]); assert_eq!(h.abort_calls, 0);
        key(&mut c, &mut h, 72, 32, true);
        let second = c.auxiliaryHolds_[0].id;
        let mut f = frame(80, Some(true)); f.auxiliary_controls.controls[0].owner_id -= 1;
        send(&mut c, &mut h, f);
        assert!(h.ended.contains(&second)); assert_eq!(h.abort_calls, 0);
        others(&h, primary, movement);
    }

    #[test]
    fn externally_cancelled_generation_clears_logical_owners_before_input_or_frame_cleanup() {
        for via_snapshot in [false, true] {
            let (mut c, mut h, _, _, _) = three_contacts();
            h.abort_ok = true;
            assert!(h.abort_all_touches()); // Runtime already cancelled every input owner.
            let count = h.next;
            if via_snapshot {
                send(&mut c, &mut h, frame(70, Some(true)));
            } else {
                c.apply_input(&mut h, Input::Release, T + 70 * MS);
            }
            assert_eq!(h.abort_calls, 1, "Core must not send a second context cancellation");
            assert!(h.ended.is_empty() && h.active.is_empty());
            assert_eq!(c.skillTouch_, -1); assert_eq!(c.movementTouch_, -1);
            assert!(c.auxiliaryHolds_.is_empty() && c.auxiliaryHeldKeys_.is_empty());
            c.apply_input(&mut h, Input::Timer(Timer::Pointer), T + 71 * MS);
            assert_eq!(h.next, count); assert!(h.ended.is_empty() && h.active.is_empty());
        }
    }

    #[test]
    fn reversed_optional_projection_cannot_downgrade_known_hold_admission() {
        let (mut c, mut h) = setup(Some(true));
        send(&mut c, &mut h, frame(70, Some(true)));
        // A newly delivered outer frame can contain an older optional read.
        let mut f = frame(80, Some(false));
        f.auxiliary_controls.sample_start_ns = n((T + 64 * MS) as f64);
        f.auxiliary_controls.sample_end_ns = n((T + 65 * MS) as f64);
        send(&mut c, &mut h, f);
        let count = h.next;
        key(&mut c, &mut h, 81, 32, true);
        assert_eq!(h.next, count);
        assert!(c.auxiliaryTaps_.is_empty() && c.auxiliaryHolds_.is_empty());
    }

    #[test]
    fn held_owner_projection_is_independent_of_optional_button_geometry_and_availability() {
        for missing_geometry in [false, true] {
            let (mut c, mut h, aux, primary, movement) = three_contacts();
            let mut f = frame(100, Some(true));
            if missing_geometry { f.ui.buttons[0].press_uv = Default::default(); }
            else { f.ui.buttons[0].active = flag(false); }
            send(&mut c, &mut h, f);
            assert!(h.active.contains_key(&aux)); others(&h, primary, movement);
            key(&mut c, &mut h, 101, 32, false);
            assert_eq!(h.ended, vec![aux]); others(&h, primary, movement);
        }
    }

    #[test]
    fn global_retirement_blocks_duplicate_physical_down_and_repeat_until_real_up() {
        for cause in 0..3 {
            let (mut c, mut h, _, _, _) = three_contacts();
            h.abort_ok = true;
            match cause {
                0 => { c.apply_input(&mut h, Input::Release, T + 70 * MS); }
                1 => { let mut f = frame(70, Some(true)); f.ready = flag(false); send(&mut c, &mut h, f); }
                2 => { assert!(h.abort_all_touches());
                    c.apply_input(&mut h, Input::Timer(Timer::Main), T + 70 * MS); }
                _ => unreachable!(),
            }
            send(&mut c, &mut h, frame(80, Some(true)));
            let count = h.next;
            for code in [32, 81] {
                key(&mut c, &mut h, 81, code, true);
                assert!(c.apply_input(&mut h, Input::Key { code, down: true, repeat: true }, T + 82 * MS));
            }
            assert!(c.apply_input(&mut h, Input::Mouse { button: 2, down: true, point: Point::new(0.7, 0.5) }, T + 83 * MS));
            c.apply_input(&mut h, Input::Timer(Timer::Pointer), T + 84 * MS);
            c.apply_input(&mut h, Input::Timer(Timer::Movement), T + 85 * MS);
            assert_eq!(h.next, count, "duplicate physical DOWN rearmed after context cause {cause}");
            assert!(h.active.is_empty() && h.ended.is_empty());
            key(&mut c, &mut h, 90, 32, false);
            key(&mut c, &mut h, 91, 81, false);
            assert!(c.apply_input(&mut h, Input::Mouse { button: 2, down: false, point: Point::new(0.7, 0.5) }, T + 92 * MS));
            send(&mut c, &mut h, frame(100, Some(true)));
            key(&mut c, &mut h, 101, 32, true);
            key(&mut c, &mut h, 102, 81, true);
            c.apply_input(&mut h, Input::Mouse { button: 2, down: true, point: Point::new(0.7, 0.5) }, T + 103 * MS);
            send(&mut c, &mut h, frame(150, Some(true)));
            assert_eq!(h.active.len(), 3, "fresh physical press must work after all real UPs");
        }
    }

    #[test]
    fn globally_retiring_a_tap_after_its_physical_up_does_not_tombstone_the_released_key() {
        let (mut c, mut h) = setup(Some(false));
        h.attack_mode = 1;
        send(&mut c, &mut h, frame(55, Some(false)));
        key(&mut c, &mut h, 60, 32, true);
        key(&mut c, &mut h, 61, 32, false);
        assert_eq!(c.auxiliaryTaps_.len(), 1);
        h.abort_ok = true;
        c.apply_input(&mut h, Input::Release, T + 65 * MS);
        send(&mut c, &mut h, frame(70, Some(false)));
        let count = h.next;
        key(&mut c, &mut h, 71, 32, true);
        assert_eq!(h.next, count + 1, "already released physical key must allow a fresh DOWN");
        assert_eq!(c.auxiliaryTaps_.len(), 1);
    }
}

impl Controller {
    pub(crate) fn tombstone_physical_inputs(&mut self) {
        self.contextIgnoredKeys_.extend(self.physicalKeys_.iter().copied());
        self.contextIgnoredMouseButtons_.extend(self.physicalMouseButtons_.iter().copied());
    }
    fn forget_owned_touch_ids(&mut self) {
        self.skillTouch_ = -1;
        self.movementTouch_ = -1;
        self.manualTouch_ = -1;
        self.auxiliaryTaps_.clear();
        self.auxiliaryHolds_.clear();
        self.skillEchoPending_ = false;
        self.movementEchoPending_ = false;
    }
    pub(crate) fn observe_host_touch_reset(&mut self, host: &mut impl Host) {
        let generation = host.touch_reset_generation();
        if generation == self.touchResetGeneration_ { return; }
        self.touchResetGeneration_ = generation;
        self.forget_owned_touch_ids();
        self.release_all(host);
        self.trace(host, "context_abort", "host_generation_retired_all_contacts");
    }
    pub(crate) fn abort_owned_context(&mut self, host: &mut impl Host) {
        if self.skillTouch_ < 0 && self.movementTouch_ < 0 && self.manualTouch_ < 0
            && self.auxiliaryTaps_.is_empty() && self.auxiliaryHolds_.is_empty()
        { return; }
        if !host.abort_all_touches() {
            self.trace(host, "context_abort", "transport_failed_legacy_cleanup");
            return;
        }
        // ACTION_CANCEL already retired the entire native pointer set. Forget
        // IDs before logical cleanup so no charged UP or deferred MOVE follows.
        self.touchResetGeneration_ = host.touch_reset_generation();
        self.trace(host, "context_abort", "all_owned_contacts_cancelled");
        self.forget_owned_touch_ids();
    }
    fn auxiliary_scope(&self, s: &snapshot::AuxiliaryControlState) -> bool {
        let start = s.sample_start_ns.get(-1.);
        let end = s.sample_end_ns.get(-1.);
        s.ready.get() && s.coherent.get()
            && s.source.as_str() == AUXILIARY_SOURCE
            && s.token.as_str() == self.frameEpoch_
            && !self.frameEpoch_.is_empty()
            && s.hero_id.is_number() && s.hero_id.value == self.heroId_ as f64
            && s.count <= 8
            && s.sample_start_ns.is_number() && s.sample_end_ns.is_number()
            && start.is_finite() && end.is_finite()
            && start >= 0. && end >= start && end - start <= FRESH_NS as f64
            && end <= self.now_ as f64
            && self.now_ as f64 - end <= FRESH_NS as f64
            && {
                let mut slots = BTreeSet::new();
                s.controls[..s.count as usize].iter().all(|row| {
                    let slot = row.slot.as_str();
                    !slot.is_empty() && slot.len() <= 64 && slots.insert(slot)
                        && (row.owner_present.missing() || row.owner_present.is_bool())
                        && (!row.owner_present.get() || row.owner_id != 0)
                })
            }
    }
    pub(crate) fn ingest_auxiliary_controls(&mut self, host: &mut impl Host, f: &snapshot::Frame) {
        self.auxiliaryControlState_ = Default::default();
        let s = &f.auxiliary_controls;
        if !f.token_string.get() || !f.token_nonempty.get() || !self.auxiliary_scope(s)
            || s.sample_end_ns.value < self.auxiliaryControlSampleNs_ as f64
        {
            return;
        }
        self.auxiliaryControlState_ = *s;
        self.auxiliaryControlSampleNs_ = s.sample_end_ns.value as i64;
        // This projection is independent of geometry and availability filters.
        // Missing/unknown optional rows cannot mean a native owner vanished.
        for row in &s.controls[..s.count as usize] {
            if row.owner_present.get() && row.hold_to_release.is_bool() {
                if row.hold_to_release.get() {
                    self.auxiliaryKnownHolds_.insert(row.slot.owned());
                } else {
                    self.auxiliaryKnownHolds_.remove(row.slot.as_str());
                }
            }
        }
        for i in (0..self.auxiliaryHolds_.len()).rev() {
            let hold = &self.auxiliaryHolds_[i];
            if hold.hero != self.heroId_ || hold.epoch != self.frameEpoch_
                || s.sample_start_ns.value < hold.down_ns as f64
            { continue; }
            let Some(row) = s.controls[..s.count as usize].iter()
                .find(|row| row.slot.as_str() == hold.slot) else { continue };
            let revoked = row.owner_present.is_bool()
                && (!row.owner_present.get() || row.owner_id != hold.owner_id);
            if revoked {
                self.trace(host, "auxiliary_hold", "post_down_native_owner_changed_cleanup");
                let hold = self.auxiliaryHolds_.remove(i);
                host.end(hold.id, hold.point);
                self.sync_clock(host);
                // Keep the physical-key tombstone until the real UP.
            }
        }
    }
    pub(crate) fn begin_auxiliary(&mut self, host: &mut impl Host, b: &Button) {
        let row = self.auxiliary_scope(&self.auxiliaryControlState_)
            .then(|| self.auxiliaryControlState_.controls[..self.auxiliaryControlState_.count as usize]
                .iter().find(|row| row.slot.as_str() == b.slot).copied())
            .flatten();
        let hold = row.filter(|row| row.owner_present.get() && row.hold_to_release.get());
        if let Some(row) = hold {
            if b.castMode == 1 {
                self.begin_auxiliary_tap(host, b);
                return;
            }
            if !b.available || self.auxiliaryHolds_.len() + self.auxiliaryTaps_.len() >= 8 {
                return;
            }
            let id = host.begin(b.press);
            if id < 0 { return; }
            self.sync_clock(host);
            self.auxiliaryHolds_.push(AuxiliaryHold {
                id, key: b.key, slot: b.slot.clone(), point: b.press,
                owner_id: row.owner_id, down_ns: self.now_,
                // Unsupported native cancellation is not synthesized by UP.
                cancel_supported: row.cancel_supported,
                hero: self.heroId_, epoch: self.frameEpoch_.clone(),
            });
            self.trace(host, "auxiliary_hold", "down_on_owned_native_button");
            return;
        }
        if self.auxiliaryKnownHolds_.contains(&b.slot)
            || row.is_some_and(|row| row.owner_present.is_bool() && !row.owner_present.get())
        {
            self.trace(host, "auxiliary_hold", "new_down_waits_for_current_native_owner");
            return;
        }
        self.begin_auxiliary_tap(host, b);
    }
    pub(crate) fn release_auxiliary_key(&mut self, host: &mut impl Host, key: i32) {
        for i in (0..self.auxiliaryTaps_.len()).rev() {
            if self.auxiliaryTaps_[i].key == key && self.auxiliaryTaps_[i].until == i64::MAX {
                let tap = self.auxiliaryTaps_.remove(i);
                host.end(tap.id, tap.point);
                self.sync_clock(host);
                self.mark_cooldown_cast(&tap.cooldownSlot, tap.cooldownVariant);
            }
        }
        for i in (0..self.auxiliaryHolds_.len()).rev() {
            if self.auxiliaryHolds_[i].key == key {
                self.trace(host, "auxiliary_hold", "physical_up");
                let hold = self.auxiliaryHolds_.remove(i);
                host.end(hold.id, hold.point);
                self.sync_clock(host);
            }
        }
    }
    pub(crate) fn finish_auxiliary_holds(&mut self, host: &mut impl Host) {
        for hold in std::mem::take(&mut self.auxiliaryHolds_) {
            host.end(hold.id, hold.point);
            self.sync_clock(host);
        }
    }
}
