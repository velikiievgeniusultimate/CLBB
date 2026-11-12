// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded read-only inventory of one owned runtime's current hero contracts.
//! This diagnostic never authorizes a production template or emits input.
use clbb_mapper::{
    aim::MotionPlan,
    platform::{Memory, linux::{ProcessMemory, now_ns}},
    reflection::{Reflection, Result, f32_at, i32_at, u64_at},
    source::{Source, Checks, capture, check},
};
use serde_json::{Value, json};
use std::{collections::{BTreeMap, BTreeSet}, fs::{self, OpenOptions}, io::Write,
          path::{Path, PathBuf}, process::Command, time::{Duration, Instant}};

fn process_identity(pid: u32) -> Result<(u64, PathBuf)> {
    let proc = PathBuf::from(format!("/proc/{pid}"));
    let stat = fs::read_to_string(proc.join("stat")).map_err(|e| e.to_string())?;
    let fields: Vec<_> = stat.rsplit_once(')').ok_or("Invalid process stat")?.1.split_whitespace().collect();
    if matches!(fields.first().copied(), Some("Z" | "X" | "x")) {
        return Err("Owned runtime has exited".into());
    }
    let birth = fields.get(19).ok_or("Missing process birth")?.parse().map_err(|_| "Invalid process birth")?;
    let executable = fs::read_link(proc.join("exe")).map_err(|e| e.to_string())?;
    Ok((birth, executable))
}

fn verify_selected_runtime(root: &Path, pid: u32) -> Result<()> {
    // Reuse the workbench's hash/manifest validation, rather than accepting a
    // same-named binary or duplicating the selected-runtime trust rules here.
    let status = Command::new("python3")
        .args(["-c", "import sys; from pathlib import Path; sys.path.insert(0,sys.argv[1]); from runtime_identity import runtime_executable_is_owned; sys.exit(0 if runtime_executable_is_owned(int(sys.argv[2]),Path(sys.argv[3])) else 1)"])
        .arg(root.join("tools")).arg(pid.to_string()).arg(root)
        .status().map_err(|e| e.to_string())?;
    if !status.success() { return Err("PID is not this checkout's owned/verified selected runtime".into()); }
    Ok(())
}

fn ptr<M: Memory>(r: &mut Reflection<M>, checks: &mut Checks, object: u64, name: &str) -> Result<u64> {
    let address = r.field(object, name)?;
    Ok(u64_at(&capture(r, checks, address, 8)?, 0))
}

fn primitive(kind: u8, raw: &[u8]) -> Result<Value> {
    Ok(match kind {
        2 => { if raw[0] > 1 { return Err("Invalid config boolean".into()); } json!(raw[0] != 0) },
        8 => json!(i32_at(raw, 0)),
        9 => json!(u32::from_le_bytes(raw.try_into().unwrap())),
        10 => json!(i64::from_le_bytes(raw.try_into().unwrap())),
        11 => json!(u64::from_le_bytes(raw.try_into().unwrap())),
        12 => { let v=f32_at(raw,0); if !v.is_finite() { return Err("Nonfinite config number".into()); } json!(v) },
        13 => { let v=f64::from_le_bytes(raw.try_into().unwrap()); if !v.is_finite() { return Err("Nonfinite config number".into()); } json!(v) },
        _ => return Err("Unsupported config primitive".into()),
    })
}

