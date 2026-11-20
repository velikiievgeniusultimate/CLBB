//! CLBB-owned settings. JSON is used only for persistence and menu edits.
use serde::{Deserialize, Serialize};
use std::{collections::{BTreeMap, BTreeSet}, path::PathBuf};
pub const MOUSE: i32 = 0x02000000;
pub const F7: i32 = 0x01000036;
pub const F11: i32 = 0x0100003a;
pub const F12: i32 = 0x0100003b;
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub values: BTreeMap<String, String>,
    #[serde(skip)]
    pub path: PathBuf,
    /// Verified current native templates are runtime defaults, not user choices.
    #[serde(skip)]
    native_skill_types: BTreeMap<i32, BTreeMap<String, u8>>,
    #[serde(skip)]
    native_charge_slots: BTreeMap<i32, BTreeSet<String>>,
    #[serde(skip)]
    pub observed_forms: BTreeMap<i32, BTreeMap<(String, i32), BTreeSet<i32>>>,
}
impl Settings {
    pub fn load() -> Self {
        let root = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
            });
        let path = root.join("clbb/settings.json");
        let mut s = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Self>(&b).ok())
            .unwrap_or_default();
        s.path = path;
        if !s.values.contains_key("migration.ewm") {
            if let Ok(raw) = std::fs::read_to_string(root.join("Evgenium/ewm-futuristic-mlbb.ini"))
            {
                s.import_legacy(&raw);
            }
            s.values.insert("migration.ewm".into(), "1".into());
            let _ = s.save();
        }
        if !s.values.contains_key("display.imported_resolution") {
            if let Ok(raw) =
                std::fs::read_to_string(root.join("Evgenium/Evgenium Waydroid Mapper.conf"))
            {
                s.import_resolution(&raw);
            }
            let _ = s.save();
        }
        if s.migrate_field_inheritance() { let _ = s.save(); }
        if s.install_directional_hero_defaults() { let _ = s.save(); }
        if s.install_guinevere_ultimate_default() { let _ = s.save(); }
        s
    }
    // Her channel is cancelled by movement. Retire the previous movement
    // intent on activation; the existing controller accepts fresh input again.
    // Seed once, preserving both personal choices and explicit inheritance.
    fn install_guinevere_ultimate_default(&mut self) -> bool {
        const MARK: &str = "migration.guinevere_ultimate_stop.v1";
        if self.values.contains_key(MARK) { return false; }
        let key = "heroes.80.bindings.skill:2.stop_movement";
        let flag = format!("{key}.override");
        if !self.values.contains_key(key) && !self.values.contains_key(&flag) {
            self.values.insert(key.into(), "true".into());
            self.values.insert(flag, "true".into());
        }
        self.values.insert(MARK.into(), "1".into());
        true
    }
    // These explicitly requested keyboard/shooting profiles are initial
    // personal defaults, not changes to shared bindings. Reset still restores
    // ordinary inheritance, and later loads never overwrite user choices.
    fn install_directional_hero_defaults(&mut self)->bool {
        const MARK:&str="migration.directional_hero_defaults.v1";
        if self.values.contains_key(MARK) {return false;}
        for hero in [71,125] {
            let mut defaults=vec![
                ("movement.click_mode","2"),("movement.up","87"),("movement.down","83"),
                ("movement.left","65"),("movement.right","68"),
                ("bindings.skill:0.key","81"),("bindings.skill:2.key","82"),
                ("bindings.attack:minion.key","88"),("bindings.attack:tower.key","67"),
                ("bindings.extra:1.key","71"),
            ];
            if hero==71 {defaults.extend([("bindings.attack.key","33554434"),("bindings.skill:1.key","69"),
                ("bindings.attack.cast_mode","0"),("bindings.attack.stop_movement","false")]);}
            else {defaults.extend([("bindings.skill:1.key","33554434"),("bindings.skill:1.cast_mode","0"),
                ("bindings.skill:1.stop_movement","false")]);}
            for (field,value)in defaults {
                let own=format!("heroes.{hero}.{field}");let flag=format!("{own}.override");
                if !self.values.contains_key(&own)&&!self.values.contains_key(&flag) {
                    self.values.insert(own,value.into());self.values.insert(flag,"true".into());
                }
            }
        }
        self.values.insert(MARK.into(),"1".into());true
    }
    fn migrate_field_inheritance(&mut self) -> bool {
        if self.values.contains_key("migration.field_inheritance") { return false; }
        let saved = self.values.clone();
        for (key, _) in &saved {
            if !key.starts_with("heroes.") { continue; }
            if let Some((base, tail)) = key.split_once(".bindings.") {
                if let Some((slot, field)) = tail.rsplit_once('.') {
                    if !matches!(field, "key" | "cast_mode" | "stop_movement") { continue; }
                    let parent = if plain_spell_slot(slot) { "extra:2" } else { slot };
                    let explicit = native_skill_slot(slot) || slot.starts_with("combo:") || saved.get(&format!("{base}.bindings.{parent}.override")).is_some_and(|v|matches!(v.as_str(),"true"|"1"));
                    self.values.insert(format!("{key}.override"), explicit.to_string());
                }
            } else if key.contains(".movement.") && [".button",".click_mode",".hold_ms",".click_distance_percent"].iter().any(|f|key.ends_with(f)) {
                self.values.insert(format!("{key}.override"), "true".into());
            } else if key.ends_with(".cancel_key") {
                self.values.insert(format!("{key}.override"), "true".into());
            }
        }
        self.values.insert("migration.field_inheritance".into(), "1".into());
        true
    }
    fn import_resolution(&mut self, raw: &str) {
        let last_width = raw
            .lines()
            .find_map(|line| line.strip_prefix("lastWidth="))
            .and_then(|v| v.parse::<u32>().ok());
        let last_height = raw
            .lines()
            .find_map(|line| line.strip_prefix("lastHeight="))
            .and_then(|v| v.parse::<u32>().ok());
        let current = last_width
            .zip(last_height)
            .and_then(|(w, h)| parse_resolution(&format!("{w}x{h}")));
        let imported = current.or_else(|| {
            raw.lines()
                .filter_map(|line| line.split_once('='))
                .filter(|(key, _)| key.ends_with("resolutionKeys"))
                .flat_map(|(_, v)| v.split(','))
                .filter_map(|v| parse_resolution(v.trim()))
                .max_by_key(|(w, h)| w * h)
        });
        if let Some((width, height)) = imported {
            let value = format!("{width}x{height}");
            self.values
                .insert("display.imported_resolution".into(), value.clone());
            self.values
                .entry("display.resolution".into())
                .or_insert(value);
        }
    }
    fn import_legacy(&mut self, raw: &str) {
        let mut section = "";
        for row in raw.lines() {
            let row = row.trim();
            if row.starts_with('[') && row.ends_with(']') {
                section = &row[1..row.len() - 1];
                continue;
            }
            let Some((key, val)) = row.split_once('=') else {
                continue;
            };
            let key = key.replace("%3A", ":");
            let p: Vec<_> = key.split('\\').collect();
            let suffix = |v: &str| {
                match v {
                    "castMode" => "cast_mode",
                    "stopMovement" => "stop_movement",
                    _ => v,
                }
                .to_owned()
            };
            let name = match (section, p.as_slice()) {
                ("common", ["buttons", slot, field])
                    if ["key", "castMode", "stopMovement"].contains(field) =>
                {
                    Some(format!("common.bindings.{slot}.{}", suffix(field)))
                }
                ("heroes", [id, "buttons", slot, field])
                    if ["key", "castMode", "stopMovement"].contains(field) =>
                {
                    Some(format!("heroes.{id}.bindings.{slot}.{}", suffix(field)))
                }
                ("heroes", [id, "overrides", slot]) => {
                    Some(format!("heroes.{id}.bindings.{slot}.override"))
                }
                ("heroes", [id, "cancelKey"]) => Some(format!("heroes.{id}.cancel_key")),
                ("movement", ["clickMode"]) => Some("movement.click_mode".into()),
                ("movement", ["holdMs"]) => Some("movement.hold_ms".into()),
                _ => None,
            };
            if let Some(name) = name {
                self.values.insert(name, val.trim().into());
            }
        }
    }
    fn canonical(key: &str, hero: i32) -> String {
        if let Some(rest) = key.strip_prefix("hero.") {
            format!("heroes.{hero}.{rest}")
        } else {
            key.into()
        }
    }
    pub fn get(&self, key: &str, hero: i32) -> String {
        if key == "enabled" { return "true".into(); }
        if let Some((scope, tail)) = key.split_once(".bindings.spell:") {
            if let Some((id, field)) = tail.split_once('.') {
                let canonical = Self::canonical(key, hero);
                let override_marker = self.values.get(&format!("{canonical}.override"));
                let has_override = override_marker.is_some_and(|value| matches!(value.as_str(), "true" | "1"));
                if scope == "hero" && !has_override && !field.ends_with("override") {
                    let common = format!("common.bindings.spell:{id}.{field}");
                    if override_marker.is_some() || self.values.contains_key(&common) {
                        return self.get(&common, hero);
                    }
                }
                if !field.ends_with("override") && (scope == "common" || !has_override)
                    && (scope != "common" || !self.values.contains_key(&canonical))
                {
                    let legacy = if field == "cast_mode"
                        && id.parse().is_ok_and(plain_spell_id)
                    {
                        format!("{scope}.bindings.extra:2:{id}.{field}")
                    } else {
                        format!("{scope}.bindings.extra:2.{field}")
                    };
                    return self.get(&legacy, hero);
                }
                if let Some(value) = self.values.get(&canonical) {
                    return value.clone();
                }
            }
        }
        if let Some((scope, tail))=key.split_once(".bindings.") {
            if let Some((slot,field))=tail.rsplit_once('.') {
                if hold_duration_skill(hero,slot) {
                    if field=="cast_mode" {return "0".into();}
                    if matches!(field,"key"|"stop_movement") {
                        if let Some(base)=variant_base(slot) {
                            return self.get(&format!("{scope}.bindings.{base}.{field}"),hero);
                        }
                    }
                }
            }
        }
        if let Some((scope, tail)) = key.split_once(".bindings.") {
            if let Some((slot, field)) = tail.rsplit_once('.') {
                if let Some(base) = variant_base(slot) {
                    if field == "key" { return self.get(&format!("{scope}.bindings.{base}.key"), hero); }
                    if matches!(field,"cast_mode"|"stop_movement") {
                        let own=Self::canonical(key,hero);
                        let explicit=self.values.get(&format!("{own}.override")).map(|v|matches!(v.as_str(),"true"|"1")).unwrap_or(self.values.contains_key(&own));
                        if scope=="hero" && explicit { if let Some(value)=self.values.get(&own) { return value.clone(); } }
                        if scope=="hero" && !explicit {
                            let common=format!("common.bindings.{slot}.{field}");
                            return if self.values.contains_key(&common) { self.get(&common,hero) }
                                else { self.get(&format!("hero.bindings.{base}.{field}"),hero) };
                        }
                        if scope=="common" && !self.values.contains_key(key) {
                            return self.get(&format!("common.bindings.{base}.{field}"),hero);
                        }
                    }
                }
            }
        }
        if let Some(tail) = key.strip_prefix("hero.bindings.") {
            if let Some((slot, field)) = tail.rsplit_once('.') {
                if matches!(field, "key" | "cast_mode" | "stop_movement") {
                    let canonical = Self::canonical(key, hero);
                    let parent = if plain_spell_slot(slot) { "extra:2" } else { slot };
                    let legacy = (native_skill_slot(slot) || slot.starts_with("combo:") || self.values.get(&Self::canonical(&format!("hero.bindings.{parent}.override"),hero)).is_some_and(|v|matches!(v.as_str(),"true"|"1"))) && self.values.contains_key(&canonical);
                    let explicit = self.values.get(&format!("{canonical}.override")).map(|v|matches!(v.as_str(),"true"|"1")).unwrap_or(legacy);
                    if !explicit { return self.get(&format!("common.bindings.{tail}"),hero); }
                }
            }
        }
        if let Some(field)=key.strip_prefix("hero.movement.").filter(|f|!f.ends_with(".override")) {
            let name=Self::canonical(key,hero);
            let explicit=self.values.get(&format!("{name}.override")).map(|v|matches!(v.as_str(),"true"|"1")).unwrap_or(self.values.contains_key(&name));
            if !explicit {return self.get(&format!("movement.{field}"),hero);}
        }
        if key == "hero.cancel_key" {
            let name=Self::canonical(key,hero);
            let explicit=self.values.get(&format!("{name}.override")).map(|v|matches!(v.as_str(),"true"|"1")).unwrap_or(self.values.contains_key(&name));
            if !explicit { return self.get("common.cancel_key",hero); }
        }
        if key == "display.fps_limit" {
            return self
                .values
                .get(key)
                .and_then(|v| v.parse::<u32>().ok())
                .filter(|fps| (30..=240).contains(fps))
                .unwrap_or(60)
                .to_string();
        }
        if key == "display.vsync" {
            return match self.values.get(key).map(String::as_str) {
                Some("false" | "0") => "false",
                _ => "true",
            }
            .into();
        }
        if key == "display.frame_pacing" {
            return format!(
                "limit={};vsync={}",
                self.get("display.fps_limit", hero),
                if self.boolean("display.vsync", hero) {
                    1
                } else {
                    0
                }
            );
        }
        if matches!(key, "display.width" | "display.height") {
            let size =
                parse_resolution(&self.get("display.resolution", hero)).unwrap_or((1280, 720));
            return if key == "display.width" {
                size.0
            } else {
                size.1
            }
            .to_string();
        }
        let name = Self::canonical(key, hero);
        if let Some(v) = self.values.get(&name) {
            return v.clone();
        }
        let value = match key {
            "display.resolution" => "1280x720".into(),
            "display.imported_resolution" => "".into(),
            "enabled" => "true".into(),
            "debug.recording" => "false".into(),
            "debug.auto_marks" => "true".into(),
            "debug.mark_shortcut" => (0x04000000 | 83).to_string(),
            "movement.click_mode" => "0".into(),
            "movement.up" => "87".into(),
            "movement.down" => "83".into(),
            "movement.left" => "65".into(),
            "movement.right" => "68".into(),
            "movement.click_distance_percent" => "100".into(),
            "movement.hold_ms" => "150".into(),
            "movement.button" => (MOUSE | 2).to_string(),
            "audio.input_device" | "audio.output_device" => "default".into(),
            "audio.volume" | "audio.game_volume" | "audio.voice_volume" => "100".into(),
            "audio.muted" | "display.show_fps" => "false".into(),
            "hotkeys.chat" => 0x01000004.to_string(),
            "hotkeys.settings" => F7.to_string(),
            "hotkeys.confine" => F12.to_string(),
            "hotkeys.fullscreen" => F11.to_string(),
            "hero.cancel_key" | "common.cancel_key" => "84".into(),
            "hero.cancel_key.override" => "false".into(),
            _ => self.binding_default(key, hero),
        };
        value
    }
    fn binding_default(&self, key: &str, hero: i32) -> String {
        let Some((scope, tail)) = key.split_once(".bindings.") else {
            return String::new();
        };
        let Some((slot, field)) = tail.rsplit_once('.') else {
            return String::new();
        };
        if field == "override" {
            return "false".into();
        }
        if scope == "hero" {
            return self.get(&format!("common.bindings.{slot}.{field}"), hero);
        }
        if field == "stop_movement" {
            return "false".into();
        }
        if field == "cast_mode" {
            // Geometry and native charge flags never opt users into quick cast.
            return "0".into();
        }
        if field != "key" {
            return String::new();
        }
        let code = match slot {
            "skill:0" => 81,
            "skill:1" | "skill:1:2521" => 87,
            "skill:2" | "combo:0" => 69,
            "skill:3" | "combo:1" => 82,
            "attack" => 32,
            "attack:minion" => 65,
            "attack:tower" => 83,
            "extra:0" => 66,
            "extra:1" => 68,
            "extra:2" => 70,
            "ally:mathilda" | "ally:johnson" => 86,
            "item:active" | "extra:3" => 0x01000007,
            "match:statistics" => 0x01000001,
            "shop:quick" => 90,
            "upgrade:0" => 49,
            "upgrade:1" => 50,
            "upgrade:2" => 51,
            "upgrade:3" => 52,
            _ => 0,
        };
        code.to_string()
    }
    pub fn observe_native_templates(&mut self, hero: i32, types: &BTreeMap<String, u8>) {
        if !(1..=10000).contains(&hero) {
            return;
        }
        let verified = || types.iter().filter(|(slot, kind)| {
            native_skill_slot(slot) && matches!(**kind, 1..=4)
        });
        let count = verified().count();
        if self.native_skill_types.get(&hero).is_some_and(|cached| {
            cached.len() == count && verified().all(|(slot, kind)| cached.get(slot) == Some(kind))
        }) {
            return;
        }
        if count == 0 {
            self.native_skill_types.remove(&hero);
        } else {
            self.native_skill_types.insert(hero, verified().map(|(slot, kind)| (slot.clone(), *kind)).collect());
        }
    }
    pub fn observe_native_holds(&mut self, hero: i32, holds: &BTreeMap<String, bool>) {
        if !(1..=10000).contains(&hero) { return; }
        let charged = || holds.iter().filter(|(slot, hold)| **hold && native_skill_slot(slot)).map(|(slot, _)| slot);
        let count = charged().count();
        if self.native_charge_slots.get(&hero).is_some_and(|cached| {
            cached.len() == count && charged().all(|slot| cached.contains(slot))
        }) { return; }
        if count == 0 {
            self.native_charge_slots.remove(&hero);
        } else {
            self.native_charge_slots.insert(hero, charged().cloned().collect());
        }
    }
    pub fn clear_native_holds(&mut self) {
        self.native_charge_slots.clear();
    }
    pub fn integer(&self, key: &str, hero: i32) -> i32 {
        self.get(key, hero).parse().unwrap_or(0)
    }
    pub fn boolean(&self, key: &str, hero: i32) -> bool {
        matches!(self.get(key, hero).as_str(), "true" | "1")
    }
    pub fn binding(&self, hero: i32, slot: &str) -> clbb_moba_core::controller::host::Binding {
        let prefix = if slot == "match:statistics" { "common" } else { "hero" };
        let key = |f| format!("{prefix}.bindings.{slot}.{f}");
        let code = self.integer(&key("key"), hero);
        let code = if valid_key(code) && !self.reserved(code, hero) {
            code
        } else {
            0
        };
        clbb_moba_core::controller::host::Binding {
            key: code,
            cast_mode: if slot == "match:statistics" { 1 } else { self.integer(&key("cast_mode"), hero) },
            aim_type: 0,
            stop_movement: self.boolean(&key("stop_movement"), hero),
        }
    }
    pub fn set(&mut self, key: &str, value: &str, hero: i32) -> Result<(), String> {
        if let Some((scope,tail))=key.split_once(".bindings.") {
            if let Some((slot,field))=tail.rsplit_once('.') {
                if hold_duration_skill(hero,slot) {
                    if field=="cast_mode" && value!="0" {
                        return Err("Навыки Су Ё применяются при отпускании: результат зависит от длительности удержания".into());
                    }
                    if matches!(field,"key"|"cast_mode"|"stop_movement") {
                        if let Some(base)=variant_base(slot) {
                            return self.set(&format!("{scope}.bindings.{base}.{field}"),value,hero);
                        }
                    }
                }
            }
        }
        if let Some((scope,tail))=key.split_once(".bindings.") {
            if let Some(slot)=tail.strip_suffix(".key") {
                if let Some(base)=variant_base(slot) { return self.set(&format!("{scope}.bindings.{base}.key"),value,hero); }
            }
        }
        if key.len() > 128 || value.len() > 64 {
            return Err("Setting is too long".into());
        }
        if key == "display.frame_pacing" {
            let (fps, vsync) =
                parse_frame_pacing(value).ok_or("Недопустимый лимит FPS или режим VSync")?;
            let old_fps = self
                .values
                .insert("display.fps_limit".into(), fps.to_string());
            let old_vsync = self
                .values
                .insert("display.vsync".into(), vsync.to_string());
            if let Err(error) = self.save() {
                for (name, old) in [("display.fps_limit", old_fps), ("display.vsync", old_vsync)] {
                    if let Some(old) = old {
                        self.values.insert(name.into(), old);
                    } else {
                        self.values.remove(name);
                    }
                }
                return Err(error);
            }
            return Ok(());
        }
        let is_binding = key.starts_with("common.bindings.") || key.starts_with("hero.bindings.");
        let integer = value.parse::<i32>().ok();
        let valid = match key {
            "enabled" => matches!(value,"true"|"1"),
            "display.resolution" => parse_resolution(value).is_some(),
            "display.fps_limit" => integer.is_some_and(|fps| (30..=240).contains(&fps)),
            "display.vsync" => matches!(value, "true" | "false" | "0" | "1"),
            "display.show_fps" | "audio.muted" | "debug.recording" | "debug.auto_marks" => matches!(value, "true" | "false" | "0" | "1"),
            "audio.input_device" | "audio.output_device" => !value.is_empty() && value.len() <= 255 && value.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b)),
            "audio.volume" | "audio.game_volume" | "audio.voice_volume" => integer.is_some_and(|v| (0..=100).contains(&v)),
            "movement.click_mode" | "hero.movement.click_mode" => matches!(value, "0" | "1" | "2"),
            "movement.click_distance_percent" | "hero.movement.click_distance_percent" => integer.is_some_and(|v| (0..=500).contains(&v)),
            "movement.hold_ms" | "hero.movement.hold_ms" => integer.is_some_and(|v| (40..=600).contains(&v)),
            "hotkeys.settings" | "hotkeys.confine" | "hotkeys.fullscreen" | "hotkeys.chat" => {
                integer.is_some_and(|v| {
                    if v == 0 { return key == "hotkeys.chat"; }
                    valid_key(v) && !["hotkeys.settings", "hotkeys.confine", "hotkeys.fullscreen", "hotkeys.chat", "debug.mark_shortcut"]
                        .into_iter()
                        .any(|k| k != key && (self.integer(k, hero) == v
                            || ((k == "hotkeys.chat" || key == "hotkeys.chat") &&
                                enter_key(v) && enter_key(self.integer(k, hero)))))
                })
            }
            "debug.mark_shortcut" => integer.is_some_and(|v| {
                let base=v & !0x0e000000;
                v==0 || (v>0 && v & !0x3fffffff==0 && valid_key(base) && base!=0
                    && (v & 0x0e000000!=0 || !["hotkeys.settings","hotkeys.confine","hotkeys.fullscreen","hotkeys.chat"].iter().any(|key|self.integer(key,hero)==v || (*key=="hotkeys.chat" && enter_key(v) && enter_key(self.integer(key,hero))))))
            }),
            "hero.cancel_key" | "common.cancel_key" | "movement.button" | "hero.movement.button" => {
                integer.is_some_and(|v| valid_key(v) && !self.reserved(v, hero))
            }
            _ if (key.starts_with("movement.") || key.starts_with("hero.movement."))
                && ["up","down","left","right"].iter().any(|f| key.ends_with(&format!(".{f}"))) => {
                integer.is_some_and(|v| valid_key(v) && !self.reserved(v, hero))
            }
            _ if is_binding && key.ends_with(".key") => {
                integer.is_some_and(|v| valid_key(v) && !self.reserved(v, hero))
            }
            _ if is_binding && (key.ends_with(".bindings.extra:2.cast_mode") || key.contains(".bindings.spell:") && key.ends_with(".cast_mode")) => {
                matches!(value, "0" | "1" | "2" | "3")
            }
            _ if is_binding && key.ends_with(".cast_mode") => matches!(value, "0" | "1"),
            _ if key.starts_with("hero.movement.")&&key.ends_with(".override") => matches!(value,"true"|"false"|"0"|"1"),
            _ if key.strip_prefix("hero.allowed_conflict.").is_some_and(|key| {
                key.parse::<i32>().is_ok_and(|code| code != 0 && valid_key(code))
            }) => value == "-" || (value.len() == 16 && value.bytes().all(|b| b.is_ascii_hexdigit())),
            "hero.cancel_key.override" => matches!(value, "true" | "false" | "0" | "1"),
            _ if is_binding && (key.ends_with(".override") || key.ends_with(".stop_movement")) => {
                matches!(value, "true" | "false" | "0" | "1")
            }
            _ => false,
        };
        if !valid {
            return Err("Недопустимое значение или занятая системная кнопка".into());
        }
        let name = Self::canonical(key, hero);
        let before = self.values.clone();
        self.values.insert(name.clone(), value.into());
        if key == "hero.cancel_key" || (key.starts_with("hero.movement.")&&!key.ends_with(".override")) || (key.starts_with("hero.bindings.") && [".key", ".cast_mode", ".stop_movement"].iter().any(|f|key.ends_with(f))) {
            self.values.insert(format!("{name}.override"), "true".into());
        }
        // Keep older clients' whole-button override switch compatible. New UI
        // writes per-field markers and never changes neighboring settings.
        if let Some(tail) = key.strip_prefix("hero.bindings.").and_then(|k|k.strip_suffix(".override")) {
            if !tail.contains('.') {
                for field in ["key", "cast_mode", "stop_movement"] {
                    self.values.insert(Self::canonical(&format!("hero.bindings.{tail}.{field}.override"),hero),value.into());
                }
            }
        }
        if let Err(e) = self.save() { self.values=before; return Err(e); }
        Ok(())
    }
    fn reserved(&self, key: i32, hero: i32) -> bool {
        key != 0
            && ["hotkeys.settings", "hotkeys.confine", "hotkeys.fullscreen", "hotkeys.chat", "debug.mark_shortcut"]
                .into_iter()
                .any(|k| self.integer(k, hero) == key ||
                    (k == "hotkeys.chat" && enter_key(key) && enter_key(self.integer(k, hero))))
    }
    fn save(&self) -> Result<(), String> {
        if self.path.as_os_str().is_empty() {
            return Ok(());
        }
        let parent = self.path.parent().ok_or("Missing settings directory")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let temp = parent.join(format!("settings.{}.tmp", std::process::id()));
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&temp, bytes)
            .and_then(|_| std::fs::rename(&temp, &self.path))
            .map_err(|e| e.to_string())
    }
}
pub fn parse_frame_pacing(value: &str) -> Option<(u32, bool)> {
    let mut limit = None;
    let mut vsync = None;
    for item in value.split(';') {
        let (key, value) = item.split_once('=')?;
        match key {
            "limit" if limit.is_none() => {
                if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                    return None;
                }
                let fps = value.parse::<u32>().ok()?;
                if !(30..=240).contains(&fps) {
                    return None;
                }
                limit = Some(fps);
            }
            "vsync" if vsync.is_none() => {
                vsync = Some(match value {
                    "0" => false,
                    "1" => true,
                    _ => return None,
                })
            }
            _ => return None,
        }
    }
    limit.zip(vsync)
}

