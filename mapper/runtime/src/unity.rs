//! Native Unity transforms and UI coordinates. Layout checks precede every use.
use crate::{
    math,
    platform::Memory,
    reflection::{Reflection, Result, f32_at, i32_at, u64_at},
};
#[derive(Clone, Debug)]
pub struct Path {
    pub checks: Vec<(u64, Vec<u8>)>,
    pub transforms: Vec<u64>,
}
impl Path {
    pub fn new<M: Memory>(r: &mut Reflection<M>, object: u64, component: bool) -> Result<Self> {
        fn add(p:u64,n:u64)->Result<u64>{p.checked_add(n).ok_or("Native transform address overflow".into())}
        if object == 0 {
            return Err("Native transform is absent".into());
        }
        let cached = r.field(object, "m_CachedPtr")?;
        let mut native = r.pointer(cached)?;
        if native == 0 || native > u64::MAX - 4096 {
            return Err("Native transform is destroyed or invalid".into());
        }
        let mut checks = vec![(cached, native.to_le_bytes().to_vec())];
        if component {
            let owner = add(native, 48)?;
            native = r.pointer(owner)?;
            if native == 0 || native > u64::MAX - 4096 {
                return Err("Native transform owner is destroyed or invalid".into());
            }
            checks.push((owner, native.to_le_bytes().to_vec()));
        }
        let components = r.pointer(add(native, 48)?)?;
        let transform = r.pointer(add(components, 8)?)?;
        if r.pointer(add(transform, 48)?)? != native {
            return Err("Transform has a different native owner".into());
        }
        let hierarchy = r.pointer(add(transform, 56)?)?;
        let mut index = r.integer(add(transform, 64)?)? as u32;
        let raw = r.read(add(hierarchy, 24)?, 16)?;
        let trs = u64_at(&raw, 0);
        let parents = u64_at(&raw, 8);
        for (address, value) in [
            (add(native, 48)?, components),
            (add(components, 8)?, transform),
            (add(transform, 48)?, native),
            (add(transform, 56)?, hierarchy),
        ] {
            checks.push((address, value.to_le_bytes().to_vec()));
        }
        checks.push((add(transform, 64)?, index.to_le_bytes().to_vec()));
        checks.push((add(hierarchy, 24)?, raw));
        let mut transforms = Vec::new();
        let mut seen = std::collections::HashSet::new();
        while index != u32::MAX {
            if index > 65536 || !seen.insert(index) || seen.len() > 128 {
                return Err("Invalid native transform hierarchy".into());
            }
            transforms.push(add(trs,index as u64 * 48)?);
            let parent = r.read(add(parents,index as u64 * 4)?, 4)?;
            checks.push((add(parents,index as u64 * 4)?, parent.clone()));
            index = i32_at(&parent, 0) as u32;
        }
        if transforms.is_empty() {
            return Err("Empty native transform hierarchy".into());
        }
        Ok(Self { checks, transforms })
    }
    pub fn validate<M: Memory>(&self, r: &Reflection<M>) -> Result<()> {
        if crate::platform::matches_reads(&r.memory, &self.checks)? {
            Ok(())
        } else {
            Err("Native transform was reparented or destroyed".into())
        }
    }
    pub fn bytes<M: Memory>(&self, r: &Reflection<M>) -> Result<Vec<u8>> {
        self.validate(r)?;
        let mut raw = Vec::with_capacity(self.transforms.len() * 48);
        for address in &self.transforms {
            raw.extend(r.read(*address, 48)?);
        }
        self.validate(r)?;
        Ok(raw)
    }
    pub fn position<M: Memory>(&self, r: &Reflection<M>) -> Result<[f64; 3]> {
        math::trs_chain(&self.bytes(r)?, None).ok_or("Invalid native transform values".into())
    }
}
pub fn matrices(raw: &[u8]) -> Result<([f64; 16], [f64; 16])> {
    if raw.len() != 128 {
        return Err("Invalid camera matrix block".into());
    }
    let view = std::array::from_fn(|i| f32_at(raw, i * 4));
    let projection = std::array::from_fn(|i| f32_at(raw, 64 + i * 4));
    Ok((view, projection))
}
pub fn ui_matrix_offset(raw: &[u8]) -> Result<usize> {
    if raw.len() < 128 {
        return Err("UI camera matrix block too short".into());
    }
    let mut found: Option<(usize, Vec<f64>)> = None;
    for off in (0..=raw.len().saturating_sub(128)).step_by(4) {
        if math::matrix_offset(&raw[off..off + 128], true) != Some(0) {
            continue;
        }
        let (v, p) = matrices(&raw[off..off + 128])?;
        let mut mapping = Vec::new();
        for point in [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]] {
            mapping.extend(
                math::project(&v, &p, point, false).ok_or("UI camera point behind screen")?,
            );
        }
        if let Some((_, previous)) = &found {
            if mapping
                .iter()
                .zip(previous)
                .any(|(a, b)| (a - b).abs() > 1e-6)
            {
                return Err("Ambiguous UI camera projection".into());
            }
        } else {
            found = Some((off, mapping));
        }
    }
    found
        .map(|v| v.0)
        .ok_or("UI camera matrices are absent".into())
}
#[derive(Clone, Debug)]
pub struct Widget {
    pub object: u64,
    pub path: Path,
    pub width: u64,
    pub height: u64,
    pub pivot: Option<u64>,
    pub region: Option<u64>,
}
impl Widget {
    /// A widget may legitimately move between native UI containers. The caller
    /// must still validate its managed ownership before refreshing this path.
    pub fn refresh_path<M:Memory>(&mut self,r:&mut Reflection<M>)->Result<()> {
        if self.path.validate(r).is_err(){*self=Self::new(r,self.object)?;}
        Ok(())
    }
    pub fn new<M: Memory>(r: &mut Reflection<M>, object: u64) -> Result<Self> {
        Ok(Self {
            object,
            path: Path::new(r, object, true)?,
            width: r.field(object, "mWidth")?,
            height: r.field(object, "mHeight")?,
            pivot: r.field(object, "mPivot").ok(),
            region: r.field(object, "mDrawRegion").ok(),
        })
    }
    pub fn geometry<M: Memory>(
        &self,
        r: &Reflection<M>,
        view: &[f64; 16],
        projection: &[f64; 16],
        pivot: bool,
    ) -> Result<([f64; 2], [f64; 2])> {
        let raw = self.path.bytes(r)?;
        let width = r.integer(self.width)? as f64;
        let height = r.integer(self.height)? as f64;
        if !(0. < width && width <= 16384. && 0. < height && height <= 16384.) {
            return Err("Invalid native widget size".into());
        }
        let mut local = None;
        let mut dx = width / 2.;
        let mut dy = height / 2.;
        if pivot {
            let anchors = [
                [0., 1.],
                [0.5, 1.],
                [1., 1.],
                [0., 0.5],
                [0.5, 0.5],
                [1., 0.5],
                [0., 0.],
                [0.5, 0.],
                [1., 0.],
            ];
            let index = r.integer(self.pivot.ok_or("Widget pivot is absent")?)?;
            if !(0..9).contains(&index) {
                return Err("Invalid widget pivot".into());
            }
            let region = r.read(self.region.ok_or("Widget region is absent")?, 16)?;
            let region: [f64; 4] = std::array::from_fn(|i| f32_at(&region, i * 4));
            if region
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
                || region[0] >= region[2]
                || region[1] >= region[3]
            {
                return Err("Invalid widget draw region".into());
            }
            let a = anchors[index as usize];
            local = Some([
                width * ((region[0] + region[2]) / 2. - a[0]),
                height * ((region[1] + region[3]) / 2. - a[1]),
                0.,
            ]);
            dx = width * (region[2] - region[0]) / 2.;
            dy = height * (region[3] - region[1]) / 2.;
        }
        let point = |p| -> Result<[f64; 2]> {
            let world = math::trs_chain(&raw, p).ok_or("Invalid widget transform")?;
            math::project(view, projection, world, false).ok_or("Widget behind UI camera".into())
        };
        let center = point(local)?;
        let base = local.unwrap_or([0.; 3]);
        let x = point(Some([base[0] + dx, base[1], base[2]]))?;
        let y = point(Some([base[0], base[1] + dy, base[2]]))?;
        let radius = std::array::from_fn(|i| (x[i] - center[i]).hypot(y[i] - center[i]));
        if radius
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.001 || *v >= 0.4)
        {
            return Err("Widget geometry outside screen bounds".into());
        }
        Ok((center, radius))
    }
    pub fn visible<M: Memory>(&self, r: &mut Reflection<M>) -> Result<bool> {
        for name in [
            "mIsActive",
            "mIsVisibleByAlpha",
            "mIsVisibleByPanel",
            "mEnabled",
        ] {
            let a = r.field(self.object, name)?;
            if !r.boolean(a)? {
                return Ok(false);
            }
        }
        let alpha = r.field(self.object, "finalAlpha")?;
        let alpha = r.float(alpha)?;
        let draw = r.field(self.object, "drawCall")?;
        Ok(alpha.is_finite() && alpha > 0. && alpha <= 1.001 && r.pointer(draw)? != 0)
    }
}
pub fn onscreen(p: [f64; 2]) -> bool {
    p.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))
}