fn charge_cache<M: Memory>(r: &mut Reflection<M>, ui: u64) -> Result<Value> {
    r.discover(&["UIChargeSkillInfo"])?;
    let expected_class=r.named["UIChargeSkillInfo"];
    let info=r.class(expected_class)?;
    if info.fields.len()!=2 {return Err("Charge info field layout changed".into());}
    let charge=info.fields.get("m_bIsChargeSkill").ok_or("Missing charge flag")?;
    let effect=info.fields.get("m_iChargeEffectId").ok_or("Missing charge effect")?;
    if charge.is_static||charge.kind!=2||effect.is_static||effect.kind!=8
        ||charge.offset!=16||effect.offset!=20 {
        return Err("Charge info primitive types/layout changed".into());
    }
    let mut checks=Vec::new();
    let dictionary=ptr(r,&mut checks,ui,"m_dictCachedChargeSkillInfos")?;
    if dictionary==0 {return Ok(json!({"present":false,"entries":[],"note":"not populated is unknown, not non-charge"}));}
    let class=u64_at(&capture(r,&mut checks,dictionary,8)?,0);
    if r.class(class)?.name!="Dictionary`2" {return Err("Unexpected charge cache collection".into());}
    let count_at=r.field(dictionary,"count")?;
    let count=i32_at(&capture(r,&mut checks,count_at,4)?,0);
    if !(0..=256).contains(&count) {return Err("Charge cache count exceeds audit bound".into());}
    let array=ptr(r,&mut checks,dictionary,"entries")?;
    if array==0 {
        if count!=0 {return Err("Charge cache entries absent".into());}
        check(r,&checks)?;
        return Ok(json!({"present":true,"count":0,"entries":[],"note":"empty cache is unknown, not non-charge"}));
    }
    let header=capture(r,&mut checks,array,32)?;
    let capacity=u64_at(&header,24);
    if capacity<count as u64||capacity>1024 {return Err("Charge cache capacity exceeds audit bound".into());}
    let array_class=u64_at(&header,0);
    let element=u64_at(&capture(r,&mut checks,array_class+0x40,8)?,0);
    let entry=r.class(element)?;
    let expected=[("hashCode",8,16),("next",8,20),("key",8,24),("value",17,28)];
    if entry.fields.len()!=4||expected.iter().any(|(name,kind,offset)|entry.fields.get(*name).is_none_or(|f|f.kind!=*kind||f.is_static||f.offset!=*offset)) {
        return Err("Charge cache inline entry layout changed".into());
    }
    // Prove the generic entry's inline value is precisely UIChargeSkillInfo.
    // Il2CppClass.byval_arg is the same validated layout used by Reflection;
    // compare type data and kind, ignoring per-field attribute bits.
    let field_table=u64_at(&capture(r,&mut checks,element+0x78,8)?,0);
    let fields=capture(r,&mut checks,field_table,entry.fields.len()*32)?;
    let mut value_type=None;
    for field in fields.chunks_exact(32) {
        if u64_at(field,16)!=element {return Err("Charge cache field owner changed".into());}
        if r.cstring(u64_at(field,0))?=="value" {
            value_type=Some(capture(r,&mut checks,u64_at(field,8),16)?);
        }
    }
    let value_type=value_type.ok_or("Charge cache value type absent")?;
    let expected_type=capture(r,&mut checks,expected_class+32,16)?;
    if u64_at(&value_type,0)!=u64_at(&expected_type,0)
        ||((u32::from_le_bytes(value_type[8..12].try_into().unwrap())>>16)&255)!=17
        ||((u32::from_le_bytes(expected_type[8..12].try_into().unwrap())>>16)&255)!=17 {
        return Err("Charge cache value is not typed UIChargeSkillInfo".into());
    }
    // Three Int32 fields followed by the verified 8-byte inline value: 20 bytes.
    let raw=capture(r,&mut checks,array+32,count as usize*20)?;
    let mut rows=Vec::new();let mut seen=BTreeSet::new();
    for row in raw.chunks_exact(20) {
        if i32_at(row,0)<0 {continue;}
        let skill=i32_at(row,8);let charged=row[12];
        if !(1..=10_000_000).contains(&skill)||charged>1||!seen.insert(skill) {
            return Err("Invalid/duplicate charge cache row".into());
        }
        rows.push(json!({"skill":skill,"is_charge_skill":charged!=0,"charge_effect_id":i32_at(row,16)}));
    }
    check(r,&checks)?;
    Ok(json!({"present":true,"count":count,"entries":rows,"value_class":"UIChargeSkillInfo","value_type_verified":true,"coherent":true,"note":"absence of a skill is unknown, not non-charge"}))
}

