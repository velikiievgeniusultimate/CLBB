//! Regressions for rejected range gestures, exercised through typed frames and
//! physical input events rather than direct calls to the recovery predicate.
use super::{api::Input, host::Binding, *};

const START: i64 = 1_000_000_000;

#[derive(Default)]
struct TestHost {
    next: i32,
    active: BTreeMap<i32, Point>,
    ended: Vec<i32>,
    traces: Vec<(String, String)>,
    aim_type: u8,
    cast_mode: i32,
}
impl Host for TestHost {
    fn begin(&mut self, p: Point) -> i32 {
        let id = self.next;
        self.next += 1;
        self.active.insert(id, p);
        id
    }
    fn move_touch(&mut self, id: i32, p: Point) -> bool {
        if let Some(old) = self.active.get_mut(&id) {
            *old = p;
            true
        } else {
            false
        }
    }
    fn end(&mut self, id: i32, _: Point) {
        assert!(self.active.remove(&id).is_some(), "contact ended twice");
        self.ended.push(id);
    }
    fn pointer_world(&mut self, _: Point) -> Option<Point> { None }
    fn ground_world(&mut self, p: Point) -> Option<Point> { Some(Point::new(p.x * 10., p.y * 10.)) }
    fn movement_direction(&mut self, _: Point) -> Option<Point> { None }
    fn hero_world(&mut self) -> Option<Point> { Some(Point::default()) }
    fn pointer_world_at_plane(&mut self, _: Point, _: f64) -> Option<Point> { None }
    fn capture_pointer_world(&mut self, _: Point) -> u64 { 0 }
    fn resolve_world(&mut self, _: u64) -> Option<Point> { None }
    fn release_resolver(&mut self, _: u64) {}
    fn navigation(&mut self, _: Point, p: Point, _: bool) -> Result<Vec<Point>, String> { Ok(vec![p]) }
    fn binding(&mut self, hero: i32, button: &Button, _: &str) -> Binding {
        let key = match (hero, button.kind.as_str(), button.slot.as_str()) {
            (_, "extra", _) => 70,
            (17 | 22 | 27 | 33 | 46 | 59 | 63 | 64 | 75 | 83 | 108, "skill", "skill:1") => 87,
            (22 | 33 | 50 | 64 | 67 | 83, "skill", "skill:2") => 69,
            (27 | 50, "skill", "skill:3") => 82,
            _ => 81,
        };
        let aim_type = if (hero, button.skillId) == (33, 3320) { 1 } else { self.aim_type };
        Binding { key, cast_mode: self.cast_mode, aim_type, stop_movement: false }
    }
    fn cancel_key(&mut self, _: i32) -> i32 { 84 }
    fn timer(&mut self, _: Timer, _: Option<i64>) {}
    fn trace(&mut self, _: &Controller, event: &str, reason: &str) {
        self.traces.push((event.into(), reason.into()));
    }
}
fn n(value: f64) -> snapshot::Number { snapshot::Number { value, kind: 1 } }
fn flag(value: bool) -> snapshot::Flag { snapshot::Flag { value: value as i32 } }
fn text(value: &str) -> snapshot::Text {
    let mut out = snapshot::Text::default();
    out.present = 1;
    out.size = value.len() as u32;
    out.bytes[..value.len()].copy_from_slice(value.as_bytes());
    out
}
fn v2(x: f64, y: f64) -> snapshot::Vector2 {
    snapshot::Vector2 { values: [n(x), n(y)], count: 2 }
}

