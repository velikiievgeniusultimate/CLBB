//! Host render-thread pacing. No game memory, clocks or simulation values are
//! changed. GTK/input threads never sleep here. Missed frames are not replayed.
use std::cell::RefCell;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

const VSYNC: u32 = 1 << 16;
static CONFIG: AtomicU32 = AtomicU32::new(60 | VSYNC);

fn configuration(limit: u32, vsync: i32) -> Option<u32> {
    if !(30..=240).contains(&limit) || !matches!(vsync, 0 | 1) {
        return None;
    }
    Some(limit | if vsync == 1 { VSYNC } else { 0 })
}

#[derive(Default)]
struct Schedule {
    configuration: u32,
    next: Option<Instant>,
}
impl Schedule {
    fn deadline(&mut self, config: u32, now: Instant) -> Instant {
        if self.configuration != config {
            self.configuration = config;
            self.next = None;
        }
        // A late producer submits at most one current frame, never a burst of
        // frames to catch up an old deadline after loading/minimize/resize.
        self.next.unwrap_or(now).max(now)
    }
    fn submitted(&mut self, config: u32, now: Instant) {
        let period = Duration::from_nanos(1_000_000_000_u64.div_ceil((config & 0xffff) as u64));
        self.next = now.checked_add(period);
    }
}
thread_local! {
    static SCHEDULE: RefCell<Schedule> = RefCell::new(Schedule::default());
}

/// Called once at startup and after a validated atomic settings change.
#[unsafe(no_mangle)]
pub extern "C" fn clbb_frame_pacer_configure(limit: u32, vsync: i32) -> i32 {
    let Some(config) = configuration(limit, vsync) else {
        return 0;
    };
    CONFIG.store(config, Ordering::Release);
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn clbb_frame_pacer_configuration() -> u32 {
    CONFIG.load(Ordering::Acquire)
}

/// Runs on an EGL producer thread, without any GTK/UI or controller lock.
#[unsafe(no_mangle)]
pub extern "C" fn clbb_frame_pacer_wait() {
    let config = CONFIG.load(Ordering::Acquire);
    SCHEDULE.with(|slot| {
        let mut schedule = slot.borrow_mut();
        let now = Instant::now();
        let deadline = schedule.deadline(config, now);
        if deadline > now {
            std::thread::sleep(deadline.duration_since(now));
        }
        // A configuration edit while sleeping applies to the next frame. The
        // maximum extra wait is one old frame period (33.4 ms at 30 FPS).
        schedule.submitted(config, Instant::now());
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_bounded_known_configurations_are_accepted() {
        assert_eq!(configuration(60, 1), Some(60 | VSYNC));
        assert_eq!(configuration(240, 0), Some(240));
        for fps in [0, 29, 241, u32::MAX] {
            assert_eq!(configuration(fps, 1), None);
        }
        assert_eq!(configuration(60, 2), None);
        assert_eq!(configuration(60, -1), None);
    }
    #[test]
    fn each_limit_caps_producer_start_intervals_without_catch_up() {
        for fps in [30, 60, 90, 120, 144, 165, 180, 240] {
            let config = configuration(fps, 0).unwrap();
            let period = Duration::from_nanos(1_000_000_000_u64.div_ceil(fps as u64));
            let start = Instant::now();
            let mut s = Schedule::default();
            assert_eq!(s.deadline(config, start), start);
            s.submitted(config, start);
            assert_eq!(s.deadline(config, start + period / 2), start + period);
            let late = start + Duration::from_secs(9);
            assert_eq!(s.deadline(config, late), late);
            s.submitted(config, late);
            assert_eq!(s.deadline(config, late), late + period);
        }
    }
    #[test]
    fn edits_reset_an_old_deadline_including_vsync_only_edits() {
        let start = Instant::now();
        let mut s = Schedule::default();
        s.deadline(30 | VSYNC, start);
        s.submitted(30 | VSYNC, start);
        let now = start + Duration::from_millis(1);
        assert_eq!(s.deadline(240, now), now);
        s.submitted(240, now);
        assert_eq!(s.deadline(240 | VSYNC, now), now);
    }
}
