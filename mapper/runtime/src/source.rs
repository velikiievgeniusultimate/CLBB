//! Match-scoped plans. Keep addresses/owners, never reuse old game values.
use crate::{
    math,
    platform::Memory,
    reflection::{Reflection, Result, f32_at, i32_at, u64_at},
    unity::{self, Path, Widget},
};
use clbb_moba_core::controller::snapshot as s;
pub fn number(value: f64) -> s::Number {
    s::Number { value, kind: 1 }
}
pub fn flag(value: bool) -> s::Flag {
    s::Flag {
        value: value as i32,
    }
}
pub fn text(value: &str) -> s::Text {
    let mut t = s::Text::default();
    if value.len() <= 256 {
        t.present = 1;
        t.size = value.len() as u32;
        t.bytes[..value.len()].copy_from_slice(value.as_bytes());
    }
    t
}
pub fn v2(value: [f64; 2]) -> s::Vector2 {
    s::Vector2 {
        values: value.map(number),
        count: 2,
    }
}
pub fn v3(value: [f64; 3]) -> s::Vector3 {
    s::Vector3 {
        values: value.map(number),
        count: 3,
    }
}
pub fn matrix(value: [f64; 16]) -> s::Vector16 {
    s::Vector16 {
        values: value.map(number),
        count: 16,
    }
}
pub fn vec3(raw: &[u8]) -> Result<[f64; 3]> {
    let v = std::array::from_fn(|i| f32_at(raw, i * 4));
    if v.iter().any(|v| !v.is_finite() || v.abs() > 1e6) {
        Err("Nonfinite world coordinate".into())
    } else {
        Ok(v)
    }
}
pub type Checks = Vec<(u64, Vec<u8>)>;
/// Bytes actually consumed by a single sample, retained until the entire
/// camera/actor/UI/native-feedback phase has been checked a second time.
pub fn capture<M: Memory>(
    r: &Reflection<M>,
    checks: &mut Checks,
    address: u64,
    size: usize,
) -> Result<Vec<u8>> {
    let bytes = r.read(address, size)?;
    checks.push((address, bytes.clone()));
    Ok(bytes)
}
fn integer<M: Memory>(r: &Reflection<M>, checks: &mut Checks, address: u64) -> Result<i32> {
    Ok(i32_at(&capture(r, checks, address, 4)?, 0))
}
fn pointer<M: Memory>(r: &Reflection<M>, checks: &mut Checks, address: u64) -> Result<u64> {
    Ok(u64_at(&capture(r, checks, address, 8)?, 0))
}
fn boolean<M: Memory>(r: &Reflection<M>, checks: &mut Checks, address: u64) -> Result<bool> {
    let raw = capture(r, checks, address, 1)?;
    match raw[0] {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err("Invalid native boolean".into()),
    }
}
fn optional_flag<M: Memory>(r: &Reflection<M>, checks: &mut Checks, address: Option<u64>) -> s::Flag {
    let checkpoint = checks.len();
    let Some(value) = address.and_then(|address| boolean(r, checks, address).ok()) else {
        checks.truncate(checkpoint);
        return s::Flag::default();
    };
    // Only valid optional observations become part of the coherent sample.
    // An unreadable/malformed flag is unknown, never permission or prohibition.
    flag(value)
}
pub fn capture_path<M: Memory>(
    r: &Reflection<M>,
    checks: &mut Checks,
    path: &Path,
) -> Result<Vec<u8>> {
    path.validate(r)?;
    checks.extend(path.checks.iter().cloned());
    let mut bytes = Vec::with_capacity(path.transforms.len() * 48);
    for address in &path.transforms {
        bytes.extend(capture(r, checks, *address, 48)?);
    }
    Ok(bytes)
}
fn path_position<M: Memory>(
    r: &Reflection<M>,
    checks: &mut Checks,
    path: &Path,
) -> Result<[f64; 3]> {
    math::trs_chain(&capture_path(r, checks, path)?, None)
        .ok_or("Invalid native transform values".into())
}
fn widget_geometry<M: Memory>(
    r: &Reflection<M>,
    checks: &mut Checks,
    widget: &Widget,
    view: &[f64; 16],
    projection: &[f64; 16],
) -> Result<([f64; 2], [f64; 2])> {
    let raw = capture_path(r, checks, &widget.path)?;
    let width = integer(r, checks, widget.width)? as f64;
    let height = integer(r, checks, widget.height)? as f64;
    if !(0. < width && width <= 16384. && 0. < height && height <= 16384.) {
        return Err("Invalid native widget size".into());
    }
    let point = |local| {
        let p = math::trs_chain(&raw, local).ok_or("Invalid widget transform")?;
        math::project(view, projection, p, false).ok_or("Widget behind UI camera")
    };
    let center = point(None)?;
    let x = point(Some([width / 2., 0., 0.]))?;
    let y = point(Some([0., height / 2., 0.]))?;
    let radius = std::array::from_fn(|i| (x[i] - center[i]).hypot(y[i] - center[i]));
    if radius
        .iter()
        .any(|v| !v.is_finite() || *v <= 0.001 || *v >= 0.4)
    {
        return Err("Widget geometry outside screen bounds".into());
    }
    Ok((center, radius))
}

pub fn check<M: Memory>(r: &Reflection<M>, checks: &Checks) -> Result<()> {
    if crate::platform::matches_reads(&r.memory, checks)? {
        Ok(())
    } else {
        Err("Match source ownership changed".into())
    }
}
/// Movement can be disabled by the game without losing the widget's native
/// coordinates. Conversely, a missing optional widget must not reject a whole
/// independently coherent camera/skill sample.
fn read_movement_geometry<M: Memory>(
    r: &mut Reflection<M>,
    widget: &mut Widget,
    view: &[f64; 16],
    projection: &[f64; 16],
) -> Option<(s::MovementGeometry, Checks)> {
    if widget.path.validate(r).is_err() {
        widget.path = Path::new(r, widget.object, true).ok()?;
    }
    let mut reads = Vec::new();
    let (center, radius) = widget_geometry(r, &mut reads, widget, view, projection).ok()?;
    if !unity::onscreen(center) || check(r, &reads).is_err() {
        return None;
    }
    Some((
        s::MovementGeometry {
            ready: flag(true),
            press_uv: v2(center),
            center_uv: v2(center),
            radius_uv: v2(radius),
        },
        reads,
    ))
}
#[derive(Default)]
struct MovementPlan {
    frames: Vec<u64>,
    widget: Option<Widget>,
    checks: Checks,
}
impl MovementPlan {
    fn refresh<M: Memory>(&mut self, r: &mut Reflection<M>, frames: &[u64],
        discover: impl FnOnce(&mut Reflection<M>, &[u64]) -> Result<(Widget, Checks)>) {
        if self.frames == frames && self.widget.is_some() && check(r, &self.checks).is_ok() {return;}
        self.frames = frames.to_vec();
        self.widget = None;
        self.checks.clear();
        if let Ok((widget, checks)) = discover(r, frames) {
            if check(r, &checks).is_ok() {self.widget=Some(widget);self.checks=checks;}
        }
    }
}
fn discover_movement<M: Memory>(r: &mut Reflection<M>, frames: &[u64]) -> Result<(Widget, Checks)> {
    let mut owners=Vec::new();
    for owner in frames.iter().copied().filter(|v| *v!=0) {
        if r.class(r.pointer(owner)?)?.name=="UIJoyStick" {owners.push(owner);}
    }
    if owners.len()!=1 {return Err("Waiting for one owned movement frame".into());}
    let owner=owners[0];let mut checks=Vec::new();
    pointer(r,&mut checks,owner)?;
    let info=link(r,owner,"m_UIFrameInfo",&mut checks)?;
    let sprite=link(r,info,"m_Sprite1",&mut checks)?;
    let widget=Widget::new(r,sprite)?;
    Ok((widget,checks))
}
fn link<M: Memory>(
    r: &mut Reflection<M>,
    object: u64,
    name: &str,
    checks: &mut Checks,
) -> Result<u64> {
    let a = r.field(object, name)?;
    let p = r.pointer(a)?;
    checks.push((a, p.to_le_bytes().to_vec()));
    Ok(p)
}

#[derive(Clone)]
pub struct ButtonPlan {
    pub object: u64,
    pub slot: String,
    pub kind: String,
    pub index: i32,
    pub path: Path,
    pub data: u64,
    pub skill: u64,
    pub variant: Option<u64>,
    pub cast_lock: Option<u64>,
    pub state_forbid: Option<u64>,
    pub(crate) charge: Option<crate::control_audit::ChargePlan>,
    charge_retry: i64,
    choice_family: bool,
    choice: Option<crate::control_audit::ChoicePlan>,
    choice_retry: i64,
    choice_touch: Option<crate::aim::SelectionTouchPlan>,
    choice_touch_retry: i64,
    choice_touch_attempts: u8,
    choice_touch_held: bool,
    target_family: bool,
    target: Option<crate::target_selection::TargetPlan>,
    target_retry: i64,
    pub timer: Option<u64>,
    pub widget: Option<Widget>,
}
pub struct UiPlan {
    pub object: u64,
    pub checks: Checks,
    pub objects: Vec<u64>,
    pub buttons: Vec<ButtonPlan>,
    pub wheel: Widget,
    pub cancel: Widget,
    movement: MovementPlan,
    pub matrix: u64,
    pub camera_object: u64,
    pub current: u64,
    pub held: [u64; 2],
    pub visible: [u64; 3],
    pub cancel_flag: Option<u64>,
    pub cancel_id: Option<u64>,
    pub optional: crate::optional_ui::OptionalUi,
    pub linked: Option<crate::control_audit::CurvePlan>,
    linked_retry: i64,
    portrait: Option<crate::portrait_picker::Plan>,
    portrait_retry: i64,
    last_choice_slot: String,
}
pub(crate) fn skill_button_class<M: Memory>(r: &mut Reflection<M>, class: &crate::reflection::Class) -> Result<bool> {
    let mut ancestry = vec![class.name.clone()];
    let mut parent = class.parent;
    for _ in 0..16 {
        if parent == 0 { break; }
        let ancestor = r.class(parent)?;
        ancestry.push(ancestor.name);
        parent = ancestor.parent;
    }
    Ok(control_kind(false, &ancestry) == "skill")
}

// An idle miss must not suppress classification of the indicator that is
// actually being held now. Keep retries bounded while native forms rebind.
fn behavior_retry_due(now:i64,deadline:i64,active:bool)->bool {
    now>=deadline || (active && deadline.saturating_sub(now)>4_000_000)
}
fn behavior_retry_delay(active:bool)->i64 {if active {4_000_000}else{1_000_000_000}}
fn held_indicator_behavior_id(base:i32, future:i32, actual:i32, charge:bool, held_slot:bool)->i32 {
    // The future release changes during charging, while the owned active
    // indicator can still be the base. Classify the indicator actually held,
    // not the future cast. Never reuse it for an idle or unverified variant.
    if charge && held_slot && base>0 && actual==base {base}else{future}
}
fn verified_button_only_form(hero:i32, slot:&str, base:i32, active:i32)->bool {
    // Badang's successful wall dash exposes its ordinary wall-removal button
    // through the SAME owned slot/data. There is intentionally no aim registry
    // entry for this recast. Never apply the exception to its original dash.
    (hero==77 && slot=="skill:1" && base==7720 && active==7725)
        // Chou's follow-up follows the target selected by the initial kick;
        // its HookLine indicator does not take a second aiming direction.
        || (hero==26 && slot=="skill:2" && base==2630 && active==2632)
        || (hero==25 && slot=="skill:2" && base==2530 && matches!(active,2530|2531))
        // Angela can stop the attachment channel through its ordinary native
        // cancel form; it has no indicator registry entry. Selection5530 and
        // the later aimed detach5535 retain their own existing behavior.
        || (hero==55 && slot=="skill:2" && base==5530 && active==5534)
        // Akai stops Heavy Spin through the same owned ultimate slot.
        // The stop form has no aiming registry, unlike other active forms.
        || (hero==9 && slot=="skill:2" && base==930 && active==938)
        // Gusion's marked-target pursuit and dagger recall are plain recasts.
        // Availability and the hit requirement come from the native variant.
        || (hero==56 && ((slot=="skill:0" && base==5610 && active==5612)
            || (slot=="skill:1" && base==5620 && active==5623)))
}
fn control_kind(is_attack: bool, ancestry: &[String]) -> &'static str {
    if is_attack { return "attack"; }
    if ancestry.iter().any(|name| name.starts_with("SkillButton") && name.ends_with("Ex")) {
        return "extra";
    }
    if ancestry.iter().any(|name| name == "SkillButton") { "skill" } else { "extra" }
}

/// A specialized choice button may become an ordinary native follow-up form.
/// Unknown current form is not permission to fall back to the base Tap class.
fn choice_selector_phase(ui:&s::Ui,state:&s::VariantState,slot:&str)->Option<bool> {
    if !ui.ready.get()||!state.ready.get()||!ui.hero_id.is_number()||ui.hero_id.int()<=0
        ||state.hero_id.kind!=ui.hero_id.kind||state.hero_id.value!=ui.hero_id.value{return None;}
    let mut buttons=ui.buttons.iter().take(ui.button_count.min(32)as usize).filter(|b|b.slot.as_str()==slot);
    let button=buttons.next()?;
    if buttons.next().is_some()||!button.active.get()||!button.skill_id.is_number()||button.skill_id.int()<=0{return None;}
    let mut variants=state.buttons.iter().take(state.count.min(32)as usize).filter(|v|v.slot.as_str()==slot);
    let variant=variants.next()?;
    if variants.next().is_some()||!variant.base_skill_id.is_number()||!variant.active_skill_id.is_number()
        ||variant.base_skill_id.int()!=button.skill_id.int()||variant.active_skill_id.int()<=0{return None;}
    Some(variant.active_skill_id.int()==variant.base_skill_id.int())
}
fn selection_plan_retry_delay(attempts:u8,held:bool)->i64 {
    if !held {1_000_000_000}else if attempts<=3 {0}else{20_000_000}
}
fn choice_control_type(phase:Option<bool>,verified_future:Option<u8>)->u8 {
    match (phase,verified_future) {
        (Some(false),Some(kind @ 1..=3))=>kind,
        // Known class + unknown form/receipt is still Selection, not aim_type0
        // (which the generic mapper interprets as Range) or implicit Tap.
        _=>4,
    }
}
fn retain_choice_family(ui:&s::Ui,state:&s::VariantState,slot:&str,
    control_types:&mut std::collections::BTreeMap<String,u8>)->Option<bool> {
    let phase=choice_selector_phase(ui,state,slot);
    control_types.insert(slot.to_owned(),choice_control_type(phase,None));
    phase
}

#[cfg(test)]
mod control_kind_tests {
    #[test]
    fn held_native_form_does_not_inherit_idle_discovery_backoff() {
        use super::{behavior_retry_due,behavior_retry_delay};
        let now=10_000_000;
        assert!(!behavior_retry_due(now,now+1_000_000_000,false));
        assert!(behavior_retry_due(now,now+1_000_000_000,true));
        let next=now+behavior_retry_delay(true);
        assert!(!behavior_retry_due(now+1_000_000,next,true));
        assert!(behavior_retry_due(next,next,true));
        assert_eq!(behavior_retry_delay(false),1_000_000_000);
    }
    use super::{control_kind, held_indicator_behavior_id};
    #[test]
    fn held_charge_keeps_base_indicator_template_but_other_variants_do_not() {
        assert_eq!(held_indicator_behavior_id(7810,7871,7810,true,true),7810);
        assert_eq!(held_indicator_behavior_id(7810,7811,7810,true,true),7810);
        for (actual,charge,held) in [(7810,false,true),(7810,true,false),(7871,true,true),(0,true,true)] {
            assert_eq!(held_indicator_behavior_id(7810,7871,actual,charge,held),7871);
        }
        assert_eq!(held_indicator_behavior_id(7720,7725,7720,false,true),7725);
    }
    #[test]
    fn specialized_hero_button_keeps_skill_slot() {
        let chain = ["KeySkilleCheckMesh", "SkillButton", "KeySkill"].map(String::from);
        assert_eq!(control_kind(false, &chain), "skill");
        assert_eq!(control_kind(true, &chain), "attack");
    }
    #[test]
    fn additional_controls_do_not_collide_with_hero_skills() {
        for chain in [vec!["SkillButtonAutoChargeEx", "SkillButton", "KeySkill"],
                      vec!["KeySkillExtra", "KeySkill"],
                      vec!["DerivedCharge", "SkillButtonAutoChargeEx", "SkillButton"]] {
            assert_eq!(control_kind(false, &chain.into_iter().map(String::from).collect::<Vec<_>>()), "extra");
        }
    }
}

