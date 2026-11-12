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

pub(crate) fn state_forbid_address<M: Memory>(r: &mut Reflection<M>, object: u64) -> Option<u64> {
    let (_, metadata) = fields(r, object, &mut Vec::new()).ok()?;
    let field = metadata.get("m_bIsStateForbid")?;
    (field.kind == 2 && !field.is_static && field.offset <= 1_048_576)
        .then(|| object + field.offset)
}

/// UI option selection is a separate family. Its XY vector and option enum
/// must never be interpreted as a world-space directional skill result.
#[derive(Clone)]
pub(crate) struct ChoicePlan {
    checks: Checks,
    button: u64,
    data: u64,
    base: u64,
    active: u64,
    current: u64,
    held: u64,
    drag: u64,
    panel: u64,
    hover: u64,
    direction: u64,
    options: Vec<i32>,
}
pub(crate) struct ChoiceObservation {
    pub owner: u64,
    pub base: i32,
    pub active: i32,
    pub current: bool,
    pub panel: bool,
    pub drag: bool,
    pub hover: i32,
    pub selected: Option<i32>,
    pub direction: [f64; 3],
}
impl ChoicePlan {
    pub(crate) fn is_family<M:Memory>(r:&mut Reflection<M>,class:&crate::reflection::Class)->Result<bool> {
        let mut class=class.clone();
        for _ in 0..16 {
            if class.name=="KeySkillMultiChoice" {return Ok(true);}
            if class.parent==0 {return Ok(false);}
            class=r.class(class.parent)?;
        }
        Err("Selection button ancestry exceeds bounds".into())
    }
    pub(crate) fn discover<M:Memory>(r:&mut Reflection<M>,ui:u64,button:u64,data:u64,local_show:u64)->Result<Self> {
        let mut checks=Vec::new();
        let class=r.class(r.pointer(button)?)?;
        if !Self::is_family(r,&class)? {return Err("Not a native UI selection button".into());}
        let(_,button_fields)=fields(r,button,&mut checks)?;
        let(data_class,data_fields)=fields(r,data,&mut checks)?;
        let(ui_class,ui_fields)=fields(r,ui,&mut checks)?;
        let(_,show_fields)=fields(r,local_show,&mut checks)?;
        if data_class!="ShowSkillData"||ui_class!="UIKeySkill" {return Err("Selection UI/data class changed".into());}
        let address=|object:u64,metadata:&HashMap<String,Field>,name:&str,kind:u8|->Result<u64>{
            let f=metadata.get(name).ok_or_else(||format!("Selection field absent:{name}"))?;
            if f.is_static||f.kind!=kind||f.offset>1_048_576 {return Err(format!("Selection field type changed:{name}"));}
            object.checked_add(f.offset).ok_or("Selection field overflow".into())
        };
        let reference=|r:&Reflection<M>,checks:&mut Checks,at:u64,expected:u64|->Result<()> {
            if expected==0||u64_at(&capture(r,checks,at,8)?,0)!=expected{return Err("Selection local ownership changed".into());}Ok(())
        };
        reference(r,&mut checks,address(button,&button_fields,"m_SkillData",18)?,data)?;
        reference(r,&mut checks,address(button,&button_fields,"m_pOwner",18)?,local_show)?;
        let local_comp=u64_at(&capture(r,&mut checks,address(local_show,&show_fields,"m_SkillComp",18)?,8)?,0);
        reference(r,&mut checks,address(data,&data_fields,"m_SkillComp",18)?,local_comp)?;
        let(comp_class,comp_fields)=fields(r,local_comp,&mut checks)?;
        if comp_class!="ShowSkillComp" {return Err("Selection local component class changed".into());}
        reference(r,&mut checks,address(local_comp,&comp_fields,"m_pOwner",18)?,local_show)?;
        // Root references must be live native GameObjects owned by this exact
        // managed object. A naked coordinate or recycled pointer is insufficient.
        let native_root=|r:&mut Reflection<M>,checks:&mut Checks,root:u64|->Result<()> {
            let(name,metadata)=fields(r,root,checks)?;
            if name!="GameObject" {return Err("Selection root is not a GameObject".into());}
            let native=u64_at(&capture(r,checks,address(root,&metadata,"m_CachedPtr",24)?,8)?,0);
            reference(r,checks,native.checked_add(40).ok_or("Selection native overflow")?,root)
        };
        let root=u64_at(&capture(r,&mut checks,address(button,&button_fields,"m_choiceRoot",18)?,8)?,0);
        native_root(r,&mut checks,root)?;
        let list=u64_at(&capture(r,&mut checks,address(button,&button_fields,"m_choiceOptions",21)?,8)?,0);
        let(list_class,list_fields)=fields(r,list,&mut checks)?;
        if list_class!="List`1" {return Err("Selection options list class changed".into());}
        let count=i32_at(&capture(r,&mut checks,address(list,&list_fields,"_size",8)?,4)?,0);
        // Only the measured two-choice enum contract is currently qualified.
        if count!=2 {return Err("Selection option count is unqualified".into());}
        let _version=capture(r,&mut checks,address(list,&list_fields,"_version",8)?,4)?;
        let array=u64_at(&capture(r,&mut checks,address(list,&list_fields,"_items",29)?,8)?,0);
        let header=capture(r,&mut checks,array,32)?;
        if u64_at(&header,24)<count as u64||u64_at(&header,24)>64{return Err("Selection options capacity changed".into());}
        let array_class=capture(r,&mut checks,u64_at(&header,0),0x50)?;
        if r.cstring(u64_at(&array_class,16))?!="ChoiceOption[]"||r.class(u64_at(&array_class,0x40))?.name!="ChoiceOption" {
            return Err("Selection options array type changed".into());
        }
        let entries=capture(r,&mut checks,array+32,count as usize*8)?;
        let mut options=Vec::new();
        for entry in entries.chunks_exact(8) {
            let option=u64_at(entry,0);let(name,metadata)=fields(r,option,&mut checks)?;
            if name!="ChoiceOption" {return Err("Selection option owner changed".into());}
            let direction=i32_at(&capture(r,&mut checks,address(option,&metadata,"Direction",17)?,4)?,0);
            if !matches!(direction,1|2)||options.contains(&direction){return Err("Selection option enum is unqualified or ambiguous".into());}
            let root=u64_at(&capture(r,&mut checks,address(option,&metadata,"Root",18)?,8)?,0);
            native_root(r,&mut checks,root)?;options.push(direction);
        }
        let plan=Self {checks,button,data,
            base:address(data,&data_fields,"m_InitSkillID",8)?,active:address(data,&data_fields,"m_TranID",8)?,
            current:address(ui,&ui_fields,"m_curVirtualButton",18)?,held:address(ui,&ui_fields,"_bPressSkill",2)?,
            drag:address(button,&button_fields,"m_hasDrag",2)?,panel:address(button,&button_fields,"m_panelVisible",2)?,
            hover:address(button,&button_fields,"m_hoverIndex",8)?,direction:address(button,&button_fields,"m_lastDragDir",17)?,options};
        check(r,&plan.checks)?;Ok(plan)
    }
    pub(crate) fn validate<M:Memory>(&self,r:&Reflection<M>)->Result<()> {check(r,&self.checks)}
    pub(crate) fn sample<M:Memory>(&self,r:&Reflection<M>,consumed:&mut Checks)->Option<ChoiceObservation> {
        let checkpoint=consumed.len();
        let read=(||->Result<ChoiceObservation>{
            self.validate(r)?;consumed.extend(self.checks.iter().cloned());
            let base=i32_at(&capture(r,consumed,self.base,4)?,0);let active=i32_at(&capture(r,consumed,self.active,4)?,0);
            if !(1..=clbb_moba_core::controller::MAX_SKILL_ID).contains(&base)||!(1..=clbb_moba_core::controller::MAX_SKILL_ID).contains(&active){return Err("Selection skill IDs unavailable".into());}
            let current=u64_at(&capture(r,consumed,self.current,8)?,0);
            let held=capture(r,consumed,self.held,1)?[0];let drag=capture(r,consumed,self.drag,1)?[0];let panel=capture(r,consumed,self.panel,1)?[0];
            if [held,drag,panel].iter().any(|v|*v>1){return Err("Selection flags unavailable".into());}
            let hover=i32_at(&capture(r,consumed,self.hover,4)?,0);
            if hover< -1||hover>=self.options.len() as i32{return Err("Selection hover index is outside owned options".into());}
            let direction=crate::source::vec3(&capture(r,consumed,self.direction,12)?)?;
            if direction.iter().any(|v|v.abs()>1e6){return Err("Selection UI direction unavailable".into());}
            let selected=(hover>=0).then(||self.options[hover as usize]);
            Ok(ChoiceObservation {owner:self.data,base,active,current:current==self.button&&held==1,panel:panel==1,drag:drag==1,hover,selected,direction})
        })();
        match read {Ok(value)=>Some(value),Err(_)=>{consumed.truncate(checkpoint);None}}
    }
}

