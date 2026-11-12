//! UI-choice regressions exercise public input/frame dispatch and transport events.
use super::{api::Input, host::Binding, *};

const T: i64 = 2_000_000_000;
const MS: i64 = 1_000_000;
const BASE: i32 = 13130;
const OWNER: u64 = 0x1234_5678_9abc;
const SLOT: &str = "skill:2";
const SOURCE: &str = "KeySkillMultiChoice.m_lastDragDir";

#[derive(Default)]
struct TestHost {
    next: i32,
    active: BTreeMap<i32, Point>,
    ended: Vec<(i32, Point)>,
    moves: Vec<(i32, Point)>,
    traces: Vec<(String, String)>,
    mode: i32,
    selection_type: u8,
    world_reads: usize,
    fail_move_id: Option<i32>,
}
impl Host for TestHost {
    fn begin(&mut self, p: Point) -> i32 {
        let id = self.next;
        self.next += 1;
        self.active.insert(id, p);
        id
    }
    fn move_touch(&mut self, id: i32, p: Point) -> bool {
        if self.fail_move_id == Some(id) { return false; }
        let Some(old) = self.active.get_mut(&id) else { return false };
        *old = p;
        self.moves.push((id, p));
        true
    }
    fn end(&mut self, id: i32, p: Point) {
        assert!(self.active.remove(&id).is_some(), "contact ended twice");
        self.ended.push((id, p));
    }
    fn pointer_world(&mut self, _: Point) -> Option<Point> {
        self.world_reads += 1;
        None
    }
    fn ground_world(&mut self, p: Point) -> Option<Point> { Some(p * 10.) }
    fn movement_direction(&mut self, _: Point) -> Option<Point> { None }
    fn hero_world(&mut self) -> Option<Point> { Some(Point::default()) }
    fn pointer_world_at_plane(&mut self, _: Point, _: f64) -> Option<Point> {
        self.world_reads += 1;
        None
    }
    fn capture_pointer_world(&mut self, _: Point) -> u64 {
        self.world_reads += 1;
        0
    }
    fn resolve_world(&mut self, _: u64) -> Option<Point> { None }
    fn release_resolver(&mut self, _: u64) {}
    fn navigation(&mut self, _: Point, p: Point, _: bool) -> Result<Vec<Point>, String> { Ok(vec![p]) }
    fn binding(&mut self, _: i32, b: &Button, _: &str) -> Binding {
        let (key, aim_type, cast_mode) = match b.slot.as_str() {
            SLOT => (69, self.selection_type, self.mode),
            "skill:0" => (81, 2, 0),
            _ => (32, 1, 0),
        };
        Binding { key, aim_type, cast_mode, stop_movement: false }
    }
    fn cancel_key(&mut self, _: i32) -> i32 { 84 }
    fn timer(&mut self, _: Timer, _: Option<i64>) {}
    fn trace(&mut self, _: &Controller, event: &str, reason: &str) {
        self.traces.push((event.into(), reason.into()));
    }
}
fn n(value: f64) -> snapshot::Number { snapshot::Number { value, kind: 1 } }
fn flag(value: bool) -> snapshot::Flag { snapshot::Flag { value: value as i32 } }
fn text(s: &str) -> snapshot::Text {
    let mut t = snapshot::Text::default();
    t.present = 1;
    t.size = s.len() as u32;
    t.bytes[..s.len()].copy_from_slice(s.as_bytes());
    t
}
fn v2(x: f64, y: f64) -> snapshot::Vector2 {
    snapshot::Vector2 { values: [n(x), n(y)], count: 2 }
}
fn v3(x: f64, y: f64, z: f64) -> snapshot::Vector3 {
    snapshot::Vector3 { values: [n(x), n(y), n(z)], count: 3 }
}
fn frame(at: i64) -> snapshot::Frame {
    let mut f = snapshot::Frame {
        ready: flag(true), hero_id: n(131.), time_ns: n(at as f64),
        game_clock_ms: n(((at - T) / MS + 1000) as f64), sample_duration_ms: n(0.25),
        skill_id: n(0.), token: text("selection-match"), token_string: flag(true),
        token_nonempty: flag(true), hero_screen_uv: v2(0.5, 0.5),
        hero_render_xyz: v3(0., 0., 0.), ..Default::default()
    };
    f.view_matrix = snapshot::Vector16 { count: 16,
        values: std::array::from_fn(|i| n(if i == 9 { -1. } else if i % 5 == 0 { 1. } else { 0. })) };
    f.ui.ready = flag(true);
    f.ui.hero_id = f.hero_id;
    f.ui.time_ns = f.time_ns;
    f.ui.resolution_px = v2(1000., 1000.);
    f.ui.cancel_uv = v2(0.95, 0.2);
    f.ui.movement = snapshot::MovementGeometry {
        ready: flag(true), press_uv: v2(0.15, 0.85), center_uv: v2(0.15, 0.85),
        radius_uv: v2(0.1, 0.1),
    };
    f.ui.button_count = 3;
    for (i, (slot, kind, skill, x)) in [
        (SLOT, "skill", BASE, 0.8), ("skill:0", "skill", 13110, 0.7),
        ("attack", "attack", 13100, 0.9),
    ].into_iter().enumerate() {
        f.ui.buttons[i] = snapshot::Button {
            slot: text(slot), kind: text(kind), index: n(i as f64), skill_id: n(skill as f64),
            active: flag(true), press_uv: v2(x, 0.8), center_uv: v2(x, 0.8),
            radius_uv: v2(0.1, 0.1), ..Default::default()
        };
    }
    f.skill_state = snapshot::VariantState {
        ready: flag(true), source: text("ShowSkillData.m_TranID"), token: f.token,
        hero_id: f.hero_id, sample_start_ns: n(at as f64 - 250_000.), sample_end_ns: f.time_ns,
        count: 2, ..Default::default()
    };
    for (i, (slot, base)) in [(SLOT, BASE), ("skill:0", 13110)].into_iter().enumerate() {
        f.skill_state.buttons[i] = snapshot::Variant {
            slot: text(slot), base_skill_id: n(base as f64), active_skill_id: n(base as f64),
            cast_locked: flag(false), state_forbidden: flag(false), charge_skill: flag(false),
        };
    }
    f.selection_input = snapshot::SelectionInput {
        kind: n(1.),
        ready: flag(true), coherent: flag(true), input_ready: flag(false),
        panel_visible: flag(false), has_drag: flag(false), selected_option_valid: flag(false),
        source: text(SOURCE), token: f.token, slot: text(SLOT), hero_id: f.hero_id,
        base_skill_id: n(BASE as f64), active_skill_id: n(BASE as f64), native_skill_id: n(0.),
        owner_id: OWNER, sample_start_ns: n(at as f64 - 250_000.), sample_end_ns: f.time_ns,
        hover_index: n(-1.), ..Default::default()
    };
    f
}
fn captured(at: i64) -> snapshot::Frame {
    let mut f = frame(at);
    f.skill_id = n(BASE as f64);
    f.ui.active_slot = text(SLOT);
    f.selection_input.native_skill_id = f.skill_id;
    f.selection_input.panel_visible = flag(true);
    f.ui.joystick = snapshot::Geometry {
        ready: flag(true), source: text("active_widget_transform"), slot: text(SLOT),
        skill_id: n(BASE as f64), sample_start_ns: f.selection_input.sample_start_ns,
        sample_end_ns: f.selection_input.sample_end_ns, center_uv: v2(0.8, 0.8),
        radius_uv: v2(0.1, 0.1),
    };
    f
}
fn ack(at: i64, p: Point) -> snapshot::Frame {
    ack_with_wheel(at, p, Point::new(0.8, 0.8), Point::new(0.1, 0.1))
}
fn ack_with_wheel(at: i64, p: Point, center: Point, radius: Point) -> snapshot::Frame {
    let mut f = captured(at);
    f.ui.joystick.center_uv = v2(center.x, center.y);
    f.ui.joystick.radius_uv = v2(radius.x, radius.y);
    let x = (p.x * 1000.).floor() * 999. / 1000.;
    let y = 1000. - (p.y * 1000.).floor() * 999. / 1000.;
    let delta = Point::new(x - center.x * 1000., y - (1. - center.y) * 1000.);
    let dir = delta / delta.length();
    f.selection_input.input_ready = flag(true);
    f.selection_input.has_drag = flag(true);
    f.selection_input.selected_option_valid = flag(true);
    f.selection_input.hover_index = n(0.);
    f.selection_input.selected_option = n(1.);
    f.selection_input.raw_touch_xyz = v3(x, y, 0.);
    f.selection_input.drag_direction_xy = v2(dir.x, dir.y);
    f
}
fn cancel_ack(at: i64) -> snapshot::Frame {
    let mut f = captured(at);
    f.cancel_state = snapshot::CancelState {
        ready: flag(true), coherent: flag(true), source: text("UIKeySkill.bCancelSkill"),
        token: f.token, hero_id: f.hero_id, slot: text(SLOT), base_skill_id: n(BASE as f64),
        cancelled: flag(true), sample_start_ns: f.selection_input.sample_start_ns,
        sample_end_ns: f.time_ns,
    };
    f
}
fn key(c: &mut Controller, h: &mut TestHost, ms: i64, code: i32, down: bool) {
    assert!(c.apply_input(h, Input::Key { code, down, repeat: false }, T + ms * MS));
}
fn snapshot(c: &mut Controller, h: &mut TestHost, f: snapshot::Frame) {
    let at = f.time_ns.value as i64;
    c.apply_snapshot(h, &f, at);
}
fn setup(mode: i32) -> (Controller, TestHost) {
    let mut c = Controller::new();
    let mut h = TestHost { mode, selection_type: 4, ..Default::default() };
    c.apply_input(&mut h, Input::Trace(true), T);
    c.apply_input(&mut h, Input::Enabled(true), T);
    c.apply_input(&mut h, Input::HoldThreshold(40), T);
    snapshot(&mut c, &mut h, frame(T));
    c.apply_input(&mut h, Input::Mouse { button: 2, down: true, point: Point::new(0.7, 0.5) }, T + MS);
    snapshot(&mut c, &mut h, frame(T + 50 * MS));
    assert!(c.movementHeld_);
    (c, h)
}
fn started(mode: i32) -> (Controller, TestHost, i32, i32) {
    let (mut c, mut h) = setup(mode);
    let movement = c.movementTouch_;
    key(&mut c, &mut h, 60, 69, true);
    let finger = c.skillTouch_;
    assert!(finger >= 0);
    snapshot(&mut c, &mut h, captured(T + 70 * MS));
    assert!(c.selection_.captured && c.selection_.geometry_fresh);
    (c, h, finger, movement)
}
fn moved(h: &TestHost, id: i32) -> usize { h.moves.iter().filter(|(i, _)| *i == id).count() }
fn released(h: &TestHost, id: i32) -> bool { h.ended.iter().any(|(i, _)| *i == id) }
fn reason(h: &TestHost, s: &str) -> bool { h.traces.iter().any(|(_, reason)| reason == s) }