impl UiPlan {
    fn new<M: Memory>(r: &mut Reflection<M>, local_show: u64) -> Result<Self> {
        r.discover(&["UIKeySkill"])?;
        let target = r.named["UIKeySkill"];
        let mut base = r.class(target)?;
        for _ in 0..16 {
            if base.name == "BaseFrame" {
                break;
            }
            base = r.class(base.parent)?;
        }
        if base.name != "BaseFrame" {
            return Err("UI frame inheritance changed".into());
        }
        let offset = base
            .fields
            .get("m_ActiveFrameList")
            .ok_or("UI frame list is absent")?
            .offset;
        let frames_address = base.statics + offset;
        let frames = r.pointer(frames_address)?;
        let objects = r.list(frames)?;
        let controls: Vec<_> = objects
            .iter()
            .copied()
            .filter(|o| *o != 0 && r.pointer(*o).ok() == Some(target))
            .collect();
        if controls.len() != 1 {
            return Err("Waiting for active match controls".into());
        }
        let object = controls[0];
        let mut checks = vec![
            (frames_address, frames.to_le_bytes().to_vec()),
            (object, target.to_le_bytes().to_vec()),
        ];
        let button_list = link(r, object, "statusKeySkillList", &mut checks)?;
        let list_header = r.read(button_list + 16, 16)?;
        let list_array = u64_at(&list_header, 0);
        let list_count = i32_at(&list_header, 8);
        if list_array == 0 || !(0..=256).contains(&list_count) {
            return Err("Invalid live skill control list".into());
        }
        let list_items = r.read(list_array + 32, list_count as usize * 8)?;
        checks.extend([
            (button_list + 16, list_header),
            (list_array + 32, list_items.clone()),
        ]);
        let mut values: Vec<_> = list_items
            .chunks_exact(8)
            .map(|raw| u64_at(raw, 0))
            .collect();
        let attack = link(r, object, "m_KeySKillATK", &mut checks)?;
        if attack != 0 && !values.contains(&attack) {
            values.push(attack);
        }
        let mut buttons = Vec::new();
        let mut slots = std::collections::HashSet::new();
        for button in values {
            if button == 0 {
                continue;
            }
            let class = r.class(r.pointer(button)?)?;
            let at = r.field(button, "m_iIndex")?;
            let index = r.integer(at)?;
            if !(0..64).contains(&index) {
                return Err("Invalid control index".into());
            }
            // Specialized hero buttons (e.g. Ling's KeySkilleCheckMesh)
            // inherit SkillButton without sharing its name prefix. Classify
            // by the validated metadata ancestry; explicit Ex subclasses
            // remain additional controls even when they inherit SkillButton.
            let kind = if button == attack { "attack" }
                else if skill_button_class(r, &class)? { "skill" } else { "extra" };
            let slot = if kind == "attack" {
                "attack".into()
            } else {
                format!("{kind}:{index}")
            };
            if !slots.insert(slot.clone()) {
                return Err("Ambiguous control slot".into());
            }
            let owner = link(r, button, "m_Self", &mut checks)?;
            let data = link(r, button, "m_SkillData", &mut checks)?;
            let skill = if data != 0 {
                r.field(data, "m_InitSkillID")?
            } else {
                0
            };
            let variant = if data != 0 {
                r.field(data, "m_TranID").ok()
            } else {
                None
            };
            let timer = r.field(button, "m_CdCoolDown").ok();
            let cast_lock = if kind == "skill" {
                crate::control_audit::cast_lock_address(r, button)
            } else {
                None
            };
            let state_forbid = if kind == "skill" {
                crate::control_audit::state_forbid_address(r, button)
            } else {
                None
            };
            let charge = if kind == "skill" {
                crate::control_audit::ChargePlan::discover(r, button, data, local_show).ok()
            } else {
                None
            };
            let choice_family=kind=="skill"&&crate::control_audit::ChoicePlan::is_family(r,&class)?;
            let choice=if choice_family {crate::control_audit::ChoicePlan::discover(r,object,button,data,local_show).ok()}else{None};
            let target_family=kind=="skill"&&crate::target_selection::TargetPlan::is_family(r,&class)?;
            checks.push((button, class.address.to_le_bytes().to_vec()));
            checks.push((at, index.to_le_bytes().to_vec()));
            buttons.push(ButtonPlan {
                object: button,
                slot,
                kind: kind.into(),
                index,
                path: Path::new(r, owner, false)?,
                data,
                skill,
                variant,
                cast_lock,
                state_forbid,
                charge,
                charge_retry: 0,
                choice_family,choice,choice_retry:0,choice_touch:None,choice_touch_retry:0,choice_touch_attempts:0,choice_touch_held:false,
                target_family,target:None,target_retry:0,
                timer,
                widget: None,
            });
        }
        let info = link(r, object, "m_UIFrameInfo", &mut checks)?;
        let wheel_object = link(r, info, "m_sprite_SkillRange02", &mut checks)?;
        let wheel = Widget::new(r, wheel_object)?;
        let cancel_object = link(r, info, "m_sprite_SkillCancel", &mut checks)?;
        let cancel = Widget::new(r, cancel_object)?;
        let camera = link(
            r,
            wheel_object,
            "s_CameraOrthographicSizeOwner",
            &mut checks,
        )?;
        // The shared orthographic-size cache may be empty on a cold start.
        // In that case both owned widgets must agree on their own UI camera.
        let camera = if camera == 0 {
            let wheel_camera = link(r, wheel_object, "mCam", &mut checks)?;
            let cancel_camera = link(r, cancel_object, "mCam", &mut checks)?;
            if wheel_camera == 0 || wheel_camera != cancel_camera {
                return Err("Waiting for a shared skill UI camera".into());
            }
            wheel_camera
        } else {
            camera
        };
        let camera_class = pointer(r, &mut checks, camera)?;
        if r.class(camera_class)?.name != "Camera" {
            return Err("Unexpected skill UI camera type".into());
        }
        let native = link(r, camera, "m_CachedPtr", &mut checks)?;
        let matrix = native + unity::ui_matrix_offset(&r.read(native, 4096)?)? as u64;
        let current = r.field(object, "m_curVirtualButton")?;
        let held = [
            r.field(object, "_bPressSkill")?,
            r.field(object, "bPressATTK")?,
        ];
        let visible = [
            r.field(object, "bInit")?,
            r.field(object, "actived")?,
            r.field(object, "m_bImShow")?,
        ];
        let cancel_flag = r.field(object, "bCancelSkill").ok();
        let cancel_id = r.field(object, "m_iPressSkillID").ok();
        let optional = crate::optional_ui::OptionalUi::prepare(r, object, &objects, local_show)?;
        let linked=crate::control_audit::CurvePlan::discover(r,object,&buttons).ok();
        Ok(Self {
            linked, linked_retry:0,portrait:None,portrait_retry:0,last_choice_slot:String::new(),
            object,
            checks,
            objects,
            buttons,
            wheel,
            cancel,
            movement: MovementPlan::default(),
            matrix,
            camera_object:camera,
            current,
            held,
            visible,
            cancel_flag,
            cancel_id,
            optional,
        })
    }
    fn sample<M: Memory>(
        &mut self,
        r: &mut Reflection<M>,
        hero: i32,
        token: &str,
        now: i64,
        size: [f64; 2],
        clock: u32,
        consumed: &mut Checks,
    ) -> Result<(s::Ui, s::VariantState, s::CancelState)> {
        check(r, &self.checks)?;
        consumed.extend(self.checks.iter().cloned());
        for a in self.visible {
            if !boolean(r, consumed, a)? {
                return Err("Match controls hidden".into());
            }
        }
        let raw = capture(r, consumed, self.matrix, 128)?;
        if math::matrix_offset(&raw, true) != Some(0) {
            return Err("UI camera changed".into());
        }
        let (view, projection) = unity::matrices(&raw)?;
        let expected = size[0] / size[1];
        let aspect = (projection[5] / projection[0]).abs();
        if !expected.is_finite()
            || !aspect.is_finite()
            || (aspect - expected).abs() > expected * 0.01
        {
            return Err("UI camera is waiting for logical display resize".into());
        }

        let held = [
            boolean(r, consumed, self.held[0])?,
            boolean(r, consumed, self.held[1])?,
        ];
        let current_value = pointer(r, consumed, self.current)?;
        let current = if held[0] || held[1] { current_value } else { 0 };
        let geometry = |w: &mut Widget,
                        r: &mut Reflection<M>,
                        consumed: &mut Checks|
         -> Result<([f64; 2], [f64; 2])> {
            if w.path.validate(r).is_err() {
                w.path = Path::new(r, w.object, true)?;
            }
            widget_geometry(r, consumed, w, &view, &projection)
        };
        let (wheel, radius) = geometry(&mut self.wheel, r, consumed)?;
        if radius.iter().any(|v| *v <= 0.005 || *v >= 0.3) {
            return Err("Invalid skill wheel size".into());
        }
        let mut ui = s::Ui {
            ready: flag(true),
            hero_id: number(hero as f64),
            time_ns: number(now as f64),
            resolution_px: v2(size),
            ..Default::default()
        };
        let mut variants = s::VariantState {
            ready: flag(true),
            source: text("ShowSkillData.m_TranID"),
            token: text(token),
            hero_id: number(hero as f64),
            sample_start_ns: number(now as f64),
            sample_end_ns: number(now as f64),
            ..Default::default()
        };
        let mut cooldown = s::CooldownState {
            present: flag(true),
            ready: flag(true),
            coherent: flag(true),
            source: text("VirtualButton.m_CdCoolDown"),
            token: text(token),
            hero_id: number(hero as f64),
            sample_start_ns: number(now as f64),
            sample_end_ns: number(now as f64),
            ..Default::default()
        };
        for button in &mut self.buttons {
            if button.path.validate(r).is_err() {
                let owner = r.link(button.object, "m_Self")?;
                button.path = Path::new(r, owner, false)?;
            }
            let press = math::project(
                &view,
                &projection,
                path_position(r, consumed, &button.path)?,
                false,
            )
            .ok_or("Control is behind UI camera")?;
            if !unity::onscreen(press) {
                continue;
            }
            let skill = if button.skill != 0 {
                integer(r, consumed, button.skill)?
            } else {
                0
            };
            let active = button.object == current;
            let center = if active && unity::onscreen(wheel) {
                wheel
            } else {
                std::array::from_fn(|i| press[i].clamp(radius[i], 1. - radius[i]))
            };
            let source = if active {
                "active_widget_transform"
            } else {
                "widget_radius_estimated_screen_clamp"
            };
            let i = ui.button_count as usize;
            if i >= 32 {
                break;
            }
            ui.buttons[i] = s::Button {
                slot: text(&button.slot),
                kind: text(&button.kind),
                geometry_source: text(source),
                index: number(button.index as f64),
                skill_id: number(skill as f64),
                active: flag(true),
                press_uv: v2(press),
                center_uv: v2(center),
                radius_uv: v2(radius),
            };
            ui.button_count += 1;
            if active {
                ui.active_slot = text(&button.slot);
                ui.joystick = s::Geometry {
                    ready: flag(true),
                    source: text(source),
                    slot: text(&button.slot),
                    skill_id: number(skill as f64),
                    sample_start_ns: number(now as f64),
                    sample_end_ns: number(now as f64),
                    center_uv: v2(center),
                    radius_uv: v2(radius),
                };
            }
            let variant = match button.variant {
                Some(a) => integer(r, consumed, a)?,
                None => 0,
            };
            if button.kind == "skill" || button.kind == "attack" {
                let cast_locked = button
                    .cast_lock
                    .and_then(|address| boolean(r, consumed, address).ok())
                    .map(flag)
                    .unwrap_or_default();
                let state_forbidden = optional_flag(r, consumed, button.state_forbid);
                let n = variants.count as usize;
                variants.buttons[n] = s::Variant {
                    slot: text(&button.slot),
                    base_skill_id: number(skill as f64),
                    active_skill_id: number(variant as f64),
                    cast_locked,
                    state_forbidden,
                    charge_skill: s::Flag::default(),
                };
                variants.count += 1;
            }
            if button.kind == "skill" || button.kind == "extra" {
                let blocked = button
                    .timer
                    .and_then(|timer_link| pointer(r, consumed, timer_link).ok())
                    .filter(|timer| *timer != 0)
                    .and_then(|timer| read_cooldown(r, timer, clock, consumed).ok());
                let n = cooldown.count as usize;
                cooldown.buttons[n] = s::CooldownRow {
                    slot: text(&button.slot),
                    base_skill_id: number(skill as f64),
                    active_skill_id: number(variant as f64),
                    ready: flag(blocked.is_some()),
                    on_cooldown: flag(blocked.unwrap_or(false)),
                };
                cooldown.count += 1;
            }
        }
        ui.cooldown_state = cooldown;
        self.optional.append(r, &view, &projection, &mut ui)?;
        let (mut cancel, _) = geometry(&mut self.cancel, r, consumed)?;
        if !unity::onscreen(cancel) {
            if let Some(b) = ui.buttons[..ui.button_count as usize]
                .iter()
                .find(|b| b.kind.as_str() == "skill")
            {
                cancel =
                    std::array::from_fn(|i| b.center_uv.values[i].value + cancel[i] - wheel[i]);
            }
        }
        if unity::onscreen(cancel) {
            ui.cancel_uv = v2(cancel);
        }
        // UIJoyStick can be allocated AFTER the first coherent skill frame.
        // Refresh only this optional plan from current, fenced membership;
        // never freeze its absence for the rest of the match.
        self.movement.refresh(r,self.optional.active_frames(),discover_movement);
        if self.objects!=self.optional.active_frames() {self.objects=self.optional.active_frames().to_vec();}
        if let Some(movement) = &mut self.movement.widget {
            let membership=self.optional.active_frame_checks();
            let membership_ready=!membership.is_empty() && check(r,membership).is_ok();
            // canUse is the game's temporary permission to move (for example
            // during Kagura's dash), not ownership or validity of this widget.
            // Keeping the real finger down lets movement resume after the dash.
            // Optional movement geometry must not invalidate coherent skill
            // feedback while this widget is briefly hidden/reparented.
            if let Some((geometry, reads)) = read_movement_geometry(r, movement, &view, &projection)
            {
                if membership_ready && check(r,&self.movement.checks).is_ok() {
                    consumed.extend(membership.iter().cloned());
                    consumed.extend(self.movement.checks.iter().cloned());
                    consumed.extend(reads);
                    ui.movement = geometry;
                }
            }
        }
        let cancel_state = if let (Some(a), Some(id)) = (self.cancel_flag, self.cancel_id) {
            s::CancelState {
                ready: flag(true),
                coherent: flag(true),
                source: text("UIKeySkill.bCancelSkill"),
                token: text(token),
                hero_id: number(hero as f64),
                slot: ui.active_slot,
                base_skill_id: number(integer(r, consumed, id)? as f64),
                cancelled: flag(boolean(r, consumed, a)?),
                sample_start_ns: number(now as f64),
                sample_end_ns: number(now as f64),
            }
        } else {
            s::CancelState::default()
        };
        check(r, &self.checks)?;
        if r.read(self.matrix, 128)? != raw {
            return Err("UI camera changed during sample".into());
        }
        Ok((ui, variants, cancel_state))
    }
}
fn read_cooldown<M: Memory>(
    r: &mut Reflection<M>,
    timer: u64,
    clock: u32,
    consumed: &mut Checks,
) -> Result<bool> {
    let class = pointer(r, consumed, timer)?;
    r.validate_class(class)?;
    let active = r.field(timer, "m_CanCountCd")?;
    let error = r.field(timer, "bIsCDError")?;
    if boolean(r, consumed, error)? {
        return Err("Game cooldown reports an error".into());
    }
    let counting = boolean(r, consumed, active)?;
    let max = r.field(timer, "maxCdTime")?;
    let start = r.field(timer, "startTime")?;
    let duration = integer(r, consumed, max)? as u32;
    let elapsed = clock.wrapping_sub(integer(r, consumed, start)? as u32);
    if !counting {
        return Ok(false);
    }
    if duration == 0 || duration > 86400000 || elapsed >= 0x80000000 {
        return Err("Invalid cooldown clock".into());
    }
    Ok(true)
}

