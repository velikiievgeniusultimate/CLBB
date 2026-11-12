//! Bounded read-only audit of the existing owner-checked mapper source.
//! No writes, input, instruction decoding, account data or raw-memory dumps.
use clbb_mapper::{
    platform::linux::{ProcessMemory, now_ns},
    source::Source,
};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

fn identity(pid: u32, birth: u64, exe: &std::path::Path) -> Result<(), String> {
    let path = PathBuf::from(format!("/proc/{pid}"));
    let stat = std::fs::read_to_string(path.join("stat")).map_err(|e| e.to_string())?;
    let fields: Vec<_> = stat
        .rsplit_once(')')
        .ok_or("Bad process identity")?
        .1
        .split_whitespace()
        .collect();
    if fields.get(19).and_then(|v| v.parse().ok()) != Some(birth)
        || matches!(fields.first().copied(), Some("Z" | "X" | "x"))
        || std::fs::read_link(path.join("exe")).map_err(|e| e.to_string())? != exe
    {
        return Err("Owned runtime identity changed".into());
    }
    Ok(())
}

// A separate, explicitly timestamped observation of local skill result data.
// It is never presented as part of Source's camera/UI coherence bracket.
fn receipt(
    source: &mut Source<ProcessMemory>,
    show: u64,
    actor: u64,
) -> Result<serde_json::Value, String> {
    let r = &mut source.reflection;
    let start = now_ns();
    let mut checks = Vec::new();
    let mut values = serde_json::Map::new();
    let mut objects = vec![("actor", actor), ("show", show)];
    for (role, parent, expected) in [("actor_skill", actor, "LogicSkillComp"),
                                     ("show_skill", show, "ShowSkillComp")] {
        let link = r.field(parent, "m_SkillComp")?;
        let raw = r.read(link, 8)?;
        let object = clbb_mapper::reflection::u64_at(&raw, 0);
        checks.push((link, raw));
        if object != 0 {
            let owner = r.pointer(object)?;
            if r.class(owner)?.name != expected {
                return Err("Unexpected local skill component class".into());
            }
            objects.push((role, object));
        }
    }
    for (role, parent) in [("actor_states",actor),("show_states",show)] {
        let link=r.field(parent,"m_States")?;
        let raw=r.read(link,8)?;let state=clbb_mapper::reflection::u64_at(&raw,0);
        checks.push((link,raw));
        if state==0 {continue;}
        let owner=r.pointer(state)?;
        if r.class(owner)?.name!="EntityStates" {return Err("Unexpected local state container".into());}
        checks.push((state,owner.to_le_bytes().to_vec()));
        let link=r.field(state,"states")?;
        let raw=r.read(link,8)?;let array=clbb_mapper::reflection::u64_at(&raw,0);
        checks.push((link,raw));
        if array==0 {continue;}
        let header=r.read(array,32)?;
        let class=clbb_mapper::reflection::u64_at(&header,0);
        let count=clbb_mapper::reflection::u64_at(&header,24);
        if count>512 {return Err("Local state array exceeds audit bounds".into());}
        // Array metadata stores the element class at +0x40; unlike ordinary
        // classes it cannot satisfy Reflection::class's self-identity check.
        let class_header=r.read(class,0x50)?;
        let class_name=r.cstring(clbb_mapper::reflection::u64_at(&class_header,16))?;
        let element=clbb_mapper::reflection::u64_at(&class_header,0x40);
        let element_name=r.class(element)?.name;
        if class_name!=format!("{element_name}[]") {return Err("Local state array type mismatch".into());}
        checks.push((class,class_header));
        checks.push((array,header));checks.push((class+0x40,element.to_le_bytes().to_vec()));
        values.insert(format!("{role}.type"),json!({"array":class_name,"element":element_name,"length":count}));
        let stride=match element_name.as_str(){"Boolean"|"Byte"=>1,"Int16"|"UInt16"=>2,"Int32"|"UInt32"=>4,_=>0};
        if stride!=0 {
            let raw=r.read(array+32,count as usize*stride)?;
            let mut data=Vec::new();
            for entry in raw.chunks_exact(stride) {
                let v=match element_name.as_str(){
                    "Boolean"=>{if entry[0]>1{return Err("Invalid state boolean".into());}json!(entry[0]!=0)},
                    "Byte"=>json!(entry[0]),"Int16"=>json!(i16::from_le_bytes(entry.try_into().unwrap())),
                    "UInt16"=>json!(u16::from_le_bytes(entry.try_into().unwrap())),
                    "Int32"=>json!(i32::from_le_bytes(entry.try_into().unwrap())),
                    "UInt32"=>json!(u32::from_le_bytes(entry.try_into().unwrap())),_=>unreachable!()};
                data.push(v);
            }
            values.insert(format!("{role}.values"),json!(data));checks.push((array+32,raw));
        }
    }
    if let Ok(link)=r.field(show,"m_baseStateComp") {
        let raw=r.read(link,8)?;let object=clbb_mapper::reflection::u64_at(&raw,0);
        checks.push((link,raw));
        if object!=0 {
            if r.class(r.pointer(object)?)?.name!="ShowStateComp" {return Err("Unexpected show state component".into());}
            objects.push(("show_state",object));
        }
    }
    let link = r.field(show, "m_SkillCompCurrentSkillData")?;
    let raw = r.read(link, 8)?;
    let data = clbb_mapper::reflection::u64_at(&raw, 0);
    checks.push((link, raw));
    if data != 0 {
        values.insert("result.owner_is_local_skill_comp".into(),json!(clbb_mapper::source::skill_result_is_local(r,show,data,&mut checks)?));
        objects.push(("result", data));
    }
    for (role, object) in objects {
        let owner = r.pointer(object)?;
        checks.push((object, owner.to_le_bytes().to_vec()));
        let mut class = owner;
        for _ in 0..16 {
            if class == 0 {
                break;
            }
            let c = r.class(class)?;
            if role == "result" && class == owner && c.name != "ShowSkillData" {
                return Err("Unexpected local skill-result class".into());
            }
            for name in [
                "m_GUID",
                "m_Guid",
                "m_uGuid",
                "m_uGUID",
                "m_EntityGuid",
                "m_uSrcOwnerGuid",
                "m_uParentSkillGuid",
                "m_uSrcSkillGuid",
                "m_TranID",
                "tempStatusPos",
                "bFreeAnima",
                "bStopAnima",
                "m_uLockMoveId",
                "m_uLockComAtkId",
                "m_SkillBit",
                "<m_EnergyPoint>k__BackingField",
                "<m_EnergyPointMax>k__BackingField",
                "m_uLockCastId",
                "m_uLockSpecialSkillId",
                "iLockSkillAnimaID",
                "m_LastSkillID",
                "m_uCurSkillId",
                "m_WaitSkill",
                "uOneFrameSkillTimer",
                "m_iFirstSkillUseCount",
                "m_iSecondSkillUseCount",
                "m_iThirdSkillUseCount",
                "m_iSkillUseCount",
                "m_glLastPushedSkillID",
                "m_InitSkillID",
                "m_SkillGuid",
                "m_fSrcDirX",
                "m_fSrcDirY",
                "m_fSrcPosX",
                "m_fSrcPosY",
            ] {
                if let Some(field) = c.fields.get(name) {
                    if field.is_static || field.offset > 1048576 {
                        continue;
                    }
                    let length = match field.kind {
                        2 => 1,
                        8 | 9 | 12 => 4,
                        13 => 8,
                        _ => continue,
                    };
                    let address = object + field.offset;
                    let raw = r.read(address, length)?;
                    let value = match field.kind {
                        2 => {
                            if raw[0] > 1 { return Err("Invalid local skill boolean".into()); }
                            json!(raw[0] != 0)
                        },
                        8 => json!(i32::from_le_bytes(raw[..4].try_into().unwrap())),
                        9 => json!(u32::from_le_bytes(raw[..4].try_into().unwrap())),
                        12 => json!(f32::from_le_bytes(raw[..4].try_into().unwrap())),
                        13 => json!(f64::from_le_bytes(raw[..8].try_into().unwrap())),
                        _ => unreachable!(),
                    };
                    values.insert(format!("{role}.{name}"), value);
                    checks.push((address, raw));
                }
            }
            class = c.parent;
        }
    }
    for (address, bytes) in checks {
        if r.read(address, bytes.len())? != bytes {
            return Err("Local result changed during receipt".into());
        }
    }
    Ok(json!({"start_ns":start,"end_ns":now_ns(),"values":values}))
}

fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 8 {
        return Err(
            "Usage: source-audit PID BIRTH WIDTH HEIGHT ATTEMPTS INTERVAL_US OUTPUT.jsonl".into(),
        );
    }
    let pid: u32 = args[1].parse().map_err(|_| "Bad pid")?;
    let birth: u64 = args[2].parse().map_err(|_| "Bad birth")?;
    let size: [f64; 2] = [
        args[3].parse().map_err(|_| "Bad width")?,
        args[4].parse().map_err(|_| "Bad height")?,
    ];
    let attempts: usize = args[5].parse().map_err(|_| "Bad attempt count")?;
    let interval_us: u64 = args[6].parse().map_err(|_| "Bad interval")?;
    if pid == 0
        || birth == 0
        || !(1..=100_000).contains(&attempts)
        || !(200..=100_000).contains(&interval_us)
        || size
            .iter()
            .any(|v| !v.is_finite() || !(32. ..=16384.).contains(v))
    {
        return Err("Audit bounds rejected".into());
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let exe = root.join("prefix/bin/android-translation-layer");
    identity(pid, birth, &exe)?;
    let mut source = Source::new(ProcessMemory::diagnostic(pid)?, format!("audit:{pid}"))?;
    let output = PathBuf::from(&args[7]);
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
        .map_err(|e| e.to_string())?;
    let mut writer = BufWriter::new(file);
    let mut errors = BTreeMap::<String, usize>::new();
    let mut clocks = BTreeSet::new();
    let mut durations = Vec::with_capacity(attempts);
    let mut valid = 0usize;
    let started = Instant::now();
    let mut schema_written = false;
    let mut diagnostic_motions = BTreeMap::new();
    let mut diagnostic_identities = BTreeMap::new();
    for attempt in 0..attempts {
        if attempt % 128 == 0 {
            identity(pid, birth, &exe)?;
        }
        let entry = Instant::now();
        let begin = now_ns();
        let row = match source.sample(begin, size) {
            Ok(s) => {
                valid += 1;
                clocks.insert(s.game_clock_ms);
                durations.push(s.frame.sample_duration_ms.value);
                let f = &s.frame;
                let drag_gate = if [2510,2511,2520,20150].contains(&f.skill_id.int()) {
                    let skill=f.skill_id.int();
                    if !diagnostic_motions.contains_key(&skill) {
                        if let Ok(plan)=clbb_mapper::aim::MotionPlan::discover(&mut source.reflection,s.indicator,skill) {
                            diagnostic_motions.insert(skill,plan);
                        }
                    }
                    diagnostic_motions.get(&skill).map(|p| {
                        let start=now_ns();
                        p.audit_drag_gate(&mut source.reflection).map(|values|json!({"start_ns":start,"end_ns":now_ns(),"values":values}))
                    })
                } else { None };

                let control_start=now_ns();
                let control_state=source.audit_control_state().map(|values|json!({"start_ns":control_start,"end_ns":now_ns(),"values":values}));
                let ui_start=now_ns();
                let ui_input=source.audit_ui_input().map(|values|json!({"start_ns":ui_start,"end_ns":now_ns(),"values":values}));
                let ui_buttons:Vec<_>=f.ui.buttons.iter().take(f.ui.button_count.min(32) as usize).map(|b|json!({"slot":b.slot.as_str(),"skill":b.skill_id.int(),"press":b.press_uv.values.map(|v|v.value),"active":b.active.get()})).collect();
                let native_values=if f.skill_id.int()>0 {
                    let start=now_ns();
                    Some(clbb_mapper::aim::MotionPlan::audit_registry_values(&mut source.reflection,s.indicator,f.skill_id.int()).map(|mut value| {
                        value["start_ns"]=json!(start);value["end_ns"]=json!(now_ns());value
                    }))
                } else {None};
                let native_identity = if f.skill_id.int()>0 {
                    let skill=f.skill_id.int();
                    if !diagnostic_identities.contains_key(&skill) {
                        if let Ok(value)=clbb_mapper::aim::MotionPlan::audit_registry_identity(&mut source.reflection,s.indicator,skill) {
                            let path=output.with_extension(format!("skill-{skill}.json"));
                            serde_json::to_writer_pretty(File::create(path).map_err(|e|e.to_string())?,&value).map_err(|e|e.to_string())?;
                            diagnostic_identities.insert(skill,value);
                        }
                    }
                    diagnostic_identities.get(&skill).map(|v|json!({"motion":v["motion"],"package":v["package"]}))
                } else {None};
                if !schema_written {
                    let ui_schema=source.audit_ui_schema();
                    serde_json::to_writer_pretty(File::create(output.with_extension("ui-fields.json")).map_err(|e|e.to_string())?,&ui_schema).map_err(|e|e.to_string())?;
                    for variant in f.skill_state.buttons.iter().take(f.skill_state.count.min(32) as usize) {
                        let skill=variant.active_skill_id.int();
                        if skill>0 {
                            let start=now_ns();
                            let result=clbb_mapper::aim::MotionPlan::audit_registry_values(&mut source.reflection,s.indicator,skill);
                            let path=output.with_extension(format!("skill-{skill}-idle-config.json"));
                            serde_json::to_writer_pretty(File::create(path).map_err(|e|e.to_string())?,&json!({"start_ns":start,"end_ns":now_ns(),"idle_values_not_current_input":true,"result":result})).map_err(|e|e.to_string())?;
                        }
                    }
                    let mut schema = Vec::new();
                    let mut objects = vec![
                        ("indicator".to_owned(), s.indicator),
                        ("show".to_owned(), s.match_owners[2]),
                        ("actor".to_owned(), s.match_owners[3]),
                    ];
                    // Only named links already owned by this local hero, one hop.
                    for (role, object) in
                        [("show", s.match_owners[2]), ("actor", s.match_owners[3])]
                    {
                        for name in [
                            "m_OwnSkillComp",
                            "m_SkillComp",
                            "m_SkillCompCurrentSkillData",
                            "m_States",
                            "m_baseStateComp",
                            "animaState",
                        ] {
                            if let Ok(address) = source.reflection.field(object, name) {
                                if let Ok(child) = source.reflection.pointer(address) {
                                    if child != 0
                                        && source
                                            .reflection
                                            .class(source.reflection.pointer(child)?)
                                            .is_ok()
                                        && source.reflection.pointer(address)? == child
                                    {
                                        objects.push((format!("{role}.{name}"), child));
                                    }
                                }
                            }
                        }
                    }
                    // Follow only explicit local skill-controller links. This
                    // is metadata discovery, not an eligibility assumption.
                    let mut children=Vec::new();
                    for (role,parent) in &objects {
                        if role!="actor.m_SkillComp" {continue;}
                        let metadata=source.reflection.class(source.reflection.pointer(*parent)?)?;
                        for name in ["m_OneFrameSkillMaxNum","spellCastNormal","spellCastTrySrc"] {
                            let Some(field)=metadata.fields.get(name) else {continue;};
                            if field.is_static || ![18,21].contains(&field.kind) {continue;}
                            if let Ok(child)=source.reflection.pointer(*parent+field.offset) {
                                if child!=0 && source.reflection.pointer(child).and_then(|c|source.reflection.class(c)).is_ok() {
                                    children.push((format!("{role}.{name}"),child));
                                }
                            }
                        }
                    }
                    for (role,object) in &children {
                        if !role.ends_with("m_OneFrameSkillMaxNum") {continue;}
                        if let Ok(entries)=source.reflection.field(*object,"entries").and_then(|p|source.reflection.pointer(p)) {
                            if entries!=0 {
                                if let Ok(class)=source.reflection.pointer(entries).and_then(|c|source.reflection.pointer(c+0x40)).and_then(|c|source.reflection.class(c)) {
                                    let fields:Vec<_>=class.fields.iter().map(|(n,f)|json!({"name":n,"kind":f.kind,"offset":f.offset,"static":f.is_static})).collect();
                                    schema.push(json!({"role":format!("{role}.entry_type"),"class":class.name,"fields":fields}));
                                }
                            }
                        }
                    }
                    objects.extend(children);
                    for (role, object) in objects {
                        let mut class = source.reflection.pointer(object)?;
                        for _ in 0..16 {
                            if class == 0 {
                                break;
                            }
                            let c = source.reflection.class(class)?;
                            let mut fields: Vec<_> = c
                                .fields
                                .iter()
                                .filter(|(name, _)| {
                                    if role.ends_with("m_SkillCompCurrentSkillData") || role.ends_with("m_OneFrameSkillMaxNum") || role.ends_with("spellCastNormal") || role.ends_with("spellCastTrySrc") {return true;}
                                    let n = name.to_ascii_lowercase();
                                    [
                                        "skill", "aim", "drag", "touch", "press", "cast", "pos",
                                        "dir", "cancel", "release", "target", "dst", "result",
                                        "req", "point", "guid", "uid",
                                        "state", "lock", "busy", "enable", "disable", "action", "anim", "time", "can",
                                    ]
                                    .iter()
                                    .any(|needle| n.contains(needle))
                                })
                                .map(|(n, v)| json!({"name":n,"kind":v.kind,"offset":v.offset,"static":v.is_static}))
                                .collect();
                            fields.sort_by_key(|v| v["name"].as_str().unwrap_or("").to_owned());
                            schema.push(json!({"role":role,"class":c.name,"fields":fields}));
                            class = c.parent;
                        }
                    }
                    let path = output.with_extension("fields.json");
                    serde_json::to_writer_pretty(
                        File::create(path).map_err(|e| e.to_string())?,
                        &schema,
                    )
                    .map_err(|e| e.to_string())?;
                    schema_written = true;
                }
                let cast_receipt = receipt(&mut source, s.match_owners[2], s.match_owners[3])
                    .map_err(|e| e.to_string());
                let variants: Vec<_> = f.skill_state.buttons.iter().take(f.skill_state.count as usize)
                    .map(|v| json!({
                        "slot": v.slot.as_str(), "base": v.base_skill_id.int(), "active": v.active_skill_id.int(),
                        "cast_locked": v.cast_locked.is_bool().then(|| v.cast_locked.get()),
                        "state_forbidden": v.state_forbidden.is_bool().then(|| v.state_forbidden.get()),
                    })).collect();
                let cooldowns:Vec<_>=f.ui.cooldown_state.buttons.iter().take(f.ui.cooldown_state.count.min(32) as usize).map(|v|json!({"slot":v.slot.as_str(),"base":v.base_skill_id.int(),"active":v.active_skill_id.int(),"ready":v.ready.get(),"on_cooldown":v.on_cooldown.get()})).collect();
                let mut row=json!({"cooldowns_ready":f.ui.cooldown_state.ready.get(),"cooldowns":cooldowns,"native_values":native_values,"control_types":s.control_types,"native_identity":native_identity,"drag_gate":drag_gate,"cast_receipt":cast_receipt,"attempt":attempt,"ok":true,"start_ns":begin,"end_ns":now_ns(),"sample_end_ns":f.time_ns.value,"read_ms":f.sample_duration_ms.value,"clock_ms":s.game_clock_ms,"revision":s.plan_revision,"hero":f.hero_id.int(),"skill":f.skill_id.int(),"active_slot":f.ui.active_slot.as_str(),"hero_render":f.hero_render_xyz.values.map(|v|v.value),"hero_logic":s.logic,"hero_uv":f.hero_screen_uv.values.map(|v|v.value),"aim_world":s.aim,"joystick_ready":f.ui.joystick.ready.get(),"center_uv":f.ui.joystick.center_uv.values.map(|v|v.value),"radius_uv":f.ui.joystick.radius_uv.values.map(|v|v.value),"logical_ready":f.logical.ready.get(),"input_ready":f.logical.input_ready.get(),"valid_drag":f.logical.valid_drag.get(),"first_drag":f.logical.first_drag.get(),"native_source":f.logical.source.as_str(),"view":f.view_matrix.values.map(|v|v.value),"projection":s.projection,"world_range":f.logical.world_range.value,"drag_ratio":f.logical.drag_ratio.value,"native_offset":f.logical.offset_xz.values.map(|v|v.value),"native_target":f.logical.target_render_xyz.values.map(|v|v.value),"raw_touch":f.motion.drag_touch_raw_xyz.values.map(|v|v.value),"direction_ready":f.direction_input.ready.get(),"direction_raw_touch":f.direction_input.raw_touch_xyz.values.map(|v|v.value),"native_direction":f.direction_input.direction_xz.values.map(|v|v.value),"variants":variants});
                row["linked"]=json!(s.linked);
                row["direction_visual"]=json!(s.direction_visual);
                row["direction_source"]=json!(f.direction_input.source.as_str());
                row["cancel_geometry"]=json!({"valid":f.ui.cancel_uv.valid(),"point":f.ui.cancel_uv.values.map(|v|v.value)});
                row["control_state"]=json!(control_state);
                let cancel = &f.cancel_state;
                row["cancel_state"] = json!({
                    "ready": cancel.ready.get(), "slot": cancel.slot.as_str(),
                    "skill": cancel.base_skill_id.int(), "cancelled": cancel.cancelled.get(),
                    "sample_start_ns": cancel.sample_start_ns.value,
                });
                row["ui_input"]=json!(ui_input);row["ui_buttons"]=json!(ui_buttons);
                if f.hero_id.int()==84 {row["local_geometry"]=json!(source.audit_local_geometry());}
                row
            }
            Err(error) => {
                *errors.entry(error.clone()).or_default() += 1;
                json!({"attempt":attempt,"ok":false,"start_ns":begin,"end_ns":now_ns(),"error":error})
            }
        };
        serde_json::to_writer(&mut writer, &row).map_err(|e| e.to_string())?;
        writer.write_all(b"\n").map_err(|e| e.to_string())?;
        let wait = Duration::from_micros(interval_us).saturating_sub(entry.elapsed());
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
    }
    writer.flush().map_err(|e| e.to_string())?;
    identity(pid, birth, &exe)?;
    durations.sort_by(f64::total_cmp);
    let percentile = |p: f64| {
        durations
            .get(((durations.len().saturating_sub(1)) as f64 * p) as usize)
            .copied()
    };
    let summary = json!({"complete":true,"pid":pid,"birth_ticks":birth,"attempts":attempts,"valid":valid,"rejected":attempts-valid,"distinct_battle_clocks":clocks.len(),"elapsed_s":started.elapsed().as_secs_f64(),"read_p50_ms":percentile(0.5),"read_p95_ms":percentile(0.95),"read_p99_ms":percentile(0.99),"read_max_ms":durations.last(),"errors":errors,"scope":"Read-only known mapper source; duplicate game clocks are not independent game updates; no accuracy claim from count alone"});
    serde_json::to_writer_pretty(
        File::create(output.with_extension("summary.json")).map_err(|e| e.to_string())?,
        &summary,
    )
    .map_err(|e| e.to_string())?;
    println!("{summary}");
    Ok(())
}
