//! Target regressions use public dispatch and a transport ledger, never native calls.
use crate::Point;
use crate::controller::{
    Button, Controller,
    api::Input,
    host::{Binding, Host, Timer},
    snapshot,
};
use std::collections::BTreeMap;
const T: i64 = 8_000_000_000;
const MS: i64 = 1_000_000;
const W: f64 = 1000.;
const H: f64 = 800.;
const BASE: i32 = 5530;
const SLOT: &str = "skill:2";
const SOURCE: &str = "UILockHeroSkill.UICamera.activeTouches";
const CANCEL_SOURCE: &str = "UILockHeroSkill.m_bCancelSkill";
const OWNER: u64 = (1 << 60) + 901;
const BACKEND: u64 = (1 << 60) + 902;
const CONTACT: u64 = (1 << 60) + 903;
const TARGET: u64 = (1 << 60) + 904;
const SELF: u64 = (1 << 60) + 905;
#[derive(Default)]
struct Ledger {
    next: i32,
    active: BTreeMap<i32, Point>,
    moves: Vec<(i32, Point)>,
    ended: Vec<(i32, Point)>,
    traces: Vec<(String, String)>,
    world_reads: usize,
    world_kinds: Vec<&'static str>,
    abort_all_calls: usize,
    fail_move_id: Option<i32>,
}
impl Host for Ledger {
    fn begin(&mut self, p: Point) -> i32 {
        let id = self.next;
        self.next += 1;
        self.active.insert(id, p);
        id
    }
    fn move_touch(&mut self, id: i32, p: Point) -> bool {
        if self.fail_move_id == Some(id) { return false; }
        let Some(v) = self.active.get_mut(&id) else {
            return false;
        };
        *v = p;
        self.moves.push((id, p));
        true
    }
    fn end(&mut self, id: i32, p: Point) {
        assert!(self.active.remove(&id).is_some(), "duplicate nativeUP");
        self.ended.push((id, p));
    }
    fn abort_all_touches(&mut self) -> bool {
        self.abort_all_calls += 1;
        false
    }
    fn pointer_world(&mut self, _: Point) -> Option<Point> {
        self.world_reads += 1;
        self.world_kinds.push("pointer_world");
        None
    }
    fn ground_world(&mut self, _: Point) -> Option<Point> {
        self.world_reads += 1;
        self.world_kinds.push("ground_world");
        None
    }
    fn movement_direction(&mut self, _: Point) -> Option<Point> {
        Some(Point::new(1., 0.))
    }
    fn hero_world(&mut self) -> Option<Point> {
        self.world_reads += 1;
        self.world_kinds.push("hero_world");
        None
    }
    fn pointer_world_at_plane(&mut self, _: Point, _: f64) -> Option<Point> {
        self.world_reads += 1;
        self.world_kinds.push("pointer_world_at_plane");
        None
    }
    fn capture_pointer_world(&mut self, _: Point) -> u64 {
        self.world_reads += 1;
        self.world_kinds.push("capture_pointer_world");
        0
    }
    fn resolve_world(&mut self, _: u64) -> Option<Point> {
        self.world_reads += 1;
        self.world_kinds.push("resolve_world");
        None
    }
    fn release_resolver(&mut self, _: u64) {}
    fn navigation(&mut self, _: Point, p: Point, _: bool) -> Result<Vec<Point>, String> {
        Ok(vec![p])
    }
    fn binding(&mut self, _: i32, b: &Button, _: &str) -> Binding {
        Binding {
            key: if b.slot == SLOT {
                69
            } else if b.slot == "skill:0" {
                81
            } else {
                32
            },
            aim_type: if b.slot == SLOT { 4 } else { 2 },
            cast_mode: 0,
            stop_movement: false,
        }
    }
    fn cancel_key(&mut self, _: i32) -> i32 {
        84
    }
    fn timer(&mut self, _: Timer, _: Option<i64>) {}
    fn trace(&mut self, _: &Controller, event: &str, reason: &str) {
        self.traces.push((event.into(), reason.into()));
    }
}
fn n(v: f64) -> snapshot::Number {
    snapshot::Number { kind: 1, value: v }
}
fn flag(v: bool) -> snapshot::Flag {
    snapshot::Flag { value: v as i32 }
}
fn text(v: &str) -> snapshot::Text {
    let mut t = snapshot::Text::default();
    t.present = 1;
    t.size = v.len() as u32;
    t.bytes[..v.len()].copy_from_slice(v.as_bytes());
    t
}
fn v2(x: f64, y: f64) -> snapshot::Vector2 {
    snapshot::Vector2 {
        count: 2,
        values: [n(x), n(y)],
    }
}
fn v3(x: f64, y: f64, z: f64) -> snapshot::Vector3 {
    snapshot::Vector3 {
        count: 3,
        values: [n(x), n(y), n(z)],
    }
}
fn frame(ms: i64) -> snapshot::Frame {
    let at = T + ms * MS;
    let mut f = snapshot::Frame {
        ready: flag(true),
        hero_id: n(55.),
        time_ns: n(at as f64),
        game_clock_ms: n((1000 + ms) as f64),
        sample_duration_ms: n(0.25),
        skill_id: n(0.),
        token: text("target-match"),
        token_nonempty: flag(true),
        token_string: flag(true),
        hero_screen_uv: v2(0.5, 0.5),
        hero_render_xyz: v3(0., 0., 0.),
        ..Default::default()
    };
    f.view_matrix = snapshot::Vector16 {
        count: 16,
        values: std::array::from_fn(|i| {
            n(if i == 9 {
                -1.
            } else if i % 5 == 0 {
                1.
            } else {
                0.
            })
        }),
    };
    f.ui.ready = flag(true);
    f.ui.hero_id = f.hero_id;
    f.ui.time_ns = f.time_ns;
    f.ui.resolution_px = v2(W, H);
    f.ui.cancel_uv = v2(0.94, 0.18);
    f.ui.movement = snapshot::MovementGeometry {
        ready: flag(true),
        press_uv: v2(0.15, 0.85),
        center_uv: v2(0.15, 0.85),
        radius_uv: v2(0.1, 0.1),
    };
    f.ui.button_count = 2;
    for (i, (slot, skill, x)) in [(SLOT, BASE, 0.8), ("skill:0", 5510, 0.7)]
        .into_iter()
        .enumerate()
    {
        f.ui.buttons[i] = snapshot::Button {
            slot: text(slot),
            kind: text("skill"),
            index: n(i as f64),
            skill_id: n(skill as f64),
            active: flag(true),
            press_uv: v2(x, 0.8),
            center_uv: v2(x, 0.8),
            radius_uv: v2(0.1, 0.1),
            ..Default::default()
        };
    }
    f.skill_state = snapshot::VariantState {
        ready: flag(true),
        source: text("ShowSkillData.m_TranID"),
        token: f.token,
        hero_id: f.hero_id,
        sample_start_ns: n(at as f64 - 250_000.),
        sample_end_ns: f.time_ns,
        count: 2,
        ..Default::default()
    };
    for (i, (slot, skill)) in [(SLOT, BASE), ("skill:0", 5510)].into_iter().enumerate() {
        f.skill_state.buttons[i] = snapshot::Variant {
            slot: text(slot),
            base_skill_id: n(skill as f64),
            active_skill_id: n(skill as f64),
            cast_locked: flag(false),
            state_forbidden: flag(false),
            charge_skill: flag(false),
        };
    }
    f.selection_input = snapshot::SelectionInput {
        kind: n(2.),
        ready: flag(true),
        coherent: flag(true),
        input_ready: flag(false),
        panel_visible: flag(false),
        has_drag: flag(false),
        source: text(SOURCE),
        token: f.token,
        slot: text(SLOT),
        hero_id: f.hero_id,
        base_skill_id: n(BASE as f64),
        active_skill_id: n(BASE as f64),
        native_skill_id: n(0.),
        owner_id: OWNER,
        backend_owner_id: BACKEND,
        backend_state: n(3.),
        contact_present: flag(false),
        sample_start_ns: n(at as f64 - 250_000.),
        sample_end_ns: f.time_ns,
        ..Default::default()
    };
    f
}
fn held(ms: i64) -> snapshot::Frame {
    let mut f = frame(ms);
    f.ui.active_slot = text(SLOT);
    f.selection_input.panel_visible = flag(true);
    f.selection_input.contact_present = flag(true);
    f.selection_input.contact_owner_id = CONTACT;
    f.selection_input.native_contact_id = n(-7.);
    f.selection_input.backend_state = n(4.);
    f.selection_input.target_cancel_ready = flag(true);
    f.selection_input.target_cancel_uv = v2(0.88, 0.27);
    f
}
fn point_receipt(ms: i64, p: Point, guid: Option<u32>) -> snapshot::Frame {
    let mut f = held(ms);
    let s = &mut f.selection_input;
    s.input_ready = flag(true);
    s.has_drag = flag(true);
    s.raw_touch_xyz = v3(
        (p.x * W).floor() * (W - 1.) / W,
        H - (p.y * H).floor() * (H - 1.) / H,
        0.,
    );
    if let Some(id) = guid {
        s.target_guid = n(id as f64);
        s.target_valid = flag(true);
        s.target_owner_id = TARGET;
        s.target_self_id = SELF;
    }
    f
}
fn cancel_receipt(ms: i64, p: Point) -> snapshot::Frame {
    let mut f = point_receipt(ms, p, None);
    f.selection_input.native_cancel = flag(true);
    f.selection_input.target_cancel_hit = flag(true);
    f.selection_input.target_valid = flag(false);
    f.cancel_state = snapshot::CancelState {
        ready: flag(true),
        coherent: flag(true),
        source: text(CANCEL_SOURCE),
        token: f.token,
        hero_id: f.hero_id,
        slot: text(SLOT),
        base_skill_id: n(BASE as f64),
        cancelled: flag(true),
        sample_start_ns: f.selection_input.sample_start_ns,
        sample_end_ns: f.time_ns,
    };
    f
}
fn send(c: &mut Controller, h: &mut Ledger, f: snapshot::Frame) {
    c.apply_snapshot(h, &f, f.time_ns.value as i64);
}
fn key(c: &mut Controller, h: &mut Ledger, ms: i64, code: i32, down: bool) {
    let aborts = h.abort_all_calls;
    assert!(c.apply_input(
        h,
        Input::Key {
            code,
            down,
            repeat: false
        },
        T + ms * MS
    ));
    if code == 84 && down {
        assert_eq!(h.abort_all_calls, aborts, "individual T must never cancel unrelated contacts");
    }
}
fn pointer(c: &mut Controller, h: &mut Ledger, ms: i64, p: Point) {
    c.apply_input(h, Input::Pointer(p), T + ms * MS);
    c.apply_input(h, Input::Timer(Timer::Pointer), T + (ms + 1) * MS);
}
fn tick(c: &mut Controller, h: &mut Ledger, ms: i64) {
    c.apply_input(h, Input::Timer(Timer::Main), T + ms * MS);
}
fn moved(h: &Ledger, id: i32) -> usize {
    h.moves.iter().filter(|(n, _)| *n == id).count()
}
fn ended(h: &Ledger, id: i32) -> bool {
    h.ended.iter().any(|(n, _)| *n == id)
}
fn traced(h: &Ledger, reason: &str) -> bool {
    h.traces.iter().any(|(_, r)| r == reason)
}
fn setup(with_movement: bool, p: Point) -> (Controller, Ledger) {
    let mut c = Controller::new();
    let mut h = Ledger {
        next: 40,
        ..Default::default()
    };
    c.apply_input(&mut h, Input::Trace(true), T);
    c.apply_input(&mut h, Input::Enabled(true), T);
    c.apply_input(&mut h, Input::HoldThreshold(40), T);
    send(&mut c, &mut h, frame(0));
    if with_movement {
        c.apply_input(
            &mut h,
            Input::Mouse {
                button: 2,
                down: true,
                point: Point::new(0.6, 0.5),
            },
            T + MS,
        );
        send(&mut c, &mut h, frame(50));
        assert!(c.movementTouch_ >= 0);
    }
    pointer(&mut c, &mut h, 55, p);
    (c, h)
}
fn started(with_movement: bool) -> (Controller, Ledger, i32) {
    let (mut c, mut h) = setup(with_movement, Point::new(0.73, 0.36));
    key(&mut c, &mut h, 60, 69, true);
    let id = c.skillTouch_;
    assert!(id >= 0);
    send(&mut c, &mut h, held(70));
    assert!(
        moved(&h, id) > 0,
        "owned plane must bootstrap before an input ACK"
    );
    (c, h, id)
}
fn learned() -> (Controller, Ledger, i32) {
    let (mut c, mut h, id) = started(true);
    let p = h.active[&id];
    send(&mut c, &mut h, point_receipt(75, p, Some(13016)));
    assert!(!ended(&h, id));
    assert_eq!(c.selection_.contact_owner, CONTACT);
    // UP below must await a new physical intent, not reuse the fresh75ms ACK.
    pointer(&mut c, &mut h, 77, Point::new(0.62, 0.46));
    (c, h, id)
}