fn charge_attack_controller<M:Memory>(r:&mut Reflection<M>,ui:u64)->Result<Value> {
    let mut checks=Vec::new();let object=ptr(r,&mut checks,ui,"m_KeySKillATK")?;
    if object==0{return Ok(Value::Null);}
    let class=u64_at(&capture(r,&mut checks,object,8)?,0);
    let mut current=r.class(class)?;let actual=current.name.clone();let mut fields=BTreeMap::new();let mut charge=false;
    for _ in 0..16 {
        if current.name=="CommonAttackPressCharge" {charge=true;}
        for (name,field)in current.fields {fields.entry(name).or_insert(field);}
        if current.parent==0 {break;}current=r.class(current.parent)?;
    }
    if !charge {check(r,&checks)?;return Ok(json!({"class":actual,"charge_controller":false}));}
    let mut values=serde_json::Map::new();
    for (name,field)in &fields {
        if field.is_static||field.offset>65536 {continue;}
        let size=match field.kind {2=>1,8|9|12=>4,10|11|13=>8,_=>continue};
        let raw=capture(r,&mut checks,object+field.offset,size)?;
        values.insert(name.clone(),json!({"kind":field.kind,"value":primitive(field.kind,&raw)?}));
        if values.len()>128 {return Err("Charge controller fields exceed bound".into());}
    }
    let mut skill_links=serde_json::Map::new();
    for name in ["keyDownSkillData","keyTimeChangeSkillData"] {
        let Some(field)=fields.get(name) else {continue};
        if field.kind!=18||field.is_static||field.offset>65536 {return Err("Charge controller skill link type changed".into());}
        let child=u64_at(&capture(r,&mut checks,object+field.offset,8)?,0);
        if child==0 {skill_links.insert(name.into(),Value::Null);continue;}
        let class=u64_at(&capture(r,&mut checks,child,8)?,0);let name_class=r.class(class)?.name;
        if name_class!="ShowSkillData" {return Err("Charge controller skill data class changed".into());}
        let mut row=serde_json::Map::new();
        for name in ["m_InitSkillID","m_TranID"] {
            let at=r.field(child,name)?;row.insert(name.into(),json!(i32_at(&capture(r,&mut checks,at,4)?,0)));
        }
        skill_links.insert(name.into(),json!({"class":name_class,"values":row}));
    }
    check(r,&checks)?;
    Ok(json!({"class":actual,"charge_controller":true,"numeric":values,"skill_links":skill_links,"coherent":true}))
}

fn button_charge_flags<M:Memory>(r:&mut Reflection<M>,ui:u64,local_show:u64)->Result<Value> {
    let mut checks=Vec::new();let list=ptr(r,&mut checks,ui,"statusKeySkillList")?;
    let header=capture(r,&mut checks,list+16,16)?;
    let array=u64_at(&header,0);let count=i32_at(&header,8);
    if !(0..=64).contains(&count)||array==0 {return Err("Charge button list outside audit bounds".into());}
    let raw=capture(r,&mut checks,array+32,count as usize*8)?;
    let mut objects:Vec<_>=raw.chunks_exact(8).map(|v|u64_at(v,0)).filter(|v|*v!=0).collect();
    let attack=ptr(r,&mut checks,ui,"m_KeySKillATK")?;
    if attack!=0&&!objects.contains(&attack){objects.push(attack);}
    let mut rows=Vec::new();let mut seen=BTreeSet::new();
    for object in objects {
        if !seen.insert(object){continue;}
        let class=u64_at(&capture(r,&mut checks,object,8)?,0);
        let class_name=r.class(class)?.name;
        let data=ptr(r,&mut checks,object,"m_SkillData")?;
        if data==0 {continue;}
        let local=clbb_mapper::source::skill_result_is_local(r,local_show,data,&mut checks)?;
        if !local {continue;}
        let data_class=r.class(r.pointer(data)?)?;
        let mut values=serde_json::Map::new();
        for (name,kind)in [("m_InitSkillID",8),("m_TranID",8),("m_bChargeSkill",2),("m_iCancelChargeSkill",8)] {
            let Some(field)=data_class.fields.get(name)else{continue};
            if field.is_static||field.kind!=kind||field.offset>65536 {return Err(format!("Charge skill field type changed: {name}"));}
            let bytes=capture(r,&mut checks,data+field.offset,if kind==2 {1}else{4})?;
            values.insert(name.into(),primitive(kind,&bytes)?);
        }
        let index_at=r.field(object,"m_iIndex")?;
        let index=i32_at(&capture(r,&mut checks,index_at,4)?,0);
        rows.push(json!({"index":index,"attack":object==attack,"button_class":class_name,"data_class":"ShowSkillData","local_skill_comp":true,"values":values}));
    }
    check(r,&checks)?;
    Ok(json!({"buttons":rows,"coherent":true,"source":"owned VirtualButton.m_SkillData -> ShowSkillData.m_bChargeSkill"}))
}

