//! Safe runtime-independent entry points for the CLBB host adapter.
use super::{Controller,host::{Host,Timer},snapshot::Frame,view::View};
use crate::Point;
pub enum Input<'a> {
    Enabled(bool), Pointer(Point), Key { code:i32, down:bool, repeat:bool },
    Mouse { button:i32, down:bool, point:Point }, Timer(Timer), Release,
    Invalid(&'a str), Navigation { available:bool, reason:&'a str },
    MovementKey { direction: usize, down: bool }, MovementMode(i32), MovementClickPercent(i32), HoldThreshold(i64), RefreshBindings, Trace(bool),
}
impl Controller {
    pub fn apply_input(&mut self,host:&mut impl Host,event:Input<'_>,now:i64)->bool {
        self.now_=now;self.changed_=false;
        self.observe_host_touch_reset(host);
        match event {
            Input::Enabled(v)=>self.set_enabled(host,v),
            Input::Pointer(p)=>self.update_pointer(host,p),
            Input::Key{code,down,repeat}=>return self.handle_key(host,code,down,repeat),
            Input::Mouse{button,down,point}=>return self.handle_mouse(host,button,down,point),
            Input::Timer(Timer::Main)=>self.tick(host),
            Input::Timer(Timer::Pointer)=>self.flush_pointer(host),
            Input::Timer(Timer::Movement)=>{if self.ready()&&self.movementReady_{self.update_movement(host,true);}},
            Input::Timer(Timer::Auxiliary)=>self.expire_auxiliary_taps(host),
            Input::Timer(Timer::Tap)=>self.tick(host),
            Input::Release=>self.release_all(host),
            Input::Invalid(s)=>self.invalidate(host,s),
            Input::Navigation{available,reason}=>self.set_navigation_state(host,available,reason.into()),
            Input::MovementKey { direction, down } => return self.keyboard_movement(host, direction, down),
            Input::MovementMode(v) => {
                if (0..=2).contains(&v) && self.movementClickMode_ != v {
                    self.finish_movement(host, "");
                    self.movementClickMode_ = v;
                }
            }
            Input::MovementClickPercent(v) => self.movementClickPercent_ = Some(v.clamp(0, 500)),
            Input::HoldThreshold(v)=>self.movementHoldMs_=v.clamp(40,600) as i32,
            Input::Trace(value)=>self.traceEnabled_=value,
            Input::RefreshBindings=>{self.cancelKey_=host.cancel_key(self.heroId_);self.refresh_bindings(host);},
        };true
    }
    pub fn apply_snapshot(&mut self,host:&mut impl Host,frame:&Frame,now:i64) {
        self.now_=now;
        self.observe_host_touch_reset(host);
        if frame.hero_screen_uv.valid(){self.heroScreen_=Point::new(frame.hero_screen_uv.values[0].value,frame.hero_screen_uv.values[1].value);}
        self.ingest_frame(host,frame);
    }
    pub fn inspect(&self)->View {View::of(self)}
    pub fn update_observation(&mut self,ready:bool,world:Point,skill:i32){self.observe_aim(ready,world,skill);}
}
