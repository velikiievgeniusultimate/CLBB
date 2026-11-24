use super::{BATTLE_SPELLS, Host, Point, Runtime, now_ns};
use clbb_moba_core::controller::snapshot::Vector2;
use std::fmt::Write;

pub(super) struct Contact {
    finger: i32,
    point: Point,
    deadline: Option<i64>,
    released: bool,
}

impl Runtime {
    pub(super) fn release_points(&mut self) {
        for (_, contact) in std::mem::take(&mut self.point_contacts) {
            self.host.end(contact.finger, contact.point);
        }
    }

    pub(super) fn expire_points(&mut self, now: i64) {
        self.point_contacts.retain(|_, contact| {
            if contact.deadline.is_some_and(|at| now >= at) {
                self.host.end(contact.finger, contact.point);
                false
            } else {
                true
            }
        });
    }

    pub(super) fn point_event(&mut self, key: i32, down: bool, repeat: bool, active_match: bool) -> bool {
        if let Some(contact) = self.point_contacts.get_mut(&key) {
            if repeat || (down && !contact.released) { return true; }
            if !down && contact.deadline.is_some() {
                contact.released = true;
                return true;
            }
            let old = self.point_contacts.remove(&key).unwrap();
            self.host.end(old.finger, old.point);
            if !down { return true; }
        }
        if !active_match || key == 0 { return false; }
        let Some(binding) = self.host.settings.points.values().find(|p| p.key == key) else { return false };
        if !repeat && down {
            let point = Point::new(binding.x as f64 / 1e6, binding.y as f64 / 1e6);
            let instant = binding.instant;
            let finger = self.host.begin(point);
            if finger >= 0 {
                self.point_contacts.insert(key, Contact {
                    finger, point, released: false, deadline: instant.then(|| now_ns() + 40_000_000),
                });
            }
        }
        true
    }

    /// Rows carry the same setting identity used by the text palette. The
    /// panel may suspend input, but the observation worker continues sampling.
    pub(super) fn editor_layout(&self) -> String {
        let mut rows = String::new();
        let settings = &self.host.settings;
        let sample = self.host.fresh().filter(|s| s.frame.ready.get() && s.frame.ui.ready.get()
            && s.frame.hero_id.int() > 0 && s.frame.hero_id.int() == s.frame.ui.hero_id.int());
        let hero = sample.map_or(0, |s| s.frame.hero_id.int());
        let mut row = |slot: &str, path: String, key: i32, x: f64, y: f64| {
            if (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y) {
                let conflict = i32::from(self.host.conflict_blocks(key, hero));
                let _ = writeln!(rows, "{slot}\t{path}\t{key}\t{x:.6}\t{y:.6}\t{conflict}");
            }
        };
        if let Some(sample) = sample {
            let ui = &sample.frame.ui;
            let coords = |v: &Vector2| v.valid().then(|| v.values.map(|n| n.value));
            let personal = |field: &str| format!("profile.{hero}.hero.{field}");
            let mut action = |slot: &str, xy: [f64; 2]| {
                let field = format!("bindings.{slot}.key");
                let path = if slot == "match:statistics" { format!("common.{field}") } else { personal(&field) };
                row(slot, path, settings.binding(hero, slot).key, xy[0], xy[1]);
            };
            for b in ui.buttons.iter().take(ui.button_count.min(32) as usize) {
                if !b.active.get() { continue; }
                let Some(xy) = coords(&b.press_uv) else { continue };
                let slot = b.slot.as_str();
                if slot.is_empty() || (sample.linked.is_some() && matches!(slot, "skill:2" | "skill:3")) { continue; }
                if slot == "extra:2" && BATTLE_SPELLS.contains(&b.skill_id.int()) {
                    action(&format!("spell:{}", b.skill_id.int()), xy);
                } else {
                    action(slot, xy);
                }
                if slot == "attack" && hero == 105 && b.skill_id.int() == 10500 { action("attack:quick", xy); }
                if let Some(link) = &sample.linked {
                    for (n, endpoint) in link.endpoints.iter().enumerate() {
                        if endpoint == slot { action(&format!("combo:{n}"), xy); }
                    }
                }
            }
            if let Some([x,y]) = coords(&ui.cancel_uv) {
                row("cancel", personal("cancel_key"), settings.integer("hero.cancel_key",hero), x,y);
            }
            if ui.movement.ready.get() && let Some([x,y]) = coords(&ui.movement.center_uv) {
                if settings.integer("hero.movement.click_mode", hero) == 2 {
                    let [rx,ry] = coords(&ui.movement.radius_uv).unwrap_or_default();
                    for (name, dx, dy) in [("up",0.,-1.), ("down",0.,1.), ("left",-1.,0.), ("right",1.,0.)] {
                        let field = format!("movement.{name}");
                        row(&field, personal(&field), settings.integer(&format!("hero.{field}"),hero), x+dx*rx*0.7,y+dy*ry*0.7);
                    }
                } else {
                    row("movement", personal("movement.button"), settings.integer("hero.movement.button",hero), x,y);
                }
            }
        }
        for (&id, point) in &settings.points {
            row(&format!("point:{id}"), format!("points.{id}.key"), point.key,
                point.x as f64 / 1e6, point.y as f64 / 1e6);
        }
        rows
    }
}
