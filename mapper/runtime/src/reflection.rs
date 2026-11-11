//! Read-only IL2CPP metadata discovery. Refuse unknown layouts and ambiguous classes.
use crate::platform::{Memory, Region};
use std::collections::{HashMap, HashSet};
pub type Result<T> = std::result::Result<T, String>;
pub fn u64_at(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}
pub fn i32_at(b: &[u8], at: usize) -> i32 {
    i32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
pub fn f32_at(b: &[u8], at: usize) -> f64 {
    f32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as f64
}
#[derive(Clone, Debug)]
pub struct Field {
    pub offset: u64,
    pub is_static: bool,
    pub kind: u8,
}
#[derive(Clone, Debug)]
pub struct Class {
    pub address: u64,
    pub name: String,
    pub parent: u64,
    pub statics: u64,
    pub fields: HashMap<String, Field>,
}
pub struct Reflection<M: Memory> {
    pub memory: M,
    pub maps: Vec<Region>,
    pub named: HashMap<String, u64>,
    classes: HashMap<u64, Class>,
}
impl<M: Memory> Reflection<M> {
    pub fn new(memory: M) -> Result<Self> {
        let maps = memory.regions()?;
        Ok(Self {
            memory,
            maps,
            named: HashMap::new(),
            classes: HashMap::new(),
        })
    }
    pub fn read(&self, address: u64, length: usize) -> Result<Vec<u8>> {
        self.memory.read(address, length)
    }
    pub fn pointer(&self, address: u64) -> Result<u64> {
        Ok(u64_at(&self.read(address, 8)?, 0))
    }
    pub fn integer(&self, address: u64) -> Result<i32> {
        Ok(i32_at(&self.read(address, 4)?, 0))
    }
    pub fn float(&self, address: u64) -> Result<f64> {
        Ok(f32_at(&self.read(address, 4)?, 0))
    }
    pub fn boolean(&self, address: u64) -> Result<bool> {
        Ok(self.read(address, 1)?[0] != 0)
    }
    pub fn cstring(&self, address: u64) -> Result<String> {
        let region = self
            .maps
            .iter()
            .find(|m| m.readable && m.start <= address && address < m.end)
            .ok_or("String outside metadata mappings")?;
        let raw = self.read(address, 192.min((region.end - address) as usize))?;
        let end = raw
            .iter()
            .position(|b| *b == 0)
            .ok_or("Unterminated metadata string")?;
        String::from_utf8(raw[..end].to_vec()).map_err(|_| "Invalid metadata text".into())
    }
    pub fn class(&mut self, address: u64) -> Result<Class> {
        if let Some(value) = self.classes.get(&address) {
            return Ok(value.clone());
        }
        let header = self.read(address, 0x118)?;
        if u64_at(&header, 0x40) != address || u64_at(&header, 0x48) != address {
            return Err("IL2CPP class identity mismatch".into());
        }
        let name = self.cstring(u64_at(&header, 16))?;
        let count = u16::from_le_bytes(header[0x104..0x106].try_into().unwrap()) as usize;
        if count > 1024 {
            return Err("Invalid reflected field count".into());
        }
        let table = self.read(u64_at(&header, 0x78), count * 32)?;
        let mut fields = HashMap::new();
        for row in table.chunks_exact(32) {
            if u64_at(row, 16) != address {
                return Err("Reflected field owner mismatch".into());
            }
            let field_name = self.cstring(u64_at(row, 0))?;
            let ty = self.read(u64_at(row, 8), 16)?;
            let attrs = u32::from_le_bytes(ty[8..12].try_into().unwrap());
            fields.insert(
                field_name,
                Field {
                    offset: u32::from_le_bytes(row[24..28].try_into().unwrap()) as u64,
                    is_static: attrs & 16 != 0,
                    kind: ((attrs >> 16) & 255) as u8,
                },
            );
        }
        let value = Class {
            address,
            name,
            parent: u64_at(&header, 0x58),
            statics: u64_at(&header, 0xa8),
            fields,
        };
        self.classes.insert(address, value.clone());
        Ok(value)
    }
    pub fn field(&mut self, object: u64, name: &str) -> Result<u64> {
        let mut address = self.pointer(object)?;
        for _ in 0..24 {
            let class = self.class(address)?;
            if let Some(field) = class.fields.get(name) {
                if field.offset > 1048576 {
                    return Err("Unsupported field offset".into());
                }
                return Ok((if field.is_static {
                    class.statics
                } else {
                    object
                }) + field.offset);
            }
            if class.parent == 0 {
                break;
            }
            address = class.parent;
        }
        Err(format!("Missing reflected field {name}"))
    }
    pub fn link(&mut self, object: u64, name: &str) -> Result<u64> {
        let a = self.field(object, name)?;
        self.pointer(a)
    }
    pub fn static_field(&mut self, name: &str, field: &str) -> Result<u64> {
        let address = *self
            .named
            .get(name)
            .ok_or_else(|| format!("Missing class {name}"))?;
        let class = self.class(address)?;
        let f = class
            .fields
            .get(field)
            .ok_or_else(|| format!("Missing static field {name}.{field}"))?;
        if !f.is_static || class.statics == 0 {
            return Err("Static field owner unavailable".into());
        }
        Ok(class.statics + f.offset)
    }
    pub fn instance(&mut self, name: &str) -> Result<u64> {
        let a = self.static_field(name, "Instance")?;
        self.pointer(a)
    }
    pub fn discover(&mut self, names: &[&str]) -> Result<()> {
        let missing: HashSet<String> = names
            .iter()
            .filter(|name| !self.named.contains_key(**name))
            .map(|v| (*v).into())
            .collect();
        if missing.is_empty() {
            return Ok(());
        }
        self.maps = self.memory.regions()?;
        let mapping = self
            .maps
            .iter()
            .find(|m| m.name.ends_with("/global-csharp-metadata.dat"))
            .ok_or("Waiting for game metadata")?
            .clone();
        let header = self.read(mapping.start, 252)?;
        if i32_at(&header, 0) as u32 != 0xfab11baf
            || i32_at(&header, 4) != 1024
            || i32_at(&header, 8) != 3
        {
            return Err("Unsupported game metadata version".into());
        }
        let strings_offset = i32_at(&header, 28) as u32 as u64;
        let strings_size = i32_at(&header, 32) as u32 as u64;
        let types_offset = i32_at(&header, 164) as u32 as u64;
        let types_size = i32_at(&header, 168) as u32 as u64;
        if types_size % 92 != 0
            || strings_offset + strings_size > mapping.end - mapping.start
            || types_offset + types_size > mapping.end - mapping.start
        {
            return Err("Metadata table bounds changed".into());
        }
        let blob = |r: &Self, at: u64, length: u64| -> Result<Vec<u8>> {
            let mut out = Vec::with_capacity(length as usize);
            for offset in (0..length).step_by(1048576) {
                out.extend(r.read(at + offset, (length - offset).min(1048576) as usize)?);
            }
            Ok(out)
        };
        let strings = blob(self, mapping.start + strings_offset, strings_size)?;
        let types = blob(self, mapping.start + types_offset, types_size)?;
        let mut needles = HashMap::new();
        for row in types.chunks_exact(92) {
            let index = i32_at(row, 0) as u32;
            if index >> 24 != 3 {
                continue;
            }
            let index = (index & 0xffffff) as usize;
            if index >= strings.len() {
                continue;
            }
            let end = strings[index..].iter().position(|b| *b == 0).unwrap_or(0) + index;
            if let Ok(name) = std::str::from_utf8(&strings[index..end]) {
                if missing.contains(name) {
                    needles.insert(
                        mapping.start + strings_offset + index as u64,
                        name.to_owned(),
                    );
                }
            }
        }
        if needles.len() != missing.len() {
            return Err("Required class absent from game metadata".into());
        }
        // Guest classes occupy readable writable Android allocations. The host
        // adapter supplies mappings; no allocator name or Waydroid path is required.
        let regions: Vec<_> = self
            .maps
            .iter()
            .filter(|m| {
                m.readable
                    && m.writable
                    && m.start < 1 << 40
                    && m.end - m.start < 512 * 1024 * 1024
                    && (m.name.is_empty() || m.name.starts_with("[anon:") || m.name == "[heap]")
            })
            .cloned()
            .collect();
        let mut hits: HashMap<String, HashSet<u64>> = HashMap::new();
        for region in regions {
            for offset in (0..region.end - region.start).step_by(1048576) {
                let start = region.start + offset;
                let size = (region.end - start).min(1048576) as usize;
                let Ok(bytes) = self.read(start, size) else {
                    continue;
                };
                for at in (0..bytes.len().saturating_sub(7)).step_by(8) {
                    let value = u64_at(&bytes, at);
                    if value < mapping.start + strings_offset
                        || value >= mapping.start + strings_offset + strings_size
                    {
                        continue;
                    }
                    if let Some(name) = needles.get(&value) {
                        if start + (at as u64) < 16 {
                            continue;
                        }
                        let candidate = start + at as u64 - 16;
                        if self.class(candidate).is_ok_and(|c| c.name == *name) {
                            hits.entry(name.clone()).or_default().insert(candidate);
                        }
                    }
                }
            }
        }
        for name in missing {
            let values = hits
                .get(&name)
                .ok_or_else(|| format!("Waiting for runtime class {name}"))?;
            if values.len() != 1 {
                return Err(format!("Ambiguous runtime class {name}"));
            }
            self.named.insert(name, *values.iter().next().unwrap());
        }
        Ok(())
    }
    pub fn list(&self, object: u64) -> Result<Vec<u64>> {
        let header = self.read(object + 16, 16)?;
        let array = u64_at(&header, 0);
        let count = i32_at(&header, 8);
        if !(0..=256).contains(&count) || array == 0 {
            return Err("Invalid live object list".into());
        }
        let raw = self.read(array + 32, count as usize * 8)?;
        Ok(raw.chunks_exact(8).map(|b| u64_at(b, 0)).collect())
    }
}