#[test]
fn absolute_target_bootstrap_ignores_hero_wheel_and_world_projection_with_native_zero() {
    for hero in [Point::new(0.05, 0.95), Point::new(0.91, 0.02)] {
        let (mut c, mut h) = setup(false, Point::new(0.73, 0.36));
        let before_world = h.world_reads;
        key(&mut c, &mut h, 60, 69, true);
        let id = c.skillTouch_;
        assert!(id >= 0);
        assert_ne!(id, -7);
        let mut f = held(70);
        f.hero_screen_uv = v2(hero.x, hero.y);
        f.ui.joystick = Default::default();
        f.active_joystick = Default::default();
        f.direction_input = Default::default();
        send(&mut c, &mut h, f);
        let p = h.active[&id];
        let expected = Point::new(
            ((0.73 * W * W / (W - 1.)).round() + 0.5) / W,
            ((0.36 * H * H / (H - 1.)).round() + 0.5) / H,
        );
        assert_eq!(p, expected);
        assert!(
            h.world_kinds[before_world..]
                .iter()
                .all(|k| *k == "hero_world"),
            "Target must not request world projection: {:?}",
            &h.world_kinds[before_world..]
        );
        assert_eq!(c.coreSkill_, 0);
        assert!(!ended(&h, id));
    }
}
#[test]
fn physical_up_freezes_absolute_point_then_exact_owned_ack_commits_without_touching_movement() {
    let (mut c, mut h, id) = learned();
    let movement = c.movementTouch_;
    assert_ne!(id, -7);
    assert!(OWNER > (1 << 53));
    key(&mut c, &mut h, 80, 69, false);
    let goal = h.active[&id];
    assert!(!ended(&h, id));
    pointer(&mut c, &mut h, 81, Point::new(0.1, 0.9));
    assert_eq!(h.active[&id], goal);
    let mut f = point_receipt(90, goal, Some(13016));
    f.ui.active_slot = text("skill:0");
    f.skill_id = n(5510.);
    f.selection_input.native_skill_id = f.skill_id;
    send(&mut c, &mut h, f);
    assert!(ended(&h, id));
    assert!(h.active.contains_key(&movement));
    assert!(traced(&h, "selection_up_after_ui_ack"));
}
#[test]
fn unknown_or_stale_target_receipts_cannot_commit_after_up() {
    for case in 0..10 {
        let (mut c, mut h, id) = learned();
        key(&mut c, &mut h, 80, 69, false);
        let goal = h.active[&id];
        let mut f = point_receipt(90, goal, Some(13016));
        match case {
            0 => f.selection_input.target_valid = Default::default(),
            1 => f.selection_input.input_ready = flag(false),
            2 => f.selection_input.sample_start_ns = n((T + 70 * MS) as f64),
            3 => f.selection_input.source = text("foreign target producer"),
            4 => f.selection_input.slot = text("skill:0"),
            5 => f.selection_input.token = text("other-match"),
            6 => f.selection_input.target_owner_id = 0,
            7 => f.selection_input.target_self_id = 0,
            8 => f.selection_input.target_guid = n(13016.5),
            9 => f.selection_input.contact_present = Default::default(),
            _ => unreachable!(),
        };
        send(&mut c, &mut h, f);
        assert!(!ended(&h, id), "case{case}");
        assert!(!c.skillCancelAcknowledged_, "case{case}");
    }
}
#[test]
fn previous_point_and_guid_are_not_a_receipt_for_the_latest_move() {
    let (mut c, mut h, id) = learned();
    let old = h.active[&id];
    pointer(&mut c, &mut h, 77, Point::new(0.42, 0.61));
    key(&mut c, &mut h, 80, 69, false);
    let goal = h.active[&id];
    assert_ne!(old, goal);
    send(&mut c, &mut h, point_receipt(90, old, Some(13016)));
    assert!(!ended(&h, id));
    send(&mut c, &mut h, point_receipt(95, goal, Some(13016)));
    assert!(ended(&h, id));
}
#[test]
fn revoked_or_unknown_target_owners_block_all_generated_echoes() {
    for case in 0..4 {
        let (mut c, mut h, id) = learned();
        key(&mut c, &mut h, 80, 69, false);
        let goal = h.active[&id];
        let mut f = point_receipt(90, goal, Some(13016));
        match case {
            0 => f.selection_input.owner_id = OWNER + 1,
            1 => f.selection_input.backend_owner_id = BACKEND + 1,
            2 => f.selection_input.contact_owner_id = CONTACT + 1,
            3 => f.selection_input.ready = Default::default(),
            _ => unreachable!(),
        };
        let count = moved(&h, id);
        send(&mut c, &mut h, f);
        tick(&mut c, &mut h, 130);
        c.apply_input(&mut h, Input::Timer(Timer::Pointer), T + 131 * MS);
        assert_eq!(moved(&h, id), count, "case{case}");
        assert!(!ended(&h, id));
    }
}
#[test]
fn native_end_needs_two_clocks_and_explicit_owned_registry_absence() {
    let (mut c, mut h, id) = learned();
    let mut empty = frame(90);
    empty.ui.active_slot = text("skill:0");
    empty.skill_id = n(5510.);
    empty.selection_input.native_skill_id = empty.skill_id;
    empty.selection_input.contact_present = Default::default();
    send(&mut c, &mut h, empty);
    assert!(!ended(&h, id));
    let mut first = frame(100);
    first.ui.active_slot = text("skill:0");
    first.skill_id = n(5510.);
    first.selection_input.native_skill_id = first.skill_id;
    first.game_clock_ms = n(40.);
    send(&mut c, &mut h, first);
    assert!(!ended(&h, id));
    let mut same = frame(110);
    same.game_clock_ms = n(40.);
    send(&mut c, &mut h, same);
    assert!(!ended(&h, id));
    let mut second = frame(120);
    second.game_clock_ms = n(41.);
    send(&mut c, &mut h, second);
    assert!(ended(&h, id));
    assert!(traced(&h, "auto_completed"));
}
#[test]
fn target_guid_accepts_full_u32_and_keeps_u64_owner_identity_exact() {
    let (mut c, mut h, id) = started(false);
    let p = h.active[&id];
    send(&mut c, &mut h, point_receipt(75, p, Some(u32::MAX - 1)));
    pointer(&mut c, &mut h, 77, Point::new(0.62, 0.46));
    key(&mut c, &mut h, 80, 69, false);
    let goal = h.active[&id];
    send(&mut c, &mut h, point_receipt(90, goal, Some(u32::MAX - 1)));
    assert!(ended(&h, id));
}

