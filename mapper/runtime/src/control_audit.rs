//! Typed, read-only observations of already owned match controls.
//! Flags and choice IDs are observations, not assumed eligibility semantics.
use crate::{platform::Memory, reflection::{Field, Reflection, Result, i32_at, u64_at},
    source::{ButtonPlan, Checks, capture, check}};
use serde_json::{Value, json};
use std::collections::HashMap;

fn fields<M: Memory>(r: &mut Reflection<M>, object: u64, consumed: &mut Checks)
    -> Result<(String, HashMap<String, Field>)> {
    let raw = capture(r, consumed, object, 8)?;
    let mut address = u64_at(&raw, 0);
    let mut name = String::new();
    let mut fields = HashMap::new();
    for _ in 0..16 {
        if address == 0 { break; }
        let class = r.class(address)?;
        if name.is_empty() { name = class.name.clone(); }
        for (key, value) in class.fields { fields.entry(key).or_insert(value); }
        address = class.parent;
    }
    Ok((name, fields))
}

fn value<M: Memory>(r: &mut Reflection<M>, object: u64, f: &Field, consumed: &mut Checks) -> Result<Value> {
    if f.is_static || f.offset > 1048576 { return Err("Unsupported control field owner".into()); }
    let at = object + f.offset;
    match f.kind {
        2 => {
            let raw = capture(r, consumed, at, 1)?;
            if raw[0] > 1 { return Err("Invalid control boolean".into()); }
            Ok(json!(raw[0] != 0))
        },
        8 => Ok(json!(i32_at(&capture(r, consumed, at, 4)?, 0))),
        9 => Ok(json!(u32::from_le_bytes(capture(r, consumed, at, 4)?.try_into().unwrap()))),
        29 => {
            let array = u64_at(&capture(r, consumed, at, 8)?, 0);
            if array == 0 { return Ok(Value::Null); }
            let header = capture(r, consumed, array, 32)?;
            let count = u64_at(&header, 24);
            if count > 64 { return Err("Control diagnostic array exceeds bounds".into()); }
            let class = capture(r, consumed, u64_at(&header, 0), 0x50)?;
            let name = r.cstring(u64_at(&class, 16))?;
            let element = r.class(u64_at(&class, 0x40))?.name;
            if name != format!("{element}[]") { return Err("Control array type mismatch".into()); }
            if !["Int32", "UInt32"].contains(&element.as_str()) {
                return Ok(json!({"type":name,"length":count,"values_unread":true}));
            }
            let raw = capture(r, consumed, array + 32, count as usize * 4)?;
            let values: Vec<_> = raw.chunks_exact(4).map(|b| if element == "Int32" {
                json!(i32::from_le_bytes(b.try_into().unwrap()))
            } else { json!(u32::from_le_bytes(b.try_into().unwrap())) }).collect();
            Ok(json!({"type":name,"values":values}))
        },
        _ => Ok(json!({"unsupported_kind":f.kind})),
    }
}

fn numeric_snapshot<M:Memory>(r:&mut Reflection<M>,object:u64,metadata:&HashMap<String,Field>,consumed:&mut Checks)->Result<Value> {
    let mut selected:Vec<_>=metadata.iter().filter_map(|(name,f)|{
        let size=match f.kind{2=>1,8|9|12=>4,10|11|13=>8,_=>return None};
        (!f.is_static&&f.offset<65536).then_some((name,f,size))
    }).collect();
    selected.sort_by_key(|(name,_,_)|*name);
    let total=selected.len();selected.truncate(256);
    let size=selected.iter().map(|(_,f,n)|f.offset as usize+n).max().unwrap_or(8);
    let raw=capture(r,consumed,object,size)?;
    let mut values=serde_json::Map::new();
    for (name,f,size)in selected {
        let data=&raw[f.offset as usize..f.offset as usize+size];
        let value=match f.kind{
            2=>if data[0]<=1{json!(data[0]!=0)}else{json!({"invalid_bool":data[0]})},
            8=>json!(i32::from_le_bytes(data.try_into().unwrap())),
            9=>json!(u32::from_le_bytes(data.try_into().unwrap())),
            10=>json!(i64::from_le_bytes(data.try_into().unwrap())),
            11=>json!(u64::from_le_bytes(data.try_into().unwrap())),
            12=>json!(f32::from_le_bytes(data.try_into().unwrap())),
            13=>json!(f64::from_le_bytes(data.try_into().unwrap())),_=>unreachable!()};
        values.insert(name.clone(),json!({"kind":f.kind,"value":value}));
    }
    Ok(json!({"values":values,"declared_numeric_fields":total,"truncated":total>256}))
}

