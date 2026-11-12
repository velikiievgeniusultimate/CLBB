//! One independent native finger for a portrait picker. Physical UP waits for
//! the game to observe the final portrait (or no portrait), not a fixed delay.
use crate::{linked_gesture::Transport, portrait_picker::Observation};
use clbb_moba_core::Point;
use serde::Serialize;
fn serialize_point<S: serde::Serializer>(p: &Point, s: S) -> std::result::Result<S::Ok, S::Error> {
    [p.x, p.y].serialize(s)
}
#[derive(Clone, Debug, Serialize)]
pub struct Copy {
    pub index: usize,
    pub guid: u32,
    pub owner: u64,
    pub source: [f64; 4],
    pub destination: [f64; 4],
}
#[derive(Debug, Serialize)]
pub struct Held {
    pub key: i32,
    pub contact: i32,
    pub epoch: String,
    pub backend: u64,
    pub started: i64,
    pub pressed: bool,
    pub physical_held: bool,
    pub copies: Vec<Copy>,
    pub goal: Option<(usize, u32, u64)>,
    pub move_ns: i64,
    #[serde(serialize_with = "serialize_point")]
    pub point: Point,
    #[serde(serialize_with = "serialize_point")]
    anchor: Point,
    #[serde(serialize_with = "serialize_point")]
    neutral: Point,
    release_ns: i64,
    cancelled: bool,
    release_goal: Option<Option<(usize, u32, u64)>>,
    copy_radius: Option<[f64; 2]>,
}
#[derive(Default, Debug, Serialize)]
pub struct Picker {
    pub held: Option<Held>,
    pub ignored: Vec<i32>,
    #[serde(serialize_with = "serialize_point")]
    pub pointer: Point,
}
fn hit(copies: &[Copy], p: Point) -> Option<&Copy> {
    copies.iter().find(|c| {
        let d = c.destination;
        let dx = (p.x - d[0] - d[2] / 2.) / (d[2] / 2.);
        let dy = (p.y - d[1] - d[3] / 2.) / (d[3] / 2.);
        dx * dx + dy * dy <= 1.
    })
}
impl Picker {
    pub fn contains(&self, key: i32) -> bool {
        self.ignored.contains(&key) || self.held.as_ref().is_some_and(|h| h.key == key)
    }
    pub fn begin(
        &mut self,
        t: &mut impl Transport,
        key: i32,
        epoch: &str,
        o: &Observation,
        anchor: Point,
        neutral: Point,
        pointer: Point,
        now: i64,
    ) -> bool {
        if self.contains(key) {
            return true;
        }
        if self.held.is_some() {
            self.ignored.push(key);
            return true;
        }
        let contact = t.down(neutral);
        if contact < 0 {
            self.ignored.push(key);
            return true;
        }
        self.pointer = pointer;
        self.held = Some(Held {
            key,
            contact,
            epoch: epoch.into(),
            backend: o.backend,
            started: now,
            pressed: true,
            physical_held: true,
            copies: Vec::new(),
            goal: None,
            move_ns: 0,
            point: neutral,
            anchor,
            neutral,
            release_ns: 0,
            cancelled: false,
            release_goal: None,
            copy_radius: None,
        });
        true
    }
    pub fn release(&mut self, key: i32, now: i64) -> bool {
        if let Some(i) = self.ignored.iter().position(|v| *v == key) {
            self.ignored.remove(i);
            return true;
        }
        if let Some(h) = self.held.as_mut().filter(|h| h.key == key) {
            h.physical_held = false;
            h.pressed = false;
            h.release_ns = now;
            h.release_goal = Some(hit(&h.copies, self.pointer).map(|c| (c.index, c.guid, c.owner)));
            return true;
        }
        false
    }
    pub fn cancel(&mut self, now: i64) {
        if let Some(h) = &mut self.held {
            h.cancelled = true;
            h.pressed = false;
            h.release_ns = now;
            h.release_goal = Some(None);
        }
    }
    pub fn reset(&mut self) {
        if let Some(h) = self.held.take() {
            if h.physical_held {
                self.ignored.push(h.key);
            }
        }
    }
    pub fn poll(
        &mut self,
        t: &mut impl Transport,
        epoch: &str,
        o: Option<&Observation>,
        sample_ns: i64,
        now: i64,
    ) {
        let Some(h) = &mut self.held else {
            return;
        };
        let Some(o) = o.filter(|o| {
            epoch == h.epoch
                && o.backend == h.backend
                && o.base == 10430
                && o.active == 10431
                && sample_ns >= h.started
                && now - sample_ns <= 100_000_000
        }) else {
            // Native reset/rebind is never permission to send a target UP.
            if now - h.started > 1_000_000_000 && t.abort(h.contact, h.neutral) {
                self.reset();
            }
            return;
        };
        if !o.held {
            return;
        }
        // Five stable slots, even when only a subset is selectable. The anchor
        // is captured once at DOWN; a camera teleport never moves the copies.
        if h.copy_radius.is_none() {
            h.copy_radius = o.portraits.first().map(|p| {
                [
                    (p.radius[0] * 1.4).clamp(0.012, 0.04),
                    (p.radius[1] * 1.4).clamp(0.012, 0.06),
                ]
            });
        }
        let [rx, ry] = h.copy_radius.unwrap_or([0.024, 0.031]);
        let x = h.anchor.x.clamp(rx * 6.5, 1. - rx * 6.5);
        let y = (h.anchor.y - 0.16).clamp(ry * 1.2, 1. - ry * 1.2);
        h.copies = o
            .portraits
            .iter()
            .map(|p| Copy {
                index: p.index,
                guid: p.guid,
                owner: p.owner,
                source: [
                    p.center[0] - p.radius[0],
                    p.center[1] - p.radius[1],
                    p.radius[0] * 2.,
                    p.radius[1] * 2.,
                ],
                destination: [
                    x + (p.index as f64 - 2.) * rx * 2.6 - rx,
                    y - ry,
                    rx * 2.,
                    ry * 2.,
                ],
            })
            .collect();
        let target = if h.cancelled {
            None
        } else if let Some(goal) = h.release_goal {
            goal.and_then(|g| h.copies.iter().find(|c| (c.index, c.guid, c.owner) == g))
        } else {
            hit(&h.copies, self.pointer)
        };
        let mut goal = target.map(|c| (c.index, c.guid, c.owner));
        let mut point = target
            .map(|c| {
                Point::new(
                    c.source[0] + c.source[2] / 2.,
                    c.source[1] + c.source[3] / 2.,
                )
            })
            .unwrap_or(h.neutral);
        if !h.pressed && now - h.release_ns > 500_000_000 {
            goal = None;
            point = h.neutral;
            h.cancelled = true;
        }
        if h.move_ns == 0
            || h.goal != goal
            || (h.point.x - point.x).abs() > 0.00005
            || (h.point.y - point.y).abs() > 0.00005
        {
            if t.motion(h.contact, point) {
                h.goal = goal;
                h.point = point;
                h.move_ns = now;
            }
            return;
        }
        // Recheck the same index/GUID/owner in the current native array before
        // releasing. Reordering, disappearing stacks and duplicate portraits
        // cannot silently turn a pending release into a different opponent.
        let matched = sample_ns > h.move_ns
            && match h.goal {
                Some((index, guid, owner)) => {
                    o.selected == index as i32
                        && o.portraits
                            .iter()
                            .any(|p| p.index == index && p.guid == guid && p.owner == owner)
                }
                None => o.selected == -1,
            };
        if !h.pressed && matched {
            let h = self.held.take().unwrap();
            t.up(h.contact, h.point);
            if h.physical_held {
                self.ignored.push(h.key);
            }
        }
    }
    pub fn overlay(&self) -> String {
        let Some(h) = &self.held else {
            return String::new();
        };
        let mut out = String::new();
        for c in &h.copies {
            use std::fmt::Write;
            let _ = writeln!(
                out,
                "{} {} {} {} {} {} {} {} {}",
                c.source[0],
                c.source[1],
                c.source[2],
                c.source[3],
                c.destination[0],
                c.destination[1],
                c.destination[2],
                c.destination[3],
                i32::from(h.goal.is_some_and(|g| g.0 == c.index && g.1 == c.guid))
            );
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portrait_picker::Portrait;
    #[derive(Default)]
    struct Wire {
        events: Vec<(u8, i32, Point)>,
    }
    impl Transport for Wire {
        fn down(&mut self, p: Point) -> i32 {
            self.events.push((0, 7, p));
            7
        }
        fn motion(&mut self, id: i32, p: Point) -> bool {
            self.events.push((1, id, p));
            true
        }
        fn up(&mut self, id: i32, p: Point) {
            self.events.push((2, id, p));
        }
    }
    fn native() -> Observation {
        Observation {
            backend: 30,
            slot: "skill:2".into(),
            base: 10430,
            active: 10431,
            held: true,
            selected: -1,
            camera_guid: 0,
            portraits: (0..5)
                .map(|i| Portrait {
                    index: i,
                    guid: 100 + i as u32,
                    owner: 200 + i as u64,
                    center: [0.9 - i as f64 * 0.04, 0.7],
                    radius: [0.02, 0.025],
                })
                .collect(),
        }
    }
    fn begin() -> (Picker, Wire, Observation) {
        let mut p = Picker::default();
        let mut w = Wire::default();
        let o = native();
        p.begin(
            &mut w,
            69,
            "match",
            &o,
            Point::new(0.5, 0.55),
            Point::new(0.94, 0.7),
            Point::new(0.2, 0.3),
            1,
        );
        p.poll(&mut w, "match", Some(&o), 2, 3);
        (p, w, o)
    }
    fn center(c: &Copy) -> Point {
        Point::new(
            c.destination[0] + c.destination[2] / 2.,
            c.destination[1] + c.destination[3] / 2.,
        )
    }
    #[test]
    fn all_five_map_to_their_own_native_portrait_and_wait_for_ack() {
        for i in 0..5 {
            let (mut p, mut w, mut o) = begin();
            p.pointer = center(&p.held.as_ref().unwrap().copies[i]);
            p.poll(&mut w, "match", Some(&o), 4, 5);
            p.release(69, 6);
            p.poll(&mut w, "match", Some(&o), 7, 8);
            assert!(!w.events.iter().any(|e| e.0 == 2));
            o.selected = i as i32;
            o.camera_guid = o.portraits[i].guid;
            p.poll(&mut w, "match", Some(&o), 9, 10);
            assert!(p.held.is_none());
            let up = w.events.last().unwrap();
            assert_eq!(up.0, 2);
            assert_eq!(up.1, 7);
            assert_eq!(
                up.2,
                Point::new(o.portraits[i].center[0], o.portraits[i].center[1])
            );
        }
    }
    #[test]
    fn outside_release_cancels_after_native_no_selection() {
        let (mut p, mut w, mut o) = begin();
        p.pointer = center(&p.held.as_ref().unwrap().copies[1]);
        p.poll(&mut w, "match", Some(&o), 4, 5);
        o.selected = 1;
        p.pointer = Point::new(0.2, 0.2);
        p.release(69, 6);
        p.poll(&mut w, "match", Some(&o), 7, 8);
        p.poll(&mut w, "match", Some(&o), 9, 10);
        assert!(p.held.is_some());
        o.selected = -1;
        p.poll(&mut w, "match", Some(&o), 11, 12);
        assert_eq!(w.events.last().unwrap().2, Point::new(0.94, 0.7));
        assert!(!p.contains(69));
    }
    #[test]
    fn camera_hover_does_not_change_clone_anchor() {
        let (mut p, mut w, mut o) = begin();
        let before = p.held.as_ref().unwrap().copies[0].destination;
        o.camera_guid = 100;
        o.portraits[0].radius = [0.03, 0.04];
        p.poll(&mut w, "match", Some(&o), 4, 5);
        assert_eq!(before, p.held.as_ref().unwrap().copies[0].destination);
    }
    #[test]
    fn recipient_replacement_during_release_cannot_cast_new_opponent() {
        let (mut p, mut w, mut o) = begin();
        p.pointer = center(&p.held.as_ref().unwrap().copies[0]);
        p.poll(&mut w, "match", Some(&o), 4, 5);
        p.release(69, 6);
        o.portraits[0].guid = 999;
        o.selected = 0;
        p.poll(&mut w, "match", Some(&o), 7, 8);
        assert_eq!(p.held.as_ref().unwrap().goal, None);
        assert_eq!(w.events.last().unwrap().2, Point::new(0.94, 0.7));
    }
    #[test]
    fn stale_or_foreign_backend_does_not_emit_target_up() {
        let (mut p, mut w, mut o) = begin();
        p.release(69, 4);
        let n = w.events.len();
        o.backend = 99;
        p.poll(&mut w, "match", Some(&o), 5, 6);
        assert_eq!(w.events.len(), n);
        p.poll(&mut w, "other", Some(&o), 7, 8);
        assert_eq!(w.events.len(), n);
    }
    #[test]
    fn cancel_key_retires_only_picker_contact_and_waits_for_physical_up() {
        let (mut p, mut w, o) = begin();
        p.cancel(4);
        p.poll(&mut w, "match", Some(&o), 5, 6);
        assert!(p.held.is_none());
        assert!(p.contains(69));
        assert!(w.events.iter().all(|e| e.1 == 7));
        p.release(69, 7);
        assert!(!p.contains(69));
    }
    #[test]
    fn release_before_panel_appears_is_retained_until_native_down() {
        let (mut p, mut w, mut o) = begin();
        o.held = false;
        p.release(69, 4);
        let n = w.events.len();
        p.poll(&mut w, "match", Some(&o), 5, 6);
        assert_eq!(n, w.events.len());
        o.held = true;
        p.poll(&mut w, "match", Some(&o), 7, 8);
        assert!(p.held.is_none());
    }
}