fn charge_lifecycle<M:Memory>(r:&mut Reflection<M>,local_show:u64)->Result<Value> {
    let target=*r.named.get("UIKeySkill").ok_or("UI controls class not owned by Source")?;
    let mut base=r.class(target)?;
    for _ in 0..16 {if base.name=="BaseFrame" {break;}if base.parent==0 {return Err("UI frame inheritance changed".into());}base=r.class(base.parent)?;}
    if base.name!="BaseFrame" {return Err("UI BaseFrame unavailable".into());}
    let list_field=base.fields.get("m_ActiveFrameList").ok_or("UI frame list absent")?;
    if !list_field.is_static||base.statics==0 {return Err("UI frame list is not a static owner".into());}
    let mut checks=Vec::new();let frames=u64_at(&capture(r,&mut checks,base.statics+list_field.offset,8)?,0);
    let header=capture(r,&mut checks,frames+16,16)?;
    let array=u64_at(&header,0);let count=i32_at(&header,8);
    if !(0..=256).contains(&count)||array==0 {return Err("UI frame list exceeds bounds".into());}
    let values=capture(r,&mut checks,array+32,count as usize*8)?;
    let controls:Vec<_>=values.chunks_exact(8).map(|v|u64_at(v,0)).filter(|v|*v!=0&&r.pointer(*v).ok()==Some(target)).collect();
    if controls.len()!=1 {return Err("UI controls absent or ambiguous".into());}
    let ui=controls[0];checks.push((ui,target.to_le_bytes().to_vec()));
    let attack=ptr(r,&mut checks,ui,"m_KeySKillATK")?;
    if attack==0 {return Err("UI attack control absent".into());}
    let data=ptr(r,&mut checks,attack,"m_SkillData")?;
    if data==0||!clbb_mapper::source::skill_result_is_local(r,local_show,data,&mut checks)? {
        return Err("UI charge skill data belongs to another local hero".into());
    }
    let cache=charge_cache(r,ui);
    let attack_controller=charge_attack_controller(r,ui);
    let button_flags=button_charge_flags(r,ui,local_show);
    check(r,&checks)?;
    Ok(json!({"cache":cache,"attack_controller":attack_controller,"button_flags":button_flags,"ui_owner_verified":true,"scope":"read-only owned UI charge cache; no lifecycle authorization"}))
}

/// Values only from the exact skill's own configuration. This is deliberately
/// independent of production MotionPlan's supported-motion allowlist: observing
/// an unknown configuration does not make that class a usable mapper template.
fn config_values<M: Memory>(r: &mut Reflection<M>, indicator: u64, skill: i32) -> Result<Value> {
    let mut checks=Vec::new();
    let dictionary=ptr(r,&mut checks,indicator,"m_BaseCamp")?;
    let array=ptr(r,&mut checks,dictionary,"entries")?;
    let at=r.field(dictionary,"count")?;
    let count=i32_at(&capture(r,&mut checks,at,4)?,0);
    if !(1..=256).contains(&count) { return Err("Registry count outside audit bounds".into()); }
    let header=capture(r,&mut checks,array,32)?;
    let class=u64_at(&header,0);let capacity=u64_at(&header,24);
    if capacity<count as u64 || capacity>4096 { return Err("Registry capacity outside audit bounds".into()); }
    let element=u64_at(&capture(r,&mut checks,class+0x40,8)?,0);
    let entry=r.class(element)?;
    let expected=[("hashCode",8),("next",8),("key",8),("value",18)];
    if entry.fields.len()!=4 || expected.iter().any(|(name,kind)|entry.fields.get(*name).is_none_or(|f|f.kind!=*kind||f.is_static)) {
        return Err("Unrecognized registry entry layout".into());
    }
    let bias=entry.fields.values().map(|f|f.offset).min().ok_or("Empty registry layout")?;
    let offsets:BTreeMap<_,_>=entry.fields.iter().map(|(name,f)|(name.as_str(),(f.offset-bias)as usize)).collect();
    let stride=offsets.iter().map(|(name,offset)|offset+if *name=="value"{8}else{4}).max().unwrap();
    if !(16..=64).contains(&stride)||stride%8!=0||offsets["hashCode"]!=0||offsets.values().collect::<BTreeSet<_>>().len()!=4 {
        return Err("Invalid registry entry stride".into());
    }
    let rows=capture(r,&mut checks,array+32,count as usize*stride)?;
    let hits:Vec<_>=rows.chunks_exact(stride).filter(|row|i32_at(row,offsets["hashCode"])>=0&&i32_at(row,offsets["key"])==skill).collect();
    if hits.len()!=1 { return Err("Skill registry entry absent or ambiguous".into()); }
    let comp=u64_at(hits[0],offsets["value"]);
    let motion=ptr(r,&mut checks,comp,"m_MontionBase")?;
    let package=ptr(r,&mut checks,comp,"m_PackageBase")?;
    if ptr(r,&mut checks,motion,"m_OwnComp")?!=comp
        ||ptr(r,&mut checks,motion,"m_OwnPackage")?!=package
        ||ptr(r,&mut checks,package,"m_OwnComp")?!=comp {
        return Err("Skill registry ownership changed".into());
    }
    let config=ptr(r,&mut checks,comp,"m_Config")?;
    let class=u64_at(&capture(r,&mut checks,config,8)?,0);
    let metadata=r.class(class)?;
    if metadata.name!="CData_Skill_Element" { return Err("Unknown configuration class".into()); }
    let id_at=r.field(config,"m_SkillID")?;
    if i32_at(&capture(r,&mut checks,id_at,4)?,0)!=skill { return Err("Configuration belongs to another skill".into()); }
    let mut fields:Vec<_>=metadata.fields.iter().filter_map(|(name,f)| {
        let size=match f.kind {2=>1,8|9|12=>4,10|11|13=>8,_=>return None};
        (!f.is_static && f.offset<65536).then_some((name,f,size))
    }).collect();
    fields.sort_by_key(|(name,_,_)|*name);
    if fields.len()>512 { return Err("Configuration fields exceed audit bound".into()); }
    let mut numeric=serde_json::Map::new();
    for (name,f,size)in fields {
        let raw=capture(r,&mut checks,config+f.offset,size)?;
        numeric.insert(name.clone(),json!({"kind":f.kind,"value":primitive(f.kind,&raw)?}));
    }
    let params=ptr(r,&mut checks,config,"m_ShowRectPara")?;
    let rect=if params==0 {Value::Null} else {
        let h=capture(r,&mut checks,params,32)?;let ac=u64_at(&h,0);let count=u64_at(&h,24);
        if count>64 {return Err("Config rect array exceeds audit bounds".into());}
        let ah=capture(r,&mut checks,ac,0x50)?;
        if r.cstring(u64_at(&ah,16))?!="Single[]" || r.class(u64_at(&ah,0x40))?.name!="Single" {return Err("Config rect array type changed".into());}
        let raw=capture(r,&mut checks,params+32,count as usize*4)?;
        let values:Vec<_>=raw.chunks_exact(4).map(|v|f32_at(v,0)).collect();
        if values.iter().any(|v|!v.is_finite()) {return Err("Nonfinite rect parameter".into());}
        json!(values)
    };
    check(r,&checks)?;
    Ok(json!({"skill":skill,"numeric":numeric,"m_ShowRectPara":rect,"coherent":true,"scope":"owned skill configuration only; no production authorization"}))
}

