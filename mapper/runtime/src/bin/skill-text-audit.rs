// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded read-only skill descriptions from owned native configuration tables.
//! Raw strings are evidence only: never input authorization or a template rule.
use clbb_mapper::{platform::{Memory, linux::{ProcessMemory, now_ns}},
    reflection::{Reflection, Result, i32_at, u64_at}, source::{Checks, capture, check}};
use serde_json::{Value, json};
use std::{collections::{BTreeMap, BTreeSet}, fs::{self, OpenOptions}, io::Write,
    path::{Path, PathBuf}, process::Command, time::{Duration, Instant}};

fn identity(pid:u32)->Result<(u64,PathBuf)> {
    let root=PathBuf::from(format!("/proc/{pid}"));
    let stat=fs::read_to_string(root.join("stat")).map_err(|e|e.to_string())?;
    let fields:Vec<_>=stat.rsplit_once(')').ok_or("Invalid process stat")?.1.split_whitespace().collect();
    if matches!(fields.first().copied(),Some("Z"|"X"|"x")){return Err("Runtime has exited".into());}
    Ok((fields.get(19).ok_or("Missing process birth")?.parse().map_err(|_|"Invalid process birth")?,
        fs::read_link(root.join("exe")).map_err(|e|e.to_string())?))
}
fn owned(root:&Path,pid:u32)->Result<()> {
    let selected:Value=serde_json::from_slice(&fs::read(root.join("logs/current-atl-pid.json")).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    if selected["pid"].as_u64()!=Some(pid as u64){return Err("PID is not this checkout's selected runtime".into());}
    let result=Command::new("python3").args(["-c","import sys;from pathlib import Path;sys.path.insert(0,sys.argv[1]);from runtime_identity import runtime_executable_is_owned;sys.exit(0 if runtime_executable_is_owned(int(sys.argv[2]),Path(sys.argv[3])) else 1)"])
        .arg(root.join("tools")).arg(pid.to_string()).arg(root).status().map_err(|e|e.to_string())?;
    if result.success(){Ok(())}else{Err("Runtime executable is not an owned selected build".into())}
}
fn ids(value:&str)->Result<BTreeSet<i32>> {
    let parts:Vec<_>=value.split(',').collect();
    if parts.is_empty()||parts.len()>64{return Err("Expected1..64 distinct native skill IDs".into());}
    let mut out=BTreeSet::new();
    for part in parts {
        if part.is_empty()||!part.bytes().all(|b|b.is_ascii_digit()){return Err("Expected comma-separated positive skill IDs".into());}
        let value:i32=part.parse().map_err(|_|"Invalid native skill ID")?;
        if !(1..=10_000_000).contains(&value)||!out.insert(value){return Err("Native skill IDs must be distinct and in1..10000000".into());}
    }
    Ok(out)
}
fn typed_field<M:Memory>(r:&mut Reflection<M>,checks:&mut Checks,object:u64,name:&str,kinds:&[u8])->Result<(u64,u8)> {
    if object==0{return Err(format!("Missing owner for{name}"));}
    let mut class=u64_at(&capture(r,checks,object,8)?,0);
    for _ in 0..20 {
        if class==0{break;}
        let metadata=r.class(class)?;
        if let Some(field)=metadata.fields.get(name) {
            if field.is_static||!kinds.contains(&field.kind)||field.offset>1_048_576{return Err(format!("Field type/layout changed:{name}"));}
            return Ok((object.checked_add(field.offset).ok_or("Field address overflow")?,field.kind));
        }
        class=metadata.parent;
    }
    Err(format!("Missing typed field:{name}"))
}
fn pointer<M:Memory>(r:&mut Reflection<M>,checks:&mut Checks,object:u64,name:&str)->Result<u64> {
    let(at,_)=typed_field(r,checks,object,name,&[18,21,28,29])?;
    Ok(u64_at(&capture(r,checks,at,8)?,0))
}
fn integer<M:Memory>(r:&mut Reflection<M>,checks:&mut Checks,object:u64,name:&str)->Result<i32> {
    let(at,_)=typed_field(r,checks,object,name,&[8])?;
    Ok(i32_at(&capture(r,checks,at,4)?,0))
}
fn owner<M:Memory>(r:&mut Reflection<M>,checks:&mut Checks,object:u64,expected:&str)->Result<u64> {
    if object==0{return Err(format!("Missing {expected} owner"));}
    let class=u64_at(&capture(r,checks,object,8)?,0);
    if r.class(class)?.name!=expected{return Err(format!("Expected exact {expected} owner"));}
    Ok(class)
}
fn table<M:Memory>(r:&mut Reflection<M>,checks:&mut Checks,name:&str)->Result<(u64,u64,u64)> {
    let class=*r.named.get(name).ok_or("Configuration table class absent")?;
    let metadata=r.class(class)?;
    let field=metadata.fields.get("m_Instance").ok_or("Configuration singleton field absent")?;
    if metadata.name!=name||!field.is_static||![18,21].contains(&field.kind)||metadata.statics==0||field.offset>1_048_576{
        return Err("Configuration singleton type changed".into());
    }
    let instance=u64_at(&capture(r,checks,metadata.statics+field.offset,8)?,0);owner(r,checks,instance,name)?;
    let shared=pointer(r,checks,instance,"m_Data")?;owner(r,checks,shared,"SharedDictionary`2")?;
    let dictionary=pointer(r,checks,shared,"m_dic")?;
    Ok((instance,shared,dictionary))
}
fn entry_layout(metadata:&clbb_mapper::reflection::Class)->Result<(BTreeMap<String,usize>,usize)> {
    let fields=&metadata.fields;
    for(name,kinds)in[("hashCode",&[8][..]),("next",&[8][..]),("key",&[8][..]),("value",&[18,21,28][..])] {
        let f=fields.get(name).ok_or("Dictionary entry field absent")?;
        if f.is_static||!kinds.contains(&f.kind){return Err(format!("Dictionary entry type changed:{name}"));}
    }
    if metadata.name!="Entry"||fields.len()!=4{return Err("Dictionary entry shape changed".into());}
    let bias=fields.values().map(|f|f.offset).min().ok_or("Empty dictionary entry")?;
    let offsets:BTreeMap<_,_>=fields.iter().map(|(name,f)|(name.clone(),(f.offset-bias)as usize)).collect();
    let stride=offsets.iter().map(|(name,offset)|offset+if name=="value"{8}else{4}).max().unwrap();
    if !(16..=64).contains(&stride)||stride%8!=0||offsets["hashCode"]!=0{return Err("Dictionary entry stride changed".into());}
    let mut used=BTreeSet::new();
    for(name,offset)in &offsets {for at in *offset..offset+if name=="value"{8}else{4}{if !used.insert(at){return Err("Dictionary entry fields overlap".into());}}}
    Ok((offsets,stride))
}
fn entries<M:Memory>(r:&mut Reflection<M>,checks:&mut Checks,dictionary:u64,limit:usize)->Result<Vec<(i32,u64)>> {
    owner(r,checks,dictionary,"Dictionary`2")?;
    let count=integer(r,checks,dictionary,"count")?;
    let _version=integer(r,checks,dictionary,"version")?;
    if count<0||count as usize>limit{return Err("Dictionary count exceeds audit bound".into());}
    let array=pointer(r,checks,dictionary,"entries")?;
    if array==0 {return if count==0{Ok(Vec::new())}else{Err("Dictionary entries unavailable".into())};}
    let header=capture(r,checks,array,32)?;let class=u64_at(&header,0);let capacity=u64_at(&header,24);
    if capacity<count as u64||capacity>32768{return Err("Dictionary capacity exceeds audit bound".into());}
    let element=u64_at(&capture(r,checks,class+0x40,8)?,0);
    let(offsets,stride)=entry_layout(&r.class(element)?)?;
    let raw=capture(r,checks,array+32,count as usize*stride)?;
    let mut keys=BTreeSet::new();let mut out=Vec::new();
    for row in raw.chunks_exact(stride) {
        if i32_at(row,offsets["hashCode"])<0{continue;}
        let key=i32_at(row,offsets["key"]);let value=u64_at(row,offsets["value"]);
        if !keys.insert(key){return Err("Duplicate live dictionary key".into());}
        if value==0{return Err("Live dictionary value unavailable".into());}
        out.push((key,value));
    }
    Ok(out)
}
fn managed_string<M:Memory>(r:&mut Reflection<M>,checks:&mut Checks,object:u64)->Result<Option<String>> {
    if object==0{return Ok(None);}
    owner(r,checks,object,"String")?;
    let raw=capture(r,checks,object+16,4)?;let count=i32_at(&raw,0);
    if !(0..=4096).contains(&count){return Err("Native text exceeds4096 UTF16-unit bound".into());}
    let raw=capture(r,checks,object+20,count as usize*2)?;
    String::from_utf16(&raw.chunks_exact(2).map(|v|u16::from_le_bytes(v.try_into().unwrap())).collect::<Vec<_>>())
        .map(Some).map_err(|_|"Invalid native UTF16".into())
}
fn skill_row<M:Memory>(r:&mut Reflection<M>,checks:&mut Checks,object:u64,skill:i32,level:i32)->Result<Value> {
    let class=owner(r,checks,object,"CData_Skill_Element")?;
    let actual=integer(r,checks,object,"m_SkillID")?;let actual_level=integer(r,checks,object,"m_SkillLevel")?;
    if actual!=skill||actual_level!=level||!(1..=100).contains(&level){return Err("Skill element ID/level differs from its exact table keys".into());}
    let mut strings=serde_json::Map::new();
    for name in ["m_SkillName","m_SkillDescription"] {
        let(at,kind)=typed_field(r,checks,object,name,&[14])?;
        let string=u64_at(&capture(r,checks,at,8)?,0);
        strings.insert(name.into(),json!({"kind":kind,"field_offset":at-object,"string_object":string,
            "text":managed_string(r,checks,string)?}));
    }
    Ok(json!({"skill_id":actual,"level":actual_level,"object":object,"class":class,"strings":strings}))
}
fn skill_rows<M:Memory>(r:&mut Reflection<M>,checks:&mut Checks,outer:u64,skill:i32,budget:&mut usize)->Result<Value> {
    owner(r,checks,outer,"SharedDictionary`2")?;let dictionary=pointer(r,checks,outer,"m_dic")?;
    let mut rows=Vec::new();
    let levels=entries(r,checks,dictionary,100)?;
    if levels.len()>*budget{return Err("Global1024 skill-level text row budget exceeded".into());}
    *budget-=levels.len();
    for(level,object)in levels {rows.push(skill_row(r,checks,object,skill,level)?);}
    if rows.is_empty(){return Err("No loaded levels for this exact skill ID".into());}
    Ok(json!({"shared_dictionary":outer,"level_dictionary":dictionary,"levels":rows}))
}
fn int_array<M:Memory>(r:&mut Reflection<M>,checks:&mut Checks,array:u64,nested:bool)->Result<Vec<i32>> {
    if array==0{return Err("Native hero skill array is absent".into());}
    let class=u64_at(&capture(r,checks,array,8)?,0);
    let header=capture(r,checks,class,0x50)?;
    // Array classes use element_class at0x40, unlike Reflection's ordinary
    // class self-identity layout. Validate their actual names and element type.
    if r.cstring(u64_at(&header,16))?!=if nested{"Int32[][]"}else{"Int32[]"}{return Err("Hero skill array type changed".into());}
    let count=u64_at(&capture(r,checks,array+24,8)?,0);
    if count>64{return Err("Hero skill array exceeds64 bound".into());}
    let element=u64_at(&header,0x40);
    let element_name=if nested{let h=capture(r,checks,element,0x50)?;r.cstring(u64_at(&h,16))?}else{r.class(element)?.name};
    if element_name!=if nested{"Int32[]"}else{"Int32"}{return Err("Hero skill array element type changed".into());}
    let raw=capture(r,checks,array+32,count as usize*if nested{8}else{4})?;
    if nested {
        let mut ids=Vec::new();
        for chunk in raw.chunks_exact(8) {
            let values=int_array(r,checks,u64_at(chunk,0),false)?;
            let id=*values.first().ok_or("Empty native hero skill row")?;
            if !(1..=10_000_000).contains(&id){return Err("Invalid native hero skill ID".into());}ids.push(id);
        }
        Ok(ids)
    }else{Ok(raw.chunks_exact(4).map(|chunk|i32_at(chunk,0)).collect())}
}
fn hero_ids<M:Memory>(r:&mut Reflection<M>,checks:&mut Checks,hero:i32)->Result<(BTreeSet<i32>,Value)> {
    let(instance,shared,dictionary)=table(r,checks,"CData_Hero")?;
    let object=entries(r,checks,dictionary,20000)?.into_iter().find(|(id,_)|*id==hero).ok_or("Requested hero is not loaded in native table")?.1;
    owner(r,checks,object,"CData_Hero_Element")?;
    if integer(r,checks,object,"m_ID")?!=hero{return Err("Hero element differs from its exact table key".into());}
    let array=pointer(r,checks,object,"m_SkillList")?;
    let values=int_array(r,checks,array,true)?;let ids:BTreeSet<_>=values.into_iter().collect();
    if ids.is_empty()||ids.len()>64{return Err("Native hero skill list is empty or exceeds bounds".into());}
    Ok((ids,json!({"hero":hero,"instance":instance,"shared_dictionary":shared,"dictionary":dictionary,"element":object,"skill_list":array})))
}
fn write_output(path:&str,output:&Value)->Result<()> {
    let raw=serde_json::to_vec_pretty(output).map_err(|e|e.to_string())?;
    if raw.len()>4*1024*1024{return Err("Text audit exceeds4MiB output bound".into());}
    let mut file=OpenOptions::new().write(true).create_new(true).open(path).map_err(|e|e.to_string())?;
    file.write_all(&raw).and_then(|_|file.write_all(b"\n")).map_err(|e|e.to_string())
}
fn main()->Result<()> {
    let args:Vec<_>=std::env::args().collect();
    if args.len()!=5||!["--skills","--hero"].contains(&args[3].as_str()) {
        return Err("Usage: skill-text-audit PID NEW_OUTPUT.json --skills ID,ID | --hero HERO_ID".into());
    }
    let pid:u32=args[1].parse().map_err(|_|"Invalid PID")?;if pid==0{return Err("PID must be positive".into());}
    let root=PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().to_path_buf();
    owned(&root,pid)?;let initial_identity=identity(pid)?;
    let mut r=Reflection::new(ProcessMemory::diagnostic(pid)?)?;
    r.discover(&["CData_Skill","CData_Skill_Element","CData_Hero","CData_Hero_Element"])?;
    let start=now_ns();let started=Instant::now();let mut checks=Vec::new();
    let(requested,hero_owner)=if args[3]=="--hero" {
        let hero:i32=args[4].parse().map_err(|_|"Invalid hero ID")?;
        if !(1..10000).contains(&hero){return Err("Hero ID must be1..9999".into());}
        let(ids,owner)=hero_ids(&mut r,&mut checks,hero)?;(ids,Some(owner))
    }else{(ids(&args[4])?,None)};
    let(instance,shared,dictionary)=table(&mut r,&mut checks,"CData_Skill")?;
    let lookup:BTreeMap<_,_>=entries(&mut r,&mut checks,dictionary,20000)?.into_iter().collect();
    let mut rows=serde_json::Map::new();let mut found=0;let mut level_budget=1024;
    for skill in &requested {
        if started.elapsed()>Duration::from_secs(8){return Err("Skill text audit exceeded8s collection bound".into());}
        if identity(pid)?!=initial_identity{return Err("Runtime identity changed during text audit".into());}
        let row=match lookup.get(skill) {
            Some(object)=>skill_rows(&mut r,&mut checks,*object,*skill,&mut level_budget),
            None=>Err("Exact requested skill ID is not loaded in native CData_Skill table".into()),
        };
        if row.is_ok(){found+=1;}
        rows.insert(skill.to_string(),json!(row));
    }
    let coherent=check(&r,&checks);
    if identity(pid)?!=initial_identity{return Err("Runtime identity changed during text audit".into());}
    owned(&root,pid)?;
    let output=json!({"format":"clbb-native-skill-text-audit","version":1,"pid":pid,"birth":initial_identity.0,"executable":initial_identity.1,
        "start_ns":start,"end_ns":now_ns(),"requested_skills":requested,"hero_owner":hero_owner,
        "table_owner":{"class":"CData_Skill","instance":instance,"shared_dictionary":shared,"dictionary":dictionary},
        "skills":rows,"found_skills":found,"coherent":coherent.is_ok(),"coherence_error":coherent.err(),"identity_unchanged":true,
        "scope":"read-only native configuration strings; no input, method invocation, localization guessing or production authorization; raw text may contain keys/markup"});
    write_output(&args[2],&output)?;
    println!("native_skill_text_found={found}/{} coherent={} output={}",requested.len(),output["coherent"],args[2]);
    if output["coherent"]!=true{return Err("Native text owners/values changed; report is not trusted".into());}
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clbb_mapper::{platform::Region, reflection::{Class,Field}};
    use std::{cell::RefCell, collections::HashMap};
    struct Bytes(RefCell<Vec<u8>>);
    impl Memory for Bytes {
        fn regions(&self)->Result<Vec<Region>>{Ok(vec![Region{start:0,end:0x10000,readable:true,writable:true,name:String::new()}])}
        fn read(&self,address:u64,size:usize)->Result<Vec<u8>>{self.0.borrow().get(address as usize..address as usize+size).map(|s|s.to_vec()).ok_or("Fixture bounds".into())}
        fn read_into(&self,address:u64,bytes:&mut[u8])->Result<()>{bytes.copy_from_slice(&self.read(address,bytes.len())?);Ok(())}
    }
    fn string_fixture(units:&[u16])->Reflection<Bytes> {
        let mut raw=vec![0;0x10000];let class=0x1000u64;
        for at in [0x1040,0x1048,0x8000]{raw[at..at+8].copy_from_slice(&class.to_le_bytes());}
        raw[0x1010..0x1018].copy_from_slice(&0x3000u64.to_le_bytes());raw[0x3000..0x3006].copy_from_slice(b"String");
        raw[0x8010..0x8014].copy_from_slice(&(units.len()as i32).to_le_bytes());
        for(i,unit)in units.iter().enumerate(){raw[0x8014+i*2..0x8016+i*2].copy_from_slice(&unit.to_le_bytes());}
        Reflection::new(Bytes(RefCell::new(raw))).unwrap()
    }
    #[test]
    fn requested_ids_are_exact_distinct_bounded_and_never_hero_arithmetic() {
        assert_eq!(ids("11830,11831").unwrap(),BTreeSet::from([11830,11831]));
        for value in ["","0","-1","11830,11830","11830,","11830;true","10000001","+11830","11830, 11831"]{assert!(ids(value).is_err(),"{value}");}
        assert!(ids(&(1..=65).map(|n|n.to_string()).collect::<Vec<_>>().join(",")).is_err());
    }
    #[test]
    fn native_strings_are_typed_bounded_strict_utf16_and_fenced() {
        let mut r=string_fixture(&"Джой 🎵".encode_utf16().collect::<Vec<_>>());let mut checks=Vec::new();
        assert_eq!(managed_string(&mut r,&mut checks,0x8000).unwrap().as_deref(),Some("Джой 🎵"));check(&r,&checks).unwrap();
        r.memory.0.borrow_mut()[0x8014]=0;assert!(check(&r,&checks).is_err());
        assert_eq!(managed_string(&mut r,&mut Vec::new(),0).unwrap(),None);
        assert_eq!(managed_string(&mut string_fixture(&[]),&mut Vec::new(),0x8000).unwrap(),Some(String::new()));
        assert!(managed_string(&mut string_fixture(&[0xd800]),&mut Vec::new(),0x8000).is_err());
        let mut long=string_fixture(&[]);long.memory.0.borrow_mut()[0x8010..0x8014].copy_from_slice(&4097i32.to_le_bytes());
        assert!(managed_string(&mut long,&mut Vec::new(),0x8000).is_err());
    }
    #[test]
    fn entry_layout_rejects_wrong_types_static_overlap_and_unbounded_stride() {
        let mut class=Class{address:1,name:"Entry".into(),parent:0,statics:0,fields:HashMap::new()};
        for(name,offset,kind)in[("hashCode",16,8),("next",20,8),("key",24,8),("value",32,21)]{class.fields.insert(name.into(),Field{offset,is_static:false,kind});}
        assert_eq!(entry_layout(&class).unwrap().1,24);
        for mode in 0..4 {let mut bad=class.clone();let f=bad.fields.get_mut("value").unwrap();
            match mode{0=>f.kind=14,1=>f.is_static=true,2=>f.offset=24,_=>f.offset=1024};assert!(entry_layout(&bad).is_err());}
    }
    #[test]
    fn output_never_overwrites_existing_evidence() {
        let path=std::env::temp_dir().join(format!("clbb-skill-text-output-{}-{}.json",std::process::id(),now_ns()));
        fs::write(&path,"original").unwrap();assert!(write_output(path.to_str().unwrap(),&json!({})).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(),"original");fs::remove_file(path).unwrap();
    }
}
