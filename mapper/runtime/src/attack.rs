// SPDX-License-Identifier: GPL-3.0-or-later
//! Current per-profile attack selection flag. Legacy SystemSetting snapshots
//! and static backing flags are deliberately not input authorization.
use crate::{
    platform::Memory,
    reflection::{Reflection, Result, i32_at, u64_at},
    source::{Checks, capture, check},
};
#[derive(Clone)]
pub(crate) struct ModePlan {
    checks: Checks,
    value_link: u64,
    profile_address: u64,
    profile: i32,
}
fn field<M: Memory>(r: &mut Reflection<M>, object: u64, name: &str, kind: u8) -> Result<u64> {
    let mut c = r.class(r.pointer(object)?)?;
    for _ in 0..16 {
        if let Some(f) = c.fields.get(name) {
            if f.is_static || f.kind != kind || f.offset > 65536 {
                return Err(format!("Attack setting field changed:{name}"));
            }
            return Ok(object + f.offset);
        }
        if c.parent == 0 {
            break;
        }
        c = r.class(c.parent)?;
    }
    Err(format!("Attack setting field absent:{name}"))
}
fn link<M: Memory>(
    r: &mut Reflection<M>,
    checks: &mut Checks,
    object: u64,
    name: &str,
    kind: u8,
) -> Result<u64> {
    let at = field(r, object, name, kind)?;
    let raw = capture(r, checks, at, 8)?;
    let p = u64_at(&raw, 0);
    if p == 0 {
        return Err("Attack setting owner absent".into());
    }
    Ok(p)
}
fn exact<M: Memory>(
    r: &mut Reflection<M>,
    checks: &mut Checks,
    object: u64,
    name: &str,
) -> Result<()> {
    let raw = capture(r, checks, object, 8)?;
    if r.class(u64_at(&raw, 0))?.name != name {
        return Err(format!("Expected attack setting owner:{name}"));
    }
    Ok(())
}
fn string<M: Memory>(r: &Reflection<M>, checks: &mut Checks, p: u64) -> Result<String> {
    let n = i32_at(&capture(r, checks, p + 16, 4)?, 0);
    if !(0..=128).contains(&n) {
        return Err("Attack setting key too long".into());
    }
    let b = capture(r, checks, p + 20, n as usize * 2)?;
    String::from_utf16(
        &b.chunks_exact(2)
            .map(|v| u16::from_le_bytes([v[0], v[1]]))
            .collect::<Vec<_>>(),
    )
    .map_err(|_| "Attack setting key UTF16".into())
}
fn lookup<M: Memory>(
    r: &mut Reflection<M>,
    checks: &mut Checks,
    dictionary: u64,
    int_key: Option<i32>,
    text_key: Option<&str>,
) -> Result<(u64, u64)> {
    exact(r, checks, dictionary, "Dictionary`2")?;
    let count_at = field(r, dictionary, "count", 8)?;
    let count = i32_at(&capture(r, checks, count_at, 4)?, 0);
    if !(1..=1024).contains(&count) {
        return Err("Attack settings dictionary bound".into());
    }
    let array = link(r, checks, dictionary, "entries", 29)?;
    let h = capture(r, checks, array, 32)?;
    let ac = u64_at(&h, 0);
    let capacity = u64_at(&h, 24);
    if capacity < count as u64 || capacity > 4096 {
        return Err("Attack settings array capacity".into());
    }
    let e = r.pointer(ac + 0x40)?;
    checks.push((ac + 0x40, e.to_le_bytes().to_vec()));
    let metadata = r.class(e)?;
    if metadata.name != "Entry" || metadata.fields.len() != 4 {
        return Err("Attack setting dictionary Entry changed".into());
    }
    let f = &metadata.fields;
    let bias = f.values().map(|v| v.offset).min().ok_or("Entry")?;
    for (name, kind) in [
        ("hashCode", 8),
        ("next", 8),
        ("key", if int_key.is_some() { 8 } else { 14 }),
        ("value", if int_key.is_some() { 18 } else { 28 }),
    ] {
        let x = f.get(name).ok_or("Entry field")?;
        if x.is_static || x.kind != kind {
            return Err("Attack setting entry type".into());
        }
    }
    let stride = f
        .values()
        .map(|v| v.offset - bias + if matches!(v.kind, 8) { 4 } else { 8 })
        .max()
        .unwrap();
    if !(16..=64).contains(&stride) || stride % 8 != 0 {
        return Err("Attack settings stride".into());
    }
    let bytes = r.read(array + 32, count as usize * stride as usize)?;
    let mut found = None;
    for (index, row) in bytes.chunks_exact(stride as usize).enumerate() {
        if i32_at(row, (f["hashCode"].offset - bias) as usize) < 0 {
            continue;
        }
        let matches = if let Some(key) = int_key {
            i32_at(row, (f["key"].offset - bias) as usize) == key
        } else {
            string(r, checks, u64_at(row, (f["key"].offset - bias) as usize))?
                == text_key.unwrap_or("")
        };
        if matches {
            if found.is_some() {
                return Err("Ambiguous attack setting".into());
            }
            let at = array + 32 + index as u64 * stride;
            checks.push((at, row.to_vec()));
            let value_at = at + f["value"].offset - bias;
            found = Some((value_at, u64_at(row, (f["value"].offset - bias) as usize)));
        }
    }
    found.ok_or("Current attack setting absent".into())
}
impl ModePlan {
    pub(crate) fn discover<M: Memory>(r: &mut Reflection<M>, hero: i32) -> Result<Self> {
        r.discover(&["Mgr"])?;
        let at = r.static_field("Mgr", "m_Instatnce")?;
        let mut checks = Vec::new();
        let p = u64_at(&capture(r, &mut checks, at, 8)?, 0);
        exact(r, &mut checks, p, "Mgr")?;
        let profile_address = field(r, p, "_curOperateSettingHeroId", 8)?;
        let profile = i32_at(&capture(r, &mut checks, profile_address, 4)?, 0);
        if profile > 0 && profile != hero {
            return Err("Attack profile belongs to another hero".into());
        }
        let repo = link(r, &mut checks, p, "_operateSettingsRepos", 18)?;
        exact(r, &mut checks, repo, "StorageRepository")?;
        let storages = link(r, &mut checks, repo, "_storages", 21)?;
        let (_, storage) = lookup(r, &mut checks, storages, Some(profile.max(0)), None)?;
        exact(r, &mut checks, storage, "Storage")?;
        let elements = link(r, &mut checks, storage, "_elements", 21)?;
        let (value_link, value) = lookup(r, &mut checks, elements, None, Some("m_bATKPosSelect"))?;
        exact(r, &mut checks, value, "Boolean")?;
        field(r, value, "m_value", 2)?;
        check(r, &checks)?;
        Ok(Self {
            checks,
            value_link,
            profile_address,
            profile,
        })
    }
    pub(crate) fn sample<M: Memory>(
        &self,
        r: &mut Reflection<M>,
        consumed: &mut Checks,
    ) -> Result<bool> {
        check(r, &self.checks)?;
        consumed.extend(self.checks.iter().cloned());
        let p = i32_at(&capture(r, consumed, self.profile_address, 4)?, 0);
        if p != self.profile {
            return Err("Attack profile changed".into());
        }
        let value = u64_at(&capture(r, consumed, self.value_link, 8)?, 0);
        exact(r, consumed, value, "Boolean")?;
        let at = field(r, value, "m_value", 2)?;
        let b = capture(r, consumed, at, 1)?[0];
        match b {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err("Attack mode is not Boolean".into()),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    struct Mem(RefCell<Vec<u8>>);
    impl Memory for Mem {
        fn regions(&self) -> Result<Vec<crate::platform::Region>> {
            Ok(vec![crate::platform::Region {
                start: 0,
                end: 8192,
                readable: true,
                writable: true,
                name: "attack-test".into(),
            }])
        }
        fn read_into(&self, address: u64, bytes: &mut [u8]) -> Result<()> {
            let b = self.0.borrow();
            let s = b
                .get(address as usize..address as usize + bytes.len())
                .ok_or("range")?;
            bytes.copy_from_slice(s);
            Ok(())
        }
        fn read(&self, address: u64, length: usize) -> Result<Vec<u8>> {
            self.0
                .borrow()
                .get(address as usize..address as usize + length)
                .map(|v| v.to_vec())
                .ok_or("range".into())
        }
    }
    fn fixture() -> (Reflection<Mem>, ModePlan) {
        let mut b = vec![0; 8192];
        let mut put = |at: usize, v: u64| b[at..at + 8].copy_from_slice(&v.to_le_bytes());
        put(0x100, 0x200);
        put(0x200, 0x400);
        put(0x440, 0x400);
        put(0x448, 0x400);
        put(0x410, 0x700);
        put(0x478, 0x600);
        put(0x600, 0x720);
        put(0x608, 0x680);
        put(0x610, 0x400);
        b[0x628..0x62c].copy_from_slice(&16u32.to_le_bytes()); // field offset is row+24
        b[0x618..0x61c].copy_from_slice(&16u32.to_le_bytes());
        b[0x688..0x68c].copy_from_slice(&(2u32 << 16).to_le_bytes());
        b[0x504..0x506].copy_from_slice(&1u16.to_le_bytes());
        b[0x700..0x708].copy_from_slice(b"Boolean\0");
        b[0x720..0x728].copy_from_slice(b"m_value\0");
        b[0x120..0x124].copy_from_slice(&(-84i32).to_le_bytes());
        let checks = vec![
            (0x100, 0x200u64.to_le_bytes().to_vec()),
            (0x120, (-84i32).to_le_bytes().to_vec()),
        ];
        (
            Reflection::new(Mem(RefCell::new(b))).unwrap(),
            ModePlan {
                checks,
                value_link: 0x100,
                profile_address: 0x120,
                profile: -84,
            },
        )
    }
    #[test]
    fn only_a_current_typed_positive_flag_authorizes_aim_and_reads_are_fenced() {
        let (mut r, p) = fixture();
        assert!(!p.sample(&mut r, &mut Vec::new()).unwrap());
        r.memory.0.borrow_mut()[0x210] = 1;
        let mut consumed = Vec::new();
        assert!(p.sample(&mut r, &mut consumed).unwrap());
        r.memory.0.borrow_mut()[0x210] = 0;
        assert!(check(&r, &consumed).is_err());
        r.memory.0.borrow_mut()[0x210] = 2;
        assert!(p.sample(&mut r, &mut Vec::new()).is_err());
    }
    #[test]
    fn setting_owner_replacement_or_foreign_profile_revokes_old_admission() {
        let (mut r, p) = fixture();
        r.memory.0.borrow_mut()[0x100..0x108].copy_from_slice(&0x280u64.to_le_bytes());
        assert!(p.sample(&mut r, &mut Vec::new()).is_err());
        let (mut r, p) = fixture();
        r.memory.0.borrow_mut()[0x120..0x124].copy_from_slice(&18i32.to_le_bytes());
        assert!(p.sample(&mut r, &mut Vec::new()).is_err());
    }
}
