//! Read-only owned Ling indicator contracts for both ground and wall forms.
use clbb_mapper::{platform::linux::{ProcessMemory,now_ns},source::Source,aim::MotionPlan};
use serde_json::json;
fn main()->Result<(),String>{let a:Vec<_>=std::env::args().collect();let p=a[1].parse().map_err(|_|"pid")?;let mut s=Source::new(ProcessMemory::diagnostic(p)?,format!("ling-audit:{p}"))?;let mut f=None;
for _ in 0..100{match s.sample(now_ns(),[4000.,3111.]){Ok(v)=>{f=Some(v);break},Err(_)=>std::thread::sleep(std::time::Duration::from_millis(5))}}
let f=f.ok_or("no coherent frame")?;if f.frame.hero_id.int()!=84{return Err("not Ling".into())}let mut ids=serde_json::Map::new();
for id in [8410,8420,8425,8430]{ids.insert(id.to_string(),json!({"identity":MotionPlan::audit_registry_identity(&mut s.reflection,f.indicator,id),"values":MotionPlan::audit_registry_values(&mut s.reflection,f.indicator,id)}));}
std::fs::write(&a[2],serde_json::to_vec_pretty(&json!({"contracts":ids,"hero":84,"types":f.control_types,"hero_render":f.frame.hero_render_xyz.values.map(|v|v.value)})).unwrap()).map_err(|e|e.to_string())?;Ok(())}