/// Charge lifecycle belongs to the button's local ShowSkillData, independently
/// of whether its visual indicator is a point, direction, or range template.
#[derive(Clone)]
pub(crate) struct ChargePlan {
    checks: Checks,
    local_component: u64,
    data_component: u64,
    component_class: u64,
    charge: u64,
    cancel_skill: u64,
}
impl ChargePlan {
    pub(crate) fn discover<M: Memory>(r: &mut Reflection<M>, button: u64, data: u64, local_show: u64) -> Result<Self> {
        if data == 0 || local_show == 0 { return Err("Charge owner unavailable".into()); }
        let mut checks = Vec::new();
        let (_, button_fields) = fields(r, button, &mut checks)?;
        let (data_name, data_fields) = fields(r, data, &mut checks)?;
        let (_, local_fields) = fields(r, local_show, &mut checks)?;
        if data_name != "ShowSkillData" { return Err("Charge data type changed".into()); }
        let address = |object: u64, metadata: &HashMap<String, Field>, name: &str, kind| -> Result<u64> {
            let field = metadata.get(name).ok_or_else(|| format!("Missing charge field {name}"))?;
            if field.is_static || field.kind != kind || field.offset > 1_048_576 {
                return Err(format!("Charge field type changed: {name}"));
            }
            object.checked_add(field.offset).ok_or("Charge field address overflow".into())
        };
        let data_link = address(button, &button_fields, "m_SkillData", 18)?;
        if u64_at(&capture(r, &mut checks, data_link, 8)?, 0) != data {
            return Err("Charge data is not owned by this button".into());
        }
        let local_component = address(local_show, &local_fields, "m_SkillComp", 18)?;
        let data_component = address(data, &data_fields, "m_SkillComp", 18)?;
        let charge = address(data, &data_fields, "m_bChargeSkill", 2)?;
        let cancel_skill = address(data, &data_fields, "m_iCancelChargeSkill", 8)?;
        let mut ownership = Vec::new();
        let local = u64_at(&capture(r, &mut ownership, local_component, 8)?, 0);
        if local == 0 || u64_at(&capture(r, &mut ownership, data_component, 8)?, 0) != local {
            return Err("Charge component is not local".into());
        }
        let component_class = u64_at(&capture(r, &mut ownership, local, 8)?, 0);
        if r.class(component_class)?.name != "ShowSkillComp" {
            return Err("Charge component type changed".into());
        }
        check(r, &checks)?;
        check(r, &ownership)?;
        // Cache typed offsets even when the current dynamic flags are unknown;
        // later samples can become available without another metadata walk.
        Ok(Self { checks, local_component, data_component, component_class, charge, cancel_skill })
    }
    fn read<M: Memory>(&self, r: &Reflection<M>, consumed: &mut Checks) -> Result<bool> {
        check(r, &self.checks)?;
        consumed.extend(self.checks.iter().cloned());
        let local = u64_at(&capture(r, consumed, self.local_component, 8)?, 0);
        let owner = u64_at(&capture(r, consumed, self.data_component, 8)?, 0);
        if local == 0 || owner != local { return Err("Charge component is not local".into()); }
        if u64_at(&capture(r, consumed, local, 8)?, 0) != self.component_class {
            return Err("Charge component type changed".into());
        }
        let charge = capture(r, consumed, self.charge, 1)?[0];
        let cancel = i32_at(&capture(r, consumed, self.cancel_skill, 4)?, 0);
        match (charge, cancel) {
            (0, 0) => Ok(false),
            (1, 1..=clbb_moba_core::controller::MAX_SKILL_ID) => Ok(true),
            _ => Err("Charge lifecycle flags unavailable or contradictory".into()),
        }
    }
    pub(crate) fn sample<M: Memory>(&self, r: &Reflection<M>, consumed: &mut Checks) -> Option<bool> {
        let checkpoint = consumed.len();
        match self.read(r, consumed) {
            Ok(value) => Some(value),
            Err(_) => { consumed.truncate(checkpoint); None },
        }
    }
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

#[cfg(test)]
mod charge_tests {
    use super::*;
    use std::cell::RefCell;
    const BUTTON: u64 = 0x1000;
    const DATA: u64 = 0x1100;
    const SHOW: u64 = 0x1200;
    const COMPONENT: u64 = 0x1300;
    const OTHER: u64 = 0x1340;
    struct TestMemory(RefCell<Vec<u8>>);
    impl Memory for TestMemory {
        fn regions(&self) -> Result<Vec<crate::platform::Region>> {
            Ok(vec![crate::platform::Region { start: 0, end: self.0.borrow().len()as u64, readable: true, writable: true, name: String::new() }])
        }
        fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
            self.0.borrow().get(address as usize..address as usize + size).map(|v| v.to_vec()).ok_or("Unreadable test memory".into())
        }
        fn read_into(&self, address: u64, bytes: &mut [u8]) -> Result<()> {
            bytes.copy_from_slice(&self.read(address, bytes.len())?); Ok(())
        }
    }
    fn put(bytes: &mut [u8], at: u64, value: u64) {
        bytes[at as usize..at as usize + 8].copy_from_slice(&value.to_le_bytes());
    }
    fn string(bytes: &mut [u8], next: &mut usize, value: &str) -> u64 {
        let at = *next; bytes[at..at + value.len()].copy_from_slice(value.as_bytes());
        *next += value.len() + 1; at as u64
    }
    fn class(bytes: &mut [u8], next: &mut usize, at: u64, name: &str, fields: &[(&str, u8, u32)]) {
        put(bytes, at + 0x40, at); put(bytes, at + 0x48, at);
        let name = string(bytes, next, name); put(bytes, at + 16, name);
        let table = *next as u64; *next += fields.len() * 32;
        put(bytes, at + 0x78, table);
        bytes[at as usize + 0x104..at as usize + 0x106].copy_from_slice(&(fields.len() as u16).to_le_bytes());
        for (i, (name, kind, offset)) in fields.iter().enumerate() {
            let row = table + i as u64 * 32;
            let name = string(bytes, next, name); put(bytes, row, name);
            let ty = *next as u64; *next += 16;
            bytes[ty as usize + 8..ty as usize + 12].copy_from_slice(&((*kind as u32) << 16).to_le_bytes());
            put(bytes, row + 8, ty); put(bytes, row + 16, at);
            bytes[row as usize + 24..row as usize + 28].copy_from_slice(&offset.to_le_bytes());
        }
    }
    fn fixture(charge_kind: u8, data_name: &str, local: bool) -> Reflection<TestMemory> {
        let mut bytes = vec![0; 0x4000]; let mut next = 0x2000;
        class(&mut bytes, &mut next, 0x100, "SkillButton", &[("m_SkillData", 18, 16)]);
        class(&mut bytes, &mut next, 0x300, data_name, &[("m_SkillComp", 18, 16), ("m_bChargeSkill", charge_kind, 24), ("m_iCancelChargeSkill", 8, 28)]);
        class(&mut bytes, &mut next, 0x500, "LocalShow", &[("m_SkillComp", 18, 16)]);
        class(&mut bytes, &mut next, 0x700, "ShowSkillComp", &[]);
        for (object, identity) in [(BUTTON, 0x100), (DATA, 0x300), (SHOW, 0x500), (COMPONENT, 0x700), (OTHER, 0x700)] { put(&mut bytes, object, identity); }
        put(&mut bytes, BUTTON + 16, DATA); put(&mut bytes, SHOW + 16, COMPONENT);
        put(&mut bytes, DATA + 16, if local { COMPONENT } else { OTHER });
        bytes[DATA as usize + 24] = 1;
        bytes[DATA as usize + 28..DATA as usize + 32].copy_from_slice(&12618_i32.to_le_bytes());
        Reflection::new(TestMemory(RefCell::new(bytes))).unwrap()
    }
    const CHOICE: u64=0x4000;
    const CHOICE_DATA:u64=0x4100;
    const CHOICE_SHOW:u64=0x4200;
    const CHOICE_COMP:u64=0x4300;
    const CHOICE_UI:u64=0x4400;
    fn choice_fixture()->Reflection<TestMemory> {
        let mut b=vec![0;0x10000];let mut next=0x6000;
        class(&mut b,&mut next,0x100,"KeySkillMultiChoice",&[("m_choiceOptions",21,32),("m_choiceRoot",18,40),
            ("m_lastDragDir",17,48),("m_hasDrag",2,60),("m_panelVisible",2,61),("m_hoverIndex",8,64)]);
        class(&mut b,&mut next,0x300,"SkillButton",&[("m_SkillData",18,16),("m_pOwner",18,24)]);put(&mut b,0x158,0x300);
        class(&mut b,&mut next,0x500,"ShowSkillData",&[("m_SkillComp",18,16),("m_InitSkillID",8,24),("m_TranID",8,28)]);
        class(&mut b,&mut next,0x700,"LocalShow",&[("m_SkillComp",18,16)]);
        class(&mut b,&mut next,0x900,"ShowSkillComp",&[("m_pOwner",18,16)]);
        class(&mut b,&mut next,0xb00,"UIKeySkill",&[("m_curVirtualButton",18,16),("_bPressSkill",2,24)]);
        class(&mut b,&mut next,0xd00,"List`1",&[("_items",29,16),("_size",8,24),("_version",8,28)]);
        class(&mut b,&mut next,0xf00,"ChoiceOption",&[("Root",18,16),("Direction",17,24)]);
        class(&mut b,&mut next,0x1100,"GameObject",&[("m_CachedPtr",24,16)]);
        for(object,identity)in[(CHOICE,0x100),(CHOICE_DATA,0x500),(CHOICE_SHOW,0x700),(CHOICE_COMP,0x900),(CHOICE_UI,0xb00),
            (0x4500,0x1100),(0x4700,0xd00),(0x4900,0xf00),(0x4a00,0xf00),(0x4b00,0x1100),(0x4c00,0x1100)]{put(&mut b,object,identity);}
        for(at,value)in[(CHOICE+16,CHOICE_DATA),(CHOICE+24,CHOICE_SHOW),(CHOICE+32,0x4700),(CHOICE+40,0x4500),
            (CHOICE_DATA+16,CHOICE_COMP),(CHOICE_SHOW+16,CHOICE_COMP),(CHOICE_COMP+16,CHOICE_SHOW),
            (CHOICE_UI+16,CHOICE),(0x4510,0x4600),(0x4628,0x4500),(0x4710,0x4800),(0x4800,0x1300),
            (0x4818,2),(0x4820,0x4900),(0x4828,0x4a00),(0x4910,0x4b00),(0x4a10,0x4c00),
            (0x4b10,0x4d00),(0x4d28,0x4b00),(0x4c10,0x4e00),(0x4e28,0x4c00),(0x1340,0xf00)]{put(&mut b,at,value);}
        let name=string(&mut b,&mut next,"ChoiceOption[]");put(&mut b,0x1310,name);
        for(at,value)in[(0x4718,2i32),(0x471c,1),(0x4918,1),(0x4a18,2),(CHOICE_DATA+24,13130),(CHOICE_DATA+28,13130),(CHOICE+64,0)] {
            b[at as usize..at as usize+4].copy_from_slice(&value.to_le_bytes());
        }
        for(at,value)in[(CHOICE+48,0.6f32),(CHOICE+52,0.8),(CHOICE+56,0.)]{b[at as usize..at as usize+4].copy_from_slice(&value.to_le_bytes());}
        b[CHOICE_UI as usize+24]=1;b[CHOICE as usize+60]=1;b[CHOICE as usize+61]=1;
        Reflection::new(TestMemory(RefCell::new(b))).unwrap()
    }
    #[test]
    fn selection_metadata_and_owned_enum_zero_index_are_distinct_from_world_direction() {
        let mut r=choice_fixture();let plan=ChoicePlan::discover(&mut r,CHOICE_UI,CHOICE,CHOICE_DATA,CHOICE_SHOW).unwrap();
        let mut consumed=Vec::new();let value=plan.sample(&r,&mut consumed).unwrap();
        assert_eq!(value.owner,CHOICE_DATA);assert_ne!(value.owner,CHOICE);assert_eq!(value.selected,Some(1));assert_eq!(value.hover,0);
        assert!(value.current&&value.panel&&value.drag);assert_eq!(value.base,13130);assert!(check(&r,&consumed).is_ok());
        r.memory.0.borrow_mut()[CHOICE as usize+64..CHOICE as usize+68].copy_from_slice(&1i32.to_le_bytes());
        assert!(check(&r,&consumed).is_err());assert_eq!(plan.sample(&r,&mut Vec::new()).unwrap().selected,Some(2));
        r.memory.0.borrow_mut()[CHOICE as usize+64..CHOICE as usize+68].copy_from_slice(&(-1i32).to_le_bytes());
        let idle=plan.sample(&r,&mut Vec::new()).unwrap();assert_eq!(idle.selected,None);assert_eq!(idle.hover,-1);
        r.memory.0.borrow_mut()[CHOICE_DATA as usize+28..CHOICE_DATA as usize+32].copy_from_slice(&2_013_132i32.to_le_bytes());
        r.memory.0.borrow_mut()[CHOICE as usize+61]=0;
        let future=plan.sample(&r,&mut Vec::new()).unwrap();
        assert_eq!(future.owner,CHOICE_DATA);assert_eq!(future.base,13130);assert_eq!(future.active,2_013_132);
        assert!(!future.panel,"fresh data ownership remains observable after the panel closes and native form advances");
    }
    #[test]
    fn selection_foreign_rebound_unknown_and_unqualified_metadata_revoke_only_optional_receipt() {
        let mut r=choice_fixture();let plan=ChoicePlan::discover(&mut r,CHOICE_UI,CHOICE,CHOICE_DATA,CHOICE_SHOW).unwrap();
        let retained=vec![(CHOICE,0x100u64.to_le_bytes().to_vec())];
        for(at,raw)in[(CHOICE+60,vec![2]),(CHOICE+64,8i32.to_le_bytes().to_vec()),
            (CHOICE+48,f32::NAN.to_le_bytes().to_vec()),(CHOICE+16,0x4150u64.to_le_bytes().to_vec()),
            (CHOICE_COMP+16,0x4250u64.to_le_bytes().to_vec())] {
            let old=r.read(at,raw.len()).unwrap();r.memory.0.borrow_mut()[at as usize..at as usize+raw.len()].copy_from_slice(&raw);
            let mut consumed=retained.clone();assert!(plan.sample(&r,&mut consumed).is_none());assert_eq!(consumed,retained);
            r.memory.0.borrow_mut()[at as usize..at as usize+old.len()].copy_from_slice(&old);
        }
        for(at,raw)in[(0x4a18,3i32.to_le_bytes().to_vec()),(0x4d28,0x4c00u64.to_le_bytes().to_vec())] {
            let mut changed=choice_fixture();changed.memory.0.borrow_mut()[at..at+raw.len()].copy_from_slice(&raw);
            assert!(ChoicePlan::discover(&mut changed,CHOICE_UI,CHOICE,CHOICE_DATA,CHOICE_SHOW).is_err());
        }
        let mut wrong=choice_fixture();let table=wrong.pointer(0x1100+0x78).unwrap();let ty=wrong.pointer(table+8).unwrap();
        wrong.memory.0.borrow_mut()[ty as usize+8..ty as usize+12].copy_from_slice(&(18u32<<16).to_le_bytes());
        assert!(ChoicePlan::discover(&mut wrong,CHOICE_UI,CHOICE,CHOICE_DATA,CHOICE_SHOW).is_err(),"native cached pointer kind24 must not become managed reference18");
    }
    #[test]
    fn discovery_requires_typed_button_data_and_its_local_skill_component() {
        for (kind, name, local) in [(8, "ShowSkillData", true), (2, "ForeignData", true), (2, "ShowSkillData", false)] {
            let mut r = fixture(kind, name, local);
            assert!(ChargePlan::discover(&mut r, BUTTON, DATA, SHOW).is_err());
        }
        let mut r = fixture(2, "ShowSkillData", true);
        assert!(ChargePlan::discover(&mut r, BUTTON, DATA, SHOW).is_ok());
    }
    #[test]
    fn charge_flags_and_owner_links_share_the_outer_sample_fence() {
        let mut r = fixture(2, "ShowSkillData", true);
        let plan = ChargePlan::discover(&mut r, BUTTON, DATA, SHOW).unwrap();
        let mut consumed = Vec::new();
        assert_eq!(plan.sample(&r, &mut consumed), Some(true));
        assert!(check(&r, &consumed).is_ok());
        put(&mut r.memory.0.borrow_mut(), DATA + 16, OTHER);
        assert!(check(&r, &consumed).is_err());
        let mut retained = vec![(BUTTON, 0x100_u64.to_le_bytes().to_vec())];
        assert_eq!(plan.sample(&r, &mut retained), None);
        assert_eq!(retained.len(), 1);
        // A newly installed local component is valid only when both current
        // references agree and its native class is still ShowSkillComp.
        put(&mut r.memory.0.borrow_mut(), SHOW + 16, OTHER);
        assert_eq!(plan.sample(&r, &mut Vec::new()), Some(true));
        put(&mut r.memory.0.borrow_mut(), OTHER, 0x300);
        assert_eq!(plan.sample(&r, &mut Vec::new()), None);
    }
    #[test]
    fn absent_or_contradictory_charge_flags_are_unknown_not_plain_tap() {
        let mut r = fixture(2, "ShowSkillData", true);
        r.memory.0.borrow_mut()[DATA as usize + 24] = 2;
        // Discovery caches the checked field contract, not the current value.
        let plan = ChargePlan::discover(&mut r, BUTTON, DATA, SHOW).unwrap();
        for (charge, cancel, expected) in [(2, 12618_i32, None), (1, 0, None), (0, 12618, None), (1, -1, None), (0, 0, Some(false)), (1, 12618, Some(true))] {
            let mut bytes = r.memory.0.borrow_mut(); bytes[DATA as usize + 24] = charge;
            bytes[DATA as usize + 28..DATA as usize + 32].copy_from_slice(&cancel.to_le_bytes()); drop(bytes);
            let retained = vec![(BUTTON, 0x100_u64.to_le_bytes().to_vec())]; let mut consumed = retained.clone();
            assert_eq!(plan.sample(&r, &mut consumed), expected);
            if expected.is_none() { assert_eq!(consumed, retained); }
            else { assert!(check(&r, &consumed).is_ok()); }
        }
        let mut unavailable = plan.clone(); unavailable.charge = 0x4000;
        assert_eq!(unavailable.sample(&r, &mut Vec::new()), None);
    }
    #[test]
    fn seven_digit_charge_cancel_ids_remain_typed_bounded_and_locally_owned() {
        let mut r=fixture(2,"ShowSkillData",true);
        let plan=ChargePlan::discover(&mut r,BUTTON,DATA,SHOW).unwrap();
        for (id,expected) in [(1_013_110_i32,Some(true)),(2_013_110,Some(true)),(10_000_000,Some(true)),
            (0,None),(-1,None),(10_000_001,None)] {
            r.memory.0.borrow_mut()[DATA as usize+28..DATA as usize+32].copy_from_slice(&id.to_le_bytes());
            let mut consumed=Vec::new();assert_eq!(plan.sample(&r,&mut consumed),expected,"cancel ID {id}");
            if expected.is_some(){assert!(check(&r,&consumed).is_ok());}
        }
        r.memory.0.borrow_mut()[DATA as usize+28..DATA as usize+32].copy_from_slice(&2_013_110_i32.to_le_bytes());
        put(&mut r.memory.0.borrow_mut(),DATA+16,OTHER);
        assert_eq!(plan.sample(&r,&mut Vec::new()),None);
        let mut r=fixture(2,"ShowSkillData",true);
        let table=r.pointer(0x300+0x78).unwrap();let ty=r.pointer(table+2*32+8).unwrap();
        r.memory.0.borrow_mut()[ty as usize+8..ty as usize+12].copy_from_slice(&(12u32<<16).to_le_bytes());
        assert!(ChargePlan::discover(&mut r,BUTTON,DATA,SHOW).is_err(),"numeric ID field must remain Int32");
    }
}
