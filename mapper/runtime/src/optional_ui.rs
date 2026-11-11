//! Optional common controls from live UI ownership and rendered widgets.
//!
//! No hero coordinates or timing cache: every append checks topology, visibility,
//! levels and gold. Hidden/absent controls remain declared as unavailable and can
//! become available on the very next sample. A missing optional family never
//! invalidates otherwise usable skill controls.
use crate::{
    math,
    platform::Memory,
    reflection::{Reflection, Result, i32_at, u64_at},
    source::{flag, number, text, v2},
    unity::{self, Path, Widget},
};
use clbb_moba_core::controller::snapshot as s;
use std::collections::HashMap;

type Checks = Vec<(u64, Vec<u8>)>;

fn current<M: Memory>(r: &Reflection<M>, checks: &Checks) -> bool {
    crate::platform::matches_reads(&r.memory, checks).unwrap_or(false)
}
fn capture<M: Memory>(
    r: &Reflection<M>,
    checks: &mut Checks,
    at: u64,
    size: usize,
) -> Result<Vec<u8>> {
    let raw = r.read(at, size)?;
    checks.push((at, raw.clone()));
    Ok(raw)
}
fn owner<M: Memory>(r: &mut Reflection<M>, checks: &mut Checks, object: u64) -> Result<()> {
    if object == 0 {
        return Err("Optional UI object is absent".into());
    }
    let raw = capture(r, checks, object, 8)?;
    r.validate_class(u64_at(&raw, 0))?;
    Ok(())
}
fn link<M: Memory>(
    r: &mut Reflection<M>,
    checks: &mut Checks,
    object: u64,
    name: &str,
) -> Result<u64> {
    let at = r.field(object, name)?;
    Ok(u64_at(&capture(r, checks, at, 8)?, 0))
}
fn list<M: Memory>(r: &Reflection<M>, checks: &mut Checks, object: u64) -> Result<Vec<u64>> {
    if object == 0 {
        return Ok(Vec::new());
    }
    let raw = capture(r, checks, object + 16, 16)?;
    let array = u64_at(&raw, 0);
    let count = i32_at(&raw, 8);
    if !(0..=256).contains(&count) || array == 0 {
        return Err("Invalid optional UI list".into());
    }
    let raw = capture(r, checks, array + 32, count as usize * 8)?;
    Ok(raw.chunks_exact(8).map(|b| u64_at(b, 0)).collect())
}