#[test]
fn unknown_choice_phase_rejects_down_until_physical_up_but_foreign_idle_packet_does_not_gate_next_press() {
    let (mut c, mut h) = setup(0);
    let mut f = frame(T + 55 * MS);
    f.skill_state = Default::default();
    snapshot(&mut c, &mut h, f);
    let before = h.next;
    key(&mut c, &mut h, 60, 69, true);
    assert_eq!(h.next, before, "unknown native phase must not create a finger");
    snapshot(&mut c, &mut h, frame(T + 65 * MS));
    key(&mut c, &mut h, 66, 69, true);
    assert_eq!(h.next, before, "held rejected key must not rearm itself");
    key(&mut c, &mut h, 67, 69, false);
    let mut foreign = frame(T + 68 * MS);
    foreign.selection_input.slot = text("skill:1");
    snapshot(&mut c, &mut h, foreign);
    key(&mut c, &mut h, 69, 69, true);
    let finger = c.skillTouch_;
    assert!(finger >= 0);
    assert_eq!(moved(&h, finger), 0);
    assert_eq!(c.selection_.owner, 0);
    snapshot(&mut c, &mut h, captured(T + 75 * MS));
    assert!(moved(&h, finger) > 0, "post-DOWN owned capture bootstraps the UI drag");
}

