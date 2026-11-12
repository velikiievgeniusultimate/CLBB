//! Single-threaded controller, direct touch delivery and a read-only source worker.
use crate::hero_names::hero_display_name;
use crate::platform::linux::{ProcessMemory, now_ns};
use crate::{
    navigation::NavMesh,
    projection::{self, Ray},
    settings::Settings,
    recorder::Recorder,
    source::{Sample, Source},
};
use clbb_moba_core::{
    Point,
    controller::{
        Button, Controller,
        api::Input,
        host::{Binding, Host, Timer},
    },
};
use std::{
    collections::BTreeMap,
    ffi::{CStr, CString, c_char, c_void},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};
type Touch = unsafe extern "C" fn(*mut c_void, i32, i32, f64, f64) -> i32;
static GAIN: AtomicU32 = AtomicU32::new(1f32.to_bits());
#[unsafe(no_mangle)]
pub extern "C" fn clbb_audio_gain() -> f32 {
    f32::from_bits(GAIN.load(Ordering::Relaxed)) * crate::audio::stream_gain()
}
static GAME_GAIN: AtomicU32 = AtomicU32::new(1f32.to_bits());
static VOICE_GAIN: AtomicU32 = AtomicU32::new(1f32.to_bits());
#[unsafe(no_mangle)]
pub extern "C" fn clbb_audio_game_gain() -> f32 {
    clbb_audio_gain() * f32::from_bits(GAME_GAIN.load(Ordering::Relaxed))
}
#[unsafe(no_mangle)]
pub extern "C" fn clbb_audio_voice_gain() -> f32 {
    clbb_audio_gain() * f32::from_bits(VOICE_GAIN.load(Ordering::Relaxed))
}
fn configure_channel_gains(settings: &Settings) {
    GAME_GAIN.store((settings.integer("audio.game_volume", 0) as f32 / 100.).to_bits(), Ordering::Relaxed);
    VOICE_GAIN.store((settings.integer("audio.voice_volume", 0) as f32 / 100.).to_bits(), Ordering::Relaxed);
}
#[derive(Default)]
struct Shared {
    sample: Option<Arc<Sample>>,
    mesh: Option<Arc<NavMesh>>,
    error: String,
    source_ms: f64,
    published_samples: u64,
    rejected_samples: u64,
    last_publish_ns: i64,
    max_publish_gap_ms: f64,
    max_navigation_ms: f64,
}
struct Worker {
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    size: Arc<Mutex<[f64; 2]>>,
}
/// Keep transient replan failures on the active cadence. An old published
/// sample is not evidence that the game is still in a match.
fn source_cadence(last_success: Option<Instant>, now: Instant, cost: Duration) -> (bool, Duration) {
    let active = last_success
        .is_some_and(|last| now.saturating_duration_since(last) <= Duration::from_millis(250));
    let wait = if active {
        Duration::from_millis(2).saturating_sub(cost)
    } else {
        // Even a slow failed metadata scan must yield before another retry.
        Duration::from_millis(750)
    };
    (active, wait)
}
impl Worker {
    fn start(recorder: Arc<Recorder>) -> Self {
        let shared = Arc::new(Mutex::new(Shared {
            sample: None,
            mesh: None,
            error: "Ожидаю матч".into(),
            source_ms: 0.,
            published_samples: 0,
            rejected_samples: 0,
            last_publish_ns: 0,
            max_publish_gap_ms: 0.,
            max_navigation_ms: 0.,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let size = Arc::new(Mutex::new([1280., 720.]));
        let (out, quit, dimensions) = (shared.clone(), stop.clone(), size.clone());
        std::thread::Builder::new()
            .name("clbb-game-source".into())
            .spawn(move || {
                let memory = ProcessMemory::current();
                let Ok(mut source) = Source::new(memory, format!("clbb:{}", std::process::id()))
                else {
                    return;
                };
                let mut mesh: Option<Arc<NavMesh>> = None;
                let mut epoch = String::new();
                let mut nav_at = Instant::now() - Duration::from_secs(5);
                let mut last_success = None;
                let mut result_at = 0i64;
                let mut audit_at=0i64;let mut full_frame_at=0i64;let mut frame_identity=(0,String::new());
                while !quit.load(Ordering::Relaxed) {
                    let start = Instant::now();
                    let size = *dimensions.lock().unwrap();
                    let result = source.sample(now_ns(), size);
                    match result {
                        Ok(sample) => {
                            last_success = Some(Instant::now());
                            if recorder.is_enabled() {
                                let f = &sample.frame;
                                recorder.push(now_ns(), "source", serde_json::json!({
                                    "start_ns": f.time_ns.value-f.sample_duration_ms.value*1e6,"end_ns":f.time_ns.value,
                                    "clock_ms":sample.game_clock_ms,"epoch":f.token.as_str(),"revision":sample.plan_revision,
                                    "linked":sample.linked,"cancel_uv_valid":f.ui.cancel_uv.valid(),"cancel_uv":f.ui.cancel_uv.values.map(|v|v.value),
                                    "control_types":sample.control_types,"control_holds":sample.control_holds,
                                    "buttons":f.ui.buttons.iter().take(f.ui.button_count.min(32) as usize).map(|b|serde_json::json!({"slot":b.slot.as_str(),"kind":b.kind.as_str(),"skill":b.skill_id.int(),"active":b.active.get(),"press_valid":b.press_uv.valid(),"press":b.press_uv.values.map(|v|v.value)})).collect::<Vec<_>>(),
                                    "hero":f.hero_id.int(),"hero_render":f.hero_render_xyz.values.map(|v|v.value),"hero_logic":sample.logic,"direction_visual":sample.direction_visual,"direction_source":f.direction_input.source.as_str(),
                                    "skill":f.skill_id.int(),"slot":f.ui.active_slot.as_str(),"resolution":f.ui.resolution_px.values.map(|v|v.value),"native_source":f.logical.source.as_str(),"view":f.view_matrix.values.map(|v|v.value),"projection":sample.projection,
                                    "aim":sample.aim,"center":f.ui.joystick.center_uv.values.map(|v|v.value),"radius":f.ui.joystick.radius_uv.values.map(|v|v.value),
                                    "logical_ready":f.logical.ready.get(),"input_ready":f.logical.input_ready.get(),"valid_drag":f.logical.valid_drag.get(),"first_drag":f.logical.first_drag.get(),
                                    "offset":f.logical.offset_xz.values.map(|v|v.value),"raw_touch":f.logical.raw_touch_xyz.values.map(|v|v.value),"range":f.logical.world_range.value,
                                    "cooldown":f.ui.cooldown_state.buttons.iter().take(f.ui.cooldown_state.count.min(32)as usize).map(|b|serde_json::json!({"slot":b.slot.as_str(),"base":b.base_skill_id.int(),"active":b.active_skill_id.int(),"ready":b.ready.get(),"on_cooldown":b.on_cooldown.get()})).collect::<Vec<_>>(),
                                    "cooldown_ready":f.ui.cooldown_state.ready.get(),"cooldown_coherent":f.ui.cooldown_state.coherent.get(),"cooldown_start_ns":f.ui.cooldown_state.sample_start_ns.value,"cooldown_end_ns":f.ui.cooldown_state.sample_end_ns.value,
                                    "direction_coherent":f.direction_input.coherent.get(),"direction_sample_start_ns":f.direction_input.sample_start_ns.value,"direction_sample_end_ns":f.direction_input.sample_end_ns.value,"direction_token":f.direction_input.token.as_str(),"direction_hero":f.direction_input.hero_id.int(),"direction_slot":f.direction_input.slot.as_str(),"direction_skill":f.direction_input.skill_id.int(),
                                    "direction_touch":f.direction_input.raw_touch_xyz.values.map(|v|v.value),"direction_ready":f.direction_input.ready.get(),"direction":f.direction_input.direction_xz.values.map(|v|v.value)
                                }));
                                let identity=(f.skill_id.int(),f.ui.active_slot.as_str().to_owned());
                                if identity!=frame_identity || now_ns()-full_frame_at>=32_000_000 {
                                    use crate::flight_schema::Encode;
                                    frame_identity=identity;full_frame_at=now_ns();
                                    recorder.push(full_frame_at,"typed_frame",f.encode());
                                }
                                if now_ns()-audit_at>=20_000_000 {
                                    audit_at=now_ns();
                                    let before=now_ns();let ui=source.audit_ui_input();let controls=source.audit_control_state();
                                    recorder.push(now_ns(),"native_ui_audit",serde_json::json!({"start_ns":before,"end_ns":now_ns(),"ui":ui,"controls":controls}));
                                }
                                if now_ns()-result_at >= 8_000_000 {
                                    result_at=now_ns();
                                    recorder.push(result_at,"skill_result",serde_json::json!(crate::recorder::skill_result(&mut source,sample.match_owners[2],sample.match_owners[3])));
                                }
                            }
                            let token = sample.frame.token.as_str();
                            if token != epoch {
                                mesh = None;
                                epoch = token.into();
                                nav_at = Instant::now() - Duration::from_secs(5);
                            }
                            {
                                let mut out = out.lock().unwrap();
                                let published = now_ns();
                                if out.last_publish_ns != 0 {
                                    out.max_publish_gap_ms = out.max_publish_gap_ms.max((published - out.last_publish_ns) as f64 / 1e6);
                                }
                                out.last_publish_ns = published;
                                out.published_samples += 1;
                                out.source_ms = sample.frame.sample_duration_ms.value;
                                out.sample = Some(Arc::new(sample));
                                out.error.clear();
                                out.mesh = mesh.clone();
                            }
                            if nav_at.elapsed()
                                > Duration::from_millis(if mesh.is_some() { 250 } else { 1500 })
                            {
                                nav_at = Instant::now();
                                let nav_started = Instant::now();
                                if mesh
                                    .as_ref()
                                    .is_some_and(|m| m.validate(&source.reflection).is_err())
                                {
                                    mesh = None;
                                }
                                if mesh.is_none() {
                                    mesh = NavMesh::discover(&mut source.reflection)
                                        .ok()
                                        .map(Arc::new);
                                }
                                let mut out = out.lock().unwrap();
                                out.mesh = mesh.clone();
                                out.max_navigation_ms = out.max_navigation_ms.max(nav_started.elapsed().as_secs_f64() * 1000.);
                            }
                        }
                        Err(e) => {
                            if recorder.is_enabled() { recorder.push(now_ns(),"source_rejected",serde_json::json!({"reason":e})); }
                            let mut out = out.lock().unwrap();
                            out.rejected_samples += 1;
                            out.error = e;
                            out.mesh = None;
                        }
                    }
                    let now = Instant::now();
                    let (active, wait) = source_cadence(last_success, now, now.duration_since(start));
                    if !active {
                        let mut out = out.lock().unwrap();
                        out.sample = None;
                        out.mesh = None;
                    }
                    if !wait.is_zero() {
                        std::thread::sleep(wait);
                    }
                }
            })
            .expect("source worker");
        Self { shared, stop, size }
    }
}
struct Adapter {
    settings: Settings,
    sample: Option<Arc<Sample>>,
    inverse: Option<[f64; 16]>,
    mesh: Option<Arc<NavMesh>>,
    callback: Touch,
    user: *mut c_void,
    touches: [bool; 16],
    reset_generation: u64,
    rotate_contacts: bool,
    next_contact: usize,
    rays: BTreeMap<u64, Ray>,
    wall_surface:projection::WallSurface,
    next_ray: u64,
    timers: [Option<i64>; 5],
    conflicts: BTreeMap<i32,Vec<String>>,
    conflict_stamp: Option<u64>,
    trace: bool,
    recorder: Arc<Recorder>,
    last_feedback_trace: Option<(String,i64)>,
}
impl Adapter {
    fn fresh(&self) -> Option<&Sample> {
        self.sample
            .as_deref()
            .filter(|s| now_ns() - s.frame.time_ns.value as i64 <= 250_000_000)
    }
    fn ray(&self, p: Point) -> Option<Ray> {
        self.fresh()?;
        projection::ray(self.inverse.as_ref()?, p)
    }
    fn touch(&self, action: i32, id: i32, p: Point) -> bool {
        if !p.x.is_finite() || !p.y.is_finite() {
            if self.recorder.is_enabled(){self.recorder.push(now_ns(),"touch_rejected",serde_json::json!({"reason":"nonfinite_coordinate","action":action,"id":id}));}
            return false;
        }
        let requested=p;
        let continuation = action != 0 && (0..16).contains(&id) && self.touches[id as usize];
        let p = if continuation {
            if !(-1. ..=2.).contains(&p.x) || !(-1. ..=2.).contains(&p.y) {
                return false;
            }
            p
        } else {
            Point::new(p.x.clamp(0., 1.), p.y.clamp(0., 1.))
        };
        // Preserve the shared controller's integer command-cell contract. The
        // native Android feedback uses an inclusive [0,size-1] device domain;
        // ATL's direct pixel injection must encode it before JNI delivery.
        let p = if let Some(sample) = &self.sample {
            android_packet_point(p, sample.frame.ui.resolution_px.values.map(|n| n.value))
        } else {
            p
        };
        let at=now_ns();
        if self.recorder.is_enabled(){self.recorder.push(at,"touch_command",serde_json::json!({"action":action,"id":id,"requested_uv":[requested.x,requested.y],"packet_uv":[p.x,p.y],"phase":"before_callback"}));}
        let accepted=unsafe { (self.callback)(self.user, action, id, p.x, p.y) != 0 };
        if self.recorder.is_enabled(){self.recorder.push(now_ns(),"touch_command",serde_json::json!({"action":action,"id":id,"requested_uv":[requested.x,requested.y],"packet_uv":[p.x,p.y],"phase":"after_callback","accepted":accepted,"callback_start_ns":at}));}
        accepted
    }
    fn gain(&self) {
        let v = if self.settings.boolean("audio.muted", 0) {
            0.
        } else {
            self.settings.integer("audio.volume", 0) as f32 / 100.
        };
        GAIN.store(v.to_bits(), Ordering::Relaxed);
        configure_channel_gains(&self.settings);
    }
}
fn android_packet_point(p: Point, size: [f64; 2]) -> Point {
    let axis = |value: f64, size: f64| {
        if size.is_finite() && size >= 32. {
            let cell = (value * size + 1e-7).floor();
            // Keep the legacy inclusive device encoding unchanged in-view;
            // extend the same affine encoding to offscreen continuation cells.
            let cell = if (0. ..=1.).contains(&value) {
                cell.clamp(0., size - 1.)
            } else {
                cell
            };
            cell * (size - 1.) / size / size
        } else {
            value
        }
    };
    Point::new(axis(p.x, size[0]), axis(p.y, size[1]))
}
fn timer_index(timer: Timer) -> usize {
    match timer {
        Timer::Main => 0,
        Timer::Pointer => 1,
        Timer::Tap => 2,
        Timer::Movement => 3,
        Timer::Auxiliary => 4,
    }
}
fn primary_cancel_wait_over_100ms(view: &clbb_moba_core::controller::view::View, now: i64) -> bool {
    view.skillCancelPending_ != 0 && view.skillCancelSentNs_ > 0
        && now - view.skillCancelSentNs_ > 100_000_000
}
fn source_context_reset(previous:Option<&Sample>,current:&Sample)->Option<&'static str> {
    let frame=&current.frame;
    if !frame.ready.get()||!frame.ui.ready.get()||!frame.token_string.get()||!frame.token_nonempty.get() {
        return Some("source_context_unavailable");
    }
    let previous=previous?;
    if previous.frame.token.as_str()!=frame.token.as_str()||previous.frame.hero_id.value!=frame.hero_id.value {
        return Some("source_context_changed");
    }
    if previous.frame.ui.resolution_px.valid()&&frame.ui.resolution_px.valid()
        &&previous.frame.ui.resolution_px.values.iter().zip(frame.ui.resolution_px.values)
            .any(|(a,b)|a.value!=b.value) {return Some("source_resolution_changed");}
    None
}
fn core_decision_diagnostics(v:&clbb_moba_core::controller::view::View,event:&str,reason:&str)->serde_json::Value {
    let mut value=serde_json::json!({"event":event,"reason":reason,"skill":v.coreSkill_,"touch":v.skillTouch_,"key":v.skillKey_,"key_held":v.skillKeyHeld_,"cast_mode":v.skillCastMode_,"cast_start_ns":v.castStartedNs_,"touch_sequence":v.touchSequence_,"pending_release":v.pendingRelease_,"direction_goal_seq":v.directionGoalSequence_,"direction_matched_seq":v.directionMatchedSequence_,"observed_skill":v.observedSkillId_});
    // Capture transition proof synchronously, before finish/reset can clear it.
    // Keep high-frequency ordinary input decisions at their existing size.
    if matches!(event,"charge_transition"|"charge_tracking_revoked") || (event=="release"&&reason=="auto_completed") {
        use crate::flight_schema::Encode;
        let object=value.as_object_mut().unwrap();
        object.insert("charge_stage".into(),v.chargeStage_.encode());
        object.insert("charge_base".into(),v.chargeBase_.encode());
        object.insert("charge_future".into(),v.chargeFuture_.encode());
        object.insert("charge_end_clock".into(),v.chargeEndClock_.encode());
        object.insert("charge_end_sample_ns".into(),v.chargeEndSampleNs_.encode());
        object.insert("charge_last_sample_ns".into(),v.chargeLastSampleNs_.encode());
    }
    if event.starts_with("selection_") || (matches!(event,"release"|"cancel_requested"|"cancel_acknowledged")&&v.selectionOwner_!=0) {
        value.as_object_mut().unwrap().insert("selection".into(),serde_json::json!({
            "kind":v.selectionKind_,"backend_owner":format!("0x{:x}",v.selectionBackendOwner_),
            "contact_owner":format!("0x{:x}",v.selectionContactOwner_),"target_guid":v.selectionTargetGuid_,
            "target_owner":format!("0x{:x}",v.selectionTargetOwner_),"target_self":format!("0x{:x}",v.selectionTargetSelf_),
            "target_invalid":v.selectionTargetInvalid_,
            "goal_target_guid":v.selectionGoalTargetGuid_,"goal_target_owner":format!("0x{:x}",v.selectionGoalTargetOwner_),
            "goal_target_self":format!("0x{:x}",v.selectionGoalTargetSelf_),
            "owner":format!("0x{:x}",v.selectionOwner_),"revoked":v.selectionRevoked_,
            "geometry_fresh":v.selectionGeometryFresh_,"accepted":v.selectionAccepted_,
            "matched_seq":v.selectionMatchedSequence_,"sample_start_ns":v.selectionSampleStartNs_,
            "sample_end_ns":v.selectionSampleEndNs_,"option":v.selectionOption_,"goal_seq":v.selectionGoalSequence_,
            "captured":v.selectionCaptured_,"end_clock":crate::flight_schema::Encode::encode(&v.selectionEndClock_),"end_sample_ns":v.selectionEndSampleNs_
        }));
    }
    if matches!(event,"context_abort"|"auxiliary_hold") {
        let holds:Vec<_>=v.auxiliaryHolds_.iter().take(v.auxiliaryHoldCount_.min(8)as usize).map(|h|serde_json::json!({
            "slot":h.slot.as_str(),"id":h.id,"key":h.key,"point":[h.point.x,h.point.y],
            "owner":format!("0x{:x}",h.owner_id),"down_ns":h.down_ns,
            "cancel_supported":crate::flight_schema::Encode::encode(&h.cancel_supported)})).collect();
        value.as_object_mut().unwrap().insert("auxiliary_holds".into(),serde_json::json!(holds));
    }
    value
}
impl Adapter {
    fn refresh_conflicts(&mut self, hero:i32, settings_changed:bool) {
        use std::hash::{Hash,Hasher};
        let mut hash=std::collections::hash_map::DefaultHasher::new();
        let stamp=self.fresh().filter(|s|s.frame.ready.get()&&s.frame.hero_id.int()==hero&&s.frame.ui.ready.get()).map(|s| {
            hero.hash(&mut hash);s.linked.is_some().hash(&mut hash);
            for b in s.frame.ui.buttons.iter().take(s.frame.ui.button_count.min(32) as usize)
                .chain(s.frame.ui.capability_controls.iter().take(s.frame.ui.capability_count.min(32) as usize)) {
                b.slot.as_str().hash(&mut hash);b.active.get().hash(&mut hash);b.press_uv.valid().hash(&mut hash);
            }
            hash.finish()
        });
        if settings_changed || stamp!=self.conflict_stamp {
            self.conflicts=self.binding_conflicts(hero);self.conflict_stamp=stamp;
        }
    }
    fn binding_conflicts(&self, hero:i32) -> BTreeMap<i32,Vec<String>> {
        let mut owners:BTreeMap<i32,Vec<String>>=BTreeMap::new();
        let Some(sample)=self.fresh().filter(|s|s.frame.ready.get()&&s.frame.hero_id.int()==hero&&s.frame.ui.ready.get()) else {return owners;};
        let mut seen=std::collections::BTreeSet::new();
        let mut add=|key:i32, name:String| { if key!=0 {owners.entry(key).or_default().push(name);} };
        for b in sample.frame.ui.buttons.iter().take(sample.frame.ui.button_count.min(32) as usize)
            .chain(sample.frame.ui.capability_controls.iter().take(sample.frame.ui.capability_count.min(32) as usize)) {
            let slot=b.slot.as_str();
            if !b.active.get()||!b.press_uv.valid()||!seen.insert(slot) {continue;}
            if sample.linked.is_some()&&matches!(slot,"skill:2"|"skill:3") {continue;}
            add(self.settings.binding(hero,slot).key,slot.into());
        }
        if sample.linked.is_some() { for n in 0..2 {let slot=format!("combo:{n}");add(self.settings.binding(hero,&slot).key,slot);} }
        add(self.settings.integer("hero.cancel_key",hero),"Отмена навыка".into());
        if self.settings.integer("hero.movement.click_mode",hero)==2 {
            for direction in ["up","down","left","right"] {add(self.settings.integer(&format!("hero.movement.{direction}"),hero),format!("Движение · {direction}"));}
        } else {add(self.settings.integer("hero.movement.button",hero),"Движение".into());}
        owners.retain(|_,names|names.len()>1);owners
    }
}
impl Host for Adapter {
    fn clock_ns(&mut self) -> Option<i64> {
        Some(now_ns())
    }
    fn touch_reset_generation(&self)->u64 {self.reset_generation}
    fn abort_all_touches(&mut self)->bool {
        let owned:Vec<_>=self.touches.iter().enumerate().filter_map(|(id,held)|held.then_some(id)).collect();
        if owned.is_empty(){return false;}
        let before=self.reset_generation;
        let success=self.touch(4,0,Point::default());
        if success {
            self.touches.fill(false);
            self.reset_generation=self.reset_generation.wrapping_add(1).max(1);
        }
        if self.recorder.is_enabled(){self.recorder.push(now_ns(),"context_abort_transport",serde_json::json!({
            "owned_ids":owned,"accepted":success,"generation_before":before,"generation_after":self.reset_generation,
            "scope":"all_adapter_owned_contacts_only_on_global_lifecycle_reset"}));}
        success
    }
    fn begin(&mut self, p: Point) -> i32 {
        let start=if self.rotate_contacts {self.next_contact} else {0};
        let Some(i)=(0..16).map(|n|(start+n)%16).find(|i|!self.touches[*i]) else {
            return -1;
        };
        if !self.touch(0, i as i32, p) {
            return -1;
        }
        self.touches[i] = true;
        if self.rotate_contacts {self.next_contact=(i+1)%16;}
        i as i32
    }
    fn move_touch(&mut self, id: i32, p: Point) -> bool {
        (0..16).contains(&id) && self.touches[id as usize] && self.touch(1, id, p)
    }
    fn end(&mut self, id: i32, p: Point) {
        if (0..16).contains(&id) && self.touches[id as usize] {
            self.touch(2, id, p);
            self.touches[id as usize] = false;
        }
    }
    fn cancel_touch(&mut self, id: i32, p: Point) -> bool {
        if !(0..16).contains(&id) || !self.touches[id as usize] || !self.touch(3, id, p) {
            return false;
        }
        self.touches[id as usize] = false;
        true
    }
    fn pointer_world(&mut self, p: Point) -> Option<Point> {
        let plane = self.fresh()?.aim?[1];
        self.ray(p)?.at_plane(plane)
    }
    fn ground_world(&mut self, p: Point) -> Option<Point> {
        let plane = self.fresh()?.frame.hero_render_xyz.values[1].value;
        self.ray(p)?.at_plane(plane)
    }
    fn skill_direction_world(&mut self,p:Point,skill:i32)->Option<Point> {
        let (visual,hero,view,projection,token)={let s=self.fresh()?;(s.direction_visual,s.frame.hero_render_xyz.values.map(|n|n.value),s.frame.view_matrix.values.map(|n|n.value),s.projection,s.frame.token)};
        if let Some(spec)=visual.filter(|v|v.skill==skill) {
            let offset=self.wall_surface.offset_for_input(token.as_str(),hero,spec.wall,p,spec.observed);
            let direction=projection::direction_input(&view,&projection,hero,p,spec,offset)?;
            if self.recorder.is_enabled() {self.recorder.push(now_ns(),"direction_projection",serde_json::json!({"skill":skill,"pointer":[p.x,p.y],"hero":hero,"wall_offset":[offset.x,offset.y],"native_wall_hit":spec.wall.is_some(),"direction":[direction.x,direction.y],"observed":spec.observed,"surface_y":spec.surface_y,"epoch":token.as_str()}));}
            return Some(Point::new(hero[0]+direction.x,hero[2]+direction.y));
        }
        if skill==8410{return None;}
        self.ground_world(p)
    }
    fn movement_direction(&mut self, p: Point) -> Option<Point> {
        let target = self.ground_world(p)?;
        let hero = self.hero_world()?;
        Some(Point::new(target.x - hero.x, -(target.y - hero.y)))
    }
    fn hero_world(&mut self) -> Option<Point> {
        let v = self.fresh()?.frame.hero_render_xyz.values;
        Some(Point::new(v[0].value, v[2].value))
    }
    fn pointer_world_at_plane(&mut self, p: Point, plane: f64) -> Option<Point> {
        self.ray(p)?.at_plane(plane)
    }
    fn capture_pointer_world(&mut self, p: Point) -> u64 {
        let Some(ray) = self.ray(p) else {
            return 0;
        };
        self.next_ray = self.next_ray.wrapping_add(1).max(1);
        self.rays.insert(self.next_ray, ray);
        self.next_ray
    }
    fn resolve_world(&mut self, id: u64) -> Option<Point> {
        self.rays.get(&id).copied()?.at_plane(self.fresh()?.aim?[1])
    }
    fn release_resolver(&mut self, id: u64) {
        self.rays.remove(&id);
    }
    fn navigation(
        &mut self,
        start: Point,
        target: Point,
        avoid: bool,
    ) -> Result<Vec<Point>, String> {
        if let Some(mesh) = &self.mesh {
            mesh.route(start, target, avoid)
        } else if avoid {
            Err("Карта проходимости недоступна".into())
        } else {
            Ok(vec![start, target])
        }
    }
    fn binding(&mut self, hero: i32, button: &Button, config: &str) -> Binding {
        if self.fresh().is_none() { self.settings.clear_native_holds(); }
        let mut binding=self.settings.binding(hero,config);
        if config == "extra:2" && !matches!(button.skillId, 20020 | 20150) {
            binding.cast_mode = match binding.cast_mode {
                2 => 1,
                3 => 0,
                mode => mode,
            };
        }
        if let Some(sample)=self.fresh().filter(|s|s.frame.hero_id.int()==hero) {
            binding.aim_type=*sample.control_types.get(&button.slot).unwrap_or(&0);
        }
        if config == "extra:2" && crate::settings::plain_spell_id(button.skillId) {
            binding.cast_mode = self.settings.integer(&format!("hero.bindings.extra:2:{}.cast_mode",button.skillId), hero);
        }
        if self.conflicts.contains_key(&binding.key) { binding.key=0; }
        binding
    }
    fn cancel_key(&mut self, hero: i32) -> i32 {
        let key=self.settings.integer("hero.cancel_key", hero);
        if self.conflicts.contains_key(&key) {0} else {key}
    }
    fn timer(&mut self, timer: Timer, delay: Option<i64>) {
        let slot = &mut self.timers[timer_index(timer)];
        match delay {
            None => *slot = None,
            Some(d) => {
                let at = now_ns() + d.max(0) * 1_000_000;
                *slot = Some(slot.map_or(at, |old| old.min(at)));
            }
        }
    }
    fn trace(&mut self, state: &Controller, event: &str, reason: &str) {
        if self.recorder.is_enabled() {
            let now=now_ns();
            let repeated=event=="direction_feedback_rejected" && self.last_feedback_trace.as_ref().is_some_and(|(r,at)|r==reason&&now-*at<50_000_000);
            if !repeated {
                if event=="direction_feedback_rejected"{self.last_feedback_trace=Some((reason.into(),now));}
                let v=state.inspect();
                let mut decision=core_decision_diagnostics(&v,event,reason);
                if event.starts_with("selection_") || matches!(event,"cancel_requested"|"cancel_acknowledged")
                    || (event=="release"&&v.selectionOwner_!=0) {
                    use crate::flight_schema::Encode;
                    decision.as_object_mut().unwrap().insert("native_selection".into(),
                        self.fresh().map(|s|s.frame.selection_input.encode()).unwrap_or(serde_json::Value::Null));
                }
                if matches!(event,"context_abort"|"auxiliary_hold") {
                    use crate::flight_schema::Encode;
                    decision.as_object_mut().unwrap().insert("native_auxiliary_controls".into(),
                        self.fresh().map(|s|s.frame.auxiliary_controls.encode()).unwrap_or(serde_json::Value::Null));
                }
                self.recorder.push(now,"core_decision",decision);
            }
        }
        if self.trace {
            let s = state.inspect();
            eprintln!(
                "[CLBB mapper] t={} event={} reason={} hero={} skill={} touch={}",
                now_ns(),
                event,
                reason,
                s.heroId_,
                s.aimSkillId_,
                s.skillTouch_
            );
        }
    }
}
impl crate::linked_gesture::Transport for Adapter {
    fn down(&mut self,p:Point)->i32 {
        if self.touches.iter().filter(|v|**v).count()>=3 {return -1;}
        self.begin(p)
    }
    fn motion(&mut self,id:i32,p:Point)->bool {self.move_touch(id,p)}
    fn up(&mut self,id:i32,p:Point) {self.end(id,p);}
    fn abort(&mut self,id:i32,p:Point)->bool {self.cancel_touch(id,p)}
    fn abort_all(&mut self)->bool {
        Host::abort_all_touches(self)
    }
}
// A hidden cancel widget is not an admission condition for a new gesture.
// Only its real owned endpoints must be available before the native DOWN.
fn linked_geometry(frame: &clbb_moba_core::controller::snapshot::Frame, endpoints: &[String;2]) -> Option<crate::linked_gesture::Geometry> {
    let controls=&frame.ui;
    let point=|slot:&str|controls.buttons.iter().take(controls.button_count.min(32) as usize)
        .find(|b|b.slot.as_str()==slot && b.active.get() && b.press_uv.valid())
        .map(|b|Point::new(b.press_uv.values[0].value,b.press_uv.values[1].value))
        .filter(|p|(0.0..=1.0).contains(&p.x) && (0.0..=1.0).contains(&p.y));
    Some(crate::linked_gesture::Geometry {
        epoch:frame.token.owned(),endpoints:[point(&endpoints[0])?,point(&endpoints[1])?],
        cancel:controls.cancel_uv.valid().then(||Point::new(controls.cancel_uv.values[0].value,controls.cancel_uv.values[1].value)),
    })
}
pub struct Runtime {
    updates: crate::updater::Updates,
    gestures: crate::linked_gesture::Gestures,
    portrait: crate::portrait_gesture::Picker,
    core: Controller,
    host: Adapter,
    worker: Worker,
    last_sample: i64,
    last_reset_generation:u64,
    suspended: bool,
    read: CString,
    error: String,
    recorder: Arc<Recorder>,
    debug_at: i64,
    debug_pointer_at: i64,
    debug_pending_since: i64,
    debug_mismatch_since: i64,
    debug_route_reason: &'static str,
    debug_full_state_at:i64,
    debug_state_key:(i32,i32,u8,u32),
}
impl Runtime {
    fn new(callback: Touch, user: *mut c_void) -> Self {
        let settings = Settings::load();
        let trace = std::env::var_os("CLBB_MAPPER_TRACE").is_some();
        let recorder = Recorder::new();
        let mut v = Self {
            updates:crate::updater::Updates::new(),
            recorder: recorder.clone(), debug_at:0, debug_pointer_at:0, debug_pending_since:0, debug_mismatch_since:0, debug_route_reason:"initial", debug_full_state_at:0, debug_state_key:(0,-1,0,0),
            gestures: Default::default(), portrait:Default::default(),
            core: Controller::new(),
            host: Adapter {
                settings,
                sample: None,
                inverse: None,
                mesh: None,
                callback,
                user,
                touches: [false; 16],
                reset_generation:0,
                rotate_contacts: false, next_contact: 0,
                rays: BTreeMap::new(),
                wall_surface:Default::default(),
                next_ray: 0,
                timers: [None; 5],
                conflicts:Default::default(), conflict_stamp:None,
                trace, recorder:recorder.clone(), last_feedback_trace:None,
            },
            worker: Worker::start(recorder),
            last_sample: 0,
            last_reset_generation:0,
            suspended: false,
            read: CString::default(),
            error: String::new(),
        };
        crate::native_debug::install(&v.recorder);
        v.host.gain();
        v.input(Input::Trace(trace));
        v.configure();
        v
    }
    fn input(&mut self, event: Input<'_>) -> bool {
        if self.recorder.is_enabled() {
            let reason=match &event {Input::Release=>Some("release_all"),Input::Invalid(_)=>Some("invalid_source"),Input::Enabled(false)=>Some("disabled"),_=>None};
            if let Some(reason)=reason{self.recorder.push(now_ns(),"runtime_lifecycle",serde_json::json!({"event":reason}));}
        }
        if matches!(&event,Input::Release|Input::Invalid(_)|Input::Enabled(false)) {
            if Host::abort_all_touches(&mut self.host) {
                self.gestures.forget_after_native_reset();self.portrait.reset();
            } else {
                self.gestures.reset_after_failed_native_abort(&mut self.host);
            }
        }
        let result=self.core.apply_input(&mut self.host,event,now_ns());
        self.reconcile_native_reset();
        result
    }
    fn reconcile_native_reset(&mut self) {
        if self.last_reset_generation!=self.host.reset_generation {
            self.gestures.forget_after_native_reset();self.portrait.reset();
            self.last_reset_generation=self.host.reset_generation;
        }
    }
    fn configure(&mut self) {
        crate::audio::configure(self.host.settings.get("audio.input_device",0),self.host.settings.get("audio.output_device",0));
        let recording=self.host.settings.boolean("debug.recording",0);
        if recording != self.recorder.is_enabled() {self.debug_pending_since=0;self.debug_mismatch_since=0;}
        self.recorder.set_enabled(recording);
        crate::native_debug::enable(recording);
        crate::native_debug::automatic_marks(self.host.settings.boolean("debug.auto_marks",0));
        if recording {self.recorder.set_context(serde_json::json!({"pid":std::process::id(),"runtime_executable":std::env::current_exe().ok(),"version":std::env::var("CLBB_RELEASE_VERSION").unwrap_or_default(),"revision":std::env::var("CLBB_RELEASE_REVISION").unwrap_or_default(),"settings":self.host.settings.values,"schema":2,"expected_channels":["native_transport","host_event","runtime_lifecycle","input_event","input_route","touch_command","android_motion","source","typed_frame","core_decision","native_ui_audit","controller","controller_state","skill_result"]}));}

        self.input(Input::Trace(self.host.trace || recording));
        if recording {self.recorder.push(now_ns(),"settings",serde_json::json!({"values":self.host.settings.values}));}
        let hero=self.host.fresh().map(|s|s.frame.hero_id.int()).unwrap_or(self.core.inspect().heroId_);
        self.configure_movement(hero);
        self.host.refresh_conflicts(hero,true);
        self.input(Input::RefreshBindings);
        self.input(Input::Enabled(
            !self.suspended && self.host.settings.boolean("enabled", 0),
        ));
    }
    fn configure_movement(&mut self, hero:i32) {
        self.input(Input::MovementMode(
            self.host.settings.integer("hero.movement.click_mode", hero),
        ));
        self.input(Input::MovementClickPercent(
            self.host.settings.integer("hero.movement.click_distance_percent", hero),
        ));
        self.input(Input::HoldThreshold(
            self.host.settings.integer("hero.movement.hold_ms", hero) as i64,
        ));
    }
    fn poll(&mut self, width: f64, height: f64) {
        if width >= 32. && height >= 32. {
            *self.worker.size.lock().unwrap() = [width, height];
        }
        let (sample, mesh) = {
            let shared = self.worker.shared.lock().unwrap();
            (shared.sample.clone(), shared.mesh.clone())
        };
        self.host.mesh = mesh;
        if let Some(sample) = sample.filter(|s| s.frame.time_ns.value as i64 != self.last_sample) {
            self.last_sample = sample.frame.time_ns.value as i64;
            if source_context_reset(self.host.sample.as_deref(),&sample).is_some() {
                self.input(Input::Invalid("F7: игровой контекст изменился — касания отменены"));
            }
            let view = sample.frame.view_matrix.values.map(|v| v.value);
            self.host.inverse = projection::inverse_product(&sample.projection, &view);
            let hero_changed=self.host.sample.as_ref().map(|old|old.frame.hero_id.int())!=Some(sample.frame.hero_id.int());
            self.host.sample = Some(sample.clone());
            if hero_changed {self.configure_movement(sample.frame.hero_id.int());}
            if self.host.fresh().is_some() {
                self.host.settings.observe_native_templates(sample.frame.hero_id.int(), &sample.control_types);
                self.host.settings.observe_native_holds(sample.frame.hero_id.int(), &sample.control_holds);
                if sample.frame.skill_state.ready.get()
                    && sample.frame.skill_state.hero_id.int()==sample.frame.hero_id.int()
                    && sample.frame.skill_state.token.as_str()==sample.frame.token.as_str() {
                    let forms=self.host.settings.observed_forms.entry(sample.frame.hero_id.int()).or_default();
                    for v in sample.frame.skill_state.buttons.iter().take(sample.frame.skill_state.count.min(32) as usize) {
                        let owned=sample.frame.ui.buttons.iter().take(sample.frame.ui.button_count.min(32) as usize)
                            .any(|b|b.kind.as_str()=="skill"&&b.slot.as_str()==v.slot.as_str()&&b.skill_id.int()==v.base_skill_id.int());
                        if owned && v.slot.as_str().starts_with("skill:") && v.active_skill_id.int()>0 && forms.len()<32 {
                            let ids=forms.entry((v.slot.owned(),v.base_skill_id.int())).or_default();
                            if ids.len()<16 {ids.insert(v.active_skill_id.int());}
                        }
                    }
                }
            }
            self.host.refresh_conflicts(sample.frame.hero_id.int(),false);
            let skill = sample.frame.skill_id.int();
            self.core.update_observation(
                sample.aim.is_some(),
                sample
                    .aim
                    .map(|p| Point::new(p[0], p[2]))
                    .unwrap_or_default(),
                skill,
            );
            self.core
                .apply_snapshot(&mut self.host, &sample.frame, now_ns());
            self.reconcile_native_reset();
            let nav = self.host.mesh.is_some();
            self.input(Input::Navigation {
                available: nav,
                reason: if nav {
                    "Карта проходимости прочитана"
                } else {
                    "Ожидаю карту проходимости"
                },
            });
        }
        if self.host.fresh().is_none() { self.host.settings.clear_native_holds(); }
        if self.host.fresh().is_none()&&self.host.touches.iter().any(|v|*v) {
            self.input(Input::Invalid("F7: данные игры устарели — касания отменены"));
        }
        let now = now_ns();
        for i in 0..5 {
            if self.host.timers[i].is_some_and(|at| now >= at) {
                self.host.timers[i] = if i == 0 || i == 3 {
                    Some(now + 16_000_000)
                } else {
                    None
                };
                let timer = [
                    Timer::Main,
                    Timer::Pointer,
                    Timer::Tap,
                    Timer::Movement,
                    Timer::Auxiliary,
                ][i];
                self.input(Input::Timer(timer));
            }
        }
        let sample=self.host.sample.clone().filter(|s|now_ns()-s.frame.time_ns.value as i64<=250_000_000);
        let cancel_feedback = sample.as_ref().map(|s| {
            let f = &s.frame;
            let cancel = &f.cancel_state;
            let point = f.ui.cancel_uv;
            crate::linked_gesture::CancelFeedback {
                point: point.valid().then(|| Point::new(point.values[0].value, point.values[1].value)),
                slot: if cancel.ready.get() && cancel.coherent.get() && cancel.cancelled.get()
                    && cancel.token.as_str() == f.token.as_str() {
                    cancel.slot.as_str()
                } else { "" },
                sample_start_ns: cancel.sample_start_ns.get(0.) as i64,
            }
        });
        self.gestures.poll(&mut self.host,sample.as_ref().map(|s|s.frame.token.as_str()).unwrap_or(""),sample.as_ref().and_then(|s|s.linked.as_ref()),sample.as_ref().map(|s|s.frame.skill_id.int()).unwrap_or(0),sample.as_ref().map(|s|s.frame.skill_state.sample_start_ns.value as i64).unwrap_or(0),now,cancel_feedback);
        self.portrait.poll(&mut self.host,sample.as_ref().map(|s|s.frame.token.as_str()).unwrap_or(""),sample.as_ref().and_then(|s|s.portrait.as_ref()),sample.as_ref().map(|s|s.frame.skill_state.sample_start_ns.value as i64).unwrap_or(0),now);
        self.reconcile_native_reset();
        self.debug_observe();
    }
    fn cancel_diagnostics(&self, view: &clbb_moba_core::controller::view::View) -> serde_json::Value {
        use crate::flight_schema::Encode;
        let now=now_ns();let sample=self.host.sample.as_ref();let fresh=self.host.fresh();
        let native=fresh.map(|sample|&sample.frame.cancel_state);
        let native_fresh=native.is_some_and(|state| {
            let start=state.sample_start_ns.get(-1.);let end=state.sample_end_ns.get(-1.);
            state.sample_start_ns.is_number()&&state.sample_end_ns.is_number()
                &&start.is_finite()&&end.is_finite()&&start>=0.&&end>=start
                &&end-start<=100e6&&end<=now as f64+10e6&&now as f64-end<=100e6
        });
        let native_owner_matches=native.is_some_and(|state| {
            state.hero_id.int()==view.heroId_&&!view.frameEpoch_.as_str().is_empty()
                &&state.token.as_str()==view.frameEpoch_.as_str()
        });
        serde_json::json!({
            "available":fresh.is_some(),"now_ns":now,"hero":view.heroId_,"epoch":view.frameEpoch_.as_str(),
            "source":{"present":sample.is_some(),"fresh":fresh.is_some(),
                "sample_start_ns":sample.map(|sample|sample.frame.time_ns.value-sample.frame.sample_duration_ms.value*1e6),
                "sample_end_ns":sample.map(|sample|sample.frame.time_ns.value),
                "age_ms":sample.map(|sample|(now as f64-sample.frame.time_ns.value)/1e6),
                "clock_ms":sample.map(|sample|sample.game_clock_ms)},
            "native":native.map(Encode::encode),"native_fresh":native_fresh,"native_owner_matches":native_owner_matches,
            "native_selection":fresh.map(|sample|sample.frame.selection_input.encode()),
            "native_slot_matches":native.is_some_and(|state|!view.activeSlot_.as_str().is_empty()&&state.slot.as_str()==view.activeSlot_.as_str()),
            "cancel_uv":fresh.map(|sample|sample.frame.ui.cancel_uv.encode()),
            "cancel_uv_valid":fresh.is_some_and(|sample|sample.frame.ui.cancel_uv.valid()),
            "controller":{"pending":view.skillCancelPending_!=0,"sent_ns":view.skillCancelSentNs_,
                "acknowledged":view.skillCancelAcknowledged_!=0,"touch":view.skillTouch_,"key":view.skillKey_,
                "slot":view.activeSlot_.as_str(),"skill":view.coreSkill_,"cast_start_ns":view.castStartedNs_,
                "sent_age_ms":(view.skillCancelSentNs_>0).then(||(now-view.skillCancelSentNs_)as f64/1e6),
                "cancel_point_ready":view.cancelReady_!=0,"cancel_point":[view.cancelPoint_.x,view.cancelPoint_.y]}
        })
    }
    fn contact_diagnostics(&self)->serde_json::Value {
        serde_json::json!({"rotation":self.host.rotate_contacts,"next":self.host.next_contact,
            "active":self.host.touches.iter().enumerate().filter_map(|(id,held)|held.then_some(id)).collect::<Vec<_>>()})
    }
    /// One immutable controller View and the serial runtime's current contact
    /// inventory. Native Source freshness remains explicit, never inferred from
    /// the controller's cached acceptance flags or worker publication counters.
    fn controller_diagnostics(&self,view:&clbb_moba_core::controller::view::View)->serde_json::Value {
        let sample=self.host.sample.as_ref();
        serde_json::json!({"schema":1,"atomic":true,"snapshot_ns":now_ns(),
            "hero":view.heroId_,"epoch":view.frameEpoch_.as_str(),
            "source":{"present":sample.is_some(),"fresh":self.host.fresh().is_some(),
                "hero":sample.map(|s|s.frame.hero_id.int()),"epoch":sample.map(|s|s.frame.token.as_str()),
                "owner_matches_view":sample.map(|s|s.frame.hero_id.int()==view.heroId_
                    &&!view.frameEpoch_.as_str().is_empty()&&s.frame.token.as_str()==view.frameEpoch_.as_str())},
            "input":self.input_diagnostics(view),"aim":self.aim_diagnostics(view),"cancel":self.cancel_diagnostics(view),
            "contacts":self.contact_diagnostics(),
            "cast":{"key":view.skillKey_,"held":view.skillKeyHeld_,"mode":view.skillCastMode_,"touch":view.skillTouch_,
                "slot":view.activeSlot_.as_str(),"start_ns":view.castStartedNs_,"previous_end_ns":view.lastSkillEndNs_,
                "sample_start_ns":view.frameCoreSampleStartNs_,"touch_sequence":view.touchSequence_,
                "transport_sequence":view.verifiedTransportSequence_,"cancel_pending":view.skillCancelPending_,
                "cancel_ack":view.skillCancelAcknowledged_,"binding_variant":view.skillBindingVariant_,
                "cooldown_variant":view.skillCooldownVariant_}})
    }
    fn input_diagnostics(&self, view: &clbb_moba_core::controller::view::View) -> serde_json::Value {
        let hero=view.heroId_;
        serde_json::json!({"hero":hero,"ready":view.ready_,"enabled":view.enabled_,"skill":view.coreSkill_,"touch":view.skillTouch_,"aim":view.aimReady_,"pending_release":view.pendingRelease_,"logical":view.logicalMode_,"source":view.logicalSource_.as_str(),"pointer":[view.pointer_.x,view.pointer_.y],"status":view.status_.as_str(),"navigation":view.navigationAvailable_,"reason":view.inputReason_.as_str(),"accepted":view.acceptedInput_,"raw":[view.traceRaw_.x,view.traceRaw_.y],"decoded":[view.traceDecoded_.x,view.traceDecoded_.y],"sent":[view.skillPoint_.x,view.skillPoint_.y],"direct":view.directGeometryFresh_,"pending_seq":view.pendingGoalSequence_,"matched_seq":view.traceMatchedSequence_,"source_aim":self.host.sample.as_ref().is_some_and(|s|s.aim.is_some()),"epoch":view.frameEpoch_.as_str(),"plan_revision":self.host.sample.as_ref().map(|s|s.plan_revision).unwrap_or(0),"direction_accepted":view.directionAccepted_,"direction_fresh":view.directionGeometryFresh_,"direction_matched_seq":view.directionMatchedSequence_,"movement_touch":view.movementTouch_,"movement_held":view.movementHeld_,"movement_pressed":view.movementPressed_,"movement_ready":view.movementReady_,"movement_status":view.movementStatus_.as_str()})
    }
    fn aim_diagnostics(&self, view: &clbb_moba_core::controller::view::View) -> serde_json::Value {
                let now = now_ns();
                let sample = self.host.sample.as_ref();
                let current_world = self.host.ray(view.pointer_).and_then(|ray| ray.at_plane(sample?.aim?[1]));
                let worker = self.worker.shared.lock().unwrap();
                let directional = serde_json::json!({
                    "hero_world":[view.frameHero_.x,view.frameHero_.y],
                    "pending_direction_valid":view.pendingDirectionValid_,
                    "pending_direction":[view.pendingDirection_.x,view.pendingDirection_.y],
                    "direction_observed":[view.directionObserved_.x,view.directionObserved_.y],
                    "direction_accepted":view.directionAccepted_,"direction_fresh":view.directionGeometryFresh_,
                    "direction_matched_seq":view.directionMatchedSequence_,

                    "direction_goal_time_ns":view.directionGoalTime_,
                    "direction_goal_seq":view.directionGoalSequence_,
                    "direction_sample_start_ns":view.directionSampleStart_,
                    "direction_sample_end_ns":view.directionSampleEnd_,"pending_seq":view.pendingGoalSequence_
                });
                let mut value = serde_json::json!({
                    "now_ns":now,"frame_end_ns":view.lastFrameNs_,
                    "source_age_ms":sample.map(|s|(now as f64-s.frame.time_ns.value)/1e6),
                    "clock_ms":sample.map(|s|s.game_clock_ms),
                    "read_ms":worker.source_ms,"published":worker.published_samples,"rejected":worker.rejected_samples,
                    "max_publish_gap_ms":worker.max_publish_gap_ms,"max_navigation_ms":worker.max_navigation_ms,
                    "logical_start_ns":view.traceSampleStart_,"logical_end_ns":view.traceSampleEnd_,
                    "logical_fresh":view.logicalSampleFresh_,"accepted_ns":view.acceptedSampleNs_,
                    "accepted_point":[view.acceptedPoint_.x,view.acceptedPoint_.y],
                    "skill_point":[view.skillPoint_.x,view.skillPoint_.y],
                    "origin":[view.logicalOrigin_.x,view.logicalOrigin_.y],
                    "offset":[view.logicalObserved_.x,view.logicalObserved_.y],
                    "current_cursor_world":current_world.map(|p|[p.x,p.y]),
                    "pending_world_valid":view.pendingWorldValid_,"pending_world":[view.pendingWorld_.x,view.pendingWorld_.y],
                    "desired_valid":view.traceDesiredValid_,"desired":[view.traceDesired_.x,view.traceDesired_.y],
                    "pending_time_ns":view.pendingGoalTime_,
                    "matched_seq":view.traceMatchedSequence_,"sent_seq":view.touchSequence_,"sent_time_ns":(view.sentTimeNs_>0).then_some(view.sentTimeNs_),"q_arming":view.qDragArming_,"q_recomputed":view.qDragRecomputeUsed_,
                    "selection":{"owner":format!("0x{:x}",view.selectionOwner_),"revoked":view.selectionRevoked_,
                        "kind":view.selectionKind_,"backend_owner":format!("0x{:x}",view.selectionBackendOwner_),
                        "contact_owner":format!("0x{:x}",view.selectionContactOwner_),"target_guid":view.selectionTargetGuid_,
                        "target_owner":format!("0x{:x}",view.selectionTargetOwner_),"target_self":format!("0x{:x}",view.selectionTargetSelf_),
                        "target_invalid":view.selectionTargetInvalid_,
                        "goal_target_guid":view.selectionGoalTargetGuid_,"goal_target_owner":format!("0x{:x}",view.selectionGoalTargetOwner_),
                        "goal_target_self":format!("0x{:x}",view.selectionGoalTargetSelf_),
                        "geometry_fresh":view.selectionGeometryFresh_,"accepted":view.selectionAccepted_,
                        "matched_seq":view.selectionMatchedSequence_,"sample_start_ns":view.selectionSampleStartNs_,
                        "sample_end_ns":view.selectionSampleEndNs_,"option":view.selectionOption_,"goal_seq":view.selectionGoalSequence_,
                        "captured":view.selectionCaptured_,"end_clock":crate::flight_schema::Encode::encode(&view.selectionEndClock_),"end_sample_ns":view.selectionEndSampleNs_},
                    "range":view.direct.world_range,"center":[view.joystickCenter_.x,view.joystickCenter_.y],
                    "radius":[view.joystickRadius_.x,view.joystickRadius_.y],
                    "skill":view.coreSkill_,"touch":view.skillTouch_,"pending_release":view.pendingRelease_,
                    "reason":view.inputReason_.as_str()
                });
                value.as_object_mut().unwrap().extend(directional.as_object().unwrap().clone());
        value
            
    }
    fn debug_observe(&mut self) {
        let now=now_ns();
        if !self.recorder.is_enabled() || now-self.debug_at<8_000_000 { return; }
        self.debug_at=now;
        let v=self.core.inspect();
        use crate::flight_schema::Encode;
        let state_key=(v.coreSkill_,v.skillTouch_,v.pendingRelease_,v.queue_depth);
        if state_key!=self.debug_state_key || now-self.debug_full_state_at>=32_000_000 {
            self.debug_state_key=state_key;self.debug_full_state_at=now;
            self.recorder.push(now,"controller_state",v.encode());
        }
        self.recorder.push(now,"controller",serde_json::json!({"render":crate::render_metrics::snapshot(),"queue":{"depth":v.queue_depth,"front_slot":v.queue_front_slot.as_str(),"front_key":v.queue_front_key,"front_held":v.queue_front_held},"cast":{"key":v.skillKey_,"held":v.skillKeyHeld_,"mode":v.skillCastMode_,"start_ns":v.castStartedNs_,"previous_end_ns":v.lastSkillEndNs_,"sample_start_ns":v.frameCoreSampleStartNs_,"touch_sequence":v.touchSequence_,"transport_sequence":v.verifiedTransportSequence_,"cancel_pending":v.skillCancelPending_,"cancel_ack":v.skillCancelAcknowledged_,"binding_variant":v.skillBindingVariant_,"cooldown_variant":v.skillCooldownVariant_},"input":self.input_diagnostics(&v),"aim":self.aim_diagnostics(&v),"contacts":self.host.touches.iter().enumerate().filter_map(|(i,v)|v.then_some(i)).collect::<Vec<_>>(),"portrait_picker":self.portrait,"native_portrait_picker":self.host.fresh().and_then(|s|s.portrait.as_ref()),"linked_gestures":self.gestures.active,"linked_ignored":self.gestures.ignored_keys() }));
        if v.pendingRelease_!=0 { if self.debug_pending_since==0 {self.debug_pending_since=now;} }
        else {self.debug_pending_since=0;}
        let delta=v.direct.clamped_world(v.traceDesired_)-v.logicalObserved_;
        let mismatch=v.skillTouch_>=0 && v.acceptedInput_!=0 && v.traceDesiredValid_!=0 && v.logicalSampleFresh_!=0 && v.direct.ready!=0
            && delta.x.hypot(delta.y)> (v.direct.world_range*0.02).max(0.08);
        if mismatch {if self.debug_mismatch_since==0{self.debug_mismatch_since=now;}}else{self.debug_mismatch_since=0;}
        if self.host.settings.boolean("debug.auto_marks",0) {
            let zero_native=self.host.sample.as_ref().is_some_and(|sample|sample.frame.direction_input.ready.get() && sample.frame.direction_input.raw_touch_xyz.values.iter().all(|v|v.value==0.));
            let reason=if v.skillTouch_>=0 && v.castStartedNs_>0 && now-v.castStartedNs_>50_000_000 && v.directionGeometryFresh_!=0 && v.directionAccepted_==0 && zero_native {Some("active_native_touch_zero_over_50ms")}
                else if self.debug_pending_since!=0 && now-self.debug_pending_since>100_000_000 {Some("release_wait_over_100ms")}
                else if primary_cancel_wait_over_100ms(&v,now) {Some("primary_cancel_wait_over_100ms")}
                else if self.gestures.active.iter().any(|g|g.cancel_requested_ns.is_some_and(|t|now-t>100_000_000)) {Some("linked_cancel_wait_over_100ms")}
                else if v.skillTouch_>=0 && now-v.lastFrameNs_>100_000_000 {Some("active_source_stale_over_100ms")}
                else if self.debug_mismatch_since!=0 && now-self.debug_mismatch_since>150_000_000 {Some("sustained_aim_discrepancy")}
                else {None};
            if let Some(reason)=reason {self.recorder.mark(now,reason,true);}
        }
    }
    fn event(&mut self, kind:i32,code:i32,down:bool,repeat:bool,p:Point)->bool {
        let at=now_ns();
        let result=self.event_inner(kind,code,down,repeat,p);
        if self.recorder.is_enabled() && kind!=0 {
            let v=self.core.inspect();
            self.recorder.push(now_ns(),"input_route",serde_json::json!({"input_ns":at,"kind":kind,"code":code,"down":down,"repeat":repeat,"consumed":result,"route":self.debug_route_reason,"suspended":self.suspended,"enabled":v.enabled_,"ready":v.ready_,"active_skill":v.coreSkill_,"touch":v.skillTouch_,"held":v.skillKeyHeld_,"cast_mode":v.skillCastMode_,"pending_release":v.pendingRelease_}));
        }
        result
    }
    fn event_inner(&mut self, kind: i32, code: i32, down: bool, repeat: bool, p: Point) -> bool {
        self.debug_route_reason="core_key_dispatch";
        if self.recorder.is_enabled() && (kind!=0 || now_ns()-self.debug_pointer_at>=4_000_000) {
            self.debug_pointer_at=now_ns();self.recorder.push(self.debug_pointer_at,"input_event",serde_json::json!({"kind":kind,"code":code,"down":down,"repeat":repeat,"pointer":[p.x,p.y]}));
        }
        if kind == 0 {
            self.debug_route_reason="pointer";self.portrait.pointer=p;
            if !self.suspended {
                self.input(Input::Pointer(p));
            }
            return false;
        }
        if kind==1 && code!=0 && self.recorder.is_enabled() && code==self.host.settings.integer("debug.mark_shortcut",0) {
            self.debug_route_reason="manual_debug_marker";
            if down && !repeat {self.recorder.mark(now_ns(),"manual",false);} return true;
        }
        // LMB always follows ATL's ordinary physical event path.
        if kind == 2 && code == 1 {
            self.debug_route_reason="native_left_mouse_passthrough";
            return false;
        }
        if self.suspended {
            self.debug_route_reason="mapper_suspended";
            return false;
        }
        let key = if kind == 2 {
            crate::settings::MOUSE | code
        } else {
            code
        };
        let hero=self.core.inspect().heroId_;
        let active_match=self.core.inspect().ready_!=0 && self.host.fresh().is_some();
        if active_match && key!=0 && self.host.conflicts.contains_key(&key) && down {
            self.debug_route_reason="conflicting_binding_disabled";
            return true;
        }
        if self.portrait.contains(key){
            self.debug_route_reason="portrait_held_key";
            if !down&&!repeat{self.portrait.pointer=p;self.portrait.release(key,now_ns());}return true;
        }
        if down&&!repeat&&key!=0&&key==self.host.settings.integer("hero.cancel_key",hero){self.portrait.cancel(now_ns());}
        if down&&!repeat&&active_match {
            if let Some(sample)=self.host.sample.clone(){if let Some(o)=&sample.portrait {
                if o.active==10431&&key!=0&&key==self.host.settings.binding(hero,&o.slot).key {
                    let button=sample.frame.ui.buttons.iter().take(sample.frame.ui.button_count.min(32)as usize).find(|b|b.slot.as_str()==o.slot&&b.active.get()&&b.press_uv.valid());
                    if let Some(button)=button.filter(|_|sample.frame.hero_screen_uv.valid()) {
                        let v=sample.frame.hero_screen_uv.values;let b=button.press_uv.values;
                        self.debug_route_reason="native_portrait_picker";
                        return self.portrait.begin(&mut self.host,key,sample.frame.token.as_str(),o,Point::new(v[0].value,v[1].value),Point::new(b[0].value,b[1].value),p,now_ns());
                    }
                }
            }}
        }
        let keyboard=self.host.settings.integer("hero.movement.click_mode",hero)==2;
        if keyboard && (active_match || !down) {
            for (direction,name) in ["up","down","left","right"].iter().enumerate() {
                if key!=0 && key==self.host.settings.integer(&format!("hero.movement.{name}"),hero) {
                    self.debug_route_reason="keyboard_movement";
                    return if repeat {true} else {self.input(Input::MovementKey {direction,down})};
                }
            }
        }
        if !keyboard && key != 0 && key == self.host.settings.integer("hero.movement.button", hero) {
            self.debug_route_reason="movement_binding";
            return self.input(Input::Mouse {
                button: 2,
                down,
                point: p,
            });
        }
        if self.gestures.contains(key) {
            self.debug_route_reason="linked_existing_gesture";
            if !down && !repeat {self.gestures.release(&mut self.host,key);}
            return true;
        }
        let hero=self.core.inspect().heroId_;
        if down && !repeat && key!=0 && key==self.host.settings.integer("hero.cancel_key",hero) {
            self.gestures.cancel(now_ns());
        }
        if down && !repeat && self.core.inspect().ready_!=0 && self.host.settings.boolean("enabled",0) {
            if let Some(sample)=self.host.sample.clone().filter(|s|now_ns()-s.frame.time_ns.value as i64<=250_000_000) {
                if let Some(link)=sample.linked.as_ref() {
                    for origin in 0..2 {
                        let binding=self.host.settings.binding(hero,&format!("combo:{origin}"));
                        if key==0 || key!=binding.key {continue;}
                        let geometry=linked_geometry(&sample.frame,&link.endpoints);
                        self.recorder.push(now_ns(),"linked_admission",serde_json::json!({"key":key,"origin":origin,"passive":link.enough_energy,"parent_cooldown":link.on_cooldown[origin],"endpoints_valid":geometry.is_some(),"cancel_available":geometry.as_ref().is_some_and(|g|g.cancel.is_some()),"reason":if geometry.is_none(){"missing_endpoints"}else if !link.enough_energy{"passive_not_ready"}else{"native_down"}}));
                        if let Some(geometry)=geometry {
                            self.debug_route_reason="linked_gesture_admission";
                            return self.gestures.begin(&mut self.host,key,origin,geometry,link,now_ns());
                        }
                        self.debug_route_reason="linked_missing_geometry";
                        return true;
                    }
                }
            }
        }
        self.input(Input::Pointer(p));
        self.input(Input::Key {
            code: key,
            down,
            repeat,
        })
    }
    fn hero_controls(&self) -> String {
        let Some(sample) = self.host.fresh().filter(|s|s.frame.ready.get()&&s.frame.hero_id.int()>0&&s.frame.ui.ready.get()&&s.frame.ui.hero_id.int()==s.frame.hero_id.int()) else { return String::new(); };
        let hero=sample.frame.hero_id.int();
        let mut text=format!("1\t{hero}\n");
        let mut seen=std::collections::BTreeSet::new();
        let linked=sample.linked.is_some();
        for b in sample.frame.ui.buttons.iter().take(sample.frame.ui.button_count.min(32) as usize)
            .chain(sample.frame.ui.capability_controls.iter().take(sample.frame.ui.capability_count.min(32) as usize)) {
            if !matches!(b.kind.as_str(),"skill"|"extra"|"attack"|"upgrade"|"quick_buy") || !b.active.get() || !b.press_uv.valid() || !seen.insert(b.slot.as_str()) {continue;}
            let slot=b.slot.as_str();
            if linked && matches!(slot,"skill:2"|"skill:3") {continue;}
            let variant=sample.frame.skill_state.buttons.iter().take(sample.frame.skill_state.count.min(32) as usize)
                .find(|v|v.slot.as_str()==slot&&v.base_skill_id.int()==b.skill_id.int()).map(|v|v.active_skill_id.int()).filter(|v|*v>0).unwrap_or(b.skill_id.int());
            let config=slot;
            let kind=sample.control_types.get(slot).copied().unwrap_or(0);
            let mode_config=if slot=="extra:2"&&crate::settings::plain_spell_id(b.skill_id.int()) {format!("extra:2:{}",b.skill_id.int())}else{config.into()};
            text.push_str(&format!("{slot}\t{config}\t{mode_config}\t{}\t{variant}\t{kind}\n",b.skill_id.int()));
            if b.kind.as_str()=="skill" && !crate::settings::hold_duration_skill(hero,slot) {
                let mut forms=self.host.settings.observed_forms.get(&hero).and_then(|m|m.get(&(slot.to_owned(),b.skill_id.int()))).cloned().unwrap_or_default();
                if variant>0 {forms.insert(variant);}
                if hero==25 {match slot {"skill:1"=>{forms.extend([2520,2521]);},"skill:2"=>{forms.extend([2530,2531]);},_=>{}}}
                if hero==84&&slot=="skill:1" {forms.extend([8420,8425]);}
                for id in forms.into_iter().filter(|id|*id!=b.skill_id.int()).take(15) {
                    text.push_str(&format!("{slot}\t{slot}:{id}\t{slot}:{id}\t{}\t{id}\t{kind}\n",b.skill_id.int()));
                }
            }
        }
        if linked {
            for n in 0..2 {text.push_str(&format!("combo:{n}\tcombo:{n}\tcombo:{n}\t0\t0\t1\n"));}
        }
        text
    }
    fn portrait_preview(&self)->String {
        use std::fmt::Write;
        let Some(sample)=self.host.fresh().filter(|s|s.frame.hero_id.int()==104)else{return String::new();};
        let backend=sample.portrait.as_ref().map(|p|p.backend).or_else(||self.portrait.held.as_ref().filter(|h|h.epoch==sample.frame.token.as_str()).map(|h|h.backend));
        let Some(backend)=backend else{return String::new();};
        let mut out=format!("{}:{backend:x}\n",sample.frame.token.as_str());
        if let Some(o)=&sample.portrait {for p in &o.portraits {
            let _=writeln!(out,"{} {} {} {} {} {}",p.guid,p.center[0]-p.radius[0],p.center[1]-p.radius[1],p.radius[0]*2.,p.radius[1]*2.,i32::from(p.rendered));
        }}
        out
    }
    fn read(&mut self, key: &str) -> *const c_char {
        if self.host.fresh().is_none() { self.host.settings.clear_native_holds(); }
        let view = self.core.inspect();
        let hero = view.heroId_;
        let v=match key {
            "audio.input_devices"|"audio.output_devices"|"audio.status"|"audio.capture_status"|"audio.playback_status"=>crate::audio::read(key),
            "hero.id"=>hero.to_string(),"hero.name"=>hero_display_name(hero),
            "status"|"diagnostics.status"=>if !self.error.is_empty(){self.error.clone()}else if !self.host.settings.boolean("enabled",0){"Управление CLBB выключено".into()}else if self.host.fresh().is_none(){"Ожидаю матч и кнопки героя".into()}else if self.suspended{"Профиль готов · управление приостановлено".into()}else{view.status_.owned()},
            "diagnostics.controller"=>self.controller_diagnostics(&view).to_string(),
            "diagnostics.input"=>self.input_diagnostics(&view).to_string(),
            "portrait.overlay"=>if !self.suspended&&self.host.fresh().is_some_and(|s|s.frame.hero_id.int()==104&&self.portrait.held.as_ref().is_some_and(|h|h.epoch==s.frame.token.as_str())){self.portrait.overlay()}else{String::new()},
            "portrait.preview"=>self.portrait_preview(),
            "portrait.visible"=>self.portrait.held.is_some().to_string(),
            "diagnostics.portrait"=>serde_json::json!({"gesture":self.portrait,"native":self.host.fresh().and_then(|s|s.portrait.as_ref())}).to_string(),
            "diagnostics.aim"=>self.aim_diagnostics(&view).to_string(),
            "diagnostics.cancel"=>self.cancel_diagnostics(&view).to_string(),
            "diagnostics.native_frame"=>self.host.fresh().map(|sample| {
                use crate::flight_schema::Encode;
                serde_json::json!({"fresh":true,"frame":sample.frame.encode(),
                    "hero":sample.frame.hero_id.int(),"epoch":sample.frame.token.as_str(),
                    "game_clock_ms":sample.game_clock_ms,"plan_revision":sample.plan_revision,
                    "control_holds":sample.control_holds,"types":sample.control_types}).to_string()
            }).unwrap_or_else(||"{\"fresh\":false}".into()),
            "diagnostics.cast_lock" => self.host.fresh().map(|sample| {
                let state = &sample.frame.skill_state;
                let buttons: Vec<_> = state.buttons.iter().take(state.count as usize).map(|button| {
                    let locked = button.cast_locked.is_bool().then(|| button.cast_locked.get());
                    let state_forbidden = button.state_forbidden.is_bool().then(|| button.state_forbidden.get());
                    serde_json::json!({"slot": button.slot.as_str(), "form": button.active_skill_id.int(), "locked": locked, "state_forbidden": state_forbidden})
                }).collect();
                serde_json::json!({
                    "epoch": state.token.as_str(), "hero": state.hero_id.int(),
                    "start_ns": state.sample_start_ns.value, "end_ns": state.sample_end_ns.value,
                    "buttons": buttons,
                }).to_string()
            }).unwrap_or_else(|| "{}".into()),
            k if k.starts_with("updates.")=>self.updates.read(k),
            "diagnostics.fps"=>format!("{:.0}",crate::render_metrics::fps()),
            "diagnostics.render"=>crate::render_metrics::snapshot().to_string(),
            "diagnostics.direction_visual"=>self.host.fresh().map(|s|serde_json::json!({"spec":s.direction_visual,"hero":s.frame.hero_render_xyz.values.map(|v|v.value),"view":s.frame.view_matrix.values.map(|v|v.value),"projection":s.projection,"sample_start_ns":s.frame.direction_input.sample_start_ns.value,"sample_end_ns":s.frame.direction_input.sample_end_ns.value}).to_string()).unwrap_or_else(||"{}".into()),
            "diagnostics.hero_state"=>self.host.fresh().map(|s| {
                let state=&s.frame.skill_state;
                let form=state.buttons.iter().take(state.count.min(32) as usize).find(|b|b.slot.as_str()=="skill:1").map(|b|b.active_skill_id.int()).unwrap_or(0);
                let name=if state.hero_id.int()==84 {match form {8420=>"ground",8425=>"wall",_=>"unknown"}} else {"unknown"};
                serde_json::json!({"hero":state.hero_id.int(),"state":name,"form":form,"source":"owned skill:1 active form","epoch":state.token.as_str(),"sample_start_ns":state.sample_start_ns.value,"sample_end_ns":state.sample_end_ns.value}).to_string()
            }).unwrap_or_else(||"{\"state\":\"unknown\"}".into()),
            "diagnostics.templates"=>self.host.fresh().map(|s|serde_json::json!({"hero":s.frame.hero_id.int(),"types":s.control_types,"holds":s.control_holds}).to_string()).unwrap_or_else(||"{}".into()),
            "diagnostics.controls" => self.host.fresh().filter(|s| s.frame.hero_id.int() == view.heroId_).map(|sample| {
                let native = |b: &clbb_moba_core::controller::snapshot::Button| serde_json::json!({
                    "slot": b.slot.as_str(), "kind": b.kind.as_str(), "index": b.index.int(),
                    "skill_id": b.skill_id.int(), "available": b.active.get(),
                    "press_valid": b.press_uv.valid(), "press": b.press_uv.values.map(|n| n.value),
                    "geometry_source": b.geometry_source.as_str(),
                });
                let buttons: Vec<_> = view.buttons.iter().take(view.button_count.min(32) as usize).map(|b| serde_json::json!({
                    "slot": b.slot.as_str(), "kind": b.kind.as_str(), "index": b.index,
                    "skill_id": b.skill_id, "key": b.key, "cast_mode": b.cast_mode,
                    "available": b.available != 0, "press": [b.press.x, b.press.y],
                    "geometry_source": b.geometry_source.as_str(),
                })).collect();
                serde_json::json!({
                    "ready": view.ready_ != 0, "hero": sample.frame.hero_id.int(), "epoch": sample.frame.token.as_str(),
                    "sample_start_ns": sample.frame.time_ns.value - sample.frame.sample_duration_ms.value * 1e6,
                    "sample_end_ns": sample.frame.time_ns.value,
                    "types": sample.control_types, "holds": sample.control_holds, "buttons": buttons,
                    "native_buttons": sample.frame.ui.buttons.iter().take(sample.frame.ui.button_count.min(32) as usize).map(native).collect::<Vec<_>>(),
                    "capability_controls": sample.frame.ui.capability_controls.iter().take(sample.frame.ui.capability_count.min(32) as usize).map(native).collect::<Vec<_>>(),
                }).to_string()
            }).unwrap_or_else(|| "{\"ready\":false}".into()),
            "hero.controls"=>self.hero_controls(),
            "bindings.conflicts"=>self.host.conflicts.iter().map(|(key,names)|format!("{key}\t{}\n",names.join(", "))).collect::<String>(),
            "common.battle_spell_id"=>self.host.fresh().and_then(|s|s.frame.ui.buttons.iter().take(s.frame.ui.button_count.min(32) as usize).find(|b|b.kind.as_str()=="extra"&&b.slot.as_str()=="extra:2")).map(|b|b.skill_id.int()).unwrap_or(0).to_string(),
            "hero.linked_controls"=>if self.host.fresh().is_some_and(|s|s.linked.is_some()) {"1".into()}else{"0".into()},
            "hero.skill0_type"|"hero.skill1_type"|"hero.skill2_type"|"hero.skill3_type"=>{
                let slot=format!("skill:{}",key.as_bytes()[10]as char);
                self.host.fresh().and_then(|s|s.control_types.get(&slot)).unwrap_or(&0).to_string()
            },
            "hero.skill0_selection_kind"|"hero.skill1_selection_kind"|"hero.skill2_selection_kind"|"hero.skill3_selection_kind"=>{
                let slot=format!("skill:{}",key.as_bytes()[10]as char);
                self.host.fresh().map(|s|&s.frame.selection_input)
                    .filter(|s|s.slot.as_str()==slot&&s.kind.is_number()&&matches!(s.kind.int(),1|2))
                    .map(|s|s.kind.int()).unwrap_or(0).to_string()
            },
            "diagnostics.gestures"=>serde_json::json!({"active":self.gestures.active,"ignored":self.gestures.ignored_keys(),"linked":self.host.fresh().and_then(|s|s.linked.as_ref()),"cancel_uv":self.host.fresh().map(|s|serde_json::json!({"valid":s.frame.ui.cancel_uv.valid(),"point":s.frame.ui.cancel_uv.values.map(|v|v.value)})),"buttons":self.host.fresh().map(|s|s.frame.ui.buttons.iter().take(s.frame.ui.button_count.min(32) as usize).filter(|b|b.kind.as_str()=="skill").map(|b|serde_json::json!({"slot":b.slot.as_str(),"active":b.active.get(),"press_valid":b.press_uv.valid(),"press":b.press_uv.values.map(|v|v.value)})).collect::<Vec<_>>())}).to_string(),
            "diagnostics.contacts"=>self.contact_diagnostics().to_string(),
            "debug.status"=>self.recorder.status(),
            "diagnostics.source_ms"=>format!("{:.3}",self.worker.shared.lock().unwrap().source_ms),
            "diagnostics.navigation"=>view.navigationStatus_.owned(),_=>self.host.settings.get(key,hero),
        };
        self.read = CString::new(v.replace('\0', "")).unwrap_or_default();
        self.read.as_ptr()
    }
    fn write(&mut self, key: &str, value: &str) -> bool {
        if key=="audio.refresh" {crate::audio::refresh();return true;}
        if matches!(key,"updates.check"|"updates.install"){return matches!(value,"1"|"true")&&self.updates.request(key); }
        if key=="debug.mark" {return self.recorder.mark(now_ns(),value,false);}
        if key=="diagnostics.contact_rotation" {
            if self.host.touches.iter().any(|v|*v) {return false;}
            return match value {
                "true"|"1"=>{self.host.rotate_contacts=true;true},
                "false"|"0"=>{self.host.rotate_contacts=false;true},
                _=>false,
            };
        }
        if key == "diagnostics.trace" {
            return match value {
                "true" | "1" => {
                    self.host.trace = true;
                    self.input(Input::Trace(true));
                    true
                }
                "false" | "0" => {
                    self.host.trace = false;
                    self.input(Input::Trace(false));
                    true
                }
                _ => false,
            };
        }
        let hero = self.core.inspect().heroId_;
        match self.host.settings.set(key, value, hero) {
            Ok(()) => {
                self.error.clear();
                self.input(Input::Release);
                self.host.gain();
                self.configure();
                true
            }
            Err(e) => {
                self.error = e;
                false
            }
        }
    }
    fn suspend(&mut self, value: bool) {
        if self.suspended != value {
            if self.recorder.is_enabled(){self.recorder.push(now_ns(),"runtime_lifecycle",serde_json::json!({"event":"suspend_changed","suspended":value}));}
            self.suspended = value;
            self.input(Input::Release);
            self.configure();
        }
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        crate::native_debug::enable(false);
        self.recorder.set_enabled(false);
        self.input(Input::Release);
        self.worker.stop.store(true, Ordering::Relaxed);
    }
}
// Pointers below are created and retained by the native GTK adapter. Dispatch is
// GTK-thread only; it excludes reentry while a synchronous touch callback runs.
#[unsafe(no_mangle)]
pub extern "C" fn clbb_mapper_create(callback: Touch, user: *mut c_void) -> *mut Runtime {
    Box::into_raw(Box::new(Runtime::new(callback, user)))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clbb_mapper_destroy(rt: *mut Runtime) {
    if !rt.is_null() {
        unsafe {
            drop(Box::from_raw(rt));
        }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clbb_mapper_poll(rt: *mut Runtime, width: f64, height: f64) {
    if let Some(rt) = unsafe { rt.as_mut() } {
        rt.poll(width, height);
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clbb_mapper_event(
    rt: *mut Runtime,
    kind: i32,
    code: i32,
    down: i32,
    repeat: i32,
    x: f64,
    y: f64,
) -> i32 {
    unsafe { rt.as_mut() }
        .is_some_and(|rt| rt.event(kind, code, down != 0, repeat != 0, Point::new(x, y))) as i32
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clbb_mapper_read(rt: *mut Runtime, key: *const c_char) -> *const c_char {
    if rt.is_null() || key.is_null() {
        return c"".as_ptr();
    }
    let key = unsafe { CStr::from_ptr(key) }.to_str().unwrap_or("");
    unsafe { &mut *rt }.read(key)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clbb_mapper_write(
    rt: *mut Runtime,
    key: *const c_char,
    value: *const c_char,
) -> i32 {
    if rt.is_null() || key.is_null() || value.is_null() {
        return 0;
    }
    let key = unsafe { CStr::from_ptr(key) }.to_str().unwrap_or("");
    let value = unsafe { CStr::from_ptr(value) }.to_str().unwrap_or("");
    unsafe { &mut *rt }.write(key, value) as i32
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clbb_mapper_suspend(rt: *mut Runtime, value: i32) {
    if let Some(rt) = unsafe { rt.as_mut() } {
        rt.suspend(value != 0);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_backoff_keeps_replans_fast_but_expires_old_success() {
        let start = Instant::now();
        assert_eq!(
            source_cadence(None, start, Duration::from_millis(1)),
            (false, Duration::from_millis(750))
        );
        for age in [0, 2, 100, 250] {
            assert_eq!(
                source_cadence(
                    Some(start),
                    start + Duration::from_millis(age),
                    Duration::from_micros(500)
                ),
                (true, Duration::from_micros(1500))
            );
        }
        assert_eq!(
            source_cadence(
                Some(start),
                start + Duration::from_millis(251),
                Duration::from_micros(500)
            ),
            (false, Duration::from_millis(750))
        );
        // A fresh match immediately leaves the slow retry cadence.
        let next = start + Duration::from_secs(2);
        assert_eq!(
            source_cadence(Some(next), next, Duration::from_millis(1)),
            (true, Duration::from_millis(1))
        );
    }
    #[test]
    fn failed_slow_discovery_always_yields_and_active_overrun_does_not_add_latency() {
        let now = Instant::now();
        let slow = Duration::from_secs(2);
        assert_eq!(source_cadence(None, now, slow), (false, Duration::from_millis(750)));
        assert_eq!(source_cadence(Some(now), now, slow), (true, Duration::ZERO));
    }
    unsafe extern "C" fn record(user: *mut c_void, action: i32, id: i32, x: f64, y: f64) -> i32 {
        unsafe { &mut *(user as *mut Vec<(i32, i32, f64, f64)>) }.push((action, id, x, y));
        1
    }
    fn fixture(events: &mut Vec<(i32, i32, f64, f64)>) -> Runtime {
        let recorder=Recorder::new();
        Runtime {
            updates:crate::updater::Updates::new(),
            gestures: Default::default(), portrait:Default::default(),
            core: Controller::new(),
            host: Adapter {
                settings: Settings::default(),
                sample: None,
                inverse: None,
                mesh: None,
                callback: record,
                user: events as *mut _ as *mut c_void,
                touches: [false; 16],
                reset_generation:0,
                rotate_contacts: false, next_contact: 0,
                rays: BTreeMap::new(),
                wall_surface:Default::default(),
                next_ray: 0,
                timers: [None; 5],
                conflicts:Default::default(), conflict_stamp:None,
                trace: false, recorder:recorder.clone(), last_feedback_trace:None,
            },
            worker: Worker {
                shared: Arc::new(Mutex::new(Shared {
                    sample: None,
                    mesh: None,
                    error: String::new(),
                    source_ms: 0.,
                    ..Default::default()
                })),
                stop: Arc::new(AtomicBool::new(false)),
                size: Arc::new(Mutex::new([1280., 720.])),
            },
            last_sample: 0,
            last_reset_generation:0,
            suspended: false,
            read: CString::default(),
            error: String::new(),
            recorder, debug_at:0, debug_pointer_at:0, debug_pending_since:0, debug_mismatch_since:0, debug_route_reason:"initial", debug_full_state_at:0, debug_state_key:(0,-1,0,0),
        }
    }
    fn template_sample(hero: i32, kind: u8, at: i64) -> Sample {
        use crate::source::{flag, number, text, v2};
        let mut frame = clbb_moba_core::controller::snapshot::Frame::default();
        frame.ready = flag(true);
        frame.hero_id = number(hero as f64);
        frame.token = text("native-template-test");
        frame.token_string = flag(true);
        frame.token_nonempty = flag(true);
        frame.ui.ready = flag(true);
        frame.ui.hero_id = number(hero as f64);
        frame.ui.resolution_px = v2([1280., 720.]);
        frame.ui.button_count = 1;
        let button = &mut frame.ui.buttons[0];
        button.slot = text("skill:2");
        button.kind = text("skill");
        button.index = number(2.);
        button.skill_id = number((hero * 100 + 30) as f64);
        button.active = flag(true);
        button.press_uv = v2([0.8, 0.8]);
        button.center_uv = button.press_uv;
        button.radius_uv = v2([0.1, 0.1]);
        frame.time_ns = number(at as f64);
        frame.sample_duration_ms = number(0.001);
        frame.ui.time_ns = frame.time_ns;
        Sample {
            plan_revision: 1, game_clock_ms: 1, match_owners: [1, 2, 3, 4], frame,
            projection: [0.; 16], logic: [0.; 2], indicator: 0,
            aim: None, direction_visual: None,
            control_types: BTreeMap::from([("skill:2".into(), kind)]), control_holds: BTreeMap::new(), linked: None, portrait:None,
        }
    }
    #[test]
    fn portrait_display_survives_optional_capture_gap_but_not_match_change() {
        use crate::portrait_picker::{Observation,Portrait};use crate::source::text;
        let mut events=Vec::new();let mut r=fixture(&mut events);let at=now_ns();
        let mut sample=template_sample(104,1,at);
        let o=Observation {backend:77,slot:"skill:2".into(),base:10430,active:10431,held:false,selected:-1,camera_guid:0,
            portraits:vec![Portrait{index:0,guid:900,owner:88,rendered:true,native_cancel_entry:false,native_collider:true,center:[0.9,0.6],radius:[0.02,0.025]}],
            resolution:[4000.,3111.],cancel_point:None,native_cancel:None,contact_owner:0,contact_point:None};
        sample.portrait=Some(o.clone());r.host.sample=Some(Arc::new(sample.clone()));
        r.portrait.begin(&mut r.host,69,sample.frame.token.as_str(),&o,Point::new(0.5,0.55),Point::new(0.9,0.8),Point::new(0.2,0.3),at);
        let read=|r:&mut Runtime,key:&str|unsafe{CStr::from_ptr(r.read(key))}.to_str().unwrap().to_owned();
        assert_eq!(read(&mut r,"portrait.overlay").lines().count(),1,"ready at key DOWN");
        sample.portrait=None;r.host.sample=Some(Arc::new(sample.clone()));
        assert_eq!(read(&mut r,"portrait.overlay").lines().count(),1,"display is independent of an optional native capture gap");
        assert!(!read(&mut r,"portrait.preview").is_empty(),"same GPU cache identity remains bound to this hold");
        assert_eq!(events.len(),1,"presentation authorizes no MOVE or UP");
        sample.frame.token=text("other-match");r.host.sample=Some(Arc::new(sample));
        assert!(read(&mut r,"portrait.overlay").is_empty());assert!(read(&mut r,"portrait.preview").is_empty());
    }
    #[test]
    fn stale_foreign_hero_forms_cannot_pollute_the_current_hero_settings() {
        use crate::source::{flag,number,text};
        let mut events=Vec::new();let mut r=fixture(&mut events);
        for case in 0..4 {
            let mut sample=template_sample(18,2,now_ns());
            sample.frame.skill_state.ready=flag(true);sample.frame.skill_state.hero_id=number(if case==0{84.}else{18.});
            sample.frame.skill_state.token=if case==1{text("other-match")}else{sample.frame.token};
            sample.frame.skill_state.count=1;sample.frame.skill_state.buttons[0]=clbb_moba_core::controller::snapshot::Variant{
                slot:text("skill:2"),base_skill_id:number(if case==2{8430.}else{1830.}),active_skill_id:number(1831.),..Default::default()};
            r.worker.shared.lock().unwrap().sample=Some(Arc::new(sample));r.poll(1280.,720.);
            assert_eq!(r.host.settings.observed_forms.get(&18).is_some_and(|v|v.get(&("skill:2".into(),1830)).is_some_and(|s|s.contains(&1831))),case==3);
        }
    }
    #[test]
    fn conflicts_disable_both_skill_and_keyboard_without_dispatching_a_touch() {
        let mut events=Vec::new(); let mut r=fixture(&mut events);
        let mut sample=template_sample(25,2,now_ns());
        sample.frame.ui.buttons[0].slot=crate::source::text("skill:1");
        sample.control_types=BTreeMap::from([("skill:1".into(),2)]);
        r.worker.shared.lock().unwrap().sample=Some(Arc::new(sample));
        r.poll(1280.,720.);
        r.host.settings.set("movement.click_mode","2",0).unwrap();
        r.host.refresh_conflicts(25,true);
        let conflicts=r.host.binding_conflicts(25);
        assert_eq!(conflicts.get(&87).unwrap().len(),2);
        let b=Button {slot:"skill:1".into(),kind:"skill".into(),skillId:2520,..Default::default()};
        assert_eq!(r.host.binding(25,&b,"skill:1").key,0);
        assert!(r.event_inner(1,87,true,false,Point::default()));
        assert_eq!(r.debug_route_reason,"conflicting_binding_disabled");
        assert!(events.is_empty());
        r.host.settings.set("hero.movement.up","16777235",25).unwrap();
        r.host.refresh_conflicts(25,true);
        assert!(!r.host.binding_conflicts(25).contains_key(&87));
        assert_eq!(r.host.binding(25,&b,"skill:1").key,87);
        r.host.settings.set("hero.movement.down","65",25).unwrap();
        assert_eq!(r.host.binding_conflicts(25).get(&65).unwrap().len(),2);
    }
    #[test]
    fn cached_match_conflicts_never_capture_menu_typing_after_source_is_lost() {
        let mut events=Vec::new();let mut r=fixture(&mut events);
        let mut sample=template_sample(25,2,now_ns());
        sample.frame.ui.buttons[0].slot=crate::source::text("skill:1");
        r.host.settings.set("movement.click_mode","2",0).unwrap();
        r.worker.shared.lock().unwrap().sample=Some(Arc::new(sample));
        r.poll(1280.,720.);
        assert!(r.host.conflicts.contains_key(&87));
        r.host.sample=None;
        assert!(!r.event_inner(1,87,true,false,Point::default()));
        assert!(events.is_empty());
    }
    #[test]
    fn known_multi_form_actions_expose_two_versions_inside_one_role() {
        for (hero,slot,base,alt) in [(25,"skill:1",2520,2521),(25,"skill:2",2530,2531),(84,"skill:1",8420,8425)] {
            let mut events=Vec::new();let mut r=fixture(&mut events);
            let mut sample=template_sample(hero,2,now_ns());
            sample.frame.ui.buttons[0].slot=crate::source::text(slot);
            sample.frame.ui.buttons[0].skill_id=crate::source::number(base as f64);
            r.host.sample=Some(Arc::new(sample));
            let layout=r.hero_controls();
            assert!(layout.contains(&format!("{slot}\t{slot}\t{slot}\t{base}")));
            assert!(layout.contains(&format!("{slot}\t{slot}:{alt}\t{slot}:{alt}\t{base}")));
        }
    }
    #[test]
    fn hold_duration_skills_have_one_ui_row_and_observed_forms_are_scoped_by_base_skill() {
        let mut events=Vec::new();let mut r=fixture(&mut events);
        for (hero,n,base) in [(126,0,12610),(126,1,12630),(126,2,12650),(78,0,7810)] {
            let slot=format!("skill:{n}");let mut sample=template_sample(hero,2,now_ns());
            sample.frame.ui.buttons[0].slot=crate::source::text(&slot);
            sample.frame.ui.buttons[0].skill_id=crate::source::number(base as f64);
            r.host.settings.observed_forms.entry(hero).or_default().insert((slot.clone(),base),[base+1,base+2,7871].into());
            r.host.sample=Some(Arc::new(sample));let rows=r.hero_controls();
            assert_eq!(rows.lines().count(),2,"{rows}");
            assert!(!rows.contains(&format!("{slot}:")));
        }
        let sample=template_sample(18,2,now_ns());
        r.host.settings.observed_forms.entry(18).or_default().insert(("skill:2".into(),2530),[2531].into());
        r.host.settings.observed_forms.entry(18).or_default().insert(("skill:2".into(),1830),[1831].into());
        r.host.sample=Some(Arc::new(sample));let rows=r.hero_controls();
        assert!(!rows.contains("2531"));assert!(rows.contains("skill:2:1831"));
    }
    #[test]
    fn hero_controls_exposes_only_fresh_present_buttons_and_current_forms() {
        let mut events=Vec::new();let mut r=fixture(&mut events);
        assert!(r.hero_controls().is_empty());
        let mut sample=template_sample(73,2,now_ns());
        r.host.sample=Some(Arc::new(sample.clone()));
        let layout=r.hero_controls();assert!(layout.starts_with("1\t73\n"));
        assert!(layout.contains("skill:2\tskill:2\tskill:2\t7330\t7330\t2"));
        assert!(!layout.contains("skill:3"));assert!(!layout.contains("extra:3"));
        sample.frame.ui.hero_id=crate::source::number(25.);
        r.host.sample=Some(Arc::new(sample.clone()));assert!(r.hero_controls().is_empty());
        sample.frame.ui.hero_id=crate::source::number(73.);
        sample.frame.ui.ready=crate::source::flag(false);
        r.host.sample=Some(Arc::new(sample.clone()));assert!(r.hero_controls().is_empty());
        sample.frame.ui.ready=crate::source::flag(true);
        sample.frame.ui.buttons[0].active=crate::source::flag(false);
        r.host.sample=Some(Arc::new(sample.clone()));assert_eq!(r.hero_controls(),"1\t73\n");
        sample.frame.ui.buttons[0].active=crate::source::flag(true);
        sample.frame.time_ns=crate::source::number((now_ns()-300_000_000)as f64);
        r.host.sample=Some(Arc::new(sample));assert!(r.hero_controls().is_empty());
    }
    #[test]
    fn polling_installs_native_defaults_before_core_binding_and_menu_reads() {
        let mut events = Vec::new();
        let mut runtime = fixture(&mut events);
        for (kind, mode) in [(3, 0), (1, 0), (2, 0), (0, 0)] {
            runtime.worker.shared.lock().unwrap().sample = Some(Arc::new(template_sample(30, kind, now_ns())));
            runtime.poll(1280., 720.);
            let view = runtime.core.inspect();
            assert_eq!(view.button_count, 1);
            assert_eq!(view.buttons[0].cast_mode, mode);
            assert_eq!(runtime.host.settings.integer("hero.bindings.skill:2.cast_mode", 30), mode);
            let button = Button { slot: "skill:2".into(), kind: "skill".into(), skillId: 3030, ..Default::default() };
            let binding = runtime.host.binding(30, &button, "skill:2");
            assert_eq!(binding.cast_mode, mode);
            assert_eq!(binding.aim_type, kind);
        }
        assert!(runtime.host.settings.values.is_empty());
    }
    #[test]
    fn stale_sample_cannot_install_a_new_native_cast_default() {
        let mut events = Vec::new();
        let mut runtime = fixture(&mut events);
        runtime.worker.shared.lock().unwrap().sample = Some(Arc::new(template_sample(30, 3, now_ns() - 500_000_000)));
        runtime.poll(1280., 720.);
        assert_eq!(runtime.host.settings.binding(30, "skill:2").cast_mode, 0);
    }
    #[test]
    fn native_charge_keeps_release_default_and_preserves_explicit_quick_cast() {
        let mut events = Vec::new(); let mut runtime = fixture(&mut events);
        for charge in [Some(true), Some(false), None] {
            let mut sample = template_sample(30, 1, now_ns());
            if let Some(charge) = charge { sample.control_holds.insert("skill:2".into(), charge); }
            runtime.worker.shared.lock().unwrap().sample = Some(Arc::new(sample));
            runtime.poll(1280., 720.);
            let expected = 0;
            assert_eq!(runtime.core.inspect().buttons[0].cast_mode, expected);
            assert_eq!(runtime.host.settings.integer("hero.bindings.skill:2.cast_mode", 30), expected);
        }
        runtime.host.settings.set("hero.bindings.skill:2.cast_mode", "1", 30).unwrap();
        let mut sample = template_sample(30, 1, now_ns());
        sample.control_holds.insert("skill:2".into(), true);
        runtime.worker.shared.lock().unwrap().sample = Some(Arc::new(sample));
        runtime.poll(1280., 720.);
        assert_eq!(runtime.core.inspect().buttons[0].cast_mode, 1);
    }
    #[test]
    fn stale_charge_observation_is_revoked_for_both_binding_and_menu() {
        let mut events = Vec::new(); let mut runtime = fixture(&mut events);
        let mut sample = template_sample(30, 1, now_ns());
        sample.control_holds.insert("skill:2".into(), true);
        runtime.worker.shared.lock().unwrap().sample = Some(Arc::new(sample));
        runtime.poll(1280., 720.);
        assert_eq!(runtime.host.settings.binding(30, "skill:2").cast_mode, 0);
        runtime.host.sample = Some(Arc::new(template_sample(30, 1, now_ns() - 500_000_000)));
        let button = Button { slot: "skill:2".into(), kind: "skill".into(), skillId: 3030, ..Default::default() };
        assert_eq!(runtime.host.binding(30, &button, "skill:2").cast_mode, 0);
        assert_eq!(runtime.host.settings.integer("hero.bindings.skill:2.cast_mode", 30), 0);
        runtime.host.settings.observe_native_holds(30, &BTreeMap::from([("skill:2".into(), true)]));
        let value = runtime.read("hero.bindings.skill:2.cast_mode");
        assert_eq!(unsafe { CStr::from_ptr(value) }.to_str().unwrap(), "0");
    }
    #[test]
    fn control_diagnostics_show_current_native_and_mapped_state_but_never_stale_cache() {
        let mut events = Vec::new(); let mut runtime = fixture(&mut events);
        let mut sample = template_sample(30, 1, now_ns());
        sample.control_holds.insert("skill:2".into(), true);
        let cap = &mut sample.frame.ui.capability_controls[0];
        cap.slot = crate::source::text("upgrade:2"); cap.kind = crate::source::text("upgrade");
        cap.index = crate::source::number(2.); cap.active = crate::source::flag(false);
        sample.frame.ui.capability_count = 1;
        runtime.worker.shared.lock().unwrap().sample = Some(Arc::new(sample)); runtime.poll(1280., 720.);
        let pointer = runtime.read("diagnostics.controls");
        let current: serde_json::Value = serde_json::from_str(unsafe { CStr::from_ptr(pointer) }.to_str().unwrap()).unwrap();
        assert_eq!(current["ready"], true); assert_eq!(current["hero"], 30);
        assert_eq!(current["types"]["skill:2"], 1); assert_eq!(current["holds"]["skill:2"], true);
        assert_eq!(current["buttons"][0]["slot"], "skill:2");
        assert_eq!(current["buttons"][0]["key"], 69); assert_eq!(current["buttons"][0]["cast_mode"], 0);
        assert_eq!(current["native_buttons"][0]["skill_id"], 3030);
        assert_eq!(current["capability_controls"][0]["slot"], "upgrade:2");
        assert_eq!(current["capability_controls"][0]["available"], false);
        runtime.host.sample = Some(Arc::new(template_sample(30, 1, now_ns() - 500_000_000)));
        let pointer = runtime.read("diagnostics.controls");
        let stale: serde_json::Value = serde_json::from_str(unsafe { CStr::from_ptr(pointer) }.to_str().unwrap()).unwrap();
        assert_eq!(stale, serde_json::json!({"ready": false}));
    }
    #[test]
    fn native_frame_diagnostic_uses_full_typed_source_only_while_fresh() {
        let mut events=Vec::new();let mut runtime=fixture(&mut events);
        let mut sample=template_sample(30,3,now_ns());
        sample.game_clock_ms=42;sample.frame.game_clock_ms=crate::source::number(42.);
        sample.control_holds.insert("skill:2".into(),true);
        runtime.host.sample=Some(Arc::new(sample));
        let pointer=runtime.read("diagnostics.native_frame");
        let current:serde_json::Value=serde_json::from_str(unsafe{CStr::from_ptr(pointer)}.to_str().unwrap()).unwrap();
        assert_eq!(current["fresh"],true);assert_eq!(current["hero"],30);
        assert_eq!(current["game_clock_ms"],42);assert_eq!(current["frame"]["game_clock_ms"]["value"],42.);
        assert_eq!(current["frame"]["cancel_state"]["cancelled"]["value"],-1);
        assert_eq!(current["frame"]["ui"]["buttons"][0]["slot"]["value"],"skill:2");
        assert_eq!(current["control_holds"]["skill:2"],true);assert_eq!(current["types"]["skill:2"],3);
        runtime.host.sample=Some(Arc::new(template_sample(30,3,now_ns()-500_000_000)));
        let pointer=runtime.read("diagnostics.native_frame");
        let stale:serde_json::Value=serde_json::from_str(unsafe{CStr::from_ptr(pointer)}.to_str().unwrap()).unwrap();
        assert_eq!(stale,serde_json::json!({"fresh":false}));
    }
    #[test]
    fn aim_diagnostics_report_exact_sent_command_sequence_without_mutating_state() {
        let mut events=Vec::new();let runtime=fixture(&mut events);
        let mut view=clbb_moba_core::controller::view::View::default();
        view.touchSequence_=u64::MAX;view.traceMatchedSequence_=7;view.traceSampleStart_=9_007_199_254_740_993;
        view.sentTimeNs_=9_007_199_254_740_991;
        let current=runtime.aim_diagnostics(&view);
        assert_eq!(current["sent_seq"].as_u64(),Some(u64::MAX));
        assert_eq!(current["matched_seq"],7);
        assert_eq!(current["sent_time_ns"].as_i64(),Some(9_007_199_254_740_991));
        assert_eq!(current["logical_start_ns"].as_i64(),Some(9_007_199_254_740_993));
        assert_eq!(view.touchSequence_,u64::MAX);
        view.sentTimeNs_=0;
        assert!(runtime.aim_diagnostics(&view)["sent_time_ns"].is_null());
    }
    #[test]
    fn charge_decisions_keep_transient_proof_without_expanding_ordinary_input_rows() {
        let mut view=clbb_moba_core::controller::view::View::default();
        view.chargeStage_=6;view.chargeBase_=12620;view.chargeFuture_=12621;
        view.chargeEndClock_=crate::source::number(u32::MAX as f64);
        view.chargeEndSampleNs_=9_007_199_254_740_993;view.chargeLastSampleNs_=9_007_199_254_740_999;
        for (event,reason) in [("charge_transition","native_form_cycle_and_two_empty_game_ticks"),
            ("charge_tracking_revoked","charge_owner_or_policy_changed"),("release","auto_completed")] {
            let row=core_decision_diagnostics(&view,event,reason);
            assert_eq!(row["event"],event);assert_eq!(row["charge_stage"],6);
            assert_eq!(row["charge_base"],12620);assert_eq!(row["charge_future"],12621);
            assert_eq!(row["charge_end_clock"]["kind"],1);
            assert_eq!(row["charge_end_clock"]["value"],u32::MAX as f64);
            assert_eq!(row["charge_end_sample_ns"].as_i64(),Some(9_007_199_254_740_993));
            assert_eq!(row["charge_last_sample_ns"].as_i64(),Some(9_007_199_254_740_999));
        }
        for (event,reason) in [("pointer_flush","latest_value"),("release","cast")] {
            assert!(core_decision_diagnostics(&view,event,reason).get("charge_stage").is_none());
        }
        view.chargeEndClock_=Default::default();
        assert_eq!(core_decision_diagnostics(&view,"charge_tracking_revoked","unavailable")["charge_end_clock"]["kind"],0);
    }
    #[test]
    fn cancel_diagnostics_keep_source_native_clock_owner_and_controller_flags_independent() {
        use crate::source::{flag,number,text,v2};
        let mut events=Vec::new();let mut runtime=fixture(&mut events);let at=now_ns();
        let mut sample=template_sample(30,2,at);
        sample.frame.ui.cancel_uv=v2([0.9,0.2]);
        sample.frame.cancel_state=clbb_moba_core::controller::snapshot::CancelState {
            ready:flag(true),coherent:flag(true),source:text("UIKeySkill.bCancelSkill"),
            token:sample.frame.token,hero_id:number(30.),slot:text("skill:1"),base_skill_id:number(3020.),
            cancelled:flag(true),sample_start_ns:number((at-2_000_000)as f64),sample_end_ns:number((at-1_000_000)as f64),
        };
        let mut view=clbb_moba_core::controller::view::View::default();
        view.heroId_=30;view.frameEpoch_=sample.frame.token;view.activeSlot_=text("skill:1");
        view.skillCancelPending_=1;view.skillCancelSentNs_=at-120_000_000;view.skillTouch_=2;view.skillKey_=87;
        runtime.host.sample=Some(Arc::new(sample.clone()));
        let current=runtime.cancel_diagnostics(&view);
        assert_eq!(current["available"],true);assert_eq!(current["source"]["fresh"],true);
        assert_eq!(current["native_fresh"],true);assert_eq!(current["native_owner_matches"],true);
        assert_eq!(current["native_slot_matches"],true);assert_eq!(current["native"]["cancelled"]["value"],1);
        assert_eq!(current["native"]["source"]["value"],"UIKeySkill.bCancelSkill");
        assert_eq!(current["controller"]["pending"],true);assert_eq!(current["controller"]["acknowledged"],false);
        assert_eq!(current["cancel_uv_valid"],true);
        sample.frame.cancel_state.sample_start_ns=number((at-151_000_000)as f64);
        sample.frame.cancel_state.sample_end_ns=number((at-150_000_000)as f64);
        runtime.host.sample=Some(Arc::new(sample.clone()));
        let stale_native=runtime.cancel_diagnostics(&view);
        assert_eq!(stale_native["source"]["fresh"],true);assert_eq!(stale_native["native_fresh"],false);
        sample.frame.cancel_state.sample_start_ns=number((at-2_000_000)as f64);
        sample.frame.cancel_state.sample_end_ns=number((at-1_000_000)as f64);
        sample.frame.cancel_state.token=text("other-match");runtime.host.sample=Some(Arc::new(sample.clone()));
        let foreign=runtime.cancel_diagnostics(&view);
        assert_eq!(foreign["native_fresh"],true);assert_eq!(foreign["native_owner_matches"],false);
        sample.frame.time_ns=number((at-500_000_000)as f64);runtime.host.sample=Some(Arc::new(sample));
        let stale_source=runtime.cancel_diagnostics(&view);
        assert_eq!(stale_source["available"],false);assert_eq!(stale_source["source"]["present"],true);
        assert_eq!(stale_source["source"]["fresh"],false);assert!(stale_source["native"].is_null());
        assert_eq!(stale_source["native_fresh"],false);assert_eq!(stale_source["controller"]["pending"],true);
    }
    #[test]
    fn atomic_controller_diagnostics_keep_one_view_direction_receipt_cast_owner_and_contacts() {
        use crate::source::text;
        let mut events=Vec::new();let mut runtime=fixture(&mut events);let at=now_ns();
        let mut sample=template_sample(30,2,at);sample.frame.token=text("atomic-match");
        runtime.host.sample=Some(Arc::new(sample));runtime.host.touches[2]=true;runtime.host.touches[7]=true;
        let mut view=clbb_moba_core::controller::view::View::default();
        view.heroId_=30;view.frameEpoch_=text("atomic-match");view.coreSkill_=3020;view.skillTouch_=7;
        view.activeSlot_=text("skill:1");view.skillKey_=87;view.skillKeyHeld_=1;view.skillCastMode_=0;
        view.castStartedNs_=at-10_000_000;view.directionAccepted_=1;view.directionGeometryFresh_=1;
        view.directionMatchedSequence_=9_007_199_254_740_993;view.touchSequence_=view.directionMatchedSequence_;
        view.sentTimeNs_=at-3_000_000;view.directionSampleStart_=at-2_000_000;view.directionSampleEnd_=at-1_000_000;
        view.directionObserved_=Point::new(0.6,0.8);view.skillCancelPending_=1;
        let actual_core=runtime.core.inspect();assert_ne!(actual_core.heroId_,view.heroId_);
        let row=runtime.controller_diagnostics(&view);
        assert_eq!(row["schema"],1);assert_eq!(row["atomic"],true);assert!(row["snapshot_ns"].as_i64().unwrap()>=at);
        assert_eq!(row["hero"],30);assert_eq!(row["epoch"],"atomic-match");
        assert_eq!(row["input"]["hero"],row["hero"]);assert_eq!(row["input"]["epoch"],row["epoch"]);
        assert_eq!(row["input"]["skill"],row["aim"]["skill"]);assert_eq!(row["input"]["touch"],row["aim"]["touch"]);
        for field in ["direction_accepted","direction_fresh","direction_matched_seq"] {assert_eq!(row["input"][field],row["aim"][field]);}
        assert_eq!(row["aim"]["direction_matched_seq"].as_u64(),Some(view.touchSequence_));
        assert_eq!(row["aim"]["sent_seq"].as_u64(),Some(view.touchSequence_));assert_eq!(row["aim"]["sent_time_ns"],view.sentTimeNs_);
        assert_eq!(row["aim"]["direction_sample_start_ns"],view.directionSampleStart_);assert_eq!(row["aim"]["direction_observed"],serde_json::json!([0.6,0.8]));
        assert_eq!(row["cast"]["start_ns"],view.castStartedNs_);assert_eq!(row["cast"]["key"],87);assert_eq!(row["cast"]["held"],1);
        assert_eq!(row["cast"]["touch"],7);assert_eq!(row["contacts"]["active"],serde_json::json!([2,7]));
        assert_eq!(row["cancel"]["controller"]["pending"],true);assert_eq!(row["cancel"]["controller"]["touch"],7);
        assert_eq!(row["source"]["fresh"],true);assert_eq!(row["source"]["owner_matches_view"],true);
        assert_eq!(runtime.core.inspect().heroId_,actual_core.heroId_);assert!(events.is_empty());
        assert!(runtime.host.touches[2]&&runtime.host.touches[7]);
        // A serialized integer sequence is never rounded through a float.
        let decoded:serde_json::Value=serde_json::from_str(&row.to_string()).unwrap();
        assert_eq!(decoded["aim"]["sent_seq"].as_u64(),Some(view.touchSequence_));
    }
    #[test]
    fn atomic_controller_diagnostics_report_source_mismatch_staleness_and_absence_independently() {
        use crate::source::{text,number};
        let mut events=Vec::new();let mut runtime=fixture(&mut events);let at=now_ns();
        let mut view=clbb_moba_core::controller::view::View::default();view.heroId_=30;view.frameEpoch_=text("view-match");view.directionAccepted_=1;
        let mut foreign=template_sample(40,2,at);foreign.frame.token=text("foreign-match");runtime.host.sample=Some(Arc::new(foreign.clone()));
        let row=runtime.controller_diagnostics(&view);assert_eq!(row["atomic"],true);assert_eq!(row["input"]["direction_accepted"],1);
        assert_eq!(row["source"]["hero"],40);assert_eq!(row["source"]["epoch"],"foreign-match");assert_eq!(row["source"]["fresh"],true);
        assert_eq!(row["source"]["owner_matches_view"],false);
        foreign.frame.time_ns=number((at-500_000_000)as f64);runtime.host.sample=Some(Arc::new(foreign));
        let stale=runtime.controller_diagnostics(&view);assert_eq!(stale["source"]["present"],true);assert_eq!(stale["source"]["fresh"],false);
        assert_eq!(stale["cancel"]["available"],false);assert!(stale["cancel"]["native"].is_null());
        runtime.host.sample=None;
        let missing=runtime.controller_diagnostics(&view);assert_eq!(missing["source"]["present"],false);assert_eq!(missing["source"]["fresh"],false);
        assert!(missing["source"]["owner_matches_view"].is_null());assert!(missing["source"]["hero"].is_null());assert!(missing["source"]["epoch"].is_null());
        let pointer=runtime.read("diagnostics.controller");let endpoint:serde_json::Value=serde_json::from_str(unsafe{CStr::from_ptr(pointer)}.to_str().unwrap()).unwrap();
        assert_eq!(endpoint["atomic"],true);assert_eq!(endpoint["hero"],endpoint["input"]["hero"]);
        assert_eq!(endpoint["input"]["touch"],endpoint["aim"]["touch"]);assert_eq!(endpoint["contacts"]["active"],serde_json::json!([]));assert!(events.is_empty());
    }
    #[test]
    fn primary_cancel_wait_mark_requires_a_sent_move_and_more_than_100ms() {
        let mut view=clbb_moba_core::controller::view::View::default();let sent=1_000_000_000;
        view.skillCancelPending_=1;
        assert!(!primary_cancel_wait_over_100ms(&view,sent+200_000_000));
        view.skillCancelSentNs_=sent;
        assert!(!primary_cancel_wait_over_100ms(&view,sent+100_000_000));
        assert!(primary_cancel_wait_over_100ms(&view,sent+100_000_001));
        view.skillCancelPending_=0;
        assert!(!primary_cancel_wait_over_100ms(&view,sent+200_000_000));
    }
    #[test]
    fn resetting_an_unknown_cancel_contact_cancels_the_native_context_without_casting() {
        let mut events=Vec::new();let mut runtime=fixture(&mut events);
        let movement=runtime.host.begin(Point::new(0.2,0.8));
        let geometry=crate::linked_gesture::Geometry {epoch:"match".into(),endpoints:[Point::new(0.7,0.8),Point::new(0.9,0.6)],cancel:None};
        let observed=crate::linked_gesture::Observation {endpoints:["skill:0".into(),"skill:1".into()],source_slot:None,pressed_slot:None,enough_energy:true,choices:[0,0],base_skills:[13320,13310],on_cooldown:[false,false]};
        runtime.gestures.begin(&mut runtime.host,87,0,geometry,&observed,1);
        assert_eq!(runtime.host.touches.iter().filter(|v|**v).count(),2);
        runtime.input(Input::Release);
        assert!(runtime.gestures.active.is_empty());
        assert!(!runtime.host.touches[movement as usize]);
        assert!(runtime.host.touches.iter().all(|v|!*v));
        assert!(events.iter().any(|e|e.0==4));
        assert!(!events.iter().any(|e|e.0==2));
    }
    #[test]
    fn empty_adapter_ledger_never_emits_native_context_cancel() {
        let mut events=Vec::new();let mut runtime=fixture(&mut events);
        assert!(!Host::abort_all_touches(&mut runtime.host));
        runtime.input(Input::Release);runtime.input(Input::Invalid("no owned mapper contacts"));
        assert!(events.is_empty(),"raw native UI has no Adapter owner and must remain untouched");
        assert_eq!(runtime.host.reset_generation,0);
    }
    #[test]
    fn global_release_retires_core_and_linked_once_without_old_up_or_rearm() {
        let mut events=Vec::new();let mut runtime=fixture(&mut events);let at=now_ns();
        let sample=template_sample(30,2,at);
        runtime.host.settings.observe_native_templates(30,&sample.control_types);
        runtime.host.sample=Some(Arc::new(sample.clone()));
        runtime.input(Input::Enabled(true));runtime.core.apply_snapshot(&mut runtime.host,&sample.frame,now_ns());
        runtime.input(Input::Key{code:69,down:true,repeat:false});
        assert!(runtime.core.inspect().skillTouch_>=0);
        let geometry=crate::linked_gesture::Geometry {epoch:"native-template-test".into(),endpoints:[Point::new(0.7,0.8),Point::new(0.9,0.6)],cancel:None};
        let observed=crate::linked_gesture::Observation {endpoints:["skill:0".into(),"skill:1".into()],source_slot:None,pressed_slot:None,enough_energy:true,choices:[0,0],base_skills:[13320,13310],on_cooldown:[false,false]};
        runtime.gestures.begin(&mut runtime.host,87,0,geometry.clone(),&observed,at);
        runtime.host.begin(Point::new(0.2,0.8));
        runtime.input(Input::Release);
        assert_eq!(events.iter().filter(|e|e.0==4).count(),1);
        assert!(!events.iter().any(|e|e.0==2));assert!(runtime.host.touches.iter().all(|v|!*v));
        assert_eq!(runtime.core.inspect().skillTouch_,-1);assert!(runtime.gestures.active.is_empty());
        let count=events.len();
        runtime.input(Input::Timer(Timer::Pointer));runtime.input(Input::Timer(Timer::Main));
        runtime.gestures.begin(&mut runtime.host,87,0,geometry.clone(),&observed,at+1);
        assert_eq!(events.len(),count,"retired held key cannot automatically allocate another contact");
        runtime.gestures.release(&mut runtime.host,87);
        runtime.gestures.begin(&mut runtime.host,87,0,geometry,&observed,at+2);
        assert_eq!(events.len(),count+1,"a new physical press is allowed after UP");
    }
    #[test]
    fn authoritative_context_change_retires_linked_even_without_any_core_contact() {
        let mut events=Vec::new();let mut runtime=fixture(&mut events);let at=now_ns();
        let previous=template_sample(30,2,at-1_000_000);
        let mut current=template_sample(30,2,at);current.frame.token=crate::source::text("next-native-match");
        runtime.host.sample=Some(Arc::new(previous));
        let geometry=crate::linked_gesture::Geometry {epoch:"native-template-test".into(),endpoints:[Point::new(0.7,0.8),Point::new(0.9,0.6)],cancel:None};
        let observed=crate::linked_gesture::Observation {endpoints:["skill:0".into(),"skill:1".into()],source_slot:None,pressed_slot:None,enough_energy:true,choices:[0,0],base_skills:[13320,13310],on_cooldown:[false,false]};
        runtime.gestures.begin(&mut runtime.host,87,0,geometry,&observed,at);
        runtime.worker.shared.lock().unwrap().sample=Some(Arc::new(current));
        runtime.poll(1280.,720.);
        assert_eq!(events.iter().filter(|e|e.0==4).count(),1);
        assert!(runtime.gestures.active.is_empty());assert!(runtime.host.touches.iter().all(|v|!*v));
        assert!(!events.iter().any(|e|matches!(e.0,1|2)));
    }
    #[test]
    fn rejected_context_abort_keeps_generation_and_uses_explicit_legacy_fallback() {
        unsafe extern "C" fn reject_abort(user:*mut c_void,action:i32,id:i32,x:f64,y:f64)->i32 {
            unsafe{&mut*(user as *mut Vec<(i32,i32,f64,f64)>)}.push((action,id,x,y));
            (action!=4)as i32
        }
        let mut events=Vec::new();let mut runtime=fixture(&mut events);runtime.host.callback=reject_abort;
        let id=runtime.host.begin(Point::new(0.8,0.8));
        assert!(!Host::abort_all_touches(&mut runtime.host));
        assert!(runtime.host.touches[id as usize]);assert_eq!(runtime.host.reset_generation,0);
        runtime.host.end(id,Point::new(0.8,0.8));
        assert_eq!(events.iter().map(|v|v.0).collect::<Vec<_>>(),vec![0,4,2]);
        assert_eq!(runtime.host.reset_generation,0,"a failed native cancel cannot fabricate a successful generation");
    }
    #[test]
    fn recorded_hirara_endpoints_are_usable_before_the_cancel_widget_exists() {
        use clbb_moba_core::controller::snapshot as s;
        let mut frame=s::Frame::default();frame.token=crate::source::text("recorded-match");
        frame.ui.button_count=2;
        for (i,uv) in [[0.781857303052746,0.8643963766009062],[0.9139779138587207,0.7058459842302351]].into_iter().enumerate() {
            frame.ui.buttons[i].slot=crate::source::text(&format!("skill:{i}"));
            frame.ui.buttons[i].active=crate::source::flag(true);
            frame.ui.buttons[i].press_uv=crate::source::v2(uv);
        }
        assert!(!frame.ui.cancel_uv.valid());
        let endpoints=["skill:0".into(),"skill:1".into()];
        let geometry=linked_geometry(&frame,&endpoints).expect("user's valid skill buttons must admit DOWN");
        assert!(geometry.cancel.is_none());
        frame.ui.buttons[1].active=crate::source::flag(false);
        assert!(linked_geometry(&frame,&endpoints).is_none());
    }
    #[test]
    fn debug_chord_works_while_suspended_and_never_creates_a_touch() {
        let mut events=Vec::new(); let mut r=fixture(&mut events);
        let dir=std::env::temp_dir().join(format!("clbb-chord-test-{}",now_ns()));
        r.recorder=Recorder::at(dir.clone()); r.suspended=true;
        r.recorder.set_enabled(true);
        let p=Point::new(0.5,0.5);
        assert!(r.event(1,0x04000053,true,false,p));
        assert!(r.event(1,0x04000053,true,true,p));
        assert!(r.event(1,0x04000053,false,false,p));
        r.recorder.set_enabled(false);
        let deadline=std::time::Instant::now()+Duration::from_secs(3);
        loop {
            let file=std::fs::read_dir(&dir).ok().into_iter().flatten().filter_map(Result::ok)
                .find(|e|e.path().extension().is_some_and(|v|v=="jsonl"));
            if let Some(file)=file {
                let data=std::fs::read_to_string(file.path()).unwrap();
                let header:serde_json::Value=serde_json::from_str(data.lines().next().unwrap()).unwrap();
                assert_eq!(header["marks"].as_array().unwrap().len(),1); break;
            }
            assert!(std::time::Instant::now()<deadline,"{}",r.recorder.status());
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(events.is_empty()); std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn contact_rotation_separates_gestures_without_reusing_a_live_finger() {
        let mut events=Vec::new();let mut runtime=fixture(&mut events);
        assert!(runtime.write("diagnostics.contact_rotation","true"));
        let p=Point::new(0.5,0.5);
        let first=runtime.host.begin(p);assert_eq!(first,0);
        runtime.host.end(first,p);
        let movement=runtime.host.begin(p);assert_eq!(movement,1);
        assert!(!runtime.write("diagnostics.contact_rotation","false"));
        for _ in 0..40 {
            let skill=runtime.host.begin(p);assert!(skill>=0 && skill!=movement);
            runtime.host.end(skill,p);
            assert!(runtime.host.touches[movement as usize]);
        }
        runtime.host.end(movement,p);
        assert!(runtime.write("diagnostics.contact_rotation","false"));
        assert_eq!(runtime.host.begin(p),0);
    }
    #[test]
    fn trace_toggle_reaches_adapter_without_releasing_live_contacts() {
        let mut events = Vec::new();
        let mut runtime = fixture(&mut events);
        let id = runtime.host.begin(Point::new(0.5, 0.5));
        assert!(id >= 0);
        let count = events.len();
        assert!(runtime.write("diagnostics.trace", "true"));
        assert!(runtime.host.trace);
        assert!(runtime.host.touches[id as usize]);
        assert_eq!(events.len(), count);
        assert!(!runtime.write("diagnostics.trace", "invalid"));
        assert!(runtime.host.trace);
        assert!(runtime.write("diagnostics.trace", "false"));
        assert!(!runtime.host.trace);
        assert!(runtime.host.touches[id as usize]);
        assert_eq!(events.len(), count);
    }
    #[test]
    fn native_packet_round_trips_to_the_recorded_command_cell() {
        for size in [[1308., 713.], [1920., 1080.], [4000., 3111.]] {
            for p in [
                Point::new(0., 0.),
                Point::new(0.2317, 0.7583),
                Point::new(0.9999, 0.9999),
                Point::new(1., 1.),
            ] {
                let native = android_packet_point(p, size);
                let decode = Point::new(
                    native.x * size[0] * size[0] / (size[0] - 1.),
                    native.y * size[1] * size[1] / (size[1] - 1.),
                );
                assert!(
                    (decode.x - (p.x * size[0] + 1e-7).floor().clamp(0., size[0] - 1.)).abs()
                        < 1e-6
                );
                assert!(
                    (decode.y - (p.y * size[1] + 1e-7).floor().clamp(0., size[1] - 1.)).abs()
                        < 1e-6
                );
            }
        }
    }
    #[test]
    fn offscreen_command_cells_keep_inclusive_encoding_without_screen_clipping() {
        for size in [[1308., 713.], [1920., 1080.], [4000., 3111.]] {
            for p in [Point::new(0.768125, 3229.5 / 3111.),
                      Point::new(-0.12, 1.12), Point::new(1.12, -0.12)] {
                let native = android_packet_point(p, size);
                for (value, encoded, extent) in [(p.x, native.x, size[0]), (p.y, native.y, size[1])] {
                    let decoded = encoded * extent * extent / (extent - 1.);
                    assert!((decoded - (value * extent + 1e-7).floor()).abs() < 1e-6);
                }
            }
        }
    }
    #[test]
    fn virtual_continuation_can_leave_view_but_initial_ui_touch_is_still_bounded() {
        let mut events = Vec::new();
        let mut rt = fixture(&mut events);
        let id = rt.host.begin(Point::new(-0.2, 1.2));
        assert!(id >= 0);
        assert_eq!((events[0].2, events[0].3), (0., 1.));
        assert!(rt.host.move_touch(id, Point::new(0.77, 1.04)));
        assert_eq!((events[1].2, events[1].3), (0.77, 1.04));
        assert!(!rt.host.move_touch(id, Point::new(0.77, f64::INFINITY)));
        assert!(!rt.host.move_touch(id, Point::new(0.77, 3.)));
        assert_eq!(events.len(), 2);
        rt.host.end(id, Point::new(0.77, 1.04));
        assert_eq!((events[2].0, events[2].3), (2, 1.04));
        assert!(!rt.host.move_touch(id, Point::new(0.77, 1.04)));
    }
    #[test]
    fn physical_lmb_never_creates_a_mapped_touch() {
        let mut events = Vec::new();
        let mut rt = fixture(&mut events);
        assert!(!rt.event(2, 1, true, false, Point::new(0.2, 0.3)));
        assert!(!rt.event(2, 1, false, false, Point::new(0.8, 0.9)));
        assert!(events.is_empty());
        assert!(rt.host.touches.iter().all(|v| !*v));
    }
    #[test]
    fn source_updates_do_not_postpone_armed_pointer_timer() {
        let mut events = Vec::new();
        let mut rt = fixture(&mut events);
        rt.host.timer(Timer::Pointer, Some(1));
        let first = rt.host.timers[1];
        for _ in 0..100 {
            rt.host.timer(Timer::Pointer, Some(1));
        }
        assert_eq!(rt.host.timers[1], first);
        rt.host.timer(Timer::Pointer, None);
        assert!(rt.host.timers[1].is_none());
    }
    #[test]
    fn failed_native_begin_does_not_reserve_finger() {
        unsafe extern "C" fn reject(_: *mut c_void, _: i32, _: i32, _: f64, _: f64) -> i32 {
            0
        }
        let mut events = Vec::new();
        let mut rt = fixture(&mut events);
        rt.host.callback = reject;
        assert_eq!(rt.host.begin(Point::new(0.5, 0.5)), -1);
        assert!(rt.host.touches.iter().all(|v| !*v));
    }
    #[test]
    fn private_fingers_do_not_alias_and_reuse_after_up() {
        let mut events = Vec::new();
        let mut rt = fixture(&mut events);
        let a = rt.host.begin(Point::new(0.1, 0.2));
        let b = rt.host.begin(Point::new(0.8, 0.9));
        assert_ne!(a, b);
        rt.host.end(a, Point::new(0.1, 0.2));
        assert_eq!(rt.host.begin(Point::new(0.3, 0.4)), a);
        assert_eq!(
            events.iter().map(|e| e.0).collect::<Vec<_>>(),
            vec![0, 0, 2, 0]
        );
    }
}
/// Called before Android Application/Resources construction. No source worker or
/// input state is created; the desktop window keeps its own physical size.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clbb_mapper_config_resolution(width: *mut i32, height: *mut i32) {
    let settings = Settings::load();
    let gain = if settings.boolean("audio.muted", 0) {
        0.0
    } else {
        settings.integer("audio.volume", 0) as f32 / 100.
    };
    GAIN.store(gain.to_bits(), Ordering::Relaxed);
    configure_channel_gains(&settings);
    crate::pacing::clbb_frame_pacer_configure(
        settings.integer("display.fps_limit", 0) as u32,
        i32::from(settings.boolean("display.vsync", 0)),
    );
    let w = settings.integer("display.width", 0);
    let h = settings.integer("display.height", 0);
    if let Some(out) = unsafe { width.as_mut() } {
        *out = w;
    }
    if let Some(out) = unsafe { height.as_mut() } {
        *out = h;
    }
}