#[test]
fn physical_up_can_reuse_a_fresh_exact_target_ack_without_an_extra_move() {
    let (mut c, mut h, id) = started(false);
    let p = h.active[&id];
    send(&mut c, &mut h, point_receipt(75, p, Some(13016)));
    let moves = moved(&h, id);
    key(&mut c, &mut h, 80, 69, false);
    assert!(ended(&h, id));
    assert_eq!(moved(&h, id), moves);
    assert!(traced(&h, "reuse_exact_ui_choice_ack"));
}
#[test]
fn positively_invalid_recipient_after_up_waits_for_scoped_native_cancel_ack() {
    let (mut c, mut h, id) = learned();
    let movement = c.movementTouch_;
    key(&mut c, &mut h, 80, 69, false);
    let goal = h.active[&id];
    let mut invalid = point_receipt(90, goal, None);
    invalid.selection_input.target_valid = flag(false);
    send(&mut c, &mut h, invalid);
    assert!(c.skillCancelPending_);
    assert!(!ended(&h, id));
    assert!(traced(&h, "invalid_target_await_native_cancel"));
    let cancel = h.active[&id];
    assert_ne!(cancel, goal);
    send(&mut c, &mut h, cancel_receipt(100, cancel));
    assert!(ended(&h, id));
    assert!(traced(&h, "cancel"));
    assert!(!traced(&h, "selection_up_after_ui_ack"));
    assert!(h.active.contains_key(&movement));
}
#[test]
fn sticky_group_cancel_or_generic_ui_flag_cannot_authorize_target_up() {
    let (mut c, mut h, id) = learned();
    key(&mut c, &mut h, 80, 84, true);
    assert!(c.skillCancelPending_);
    let cancel = h.active[&id];
    let mut sticky = cancel_receipt(90, cancel);
    sticky.selection_input.target_cancel_hit = flag(false);
    send(&mut c, &mut h, sticky);
    assert!(!ended(&h, id));
    assert!(!c.skillCancelAcknowledged_);
    let mut generic = cancel_receipt(100, cancel);
    generic.cancel_state.source = text("UIKeySkill.bCancelSkill");
    generic.selection_input.target_cancel_hit = flag(false);
    generic.selection_input.native_cancel = flag(false);
    send(&mut c, &mut h, generic);
    assert!(!ended(&h, id));
    assert!(!c.skillCancelAcknowledged_);
    let mut owned = cancel_receipt(110, cancel);
    // The shared UI row may currently describe an unrelated Q. Target owns
    // its own native flag and point receipt, so that global row is irrelevant.
    owned.cancel_state.source = text("UIKeySkill.bCancelSkill");
    owned.cancel_state.token = text("foreign-global-row");
    owned.cancel_state.slot = text("skill:0");
    owned.cancel_state.base_skill_id = n(5510.);
    send(&mut c, &mut h, owned);
    assert!(ended(&h, id));
}
#[test]
fn native_target_cancel_still_requires_the_exact_latest_owned_point_and_scope() {
    for case in 0..6 {
        let (mut c, mut h, id) = learned();
        let previous = h.active[&id];
        key(&mut c, &mut h, 80, 84, true);
        let cancel = h.active[&id];
        let mut f = cancel_receipt(90, cancel);
        match case {
            0 => {
                let wrong = point_receipt(90, previous, None);
                f.selection_input.raw_touch_xyz = wrong.selection_input.raw_touch_xyz;
            }
            1 => {
                f.selection_input.sample_start_ns = n((T + 70 * MS) as f64);
                f.cancel_state.sample_start_ns = f.selection_input.sample_start_ns;
            }
            2 => f.selection_input.backend_owner_id = BACKEND + 1,
            3 => f.selection_input.contact_owner_id = CONTACT + 1,
            4 => f.selection_input.native_cancel = flag(false),
            5 => f.selection_input.token = text("foreign-match"),
            _ => unreachable!(),
        };
        send(&mut c, &mut h, f);
        assert!(!ended(&h, id), "case{case}");
        assert!(!c.skillCancelAcknowledged_, "case{case}");
    }
}
#[test]
fn target_identity_frozen_at_up_cannot_be_replaced_by_another_valid_guid() {
    let (mut c, mut h, id) = started(false);
    let p = h.active[&id];
    send(&mut c, &mut h, point_receipt(75, p, Some(13016)));
    // A fresh owner/plane with temporarily unknown recipient forbids ACK reuse,
    // but must retain the last valid target identity at this unchanged point.
    send(&mut c, &mut h, held(78));
    key(&mut c, &mut h, 80, 69, false);
    assert!(!ended(&h, id));
    let frozen = c.inspect();
    assert_eq!(frozen.selectionGoalTargetGuid_, 13016);
    assert_eq!(frozen.selectionGoalTargetOwner_, TARGET);
    assert_eq!(frozen.selectionGoalTargetSelf_, SELF);
    let goal = h.active[&id];
    let mut changed = point_receipt(90, goal, Some(13017));
    changed.selection_input.target_owner_id = TARGET + 1;
    changed.selection_input.target_self_id = SELF + 1;
    send(&mut c, &mut h, changed);
    assert!(c.skillCancelPending_);
    assert!(!ended(&h, id));
    assert!(traced(&h, "target_identity_changed_after_up"));
    let rejected = c.inspect();
    assert_eq!(rejected.selectionGoalTargetGuid_, 13016);
    assert_eq!(rejected.selectionTargetGuid_, 13017);
}
#[test]
fn empty_ui_or_callback_state_with_a_live_owned_contact_is_not_native_end() {
    let (mut c, mut h, id) = learned();
    for ms in [90, 100, 110] {
        let mut f = frame(ms);
        f.selection_input.contact_present = flag(true);
        f.selection_input.contact_owner_id = CONTACT;
        f.selection_input.native_contact_id = n(-7.);
        f.selection_input.panel_visible = flag(false);
        f.selection_input.has_drag = flag(false);
        f.selection_input.backend_state = n(3.);
        send(&mut c, &mut h, f);
        assert!(!ended(&h, id));
    }
}

