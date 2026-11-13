// SPDX-License-Identifier: GPL-3.0-or-later
//! Owned training diagnostics only: read live battle entity metadata, no input/writes.
use clbb_mapper::reflection::{Reflection, f32_at, i32_at, u64_at};
use clbb_mapper::{
    platform::linux::{ProcessMemory, now_ns},
    source::Source,
};
use serde_json::json;
use std::{fs::OpenOptions, io::Write, path::PathBuf};
fn coherent(source: &mut Source<ProcessMemory>) -> Result<clbb_mapper::source::Sample, String> {
    let started = std::time::Instant::now();
    let mut last = String::new();
    for _ in 0..40 {
        match source.sample(now_ns(), [4000., 3111.]) {
            Ok(s) => return Ok(s),
            Err(e) => last = e,
        };
        if started.elapsed() > std::time::Duration::from_secs(3) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    Err(format!("No coherent current battle sample: {last}"))
}
fn typed(
    r: &mut Reflection<ProcessMemory>,
    object: u64,
    name: &str,
    kind: u8,
) -> Result<u64, String> {
    let mut c = r.class(r.pointer(object)?)?;
    for _ in 0..24 {
        if let Some(f) = c.fields.get(name) {
            if f.is_static || f.kind != kind || f.offset > 65536 {
                return Err(format!("Unexpected field layout:{name}"));
            }
            return object
                .checked_add(f.offset)
                .ok_or_else(|| "Field overflow".into());
        }
        if c.parent == 0 {
            break;
        }
        c = r.class(c.parent)?;
    }
    Err(format!("Field missing:{name}"))
}
fn identity(pid: u32) -> Result<(u64, PathBuf), String> {
    let root = PathBuf::from(format!("/proc/{pid}"));
    let stat = std::fs::read_to_string(root.join("stat")).map_err(|e| e.to_string())?;
    let fields: Vec<_> = stat
        .rsplit_once(')')
        .ok_or("Malformed process stat")?
        .1
        .split_whitespace()
        .collect();
    if matches!(fields.first().copied(), Some("Z" | "X" | "x")) {
        return Err("Game exited".into());
    }
    let birth = fields
        .get(19)
        .ok_or("Missing birth identity")?
        .parse::<u64>()
        .map_err(|_| "Invalid birth identity")?;
    Ok((
        birth,
        std::fs::read_link(root.join("exe")).map_err(|e| e.to_string())?,
    ))
}
fn show_entity_class(r: &mut Reflection<ProcessMemory>, address: u64) -> Result<(), String> {
    let mut c = r.class(address)?;
    for _ in 0..24 {
        if c.name == "ShowEntity" {
            return Ok(());
        }
        if c.parent == 0 {
            break;
        }
        c = r.class(c.parent)?;
    }
    Err("Expected ShowEntity reference element".into())
}
fn players(
    r: &mut Reflection<ProcessMemory>,
    battle: u64,
) -> Result<Vec<serde_json::Value>, String> {
    let mut actors = Vec::new();
    for container in ["m_ShowPlayers", "m_ShowPlayerSummons"] {
        let list_at = typed(r, battle, container, 21)?;
        let list = r.pointer(list_at)?;
        if r.class(r.pointer(list)?)?.name != "List`1" {
            return Err("Player list class changed".into());
        }
        let head = r.read(list + 16, 16)?;
        let array = u64_at(&head, 0);
        let count = i32_at(&head, 8);
        if !(0..=256).contains(&count) || array == 0 {
            return Err("Player list bounds changed".into());
        }
        let array_header = r.read(array, 32)?;
        let capacity = u64_at(&array_header, 24);
        if capacity < count as u64 || capacity > 4096 {
            return Err("Player array capacity changed".into());
        }
        let array_class = u64_at(&array_header, 0);
        show_entity_class(r, r.pointer(array_class + 0x40)?)?;
        let players = r.list(list)?;
        for object in players.into_iter().filter(|v| *v != 0) {
            let actor_start = now_ns();
            let result = (|| -> Result<serde_json::Value, String> {
                let class = r.class(r.pointer(object)?)?;
                show_entity_class(r, class.address)?;
                let pos = typed(r, object, "m_vCachePosition", 17)?;
                let before = r.read(pos, 12)?;
                let world = [f32_at(&before, 0), f32_at(&before, 4), f32_at(&before, 8)];
                if world.iter().any(|v| !v.is_finite() || v.abs() > 100000.) {
                    return Err("Invalid rendered position".into());
                }
                let id_at = typed(r, object, "m_ID", 8)?;
                let guid_at = typed(r, object, "m_uGuid", 9)?;
                let hp_at = typed(r, object, "m_Hp", 8)?;
                let camp_at = typed(r, object, "m_bSameCampType", 2)?;
                let id = r.integer(id_at)?;
                let guid = r.integer(guid_at)? as u32;
                let hp = r.integer(hp_at)?;
                let same_camp = r.boolean(camp_at)?;
                if before != r.read(pos, 12)?
                    || class.address != r.pointer(object)?
                    || id != r.integer(id_at)?
                    || guid != r.integer(guid_at)? as u32
                    || hp != r.integer(hp_at)?
                    || same_camp != r.boolean(camp_at)?
                {
                    return Err("Entity moved or changed during observation".into());
                }
                Ok(
                    json!({"object":object,"class":class.name,"id":id,"guid":guid,"hp":hp,"same_camp":same_camp,"render_world":world,"coherent":true,"start_ns":actor_start,"end_ns":now_ns()}),
                )
            })();
            actors.push(json!({"object":object,"container":container,"observation":result}));
        }
        if head != r.read(list + 16, 16)? || list != r.pointer(list_at)? {
            return Err("Player list topology changed".into());
        }
    }
    Ok(actors)
}
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 && args.len() != 5 {
        return Err(
            "Usage: training-actors-audit PID NEW_OUTPUT.json [--watch-ms 100..10000]".into(),
        );
    }
    let duration = if args.len() == 5 {
        if args[3] != "--watch-ms" {
            return Err("Unknown watch option".into());
        }
        let ms: u64 = args[4].parse().map_err(|_| "Invalid watch duration")?;
        if !(100..=10000).contains(&ms) {
            return Err("Watch duration out of range".into());
        }
        Some(ms)
    } else {
        None
    };
    let pid: u32 = args[1].parse().map_err(|_| "Invalid PID")?;
    if pid == 0 {
        return Err("PID must be positive".into());
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    let owned=std::process::Command::new("python3").args(["-c","import pathlib,sys; root=pathlib.Path(sys.argv[1]); sys.path.insert(0,str(root/'tools')); from runtime_identity import runtime_executable_is_owned; sys.exit(0 if runtime_executable_is_owned(int(sys.argv[2]),root) else 1)"]).arg(&root).arg(pid.to_string()).status().map_err(|e|e.to_string())?;
    if !owned.success() {
        return Err("Not this checkout's selected owned runtime".into());
    }
    let expected = identity(pid)?;
    let mut source = Source::new(
        ProcessMemory::diagnostic(pid)?,
        format!("training-actors:{pid}"),
    )?;
    let sample = coherent(&mut source)?;
    let start = now_ns();
    let mut objects = Vec::new();
    for (role, object) in ["battle", "manager", "local_show", "local_actor"]
        .into_iter()
        .zip(sample.match_owners)
    {
        let r = &mut source.reflection;
        let mut class = r.class(r.pointer(object)?)?;
        let name = class.name.clone();
        let mut fields = Vec::new();
        for _ in 0..24 {
            for (key, f) in &class.fields {
                if f.is_static {
                    continue;
                }
                let mut row =
                    json!({"name":key,"kind":f.kind,"offset":f.offset,"declaring":class.name});
                if [18, 21, 29].contains(&f.kind)
                    && ["actor", "entity", "player"]
                        .iter()
                        .any(|p| key.to_ascii_lowercase().contains(p))
                {
                    if let Ok(child) = r.pointer(object + f.offset) {
                        if child != 0 {
                            if let Ok(c) = r.pointer(child).and_then(|v| r.class(v)) {
                                row["reference"] = json!({"object":child,"class":c.name,"fields":c.fields.iter().map(|(k,v)|json!({"name":k,"kind":v.kind,"offset":v.offset,"static":v.is_static})).collect::<Vec<_>>()});
                            }
                        }
                    }
                }
                fields.push(row);
            }
            if class.parent == 0 {
                break;
            }
            class = r.class(class.parent)?;
        }
        objects.push(json!({"role":role,"object":object,"class":name,"fields":fields}));
    }
    let actors = players(&mut source.reflection, sample.match_owners[0])?;
    let final_sample = coherent(&mut source)?;
    if sample.match_owners != final_sample.match_owners
        || sample.frame.hero_id.int() != final_sample.frame.hero_id.int()
    {
        return Err("Current match identity changed".into());
    }
    if identity(pid)? != expected {
        return Err("Runtime identity changed".into());
    }
    let value = json!({"format":"clbb-training-actors-schema","version":1,"pid":pid,"birth":expected.0,"executable":expected.1,"hero":sample.frame.hero_id.int(),"owners":sample.match_owners,"start_ns":start,"end_ns":now_ns(),"same_match":true,"objects":objects,"actors":actors,"scope":"Read-only owned live ShowPlayers and typed rendered positions; rows with coherence errors are not targets. No input or writes; not production authorization"});
    let raw = if duration.is_some() {
        serde_json::to_vec(&value)
    } else {
        serde_json::to_vec_pretty(&value)
    }
    .map_err(|e| e.to_string())?;
    if raw.len() > 2 * 1024 * 1024 {
        return Err("Schema exceeds bound".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])
        .map_err(|e| e.to_string())?;
    file.write_all(&raw)
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|e| e.to_string())?;
    if let Some(ms) = duration {
        file.flush().map_err(|e| e.to_string())?;
        println!("READY hero={} pid={pid}", sample.frame.hero_id.int());
        std::io::stdout().flush().map_err(|e| e.to_string())?;
        let started = std::time::Instant::now();
        let mut snapshots = 0u64;
        while started.elapsed() < std::time::Duration::from_millis(ms) {
            if identity(pid)? != expected {
                return Err("Runtime identity changed during watch".into());
            }
            let current = coherent(&mut source)?;
            if current.match_owners != sample.match_owners
                || current.frame.hero_id.int() != sample.frame.hero_id.int()
            {
                return Err("Match identity changed during actor watch".into());
            }
            let row_start = now_ns();
            let observed = players(&mut source.reflection, sample.match_owners[0]);
            let event = json!({"event":"actors","hero":sample.frame.hero_id.int(),"pid":pid,"start_ns":row_start,"end_ns":now_ns(),"game_clock_ms":current.game_clock_ms,"observation":observed});
            snapshots += 1;
            serde_json::to_writer(&mut file, &event).map_err(|e| e.to_string())?;
            file.write_all(b"\n")
                .and_then(|_| file.flush())
                .map_err(|e| e.to_string())?;
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        if identity(pid)? != expected {
            return Err("Runtime identity changed at watch end".into());
        }
        let last = coherent(&mut source)?;
        if last.match_owners != sample.match_owners
            || last.frame.hero_id.int() != sample.frame.hero_id.int()
        {
            return Err("Match changed at watch end".into());
        }
        serde_json::to_writer(&mut file,&json!({"event":"summary","complete":true,"snapshots":snapshots,"same_match":true,"end_ns":now_ns()})).map_err(|e|e.to_string())?;
        file.write_all(b"\n").map_err(|e| e.to_string())?;
    }
    println!(
        "hero={} same_match=true output={}",
        sample.frame.hero_id.int(),
        args[2]
    );
    Ok(())
}