#[derive(Clone)]
struct Control {
    slot: String,
    kind: &'static str,
    index: i32,
    widget: Widget,
    target: Path,
    visibility: Vec<(u64, usize)>,
    level: Option<(u64, u64)>,
    price: Option<(u64, u64)>, // UILabel.mText pointer field; ShowEntity._iGold.
}
impl Control {
    fn new<M: Memory>(
        r: &mut Reflection<M>,
        checks: &mut Checks,
        slot: String,
        kind: &'static str,
        index: i32,
        target: u64,
        sprite: u64,
    ) -> Result<Self> {
        owner(r, checks, target)?;
        owner(r, checks, sprite)?;
        let widget = Widget::new(r, sprite)?;
        let target = Path::new(r, target, false)?;
        let mut visibility = Vec::new();
        for name in [
            "mIsActive",
            "mIsVisibleByAlpha",
            "mIsVisibleByPanel",
            "mEnabled",
        ] {
            visibility.push((r.field(sprite, name)?, 1));
        }
        visibility.push((r.field(sprite, "finalAlpha")?, 4));
        visibility.push((r.field(sprite, "drawCall")?, 8));
        Ok(Self {
            slot,
            kind,
            index,
            widget,
            target,
            visibility,
            level: None,
            price: None,
        })
    }
    fn sample<M: Memory>(
        &self,
        r: &mut Reflection<M>,
        view: &[f64; 16],
        projection: &[f64; 16],
    ) -> Result<s::Button> {
        self.widget.path.validate(r)?;
        self.target.validate(r)?;
        let mut state = Vec::new();
        for (a, n) in &self.visibility {
            capture(r, &mut state, *a, *n)?;
        }
        capture(r, &mut state, self.widget.width, 4)?;
        capture(r, &mut state, self.widget.height, 4)?;
        if let Some(a) = self.widget.pivot {
            capture(r, &mut state, a, 4)?;
        }
        if let Some(a) = self.widget.region {
            capture(r, &mut state, a, 16)?;
        }
        // Equality brackets include the actual transforms used for projection,
        // not just parent ownership, so UI movement cannot mix two poses.
        for a in &self.widget.path.transforms {
            capture(r, &mut state, *a, 48)?;
        }
        for a in &self.target.transforms {
            capture(r, &mut state, *a, 48)?;
        }
        let mut active = true;
        for (_, raw) in state.iter().take(4) {
            if raw[0] > 1 {
                return Err("Invalid optional widget visibility".into());
            }
            active &= raw[0] == 1;
        }
        let alpha = f32::from_le_bytes(state[4].1[..].try_into().unwrap());
        active &= alpha.is_finite() && alpha > 0. && alpha <= 1.001 && u64_at(&state[5].1, 0) != 0;
        if let Some((level, max)) = self.level {
            let level = i32_at(&capture(r, &mut state, level, 4)?, 0);
            let max = i32_at(&capture(r, &mut state, max, 4)?, 0);
            active &= 0 <= level && level < max && max <= 100;
        }
        if let Some((label, gold)) = self.price {
            let string = u64_at(&capture(r, &mut state, label, 8)?, 0);
            owner(r, &mut state, string)?;
            if r.class_name(r.pointer(string)?)? != "String" {
                return Err("Invalid quick-buy label".into());
            }
            let length = i32_at(&capture(r, &mut state, string + 16, 4)?, 0);
            if !(1..=32).contains(&length) {
                return Err("Invalid quick-buy price length".into());
            }
            let raw = capture(r, &mut state, string + 20, length as usize * 2)?;
            let utf16: Vec<_> = raw
                .chunks_exact(2)
                .map(|v| u16::from_le_bytes(v.try_into().unwrap()))
                .collect();
            let price = String::from_utf16(&utf16)
                .ok()
                .and_then(|v| parse_price(&v));
            let gold = i32_at(&capture(r, &mut state, gold, 4)?, 0);
            active &= price
                .is_some_and(|price| 0 <= gold && gold < 1_000_000_000 && price <= gold as u32);
        }
        // UIWidget pivot/drawRegion gives the rendered clickable image center;
        // an off-screen or edge-aligned GameObject anchor is not a click point.
        let pivot = self.widget.pivot.is_some() && self.widget.region.is_some();
        let (center, radius) = self.widget.geometry(r, view, projection, pivot)?;
        if !unity::onscreen(center) {
            return Err("Optional widget is off screen".into());
        }
        if !self
            .widget
            .path
            .transforms
            .contains(&self.target.transforms[0])
        {
            // Some upgrades/shops keep their target and sprite as siblings.
            // Their target must still overlap the measured rendered widget.
            let target = math::project(view, projection, self.target.position(r)?, false)
                .ok_or("Optional target behind camera")?;
            if !unity::onscreen(target)
                || (0..2)
                    .map(|i| ((target[i] - center[i]) / radius[i]).powi(2))
                    .sum::<f64>()
                    > 1.01
            {
                return Err("Optional target does not own its rendered widget".into());
            }
        }
        if !current(r, &state) {
            return Err("Optional UI changed during sample".into());
        }
        self.widget.path.validate(r)?;
        self.target.validate(r)?;
        Ok(s::Button {
            slot: text(&self.slot),
            kind: text(self.kind),
            index: number(self.index as f64),
            skill_id: number(0.),
            active: flag(active),
            press_uv: v2(center),
            center_uv: v2(center),
            radius_uv: v2(radius),
            geometry_source: text("native_widget_transform"),
        })
    }
}