pub(crate) fn cast_lock_address<M: Memory>(r: &mut Reflection<M>, object: u64) -> Option<u64> {
    let (_, metadata) = fields(r, object, &mut Vec::new()).ok()?;
    let field = metadata.get("m_bIsLockForbid")?;
    (field.kind == 2 && !field.is_static && field.offset <= 1_048_576)
        .then(|| object + field.offset)
}

pub fn observe<M: Memory>(r: &mut Reflection<M>, ui: u64, buttons: &[ButtonPlan]) -> Result<Value> {
    let mut consumed = Vec::new();
    let mut rows = Vec::new();
    for button in buttons {
        let (class_name, metadata) = fields(r, button.object, &mut consumed)?;
        let numeric=numeric_snapshot(r,button.object,&metadata,&mut consumed)?;
        let mut values = serde_json::Map::new();
        for name in ["m_bIsLockForbid", "m_bIsStateForbid", "m_bUnStateForbid",
            "<m_iHoverButtonSkillId>k__BackingField", "<m_iLeaveButtonSkillId>k__BackingField",
            "<CancelSkillID>k__BackingField", "m_releaseButtonChooseSkillId",
            "m_releaseButtonChooseSkillId_LastValid", "m_releaseButtonChooseSkillId_LastValidTimestamp",
            "m_iUIkeySkillParaType", "sUIkeySkillPara1", "m_listExtraId",
            "iTouchFingerID", "m_mainTouchId", "m_FingerId", "m_bPressed", "m_bIsPressed", "bFirstDrag", "m_bVaildDraged", "m_dragged", "m_bCanUseSkill"] {
            if let Some(f) = metadata.get(name) {
                values.insert(name.into(), match value(r, button.object, f, &mut consumed){Ok(v)=>v,Err(error)=>json!({"unavailable":error})});
            }
        }
        rows.push(json!({"slot":button.slot,"class":class_name,"values":values,"numeric":numeric}));
    }
    let (_, metadata) = fields(r, ui, &mut consumed)?;
    let numeric_ui=numeric_snapshot(r,ui,&metadata,&mut consumed)?;
    let mut curve = Value::Null;
    if let Some(f) = metadata.get("m_buttonCurveLink") {
        if f.kind != 18 || f.is_static { return Err("Control curve link type changed".into()); }
        let object = u64_at(&capture(r, &mut consumed, ui + f.offset, 8)?, 0);
        if object != 0 {
            let (name, metadata) = fields(r, object, &mut consumed)?;
            if name != "ButtonCurveLink" { return Err("Unknown curve controller".into()); }
            let mut slots = Vec::new();
            let mut source = Value::Null;
            for name in ["m_button1", "m_button2", "<OngoingDraggingSourceButton>k__BackingField"] {
                let f = metadata.get(name).ok_or("Curve endpoint missing")?;
                if f.kind != 18 || f.is_static { return Err("Curve endpoint type changed".into()); }
                let target = u64_at(&capture(r, &mut consumed, object + f.offset, 8)?, 0);
                let slot = buttons.iter().find(|b| b.object == target && b.kind == "skill").map(|b| b.slot.clone());
                if target != 0 && slot.is_none() { return Err("Curve endpoint is not an owned skill control".into()); }
                if name.starts_with('<') { source = json!(slot); } else { slots.push(slot); }
            }
            let f = metadata.get("<EnoughEnergy>k__BackingField").ok_or("Curve energy flag missing")?;
            if f.kind != 2 { return Err("Curve energy flag type changed".into()); }
            let enough = value(r, object, f, &mut consumed)?;
            curve = json!({"endpoints":slots,"source_slot":source,"enough_energy":enough});
        }
    }
    check(r, &consumed)?;
    Ok(json!({"buttons":rows,"curve":curve,"numeric_ui":numeric_ui}))
}

