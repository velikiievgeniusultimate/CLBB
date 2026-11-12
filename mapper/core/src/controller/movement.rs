use super::*;
impl Controller {
    pub(crate) fn finish_movement(&mut self, host: &mut impl Host, reason: &str) {
        self.movementPressed_ = false;
        self.movementPressTime_.invalidate();
        self.stop_movement_output(host, reason);
    }
    fn stop_movement_output(&mut self, host: &mut impl Host, reason: &str) {
        let active = self.movementTouch_ >= 0 || self.movementAuto_;
        if self.movementTouch_ >= 0 {
            host.end(self.movementTouch_, self.movementTouchPoint_)
        }
        host.timer(Timer::Movement, None);
        self.movementEchoPending_ = false;
        self.movementTouch_ = -1;
        self.movementHeld_ = false;
        self.movementAuto_ = false;
        self.movementPath_.clear();
        self.movementWaypoint_ = 0;
        self.movementRouteTime_.invalidate();
        self.movementProgressTime_.invalidate();
        self.movementReplanTime_.invalidate();
        if active || !reason.is_empty() {
            self.set_movement_status(if reason.is_empty() {
                "Остановлено"
            } else {
                reason
            });
            self.trace(host, "movement", "stopped")
        }
    }
    fn start_movement_touch(&mut self, host: &mut impl Host) -> bool {
        if self.movementTouch_ >= 0 {
            return true;
        }
        self.movementTouchPoint_ = self.movementPress_;
        self.movementTouch_ = host.begin(self.movementPress_);
        if self.movementTouch_ < 0 {
            self.finish_movement(host, "Не удалось начать движение");
            return false;
        }
        host.timer(Timer::Movement, Some(16));
        true
    }
    pub(crate) fn activate_movement_hold(&mut self, host: &mut impl Host) {
        if !self.movementPressed_
            || self.movementHeld_
            || !self.movementReady_
            || self.movementPressTime_.elapsed(self.now_) < self.movementHoldMs_ as i64
        {
            return;
        }
        self.movementAuto_ = false;
        self.movementPath_.clear();
        self.movementWaypoint_ = 0;
        if self.start_movement_touch(host) {
            self.movementHeld_ = true;
            self.set_movement_status("Следование за курсором");
            self.update_movement(host, false);
            self.trace(host, "movement", "hold_threshold_reached");
        }
    }
    pub(crate) fn movement_point(&self, host: &mut impl Host) -> Option<Point> {
        if !self.frameCameraReady_ {
            return None;
        }
        let target = if self.movementAuto_ {
            *self.movementPath_.get(self.movementWaypoint_ as usize)?
        } else {
            host.ground_world(self.pointer_).filter(|p| finite(*p))?
        };
        let delta = target - self.frameHero_;
        let size = delta.length();
        let mut angular = AngularRadialSolver::default();
        if !angular.configure(
            self.pixels(self.movementCenter_),
            self.radius_pixels(self.movementRadius_),
            self.frameMovementCamera_,
            1.,
            self.androidSize_,
        ) {
            return None;
        }
        angular.predict(if size > 0.03 {
            delta / size
        } else {
            Point::default()
        })
    }
    pub(crate) fn begin_movement_route(&mut self, host: &mut impl Host, target: Point) -> bool {
        if !self.frameCameraReady_ || !finite(target) {
            self.stop_movement_output(host, "Нет свежих координат цели");
            return false;
        }
        let target = if self.movementClickMode_ == 0 {
            let percent = self.movementClickPercent_.unwrap_or(100);
            if percent == 0 {
                self.stop_movement_output(host, "Точка достигнута");
                return true;
            }
            if percent == 100 {
                target
            } else {
                self.frameHero_ + (target - self.frameHero_) * (f64::from(percent) / 100.)
            }
        } else {
            target
        };
        let path = if self.movementClickMode_ == 0 {
            vec![target]
        } else if self.navigationAvailable_ {
            match host.navigation(self.frameHero_, target, true) {
                Ok(path) => path,
                Err(reason) => {
                    self.stop_movement_output(
                        host,
                        if reason.is_empty() {
                            "Недостижимая точка"
                        } else {
                            &reason
                        },
                    );
                    return false;
                }
            }
        } else {
            self.stop_movement_output(host, "Обход недоступен: карта проходимости ещё не получена");
            return false;
        };
        if path.is_empty() {
            self.stop_movement_output(host, "Точка достигнута");
            return true;
        }
        if path.len() > 4096 {
            self.stop_movement_output(host, "Нет безопасного маршрута");
            return false;
        }
        if path
            .iter()
            .any(|p| !finite(*p) || p.x.abs() > 1e6 || p.y.abs() > 1e6)
        {
            self.stop_movement_output(host, "Некорректный маршрут");
            return false;
        }
        self.movementTarget_ = *path.last().unwrap();
        self.movementPath_ = path;
        self.movementWaypoint_ = 0;
        self.movementRequestedTarget_ = target;
        self.movementAuto_ = true;
        self.movementHeld_ = false;
        self.movementPreviousHero_ = self.frameHero_;
        self.movementRouteTime_.restart(self.now_);
        self.movementProgressTime_.restart(self.now_);
        self.movementReplanTime_.restart(self.now_);
        self.movementProgressHero_ = self.frameHero_;
        self.set_movement_status(if self.movementClickMode_ == 1 {
            "Движение с обходом препятствий"
        } else {
            "Движение к точке по прямой"
        });
        self.trace(host, "movement", "captured_world_goal");
        if !self.start_movement_touch(host) {
            return false;
        }
        self.update_movement(host, false);
        self.movementTouch_ >= 0
    }
    fn movement_reached(&mut self, host: &mut impl Host) -> bool {
        while (self.movementWaypoint_ as usize) < self.movementPath_.len() {
            let goal = self.movementPath_[self.movementWaypoint_ as usize];
            let travelled = self.frameHero_ - self.movementPreviousHero_;
            let square = travelled.dot(travelled);
            let along = if square > 1e-10 {
                ((goal - self.movementPreviousHero_).dot(travelled) / square).clamp(0., 1.)
            } else {
                0.
            };
            let crossed = square > 1e-10
                && along > 0.
                && along < 1.
                && distance(goal, self.movementPreviousHero_ + travelled * along) < 0.12;
            if distance(self.frameHero_, goal) > 0.12 && !crossed {
                break;
            }
            self.movementWaypoint_ += 1;
        }
        if (self.movementWaypoint_ as usize) < self.movementPath_.len() {
            return false;
        }
        self.stop_movement_output(host, "Точка достигнута");
        true
    }
    pub(crate) fn advance_movement_route(&mut self, host: &mut impl Host) {
        if !self.movementAuto_ || !self.frameCameraReady_ {
            return;
        }
        if self.movement_reached(host) {
            return;
        }
        if self.movementClickMode_ == 0 {
            self.movementPreviousHero_ = self.frameHero_;
            return;
        }
        if self.movementRouteTime_.elapsed(self.now_) > 15000 {
            self.stop_movement_output(host, "Маршрут остановлен по времени");
            return;
        }
        if distance(self.frameHero_, self.movementProgressHero_) > 0.03 {
            self.movementProgressHero_ = self.frameHero_;
            self.movementProgressTime_.restart(self.now_)
        }
        if self.movementProgressTime_.elapsed(self.now_) > 700 {
            self.stop_movement_output(host, "Остановлено у препятствия: нет продвижения");
            return;
        }
        if self.navigationAvailable_ && self.movementReplanTime_.elapsed(self.now_) >= 500
        {
            match host.navigation(self.frameHero_, self.movementRequestedTarget_, true) {
                Ok(path) => {
                    if path.is_empty() {
                        self.stop_movement_output(host, "Точка достигнута");
                        return;
                    }
                    if path.len() > 4096
                        || path
                            .iter()
                            .any(|p| !finite(*p) || p.x.abs() > 1e6 || p.y.abs() > 1e6)
                    {
                        self.stop_movement_output(host, "Обход остановлен: маршрут недоступен");
                        return;
                    }
                    self.movementTarget_ = *path.last().unwrap();
                    self.movementPath_ = path;
                    self.movementWaypoint_ = 0;
                    self.movementReplanTime_.restart(self.now_);
                    self.trace(host, "movement", "navigation_replanned");
                    if self.movement_reached(host) {
                        return;
                    }
                }
                Err(reason) => {
                    self.stop_movement_output(
                        host,
                        if reason.is_empty() {
                            "Обход остановлен: маршрут недоступен"
                        } else {
                            &reason
                        },
                    );
                    return;
                }
            }
        }
        self.movementPreviousHero_ = self.frameHero_;
    }
    pub(crate) fn update_movement(&mut self, host: &mut impl Host, force: bool) {
        if self.movementTouch_ < 0 {
            return;
        }
        if self.movementAuto_ {
            self.advance_movement_route(host);
            if self.movementTouch_ < 0 {
                return;
            }
        }
        if !self.movementReady_ {
            // Retain the already held finger, but never compute a new command
            // from geometry which this fresh source frame could not validate.
            return;
        }
        let Some(next) = self.movement_point(host) else {
            return;
        };
        let goal = self.normalized(next);
        if distance(goal, self.movementTouchPoint_) >= 1e-8 {
            if !host.move_touch(self.movementTouch_, goal) {
                self.finish_movement(host, "Не удалось передать движение");
                return;
            }
            self.movementTouchPoint_ = goal;
            self.movementEchoPending_ = true;
            self.request_pointer_flush(host)
        } else if self.movementEchoPending_ || force {
            let echo = self.movementEchoPending_;
            self.movementEchoPending_ = false;
            if !host.move_touch(self.movementTouch_, self.movementTouchPoint_) {
                self.finish_movement(host, "Не удалось передать движение");
                return;
            }
            self.trace(
                host,
                "movement_echo",
                if echo {
                    "zero_velocity"
                } else {
                    "stationary_keepalive"
                },
            )
        }
    }
    pub(crate) fn handle_mouse(
        &mut self,
        host: &mut impl Host,
        button: i32,
        pressed: bool,
        pointer: Point,
    ) -> bool {
        if !pressed { self.physicalMouseButtons_.remove(&button); }
        if self.contextIgnoredMouseButtons_.contains(&button) {
            if !pressed { self.contextIgnoredMouseButtons_.remove(&button); }
            return true;
        }
        let handled = self.handle_mouse_inner(host, button, pressed, pointer);
        if handled && pressed { self.physicalMouseButtons_.insert(button); }
        handled
    }
    fn handle_mouse_inner(
        &mut self,
        host: &mut impl Host,
        button: i32,
        pressed: bool,
        pointer: Point,
    ) -> bool {
        self.update_pointer(host, pointer);
        self.flush_pointer(host);
        if button == 1 {
            if pressed {
                self.finish_manual(host);
                self.manualPoint_ = self.pointer_;
                self.manualTouch_ = host.begin(self.manualPoint_)
            } else {
                self.finish_manual(host)
            }
            return true;
        }
        if button != 2 {
            return false;
        }
        // UP must not be gated by current geometry/readiness. A widget may be
        // temporarily unavailable during a dash; ignoring UP would leave the
        // physical command held indefinitely when geometry returns.
        if !pressed && self.movementPressed_ {
            self.movementPressed_ = false;
            let was_hold = self.movementHeld_
                || self.movementPressTime_.elapsed(self.now_) >= self.movementHoldMs_ as i64;
            self.movementPressTime_.invalidate();
            if !self.ready() || was_hold {
                self.finish_movement(host, "Удержание отпущено")
            }
            return true;
        }
        if !self.ready() || !self.movementReady_ {
            return false;
        }
        if pressed {
            if self.movementPressed_ {
                return true;
            }
            self.movementPressed_ = true;
            self.movementPressTime_.restart(self.now_);
            if let Some(target) = host.ground_world(self.pointer_).filter(|p| finite(*p)) {
                self.begin_movement_route(host, target);
            } else {
                self.stop_movement_output(host, "Нет свежих координат цели");
            }
        }
        true
    }
}
