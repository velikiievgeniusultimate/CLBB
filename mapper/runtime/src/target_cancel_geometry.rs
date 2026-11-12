//! A Target cancel point requires a currently visible, owned native widget.
//! There is no GameObject-anchor or unrelated-camera fallback.
use crate::{
    math,
    platform::Memory,
    reflection::{Reflection, Result, f32_at, i32_at, u64_at},
    source::{Checks, capture, capture_path, check},
    unity::{self, Widget},
};
#[derive(Clone)]
pub(crate) struct CachedCancelWidgetPlan {
    checks: Checks,
    widget: Widget,
    matrix: u64,
    active: [u64; 4],
    alpha: u64,
    draw_call: u64,
    pivot: u64,
    region: u64,
}
fn ptr<M: Memory>(r: &Reflection<M>, c: &mut Checks, a: u64) -> Result<u64> {
    Ok(u64_at(&capture(r, c, a, 8)?, 0))
}
fn field<M: Memory>(
    r: &mut Reflection<M>,
    c: &mut Checks,
    o: u64,
    n: &str,
    kind: u8,
) -> Result<u64> {
    let mut k = ptr(r, c, o)?;
    for _ in 0..16 {
        let cl = r.class(k)?;
        if let Some(f) = cl.fields.get(n) {
            if f.is_static || f.kind != kind || f.offset > 1_048_576 {
                return Err(format!("Target cancel widget field differs:{n}"));
            }
            return o
                .checked_add(f.offset)
                .ok_or("Target cancel widget address overflow".into());
        }
        if cl.parent == 0 {
            break;
        }
        k = cl.parent;
    }
    Err(format!("Target cancel widget field absent:{n}"))
}
fn link<M: Memory>(
    r: &mut Reflection<M>,
    c: &mut Checks,
    o: u64,
    n: &str,
    kind: u8,
) -> Result<u64> {
    let a = field(r, c, o, n, kind)?;
    ptr(r, c, a)
}
impl CachedCancelWidgetPlan {
    pub(crate) fn discover<M: Memory>(r: &mut Reflection<M>, cancel_go: u64) -> Result<Self> {
        if cancel_go == 0 {
            return Err("Target cancel GameObject absent".into());
        }
        let mut checks = vec![];
        let class = ptr(r, &mut checks, cancel_go)?;
        if r.class(class)?.name != "GameObject" {
            return Err("Target cancel GameObject class differs".into());
        }
        let native = link(r, &mut checks, cancel_go, "m_CachedPtr", 24)?;
        if native == 0 || ptr(r, &mut checks, native + 40)? != cancel_go {
            return Err("Target cancel GameObject reciprocal owner differs".into());
        }
        let table = ptr(r, &mut checks, native + 48)?;
        let count = ptr(r, &mut checks, native + 64)?;
        if count == 0 || count > 32 || table == 0 {
            return Err("Target cancel component table outside bounds".into());
        }
        let rows = capture(r, &mut checks, table, count as usize * 16)?;
        let mut candidates = vec![];
        for row in rows.chunks_exact(16) {
            let component = u64_at(row, 8);
            if component == 0 || ptr(r, &mut checks, component + 48)? != native {
                return Err("Target cancel native component owner differs".into());
            }
            let object = ptr(r, &mut checks, component + 40)?;
            if object == 0 {
                continue;
            }
            let class = ptr(r, &mut checks, object)?;
            if link(r, &mut checks, object, "m_CachedPtr", 24)? != component {
                return Err("Target cancel managed component owner differs".into());
            }
            if r.class(class)?.name == "UISprite" {
                candidates.push(object);
            }
        }
        if candidates.len() != 1 {
            return Err("Target cancel UISprite absent/ambiguous".into());
        }
        let object = candidates[0];
        let camera = link(r, &mut checks, object, "mCam", 18)?;
        if camera == 0 {
            return Err("Target cancel widget own camera absent".into());
        }
        let class = ptr(r, &mut checks, camera)?;
        if r.class(class)?.name != "Camera" {
            return Err("Target cancel widget camera class differs".into());
        }
        let camera_native = link(r, &mut checks, camera, "m_CachedPtr", 24)?;
        if camera_native == 0 || ptr(r, &mut checks, camera_native + 40)? != camera {
            return Err("Target cancel camera reciprocal owner differs".into());
        }
        let matrix = camera_native + unity::ui_matrix_offset(&r.read(camera_native, 4096)?)? as u64;
        let width = field(r, &mut checks, object, "mWidth", 8)?;
        let height = field(r, &mut checks, object, "mHeight", 8)?;
        let pivot = field(r, &mut checks, object, "mPivot", 17)?;
        let region = field(r, &mut checks, object, "mDrawRegion", 17)?;
        let active = [
            "mIsActive",
            "mIsVisibleByAlpha",
            "mIsVisibleByPanel",
            "mEnabled",
        ]
        .map(|n| field(r, &mut checks, object, n, 2))
        .into_iter()
        .collect::<Result<Vec<_>>>()?
        .try_into()
        .unwrap();
        let alpha = field(r, &mut checks, object, "finalAlpha", 12)?;
        let draw_call = field(r, &mut checks, object, "drawCall", 18)?;
        let widget = Widget::new(r, object)?;
        if widget.width != width
            || widget.height != height
            || widget.pivot != Some(pivot)
            || widget.region != Some(region)
        {
            return Err("Target cancel widget layout differs".into());
        }
        let plan = Self {
            checks,
            widget,
            matrix,
            active,
            alpha,
            draw_call,
            pivot,
            region,
        };
        plan.validate(r)?;
        Ok(plan)
    }
    pub(crate) fn validate<M: Memory>(&self, r: &Reflection<M>) -> Result<()> {
        check(r, &self.checks)?;
        self.widget.path.validate(r)
    }
    /// The enclosing Source transaction rechecks `consumed` with its exact
    /// Target owner/attempt. Hidden or partly initialized widgets stay unknown.
    pub(crate) fn sample<M: Memory>(
        &self,
        r: &Reflection<M>,
        consumed: &mut Checks,
    ) -> Result<[f64; 2]> {
        self.validate(r)?;
        consumed.extend(self.checks.iter().cloned());
        for at in self.active {
            if capture(r, consumed, at, 1)?[0] != 1 {
                return Err("Target cancel widget hidden".into());
            }
        }
        let alpha = f32_at(&capture(r, consumed, self.alpha, 4)?, 0);
        if !alpha.is_finite()
            || alpha <= 0.
            || alpha > 1.001
            || ptr(r, consumed, self.draw_call)? == 0
        {
            return Err("Target cancel widget is not rendered".into());
        }
        let width = i32_at(&capture(r, consumed, self.widget.width, 4)?, 0) as f64;
        let height = i32_at(&capture(r, consumed, self.widget.height, 4)?, 0) as f64;
        if !(0. < width && width <= 16384. && 0. < height && height <= 16384.) {
            return Err("Target cancel widget dimensions invalid".into());
        }
        let pivot = i32_at(&capture(r, consumed, self.pivot, 4)?, 0);
        if !(0..9).contains(&pivot) {
            return Err("Target cancel widget pivot invalid".into());
        }
        let raw = capture(r, consumed, self.region, 16)?;
        let region: [f64; 4] = std::array::from_fn(|i| f32_at(&raw, i * 4));
        if region
            .iter()
            .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
            || region[0] >= region[2]
            || region[1] >= region[3]
        {
            return Err("Target cancel draw region invalid".into());
        }
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
        let local = [
            width * ((region[0] + region[2]) / 2. - anchors[pivot as usize][0]),
            height * ((region[1] + region[3]) / 2. - anchors[pivot as usize][1]),
            0.,
        ];
        let transforms = capture_path(r, consumed, &self.widget.path)?;
        let camera = capture(r, consumed, self.matrix, 128)?;
        if math::matrix_offset(&camera, true) != Some(0) {
            return Err("Target cancel widget camera matrix invalid".into());
        }
        let (view, projection) = unity::matrices(&camera)?;
        let project = |point| {
            let world = math::trs_chain(&transforms, Some(point))
                .ok_or("Target cancel widget transform invalid")?;
            math::project(&view, &projection, world, false)
                .ok_or("Target cancel widget behind camera")
        };
        let center = project(local)?;
        let x = project([
            local[0] + width * (region[2] - region[0]) / 2.,
            local[1],
            0.,
        ])?;
        let y = project([
            local[0],
            local[1] + height * (region[3] - region[1]) / 2.,
            0.,
        ])?;
        let radius: [f64; 2] = std::array::from_fn(|i| (x[i] - center[i]).hypot(y[i] - center[i]));
        if !unity::onscreen(center) || radius.iter().any(|v| !v.is_finite() || *v <= 0. || *v > 1.)
        {
            return Err("Target cancel widget geometry invalid/offscreen".into());
        }
        Ok(center)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Region;
    use std::cell::RefCell;
    struct Fake(RefCell<Vec<u8>>);
    impl Memory for Fake {
        fn regions(&self) -> Result<Vec<Region>> {
            Ok(vec![Region {
                start: 0,
                end: self.0.borrow().len() as u64,
                readable: true,
                writable: true,
                name: String::new(),
            }])
        }
        fn read(&self, at: u64, n: usize) -> Result<Vec<u8>> {
            self.0
                .borrow()
                .get(at as usize..at as usize + n)
                .map(|v| v.to_vec())
                .ok_or("geometry fixture out of bounds".into())
        }
        fn read_into(&self, at: u64, out: &mut [u8]) -> Result<()> {
            out.copy_from_slice(&self.read(at, out.len())?);
            Ok(())
        }
    }
    const GO: u64 = 0x9000;
    const SPRITE: u64 = 0x9200;
    const CAM: u64 = 0x9300;
    const NATIVE_GO: u64 = 0xa000;
    const TRANSFORM: u64 = 0xa100;
    const NATIVE_SPRITE: u64 = 0xa200;
    const TABLE: u64 = 0xb000;
    const NATIVE_CAM: u64 = 0xc000;
    const MATRIX: u64 = 0xc100;
    const HIERARCHY: u64 = 0xd000;
    const TRS: u64 = 0xd100;
    const PARENTS: u64 = 0xd200;
    fn u64put(b: &mut [u8], at: u64, v: u64) {
        b[at as usize..at as usize + 8].copy_from_slice(&v.to_le_bytes());
    }
    fn i32put(b: &mut [u8], at: u64, v: i32) {
        b[at as usize..at as usize + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn real(b: &mut [u8], at: u64, v: f32) {
        b[at as usize..at as usize + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn class(b: &mut [u8], next: &mut usize, at: u64, name: &str, fields: &[(&str, u8, u64)]) {
        u64put(b, at + 0x40, at);
        u64put(b, at + 0x48, at);
        let n = *next;
        *next += name.len() + 1;
        b[n..n + name.len()].copy_from_slice(name.as_bytes());
        u64put(b, at + 16, n as u64);
        *next = (*next + 7) & !7;
        let table = *next;
        *next += fields.len() * 32;
        u64put(b, at + 0x78, table as u64);
        b[at as usize + 0x104..at as usize + 0x106]
            .copy_from_slice(&(fields.len() as u16).to_le_bytes());
        for (i, (name, kind, offset)) in fields.iter().enumerate() {
            let n = *next;
            *next += name.len() + 1;
            b[n..n + name.len()].copy_from_slice(name.as_bytes());
            *next = (*next + 7) & !7;
            let ty = *next;
            *next += 16;
            let row = table + i * 32;
            u64put(b, row as u64, n as u64);
            u64put(b, row as u64 + 8, ty as u64);
            u64put(b, row as u64 + 16, at);
            i32put(b, row as u64 + 24, *offset as i32);
            i32put(b, ty as u64 + 8, (*kind as i32) << 16);
        }
    }
    fn fixture() -> Reflection<Fake> {
        let mut b = vec![0; 0x20000];
        let mut next = 0x1000;
        class(
            &mut b,
            &mut next,
            0x100,
            "GameObject",
            &[("m_CachedPtr", 24, 16)],
        );
        class(
            &mut b,
            &mut next,
            0x500,
            "UISprite",
            &[
                ("m_CachedPtr", 24, 16),
                ("mCam", 18, 24),
                ("mWidth", 8, 32),
                ("mHeight", 8, 36),
                ("mPivot", 17, 40),
                ("mDrawRegion", 17, 44),
                ("mIsActive", 2, 64),
                ("mIsVisibleByAlpha", 2, 65),
                ("mIsVisibleByPanel", 2, 66),
                ("mEnabled", 2, 67),
                ("finalAlpha", 12, 68),
                ("drawCall", 18, 72),
            ],
        );
        class(
            &mut b,
            &mut next,
            0x700,
            "Camera",
            &[("m_CachedPtr", 24, 16)],
        );
        u64put(&mut b, GO, 0x100);
        u64put(&mut b, GO + 16, NATIVE_GO);
        u64put(&mut b, NATIVE_GO + 40, GO);
        u64put(&mut b, NATIVE_GO + 48, TABLE);
        u64put(&mut b, NATIVE_GO + 64, 2);
        u64put(&mut b, TABLE + 8, TRANSFORM);
        u64put(&mut b, TABLE + 24, NATIVE_SPRITE);
        u64put(&mut b, TRANSFORM + 48, NATIVE_GO);
        u64put(&mut b, TRANSFORM + 56, HIERARCHY);
        i32put(&mut b, TRANSFORM + 64, 0);
        u64put(&mut b, HIERARCHY + 24, TRS);
        u64put(&mut b, HIERARCHY + 32, PARENTS);
        i32put(&mut b, PARENTS, -1);
        real(&mut b, TRS, 1.);
        real(&mut b, TRS + 4, 2.);
        real(&mut b, TRS + 28, 1.);
        for i in 8..=10 {
            real(&mut b, TRS + i * 4, 1.);
        }
        u64put(&mut b, NATIVE_SPRITE + 48, NATIVE_GO);
        u64put(&mut b, NATIVE_SPRITE + 40, SPRITE);
        u64put(&mut b, SPRITE, 0x500);
        u64put(&mut b, SPRITE + 16, NATIVE_SPRITE);
        u64put(&mut b, SPRITE + 24, CAM);
        i32put(&mut b, SPRITE + 32, 4);
        i32put(&mut b, SPRITE + 36, 2);
        i32put(&mut b, SPRITE + 40, 4);
        for (i, v) in [0., 0., 1., 1.].into_iter().enumerate() {
            real(&mut b, SPRITE + 44 + i as u64 * 4, v);
        }
        b[SPRITE as usize + 64..SPRITE as usize + 68].fill(1);
        real(&mut b, SPRITE + 68, 1.);
        u64put(&mut b, SPRITE + 72, 0xf000);
        u64put(&mut b, CAM, 0x700);
        u64put(&mut b, CAM + 16, NATIVE_CAM);
        u64put(&mut b, NATIVE_CAM + 40, CAM);
        for i in [0, 5, 10, 15] {
            real(&mut b, MATRIX + i * 4, 1.);
        }
        real(&mut b, MATRIX + 64, 0.1);
        real(&mut b, MATRIX + 64 + 5 * 4, 0.1);
        real(&mut b, MATRIX + 64 + 10 * 4, -1.);
        real(&mut b, MATRIX + 64 + 15 * 4, 1.);
        Reflection::new(Fake(RefCell::new(b))).unwrap()
    }
    fn sample(r: &mut Reflection<Fake>) -> Result<[f64; 2]> {
        let p = CachedCancelWidgetPlan::discover(r, GO)?;
        let mut c = vec![];
        let v = p.sample(r, &mut c)?;
        check(r, &c)?;
        Ok(v)
    }
    fn close(a: [f64; 2], b: [f64; 2]) {
        assert!(
            (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6,
            "{a:?} != {b:?}"
        );
    }
    #[test]
    fn visible_owned_widget_uses_its_own_camera_and_current_pivot_region() {
        let mut r = fixture();
        close(sample(&mut r).unwrap(), [0.55, 0.4]);
        i32put(&mut r.memory.0.borrow_mut(), SPRITE + 40, 0);
        close(sample(&mut r).unwrap(), [0.65, 0.45]);
        {
            let mut b = r.memory.0.borrow_mut();
            real(&mut b, SPRITE + 44, 0.25);
            real(&mut b, SPRITE + 52, 0.75);
        }
        close(sample(&mut r).unwrap(), [0.65, 0.45]);
    }
    #[test]
    fn hidden_or_unrendered_widget_never_falls_back_to_onscreen_gameobject_anchor() {
        for at in SPRITE + 64..SPRITE + 68 {
            let mut r = fixture();
            r.memory.0.borrow_mut()[at as usize] = 0;
            assert!(sample(&mut r).is_err());
        }
        for alpha in [0., -1., f32::NAN, 2.] {
            let mut r = fixture();
            real(&mut r.memory.0.borrow_mut(), SPRITE + 68, alpha);
            assert!(sample(&mut r).is_err());
        }
        let mut r = fixture();
        u64put(&mut r.memory.0.borrow_mut(), SPRITE + 72, 0);
        assert!(sample(&mut r).is_err());
        let mut r = fixture();
        u64put(&mut r.memory.0.borrow_mut(), SPRITE + 24, 0);
        assert!(CachedCancelWidgetPlan::discover(&mut r, GO).is_err());
    }
    #[test]
    fn malformed_size_pivot_region_or_offscreen_geometry_is_unknown() {
        for (at, v) in [
            (SPRITE + 32, 0),
            (SPRITE + 36, 0),
            (SPRITE + 40, 9),
            (SPRITE + 32, 16385),
        ] {
            let mut r = fixture();
            i32put(&mut r.memory.0.borrow_mut(), at, v);
            assert!(sample(&mut r).is_err());
        }
        for (at, v) in [
            (SPRITE + 44, f32::NAN),
            (SPRITE + 44, 1.),
            (SPRITE + 56, 0.),
            (TRS, 100.),
        ] {
            let mut r = fixture();
            real(&mut r.memory.0.borrow_mut(), at, v);
            assert!(sample(&mut r).is_err());
        }
    }
    #[test]
    fn reciprocal_owners_and_unique_sprite_are_required() {
        for (at, v) in [
            (NATIVE_GO + 40, GO + 1),
            (NATIVE_SPRITE + 48, NATIVE_GO + 1),
            (SPRITE + 16, 0),
            (NATIVE_CAM + 40, CAM + 1),
            (NATIVE_GO + 64, 33),
        ] {
            let mut r = fixture();
            u64put(&mut r.memory.0.borrow_mut(), at, v);
            assert!(
                CachedCancelWidgetPlan::discover(&mut r, GO).is_err(),
                "{at:#x}"
            );
        }
        let mut r = fixture();
        u64put(&mut r.memory.0.borrow_mut(), NATIVE_GO + 64, 1);
        assert!(CachedCancelWidgetPlan::discover(&mut r, GO).is_err());
        let mut r = fixture();
        {
            let mut b = r.memory.0.borrow_mut();
            u64put(&mut b, NATIVE_GO + 64, 3);
            u64put(&mut b, TABLE + 40, NATIVE_SPRITE);
        }
        assert!(CachedCancelWidgetPlan::discover(&mut r, GO).is_err());
    }
    #[test]
    fn cached_rebind_reparent_and_consumed_geometry_changes_revoke_projection() {
        for (at, v) in [
            (SPRITE + 24, CAM + 1),
            (SPRITE + 16, NATIVE_SPRITE + 1),
            (TRANSFORM + 56, HIERARCHY + 1),
            (NATIVE_CAM + 40, CAM + 1),
        ] {
            let mut r = fixture();
            let p = CachedCancelWidgetPlan::discover(&mut r, GO).unwrap();
            u64put(&mut r.memory.0.borrow_mut(), at, v);
            assert!(p.validate(&r).is_err());
        }
        let mut r = fixture();
        let p = CachedCancelWidgetPlan::discover(&mut r, GO).unwrap();
        let mut c = vec![];
        p.sample(&r, &mut c).unwrap();
        real(&mut r.memory.0.borrow_mut(), TRS, 3.);
        assert!(check(&r, &c).is_err());
        let mut r = fixture();
        let p = CachedCancelWidgetPlan::discover(&mut r, GO).unwrap();
        let mut c = vec![];
        p.sample(&r, &mut c).unwrap();
        real(&mut r.memory.0.borrow_mut(), MATRIX + 12 * 4, 0.1);
        assert!(check(&r, &c).is_err());
    }
}