#[derive(Default)]
struct Family {
    checks: Checks,
    controls: Vec<Control>,
    retry: bool,
}
impl Family {
    fn valid<M: Memory>(&self, r: &Reflection<M>) -> bool {
        !self.retry && !self.checks.is_empty() && current(r, &self.checks)
    }
}

/// Optional discovery is scoped to a live UIKeySkill and local actor. The caller
/// must replace this object when either root owner changes (as Source already does).
pub struct OptionalUi {
    ui: u64,
    show: u64,
    roots: Checks,
    active_frames_field: u64,
    frames: Vec<u64>,
    upgrades: Family,
    shop: Family,
    attacks: Family,
}
impl OptionalUi {
    pub fn prepare<M: Memory>(
        r: &mut Reflection<M>,
        ui_key_skill: u64,
        active_frames: &[u64],
        local_show: u64,
    ) -> Result<Self> {
        let mut roots = Vec::new();
        owner(r, &mut roots, ui_key_skill)?;
        owner(r, &mut roots, local_show)?;
        let mut base = r.class(r.pointer(ui_key_skill)?)?;
        for _ in 0..24 {
            if base.name == "BaseFrame" {
                break;
            }
            if base.parent == 0 {
                return Err("Optional UI frame root is absent".into());
            }
            base = r.class(base.parent)?;
        }
        if base.name != "BaseFrame" {
            return Err("Optional UI frame inheritance changed".into());
        }
        let field = base
            .fields
            .get("m_ActiveFrameList")
            .ok_or("Optional active frame list missing")?;
        if !field.is_static || base.statics == 0 {
            return Err("Optional active frame list has no static owner".into());
        }
        let active_frames_field = base.statics + field.offset;
        let mut result = Self {
            ui: ui_key_skill,
            show: local_show,
            roots,
            active_frames_field,
            frames: active_frames.to_vec(),
            upgrades: Family::default(),
            shop: Family::default(),
            attacks: Family::default(),
        };
        result.upgrades = result.build_upgrades(r);
        result.shop = result.build_shop(r);
        result.attacks = result.build_attacks(r);
        Ok(result)
    }

    pub fn append<M: Memory>(
        &mut self,
        r: &mut Reflection<M>,
        view: &[f64; 16],
        projection: &[f64; 16],
        ui: &mut s::Ui,
    ) -> Result<()> {
        if !current(r, &self.roots) {
            return Err("Optional UI root owner changed".into());
        }
        // Always refresh membership, including frames first created after match
        // initialization. No fixed refresh delay or one-time-only discovery.
        let mut membership = Vec::new();
        let frame_list = u64_at(
            &capture(r, &mut membership, self.active_frames_field, 8)?,
            0,
        );
        let frames = list(r, &mut membership, frame_list)?;
        if !current(r, &membership) {
            return Ok(());
        }
        if frames != self.frames {
            self.frames = frames;
            self.shop.retry = true;
        }
        if !self.upgrades.valid(r) {
            self.upgrades = self.build_upgrades(r);
        }
        if !self.shop.valid(r) {
            self.shop = self.build_shop(r);
        }
        if !self.attacks.valid(r) {
            self.attacks = self.build_attacks(r);
        }

        let mut discovered = Vec::new();
        for family in [&mut self.upgrades, &mut self.shop, &mut self.attacks] {
            let mut candidates: HashMap<String, Vec<s::Button>> = HashMap::new();
            if !current(r, &family.checks) {
                family.retry = true;
                continue;
            }
            for control in &family.controls {
                match control.sample(r, view, projection) {
                    Ok(button) => candidates
                        .entry(control.slot.clone())
                        .or_default()
                        .push(button),
                    Err(_) => family.retry = true,
                }
            }
            if !current(r, &family.checks) {
                family.retry = true;
                continue;
            }
            for (_, mut options) in candidates {
                // Nested background sprites can have the same touch owner.
                // Prefer a visible, smaller hit image; never select a hidden
                // decoration just because it happened to be discovered first.
                options.sort_by(|a, b| {
                    (!a.active.get()).cmp(&(!b.active.get())).then_with(|| {
                        let area = |v: &s::Button| {
                            v.radius_uv.values[0].value * v.radius_uv.values[1].value
                        };
                        area(a).total_cmp(&area(b))
                    })
                });
                discovered.push(options[0]);
            }
        }
        if !current(r, &self.roots) || !current(r, &membership) {
            return Ok(());
        }
        discovered.sort_by(|a, b| a.slot.as_str().cmp(b.slot.as_str()));
        for button in discovered {
            append_button(ui, button);
        }
        for (slot, kind, index) in [
            ("upgrade:0", "upgrade", 0),
            ("upgrade:1", "upgrade", 1),
            ("upgrade:2", "upgrade", 2),
            ("shop:quick", "quick_buy", 0),
            ("attack:minion", "attack", 1),
            ("attack:tower", "attack", 2),
        ] {
            append_capability(ui, slot, kind, index);
        }
        // A fourth upgrade only exists on heroes that really provide skill:3.
        if self.upgrades.controls.iter().any(|c| c.index == 3) {
            append_capability(ui, "upgrade:3", "upgrade", 3);
        }
        Ok(())
    }

