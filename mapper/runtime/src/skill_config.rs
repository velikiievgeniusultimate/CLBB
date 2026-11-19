// SPDX-License-Identifier: GPL-3.0-or-later
//! Typed access to loaded game-owned configuration dictionaries.
use crate::{
    platform::Memory,
    reflection::{Reflection, Result, i32_at, u64_at},
    source::{Checks, capture},
};
use std::collections::{BTreeMap, BTreeSet};

pub fn typed_field<M: Memory>(
    r: &mut Reflection<M>,
    checks: &mut Checks,
    object: u64,
    name: &str,
    kinds: &[u8],
) -> Result<(u64, u8)> {
    if object == 0 {
        return Err(format!("Missing owner for{name}"));
    }
    let mut class = u64_at(&capture(r, checks, object, 8)?, 0);
    for _ in 0..20 {
        if class == 0 {
            break;
        }
        let metadata = r.class(class)?;
        if let Some(field) = metadata.fields.get(name) {
            if field.is_static || !kinds.contains(&field.kind) || field.offset > 1_048_576 {
                return Err(format!("Field type/layout changed:{name}"));
            }
            return Ok((
                object
                    .checked_add(field.offset)
                    .ok_or("Field address overflow")?,
                field.kind,
            ));
        }
        class = metadata.parent;
    }
    Err(format!("Missing typed field:{name}"))
}
pub fn pointer<M: Memory>(
    r: &mut Reflection<M>,
    checks: &mut Checks,
    object: u64,
    name: &str,
) -> Result<u64> {
    let (at, _) = typed_field(r, checks, object, name, &[18, 21, 28, 29])?;
    Ok(u64_at(&capture(r, checks, at, 8)?, 0))
}
pub fn integer<M: Memory>(
    r: &mut Reflection<M>,
    checks: &mut Checks,
    object: u64,
    name: &str,
) -> Result<i32> {
    let (at, _) = typed_field(r, checks, object, name, &[8])?;
    Ok(i32_at(&capture(r, checks, at, 4)?, 0))
}
pub fn owner<M: Memory>(
    r: &mut Reflection<M>,
    checks: &mut Checks,
    object: u64,
    expected: &str,
) -> Result<u64> {
    if object == 0 {
        return Err(format!("Missing {expected} owner"));
    }
    let class = u64_at(&capture(r, checks, object, 8)?, 0);
    if r.class(class)?.name != expected {
        return Err(format!("Expected exact {expected} owner"));
    }
    Ok(class)
}
pub fn table<M: Memory>(
    r: &mut Reflection<M>,
    checks: &mut Checks,
    name: &str,
) -> Result<(u64, u64, u64)> {
    let class = *r
        .named
        .get(name)
        .ok_or("Configuration table class absent")?;
    let metadata = r.class(class)?;
    let field = metadata
        .fields
        .get("m_Instance")
        .ok_or("Configuration singleton field absent")?;
    if metadata.name != name
        || !field.is_static
        || ![18, 21].contains(&field.kind)
        || metadata.statics == 0
        || field.offset > 1_048_576
    {
        return Err("Configuration singleton type changed".into());
    }
    let instance = u64_at(&capture(r, checks, metadata.statics + field.offset, 8)?, 0);
    owner(r, checks, instance, name)?;
    let shared = pointer(r, checks, instance, "m_Data")?;
    owner(r, checks, shared, "SharedDictionary`2")?;
    let dictionary = pointer(r, checks, shared, "m_dic")?;
    Ok((instance, shared, dictionary))
}
pub fn entry_layout(
    metadata: &crate::reflection::Class,
) -> Result<(BTreeMap<String, usize>, usize)> {
    let fields = &metadata.fields;
    for (name, kinds) in [
        ("hashCode", &[8][..]),
        ("next", &[8][..]),
        ("key", &[8][..]),
        ("value", &[18, 21, 28][..]),
    ] {
        let f = fields.get(name).ok_or("Dictionary entry field absent")?;
        if f.is_static || !kinds.contains(&f.kind) {
            return Err(format!("Dictionary entry type changed:{name}"));
        }
    }
    if metadata.name != "Entry" || fields.len() != 4 {
        return Err("Dictionary entry shape changed".into());
    }
    let bias = fields
        .values()
        .map(|f| f.offset)
        .min()
        .ok_or("Empty dictionary entry")?;
    let offsets: BTreeMap<_, _> = fields
        .iter()
        .map(|(name, f)| (name.clone(), (f.offset - bias) as usize))
        .collect();
    let stride = offsets
        .iter()
        .map(|(name, offset)| offset + if name == "value" { 8 } else { 4 })
        .max()
        .unwrap();
    if !(16..=64).contains(&stride) || stride % 8 != 0 || offsets["hashCode"] != 0 {
        return Err("Dictionary entry stride changed".into());
    }
    let mut used = BTreeSet::new();
    for (name, offset) in &offsets {
        for at in *offset..offset + if name == "value" { 8 } else { 4 } {
            if !used.insert(at) {
                return Err("Dictionary entry fields overlap".into());
            }
        }
    }
    Ok((offsets, stride))
}
pub fn entries<M: Memory>(
    r: &mut Reflection<M>,
    checks: &mut Checks,
    dictionary: u64,
    limit: usize,
) -> Result<Vec<(i32, u64)>> {
    owner(r, checks, dictionary, "Dictionary`2")?;
    let count = integer(r, checks, dictionary, "count")?;
    let _version = integer(r, checks, dictionary, "version")?;
    if count < 0 || count as usize > limit {
        return Err("Dictionary count exceeds audit bound".into());
    }
    let array = pointer(r, checks, dictionary, "entries")?;
    if array == 0 {
        return if count == 0 {
            Ok(Vec::new())
        } else {
            Err("Dictionary entries unavailable".into())
        };
    }
    let header = capture(r, checks, array, 32)?;
    let class = u64_at(&header, 0);
    let capacity = u64_at(&header, 24);
    if capacity < count as u64 || capacity > 32768 {
        return Err("Dictionary capacity exceeds audit bound".into());
    }
    let element = u64_at(&capture(r, checks, class + 0x40, 8)?, 0);
    let (offsets, stride) = entry_layout(&r.class(element)?)?;
    let raw = capture(r, checks, array + 32, count as usize * stride)?;
    let mut keys = BTreeSet::new();
    let mut out = Vec::new();
    for row in raw.chunks_exact(stride) {
        if i32_at(row, offsets["hashCode"]) < 0 {
            continue;
        }
        let key = i32_at(row, offsets["key"]);
        let value = u64_at(row, offsets["value"]);
        if !keys.insert(key) {
            return Err("Duplicate live dictionary key".into());
        }
        if value == 0 {
            return Err("Live dictionary value unavailable".into());
        }
        out.push((key, value));
    }
    Ok(out)
}