#[test]
fn ordinary_up_requires_exact_owned_ui_ack_and_keeps_frozen_goal_and_movement() {
    let (mut c, mut h, finger, movement) = started(0);
    key(&mut c, &mut h, 80, 69, false);
    assert!(!released(&h, finger), "successful MOVE alone is not Selection release proof");
    let goal = h.active[&finger];
    c.apply_input(&mut h, Input::Pointer(Point::new(0.1, 0.1)), T + 81 * MS);
    c.apply_input(&mut h, Input::Timer(Timer::Pointer), T + 82 * MS);
    assert_eq!(h.active[&finger], goal, "pointer after UP must not change the frozen choice");
    snapshot(&mut c, &mut h, ack(T + 90 * MS, goal));
    assert!(released(&h, finger));
    assert!(reason(&h, "selection_up_after_ui_ack"));
    assert!(h.active.contains_key(&movement) && c.movementHeld_);
    assert_eq!(h.world_reads, 0, "direct Selection dispatch must not project into world space");
    snapshot(&mut c, &mut h, frame(T + 100 * MS));
    key(&mut c, &mut h, 101, 69, true);
    assert!(c.skillTouch_ >= 0 && c.skillTouch_ != finger);
}

#[test]
fn cold_up_freezes_original_hero_bearing_before_late_owned_capture() {
    let (mut c, mut h) = setup(0);
    let movement = c.movementTouch_;
    key(&mut c, &mut h, 60, 69, true);
    let finger = c.skillTouch_;
    key(&mut c, &mut h, 61, 69, false);
    assert_eq!(moved(&h, finger), 0);
    c.apply_input(&mut h, Input::Pointer(Point::new(0.1, 0.1)), T + 62 * MS);
    let mut f = captured(T + 70 * MS);
    f.hero_screen_uv = v2(0.7, 0.6);
    snapshot(&mut c, &mut h, f);
    let goal = h.active[&finger];
    assert!(goal.x > 0.89 && (goal.y - 0.8008008008).abs() < 0.002,
        "late hero position changed the rightward physical-UP bearing: {goal:?}");
    assert!(!released(&h, finger) && h.active.contains_key(&movement));
    snapshot(&mut c, &mut h, ack(T + 80 * MS, goal));
    assert!(released(&h, finger) && h.active.contains_key(&movement));
}

