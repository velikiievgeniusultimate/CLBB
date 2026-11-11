//! CLBB-owned settings. JSON is used only for persistence and menu edits.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};
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
        s
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
        if let Some(tail) = key.strip_prefix("hero.bindings.") {
            if let Some((slot, field)) = tail.rsplit_once('.') {
                if !(slot.starts_with("skill:") || slot.starts_with("combo:"))
                    && field != "override"
                    && !self.boolean(&format!("hero.bindings.{slot}.override"), hero)
                {
                    return self.get(&format!("common.bindings.{slot}.{field}"), hero);
                }
            }
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
            "movement.click_distance_percent" => "100".into(),
            "movement.hold_ms" => "150".into(),
            "movement.button" => (MOUSE | 2).to_string(),
            "audio.input_device" | "audio.output_device" => "default".into(),
            "audio.volume" => "100".into(),
            "audio.muted" => "false".into(),
            "hotkeys.settings" => F7.to_string(),
            "hotkeys.confine" => F12.to_string(),
            "hotkeys.fullscreen" => F11.to_string(),
            "hero.cancel_key" => "84".into(),
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
        if scope == "hero" && !(slot.starts_with("skill:") || slot.starts_with("combo:")) {
            return self.get(&format!("common.bindings.{slot}.{field}"), hero);
        }
        if field == "stop_movement" {
            return "false".into();
        }
        if field == "cast_mode" {
            return if slot == "skill:1:2521" || slot == "skill:2" || tap_only(slot) {
                "1"
            } else {
                "0"
            }
            .into();
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
            "extra:3" => 0x01000007,
            "shop:quick" => 90,
            "upgrade:0" => 49,
            "upgrade:1" => 50,
            "upgrade:2" => 51,
            "upgrade:3" => 52,
            _ => 0,
        };
        code.to_string()
    }
    pub fn integer(&self, key: &str, hero: i32) -> i32 {
        self.get(key, hero).parse().unwrap_or(0)
    }
    pub fn boolean(&self, key: &str, hero: i32) -> bool {
        matches!(self.get(key, hero).as_str(), "true" | "1")
    }
    pub fn binding(&self, hero: i32, slot: &str) -> clbb_moba_core::controller::host::Binding {
        let personal = slot.starts_with("skill:") || slot.starts_with("combo:")
            || self.boolean(&format!("hero.bindings.{slot}.override"), hero);
        let prefix = if personal { "hero" } else { "common" };
        let key = |f| format!("{prefix}.bindings.{slot}.{f}");
        let code = self.integer(&key("key"), hero);
        let code = if valid_key(code) && !self.reserved(code, hero) {
            code
        } else {
            0
        };
        clbb_moba_core::controller::host::Binding {
            key: code,
            cast_mode: if tap_only(slot) {
                1
            } else {
                self.integer(&key("cast_mode"), hero)
            },
            aim_type: 0,
            stop_movement: self.boolean(&key("stop_movement"), hero),
        }
    }
    pub fn set(&mut self, key: &str, value: &str, hero: i32) -> Result<(), String> {
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
            "display.resolution" => parse_resolution(value).is_some(),
            "display.fps_limit" => integer.is_some_and(|fps| (30..=240).contains(&fps)),
            "display.vsync" => matches!(value, "true" | "false" | "0" | "1"),
            "enabled" | "audio.muted" | "debug.recording" | "debug.auto_marks" => matches!(value, "true" | "false" | "0" | "1"),
            "audio.input_device" | "audio.output_device" => !value.is_empty() && value.len() <= 255 && value.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b)),
            "audio.volume" => integer.is_some_and(|v| (0..=100).contains(&v)),
            "movement.click_mode" => matches!(value, "0" | "1"),
            "movement.click_distance_percent" => integer.is_some_and(|v| (0..=500).contains(&v)),
            "movement.hold_ms" => integer.is_some_and(|v| (40..=600).contains(&v)),
            "hotkeys.settings" | "hotkeys.confine" | "hotkeys.fullscreen" => {
                integer.is_some_and(|v| {
                    valid_key(v)
                        && v != 0
                        && !["hotkeys.settings", "hotkeys.confine", "hotkeys.fullscreen", "debug.mark_shortcut"]
                            .into_iter()
                            .any(|k| k != key && self.integer(k, hero) == v)
                })
            }
            "debug.mark_shortcut" => integer.is_some_and(|v| {
                let base=v & 0x01ffffff;
                v==0 || (v>0 && v & !0x0fffffff==0 && valid_key(base) && base!=0
                    && (v & 0x0e000000!=0 || !["hotkeys.settings","hotkeys.confine","hotkeys.fullscreen"].iter().any(|key|self.integer(key,hero)==v)))
            }),
            "hero.cancel_key" | "movement.button" => {
                integer.is_some_and(|v| valid_key(v) && !self.reserved(v, hero))
            }
            _ if is_binding && key.ends_with(".key") => {
                integer.is_some_and(|v| valid_key(v) && !self.reserved(v, hero))
            }
            _ if is_binding && key.ends_with(".bindings.extra:2.cast_mode") => {
                matches!(value, "0" | "1" | "2" | "3")
            }
            _ if is_binding && key.ends_with(".cast_mode") => matches!(value, "0" | "1"),
            _ if is_binding && (key.ends_with(".override") || key.ends_with(".stop_movement")) => {
                matches!(value, "true" | "false" | "0" | "1")
            }
            _ => false,
        };
        if !valid {
            return Err("Недопустимое значение или занятая системная кнопка".into());
        }
        let name = Self::canonical(key, hero);
        let old = self.values.insert(name.clone(), value.into());
        if let Err(e) = self.save() {
            if let Some(old) = old {
                self.values.insert(name, old);
            } else {
                self.values.remove(&name);
            }
            return Err(e);
        }
        Ok(())
    }
    fn reserved(&self, key: i32, hero: i32) -> bool {
        key != 0
            && ["hotkeys.settings", "hotkeys.confine", "hotkeys.fullscreen", "debug.mark_shortcut"]
                .into_iter()
                .any(|k| self.integer(k, hero) == key)
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
pub fn valid_key(key: i32) -> bool {
    key == 0
        || ((32..=126).contains(&key))
        || ((0x01000000..=0x010000ff).contains(&key)
            && ![0x01000020, 0x01000021, 0x01000022, 0x01000023].contains(&key))
        || ((MOUSE | 2)..=(MOUSE | 31)).contains(&key)
}
pub fn tap_only(slot: &str) -> bool {
    slot.starts_with("attack")
        || slot.starts_with("upgrade:")
        || slot == "shop:quick"
        || ["extra:0", "extra:1"].contains(&slot)
}
#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(s.binding(25, "skill:1:2521").cast_mode, 1);
    }
    #[test]
    fn global_override_inherits() {
        let mut s = Settings::default();
        s.set("common.bindings.extra:2.key", "88", 25).unwrap();
        assert_eq!(s.binding(25, "extra:2").key, 88);
        s.set("hero.bindings.extra:2.key", "70", 25).unwrap();
        assert_eq!(s.binding(25, "extra:2").key, 88);
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
