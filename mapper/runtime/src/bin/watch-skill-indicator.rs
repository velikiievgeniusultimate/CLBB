// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded read-only watch of one current owned hero's native skill indicator.
//! READY is flushed before watching so a separate owner can supply test input.
use clbb_mapper::{aim::MotionPlan, platform::{Memory, linux::{ProcessMemory, now_ns}},
    reflection::{Reflection, u64_at}, source::{Source, Checks, capture, check}};
use serde_json::{Value, json};
use std::{fs::{self, OpenOptions}, io::{self, Write, BufWriter}, path::{Path, PathBuf},
          process::Command, time::{Duration, Instant}};

fn identity(pid:u32)->Result<(u64,PathBuf),String> {
    let root=PathBuf::from(format!("/proc/{pid}"));
    let stat=fs::read_to_string(root.join("stat")).map_err(|e|e.to_string())?;
    let fields:Vec<_>=stat.rsplit_once(')').ok_or("Invalid process stat")?.1.split_whitespace().collect();
    if matches!(fields.first().copied(),Some("Z"|"X"|"x")){return Err("Runtime exited".into());}
    Ok((fields.get(19).ok_or("Missing process birth")?.parse().map_err(|_|"Invalid process birth")?,
        fs::read_link(root.join("exe")).map_err(|e|e.to_string())?))
}
fn owned(root:&Path,pid:u32)->Result<(),String> {
    let status=Command::new("python3").args(["-c","import sys; from pathlib import Path; sys.path.insert(0,sys.argv[1]); from runtime_identity import runtime_executable_is_owned; sys.exit(0 if runtime_executable_is_owned(int(sys.argv[2]),Path(sys.argv[3])) else 1)"])
        .arg(root.join("tools")).arg(pid.to_string()).arg(root).status().map_err(|e|e.to_string())?;
    if status.success(){Ok(())}else{Err("PID is not this checkout's verified owned runtime".into())}
}
fn bounds(pid:u32,skill:i32,duration:u64)->Result<(),String> {
    if pid==0||!(1..=10_000_000).contains(&skill)||!(100..=10_000).contains(&duration) {
        Err("Expected positive PID/skill and duration 100..10000 ms".into())
    }else{Ok(())}
}
fn emit(writer:&mut impl Write,row:&Value)->Result<(),String> {
    serde_json::to_writer(&mut *writer,row).map_err(|e|e.to_string())?;
    writer.write_all(b"\n").map_err(|e|e.to_string())
}