#[test]
fn choice_uses_current_wheel_geometry_instead_of_down_button_center() {
    let (mut c, mut h) = setup(0);
    key(&mut c, &mut h, 60, 69, true);
    let finger = c.skillTouch_;
    let movement = c.movementTouch_;
    let center = Point::new(0.903, 0.67);
    let radius = Point::new(0.065, 0.065);
    let mut f = captured(T + 70 * MS);
    f.ui.joystick.center_uv = v2(center.x, center.y);
    f.ui.joystick.radius_uv = v2(radius.x, radius.y);
    snapshot(&mut c, &mut h, f);
    key(&mut c, &mut h, 80, 69, false);
    let goal = h.active[&finger];
    assert!(goal.x > 0.967 && (goal.y - 0.6706706707).abs() < 0.002,
        "selection followed button DOWN center rather than current clamped wheel: {goal:?}");
    snapshot(&mut c, &mut h, ack_with_wheel(T + 90 * MS, goal, center, radius));
    assert!(released(&h, finger) && h.active.contains_key(&movement));
}

#[test]
fn malformed_foreign_or_precommand_choice_feedback_never_commits_the_pending_up() {
    for case in 0..13 {
        let (mut c, mut h, finger, movement) = started(0);
        key(&mut c, &mut h, 80, 69, false);
        let mut f = ack(T + 90 * MS, h.active[&finger]);
        match case {
            0 => f.selection_input.source = text("IndicatorMotionRot.m_vDragResult"),
            1 => f.selection_input.owner_id += 1,
            2 => f.selection_input.token = text("other-match"),
            3 => f.selection_input.hero_id = n(25.),
            4 => f.selection_input.slot = text("skill:0"),
            5 => f.selection_input.base_skill_id = n(13120.),
            6 => { f.selection_input.native_skill_id = n(0.); f.skill_id = n(0.); }
            7 => { f.selection_input.sample_start_ns = n((T + 79 * MS) as f64);
                f.ui.joystick.sample_start_ns = f.selection_input.sample_start_ns; }
            8 => f.selection_input.selected_option_valid = Default::default(),
            9 => f.selection_input.selected_option = n(0.),
            10 => f.selection_input.hover_index = n(-1.),
            11 => f.selection_input.drag_direction_xy = v2(0., -1.),
            12 => f.selection_input.raw_touch_xyz.values[0].kind = 2,
            _ => unreachable!(),
        }
        snapshot(&mut c, &mut h, f);
        assert!(!released(&h, finger), "case {case}");
        assert!(h.active.contains_key(&movement), "case {case}");
        assert!(!reason(&h, "selection_up_after_ui_ack"), "case {case}");
    }
}