    fn build_upgrades<M: Memory>(&self, r: &mut Reflection<M>) -> Family {
        let mut family = Family::default();
        let result = (|| -> Result<()> {
            let object = link(r, &mut family.checks, self.ui, "statusKeySkillList")?;
            let buttons = list(r, &mut family.checks, object)?;
            let mut indices = HashMap::new();
            for button in buttons.into_iter().filter(|v| *v != 0) {
                owner(r, &mut family.checks, button)?;
                let class = r.class(r.pointer(button)?)?;
                if !crate::source::skill_button_class(r, &class)? {
                    continue;
                }
                let at = r.field(button, "m_iIndex")?;
                let index = i32_at(&capture(r, &mut family.checks, at, 4)?, 0);
                if !(0..4).contains(&index) {
                    continue;
                }
                if indices.insert(index, button).is_some() {
                    return Err("Ambiguous skill upgrade index".into());
                }
            }
            for (index, button) in indices {
                // Zero targets are monitored too: a later allocation causes an
                // immediate rebuild instead of permanently missing this slot.
                let built = (|| -> Result<Control> {
                    let target = link(r, &mut family.checks, button, "m_SkillAddBtn")?;
                    let sprite =
                        link(r, &mut family.checks, button, "m_Background_SkillAddSprite")?;
                    let data = link(r, &mut family.checks, button, "m_SkillData")?;
                    owner(r, &mut family.checks, data)?;
                    let mut control = Control::new(
                        r,
                        &mut family.checks,
                        format!("upgrade:{index}"),
                        "upgrade",
                        index,
                        target,
                        sprite,
                    )?;
                    control.level = Some((
                        r.field(data, "skillLvl")?,
                        r.field(button, "m_iMaxSkillLevel")?,
                    ));
                    Ok(control)
                })();
                match built {
                    Ok(control) => family.controls.push(control),
                    // A sprite can exist before its native transform does.
                    // Retry failed construction even if its managed links stay
                    // unchanged, while retaining the other usable upgrades.
                    Err(_) => family.retry = true,
                }
            }
            Ok(())
        })();
        let invalid = result.is_err() || !current(r, &family.checks);
        family.retry |= invalid;
        if invalid {
            family.controls.clear();
        }
        family
    }