#[test]
fn generic_cancel_geometry_cannot_move_target_without_fresh_packet_local_ownership() {
    for case in 0..7 {
        let (mut c, mut h, id) = started(false);
        let mut f = held(75);
        match case {
            0 => f.selection_input.target_cancel_ready = Default::default(),
            1 => f.selection_input.target_cancel_ready = flag(false),
            2 => f.selection_input.target_cancel_uv = Default::default(),
            3 => f.selection_input.owner_id = OWNER + 1,
            4 => f.selection_input.backend_owner_id = BACKEND + 1,
            5 => f.selection_input.ready = Default::default(),
            6 => { f.selection_input.panel_visible = flag(false);
                f.selection_input.backend_state = n(3.); }
            _ => unreachable!(),
        }
        send(&mut c, &mut h, f);
        let before = h.active[&id];
        let count = moved(&h, id);
        key(&mut c, &mut h, 80, 84, true);
        tick(&mut c, &mut h, 90);
        assert!(c.skillCancelPending_, "cancel scope case{case}");
        assert_eq!(c.skillCancelSentNs_, 0, "cancel scope case{case}");
        assert_eq!(h.active[&id], before, "shared global cancel geometry moved Target, case{case}");
        assert_eq!(moved(&h, id), count, "cancel scope case{case}");
        assert!(!ended(&h, id));
        assert_eq!(h.abort_all_calls, 0);
    }
}

