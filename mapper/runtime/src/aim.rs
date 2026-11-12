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
        ("IndicatorMotionRotAndPosRot" | "IndicatorMotionFaTiaoQiu2","IndicatorPackage_JianTou_Yuan") => Some(3),
        _=>None,
    }
}
fn zero_extent_template(motion:&str,package:&str,rect:i32,params:&[f64])->bool {
    motion=="IndicatorMotionPos" && package=="IndicatorPackage_TwoPartAndPos_Direction"
        && rect==11 && params.len()>=2 && params[0]==11. && params[1]==0.
}
fn movement_driven_template(motion:&str,package:&str,rect:i32,params:&[f64])->bool {
    motion=="IndicatorMotionRot" && package=="IndicatorPackageEasy_JianTou_DrivenByJoystick"
        && rect==11 && params.len()>=2 && params[0]==27.
        && params[1].is_finite() && params[1]>0. && params[1]<1e4
}
fn fixed_part_direction_template(motion:&str,package:&str,rect:i32,params:&[f64])->bool {
    motion=="IndicatorMotionRot" && package=="IndicatorPackage_JianTou_OtherFixedPart"
        && rect==11 && params.len()>=2 && params[0]==15.
        && params[1].is_finite() && params[1]>0. && params[1]<1e4
}
fn native_range_shape(motion:&str,package:&str)->Option<f64> {
    match (motion,package) {
        ("IndicatorMotionRotAndPosRot"|"IndicatorMotionFaTiaoQiu2","IndicatorPackage_JianTou_Yuan")=>Some(8.),
        ("IndicatorMotionPos","IndicatorPackageEasy_Yuan")=>Some(3.),
        _=>None,
    }
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
    Ok(Some((address,shape)))
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
impl MotionPlan {
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
        for (object,field,expected) in [(motion,"m_OwnComp",comp),(motion,"m_OwnPackage",package),(package,"m_OwnComp",comp)] {
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
        if !["IndicatorMotionRot","IndicatorMotionPos","IndicatorMotionRotAndPosRot","IndicatorMotionFaTiaoQiu2","IndicatorMotionArcSurface"].contains(&mc.name.as_str()) {return Err("Unverified diagnostic motion layout".into());}
        for (object,field,expected) in [(motion,"m_OwnComp",comp),(motion,"m_OwnPackage",package),(package,"m_OwnComp",comp)] {
            if owner(r,object,field,&mut checks)?!=expected {return Err("Audit motion owner mismatch".into());}
        }
        let mut values=serde_json::Map::new();
        for name in ["m_vDragResult","m_vDragTouchPos","m_vDragDir","m_vActiveDir"] {
            let at=r.field(motion,name)?;
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
            let at=r.field(motion,name)?;
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
        let kind=if let Some(kind)=native_template(&mc.name,&pc.name) {
            if kind==3 {native_range_contract(r,comp,motion,package,skill,&mc.name,&pc.name,&mut checks)?.ok_or("Unverified native range family")?;}
            kind
        }
        else if fixed_part_direction_contract(r,comp,skill,&mc.name,&pc.name,&mut checks)? {2}
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
            if owner(r,object,field,&mut checks)?!=expected {return Err("Native template owner mismatch".into());}
        }
        check(r,&checks)?;Ok((checks,kind))
    }
    pub fn discover<M: Memory>(r: &mut Reflection<M>, indicator: u64, skill: i32) -> Result<Self> {
        let (mut checks,comp,motion,package,motion_class,package_class)=Self::registry_owner(r,indicator,skill)?;
        let name=motion_class.name.clone();
        let range_contract=native_range_contract(r,comp,motion,package,skill,&name,&package_class.name,&mut checks)?;
        let fixed_part_direction=fixed_part_direction_contract(r,comp,skill,&name,&package_class.name,&mut checks)?;
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
            _ if fixed_part_direction || matches!(native_template(&name,&package_class.name),Some(2 | 3)) => Some((name.as_str(),package_class.name.as_str())),
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
        if range_contract.is_some() {
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
        frame.logical = if !matches!(self.name.as_str(), "IndicatorMotionRot" | "IndicatorLieShaMotion" | "IndicatorMotionArcSurface") {
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
        if matches!(self.name.as_str(), "IndicatorMotionRot" | "IndicatorLieShaMotion" | "IndicatorMotionArcSurface") {
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
        plan.range=None;plan.protect=None;
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
        assert!(fixed_part_direction_template(m,p,11,&[15.,7.5,12658.]));
        for params in [&[2.,3.4][..],&[15.,0.][..],&[15.,-1.][..],&[15.,1e4][..],
                       &[15.,f64::NAN][..],&[15.,f64::INFINITY][..],&[f64::NAN,3.4][..],&[15.][..],&[][..]] {
            assert!(!fixed_part_direction_template(m,p,11,params));
        }
        assert!(!fixed_part_direction_template("IndicatorMotionPos",p,11,&[15.,3.4]));
        assert!(!fixed_part_direction_template("Unknown",p,11,&[15.,3.4]));
        assert!(!fixed_part_direction_template(m,"IndicatorPackageEasy_JianTou",11,&[15.,3.4]));
        assert!(!fixed_part_direction_template(m,p,0,&[15.,3.4]));
        // No broad Rot/package allowlist entry: both discovery paths must go
        // through the same typed, owner-checked configuration contract.
        assert_eq!(native_template(m,p),None);
    }

}