    fn build_shop<M: Memory>(&self, r: &mut Reflection<M>) -> Family {
        let mut family = Family::default();
        let result = (|| -> Result<()> {
            // Keep a watch even when no shop frame exists yet.
            family.checks.extend(self.roots.iter().cloned());
            let mut frames = Vec::new();
            for frame in self.frames.iter().copied().filter(|v| *v != 0) {
                if r.class_name(r.pointer(frame)?)? == "UIBattleRecEquip" {
                    frames.push(frame);
                }
            }
            if frames.is_empty() {
                return Ok(());
            }
            if frames.len() != 1 {
                return Err("Ambiguous live quick-buy frame".into());
            }
            let frame = frames[0];
            owner(r, &mut family.checks, frame)?;
            let info = link(r, &mut family.checks, frame, "m_UIFrameInfo")?;
            owner(r, &mut family.checks, info)?;
            let target = link(r, &mut family.checks, info, "m_button_RecEquip01")?;
            let sprite = link(r, &mut family.checks, info, "m_sprite_RecEquip01")?;
            let label = link(r, &mut family.checks, info, "m_label_RecEquipGold01")?;
            owner(r, &mut family.checks, label)?;
            let mut control = Control::new(
                r,
                &mut family.checks,
                "shop:quick".into(),
                "quick_buy",
                0,
                target,
                sprite,
            )?;
            control.price = Some((r.field(label, "mText")?, r.field(self.show, "_iGold")?));
            family.controls.push(control);
            Ok(())
        })();
        family.retry = result.is_err() || !current(r, &family.checks);
        if family.retry {
            family.controls.clear();
        }
        family
    }

    fn build_attacks<M: Memory>(&self, r: &mut Reflection<M>) -> Family {
        let mut family = Family::default();
        let result = (|| -> Result<()> {
            let attack = link(r, &mut family.checks, self.ui, "m_AttackComp")?;
            owner(r, &mut family.checks, attack)?;
            if r.class_name(r.pointer(attack)?)? != "UIKeyAttack" {
                return Err("Unknown attack control owner".into());
            }
            let info = link(r, &mut family.checks, attack, "m_UIFrameInfo")?;
            owner(r, &mut family.checks, info)?;
            let class = r.class(r.pointer(info)?)?;
            let at = r.field(attack, "iAttackUIIndex")?;
            let index = i32_at(&capture(r, &mut family.checks, at, 4)?, 0);
            if !(0..64).contains(&index) {
                return Err("Invalid attack UI layout".into());
            }
            let mut roles = HashMap::new();
            let mut excluded = Vec::new();
            for (slot, name) in [
                ("attack:minion", "m_Attack02_List"),
                ("attack:tower", "m_button_LastHit_List"),
                ("", "m_button_SwitchMonster_List"),
                ("", "m_button_SwitchHero_List"),
            ] {
                let p = link(r, &mut family.checks, attack, name)?;
                let items = list(r, &mut family.checks, p)?;
                excluded.extend(items.iter().copied().filter(|v| *v != 0));
                if !slot.is_empty() {
                    if let Some(value) = items.get(index as usize).copied().filter(|v| *v != 0) {
                        roles.insert(value, slot);
                    }
                }
            }
            let mut targets = HashMap::new();
            let mut sprites = Vec::new();
            let mut fields: Vec<_> = class.fields.iter().collect();
            fields.sort_by(|a, b| a.0.cmp(b.0));
            for (name, meta) in fields {
                if meta.is_static || meta.kind != 18 {
                    continue;
                }
                let target = target_field(name);
                let sprite = (name.starts_with("m_Sprite_Attack")
                    || name.starts_with("m_Sprite_LastHit"))
                    && !name.contains("Forbid");
                if !target && !sprite {
                    continue;
                }
                let object = link(r, &mut family.checks, info, name)?;
                if object == 0 {
                    continue;
                }
                owner(r, &mut family.checks, object)?;
                if target {
                    targets.insert(object, Path::new(r, object, false)?);
                }
                if sprite {
                    sprites.push(object);
                }
            }
            for object in excluded {
                if let std::collections::hash_map::Entry::Vacant(entry) = targets.entry(object) {
                    owner(r, &mut family.checks, object)?;
                    entry.insert(Path::new(r, object, false)?);
                }
            }
            for sprite in sprites {
                let widget = Widget::new(r, sprite)?;
                let nearest = targets
                    .iter()
                    .filter_map(|(object, path)| {
                        widget
                            .path
                            .transforms
                            .iter()
                            .position(|a| *a == path.transforms[0])
                            .map(|distance| (distance, *object))
                    })
                    .min();
                let Some((_, target)) = nearest else {
                    continue;
                };
                let Some(slot) = roles.get(&target).copied() else {
                    continue;
                };
                let index = if slot == "attack:minion" { 1 } else { 2 };
                family.controls.push(Control::new(
                    r,
                    &mut family.checks,
                    slot.into(),
                    "attack",
                    index,
                    target,
                    sprite,
                )?);
            }
            Ok(())
        })();
        family.retry = result.is_err() || !current(r, &family.checks);
        if family.retry {
            family.controls.clear();
        }
        family
    }
}

