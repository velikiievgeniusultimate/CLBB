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
    fn binding(&mut self, _: i32, button: &Button, _: &str) -> Binding {
        Binding { key: if button.kind == "extra" { 70 } else { 81 },
            cast_mode: self.cast_mode, aim_type: self.aim_type, stop_movement: false }
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