/// Observe a result's skill-component owner; UI result context alone is not proof.
/// The caller must reread all returned consumed bytes before accepting its receipt.
pub fn skill_result_is_local<M:Memory>(r:&mut Reflection<M>,show:u64,data:u64,consumed:&mut Checks)->Result<bool> {
    let raw=capture(r,consumed,data,8)?;let class=r.class(u64_at(&raw,0))?;
    if class.name!="ShowSkillData" {return Err("Unexpected skill result owner type".into());}
    let field=class.fields.get("m_SkillComp").ok_or("Skill result owner field absent")?;
    if field.is_static || field.kind!=18 || field.offset>1048576 {return Err("Skill result owner link type changed".into());}
    let result_owner=capture(r,consumed,data+field.offset,8)?;
    let at=r.field(show,"m_SkillComp")?;
    let local_owner=capture(r,consumed,at,8)?;
    let owner=u64_at(&result_owner,0);let local=u64_at(&local_owner,0);
    if local==0 || owner==0 {return Ok(false);}
    let raw=capture(r,consumed,local,8)?;
    if r.class(u64_at(&raw,0))?.name!="ShowSkillComp" {return Err("Unexpected local skill component".into());}
    Ok(owner==local)
}

#[derive(Clone)]
pub struct Sample {
    pub plan_revision: u64,
    /// Battle clock consumed and verified in the same bracket as the geometry.
    /// Repeated reads of one game update must not be counted as new updates.
    pub game_clock_ms: u32,
    pub match_owners: [u64; 4],
    pub frame: s::Frame,
    pub projection: [f64; 16],
    pub logic: [f64; 2],
    pub indicator: u64,
    pub aim: Option<[f64; 3]>,
    pub direction_visual: Option<crate::projection::DirectionVisual>,
    pub direction_origin: Option<crate::projection::DirectionOrigin>,
    pub control_types: std::collections::BTreeMap<String,u8>,
    /// Current coherent charge lifecycle observations; absence is unknown.
    pub control_holds: std::collections::BTreeMap<String, bool>,
    pub linked: Option<crate::linked_gesture::Observation>,
    pub portrait: Option<crate::portrait_picker::Observation>,
}
struct Plan {
    checks: Checks,
    match_checks: Checks,
    match_owners: [u64; 4],
    wall_paths: std::collections::HashMap<(u64,u64),Path>,
    render: u64,
    logic: u64,
    hero: u64,
    camera: u64,
    camera_path: Path,
    indicator: u64,
    skill: u64,
    clock: u64,
    ui: UiPlan,
    motions: std::collections::HashMap<i32, crate::aim::MotionPlan>,
    attack_mode: Option<crate::attack::ModePlan>,
    attack_mode_retry:i64,
    attack_motion:Option<crate::aim::MotionPlan>,
    behaviors: std::collections::HashMap<i32,(Checks,u8)>,
    behavior_retry: std::collections::HashMap<i32,i64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MatchIdentity {
    owners: [u64; 4],
    hero: i32,
}
#[derive(Default)]
struct AuxiliaryProducer {
    scope:Option<(MatchIdentity,u64)>,ui:u64,plan:Option<crate::control_audit::AuxiliaryHoldPlan>,retry:i64,
    current:u64,retired:[u64;7],retired_count:usize,
}
impl AuxiliaryProducer {
    fn retire(&mut self,owner:u64) {
        if owner==0||self.retired[..self.retired_count].contains(&owner){return;}
        if self.retired_count==self.retired.len(){self.retired.rotate_left(1);self.retired_count-=1;}
        self.retired[self.retired_count]=owner;self.retired_count+=1;
    }
    fn project(&mut self,observation:crate::control_audit::AuxiliaryHoldObservation,token:&str,hero:i32,now:i64)->s::AuxiliaryControlState {
        let current=if observation.owner_present{observation.owner_id}else{observation.replacement_owner.unwrap_or(0)};
        let hold=if observation.owner_present{observation.hold_to_release}else{observation.replacement_hold};
        if current==0{return Default::default();}
        if self.current!=current{self.retire(self.current);self.current=current;}
        if !observation.owner_present{self.retire(observation.owner_id);}
        let mut packet=s::AuxiliaryControlState {ready:flag(true),coherent:flag(true),source:text("UIKeySkill.auxiliary_controls"),
            token:text(token),hero_id:number(hero as f64),sample_start_ns:number(now as f64),sample_end_ns:number(now as f64),..Default::default()};
        packet.controls[0]=s::AuxiliaryControlRow {slot:text("attack"),owner_present:flag(true),owner_id:current,
            hold_to_release:hold.map(flag).unwrap_or_default(),..Default::default()};packet.count=1;
        for owner in self.retired[..self.retired_count].iter().copied().filter(|owner|*owner!=current) {
            packet.controls[packet.count as usize]=s::AuxiliaryControlRow {slot:text("attack"),owner_present:flag(false),owner_id:owner,..Default::default()};packet.count+=1;
        }
        packet
    }
    fn sample<M:Memory>(&mut self,r:&mut Reflection<M>,identity:MatchIdentity,epoch:u64,ui:u64,button:u64,
        token:&str,now:i64,consumed:&mut Checks)->s::AuxiliaryControlState {
        if self.scope!=Some((identity,epoch)){*self=Self{scope:Some((identity,epoch)),..Default::default()};}
        if self.ui!=ui{self.ui=ui;self.plan=None;self.retry=0;}
        if self.plan.as_ref().is_some_and(|plan|plan.validate(r).is_err()){self.plan=None;self.retry=0;}
        if self.plan.is_none()&&button!=0&&now>=self.retry {
            self.plan=crate::control_audit::AuxiliaryHoldPlan::discover(r,ui,button,identity.owners[2]).ok();
            self.retry=now.saturating_add(20_000_000);
        }
        let Some(plan)=&mut self.plan else{return Default::default();};
        match plan.sample(r,consumed) {
            Ok(value)=>self.project(value,token,identity.hero,now),
            Err(_)=>Default::default(), // missing/foreign rows stay Unknown.
        }
    }
}
/// Input identity follows the battle/local actor, not a cached UI read plan.
/// A normal form/geometry replan must never look like a new match to the core.
#[derive(Clone, Copy, Default)]
struct MatchEpoch {
    identity: Option<MatchIdentity>,
    clock: Option<u32>,
    serial: u64,
}
impl MatchEpoch {
    fn observe(&mut self, identity: MatchIdentity, clock: u32) -> u64 {
        let clock_reset = self
            .clock
            .is_some_and(|old| clock.wrapping_sub(old) >= 0x80000000);
        if self.identity != Some(identity) || clock_reset {
            self.serial = self.serial.wrapping_add(1).max(1);
        }
        self.identity = Some(identity);
        self.clock = Some(clock);
        self.serial
    }
    fn invalidate(&mut self) {
        self.identity = None;
        self.clock = None;
    }
}
pub struct Source<M: Memory> {
    pub reflection: Reflection<M>,
    pub portrait_error: String,
    plan: Option<Plan>,
    pub token: String,
    generation: u64,
    match_checks: Checks,
    match_epoch: MatchEpoch,
    auxiliary:AuxiliaryProducer,
}
impl<M: Memory> Source<M> {
    pub fn new(memory: M, token: String) -> Result<Self> {
        Ok(Self {
            reflection: Reflection::new(memory)?,portrait_error:String::new(),
            plan: None,
            token,
            generation: 0,
            match_checks: Vec::new(),
            match_epoch: MatchEpoch::default(),
            auxiliary:AuxiliaryProducer::default(),
        })
    }
    /// Read-only inventory of the already owned match-control roots.
    /// No arbitrary object traversal or new live-control authorization.
    pub fn audit_ui_schema(&mut self)->Result<serde_json::Value> {
        let plan=self.plan.as_ref().ok_or("No current control plan")?;
        check(&self.reflection,&plan.ui.checks)?;
        let r=&mut self.reflection;
        let mut consumed=Vec::new();
        let mut objects=vec![("controls".to_owned(),plan.ui.object)];
        if let Ok(at)=r.field(plan.ui.object,"m_UIFrameInfo") {
            let raw=capture(r,&mut consumed,at,8)?;let child=u64_at(&raw,0);
            if child!=0 {objects.push(("frame_info".to_owned(),child));}
        }
        // Named local control references only; inventory is not input authorization.
        for (role,object) in objects.clone() {
            let class=r.class(r.pointer(object)?)?;
            if let Some(f)=class.fields.get("m_buttonCurveLink") {
                if !f.is_static && f.kind==18 {
                    let raw=capture(r,&mut consumed,object+f.offset,8)?;
                    let child=u64_at(&raw,0);
                    if child!=0 {objects.push((format!("{role}.curve_link"),child));}
                }
            }
        }
        for (role,object) in objects.clone() {
            let class=r.class(r.pointer(object)?)?;
            if class.name!="ButtonCurveLink" {continue;}
            for name in ["m_button1","m_button2"] {
                let f=class.fields.get(name).ok_or("Missing curve endpoint")?;
                if f.is_static || f.kind!=18 {return Err("Curve endpoint type changed".into());}
                let raw=capture(r,&mut consumed,object+f.offset,8)?;let child=u64_at(&raw,0);
                if child!=0 && plan.ui.buttons.iter().any(|b|b.object==child && b.kind=="skill") {
                    objects.push((format!("{role}.{name}"),child));
                }
            }
        }
        // These are only frames already enumerated by the owned match UI plan.
        for object in plan.ui.objects.iter().copied().take(64) {
            if object==0 || object==plan.ui.object {continue;}
            let class=r.class(r.pointer(object)?)?;
            if objects.len()<128 {
                objects.push((format!("active:{}",class.name),object));
                if let Ok(at)=r.field(object,"m_UIFrameInfo") {
                    let raw=capture(r,&mut consumed,at,8)?;let child=u64_at(&raw,0);
                    if child!=0 {objects.push((format!("active_info:{}",class.name),child));}
                }
            }
        }
        let camera=capture(r,&mut consumed,plan.ui.matrix,128)?;
        let (view,projection)=unity::matrices(&camera)?;
        let mut schema=Vec::new();
        for (role,object) in objects {
            let raw=capture(r,&mut consumed,object,8)?;let mut class=u64_at(&raw,0);
            for _ in 0..16 {
                if class==0 {break;}
                let c=r.class(class)?;
                let mut fields:Vec<_>=c.fields.iter().map(|(name,f)|serde_json::json!({"name":name,"kind":f.kind,"static":f.is_static})).collect();
                fields.sort_by_key(|v|v["name"].as_str().unwrap_or("").to_owned());
                let mut close_candidates=Vec::new();
                for (name,f) in &c.fields {
                    if !f.is_static && f.kind==18 && (name.to_ascii_lowercase().contains("close") || name=="m_button_PlayerInfo") {
                        let raw=capture(r,&mut consumed,object+f.offset,8)?;let target=u64_at(&raw,0);
                        if target==0 {continue;}
                        for component in [false,true] {
                            if let Ok(path)=Path::new(r,target,component) {
                                if let Ok(pos)=path_position(r,&mut consumed,&path) {
                                    if let Some(uv)=math::project(&view,&projection,pos,false).filter(|p|unity::onscreen(*p)) {
                                        close_candidates.push(serde_json::json!({"field":name,"uv":uv}));break;
                                    }
                                }
                            }
                        }
                    }
                }
                schema.push(serde_json::json!({"role":role,"class":c.name,"fields":fields,"close_candidates":close_candidates}));class=c.parent;
            }
        }
        check(r,&consumed)?;check(r,&plan.ui.checks)?;
        Ok(serde_json::json!(schema))
    }
    /// Independent typed observation of shared UI drag/transition state.
    pub fn audit_ui_input(&mut self)->Result<serde_json::Value> {
        let plan=self.plan.as_ref().ok_or("No current control plan")?;
        let r=&mut self.reflection;check(r,&plan.ui.checks)?;
        let object=plan.ui.object;let class=r.class(r.pointer(object)?)?;
        let mut checks=Vec::new();let mut values=serde_json::Map::new();
        for name in ["bFirstDrag","bSkillCDDragged","bDragSnapCancel","iDragSnapDragParm","iDragSnapSkillID","m_iPressSkillID"] {
            if let Some(f)=class.fields.get(name) {
                if f.is_static || ![2,8].contains(&f.kind) || f.offset>1048576 {return Err("UI diagnostic field type changed".into());}
                let raw=capture(r,&mut checks,object+f.offset,if f.kind==2{1}else{4})?;
                let value=if f.kind==2 {if raw[0]>1{return Err("Invalid UI diagnostic boolean".into());}serde_json::json!(raw[0]!=0)} else {serde_json::json!(i32_at(&raw,0))};
                values.insert(name.into(),value);
            }
        }
        if let Some(f)=class.fields.get("curDragTransButton") {
            if f.kind!=18 || f.is_static {return Err("UI transition link type changed".into());}
            let raw=capture(r,&mut checks,object+f.offset,8)?;let child=u64_at(&raw,0);
            if child!=0 {
                let raw=capture(r,&mut checks,child,8)?;let c=r.class(u64_at(&raw,0))?;
                values.insert("transition_button_class".into(),serde_json::json!(c.name));
            }
        }
        check(r,&checks)?;check(r,&plan.ui.checks)?;Ok(serde_json::json!(values))
    }
    /// Read-only local visual anchors, independently bracketed for diagnostics.
    pub fn audit_local_geometry(&mut self)->Result<serde_json::Value> {
        let plan=self.plan.as_ref().ok_or("No local plan")?;
        check(&self.reflection,&plan.match_checks)?;
        let show=plan.match_owners[2];let r=&mut self.reflection;let mut checks=Vec::new();let mut result=Vec::new();
        let mut objects=vec![("show".to_owned(),show,0)];let mut cursor=0;
        while cursor<objects.len() && cursor<24 {
            let(role,object,depth)=objects[cursor].clone();cursor+=1;
            let mut class=r.class(r.pointer(object)?)?;
            let mut values=serde_json::Map::new();let mut schema=Vec::new();
            for _ in 0..16 {
                for(name,f)in &class.fields {
                    if f.is_static || f.offset>65536 {continue;}
                    let lower=name.to_ascii_lowercase();
                    if role=="show"{schema.push(serde_json::json!({"name":name,"kind":f.kind}));}
                    if f.kind==17 && (lower.contains("pos") || lower.contains("offset") || lower.contains("height")) {
                        let raw=capture(r,&mut checks,object+f.offset,12)?;
                        if let Ok(v)=vec3(&raw){values.insert(name.clone(),serde_json::json!(v));}
                    }
                    if f.kind==18 && depth<2 && (lower.contains("trans") || lower.contains("model") || lower.contains("gameobject")) {
                        let raw=capture(r,&mut checks,object+f.offset,8)?;let child=u64_at(&raw,0);
                        if child==0{continue;}
                        if let Ok(c)=r.pointer(child).and_then(|a|r.class(a)) {
                            values.insert(format!("{name}.class"),serde_json::json!(c.name));
                            for component in [false,true] {
                                if let Ok(path)=Path::new(r,child,component) {
                                    let raw=capture_path(r,&mut checks,&path)?;
                                    if let Some(p)=math::trs_chain(&raw,None){values.insert(format!("{name}.world"),serde_json::json!(p));break;}
                                }
                            }
                            if !objects.iter().any(|(_,o,_)|*o==child) && objects.len()<24 {objects.push((format!("{role}.{name}"),child,depth+1));}
                        }
                    }
                }
                if class.parent==0{break}class=r.class(class.parent)?;
            }
            result.push(serde_json::json!({"role":role,"values":values,"schema":schema}));
        }
        check(r,&checks)?;check(r,&plan.match_checks)?;
        Ok(serde_json::json!(result))
    }
    pub fn audit_control_state(&mut self)->Result<serde_json::Value> {
        let plan=self.plan.as_ref().ok_or("No current control plan")?;
        check(&self.reflection,&plan.ui.checks)?;
        let result=crate::control_audit::observe(&mut self.reflection,plan.ui.object,&plan.ui.buttons)?;
        check(&self.reflection,&plan.ui.checks)?;Ok(result)
    }
    fn prepare(&mut self) -> Result<Plan> {
        let r = &mut self.reflection;
        r.discover(&[
            "BattleManager",
            "LogicBattleManager",
            "GameMethod",
            "SkillIndicatorComp",
            "UIKeySkill",
        ])?;
        let mut checks = Vec::new();
        let battle_address = r.static_field("BattleManager", "Instance")?;
        let battle = r.pointer(battle_address)?;
        let logic_address = r.static_field("LogicBattleManager", "Instance")?;
        let manager = r.pointer(logic_address)?;
        checks.push((battle_address, battle.to_le_bytes().to_vec()));
        checks.push((logic_address, manager.to_le_bytes().to_vec()));
        let show = link(r, battle, "m_LocalPlayerShow", &mut checks)?;
        let actor = link(
            r,
            manager,
            "<m_LocalPlayerLogic>k__BackingField",
            &mut checks,
        )?;
        if show == 0 || actor == 0 {
            return Err("Waiting for local player".into());
        }
        checks.push((show, r.pointer(show)?.to_le_bytes().to_vec()));
        checks.push((actor, r.pointer(actor)?.to_le_bytes().to_vec()));
        let match_checks = checks.clone();
        let match_owners = [battle, manager, show, actor];
        let render = r.field(show, "m_vCachePosition")?;
        let logic = r.field(actor, "m_dRealPosX")?;
        let hero = r.field(actor, "m_ID")?;
        if r.field(actor, "m_dRealPosY")? != logic + 8 {
            return Err("Logical actor coordinates changed layout".into());
        }
        let camera_address = r.static_field("GameMethod", "m_Camra")?;
        let camera_object = r.pointer(camera_address)?;
        checks.push((camera_address, camera_object.to_le_bytes().to_vec()));
        let native = link(r, camera_object, "m_CachedPtr", &mut checks)?;
        let offset = math::matrix_offset(&r.read(native, 4096)?, false)
            .ok_or("Waiting for native game camera")?;
        let camera = native + offset as u64;
        let camera_path = Path::new(r, camera_object, true)?;
        let indicator_address = r.static_field("SkillIndicatorComp", "Instance")?;
        let indicator = r.pointer(indicator_address)?;
        checks.push((indicator_address, indicator.to_le_bytes().to_vec()));
        let skill = r.field(indicator, "m_iCurSkillID")?;
        let clock = r.field(manager, "m_uiFrameTime")?;
        let ui = UiPlan::new(r, show)?;
        let mut motions = std::collections::HashMap::new();
        for id in [2510, 2511, 2520, 20020, 20150, 8410] {
            if let Ok(m) = crate::aim::MotionPlan::discover(r, indicator, id) {
                motions.insert(id, m);
            }
        }
        Ok(Plan {
            checks,
            match_checks,
            match_owners,
            wall_paths:Default::default(),
            render,
            logic,
            hero,
            camera,
            camera_path,
            indicator,
            skill,
            clock,
            ui,
            motions, attack_mode:None,attack_mode_retry:0,attack_motion:None,
            behaviors: Default::default(),
            behavior_retry: Default::default(),
        })
    }
    pub fn sample(&mut self, now: i64, size: [f64; 2]) -> Result<Sample> {
        let entry = std::time::Instant::now();
        if self.plan.is_none() {
            // Retain input continuity across UI/camera plan changes, but revoke
            // it if the actual match owners disappeared (including pooled
            // pointer reuse after an observed inactive interval).
            if check(&self.reflection, &self.match_checks).is_err() {
                self.match_epoch.invalidate();
                self.match_checks.clear();
            }
            let plan = self.prepare()?;
            self.match_checks = plan.match_checks.clone();
            self.plan = Some(plan);
            self.generation += 1;
        }
        let begin = now.saturating_add(entry.elapsed().as_nanos().min(i64::MAX as u128) as i64);
        let result = self.sample_plan(begin, size);
        if result.is_err() {
            let match_lost = check(&self.reflection, &self.match_checks).is_err();
            if match_lost {
                self.match_epoch.invalidate();
                self.match_checks.clear();
            }
            if match_lost
                || self.plan.as_ref().is_some_and(|p| {
                    check(&self.reflection, &p.checks).is_err()
                        || check(&self.reflection, &p.ui.checks).is_err()
                        || p.camera_path.validate(&self.reflection).is_err()
                })
            {
                self.plan = None;
            }
        }
        result
    }
    fn sample_plan(&mut self, now: i64, size: [f64; 2]) -> Result<Sample> {
        let r = &mut self.reflection;
        let plan = self.plan.as_mut().ok_or("Source plan unavailable")?;
        let phase = std::time::Instant::now();
        let mut consumed = plan.checks.clone();
        check(r, &plan.checks)?;
        let hero = integer(r, &mut consumed, plan.hero)?;
        if !(1..10000).contains(&hero) {
            return Err("Invalid hero identity".into());
        }
        let camera = capture(r, &mut consumed, plan.camera, 128)?;
        if math::matrix_offset(&camera, false) != Some(0) {
            return Err("Native game camera reset".into());
        }
        let (view, projection) = unity::matrices(&camera)?;
        if !math::camera_pose(&view, &capture_path(r, &mut consumed, &plan.camera_path)?) {
            return Err("Native camera pose does not match its matrix".into());
        }
        let render = vec3(&capture(r, &mut consumed, plan.render, 12)?)?;
        let raw = capture(r, &mut consumed, plan.logic, 16)?;
        let logic =
            std::array::from_fn(|i| f64::from_le_bytes(raw[i * 8..i * 8 + 8].try_into().unwrap()));
        if logic.iter().any(|v| !v.is_finite() || v.abs() > 1e6) {
            return Err("Invalid logical actor coordinates".into());
        }
        let skill = integer(r, &mut consumed, plan.skill)?;
        let clock = integer(r, &mut consumed, plan.clock)? as u32;
        // Commit clock/identity only after the whole read bracket succeeds.
        // A rejected concurrent sample must not advance or reset input epoch.
        let mut next_epoch = self.match_epoch;
        let epoch = next_epoch.observe(
            MatchIdentity {
                owners: plan.match_owners,
                hero,
            },
            clock,
        );
        let token = format!("{}:{}", self.token, epoch);
        let (ui, mut skill_state, cancel_state) =
            plan.ui
                .sample(r, hero, &token, now, size, clock, &mut consumed)?;
        if plan.ui.linked.as_ref().is_some_and(|p|p.validate(r).is_err()) {plan.ui.linked=None;plan.ui.linked_retry=0;}
        if plan.ui.linked.is_none() && now>=plan.ui.linked_retry {
            plan.ui.linked=crate::control_audit::CurvePlan::discover(r,plan.ui.object,&plan.ui.buttons).ok();
            plan.ui.linked_retry=now+1_000_000_000;
        }
        let linked=if let Some(p)=&plan.ui.linked {let(mut v,c)=p.sample(r)?;v.pressed_slot=(!ui.active_slot.as_str().is_empty()).then(||ui.active_slot.owned());
            for i in 0..2 {
                v.base_skills[i]=ui.buttons.iter().take(ui.button_count.min(32) as usize)
                    .find(|b|b.slot.as_str()==v.endpoints[i]).map(|b|b.skill_id.int()).unwrap_or(0);
                v.on_cooldown[i]=ui.cooldown_state.buttons.iter().take(ui.cooldown_state.count.min(32) as usize)
                    .any(|b|b.slot.as_str()==v.endpoints[i] && b.ready.get() && b.on_cooldown.get());
            }
            consumed.extend(c);Some(v)} else {None};
        let mut control_types=std::collections::BTreeMap::new();
        let mut control_holds = std::collections::BTreeMap::new();
        let checkpoint=consumed.len();
        let mut attack_mode=plan.attack_mode.as_ref().and_then(|p|p.sample(r,&mut consumed).ok());
        if attack_mode.is_none() {
            consumed.truncate(checkpoint);plan.attack_mode=None;
            if now>=plan.attack_mode_retry {
                plan.attack_mode=crate::attack::ModePlan::discover(r,hero).ok();
                plan.attack_mode_retry=now+20_000_000;
                attack_mode=plan.attack_mode.as_ref().and_then(|p|p.sample(r,&mut consumed).ok());
                if attack_mode.is_none(){consumed.truncate(checkpoint);}
            }
        }
        let plain_owner=plan.ui.buttons.iter().find(|b|b.slot=="attack")
            .and_then(|b|r.pointer(b.object).and_then(|c|r.class(c)).ok()).is_some_and(|c|c.name=="CommonAttack");
        let aimed_attack=attack_mode==Some(true)&&plain_owner;
        if let Some(mode)=attack_mode {control_types.insert("attack".into(),if mode&&plain_owner {3}else{1});}

        let mut selection_observations=Vec::new();
        let mut selection_variant_unknown=false;
        for button in plan.ui.buttons.iter_mut().filter(|button| button.kind == "skill") {
            if button.charge.is_none() && now >= button.charge_retry {
                button.charge = crate::control_audit::ChargePlan::discover(r, button.object, button.data, plan.match_owners[2]).ok();
                button.charge_retry = now.saturating_add(1_000_000_000);
            }
            if let Some(value) = button.charge.as_ref().and_then(|charge| charge.sample(r, &mut consumed)) {
                control_holds.insert(button.slot.clone(), value);
            }
            if button.choice_family {
                // Family identity is fenced by the owned UiPlan. Optional
                // feedback/form failure must never fall into generic Range or
                // instant Tap. A verified future template may override4 below.
                let phase=retain_choice_family(&ui,&skill_state,&button.slot,&mut control_types);
                let Some(selector_phase)=phase else{selection_variant_unknown=true;continue;};
                if button.choice.as_ref().is_some_and(|choice|choice.validate(r).is_err()) {
                    button.choice=None;button.choice_touch=None;
                }
                if button.choice.is_none()&&now>=button.choice_retry {
                    button.choice=crate::control_audit::ChoicePlan::discover(r,plan.ui.object,button.object,button.data,plan.match_owners[2]).ok();
                    button.choice_retry=now.saturating_add(1_000_000_000);
                }
                if let Some(observation)=button.choice.as_ref().and_then(|choice|choice.sample(r,&mut consumed)) {
                    let current=ui.buttons.iter().take(ui.button_count.min(32)as usize).find(|b|b.slot.as_str()==button.slot)
                        .is_some_and(|b|b.skill_id.int()==observation.base);
                    let same_form=skill_state.buttons.iter().take(skill_state.count.min(32)as usize).find(|v|v.slot.as_str()==button.slot)
                        .is_some_and(|v|v.base_skill_id.int()==observation.base&&v.active_skill_id.int()==observation.active);
                    if current&&same_form {
                        if selector_phase {
                            if observation.current&&!button.choice_touch_held {
                                button.choice_touch_retry=0;button.choice_touch_attempts=0;
                            }
                            if button.choice_touch.as_ref().is_some_and(|touch|touch.skill!=observation.active||touch.validate(r).is_err()) {
                                button.choice_touch=None;button.choice_touch_retry=0;button.choice_touch_attempts=0;
                            }
                            // Resolve small typed plans while idle too. A cold
                            // first MOVE must not inherit an idle one-second
                            // retry deadline when the native registry appears.
                            if button.choice_touch.is_none()&&now>=button.choice_touch_retry {
                                button.choice_touch=crate::aim::SelectionTouchPlan::discover(r,plan.indicator,observation.active).ok();
                                button.choice_touch_attempts=button.choice_touch_attempts.saturating_add(1);
                                button.choice_touch_retry=now.saturating_add(selection_plan_retry_delay(button.choice_touch_attempts,observation.current));
                            }
                        }
                        button.choice_touch_held=observation.current&&selector_phase;
                        selection_observations.push((button.slot.clone(),observation));
                    }
                }
            }
            if button.target_family {
                retain_choice_family(&ui,&skill_state,&button.slot,&mut control_types);
            }
        }
        // Charge flags share the same owner-fenced bracket as the local
        // skill variants. Missing/contradictory native flags remain unknown.
        for variant in skill_state.buttons.iter_mut().take(skill_state.count.min(32) as usize) {
            variant.charge_skill = control_holds.get(variant.slot.as_str())
                .copied().map(flag).unwrap_or_default();
        }
        for entry in ui.buttons.iter().take(ui.button_count.min(32) as usize).filter(|b|b.kind.as_str()=="skill" || b.kind.as_str()=="extra") {
            let choice_family=plan.ui.buttons.iter().any(|b|b.slot==entry.slot.as_str()&&(b.choice_family||b.target_family));
            if choice_family
                &&choice_selector_phase(&ui,&skill_state,entry.slot.as_str())!=Some(false){continue;}
            let id=skill_state.buttons.iter().take(skill_state.count.min(32) as usize)
                .find(|v|v.slot.as_str()==entry.slot.as_str() && v.base_skill_id.int()==entry.skill_id.int())
                .map(|v|v.active_skill_id.int()).filter(|id|*id>0).unwrap_or(entry.skill_id.int());
            let id=if !choice_family {held_indicator_behavior_id(entry.skill_id.int(),id,skill,
                control_holds.get(entry.slot.as_str())==Some(&true),ui.active_slot.as_str()==entry.slot.as_str())}else{id};
            if id<=0 {continue;}
            if !choice_family && hero == 84 && entry.slot.as_str() == "skill:2" && id == 8430 {
                // Ling's ultimate rotates its blade landing arrangement even
                // though its generic package is also used by plain tap skills.
                control_types.insert(entry.slot.owned(), 2);
                continue;
            }
            if verified_button_only_form(hero,entry.slot.as_str(),entry.skill_id.int(),id) {
                // Explicitly verified recasts/ultimates are button presses,
                // even when their original ability has directional geometry.
                control_types.insert(entry.slot.owned(), 1);
                continue;
            }
            if plan.behaviors.get(&id).is_some_and(|(c,_)|check(r,c).is_err()) {
                plan.behaviors.remove(&id);plan.behavior_retry.remove(&id);
            }
            let held_current=ui.active_slot.as_str()==entry.slot.as_str() && skill==id;
            if !plan.behaviors.contains_key(&id) && behavior_retry_due(now,*plan.behavior_retry.get(&id).unwrap_or(&0),held_current) {
                if let Ok(value)=crate::aim::MotionPlan::discover_behavior(r,plan.indicator,id) {plan.behaviors.insert(id,value);}
                else {plan.behavior_retry.insert(id,now.saturating_add(behavior_retry_delay(held_current)));}
            }
            if let Some((checks,kind))=plan.behaviors.get(&id) {
                consumed.extend(checks.iter().cloned());
                control_types.insert(entry.slot.owned(),if choice_family{choice_control_type(Some(false),Some(*kind))}else{*kind});
            }
        }
        let point =
            math::project(&view, &projection, render, false).ok_or("Local player behind camera")?;
        let mut frame = s::Frame {
            ready: flag(true),
            hero_id: number(hero as f64),
            time_ns: number(now as f64),
            sample_duration_ms: number(0.),
            game_clock_ms: number(clock as f64),
            skill_id: number(skill as f64),
            token: text(&token),
            token_nonempty: flag(true),
            token_string: flag(true),
            view_matrix: matrix(view),
            hero_render_xyz: v3(render),
            hero_screen_uv: v2(point),
            ui,
            skill_state,
            cancel_state,
            ..Default::default()
        };
        frame.auxiliary_controls=self.auxiliary.sample(r,MatchIdentity{owners:plan.match_owners,hero},epoch,plan.ui.object,
            plan.ui.buttons.iter().find(|b|b.kind=="attack").map(|b|b.object).unwrap_or(0),&token,now,&mut consumed);
        // Read idle Arc geometry too, so even an immediate key release has
        // current visual intent before the native DOWN preview appears.
        if hero==84 && !plan.motions.contains_key(&8410) {
            if let Ok(m)=crate::aim::MotionPlan::discover(r,plan.indicator,8410){plan.motions.insert(8410,m);}
        }
        let mut direction_visual=if hero==84 {
            plan.motions.get_mut(&8410).and_then(|m| {
                let mut checks=Vec::new();
                let value=match m.sample_direction_visual(r,render,skill==8410,&mut checks) {
                    Ok(value)=>value,
                    Err(_)=>{
                        // Pooled preview transforms may disappear during cancel.
                        // Revoke optional visual feedback, never the movement UI.
                        checks.clear();m.sample_direction_visual(r,render,false,&mut checks).ok().flatten()
                    }
                };
                if value.is_some(){consumed.extend(checks);}value
            })
        }else{None};
        if let Some(spec)=direction_visual.as_mut() {
            let g=&frame.ui.joystick;let resolution=frame.ui.resolution_px.values.map(|n|n.value);
            if g.ready.get()&&g.radius_uv.valid() {
                let radius=(g.radius_uv.values[0].value*resolution[0]+g.radius_uv.values[1].value*resolution[1])*0.5;
                if radius>4. {spec.angular_cell=2f64.sqrt()/radius;}
            }
        }
        // The collision root supplies the marker's scene plane even before
        // DOWN and during a MISS. Caster wall height is not that plane.
        if let Some(spec)=direction_visual.as_mut() {
            if let Some(q)=plan.ui.buttons.iter().find(|b|b.slot=="skill:0") {
                let mut proof=Vec::new();
                let surface=(||->Result<f64>{
                    let at=r.field(q.object,"collidRoot")?;let object=pointer(r,&mut proof,at)?;
                    if object==0{return Err("No collision plane root".into());}
                    let class=pointer(r,&mut proof,object)?;
                    if r.class(class)?.name!="GameObject"{return Err("Unknown collision plane root".into());}
                    let at=r.field(object,"m_CachedPtr")?;let native=pointer(r,&mut proof,at)?;
                    if native==0{return Err("Destroyed collision root".into());}
                    let key=(object,native);
                    if plan.wall_paths.get(&key).is_none_or(|p|p.validate(r).is_err()) {
                        if plan.wall_paths.len()>=32{plan.wall_paths.clear();}
                        plan.wall_paths.insert(key,Path::new(r,object,false)?);
                    }
                    let raw=capture_path(r,&mut proof,&plan.wall_paths[&key])?;
                    let origin=math::trs_chain(&raw,None).ok_or("Invalid collision root")?;
                    let mut horizontal=0;
                    for axis in [[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]] {
                        let p=math::trs_chain(&raw,Some(axis)).ok_or("Invalid plane basis")?;
                        if (p[1]-origin[1]).abs()<0.001{horizontal+=1;}
                    }
                    if horizontal!=2{return Err("Non-horizontal collision root".into());}
                    check(r,&proof)?;Ok(origin[1]+spec.envelope_end[1])
                })();
                if let Ok(y)=surface {consumed.extend(proof);spec.surface_y=Some(y);}
            }
        }
        if skill==8410 {
            if let Some(spec)=direction_visual.as_mut().filter(|s|s.observed) {
                if let Some(q)=plan.ui.buttons.iter().find(|b|b.slot=="skill:0") {
                    let mut checks=Vec::new();
                    let hit=(||->Result<crate::projection::WallHit>{
                        let at=r.field(q.object,"m_CheakWall")?;
                        if !boolean(r,&mut checks,at)? {return Err("No native wall hit".into());}
                        let at=r.field(q.object,"vPos")?;let point=vec3(&capture(r,&mut checks,at,12)?)?;
                        let target=spec.target.ok_or("No wall endpoint")?;
                        if (0..3).any(|i|(point[i]+spec.envelope_end[i]-target[i]).abs()>0.02){return Err("Wall selection/visual not yet coherent".into());}
                        let at=r.field(q.object,"currentCollidWall")?;let wall=pointer(r,&mut checks,at)?;
                        if wall==0{return Err("No native wall owner".into());}
                        let class=pointer(r,&mut checks,wall)?;
                        if r.class(class)?.name!="GameObject"{return Err("Unexpected wall owner".into());}
                        let at=r.field(wall,"m_CachedPtr")?;let native=pointer(r,&mut checks,at)?;
                        if native==0{return Err("Destroyed wall".into());}
                        let key=(wall,native);
                        if plan.wall_paths.get(&key).is_none_or(|p|p.validate(r).is_err()) {
                            if plan.wall_paths.len()>=32{plan.wall_paths.clear();}
                            plan.wall_paths.insert(key,Path::new(r,wall,false)?);
                        }
                        let raw=capture_path(r,&mut checks,&plan.wall_paths[&key])?;
                        let mut pose=[0.;12];
                        for (i,local) in [None,Some([1.,0.,0.]),Some([0.,1.,0.]),Some([0.,0.,1.])].into_iter().enumerate(){
                            let p=math::trs_chain(&raw,local).ok_or("Invalid wall pose")?;pose[i*3..i*3+3].copy_from_slice(&p);
                        }
                        let at=r.field(q.object,"SphereCastRadius")?;let radius=f32_at(&capture(r,&mut checks,at,4)?,0);
                        if !radius.is_finite()||radius<=0.||radius>100.{return Err("Invalid wall query radius".into());}
                        let at=r.field(q.object,"m_fCheckWidth")?;let width=f32_at(&capture(r,&mut checks,at,4)?,0);
                        if !width.is_finite()||width<0.||width>radius*2.{return Err("Invalid native wall check width".into());}
                        let at=r.field(q.object,"currentDirect")?;let direction=vec3(&capture(r,&mut checks,at,12)?)?;
                        let norm=direction[0].hypot(direction[2]);
                        if (norm-1.).abs()>0.01||direction[1].abs()>0.01{return Err("Native wall query direction unavailable".into());}
                        check(r,&checks)?;
                        Ok(crate::projection::WallHit{owner:wall,native,pose,point:target,radius,width,direction:[direction[0]/norm,direction[2]/norm]})
                    })();
                    if let Ok(hit)=hit{consumed.extend(checks);spec.surface_y=Some(hit.point[1]);spec.wall=Some(hit);}
                }
            }
        }
        let mut aim = None;
        let attack_binding=frame.ui.active_slot.as_str()=="attack"&&frame.skill_state.ready.get()
            && frame.skill_state.buttons.iter().take(frame.skill_state.count.min(32)as usize)
                .any(|v|v.slot.as_str()=="attack"&&v.active_skill_id.int()==skill&&skill>0);
        if aimed_attack && attack_binding {
            if plan.attack_motion.as_ref().is_none_or(|m|m.skill!=skill||m.validate(r).is_err()) {
                plan.attack_motion=crate::aim::MotionPlan::discover_attack(r,plan.indicator,skill,frame.ui.buttons.iter().take(frame.ui.button_count.min(32)as usize).find(|b|b.slot.as_str()=="attack").map(|b|b.skill_id.int()).unwrap_or(0)).ok();
            }
            let checkpoint=consumed.len();
            if let Some(m)=plan.attack_motion.as_mut() {
                m.refresh_visual(r);
                match m.sample_bracketed_with_fixed_range(r,&mut frame,&mut consumed,false) {
                    Ok(value)=>aim=value,
                    Err(_)=>{consumed.truncate(checkpoint);m.unavailable(&mut frame);plan.attack_motion=None;}
                }
            }
        } else if skill > 0 {
            if !plan.motions.contains_key(&skill) {
                if let Ok(m) = crate::aim::MotionPlan::discover(r, plan.indicator, skill) {
                    plan.motions.insert(skill, m);
                }
            }
            if let Some(m) = plan.motions.get_mut(&skill) {
                m.refresh_visual(r);
                let allow_fixed_range=control_holds.get(frame.ui.active_slot.as_str())==Some(&false);
                match m.sample_bracketed_with_fixed_range(r, &mut frame, &mut consumed,allow_fixed_range) {
                    Ok(p) => aim = p,
                    Err(_) => {
                        m.unavailable(&mut frame);
                        if m.validate(r).is_err() {
                            plan.motions.remove(&skill);
                        }
                    }
                }
            }
        }
        let direction_origin=if hero==31 && skill==3190 && frame.ui.active_slot.as_str()=="skill:2" {
            let checkpoint=consumed.len();
            let value=plan.motions.get_mut(&skill).and_then(|m|m.sample_direction_origin(r,&mut consumed).ok().flatten());
            if value.is_none(){consumed.truncate(checkpoint);}value
        }else{None};
        // Choice feedback is native UI XY, never a world-space Direction ACK.
        frame.selection_input.reason=text(if selection_variant_unknown{"choice_variant_unknown"}else{"owner_receipt_unavailable"});
        if let Some(button)=plan.ui.buttons.iter().filter(|b|b.choice_family).min_by_key(|b|{
            if b.slot==frame.ui.active_slot.as_str(){0}else if b.slot==plan.ui.last_choice_slot{1}else{2}
        }) {frame.selection_input.kind=number(1.);frame.selection_input.slot=text(&button.slot);}
        if selection_observations.iter().any(|(slot,_)|slot==frame.ui.active_slot.as_str()) {
            if plan.ui.last_choice_slot!=frame.ui.active_slot.as_str(){plan.ui.last_choice_slot=frame.ui.active_slot.owned();}
        }
        if let Some((slot,observation))=selection_observations.into_iter().min_by_key(|(slot,_)| {
            if slot==frame.ui.active_slot.as_str(){0}else if slot==&plan.ui.last_choice_slot{1}else{2}
        }) {
            let selected=observation.selected;
            let unit=observation.direction[0].hypot(observation.direction[1]);
            let direction_valid=(unit-1.).abs()<=0.01&&observation.direction[2].abs()<=0.01;
            let mut selection=s::SelectionInput {
                kind:number(1.),ready:flag(true),coherent:flag(true),input_ready:flag(false),
                reason:text(if observation.active==observation.base{"inactive_choice"}else{"selector_phase_complete"}),
                panel_visible:flag(observation.panel),has_drag:flag(observation.drag),
                selected_option_valid:flag(selected.is_some()),source:text("KeySkillMultiChoice.m_lastDragDir"),
                token:frame.token,slot:text(&slot),hero_id:frame.hero_id,
                base_skill_id:number(observation.base as f64),active_skill_id:number(observation.active as f64),
                native_skill_id:frame.skill_id,owner_id:observation.owner,
                sample_start_ns:number(now as f64),sample_end_ns:number(now as f64),hover_index:number(observation.hover as f64),
                selected_option:selected.map(|v|number(v as f64)).unwrap_or_default(),
                drag_direction_xy:if direction_valid {v2([observation.direction[0],observation.direction[1]])}else{Default::default()},
                ..Default::default()
            };
            if observation.active==observation.base&&observation.current&&slot==frame.ui.active_slot.as_str()&&skill==observation.active {
                selection.reason=text("awaiting_native_touch");
                if let Some(button)=plan.ui.buttons.iter_mut().find(|button|button.slot==slot&&button.choice_family) {
                    let checkpoint=consumed.len();
                    if let Some(raw)=button.choice_touch.as_ref().and_then(|touch|touch.sample(r,&mut consumed).ok()) {
                        selection.raw_touch_xyz=v3(raw);
                        let ready=observation.panel&&observation.drag&&selected.is_some()&&direction_valid;
                        selection.input_ready=flag(ready);
                        selection.reason=text(if ready{"ready"}else{"invalid_ui_feedback"});
                    }else{consumed.truncate(checkpoint);}
                }
            }
            frame.selection_input=selection;
        }
        // Absolute native UI targets share neither Choice's radial XY goal nor
        // any world-space indicator. Their raw receipt comes from the owned
        // persistent native contact registry, including native skill ID zero.
        for button in plan.ui.buttons.iter_mut().filter(|b|b.target_family) {
            let phase=choice_selector_phase(&frame.ui,&frame.skill_state,&button.slot);
            let mut target=s::SelectionInput {kind:number(2.),slot:text(&button.slot),
                reason:text(if phase.is_none(){"target_variant_unknown"}else{"target_owner_unavailable"}),..Default::default()};
            if phase.is_some() {
                if button.target.as_ref().is_some_and(|p|p.validate(r).is_err()){button.target=None;button.target_retry=0;}
                if button.target.is_none()&&now>=button.target_retry {
                    button.target=crate::target_selection::TargetPlan::discover(r,plan.ui.object,button.object,button.data,plan.match_owners[2],plan.ui.camera_object).ok();
                    button.target_retry=now.saturating_add(20_000_000);
                }
                let checkpoint=consumed.len();
                if let Some(plan)=button.target.as_mut() {
                    match plan.sample(r,&frame,&button.slot,now,&mut consumed) {
                        Ok(value)=>{
                            let matching=frame.skill_state.buttons.iter().take(frame.skill_state.count.min(32)as usize)
                                .find(|v|v.slot.as_str()==button.slot)
                                .is_some_and(|v|v.base_skill_id.int()==value.base_skill_id.int()&&v.active_skill_id.int()==value.active_skill_id.int());
                            if matching{target=value;}else{consumed.truncate(checkpoint);}
                        }
                        Err(_)=>{consumed.truncate(checkpoint);target.reason=text("target_optional_read_unknown");}
                    }
                }
            }
            if target.slot.as_str()==frame.ui.active_slot.as_str(){plan.ui.last_choice_slot=target.slot.owned();}
            let priority=|slot:&str|if slot==frame.ui.active_slot.as_str(){0}else if slot==plan.ui.last_choice_slot{1}else{2};
            if frame.selection_input.kind.missing()||priority(target.slot.as_str())<priority(frame.selection_input.slot.as_str()) {
                frame.selection_input=target;
            }
        }
        let mut target_cancel_error=None;
        let target_cancel=if frame.selection_input.kind.int()==2&&frame.selection_input.ready.get()&&frame.selection_input.coherent.get()&&frame.selection_input.panel_visible.get() {
            plan.ui.buttons.iter_mut().find(|b|b.target_family&&b.slot==frame.selection_input.slot.as_str()).and_then(|b|b.target.as_mut()).and_then(|target| {
                let checkpoint=consumed.len();match target.cancel_uv(r,now,&mut consumed) {
                    Ok(point)=>Some(point),Err(error)=>{consumed.truncate(checkpoint);target_cancel_error=Some(error);None}
                }
            })
        }else{None};
        attach_target_cancel(&mut frame,target_cancel);
        if let Some(error)=target_cancel_error {
            let reason=format!("{};cancel_geometry_unknown:{error}",frame.selection_input.reason.as_str());
            frame.selection_input.reason=if reason.len()<=256{text(&reason)}else{text("target_cancel_geometry_unknown")};
        }
        // A verified direction joystick may need its first MOVE before the
        // motion publishes a unit vector. UI ownership/geometry are sampled
        // independently; missing feedback must not erase that bootstrap path.
        let direction_motion = plan.motions.get(&skill).is_some_and(|motion| {
            motion.is_direction_geometry() && motion.validate(r).is_ok()
        });
        // Covered sub-indicators are authorized by a fresh typed relationship
        // from this very button's base motion, never by a nearby numeric ID.
        set_covered_direction_binding(&mut frame, None);
        if direction_motion && control_types.get(frame.ui.active_slot.as_str()) == Some(&2) {
            let slot = frame.ui.active_slot.as_str();
            let button = frame.ui.buttons[..frame.ui.button_count.min(32) as usize].iter()
                .find(|button| button.slot.as_str() == slot && button.kind.as_str() == "skill" && button.active.get());
            if let Some(button) = button {
                let base = button.skill_id.int();
                let active = frame.skill_state.buttons[..frame.skill_state.count.min(32) as usize].iter()
                    .find(|row| row.slot.as_str() == slot && row.base_skill_id.int() == base)
                    .map(|row| row.active_skill_id.int()).unwrap_or(0);
                if base > 0 && skill > 0 && skill != base && skill != active {
                    let checkpoint = consumed.len();
                    let mut denied = "local_button_owner";
                    let binding = (|| -> Result<bool> {
                        let native = plan.ui.buttons.iter().find(|button| button.slot == slot && button.kind == "skill")
                            .ok_or("Covered direction button unavailable")?;
                        verify_local_covered_button(r, &mut consumed, native.object, native.data, plan.match_owners[2], base)?;
                        denied = "base_motion_contract";
                        if plan.motions.get(&base).is_some_and(|motion| motion.validate(r).is_err()) {
                            plan.motions.remove(&base);
                        }
                        if !plan.motions.contains_key(&base) {
                            let motion = crate::aim::MotionPlan::discover(r, plan.indicator, base)?;
                            plan.motions.insert(base, motion);
                        }
                        denied = "typed_cover_list";
                        plan.motions.get(&base).ok_or("Covered base motion unavailable")?
                            .covers_direction_skill(r, skill, &mut consumed)
                    })();
                    if binding == Ok(true) {
                        set_covered_direction_binding(&mut frame, Some((base, skill)));
                        if !frame.direction_input.covered_binding_ready.get() {
                            deny_covered_direction_binding(&mut frame, base, skill, "feedback_scope");
                        }
                    } else {
                        // Failed optional binding discovery cannot poison the
                        // independently valid camera/UI/native feedback sample.
                        consumed.truncate(checkpoint);
                        deny_covered_direction_binding(&mut frame, base, skill,
                            if binding == Ok(false) { "not_in_cover_list" } else { denied });
                    }
                }
            }
        }
        attach_owned_direction_geometry(&mut frame, &control_types, direction_motion);
        if skill==8410 && frame.direction_input.ready.get() {
            let query=plan.ui.buttons.iter().find(|b|b.slot=="skill:0").and_then(|q| {
                let at=r.field(q.object,"currentDirect").ok()?;
                let raw=capture(r,&mut consumed,at,12).ok()?;let v=vec3(&raw).ok()?;let n=v[0].hypot(v[2]);
                ((n-1.).abs()<0.01&&v[1].abs()<0.01).then_some([v[0]/n,v[2]/n])
            });
            if let Some(unit)=query {frame.direction_input.direction_xz=v2(unit);frame.direction_input.source=text("KeySkilleCheckMesh.currentDirect");}
            else {frame.direction_input.ready=flag(false);}
        }
        let portrait = if hero==104 {
            if plan.ui.portrait.as_ref().is_some_and(|p|p.validate(r).is_err()){plan.ui.portrait=None;plan.ui.portrait_retry=0;}
            if plan.ui.portrait.is_none() && now>=plan.ui.portrait_retry {
                plan.ui.portrait=match crate::portrait_picker::Plan::discover(r,plan.ui.object,&plan.ui.buttons,plan.match_owners[2],plan.ui.camera_object){Ok(p)=>Some(p),Err(e)=>{self.portrait_error=e;None}};
                plan.ui.portrait_retry=now.saturating_add(100_000_000);
            }
            let checkpoint=consumed.len();
            let observed=plan.ui.portrait.as_mut().and_then(|p|match p.sample(r,plan.ui.matrix,frame.ui.active_slot.as_str(),frame.ui.resolution_px.values.map(|v|v.value),&mut consumed){Ok(mut o)=>{
                if frame.ui.active_slot.as_str()==o.slot {
                    if frame.ui.cancel_uv.valid(){o.cancel_point=Some(frame.ui.cancel_uv.values.map(|v|v.value));}
                    let c=&frame.cancel_state;
                    if c.ready.get()&&c.coherent.get()&&c.slot.as_str()==o.slot&&c.cancelled.is_bool()
                        &&c.base_skill_id.int()==o.base {o.native_cancel=Some(c.cancelled.get());}
                }
                self.portrait_error.clear();Some(o)
            },Err(e)=>{self.portrait_error=e;None}});
            if observed.is_none(){consumed.truncate(checkpoint);}
            observed
        } else {None};
        check(r, &consumed)
            .map_err(|_| "Camera/actor/UI/native feedback changed during sample".to_string())?;
        let end = now.saturating_add(phase.elapsed().as_nanos().min(i64::MAX as u128) as i64);
        if end - now > 100_000_000 {
            return Err("Source coherence phase exceeded 100 ms".into());
        }
        stamp_frame(&mut frame, now, end);
        self.match_epoch = next_epoch;
        Ok(Sample {
            plan_revision: self.generation,
            game_clock_ms: clock,
            match_owners: plan.match_owners,
            frame,
            projection,
            logic,
            indicator: plan.indicator,
            aim,
            direction_visual, direction_origin,
            control_types,
            control_holds,
            linked, portrait,
        })
    }
}

/// All native receipts and the exact geometry used to interpret them share one
/// phase. Controller freshness/causality must see real boundaries, never a
/// fabricated zero-duration bracket or a pre-read end timestamp.
fn covered_instance_field<M: Memory>(r: &mut Reflection<M>, checks: &mut Checks,
    object: u64, name: &str, kind: u8) -> Result<u64> {
    if object == 0 { return Err("Covered direction object absent".into()); }
    let mut class = u64_at(&capture(r, checks, object, 8)?, 0);
    for _ in 0..24 {
        let metadata = r.class(class)?;
        if let Some(field) = metadata.fields.get(name) {
            if field.is_static || field.kind != kind || field.offset > 1_048_576 {
                return Err("Covered direction field type changed".into());
            }
            return object.checked_add(field.offset).ok_or("Covered direction address overflow".into());
        }
        if metadata.parent == 0 { break; }
        class = metadata.parent;
    }
    Err(format!("Covered direction field absent: {name}"))
}
fn verify_local_covered_button<M: Memory>(r: &mut Reflection<M>, checks: &mut Checks,
    button: u64, data: u64, local_show: u64, base: i32) -> Result<()> {
    let at = covered_instance_field(r, checks, button, "m_SkillData", 18)?;
    if data == 0 || u64_at(&capture(r, checks, at, 8)?, 0) != data {
        return Err("Covered data no longer owned by button".into());
    }
    let class = u64_at(&capture(r, checks, data, 8)?, 0);
    if r.class(class)?.name != "ShowSkillData" { return Err("Covered data type changed".into()); }
    let at = covered_instance_field(r, checks, data, "m_InitSkillID", 8)?;
    if i32_at(&capture(r, checks, at, 4)?, 0) != base { return Err("Covered base binding changed".into()); }
    let at = covered_instance_field(r, checks, local_show, "m_SkillComp", 18)?;
    let local = u64_at(&capture(r, checks, at, 8)?, 0);
    let at = covered_instance_field(r, checks, data, "m_SkillComp", 18)?;
    if local == 0 || u64_at(&capture(r, checks, at, 8)?, 0) != local {
        return Err("Covered data component is not local".into());
    }
    let class = u64_at(&capture(r, checks, local, 8)?, 0);
    if r.class(class)?.name != "ShowSkillComp" { return Err("Covered local component type changed".into()); }
    Ok(())
}
fn set_covered_direction_binding(frame: &mut s::Frame, binding: Option<(i32, i32)>) {
    let input = &mut frame.direction_input;
    input.covered_binding_ready = s::Flag::default();
    input.covered_base_skill_id = s::Number::default();
    input.covered_actual_skill_id = s::Number::default();
    input.covered_source = s::Text::default();
    if let Some((base, actual)) = binding {
        if input.ready.get() && (input.token.as_str() != frame.token.as_str()
            || input.hero_id.int() != frame.hero_id.int()
            || input.slot.as_str() != frame.ui.active_slot.as_str()
            || input.skill_id.int() != actual) {
            return;
        }
        input.covered_binding_ready = flag(true);
        input.covered_base_skill_id = number(base as f64);
        input.covered_actual_skill_id = number(actual as f64);
        input.covered_source = text("IndicatorMotionBase.lCoverSkills");
        // This scope also accompanies geometry when native unit feedback is
        // not ready. Neither readiness nor raw/direction values are fabricated.
        input.token = frame.token;
        input.hero_id = frame.hero_id;
        input.slot = frame.ui.active_slot;
        input.skill_id = number(actual as f64);
        input.sample_start_ns = frame.time_ns;
        input.sample_end_ns = frame.time_ns;
    }
}
/// Failed optional discovery is diagnostic evidence only. A stable reason code
/// records the attempted relationship without publishing ownership or input ACK.
fn deny_covered_direction_binding(frame: &mut s::Frame, base: i32, actual: i32, reason: &str) {
    let input = &mut frame.direction_input;
    input.covered_binding_ready = flag(false);
    input.covered_base_skill_id = number(base as f64);
    input.covered_actual_skill_id = number(actual as f64);
    input.covered_source = text(&format!("binding_unavailable:{reason}"));
}
fn current_covered_direction_binding(frame: &s::Frame, base: i32, actual: i32) -> bool {
    let input = &frame.direction_input;
    input.covered_binding_ready.get()
        && input.covered_source.as_str() == "IndicatorMotionBase.lCoverSkills"
        && input.covered_base_skill_id.is_number() && input.covered_base_skill_id.int() == base
        && input.covered_actual_skill_id.is_number() && input.covered_actual_skill_id.int() == actual
        && base > 0 && actual > 0
        && input.token.as_str() == frame.token.as_str() && input.hero_id.int() == frame.hero_id.int()
        && input.slot.as_str() == frame.ui.active_slot.as_str() && input.skill_id.int() == actual
        && input.sample_start_ns.kind == frame.time_ns.kind && input.sample_start_ns.value == frame.time_ns.value
        && input.sample_end_ns.kind == frame.time_ns.kind && input.sample_end_ns.value == frame.time_ns.value
}
/// Authorize a geometry handoff from this sample's already owned UI button.
/// The caller proves the actual native motion's Direction family and retains
/// its owner checks in the Source coherence fence. This creates no input ACK.
fn attach_owned_direction_geometry(
    frame: &mut s::Frame,
    types: &std::collections::BTreeMap<String, u8>,
    direction_motion_verified: bool,
) {
    let slot = frame.ui.active_slot.as_str();
    let actual = frame.skill_id;
    let g = frame.ui.joystick;
    let now = frame.time_ns;
    if !direction_motion_verified || types.get(slot) != Some(&2)
        || !frame.ready.get() || !frame.ui.ready.get()
        || !frame.token_string.get() || !frame.token_nonempty.get()
        || frame.token.as_str().is_empty() || slot.is_empty()
        || !frame.hero_id.is_number() || frame.hero_id.int() <= 0
        || frame.ui.hero_id.int() != frame.hero_id.int()
        || !actual.is_number() || actual.int() <= 0
        || !now.is_number() || !now.value.is_finite()
        || frame.ui.time_ns.kind != now.kind || frame.ui.time_ns.value != now.value
        || !g.ready.get() || g.source.as_str() != "active_widget_transform"
        || g.slot.as_str() != slot || !g.center_uv.valid() || !g.radius_uv.valid()
        || g.center_uv.values.iter().any(|v| !v.value.is_finite() || !(0. ..=1.).contains(&v.value))
        || g.radius_uv.values.iter().any(|v| !v.value.is_finite() || v.value <= 0. || v.value > 1.)
        || g.sample_start_ns.kind != now.kind || g.sample_start_ns.value != now.value
        || g.sample_end_ns.kind != now.kind || g.sample_end_ns.value != now.value
        || frame.ui.button_count > 32
    {
        return;
    }
    let mut buttons = frame.ui.buttons[..frame.ui.button_count as usize].iter()
        .filter(|button| button.slot.as_str() == slot);
    let Some(button) = buttons.next() else { return };
    if buttons.next().is_some() || !button.active.get()
        || !button.skill_id.is_number() || button.skill_id.int() <= 0
        || !g.skill_id.is_number() || g.skill_id.value != button.skill_id.value
    {
        return;
    }
    let actual_owned = match button.kind.as_str() {
        "skill" | "attack" => {
            let state = &frame.skill_state;
            if !state.ready.get() || state.count > 32
                || state.source.as_str() != "ShowSkillData.m_TranID"
                || state.token.as_str() != frame.token.as_str()
                || state.hero_id.int() != frame.hero_id.int()
                || state.sample_start_ns.kind != now.kind || state.sample_start_ns.value != now.value
                || state.sample_end_ns.kind != now.kind || state.sample_end_ns.value != now.value
            {
                return;
            }
            let mut rows = state.buttons[..state.count as usize].iter()
                .filter(|row| row.slot.as_str() == slot);
            let Some(row) = rows.next() else { return };
            rows.next().is_none() && row.base_skill_id.is_number()
                && row.base_skill_id.value == button.skill_id.value
                && row.active_skill_id.is_number() && row.active_skill_id.int() > 0
                && (actual.value == button.skill_id.value || actual.value == row.active_skill_id.value
                    || current_covered_direction_binding(frame, button.skill_id.int(), actual.int()))
        }
        "extra" => actual.value == button.skill_id.value,
        _ => false,
    };
    if actual_owned {
        frame.active_joystick = s::Geometry { skill_id: actual, ..g };
    }
}

fn stamp_frame(frame: &mut s::Frame, begin: i64, end: i64) {
    let start = number(begin as f64);
    let finish = number(end as f64);
    frame.time_ns = finish;
    frame.sample_duration_ms = number((end - begin) as f64 / 1e6);
    frame.ui.time_ns = finish;
    frame.skill_state.sample_start_ns = start;
    frame.skill_state.sample_end_ns = finish;
    frame.cancel_state.sample_start_ns = start;
    frame.cancel_state.sample_end_ns = finish;
    frame.ui.cooldown_state.sample_start_ns = start;
    frame.ui.cooldown_state.sample_end_ns = finish;
    frame.ui.joystick.sample_start_ns = start;
    frame.ui.joystick.sample_end_ns = finish;
    frame.active_joystick.sample_start_ns = start;
    frame.active_joystick.sample_end_ns = finish;
    frame.logical.sample_start_ns = start;
    frame.logical.sample_end_ns = finish;
    frame.logical.joystick.sample_start_ns = start;
    frame.logical.joystick.sample_end_ns = finish;
    frame.motion.sample_start_ns = start;
    frame.motion.sample_end_ns = finish;
    frame.direction_input.sample_start_ns = start;
    frame.direction_input.sample_end_ns = finish;
    if frame.selection_input.ready.get() {
        frame.selection_input.sample_start_ns=start;
        frame.selection_input.sample_end_ns=finish;
    }
    if frame.auxiliary_controls.ready.get() {
        frame.auxiliary_controls.sample_start_ns=start;
        frame.auxiliary_controls.sample_end_ns=finish;
    }
}

/// Keep Target cancellation local to its packet: an idle or concurrent Target
/// must never replace another skill's generic cancellation geometry or receipt.
fn attach_target_cancel(frame:&mut s::Frame,point:Option<[f64;2]>) {
    let selection=&mut frame.selection_input;
    if selection.kind.int()!=2{return;}
    selection.target_cancel_ready=Default::default();selection.target_cancel_uv=Default::default();
    if !selection.ready.get()||!selection.coherent.get()||!selection.panel_visible.get()
        ||selection.owner_id==0||selection.backend_owner_id==0{return;}
    if let Some(point)=point.filter(|point|unity::onscreen(*point)) {
        selection.target_cancel_ready=flag(true);selection.target_cancel_uv=v2(point);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    struct MutableMemory(RefCell<Vec<u8>>);
    impl Memory for MutableMemory {
        fn regions(&self) -> Result<Vec<crate::platform::Region>> {
            Ok(Vec::new())
        }
        fn read(&self, address: u64, length: usize) -> Result<Vec<u8>> {
            self.0
                .borrow()
                .get(address as usize..address as usize + length)
                .map(|v| v.to_vec())
                .ok_or("Invalid test memory".into())
        }
        fn read_into(&self, address: u64, bytes: &mut [u8]) -> Result<()> {
            bytes.copy_from_slice(&self.read(address, bytes.len())?);
            Ok(())
        }
    }
    #[test]
    fn optional_state_flag_is_unknown_when_absent_invalid_or_unreadable() {
        let r = Reflection::new(MutableMemory(RefCell::new(vec![0, 1, 2]))).unwrap();
        for address in [None, Some(2), Some(3)] {
            let retained = vec![(0, vec![0])];
            let mut reads = retained.clone();
            let observed = optional_flag(&r, &mut reads, address);
            assert!(observed.missing());
            assert_eq!(reads, retained);
            assert!(check(&r, &reads).is_ok());
        }
    }
    #[test]
    fn optional_state_flag_false_and_true_are_fenced_against_game_updates() {
        let r = Reflection::new(MutableMemory(RefCell::new(vec![0, 1]))).unwrap();
        for (address, expected) in [(0, false), (1, true)] {
            let mut reads = Vec::new();
            let observed = optional_flag(&r, &mut reads, Some(address));
            assert!(observed.is_bool());
            assert_eq!(observed.get(), expected);
            assert_eq!(reads, vec![(address, vec![expected as u8])]);
            assert!(check(&r, &reads).is_ok());
            r.memory.0.borrow_mut()[address as usize] = (!expected) as u8;
            assert!(check(&r, &reads).is_err());
            r.memory.0.borrow_mut()[address as usize] = expected as u8;
        }
    }
    fn cold_direction_geometry_fixture() -> (s::Frame, std::collections::BTreeMap<String, u8>) {
        let mut f = s::Frame::default();
        f.ready = flag(true); f.token = text("owned-match");
        f.token_string = flag(true); f.token_nonempty = flag(true);
        f.hero_id = number(126.); f.skill_id = number(12610.); f.time_ns = number(1000.);
        f.ui.ready = flag(true); f.ui.hero_id = f.hero_id; f.ui.time_ns = f.time_ns;
        f.ui.active_slot = text("skill:0"); f.ui.button_count = 1;
        f.ui.buttons[0] = s::Button {
            slot: text("skill:0"), kind: text("skill"), skill_id: number(12610.),
            active: flag(true), ..Default::default()
        };
        f.ui.joystick = s::Geometry {
            ready: flag(true), source: text("active_widget_transform"), slot: text("skill:0"),
            skill_id: number(12610.), sample_start_ns: f.time_ns, sample_end_ns: f.time_ns,
            center_uv: v2([0.8, 0.8]), radius_uv: v2([0.1, 0.1]),
        };
        f.skill_state = s::VariantState {
            ready: flag(true), source: text("ShowSkillData.m_TranID"), token: f.token,
            hero_id: f.hero_id, sample_start_ns: f.time_ns, sample_end_ns: f.time_ns,
            count: 1, ..Default::default()
        };
        f.skill_state.buttons[0] = s::Variant {
            slot: text("skill:0"), base_skill_id: number(12610.), active_skill_id: number(12611.),
            ..Default::default()
        };
        (f, [("skill:0".into(), 2)].into_iter().collect())
    }
    #[test]
    fn owned_direction_ui_geometry_survives_missing_native_unit_feedback_without_ack() {
        for actual in [12610., 12611.] {
            let (mut f, types) = cold_direction_geometry_fixture();
            f.skill_id = number(actual);
            attach_owned_direction_geometry(&mut f, &types, true);
            assert!(f.active_joystick.ready.get());
            assert_eq!(f.active_joystick.skill_id.value, actual);
            assert!(f.direction_input.ready.missing(), "geometry handoff must not fabricate a native receipt");
            assert!(f.direction_input.raw_touch_xyz.values.iter().all(|v| v.missing()));
            stamp_frame(&mut f, 1000, 1200);
            assert_eq!(f.active_joystick.sample_start_ns.value, 1000.);
            assert_eq!(f.active_joystick.sample_end_ns.value, 1200.);
        }
    }
    #[test]
    fn cold_direction_geometry_requires_current_motion_slot_form_and_source_ownership() {
        for case in 0..14 {
            let (mut f, mut types) = cold_direction_geometry_fixture();
            let mut motion_verified = true;
            match case {
                0 => motion_verified = false,
                1 => { types.insert("skill:0".into(), 3); }
                2 => f.ui.active_slot = text("skill:1"),
                3 => f.ui.joystick.slot = text("skill:1"),
                4 => f.ui.joystick.skill_id = number(12630.),
                5 => f.skill_id = number(777.),
                6 => f.skill_state.token = text("previous-match"),
                7 => f.ui.hero_id = number(25.),
                8 => f.ui.joystick.sample_start_ns = number(999.),
                9 => f.ui.joystick.source = text("estimated_widget"),
                10 => { f.ui.buttons[1] = f.ui.buttons[0]; f.ui.button_count = 2; }
                11 => { f.skill_state.buttons[1] = f.skill_state.buttons[0]; f.skill_state.count = 2; }
                12 => f.ui.buttons[0].active = flag(false),
                _ => f.ui.joystick.radius_uv.values[0] = number(f64::NAN),
            }
            attach_owned_direction_geometry(&mut f, &types, motion_verified);
            assert!(!f.active_joystick.ready.get(), "case {case}: unowned/stale/Range geometry must not bootstrap Direction");
            assert!(!f.direction_input.ready.get());
        }
    }
    #[test]
    fn covered_direction_binding_is_independent_of_unit_feedback_and_revoked_per_sample() {
        let (mut f, types) = cold_direction_geometry_fixture();
        f.skill_id = number(12612.);
        set_covered_direction_binding(&mut f, Some((12610, 12612)));
        attach_owned_direction_geometry(&mut f, &types, true);
        assert!(f.active_joystick.ready.get());
        assert_eq!(f.active_joystick.skill_id.int(), 12612);
        assert!(f.direction_input.covered_binding_ready.get());
        assert!(!f.direction_input.ready.get(), "cover membership cannot fabricate a unit-vector receipt");
        assert!(f.direction_input.raw_touch_xyz.values.iter().all(|value| value.missing()));
        set_covered_direction_binding(&mut f, None);
        f.active_joystick = Default::default();
        attach_owned_direction_geometry(&mut f, &types, true);
        assert!(!f.active_joystick.ready.get());
        assert!(f.direction_input.covered_binding_ready.missing());
        assert!(f.direction_input.covered_base_skill_id.missing());
        assert!(f.direction_input.covered_actual_skill_id.missing());
        assert!(f.direction_input.covered_source.missing());
    }
    #[test]
    fn covered_source_geometry_does_not_accept_foreign_or_stale_binding_scope() {
        for case in 0..10 {
            let (mut f, mut types) = cold_direction_geometry_fixture();
            f.skill_id = number(12612.);
            set_covered_direction_binding(&mut f, Some((12610, 12612)));
            match case {
                0 => f.direction_input.covered_base_skill_id = number(12630.),
                1 => f.direction_input.covered_actual_skill_id = number(12699.),
                2 => f.direction_input.covered_source = text("UnknownCoverList"),
                3 => f.direction_input.token = text("other-match"),
                4 => f.direction_input.hero_id = number(84.),
                5 => f.direction_input.slot = text("skill:1"),
                6 => f.direction_input.sample_start_ns = number(999.),
                7 => { types.insert("skill:0".into(),3); }
                8 => f.direction_input.covered_binding_ready = flag(false),
                _ => f.direction_input.covered_actual_skill_id.kind = 2,
            }
            attach_owned_direction_geometry(&mut f, &types, true);
            assert!(!f.active_joystick.ready.get(), "case {case}: invalid covered scope cannot retag joystick ownership");
            assert!(!f.direction_input.ready.get());
        }
    }
    #[test]
    fn denied_covered_binding_is_diagnostic_only_and_does_not_poison_base_identity() {
        let (mut f, types) = cold_direction_geometry_fixture();
        f.skill_id = number(12612.);
        deny_covered_direction_binding(&mut f, 12610, 12612, "not_in_cover_list");
        attach_owned_direction_geometry(&mut f, &types, true);
        assert!(!f.active_joystick.ready.get());
        assert!(f.direction_input.covered_binding_ready.is_bool());
        assert!(!f.direction_input.covered_binding_ready.get());
        assert_eq!(f.direction_input.covered_source.as_str(), "binding_unavailable:not_in_cover_list");
        assert!(!f.direction_input.ready.get());
        f.skill_id = number(12610.);
        attach_owned_direction_geometry(&mut f, &types, true);
        assert!(f.active_joystick.ready.get(), "optional denied alias must not suppress the owned base indicator");
        assert!(!f.direction_input.ready.get());
        set_covered_direction_binding(&mut f, None);
        assert!(f.direction_input.covered_binding_ready.missing());
        assert!(f.direction_input.covered_source.missing());
    }
    #[test]
    fn actor_and_ui_bytes_cannot_be_combined_across_game_updates() {
        let r = Reflection::new(MutableMemory(RefCell::new(vec![0; 128]))).unwrap();
        let mut phase = Vec::new();
        capture(&r, &mut phase, 16, 12).unwrap(); // hero render position
        capture(&r, &mut phase, 40, 16).unwrap(); // actual widget pose fragment
        assert!(check(&r, &phase).is_ok());
        r.memory.0.borrow_mut()[20] = 1; // hero moved while feedback was read
        assert!(check(&r, &phase).is_err());
        r.memory.0.borrow_mut()[20] = 0;
        r.memory.0.borrow_mut()[48] = 1;
        assert!(check(&r, &phase).is_err());
    }
    #[test]
    fn receipts_share_real_bounds_and_core_reconstructs_the_actual_start() {
        let mut frame = s::Frame::default();
        let begin = 1_000_000_000;
        let end = 1_003_250_000;
        stamp_frame(&mut frame, begin, end);
        assert_eq!(frame.time_ns.value, end as f64);
        assert_eq!(
            frame.time_ns.value - frame.sample_duration_ms.value * 1e6,
            begin as f64
        );
        for (a, b) in [
            (frame.logical.sample_start_ns, frame.logical.sample_end_ns),
            (
                frame.logical.joystick.sample_start_ns,
                frame.logical.joystick.sample_end_ns,
            ),
            (
                frame.direction_input.sample_start_ns,
                frame.direction_input.sample_end_ns,
            ),
            (
                frame.skill_state.sample_start_ns,
                frame.skill_state.sample_end_ns,
            ),
            (
                frame.cancel_state.sample_start_ns,
                frame.cancel_state.sample_end_ns,
            ),
            (
                frame.ui.cooldown_state.sample_start_ns,
                frame.ui.cooldown_state.sample_end_ns,
            ),
        ] {
            assert_eq!(a.value, begin as f64);
            assert_eq!(b.value, end as f64);
        }
    }
    #[test]
    fn unknown_selection_does_not_gain_a_fresh_receipt_when_global_frame_is_stamped() {
        let mut frame=s::Frame::default();
        frame.selection_input.reason=text("owner_receipt_unavailable");
        stamp_frame(&mut frame,1000,1200);
        assert!(frame.selection_input.ready.missing());
        assert!(frame.selection_input.input_ready.missing());
        assert!(frame.selection_input.sample_start_ns.missing());
        assert!(frame.selection_input.sample_end_ns.missing());
        assert!(frame.selection_input.selected_option_valid.missing());
        assert!(frame.direction_input.ready.missing());
        frame.selection_input=s::SelectionInput {ready:flag(true),coherent:flag(true),input_ready:flag(false),
            owner_id:140_128_804_941_248,hover_index:number(-1.),selected_option_valid:flag(false),
            reason:text("inactive_choice"),..Default::default()};
        stamp_frame(&mut frame,2000,2500);
        assert_eq!(frame.selection_input.sample_start_ns.value,2000.);
        assert_eq!(frame.selection_input.sample_end_ns.value,2500.);
        assert!(!frame.selection_input.input_ready.get());
        assert!(frame.selection_input.selected_option.missing());
        assert!(frame.selection_input.raw_touch_xyz.values.iter().all(|v|v.missing()));
        assert!(frame.direction_input.ready.missing(),"Selection ownership must not publish a world direction ACK");
    }
    #[test]
    fn selection_family_overrides_base_tap_only_for_the_fresh_exact_selector_form() {
        let mut ui=s::Ui {ready:flag(true),hero_id:number(131.),button_count:1,..Default::default()};
        ui.buttons[0]=s::Button {slot:text("skill:2"),skill_id:number(13130.),active:flag(true),..Default::default()};
        let mut state=s::VariantState {ready:flag(true),hero_id:ui.hero_id,count:1,..Default::default()};
        state.buttons[0]=s::Variant {slot:text("skill:2"),base_skill_id:number(13130.),active_skill_id:number(13130.),..Default::default()};
        assert_eq!(choice_selector_phase(&ui,&state,"skill:2"),Some(true));
        for actual in [1_013_132.,2_013_132.,42.] {
            state.buttons[0].active_skill_id=number(actual);
            assert_eq!(choice_selector_phase(&ui,&state,"skill:2"),Some(false),"follow-up form must use its actual native template, never numeric hero arithmetic");
        }
        for case in 0..8 {
            let mut u=ui;let mut v=state;
            match case {
                0=>v.ready=Default::default(),1=>v.hero_id=number(25.),2=>v.buttons[0].base_skill_id=number(999.),
                3=>v.buttons[0].active_skill_id=Default::default(),4=>v.count=0,
                5=>{v.buttons[1]=v.buttons[0];v.count=2;},6=>u.buttons[0].active=flag(false),
                _=>{u.buttons[1]=u.buttons[0];u.button_count=2;}
            }
            assert_eq!(choice_selector_phase(&u,&v,"skill:2"),None,"case{case}: unknown form must not restore base Tap fallback");
        }
    }
    #[test]
    fn unknown_choice_phase_retains_selection_until_future_native_template_is_positively_verified() {
        let unknown=s::SelectionInput {reason:text("choice_variant_unknown"),..Default::default()};
        for phase in [None,Some(true)] {
            for future in [None,Some(0),Some(1),Some(2),Some(3),Some(4)] {
                assert_eq!(choice_control_type(phase,future),4,"unknown/selector phase cannot use a future/base template");
            }
        }
        for unverified in [None,Some(0),Some(4),Some(255)] {
            assert_eq!(choice_control_type(Some(false),unverified),4,"unknown future must never expose aim_type0/generic Range or implicit Tap");
        }
        for native in [1,2,3] {assert_eq!(choice_control_type(Some(false),Some(native)),native);}
        assert_eq!(choice_control_type(Some(false),Some(1)),1,"a positively verified follow-up Tap remains Tap");
        assert!(unknown.ready.missing());assert!(unknown.input_ready.missing());
        assert!(unknown.sample_start_ns.missing());assert!(unknown.sample_end_ns.missing());
        assert_eq!(unknown.reason.as_str(),"choice_variant_unknown");
        let mut ui=s::Ui {ready:flag(true),hero_id:number(131.),button_count:1,..Default::default()};
        ui.buttons[0]=s::Button {slot:text("skill:2"),active:flag(true),skill_id:number(13130.),..Default::default()};
        let mut state=s::VariantState {ready:flag(true),hero_id:ui.hero_id,count:1,..Default::default()};
        state.buttons[0]=s::Variant {slot:text("skill:2"),base_skill_id:number(13130.),..Default::default()};
        let mut published=std::collections::BTreeMap::new();
        assert_eq!(retain_choice_family(&ui,&state,"skill:2",&mut published),None);
        assert_eq!(published.get("skill:2"),Some(&4),"the production pre-discovery path must publish4 before rejecting an unknown variant");
        assert!(unknown.ready.missing());
        state.buttons[0].active_skill_id=number(2_013_132.);
        assert_eq!(retain_choice_family(&ui,&state,"skill:2",&mut published),Some(false));
        assert_eq!(published.get("skill:2"),Some(&4),"a known future ID still needs independently verified native geometry");
        published.insert("skill:2".into(),choice_control_type(Some(false),Some(1)));
        assert_eq!(published.get("skill:2"),Some(&1));
    }
    #[test]
    fn gusion_recasts_are_plain_only_after_the_native_form_switch() {
        assert!(verified_button_only_form(56,"skill:0",5610,5612));
        assert!(verified_button_only_form(56,"skill:1",5620,5623));
        for(hero,slot,base,active)in [(56,"skill:0",5610,5610),(56,"skill:1",5620,5620),
            (56,"skill:0",5610,5611),(56,"skill:1",5620,5621),(56,"skill:0",5620,5623),
            (56,"skill:1",5610,5612),(55,"skill:0",5610,5612),(56,"skill:2",5620,5623)] {
            assert!(!verified_button_only_form(hero,slot,base,active));
        }
    }
    #[test]
    fn akai_spin_cancel_is_only_the_owned_stop_form() {
        assert!(verified_button_only_form(9,"skill:2",930,938));
        for(hero,slot,base,active)in [(9,"skill:2",930,930),(9,"skill:2",930,939),
            (9,"skill:0",930,938),(55,"skill:2",930,938),(9,"skill:2",938,938)] {
            assert!(!verified_button_only_form(hero,slot,base,active));
        }
    }
    #[test]
    fn angela_cancel_is_only_the_owned_channel_form_after_target_selection() {
        assert!(verified_button_only_form(55,"skill:2",5530,5534));
        for(hero,slot,base,active)in [(55,"skill:2",5530,5530),(55,"skill:2",5530,5535),(55,"skill:1",5530,5534),(31,"skill:2",5530,5534),(55,"skill:2",5534,5534)] {
            assert!(!verified_button_only_form(hero,slot,base,active));
        }
        let mut ui=s::Ui::default();ui.ready=flag(true);ui.hero_id=number(55.);ui.button_count=1;
        ui.buttons[0]=s::Button{slot:text("skill:2"),active:flag(true),skill_id:number(5530.),..Default::default()};
        let mut state=s::VariantState::default();state.ready=flag(true);state.hero_id=number(55.);state.count=1;
        state.buttons[0]=s::Variant{slot:text("skill:2"),base_skill_id:number(5530.),active_skill_id:number(5534.),..Default::default()};
        assert_eq!(choice_selector_phase(&ui,&state,"skill:2"),Some(false));
        assert_eq!(choice_control_type(Some(false),Some(1)),1);
        state.buttons[0].active_skill_id=number(5530.);
        assert_eq!(choice_selector_phase(&ui,&state,"skill:2"),Some(true));
        assert_eq!(choice_control_type(Some(true),Some(1)),4);
    }
    #[test]
    fn badang_wall_removal_is_only_the_owned_cancel_form_not_the_dash() {
        assert!(verified_button_only_form(77,"skill:1",7720,7725));
        for (hero,slot,base,id) in [(77,"skill:1",7720,7720),(77,"skill:0",7720,7725),(25,"skill:1",7720,7725),(77,"skill:1",7710,7725),(77,"skill:1",7720,7726)] {
            assert!(!verified_button_only_form(hero,slot,base,id));
        }
    }
    #[test]
    fn chou_follow_up_is_plain_without_reclassifying_the_first_kick_or_other_slots() {
        assert!(verified_button_only_form(26,"skill:2",2630,2632));
        for (hero,slot,base,id) in [(26,"skill:2",2630,2630),(26,"skill:2",2630,2631),
            (26,"skill:1",2630,2632),(25,"skill:2",2630,2632),(26,"skill:2",2620,2632)] {
            assert!(!verified_button_only_form(hero,slot,base,id));
        }
    }
    #[test]
    fn cold_selection_registry_retries_do_not_inherit_idle_delay_or_spin_without_bound() {
        for attempt in 1..=3 {assert_eq!(selection_plan_retry_delay(attempt,true),0,"first held discovery failures must retry on the next real Source sample");}
        for attempt in [4,5,100,u8::MAX] {assert_eq!(selection_plan_retry_delay(attempt,true),20_000_000,"persistent missing metadata must be throttled after the bounded cold burst");}
        assert_eq!(selection_plan_retry_delay(1,false),1_000_000_000,"idle discovery remains infrequent and does not change active input cadence");
    }
    fn movement_fixture() -> (Reflection<MutableMemory>, Widget, [f64; 16], [f64; 16]) {
        let mut bytes = vec![0; 512];
        let trs = [0_f32, 0., 0., 0., 0., 0., 0., 1., 1., 1., 1., 0.];
        for (i, v) in trs.into_iter().enumerate() {
            bytes[128 + i * 4..132 + i * 4].copy_from_slice(&v.to_le_bytes());
        }
        bytes[200..204].copy_from_slice(&2_i32.to_le_bytes());
        bytes[204..208].copy_from_slice(&2_i32.to_le_bytes());
        bytes[240] = 0; // a legitimate native canUse=false during a dash
        let r = Reflection::new(MutableMemory(RefCell::new(bytes))).unwrap();
        let widget = Widget {
            object: 16,
            path: Path {
                checks: vec![(16, vec![0; 8])],
                transforms: vec![128],
            },
            width: 200,
            height: 204,
            pivot: None,
            region: None,
        };
        let view = std::array::from_fn(|i| if i % 5 == 0 { 1. } else { 0. });
        let mut projection = view;
        projection[0] = 0.1;
        projection[5] = 0.1;
        (r, widget, view, projection)
    }
    #[test]
    fn late_movement_frame_and_native_allocation_recover_without_restarting_skill_source() {
        let (mut r,widget,view,projection)=movement_fixture();
        let mut plan=MovementPlan::default();
        plan.refresh(&mut r,&[],|_,_|Err("not created".into()));
        assert!(plan.widget.is_none());
        // The active frame appears before its native transform is initialized.
        plan.refresh(&mut r,&[16],|_,_|Err("transform pending".into()));
        assert!(plan.widget.is_none());
        plan.refresh(&mut r,&[16],|_,frames|{assert_eq!(frames,&[16]);Ok((widget.clone(),vec![(32,vec![0;8])]))});
        assert!(read_movement_geometry(&mut r,plan.widget.as_mut().unwrap(),&view,&projection).unwrap().0.ready.get());
        plan.refresh(&mut r,&[16],|_,_|panic!("stable widget should not be rediscovered"));
        r.memory.0.borrow_mut()[32]=1;
        plan.refresh(&mut r,&[16],|_,_|Err("owner replaced".into()));
        assert!(plan.widget.is_none());
        plan.refresh(&mut r,&[],|_,_|Err("frame removed".into()));
        assert!(plan.widget.is_none());
    }
    #[test]
    fn temporary_movement_permission_does_not_hide_owned_geometry() {
        let (mut r, mut widget, view, projection) = movement_fixture();
        assert!(!r.boolean(240).unwrap());
        let (geometry, reads) =
            read_movement_geometry(&mut r, &mut widget, &view, &projection).unwrap();
        assert!(geometry.ready.get());
        assert_eq!(geometry.center_uv.values[0].value, 0.5);
        assert_eq!(geometry.center_uv.values[1].value, 0.5);
        assert!(check(&r, &reads).is_ok());
    }
    #[test]
    fn invalid_optional_movement_geometry_does_not_poison_other_source_reads() {
        let (mut r, mut widget, view, projection) = movement_fixture();
        let mut core = Vec::new();
        capture(&r, &mut core, 32, 16).unwrap();
        r.memory.0.borrow_mut()[200..204].copy_from_slice(&0_i32.to_le_bytes());
        assert!(read_movement_geometry(&mut r, &mut widget, &view, &projection).is_none());
        assert_eq!(core.len(), 1);
        assert!(check(&r, &core).is_ok());
    }
    #[test]
    fn ui_replans_do_not_change_the_input_epoch_for_the_same_match() {
        let identity = MatchIdentity {
            owners: [100, 200, 300, 400],
            hero: 25,
        };
        let mut epoch = MatchEpoch::default();
        let first = epoch.observe(identity, 1000);
        // Each observation represents a newly prepared UI plan with a different
        // W form/control list/transform hierarchy. Those are not match owners.
        for clock in [1001, 1040, 1080, 1200] {
            assert_eq!(epoch.observe(identity, clock), first);
        }
        let mut changed = identity;
        changed.owners[3] = 401;
        assert_ne!(epoch.observe(changed, 1201), first);
    }
    #[test]
    fn hero_owner_changes_and_clock_reset_start_a_new_input_epoch() {
        let identity = MatchIdentity {
            owners: [100, 200, 300, 400],
            hero: 25,
        };
        let mut epoch = MatchEpoch::default();
        let first = epoch.observe(identity, 1000);
        let next = epoch.observe(
            MatchIdentity {
                hero: 26,
                ..identity
            },
            1001,
        );
        assert_ne!(first, next);
        let reset = epoch.observe(
            MatchIdentity {
                hero: 26,
                ..identity
            },
            0,
        );
        assert_ne!(reset, next);
        epoch.invalidate();
        assert_ne!(
            epoch.observe(
                MatchIdentity {
                    hero: 26,
                    ..identity
                },
                1
            ),
            reset
        );
    }
    #[test]
    fn normal_u32_match_clock_wrap_is_not_a_restart() {
        let identity = MatchIdentity {
            owners: [100, 200, 300, 400],
            hero: 25,
        };
        let mut epoch = MatchEpoch::default();
        let first = epoch.observe(identity, 0xfffffff0);
        assert_eq!(epoch.observe(identity, 0x10), first);
    }
    fn auxiliary_observation(owner:u64,present:bool,hold:Option<bool>,replacement:Option<(u64,Option<bool>)>)->crate::control_audit::AuxiliaryHoldObservation {
        crate::control_audit::AuxiliaryHoldObservation {owner_id:owner,owner_present:present,hold_to_release:hold,
            replacement_owner:replacement.map(|v|v.0),replacement_hold:replacement.and_then(|v|v.1),skill_use_type:None,button_bits:None}
    }
    #[test]
    fn auxiliary_current_owner_and_retired_tombstones_follow_positive_replacement_proof() {
        let mut source=AuxiliaryProducer::default();
        let a=source.project(auxiliary_observation(100,true,Some(true),None),"scope",97,1000);
        assert!(a.ready.get()&&a.coherent.get());assert_eq!(a.count,1);assert_eq!(a.controls[0].owner_id,100);assert!(a.controls[0].hold_to_release.get());
        let b=source.project(auxiliary_observation(100,false,None,Some((200,Some(false)))),"scope",97,1100);
        assert_eq!(b.count,2);assert_eq!(b.controls[0].owner_id,200);assert!(b.controls[0].owner_present.get());
        assert!(b.controls[0].hold_to_release.is_bool()&&!b.controls[0].hold_to_release.get());
        assert_eq!(b.controls[1].owner_id,100);assert!(b.controls[1].owner_present.is_bool()&&!b.controls[1].owner_present.get());
        let c=source.project(auxiliary_observation(100,false,None,Some((300,Some(true)))),"scope",97,1200);
        assert_eq!(c.count,3);assert_eq!(c.controls[0].owner_id,300);
        for owner in [100,200]{assert!(c.controls.iter().take(c.count as usize).any(|v|v.owner_id==owner&&v.owner_present.is_bool()&&!v.owner_present.get()));}
        assert!(c.controls.iter().all(|v|v.cancel_supported.missing()),"native hold is not cancellation capability");
    }
    #[test]
    fn auxiliary_unknown_hold_remains_present_unknown_and_reused_owner_has_no_false_tombstone() {
        let mut source=AuxiliaryProducer::default();
        source.project(auxiliary_observation(100,true,Some(true),None),"scope",97,1000);
        source.project(auxiliary_observation(100,false,None,Some((200,None))),"scope",97,1100);
        let a=source.project(auxiliary_observation(100,true,None,None),"scope",97,1200);
        assert!(a.controls[0].owner_present.get());assert!(a.controls[0].hold_to_release.missing());
        assert_eq!(a.controls.iter().take(a.count as usize).filter(|v|v.owner_id==100).count(),1);
        let unknown=source.project(auxiliary_observation(100,false,None,None),"scope",97,1300);
        assert!(unknown.ready.missing());assert_eq!(unknown.count,0,"missing replacement does not imply original owner absent");
    }
    #[test]
    fn auxiliary_history_is_bounded_and_match_scope_reset_cannot_republish_old_tombstones() {
        let mut source=AuxiliaryProducer::default();
        for current in 1..=20 {let packet=source.project(auxiliary_observation(current,true,Some(false),None),"scope",97,1000);
            assert!(packet.count<=8);assert_eq!(packet.controls[0].owner_id,current);}
        let mut r=Reflection::new(MutableMemory(RefCell::new(vec![0;128]))).unwrap();
        let identity=MatchIdentity {owners:[1,2,3,4],hero:97};
        let missing=source.sample(&mut r,identity,2,0,0,"new-scope",2000,&mut Vec::new());
        assert!(missing.ready.missing());assert_eq!(source.current,0);assert_eq!(source.retired_count,0);
        let mut frame=s::Frame::default();frame.auxiliary_controls=missing;stamp_frame(&mut frame,2000,2100);
        assert!(frame.auxiliary_controls.sample_start_ns.missing());assert!(frame.auxiliary_controls.sample_end_ns.missing());
        frame.auxiliary_controls=source.project(auxiliary_observation(400,true,Some(true),None),"new-scope",97,2000);
        stamp_frame(&mut frame,2000,2100);assert_eq!(frame.auxiliary_controls.sample_start_ns.value,2000.);assert_eq!(frame.auxiliary_controls.sample_end_ns.value,2100.);
    }

    #[test]
    fn target_cancel_geometry_is_packet_local_and_never_changes_another_skills_generic_channel() {
        let mut frame=s::Frame::default();frame.token=text("scope");frame.hero_id=number(55.);
        frame.ui.cancel_uv=v2([0.9,0.1]);frame.cancel_state.cancelled=flag(true);frame.cancel_state.source=text("UIKeySkill.bCancelSkill");
        frame.cancel_state.slot=text("skill:0");
        frame.selection_input=s::SelectionInput {kind:number(2.),ready:flag(true),coherent:flag(true),owner_id:100,backend_owner_id:200,
            panel_visible:flag(true),slot:text("skill:2"),base_skill_id:number(5530.),native_cancel:flag(true),..Default::default()};
        attach_target_cancel(&mut frame,None);
        assert_eq!(frame.ui.cancel_uv.values[0].value,0.9);assert!(frame.cancel_state.cancelled.get());
        assert_eq!(frame.cancel_state.source.as_str(),"UIKeySkill.bCancelSkill");assert_eq!(frame.cancel_state.slot.as_str(),"skill:0");
        assert!(frame.selection_input.target_cancel_ready.missing());assert!(!frame.selection_input.target_cancel_uv.valid());
        attach_target_cancel(&mut frame,Some([0.7,0.2]));assert!(frame.selection_input.target_cancel_ready.get());
        assert_eq!(frame.selection_input.target_cancel_uv.values[0].value,0.7);assert!(frame.selection_input.target_cancel_hit.missing());
        for case in 0..4 {
            let mut current=frame;
            match case {0=>current.selection_input.ready=Default::default(),1=>current.selection_input.coherent=flag(false),
                2=>current.selection_input.panel_visible=flag(false),_=>current.selection_input.backend_owner_id=0};
            attach_target_cancel(&mut current,Some([0.7,0.2]));assert!(current.selection_input.target_cancel_ready.missing());
            assert!(!current.selection_input.target_cancel_uv.valid());assert_eq!(current.ui.cancel_uv.values[0].value,0.9);
            assert_eq!(current.cancel_state.slot.as_str(),"skill:0");assert!(current.cancel_state.cancelled.get());
        }
    }

}
