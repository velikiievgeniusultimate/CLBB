//! Native SimpleLockHeroSkill portraits. Reading and pointer routing never use
//! world projection: its hover is allowed to move the game camera.
use crate::target_selection::{address, boolean, fields, guid, int, ptr};
use crate::{
    platform::Memory,
    reflection::{Reflection, Result, u64_at},
    source::{ButtonPlan, Checks, capture, check},
    unity::{self, Widget},
};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Portrait {
    pub index: usize,
    pub guid: u32,
    pub owner: u64,
    pub rendered: bool,
    pub native_cancel_entry: bool,
    pub native_collider: bool,
    pub center: [f64; 2],
    pub radius: [f64; 2],
}
#[derive(Clone, Debug, Serialize)]
pub struct Observation {
    pub backend: u64,
    pub slot: String,
    pub base: i32,
    pub active: i32,
    pub held: bool,
    pub selected: i32,
    pub camera_guid: u32,
    pub portraits: Vec<Portrait>,
    pub resolution: [f64; 2],
    pub cancel_point: Option<[f64; 2]>,
    pub native_cancel: Option<bool>,
    pub contact_owner: u64,
    pub contact_point: Option<[f64; 2]>,
}
#[derive(Clone)]
struct Icon {
    object: u64,
    checks: Checks,
    active: u64,
    cancel: u64,
    collider: u64,
    id: (u64, u8),
    index: u64,
    widget: Widget,
}
#[derive(Clone)]
pub struct Plan {
    checks: Checks,
    backend: u64,
    slot: String,
    base: u64,
    active: u64,
    data_base: u64,
    data_active: u64,
    icons: Vec<Icon>,
    entities: u64,
    selected: u64,
    camera_guid: (u64, u8),
    follow: u64,
    camera: u64,
    contacts: Option<crate::target_contacts::ContactRegistryPlan>,
}
fn array<M: Memory>(
    r: &mut Reflection<M>,
    at: u64,
    name: &str,
    consumed: &mut Checks,
) -> Result<Vec<u64>> {
    let object = ptr(r, consumed, at)?;
    let h = capture(r, consumed, object, 32)?;
    let count = u64_at(&h, 24);
    if count != 5 {
        return Err("portrait_array_count".into());
    }
    let c = capture(r, consumed, u64_at(&h, 0), 0x50)?;
    if r.cstring(u64_at(&c, 16))? != format!("{name}[]") || r.class(u64_at(&c, 0x40))?.name != name
    {
        return Err("portrait_array_type".into());
    }
    Ok(capture(r, consumed, object + 32, 40)?
        .chunks_exact(8)
        .map(|v| u64_at(v, 0))
        .collect())
}
impl Plan {
    pub fn discover<M: Memory>(
        r: &mut Reflection<M>,
        ui: u64,
        buttons: &[ButtonPlan],
        show: u64,
        camera: u64,
    ) -> Result<Self> {
        let mut checks = Vec::new();
        let (_, uf) = fields(r, &mut checks, ui)?;
        let logic = ptr(r, &mut checks, address(ui, &uf, "m_LogicExtra", &[18])?.0)?;
        let (name, lf) = fields(r, &mut checks, logic)?;
        if name != "KeySkillLogicExtra" {
            return Err("portrait_logic_type".into());
        }
        let backend = ptr(
            r,
            &mut checks,
            address(logic, &lf, "m_UiLockHeroSkill", &[18])?.0,
        )?;
        let (name, f) = fields(r, &mut checks, backend)?;
        if name != "SimpleLockHeroSkill" {
            return Err("portrait_backend_type".into());
        }
        if ptr(r, &mut checks, address(backend, &f, "m_pOwner", &[18])?.0)? != show {
            return Err("portrait_foreign_owner".into());
        }
        let follow = ptr(r, &mut checks, address(backend, &f, "m_followGo", &[18])?.0)?;
        let (name, gf) = fields(r, &mut checks, follow)?;
        if name != "GameObject" {
            return Err("portrait_follow_type".into());
        }
        let native = ptr(
            r,
            &mut checks,
            address(follow, &gf, "m_CachedPtr", &[24])?.0,
        )?;
        if ptr(
            r,
            &mut checks,
            native.checked_add(40).ok_or("portrait_native_overflow")?,
        )? != follow
        {
            return Err("portrait_follow_owner".into());
        }
        let base = address(backend, &f, "m_iBindInitSkillId", &[8])?.0;
        let active = address(backend, &f, "m_iBindSkillId", &[8])?.0;
        let base_id = int(r, &mut checks, base)?;
        let active_id = int(r, &mut checks, active)?;
        // First measured frontend contract. Do not silently enable untested
        // SimpleLock modes belonging to another skill or hero.
        if base_id != 10430 || active_id != 10431 {
            return Err("portrait_unverified_binding".into());
        }
        let b = buttons
            .iter()
            .find(|b| b.kind == "skill" && r.integer(b.skill).ok() == Some(base_id))
            .ok_or("portrait_button_absent")?;
        let (_, df) = fields(r, &mut checks, b.data)?;
        let icon_array = address(backend, &f, "_nearHeroIcon", &[29])?.0;
        let objects = array(r, icon_array, "NearIconInfo", &mut checks)?;
        let mut icons = Vec::new();
        for object in objects {
            if object == 0 || icons.iter().any(|i: &Icon| i.object == object) {
                return Err("portrait_duplicate_or_null_icon".into());
            }
            let mut owned = Vec::new();
            let (name, of) = fields(r, &mut owned, object)?;
            if name != "NearIconInfo" {
                return Err("portrait_icon_type".into());
            }
            let head = ptr(r, &mut owned, address(object, &of, "m_HeadIcon", &[18])?.0)?;
            let widget = Widget::new(r, head)?;
            icons.push(Icon {
                object,
                checks: owned,
                widget,
                active: address(object, &of, "m_bActive", &[2])?.0,
                cancel: address(object, &of, "m_bCancel", &[2])?.0,
                collider: address(object, &of, "m_bCollider", &[2])?.0,
                id: address(object, &of, "m_uGuid", &[9])?,
                index: address(object, &of, "m_iIndex", &[8])?.0,
            });
        }
        check(r, &checks)?;
        Ok(Self {
            contacts: crate::target_contacts::discover_ui_camera_class(r, camera)
                .and_then(|k| crate::target_contacts::ContactRegistryPlan::discover(r, k))
                .ok(),
            follow,
            camera,
            checks,
            backend,
            slot: b.slot.clone(),
            base,
            active,
            data_base: address(b.data, &df, "m_InitSkillID", &[8])?.0,
            data_active: address(b.data, &df, "m_TranID", &[8])?.0,
            icons,
            entities: address(backend, &f, "_nearEntities", &[29])?.0,
            selected: address(backend, &f, "_selectIndex", &[8])?.0,
            camera_guid: address(backend, &f, "_cameraMoveGuid", &[9])?,
        })
    }
    pub fn validate<M: Memory>(&self, r: &Reflection<M>) -> Result<()> {
        check(r, &self.checks)
    }
    pub fn sample<M: Memory>(
        &mut self,
        r: &mut Reflection<M>,
        matrix: u64,
        held_slot: &str,
        resolution: [f64; 2],
        consumed: &mut Checks,
    ) -> Result<Observation> {
        self.validate(r)?;
        consumed.extend(self.checks.iter().cloned());
        let base = int(r, consumed, self.base)?;
        let active = int(r, consumed, self.active)?;
        let current = int(r, consumed, self.data_active)?;
        if int(r, consumed, self.data_base)? != base || ![base, active].contains(&current) {
            return Err("portrait_inactive_form".into());
        }
        let selected = int(r, consumed, self.selected)?;
        if !(-1..5).contains(&selected) {
            return Err("portrait_selection_index".into());
        }
        let camera_guid = guid(r, consumed, self.camera_guid)?;
        let entities = array(r, self.entities, "ShowEntity", consumed)?;
        let (view, projection) = unity::matrices(&capture(r, consumed, matrix, 128)?)?;
        let mut portraits = Vec::new();
        for (index, i) in self
            .icons
            .iter_mut()
            .enumerate()
            .filter(|_| current == active)
        {
            check(r, &i.checks)?;
            consumed.extend(i.checks.iter().cloned());
            // Membership belongs to the native target roster. Hovering may
            // hide/reconfigure a sprite or disable its click collider without
            // removing the enemy from that roster. Those are not revocations.
            if !boolean(r, consumed, i.active)? {
                continue;
            }
            let native_cancel_entry = boolean(r, consumed, i.cancel)?;
            let native_collider = boolean(r, consumed, i.collider)?;
            let id = guid(r, consumed, i.id)?;
            let entity = entities[index];
            if id == 0 || entity == 0 || int(r, consumed, i.index)? != index as i32 {
                return Err("portrait_recipient_unknown".into());
            }
            let (name, ef) = fields(r, consumed, entity)?;
            if name != "ShowPlayer"
                || guid(r, consumed, address(entity, &ef, "m_uGuid", &[9])?)? != id
            {
                return Err("portrait_entity_mismatch".into());
            }
            if portraits.iter().any(|p: &Portrait| p.guid == id) {
                return Err("portrait_duplicate_recipient".into());
            }
            // Opening/closing SimpleLock reparents the same owned portrait
            // between its preview and expanded containers. Rebuild its path,
            // never revoke an otherwise valid target for the entire hold.
            i.widget.refresh_path(r)?;
            consumed.extend(i.widget.path.checks.iter().cloned());
            let rendered = i.widget.visible(r)?;
            let (center, radius) = i.widget.geometry(r, &view, &projection, true)?;
            if !unity::onscreen(center)
                || radius
                    .iter()
                    .any(|v| !v.is_finite() || *v <= 0. || *v > 0.15)
            {
                return Err("portrait_invalid_geometry".into());
            }
            portraits.push(Portrait {
                index,
                guid: id,
                owner: i.object,
                rendered,
                native_cancel_entry,
                native_collider,
                center,
                radius,
            });
        }
        let mut contact_owner = 0;
        let mut contact_point = None;
        {
            if self
                .contacts
                .as_ref()
                .is_some_and(|p| p.validate(r).is_err())
            {
                self.contacts = None;
            }
            if self.contacts.is_none() {
                self.contacts = crate::target_contacts::discover_ui_camera_class(r, self.camera)
                    .and_then(|k| crate::target_contacts::ContactRegistryPlan::discover(r, k))
                    .ok();
            }
            let checkpoint = consumed.len();
            if let Some(c) = self
                .contacts
                .as_ref()
                .and_then(|p| p.sample(r, self.follow, 0, consumed).ok())
                .filter(|c| c.present && c.press_started)
            {
                let [w, h] = resolution;
                if w > 1. && h > 1. {
                    contact_owner = c.contact_owner_id;
                    contact_point = Some([c.raw_pos[0] / (w - 1.), (h - c.raw_pos[1]) / (h - 1.)]);
                }
            } else {
                consumed.truncate(checkpoint);
            }
        }
        Ok(Observation {
            backend: self.backend,
            slot: self.slot.clone(),
            base,
            active: current,
            held: held_slot == self.slot || contact_owner != 0,
            selected,
            camera_guid,
            portraits,
            contact_owner,
            contact_point,
            resolution,
            cancel_point: None,
            native_cancel: None,
        })
    }
}
