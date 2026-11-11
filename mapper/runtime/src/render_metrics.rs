//! Rates of real EGL submissions and GTK texture handoffs, per surface.
//! These counters do not claim scan-out/monitor FPS or simulation tick rate.
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};
#[derive(Default)]
struct Series {
    rows: VecDeque<(Instant, u32)>,
    count: u64,
}
impl Series {
    fn observe(&mut self, now: Instant, serial: u32) {
        if self.rows.back().is_some_and(|(_, last)| *last == serial) {
            return;
        }
        if self.rows.back().is_some_and(|(_, last)| serial < *last) {
            self.rows.clear();
        }
        self.count += 1;
        self.rows.push_back((now, serial));
        while self.rows.len() > 1024
            || self
                .rows
                .front()
                .is_some_and(|(t, _)| now.saturating_duration_since(*t) > Duration::from_secs(2))
        {
            self.rows.pop_front();
        }
    }
    fn snapshot(&self, now: Instant) -> Value {
        let age = self
            .rows
            .back()
            .map(|(t, _)| now.saturating_duration_since(*t).as_secs_f64() * 1000.);
        let mut intervals: Vec<_> = self
            .rows
            .iter()
            .zip(self.rows.iter().skip(1))
            .map(|(a, b)| b.0.saturating_duration_since(a.0).as_secs_f64() * 1000.)
            .collect();
        intervals.sort_by(f64::total_cmp);
        let rate = self.rows.front().zip(self.rows.back()).and_then(|(a, b)| {
            let span = b.0.saturating_duration_since(a.0).as_secs_f64();
            (span > 0.).then_some((self.rows.len() - 1) as f64 / span)
        });
        json!({"total":self.count,"samples":self.rows.len(),"hz":if age.is_some_and(|a|a>500.){Some(0.)}else{rate},"last_age_ms":age,
            "interval_p50_ms":intervals.get(intervals.len()/2),"interval_p95_ms":intervals.get(intervals.len().saturating_sub(1)*95/100)})
    }
}
struct Surface {
    key: u64,
    size: [u32; 2],
    last: Instant,
    submit: Series,
    handoff: Series,
    painted: Series,
}
#[derive(Default)]
struct Metrics {
    surfaces: Vec<Surface>,
}
static METRICS: LazyLock<Mutex<Metrics>> = LazyLock::new(|| Mutex::new(Metrics::default()));
#[unsafe(no_mangle)]
pub extern "C" fn clbb_render_frame_event(
    key: u64,
    width: u32,
    height: u32,
    serial: u32,
    stage: u32,
) {
    if key == 0 || width == 0 || height == 0 || width > 32768 || height > 32768 || stage > 2 {
        return;
    }
    let now = Instant::now();
    let mut m = METRICS.lock().unwrap();
    let slot = if let Some(i) = m.surfaces.iter().position(|s| s.key == key) {
        i
    } else {
        if m.surfaces.len() >= 8 {
            let old = m
                .surfaces
                .iter()
                .enumerate()
                .min_by_key(|(_, s)| s.last)
                .map(|(i, _)| i)
                .unwrap();
            m.surfaces.swap_remove(old);
        }
        m.surfaces.push(Surface {
            key,
            size: [width, height],
            last: now,
            submit: Series::default(),
            handoff: Series::default(),
            painted: Series::default(),
        });
        m.surfaces.len() - 1
    };
    let s = &mut m.surfaces[slot];
    s.size = [width, height];
    s.last = now;
    if stage == 0 {
        s.submit.observe(now, serial)
    } else if stage == 1 {
        s.handoff.observe(now, serial)
    } else {
        s.painted.observe(now, serial)
    }
}
fn visible_series(surface: &Surface) -> &Series {
    if surface.painted.rows.is_empty() { &surface.handoff } else { &surface.painted }
}
fn displayed_surface(surfaces: &[Surface], now: Instant) -> Option<&Surface> {
    surfaces.iter().filter_map(|s| {
        let rows=&visible_series(s).rows;
        let last=rows.back()?.0;
        (rows.len()>1 && now.saturating_duration_since(last)<=Duration::from_millis(500))
            .then_some((s,last))
    }).max_by_key(|(s,last)|(u64::from(s.size[0])*u64::from(s.size[1]),*last)).map(|(s,_)|s)
}
/// Unique game frames included in GTK snapshots, not physical monitor scan-out.
/// Prefer the largest live game surface; unrelated HUD redraws never renew a frame.
pub fn fps() -> f64 {
    let now=Instant::now();let m=METRICS.lock().unwrap();
    let Some(s)=displayed_surface(&m.surfaces,now) else{return 0.};
    let rows=&visible_series(s).rows;
    let first=rows.front().unwrap().0;let last=rows.back().unwrap().0;
    let span=last.saturating_duration_since(first).as_secs_f64();
    if span>0. {(rows.len()-1) as f64/span}else{0.}
}
pub fn snapshot() -> Value {
    let now = Instant::now();
    let m = METRICS.lock().unwrap();
    json!({"meaning":"EGL submissions, GTK texture handoffs and unique GTK snapshots; none is monitor scan-out or simulation FPS",
        "surfaces":m.surfaces.iter().enumerate().map(|(i,s)|json!({"surface":i,"size":s.size,"egl_submit":s.submit.snapshot(now),"gtk_handoff":s.handoff.snapshot(now),"gtk_snapshot":s.painted.snapshot(now)})).collect::<Vec<_>>()})
}
#[cfg(test)]
mod tests {
    use super::*;
    fn painted_surface(key:u64,size:[u32;2],now:Instant,age:Duration)->Surface {
        let mut s=Surface{key,size,last:now,submit:Series::default(),handoff:Series::default(),painted:Series::default()};
        s.painted.observe(now-age-Duration::from_millis(10),1);s.painted.observe(now-age,2);s
    }
    #[test]
    fn hud_prefers_large_live_game_over_small_video() {
        let now=Instant::now();let rows=vec![painted_surface(1,[4000,3111],now,Duration::from_millis(5)),painted_surface(2,[320,200],now,Duration::ZERO)];
        assert_eq!(displayed_surface(&rows,now).unwrap().key,1);
    }
    #[test]
    fn repeated_snapshot_does_not_make_stale_surface_live_again() {
        let now=Instant::now();let mut stale=painted_surface(1,[4000,3111],now,Duration::from_secs(2));
        stale.last=now;stale.painted.observe(now,2);
        let rows=vec![stale,painted_surface(2,[320,200],now,Duration::ZERO)];
        assert_eq!(displayed_surface(&rows,now).unwrap().key,2);
        assert!(displayed_surface(&rows[..1],now).is_none());
    }
    #[test]
    fn reports_known_rate_without_counting_duplicate_frames() {
        let now = Instant::now();
        let mut s = Series::default();
        for i in 0..241 {
            s.observe(now + Duration::from_nanos(i as u64 * 4_166_667), i);
            s.observe(now + Duration::from_nanos(i as u64 * 4_166_667), i);
        }
        let v = s.snapshot(now + Duration::from_secs(1));
        assert_eq!(s.count, 241);
        assert!((v["hz"].as_f64().unwrap() - 240.).abs() < 0.001);
        assert_eq!(s.snapshot(now + Duration::from_secs(3))["hz"], 0.);
    }
    #[test]
    fn history_is_bounded_and_restart_discards_old_intervals() {
        let now = Instant::now();
        let mut s = Series::default();
        for i in 0..4000 {
            s.observe(now + Duration::from_millis(i), i as u32);
        }
        assert!(s.rows.len() <= 1024);
        s.observe(now + Duration::from_secs(4), 1);
        assert_eq!(s.rows.len(), 1);
        assert!(s.snapshot(now + Duration::from_secs(4))["hz"].is_null());
    }
}