#[test]
fn owner_rebind_permanently_blocks_both_pointer_and_timer_echoes() {
    let (mut c, mut h, finger, movement) = started(0);
    key(&mut c, &mut h, 80, 69, false);
    let mut f = captured(T + 90 * MS);
    f.selection_input.owner_id += 1;
    snapshot(&mut c, &mut h, f);
    assert!(c.selection_.revoked);
    let count = moved(&h, finger);
    c.apply_input(&mut h, Input::Pointer(Point::new(0.2, 0.9)), T + 100 * MS);
    c.apply_input(&mut h, Input::Timer(Timer::Pointer), T + 125 * MS);
    c.apply_input(&mut h, Input::Timer(Timer::Main), T + 130 * MS);
    snapshot(&mut c, &mut h, captured(T + 140 * MS));
    assert_eq!(moved(&h, finger), count, "owner cannot recover by a later matching packet");
    assert!(!released(&h, finger) && h.active.contains_key(&movement));
}

#[test]
fn cancel_requires_own_post_cancel_move_native_flag_not_choice_or_direction_feedback() {
    for case in 0..6 {
        let (mut c, mut h, finger, movement) = started(0);
        key(&mut c, &mut h, 80, 84, true);
        key(&mut c, &mut h, 81, 69, false);
        assert!(c.skillCancelPending_ && !released(&h, finger));
        let mut f = cancel_ack(T + 90 * MS);
        match case {
            0 => f.cancel_state.cancelled = flag(false),
            1 => f.cancel_state.source = text("KeySkillMultiChoice.m_lastDragDir"),
            2 => f.selection_input.owner_id += 1,
            3 => f.cancel_state.sample_start_ns = n((T + 79 * MS) as f64),
            4 => f.cancel_state.base_skill_id = n(13120.),
            5 => {},
            _ => unreachable!(),
        }
        snapshot(&mut c, &mut h, f);
        assert_eq!(released(&h, finger), case == 5, "cancel case {case}");
        assert!(h.active.contains_key(&movement), "cancel case {case}");
        assert!(!reason(&h, "selection_up_after_ui_ack"));
        assert!(c.cooldownCasts_.is_empty(), "cancel is not a cast reservation");
    }
}

#[test]
fn queued_next_skill_waits_for_selection_cancel_ack_and_new_native_idle_frame() {
    let (mut c, mut h, finger, movement) = started(0);
    key(&mut c, &mut h, 80, 32, true);
    let aux = h.next - 1;
    assert!(aux != finger && aux != movement && h.active.contains_key(&aux));
    key(&mut c, &mut h, 81, 81, true);
    assert_eq!(c.queuedSkills_.len(), 1);
    assert!(c.skillCancelPending_);
    snapshot(&mut c, &mut h, captured(T + 85 * MS));
    assert_eq!(c.skillTouch_, finger);
    assert!(h.active.contains_key(&aux) && h.active.contains_key(&movement));
    snapshot(&mut c, &mut h, cancel_ack(T + 90 * MS));
    assert!(released(&h, finger));
    assert_eq!(c.skillTouch_, -1, "same pre-UP native snapshot cannot dispatch queued Q");
    assert_eq!(c.queuedSkills_.len(), 1);
    assert!(h.active.contains_key(&aux) && h.active.contains_key(&movement));
    snapshot(&mut c, &mut h, frame(T + 100 * MS));
    assert!(c.skillTouch_ >= 0 && c.skillTouch_ != finger);
    assert_eq!(c.activeSlot_, "skill:0");
    assert!(c.queuedSkills_.is_empty());
    assert!(h.active.contains_key(&movement) && h.active.contains_key(&aux));
}

#[test]
fn instant_choice_still_waits_for_ui_ack_and_normal_tap_and_direction_remain_independent() {
    let (mut c, mut h, finger, movement) = started(1);
    assert!(c.pendingRelease_ && c.skillKeyHeld_);
    assert!(!released(&h, finger));
    let goal = h.active[&finger];
    snapshot(&mut c, &mut h, ack(T + 80 * MS, goal));
    assert!(released(&h, finger) && h.active.contains_key(&movement));
    assert!(reason(&h, "selection_up_after_ui_ack"));
    key(&mut c, &mut h, 81, 69, false);
    snapshot(&mut c, &mut h, frame(T + 90 * MS));
    key(&mut c, &mut h, 91, 32, true);
    let aux = h.next - 1;
    c.apply_input(&mut h, Input::Timer(Timer::Auxiliary), T + 108 * MS);
    assert!(!released(&h, aux), "default auxiliary action waits for physical UP");
    key(&mut c, &mut h, 108, 32, false);
    assert!(released(&h, aux));
    key(&mut c, &mut h, 109, 81, true);
    let direction = c.skillTouch_;
    key(&mut c, &mut h, 110, 81, false);
    assert!(released(&h, direction), "ordinary Direction keeps its physical-UP semantics");
    assert!(h.active.contains_key(&movement));
}

