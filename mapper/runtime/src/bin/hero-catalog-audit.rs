// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded read-only discovery of game-owned hero/skill configuration metadata.
//! Does not send input, invoke game methods, or inspect account/network state.
use clbb_mapper::{platform::linux::ProcessMemory, reflection::{Class, Reflection}};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::PathBuf};
fn identity(pid:u32,birth:u64,exe:&PathBuf)->Result<(),String>{
    let root=PathBuf::from(format!("/proc/{pid}"));
    let stat=std::fs::read_to_string(root.join("stat")).map_err(|e|e.to_string())?;
    let fields:Vec<_>=stat.rsplit_once(')').ok_or("Invalid stat")?.1.split_whitespace().collect();
    if fields.get(19).and_then(|v|v.parse::<u64>().ok())!=Some(birth)
        || matches!(fields.first().copied(),Some("Z"|"X"|"x"))
        || std::fs::read_link(root.join("exe")).map_err(|e|e.to_string())?!=*exe {
        return Err("Owned runtime identity changed".into());
    }Ok(())
}
fn schema(class:&Class)->Value{
    let fields:BTreeMap<_,_>=class.fields.iter().map(|(name,f)|(name,json!({"kind":f.kind,"static":f.is_static,"offset":f.offset}))).collect();
    json!({"name":class.name,"address":class.address,"parent":class.parent,"statics":class.statics,"fields":fields})
}
fn managed_string(r:&mut Reflection<ProcessMemory>,p:u64)->Result<String,String>{
    if p==0{return Ok(String::new());}
    if r.class(r.pointer(p)?)?.name!="String" {return Err("Expected configuration string".into());}
    let n=r.integer(p+16)?;
    if !(0..=4096).contains(&n){return Err("Configuration string exceeds bound".into());}
    let b=r.read(p+20,n as usize*2)?;
    String::from_utf16(&b.chunks_exact(2).map(|b|u16::from_le_bytes(b.try_into().unwrap())).collect::<Vec<_>>()).map_err(|e|e.to_string())
}
fn config_value(r:&mut Reflection<ProcessMemory>,object:u64,name:&str,kind:u8)->Result<Value,String>{
    let at=r.field(object,name)?;
    match kind {
        8=>Ok(json!(r.integer(at)?)),
        12=>Ok(json!(r.float(at)?)),
        14=>{let p=r.pointer(at)?;Ok(json!(managed_string(r,p)?))},
        29=>{
            let p=r.pointer(at)?;if p==0{return Ok(Value::Null);}
            let class=r.pointer(p)?;let n=r.pointer(p+24)?;
            if n>256{return Err("Config array exceeds bound".into());}
            let class_name=r.cstring(r.pointer(class+16)?)?;
            let elem=r.class(r.pointer(class+0x40)?)?.name;
            if !["Int32","Single","String"].contains(&elem.as_str()) {return Ok(json!({"array_type":class_name,"length":n}));}
            let stride=if elem=="String" {8} else {4};
            let b=r.read(p+32,n as usize*stride)?;let mut out=Vec::new();
            for v in b.chunks_exact(stride) {
                out.push(match elem.as_str(){
                    "Int32"=>json!(i32::from_le_bytes(v.try_into().unwrap())),
                    "Single"=>json!(f32::from_le_bytes(v.try_into().unwrap())),
                    _=>json!(managed_string(r,u64::from_le_bytes(v.try_into().unwrap()))?),
                });
            }
            Ok(json!({"array_type":class_name,"values":out}))
        },
        _=>Err("Unsupported configuration field".into()),
    }
}
fn table_rows(r:&mut Reflection<ProcessMemory>,name:&str)->Result<Vec<Value>,String>{
    let root_at=r.static_field(name,"m_Instance")?;let root=r.pointer(root_at)?;
    let shared=r.link(root,"m_Data")?;
    if r.class(r.pointer(shared)?)?.name!="SharedDictionary`2"{return Err("Unexpected configuration collection".into());}
    let dict=r.link(shared,"m_dic")?;
    let count_at=r.field(dict,"count")?;let count=r.integer(count_at)?;
    if !(0..=20000).contains(&count){return Err("Configuration row count exceeds bound".into());}
    let entries_at=r.field(dict,"entries")?;let array=r.pointer(entries_at)?;
    let arrayclass=r.pointer(array)?;let capacity=r.pointer(array+24)?;
    if capacity<count as u64 || capacity>32768{return Err("Configuration capacity exceeds bound".into());}
    let entry=r.class(r.pointer(arrayclass+0x40)?)?;
    let fields=&entry.fields;
    for (f,kind) in [("hashCode",8),("next",8),("key",8),("value",18)] {
        let field=fields.get(f).ok_or("Missing configuration entry field")?;
        if field.is_static || (field.kind!=kind && !(f=="value" && [21,28].contains(&field.kind))) {return Err(format!("Configuration entry field changed: {f} kind {} static {} entry {:?}",field.kind,field.is_static,entry));}
    }
    if fields.len()!=4{return Err("Configuration entry layout changed".into());}
    let bias=fields.values().map(|f|f.offset).min().ok_or("Empty entry")?;
    let offsets:BTreeMap<_,_>=fields.iter().map(|(n,f)|(n.as_str(),(f.offset-bias) as usize)).collect();
    let stride=offsets.iter().map(|(n,o)|o+if *n=="value"{8}else{4}).max().ok_or("Empty layout")?;
    if !(16..=64).contains(&stride)||stride%8!=0||offsets["hashCode"]!=0{return Err("Invalid configuration entry stride".into());}
    let raw=r.read(array+32,count as usize*stride)?;
    let expected=format!("{name}_Element");let mut out=Vec::new();
    let names=if name=="CData_Hero"{
        vec!["m_ID","m_mName","m_SkillList","m_SkillsShowType1","m_SkillsShowType2","m_SkillsShowType3"]
    }else{
        vec!["m_SkillID","m_SkillLevel","m_SkillName","m_RectType","m_ShowRectPara","m_ShowRectParaSpecial","m_BaseType","m_SpecialParam","m_NeedMP","m_LockTime","m_FinishLockTime"]
    };
    for row in raw.chunks_exact(stride){
        if clbb_mapper::reflection::i32_at(row,offsets["hashCode"])<0{continue;}
        let key=clbb_mapper::reflection::i32_at(row,offsets["key"]);
        let object=clbb_mapper::reflection::u64_at(row,offsets["value"]);
        if object==0{continue;}
        let owner=r.pointer(object)?;let class=r.class(owner)?;
        if class.name!=expected{return Err("Unexpected configuration element owner".into());}
        let mut values=BTreeMap::new();
        for field in &names{
            if let Some(f)=class.fields.get(*field){values.insert(*field,config_value(r,object,field,f.kind));}
        }
        if r.pointer(object)?!=owner{return Err("Configuration element owner changed".into());}
        out.push(json!({"key":key,"class":class.name,"values":values}));
    }
    if r.integer(count_at)?!=count||r.pointer(entries_at)?!=array||r.pointer(array)?!=arrayclass{return Err("Configuration collection changed".into());}
    Ok(out)
}
fn main()->Result<(),String>{
    let args:Vec<_>=std::env::args().collect();
    if args.len()!=4{return Err("Usage: hero-catalog-audit PID START_TICKS OUTPUT.json".into());}
    let pid:u32=args[1].parse().map_err(|_|"Invalid PID")?;
    let birth:u64=args[2].parse().map_err(|_|"Invalid start ticks")?;
    let exe=std::fs::read_link(format!("/proc/{pid}/exe")).map_err(|e|e.to_string())?;
    identity(pid,birth,&exe)?;
    let mut r=Reflection::new(ProcessMemory::diagnostic(pid)?)?;
    let names=["CData_Hero","CData_Hero_Element","CData_Skill","CData_Skill_Element"];
    r.discover(&names)?;
    let heroes=table_rows(&mut r,"CData_Hero");let skills=table_rows(&mut r,"CData_Skill");
    let mut classes=Vec::new();let mut statics=Vec::new();let mut instances=Vec::new();
    for name in names {
        let class=r.class(r.named[name])?;
        classes.push(schema(&class));
        if ["CData_Hero","CData_Skill"].contains(&name) {
            let root=r.static_field(name,"m_Instance").and_then(|a|r.pointer(a))?;
            let mut current=class.clone();let mut ancestors=Vec::new();let mut fields=Vec::new();
            for _ in 0..16 {
                ancestors.push(schema(&current));
                for (field,f) in &current.fields {
                    if f.is_static || f.offset>1_048_576 {continue;}
                    let value=match f.kind {
                        2=>r.boolean(root+f.offset).map(|v|json!(v)),
                        8=>r.integer(root+f.offset).map(|v|json!(v)),
                        18|21|29=>r.pointer(root+f.offset).and_then(|p|{
                            if p==0 {return Ok(Value::Null);}
                            let cls=r.class(r.pointer(p)?)?;
                            Ok(json!({"object":p,"class":schema(&cls)}))
                        }),
                        _=>continue,
                    };
                    fields.push(json!({"name":field,"kind":f.kind,"value":value}));
                }
                if current.parent==0 {break;}current=r.class(current.parent)?;
            }
            instances.push(json!({"class":name,"object":root,"ancestors":ancestors,"fields":fields}));
        }
        for (field,f) in &class.fields {
            if !f.is_static || class.statics==0 || f.offset>1_048_576 {continue;}
            let address=class.statics+f.offset;
            let value=match f.kind {
                2=>r.boolean(address).map(|v|json!(v)),
                8=>r.integer(address).map(|v|json!(v)),
                18|21=>r.pointer(address).and_then(|p|{
                    if p==0 {return Ok(Value::Null);}
                    let cls=r.class(r.pointer(p)?)?;
                    Ok(json!({"object":p,"class":schema(&cls)}))
                }),
                _=>continue,
            };
            statics.push(json!({"owner":name,"field":field,"kind":f.kind,"value":value}));
        }
    }
    identity(pid,birth,&exe)?;
    let output=json!({"pid":pid,"start_ticks":birth,"executable":exe,"classes":classes,"statics":statics,"instances":instances,"heroes":heroes,"skills":skills});
    std::fs::write(&args[3],serde_json::to_vec_pretty(&output).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    println!("Read-only configuration metadata saved: {}",args[3]);Ok(())
}
