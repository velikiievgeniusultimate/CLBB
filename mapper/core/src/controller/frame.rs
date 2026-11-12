use super::*;
pub(crate) fn uv(v: &snapshot::Vector2, radius: bool) -> Option<Point> {
    if !v.valid() {
        return None;
    }
    let p = Point::new(v.values[0].value, v.values[1].value);
    let lower = if radius { 0.0001 } else { 0. };
    if p.x < lower || p.y < lower || p.x > 1. || p.y > 1. {
        None
    } else {
        Some(p)
    }
}
impl Controller {
    pub(crate) fn ingest_frame(&mut self, host: &mut impl Host, f: &snapshot::Frame) {
        let ui = &f.ui;
        let hero = f.hero_id.int();
        let now = self.now_ as f64;
        let age = (now - f.time_ns.get(-1.)) / 1e6;
        let ui_age = (now - ui.time_ns.get(-1.)) / 1e6;
        if !f.ready.get()
            || !ui.ready.get()
            || hero <= 0
            || hero > 10000
            || !f.hero_id.is_number()
            || f.hero_id.value != hero as f64
            || !ui.hero_id.is_number()
            || ui.hero_id.value != hero as f64
            || !f.time_ns.is_number()
            || !ui.time_ns.is_number()
            || !age.is_finite()
            || !ui_age.is_finite()
            || age < -10.
            || age > 250.
            || ui_age < -10.
            || ui_age > 250.
        {
            self.invalidate(host, "F7: ожидаю свежие данные матча и кнопок");
            return;
        }
        if hero != self.heroId_ {
            self.release_all(host);
            self.cooldownSeen_ = false;
            self.cooldownState_ = snapshot::CooldownState::default();
            self.cooldownCasts_.clear();
            self.responseModels_.clear();
            self.directStartups_.clear();
            self.directRanges_.clear();
            self.heroId_ = hero;
            self.cancelKey_ = host.cancel_key(hero);
            self.refresh_bindings(host)
        }
        let epoch = f.token.owned();
        if epoch != self.frameEpoch_ {
            if !self.frameEpoch_.is_empty()
                && self.frameEpoch_ != "null"
                && self.frameEpoch_ != "0"
                && self.frameEpoch_ != "\"\""
            {
                self.release_all(host);
                self.logicalMode_ = false;
                self.logicalSampleFresh_ = false;
                self.directGeometryFresh_ = false;
                self.logicalSource_.clear();
                self.logicalSampleNs_ = 0;
                self.direct_.reset()
            }
            self.directStartups_.clear();
            self.directRanges_.clear();
            self.frameEpoch_ = epoch;
            self.cooldownSeen_ = false;
            self.cooldownState_ = snapshot::CooldownState::default();
            self.cooldownCasts_.clear();
        }
        if ui.cooldown_state.present.get() {
            self.cooldownSeen_ = true
        }
        self.cooldownState_ = ui.cooldown_state;
        self.frameCameraReady_ = false;
        if f.view_matrix.valid() && f.hero_render_xyz.valid() {
            let v = &f.view_matrix.values;
            self.frameCamera_ = [v[0].value, -v[1].value, v[8].value, -v[9].value];
            self.frameMovementCamera_ = [v[0].value, v[2].value, v[8].value, v[10].value];
            self.frameHero_ = Point::new(
                f.hero_render_xyz.values[0].value,
                f.hero_render_xyz.values[2].value,
            );
            self.frameCameraReady_ = true;
        }
        if ui.resolution_px.count == 2 {
            self.set_android_size(
                host,
                Point::new(
                    ui.resolution_px.values[0].get(0.),
                    ui.resolution_px.values[1].get(0.),
                ),
            )
        }
        self.ingest_skill_variants(f);
        self.observe_kagura_throw(host);
        let mut fresh = Vec::new();
        for entry in ui.buttons.iter().take(ui.button_count.min(32) as usize) {
            let mut b = Button {
                slot: entry.slot.owned(),
                kind: entry.kind.owned(),
                index: entry.index.int(),
                skillId: entry.skill_id.int(),
                geometrySource: entry.geometry_source.owned(),
                available: entry.active.get(),
                ..Button::default()
            };
            if b.slot.is_empty() || b.slot.len() > 64 || (!b.available && !Self::tap_only(&b)) {
                continue;
            }
            let (Some(press), Some(center), Some(radius)) = (
                uv(&entry.press_uv, false),
                uv(&entry.center_uv, false),
                uv(&entry.radius_uv, true),
            ) else {
                continue;
            };
            if fresh.iter().any(|other: &Button| other.slot == b.slot) {
                continue;
            }
            b.press = press;
            b.center = center;
            b.radius = radius;
            let config = self.current_config_slot(&b);
            let binding = host.binding(self.heroId_, &b, &config);
            b.key = binding.key;
            b.castMode = binding.cast_mode;
            b.aimType = binding.aim_type;
            b.stopMovement = binding.stop_movement;
            fresh.push(b);
            if fresh.len() >= 32 {
                break;
            }
        }
        for e in ui
            .capability_controls
            .iter()
            .take(ui.capability_count.min(32) as usize)
        {
            let mut b = Button {
                slot: e.slot.owned(),
                kind: e.kind.owned(),
                index: e.index.int(),
                available: false,
                ..Button::default()
            };
            if !Self::tap_only(&b)
                || (b.kind != "upgrade" && b.kind != "quick_buy" && b.kind != "attack")
                || fresh.iter().any(|other| other.slot == b.slot)
                || fresh.len() >= 32
            {
                continue;
            }
            let binding = host.binding(self.heroId_, &b, &b.slot);
            b.key = binding.key;
            b.castMode = binding.cast_mode;
            b.aimType = binding.aim_type;
            b.stopMovement = binding.stop_movement;
            fresh.push(b);
        }
        if fresh.is_empty() || self.androidSize_.x <= 0. || self.androidSize_.y <= 0. {
            self.invalidate(host, "F7: геометрия кнопок ещё не готова");
            return;
        }
        if self.skillTouch_ >= 0 {
            let old = self.find_button(&self.activeSlot_);
            let valid = fresh.iter().any(|b| {
                b.slot == self.activeSlot_ && old.is_some_and(|old| old.skillId == b.skillId)
            });
            if !valid {
                self.finish_skill(host, true)
            }
        }
        self.buttons_ = fresh;
        self.changed_ = true;
        self.frameActiveSlot_ = ui.active_slot.owned();
        self.joystickReady_ = false;
        if ui.joystick.ready.get() {
            if let (Some(c), Some(r)) = (
                uv(&ui.joystick.center_uv, false),
                uv(&ui.joystick.radius_uv, true),
            ) {
                self.joystickCenter_ = c;
                self.joystickRadius_ = r;
                self.joystickReady_ = true
            }
        }
        self.joystickMeasured_ =
            self.joystickReady_ && ui.joystick.source.as_str() == "active_widget_transform";
        self.cancelReady_ = false;
        if let Some(p) = uv(&ui.cancel_uv, false) {
            self.cancelReady_ = true;
            self.cancelPoint_ = p
        }
        self.movementReady_ = false;
        if ui.movement.ready.get() {
            if let (Some(p), Some(c), Some(r)) = (
                uv(&ui.movement.press_uv, false),
                uv(&ui.movement.center_uv, false),
                uv(&ui.movement.radius_uv, true),
            ) {
                self.movementReady_ = true;
                self.movementPress_ = p;
                self.movementCenter_ = c;
                self.movementRadius_ = r
            }
        }
        // A fresh match frame can temporarily lack only joystick geometry
        // while a skill dashes/hides/reparents that widget. It is not a physical
        // RMB release: preserve the held finger and resume on the next valid
        // geometry. Epoch/focus/explicit stop paths still release normally.
        if let Some(p) = uv(&f.hero_screen_uv, false) {
            self.heroScreen_ = p
        }
        self.ready_ = true;
        self.lastFrameNs_ = self.now_;
        self.set_status(format!(
            "F7: герой {} · {} кнопок · {}",
            self.heroId_,
            self.buttons_.len(),
            if self.skillTouch_ >= 0 {
                "наведение"
            } else {
                "готово"
            }
        ));
        self.coreSkill_ = f.skill_id.int();
        let d = f.sample_duration_ms.get(-1.);
        let start = f.time_ns.get(-1.) - d * 1e6;
        self.frameCoreSampleStartNs_ =
            if f.sample_duration_ms.is_number() && d.is_finite() && d >= 0. && d <= 100. {
                start as i64
            } else {
                0
            };
        // UI ownership is acceptance even when an animation/buffered cast has
        // not produced a logical target yet. Never infer rejection later merely
        // because that indicator disappears or becomes temporarily forbidden.
        if self.skillTouch_ >= 0 && self.frameCoreSampleStartNs_ >= self.castStartedNs_ {
            let matching_skill = self.coreSkill_ > 0 && self.find_button(&self.activeSlot_)
                .is_some_and(|b| self.coreSkill_ == b.skillId
                    || self.coreSkill_ == self.skillCooldownVariant_);
            if self.frameActiveSlot_ == self.activeSlot_ || matching_skill {
                self.skillActivationObserved_ = true;
            }
        }
        if self.skillTouch_ >= 0
            && !self.skillCancelPending_
            && self.startupArmed_
            && f.sample_duration_ms.is_number()
            && d.is_finite()
            && d >= 0.
            && d <= 100.
            && start >= self.castStartedNs_ as f64
        {
            if self.coreSkill_ == 0 {
                self.startupMayActivate_ = true
            } else if self.startupMayActivate_ {
                if let Some(b) = self.find_button(&self.activeSlot_).cloned() {
                    self.predict_startup(host, &b, self.coreSkill_)
                }
            }
        }
        self.directionGeometryFresh_ = false;
        let g = &f.active_joystick;
        let gs = g.sample_start_ns.get(-1.);
        let ge = g.sample_end_ns.get(-1.);
        let expected = self
            .find_button(&self.activeSlot_)
            .map(|b| self.measured_direction_skill_id(b))
            .unwrap_or(0);
        if self.skillTouch_ >= 0
            && g.ready.get()
            && g.source.as_str() == "active_widget_transform"
            && g.slot.as_str() == self.activeSlot_
            && g.skill_id.int() == self.coreSkill_
            && expected > 0
            && self.coreSkill_ == expected
            && g.sample_start_ns.is_number()
            && g.sample_end_ns.is_number()
            && gs.is_finite()
            && ge.is_finite()
            && gs >= self.castStartedNs_ as f64
            && ge >= gs
            && ge - gs <= 100e6
            && now - ge <= 100e6
            && ge <= now + 10e6
        {
            if let (Some(c), Some(r)) = (uv(&g.center_uv, false), uv(&g.radius_uv, true)) {
                self.directionCenter_ = c;
                self.directionRadius_ = r;
                self.directionGeometryFresh_ = true
            }
        }
        self.select_logical_coordinates(host, f);
        self.configure_direct(host, f);
        self.update_range_drag_arming(host, f);
        self.sample_accepted_input(f, host);
        self.sample_direction_input(host, f);
        if self.acceptedInput_ || self.directionAccepted_ {
            self.skillActivationObserved_ = true;
        }
        self.sample_skill_cancel(host, f);
        self.tick(host);
    }
}