pub fn parse_resolution(value: &str) -> Option<(u32, u32)> {
    let (w, h) = value.split_once('x')?;
    let w = w.parse::<u32>().ok()?;
    let h = h.parse::<u32>().ok()?;
    ((256..=8192).contains(&w)
        && (256..=8192).contains(&h)
        && u64::from(w) * u64::from(h) <= 32 * 1024 * 1024)
        .then_some((w, h))
}
pub fn plain_spell_id(id:i32)->bool {matches!(id,20030|20040|20080|20190)}
fn plain_spell_slot(slot:&str)->bool {slot.strip_prefix("extra:2:").and_then(|s|s.parse::<i32>().ok()).is_some_and(plain_spell_id)}
pub fn hold_duration_skill(hero: i32, slot: &str) -> bool {
    let base=variant_base(slot).unwrap_or(slot);
    (hero==126 && matches!(base,"skill:0"|"skill:1"|"skill:2"))
        || (hero==78 && base=="skill:0")
        || (hero==104 && base=="skill:2")
}
fn enter_key(key: i32) -> bool {
    matches!(key, 0x01000004 | 0x01000005)
}

pub fn valid_key(key: i32) -> bool {
    key == 0
        || ((32..=0x10ffff).contains(&key) && !(0xd800..=0xdfff).contains(&key))
        || ((key & !0xffffff)==0x10000000 && key & 0xffffff != 0)
        || ((key & !0xffff)==0x20000000 && key & 0xffff != 0)
        || ((0x01000000..=0x010000ff).contains(&key)
            && ![0x01000020, 0x01000021, 0x01000022, 0x01000023].contains(&key))
        || ((key & MOUSE) != 0 && {
            let mask = key ^ MOUSE;
            mask >= 2 && mask <= (1 << 24) && (mask as u32).is_power_of_two()
        })
        || ((key & !0xffff) == 0x03000000 && matches!(key & 0xffff, 4..=7 | 30..=65535))
}
pub fn variant_base(slot: &str) -> Option<&str> {
    let (base, id)=slot.rsplit_once(':')?;
    (native_skill_slot(base) && id.parse::<i32>().is_ok_and(|v|v>0)).then_some(base)
}
fn native_skill_slot(slot: &str) -> bool {
    slot.strip_prefix("skill:")
        .is_some_and(|index| !index.is_empty() && index.bytes().all(|v| v.is_ascii_digit())
            && index.parse::<u8>().is_ok())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn battle_spell_settings_preserve_existing_binds_and_allow_independent_overrides() {
        let mut settings=Settings::default();
        settings.set("hero.bindings.extra:2.key","71",25).unwrap();
        assert_eq!(settings.binding(25,"spell:20150").key,71);
        assert_eq!(settings.binding(25,"spell:20100").key,71);
        settings.set("hero.bindings.spell:20150.key","72",25).unwrap();
        assert_eq!(settings.binding(25,"spell:20150").key,72);
        assert_eq!(settings.binding(25,"spell:20100").key,71);
        assert_eq!(settings.binding(53,"spell:20150").key,70);
        settings.set("hero.bindings.spell:20150.key.override","false",25).unwrap();
        assert_eq!(settings.binding(25,"spell:20150").key,70);
    }
    #[test]
    fn mathilda_gift_inherits_until_a_hero_has_an_override() {
        let mut s=Settings::default();
        for hero in 1..=133 {
            assert_eq!(s.binding(hero,"ally:mathilda").key,86);
            assert_eq!(s.binding(hero,"ally:mathilda").cast_mode,0);
        }
        for mode in [0,1] {
            s.set("common.bindings.ally:mathilda.key","88",53).unwrap();
            s.set("common.bindings.ally:mathilda.cast_mode",&mode.to_string(),53).unwrap();
            s.set("hero.bindings.ally:mathilda.key","90",102).unwrap();
            for hero in 1..=133 {
                assert_eq!(s.binding(hero,"ally:mathilda").key,if hero==102 {90} else {88});
                assert_eq!(s.binding(hero,"ally:mathilda").cast_mode,mode);
            }
        }
    }
    #[test]
    fn active_item_has_two_modes_and_supports_personal_overrides() {
        let mut s = Settings::default();
        assert_eq!(s.binding(25, "item:active").key, 0x01000007);
        assert_eq!(s.binding(25, "item:active").cast_mode, 0);
        for mode in [0, 1] {
            s.set("common.bindings.item:active.cast_mode", &mode.to_string(), 25).unwrap();
            s.set("common.bindings.item:active.key", "88", 25).unwrap();
            s.set("hero.bindings.item:active.cast_mode", &(1-mode).to_string(), 25).unwrap();
            s.set("hero.bindings.item:active.key", "90", 25).unwrap();
            for hero in 1..=133 {
                assert_eq!(s.binding(hero, "item:active").key, if hero==25 {90} else {88});
                assert_eq!(s.binding(hero, "item:active").cast_mode, if hero==25 {1-mode} else {mode});
                assert_eq!(s.binding(hero, "extra:3").key, 0x01000007);
            }
        }
        assert!(s.set("common.bindings.item:active.cast_mode", "2", 25).is_err());
    }
    #[test]
    fn statistics_binding_is_global_rebindable_and_does_not_inherit_hero_overrides() {
        let mut s = Settings::default();
        for hero in 1..=133 { assert_eq!(s.binding(hero, "match:statistics").key, 0x01000001); }
        s.set("common.bindings.match:statistics.key", "88", 25).unwrap();
        s.set("hero.bindings.match:statistics.key", "90", 25).unwrap();
        for hero in 1..=133 {
            assert_eq!(s.binding(hero, "match:statistics").key, 88);
            assert_eq!(s.binding(hero, "match:statistics").cast_mode, 1);
        }
        s.set("common.bindings.match:statistics.key", "0", 25).unwrap();
        assert_eq!(s.binding(25, "match:statistics").key, 0);
    }
    #[test]
    fn guinevere_ultimate_stop_is_personal_editable_and_seeded_once() {
        let mut s = Settings::default();
        assert!(s.install_guinevere_ultimate_default());
        assert!(s.binding(80, "skill:2").stop_movement);
        assert!(s.binding(80, "skill:2:8030").stop_movement);
        for hero in 1..=133 {
            for slot in ["skill:0", "skill:1", "skill:2"] {
                assert_eq!(s.binding(hero, slot).stop_movement, hero == 80 && slot == "skill:2");
            }
        }
        s.set("hero.bindings.skill:2.stop_movement", "false", 80).unwrap();
        assert!(!s.install_guinevere_ultimate_default());
        assert!(!s.binding(80, "skill:2").stop_movement);
    }
    #[test]
    fn guinevere_ultimate_default_preserves_existing_personal_choice_or_inheritance() {
        for (key, value) in [
            ("heroes.80.bindings.skill:2.stop_movement", "false"),
            ("heroes.80.bindings.skill:2.stop_movement.override", "false"),
        ] {
            let mut s = Settings::default();
            s.values.insert(key.into(), value.into());
            s.install_guinevere_ultimate_default();
            assert_eq!(s.values.get(key).map(String::as_str), Some(value));
            assert!(!s.binding(80, "skill:2").stop_movement);
        }
    }
    #[test]
    fn requested_directional_profiles_are_personal_editable_and_idempotent() {
        let mut s=Settings::default();s.values.insert("common.bindings.attack.key".into(),"32".into());
        s.values.insert("movement.click_mode".into(),"0".into());let before=s.values.clone();
        assert!(s.install_directional_hero_defaults());
        for hero in [71,125] {
            assert_eq!(s.integer("hero.movement.click_mode",hero),2);
            for (field,code)in[("up",87),("down",83),("left",65),("right",68)] {
                assert_eq!(s.integer(&format!("hero.movement.{field}"),hero),code);
            }
            assert_eq!(s.binding(hero,"attack:minion").key,88);assert_eq!(s.binding(hero,"attack:tower").key,67);
            assert_eq!(s.binding(hero,"extra:1").key,71);
        }
        assert_eq!(s.binding(71,"attack").key,MOUSE|2);assert_eq!(s.binding(71,"skill:1").key,69);
        assert_eq!(s.binding(125,"skill:1").key,MOUSE|2);assert_eq!(s.binding(125,"attack").key,32);
        for (k,v)in before {assert_eq!(s.values.get(&k),Some(&v));}
        for hero in [9,13,21,22,25,26,31,40,43,47,55,56,73,77,78,83,84,90,95,97,104,118,122,126,127,131,133] {
            assert_eq!(s.integer("hero.movement.click_mode",hero),0);assert_eq!(s.binding(hero,"attack").key,32);
        }
        s.values.insert("heroes.71.bindings.attack.key.override".into(),"false".into());
        assert_eq!(s.binding(71,"attack").key,32);assert!(!s.install_directional_hero_defaults());
    }
    #[test]
    fn requested_profiles_preserve_existing_personal_values_and_explicit_inheritance() {
        let mut s=Settings::default();s.values.insert("heroes.71.bindings.attack.key".into(),"88".into());
        s.values.insert("heroes.71.bindings.attack.key.override".into(),"true".into());
        s.values.insert("heroes.125.movement.click_mode.override".into(),"false".into());
        s.install_directional_hero_defaults();assert_eq!(s.binding(71,"attack").key,88);
        assert_eq!(s.integer("hero.movement.click_mode",125),0);
        assert!(!s.values.contains_key("heroes.125.movement.click_mode"));
    }
    #[test]
    fn keyboard_defaults_arrow_remapping_and_per_direction_inheritance() {
        let mut s=Settings::default();
        s.set("movement.click_mode","2",0).unwrap();
        for (name,code) in [("up",87),("down",83),("left",65),("right",68)] {
            assert_eq!(s.integer(&format!("hero.movement.{name}"),25),code);
        }
        s.set("hero.movement.up","16777235",25).unwrap();
        s.set("movement.down","16777237",0).unwrap();
        assert_eq!(s.integer("hero.movement.up",25),16777235);
        assert_eq!(s.integer("hero.movement.up",84),87);
        assert_eq!(s.integer("hero.movement.down",25),16777237);
        assert!(s.set("movement.click_mode","3",0).is_err());
    }
    #[test]
    fn alternate_modes_and_stop_flags_are_independent_but_key_is_shared() {
        for (hero,base,alt) in [(25,"skill:1","skill:1:2521"),(25,"skill:2","skill:2:2531"),(84,"skill:1","skill:1:8425"),(131,"skill:2","skill:2:13131")] {
            let mut s=Settings::default();
            s.set(&format!("hero.bindings.{base}.cast_mode"),"1",hero).unwrap();
            assert_eq!(s.binding(hero,alt).cast_mode,1);
            s.set(&format!("hero.bindings.{alt}.cast_mode"),"0",hero).unwrap();
            s.set(&format!("hero.bindings.{alt}.stop_movement"),"true",hero).unwrap();
            s.set(&format!("hero.bindings.{base}.key"),"88",hero).unwrap();
            assert_eq!(s.binding(hero,base).cast_mode,1);
            assert_eq!(s.binding(hero,alt).cast_mode,0);
            assert!(s.binding(hero,alt).stop_movement);
            assert!(!s.binding(hero,base).stop_movement);
            assert_eq!(s.binding(hero,alt).key,88);
            s.set(&format!("hero.bindings.{alt}.cast_mode.override"),"false",hero).unwrap();
            assert_eq!(s.binding(hero,alt).cast_mode,1);
        }
    }
    #[test]
    fn every_unconfigured_action_defaults_to_release_and_quick_cast_is_explicit() {
        let mut s = Settings::default();
        let slots = ["skill:0", "skill:1", "skill:2", "skill:3", "skill:1:2521",
            "combo:0", "combo:1", "attack", "attack:minion", "attack:tower",
            "upgrade:0", "upgrade:1", "upgrade:2", "upgrade:3", "shop:quick",
            "extra:0", "extra:1", "extra:2", "extra:3", "extra:2:20080"];
        for hero in [0, 25, 30, 73, 131] {
            for kind in 0..=4 {
                s.observe_native_templates(hero, &BTreeMap::from([("skill:2".into(), kind)]));
                for slot in slots {
                    assert_eq!(s.binding(hero, slot).cast_mode, 0, "hero={hero}, slot={slot}, type={kind}");
                }
            }
        }
        for slot in slots {
            s.set(&format!("common.bindings.{slot}.cast_mode"), "1", 0).unwrap();
            assert_eq!(s.binding(73, slot).cast_mode, 1);
            s.set(&format!("hero.bindings.{slot}.cast_mode"), "0", 73).unwrap();
            assert_eq!(s.binding(73, slot).cast_mode, 0);
            assert_eq!(s.binding(25, slot).cast_mode, 1);
        }
    }
    #[test]
    fn native_skill_templates_share_cast_defaults_between_menu_and_bindings() {
        let mut s = Settings::default();
        s.observe_native_templates(30, &BTreeMap::from([
            ("skill:0".into(), 1), ("skill:1".into(), 2), ("skill:2".into(), 3),
        ]));
        for (slot, expected) in [("skill:0", 0), ("skill:1", 0), ("skill:2", 0)] {
            assert_eq!(s.integer(&format!("hero.bindings.{slot}.cast_mode"), 30), expected);
            assert_eq!(s.binding(30, slot).cast_mode, expected);
        }
        assert!(s.values.is_empty());
        let saved = serde_json::to_string(&s).unwrap();
        assert_eq!(saved, "{\"values\":{}}");
        let restored: Settings = serde_json::from_str(&saved).unwrap();
        assert_eq!(restored.binding(30, "skill:2").cast_mode, 0);
    }
    #[test]
    fn native_template_updates_preserve_explicit_modes_keys_and_form_settings() {
        let mut s = Settings::default();
        s.set("hero.bindings.skill:0.cast_mode", "0", 30).unwrap();
        s.set("hero.bindings.skill:2.cast_mode", "1", 30).unwrap();
        s.set("hero.bindings.skill:0.key", "88", 30).unwrap();
        s.set("hero.bindings.skill:2.stop_movement", "true", 30).unwrap();
        s.set("hero.bindings.skill:1:2521.cast_mode", "0", 25).unwrap();
        let explicit = s.values.clone();
        for types in [
            BTreeMap::from([("skill:0".into(), 1), ("skill:2".into(), 3)]),
            BTreeMap::from([("skill:0".into(), 3), ("skill:2".into(), 1)]),
            BTreeMap::new(),
        ] {
            s.observe_native_templates(30, &types);
            assert_eq!(s.binding(30, "skill:0").cast_mode, 0);
            assert_eq!(s.binding(30, "skill:2").cast_mode, 1);
            assert_eq!(s.binding(30, "skill:0").key, 88);
            assert!(s.binding(30, "skill:2").stop_movement);
            assert_eq!(s.binding(25, "skill:1:2521").cast_mode, 0);
            assert_eq!(s.values, explicit);
        }
    }
    #[test]
    fn native_geometry_changes_never_enable_quick_cast() {
        let mut s = Settings::default();
        s.observe_native_templates(30, &BTreeMap::from([("skill:2".into(), 3)]));
        assert_eq!(s.binding(30, "skill:2").cast_mode, 0);
        assert_eq!(s.binding(31, "skill:2").cast_mode, 0);
        s.observe_native_templates(31, &BTreeMap::from([("skill:2".into(), 2)]));
        assert_eq!(s.binding(31, "skill:2").cast_mode, 0);
        assert_eq!(s.binding(30, "skill:2").cast_mode, 0);
        s.observe_native_templates(30, &BTreeMap::from([("skill:2".into(), 1)]));
        assert_eq!(s.binding(30, "skill:2").cast_mode, 0);
        s.observe_native_templates(30, &BTreeMap::from([("skill:2".into(), 3)]));
        s.observe_native_templates(30, &BTreeMap::from([("skill:2".into(), 0)]));
        assert_eq!(s.binding(30, "skill:2").cast_mode, 0);
        s.observe_native_templates(30, &BTreeMap::from([("skill:0".into(), 1)]));
        assert_eq!(s.binding(30, "skill:0").cast_mode, 0);
        s.observe_native_templates(30, &BTreeMap::new());
        assert_eq!(s.binding(30, "skill:0").cast_mode, 0);
    }
    #[test]
    fn native_selection_and_following_forms_default_to_release() {
        let mut s=Settings::default();
        s.observe_native_templates(131,&BTreeMap::from([("skill:2".into(),4)]));
        assert_eq!(s.binding(131,"skill:2").cast_mode,0);
        assert_eq!(s.binding(25,"skill:2").cast_mode,0);
        s.observe_native_templates(131,&BTreeMap::from([("skill:2".into(),1)]));
        assert_eq!(s.binding(131,"skill:2").cast_mode,0);
        s.set("hero.bindings.skill:2.cast_mode","1",131).unwrap();
        s.observe_native_templates(131,&BTreeMap::from([("skill:2".into(),4)]));
        assert_eq!(s.binding(131,"skill:2").cast_mode,1,"a native default must preserve an explicit user's mode");
        s.observe_native_templates(131,&BTreeMap::new());
        assert_eq!(s.binding(131,"skill:2").cast_mode,1);
    }
    #[test]
    fn native_skill_defaults_never_change_common_controls_or_kagura_away_form() {
        let mut s = Settings::default();
        s.observe_native_templates(25, &BTreeMap::from([
            ("skill:1".into(), 2), ("skill:1:2521".into(), 3),
            ("combo:0".into(), 1), ("extra:2".into(), 1), ("attack".into(), 3),
        ]));
        assert_eq!(s.binding(25, "skill:1:2521").cast_mode, 0);
        assert_eq!(s.binding(25, "combo:0").cast_mode, 0);
        assert_eq!(s.binding(25, "extra:2").cast_mode, 0);
        assert_eq!(s.binding(25, "attack").cast_mode, 0);
        assert_eq!(s.integer("common.bindings.skill:1.cast_mode", 25), 0);
        s.set("common.bindings.extra:2.cast_mode", "3", 25).unwrap();
        s.observe_native_templates(25, &BTreeMap::from([("extra:2".into(), 1)]));
        assert_eq!(s.binding(25, "extra:2").cast_mode, 3);
    }
    #[test]
    fn native_charge_and_tap_geometry_share_the_release_default() {
        let mut s = Settings::default();
        s.observe_native_templates(30, &BTreeMap::from([("skill:0".into(), 1)]));
        assert_eq!(s.binding(30, "skill:0").cast_mode, 0);
        s.observe_native_holds(30, &BTreeMap::from([("skill:0".into(), true), ("skill:2".into(), true)]));
        for slot in ["skill:0", "skill:2"] {
            assert_eq!(s.binding(30, slot).cast_mode, 0);
            assert_eq!(s.integer(&format!("hero.bindings.{slot}.cast_mode"), 30), 0);
        }
        s.observe_native_holds(30, &BTreeMap::from([("skill:0".into(), false)]));
        assert_eq!(s.binding(30, "skill:0").cast_mode, 0);
        assert_eq!(s.binding(30, "skill:2").cast_mode, 0);
        s.observe_native_holds(30, &BTreeMap::from([("skill:0".into(), true)]));
        s.observe_native_holds(30, &BTreeMap::new());
        assert_eq!(s.binding(30, "skill:0").cast_mode, 0);
        assert!(s.values.is_empty());
    }
    #[test]
    fn native_charge_never_overrides_saved_choices_other_heroes_or_common_buttons() {
        let mut s = Settings::default();
        s.set("hero.bindings.skill:0.cast_mode", "1", 30).unwrap();
        s.set("hero.bindings.skill:1.cast_mode", "0", 30).unwrap();
        let saved = s.values.clone();
        s.observe_native_templates(30, &BTreeMap::from([("skill:0".into(), 1), ("skill:1".into(), 1)]));
        s.observe_native_holds(30, &BTreeMap::from([
            ("skill:0".into(), true), ("skill:1".into(), true), ("skill:2".into(), true),
            ("extra:2".into(), true), ("attack".into(), true), ("combo:0".into(), true),
        ]));
        assert_eq!(s.binding(30, "skill:0").cast_mode, 1);
        assert_eq!(s.binding(30, "skill:1").cast_mode, 0);
        assert_eq!(s.binding(30, "skill:2").cast_mode, 0);
        assert_eq!(s.binding(31, "skill:2").cast_mode, 0);
        assert_eq!(s.binding(30, "extra:2").cast_mode, 0);
        assert_eq!(s.binding(30, "attack").cast_mode, 0);
        assert_eq!(s.binding(30, "combo:0").cast_mode, 0);
        s.clear_native_holds();
        assert_eq!(s.binding(30, "skill:2").cast_mode, 0);
        assert_eq!(s.values, saved);
        let restored: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(restored.values, saved);
        assert_eq!(restored.binding(30, "skill:0").cast_mode, 1);
    }
    #[test]
    fn mapper_is_always_enabled_even_with_legacy_disabled_setting() {
        let mut s=Settings::default();s.values.insert("enabled".into(),"false".into());
        assert!(s.boolean("enabled",0));assert!(s.set("enabled","false",0).is_err());
    }
    #[test]
    fn audio_channels_are_independent_and_old_settings_default_full_volume() {
        let mut s=Settings::default();
        s.set("audio.volume","72",0).unwrap();
        assert_eq!(s.integer("audio.game_volume",0),100);
        assert_eq!(s.integer("audio.voice_volume",0),100);
        s.set("audio.game_volume","0",0).unwrap();
        s.set("audio.voice_volume","35",0).unwrap();
        let restored:Settings=serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(restored.integer("audio.volume",0),72);
        assert_eq!(restored.integer("audio.game_volume",0),0);
        assert_eq!(restored.integer("audio.voice_volume",0),35);
        for key in ["audio.volume","audio.game_volume","audio.voice_volume"] {
            for bad in ["-1","101","NaN"] {assert!(s.set(key,bad,0).is_err());}
        }
    }

    #[test]
    fn debug_shortcuts_preserve_modifiers_and_do_not_steal_plain_s() {
        let mut s=Settings::default();assert!(!s.boolean("debug.recording",25));assert!(s.boolean("debug.auto_marks",25));
        assert_eq!(s.integer("debug.mark_shortcut",25),0x04000053);
        s.set("common.bindings.attack:tower.key","83",25).unwrap();
        s.set("debug.recording","true",25).unwrap();
        for chord in [0x04000053,0x06000053,0x08000053,0x01000037] {s.set("debug.mark_shortcut",&chord.to_string(),25).unwrap();}
        s.set("debug.mark_shortcut",&0x01000037.to_string(),25).unwrap();
        assert!(s.set("hotkeys.settings",&0x01000037.to_string(),25).is_err());
        for key in [0x02000001,0x02000002,0x01000036,0x40000053] {assert!(s.set("debug.mark_shortcut",&key.to_string(),25).is_err());}
    }
    #[test]
    fn frame_pacing_defaults_and_global_settings() {
        let mut s = Settings::default();
        assert_eq!(s.get("display.fps_limit", 25), "60");
        assert!(s.boolean("display.vsync", 25));
        assert_eq!(s.get("display.frame_pacing", 25), "limit=60;vsync=1");
        s.set("display.frame_pacing", "limit=240;vsync=0", 25)
            .unwrap();
        assert_eq!(s.integer("display.fps_limit", 1), 240);
        assert!(!s.boolean("display.vsync", 1));
        assert!(!s.values.contains_key("display.frame_pacing"));
        for fps in [30, 31, 60, 90, 120, 144, 165, 180, 240] {
            s.set("display.fps_limit", &fps.to_string(), 0).unwrap();
            assert_eq!(s.integer("display.fps_limit", 0), fps);
        }
        s.set("display.vsync", "true", 0).unwrap();
        assert!(s.boolean("display.vsync", 0));
        assert!(s.set("display.fps_limit", "241", 0).is_err());
        assert!(s.set("display.fps_limit", "29", 0).is_err());
        assert!(s.set("display.vsync", "maybe", 0).is_err());
        s.values.insert("display.fps_limit".into(), "999999".into());
        s.values.insert("display.vsync".into(), "invalid".into());
        assert_eq!(s.integer("display.fps_limit", 0), 60);
        assert!(s.boolean("display.vsync", 0));
    }
    #[test]
    fn frame_pacing_parser_rejects_partial_duplicate_unknown_and_unbounded() {
        assert_eq!(parse_frame_pacing("limit=120;vsync=0"), Some((120, false)));
        assert_eq!(parse_frame_pacing("vsync=1;limit=30"), Some((30, true)));
        for value in [
            "",
            "limit=120",
            "vsync=1",
            "limit=120;vsync=1;",
            "limit=120;vsync=1;extra=0",
            "limit=120;limit=60;vsync=1",
            "limit=120;vsync=1;vsync=0",
            "limit=29;vsync=1",
            "limit=241;vsync=1",
            "limit=-1;vsync=1",
            "limit=+60;vsync=1",
            "limit=999999999999999999;vsync=1",
            "limit=60;vsync=true",
            "limit=60;vsync=2",
            " limit=60;vsync=1",
            "limit=60;vsync=1=0",
        ] {
            assert_eq!(parse_frame_pacing(value), None, "{value}");
        }
        let mut s = Settings::default();
        s.set("display.frame_pacing", "limit=165;vsync=0", 0)
            .unwrap();
        let before = s.values.clone();
        assert!(
            s.set("display.frame_pacing", "limit=240;vsync=1;unknown=2", 0)
                .is_err()
        );
        assert_eq!(s.values, before);
    }
    #[test]
    fn frame_pacing_atomic_write_rolls_back_both_fields_on_save_error() {
        let mut s = Settings::default();
        s.set("display.frame_pacing", "limit=90;vsync=0", 0)
            .unwrap();
        let before = s.values.clone();
        let marker = std::env::temp_dir().join(format!(
            "clbb-pacing-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&marker, b"not a directory").unwrap();
        s.path = marker.join("settings.json");
        assert!(
            s.set("display.frame_pacing", "limit=240;vsync=1", 0)
                .is_err()
        );
        assert_eq!(s.values, before);
        std::fs::remove_file(marker).unwrap();
    }
    #[test]
    fn resolution_import_is_independent_and_atomic() {
        let mut s = Settings::default();
        s.import_resolution("profile\\resolutionKeys=1600x1400, 1800x1400, 4000x3111\n");
        assert_eq!(s.get("display.width", 0), "4000");
        assert_eq!(s.get("display.height", 0), "3111");
        assert!(s.set("display.resolution", "8192x8192", 0).is_err());
        assert_eq!(s.get("display.width", 0), "4000");
        s.set("display.resolution", "1920x1080", 0).unwrap();
        assert_eq!(s.get("display.height", 0), "1080");
    }
    #[test]
    fn quick_chat_defaults_to_enter_and_can_be_rebound() {
        let mut s = Settings::default();
        assert_eq!(s.integer("hotkeys.chat", 0), 0x01000004);
        assert!(s.set("hotkeys.fullscreen", &0x01000005.to_string(), 0).is_err());
        assert!(s.set("common.bindings.skill:0.key", &0x01000005.to_string(), 0).is_err());
        assert!(s.set("hotkeys.chat", &s.integer("hotkeys.settings", 0).to_string(), 0).is_err());
        s.set("hotkeys.chat", "0", 0).unwrap();
        s.set("common.bindings.skill:0.key", &0x01000005.to_string(), 0).unwrap();
        s.set("hotkeys.chat", &0x01000037.to_string(), 0).unwrap();
        assert_eq!(s.integer("hotkeys.chat", 0), 0x01000037);
    }
    #[test]
    fn lmb_never_bindable() {
        let mut s = Settings::default();
        for k in [
            "movement.button",
            "hotkeys.settings",
            "hero.bindings.skill:0.key",
        ] {
            assert!(s.set(k, &(MOUSE | 1).to_string(), 25).is_err());
        }
    }
    #[test]
    fn same_form_key_supported() {
        let mut s = Settings::default();
        s.set("hero.bindings.skill:1.key", "87", 25).unwrap();
        s.set("hero.bindings.skill:1:2521.key", "87", 25).unwrap();
        assert_eq!(s.binding(25, "skill:1:2521").key, 87);
        assert_eq!(s.binding(25, "skill:1:2521").cast_mode, 0);
    }
    #[test]
    fn global_override_inherits() {
        let mut s = Settings::default();
        s.set("common.bindings.extra:2.key", "88", 25).unwrap();
        assert_eq!(s.binding(25, "extra:2").key, 88);
        s.set("hero.bindings.extra:2.key", "70", 25).unwrap();
        assert_eq!(s.binding(25, "extra:2").key, 70);
        s.set("hero.bindings.extra:2.override", "true", 25).unwrap();
        assert_eq!(s.binding(25, "extra:2").key, 70);
        assert_eq!(s.binding(26, "extra:2").key, 88);
    }
    #[test]
    fn imports_without_runtime_dependency() {
        let mut s = Settings::default();
        s.import_legacy("[common]\nbuttons\\extra%3A2\\key=70\n[heroes]\n25\\buttons\\skill%3A1%3A2521\\castMode=1\n[movement]\nholdMs=150");
        assert_eq!(s.binding(25, "extra:2").key, 70);
        assert_eq!(s.get("movement.hold_ms", 25), "150");
    }
}

#[cfg(test)]
mod purify_settings_tests {
    use super::*;
    #[test]
    fn purify_mode_is_shared_without_changing_aimed_spells() {
        let mut s=Settings::default();
        assert_eq!(s.integer("common.bindings.extra:2:20080.cast_mode",73),0);
        s.values.insert("common.bindings.extra:2:20080.cast_mode".into(),"0".into());
        s.values.insert("common.bindings.extra:2.cast_mode".into(),"3".into());
        for hero in [25,73] {assert_eq!(s.integer("hero.bindings.extra:2:20080.cast_mode",hero),0);assert_eq!(s.binding(hero,"extra:2").cast_mode,3);}
        s.values.insert("heroes.73.bindings.extra:2.override".into(),"true".into());
        s.values.insert("heroes.73.bindings.extra:2:20080.cast_mode".into(),"1".into());
        assert_eq!(s.integer("hero.bindings.extra:2:20080.cast_mode",73),1);
        assert_eq!(s.integer("hero.bindings.extra:2:20080.cast_mode",25),0);
    }
}

#[cfg(test)]
mod field_inheritance_tests {
    use super::*;
    #[test]
    fn each_field_inherits_until_edited_and_resets_independently() {
        let mut s=Settings::default();
        s.set("common.bindings.skill:0.key","81",0).unwrap();
        s.set("common.bindings.skill:0.cast_mode","0",0).unwrap();
        s.set("hero.bindings.skill:0.key","88",73).unwrap();
        s.set("common.bindings.skill:0.key","89",0).unwrap();
        s.set("common.bindings.skill:0.cast_mode","1",0).unwrap();
        assert_eq!(s.binding(73,"skill:0").key,88);
        assert_eq!(s.binding(25,"skill:0").key,89);
        assert_eq!(s.binding(73,"skill:0").cast_mode,1);
        s.set("hero.bindings.skill:0.cast_mode","0",73).unwrap();
        s.set("hero.bindings.skill:0.key.override","false",73).unwrap();
        assert_eq!(s.binding(73,"skill:0").key,89);
        assert_eq!(s.binding(73,"skill:0").cast_mode,0);
        s.set("common.bindings.skill:0.key","90",0).unwrap();
        assert_eq!(s.binding(73,"skill:0").key,90);
        assert_eq!(s.binding(25,"skill:0").cast_mode,1);
    }
    #[test]
    fn explicit_equal_value_is_still_personal_and_spells_cross_heroes() {
        let mut s=Settings::default();
        s.set("common.bindings.extra:2.key","70",0).unwrap();
        s.set("hero.bindings.extra:2.key","70",73).unwrap();
        s.set("common.bindings.extra:2.key","71",0).unwrap();
        assert_eq!(s.binding(73,"extra:2").key,70);
        assert_eq!(s.binding(25,"extra:2").key,71);
        s.set("common.bindings.extra:2.cast_mode","3",0).unwrap();
        assert_eq!(s.binding(73,"extra:2").cast_mode,3);
        assert_eq!(s.binding(25,"extra:2").cast_mode,3);
    }
    #[test]
    fn migration_preserves_active_legacy_choices_and_dormant_overrides() {
        let mut s=Settings::default();
        for (k,v) in [("common.bindings.extra:2.key","70"),("heroes.73.bindings.extra:2.key","71"),("heroes.73.bindings.extra:2.override","false"),("heroes.25.bindings.extra:2.key","72"),("heroes.25.bindings.extra:2.override","true"),("heroes.73.bindings.skill:0.key","90"),("heroes.73.cancel_key","85")] {s.values.insert(k.into(),v.into());}
        assert!(s.migrate_field_inheritance());
        assert_eq!(s.binding(73,"extra:2").key,70);
        assert_eq!(s.binding(25,"extra:2").key,72);
        assert_eq!(s.binding(73,"skill:0").key,90);
        assert_eq!(s.integer("hero.cancel_key",73),85);
        s.set("hero.bindings.skill:0.key.override","false",73).unwrap();
        assert!(!s.migrate_field_inheritance());
        assert_eq!(s.binding(73,"skill:0").key,81);
    }
    #[test]
    fn failed_persistence_rolls_back_both_value_and_override_marker() {
        let mut s=Settings::default();
        let path=std::env::temp_dir().join(format!("clbb-inheritance-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::write(&path,b"not a directory").unwrap();s.path=path.join("settings.json");
        let before=s.values.clone();
        assert!(s.set("hero.bindings.extra:2.key","70",73).is_err());
        assert_eq!(s.values,before);std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn inspire_has_plain_modes_independent_of_aimed_spell_modes() {
        let mut s=Settings::default();s.set("common.bindings.extra:2.cast_mode","3",0).unwrap();
        assert_eq!(s.integer("hero.bindings.extra:2:20030.cast_mode",77),0);
        s.set("common.bindings.extra:2:20030.cast_mode","1",0).unwrap();
        assert_eq!(s.integer("hero.bindings.extra:2:20030.cast_mode",77),1);
        assert_eq!(s.binding(77,"extra:2").cast_mode,3);
        assert_eq!(s.integer("hero.bindings.extra:2:20080.cast_mode",77),0);
    }
    #[test]
    fn sprint_uses_plain_release_default_and_independent_inherited_modes() {
        let mut s=Settings::default();
        s.set("common.bindings.extra:2.cast_mode","3",0).unwrap();
        assert_eq!(s.integer("hero.bindings.extra:2:20040.cast_mode",43),0);
        s.set("common.bindings.extra:2:20040.cast_mode","1",0).unwrap();
        for hero in [25,43,55] { assert_eq!(s.integer("hero.bindings.extra:2:20040.cast_mode",hero),1); }
        s.set("hero.bindings.extra:2:20040.cast_mode","0",43).unwrap();
        assert_eq!(s.integer("hero.bindings.extra:2:20040.cast_mode",43),0);
        assert_eq!(s.integer("hero.bindings.extra:2:20040.cast_mode",55),1);
        assert_eq!(s.binding(43,"extra:2").cast_mode,3);
        assert_eq!(s.integer("hero.bindings.extra:2:20030.cast_mode",43),0);
        assert!(s.set("hero.bindings.extra:2:20040.cast_mode","3",43).is_err());
    }
    #[test]
    fn vengeance_plain_mode_is_separate_from_aimed_spells_and_other_plain_spells() {
        let mut s=Settings::default();
        s.set("common.bindings.extra:2.cast_mode","3",0).unwrap();
        assert_eq!(s.integer("hero.bindings.extra:2:20190.cast_mode",127),0);
        s.set("hero.bindings.extra:2:20190.cast_mode","1",127).unwrap();
        assert_eq!(s.integer("hero.bindings.extra:2:20190.cast_mode",127),1);
        assert_eq!(s.binding(127,"extra:2").cast_mode,3);
        assert_eq!(s.integer("hero.bindings.extra:2:20030.cast_mode",127),0);
        assert!(s.set("hero.bindings.extra:2:20190.cast_mode","3",127).is_err());
    }
    #[test]
    fn suyou_three_skills_always_release_and_internal_forms_share_one_setting() {
        let mut s=Settings::default();
        for (n,base) in [(0,12610),(1,12630),(2,12650)] {
            let slot=format!("skill:{n}");let alt=format!("{slot}:{}",base+2);
            // Previously saved quick modes must not bypass the restriction.
            s.values.insert(format!("common.bindings.{slot}.cast_mode"),"1".into());
            s.values.insert(format!("heroes.126.bindings.{slot}.cast_mode"),"1".into());
            s.values.insert(format!("heroes.126.bindings.{alt}.cast_mode"),"1".into());
            assert_eq!(s.binding(126,&slot).cast_mode,0);
            assert_eq!(s.binding(126,&alt).cast_mode,0);
            assert_eq!(s.binding(25,&slot).cast_mode,1);
            assert!(s.set(&format!("hero.bindings.{slot}.cast_mode"),"1",126).is_err());
            assert!(s.set(&format!("common.bindings.{alt}.cast_mode"),"1",126).is_err());
            s.set(&format!("hero.bindings.{alt}.stop_movement"),"true",126).unwrap();
            assert!(s.binding(126,&slot).stop_movement);
            assert!(s.binding(126,&alt).stop_movement);
        }
        s.set("hero.bindings.extra:2.cast_mode","1",126).unwrap();
        assert_eq!(s.binding(126,"extra:2").cast_mode,1);
    }
    #[test]
    fn khufra_first_skill_is_one_release_only_charge_without_restricting_other_skills() {
        let mut s=Settings::default();
        s.values.insert("common.bindings.skill:0.cast_mode".into(),"1".into());
        s.values.insert("heroes.78.bindings.skill:0:7871.cast_mode".into(),"1".into());
        for slot in ["skill:0","skill:0:7811","skill:0:7871"] {
            assert_eq!(s.binding(78,slot).cast_mode,0);
            assert!(s.set(&format!("hero.bindings.{slot}.cast_mode"),"1",78).is_err());
            assert!(s.set(&format!("common.bindings.{slot}.cast_mode"),"1",78).is_err());
        }
        s.set("hero.bindings.skill:0:7871.stop_movement","true",78).unwrap();
        assert!(s.binding(78,"skill:0").stop_movement);
        s.set("hero.bindings.skill:1.cast_mode","1",78).unwrap();
        assert_eq!(s.binding(78,"skill:1").cast_mode,1);
        assert_eq!(s.binding(25,"skill:0").cast_mode,1);
    }
    #[test]
    fn extended_keyboard_identities_are_saved_without_aliasing_mouse_or_function_keys() {
        let mut s=Settings::default();
        for key in [0x0100003c,0x01000052,0x1008ff12,0x1000ffe1,0x200000fa,0x0416] {
            assert!(valid_key(key));
            s.set("hero.bindings.skill:0.key",&key.to_string(),25).unwrap();
            assert_eq!(s.integer("hero.bindings.skill:0.key",25),key);
            s.set("debug.mark_shortcut",&(key|0x04000000).to_string(),25).unwrap();
        }
        for key in [0x10000000,0x20000000,0x20010000,0x300000fa,-1,0xd800] {assert!(!valid_key(key),"{key:x}");}
    }
    #[test]
    fn all_os_mouse_buttons_roundtrip_and_non_buttons_are_rejected() {
        for raw in 2..=255 {
            let key=match raw {2=>MOUSE|4,3=>MOUSE|2,8..=29=>MOUSE|(1<<(raw-5)),_=>0x03000000|raw};
            assert!(valid_key(key),"raw mouse {raw}");
            assert_eq!(MOUSE|(key&!MOUSE),key);
        }
        for invalid in [MOUSE|1,MOUSE|3,MOUSE|6,0x03000001,0x03000002,0x03010000] {assert!(!valid_key(invalid),"{invalid:x}");}
    }
}

#[cfg(test)]
mod movement_inheritance_tests {
    use super::*;
    #[test]
    fn movement_key_mode_distance_and_threshold_override_independently() {
        let mut s=Settings::default();
        s.set("hero.movement.button",&(MOUSE|32).to_string(),73).unwrap();
        s.set("movement.click_mode","1",0).unwrap();
        s.set("movement.click_distance_percent","200",0).unwrap();
        assert_eq!(s.integer("hero.movement.button",73),MOUSE|32);
        assert_eq!(s.integer("hero.movement.button",25),MOUSE|2);
        assert_eq!(s.integer("hero.movement.click_mode",73),1);
        assert_eq!(s.integer("hero.movement.click_distance_percent",73),200);
        s.set("hero.movement.click_mode","0",73).unwrap();
        s.set("hero.movement.hold_ms","300",73).unwrap();
        s.set("movement.hold_ms","100",0).unwrap();
        assert_eq!(s.integer("hero.movement.click_mode",73),0);
        assert_eq!(s.integer("hero.movement.hold_ms",73),300);
        assert_eq!(s.integer("hero.movement.hold_ms",25),100);
        s.set("hero.movement.button.override","false",73).unwrap();
        assert_eq!(s.integer("hero.movement.button",73),MOUSE|2);
        assert_eq!(s.integer("hero.movement.hold_ms",73),300);
    }
    #[test]
    fn common_second_skill_key_applies_to_untouched_umbrella_form() {
        let mut s=Settings::default();
        s.set("common.bindings.skill:1.key","88",0).unwrap();
        s.set("common.bindings.skill:1.cast_mode","0",0).unwrap();
        assert_eq!(s.binding(25,"skill:1:2521").key,88);
        assert_eq!(s.binding(25,"skill:1:2521").cast_mode,0);
        s.set("hero.bindings.skill:1:2521.key","87",25).unwrap();
        s.set("common.bindings.skill:1.key","89",0).unwrap();
        assert_eq!(s.binding(25,"skill:1:2521").key,87);
        assert_eq!(s.binding(73,"skill:1").key,89);
    }
}
