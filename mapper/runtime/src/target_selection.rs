//! Cached native UI target-selection observations. No input or native calls.
//! The persistent UI contact is joined through its owned pressed/dragged GO;
//! neither an Android/host finger ID nor a world-space indicator is an ACK.
use crate::{
    platform::Memory,
    reflection::{Field, Reflection, Result, i32_at, u64_at},
    source::{Checks, capture, check, flag, number, text, v3},
    target_contacts::{self, ContactRegistryPlan},
};
use clbb_moba_core::controller::snapshot as s;
use std::collections::HashMap;

pub(crate) const SOURCE: &str = "UILockHeroSkill.UICamera.activeTouches";
pub(crate) fn fields<M: Memory>(
    r: &mut Reflection<M>,
    checks: &mut Checks,
    object: u64,
) -> Result<(String, HashMap<String, Field>)> {
    if object == 0 {
        return Err("target_owner_absent".into());
    }
    let mut class = u64_at(&capture(r, checks, object, 8)?, 0);
    let mut name = String::new();
    let mut out = HashMap::new();
    for _ in 0..20 {
        if class == 0 {
            break;
        }
        let c = r.class(class)?;
        if name.is_empty() {
            name = c.name.clone();
        }
        for (n, f) in c.fields {
            out.entry(n).or_insert(f);
        }
        class = c.parent;
    }
    Ok((name, out))
}
pub(crate) fn address(
    object: u64,
    metadata: &HashMap<String, Field>,
    name: &str,
    kinds: &[u8],
) -> Result<(u64, u8)> {
    let f = metadata
        .get(name)
        .ok_or_else(|| format!("target_field_absent:{name}"))?;
    if f.is_static || !kinds.contains(&f.kind) || f.offset > 1_048_576 {
        return Err(format!("target_field_type:{name}"));
    }
    Ok((
        object
            .checked_add(f.offset)
            .ok_or("target_field_overflow")?,
        f.kind,
    ))
}
pub(crate) fn ptr<M: Memory>(r: &Reflection<M>, checks: &mut Checks, at: u64) -> Result<u64> {
    Ok(u64_at(&capture(r, checks, at, 8)?, 0))
}
pub(crate) fn int<M: Memory>(r: &Reflection<M>, checks: &mut Checks, at: u64) -> Result<i32> {
    Ok(i32_at(&capture(r, checks, at, 4)?, 0))
}
pub(crate) fn boolean<M: Memory>(r: &Reflection<M>, checks: &mut Checks, at: u64) -> Result<bool> {
    match capture(r, checks, at, 1)?[0] {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err("target_invalid_bool".into()),
    }
}
pub(crate) fn guid<M: Memory>(r: &Reflection<M>, checks: &mut Checks, field: (u64, u8)) -> Result<u32> {
    let raw = capture(r, checks, field.0, 4)?;
    if field.1 == 8 && i32_at(&raw, 0) < 0 {
        return Err("target_negative_guid".into());
    }
    Ok(u32::from_le_bytes(raw.try_into().unwrap()))
}
fn optional<M: Memory, T>(
    r: &Reflection<M>,
    checks: &mut Checks,
    read: impl FnOnce(&Reflection<M>, &mut Checks) -> Result<T>,
) -> Option<T> {
    let checkpoint = checks.len();
    match read(r, checks) {
        Ok(value) => Some(value),
        Err(_) => {
            checks.truncate(checkpoint);
            None
        }
    }
}
fn reference<M: Memory>(
    r: &Reflection<M>,
    checks: &mut Checks,
    at: u64,
    expected: u64,
) -> Result<()> {
    if expected == 0 || ptr(r, checks, at)? != expected {
        return Err("target_foreign_owner".into());
    }
    Ok(())
}
#[derive(Clone)]
struct GameObjectPlan {
    object: u64,
    checks: Checks,
}
impl GameObjectPlan {
    fn new<M: Memory>(r: &mut Reflection<M>, object: u64) -> Result<Self> {
        let mut checks = Vec::new();
        let (name, f) = fields(r, &mut checks, object)?;
        if name != "GameObject" {
            return Err("target_gameobject_type".into());
        }
        let native = ptr(r, &mut checks, address(object, &f, "m_CachedPtr", &[24])?.0)?;
        if native == 0 {
            return Err("target_gameobject_destroyed".into());
        }
        reference(
            r,
            &mut checks,
            native.checked_add(40).ok_or("target_native_overflow")?,
            object,
        )?;
        check(r, &checks)?;
        Ok(Self { object, checks })
    }
    fn sample<M: Memory>(&self, r: &Reflection<M>, consumed: &mut Checks) -> Result<()> {
        check(r, &self.checks)?;
        consumed.extend(self.checks.iter().cloned());
        Ok(())
    }
}
#[derive(Clone)]
struct PlayerPlan {
    object: u64,
    checks: Checks,
    guid: (u64, u8),
}
#[derive(Clone)]
struct IconPlan {
    object: u64,
    checks: Checks,
    self_go: GameObjectPlan,
    guid: (u64, u8),
    player_link: u64,
    player: Option<PlayerPlan>,
    forbid: u64,
    cancel: u64,
    collider: u64,
}
#[derive(Clone, Copy)]
struct IconObservation {
    owner: u64,
    self_id: u64,
    guid: u32,
    player_matches: Option<bool>,
    forbid: bool,
    cancel: bool,
    collider: bool,
}
impl IconPlan {
    fn new<M: Memory>(r: &mut Reflection<M>, object: u64) -> Result<Self> {
        let mut checks = Vec::new();
        let (name, f) = fields(r, &mut checks, object)?;
        if name != "NearIconInfo" {
            return Err("target_icon_type".into());
        }
        let self_id = ptr(r, &mut checks, address(object, &f, "m_Self", &[18])?.0)?;
        let self_go = GameObjectPlan::new(r, self_id)?;
        let plan = Self {
            object,
            checks,
            self_go,
            guid: address(object, &f, "m_uGuid", &[8, 9])?,
            player_link: address(object, &f, "player", &[18])?.0,
            player: None,
            forbid: address(object, &f, "m_bForbid", &[2])?.0,
            cancel: address(object, &f, "m_bCancel", &[2])?.0,
            collider: address(object, &f, "m_bCollider", &[2])?.0,
        };
        check(r, &plan.checks)?;
        Ok(plan)
    }
    fn sample<M: Memory>(
        &mut self,
        r: &mut Reflection<M>,
        consumed: &mut Checks,
        current_go: u64,
    ) -> Result<IconObservation> {
        check(r, &self.checks)?;
        consumed.extend(self.checks.iter().cloned());
        self.self_go.sample(r, consumed)?;
        if self.self_go.object != current_go {
            return Ok(IconObservation {
                owner: self.object,
                self_id: self.self_go.object,
                guid: 0,
                player_matches: None,
                forbid: false,
                cancel: false,
                collider: false,
            });
        }
        let id = guid(r, consumed, self.guid)?;
        let mut player_matches = None;
        if self.self_go.object == current_go {
            let player = ptr(r, consumed, self.player_link)?;
            if player != 0 && id != 0 {
                if self
                    .player
                    .as_ref()
                    .is_none_or(|p| p.object != player || check(r, &p.checks).is_err())
                {
                    let mut checks = Vec::new();
                    let (name, pf) = fields(r, &mut checks, player)?;
                    if name != "ShowPlayer" {
                        return Err("target_player_type".into());
                    }
                    self.player = Some(PlayerPlan {
                        object: player,
                        checks,
                        guid: address(player, &pf, "m_uGuid", &[8, 9])?,
                    });
                }
                let plan = self.player.as_ref().unwrap();
                check(r, &plan.checks)?;
                consumed.extend(plan.checks.iter().cloned());
                player_matches = Some(guid(r, consumed, plan.guid)? == id);
            }
        }
        Ok(IconObservation {
            owner: self.object,
            self_id: self.self_go.object,
            guid: id,
            player_matches,
            forbid: boolean(r, consumed, self.forbid)?,
            cancel: boolean(r, consumed, self.cancel)?,
            collider: boolean(r, consumed, self.collider)?,
        })
    }
}
/// A native point receipt and the recipient authorization are independent.
/// A partially updated GUID/player join is unknown, never a negative result.
fn recipient_valid(icon: &IconObservation, selected_guid: u32) -> Option<bool> {
    if icon.forbid || icon.cancel || !icon.collider {
        Some(false)
    } else if icon.guid != 0 && selected_guid == icon.guid && icon.player_matches == Some(true) {
        Some(true)
    } else {
        None
    }
}
#[derive(Clone)]
struct IconListPlan {
    checks: Checks,
    items: u64,
    count: u64,
    version: u64,
    array: u64,
    capacity: u64,
    objects: [u64; 16],
    len: usize,
    icons: Vec<IconPlan>,
}
impl IconListPlan {
    fn new<M: Memory>(r: &mut Reflection<M>, list: u64) -> Result<Self> {
        let mut checks = Vec::new();
        let (name, f) = fields(r, &mut checks, list)?;
        if name != "List`1" {
            return Err("target_icon_list_type".into());
        }
        let items = address(list, &f, "_items", &[29])?.0;
        let count = address(list, &f, "_size", &[8])?.0;
        let version = address(list, &f, "_version", &[8])?.0;
        let array = ptr(r, &mut Vec::new(), items)?;
        let len = int(r, &mut Vec::new(), count)?;
        if !(0..=16).contains(&len) || array == 0 {
            return Err("target_icon_list_bounds".into());
        }
        let array_header = capture(r, &mut checks, array, 32)?;
        let capacity = u64_at(&array_header, 24);
        if capacity < len as u64 || capacity > 64 {
            return Err("target_icon_array_bounds".into());
        }
        let class = capture(r, &mut checks, u64_at(&array_header, 0), 0x50)?;
        if r.cstring(u64_at(&class, 16))? != "NearIconInfo[]"
            || r.class(u64_at(&class, 0x40))?.name != "NearIconInfo"
        {
            return Err("target_icon_array_type".into());
        }
        let raw = r.read(array + 32, len as usize * 8)?;
        let mut objects = [0; 16];
        let mut icons = Vec::new();
        for (i, b) in raw.chunks_exact(8).enumerate() {
            let object = u64_at(b, 0);
            if object == 0 {
                return Err("target_null_icon".into());
            }
            if objects[..i].contains(&object) {
                return Err("target_duplicate_icon".into());
            }
            objects[i] = object;
            icons.push(IconPlan::new(r, object)?);
        }
        let plan = Self {
            checks,
            items,
            count,
            version,
            array,
            capacity,
            objects,
            len: len as usize,
            icons,
        };
        let mut consumed = Vec::new();
        plan.topology(r, &mut consumed)?;
        check(r, &consumed)?;
        Ok(plan)
    }
    fn topology<M: Memory>(&self, r: &Reflection<M>, consumed: &mut Checks) -> Result<()> {
        check(r, &self.checks)?;
        consumed.extend(self.checks.iter().cloned());
        if ptr(r, consumed, self.items)? != self.array
            || int(r, consumed, self.count)? != self.len as i32
        {
            return Err("target_icon_topology_changed".into());
        }
        int(r, consumed, self.version)?;
        if ptr(r, consumed, self.array + 24)? != self.capacity {
            return Err("target_icon_capacity_changed".into());
        }
        let raw = capture(r, consumed, self.array + 32, self.len * 8)?;
        if raw
            .chunks_exact(8)
            .enumerate()
            .any(|(i, b)| u64_at(b, 0) != self.objects[i])
        {
            return Err("target_icon_items_changed".into());
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct TargetPlan {
    checks: Checks,
    data: u64,
    backend: u64,
    base: u64,
    active: u64,
    use_skill: u64,
    follow: u64,
    cancel_go: u64,
    icon_list: u64,
    state: u64,
    target_guid: (u64, u8),
    native_cancel: u64,
    unselectable: u64,
    owned_camera: u64,
    contacts: Option<ContactRegistryPlan>,
    contact_retry: i64,
    contact_attempts: u8,
    was_held: bool,
    had_contact: bool,
    follow_plan: Option<GameObjectPlan>,
    cancel_plan: Option<GameObjectPlan>,
    cancel_widget: Option<(u64, crate::target_cancel_geometry::CachedCancelWidgetPlan)>,
    cancel_object: u64,
    cancel_retry: i64,
    cancel_attempts: u8,
    icons: Option<(u64, IconListPlan)>,
}
impl TargetPlan {
    pub(crate) fn is_family<M: Memory>(
        r: &mut Reflection<M>,
        class: &crate::reflection::Class,
    ) -> Result<bool> {
        let mut c = class.clone();
        for _ in 0..20 {
            if c.name == "KeySkillLockHero" {
                return Ok(true);
            }
            if c.parent == 0 {
                return Ok(false);
            }
            c = r.class(c.parent)?;
        }
        Err("target_ancestry_bounds".into())
    }
    pub(crate) fn discover<M: Memory>(
        r: &mut Reflection<M>,
        ui: u64,
        button: u64,
        data: u64,
        show: u64,
        owned_camera: u64,
    ) -> Result<Self> {
        let mut checks = Vec::new();
        let (class, bf) = fields(r, &mut checks, button)?;
        let actual = r.class(r.pointer(button)?)?;
        if !Self::is_family(r, &actual)? {
            return Err(format!("target_button_type:{class}"));
        }
        let (data_class, df) = fields(r, &mut checks, data)?;
        let (ui_class, uf) = fields(r, &mut checks, ui)?;
        let (_, sf) = fields(r, &mut checks, show)?;
        if data_class != "ShowSkillData" || ui_class != "UIKeySkill" {
            return Err("target_ui_data_type".into());
        }
        reference(
            r,
            &mut checks,
            address(button, &bf, "m_SkillData", &[18])?.0,
            data,
        )?;
        reference(
            r,
            &mut checks,
            address(button, &bf, "m_pOwner", &[18])?.0,
            show,
        )?;
        let comp = ptr(r, &mut checks, address(show, &sf, "m_SkillComp", &[18])?.0)?;
        reference(
            r,
            &mut checks,
            address(data, &df, "m_SkillComp", &[18])?.0,
            comp,
        )?;
        let (comp_class, cf) = fields(r, &mut checks, comp)?;
        if comp_class != "ShowSkillComp" {
            return Err("target_local_component_type".into());
        }
        reference(
            r,
            &mut checks,
            address(comp, &cf, "m_pOwner", &[18])?.0,
            show,
        )?;
        // Exact observed frontend mode, independent of hero or numeric skill ID.
        if int(
            r,
            &mut checks,
            address(button, &bf, "m_SkillUseType", &[8, 17])?.0,
        )? != 1
            || int(r, &mut checks, address(button, &bf, "m_iBit", &[8, 9])?.0)? != 64
        {
            return Err("target_frontend_mode_unqualified".into());
        }
        let logic = ptr(r, &mut checks, address(ui, &uf, "m_LogicExtra", &[18])?.0)?;
        let (name, lf) = fields(r, &mut checks, logic)?;
        if name != "KeySkillLogicExtra" {
            return Err("target_logic_type".into());
        }
        let backend = ptr(
            r,
            &mut checks,
            address(logic, &lf, "m_UiLockHeroSkill", &[18])?.0,
        )?;
        let (name, f) = fields(r, &mut checks, backend)?;
        if name != "UILockHeroSkill" {
            return Err("target_backend_unqualified".into());
        }
        let contacts = target_contacts::discover_ui_camera_class(r, owned_camera)
            .and_then(|class| ContactRegistryPlan::discover(r, class))
            .ok();
        check(r, &checks)?;
        Ok(Self {
            checks,
            data,
            backend,
            base: address(data, &df, "m_InitSkillID", &[8])?.0,
            active: address(data, &df, "m_TranID", &[8])?.0,
            use_skill: address(backend, &f, "<iUseSkillId>k__BackingField", &[8])?.0,
            follow: address(backend, &f, "m_followGo", &[18])?.0,
            cancel_go: address(backend, &f, "m_skillCancel", &[18])?.0,
            icon_list: address(backend, &f, "lNearHeroIcon", &[21])?.0,
            state: address(backend, &f, "m_eState", &[8, 17])?.0,
            target_guid: address(backend, &f, "iSkillTargetGuid", &[8, 9])?,
            native_cancel: address(backend, &f, "m_bCancelSkill", &[2])?.0,
            unselectable: address(backend, &f, "m_bSelectUnselectableTarget", &[2])?.0,
            owned_camera,
            contacts,
            contact_retry: 0,
            contact_attempts: 0,
            was_held: false,
            had_contact: false,
            follow_plan: None,
            cancel_plan: None,
            cancel_widget: None,
            cancel_object: 0,
            cancel_retry: 0,
            cancel_attempts: 0,
            icons: None,
        })
    }
    pub(crate) fn validate<M: Memory>(&self, r: &Reflection<M>) -> Result<()> {
        check(r, &self.checks)
    }
    /// Cancellation uses the rendered widget owned by this backend. A raw
    /// GameObject anchor or another skill's camera is never a fallback point.
    pub(crate) fn cancel_uv<M: Memory>(
        &mut self,
        r: &mut Reflection<M>,
        now: i64,
        consumed: &mut Checks,
    ) -> Result<[f64; 2]> {
        self.validate(r)?;
        consumed.extend(self.checks.iter().cloned());
        let object = ptr(r, consumed, self.cancel_go)?;
        if object == 0 {
            return Err("target_cancel_object_absent".into());
        }
        if self.cancel_object != object
            || self
                .cancel_widget
                .as_ref()
                .is_some_and(|(_, plan)| plan.validate(r).is_err())
        {
            self.cancel_object = object;
            self.cancel_widget = None;
            self.cancel_retry = 0;
            self.cancel_attempts = 0;
        }
        if self.cancel_widget.is_none() {
            if now < self.cancel_retry {
                return Err("target_cancel_metadata_retry_pending".into());
            }
            match crate::target_cancel_geometry::CachedCancelWidgetPlan::discover(r, object) {
                Ok(plan) => self.cancel_widget = Some((object, plan)),
                Err(error) => {
                    self.cancel_attempts = self.cancel_attempts.saturating_add(1);
                    self.cancel_retry = now.saturating_add(if self.cancel_attempts <= 3 {
                        0
                    } else {
                        20_000_000
                    });
                    return Err(error);
                }
            }
        }
        self.cancel_widget.as_ref().unwrap().1.sample(r, consumed)
    }
    pub(crate) fn sample<M: Memory>(
        &mut self,
        r: &mut Reflection<M>,
        frame: &s::Frame,
        slot: &str,
        now: i64,
        consumed: &mut Checks,
    ) -> Result<s::SelectionInput> {
        self.validate(r)?;
        consumed.extend(self.checks.iter().cloned());
        let base = int(r, consumed, self.base)?;
        let active = int(r, consumed, self.active)?;
        if !(1..=10_000_000).contains(&base) || !(1..=10_000_000).contains(&active) {
            return Err("target_skill_identity_unknown".into());
        }
        // Global current-button/press flags can belong to another simultaneous
        // contact; only this owned backend/contact authorizes Target feedback.
        let use_skill = int(r, consumed, self.use_skill)?;
        let state = int(r, consumed, self.state)?;
        let native_cancel = optional(r, consumed, |r, c| boolean(r, c, self.native_cancel));
        let unselectable = optional(r, consumed, |r, c| boolean(r, c, self.unselectable));
        let selected_guid = optional(r, consumed, |r, c| guid(r, c, self.target_guid));
        let mut packet = s::SelectionInput {
            kind: number(2.),
            ready: flag(true),
            coherent: flag(true),
            source: text(SOURCE),
            reason: text(if active == base {
                "target_contact_unavailable"
            } else {
                "selector_phase_complete"
            }),
            token: frame.token,
            slot: text(slot),
            hero_id: frame.hero_id,
            base_skill_id: number(base as f64),
            active_skill_id: number(active as f64),
            native_skill_id: frame.skill_id,
            owner_id: self.data,
            backend_owner_id: self.backend,
            sample_start_ns: number(now as f64),
            sample_end_ns: number(now as f64),
            panel_visible: if use_skill == base && state == 4 {
                flag(true)
            } else if use_skill == base && state == 3 {
                flag(false)
            } else {
                Default::default()
            },
            backend_state: number(state as f64),
            target_guid: selected_guid.map(|v| number(v as f64)).unwrap_or_default(),
            native_cancel: native_cancel.map(flag).unwrap_or_default(),
            native_unselectable: unselectable.map(flag).unwrap_or_default(),
            ..Default::default()
        };
        // The observed ConstantShow mode has state4 during the panel capture,
        // state3 idle/end; unknown and native blocked7/8 are not false absence.
        // This qualification belongs only to the exact UILock frontend mode.
        let native_scope = active == base && use_skill == base;
        let attempt_active = native_scope && state == 4;
        if !attempt_active {
            packet.input_ready = flag(false);
            self.was_held = false;
        }
        if attempt_active && !self.was_held {
            self.contact_retry = 0;
            self.contact_attempts = 0;
            self.cancel_retry = 0;
            self.cancel_attempts = 0;
        }
        self.was_held = attempt_active;
        let follow = optional(r, consumed, |r, c| ptr(r, c, self.follow)).unwrap_or(0);
        let cancel = optional(r, consumed, |r, c| ptr(r, c, self.cancel_go)).unwrap_or(0);
        if follow == 0 {
            return Ok(packet);
        }
        let owner_checkpoint = consumed.len();
        if self
            .follow_plan
            .as_ref()
            .is_none_or(|p| p.object != follow || check(r, &p.checks).is_err())
        {
            match GameObjectPlan::new(r, follow) {
                Ok(plan) => self.follow_plan = Some(plan),
                Err(_) => {
                    consumed.truncate(owner_checkpoint);
                    return Ok(packet);
                }
            }
        }
        if self
            .follow_plan
            .as_ref()
            .unwrap()
            .sample(r, consumed)
            .is_err()
        {
            consumed.truncate(owner_checkpoint);
            return Ok(packet);
        }
        // The cancel widget is optional to ordinary target-point receipt. Its
        // absence can never revoke the independently owned pressed/dragged GO.
        let cancel_checkpoint = consumed.len();
        let cancel = if cancel == 0 {
            0
        } else {
            if self
                .cancel_plan
                .as_ref()
                .is_none_or(|p| p.object != cancel || check(r, &p.checks).is_err())
            {
                self.cancel_plan = GameObjectPlan::new(r, cancel).ok();
            }
            if self
                .cancel_plan
                .as_ref()
                .is_some_and(|p| p.object == cancel && p.sample(r, consumed).is_ok())
            {
                cancel
            } else {
                consumed.truncate(cancel_checkpoint);
                0
            }
        };
        if self
            .contacts
            .as_ref()
            .is_some_and(|p| p.validate(r).is_err())
        {
            self.contacts = None;
            self.contact_retry = 0;
        }
        if self.contacts.is_none()
            && (attempt_active || self.had_contact)
            && now >= self.contact_retry
        {
            self.contacts = target_contacts::discover_ui_camera_class(r, self.owned_camera)
                .and_then(|class| ContactRegistryPlan::discover(r, class))
                .ok();
            self.contact_attempts = self.contact_attempts.saturating_add(1);
            self.contact_retry = now.saturating_add(if self.contact_attempts <= 3 {
                0
            } else {
                20_000_000
            });
        }
        let Some(registry) = &self.contacts else {
            return Ok(packet);
        };
        let contact_checkpoint = consumed.len();
        let contact = match registry.sample(r, follow, cancel, consumed) {
            Ok(value) => value,
            Err(_) => {
                consumed.truncate(contact_checkpoint);
                return Ok(packet);
            }
        };
        packet.contact_present = flag(contact.present);
        packet.has_drag = flag(contact.present && contact.drag_started);
        if !contact.present {
            self.had_contact = false;
            packet.input_ready = flag(false);
            packet.target_cancel_hit = flag(false);
            if attempt_active {
                packet.reason = text("target_contact_absent");
            }
            return Ok(packet);
        }
        self.had_contact = true;
        packet.native_contact_id = number(contact.native_id as f64);
        packet.contact_owner_id = contact.contact_owner_id;
        packet.raw_touch_xyz = v3([contact.raw_pos[0], contact.raw_pos[1], 0.]);
        packet.input_ready = flag(native_scope && contact.press_started && contact.drag_started);
        packet.target_cancel_hit = if !attempt_active {
            flag(false)
        } else if cancel != 0 {
            native_cancel
                .map(|value| flag(contact.cancel_hit && value))
                .unwrap_or_default()
        } else {
            Default::default()
        };
        if !attempt_active {
            return Ok(packet);
        }
        packet.reason = text("target_recipient_unknown");
        if packet.target_cancel_hit.get() {
            packet.target_valid = flag(false);
            packet.reason = text("target_cancel_hit");
            return Ok(packet);
        }
        if unselectable == Some(true) {
            packet.target_valid = flag(false);
            packet.reason = text("target_unselectable");
            return Ok(packet);
        }
        let icon_checkpoint = consumed.len();
        let list = match ptr(r, consumed, self.icon_list) {
            Ok(list) => list,
            Err(_) => {
                consumed.truncate(icon_checkpoint);
                return Ok(packet);
            }
        };
        let topology_ok = self.icons.as_ref().is_some_and(|(object, plan)| {
            *object == list && plan.topology(r, &mut Vec::new()).is_ok()
        });
        if !topology_ok {
            match IconListPlan::new(r, list) {
                Ok(plan) => self.icons = Some((list, plan)),
                Err(_) => {
                    consumed.truncate(icon_checkpoint);
                    self.icons = None;
                    return Ok(packet);
                }
            }
        }
        let icons = &mut self.icons.as_mut().unwrap().1;
        if icons.topology(r, consumed).is_err() {
            consumed.truncate(icon_checkpoint);
            self.icons = None;
            return Ok(packet);
        }
        let mut target = None;
        for icon in &mut icons.icons {
            let observed = match icon.sample(r, consumed, contact.current_go) {
                Ok(value) => value,
                Err(_) => {
                    consumed.truncate(icon_checkpoint);
                    self.icons = None;
                    return Ok(packet);
                }
            };
            if observed.self_id == contact.current_go {
                if target.is_some() {
                    return Err("target_ambiguous_current_icon".into());
                }
                target = Some(observed);
            }
        }
        if let Some(icon) = target {
            packet.target_owner_id = icon.owner;
            packet.target_self_id = icon.self_id;
            match selected_guid.and_then(|selected| recipient_valid(&icon, selected)) {
                Some(true) if unselectable == Some(false) => {
                    packet.target_valid = flag(true);
                    packet.reason = text("target_ready");
                }
                Some(false) => {
                    packet.target_valid = flag(false);
                    packet.reason = text("target_native_forbidden");
                }
                Some(true) | None => {}
            }
            // Allowed portrait with an unsettled backend GUID stays Unknown,
            // not a positive invalid target or a delayed/cached authorization.
        }
        Ok(packet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    struct Fake(RefCell<Vec<u8>>);
    impl Memory for Fake {
        fn regions(&self) -> Result<Vec<crate::platform::Region>> {
            Ok(Vec::new())
        }
        fn read(&self, address: u64, length: usize) -> Result<Vec<u8>> {
            self.0
                .borrow()
                .get(address as usize..address as usize + length)
                .map(|v| v.to_vec())
                .ok_or("test read outside memory".into())
        }
        fn read_into(&self, address: u64, bytes: &mut [u8]) -> Result<()> {
            bytes.copy_from_slice(&self.read(address, bytes.len())?);
            Ok(())
        }
    }
    fn write(r: &Reflection<Fake>, address: u64, bytes: &[u8]) {
        r.memory.0.borrow_mut()[address as usize..address as usize + bytes.len()]
            .copy_from_slice(bytes);
    }
    fn fixture(state: i32, active: i32) -> (Reflection<Fake>, TargetPlan, s::Frame) {
        let r = Reflection::new(Fake(RefCell::new(vec![0; 512]))).unwrap();
        for (address, value) in [
            (8, 5530),
            (16, active),
            (48, 5530),
            (72, state),
            (80, 13016),
        ] {
            write(&r, address, &value.to_le_bytes());
        }
        write(&r, 32, &999u64.to_le_bytes()); // an unrelated current native UI button
        write(&r, 40, &[1]);
        write(&r, 88, &[1]); // global-held and sticky backend cancel
        let plan = TargetPlan {
            checks: Vec::new(),
            data: 2000,
            backend: 3000,
            base: 8,
            active: 16,
            use_skill: 48,
            follow: 56,
            cancel_go: 64,
            icon_list: 96,
            state: 72,
            target_guid: (80, 9),
            native_cancel: 88,
            unselectable: 89,
            owned_camera: 4000,
            contacts: None,
            contact_retry: 0,
            contact_attempts: 0,
            was_held: false,
            had_contact: false,
            follow_plan: None,
            cancel_plan: None,
            cancel_widget: None,
            cancel_object: 0,
            cancel_retry: 0,
            cancel_attempts: 0,
            icons: None,
        };
        let frame = s::Frame {
            token: text("native-match"),
            hero_id: number(55.),
            skill_id: number(0.),
            ..Default::default()
        };
        (r, plan, frame)
    }
    #[test]
    fn target_panel_is_backend_owned_even_when_another_finger_changes_global_current() {
        let (mut r, mut plan, frame) = fixture(4, 5530);
        let mut consumed = Vec::new();
        let packet = plan
            .sample(&mut r, &frame, "skill:2", 1000, &mut consumed)
            .unwrap();
        assert!(packet.ready.get() && packet.coherent.get() && packet.panel_visible.get());
        assert_eq!(packet.kind.value, 2.);
        assert_eq!(packet.owner_id, 2000);
        assert_eq!(packet.backend_owner_id, 3000);
        assert_eq!(packet.native_skill_id.value, 0.);
        assert!(packet.contact_present.missing());
        assert!(packet.input_ready.missing());
        assert!(packet.target_valid.missing());
        assert!(packet.target_cancel_hit.missing());
        assert!(!consumed.is_empty());
        write(&r, 32, &123456u64.to_le_bytes());
        write(&r, 40, &[0]);
        assert!(
            check(&r, &consumed).is_ok(),
            "unrelated global capture cannot revoke independently owned Target feedback"
        );
    }
    #[test]
    fn sticky_idle_cancel_and_unknown_states_do_not_fabricate_native_end_or_contact_absence() {
        for state in [3, 7, 8, 91] {
            let (mut r, mut plan, frame) = fixture(state, 5530);
            let mut consumed = Vec::new();
            let packet = plan
                .sample(&mut r, &frame, "skill:2", 1000, &mut consumed)
                .unwrap();
            assert!(packet.native_cancel.get());
            assert!(packet.target_cancel_hit.missing());
            assert!(packet.contact_present.missing());
            assert!(!packet.input_ready.get());
            if state == 3 {
                assert!(packet.panel_visible.is_bool() && !packet.panel_visible.get());
            } else {
                assert!(packet.panel_visible.missing());
            }
            assert!(packet.target_valid.missing());
            assert!(check(&r, &consumed).is_ok());
            write(&r, 72, &4i32.to_le_bytes());
            assert!(check(&r, &consumed).is_err());
        }
    }
    #[test]
    fn verified_future_phase_keeps_independent_owner_without_authorizing_a_target_point() {
        let (mut r, mut plan, frame) = fixture(3, 5534);
        let mut consumed = Vec::new();
        let packet = plan
            .sample(&mut r, &frame, "skill:2", 1000, &mut consumed)
            .unwrap();
        assert!(packet.ready.get() && packet.coherent.get());
        assert_eq!(packet.reason.as_str(), "selector_phase_complete");
        assert_eq!(packet.active_skill_id.value, 5534.);
        assert!(!packet.input_ready.get());
        assert!(packet.contact_present.missing());
        assert_eq!(packet.owner_id, 2000);
        assert_eq!(packet.backend_owner_id, 3000);
        assert!(packet.raw_touch_xyz.values.iter().all(|v| v.missing()));
        assert!(packet.drag_direction_xy.values.iter().all(|v| v.missing()));
    }
    #[test]
    fn recipient_join_unknown_is_distinct_from_positive_native_forbid() {
        let valid = IconObservation {
            owner: 10,
            self_id: 20,
            guid: 13016,
            player_matches: Some(true),
            forbid: false,
            cancel: false,
            collider: true,
        };
        assert_eq!(recipient_valid(&valid, 13016), Some(true));
        for selected in [0, 13017] {
            assert_eq!(recipient_valid(&valid, selected), None);
        }
        for player_matches in [None, Some(false)] {
            assert_eq!(
                recipient_valid(
                    &IconObservation {
                        player_matches,
                        ..valid
                    },
                    13016
                ),
                None
            );
        }
        assert_eq!(
            recipient_valid(&IconObservation { guid: 0, ..valid }, 0),
            None
        );
        for icon in [
            IconObservation {
                forbid: true,
                ..valid
            },
            IconObservation {
                cancel: true,
                ..valid
            },
            IconObservation {
                collider: false,
                ..valid
            },
        ] {
            assert_eq!(recipient_valid(&icon, 0), Some(false));
        }
    }
    #[test]
    fn unused_or_unbound_portrait_player_is_not_required_for_another_owned_target() {
        let r = Reflection::new(Fake(RefCell::new(vec![0; 128]))).unwrap();
        write(&r, 16, &13016u32.to_le_bytes());
        write(&r, 32, &[1]);
        let mut icon = IconPlan {
            object: 100,
            checks: Vec::new(),
            self_go: GameObjectPlan {
                object: 101,
                checks: Vec::new(),
            },
            guid: (16, 9),
            player_link: 1024,
            player: None,
            forbid: 30,
            cancel: 31,
            collider: 32,
        };
        let mut r = r;
        write(&r, 30, &[2]);
        let mut consumed = Vec::new();
        let observed = icon.sample(&mut r, &mut consumed, 999).unwrap();
        assert!(observed.player_matches.is_none());
        // This unused entry's player is deliberately unreadable; candidate-only
        // player validation preserves independently owned target receipts.
        assert!(check(&r, &consumed).is_ok());
        icon.player_link = 40;
        write(&r, 30, &[0]);
        write(&r, 16, &0u32.to_le_bytes());
        let unbound = icon.sample(&mut r, &mut Vec::new(), 101).unwrap();
        assert_eq!(unbound.guid, 0);
        assert!(unbound.player_matches.is_none());
    }
    #[test]
    fn malformed_optional_native_values_never_become_false_permission() {
        for address in [88, 89] {
            let (mut r, mut plan, frame) = fixture(4, 5530);
            write(&r, address, &[2]);
            let packet = plan
                .sample(&mut r, &frame, "skill:2", 1000, &mut Vec::new())
                .unwrap();
            assert!(packet.ready.get());
            assert!(packet.target_valid.missing());
            assert!(packet.target_cancel_hit.missing());
            if address == 88 {
                assert!(packet.native_cancel.missing());
            } else {
                assert!(packet.native_unselectable.missing());
            }
        }
    }
    #[test]
    fn failed_cold_cancel_discovery_is_bounded_and_fresh_attempt_or_owner_rebind_retries_immediately()
     {
        let (mut r, mut plan, frame) = fixture(3, 5530);
        write(&r, 64, &200u64.to_le_bytes());
        for now in 1000..1004 {
            assert!(plan.cancel_uv(&mut r, now, &mut Vec::new()).is_err());
        }
        assert_eq!(plan.cancel_attempts, 4);
        assert_eq!(plan.cancel_retry, 20_001_003);
        let pending = plan.cancel_uv(&mut r, 1004, &mut Vec::new()).unwrap_err();
        assert_eq!(pending, "target_cancel_metadata_retry_pending");
        assert_eq!(plan.cancel_attempts, 4);
        write(&r, 64, &208u64.to_le_bytes());
        assert!(plan.cancel_uv(&mut r, 1005, &mut Vec::new()).is_err());
        assert_eq!(plan.cancel_object, 208);
        assert_eq!(plan.cancel_attempts, 1);
        plan.cancel_retry = 90_000_000;
        plan.cancel_attempts = 9;
        plan.sample(&mut r, &frame, "skill:2", 2000, &mut Vec::new())
            .unwrap();
        write(&r, 72, &4i32.to_le_bytes());
        plan.sample(&mut r, &frame, "skill:2", 2001, &mut Vec::new())
            .unwrap();
        assert_eq!(plan.cancel_retry, 0);
        assert_eq!(plan.cancel_attempts, 0);
        assert!(plan.cancel_uv(&mut r, 2002, &mut Vec::new()).is_err());
        assert_eq!(plan.cancel_attempts, 1);
    }
}
