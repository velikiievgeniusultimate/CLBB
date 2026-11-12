//! Native, owner-checked input feedback; never infer acknowledgement from visual proximity.
use crate::{
    platform::Memory,
    reflection::{Reflection, Result, f32_at, i32_at, u64_at},
    source::{self, Checks, capture, capture_path, check, flag, number, text, v2, v3},
    unity::Path,
};
use clbb_moba_core::controller::snapshot as s;
use std::collections::HashMap;
#[derive(Clone)]
pub struct MotionPlan {
    checks: Vec<(u64, Vec<u8>)>,
    pub skill: i32,
    pub name: String,
    motion: u64,
    direction_geometry: bool,
    attack_geometry: bool,
    fields: HashMap<String, u64>,
    circle: Option<Path>,
    circle_field: Option<u64>,
    circle_owner: u64,
    range: Option<u64>,
    range_shape: Option<f64>,
    legacy_range: bool,
    protect: Option<(u64, u64, u64)>,
    direction_visual: Option<(u64,u64,u64)>,
    direction_arrow_field: Option<u64>,
    direction_flags: Option<(u64,u64)>,
    direction_endpoint: Option<(u64,Path)>,
}
/// Input pixels for an independently qualified native UI-choice controller.
/// This plan never produces Motion/Logical/Direction acknowledgements: the
/// caller must join its UI owner, selected option and frame coherence fence.
#[derive(Clone)]
pub(crate) struct SelectionTouchPlan {
    checks: Checks,
    pub(crate) skill: i32,
    address: u64,
}
impl SelectionTouchPlan {
    pub(crate) fn discover<M:Memory>(r:&mut Reflection<M>,indicator:u64,skill:i32)->Result<Self> {
        if !(1..=10_000_000).contains(&skill){return Err("Invalid selection touch identity".into());}
        let(mut checks,comp,motion,package,mc,pc)=MotionPlan::registry_owner(r,indicator,skill)?;
        if mc.name!="IndicatorMotionPos"||pc.name!="IndicatorPackageEasy_Yuan" {
            return Err("Unverified native selection touch layout".into());
        }
        typed_field(r,comp,"m_Config",17)?;
        let config=owner(r,comp,"m_Config",&mut checks)?;
        typed_field(r,config,"m_ShowRectPara",29)?;
        let(rect,params)=native_rect_config(r,comp,skill,&mut checks)?;
        if !selection_touch_template(&mc.name,&pc.name,rect,&params) {
            return Err("Unverified selection zero-extent configuration".into());
        }
        for(object,field,expected)in [(motion,"m_OwnComp",comp),(motion,"m_OwnPackage",package),(package,"m_OwnComp",comp)] {
            let at=typed_field(r,object,field,18)?;
            if u64_at(&capture(r,&mut checks,at,8)?,0)!=expected{return Err("Selection touch owner mismatch".into());}
        }
        let address=typed_field(r,motion,"m_vDragTouchPos",17)?;
        if address<motion||address.checked_add(12).is_none_or(|end|end>motion+4096) {
            return Err("Selection touch outside native motion bounds".into());
        }
        check(r,&checks)?;
        Ok(Self{checks,skill,address})
    }
    pub(crate) fn validate<M:Memory>(&self,r:&Reflection<M>)->Result<()> {check(r,&self.checks)}
    /// The enclosing Source sample must check `consumed` after its UI reads.
    /// A zero/idle value is valid raw data, never proof of delivered input.
    pub(crate) fn sample<M:Memory>(&self,r:&Reflection<M>,consumed:&mut Checks)->Result<[f64;3]> {
        self.validate(r)?;
        consumed.extend(self.checks.iter().cloned());
        source::vec3(&capture(r,consumed,self.address,12)?)
    }
}
fn selection_touch_template(motion:&str,package:&str,rect:i32,params:&[f64])->bool {
    motion=="IndicatorMotionPos"&&package=="IndicatorPackageEasy_Yuan"
        &&rect==11&&params.len()>=2&&params[0]==3.&&params[1]==0.
}
fn owner<M: Memory>(
    r: &mut Reflection<M>,
    obj: u64,
    name: &str,
    checks: &mut Vec<(u64, Vec<u8>)>,
) -> Result<u64> {
    let a = r.field(obj, name)?;
    let v = r.pointer(a)?;
    checks.push((a, v.to_le_bytes().to_vec()));
    Ok(v)
}
fn typed_field<M:Memory>(r:&mut Reflection<M>,object:u64,name:&str,kind:u8)->Result<u64> {
    typed_value_field(r,object,name,kind,false)
}
fn typed_value_field<M:Memory>(r:&mut Reflection<M>,object:u64,name:&str,kind:u8,allow_static:bool)->Result<u64> {
    let at=r.field(object,name)?;let mut class=r.class(r.pointer(object)?)?;
    for _ in 0..16 {
        if let Some(f)=class.fields.get(name) {if f.kind!=kind||(f.is_static&&!allow_static){return Err("Native visual field type changed".into());}return Ok(at);}
        if class.parent==0{break}class=r.class(class.parent)?;
    }
    Err("Native visual field absent".into())
}
// Class contracts observed across independent heroes, not numeric hero/skill IDs.
fn native_template(motion:&str,package:&str)->Option<u8> {
    match (motion,package) {
        ("IndicatorMotionRot","IndicatorPackageEasy") => Some(1),
        ("IndicatorMotionRot","IndicatorPackageEasy_JianTou" | "IndicatorPackageEasy_ShanXing_Normal" | "IndicatorPackageEasy_ShanXing_Ranger") => Some(2),
        ("IndicatorLieShaMotion", "IndicatorPackageEasy_ShanXing_Normal") => Some(2),
        ("IndicatorMotionArcSurface", "IndicatorPackage_ArcSurface") => Some(2),
        ("IndicatorMotionPos", "IndicatorPackageEasy_Yuan") => Some(3),
        ("IndicatorMotionRotAndPosRot","IndicatorPackage_JianTou_OtherPart") => Some(3),
        ("IndicatorMotionRotAndPosRot" | "IndicatorMotionFaTiaoQiu2","IndicatorPackage_JianTou_Yuan") => Some(3),
        _=>None,
    }
}
fn zero_extent_template(motion:&str,package:&str,rect:i32,params:&[f64])->bool {
    motion=="IndicatorMotionPos" && package=="IndicatorPackage_TwoPartAndPos_Direction"
        && rect==11 && params.len()>=2 && params[0]==11. && params[1]==0.
}
fn zero_extent_base_tap_template(motion:&str,package:&str,rect:i32,params:&[f64])->bool {
    motion=="IndicatorMotionBase"&&package=="IndicatorPackageEasy_Yuan"
        &&rect==11&&params.len()>=2&&params[0]==3.&&params[1]==0.
}
fn movement_driven_template(motion:&str,package:&str,rect:i32,params:&[f64])->bool {
    motion=="IndicatorMotionRot" && package=="IndicatorPackageEasy_JianTou_DrivenByJoystick"
        && rect==11 && params.len()>=2 && params[0]==27.
        && params[1].is_finite() && params[1]>0. && params[1]<1e4
}
fn fixed_part_direction_template(motion:&str,package:&str,rect:i32,params:&[f64])->bool {
    motion=="IndicatorMotionRot" && package=="IndicatorPackage_JianTou_OtherFixedPart"
        && matches!(rect,0|11) && params.len()>=2 && params[0]==15.
        && params[1].is_finite() && params[1]>0. && params[1]<1e4
}
/// Unit-vector feedback measured independently of drag ratio for these exact
/// native layouts. Similar motion names and other shapes remain unqualified.
fn additional_direction_template(motion:&str,package:&str,rect:i32,params:&[f64])->bool {
    let shape=match package {
        "IndicatorPackage_TwoPartAndPos_Direction" if matches!(rect,1|2)=>11.,
        "IndicatorPackage_Multi_JianTou" if rect==0=>7.,
        "IndicatorPackage_TwoPart_Direction" if rect==19=>6.,
        _=>return false,
    };
    motion=="IndicatorMotionRot" && params.len()>=2 && params[0]==shape
        && params[1].is_finite() && params[1]>0. && params[1]<1e4
}
fn native_range_shape(motion:&str,package:&str)->Option<f64> {
    match (motion,package) {
        ("IndicatorMotionRotAndPosRot"|"IndicatorMotionFaTiaoQiu2","IndicatorPackage_JianTou_Yuan")=>Some(8.),
        ("IndicatorMotionPos","IndicatorPackageEasy_Yuan")=>Some(3.),
        ("IndicatorMotionRotAndPosRot","IndicatorPackage_JianTou_OtherPart")=>Some(9.),
        _=>None,
    }
}
fn verified_direction_geometry(skill:i32,motion:&str,package:&str,configured_direction:bool)->bool {
    native_template(motion,package)==Some(2)||configured_direction
        ||(skill==8430&&motion=="IndicatorMotionRot"&&package=="IndicatorPackageEasy")
}
fn point_visual_fallback(motion:&str,package:&str)->bool {
    motion=="IndicatorMotionPos"&&package=="IndicatorPackageEasy_Yuan"
}
fn valid_native_range(shape:f64,values:[f64;2])->bool {
    values[0]==shape && values[1].is_finite() && values[1]>0. && values[1]<=1e4
}
/// A configuration radius is meaningful only within these measured families.
/// Keep distance bytes out of the persistent identity fence: buffs may replace
/// the distance while preserving the same typed configuration/array owner.
fn native_range_contract<M:Memory>(r:&mut Reflection<M>,comp:u64,motion:u64,package:u64,skill:i32,
    motion_name:&str,package_name:&str,checks:&mut Checks)->Result<Option<(u64,f64)>> {
    let Some(shape)=native_range_shape(motion_name,package_name)else{return Ok(None)};
    if shape==9. {
        let(rect,params)=native_rect_config(r,comp,skill,checks)?;
        if rect!=1||!valid_native_range(shape,params){return Err("Unverified shape-nine native range configuration".into());}
    }
    native_range_contract_for_shape(r,comp,motion,package,skill,shape,checks).map(Some)
}
fn native_range_contract_for_shape<M:Memory>(r:&mut Reflection<M>,comp:u64,motion:u64,package:u64,
    skill:i32,shape:f64,checks:&mut Checks)->Result<(u64,f64)> {
    let mut config=0;
    for object in [comp,motion,package] {
        let at=typed_field(r,object,"m_Config",17)?;
        let linked=u64_at(&capture(r,checks,at,8)?,0);
        if linked==0||(config!=0&&linked!=config){return Err("Native range owners differ".into());}
        config=linked;
    }
    let owner=u64_at(&capture(r,checks,config,8)?,0);
    if r.class(owner)?.name!="CData_Skill_Element"{return Err("Native range configuration type changed".into());}
    let id_at=typed_field(r,config,"m_SkillID",8)?;
    if i32_at(&capture(r,checks,id_at,4)?,0)!=skill{return Err("Native range belongs to another skill".into());}
    let params_at=typed_field(r,config,"m_ShowRectPara",29)?;
    let params=u64_at(&capture(r,checks,params_at,8)?,0);
    let header=capture(r,checks,params,32)?;let count=u64_at(&header,24);
    if !(4..=64).contains(&count){return Err("Native range array changed".into());}
    let array=capture(r,checks,u64_at(&header,0),0x50)?;
    if r.cstring(u64_at(&array,16))?!="Single[]"||r.class(u64_at(&array,0x40))?.name!="Single" {
        return Err("Native range array type changed".into());
    }
    let address=params+32;let raw=r.read(address,8)?;
    if !valid_native_range(shape,[f32_at(&raw,0),f32_at(&raw,4)]) {return Err("Invalid native world range".into());}
    checks.push((address,raw[..4].to_vec()));
    if r.read(address,8)?!=raw{return Err("Native range changed during discovery".into());}
    Ok((address,shape))
}
/// The independently measured shape-nine family retains its stricter rect
/// witness in addition to the shared native Range owner/type contract.
fn candidate_range_contract<M:Memory>(r:&mut Reflection<M>,comp:u64,motion:u64,package:u64,skill:i32,
    motion_name:&str,package_name:&str,checks:&mut Checks)->Result<Option<(u64,f64)>> {
    if motion_name!="IndicatorMotionRotAndPosRot"||package_name!="IndicatorPackage_JianTou_OtherPart" {return Ok(None);}
    if !(1..=10_000_000).contains(&skill){return Err("Invalid candidate range identity".into());}
    let(rect,params)=native_rect_config(r,comp,skill,checks)?;
    if rect!=1||!valid_native_range(9.,params){return Err("Unverified candidate range configuration".into());}
    native_range_contract_for_shape(r,comp,motion,package,skill,9.,checks).map(Some)
}
fn native_rect_config<M:Memory>(r:&mut Reflection<M>,comp:u64,skill:i32,checks:&mut Checks)->Result<(i32,[f64;2])> {
    let config=owner(r,comp,"m_Config",checks)?;
    let class=r.class(r.pointer(config)?)?;
    if class.name!="CData_Skill_Element" {return Err("Unknown native tap config".into());}
    checks.push((config,class.address.to_le_bytes().to_vec()));
    let mut ints=Vec::new();
    for name in ["m_SkillID","m_RectType"] {
        let field=class.fields.get(name).ok_or("Missing tap config field")?;
        if field.kind!=8 || field.is_static {return Err("Native tap config type changed".into());}
        let raw=capture(r,checks,config+field.offset,4)?;ints.push(i32_at(&raw,0));
    }
    if ints[0]!=skill {return Err("Native tap config belongs to another skill".into());}
    let params=owner(r,config,"m_ShowRectPara",checks)?;
    let header=capture(r,checks,params,32)?;
    let count=u64_at(&header,24);
    if !(2..=64).contains(&count) {return Err("Native tap config array size changed".into());}
    let array=capture(r,checks,u64_at(&header,0),0x50)?;
    if r.cstring(u64_at(&array,16))?!="Single[]" || r.class(u64_at(&array,0x40))?.name!="Single" {
        return Err("Native tap config array type changed".into());
    }
    let raw=capture(r,checks,params+32,8)?;
    Ok((ints[1],[f32_at(&raw,0),f32_at(&raw,4)]))
}
fn fixed_part_direction_contract<M:Memory>(r:&mut Reflection<M>,comp:u64,skill:i32,motion:&str,package:&str,checks:&mut Checks)->Result<bool> {
    if motion!="IndicatorMotionRot" || package!="IndicatorPackage_JianTou_OtherFixedPart" {
        return Ok(false);
    }
    let (rect,params)=native_rect_config(r,comp,skill,checks)?;
    if !fixed_part_direction_template(motion,package,rect,&params) {
        return Err("Unverified fixed-part direction configuration".into());
    }
    Ok(true)
}
fn additional_direction_contract<M:Memory>(r:&mut Reflection<M>,comp:u64,skill:i32,motion:&str,package:&str,checks:&mut Checks)->Result<bool> {
    if motion!="IndicatorMotionRot" || !matches!(package,"IndicatorPackage_TwoPartAndPos_Direction"
        |"IndicatorPackage_Multi_JianTou"|"IndicatorPackage_TwoPart_Direction") {
        return Ok(false);
    }
    if !(1..=10_000_000).contains(&skill) {return Err("Invalid additional direction identity".into());}
    // The current registry comp owns the typed configuration. A package may
    // share another form's config, so that pointer is not this skill's identity.
    let config_at=typed_field(r,comp,"m_Config",17)?;
    let config=u64_at(&capture(r,checks,config_at,8)?,0);
    typed_field(r,config,"m_ShowRectPara",29)?;
    let(rect,params)=native_rect_config(r,comp,skill,checks)?;
    if !additional_direction_template(motion,package,rect,&params) {
        return Err("Unverified additional direction configuration".into());
    }
    Ok(true)
}
/// This base motion has no m_OwnPackage field. It is a Tap only when both
/// actual owners and all three configuration links prove the zero-extent
/// contract; an unknown motion with a missing owner field remains rejected.
fn zero_extent_base_tap_contract<M:Memory>(r:&mut Reflection<M>,comp:u64,motion:u64,package:u64,skill:i32,
    motion_name:&str,package_name:&str,checks:&mut Checks)->Result<bool> {
    if motion_name!="IndicatorMotionBase"||package_name!="IndicatorPackageEasy_Yuan"{return Ok(false);}
    let mut config=0;
    for object in [comp,motion,package] {
        let at=typed_field(r,object,"m_Config",17)?;
        let linked=u64_at(&capture(r,checks,at,8)?,0);
        if linked==0||(config!=0&&linked!=config){return Err("Native base-tap configuration owners differ".into());}
        config=linked;
    }
    // Validate the reference's metadata kind as well as the array's actual
    // Single[] identity checked by native_rect_config.
    typed_field(r,config,"m_ShowRectPara",29)?;
    let(rect,params)=native_rect_config(r,comp,skill,checks)?;
    if !zero_extent_base_tap_template(motion_name,package_name,rect,&params) {
        return Err("Unverified nonzero base-tap configuration".into());
    }
    for object in [motion,package] {
        let at=typed_field(r,object,"m_OwnComp",18)?;
        if u64_at(&capture(r,checks,at,8)?,0)!=comp{return Err("Native base-tap owner mismatch".into());}
    }
    Ok(true)
}
impl MotionPlan {
    /// Read-only proof for the separately measured shape-nine positional family.
    /// No native motion, input, or persistent plan is modified.
    pub fn audit_candidate_range_contract<M:Memory>(r:&mut Reflection<M>,indicator:u64,skill:i32)->Result<serde_json::Value> {
        let elapsed=std::time::Instant::now();
        let(mut checks,comp,motion,package,mc,pc)=Self::registry_owner(r,indicator,skill)?;
        for(object,field,expected)in[(motion,"m_OwnComp",comp),(motion,"m_OwnPackage",package),(package,"m_OwnComp",comp)] {
            let at=typed_field(r,object,field,18)?;
            if u64_at(&capture(r,&mut checks,at,8)?,0)!=expected{return Err("Candidate range motion owner changed".into());}
        }
        let(address,shape)=candidate_range_contract(r,comp,motion,package,skill,&mc.name,&pc.name,&mut checks)?
            .ok_or("Unqualified candidate range family")?;
        let values=capture(r,&mut checks,address,8)?;
        let range=f32_at(&values,4);
        if !valid_native_range(shape,[f32_at(&values,0),range]){return Err("Candidate range changed during audit".into());}
        check(r,&checks)?;
        Ok(serde_json::json!({"skill":skill,"motion":mc.name,"package":pc.name,"rect":1,"shape":shape,
            "world_range":range,"world_range_source":"CData_Skill_Element.m_ShowRectPara[1]",
            "typed_config_owners_equal":true,"registry_owner_checked":true,"production_enabled":native_range_shape(&mc.name,&pc.name)==Some(shape),
            "checks":checks.len(),"duration_ns":elapsed.elapsed().as_nanos()}))
    }
    /// Separate read-only diagnostic bracket; does not alter live acknowledgement rules.
    pub fn audit_drag_gate<M: Memory>(&self, r: &mut Reflection<M>) -> Result<serde_json::Value> {
        check(r, &self.checks)?;
        let mut consumed = Vec::new();
        let mut values = serde_json::Map::new();
        let mut objects = vec![("motion", self.motion)];
        if let Ok(link) = r.field(self.motion, "m_DragProtectComp") {
            let raw = capture(r, &mut consumed, link, 8)?;
            let child = u64_at(&raw, 0);
            if child != 0 {
                let owner = r.pointer(child)?;
                if r.class(owner)?.name != "DragProtectComp" {
                    return Err("Unexpected diagnostic drag protector class".into());
                }
                consumed.push((child, owner.to_le_bytes().to_vec()));
                let owner_field=typed_field(r,child,"m_Owner",18)?;
                let motion=u64_at(&capture(r,&mut consumed,owner_field,8)?,0);
                values.insert("protector.owner_matches_motion".into(),serde_json::json!(motion==self.motion));
                if motion!=0 {
                    let class=u64_at(&capture(r,&mut consumed,motion,8)?,0);
                    values.insert("protector.owner_class".into(),serde_json::json!(r.class(class)?.name));
                }
                objects.push(("protector", child));
            }
        }
        for (role, object) in objects {
            let mut class = r.pointer(object)?;
            consumed.push((object, class.to_le_bytes().to_vec()));
            for _ in 0..16 {
                if class == 0 { break; }
                let metadata = r.class(class)?;
                for name in ["m_dragged", "m_bVaildDraged", "bInFirstDrag"] {
                    if let Some(field) = metadata.fields.get(name) {
                        if field.is_static || field.kind != 2 || field.offset > 4096 { continue; }
                        let raw = capture(r, &mut consumed, object + field.offset, 1)?;
                        if raw[0] > 1 { return Err("Invalid diagnostic drag boolean".into()); }
                        values.insert(format!("{role}.{name}"), serde_json::json!(raw[0] != 0));
                    }
                }
                for (name, kind, length) in [("m_vFirstPress", 17, 12), ("m_fSkillDeadZone", 12, 4)] {
                    if let Some(field) = metadata.fields.get(name) {
                        if field.is_static || field.kind != kind || field.offset > 4096 { continue; }
                        let raw = capture(r, &mut consumed, object + field.offset, length)?;
                        let value = if name == "m_vFirstPress" {
                            serde_json::json!(source::vec3(&raw)?)
                        } else {
                            let value=f32_at(&raw,0);
                            if !value.is_finite() || value < 0. || value > 1e6 {return Err("Invalid diagnostic dead zone".into());}
                            serde_json::json!(value)
                        };
                        values.insert(format!("{role}.{name}"),value);
                    }
                }
                class = metadata.parent;
            }
        }
        check(r, &consumed)?;
        check(r, &self.checks)?;
        Ok(serde_json::Value::Object(values))
    }
    fn registry_owner<M: Memory>(r: &mut Reflection<M>, indicator: u64, skill: i32)
        -> Result<(Checks, u64, u64, u64, crate::reflection::Class, crate::reflection::Class)> {
        let mut checks = Vec::new();
        let dictionary = owner(r, indicator, "m_BaseCamp", &mut checks)?;
        let array = owner(r, dictionary, "entries", &mut checks)?;
        let count_a = r.field(dictionary, "count")?;
        let count = r.integer(count_a)?;
        if !(1..=256).contains(&count) {
            return Err("Skill registry is unavailable".into());
        }
        let class = r.pointer(array)?;
        let capacity = r.pointer(array + 24)?;
        if capacity < count as u64 || capacity > 4096 {
            return Err("Invalid skill registry capacity".into());
        }
        let element = r.pointer(class + 0x40)?;
        let entry = r.class(element)?;
        checks.push((class + 0x40, element.to_le_bytes().to_vec()));
        let expected = [("hashCode", 8), ("next", 8), ("key", 8), ("value", 18)];
        if entry.fields.len() != 4
            || expected.iter().any(|(name, kind)| {
                entry
                    .fields
                    .get(*name)
                    .is_none_or(|f| f.kind != *kind || f.is_static)
            })
        {
            return Err("Skill registry entry layout changed".into());
        }
        let bias = entry
            .fields
            .values()
            .map(|f| f.offset)
            .min()
            .ok_or("Empty skill registry entry")?;
        let offsets: HashMap<_, _> = entry
            .fields
            .iter()
            .map(|(n, f)| (n.as_str(), (f.offset - bias) as usize))
            .collect();
        let stride = offsets
            .iter()
            .map(|(n, o)| o + if *n == "value" { 8 } else { 4 })
            .max()
            .unwrap();
        if !(16..=64).contains(&stride)
            || stride % 8 != 0
            || offsets["hashCode"] != 0
            || offsets
                .values()
                .copied()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != 4
        {
            return Err("Invalid skill registry stride".into());
        }
        let bytes = r.read(array + 32, count as usize * stride)?;
        let hits: Vec<_> = bytes
            .chunks_exact(stride)
            .enumerate()
            .filter(|(_, row)| {
                i32_at(row, offsets["hashCode"]) >= 0 && i32_at(row, offsets["key"]) == skill
            })
            .collect();
        if hits.len() != 1 {
            return Err("Current skill registry comp is absent".into());
        }
        let (index, row) = hits[0];
        let comp = u64_at(row, offsets["value"]);
        checks.extend([
            (array, class.to_le_bytes().to_vec()),
            (array + 24, capacity.to_le_bytes().to_vec()),
            (count_a, count.to_le_bytes().to_vec()),
        ]);
        for name in ["hashCode", "key", "value"] {
            let size = if name == "value" { 8 } else { 4 };
            let offset = offsets[name];
            checks.push((
                array + 32 + (index * stride + offset) as u64,
                row[offset..offset + size].to_vec(),
            ));
        }
        let motion = owner(r, comp, "m_MontionBase", &mut checks)?;
        let package = owner(r, comp, "m_PackageBase", &mut checks)?;
        let motion_class = r.class(r.pointer(motion)?)?;
        let package_class = r.class(r.pointer(package)?)?;
        checks.extend([
            (motion, motion_class.address.to_le_bytes().to_vec()),
            (package, package_class.address.to_le_bytes().to_vec()),
            (comp, r.pointer(comp)?.to_le_bytes().to_vec()),
        ]);
        check(r, &checks)?;
        Ok((checks,comp,motion,package,motion_class,package_class))
    }
    /// Names and field types only; unverified motion classes never become live plans.
    pub fn audit_registry_identity<M: Memory>(r: &mut Reflection<M>, indicator: u64, skill: i32) -> Result<serde_json::Value> {
        let (mut checks,comp,motion,package,mc,pc)=Self::registry_owner(r,indicator,skill)?;
        let base_tap=zero_extent_base_tap_contract(r,comp,motion,package,skill,&mc.name,&pc.name,&mut checks)?;
        for (object,field,expected) in [(motion,"m_OwnComp",comp),(motion,"m_OwnPackage",package),(package,"m_OwnComp",comp)] {
            if base_tap&&object==motion&&field=="m_OwnPackage" {
                match r.field(object,field) {
                    Err(error)if error=="Missing reflected field m_OwnPackage"=>continue,
                    Err(error)=>return Err(error),
                    Ok(_)=>{typed_field(r,object,field,18)?;}
                }
            }
            if owner(r,object,field,&mut checks)?!=expected {return Err("Audit motion owner mismatch".into());}
        }
        let mut objects=vec![("motion",motion),("package",package),("comp",comp)];
        if let Ok(config)=owner(r,comp,"m_Config",&mut checks) {
            if config!=0 {
                let id=r.field(config,"m_SkillID")?;
                let raw=r.read(id,4)?;
                if i32_at(&raw,0)!=skill{return Err("Audit config skill mismatch".into());}
                checks.push((id,raw));objects.push(("config",config));
            }
        }
        let mut schema=Vec::new();
        for (role,obj) in objects {
            let mut cls=r.pointer(obj)?;
            checks.push((obj,cls.to_le_bytes().to_vec()));
            for _ in 0..16 {
                if cls==0 {break;}
                let c=r.class(cls)?;
                let mut fields:Vec<_>=c.fields.iter().map(|(name,f)|serde_json::json!({"name":name,"kind":f.kind,"static":f.is_static})).collect();
                fields.sort_by_key(|v|v["name"].as_str().unwrap_or("").to_owned());
                schema.push(serde_json::json!({"role":role,"class":c.name,"fields":fields}));cls=c.parent;
            }
        }
        check(r,&checks)?;
        Ok(serde_json::json!({"skill":skill,"motion":mc.name,"package":pc.name,"schema":schema}))
    }
    /// Separate audit bracket: values of known shared indicator fields only.
    /// These observations never authorize an unverified production template.
    pub fn audit_registry_values<M: Memory>(r: &mut Reflection<M>, indicator:u64, skill:i32)->Result<serde_json::Value> {
        let elapsed=std::time::Instant::now();
        let (mut checks,comp,motion,package,mc,pc)=Self::registry_owner(r,indicator,skill)?;
        let base_tap=zero_extent_base_tap_contract(r,comp,motion,package,skill,&mc.name,&pc.name,&mut checks)?;
        if !base_tap&&!["IndicatorMotionRot","IndicatorMotionPos","IndicatorMotionRotAndPosRot","IndicatorMotionFaTiaoQiu2","IndicatorMotionArcSurface"].contains(&mc.name.as_str()) {return Err("Unverified diagnostic motion layout".into());}
        for (object,field,expected) in [(motion,"m_OwnComp",comp),(motion,"m_OwnPackage",package),(package,"m_OwnComp",comp)] {
            if base_tap&&object==motion&&field=="m_OwnPackage" {
                match r.field(object,field) {
                    Err(error)if error=="Missing reflected field m_OwnPackage"=>continue,
                    Err(error)=>return Err(error),
                    Ok(_)=>{typed_field(r,object,field,18)?;}
                }
            }
            if owner(r,object,field,&mut checks)?!=expected {return Err("Audit motion owner mismatch".into());}
        }
        let mut values=serde_json::Map::new();
        for name in ["m_vDragResult","m_vDragTouchPos","m_vDragDir","m_vActiveDir"] {
            let at=if base_tap{typed_field(r,motion,name,17)?}else{r.field(motion,name)?};
            if at<motion || at+12>motion+4096 {return Err("Audit vector outside motion".into());}
            let raw=capture(r,&mut checks,at,12)?;
            values.insert(name.into(),serde_json::json!(source::vec3(&raw)?));
        }
        for name in ["m_lastMotioningPos","m_lastMotioningDir","m_vLastUpdateDir"] {
            if let Ok(at)=r.field(motion,name) {
                let raw=capture(r,&mut checks,at,12)?;
                values.insert(name.into(),serde_json::json!(source::vec3(&raw)?));
            }
        }
        for name in ["m_fDragRatio","m_dragged","m_bVaildDraged"] {
            let at=if base_tap{typed_field(r,motion,name,if name=="m_fDragRatio"{12}else{2})?}else{r.field(motion,name)?};
            let raw=capture(r,&mut checks,at,if name=="m_fDragRatio" {4}else{1})?;
            let value=if name=="m_fDragRatio" {
                let v=f32_at(&raw,0);if !v.is_finite() || v<0. || v>=1e6{return Err("Invalid audit ratio".into());}serde_json::json!(v)
            } else {if raw[0]>1{return Err("Invalid audit drag boolean".into());}serde_json::json!(raw[0]!=0)};
            values.insert(name.into(),value);
        }
        if mc.name == "IndicatorMotionArcSurface" {
            for name in ["m_vStartOffsete","m_vEndOffsete"] {
                let at=r.field(package,name)?;let raw=capture(r,&mut checks,at,12)?;
                values.insert(format!("arc.package.{name}"),serde_json::json!(source::vec3(&raw)?));
            }
            if let Ok(at)=r.field(package,"_dragArrow") {
                let child=u64_at(&capture(r,&mut checks,at,8)?,0);
                if child!=0 {
                    let mut c=r.class(r.pointer(child)?)?;let mut schema=Vec::new();
                    values.insert("arc.arrow.class".into(),serde_json::json!(c.name));
                    for _ in 0..16 {
                        for (name,f) in &c.fields {
                            if f.is_static || f.offset>8192 {continue;}
                            schema.push(serde_json::json!({"name":name,"kind":f.kind}));
                            if f.kind==17 && (name.to_ascii_lowercase().contains("pos") || name.to_ascii_lowercase().contains("dir") || name.to_ascii_lowercase().contains("offset") || matches!(name.as_str(),"resultV3"|"resultV3Local"|"v1Temp"|"v2Temp")) {
                                let raw=capture(r,&mut checks,child+f.offset,12)?;
                                values.insert(format!("arc.arrow.{name}"),serde_json::json!(source::vec3(&raw)?));
                            }
                            if f.kind==18 {
                                let object=u64_at(&capture(r,&mut checks,child+f.offset,8)?,0);
                                if object==0{continue;}
                                if let Ok(cls)=r.pointer(object).and_then(|a|r.class(a)){values.insert(format!("arc.arrow.link.{name}.class"),serde_json::json!(cls.name));}
                                for component in [false,true] {
                                    if let Ok(path)=Path::new(r,object,component) {
                                        let raw=capture_path(r,&mut checks,&path)?;
                                        if let Some(pos)=crate::math::trs_chain(&raw,None){values.insert(format!("arc.arrow.link.{name}.position"),serde_json::json!(pos));break;}
                                    }
                                }
                            }
                        }
                        if c.parent==0{break}c=r.class(c.parent)?;
                    }
                    values.insert("arc.arrow.schema".into(),serde_json::json!(schema));
                }
            }
            let surface=owner(r,motion,"m_ArcSurface",&mut checks)?;
            if surface!=0 {
                let mut cls=r.class(r.pointer(surface)?)?;
                values.insert("arc.surface_class".into(),serde_json::json!(cls.name));
                for _ in 0..16 {
                    for (name,f) in &cls.fields {
                        if f.is_static || f.offset>8192 {continue;}
                        let length=match f.kind {2=>1,8|12=>4,17=>12,_=>continue};
                        if f.kind==17 && !name.to_ascii_lowercase().contains("pos") && !name.to_ascii_lowercase().contains("dir") && !name.to_ascii_lowercase().contains("offset") {continue;}
                        let raw=capture(r,&mut checks,surface+f.offset,length)?;
                        let v=match f.kind {2=>serde_json::json!(raw[0]!=0),8=>serde_json::json!(i32_at(&raw,0)),12=>serde_json::json!(f32_at(&raw,0)),17=>serde_json::json!(source::vec3(&raw)?),_=>unreachable!()};
                        values.insert(format!("arc.surface.{name}"),v);
                    }
                    if cls.parent==0{break;}cls=r.class(cls.parent)?;
                }
                for component in [true,false] {
                    if let Ok(path)=Path::new(r,surface,component) {
                        let raw=capture_path(r,&mut checks,&path)?;
                        if let Some(pos)=crate::math::trs_chain(&raw,None){values.insert(format!("arc.surface.position.{component}"),serde_json::json!(pos));}
                    }
                }
            }
        }
        let config=owner(r,comp,"m_Config",&mut checks)?;
        let cls=r.class(r.pointer(config)?)?;
        if cls.name!="CData_Skill_Element" {return Err("Unexpected audit config class".into());}
        checks.push((config,cls.address.to_le_bytes().to_vec()));
        for name in ["m_SkillID","m_RectType","m_BaseType"] {
            let f=cls.fields.get(name).ok_or("Missing typed audit config field")?;
            if f.kind!=8 || f.is_static {return Err("Audit config integer type changed".into());}
            let raw=capture(r,&mut checks,config+f.offset,4)?;
            let value=i32_at(&raw,0);
            if name=="m_SkillID" && value!=skill {return Err("Audit config skill changed".into());}
            values.insert(name.into(),serde_json::json!(value));
        }
        let params=owner(r,config,"m_ShowRectPara",&mut checks)?;
        if params!=0 {
            let h=capture(r,&mut checks,params,32)?;
            let ac=u64_at(&h,0);let count=u64_at(&h,24);
            if count>64 {return Err("Audit config array too long".into());}
            let ah=capture(r,&mut checks,ac,0x50)?;
            if r.cstring(u64_at(&ah,16))?!="Single[]" || r.class(u64_at(&ah,0x40))?.name!="Single" {return Err("Audit config array type changed".into());}
            let raw=capture(r,&mut checks,params+32,count as usize*4)?;
            let data:Vec<_>=(0..count as usize).map(|i|f32_at(&raw,i*4)).collect();
            if data.iter().any(|v|!v.is_finite()){return Err("Invalid config float".into());}
            values.insert("m_ShowRectPara".into(),serde_json::json!(data));
        }
        for name in ["<m_gMain>k__BackingField", "m_gIndicator1", "m_gIndicator2"] {
            if let Ok(at)=r.field(package,name) {
                let object=u64_at(&capture(r,&mut checks,at,8)?,0);
                if object!=0 { if let Ok(path)=Path::new(r,object,false) {
                    let raw=capture_path(r,&mut checks,&path)?;
                    if let Some(pos)=crate::math::trs_chain(&raw,None) {values.insert(format!("visual.{name}"),serde_json::json!(pos));}
                }}
            }
        }
        check(r,&checks)?;
        Ok(serde_json::json!({"duration_ns":elapsed.elapsed().as_nanos(),"motion":mc.name,"package":pc.name,"values":values}))
    }
    pub fn discover_behavior<M: Memory>(r: &mut Reflection<M>, indicator: u64, skill: i32) -> Result<(Checks,u8)> {
        let (mut checks,comp,motion,package,mc,pc)=Self::registry_owner(r,indicator,skill)?;
        // Verified shared self-use buttons have no aiming indicator. Their
        // base pair is not a missing Range plan: validate config identity and
        // reciprocal native owners before publishing Tap, on every hero.
        if matches!(skill,20010|20030|20080) && mc.name == "IndicatorMotionBase" && pc.name == "IndicatorPackageBase" {
            let config = owner(r, comp, "m_Config", &mut checks)?;
            let config_class = r.pointer(config)?;
            checks.push((config, config_class.to_le_bytes().to_vec()));
            if r.class(config_class)?.name != "CData_Skill_Element" { return Err("Unknown shared tap config".into()); }
            let at = typed_field(r, config, "m_SkillID", 8)?;
            if i32_at(&capture(r, &mut checks, at, 4)?, 0) != skill { return Err("Shared tap config identity changed".into()); }
            if owner(r, motion, "m_OwnComp", &mut checks)? != comp || owner(r, package, "m_OwnComp", &mut checks)? != comp {
                return Err("Shared tap owner changed".into());
            }
            check(r, &checks)?;
            return Ok((checks, 1));
        }
        let base_tap=zero_extent_base_tap_contract(r,comp,motion,package,skill,&mc.name,&pc.name,&mut checks)?;
        let kind=if let Some(kind)=native_template(&mc.name,&pc.name) {
            if kind==3 {native_range_contract(r,comp,motion,package,skill,&mc.name,&pc.name,&mut checks)?.ok_or("Unverified native range family")?;}
            kind
        }
        else if base_tap {1}
        else if fixed_part_direction_contract(r,comp,skill,&mc.name,&pc.name,&mut checks)?
            || additional_direction_contract(r,comp,skill,&mc.name,&pc.name,&mut checks)? {2}
        else if mc.name=="IndicatorMotionPos" && pc.name=="IndicatorPackage_TwoPartAndPos_Direction" {
            let (rect,params)=native_rect_config(r,comp,skill,&mut checks)?;
            if !zero_extent_template(&mc.name,&pc.name,rect,&params) {return Err("Unverified nonzero point template".into());}
            1
        } else if mc.name=="IndicatorMotionRot" && pc.name=="IndicatorPackageEasy_JianTou_DrivenByJoystick" {
            let (rect,params)=native_rect_config(r,comp,skill,&mut checks)?;
            if !movement_driven_template(&mc.name,&pc.name,rect,&params) {return Err("Unverified movement-driven template".into());}
            // The game takes this action's direction from movement/facing.
            // Its drag preview is not an authoritative cast-direction receipt.
            1
        } else {return Err("Unverified native template".into());};
        for (object,field,expected) in [(motion,"m_OwnComp",comp),(motion,"m_OwnPackage",package),(package,"m_OwnComp",comp)] {
            if base_tap&&object==motion&&field=="m_OwnPackage"{continue;}
            if owner(r,object,field,&mut checks)?!=expected {return Err("Native template owner mismatch".into());}
        }
        check(r,&checks)?;Ok((checks,kind))
    }
    pub fn discover<M: Memory>(r: &mut Reflection<M>, indicator: u64, skill: i32) -> Result<Self> {
        let owner=Self::registry_owner(r,indicator,skill)?;
        Self::discover_owned(r,skill,owner,false)
    }
    pub fn audit_attack_preview<M:Memory>(r:&mut Reflection<M>,indicator:u64,skill:i32,base:i32)->Result<serde_json::Value>{
        let mut checks=Vec::new();
        let circle=owner(r,indicator,"m_CircleComp",&mut checks)?;
        let comp=owner(r,circle,"m_BaseComp",&mut checks)?;
        let config=owner(r,comp,"m_Config",&mut checks)?;
        let id=typed_field(r,config,"m_SkillID",8)?;
        let configured=i32_at(&capture(r,&mut checks,id,4)?,0);
        let shape=native_rect_config(r,comp,configured,&mut checks)?;
        let plan=Self::discover_attack(r,indicator,skill,base).map_err(|e|format!("{e}; base={base}, active={skill}, configured={configured}, shape={shape:?}"))?;
        Ok(serde_json::json!({"motion":plan.name,"configured_skill":configured,"drag_gate":plan.audit_drag_gate(r)?}))
    }
    pub(crate) fn discover_attack<M:Memory>(r:&mut Reflection<M>,indicator:u64,skill:i32,base:i32)->Result<Self>{
        let(mut checks,default,_,_,_,_)=Self::registry_owner(r,indicator,skill)?;
        let circle=owner(r,indicator,"m_CircleComp",&mut checks)?;
        let comp=owner(r,circle,"m_BaseComp",&mut checks)?;
        if owner(r,comp,"m_DefBaseComp",&mut checks)?!=default{return Err("Attack preview default owner mismatch".into());}
        let motion=owner(r,comp,"m_MontionBase",&mut checks)?;
        let package=owner(r,comp,"m_PackageBase",&mut checks)?;
        let mc=r.class(r.pointer(motion)?)?;let pc=r.class(r.pointer(package)?)?;
        if mc.name!="IndicatorMotionPosSelectAndRotATK"||pc.name!="IndicatorPackage_JianTou_Yuan_SameLengthWithRange" {return Err("Unsupported attack preview".into());}
        for(object,c)in[(comp,r.pointer(comp)?),(motion,mc.address),(package,pc.address)]{checks.push((object,c.to_le_bytes().to_vec()));}
        let flag_at=typed_field(r,motion,"bCommonAtk",2)?;
        if capture(r,&mut checks,flag_at,1)?!=[1]{return Err("Preview is not a common attack".into());}
        if base<=0||base>10_000_000{return Err("Invalid attack base binding".into());}
        // The game overlays a shared selection skill (currently28910), not
        // the actual damage skill. The reciprocal/default owners bind it to
        // this current attack; equating those two IDs rejects valid previews.
        let config=owner(r,comp,"m_Config",&mut checks)?;
        let cc=r.class(r.pointer(config)?)?;
        if cc.name!="CData_Skill_Element"{return Err("Attack selector configuration type".into());}
        checks.push((config,cc.address.to_le_bytes().to_vec()));
        let id_at=typed_field(r,config,"m_SkillID",8)?;
        let configured=i32_at(&capture(r,&mut checks,id_at,4)?,0);
        if configured<=0||configured>10_000_000{return Err("Invalid attack selector identity".into());}
        let(rect,params)=native_rect_config(r,comp,configured,&mut checks)?;
        if rect!=0||params[0]!=14.||!params[1].is_finite()||params[1]<=0. {return Err("Unsupported attack configuration".into());}
        let config=owner(r,comp,"m_Config",&mut checks)?;
        if owner(r,motion,"m_Config",&mut checks)?!=config||owner(r,package,"m_Config",&mut checks)?!=config{return Err("Attack configuration ownership mismatch".into());}
        Self::discover_owned(r,skill,(checks,comp,motion,package,mc,pc),true)
    }
    fn discover_owned<M:Memory>(r:&mut Reflection<M>,skill:i32,owned:(Checks,u64,u64,u64,crate::reflection::Class,crate::reflection::Class),attack_geometry:bool)->Result<Self>{
        let(mut checks,comp,motion,package,motion_class,package_class)=owned;
        let name=motion_class.name.clone();
        let range_contract=native_range_contract(r,comp,motion,package,skill,&name,&package_class.name,&mut checks)?;
        let configured_direction=fixed_part_direction_contract(r,comp,skill,&name,&package_class.name,&mut checks)?
            || additional_direction_contract(r,comp,skill,&name,&package_class.name,&mut checks)?;
        let direction_geometry=verified_direction_geometry(skill,&name,&package_class.name,configured_direction);
        let expected = match skill {
            2510 => Some((
                "IndicatorMotionRotAndPosRot",
                "IndicatorPackage_JianTou_Yuan",
            )),
            2511 => Some(("IndicatorMotionFaTiaoQiu2", "IndicatorPackage_JianTou_Yuan")),
            2520 => Some(("IndicatorMotionRot", "IndicatorPackageEasy_JianTou")),
            20150 => Some(("IndicatorMotionRot", "IndicatorPackageEasy_ShanXing_Normal")),
            20020 => Some(("IndicatorLieShaMotion", "IndicatorPackageEasy_ShanXing_Normal")),
            8430 => Some(("IndicatorMotionRot", "IndicatorPackageEasy")),
            _ if attack_geometry || configured_direction || matches!(native_template(&name,&package_class.name),Some(2 | 3)) => Some((name.as_str(),package_class.name.as_str())),
            _ => None,
        };
        if expected.is_none_or(|(motion, package)| name != motion || package_class.name != package)
        {
            return Err(format!(
                "Unverified native skill/motion/package {skill}/{name}/{}",
                package_class.name
            ));
        }
        checks.extend([
            (motion, motion_class.address.to_le_bytes().to_vec()),
            (package, package_class.address.to_le_bytes().to_vec()),
            (comp, r.pointer(comp)?.to_le_bytes().to_vec()),
        ]);
        if owner(r, motion, "m_OwnComp", &mut checks)? != comp
            || owner(r, motion, "m_OwnPackage", &mut checks)? != package
            || owner(r, package, "m_OwnComp", &mut checks)? != comp
        {
            return Err("Native motion owner mismatch".into());
        }
        let mut fields = HashMap::new();
        for n in [
            "m_vDragResult",
            "m_vDragTouchPos",
            "m_vDragDir",
            "m_vActiveDir",
            "m_fDragRatio",
        ] {
            fields.insert(n.into(), r.field(motion, n)?);
        }
        let (direction_visual,direction_arrow_field,direction_flags)=if name=="IndicatorMotionArcSurface" {
            (Some((typed_field(r,package,"fDefaultRayLength",12)?,typed_field(r,package,"m_vStartOffsete",17)?,typed_field(r,package,"m_vEndOffsete",17)?)),
             Some(typed_field(r,package,"_dragArrow",18)?),
             Some((typed_field(r,motion,"m_dragged",2)?,typed_field(r,motion,"m_bVaildDraged",2)?)))
        }else{(None,None,None)};
        let circle_field = typed_field(r,package, "m_gYuan",18).ok().or_else(|| {
            point_visual_fallback(&name,&package_class.name)
                .then(||typed_field(r,package,"<m_gMain>k__BackingField",18).ok()).flatten()
        });
        let circle_owner = circle_field.and_then(|a| r.pointer(a).ok()).unwrap_or(0);
        let circle = if let Some(a) = circle_field {
            let o = r.pointer(a)?;
            if o != 0 {
                Path::new(r, o, false).ok()
            } else {
                None
            }
        } else {
            None
        };
        let range = range_contract.map(|(address,_)|address);
        let range_shape = range_contract.map(|(_,shape)|shape);
        let legacy_range=matches!(skill,2510|2511|8425);
        let mut protect = None;
        if range_contract.is_some() || attack_geometry {
            for n in ["m_dragged", "m_bVaildDraged"] {
                fields.insert(n.into(), typed_field(r,motion,n,2)?);
            }
            let at=typed_field(r,motion,"m_DragProtectComp",18)?;
            let p=u64_at(&capture(r,&mut checks,at,8)?,0);
            let cls = r.class(r.pointer(p)?)?;
            if cls.name != "DragProtectComp" {
                return Err("Native drag protector changed type".into());
            }
            checks.push((p, cls.address.to_le_bytes().to_vec()));
            let owner_at=typed_field(r,p,"m_Owner",18)?;
            if u64_at(&capture(r,&mut checks,owner_at,8)?,0)!=motion {return Err("Native drag protector belongs to another motion".into());}
            protect = Some((
                typed_field(r,p,"m_vFirstPress",17)?,
                typed_field(r,p,"bInFirstDrag",2)?,
                typed_value_field(r,p,"m_fSkillDeadZone",12,true)?,
            ));
        }
        check(r, &checks)?;
        Ok(Self {
            checks,
            skill,
            name,
            motion,
            direction_geometry,
            attack_geometry,
            fields,
            circle,
            circle_field,
            circle_owner,
            range,
            range_shape,
            legacy_range,
            protect,
            direction_visual,
            direction_arrow_field,
            direction_flags,
            direction_endpoint:None,
        })
    }
    /// This flag is set only by a verified motion/package/configuration contract.
    /// Rot alone is insufficient: the same motion also implements plain Taps.
    pub(crate) fn is_direction_geometry(&self)->bool {self.direction_geometry}
    /// A charge phase may name a different indicator than its physical button.
    /// Authorize that relationship only from this owned base motion's current
    /// typed cover list. Registry membership or similar numeric IDs do not bind
    /// a phase to the button, and list membership is never cached across samples.
    pub(crate) fn covers_direction_skill<M:Memory>(&self,r:&mut Reflection<M>,actual:i32,consumed:&mut Checks)->Result<bool> {
        check(r,&self.checks)?;
        if !self.direction_geometry{return Ok(false);}
        if !(1..=10_000_000).contains(&actual){return Err("Invalid covered skill identity".into());}
        let mut reads=self.checks.clone();
        let class=u64_at(&capture(r,&mut reads,self.motion,8)?,0);
        r.class(class)?;
        let field=typed_field(r,self.motion,"lCoverSkills",21)?;
        let list=u64_at(&capture(r,&mut reads,field,8)?,0);
        if list==0 {
            check(r,&reads)?;consumed.extend(reads);return Ok(false);
        }
        let class=u64_at(&capture(r,&mut reads,list,8)?,0);
        if r.class(class)?.name!="List`1"{return Err("Covered skill list type changed".into());}
        let items=typed_field(r,list,"_items",29)?;
        let size=typed_field(r,list,"_size",8)?;
        let array=u64_at(&capture(r,&mut reads,items,8)?,0);
        let count=i32_at(&capture(r,&mut reads,size,4)?,0);
        if !(0..=128).contains(&count){return Err("Covered skill list size changed".into());}
        if array==0 {
            if count!=0{return Err("Covered skill list lost its array".into());}
            check(r,&reads)?;consumed.extend(reads);return Ok(false);
        }
        let header=capture(r,&mut reads,array,32)?;
        let capacity=u64_at(&header,24);
        if capacity<count as u64||capacity>4096{return Err("Covered skill array size changed".into());}
        let metadata=capture(r,&mut reads,u64_at(&header,0),0x50)?;
        if r.cstring(u64_at(&metadata,16))?!="Int32[]"||r.class(u64_at(&metadata,0x40))?.name!="Int32" {
            return Err("Covered skill array element type changed".into());
        }
        let values=capture(r,&mut reads,array+32,count as usize*4)?;
        let mut found=false;
        for value in values.chunks_exact(4) {
            let skill=i32_at(value,0);
            if !(1..=10_000_000).contains(&skill){return Err("Invalid covered skill list member".into());}
            found|=skill==actual;
        }
        check(r,&reads)?;
        consumed.extend(reads);
        Ok(found)
    }
    pub fn sample_direction_visual<M:Memory>(&mut self,r:&mut Reflection<M>,hero:[f64;3],active:bool,consumed:&mut Checks)->Result<Option<crate::projection::DirectionVisual>> {
        let Some((range,start,end))=self.direction_visual else{return Ok(None);};
        check(r,&self.checks)?;consumed.extend(self.checks.iter().cloned());
        let range=f32_at(&capture(r,consumed,range,4)?,0);
        let start=source::vec3(&capture(r,consumed,start,12)?)?;
        let end=source::vec3(&capture(r,consumed,end,12)?)?;
        if !range.is_finite()||range<=0.||range>1e4||start.iter().chain(end.iter()).any(|v|v.abs()>1e3){return Err("Invalid native visual direction geometry".into());}
        let mut spec=crate::projection::DirectionVisual::new(self.skill,range,start,end);
        if !active{return Ok(Some(spec));}
        let Some((dragged,valid))=self.direction_flags else{return Ok(Some(spec));};
        let dragged=capture(r,consumed,dragged,1)?[0];let valid=capture(r,consumed,valid,1)?[0];
        if dragged>1||valid>1{return Err("Invalid arc drag flags".into());}
        if dragged==0||valid==0{return Ok(Some(spec));}
        let Some(at)=self.direction_arrow_field else{return Ok(Some(spec));};
        let arrow=u64_at(&capture(r,consumed,at,8)?,0);if arrow==0{return Ok(Some(spec));}
        let class=capture(r,consumed,arrow,8)?;
        if r.class(u64_at(&class,0))?.name!="CreateArcSurface"{return Err("Arc renderer class changed".into());}
        let endpoint_at=typed_field(r,arrow,"endTrans",18)?;
        let endpoint=u64_at(&capture(r,consumed,endpoint_at,8)?,0);if endpoint==0{return Ok(Some(spec));}
        let class=capture(r,consumed,endpoint,8)?;
        if r.class(u64_at(&class,0))?.name!="Transform"{return Err("Arc endpoint is not a Transform".into());}
        if self.direction_endpoint.as_ref().is_none_or(|(owner,path)|*owner!=endpoint||path.validate(r).is_err()) {
            self.direction_endpoint=Some((endpoint,Path::new(r,endpoint,true)?));
        }
        let path=&self.direction_endpoint.as_ref().unwrap().1;
        let raw=capture_path(r,consumed,path)?;
        let point=crate::math::trs_chain(&raw,None).ok_or("Invalid arc endpoint transform")?;
        let distance=(point[0]-hero[0]-end[0]).hypot(point[2]-hero[2]-end[2]);
        if distance>0.01&&distance<=range*1.2&&(point[1]-hero[1]).abs()<100. {
            // A wall hit shortens the arrow and may be at another elevation.
            // Use its actual geometry, not the maximum-length miss indicator.
            spec.range=distance;spec.end[1]=point[1]-hero[1];spec.observed=true;spec.target=Some(point);
        }
        Ok(Some(spec))
    }
    pub fn validate<M: Memory>(&self, r: &Reflection<M>) -> Result<()> {
        check(r, &self.checks)
    }
    /// Unity pools and reparents visual indicators between casts. Their
    /// transform cache has a shorter lifetime than the native skill owner.
    pub fn refresh_visual<M: Memory>(&mut self, r: &mut Reflection<M>) {
        self.refresh_visual_with(r, |r, object| Path::new(r, object, false));
    }
    fn refresh_visual_with<M: Memory>(
        &mut self,
        r: &mut Reflection<M>,
        mut discover: impl FnMut(&mut Reflection<M>, u64) -> Result<Path>,
    ) {
        let Some(address) = self.circle_field else {
            return;
        };
        let owner = r.pointer(address).unwrap_or(0);
        if owner == self.circle_owner && self.circle.as_ref().is_some_and(|p| p.validate(r).is_ok())
        {
            return;
        }
        self.circle = None;
        self.circle_owner = owner;
        if owner != 0 && self.validate(r).is_ok() {
            if let Ok(path) = discover(r, owner) {
                if r.pointer(address).ok() == Some(owner) && path.validate(r).is_ok() {
                    self.circle = Some(path);
                }
            }
        }
    }
    pub fn unavailable(&self, frame: &mut s::Frame) {
        // A transient read cannot switch Q from relative to visual coordinates.
        // Keep verified identity while revoking every value and acknowledgement.
        frame.logical = if !self.direction_geometry {
            s::Logical {
                source: text(&format!("{}.m_vDragResult", self.name)),
                skill_id: number(self.skill as f64),
                ..Default::default()
            }
        } else {
            Default::default()
        };
        frame.direction_input = Default::default();
        frame.motion = Default::default();
        frame.active_joystick = Default::default();
    }
    pub fn sample<M: Memory>(
        &self,
        r: &Reflection<M>,
        frame: &mut s::Frame,
    ) -> Result<Option<[f64; 3]>> {
        let mut consumed = Vec::new();
        let result = self.sample_bracketed(r, frame, &mut consumed)?;
        check(r, &consumed)?;
        Ok(result)
    }
    pub fn sample_bracketed<M: Memory>(
        &self,
        r: &Reflection<M>,
        frame: &mut s::Frame,
        consumed: &mut Checks,
    ) -> Result<Option<[f64; 3]>> {
        self.sample_bracketed_with_fixed_range(r,frame,consumed,false)
    }
    /// A new family receives fixed-range metadata only after the caller's
    /// coherent local skill-data observation proves this is not a charge skill.
    /// The old entry point preserves established Kagura/Ling behavior.
    pub fn sample_bracketed_with_fixed_range<M: Memory>(
        &self,r:&Reflection<M>,frame:&mut s::Frame,consumed:&mut Checks,allow_fixed_range:bool,
    )->Result<Option<[f64;3]>> {
        check(r, &self.checks)?;
        consumed.extend(self.checks.iter().cloned());
        let low = *self
            .fields
            .values()
            .min()
            .ok_or("Empty native motion plan")?;
        let high = self
            .fields
            .iter()
            .map(|(n, a)| {
                a + if ["m_dragged", "m_bVaildDraged"].contains(&n.as_str()) {
                    1
                } else if n == "m_fDragRatio" {
                    4
                } else {
                    12
                }
            })
            .max()
            .unwrap();
        if low < self.motion || high > self.motion + 4096 {
            return Err("Invalid native motion bounds".into());
        }
        let raw = capture(r, consumed, low, (high - low) as usize)?;
        let vector = |name: &str| -> Result<[f64; 3]> {
            let at = (self.fields[name] - low) as usize;
            source::vec3(&raw[at..at + 12])
        };
        let result = vector("m_vDragResult")?;
        let touch = vector("m_vDragTouchPos")?;
        vector("m_vDragDir")?;
        vector("m_vActiveDir")?;
        let ratio = f32_at(&raw, (self.fields["m_fDragRatio"] - low) as usize);
        if !ratio.is_finite() || ratio < 0. || ratio >= 1e6 {
            return Err("Invalid native drag ratio".into());
        }
        let now = frame.time_ns;
        let origin = frame.hero_render_xyz.values.map(|n| n.value);
        if let Some(address) = self.circle_field {
            let raw = capture(r, consumed, address, 8)?;
            if u64_at(&raw, 0) != self.circle_owner {
                return Err("Native visual indicator owner changed during sample".into());
            }
        }
        let aim = if let Some(path) = &self.circle {
            if path.validate(r).is_ok() {
                let raw = capture_path(r, consumed, path)?;
                crate::math::trs_chain(&raw, None)
            } else {
                None
            }
        } else {
            None
        };
        if self.direction_geometry {
            if (result[0].hypot(result[2]) - 1.).abs() > 0.01 {
                return Err("Native direction is not a unit vector".into());
            }
            frame.direction_input = s::Direction {
                ready: flag(true),
                coherent: flag(true),
                source: text("IndicatorMotionRot.m_vDragResult"),
                token: frame.token,
                hero_id: frame.hero_id,
                slot: frame.ui.active_slot,
                skill_id: number(self.skill as f64),
                sample_start_ns: now,
                sample_end_ns: now,
                raw_touch_xyz: v3(touch),
                direction_xz: v2([result[0], result[2]]),
                ..Default::default()
            };
        } else {
            let mut logical = s::Logical {
                ready: flag(true),
                coherent: flag(true),
                source: text(&format!("{}.m_vDragResult", self.name)),
                skill_id: number(self.skill as f64),
                sample_start_ns: now,
                sample_end_ns: now,
                origin_render_xyz: v3(origin),
                raw_touch_xyz: v3(touch),
                offset_xz: v2([result[0], result[2]]),
                target_render_xyz: v3([origin[0] + result[0], origin[1], origin[2] + result[2]]),
                joystick: frame.ui.joystick,
                drag_ratio: number(ratio),
                ..Default::default()
            };
            if let Some(a) = self.range {
                if self.legacy_range||allow_fixed_range {
                    let values = capture(r, consumed, a, 8)?;
                    let distance = f32_at(&values, 4);
                    let shape=self.range_shape.ok_or("Native range shape missing")?;
                    if !valid_native_range(shape,[f32_at(&values,0),distance]) {
                        return Err("Invalid native world range".into());
                    }
                    logical.world_range = number(distance);
                    logical.world_range_source = text("CData_Skill_Element.m_ShowRectPara[1]");
                    logical.world_range_source_present = flag(true);
                    if r.read(a, 8)? != values {
                        return Err("Native range changed during sample".into());
                    }
                } else {
                    // The fixed configuration value is not an observation of
                    // a charged/growing range. Revoke only this metadata while
                    // retaining current drag readiness and the visual marker.
                    logical.world_range_source_present=flag(false);
                }
            }
            if let Some((first, active, dead_zone)) = self.protect {
                let dragged = raw[(self.fields["m_dragged"] - low) as usize];
                let valid = raw[(self.fields["m_bVaildDraged"] - low) as usize];
                let active_value = capture(r, consumed, active, 1)?[0];
                if [dragged, valid, active_value].iter().any(|v| *v > 1) {
                    return Err("Invalid native drag flags".into());
                }
                let first_value = source::vec3(&capture(r, consumed, first, 12)?)?;
                let zone = f32_at(&capture(r, consumed, dead_zone, 4)?, 0);
                if !zone.is_finite()
                    || zone < 0.
                    || zone >= 1e6
                    || first_value.iter().any(|v| v.abs() > 1e6)
                {
                    return Err("Invalid native dead zone".into());
                }
                logical.input_ready = flag(true);
                logical.valid_drag = flag(valid != 0);
                logical.first_drag = flag(active_value != 0);
                logical.ready = flag(dragged != 0 && valid != 0 && active_value == 0);
                if r.read(active, 1)?[0] != active_value {
                    return Err("Native drag gate changed during sample".into());
                }
            }
            if self.attack_geometry && logical.ready.get() && ratio>=0.05 && ratio<=1.001 {
                let range=result[0].hypot(result[2])/ratio;
                if range.is_finite() && range>0. && range<=1e4 {
                    logical.world_range=number(range);
                    logical.world_range_source=text("IndicatorMotionPosSelectAndRotATK.m_vDragResult/m_fDragRatio");
                    logical.world_range_source_present=flag(true);
                }
            }
            if let Some(p) = aim {
                logical.visual_plane_y = number(p[1]);
            }
            frame.motion = s::Motion {
                source: text("IndicatorMotionBase"),
                available: flag(true),
                bracket_stable: flag(true),
                drag_touch_raw_xyz: v3(touch),
                sample_start_ns: now,
                sample_end_ns: now,
            };
            frame.logical = logical;
        }
        frame.active_joystick = frame.ui.joystick;
        if r.read(low, (high - low) as usize)? != raw {
            return Err("Native motion changed during sample".into());
        }
        check(r, &self.checks)?;
        Ok(aim)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    struct MutableMemory(RefCell<Vec<u8>>);
    impl Memory for MutableMemory {
        fn regions(&self) -> Result<Vec<crate::platform::Region>> {
            Ok(vec![crate::platform::Region {start:0,end:self.0.borrow().len()as u64,readable:true,writable:true,name:"test-owned".into()}])
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
    fn q_fixture() -> (Reflection<MutableMemory>, MotionPlan) {
        let mut bytes = vec![0; 2048];
        for (at, value) in [
            (528, 3_f32),
            (536, 4.),
            (540, 700.),
            (544, 400.),
            (576, 0.5),
            (800, 8.),
            (804, 10.),
            (1040, 0.1),
        ] {
            bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[580] = 1;
        bytes[581] = 0;
        bytes[1036] = 1;
        let fields = [
            ("m_vDragResult", 528),
            ("m_vDragTouchPos", 540),
            ("m_vDragDir", 552),
            ("m_vActiveDir", 564),
            ("m_fDragRatio", 576),
            ("m_dragged", 580),
            ("m_bVaildDraged", 581),
        ]
        .into_iter()
        .map(|(n, a)| (n.to_owned(), a))
        .collect();
        let plan = MotionPlan {
            checks: Vec::new(),
            skill: 2510,
            name: "IndicatorMotionRotAndPosRot".into(),
            motion: 512,
            direction_geometry:false, attack_geometry:false,
            fields,
            circle: None,
            circle_field: None,
            circle_owner: 0,
            range: Some(800),
            range_shape:Some(8.),
            legacy_range:true,
            protect: Some((1024, 1036, 1040)),
            direction_visual: None,
            direction_arrow_field:None,
            direction_flags:None,
            direction_endpoint:None,
        };
        (
            Reflection::new(MutableMemory(RefCell::new(bytes))).unwrap(),
            plan,
        )
    }
    #[test]
    fn attack_range_is_measured_from_current_processed_drag_not_the_attack_distance_config() {
        let(r,mut plan)=q_fixture();plan.attack_geometry=true;plan.range=None;
        plan.name="IndicatorMotionPosSelectAndRotATK".into();
        {let mut b=r.memory.0.borrow_mut();b[528..532].copy_from_slice(&3f32.to_le_bytes());b[532..536].copy_from_slice(&0f32.to_le_bytes());b[536..540].copy_from_slice(&4f32.to_le_bytes());b[576..580].copy_from_slice(&0.5f32.to_le_bytes());b[581]=1;b[1036]=0;}
        let mut f=s::Frame::default();plan.sample(&r,&mut f).unwrap();
        assert!(f.logical.ready.get());assert_eq!(f.logical.world_range.value,10.);
        assert_eq!(f.logical.world_range_source.as_str(),"IndicatorMotionPosSelectAndRotATK.m_vDragResult/m_fDragRatio");
        for ratio in [0.,0.01,1.1] {
            r.memory.0.borrow_mut()[576..580].copy_from_slice(&(ratio as f32).to_le_bytes());
            let mut f=s::Frame::default();plan.sample(&r,&mut f).unwrap();assert!(f.logical.world_range.missing());
        }
        r.memory.0.borrow_mut()[576..580].copy_from_slice(&0.5f32.to_le_bytes());r.memory.0.borrow_mut()[581]=0;
        let mut f=s::Frame::default();plan.sample(&r,&mut f).unwrap();assert!(f.logical.world_range.missing());
    }
    #[test]
    fn live_arc_visual_parameters_share_the_outer_coherence_bracket() {
        let (mut r,mut plan)=q_fixture();plan.skill=8410;plan.direction_visual=Some((1200,1216,1232));
        r.memory.0.borrow_mut()[1200..1204].copy_from_slice(&12f32.to_le_bytes());
        r.memory.0.borrow_mut()[1236..1240].copy_from_slice(&1.5f32.to_le_bytes());
        let mut checks=Vec::new();let spec=plan.sample_direction_visual(&mut r,[0.;3],false,&mut checks).unwrap().unwrap();
        assert_eq!(spec.range,12.);assert_eq!(spec.end,[0.,1.5,0.]);
        r.memory.0.borrow_mut()[1236..1240].copy_from_slice(&2f32.to_le_bytes());
        assert!(check(&r,&checks).is_err());
        r.memory.0.borrow_mut()[1200..1204].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(plan.sample_direction_visual(&mut r,[0.;3],false,&mut Vec::new()).is_err());
    }
    #[test]
    fn ling_arc_is_directional_while_wall_w_is_positional() {
        assert_eq!(native_template("IndicatorMotionArcSurface","IndicatorPackage_ArcSurface"),Some(2));
        assert_eq!(native_template("IndicatorMotionPos","IndicatorPackageEasy_Yuan"),Some(3));
        assert_eq!(native_template("IndicatorMotionArcSurface","Unknown"),None);
    }
    #[test]
    fn ling_arc_unit_receipt_never_synthesizes_a_radial_target() {
        let (r,mut plan)=q_fixture();plan.skill=8410;plan.name="IndicatorMotionArcSurface".into();
        plan.direction_geometry=true;plan.range=None;plan.protect=None;
        let set=|at:usize,v:f32|r.memory.0.borrow_mut()[at..at+4].copy_from_slice(&v.to_le_bytes());
        set(528,0.6);set(536,0.8);
        for ratio in [0.2,0.8,1.] {
            set(576,ratio);
            let mut frame=s::Frame::default();plan.sample(&r,&mut frame).unwrap();
            assert!(frame.direction_input.ready.get());assert!(!frame.logical.ready.get());
            assert!((frame.direction_input.direction_xz.values[0].value-0.6).abs()<1e-6);
            assert!((frame.direction_input.direction_xz.values[1].value-0.8).abs()<1e-6);
        }
        set(528,3.);set(536,4.);
        assert!(plan.sample(&r,&mut s::Frame::default()).is_err());
    }
    #[test]
    fn pooled_visual_rebuilds_after_reparent_and_missing_initial_transform() {
        let (mut r, mut plan) = q_fixture();
        plan.circle_field = Some(1200);
        r.memory.0.borrow_mut()[1200..1208].copy_from_slice(&1400u64.to_le_bytes());
        let mut discovers = 0;
        let mut discover = |_r: &mut Reflection<MutableMemory>, object| {
            discovers += 1;
            assert_eq!(object, 1400);
            Ok(Path {
                checks: vec![(1300, vec![1])],
                transforms: vec![],
            })
        };
        // A native indicator created after initial metadata discovery must be retried.
        r.memory.0.borrow_mut()[1300] = 1;
        plan.refresh_visual_with(&mut r, &mut discover);
        assert!(plan.circle.is_some());
        plan.refresh_visual_with(&mut r, &mut discover);
        assert_eq!(discovers, 1);
        // Same pooled GameObject, different hierarchy: invalidate and reacquire.
        r.memory.0.borrow_mut()[1300] = 2;
        plan.refresh_visual_with(&mut r, |_r, object| {
            assert_eq!(object, 1400);
            Ok(Path {
                checks: vec![(1300, vec![2])],
                transforms: vec![],
            })
        });
        assert!(plan.circle.as_ref().unwrap().validate(&r).is_ok());
        // A pool release revokes the old transform; another object can replace it.
        r.memory.0.borrow_mut()[1200..1208].copy_from_slice(&0u64.to_le_bytes());
        plan.refresh_visual_with(&mut r, |_, _| {
            panic!("null visual must not be dereferenced")
        });
        assert!(plan.circle.is_none());
        r.memory.0.borrow_mut()[1200..1208].copy_from_slice(&1500u64.to_le_bytes());
        plan.refresh_visual_with(&mut r, |_, object| {
            assert_eq!(object, 1500);
            Ok(Path {
                checks: vec![],
                transforms: vec![],
            })
        });
        assert_eq!(plan.circle_owner, 1500);
        assert!(plan.circle.is_some());
    }
    #[test]
    fn visual_owner_replacement_revokes_the_whole_sample_receipt() {
        let (r, mut plan) = q_fixture();
        plan.circle_field = Some(1200);
        plan.circle_owner = 1400;
        r.memory.0.borrow_mut()[1200..1208].copy_from_slice(&1400u64.to_le_bytes());
        let mut frame = s::Frame::default();
        let mut consumed = Vec::new();
        plan.sample_bracketed(&r, &mut frame, &mut consumed)
            .unwrap();
        r.memory.0.borrow_mut()[1200..1208].copy_from_slice(&1500u64.to_le_bytes());
        assert!(check(&r, &consumed).is_err());
        assert!(plan.sample(&r, &mut frame).is_err());
    }
    #[test]
    fn protected_q_exposes_input_but_never_acknowledges_processed_result() {
        let (r, plan) = q_fixture();
        let mut frame = s::Frame::default();
        plan.sample(&r, &mut frame).unwrap();
        assert!(frame.logical.input_ready.get());
        assert!(!frame.logical.ready.get());
        assert!(!frame.logical.valid_drag.get());
        assert!(frame.logical.first_drag.get());
        r.memory.0.borrow_mut()[581] = 1;
        r.memory.0.borrow_mut()[1036] = 0;
        plan.sample(&r, &mut frame).unwrap();
        assert!(frame.logical.ready.get());
        assert!(frame.logical.valid_drag.get());
        assert!(!frame.logical.first_drag.get());
    }
    #[test]
    fn generic_point_fixed_range_requires_non_charge_policy_and_revokes_without_losing_drag() {
        let (r,mut plan)=q_fixture();
        plan.skill=11810;plan.name="IndicatorMotionPos".into();plan.range_shape=Some(3.);plan.legacy_range=false;
        r.memory.0.borrow_mut()[800..804].copy_from_slice(&3f32.to_le_bytes());
        r.memory.0.borrow_mut()[804..808].copy_from_slice(&3f32.to_le_bytes());
        r.memory.0.borrow_mut()[581]=1;r.memory.0.borrow_mut()[1036]=0;
        plan.checks.push((800,3f32.to_le_bytes().to_vec()));
        let mut frame=s::Frame::default();let mut reads=Vec::new();
        plan.sample_bracketed_with_fixed_range(&r,&mut frame,&mut reads,true).unwrap();
        assert_eq!(frame.logical.world_range.value,3.);
        assert!(frame.logical.world_range_source_present.get());
        assert_eq!(frame.logical.world_range_source.as_str(),"CData_Skill_Element.m_ShowRectPara[1]");
        // Unknown/charged share the explicit false policy. Even a nonsensical
        // static distance must not suppress unrelated current input feedback.
        r.memory.0.borrow_mut()[804..808].copy_from_slice(&f32::NAN.to_le_bytes());
        reads.clear();
        plan.sample_bracketed_with_fixed_range(&r,&mut frame,&mut reads,false).unwrap();
        assert!(frame.logical.world_range.missing());
        assert!(frame.logical.world_range_source.missing());
        assert!(frame.logical.world_range_source_present.is_bool());
        assert!(!frame.logical.world_range_source_present.get());
        assert!(frame.logical.ready.get()&&frame.logical.input_ready.get());
        assert!(frame.logical.valid_drag.get()&&!frame.logical.first_drag.get());
        assert!(plan.sample_bracketed_with_fixed_range(&r,&mut frame,&mut Vec::new(),true).is_err());
        // The compatibility entry point cannot silently authorize a new family.
        plan.sample(&r,&mut frame).unwrap();assert!(frame.logical.world_range.missing());
        // A changed shape revokes the cached contract even without permission
        // to read the distance itself.
        r.memory.0.borrow_mut()[800..804].copy_from_slice(&8f32.to_le_bytes());
        assert!(plan.sample(&r,&mut frame).is_err());
    }
    #[test]
    fn known_range_retains_legacy_metadata_and_generic_owner_replacement_revokes_sample() {
        let (r,mut plan)=q_fixture();let mut frame=s::Frame::default();
        plan.sample_bracketed_with_fixed_range(&r,&mut frame,&mut Vec::new(),false).unwrap();
        assert_eq!(frame.logical.world_range.value,10.);
        plan.skill=11810;plan.name="IndicatorMotionPos".into();plan.range_shape=Some(3.);plan.legacy_range=false;
        r.memory.0.borrow_mut()[800..804].copy_from_slice(&3f32.to_le_bytes());
        plan.circle_field=Some(1200);plan.circle_owner=1400;
        r.memory.0.borrow_mut()[1200..1208].copy_from_slice(&1400u64.to_le_bytes());
        let mut reads=Vec::new();plan.sample_bracketed_with_fixed_range(&r,&mut frame,&mut reads,false).unwrap();
        r.memory.0.borrow_mut()[1200..1208].copy_from_slice(&1500u64.to_le_bytes());
        assert!(check(&r,&reads).is_err());
        assert!(plan.sample(&r,&mut frame).is_err());
    }
    #[test]
    fn native_range_shapes_and_visual_fallback_are_exact_families() {
        for (motion,package,shape)in [
            ("IndicatorMotionPos","IndicatorPackageEasy_Yuan",3.),
            ("IndicatorMotionRotAndPosRot","IndicatorPackage_JianTou_Yuan",8.),
            ("IndicatorMotionFaTiaoQiu2","IndicatorPackage_JianTou_Yuan",8.),
        ] {
            assert_eq!(native_range_shape(motion,package),Some(shape));
            assert!(valid_native_range(shape,[shape,3.]));
            for values in [[shape+1.,3.],[shape,0.],[shape,-1.],[shape,f64::NAN],[shape,f64::INFINITY],[shape,10001.]] {
                assert!(!valid_native_range(shape,values));
            }
        }
        assert_eq!(native_range_shape("IndicatorMotionPos","Unknown"),None);
        assert_eq!(native_range_shape("Unknown","IndicatorPackageEasy_Yuan"),None);
        assert_eq!(native_range_shape("IndicatorMotionRot","IndicatorPackageEasy_Yuan"),None);
        assert!(point_visual_fallback("IndicatorMotionPos","IndicatorPackageEasy_Yuan"));
        assert!(!point_visual_fallback("IndicatorMotionPos","IndicatorPackage_TwoPartAndPos_Direction"));
        assert!(!point_visual_fallback("IndicatorMotionRot","IndicatorPackageEasy_Yuan"));
    }
    #[test]
    fn typed_range_field_rejects_wrong_kind_and_only_explicitly_allows_static_deadzone() {
        let mut bytes=vec![0u8;4096];let object=512u64;let class=1024u64;
        bytes[512..520].copy_from_slice(&class.to_le_bytes());
        for (at,value)in [(1040,1400u64),(1088,class),(1096,class),(1144,1600),(1192,2200)] {bytes[at..at+8].copy_from_slice(&value.to_le_bytes());}
        bytes[1284..1286].copy_from_slice(&1u16.to_le_bytes());
        bytes[1400..1405].copy_from_slice(b"Test\0");
        bytes[1600..1608].copy_from_slice(&1800u64.to_le_bytes());
        bytes[1608..1616].copy_from_slice(&1900u64.to_le_bytes());
        bytes[1616..1624].copy_from_slice(&class.to_le_bytes());
        bytes[1624..1628].copy_from_slice(&32u32.to_le_bytes());
        bytes[1800..1806].copy_from_slice(b"value\0");
        bytes[1908..1912].copy_from_slice(&((12u32<<16)|16).to_le_bytes());
        let mut r=Reflection::new(MutableMemory(RefCell::new(bytes))).unwrap();
        assert!(typed_field(&mut r,object,"value",12).is_err());
        assert_eq!(typed_value_field(&mut r,object,"value",12,true).unwrap(),2232);
        assert!(typed_value_field(&mut r,object,"value",2,true).is_err());
    }
    #[test]
    fn first_press_and_dead_zone_participate_in_the_outer_receipt_bracket() {
        let (r, plan) = q_fixture();
        let mut frame = s::Frame::default();
        let mut consumed = Vec::new();
        plan.sample_bracketed(&r, &mut frame, &mut consumed)
            .unwrap();
        assert!(check(&r, &consumed).is_ok());
        r.memory.0.borrow_mut()[1024] = 1;
        assert!(check(&r, &consumed).is_err());
        r.memory.0.borrow_mut()[1024] = 0;
        r.memory.0.borrow_mut()[1040] = 1;
        assert!(check(&r, &consumed).is_err());
    }
    #[test]
    fn transient_q_failure_keeps_only_the_verified_coordinate_identity() {
        let (_, plan) = q_fixture();
        let mut frame = s::Frame::default();
        frame.logical.ready = flag(true);
        frame.logical.input_ready = flag(true);
        plan.unavailable(&mut frame);
        assert_eq!(
            frame.logical.source.as_str(),
            "IndicatorMotionRotAndPosRot.m_vDragResult"
        );
        assert_eq!(frame.logical.skill_id.int(), 2510);
        assert!(!frame.logical.ready.get());
        assert!(!frame.logical.input_ready.get());
        assert!(!frame.motion.available.get());
    }
    #[test]
    fn native_templates_require_the_verified_motion_package_pair() {
        assert_eq!(native_template("IndicatorMotionRot","IndicatorPackageEasy"),Some(1));
        assert_eq!(native_template("IndicatorMotionRot","IndicatorPackageEasy_JianTou"),Some(2));
        assert_eq!(native_template("IndicatorMotionRot","IndicatorPackageEasy_ShanXing_Ranger"),Some(2));
        assert_eq!(native_template("IndicatorMotionRot","Unknown"),None);
        assert_eq!(native_template("Unknown","IndicatorPackageEasy"),None);
        assert_eq!(native_template("IndicatorMotionRotAndPosRot","IndicatorPackageEasy"),None);
    }

    #[test]
    fn zero_extent_tap_requires_exact_verified_point_config() {
        let m="IndicatorMotionPos";let p="IndicatorPackage_TwoPartAndPos_Direction";
        assert!(zero_extent_template(m,p,11,&[11.,0.]));
        for params in [&[11.,3.5][..],&[11.,-1.][..],&[11.,f64::NAN][..],&[11.][..],&[2.,0.][..]] {
            assert!(!zero_extent_template(m,p,11,params));
        }
        assert!(!zero_extent_template("Unknown",p,11,&[11.,0.]));
        assert!(!zero_extent_template(m,"Unknown",11,&[11.,0.]));
        assert!(!zero_extent_template(m,p,2,&[11.,0.]));
    }
    #[test]
    fn zero_extent_base_tap_requires_exact_class_pair_rect_and_zero_range() {
        let m="IndicatorMotionBase";let p="IndicatorPackageEasy_Yuan";
        assert!(zero_extent_base_tap_template(m,p,11,&[3.,0.,30006.,0.5]));
        for params in [&[3.,0.001][..],&[3.,-1.][..],&[3.,f64::NAN][..],&[3.,f64::INFINITY][..],
                       &[8.,0.][..],&[f64::NAN,0.][..],&[3.][..],&[][..]] {
            assert!(!zero_extent_base_tap_template(m,p,11,params));
        }
        for rect in [-1,0,1,10,12]{assert!(!zero_extent_base_tap_template(m,p,rect,&[3.,0.]));}
        assert!(!zero_extent_base_tap_template("IndicatorMotionPos",p,11,&[3.,0.]));
        assert!(!zero_extent_base_tap_template("Unknown",p,11,&[3.,0.]));
        assert!(!zero_extent_base_tap_template(m,"IndicatorPackageEasy",11,&[3.,0.]));
        // Neither the broad class allowlist nor feedback-plan discovery is
        // expanded: this Tap must pass the separate owner/configuration gate.
        assert_eq!(native_template(m,p),None);
    }
    #[test]
    fn additional_direction_requires_measured_exact_pair_rect_and_shape() {
        let m="IndicatorMotionRot";let p="IndicatorPackage_TwoPartAndPos_Direction";
        for rect in [1,2] {assert!(additional_direction_template(m,p,rect,&[11.,4.1]));}
        for rect in [-1,0,3,6,7,9,11] {assert!(!additional_direction_template(m,p,rect,&[11.,4.1]));}
        for values in [&[][..],&[11.][..],&[11.,0.][..],&[11.,-1.][..],&[11.,1e4][..],
            &[11.,f64::NAN][..],&[11.,f64::INFINITY][..],&[7.,4.1][..],&[9.,4.1][..],&[f64::NAN,4.1][..]] {
            assert!(!additional_direction_template(m,p,1,values));
        }
        for pair in [("IndicatorMotionPos",p),(m,"IndicatorPackageEasy_Yuan"),("Unknown",p),(m,"Unknown")] {
            assert!(!additional_direction_template(pair.0,pair.1,1,&[11.,4.1]));
        }
        assert_eq!(native_template(m,p),None,"class names alone do not authorize this direction family");
        for(package,rect,shape)in [("IndicatorPackage_Multi_JianTou",0,7.),("IndicatorPackage_TwoPart_Direction",19,6.)] {
            assert!(additional_direction_template(m,package,rect,&[shape,5.2]));
            assert!(!additional_direction_template(m,package,rect+1,&[shape,5.2]));
            assert!(!additional_direction_template(m,package,rect,&[11.,5.2]));
            assert!(!additional_direction_template(m,package,rect,&[shape,0.]));
            assert!(!additional_direction_template(m,package,rect,&[shape,f64::NAN]));
            assert!(!additional_direction_template("IndicatorMotionPos",package,rect,&[shape,5.2]));
        }
    }
    fn base_tap_fixture()->Reflection<MutableMemory> {
        fn put64(bytes:&mut[u8],at:usize,value:u64){bytes[at..at+8].copy_from_slice(&value.to_le_bytes());}
        fn class(bytes:&mut[u8],address:usize,name:&str,fields:&[(&str,u8,u32)],cursor:&mut usize){
            let name_at=*cursor;bytes[name_at..name_at+name.len()].copy_from_slice(name.as_bytes());*cursor+=name.len()+1;
            put64(bytes,address+16,name_at as u64);put64(bytes,address+0x40,address as u64);put64(bytes,address+0x48,address as u64);
            let table=*cursor;*cursor+=fields.len()*32;put64(bytes,address+0x78,table as u64);
            bytes[address+0x104..address+0x106].copy_from_slice(&(fields.len()as u16).to_le_bytes());
            for(i,(name,kind,offset))in fields.iter().enumerate(){
                let name_at=*cursor;bytes[name_at..name_at+name.len()].copy_from_slice(name.as_bytes());*cursor+=name.len()+1;
                let ty=*cursor;*cursor+=16;bytes[ty+8..ty+12].copy_from_slice(&((*kind as u32)<<16).to_le_bytes());
                let row=table+i*32;put64(bytes,row,name_at as u64);put64(bytes,row+8,ty as u64);put64(bytes,row+16,address as u64);
                bytes[row+24..row+28].copy_from_slice(&offset.to_le_bytes());
            }
        }
        let mut bytes=vec![0u8;32768];let mut cursor=8192;
        class(&mut bytes,3072,"Holder",&[("m_Config",17,32),("m_OwnComp",18,40)],&mut cursor);
        class(&mut bytes,4096,"CData_Skill_Element",&[("m_SkillID",8,32),("m_RectType",8,36),("m_ShowRectPara",29,40)],&mut cursor);
        class(&mut bytes,5120,"Single",&[],&mut cursor);
        class(&mut bytes,5632,"Single[]",&[],&mut cursor);put64(&mut bytes,5632+0x40,5120);
        for object in [128,256,384]{put64(&mut bytes,object,3072);put64(&mut bytes,object+32,1024);put64(&mut bytes,object+40,128);}
        put64(&mut bytes,1024,4096);bytes[1056..1060].copy_from_slice(&4030i32.to_le_bytes());
        bytes[1060..1064].copy_from_slice(&11i32.to_le_bytes());put64(&mut bytes,1064,1536);
        put64(&mut bytes,1536,5632);put64(&mut bytes,1560,4);
        for(at,v)in [(1568,3f32),(1572,0.),(1576,30006.),(1580,0.5)]{bytes[at..at+4].copy_from_slice(&v.to_le_bytes());}
        Reflection::new(MutableMemory(RefCell::new(bytes))).unwrap()
    }
    #[test]
    fn base_tap_contract_verifies_all_config_owners_type_skill_and_lifetime() {
        let mut r=base_tap_fixture();let mut reads=Vec::new();
        assert!(zero_extent_base_tap_contract(&mut r,128,256,384,4030,"IndicatorMotionBase","IndicatorPackageEasy_Yuan",&mut reads).unwrap());
        assert!(check(&r,&reads).is_ok());
        // A pooled package being rebound invalidates the already produced
        // behavioral receipt rather than preserving a previous hero's Tap.
        r.memory.0.borrow_mut()[424..432].copy_from_slice(&256u64.to_le_bytes());
        assert!(check(&r,&reads).is_err());
        for (address,bytes)in [
            (416,1280u64.to_le_bytes().to_vec()), // package configuration differs
            (424,256u64.to_le_bytes().to_vec()),  // package belongs to another comp
            (296,384u64.to_le_bytes().to_vec()),  // motion belongs to another comp
            (1056,999i32.to_le_bytes().to_vec()), // configuration skill differs
            (1060,0i32.to_le_bytes().to_vec()),   // unobserved rect
            (1568,8f32.to_le_bytes().to_vec()),   // unobserved shape
            (1572,1f32.to_le_bytes().to_vec()),   // nonzero extent
            (1572,f32::NAN.to_le_bytes().to_vec()),
        ] {
            let mut r=base_tap_fixture();r.memory.0.borrow_mut()[address..address+bytes.len()].copy_from_slice(&bytes);
            assert!(zero_extent_base_tap_contract(&mut r,128,256,384,4030,"IndicatorMotionBase","IndicatorPackageEasy_Yuan",&mut Vec::new()).is_err());
        }
        // Config kind cannot be faked merely by keeping readable pointer bytes.
        let mut r=base_tap_fixture();
        let table=u64_at(&r.memory.0.borrow()[4096+0x78..4096+0x80],0)as usize;
        let ty=u64_at(&r.memory.0.borrow()[table+64+8..table+64+16],0)as usize;
        r.memory.0.borrow_mut()[ty+8..ty+12].copy_from_slice(&(8u32<<16).to_le_bytes());
        assert!(zero_extent_base_tap_contract(&mut r,128,256,384,4030,"IndicatorMotionBase","IndicatorPackageEasy_Yuan",&mut Vec::new()).is_err());
    }
    fn additional_direction_fixture()->Reflection<MutableMemory> {
        let r=base_tap_fixture();
        for(at,value)in [(1056,12710i32),(1060,2)] {r.memory.0.borrow_mut()[at..at+4].copy_from_slice(&value.to_le_bytes());}
        for(at,value)in [(1568,11f32),(1572,4.1)] {r.memory.0.borrow_mut()[at..at+4].copy_from_slice(&value.to_le_bytes());}
        r
    }
    #[test]
    fn additional_direction_contract_owns_typed_config_and_revokes_rebinding() {
        let m="IndicatorMotionRot";let p="IndicatorPackage_TwoPartAndPos_Direction";
        let mut r=additional_direction_fixture();let mut reads=Vec::new();
        assert!(additional_direction_contract(&mut r,128,12710,m,p,&mut reads).unwrap());
        assert!(check(&r,&reads).is_ok());
        r.memory.0.borrow_mut()[1568..1572].copy_from_slice(&7f32.to_le_bytes());
        assert!(check(&r,&reads).is_err(),"a pooled plan may not keep its previous shape authorization");
        for(at,bytes)in [
            (160,0u64.to_le_bytes().to_vec()),
            (1024,5120u64.to_le_bytes().to_vec()),
            (1056,999i32.to_le_bytes().to_vec()),
            (1060,11i32.to_le_bytes().to_vec()),
            (1064,0u64.to_le_bytes().to_vec()),
            (1536,5120u64.to_le_bytes().to_vec()),
            (1560,1u64.to_le_bytes().to_vec()),
            (1568,3f32.to_le_bytes().to_vec()),
            (1572,0f32.to_le_bytes().to_vec()),
            (1572,(-1f32).to_le_bytes().to_vec()),
            (1572,f32::NAN.to_le_bytes().to_vec()),
        ] {
            let mut r=additional_direction_fixture();r.memory.0.borrow_mut()[at..at+bytes.len()].copy_from_slice(&bytes);
            assert!(additional_direction_contract(&mut r,128,12710,m,p,&mut Vec::new()).is_err(),"case at {at}");
        }
        for(class,field)in [(3072,0usize),(4096,0),(4096,1),(4096,2)] {
            let mut r=additional_direction_fixture();
            let table=u64_at(&r.memory.0.borrow()[class+0x78..class+0x80],0)as usize;
            let ty=u64_at(&r.memory.0.borrow()[table+field*32+8..table+field*32+16],0)as usize;
            r.memory.0.borrow_mut()[ty+8..ty+12].copy_from_slice(&(12u32<<16).to_le_bytes());
            assert!(additional_direction_contract(&mut r,128,12710,m,p,&mut Vec::new()).is_err());
        }
        for skill in [0i32,-1,10_000_001] {
            let mut r=additional_direction_fixture();r.memory.0.borrow_mut()[1056..1060].copy_from_slice(&skill.to_le_bytes());
            assert!(additional_direction_contract(&mut r,128,skill,m,p,&mut Vec::new()).is_err());
        }
        for(package,rect,shape)in [("IndicatorPackage_Multi_JianTou",0i32,7f32),("IndicatorPackage_TwoPart_Direction",19,6.)] {
            let mut r=additional_direction_fixture();
            r.memory.0.borrow_mut()[1060..1064].copy_from_slice(&rect.to_le_bytes());
            r.memory.0.borrow_mut()[1568..1572].copy_from_slice(&shape.to_le_bytes());
            assert!(additional_direction_contract(&mut r,128,12710,m,package,&mut Vec::new()).unwrap());
        }
    }
    #[test]
    fn verified_additional_direction_uses_unit_receipt_independent_of_ratio() {
        let(r,mut plan)=q_fixture();plan.skill=12710;plan.name="IndicatorMotionRot".into();
        plan.direction_geometry=true;plan.range=None;plan.protect=None;
        for(at,value)in [(528,0.6f32),(536,0.8),(576,0.34)] {r.memory.0.borrow_mut()[at..at+4].copy_from_slice(&value.to_le_bytes());}
        let mut frame=s::Frame::default();plan.sample(&r,&mut frame).unwrap();
        assert!(frame.direction_input.ready.get()&&frame.direction_input.coherent.get());
        assert!(!frame.logical.ready.get());assert!(frame.logical.world_range.missing());
        assert!((frame.direction_input.direction_xz.values[0].value-0.6).abs()<1e-6);
        plan.unavailable(&mut frame);assert!(!frame.direction_input.ready.get());assert!(frame.logical.source.missing());
        r.memory.0.borrow_mut()[528..532].copy_from_slice(&0.3f32.to_le_bytes());
        assert!(plan.sample(&r,&mut s::Frame::default()).is_err(),"direction authorization does not normalize a non-unit native result into an ACK");
    }
    fn candidate_range_fixture()->Reflection<MutableMemory> {
        let r=base_tap_fixture();
        for(at,value)in [(1056,12230i32),(1060,1)] {r.memory.0.borrow_mut()[at..at+4].copy_from_slice(&value.to_le_bytes());}
        for(at,value)in [(1568,9f32),(1572,3.)] {r.memory.0.borrow_mut()[at..at+4].copy_from_slice(&value.to_le_bytes());}
        r
    }
    #[test]
    fn candidate_range_audit_requires_exact_family_rect_and_all_typed_config_owners() {
        let m="IndicatorMotionRotAndPosRot";let p="IndicatorPackage_JianTou_OtherPart";
        let mut r=candidate_range_fixture();let mut reads=Vec::new();
        assert_eq!(candidate_range_contract(&mut r,128,256,384,12230,m,p,&mut reads).unwrap(),Some((1568,9.)));
        assert!(check(&r,&reads).is_ok());
        assert_eq!(native_range_shape(m,p),Some(9.));
        assert_eq!(native_range_contract(&mut r,128,256,384,12230,m,p,&mut Vec::new()).unwrap(),Some((1568,9.)));
        r.memory.0.borrow_mut()[288..296].copy_from_slice(&1280u64.to_le_bytes());
        assert!(check(&r,&reads).is_err());
        for(at,bytes)in [
            (160,1280u64.to_le_bytes().to_vec()),
            (288,1280u64.to_le_bytes().to_vec()),
            (416,1280u64.to_le_bytes().to_vec()),
            (1056,999i32.to_le_bytes().to_vec()),
            (1060,2i32.to_le_bytes().to_vec()),
            (1568,8f32.to_le_bytes().to_vec()),
            (1572,0f32.to_le_bytes().to_vec()),
            (1572,f32::INFINITY.to_le_bytes().to_vec()),
        ] {
            let mut r=candidate_range_fixture();r.memory.0.borrow_mut()[at..at+bytes.len()].copy_from_slice(&bytes);
            assert!(candidate_range_contract(&mut r,128,256,384,12230,m,p,&mut Vec::new()).is_err(),"case at {at}");
            assert!(native_range_contract(&mut r,128,256,384,12230,m,p,&mut Vec::new()).is_err(),"live range case at {at}");
        }
        let mut r=candidate_range_fixture();
        let table=u64_at(&r.memory.0.borrow()[3072+0x78..3072+0x80],0)as usize;
        let ty=u64_at(&r.memory.0.borrow()[table+8..table+16],0)as usize;
        r.memory.0.borrow_mut()[ty+8..ty+12].copy_from_slice(&(18u32<<16).to_le_bytes());
        assert!(candidate_range_contract(&mut r,128,256,384,12230,m,p,&mut Vec::new()).is_err());
        assert!(!additional_direction_template(m,p,1,&[9.,3.]));
    }

    fn selection_touch_fixture()->Reflection<MutableMemory> {
        fn put64(bytes:&mut[u8],at:usize,value:u64){bytes[at..at+8].copy_from_slice(&value.to_le_bytes());}
        fn class(bytes:&mut[u8],address:usize,name:&str,fields:&[(&str,u8,u32)],cursor:&mut usize){
            let name_at=*cursor;bytes[name_at..name_at+name.len()].copy_from_slice(name.as_bytes());*cursor+=name.len()+1;
            put64(bytes,address+16,name_at as u64);put64(bytes,address+0x40,address as u64);put64(bytes,address+0x48,address as u64);
            let table=*cursor;*cursor+=fields.len()*32;put64(bytes,address+0x78,table as u64);
            bytes[address+0x104..address+0x106].copy_from_slice(&(fields.len()as u16).to_le_bytes());
            for(i,(name,kind,offset))in fields.iter().enumerate(){
                let name_at=*cursor;bytes[name_at..name_at+name.len()].copy_from_slice(name.as_bytes());*cursor+=name.len()+1;
                let ty=*cursor;*cursor+=16;bytes[ty+8..ty+12].copy_from_slice(&((*kind as u32)<<16).to_le_bytes());
                let row=table+i*32;put64(bytes,row,name_at as u64);put64(bytes,row+8,ty as u64);put64(bytes,row+16,address as u64);
                bytes[row+24..row+28].copy_from_slice(&offset.to_le_bytes());
            }
        }
        let mut bytes=vec![0u8;65536];let mut cursor=32768;
        class(&mut bytes,8192,"SkillIndicatorComp",&[("m_BaseCamp",18,16)],&mut cursor);
        class(&mut bytes,8704,"Dictionary`2",&[("entries",29,16),("count",8,24)],&mut cursor);
        class(&mut bytes,9216,"Entry[]",&[],&mut cursor);
        class(&mut bytes,9728,"Entry",&[("hashCode",8,16),("next",8,20),("key",8,24),("value",18,32)],&mut cursor);
        class(&mut bytes,10240,"IndicatorBaseComp",&[("m_Config",17,16),("m_MontionBase",18,24),("m_PackageBase",18,32)],&mut cursor);
        class(&mut bytes,10752,"IndicatorMotionPos",&[("m_OwnComp",18,16),("m_OwnPackage",18,24),("m_vDragTouchPos",17,48)],&mut cursor);
        class(&mut bytes,11264,"IndicatorPackageEasy_Yuan",&[("m_OwnComp",18,16)],&mut cursor);
        class(&mut bytes,11776,"CData_Skill_Element",&[("m_SkillID",8,16),("m_RectType",8,20),("m_ShowRectPara",29,24)],&mut cursor);
        class(&mut bytes,12288,"Single[]",&[],&mut cursor);
        class(&mut bytes,12800,"Single",&[],&mut cursor);
        for(at,value)in [(128,8192u64),(144,256),(256,8704),(272,512),(512,9216),(536,1),(9216+0x40,9728),
                         (560,1024),(1024,10240),(1040,2560),(1048,1536),(1056,2048),(1536,10752),(1552,1024),(1560,2048),
                         (2048,11264),(2064,1024),(2560,11776),(2584,3072),(3072,12288),(3096,4),(12288+0x40,12800)] {
            put64(&mut bytes,at,value);
        }
        for(at,value)in [(280,1i32),(544,1),(548,-1),(552,13130),(2576,13130),(2580,11)]{bytes[at..at+4].copy_from_slice(&value.to_le_bytes());}
        for(at,value)in [(3104,3f32),(3108,0.),(3112,30000.),(3116,0.),(1584,3920.),(1588,1319.54),(1592,0.)] {
            bytes[at..at+4].copy_from_slice(&value.to_le_bytes());
        }
        Reflection::new(MutableMemory(RefCell::new(bytes))).unwrap()
    }
    fn base_tap_registry_fixture()->Reflection<MutableMemory> {
        fn put64(b:&mut[u8],at:usize,v:u64){b[at..at+8].copy_from_slice(&v.to_le_bytes());}
        fn class(b:&mut[u8],address:usize,name:&str,fields:&[(&str,u8,u32)],cursor:&mut usize){
            let name_at=*cursor;b[name_at..name_at+name.len()].copy_from_slice(name.as_bytes());*cursor+=name.len()+1;
            put64(b,address+16,name_at as u64);put64(b,address+0x40,address as u64);put64(b,address+0x48,address as u64);
            let table=*cursor;*cursor+=fields.len()*32;put64(b,address+0x78,table as u64);
            b[address+0x104..address+0x106].copy_from_slice(&(fields.len()as u16).to_le_bytes());
            for(i,(name,kind,offset))in fields.iter().enumerate(){
                let name_at=*cursor;b[name_at..name_at+name.len()].copy_from_slice(name.as_bytes());*cursor+=name.len()+1;
                let ty=*cursor;*cursor+=16;b[ty+8..ty+12].copy_from_slice(&((*kind as u32)<<16).to_le_bytes());
                let row=table+i*32;put64(b,row,name_at as u64);put64(b,row+8,ty as u64);put64(b,row+16,address as u64);
                b[row+24..row+28].copy_from_slice(&offset.to_le_bytes());
            }
        }
        let r=selection_touch_fixture();let mut cursor=50000;
        {
            let mut b=r.memory.0.borrow_mut();
            class(&mut b,10752,"IndicatorMotionBase",&[("m_OwnComp",18,16),("m_Config",17,32),("m_vDragTouchPos",17,48),
                ("m_vDragResult",17,64),("m_vDragDir",17,80),("m_vActiveDir",17,96),("m_fDragRatio",12,108),
                ("m_dragged",2,112),("m_bVaildDraged",2,113)],&mut cursor);
            class(&mut b,11264,"IndicatorPackageEasy_Yuan",&[("m_OwnComp",18,16),("m_Config",17,24)],&mut cursor);
            class(&mut b,11776,"CData_Skill_Element",&[("m_SkillID",8,16),("m_RectType",8,20),("m_ShowRectPara",29,24),("m_BaseType",8,32)],&mut cursor);
            put64(&mut b,1568,2560);put64(&mut b,2072,2560);
            for(at,value)in [(552,4030i32),(2576,4030),(2592,1)]{b[at..at+4].copy_from_slice(&value.to_le_bytes());}
        }
        r
    }
    fn purify_fixture() -> Reflection<MutableMemory> {
        let r=base_tap_registry_fixture();
        {
            let mut b=r.memory.0.borrow_mut();
            let at=u64_at(&b,11264+16) as usize;
            b[at..at+25].fill(0);
            b[at..at+20].copy_from_slice(b"IndicatorPackageBase");
            b[552..556].copy_from_slice(&20080i32.to_le_bytes());
            b[2576..2580].copy_from_slice(&20080i32.to_le_bytes());
        }
        r
    }
    #[test]
    fn inspire_and_regen_are_owner_checked_taps_without_an_aim_plan() {
        for skill in [20010i32,20030] {
            let mut r=purify_fixture();
            {let mut b=r.memory.0.borrow_mut();for at in [552,2576] {b[at..at+4].copy_from_slice(&skill.to_le_bytes());}}
            assert_eq!(MotionPlan::discover_behavior(&mut r,128,skill).unwrap().1,1);
            assert!(MotionPlan::discover(&mut r,128,skill).is_err());
            r.memory.0.borrow_mut()[2576..2580].copy_from_slice(&20020i32.to_le_bytes());
            assert!(MotionPlan::discover_behavior(&mut r,128,skill).is_err());
        }
    }
    #[test]
    fn purify_is_owner_checked_shared_tap_without_aim_plan() {
        let mut r=purify_fixture();
        assert_eq!(MotionPlan::discover_behavior(&mut r,128,20080).unwrap().1,1);
        assert!(MotionPlan::discover(&mut r,128,20080).is_err());
        for (at,bytes) in [(1552,2048u64.to_le_bytes().to_vec()),(2064,1536u64.to_le_bytes().to_vec()),(2576,20020i32.to_le_bytes().to_vec())] {
            let mut r=purify_fixture();r.memory.0.borrow_mut()[at..at+bytes.len()].copy_from_slice(&bytes);
            assert!(MotionPlan::discover_behavior(&mut r,128,20080).is_err());
        }
    }
    #[test]
    fn base_tap_readonly_audits_allow_missing_own_package_only_after_full_contract() {
        let mut r=base_tap_registry_fixture();
        assert!(r.field(1536,"m_OwnPackage").is_err());
        let identity=MotionPlan::audit_registry_identity(&mut r,128,4030).unwrap();
        assert_eq!(identity["motion"],"IndicatorMotionBase");assert_eq!(identity["package"],"IndicatorPackageEasy_Yuan");
        let values=MotionPlan::audit_registry_values(&mut r,128,4030).unwrap();
        assert_eq!(values["values"]["m_vDragTouchPos"][0],3920.);
        assert_eq!(values["values"]["m_ShowRectPara"][1],0.);
        // Diagnostic recognition cannot authorize a positional/directional
        // feedback plan for a Tap with no such native acknowledgement.
        assert!(MotionPlan::discover(&mut r,128,4030).is_err());
        assert_eq!(MotionPlan::discover_behavior(&mut r,128,4030).unwrap().1,1);
    }
    #[test]
    fn base_tap_audits_reject_nonzero_foreign_config_and_unqualified_missing_owner() {
        for(at,bytes)in [(1552,2048u64.to_le_bytes().to_vec()),(2064,1536u64.to_le_bytes().to_vec()),
                         (1568,2048u64.to_le_bytes().to_vec()),(2072,2048u64.to_le_bytes().to_vec()),
                         (2576,999i32.to_le_bytes().to_vec()),(2580,0i32.to_le_bytes().to_vec()),
                         (3104,8f32.to_le_bytes().to_vec()),(3108,1f32.to_le_bytes().to_vec()),(3108,f32::NAN.to_le_bytes().to_vec())] {
            let mut r=base_tap_registry_fixture();r.memory.0.borrow_mut()[at..at+bytes.len()].copy_from_slice(&bytes);
            assert!(MotionPlan::audit_registry_identity(&mut r,128,4030).is_err(),"identity {at}");
            assert!(MotionPlan::audit_registry_values(&mut r,128,4030).is_err(),"values {at}");
        }
        // A different Base package does not receive the missing-owner exception.
        let mut r=base_tap_registry_fixture();let at=u64_at(&r.memory.0.borrow()[11264+16..11264+24],0)as usize;
        r.memory.0.borrow_mut()[at..at+25].fill(0);r.memory.0.borrow_mut()[at..at+20].copy_from_slice(b"IndicatorPackageBase");
        assert!(MotionPlan::audit_registry_identity(&mut r,128,4030).is_err());
        assert!(MotionPlan::audit_registry_values(&mut r,128,4030).is_err());
        // Pointer-looking bytes with a changed configuration kind remain invalid.
        let mut r=base_tap_registry_fixture();let table=u64_at(&r.memory.0.borrow()[10752+0x78..10752+0x80],0)as usize;
        let ty=u64_at(&r.memory.0.borrow()[table+32+8..table+32+16],0)as usize;
        r.memory.0.borrow_mut()[ty+8..ty+12].copy_from_slice(&(18u32<<16).to_le_bytes());
        assert!(MotionPlan::audit_registry_identity(&mut r,128,4030).is_err());
        assert!(MotionPlan::audit_registry_values(&mut r,128,4030).is_err());
    }
    #[test]
    fn base_tap_audit_present_own_package_is_checked_instead_of_exempted() {
        for(value,kind,accepted)in [(2048u64,18u32,true),(1024,18,false),(2048,8,false)] {
            let mut r=base_tap_registry_fixture();
            {
                let mut b=r.memory.0.borrow_mut();let old=u64_at(&b[10752+0x78..10752+0x80],0)as usize;
                let count=u16::from_le_bytes(b[10752+0x104..10752+0x106].try_into().unwrap())as usize;
                let table=b[old..old+count*32].to_vec();b[60000..60000+table.len()].copy_from_slice(&table);
                b[65000..65013].copy_from_slice(b"m_OwnPackage\0");b[65108..65112].copy_from_slice(&(kind<<16).to_le_bytes());
                let row=60000+table.len();
                for(at,pointer)in [(row,65000u64),(row+8,65100),(row+16,10752),(10752+0x78,60000),(1560,value)] {
                    b[at..at+8].copy_from_slice(&pointer.to_le_bytes());
                }
                b[row+24..row+28].copy_from_slice(&24u32.to_le_bytes());
                b[10752+0x104..10752+0x106].copy_from_slice(&((count+1)as u16).to_le_bytes());
            }
            assert_eq!(MotionPlan::audit_registry_identity(&mut r,128,4030).is_ok(),accepted);
            assert_eq!(MotionPlan::audit_registry_values(&mut r,128,4030).is_ok(),accepted);
        }
    }
    #[test]
    fn selection_touch_reads_only_owned_raw_pixels_and_keeps_outer_fence() {
        let mut r=selection_touch_fixture();let plan=SelectionTouchPlan::discover(&mut r,128,13130).unwrap();
        assert_eq!(plan.skill,13130);assert!(plan.validate(&r).is_ok());
        let mut reads=Vec::new();let touch=plan.sample(&r,&mut reads).unwrap();
        assert_eq!(touch[0],3920.);assert!((touch[1]-1319.54).abs()<0.001);assert_eq!(touch[2],0.);
        assert!(check(&r,&reads).is_ok());
        // A later input value revokes the enclosing UI/selection receipt even
        // though the native object and its plan remain correctly owned.
        r.memory.0.borrow_mut()[1584..1588].copy_from_slice(&3000f32.to_le_bytes());
        assert!(check(&r,&reads).is_err());assert!(plan.validate(&r).is_ok());
        // Idle zeros remain raw data; no direction or result packet is produced.
        r.memory.0.borrow_mut()[1584..1596].fill(0);
        assert_eq!(plan.sample(&r,&mut Vec::new()).unwrap(),[0.;3]);
    }
    #[test]
    fn selection_touch_rejects_missing_foreign_types_config_and_nonfinite_values() {
        for skill in [0,-1,10_000_001,13131] {
            assert!(SelectionTouchPlan::discover(&mut selection_touch_fixture(),128,skill).is_err());
        }
        for(at,value)in [(552,999i32),(2576,999),(2580,0)] {
            let mut r=selection_touch_fixture();r.memory.0.borrow_mut()[at..at+4].copy_from_slice(&value.to_le_bytes());
            assert!(SelectionTouchPlan::discover(&mut r,128,13130).is_err());
        }
        for(at,value)in [(560,0u64),(1040,0),(1048,2048),(1056,1536),(1552,2048),(1560,1536),(2064,1536)] {
            let mut r=selection_touch_fixture();r.memory.0.borrow_mut()[at..at+8].copy_from_slice(&value.to_le_bytes());
            assert!(SelectionTouchPlan::discover(&mut r,128,13130).is_err(),"{at}");
        }
        for(at,value)in [(3104,8f32),(3108,1.),(3108,f32::NAN)] {
            let mut r=selection_touch_fixture();r.memory.0.borrow_mut()[at..at+4].copy_from_slice(&value.to_le_bytes());
            assert!(SelectionTouchPlan::discover(&mut r,128,13130).is_err());
        }
        for(kind,static_field)in [(8u32,false),(17,true)] {
            let mut r=selection_touch_fixture();let table=u64_at(&r.memory.0.borrow()[10752+0x78..10752+0x80],0)as usize;
            let ty=u64_at(&r.memory.0.borrow()[table+64+8..table+64+16],0)as usize;
            r.memory.0.borrow_mut()[ty+8..ty+12].copy_from_slice(&((kind<<16)|if static_field{16}else{0}).to_le_bytes());
            assert!(SelectionTouchPlan::discover(&mut r,128,13130).is_err());
        }
        for offset in [4090u32,1_048_577] {
            let mut r=selection_touch_fixture();let table=u64_at(&r.memory.0.borrow()[10752+0x78..10752+0x80],0)as usize;
            r.memory.0.borrow_mut()[table+64+24..table+64+28].copy_from_slice(&offset.to_le_bytes());
            assert!(SelectionTouchPlan::discover(&mut r,128,13130).is_err());
        }
        let mut r=selection_touch_fixture();r.memory.0.borrow_mut()[10752+0x104..10752+0x106].copy_from_slice(&2u16.to_le_bytes());
        assert!(SelectionTouchPlan::discover(&mut r,128,13130).is_err());
        for value in [f32::NAN,f32::INFINITY,1_000_001.] {
            let mut r=selection_touch_fixture();let plan=SelectionTouchPlan::discover(&mut r,128,13130).unwrap();
            r.memory.0.borrow_mut()[1584..1588].copy_from_slice(&value.to_le_bytes());assert!(plan.sample(&r,&mut Vec::new()).is_err());
        }
        assert!(!selection_touch_template("IndicatorMotionRot","IndicatorPackageEasy_Yuan",11,&[3.,0.]));
        assert!(!selection_touch_template("IndicatorMotionPos","IndicatorPackage_TwoPartAndPos_Direction",11,&[3.,0.]));
    }
    #[test]
    fn selection_touch_cached_registry_and_config_owners_revoke_after_rebind() {
        for(at,value)in [(144,384u64),(272,768),(560,2048),(1040,2304),(1048,2048),(1056,1536),
                         (1536,11264),(1552,2048),(1560,1536),(2048,10752),(2064,1536),(2584,3328),(3072,9216)] {
            let mut r=selection_touch_fixture();let plan=SelectionTouchPlan::discover(&mut r,128,13130).unwrap();let mut reads=Vec::new();
            plan.sample(&r,&mut reads).unwrap();r.memory.0.borrow_mut()[at..at+8].copy_from_slice(&value.to_le_bytes());
            assert!(plan.validate(&r).is_err(),"{at}");assert!(plan.sample(&r,&mut Vec::new()).is_err(),"{at}");assert!(check(&r,&reads).is_err(),"{at}");
        }
        for(at,value)in [(552,13131i32),(2576,13131),(2580,0)] {
            let mut r=selection_touch_fixture();let plan=SelectionTouchPlan::discover(&mut r,128,13130).unwrap();
            r.memory.0.borrow_mut()[at..at+4].copy_from_slice(&value.to_le_bytes());assert!(plan.validate(&r).is_err());
        }
    }
    fn cover_fixture()->(Reflection<MutableMemory>,MotionPlan) {
        fn put64(bytes:&mut[u8],at:usize,value:u64){bytes[at..at+8].copy_from_slice(&value.to_le_bytes());}
        fn class(bytes:&mut[u8],address:usize,name:&str,fields:&[(&str,u8,u32)],cursor:&mut usize){
            let name_at=*cursor;bytes[name_at..name_at+name.len()].copy_from_slice(name.as_bytes());*cursor+=name.len()+1;
            put64(bytes,address+16,name_at as u64);put64(bytes,address+0x40,address as u64);put64(bytes,address+0x48,address as u64);
            let table=*cursor;*cursor+=fields.len()*32;put64(bytes,address+0x78,table as u64);
            bytes[address+0x104..address+0x106].copy_from_slice(&(fields.len()as u16).to_le_bytes());
            for(i,(name,kind,offset))in fields.iter().enumerate(){
                let name_at=*cursor;bytes[name_at..name_at+name.len()].copy_from_slice(name.as_bytes());*cursor+=name.len()+1;
                let ty=*cursor;*cursor+=16;bytes[ty+8..ty+12].copy_from_slice(&((*kind as u32)<<16).to_le_bytes());
                let row=table+i*32;put64(bytes,row,name_at as u64);put64(bytes,row+8,ty as u64);put64(bytes,row+16,address as u64);
                bytes[row+24..row+28].copy_from_slice(&offset.to_le_bytes());
            }
        }
        let mut bytes=vec![0u8;32768];let mut cursor=8192;
        class(&mut bytes,4096,"IndicatorMotionRot",&[("lCoverSkills",21,256)],&mut cursor);
        class(&mut bytes,5120,"List`1",&[("_items",29,32),("_size",8,40)],&mut cursor);
        class(&mut bytes,6144,"Int32",&[],&mut cursor);
        class(&mut bytes,6656,"Int32[]",&[],&mut cursor);put64(&mut bytes,6656+0x40,6144);
        put64(&mut bytes,512,4096);put64(&mut bytes,768,1536);
        put64(&mut bytes,1536,5120);put64(&mut bytes,1568,2048);bytes[1576..1580].copy_from_slice(&3i32.to_le_bytes());
        put64(&mut bytes,2048,6656);put64(&mut bytes,2072,4);
        for(at,value)in [(2080,12652i32),(2084,12653),(2088,12653)]{bytes[at..at+4].copy_from_slice(&value.to_le_bytes());}
        let (_,mut plan)=q_fixture();plan.skill=12650;plan.name="IndicatorMotionRot".into();plan.direction_geometry=true;
        plan.checks=vec![(512,4096u64.to_le_bytes().to_vec())];
        (Reflection::new(MutableMemory(RefCell::new(bytes))).unwrap(),plan)
    }
    #[test]
    fn covered_direction_requires_current_typed_membership_and_is_not_cached() {
        let (mut r,plan)=cover_fixture();let mut reads=Vec::new();
        assert!(plan.covers_direction_skill(&mut r,12652,&mut reads).unwrap());
        assert!(!plan.covers_direction_skill(&mut r,12658,&mut Vec::new()).unwrap());
        assert!(check(&r,&reads).is_ok());
        r.memory.0.borrow_mut()[2080..2084].copy_from_slice(&12654i32.to_le_bytes());
        assert!(check(&r,&reads).is_err());
        assert!(!plan.covers_direction_skill(&mut r,12652,&mut Vec::new()).unwrap());
        assert!(plan.covers_direction_skill(&mut r,12654,&mut Vec::new()).unwrap());
        let (mut r,mut plan)=cover_fixture();plan.direction_geometry=false;
        assert!(!plan.covers_direction_skill(&mut r,12652,&mut Vec::new()).unwrap());
    }
    #[test]
    fn covered_direction_rejects_wrong_types_bounds_owner_rebind_and_invalid_ids() {
        for (at,value)in [(1576,-1i32),(1576,129),(2080,0),(2080,-1),(2080,10_000_001)] {
            let(mut r,plan)=cover_fixture();r.memory.0.borrow_mut()[at..at+4].copy_from_slice(&value.to_le_bytes());
            assert!(plan.covers_direction_skill(&mut r,12652,&mut Vec::new()).is_err());
        }
        for (at,value)in [(768,0u64),(1568,0),(2072,2),(2072,4097),(512,5120),(1536,4096),(6656+0x40,4096)] {
            let(mut r,plan)=cover_fixture();r.memory.0.borrow_mut()[at..at+8].copy_from_slice(&value.to_le_bytes());
            let result=plan.covers_direction_skill(&mut r,12652,&mut Vec::new());
            if at==768{assert_eq!(result.unwrap(),false);}else{assert!(result.is_err(),"{at}");}
        }
        // The same readable pointers cannot qualify a field with the wrong
        // metadata kind or a static field masquerading as an instance member.
        for (class,field_index,attrs)in [(4096,0,18u32<<16),(4096,0,(21u32<<16)|16),
                                       (5120,0,18u32<<16),(5120,1,12u32<<16)] {
            let(mut r,plan)=cover_fixture();
            let table=u64_at(&r.memory.0.borrow()[class+0x78..class+0x80],0)as usize;
            let row=table+field_index*32;let ty=u64_at(&r.memory.0.borrow()[row+8..row+16],0)as usize;
            r.memory.0.borrow_mut()[ty+8..ty+12].copy_from_slice(&attrs.to_le_bytes());
            assert!(plan.covers_direction_skill(&mut r,12652,&mut Vec::new()).is_err());
        }
        for actual in [0,-1,10_000_001] {
            let(mut r,plan)=cover_fixture();assert!(plan.covers_direction_skill(&mut r,actual,&mut Vec::new()).is_err());
        }
        // All consumed owner/list/array links remain in the caller's outer fence.
        for (at,value)in [(768,1664u64),(1536,4096),(1568,2304),(2048,4096),(6656+0x40,4096)] {
            let(mut r,plan)=cover_fixture();let mut reads=Vec::new();
            assert!(plan.covers_direction_skill(&mut r,12652,&mut reads).unwrap());
            r.memory.0.borrow_mut()[at..at+8].copy_from_slice(&value.to_le_bytes());
            assert!(check(&r,&reads).is_err(),"{at}");
        }
    }
    #[test]
    fn direction_geometry_flag_comes_from_verified_contract_not_motion_name() {
        assert!(verified_direction_geometry(2520,"IndicatorMotionRot","IndicatorPackageEasy_JianTou",false));
        assert!(verified_direction_geometry(12650,"IndicatorMotionRot","IndicatorPackage_JianTou_OtherFixedPart",true));
        assert!(verified_direction_geometry(8430,"IndicatorMotionRot","IndicatorPackageEasy",false));
        assert!(!verified_direction_geometry(2530,"IndicatorMotionRot","IndicatorPackageEasy",false));
        assert!(!verified_direction_geometry(4030,"IndicatorMotionBase","IndicatorPackageEasy_Yuan",false));
        assert!(!verified_direction_geometry(11810,"IndicatorMotionPos","IndicatorPackageEasy_Yuan",false));
        assert!(!verified_direction_geometry(12650,"IndicatorMotionRot","Unknown",false));
        let(_,mut plan)=q_fixture();assert!(!plan.is_direction_geometry());plan.direction_geometry=true;assert!(plan.is_direction_geometry());
    }

    #[test]
    fn movement_driven_click_requires_verified_native_config() {
        let m="IndicatorMotionRot";let p="IndicatorPackageEasy_JianTou_DrivenByJoystick";
        assert!(movement_driven_template(m,p,11,&[27.,3.]));
        assert!(movement_driven_template(m,p,11,&[27.,2.5]));
        for params in [&[2.,3.][..],&[27.,0.][..],&[27.,-1.][..],&[27.,f64::NAN][..],&[27.,f64::INFINITY][..],&[27.][..]] {
            assert!(!movement_driven_template(m,p,11,params));
        }
        assert!(!movement_driven_template("Unknown",p,11,&[27.,3.]));
        assert!(!movement_driven_template(m,"Unknown",11,&[27.,3.]));
        assert!(!movement_driven_template(m,p,2,&[27.,3.]));
    }

    #[test]
    fn fixed_part_direction_requires_exact_pair_and_native_shape() {
        let m="IndicatorMotionRot";let p="IndicatorPackage_JianTou_OtherFixedPart";
        assert!(fixed_part_direction_template(m,p,11,&[15.,3.4]));
        assert!(fixed_part_direction_template(m,p,0,&[15.,3.8]));
        assert!(fixed_part_direction_template(m,p,11,&[15.,7.5,12658.]));
        for params in [&[2.,3.4][..],&[15.,0.][..],&[15.,-1.][..],&[15.,1e4][..],
                       &[15.,f64::NAN][..],&[15.,f64::INFINITY][..],&[f64::NAN,3.4][..],&[15.][..],&[][..]] {
            for rect in [0,11] {assert!(!fixed_part_direction_template(m,p,rect,params));}
        }
        assert!(!fixed_part_direction_template("IndicatorMotionPos",p,11,&[15.,3.4]));
        assert!(!fixed_part_direction_template("Unknown",p,11,&[15.,3.4]));
        assert!(!fixed_part_direction_template(m,"IndicatorPackageEasy_JianTou",11,&[15.,3.4]));
        for rect in [-1,1,2,10,12] {assert!(!fixed_part_direction_template(m,p,rect,&[15.,3.4]));}
        // No broad Rot/package allowlist entry: both discovery paths must go
        // through the same typed, owner-checked configuration contract.
        assert_eq!(native_template(m,p),None);
    }

}