fn main() -> Result<()> {
    let args:Vec<_>=std::env::args().collect();
    if args.len()!=3 && args.len()!=5 { return Err("Usage: hero-contract-audit PID OUTPUT.json [WIDTH HEIGHT]".into()); }
    let pid:u32=args[1].parse().map_err(|_|"Invalid PID")?;
    if pid==0 {return Err("PID must be positive".into());}
    let size=if args.len()==5 {[args[3].parse().map_err(|_|"Invalid width")?,args[4].parse().map_err(|_|"Invalid height")?]} else {[4000.,3111.]};
    if size.iter().any(|v:&f64|!v.is_finite()||!(32. ..=16384.).contains(v)) {return Err("Resolution outside audit bounds".into());}
    let root=PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().to_path_buf();
    verify_selected_runtime(&root,pid)?;
    let identity=process_identity(pid)?;
    let started=Instant::now();let start_ns=now_ns();
    let mut source=Source::new(ProcessMemory::diagnostic(pid)?,format!("hero-contract-audit:{pid}"))?;
    let mut errors=BTreeMap::new();let mut snapshot=None;
    for _ in 0..100 {
        if started.elapsed()>Duration::from_secs(10) {break;}
        match source.sample(now_ns(),size) {
            Ok(value)=>{snapshot=Some(value);break;},
            Err(error)=>{*errors.entry(error).or_insert(0usize)+=1;std::thread::sleep(Duration::from_millis(5));},
        }
    }
    let sample=snapshot.ok_or_else(||format!("No coherent hero sample in audit bounds: {errors:?}"))?;
    let frame=&sample.frame;let hero=frame.hero_id.int();
    let buttons:Vec<_>=frame.ui.buttons.iter().take(frame.ui.button_count.min(32)as usize).map(|b|json!({
        "slot":b.slot.as_str(),"kind":b.kind.as_str(),"base_skill":b.skill_id.int(),"active":b.active.get(),
        "press":b.press_uv.values.map(|v|v.value),"center":b.center_uv.values.map(|v|v.value),"radius":b.radius_uv.values.map(|v|v.value),
        "geometry_source":b.geometry_source.as_str(),"template":sample.control_types.get(b.slot.as_str()),
    })).collect();
    let variants:Vec<_>=frame.skill_state.buttons.iter().take(frame.skill_state.count.min(32)as usize).map(|v|json!({
        "slot":v.slot.as_str(),"base":v.base_skill_id.int(),"active":v.active_skill_id.int(),
        "cast_locked":v.cast_locked.is_bool().then(||v.cast_locked.get()),
        "state_forbidden":v.state_forbidden.is_bool().then(||v.state_forbidden.get()),
    })).collect();
    let mut skills=BTreeSet::new();
    for b in frame.ui.buttons.iter().take(frame.ui.button_count.min(32)as usize) {if b.skill_id.int()>0 {skills.insert(b.skill_id.int());}}
    for v in frame.skill_state.buttons.iter().take(frame.skill_state.count.min(32)as usize) {if v.active_skill_id.int()>0 {skills.insert(v.active_skill_id.int());}}
    if frame.skill_id.int()>0 {skills.insert(frame.skill_id.int());}
    if skills.len()>64 {return Err("Skill inventory exceeds audit bounds".into());}
    let mut contracts=serde_json::Map::new();
    for skill in skills {
        if process_identity(pid)?!=identity {return Err("Runtime identity changed during audit".into());}
        let start=now_ns();
        contracts.insert(skill.to_string(),json!({
            "start_ns":start,
            "identity":MotionPlan::audit_registry_identity(&mut source.reflection,sample.indicator,skill),
            "values":MotionPlan::audit_registry_values(&mut source.reflection,sample.indicator,skill),
            "configuration":config_values(&mut source.reflection,sample.indicator,skill),
            "drag_gate":MotionPlan::discover(&mut source.reflection,sample.indicator,skill)
                .and_then(|plan|plan.audit_drag_gate(&mut source.reflection)),
            "end_ns":now_ns(),
        }));
    }
    let controls=source.audit_control_state();let ui=source.audit_ui_input();
    let charge_lifecycle=charge_lifecycle(&mut source.reflection,sample.match_owners[2]);
    let final_sample=source.sample(now_ns(),size);
    let same_match=final_sample.as_ref().is_ok_and(|value|value.match_owners==sample.match_owners&&value.indicator==sample.indicator&&value.frame.hero_id.int()==hero);
    if process_identity(pid)?!=identity {return Err("Runtime identity changed during audit".into());}
    let output=json!({
        "format":"clbb-hero-contract-audit","version":1,"pid":pid,"birth":identity.0,"executable":identity.1,
        "start_ns":start_ns,"end_ns":now_ns(),"hero":hero,"epoch":frame.token.as_str(),"resolution":size,
        "sample_ns":frame.time_ns.value,"active_skill":frame.skill_id.int(),"active_slot":frame.ui.active_slot.as_str(),
        "read_ms":frame.sample_duration_ms.value,"types":sample.control_types,"buttons":buttons,"variants":variants,
        "linked":sample.linked,"contracts":contracts,"controls":controls,"ui":ui,"charge_lifecycle":charge_lifecycle,"sample_rejections":errors,
        "same_match_at_end":same_match,"final_sample_error":final_sample.err(),
        "scope":"read-only owned gameplay controls/configuration; no memory writes, input or production template authorization",
    });
    let bytes=serde_json::to_vec_pretty(&output).map_err(|e|e.to_string())?;
    if bytes.len()>4*1024*1024 {return Err("Audit output exceeds 4MiB bound".into());}
    let mut file=OpenOptions::new().write(true).create_new(true).open(&args[2]).map_err(|e|e.to_string())?;
    file.write_all(&bytes).and_then(|_|file.write_all(b"\n")).map_err(|e|e.to_string())?;
    println!("hero={hero} same_match={same_match} contracts={} output={}",output["contracts"].as_object().unwrap().len(),args[2]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refuses_non_boolean_and_non_finite_config_values() {
        assert!(primitive(2,&[2]).is_err());
        assert!(primitive(12,&f32::NAN.to_le_bytes()).is_err());
        assert!(primitive(13,&f64::INFINITY.to_le_bytes()).is_err());
        assert_eq!(primitive(2,&[1]).unwrap(),json!(true));
        assert_eq!(primitive(8,&(-7i32).to_le_bytes()).unwrap(),json!(-7));
    }
}
