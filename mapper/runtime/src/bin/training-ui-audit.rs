// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded read-only inventory of active game UI; never sends input or changes focus.
use clbb_mapper::{
    platform::linux::ProcessMemory,
    reflection::{Reflection, Result, Class},
    unity::{self, Widget, Path},
    math,
};
use serde_json::{Value, json};
use std::{collections::{BTreeMap, BTreeSet, VecDeque}, path::PathBuf, time::{Duration, Instant}};

fn schema(r: &mut Reflection<ProcessMemory>, object: u64) -> Result<(String, BTreeMap<String, clbb_mapper::reflection::Field>)> {
    let mut class = r.class(r.pointer(object)?)?;
    let name = class.name.clone();
    let mut fields = BTreeMap::new();
    for _ in 0..20 {
        for (key, field) in class.fields { fields.entry(key).or_insert(field); }
        if class.parent == 0 { break; }
        class = r.class(class.parent)?;
    }
    Ok((name, fields))
}
fn managed_string(r: &mut Reflection<ProcessMemory>, string: u64) -> Result<String> {
    if r.class(r.pointer(string)?)?.name != "String" { return Err("not a managed string".into()); }
    let count = r.integer(string + 16)?;
    if !(0..=512).contains(&count) { return Err("UI text exceeds audit bound".into()); }
    let raw = r.read(string + 20, count as usize * 2)?;
    String::from_utf16(&raw.chunks_exact(2).map(|v|u16::from_le_bytes(v.try_into().unwrap())).collect::<Vec<_>>())
        .map_err(|_|"invalid UI UTF-16".into())
}
fn camera_matrices(r: &mut Reflection<ProcessMemory>, camera:u64) -> Option<([f64;16],[f64;16])> {
    if r.class(r.pointer(camera).ok()?).ok()?.name != "Camera" { return None; }
    let native = r.link(camera,"m_CachedPtr").ok()?;
    let matrix = native + unity::ui_matrix_offset(&r.read(native,4096).ok()?).ok()? as u64;
    unity::matrices(&r.read(matrix,128).ok()?).ok()
}
fn widget(r: &mut Reflection<ProcessMemory>, object: u64) -> Option<Value> {
    let w = Widget::new(r,object).ok()?;
    let visible = w.visible(r).ok();
    let camera = r.link(object,"mCam").ok().filter(|v|*v!=0)
        .or_else(||r.link(object,"s_CameraOrthographicSizeOwner").ok().filter(|v|*v!=0))?;
    let (view,projection)=camera_matrices(r,camera)?;
    let pivot=w.pivot.is_some()&&w.region.is_some();
    let (center,radius)=w.geometry(r,&view,&projection,pivot).ok()?;
    Some(json!({"visible":visible,"center_uv":center,"radius_uv":radius,"onscreen":unity::onscreen(center)}))
}
fn interesting(name:&str)->bool {
    let name=name.to_ascii_lowercase();
    ["hero","item","label","text","button","info","sprite","scroll","grid","cell","choose","select","title","name","content","train","list","tog","ok","confirm","model"].iter().any(|part|name.contains(part))
}
fn birth(pid:u32)->Result<String> {
    let stat=std::fs::read_to_string(format!("/proc/{pid}/stat")).map_err(|e|e.to_string())?;
    let fields=stat.rsplit_once(')').ok_or("invalid process identity")?.1.split_whitespace().collect::<Vec<_>>();
    if matches!(fields.first().copied(),Some("Z"|"X"|"x")) {return Err("runtime is not live".into());}
    fields.get(19).map(|v|v.to_string()).ok_or("process birth absent".into())
}
fn run()->Result<()> {
    let args=std::env::args().collect::<Vec<_>>();
    if !(3..=4).contains(&args.len()) {return Err("usage: training-ui-audit PID OUTPUT [FRAME_SUBSTRING]; no input is sent".into());}
    let pid:u32=args[1].parse().map_err(|_|"invalid pid")?;
    let root=PathBuf::from(std::env::var_os("CLBB_STATE_ROOT").unwrap_or_else(||std::env::current_dir().unwrap().into_os_string()));
    let manifest:Value=serde_json::from_slice(&std::fs::read(root.join("logs/current-atl-pid.json")).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    if manifest["pid"].as_u64()!=Some(pid as u64) {return Err("PID does not match this checkout's recorded runtime".into());}
    // Share exactly the release-workbench ownership check, including selected
    // candidate manifest/hash validation; a matching executable basename is not enough.
    let owned = std::process::Command::new("python3")
        .arg("-c")
        .arg("import pathlib,sys; root=pathlib.Path(sys.argv[1]); sys.path.insert(0,str(root/'tools')); from runtime_identity import runtime_executable_is_owned; sys.exit(0 if runtime_executable_is_owned(int(sys.argv[2]),root) else 1)")
        .arg(&root).arg(pid.to_string()).status().map_err(|e|e.to_string())?;
    if !owned.success(){return Err("Recorded runtime executable is not the selected owned build".into());}
    let initial_birth=birth(pid)?;
    let executable=std::fs::read_link(format!("/proc/{pid}/exe")).map_err(|e|e.to_string())?;
    let mut r=Reflection::new(ProcessMemory::diagnostic(pid)?)?;
    r.discover(&["UIKeySkill"])?;
    let mut base:Class=r.class(r.named["UIKeySkill"])?;
    for _ in 0..20 {if base.name=="BaseFrame" {break;} if base.parent==0 {return Err("BaseFrame owner absent".into());} base=r.class(base.parent)?;}
    if base.name!="BaseFrame" {return Err("BaseFrame inheritance changed".into());}
    let field=base.fields.get("m_ActiveFrameList").ok_or("active frame field absent")?;
    if !field.is_static||base.statics==0{return Err("active frame owner invalid".into());}
    let at=base.statics+field.offset;
    let list=r.pointer(at)?;
    let frames=r.list(list)?;
    let ui_camera = frames.iter().copied().filter(|v|*v!=0).find(|v|r.pointer(*v).ok()==Some(r.named["UIKeySkill"]))
        .and_then(|v|r.link(v,"m_UIFrameInfo").ok())
        .and_then(|v|r.link(v,"m_sprite_SkillRange02").ok())
        .and_then(|v|r.link(v,"mCam").ok())
        .and_then(|v|camera_matrices(&mut r,v));
    let started=Instant::now();
    let filter=args.get(3).map(|v|v.to_ascii_lowercase());
    let mut roots=Vec::new();let mut queue=VecDeque::new();
    for object in &frames {
        if *object==0 {continue;}
        let Ok((name,fields))=schema(&mut r,*object) else {continue};
        roots.push(json!({"object":object,"class":name,"fields":fields.iter().map(|(k,v)|json!({"name":k,"kind":v.kind})).collect::<Vec<_>>()}));
        if filter.as_ref().is_some_and(|f|name.to_ascii_lowercase().contains(f)) {
            queue.push_back((*object,name,0usize));
        }
    }
    let mut seen=BTreeSet::new();let mut objects=Vec::new();
    while let Some((object,path,depth))=queue.pop_front() {
        if seen.len()>=768||started.elapsed()>Duration::from_secs(8){break;}
        if object==0||!seen.insert(object){continue;}
        let Ok((name,fields))=schema(&mut r,object) else{continue};
        let mut values=serde_json::Map::new();
        let geometry=widget(&mut r,object);
        for (key,field) in fields {
            if field.is_static||field.offset>65536||(!interesting(&key)&&depth>0){continue;}
            let address=object+field.offset;
            if matches!(field.kind,14|18|21|29){
                let Ok(child)=r.pointer(address) else{continue};
                if child==0 {continue;}
                let Ok(child_class)=r.class(r.pointer(child).unwrap_or(0)) else{continue};
                if child_class.name=="String" {
                    if let Ok(text)=managed_string(&mut r,child){values.insert(key,json!(text));}
                } else if depth<5 {
                    if child_class.name.starts_with("List`")||child_class.name.starts_with("BetterList`") {
                        if let Ok(items)=r.list(child) {
                            values.insert(key.clone(),json!({"class":child_class.name,"count":items.len()}));
                            for (i,item)in items.into_iter().enumerate(){if item!=0{queue.push_back((item,format!("{path}.{key}[{i}]"),depth+1));}}
                        }
                    } else if interesting(&key)||interesting(&child_class.name)||depth==0 {
                        values.insert(key.clone(),json!({"class":child_class.name,"object":child}));
                        queue.push_back((child,format!("{path}.{key}"),depth+1));
                    }
                }
            } else if field.kind==8&&interesting(&key) {
                if let Ok(v)=r.integer(address){values.insert(key,json!(v));}
            } else if field.kind==2&&["active","enable","show","visible","choose","select"].iter().any(|part|key.to_ascii_lowercase().contains(part)) {
                if let Ok(raw)=r.read(address,1){if raw[0]<=1{values.insert(key,json!(raw[0]==1));}}
            }
        }
        let anchor = ui_camera.as_ref().and_then(|(view,projection)| {
            let component=name!="GameObject";
            let position=Path::new(&mut r,object,component).ok()?.position(&r).ok()?;
            math::project(view,projection,position,false)
        });
        objects.push(json!({"path":path,"object":object,"class":name,"values":values,"widget":geometry,"anchor_uv":anchor}));
    }
    if birth(pid)?!=initial_birth||std::fs::read_link(format!("/proc/{pid}/exe")).map_err(|e|e.to_string())?!=executable{return Err("runtime identity changed".into());}
    let topology_unchanged=r.pointer(at)?==list&&r.list(list)?==frames;
    let result=json!({"pid":pid,"process_birth":initial_birth,"frame_filter":filter,"frames":roots,"objects":objects,"topology_unchanged":topology_unchanged,"bounded":true,"visited":seen.len(),"truncated":!queue.is_empty(),"elapsed_after_discovery_ms":started.elapsed().as_secs_f64()*1000.,"scope":"read-only UI labels and geometry; not a coherent input receipt"});
    std::fs::write(&args[2],serde_json::to_vec_pretty(&result).unwrap()).map_err(|e|e.to_string())?;
    println!("{}",json!({"output":args[2],"frames":roots.len(),"objects":objects.len(),"topology_unchanged":topology_unchanged}));
    Ok(())
}
fn main(){if let Err(error)=run(){eprintln!("training-ui-audit: {error}");std::process::exit(1);}}