#[derive(Clone, Copy)]
struct Skill { hero: i32, base: i32, form: i32, slot: &'static str }
const KAGURA: Skill = Skill { hero: 25, base: 2510, form: 2510, slot: "skill:0" };
const LING_WALL_W: Skill = Skill { hero: 84, base: 8420, form: 8425, slot: "skill:1" };
const FREYA_W: Skill = Skill { hero: 22, base: 2298, form: 2298, slot: "skill:1" };

fn frame(now: i64, skill: Skill, forbidden: Option<bool>) -> snapshot::Frame {
    let mut f = snapshot::Frame {
        ready: flag(true), hero_id: n(skill.hero as f64), time_ns: n(now as f64),
        sample_duration_ms: n(0.5), token: text("rejected-range-match"),
        token_nonempty: flag(true), token_string: flag(true), ..Default::default()
    };
    f.ui.ready = flag(true);
    f.ui.hero_id = f.hero_id;
    f.ui.time_ns = f.time_ns;
    f.ui.resolution_px = v2(1000., 1000.);
    f.view_matrix = snapshot::Vector16 {
        values: std::array::from_fn(|i| n(if i == 9 { -1. } else if i % 5 == 0 { 1. } else { 0. })),
        count: 16,
    };
    f.hero_render_xyz = snapshot::Vector3 { values: [n(0.); 3], count: 3 };
    f.hero_screen_uv = v2(0.5, 0.5);
    f.ui.movement = snapshot::MovementGeometry {
        ready: flag(true), press_uv: v2(0.15, 0.85), center_uv: v2(0.15, 0.85), radius_uv: v2(0.1, 0.1),
    };
    f.ui.button_count = 2;
    f.ui.buttons[0] = snapshot::Button {
        slot: text(skill.slot), kind: text("skill"), index: n(0.), skill_id: n(skill.base as f64),
        active: flag(true), press_uv: v2(0.8, 0.8), center_uv: v2(0.8, 0.8), radius_uv: v2(0.1, 0.1),
        ..Default::default()
    };
    f.ui.buttons[1] = snapshot::Button {
        slot: text("extra:2"), kind: text("extra"), index: n(2.), skill_id: n(20150.),
        active: flag(true), press_uv: v2(0.6, 0.8), center_uv: v2(0.6, 0.8), radius_uv: v2(0.1, 0.1),
        ..Default::default()
    };
    f.skill_state = snapshot::VariantState {
        ready: flag(true), source: text("ShowSkillData.m_TranID"), token: f.token,
        hero_id: f.hero_id, sample_start_ns: n(now as f64 - 500_000.), sample_end_ns: f.time_ns,
        count: 1, ..Default::default()
    };
    f.skill_state.buttons[0] = snapshot::Variant {
        slot: text(skill.slot), base_skill_id: n(skill.base as f64), active_skill_id: n(skill.form as f64),
        cast_locked: flag(false), state_forbidden: forbidden.map(flag).unwrap_or_default(),
        charge_skill: snapshot::Flag::default(),
    };
    f
}
fn key(c: &mut Controller, h: &mut TestHost, at: i64, code: i32, down: bool) {
    assert!(c.apply_input(h, Input::Key { code, down, repeat: false }, at));
}
fn setup(skill: Skill, forbidden: Option<bool>, aim_type: u8, cast_mode: i32) -> (Controller, TestHost) {
    let mut c = Controller::new();
    let mut h = TestHost { aim_type, cast_mode, ..Default::default() };
    c.apply_input(&mut h, Input::Trace(true), START);
    c.apply_input(&mut h, Input::Enabled(true), START);
    c.apply_input(&mut h, Input::HoldThreshold(40), START);
    c.apply_snapshot(&mut h, &frame(START, skill, forbidden), START);
    assert!(c.apply_input(&mut h, Input::Mouse { button: 2, down: true, point: Point::new(0.6, 0.4) }, START));
    let now = START + 50_000_000;
    c.apply_snapshot(&mut h, &frame(now, skill, forbidden), now);
    assert!(c.movementHeld_);
    (c, h)
}
fn resource_frame(now: i64, skill: Skill, shortage: bool) -> snapshot::Frame {
    let mut f = frame(now, skill, Some(false));
    f.ui.cooldown_state = snapshot::CooldownState {
        present: flag(true),
        ready: flag(true),
        coherent: flag(true),
        source: text("VirtualButton.m_CdCoolDown"),
        token: f.token,
        hero_id: f.hero_id,
        sample_start_ns: n(now as f64 - 500_000.),
        sample_end_ns: f.time_ns,
        count: 1,
        ..Default::default()
    };
    f.ui.cooldown_state.buttons[0] = snapshot::CooldownRow {
        slot: text(skill.slot),
        base_skill_id: n(skill.base as f64),
        active_skill_id: n(skill.form as f64),
        ready: flag(true),
        on_cooldown: flag(false),
        resource_shortage: flag(shortage),
    };
    f
}

#[test]
fn empty_energy_or_mana_discards_press_until_a_fresh_press_after_recovery() {
    let cable = Skill { hero: 17, base: 1720, form: 1723, slot: "skill:1" };
    for (skill, code, aim) in [(cable, 87, 2), (KAGURA, 81, 3), (FREYA_W, 87, 2)] {
        let (mut c, mut h) = setup(skill, Some(false), aim, 0);
        let movement = c.movementTouch_;
        let now = START + 60_000_000;
        c.apply_snapshot(&mut h, &resource_frame(now, skill, true), now);
        key(&mut c, &mut h, now + 1_000_000, code, true);
        assert!(c.skillTouch_ < 0 && c.queuedSkills_.is_empty());
        assert!(h.traces.iter().any(|(event, reason)| event == "cooldown_ignored" && reason == "insufficient_resource"));
        let now = now + 8_000_000;
        c.apply_snapshot(&mut h, &resource_frame(now, skill, false), now);
        assert!(c.apply_input(&mut h, Input::Key { code, down: true, repeat: true }, now + 1_000_000));
        key(&mut c, &mut h, now + 2_000_000, code, false);
        assert_eq!(h.active.keys().copied().collect::<Vec<_>>(), [movement]);
        key(&mut c, &mut h, now + 3_000_000, code, true);
        assert!(h.active.contains_key(&c.skillTouch_));
        assert!(h.active.contains_key(&movement));
    }
}

#[test]
fn locked_enhancements_block_repeat_without_cancelling_movement() {
    let miya = Skill { hero: 1, base: 110, form: 110, slot: "skill:0" };
    let layla_q = Skill { hero: 18, base: 1810, form: 1810, slot: "skill:0" };
    let freya = Skill { hero: 22, base: 2250, form: 2250, slot: "skill:2" };
    let sun_q = Skill { hero: 27, base: 2710, form: 2710, slot: "skill:0" };
    let sun_w = Skill { hero: 27, base: 2750, form: 2750, slot: "skill:1" };
    let sun_r = Skill { hero: 27, base: 2730, form: 2730, slot: "skill:3" };
    let odette_q = Skill { hero: 46, base: 4610, form: 4610, slot: "skill:0" };
    let odette_w = Skill { hero: 46, base: 4620, form: 4620, slot: "skill:1" };
    let zhask_q = Skill { hero: 50, base: 5010, form: 5010, slot: "skill:0" };
    let zhask_e = Skill { hero: 50, base: 5030, form: 5030, slot: "skill:2" };
    let zhask_r = Skill { hero: 50, base: 5040, form: 5040, slot: "skill:3" };
    let jawhead_q = Skill { hero: 54, base: 5410, form: 5410, slot: "skill:0" };
    let uranus_w = Skill { hero: 59, base: 5920, form: 5920, slot: "skill:1" };
    let change_q = Skill { hero: 61, base: 6110, form: 6110, slot: "skill:0" };
    let aldous_q = Skill { hero: 64, base: 6410, form: 6410, slot: "skill:0" };
    let aldous_w = Skill { hero: 64, base: 6420, form: 6420, slot: "skill:1" };
    let aldous_e = Skill { hero: 64, base: 6430, form: 6430, slot: "skill:2" };
    let leomord_e = Skill { hero: 67, base: 6730, form: 6730, slot: "skill:2" };
    let kadita_q = Skill { hero: 75, base: 7510, form: 7510, slot: "skill:0" };
    let kadita_w = Skill { hero: 75, base: 7520, form: 7520, slot: "skill:1" };
    let xborg_e = Skill { hero: 83, base: 8330, form: 8330, slot: "skill:2" };
    let aulus_w = Skill { hero: 108, base: 10820, form: 10820, slot: "skill:1" };
    let cici_q = Skill { hero: 123, base: 12310, form: 12310, slot: "skill:0" };
    for (skill, code, form) in [
        (miya, 81, 111), (layla_q, 81, 201812), (freya, 69, 2254),
        (sun_q, 81, 2710), (sun_q, 81, 2705), (sun_q, 81, 2708),
        (sun_w, 87, 2706), (sun_w, 87, 2707),
        (sun_r, 82, 2709), (sun_r, 82, 2739),
        (odette_q, 81, 4610), (odette_w, 87, 4620),
        (zhask_q, 81, 5010), (zhask_q, 81, 5013),
        (zhask_e, 69, 5030), (zhask_e, 69, 105040),
        (zhask_r, 82, 5040), (zhask_r, 82, 5076),
        (jawhead_q, 81, 5417),
        (uranus_w, 87, 5921),
        (change_q, 81, 6110), (Skill { form: 6150, ..change_q }, 81, 6150),
        (aldous_q, 81, 6417),
        (aldous_w, 87, 6420), (Skill { form: 6421, ..aldous_w }, 87, 6421),
        (aldous_e, 69, 6430), (aldous_e, 69, 6431),
        (leomord_e, 69, 206738), (leomord_e, 69, 6738),
        (kadita_q, 81, 7510), (kadita_w, 87, 7520),
        (xborg_e, 69, 8330),
        (aulus_w, 87, 10820), (Skill { form: 10824, ..aulus_w }, 87, 10824),
        (cici_q, 81, 12370),
    ] {
        let (mut c, mut h) = setup(skill, Some(false), 1, 0);
        let movement = c.movementTouch_;
        let now = START + 60_000_000;
        let mut f = resource_frame(now, Skill { form, ..skill }, false);
        f.skill_state.buttons[0].cast_locked = flag(true);
        c.apply_snapshot(&mut h, &f, now);
        key(&mut c, &mut h, now + 1_000_000, code, true);
        assert!(c.queuedSkills_.is_empty());
        assert_eq!(h.active.keys().copied().collect::<Vec<_>>(), [movement]);
        assert!(h.traces.iter().any(|(event, reason)| event == "cooldown_ignored" && reason == "native_locked_enhancement"));

        let now = now + 8_000_000;
        c.apply_snapshot(&mut h, &resource_frame(now, skill, false), now);
        key(&mut c, &mut h, now + 1_000_000, code, false);
        assert_eq!(h.active.keys().copied().collect::<Vec<_>>(), [movement]);
        key(&mut c, &mut h, now + 2_000_000, code, true);
        assert!(h.active.len() > 1, "a fresh press works as soon as the enhancement expires");
    }
}

#[test]
fn aldous_repeat_waits_for_a_native_transition_and_needs_a_fresh_down() {
    let stance = Skill { hero: 64, base: 6420, form: 6420, slot: "skill:1" };
    let recast = Skill { form: 6421, ..stance };
    for changed_form in [true, false] {
        let (mut c, mut h) = setup(stance, Some(false), 1, 0);
        let movement = c.movementTouch_;
        let at = |ms: i64| START + ms * 1_000_000;
        let observe = |c: &mut Controller, h: &mut TestHost, ms, tick, skill, locked| {
            let mut f = resource_frame(at(ms), skill, false);
            f.game_clock_ms = n(tick);
            f.skill_state.buttons[0].cast_locked = flag(locked);
            c.apply_snapshot(h, &f, at(ms));
        };

        observe(&mut c, &mut h, 60, 300., stance, false);
        key(&mut c, &mut h, at(61), 87, true);
        key(&mut c, &mut h, at(80), 87, false);
        observe(&mut c, &mut h, 85, 300., stance, false);
        key(&mut c, &mut h, at(86), 87, true);
        assert!(h.traces.iter().any(|(event, reason)|
            event == "cooldown_ignored" && reason == "await_native_phase_after_release"));
        let button = c.find_button(stance.slot).unwrap().clone();
        for ms in [90, 95, 98] {
            observe(&mut c, &mut h, ms, 300., stance, false);
            assert_eq!(c.cooldown_block(&button, 0), "await_native_phase_after_release");
        }
        if changed_form {
            observe(&mut c, &mut h, 130, 366., recast, true);
            assert_eq!(c.cooldown_block(&button, 0), "native_locked_enhancement");
            observe(&mut c, &mut h, 145, 432., recast, false);
        } else {
            observe(&mut c, &mut h, 130, 366., stance, false);
            assert_eq!(c.cooldown_block(&button, 0), "await_native_phase_after_release");
            observe(&mut c, &mut h, 145, 432., stance, false);
        }
        assert!(c.queuedSkills_.is_empty());
        assert_eq!(h.active.keys().copied().collect::<Vec<_>>(), [movement]);
        key(&mut c, &mut h, at(146), 87, false);
        assert_eq!(h.active.keys().copied().collect::<Vec<_>>(), [movement]);
        key(&mut c, &mut h, at(147), 87, true);
        assert!(h.active.len() > 1);
    }
}

#[test]
fn selena_early_repeat_starts_only_a_stationary_follow_while_held() {
    let arrow = Skill { hero: 63, base: 6320, form: 6320, slot: "skill:1" };
    let follow = Skill { form: 206320, ..arrow };
    for end in ["release", "native_expiry", "cancel", "early_release", "no_projectile"] {
        let (mut c, mut h) = setup(arrow, Some(false), 2, 1);
        let movement = c.movementTouch_;
        let at = |ms: i64| START + ms * 1_000_000;
        c.apply_snapshot(&mut h, &resource_frame(at(60), arrow, false), at(60));
        c.mark_cooldown_cast(arrow.slot, arrow.form);
        key(&mut c, &mut h, at(80), 87, true);
        assert_eq!(h.active.keys().copied().collect::<Vec<_>>(), [movement]);
        assert!(c.queuedSkills_[0].arrowFollow);

        if end == "early_release" {
            key(&mut c, &mut h, at(140), 87, false);
        }
        let next = if end == "no_projectile" { arrow } else { follow };
        let now = if end == "no_projectile" { at(430) } else { at(265) };
        c.apply_snapshot(&mut h, &resource_frame(now, next, false), now);
        assert!(c.queuedSkills_.is_empty());
        if matches!(end, "early_release" | "no_projectile") {
            assert_eq!(h.active.keys().copied().collect::<Vec<_>>(), [movement]);
            continue;
        }
        let contact = c.skillTouch_;
        assert_eq!(c.skillBehavior_, SkillBehavior::Tap);
        assert_eq!(c.skillCastMode_, 0);
        assert_eq!(h.active[&contact], c.find_button(arrow.slot).unwrap().press);
        c.update_pointer(&mut h, Point::new(0.2, 0.2));
        c.flush_pointer(&mut h);
        assert_eq!(h.active[&contact], c.find_button(arrow.slot).unwrap().press);
        let now = now + 40_000_000;
        match end {
            "release" => key(&mut c, &mut h, now, 87, false),
            "cancel" => key(&mut c, &mut h, now, 84, true),
            _ => c.apply_snapshot(&mut h, &resource_frame(now, arrow, false), now),
        }
        assert_eq!(h.active.keys().copied().collect::<Vec<_>>(), [movement]);
        assert!(!c.cooldownCasts_.contains_key("skill:1:206320"));
        assert!(!c.skillCancelPending_);
    }
}

#[test]
fn cancelling_unclaimed_cyclops_ultimate_retires_only_its_finger() {
    let skill = Skill { hero: 33, base: 3330, form: 3330, slot: "skill:2" };
    let cross = Point::new(0.68, 0.39);
    for case in ["unclaimed", "claimed", "foreign", "before_cancel_move", "too_soon"] {
        let (mut c, mut h) = setup(skill, Some(false), 2, 0);
        let movement = c.movementTouch_;
        let at = |ms: i64| START + ms * 1_000_000;
        key(&mut c, &mut h, at(60), 69, true);
        let finger = c.skillTouch_;
        let mut preview = frame(at(75), skill, Some(false));
        preview.ui.cancel_uv = v2(cross.x, cross.y);
        if case == "claimed" {
            preview.ui.active_slot = text("skill:2");
            preview.skill_id = n(3330.);
        }
        c.apply_snapshot(&mut h, &preview, at(75));

        key(&mut c, &mut h, at(80), 84, true);
        key(&mut c, &mut h, at(85), 84, false);
        key(&mut c, &mut h, at(90), 69, false);
        assert_eq!(h.active[&finger], cross, "{case}");

        let now = at(if case == "too_soon" { 160 } else { 190 });
        let mut idle = frame(now, skill, Some(false));
        idle.ui.cancel_uv = v2(cross.x, cross.y);
        idle.cancel_state = snapshot::CancelState {
            ready: flag(true), coherent: flag(true), source: text("UIKeySkill.bCancelSkill"),
            token: if case == "foreign" { text("foreign") } else { idle.token },
            hero_id: idle.hero_id, base_skill_id: n(3300.), cancelled: flag(false),
            sample_start_ns: n(if case == "before_cancel_move" { at(78) } else { now - 500_000 } as f64),
            sample_end_ns: idle.time_ns,
            ..Default::default()
        };
        c.apply_snapshot(&mut h, &idle, now);
        assert_eq!(h.active.contains_key(&finger), case != "unclaimed", "{case}");
        assert!(h.active.contains_key(&movement));
        assert!(c.cooldownCasts_.is_empty());
        if case == "unclaimed" {
            assert_eq!(h.ended, [finger]);
            assert!(!c.skillCancelPending_);
            key(&mut c, &mut h, at(191), 69, true);
            assert!(h.active.contains_key(&c.skillTouch_));
        }
    }
}

#[test]
fn cyclops_w_leaves_queue_after_automatically_cancelling_an_unclaimed_q() {
    let q = Skill { hero: 33, base: 3310, form: 3310, slot: "skill:0" };
    let w = Skill { hero: 33, base: 3320, form: 3320, slot: "skill:1" };
    let at = |ms: i64| START + ms * 1_000_000;
    let controls = |now: i64| {
        let mut f = frame(now, q, Some(false));
        f.ui.buttons[2] = frame(now, w, Some(false)).ui.buttons[0];
        f.ui.button_count = 3;
        f.ui.cancel_uv = v2(0.835, 0.375);
        f.cancel_state = snapshot::CancelState {
            ready: flag(true), coherent: flag(true), source: text("UIKeySkill.bCancelSkill"),
            token: f.token, hero_id: f.hero_id, base_skill_id: n(3310.), cancelled: flag(false),
            sample_start_ns: n(now as f64 - 500_000.), sample_end_ns: f.time_ns,
            ..Default::default()
        };
        f
    };
    let (mut c, mut h) = setup(q, Some(false), 2, 0);
    let movement = c.movementTouch_;
    c.apply_snapshot(&mut h, &controls(at(56)), at(56));
    key(&mut c, &mut h, at(60), 81, true);
    let rejected_q = c.skillTouch_;
    key(&mut c, &mut h, at(107), 87, true);
    assert!(c.skillCancelPending_);
    assert_eq!(c.queuedSkills_[0].slot, "skill:1");
    assert_eq!(h.active[&rejected_q], Point::new(0.835, 0.375));
    key(&mut c, &mut h, at(155), 81, false);
    key(&mut c, &mut h, at(216), 87, false);
    c.apply_snapshot(&mut h, &controls(at(217)), at(217));
    assert_eq!(h.ended, [rejected_q]);
    assert!(c.cooldownCasts_.is_empty());
    assert_eq!(c.queuedSkills_.len(), 1);
    c.apply_snapshot(&mut h, &controls(at(220)), at(220));
    let w_touch = c.skillTouch_;
    assert_eq!(c.activeSlot_, "skill:1");
    assert!(c.queuedSkills_.is_empty());
    c.apply_snapshot(&mut h, &controls(at(239)), at(239));
    assert_eq!(h.ended, [rejected_q, w_touch]);
    assert_eq!(h.active.keys().copied().collect::<Vec<_>>(), [movement]);
    assert!(c.cooldownCasts_.contains_key("skill:1:3320"));
}

#[test]
fn resource_spent_by_previous_cast_discards_queued_press_before_touch_down() {
    let skill = FREYA_W;
    let (mut c, mut h) = setup(skill, Some(false), 2, 0);
    let movement = c.movementTouch_;
    let now = START + 60_000_000;
    c.apply_snapshot(&mut h, &resource_frame(now, skill, false), now);
    // Represent an outstanding release frame: new W is admissible with the
    // last resource sample, but dispatch needs fresh post-cast evidence.
    c.lastSkillEndNs_ = now + 1_000_000;
    let button = c.find_button("skill:1").unwrap().clone();
    c.queue_skill(&mut h, &button);
    c.dispatch_queued_skill(&mut h);
    assert_eq!(c.queuedSkills_.len(), 1);
    let now = now + 8_000_000;
    c.apply_snapshot(&mut h, &resource_frame(now, skill, true), now);
    assert!(c.queuedSkills_.is_empty());
    assert_eq!(h.active.keys().copied().collect::<Vec<_>>(), [movement]);
    assert!(h.traces.iter().any(|(event, reason)| event == "cooldown_ignored" && reason == "insufficient_resource"));
}

fn rejected(h: &TestHost) -> usize {
    h.traces.iter().filter(|(event, _)| event == "range_rejected").count()
}

#[test]
fn forbidden_unclaimed_range_releases_on_physical_up_for_kagura_and_ling_wall_form() {
    for skill in [KAGURA, LING_WALL_W] {
        let (mut c, mut h) = setup(skill, Some(true), 3, 0);
        let movement = c.movementTouch_;
        key(&mut c, &mut h, START + 60_000_000, 81, true);
        let finger = c.skillTouch_;
        assert!(h.active.contains_key(&finger));
        let now = START + 70_000_000;
        c.apply_snapshot(&mut h, &frame(now, skill, Some(true)), now);
        key(&mut c, &mut h, START + 80_000_000, 81, false);
        assert_eq!(h.ended, vec![finger]);
        assert_eq!(c.skillTouch_, -1);
        assert!(!c.pendingRelease_);
        assert!(c.movementHeld_ && h.active.contains_key(&movement));
        assert_eq!(rejected(&h), 1);
        assert!(c.cooldownCasts_.is_empty(), "rejection is not a cast/cooldown reservation");
        assert!(c.kaguraThrow_.is_none(), "rejection must not create a Q-to-W throw dependency");
        let now = START + 90_000_000;
        c.apply_snapshot(&mut h, &frame(now, skill, Some(false)), now);
        key(&mut c, &mut h, now, 81, true);
        assert!(c.skillTouch_ >= 0 && c.skillTouch_ != finger);
        assert!(c.queuedSkills_.is_empty());
        assert!(!c.skillActivationObserved_ && !c.skillStateForbiddenAtDown_);
    }
}

#[test]
fn early_fanny_release_waits_at_w_even_during_movement_pointer_flushes() {
    let cable = Skill { hero: 17, base: 1720, form: 1723, slot: "skill:1" };
    let (mut c, mut h) = setup(cable, Some(false), 2, 0);
    let movement = c.movementTouch_;
    key(&mut c, &mut h, START + 60_000_000, 87, true);
    let finger = c.skillTouch_;
    let press = h.active[&finger];
    key(&mut c, &mut h, START + 61_000_000, 87, false);
    // This callback used to move an unopened W onto Q before the game
    // recognized either button. Keep the independent movement contact alive.
    for ms in [2, 6, 11, 20] {
        let at = START + (60 + ms) * 1_000_000;
        c.apply_input(&mut h, Input::Pointer(Point::new(0.1, 0.9)), at);
        c.apply_input(&mut h, Input::Timer(Timer::Pointer), at);
        c.apply_snapshot(&mut h, &frame(at, cable, Some(false)), at);
        assert_eq!(h.active.get(&finger), Some(&press));
        assert!(c.pendingRelease_ && h.active.contains_key(&movement));
    }
    let now = START + 165_000_000;
    c.apply_snapshot(&mut h, &frame(now, cable, Some(false)), now);
    assert_eq!(h.ended, [finger]);
    assert!(c.movementHeld_ && h.active.contains_key(&movement));
    assert!(c.cooldownCasts_.is_empty());
    assert!(h.traces.iter().any(|(_, reason)| reason == "cable_capture_unavailable_end_at_last_point"));
}

#[test]
fn fanny_cable_commits_only_the_angle_chosen_at_physical_release() {
    let cable = Skill { hero: 17, base: 1720, form: 1723, slot: "skill:1" };
    let observed = |at: i64, raw: Point, direction: Point| {
        let mut f = frame(at, cable, Some(false));
        f.skill_id = n(1723.);
        f.ui.active_slot = text(cable.slot);
        f.ui.cancel_uv = v2(0.83, 0.42);
        f.active_joystick = snapshot::Geometry {
            ready: flag(true), source: text("active_widget_transform"), slot: text(cable.slot),
            skill_id: f.skill_id, center_uv: v2(0.8, 0.8), radius_uv: v2(0.1, 0.1),
            sample_start_ns: f.skill_state.sample_start_ns, sample_end_ns: f.time_ns,
        };
        f.direction_input = snapshot::Direction {
            ready: flag(true), coherent: flag(true), token: f.token, hero_id: f.hero_id,
            source: text("IndicatorMotionRot.m_vDragResult"), slot: f.ui.active_slot, skill_id: f.skill_id,
            sample_start_ns: f.skill_state.sample_start_ns, sample_end_ns: f.time_ns,
            raw_touch_xyz: snapshot::Vector3 { count: 3, values: [n(raw.x), n(raw.y), n(0.)] },
            direction_xz: v2(direction.x, direction.y), ..Default::default()
        };
        f
    };
    for acknowledge in [true, false] {
        let (mut c, mut h) = setup(cable, Some(false), 2, 0);
        let movement = c.movementTouch_;
        let start = START + 60_000_000;
        let old = Point::new(1., 0.);
        c.apply_input(&mut h, Input::Pointer(old), start);
        key(&mut c, &mut h, start, 87, true);
        let finger = c.skillTouch_;
        let at = start + 8_000_000;
        c.apply_snapshot(&mut h, &observed(at, Point::default(), old), at);
        assert!(c.skillActivationObserved_ && c.directionGeometryFresh_ && !c.directionAccepted_);
        let target = Point::new(0., 1.);
        let up = at + 1_500_000;
        c.apply_input(&mut h, Input::Pointer(target), up);
        key(&mut c, &mut h, up, 87, false);
        assert!(h.ended.is_empty(), "W opened the HUD but the game still holds the old angle");
        let sent = h.active[&finger];
        c.apply_input(&mut h, Input::Pointer(old), up + 1_000_000);
        c.apply_input(&mut h, Input::Timer(Timer::Pointer), up + 2_000_000);
        assert_eq!(h.active[&finger], sent);
        assert_eq!(c.pendingDirection_, target);
        let cell = c.pixels(sent);
        let raw = Point::new(cell.x.floor() * 0.999, 1000. - cell.y.floor() * 0.999);
        let at = up + 7_000_000;
        c.apply_snapshot(&mut h, &observed(at, raw, old), at);
        assert!(h.ended.is_empty(), "coordinates alone cannot acknowledge a stale angle");
        if acknowledge {
            let at = up + 14_000_000;
            c.apply_snapshot(&mut h, &observed(at, raw, target), at);
            assert_eq!(h.ended, [finger]);
            assert!(h.traces.iter().any(|(_, reason)| reason == "native_direction_ack_release"));
        } else {
            let at = up + 105_000_000;
            c.apply_snapshot(&mut h, &observed(at, raw, old), at);
            assert!(c.skillCancelPending_ && h.ended.is_empty());
            assert_eq!(h.active[&finger], Point::new(0.83, 0.42));
            assert!(!h.traces.iter().any(|(_, reason)| reason == "cast"));
        }
        assert!(c.movementHeld_ && h.active.contains_key(&movement));
    }
}

#[test]
fn expiring_fanny_cable_cancel_releases_only_w_and_accepts_the_next_physical_press() {
    const CHAIN: Skill = Skill { hero: 17, base: 1720, form: 1723, slot: "skill:1" };
    const NORMAL: Skill = Skill { form: 1720, ..CHAIN };
    let cancel_point = Point::new(0.83, 0.42);
    let idle = |at, form| {
        let mut f = frame(at, form, Some(false));
        f.skill_id = n(0.);
        f.ui.cancel_uv = v2(cancel_point.x, cancel_point.y);
        f.cancel_state = snapshot::CancelState {
            ready: flag(true), coherent: flag(true), token: f.token, hero_id: f.hero_id,
            source: text("UIKeySkill.bCancelSkill"), base_skill_id: n(1720.), cancelled: flag(false),
            sample_start_ns: f.skill_state.sample_start_ns, sample_end_ns: f.time_ns,
            ..Default::default()
        };
        f
    };
    for early_press in [false, true] {
        let (mut c, mut h) = setup(CHAIN, Some(false), 2, 0);
        let movement = c.movementTouch_;
        let start = START + 60_000_000;
        key(&mut c, &mut h, start, 87, true);
        let finger = c.skillTouch_;
        let mut owned = idle(start + 8_000_000, CHAIN);
        owned.skill_id = n(1723.);
        owned.ui.active_slot = text(CHAIN.slot);
        c.apply_snapshot(&mut h, &owned, start + 8_000_000);
        assert!(c.skillActivationObserved_);
        c.apply_snapshot(&mut h, &idle(start + 90_000_000, CHAIN), start + 90_000_000);
        let cancel = start + 137_000_000;
        key(&mut c, &mut h, cancel, 84, true);
        key(&mut c, &mut h, cancel + 1_000_000, 84, false);
        assert_eq!(h.active[&finger], cancel_point);
        key(&mut c, &mut h, cancel + 2_000_000, 87, false);
        if early_press {
            key(&mut c, &mut h, cancel + 3_000_000, 87, true);
        }
        for (i, case) in ["chain", "old_cancel", "old_variant", "wrong_epoch", "other_slot", "missing_skill", "other_aim"].iter().enumerate() {
            let at = cancel + (5 + i as i64) * 1_000_000;
            let mut f = idle(at, NORMAL);
            match *case {
                "chain" => f.skill_state.buttons[0].active_skill_id = n(1723.),
                "old_cancel" => f.cancel_state.sample_start_ns = n((cancel - 1) as f64),
                "old_variant" => f.skill_state.sample_start_ns = n((cancel - 1) as f64),
                "wrong_epoch" => f.cancel_state.token = text("different-match"),
                "other_slot" => f.cancel_state.slot = text("skill:0"),
                "missing_skill" => f.skill_id = Default::default(),
                "other_aim" => f.ui.active_slot = text("skill:0"),
                _ => unreachable!(),
            }
            c.apply_snapshot(&mut h, &f, at);
            assert!(c.skillCancelPending_ && h.active.contains_key(&finger), "{case}");
        }
        let at = cancel + 22_000_000;
        c.apply_snapshot(&mut h, &idle(at, NORMAL), at);
        assert_eq!(h.ended, [finger]);
        assert!(!c.skillCancelPending_ && c.cooldownCasts_.is_empty());
        assert!(h.active.contains_key(&movement) && c.movementHeld_);
        c.apply_snapshot(&mut h, &idle(at + 5_000_000, NORMAL), at + 5_000_000);
        if !early_press {
            assert_eq!(h.active.len(), 1);
            key(&mut c, &mut h, at + 6_000_000, 87, true);
        }
        assert_eq!(c.activeSlot_, "skill:1");
        assert!(c.skillKeyHeld_ && c.skillTouch_ != finger && h.active.len() == 2);
    }
}

#[test]
fn recorded_q_timings_do_not_lock_out_the_later_unblocked_press() {
    let (mut c, mut h) = setup(KAGURA, Some(true), 3, 0);
    let down = START + 60_000_000;
    key(&mut c, &mut h, down, 81, true);
    let first = c.skillTouch_;
    for relative in [2_037_000, 114_000_000] {
        let now = down + relative;
        c.apply_snapshot(&mut h, &frame(now, KAGURA, Some(true)), now);
    }
    key(&mut c, &mut h, down + 115_651_000, 81, false);
    assert!(!h.active.contains_key(&first));
    assert_eq!(c.lastSkillEndNs_, down + 115_651_000);
    let now = down + 615_000_000;
    c.apply_snapshot(&mut h, &frame(now, KAGURA, Some(true)), now);
    key(&mut c, &mut h, down + 616_846_000, 81, true);
    let second = c.skillTouch_;
    assert_ne!(first, second);
    let now = down + 704_000_000;
    c.apply_snapshot(&mut h, &frame(now, KAGURA, Some(true)), now);
    key(&mut c, &mut h, down + 705_696_000, 81, false);
    assert!(!h.active.contains_key(&second));
    for relative in [782_066_000, 895_000_000] {
        let now = down + relative;
        c.apply_snapshot(&mut h, &frame(now, KAGURA, Some(false)), now);
    }
    key(&mut c, &mut h, down + 896_677_000, 81, true);
    assert!(c.skillTouch_ >= 0 && c.skillTouch_ != second);
    assert!(c.queuedSkills_.is_empty());
    assert_eq!(rejected(&h), 2);
}

#[test]
fn post_down_receipt_arriving_after_up_recovers_on_that_frame_not_a_timeout() {
    let (mut c, mut h) = setup(KAGURA, Some(true), 3, 0);
    key(&mut c, &mut h, START + 60_000_000, 81, true);
    let finger = c.skillTouch_;
    key(&mut c, &mut h, START + 61_000_000, 81, false);
    assert!(c.pendingRelease_ && h.active.contains_key(&finger));
    let now = START + 62_000_000;
    c.apply_snapshot(&mut h, &frame(now, KAGURA, Some(true)), now);
    assert_eq!(h.ended, vec![finger]);
    assert!(!c.pendingRelease_ && c.skillTouch_ < 0);
    assert_eq!(rejected(&h), 1);
}

#[test]
fn any_owned_ui_receipt_protects_buffered_range_even_without_a_logical_target() {
    for skill in [KAGURA, LING_WALL_W] {
        let (mut c, mut h) = setup(skill, Some(true), 3, 0);
        key(&mut c, &mut h, START + 60_000_000, 81, true);
        let finger = c.skillTouch_;
        let now = START + 70_000_000;
        let mut owned = frame(now, skill, Some(true));
        owned.ui.active_slot = text(skill.slot);
        c.apply_snapshot(&mut h, &owned, now);
        assert!(c.skillActivationObserved_);
        assert!(!c.aimReady_, "test deliberately has no logical aiming result");
        let now = START + 75_000_000;
        c.apply_snapshot(&mut h, &frame(now, skill, Some(true)), now);
        key(&mut c, &mut h, START + 80_000_000, 81, false);
        assert!(c.pendingRelease_ && h.active.contains_key(&finger));
        assert_eq!(rejected(&h), 0);
        assert!(h.ended.is_empty());
    }
}

#[test]
fn matching_native_skill_receipt_protects_range_when_ui_capture_sample_is_missed() {
    for (skill, observed) in [(KAGURA, 2510), (LING_WALL_W, 8420), (LING_WALL_W, 8425)] {
        let (mut c, mut h) = setup(skill, Some(true), 3, 0);
        key(&mut c, &mut h, START + 60_000_000, 81, true);
        let finger = c.skillTouch_;
        let now = START + 70_000_000;
        let mut native = frame(now, skill, Some(true));
        native.skill_id = n(observed as f64);
        assert!(native.ui.active_slot.as_str().is_empty());
        c.apply_snapshot(&mut h, &native, now);
        assert!(c.skillActivationObserved_, "native skill {observed} proves acceptance without a UI capture sample");
        let now = START + 75_000_000;
        c.apply_snapshot(&mut h, &frame(now, skill, Some(true)), now);
        assert_eq!(c.coreSkill_, 0);
        key(&mut c, &mut h, START + 80_000_000, 81, false);
        assert!(c.pendingRelease_ && h.active.contains_key(&finger));
        assert_eq!(rejected(&h), 0, "acceptance remains sticky after native/UI feedback disappears");
        assert!(h.ended.is_empty());
    }
}

#[test]
fn recovery_requires_forbid_at_down_and_fresh_same_owner_same_form_confirmation() {
    for case in ["initial_false", "initial_unknown", "missing", "stale", "wrong_hero", "wrong_epoch", "wrong_slot", "form_changed", "unblocked", "other_ui_owned"] {
        let initial = match case { "initial_false" => Some(false), "initial_unknown" => None, _ => Some(true) };
        let (mut c, mut h) = setup(KAGURA, initial, 3, 0);
        key(&mut c, &mut h, START + 60_000_000, 81, true);
        let finger = c.skillTouch_;
        let now = START + 70_000_000;
        let mut latest = frame(now, KAGURA, Some(true));
        match case {
            "missing" => latest.skill_state.buttons[0].state_forbidden = snapshot::Flag::default(),
            "stale" => {
                latest.skill_state.sample_start_ns = n((now - 101_500_000) as f64);
                latest.skill_state.sample_end_ns = n((now - 101_000_000) as f64);
            },
            "wrong_hero" => latest.skill_state.hero_id = n(84.),
            "wrong_epoch" => latest.skill_state.token = text("other-match"),
            "wrong_slot" => latest.skill_state.buttons[0].slot = text("skill:2"),
            "form_changed" => latest.skill_state.buttons[0].active_skill_id = n(2511.),
            "unblocked" => latest.skill_state.buttons[0].state_forbidden = flag(false),
            "other_ui_owned" => latest.ui.active_slot = text("extra:2"),
            _ => {},
        }
        c.apply_snapshot(&mut h, &latest, now);
        key(&mut c, &mut h, START + 80_000_000, 81, false);
        assert!(h.active.contains_key(&finger), "{case} must not prove rejection");
        assert!(c.pendingRelease_, "{case} keeps the original release path");
        assert_eq!(rejected(&h), 0, "{case}");
    }
}

#[test]
fn direction_tap_and_instant_range_do_not_enter_rejected_range_recovery() {
    for (aim_type, mode) in [(2, 0), (1, 0), (3, 1)] {
        let (mut c, mut h) = setup(LING_WALL_W, Some(true), aim_type, mode);
        key(&mut c, &mut h, START + 60_000_000, 81, true);
        let now = START + 70_000_000;
        c.apply_snapshot(&mut h, &frame(now, LING_WALL_W, Some(true)), now);
        key(&mut c, &mut h, START + 80_000_000, 81, false);
        assert_eq!(rejected(&h), 0, "aim_type={aim_type},mode={mode}");
        if aim_type == 2 || aim_type == 1 {
            assert!(c.skillTouch_ < 0, "existing direction/tap UP must still complete");
        } else {
            assert!(c.skillTouch_ >= 0, "instant range still awaits its normal aiming evidence");
        }
    }
}

#[test]
fn explicit_user_cancel_keeps_its_existing_acknowledgement_path() {
    let (mut c, mut h) = setup(KAGURA, Some(true), 3, 0);
    key(&mut c, &mut h, START + 60_000_000, 81, true);
    let finger = c.skillTouch_;
    let now = START + 70_000_000;
    c.apply_snapshot(&mut h, &frame(now, KAGURA, Some(true)), now);
    key(&mut c, &mut h, START + 72_000_000, 84, true);
    key(&mut c, &mut h, START + 80_000_000, 81, false);
    assert!(c.skillCancelPending_ && h.active.contains_key(&finger));
    assert_eq!(rejected(&h), 0);
}

#[test]
fn rejected_predecessor_keeps_existing_dependent_queue_cancellation_policy() {
    let (mut c, mut h) = setup(KAGURA, Some(true), 3, 0);
    key(&mut c, &mut h, START + 60_000_000, 81, true);
    key(&mut c, &mut h, START + 61_000_000, 81, false);
    key(&mut c, &mut h, START + 61_100_000, 70, true);
    assert_eq!(c.queuedSkills_.len(), 1);
    let now = START + 62_000_000;
    c.apply_snapshot(&mut h, &frame(now, KAGURA, Some(true)), now);
    assert!(c.queuedSkills_.is_empty());
    assert!(h.traces.iter().any(|(event, reason)| event == "skill_queue" && reason == "predecessor_aborted"));
    assert_eq!(rejected(&h), 1);
    assert_eq!(h.active.len(), 1, "only movement remains");
}

const UNLISTED: Skill = Skill { hero: 936, base: 93610, form: 93617, slot: "skill:0" };

fn native_lock_frame(ms: i64, tick: u32, cast_locked: bool, state_forbidden: bool) -> snapshot::Frame {
    let mut f = frame(START + ms * 1_000_000, UNLISTED, Some(state_forbidden));
    f.skill_state.buttons[0].cast_locked = flag(cast_locked);
    f.ui.cancel_uv = v2(0.9, 0.3);
    f.ui.active_slot = text("");
    f.skill_id = n(0.);
    f.game_clock_ms = n(f64::from(tick));
    f
}

fn native_sample(c: &mut Controller, h: &mut TestHost, f: snapshot::Frame) {
    c.apply_snapshot(h, &f, f.time_ns.value as i64);
}

#[test]
fn unlisted_locked_range_retires_only_its_finger_and_dispatches_the_successor() {
    let (mut c, mut h) = setup(UNLISTED, Some(false), 3, 0);
    let movement = c.movementTouch_;
    native_sample(&mut c, &mut h, native_lock_frame(60, 100, true, false));
    key(&mut c, &mut h, START + 61_000_000, 81, true);
    let finger = c.skillTouch_;
    key(&mut c, &mut h, START + 62_000_000, 81, false);
    key(&mut c, &mut h, START + 63_000_000, 70, true);
    key(&mut c, &mut h, START + 64_000_000, 70, false);
    assert_eq!(c.queuedSkills_.len(), 1);
    assert!(!c.queuedSkills_[0].held);

    for ms in [65, 75, 85] {
        native_sample(&mut c, &mut h, native_lock_frame(ms, 120, true, false));
        assert!(h.active.contains_key(&finger), "sampling one tick repeatedly is not proof");
    }
    native_sample(&mut c, &mut h, native_lock_frame(95, 140, true, false));
    assert_eq!(h.ended, [finger]);
    assert!(h.active.contains_key(&movement));
    assert!(c.cooldownCasts_.is_empty());
    assert_eq!(c.queuedSkills_.len(), 1);
    assert!(h.traces.iter().any(|(event, reason)| event == "release" && reason == "native_cast_lock_without_capture"));
    native_sample(&mut c, &mut h, native_lock_frame(105, 150, true, false));
    assert!(h.traces.iter().any(|(event, reason)| event == "skill_queue" && reason == "dispatched_with_fresh_form"));
    assert!(c.cooldownCasts_.contains_key("extra:2:20150"), "the successor's saved UP commits its cast");
    assert!(c.queuedSkills_.is_empty());
}

#[test]
fn native_lock_cancel_does_not_wait_for_an_ack_from_an_unclaimed_direction() {
    let (mut c, mut h) = setup(UNLISTED, Some(false), 2, 0);
    native_sample(&mut c, &mut h, native_lock_frame(60, 100, false, true));
    key(&mut c, &mut h, START + 61_000_000, 81, true);
    let finger = c.skillTouch_;
    native_sample(&mut c, &mut h, native_lock_frame(70, 120, false, true));
    key(&mut c, &mut h, START + 71_000_000, 84, true);
    assert!(c.skillCancelPending_ && h.active.contains_key(&finger));
    native_sample(&mut c, &mut h, native_lock_frame(90, 140, false, true));
    assert_eq!(h.ended, [finger]);
    assert!(!c.skillCancelPending_);
    assert!(c.cooldownCasts_.is_empty());
    assert!(h.traces.iter().any(|(event, reason)| event == "release" && reason == "native_state_forbid_without_capture"));
    native_sample(&mut c, &mut h, native_lock_frame(110, 150, false, false));
    assert!(c.apply_input(&mut h, Input::Key { code: 81, down: true, repeat: true }, START + 111_000_000));
    key(&mut c, &mut h, START + 112_000_000, 81, false);
    assert_eq!(c.skillTouch_, -1);
    key(&mut c, &mut h, START + 113_000_000, 81, true);
    assert!(h.active.contains_key(&c.skillTouch_));
}

#[test]
fn confirmed_lock_retires_instant_aim_but_does_not_delay_a_normal_quick_tap() {
    for (aim, release_ms) in [(1, 77), (3, 90)] {
        let (mut c, mut h) = setup(UNLISTED, Some(false), aim, 1);
        native_sample(&mut c, &mut h, native_lock_frame(60, 100, true, false));
        key(&mut c, &mut h, START + 61_000_000, 81, true);
        let finger = c.skillTouch_;
        native_sample(&mut c, &mut h, native_lock_frame(70, 120, true, false));
        native_sample(&mut c, &mut h, native_lock_frame(release_ms, 140, true, false));
        assert_eq!(h.ended, [finger]);
        let reason = if aim == 1 { "cast" } else { "native_cast_lock_without_capture" };
        assert!(h.traces.iter().any(|(event, seen)| event == "release" && seen == reason));
    }
}

#[test]
fn locks_cannot_retire_a_buffered_or_unverifiable_action() {
    for case in ["hud_capture", "native_indicator", "uncertain_hud", "no_indicator_read", "wrong_form", "unlocked", "stale", "foreign_epoch", "forbid_only"] {
        let (mut c, mut h) = setup(UNLISTED, Some(false), 3, 0);
        native_sample(&mut c, &mut h, native_lock_frame(60, 100, case != "forbid_only", case == "forbid_only"));
        key(&mut c, &mut h, START + 61_000_000, 81, true);
        let finger = c.skillTouch_;
        let mut f = native_lock_frame(70, 120, true, false);
        match case {
            "hud_capture" => f.ui.active_slot = text(UNLISTED.slot),
            "native_indicator" => f.skill_id = n(UNLISTED.form as f64),
            "uncertain_hud" => f.ui.active_slot = snapshot::Text::default(),
            "no_indicator_read" => f.skill_id = snapshot::Number::default(),
            "wrong_form" => f.skill_state.buttons[0].active_skill_id = n(93618.),
            "unlocked" => f.skill_state.buttons[0].cast_locked = flag(false),
            "stale" => f.skill_state.sample_start_ns = n(START as f64 - 600_000_000.),
            "foreign_epoch" => f.skill_state.token = text("different-match"),
            "forbid_only" => {
                f.skill_state.buttons[0].cast_locked = flag(false);
                f.skill_state.buttons[0].state_forbidden = flag(true);
            }
            _ => unreachable!(),
        }
        native_sample(&mut c, &mut h, f);
        // An exclusively StateForbid-at-DOWN range follows the legacy policy
        // only if the release sample still has StateForbid.
        native_sample(&mut c, &mut h, native_lock_frame(90, 140, true, false));
        native_sample(&mut c, &mut h, native_lock_frame(110, 160, true, false));
        key(&mut c, &mut h, START + 111_000_000, 81, false);
        assert!(h.active.contains_key(&finger), "{case}");
        assert!(c.pendingRelease_, "{case}");
        assert!(h.ended.is_empty(), "{case}");
    }
}

#[test]
fn cancel_after_native_unlock_retires_only_the_original_unclaimed_contact() {
    for cancel_key in [84, 70] {
        let (mut c, mut h) = setup(UNLISTED, Some(false), 3, 0);
        let movement = c.movementTouch_;
        native_sample(&mut c, &mut h, native_lock_frame(60, 100, true, false));
        key(&mut c, &mut h, START + 61_000_000, 81, true);
        let finger = c.skillTouch_;
        native_sample(&mut c, &mut h, native_lock_frame(70, 120, true, false));
        native_sample(&mut c, &mut h, native_lock_frame(90, 140, true, false));
        native_sample(&mut c, &mut h, native_lock_frame(120, 160, false, false));
        assert!(h.active.contains_key(&finger), "hold can still be accepted");
        key(&mut c, &mut h, START + 121_000_000, cancel_key, true);
        assert!(c.skillCancelPending_ && h.active.contains_key(&finger));
        native_sample(&mut c, &mut h, native_lock_frame(130, 180, false, false));
        assert_eq!(h.ended, [finger]);
        assert!(!c.skillCancelPending_);
        assert!(h.active.contains_key(&movement));
        assert!(c.cooldownCasts_.is_empty());
        assert_eq!(c.queuedSkills_.len(), usize::from(cancel_key == 70));
        assert!(h.traces.iter().any(|(event, reason)| event == "release"
            && reason == "native_cast_lock_ended_without_capture"));
        key(&mut c, &mut h, START + 131_000_000, 81, false);
        assert!(!h.active.contains_key(&finger));
    }
}

#[test]
fn a_buffer_received_after_unlock_still_uses_normal_native_cancellation() {
    let (mut c, mut h) = setup(UNLISTED, Some(false), 3, 0);
    native_sample(&mut c, &mut h, native_lock_frame(60, 100, true, false));
    key(&mut c, &mut h, START + 61_000_000, 81, true);
    let finger = c.skillTouch_;
    native_sample(&mut c, &mut h, native_lock_frame(70, 120, true, false));
    native_sample(&mut c, &mut h, native_lock_frame(90, 140, true, false));
    native_sample(&mut c, &mut h, native_lock_frame(120, 160, false, false));
    let mut captured = native_lock_frame(130, 180, false, false);
    captured.ui.active_slot = text(UNLISTED.slot);
    captured.skill_id = n(f64::from(UNLISTED.form));
    native_sample(&mut c, &mut h, captured);
    key(&mut c, &mut h, START + 131_000_000, 84, true);
    native_sample(&mut c, &mut h, native_lock_frame(145, 190, false, false));
    assert!(c.skillActivationObserved_ && c.skillCancelPending_);
    assert!(h.active.contains_key(&finger), "wait for the actual cancellation of the buffered cast");
}

#[test]
fn holding_empty_badang_q_does_not_create_a_contact_or_cast_on_recharge() {
    let badang_q = Skill { hero: 77, base: 7710, form: 7710, slot: "skill:0" };
    let (mut c, mut h) = setup(badang_q, Some(false), 2, 0);
    let movement = c.movementTouch_;
    let ms = |offset: i64| START + offset * 1_000_000;
    // The source publishes an effective cooldown for a validated empty pool
    // even if the native timer is idle and the cast lock flags are both false.
    for offset in (60..=1260).step_by(80) {
        let now = ms(offset);
        let mut f = resource_frame(now, badang_q, false);
        f.ui.cooldown_state.buttons[0].on_cooldown = flag(true);
        c.apply_snapshot(&mut h, &f, now);
        if offset == 60 {
            key(&mut c, &mut h, now + 1_000_000, 81, true);
            key(&mut c, &mut h, now + 2_000_000, 84, true);
            key(&mut c, &mut h, now + 3_000_000, 84, false);
        }
        assert_eq!(h.active.keys().copied().collect::<Vec<_>>(), [movement]);
        assert!(!c.skillCancelPending_ && c.queuedSkills_.is_empty());
    }
    let recovered = resource_frame(ms(1300), badang_q, false);
    c.apply_snapshot(&mut h, &recovered, ms(1300));
    assert!(c.apply_input(&mut h, Input::Key { code: 81, down: true, repeat: true }, ms(1301)));
    key(&mut c, &mut h, ms(1302), 81, false);
    assert_eq!(h.active.keys().copied().collect::<Vec<_>>(), [movement]);
    assert!(c.cooldownCasts_.is_empty());
    key(&mut c, &mut h, ms(1303), 81, true);
    assert!(h.active.contains_key(&c.skillTouch_));
}

#[test]
fn xborg_armor_change_releases_only_the_revoked_aim_and_preserves_a_queued_successor() {
    for (slot, base, old, current, code, queued) in [
        ("skill:0", 8310, 8315, 8310, 81, false),
        ("skill:0", 8310, 8315, 8310, 81, true),
        ("skill:1", 8320, 8320, 8325, 87, false),
    ] {
        let skill = Skill { hero: 83, slot, base, form: old };
        let (mut c, mut h) = setup(skill, Some(false), 2, 0);
        let movement = c.movementTouch_;
        let ms = |offset: i64| START + offset * 1_000_000;
        let sample = |time, form, owner: Option<i32>| {
            let mut f = frame(ms(time), Skill { form, ..skill }, Some(false));
            f.ui.cancel_uv = v2(0.9, 0.3);
            f.ui.active_slot = text(if owner.is_some() { slot } else { "" });
            f.skill_id = n(f64::from(owner.unwrap_or(0)));
            f
        };
        key(&mut c, &mut h, ms(61), code, true);
        let finger = c.skillTouch_;
        native_sample(&mut c, &mut h, sample(70, old, Some(old)));
        if queued {
            key(&mut c, &mut h, ms(71), 70, true);
            key(&mut c, &mut h, ms(72), 70, false);
            assert!(c.skillCancelPending_);
        }
        // An idle HUD alone, or an active native reticle on the same slot,
        // must never terminate a skill merely because a sample changed.
        native_sample(&mut c, &mut h, sample(75, old, None));
        native_sample(&mut c, &mut h, sample(80, current, Some(old)));
        assert!(h.active.contains_key(&finger));
        native_sample(&mut c, &mut h, sample(90, current, None));
        assert_eq!(h.ended, [finger]);
        assert!(!c.skillCancelPending_ && c.cooldownCasts_.is_empty());
        assert_eq!(h.active.keys().copied().collect::<Vec<_>>(), [movement]);
        assert_eq!(c.queuedSkills_.len(), usize::from(queued));
        assert!(h.traces.iter().any(|(event, reason)| event == "release" && reason == "xborg_armor_form_changed"));
        key(&mut c, &mut h, ms(91), code, false);
        if queued {
            native_sample(&mut c, &mut h, sample(100, current, None));
            assert!(c.cooldownCasts_.contains_key("extra:2:20150"));
        } else {
            key(&mut c, &mut h, ms(92), code, true);
            assert!(h.active.contains_key(&c.skillTouch_));
        }
    }
}