#[test]
fn captured_choice_external_end_retires_only_old_contact_without_cast_or_cancel_claim() {
    for ending in 0..3 {
        let (mut c, mut h, finger, movement) = started(0);
        if ending == 1 { key(&mut c, &mut h, 80, 69, false); }
        if ending == 2 { key(&mut c, &mut h, 80, 84, true); }
        let mut first = frame(T + 90 * MS);
        first.game_clock_ms = n(1200.);
        snapshot(&mut c, &mut h, first);
        assert!(!released(&h, finger), "one native empty tick is insufficient");
        let mut second = frame(T + 100 * MS);
        second.game_clock_ms = n(1201.);
        snapshot(&mut c, &mut h, second);
        assert!(released(&h, finger), "end state {ending}");
        assert!(reason(&h, "owned_panel_capture_and_two_empty_native_ticks"));
        assert!(reason(&h, "auto_completed"));
        assert!(!reason(&h, "selection_up_after_ui_ack"));
        assert!(!reason(&h, "cancel"));
        assert!(c.cooldownCasts_.is_empty());
        assert!(h.active.contains_key(&movement) && c.movementHeld_);
        key(&mut c, &mut h, 101, 69, false);
        assert_eq!(h.ended.iter().filter(|(id, _)| *id == finger).count(), 1);
    }
}

#[test]
fn external_end_rejects_no_capture_unknown_clock_overlap_and_pre_down_evidence() {
    for case in 0..8 {
        let (mut c, mut h) = setup(0);
        key(&mut c, &mut h, 60, 69, true);
        let finger = c.skillTouch_;
        let movement = c.movementTouch_;
        if case != 0 { snapshot(&mut c, &mut h, captured(T + 70 * MS)); }
        let mut first = frame(T + 90 * MS);
        first.game_clock_ms = n(1200.);
        if case == 7 { first.selection_input.panel_visible = Default::default(); }
        snapshot(&mut c, &mut h, first);
        let mut second = frame(T + 100 * MS);
        second.game_clock_ms = n(1201.);
        match case {
            0 => {},
            1 => second.game_clock_ms = n(1200.),
            2 => second.game_clock_ms = Default::default(),
            3 => second.selection_input.sample_start_ns = n((T + 89 * MS) as f64),
            4 => second.selection_input.sample_start_ns = n((T + 59 * MS) as f64),
            5 => second.selection_input.has_drag = Default::default(),
            6 => second.selection_input = Default::default(),
            7 => {},
            _ => unreachable!(),
        }
        snapshot(&mut c, &mut h, second);
        assert!(!released(&h, finger), "native end guard {case}");
        assert!(h.active.contains_key(&movement), "native end guard {case}");
        assert!(!reason(&h, "owned_panel_capture_and_two_empty_native_ticks"));
    }
}

#[test]
fn verified_future_phase_waits_for_owned_ui_to_clear_then_two_native_ticks_without_morphing() {
    let (mut c, mut h, finger, movement) = started(0);
    h.selection_type = 1;
    let future_frame = |at: i64, pressed: bool| {
        let mut f = if pressed { captured(at) } else { frame(at) };
        f.skill_state.buttons[0].active_skill_id = n(13131.);
        f.selection_input.active_skill_id = n(13131.);
        f.selection_input.native_skill_id = n(13131.);
        f.skill_id = n(13131.);
        f
    };
    let count = moved(&h, finger);
    snapshot(&mut c, &mut h, future_frame(T + 80 * MS, true));
    assert_eq!(c.skillBehavior_, SkillBehavior::Selection);
    assert!(!c.selection_.revoked && !released(&h, finger));
    assert_eq!(moved(&h, finger), count);
    snapshot(&mut c, &mut h, future_frame(T + 90 * MS, false));
    assert!(!released(&h, finger));
    snapshot(&mut c, &mut h, future_frame(T + 100 * MS, false));
    assert!(released(&h, finger) && h.active.contains_key(&movement));
    assert!(reason(&h, "auto_completed") && c.cooldownCasts_.is_empty());
}

