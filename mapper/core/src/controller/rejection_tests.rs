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
            (17 | 22 | 27 | 33, "skill", "skill:1") => 87,
            (22 | 33, "skill", "skill:2") => 69,
            (27, "skill", "skill:3") => 82,
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
    let freya = Skill { hero: 22, base: 2250, form: 2250, slot: "skill:2" };
    let sun_q = Skill { hero: 27, base: 2710, form: 2710, slot: "skill:0" };
    let sun_w = Skill { hero: 27, base: 2750, form: 2750, slot: "skill:1" };
    let sun_r = Skill { hero: 27, base: 2730, form: 2730, slot: "skill:3" };
    for (skill, code, form) in [
        (miya, 81, 111), (freya, 69, 2254),
        (sun_q, 81, 2710), (sun_q, 81, 2705), (sun_q, 81, 2708),
        (sun_w, 87, 2706), (sun_w, 87, 2707),
        (sun_r, 82, 2709), (sun_r, 82, 2739),
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