#[test]
fn owned_packet_cancel_geometry_bootstraps_before_contact_ack_and_ignores_global_geometry() {
    let (mut c, mut h, id) = started(false);
    assert_eq!(c.selection_.contact_owner, 0);
    assert!(!c.selection_.accepted);
    key(&mut c, &mut h, 80, 84, true);
    let cancel = h.active[&id];
    assert_eq!(cancel, Point::new(0.88, 0.27));
    assert_ne!(cancel, Point::new(0.94, 0.18), "global UI cancel must not be substituted");
    assert!(c.skillCancelSentNs_ > 0 && !ended(&h, id));
    let mut f = cancel_receipt(90, cancel);
    f.cancel_state = Default::default();
    send(&mut c, &mut h, f);
    assert!(ended(&h, id));
    assert!(traced(&h, "owned_target_cancel_flag_and_exact_native_touch"));
    assert_eq!(h.abort_all_calls, 0);
}

#[test]
fn unknown_selection_family_waits_for_owned_packet_before_any_cancel_move() {
    let (mut c, mut h) = setup(false, Point::new(0.73, 0.36));
    let mut f = frame(58);
    f.selection_input = Default::default();
    send(&mut c, &mut h, f);
    key(&mut c, &mut h, 60, 69, true);
    let id = c.skillTouch_;
    let point = h.active[&id];
    assert_eq!(c.selection_.kind, 0);
    key(&mut c, &mut h, 65, 84, true);
    assert_eq!(h.active[&id], point);
    assert_eq!(c.skillCancelSentNs_, 0);
    send(&mut c, &mut h, held(70));
    assert_eq!(c.selection_.kind, 2);
    assert_eq!(h.active[&id], Point::new(0.88, 0.27));
    assert!(c.skillCancelSentNs_ > 0 && !ended(&h, id));
    assert_eq!(h.abort_all_calls, 0);
}