fn instance_field<M:Memory>(r:&mut Reflection<M>,checks:&mut Checks,object:u64,name:&str,kinds:&[u8])->Result<(u64,u8),String> {
    let mut class=u64_at(&capture(r,checks,object,8)?,0);
    for _ in 0..16 {
        if class==0 {break;}
        let metadata=r.class(class)?;
        if let Some(field)=metadata.fields.get(name) {
            if field.is_static||field.offset>1_048_576||!kinds.contains(&field.kind) {
                return Err(format!("Unexpected local field type/layout: {name}"));
            }
            return Ok((object+field.offset,field.kind));
        }
        class=metadata.parent;
    }
    Err(format!("Missing local field: {name}"))
}
fn instance_link<M:Memory>(r:&mut Reflection<M>,checks:&mut Checks,object:u64,name:&str)->Result<u64,String> {
    let (at,_)=instance_field(r,checks,object,name,&[18,21])?;
    Ok(u64_at(&capture(r,checks,at,8)?,0))
}
fn instance_integer<M:Memory>(r:&mut Reflection<M>,checks:&mut Checks,object:u64,name:&str)->Result<(Value,&'static str),String> {
    let (at,kind)=instance_field(r,checks,object,name,&[8,9])?;
    let raw=capture(r,checks,at,4)?;
    Ok(if kind==8 {(json!(i32::from_le_bytes(raw.try_into().unwrap())),"i32")}
       else {(json!(u32::from_le_bytes(raw.try_into().unwrap())),"u32")})
}
/// An independent receipt, not part of Source's camera/UI bracket. Re-resolve
/// current local-player links even when an unsupported motion rejects Source.
/// Do not read currentSkillResult here: its UI context may belong to another
/// entity, whereas these counters come only from the checked local Actor.
fn local_cast_receipt<M:Memory>(r:&mut Reflection<M>,owners:[u64;4],indicator:u64,hero:i32)->Result<Value,String> {
    let start=now_ns();let mut checks=Vec::new();
    if owners.contains(&0)||indicator==0 {return Err("Missing initial local owner".into());}
    for (name,expected) in [("BattleManager",owners[0]),("LogicBattleManager",owners[1]),("SkillIndicatorComp",indicator)] {
        let at=r.static_field(name,"Instance")?;
        if u64_at(&capture(r,&mut checks,at,8)?,0)!=expected {return Err(format!("Local {name} instance changed"));}
    }
    for object in owners.into_iter().chain([indicator]) {
        let class=u64_at(&capture(r,&mut checks,object,8)?,0);r.validate_class(class)?;
    }
    if instance_link(r,&mut checks,owners[0],"m_LocalPlayerShow")?!=owners[2]
        ||instance_link(r,&mut checks,owners[1],"<m_LocalPlayerLogic>k__BackingField")?!=owners[3] {
        return Err("Local player show/Actor link changed".into());
    }
    let (observed_hero,_)=instance_integer(r,&mut checks,owners[3],"m_ID")?;
    if observed_hero.as_i64()!=Some(hero as i64) {return Err("Local Actor hero changed".into());}
    let mut values=serde_json::Map::new();let mut types=serde_json::Map::new();
    for name in ["m_iFirstSkillUseCount","m_iSecondSkillUseCount","m_iThirdSkillUseCount","m_iSkillUseCount","m_glLastPushedSkillID"] {
        let (value,kind)=instance_integer(r,&mut checks,owners[3],name)?;
        values.insert(format!("actor.{name}"),value);types.insert(format!("actor.{name}"),json!(kind));
    }
    check(r,&checks)?;
    Ok(json!({"start_ns":start,"end_ns":now_ns(),"hero":hero,"same_match":true,"values":values,"types":types,
        "scope":"coherent local Actor counters; separate bracket from source/native; no foreign result context"}))
}
fn main()->Result<(),String> {
    let args:Vec<_>=std::env::args().collect();
    if args.len()!=5{return Err("Usage: watch-skill-indicator PID SKILL DURATION_MS OUTPUT.jsonl".into());}
    let pid:u32=args[1].parse().map_err(|_|"Invalid PID")?;
    let skill:i32=args[2].parse().map_err(|_|"Invalid skill")?;
    let duration:u64=args[3].parse().map_err(|_|"Invalid duration")?;
    bounds(pid,skill,duration)?;
    let root=PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().to_path_buf();
    owned(&root,pid)?;let expected=identity(pid)?;
    let mut source=Source::new(ProcessMemory::diagnostic(pid)?,format!("watch-skill:{pid}"))?;
    let init=Instant::now();let mut initial=None;let mut last_error=String::new();
    for _ in 0..100 {
        if init.elapsed()>Duration::from_secs(10){break;}
        match source.sample(now_ns(),[4000.,3111.]) {
            Ok(frame)=>{initial=Some(frame);break},
            Err(error)=>{last_error=error;std::thread::sleep(Duration::from_millis(5));},
        }
    }
    let initial=initial.ok_or_else(||format!("No coherent initial hero sample: {last_error}"))?;
    let hero=initial.frame.hero_id.int();let owners=initial.match_owners;let indicator=initial.indicator;
    // Resolve and validate the selected registry entry before advertising READY.
    let contract=MotionPlan::audit_registry_identity(&mut source.reflection,indicator,skill)?;
    let initial_cast_receipt=local_cast_receipt(&mut source.reflection,owners,indicator,hero);
    if identity(pid)?!=expected{return Err("Runtime identity changed before READY".into());}
    let file=OpenOptions::new().write(true).create_new(true).open(&args[4]).map_err(|e|e.to_string())?;
    let mut writer=BufWriter::new(file);let start_ns=now_ns();
    emit(&mut writer,&json!({"event":"header","format":"clbb-skill-indicator-watch","version":1,
        "pid":pid,"birth":expected.0,"executable":expected.1,"hero":hero,"skill":skill,"epoch":initial.frame.token.as_str(),
        "start_ns":start_ns,"duration_ms":duration,"contract":contract,"initial_cast_receipt":initial_cast_receipt,
        "scope":"read-only owned registry observations, not production authorization; source/native rows have separate brackets"}))?;
    writer.flush().map_err(|e|e.to_string())?;
    println!("READY hero={hero} skill={skill} pid={pid} output={}",args[4]);
    io::stdout().flush().map_err(|e|e.to_string())?;
    let started=Instant::now();let mut next_source=Instant::now();let mut next_identity=Instant::now();
    let mut rows=0usize;let mut native_ok=0usize;let mut source_ok=0usize;let mut cast_receipt_ok=0usize;let mut same_match=true;let mut stop_reason=String::new();
    while started.elapsed()<Duration::from_millis(duration) {
        let tick=Instant::now();
        if tick>=next_identity {
            if identity(pid).ok().as_ref()!=Some(&expected){same_match=false;stop_reason="runtime_identity_changed".into();break;}
            next_identity=tick+Duration::from_millis(100);
        }
        if tick>=next_source {
            let ns=now_ns();let result=source.sample(ns,[4000.,3111.]);
            // This read remains available when only motion values are unsupported.
            // It validates the original match links before reading any counters.
            let cast_receipt=local_cast_receipt(&mut source.reflection,owners,indicator,hero);
            if cast_receipt.is_ok(){cast_receipt_ok+=1;}
            match result {
                Ok(frame)=>{
                    source_ok+=1;
                    let same=frame.frame.hero_id.int()==hero&&frame.match_owners==owners&&frame.indicator==indicator;
                    emit(&mut writer,&json!({"event":"source","start_ns":ns,"end_ns":now_ns(),"same_match":same,
                        "hero":frame.frame.hero_id.int(),"skill":frame.frame.skill_id.int(),"active_slot":frame.frame.ui.active_slot.as_str(),
                        "sample_start_ns":frame.frame.time_ns.value-frame.frame.sample_duration_ms.value*1e6,"sample_end_ns":frame.frame.time_ns.value,
                        "logical_ready":frame.frame.logical.ready.get(),"input_ready":frame.frame.logical.input_ready.get(),
                        "valid_drag":frame.frame.logical.valid_drag.get(),"first_drag":frame.frame.logical.first_drag.get(),
                        "native_source":frame.frame.logical.source.as_str(),"aim_world":frame.aim,"types":frame.control_types,
                        "control_holds":frame.control_holds,"cast_receipt":cast_receipt,
                        "world_range":{"value":frame.frame.logical.world_range.value,"kind":frame.frame.logical.world_range.kind},
                        "world_range_source":frame.frame.logical.world_range_source.as_str(),
                        "world_range_source_present":frame.frame.logical.world_range_source_present.get(),
                        "active_joystick":{"ready":frame.frame.active_joystick.ready.get(),"source":frame.frame.active_joystick.source.as_str(),
                            "slot":frame.frame.active_joystick.slot.as_str(),"skill":frame.frame.active_joystick.skill_id.int(),
                            "center":frame.frame.active_joystick.center_uv.values.map(|v|v.value),"center_count":frame.frame.active_joystick.center_uv.count,
                            "radius":frame.frame.active_joystick.radius_uv.values.map(|v|v.value),"radius_count":frame.frame.active_joystick.radius_uv.count,
                            "sample_start_ns":frame.frame.active_joystick.sample_start_ns.value,"sample_end_ns":frame.frame.active_joystick.sample_end_ns.value},
                        "direction_ready":frame.frame.direction_input.ready.get(),"direction":frame.frame.direction_input.direction_xz.values.map(|v|v.value),
                        "raw_touch":frame.frame.motion.drag_touch_raw_xyz.values.map(|v|v.value),"logical_offset":frame.frame.logical.offset_xz.values.map(|v|v.value)}))?;
                    if !same {same_match=false;stop_reason="hero_or_match_changed".into();break;}
                },
                Err(error)=>emit(&mut writer,&json!({"event":"source_error","start_ns":ns,"end_ns":now_ns(),"error":error,"cast_receipt":cast_receipt}))?,
            }
            next_source=tick+Duration::from_millis(20);
        }
        let ns=now_ns();let result=MotionPlan::audit_registry_values(&mut source.reflection,indicator,skill);
        if result.is_ok(){native_ok+=1;}
        emit(&mut writer,&json!({"event":"native","start_ns":ns,"end_ns":now_ns(),"skill":skill,"result":result}))?;
        rows+=1;
        if rows%64==0 {writer.flush().map_err(|e|e.to_string())?;}
        if let Some(delay)=Duration::from_millis(3).checked_sub(tick.elapsed()){std::thread::sleep(delay);}
    }
    let final_cast_receipt=local_cast_receipt(&mut source.reflection,owners,indicator,hero);
    if same_match {
        match source.sample(now_ns(),[4000.,3111.]) {
            Ok(frame)=>same_match=frame.frame.hero_id.int()==hero&&frame.match_owners==owners&&frame.indicator==indicator,
            Err(error)=>{same_match=final_cast_receipt.is_ok();stop_reason=format!("final_source_unavailable: {error}");},
        }
    }
    let identity_unchanged=identity(pid).ok().as_ref()==Some(&expected);
    if !identity_unchanged {same_match=false;}
    let summary=json!({"event":"summary","end_ns":now_ns(),"rows":rows,"native_ok":native_ok,"source_ok":source_ok,"cast_receipt_ok":cast_receipt_ok,
        "final_cast_receipt":final_cast_receipt,
        "same_match":same_match,"identity_unchanged":identity_unchanged,"stop_reason":stop_reason,"elapsed_ms":started.elapsed().as_secs_f64()*1e3});
    emit(&mut writer,&summary)?;writer.flush().map_err(|e|e.to_string())?;println!("DONE {summary}");
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell,RefCell};
    const OWNERS:[u64;4]=[0x9000,0x9100,0x9200,0x9300];
    const INDICATOR:u64=0x9400;
    struct FixtureMemory {bytes:RefCell<Vec<u8>>,race_owner_on_counter:Cell<bool>}
    impl Memory for FixtureMemory {
        fn regions(&self)->Result<Vec<clbb_mapper::platform::Region>,String> {
            Ok(vec![clbb_mapper::platform::Region{start:0,end:self.bytes.borrow().len() as u64,readable:true,writable:false,name:"owned synthetic metadata".into()}])
        }
        fn read(&self,address:u64,length:usize)->Result<Vec<u8>,String> {
            let start=address as usize;let end=start.checked_add(length).ok_or("Fixture overflow")?;
            let result=self.bytes.borrow().get(start..end).ok_or("Fixture read outside bounds")?.to_vec();
            if address==OWNERS[3]+20&&self.race_owner_on_counter.replace(false) {
                self.bytes.borrow_mut()[OWNERS[0] as usize+16..OWNERS[0] as usize+24].copy_from_slice(&0x9999u64.to_le_bytes());
            }
            Ok(result)
        }
        fn read_into(&self,address:u64,bytes:&mut[u8])->Result<(),String> {bytes.copy_from_slice(&self.read(address,bytes.len())?);Ok(())}
    }
    fn put64(bytes:&mut[u8],at:usize,value:u64){bytes[at..at+8].copy_from_slice(&value.to_le_bytes());}
    fn fixture(counter_kind:u8,counter_static:bool)->Reflection<FixtureMemory> {
        let mut bytes=vec![0u8;0x10000];let mut string_at=0x5000;let mut table_at=0x3000;let mut type_at=0x4000;
        let mut named=Vec::new();
        let classes:Vec<(&str,u64,Vec<(&str,u32,u8,bool)>)>=vec![
            ("BattleManager",OWNERS[0],vec![("Instance",0,18,true),("m_LocalPlayerShow",16,18,false)]),
            ("LogicBattleManager",OWNERS[1],vec![("Instance",0,18,true),("<m_LocalPlayerLogic>k__BackingField",16,18,false)]),
            ("ShowEntity",OWNERS[2],vec![]),
            ("Actor",OWNERS[3],vec![("m_ID",16,8,false),("m_iFirstSkillUseCount",20,counter_kind,counter_static),
                ("m_iSecondSkillUseCount",24,8,false),("m_iThirdSkillUseCount",28,8,false),("m_iSkillUseCount",32,8,false),("m_glLastPushedSkillID",36,9,false)]),
            ("SkillIndicatorComp",INDICATOR,vec![("Instance",0,18,true)]),
        ];
        for (index,(name,object,fields)) in classes.into_iter().enumerate() {
            let class=0x1000+index*0x200;let statics=0x8000+index*32;
            bytes[string_at..string_at+name.len()].copy_from_slice(name.as_bytes());put64(&mut bytes,class+16,string_at as u64);string_at+=256;
            put64(&mut bytes,class+0x40,class as u64);put64(&mut bytes,class+0x48,class as u64);
            put64(&mut bytes,class+0xa8,statics as u64);put64(&mut bytes,class+0x78,table_at as u64);
            bytes[class+0x104..class+0x106].copy_from_slice(&(fields.len() as u16).to_le_bytes());
            put64(&mut bytes,object as usize,class as u64);put64(&mut bytes,statics,object);
            for (field,offset,kind,is_static) in fields {
                bytes[string_at..string_at+field.len()].copy_from_slice(field.as_bytes());put64(&mut bytes,table_at,string_at as u64);string_at+=256;
                put64(&mut bytes,table_at+8,type_at as u64);put64(&mut bytes,table_at+16,class as u64);
                bytes[table_at+24..table_at+28].copy_from_slice(&offset.to_le_bytes());
                let attrs=((kind as u32)<<16)|if is_static {16}else{0};bytes[type_at+8..type_at+12].copy_from_slice(&attrs.to_le_bytes());
                table_at+=32;type_at+=16;
            }
            named.push((name.to_owned(),class as u64));
        }
        put64(&mut bytes,OWNERS[0] as usize+16,OWNERS[2]);put64(&mut bytes,OWNERS[1] as usize+16,OWNERS[3]);
        for (offset,value) in [(16,77u32),(20,7),(24,8),(28,9),(32,1),(36,u32::MAX)] {
            bytes[OWNERS[3] as usize+offset..OWNERS[3] as usize+offset+4].copy_from_slice(&value.to_le_bytes());
        }
        let mut reflection=Reflection::new(FixtureMemory{bytes:RefCell::new(bytes),race_owner_on_counter:Cell::new(false)}).unwrap();
        reflection.named.extend(named);reflection
    }
    #[test]
    fn rejects_unbounded_or_invalid_watch_arguments() {
        for (pid,skill,ms)in [(0,12650,4000),(1,0,4000),(1,-1,4000),(1,12650,99),(1,12650,10001)] {assert!(bounds(pid,skill,ms).is_err());}
        assert!(bounds(1,12650,4000).is_ok());
    }
    #[test]
    fn local_receipt_preserves_signed_and_unsigned_counter_types() {
        let mut r=fixture(8,false);let receipt=local_cast_receipt(&mut r,OWNERS,INDICATOR,77).unwrap();
        assert_eq!(receipt["values"]["actor.m_iFirstSkillUseCount"],7);
        assert_eq!(receipt["values"]["actor.m_iSecondSkillUseCount"],8);
        assert_eq!(receipt["values"]["actor.m_iThirdSkillUseCount"],9);
        assert_eq!(receipt["values"]["actor.m_glLastPushedSkillID"].as_u64(),Some(u32::MAX as u64));
        assert_eq!(receipt["types"]["actor.m_iFirstSkillUseCount"],"i32");
        assert_eq!(receipt["types"]["actor.m_glLastPushedSkillID"],"u32");
        assert!(receipt["start_ns"].as_i64().unwrap()<=receipt["end_ns"].as_i64().unwrap());
    }
    #[test]
    fn receipt_rejects_wrong_hero_owner_rebinding_and_non_integer_or_static_fields() {
        assert!(local_cast_receipt(&mut fixture(8,false),OWNERS,INDICATOR,78).is_err());
        assert!(local_cast_receipt(&mut fixture(12,false),OWNERS,INDICATOR,77).is_err());
        assert!(local_cast_receipt(&mut fixture(8,true),OWNERS,INDICATOR,77).is_err());
        let mut r=fixture(8,false);
        r.memory.bytes.borrow_mut()[OWNERS[0] as usize+16..OWNERS[0] as usize+24].copy_from_slice(&0x9999u64.to_le_bytes());
        assert!(local_cast_receipt(&mut r,OWNERS,INDICATOR,77).is_err());
        let mut r=fixture(8,false);r.memory.race_owner_on_counter.set(true);
        assert!(local_cast_receipt(&mut r,OWNERS,INDICATOR,77).is_err(),"owner changed while reading counters must invalidate the full receipt");
    }
}