#[cfg(test)]
mod movement_continuity_tests {
    use super::super::host::{Binding, Timer};
    use super::*;
    #[derive(Default)]
    struct TouchHost {
        next: i32,
        active: std::collections::BTreeMap<i32, Point>,
        ended: Vec<i32>,
        moved: Vec<i32>,
        stop_on_w: bool,
    }
    impl Host for TouchHost {
        fn begin(&mut self, p: Point) -> i32 {
            let id = self.next;
            self.next += 1;
            self.active.insert(id, p);
            id
        }
        fn move_touch(&mut self, id: i32, p: Point) -> bool {
            if let Some(old) = self.active.get_mut(&id) {
                *old = p;
                self.moved.push(id);
                true
            } else {
                false
            }
        }
        fn end(&mut self, id: i32, _: Point) {
            self.active.remove(&id);
            self.ended.push(id);
        }
        fn pointer_world(&mut self, _: Point) -> Option<Point> {
            None
        }
        fn ground_world(&mut self, p: Point) -> Option<Point> {
            Some(Point::new(p.x * 10., p.y * 10.))
        }
        fn movement_direction(&mut self, _: Point) -> Option<Point> {
            None
        }
        fn hero_world(&mut self) -> Option<Point> {
            Some(Point::default())
        }
        fn pointer_world_at_plane(&mut self, _: Point, _: f64) -> Option<Point> {
            None
        }
        fn capture_pointer_world(&mut self, _: Point) -> u64 {
            0
        }
        fn resolve_world(&mut self, _: u64) -> Option<Point> {
            None
        }
        fn release_resolver(&mut self, _: u64) {}
        fn navigation(&mut self, _: Point, p: Point, avoid: bool) -> Result<Vec<Point>, String> {
            assert!(avoid, "direct movement must not query navigation");
            Ok(vec![p])
        }
        fn binding(&mut self, _: i32, _: &Button, slot: &str) -> Binding {
            Binding {
                key: 87,
                cast_mode: 0,
                aim_type: 0,
                stop_movement: slot == "skill:1" && self.stop_on_w,
            }
        }
        fn cancel_key(&mut self, _: i32) -> i32 {
            84
        }
        fn timer(&mut self, _: Timer, _: Option<i64>) {}
        fn trace(&mut self, _: &Controller, _: &str, _: &str) {}
    }
    fn n(v: f64) -> snapshot::Number {
        snapshot::Number { value: v, kind: 1 }
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
            values: [n(x), n(y)],
            count: 2,
        }
    }
    fn fresh(now: i64, movement: bool) -> snapshot::Frame {
        let mut f = snapshot::Frame {
            ready: flag(true),
            hero_id: n(25.),
            time_ns: n(now as f64),
            sample_duration_ms: n(0.5),
            token: text("same-match"),
            token_nonempty: flag(true),
            token_string: flag(true),
            ..Default::default()
        };
        f.ui.ready = flag(true);
        f.ui.hero_id = f.hero_id;
        f.ui.time_ns = f.time_ns;
        f.ui.resolution_px = v2(1000., 1000.);
        f.view_matrix = snapshot::Vector16 {
            values: std::array::from_fn(|i| n(if i % 5 == 0 { 1. } else { 0. })),
            count: 16,
        };
        f.hero_render_xyz = snapshot::Vector3 {
            values: [n(0.); 3],
            count: 3,
        };
        f.hero_screen_uv = v2(0.5, 0.5);
        f.ui.button_count = 1;
        f.ui.buttons[0] = snapshot::Button {
            slot: text("skill:1"),
            kind: text("skill"),
            index: n(1.),
            skill_id: n(2520.),
            active: flag(true),
            press_uv: v2(0.8, 0.7),
            center_uv: v2(0.8, 0.7),
            radius_uv: v2(0.1, 0.1),
            ..Default::default()
        };
        f.skill_state = snapshot::VariantState {
            ready: flag(true),
            source: text("ShowSkillData.m_TranID"),
            token: f.token,
            hero_id: f.hero_id,
            sample_start_ns: n(now as f64 - 500_000.),
            sample_end_ns: f.time_ns,
            count: 1,
            ..Default::default()
        };
        f.skill_state.buttons[0] = snapshot::Variant {
            slot: text("skill:1"),
            base_skill_id: n(2520.),
            active_skill_id: n(2520.),
            ..Default::default()
        };
        if movement {
            f.ui.movement = snapshot::MovementGeometry {
                ready: flag(true),
                press_uv: v2(0.15, 0.85),
                center_uv: v2(0.15, 0.85),
                radius_uv: v2(0.1, 0.1),
            };
        }
        f
    }
    fn controller() -> (Controller, TouchHost) {
        let mut c = Controller::new();
        let mut h = TouchHost::default();
        let press = 880_000_000;
        c.apply_input(&mut h, api::Input::Enabled(true), press);
        c.apply_snapshot(&mut h, &fresh(press, true), press);
        assert!(c.handle_mouse(&mut h, 2, true, Point::new(0.6, 0.5)));
        assert!(c.movementTouch_ >= 0);
        assert!(!c.movementHeld_ && c.movementAuto_);
        c.apply_snapshot(&mut h, &fresh(1_000_000_000, true), 1_000_000_000);
        assert!(c.movementTouch_ >= 0);
        (c, h)
    }
    #[test]
    fn movement_click_does_not_follow_cursor_until_hold_threshold() {
        let mut c = Controller::new();
        let mut h = TouchHost::default();
        let start = 1_000_000_000;
        c.apply_input(&mut h, api::Input::Enabled(true), start);
        c.apply_input(&mut h, api::Input::HoldThreshold(300), start);
        c.apply_snapshot(&mut h, &fresh(start, true), start);
        assert!(c.handle_mouse(&mut h, 2, true, Point::new(0.6, 0.5)));
        let touch = c.movementTouch_;
        let first_point = c.movementTouchPoint_;
        assert!(h.active.contains_key(&touch));
        assert_ne!(first_point, c.movementPress_);
        assert_eq!(c.movementTarget_, Point::new(6., 5.));
        let before_threshold = start + 299_000_000;
        c.apply_snapshot(&mut h, &fresh(before_threshold, true), before_threshold);
        assert!(c.movementPressed_ && !c.movementHeld_);
        assert!(c.handle_mouse(&mut h, 2, false, Point::new(0.7, 0.5)));
        assert!(c.movementAuto_ && !c.movementHeld_);
        assert_eq!(c.movementTarget_, Point::new(6., 5.));
        assert_eq!(c.movementTouchPoint_, first_point);
        assert_eq!(c.movementTouch_, touch);

        assert!(c.handle_mouse(&mut h, 2, true, Point::new(0.5, 0.7)));
        let second_point = c.movementTouchPoint_;
        c.update_pointer(&mut h, Point::new(0.7, 0.5));
        c.flush_pointer(&mut h);
        assert_eq!(c.movementTouchPoint_, second_point);
        let deadline = before_threshold + 300_000_000;
        c.apply_snapshot(&mut h, &fresh(deadline, true), deadline);
        assert!(c.movementHeld_ && !c.movementAuto_);
        assert_eq!(c.movementTouch_, touch);
        assert_ne!(c.movementTouchPoint_, second_point);
        assert!(c.handle_mouse(&mut h, 2, false, Point::new(0.5, 0.7)));
        assert!(h.active.is_empty());
        assert!(!c.movementPressed_ && !c.movementHeld_ && !c.movementAuto_);
    }
    #[test]
    fn direct_click_scales_from_hero_and_avoidance_uses_cursor() {
        let mut c = Controller::new();
        let mut h = TouchHost::default();
        let now = 1_000_000_000;
        c.apply_input(&mut h, api::Input::Enabled(true), now);
        c.apply_snapshot(&mut h, &fresh(now, true), now);
        c.frameHero_ = Point::new(2., 3.);
        let cursor = Point::new(0.6, 0.5);

        assert!(c.handle_mouse(&mut h, 2, true, cursor));
        assert_eq!(c.movementTarget_, Point::new(6., 5.));
        c.handle_mouse(&mut h, 2, false, cursor);
        for (percent, expected) in [(50, Point::new(4., 4.)), (200, Point::new(10., 7.))] {
            c.apply_input(&mut h, api::Input::MovementClickPercent(percent), now);
            assert!(c.handle_mouse(&mut h, 2, true, cursor));
            assert_eq!(c.movementTarget_, expected);
            c.handle_mouse(&mut h, 2, false, cursor);
        }

        c.apply_input(&mut h, api::Input::MovementMode(1), now);
        c.set_navigation_state(&mut h, true, String::new());
        assert!(c.handle_mouse(&mut h, 2, true, cursor));
        assert_eq!(c.movementTarget_, Point::new(6., 5.));
    }
    #[test]
    fn direct_click_keeps_joystick_active_when_progress_stalls() {
        let mut c = Controller::new();
        let mut h = TouchHost::default();
        let now = 1_000_000_000;
        let cursor = Point::new(0.6, 0.5);
        c.apply_input(&mut h, api::Input::Enabled(true), now);
        c.apply_snapshot(&mut h, &fresh(now, true), now);
        c.set_navigation_state(&mut h, true, String::new());
        assert!(c.handle_mouse(&mut h, 2, true, cursor));
        let touch = c.movementTouch_;
        assert!(h.active.contains_key(&touch));
        c.handle_mouse(&mut h, 2, false, cursor);
        for elapsed in [1_000_000_000, 16_000_000_000] {
            let time = now + elapsed;
            c.apply_snapshot(&mut h, &fresh(time, true), time);
            c.update_movement(&mut h, true);
            assert_eq!(c.movementTouch_, touch);
            assert!(h.active.contains_key(&touch));
        }
        c.frameHero_ = c.movementTarget_;
        c.update_movement(&mut h, true);
        assert!(h.active.is_empty());
    }
    #[test]
    fn retribution_modes_distinguish_aim_and_physical_release() {
        let start = 1_000_000_000;
        for (mode, behavior, automatic) in [
            (0, SkillBehavior::Direction, false),
            (1, SkillBehavior::Direction, true),
            (2, SkillBehavior::Tap, true),
            (3, SkillBehavior::Tap, false),
        ] {
            let mut c = Controller::new();
            let mut h = TouchHost::default();
            c.apply_input(&mut h, api::Input::Enabled(true), start);
            let mut frame = fresh(start, true);
            frame.ui.buttons[0].slot = text("extra:2");
            frame.ui.buttons[0].kind = text("extra");
            frame.ui.buttons[0].skill_id = n(20150.);
            c.apply_snapshot(&mut h, &frame, start);
            let mut button = c.find_button("extra:2").unwrap().clone();
            button.castMode = mode;
            button.aimType = 2;
            assert!(c.begin_skill(&mut h, &button));
            assert_eq!(c.skillBehavior_, behavior);
            assert_eq!(c.pendingRelease_, automatic && behavior == SkillBehavior::Direction);

            if behavior == SkillBehavior::Tap {
                c.update_pointer(&mut h, Point::new(0.3, 0.4));
                c.flush_pointer(&mut h);
                assert!(h.moved.is_empty());
                c.now_ = start + 20_000_000;
                c.tick(&mut h);
                assert_eq!(h.active.is_empty(), automatic);
                if !automatic {
                    assert!(c.handle_key(&mut h, button.key, false, false));
                    assert!(h.active.is_empty());
                }
            }

            if h.active.is_empty() {
                assert!(c.begin_skill(&mut h, &button));
            }
            c.request_skill_cancel(&mut h);
            assert!(c.skillCancelPending_);
            assert!(c.handle_key(&mut h, button.key, false, false));
            assert!(!h.active.is_empty());
            c.skillCancelAcknowledged_ = true;
            c.tick(&mut h);
            assert!(h.active.is_empty());
        }
    }
    fn w_lock_frame(now: i64, form: i32, locked: bool) -> snapshot::Frame {
        let mut f = fresh(now, true);
        f.skill_state.buttons[0].active_skill_id = n(form as f64);
        f.skill_state.buttons[0].cast_locked = flag(locked);
        f
    }
    fn finish_kagura_q(c: &mut Controller, h: &mut TouchHost, form: i32, cancel: bool) {
        let w = c.find_button("skill:1").unwrap().clone();
        let mut q = w;
        q.slot = "skill:0".into();
        q.skillId = 2510;
        assert!(c.begin_skill(h, &q));
        c.skillCooldownVariant_ = form;
        c.finish_skill(h, cancel);
    }
    #[test]
    fn kagura_throw_buffers_w_through_fresh_lock_cycle_with_movement_held() {
        let (mut c, mut h) = controller();
        let movement = c.movementTouch_;
        let start = 1_010_000_000;
        c.apply_snapshot(&mut h, &w_lock_frame(start, 2520, false), start);
        finish_kagura_q(&mut c, &mut h, 2510, false);
        let begun = h.next;
        let now = start + 10_000_000;
        c.apply_snapshot(&mut h, &w_lock_frame(now, 2520, false), now);
        assert!(c.handle_key(&mut h, 87, true, false));
        assert!(c.handle_key(&mut h, 87, false, false));
        assert_eq!(h.next, begun);
        assert_eq!(c.queuedSkills_.len(), 1);
        let now = start + 55_000_000;
        c.apply_snapshot(&mut h, &w_lock_frame(now, 2520, true), now);
        assert_eq!(h.next, begun);
        let now = start + 80_000_000;
        let mut stale = w_lock_frame(now, 2521, false);
        stale.skill_state.sample_start_ns = n((start - 1_000_000) as f64);
        c.apply_snapshot(&mut h, &stale, now);
        assert_eq!(h.next, begun);
        let now = start + 115_000_000;
        c.apply_snapshot(&mut h, &w_lock_frame(now, 2521, false), now);
        assert_eq!(h.next, begun + 1);
        assert_eq!(c.skillBehavior_, SkillBehavior::Tap);
        assert!(c.queuedSkills_.is_empty());
        let now = now + 20_000_000;
        c.apply_snapshot(&mut h, &w_lock_frame(now, 2521, false), now);
        assert_eq!(h.next, begun + 1);
        assert!(h.active.contains_key(&movement));
    }
    #[test]
    fn kagura_return_and_cancel_do_not_arm_w_and_failed_throw_has_a_bounded_wait() {
        let (mut c, mut h) = controller();
        let start = 1_010_000_000;
        c.apply_snapshot(&mut h, &w_lock_frame(start, 2521, false), start);
        finish_kagura_q(&mut c, &mut h, 2511, false);
        assert!(c.kaguraThrow_.is_none());
        finish_kagura_q(&mut c, &mut h, 2510, true);
        assert!(c.kaguraThrow_.is_none());
        finish_kagura_q(&mut c, &mut h, 2510, false);
        assert!(c.kaguraThrow_.is_some());
        assert!(c.handle_key(&mut h, 87, true, false));
        let begun = h.next;
        for elapsed in [100_000_000, 200_000_000] {
            let now = start + elapsed;
            c.apply_snapshot(&mut h, &w_lock_frame(now, 2520, false), now);
            assert_eq!(h.next, begun);
        }
        let now = start + 225_000_000;
        c.apply_snapshot(&mut h, &w_lock_frame(now, 2520, false), now);
        assert_eq!(h.next, begun + 1);
        assert!(c.queuedSkills_.is_empty());
        c.release_all(&mut h);
        assert!(c.kaguraThrow_.is_none());
    }
    #[test]
    fn held_rmb_survives_w_and_temporary_widget_loss_then_resumes() {
        let (mut c, mut h) = controller();
        let movement = c.movementTouch_;
        let w = c.find_button("skill:1").unwrap().clone();
        assert!(c.begin_skill(&mut h, &w));
        let skill = c.skillTouch_;
        assert_ne!(skill, movement);
        for step in 1..=3 {
            let now = 1_000_000_000 + step * 20_000_000;
            let mut f = fresh(now, false);
            f.skill_id = n(2520.);
            f.ui.active_slot = text("skill:1");
            c.apply_snapshot(&mut h, &f, now);
            let sent = h.moved.iter().filter(|id| **id == movement).count();
            c.flush_pointer(&mut h);
            assert_eq!(h.moved.iter().filter(|id| **id == movement).count(), sent);
            assert_eq!(c.movementTouch_, movement);
            assert_eq!(c.skillTouch_, skill);
            assert!(c.movementHeld_ && c.movementPressed_);
            assert!(h.ended.is_empty());
        }
        let now = 1_100_000_000;
        let mut f = fresh(now, true);
        f.skill_id = n(2520.);
        f.ui.active_slot = text("skill:1");
        f.ui.movement.center_uv = v2(0.17, 0.83);
        c.apply_snapshot(&mut h, &f, now);
        c.flush_pointer(&mut h);
        assert_eq!(c.movementTouch_, movement);
        assert!(h.active.contains_key(&movement));
        assert!(h.moved.contains(&movement));
        assert!(h.ended.is_empty());
        c.finish_skill(&mut h, false);
        assert!(h.active.contains_key(&movement));
        c.now_ = 1_300_000_000;
        assert!(c.handle_mouse(&mut h, 2, false, Point::new(0.6, 0.5)));
        assert_eq!(c.movementTouch_, -1);
        assert_eq!(h.ended.iter().filter(|id| **id == movement).count(), 1);
    }
    #[test]
    fn physical_up_is_not_lost_when_geometry_is_temporarily_unavailable() {
        let (mut c, mut h) = controller();
        let movement = c.movementTouch_;
        let now = 1_020_000_000;
        c.apply_snapshot(&mut h, &fresh(now, false), now);
        assert!(!c.movementReady_);
        assert_eq!(c.movementTouch_, movement);
        assert!(c.handle_mouse(&mut h, 2, false, Point::new(0.6, 0.5)));
        assert_eq!(c.movementTouch_, -1);
        assert!(!c.movementHeld_ && !c.movementAuto_);
        assert_eq!(h.ended, vec![movement]);
        c.apply_snapshot(&mut h, &fresh(1_040_000_000, true), 1_040_000_000);
        assert_eq!(c.movementTouch_, -1);
        assert!(!c.movementHeld_);
    }
    #[test]
    fn explicit_stop_setting_still_releases_movement_and_does_not_resume_it() {
        let (mut c, mut h) = controller();
        let movement = c.movementTouch_;
        h.stop_on_w = true;
        c.refresh_bindings(&mut h);
        let w = c.find_button("skill:1").unwrap().clone();
        assert!(c.begin_skill(&mut h, &w));
        assert_eq!(c.movementTouch_, -1);
        assert_eq!(h.ended, vec![movement]);
        assert!(!c.movementHeld_);
        c.apply_snapshot(&mut h, &fresh(1_030_000_000, true), 1_030_000_000);
        assert_eq!(c.movementTouch_, -1);
        assert!(!c.movementHeld_);
    }
    #[test]
    fn match_owner_change_revokes_held_touch_instead_of_reusing_old_geometry() {
        let (mut c, mut h) = controller();
        let movement = c.movementTouch_;
        let now = 1_020_000_000;
        let mut f = fresh(now, false);
        f.token = text("different-match");
        c.apply_snapshot(&mut h, &f, now);
        assert_eq!(c.movementTouch_, -1);
        assert!(h.ended.contains(&movement));
        assert!(!c.movementHeld_);
        assert!(!c.handle_mouse(&mut h, 2, true, Point::new(0.5, 0.5)));
    }
}