#[test]
fn selection_transport_failure_retains_the_finger_until_owned_cancel_move_and_ack_recover() {
    for case in 0..5 {
        let (mut c, mut h, id) = learned();
        let movement = c.movementTouch_;
        match case {
            0 => {
                h.fail_move_id = Some(id);
                pointer(&mut c, &mut h, 79, Point::new(0.4, 0.6));
                assert!(traced(&h, "selection_pointer_move_delivery_failed"));
            }
            1 => {
                h.fail_move_id = Some(id);
                key(&mut c, &mut h, 80, 84, true);
                assert!(traced(&h, "selection_cancel_move_delivery_failed"));
            }
            2 => {
                let mut unknown = held(79);
                unknown.selection_input.target_cancel_ready = Default::default();
                send(&mut c, &mut h, unknown);
                key(&mut c, &mut h, 80, 84, true);
                assert_eq!(c.skillCancelSentNs_, 0);
                h.fail_move_id = Some(id);
                send(&mut c, &mut h, held(90));
                assert!(traced(&h, "selection_cancel_retry_delivery_failed"));
            }
            3 => {
                key(&mut c, &mut h, 80, 69, false);
                h.fail_move_id = Some(id);
                tick(&mut c, &mut h, 130);
                assert!(traced(&h, "selection_keepalive_delivery_failed"));
            }
            4 => {
                h.fail_move_id = Some(id);
                c.apply_input(&mut h, Input::Timer(Timer::Pointer), T + 79 * MS);
                assert!(traced(&h, "selection_pointer_echo_delivery_failed"));
            }
            _ => unreachable!(),
        }
        assert!(c.skillCancelPending_ && !ended(&h, id), "failure path{case}");
        assert_eq!(c.skillCancelSentNs_, 0, "failure path{case}");
        assert!(h.active.contains_key(&movement));
        assert_eq!(h.abort_all_calls, 0);
        h.fail_move_id = None;
        send(&mut c, &mut h, held(140));
        let cancel = h.active[&id];
        assert_eq!(cancel, Point::new(0.88, 0.27));
        assert!(c.skillCancelSentNs_ > 0 && !ended(&h, id));
        send(&mut c, &mut h, cancel_receipt(150, cancel));
        assert!(ended(&h, id), "recovered cancellation path{case}");
        assert!(h.active.contains_key(&movement));
        assert!(!traced(&h, "selection_up_after_ui_ack"));
    }
}

#[test]
fn cancelling_at_the_already_delivered_point_sends_a_new_causal_move_for_native_ack() {
    let (mut c, mut h, id) = learned();
    let point = h.active[&id];
    let mut f = held(79);
    f.selection_input.target_cancel_uv = v2(point.x, point.y);
    send(&mut c, &mut h, f);
    let count = moved(&h, id);
    key(&mut c, &mut h, 80, 84, true);
    assert_eq!(h.active[&id], point);
    assert!(moved(&h, id) > count, "same-point cancel needs a real post-request MOVE");
    assert!(!ended(&h, id));
    send(&mut c, &mut h, cancel_receipt(90, point));
    assert!(ended(&h, id));
    assert!(traced(&h, "owned_target_cancel_flag_and_exact_native_touch"));
    assert_eq!(h.abort_all_calls, 0);
}