/// Cached typed offsets for the live linked-button path; no metadata walks in
/// the hot sample loop. All ownership links are revalidated with the sample.
pub struct CurvePlan {
    checks: Checks,
    slots: [String; 2],
    objects: [u64; 2],
    source: u64,
    enough: u64,
    choices: [u64; 2],
}
impl CurvePlan {
    pub fn discover<M:Memory>(r:&mut Reflection<M>, ui:u64, buttons:&[ButtonPlan])->Result<Self> {
        let mut checks=Vec::new();
        let (_, metadata)=fields(r,ui,&mut checks)?;
        let f=metadata.get("m_buttonCurveLink").ok_or("No linked controls")?;
        if f.kind!=18 || f.is_static {return Err("Invalid linked controller field".into());}
        let address=ui+f.offset;
        let object=u64_at(&capture(r,&mut checks,address,8)?,0);
        if object==0 {return Err("No active linked controller".into());}
        let (name, metadata)=fields(r,object,&mut checks)?;
        if name!="ButtonCurveLink" {return Err("Unsupported linked controller".into());}
        let mut slots=[String::new(),String::new()];let mut objects=[0;2];let mut choices=[0;2];
        for (i,name) in ["m_button1","m_button2"].iter().enumerate() {
            let f=metadata.get(*name).ok_or("Linked endpoint missing")?;
            if f.kind!=18 || f.is_static {return Err("Linked endpoint type changed".into());}
            objects[i]=u64_at(&capture(r,&mut checks,object+f.offset,8)?,0);
            let b=buttons.iter().find(|b|b.object==objects[i] && b.kind=="skill")
                .ok_or("Endpoint is not an owned skill")?;
            slots[i]=b.slot.clone();
            let (_,metadata)=fields(r,b.object,&mut checks)?;
            let f=metadata.get("m_releaseButtonChooseSkillId").ok_or("No current linked choice")?;
            if f.kind!=8 || f.is_static {return Err("Linked choice type changed".into());}
            choices[i]=b.object+f.offset;
        }
        if objects[0]==objects[1] {return Err("Linked endpoints alias".into());}
        let f=metadata.get("<OngoingDraggingSourceButton>k__BackingField").ok_or("No drag owner")?;
        if f.kind!=18 || f.is_static {return Err("Drag owner type changed".into());}
        let source=object+f.offset;
        let f=metadata.get("<EnoughEnergy>k__BackingField").ok_or("No linked eligibility")?;
        if f.kind!=2 || f.is_static {return Err("Linked eligibility type changed".into());}
        let enough=object+f.offset;
        check(r,&checks)?;
        Ok(Self {checks,slots,objects,source,enough,choices})
    }
    pub fn validate<M:Memory>(&self,r:&Reflection<M>)->Result<()> {check(r,&self.checks)}
    pub fn sample<M:Memory>(&self,r:&mut Reflection<M>)->Result<(crate::linked_gesture::Observation,Checks)> {
        check(r,&self.checks)?;
        let mut consumed=self.checks.clone();
        let source=u64_at(&capture(r,&mut consumed,self.source,8)?,0);
        let source_slot=if source==0 {None} else {
            Some(self.slots[self.objects.iter().position(|p|*p==source).ok_or("Foreign linked drag owner")?].clone())
        };
        let energy=capture(r,&mut consumed,self.enough,1)?[0];
        if energy>1 {return Err("Invalid linked energy flag".into());}
        let mut choices=[0;2];
        for (i,address) in self.choices.iter().enumerate() {choices[i]=i32_at(&capture(r,&mut consumed,*address,4)?,0);}
        check(r,&consumed)?;
        Ok((crate::linked_gesture::Observation {endpoints:self.slots.clone(),source_slot,pressed_slot:None,enough_energy:energy==1,choices,base_skills:[0;2],on_cooldown:[false;2]},consumed))
    }
}
