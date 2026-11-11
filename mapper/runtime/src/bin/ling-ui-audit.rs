//! Read-only inventory of owned live skill buttons, including duplicate slots.
use clbb_mapper::{platform::linux::ProcessMemory,reflection::Reflection};
use serde_json::json;
fn main()->Result<(),String>{
 let args:Vec<_>=std::env::args().collect();let pid=args.get(1).ok_or("pid")?.parse().map_err(|_|"pid")?;
 let mut r=Reflection::new(ProcessMemory::diagnostic(pid)?)?;r.discover(&["UIKeySkill"])?;
 let target=r.named["UIKeySkill"];let mut base=r.class(target)?;
 for _ in 0..16{if base.name=="BaseFrame"{break}base=r.class(base.parent)?;}
 if base.name!="BaseFrame"{return Err("frame inheritance".into())}
 let frames=r.pointer(base.statics+base.fields.get("m_ActiveFrameList").ok_or("frames")?.offset)?;
 let objects=r.list(frames)?;let controls:Vec<_>=objects.into_iter().filter(|o|*o!=0&&r.pointer(*o).ok()==Some(target)).collect();
 if controls.len()!=1{return Err("ambiguous live frame".into())}let object=controls[0];
 let at=r.field(object,"statusKeySkillList")?;let list=r.pointer(at)?;let buttons=r.list(list)?;let mut out=vec![];
 for button in buttons{if button==0{continue}let class=r.class(r.pointer(button)?)?;let mut fields=serde_json::Map::new();let mut schema=vec![];let mut c=class.clone();
 for _ in 0..16{for(name,f)in &c.fields{if f.is_static{continue}schema.push(json!({"name":name,"kind":f.kind}));let a=button+f.offset;
 if f.kind==18 && ["hit","wall","collid"].iter().any(|w|name.to_lowercase().contains(w)) {if let Ok(child)=r.pointer(a){if child!=0{if let Ok(cc)=r.pointer(child).and_then(|p|r.class(p)){fields.insert(format!("{name}.class"),json!(cc.name));fields.insert(format!("{name}.schema"),json!(cc.fields.keys().collect::<Vec<_>>()));}}}}
 if f.kind==17 && ["DownSKillResult","PressSKillResult","vPos","currentDirect"].contains(&name.as_str()) {if let Ok(raw)=r.read(a,24){fields.insert(format!("{name}.geometry_floats"),json!(raw.chunks_exact(4).map(|v|f32::from_le_bytes(v.try_into().unwrap())).collect::<Vec<_>>()));}}
 if f.kind==17 && ["hit","ray"].iter().any(|w|name.to_lowercase().contains(w)) {if let Ok(raw)=r.read(a,44){fields.insert(format!("{name}.raw_geometry_floats"),json!(raw.chunks_exact(4).map(|v|f32::from_le_bytes(v.try_into().unwrap())).collect::<Vec<_>>()));}}
 let v=match f.kind{2=>r.boolean(a).map(|v|json!(v)),8=>r.integer(a).map(|v|json!(v)),12=>r.float(a).map(|v|json!(v)),_=>continue};if let Ok(v)=v{fields.insert(name.clone(),v);}}if c.parent==0{break}c=r.class(c.parent)?;}
 let skill=r.field(button,"m_SkillData").and_then(|a|r.pointer(a)).and_then(|d|r.field(d,"m_InitSkillID")).and_then(|a|r.integer(a)).ok();
 out.push(json!({"object":button,"class":class.name,"skill":skill,"fields":fields,"schema":schema}));}
 let result=json!({"pid":pid,"controls":object,"buttons":out});std::fs::write(args.get(2).ok_or("output")?,serde_json::to_vec_pretty(&result).unwrap()).map_err(|e|e.to_string())?;Ok(())
}