fn append_button(ui: &mut s::Ui, button: s::Button) {
    if ui.buttons[..ui.button_count.min(32) as usize]
        .iter()
        .any(|v| v.slot.as_str() == button.slot.as_str())
    {
        return;
    }
    if ui.button_count < 32 {
        ui.buttons[ui.button_count as usize] = button;
        ui.button_count += 1;
    }
}
fn append_capability(ui: &mut s::Ui, slot: &str, kind: &str, index: i32) {
    if ui.capability_count >= 32
        || ui.capability_controls[..ui.capability_count as usize]
            .iter()
            .any(|v| v.slot.as_str() == slot)
    {
        return;
    }
    ui.capability_controls[ui.capability_count as usize] = s::Button {
        slot: text(slot),
        kind: text(kind),
        index: number(index as f64),
        active: flag(false),
        ..Default::default()
    };
    ui.capability_count += 1;
}

fn target_field(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("m_Attack") else {
        return false;
    };
    let rest = rest.trim_start_matches(|v: char| v.is_ascii_digit());
    if rest.is_empty() {
        return true;
    }
    let rest = rest.strip_prefix('_').unwrap_or(rest);
    rest.strip_prefix("Hero")
        .is_some_and(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()))
}
fn parse_price(value: &str) -> Option<u32> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if value.chars().all(|v| v.is_ascii_digit()) {
        return value.parse().ok();
    }
    let delimiter = value.chars().find(|v| !v.is_ascii_digit())?;
    if ![',', ' ', '\u{a0}', '\u{202f}'].contains(&delimiter) {
        return None;
    }
    let groups: Vec<_> = value.split(delimiter).collect();
    if !(1..=3).contains(&groups[0].len())
        || groups[1..].iter().any(|v| v.len() != 3)
        || groups
            .iter()
            .any(|v| !v.chars().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    groups.concat().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_only_unambiguous_unsigned_prices() {
        for (raw, expected) in [
            ("1200", 1200),
            ("1,200", 1200),
            ("12 345 678", 12_345_678),
            ("1\u{a0}200", 1200),
            ("1\u{202f}200", 1200),
        ] {
            assert_eq!(parse_price(raw), Some(expected));
        }
        for raw in [
            "",
            "1.200",
            "1,20",
            "-1200",
            "1k",
            "1,200 000",
            "١٢٣",
            "1,,200",
            "4294967296",
        ] {
            assert_eq!(parse_price(raw), None);
        }
    }
    #[test]
    fn target_names_exclude_other_attack_settings() {
        for raw in [
            "m_Attack",
            "m_Attack01",
            "m_AttackHero3",
            "m_Attack02_Hero12",
        ] {
            assert!(target_field(raw), "{raw}");
        }
        for raw in [
            "m_AttackComp",
            "m_Attack02_List",
            "m_AttackHero",
            "m_Attack_Hero3x",
            "m_Sprite_Attack",
        ] {
            assert!(!target_field(raw), "{raw}");
        }
    }
    #[test]
    fn capabilities_are_unique_and_never_make_hidden_buttons_clickable() {
        let mut ui = s::Ui::default();
        append_capability(&mut ui, "attack:tower", "attack", 2);
        append_capability(&mut ui, "attack:tower", "attack", 2);
        assert_eq!(ui.capability_count, 1);
        assert_eq!(ui.button_count, 0);
        assert!(!ui.capability_controls[0].active.get());
    }
}