#[test]
fn missing_choice_receipt_at_release_deadline_requests_cancel_and_never_forces_up() {
    let (mut c, mut h, finger, movement) = started(0);
    key(&mut c, &mut h, 80, 69, false);
    for ms in [250, 450, 650, 850, 1050, 1250, 1450, 1580] {
        snapshot(&mut c, &mut h, captured(T + ms * MS));
    }
    assert!(c.skillCancelPending_);
    assert!(!released(&h, finger));
    assert!(h.active.contains_key(&movement));
    snapshot(&mut c, &mut h, cancel_ack(T + 1590 * MS));
    assert!(released(&h, finger) && h.active.contains_key(&movement));
    assert!(c.cooldownCasts_.is_empty());
}

#[test]
fn unproven_plain_and_center_modes_never_fall_through_to_choice_or_tap() {
    for mode in [2, 3] {
        let (mut c, mut h) = setup(mode);
        let movement = c.movementTouch_;
        let count = h.next;
        key(&mut c, &mut h, 60, 69, true);
        assert_eq!(h.next, count, "unproven Selection mode {mode}");
        assert!(reason(&h, "selection_plain_or_center_mode_unproven"));
        assert!(h.active.contains_key(&movement));
        h.mode = 0;
        snapshot(&mut c, &mut h, frame(T + 65 * MS));
        key(&mut c, &mut h, 66, 69, true);
        assert_eq!(h.next, count, "changing mode cannot rearm the held rejected key");
        key(&mut c, &mut h, 67, 69, false);
        key(&mut c, &mut h, 68, 69, true);
        assert!(c.skillTouch_ >= 0 && c.skillBehavior_ == SkillBehavior::Selection);
    }
}

#[test]
fn queued_choice_rejected_after_its_physical_up_does_not_swallow_the_next_real_press() {
    let (mut c, mut h) = setup(0);
    key(&mut c, &mut h, 60, 81, true);
    let previous = c.skillTouch_;
    key(&mut c, &mut h, 65, 69, true);
    key(&mut c, &mut h, 66, 69, false);
    assert_eq!(c.queuedSkills_.len(), 1);
    assert!(!c.queuedSkills_[0].held && c.skillCancelPending_);
    h.mode = 2;
    let mut f = frame(T + 75 * MS);
    f.skill_id = n(13110.);
    f.ui.active_slot = text("skill:0");
    f.cancel_state = snapshot::CancelState {
        ready: flag(true), coherent: flag(true), source: text("UIKeySkill.bCancelSkill"),
        token: f.token, hero_id: f.hero_id, slot: text("skill:0"), base_skill_id: n(13110.),
        cancelled: flag(true), sample_start_ns: n((T + 74 * MS) as f64), sample_end_ns: f.time_ns,
    };
    snapshot(&mut c, &mut h, f);
    assert!(released(&h, previous));
    snapshot(&mut c, &mut h, frame(T + 80 * MS));
    assert!(c.queuedSkills_.is_empty() && c.skillTouch_ < 0);
    assert!(!c.auxiliaryHeldKeys_.contains(&69), "queued physical UP already consumed this key");
    h.mode = 0;
    snapshot(&mut c, &mut h, frame(T + 90 * MS));
    key(&mut c, &mut h, 91, 69, true);
    assert!(c.skillTouch_ >= 0 && c.skillBehavior_ == SkillBehavior::Selection);
}

#[test]
fn choice_final_move_failure_waits_for_native_cancel_ack_and_preserves_movement() {
    let (mut c, mut h, finger, movement) = started(0);
    h.fail_move_id = Some(finger);
    key(&mut c, &mut h, 80, 69, false);
    assert!(c.skillCancelPending_ && !released(&h, finger));
    assert_eq!(c.skillCancelSentNs_, 0);
    assert!(h.active.contains_key(&movement));
    h.fail_move_id = None;
    snapshot(&mut c, &mut h, captured(T + 90 * MS));
    assert!(c.skillCancelSentNs_ > 0 && !released(&h, finger));
    snapshot(&mut c, &mut h, cancel_ack(T + 100 * MS));
    assert!(released(&h, finger) && h.active.contains_key(&movement));
    assert!(!reason(&h, "selection_up_after_ui_ack"));
    assert!(c.cooldownCasts_.is_empty());
}
