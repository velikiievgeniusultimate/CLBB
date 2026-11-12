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
            "end_ns":now_ns(),
        }));
    }
    let controls=source.audit_control_state();let ui=source.audit_ui_input();
    let final_sample=source.sample(now_ns(),size);
    let same_match=final_sample.as_ref().is_ok_and(|value|value.match_owners==sample.match_owners&&value.indicator==sample.indicator&&value.frame.hero_id.int()==hero);
    if process_identity(pid)?!=identity {return Err("Runtime identity changed during audit".into());}
    let output=json!({
        "format":"clbb-hero-contract-audit","version":1,"pid":pid,"birth":identity.0,"executable":identity.1,
        "start_ns":start_ns,"end_ns":now_ns(),"hero":hero,"epoch":frame.token.as_str(),"resolution":size,
        "sample_ns":frame.time_ns.value,"active_skill":frame.skill_id.int(),"active_slot":frame.ui.active_slot.as_str(),
        "read_ms":frame.sample_duration_ms.value,"types":sample.control_types,"buttons":buttons,"variants":variants,
        "linked":sample.linked,"contracts":contracts,"controls":controls,"ui":ui,"sample_rejections":errors,
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
