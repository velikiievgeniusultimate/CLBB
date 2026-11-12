//! Cached native UI-contact receipts. Positions are UICamera pixels, never world aim.
use crate::{
    platform::Memory,
    reflection::{Reflection, Result, f32_at, i32_at, u64_at},
    source::{Checks, capture, check},
};
use std::collections::BTreeSet;
const MAX_CONTACTS: usize = 16;
const MAX_CAPACITY: u64 = 256;

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ContactObservation {
    /// False means there is no owned contact; all remaining values are unknown.
    pub(crate) present: bool,
    pub(crate) native_id: i32,
    pub(crate) contact_owner_id: u64,
    pub(crate) object: u64,
    pub(crate) current_go: u64,
    pub(crate) raw_pos: [f64; 2],
    pub(crate) pressed_camera: u64,
    pub(crate) drag_started: bool,
    pub(crate) press_started: bool,
    pub(crate) cancel_hit: bool,
}
#[derive(Clone)]
struct ListPlan {
    items: u64,
    size: u64,
    version: u64,
    array_class: u64,
    element_class: u64,
}
#[derive(Clone)]
pub(crate) struct ContactRegistryPlan {
    checks: Checks,
    touches: ListPlan,
    ids: ListPlan,
    touch_class: u64,
    pos: u64,
    pressed_cam: u64,
    current: u64,
    pressed: u64,
    dragged: u64,
    press_started: u64,
    drag_started: u64,
}
fn pointer<M: Memory>(r: &Reflection<M>, c: &mut Checks, at: u64) -> Result<u64> {
    Ok(u64_at(&capture(r, c, at, 8)?, 0))
}
fn offset<M: Memory>(r: &mut Reflection<M>, mut k: u64, name: &str, kind: u8) -> Result<u64> {
    for _ in 0..16 {
        let cl = r.class(k)?;
        if let Some(f) = cl.fields.get(name) {
            if f.is_static || f.kind != kind || f.offset > 1_048_576 {
                return Err(format!("Target contact field type differs: {name}"));
            }
            return Ok(f.offset);
        }
        if cl.parent == 0 {
            break;
        }
        k = cl.parent;
    }
    Err(format!("Target contact field absent: {name}"))
}
fn object_field<M: Memory>(
    r: &mut Reflection<M>,
    c: &mut Checks,
    o: u64,
    name: &str,
    kind: u8,
) -> Result<u64> {
    let k = pointer(r, c, o)?;
    Ok(o + offset(r, k, name, kind)?)
}
fn link<M: Memory>(
    r: &mut Reflection<M>,
    c: &mut Checks,
    o: u64,
    name: &str,
    kind: u8,
) -> Result<u64> {
    let at = object_field(r, c, o, name, kind)?;
    pointer(r, c, at)
}
fn list_plan<M: Memory>(
    r: &mut Reflection<M>,
    c: &mut Checks,
    o: u64,
    element: &str,
) -> Result<ListPlan> {
    if o == 0 {
        return Err("Native contact list absent".into());
    }
    let k = pointer(r, c, o)?;
    if r.class(k)?.name != "List`1" {
        return Err("Native contact collection is not List".into());
    }
    let items = o + offset(r, k, "_items", 29)?;
    let size = o + offset(r, k, "_size", 8)?;
    let version = o + offset(r, k, "_version", 8)?;
    // Arrays may be replaced as the list grows: cache the typed array class,
    // never the mutable array pointer, length or list version.
    let a = r.pointer(items)?;
    if a == 0 {
        return Err("Native contact array not initialized".into());
    }
    let h = r.read(a, 32)?;
    let array_class = u64_at(&h, 0);
    let element_class = pointer(r, c, array_class + 0x40)?;
    if r.class(element_class)?.name != element {
        return Err("Native contact array element differs".into());
    }
    let p = ListPlan {
        items,
        size,
        version,
        array_class,
        element_class,
    };
    let mut initial = vec![];
    list_bytes(r, &mut initial, &p, if element == "Int32" { 4 } else { 8 })?;
    check(r, &initial)?;
    Ok(p)
}
fn list_bytes<M: Memory>(
    r: &Reflection<M>,
    c: &mut Checks,
    p: &ListPlan,
    stride: usize,
) -> Result<Vec<u8>> {
    let a = pointer(r, c, p.items)?;
    let n = i32_at(&capture(r, c, p.size, 4)?, 0);
    capture(r, c, p.version, 4)?;
    if a == 0 || n < 0 || n as usize > MAX_CONTACTS {
        return Err("Native contact count outside bound".into());
    }
    let h = capture(r, c, a, 32)?;
    let capacity = u64_at(&h, 24);
    if u64_at(&h, 0) != p.array_class
        || capacity < n as u64
        || capacity > MAX_CAPACITY
        || pointer(r, c, p.array_class + 0x40)? != p.element_class
    {
        return Err("Native contact array type/capacity differs".into());
    }
    if n == 0 {
        return Ok(vec![]);
    }
    capture(r, c, a + 32, n as usize * stride)
}
impl ContactRegistryPlan {
    pub(crate) fn discover<M: Memory>(r: &mut Reflection<M>, ui_camera_class: u64) -> Result<Self> {
        let cl = r.class(ui_camera_class)?;
        if cl.name != "UICamera" || cl.statics == 0 {
            return Err("Native UICamera class/statics absent".into());
        }
        let mut checks = vec![];
        if pointer(r, &mut checks, ui_camera_class + 0xa8)? != cl.statics {
            return Err("Native UICamera statics changed".into());
        }
        let mut static_list = |name: &str| -> Result<u64> {
            let f = cl
                .fields
                .get(name)
                .ok_or("Native contact registry field absent")?;
            if !f.is_static || f.kind != 21 || f.offset > 1_048_576 {
                return Err("Native contact registry type differs".into());
            }
            pointer(r, &mut checks, cl.statics + f.offset)
        };
        let touches = static_list("activeTouches")?;
        let ids = static_list("mTouchIDs")?;
        let touches = list_plan(r, &mut checks, touches, "MouseOrTouch")?;
        let ids = list_plan(r, &mut checks, ids, "Int32")?;
        let touch_class = touches.element_class;
        let out = Self {
            pos: offset(r, touch_class, "pos", 17)?,
            pressed_cam: offset(r, touch_class, "pressedCam", 18)?,
            current: offset(r, touch_class, "current", 18)?,
            pressed: offset(r, touch_class, "pressed", 18)?,
            dragged: offset(r, touch_class, "dragged", 18)?,
            press_started: offset(r, touch_class, "pressStarted", 2)?,
            drag_started: offset(r, touch_class, "dragStarted", 2)?,
            checks,
            touches,
            ids,
            touch_class,
        };
        out.validate(r)?;
        Ok(out)
    }
    pub(crate) fn validate<M: Memory>(&self, r: &Reflection<M>) -> Result<()> {
        check(r, &self.checks)
    }
    /// The caller owns follow/cancel GO identity and must recheck `consumed`
    /// with its backend, GUID, geometry and attempt fences before publishing.
    pub(crate) fn sample<M: Memory>(
        &self,
        r: &Reflection<M>,
        follow_go: u64,
        cancel_go: u64,
        consumed: &mut Checks,
    ) -> Result<ContactObservation> {
        if follow_go == 0 {
            return Err("Owned target follow GameObject absent".into());
        }
        self.validate(r)?;
        consumed.extend(self.checks.iter().cloned());
        let objects = list_bytes(r, consumed, &self.touches, 8)?;
        let ids = list_bytes(r, consumed, &self.ids, 4)?;
        if objects.len() / 8 != ids.len() / 4 {
            return Err("Native contact ID/object count differs".into());
        }
        let mut unique_ids = BTreeSet::new();
        let mut unique_objects = BTreeSet::new();
        let mut found = None;
        for (o, id) in objects.chunks_exact(8).zip(ids.chunks_exact(4)) {
            let object = u64_at(o, 0);
            let id = i32_at(id, 0);
            if object == 0
                || !unique_ids.insert(id)
                || !unique_objects.insert(object)
                || pointer(r, consumed, object)? != self.touch_class
            {
                return Err("Native contact identities are ambiguous".into());
            }
            let pressed = pointer(r, consumed, object + self.pressed)?;
            let dragged = pointer(r, consumed, object + self.dragged)?;
            if pressed != follow_go || dragged != follow_go {
                continue;
            }
            if found.is_some() {
                return Err("Multiple native contacts own target selector".into());
            }
            let pos = capture(r, consumed, object + self.pos, 8)?;
            let raw_pos = [f32_at(&pos, 0), f32_at(&pos, 4)];
            if raw_pos
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 1_000_000.)
            {
                return Err("Invalid owned native contact position".into());
            }
            let current_go = pointer(r, consumed, object + self.current)?;
            let pressed_camera = pointer(r, consumed, object + self.pressed_cam)?;
            let flag = |at, c: &mut Checks| -> Result<bool> {
                let v = capture(r, c, at, 1)?[0];
                if v > 1 {
                    return Err("Invalid native contact flag".into());
                }
                Ok(v != 0)
            };
            found = Some(ContactObservation {
                present: true,
                native_id: id,
                contact_owner_id: object,
                object,
                current_go,
                raw_pos,
                pressed_camera,
                press_started: flag(object + self.press_started, consumed)?,
                drag_started: flag(object + self.drag_started, consumed)?,
                cancel_hit: cancel_go != 0 && current_go == cancel_go,
            });
        }
        Ok(found.unwrap_or_default())
    }
}

/// Cold, bounded ownership discovery; no fixed ELF class slot or heap scan.
/// A camera without a validated co-located UICamera remains unknown.
pub(crate) fn discover_ui_camera_class<M: Memory>(
    r: &mut Reflection<M>,
    owned_camera: u64,
) -> Result<u64> {
    let mut c = vec![];
    let k = pointer(r, &mut c, owned_camera)?;
    if r.class(k)?.name != "Camera" {
        return Err("Owned UI camera class differs".into());
    }
    let camera_native = link(r, &mut c, owned_camera, "m_CachedPtr", 24)?;
    if camera_native == 0 || pointer(r, &mut c, camera_native + 40)? != owned_camera {
        return Err("Camera native/managed owner differs".into());
    }
    let go_native = pointer(r, &mut c, camera_native + 48)?;
    let go = pointer(r, &mut c, go_native + 40)?;
    let go_class = pointer(r, &mut c, go)?;
    if go_native == 0
        || go == 0
        || r.class(go_class)?.name != "GameObject"
        || link(r, &mut c, go, "m_CachedPtr", 24)? != go_native
    {
        return Err("Camera GameObject owner differs".into());
    }
    let table = pointer(r, &mut c, go_native + 48)?;
    let count = pointer(r, &mut c, go_native + 64)?;
    if count > 32 || (count != 0 && table == 0) {
        return Err("Camera component table outside bounds".into());
    }
    let rows = if count == 0 {
        vec![]
    } else {
        capture(r, &mut c, table, count as usize * 16)?
    };
    let mut found = vec![];
    for row in rows.chunks_exact(16) {
        let native = u64_at(row, 8);
        if native == 0 || pointer(r, &mut c, native + 48)? != go_native {
            return Err("Camera component reciprocal owner differs".into());
        }
        let object = pointer(r, &mut c, native + 40)?;
        if object == 0 {
            continue;
        }
        let class = pointer(r, &mut c, object)?;
        let cl = r.class(class)?;
        if link(r, &mut c, object, "m_CachedPtr", 24)? != native {
            return Err("Camera managed component owner differs".into());
        }
        if cl.name == "UICamera" && link(r, &mut c, object, "mCam", 18)? == owned_camera {
            found.push(class);
        }
    }
    if found.len() != 1 {
        return Err("Owned UICamera component absent/ambiguous".into());
    }
    check(r, &c)?;
    Ok(found[0])
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
            let b = self.0.borrow();
            b.get(at as usize..at as usize + n)
                .map(|v| v.to_vec())
                .ok_or("fixture read outside bounds".into())
        }
        fn read_into(&self, at: u64, out: &mut [u8]) -> Result<()> {
            out.copy_from_slice(&self.read(at, out.len())?);
            Ok(())
        }
    }
    const UI: u64 = 0x100;
    const LIST: u64 = 0x300;
    const TOUCH: u64 = 0x500;
    const INT: u64 = 0x700;
    const GO_CLASS: u64 = 0x900;
    const CAM_CLASS: u64 = 0xb00;
    const ATOUCH: u64 = 0xd00;
    const AINT: u64 = 0xe00;
    const STATICS: u64 = 0x8000;
    const TOUCHES: u64 = 0x9000;
    const IDS: u64 = 0x9100;
    const TOUCH_ARRAY: u64 = 0xa000;
    const ID_ARRAY: u64 = 0xa100;
    const T0: u64 = 0xb000;
    const T1: u64 = 0xb100;
    const FOLLOW: u64 = 0xc000;
    const CANCEL: u64 = 0xc100;
    const TARGET: u64 = 0xc200;
    const CAMERA: u64 = 0xc300;
    fn put64(b: &mut [u8], a: u64, v: u64) {
        b[a as usize..a as usize + 8].copy_from_slice(&v.to_le_bytes());
    }
    fn put32(b: &mut [u8], a: u64, v: i32) {
        b[a as usize..a as usize + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn real(b: &mut [u8], a: u64, v: f32) {
        b[a as usize..a as usize + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn class(
        b: &mut [u8],
        next: &mut usize,
        at: u64,
        name: &str,
        statics: u64,
        fields: &[(&str, u8, u64, bool)],
    ) {
        put64(b, at + 0x40, at);
        put64(b, at + 0x48, at);
        put64(b, at + 0xa8, statics);
        let name_at = *next;
        *next += name.len() + 1;
        b[name_at..name_at + name.len()].copy_from_slice(name.as_bytes());
        put64(b, at + 16, name_at as u64);
        *next = (*next + 7) & !7;
        let table = *next;
        *next += fields.len() * 32;
        put64(b, at + 0x78, table as u64);
        b[at as usize + 0x104..at as usize + 0x106]
            .copy_from_slice(&(fields.len() as u16).to_le_bytes());
        for (i, (name, kind, offset, static_field)) in fields.iter().enumerate() {
            let n = *next;
            *next += name.len() + 1;
            b[n..n + name.len()].copy_from_slice(name.as_bytes());
            *next = (*next + 7) & !7;
            let ty = *next;
            *next += 16;
            let row = table + i * 32;
            put64(b, row as u64, n as u64);
            put64(b, row as u64 + 8, ty as u64);
            put64(b, row as u64 + 16, at);
            put32(b, row as u64 + 24, *offset as i32);
            put32(
                b,
                ty as u64 + 8,
                ((*kind as u32) << 16 | if *static_field { 16 } else { 0 }) as i32,
            );
        }
    }
    fn fixture() -> Reflection<Fake> {
        let mut b = vec![0; 0x20000];
        let mut next = 0x1000;
        class(
            &mut b,
            &mut next,
            UI,
            "UICamera",
            STATICS,
            &[
                ("activeTouches", 21, 360, true),
                ("mTouchIDs", 21, 368, true),
                ("currentTouch", 18, 160, true),
                ("mCam", 18, 128, false),
                ("m_CachedPtr", 24, 16, false),
            ],
        );
        class(
            &mut b,
            &mut next,
            LIST,
            "List`1",
            0,
            &[
                ("_items", 29, 16, false),
                ("_size", 8, 24, false),
                ("_version", 8, 28, false),
            ],
        );
        class(
            &mut b,
            &mut next,
            TOUCH,
            "MouseOrTouch",
            0,
            &[
                ("pos", 17, 16, false),
                ("pressedCam", 18, 48, false),
                ("current", 18, 64, false),
                ("pressed", 18, 72, false),
                ("dragged", 18, 80, false),
                ("pressStarted", 2, 101, false),
                ("dragStarted", 2, 102, false),
            ],
        );
        class(&mut b, &mut next, INT, "Int32", 0, &[]);
        class(
            &mut b,
            &mut next,
            GO_CLASS,
            "GameObject",
            0,
            &[("m_CachedPtr", 24, 16, false)],
        );
        class(
            &mut b,
            &mut next,
            CAM_CLASS,
            "Camera",
            0,
            &[("m_CachedPtr", 24, 16, false)],
        );
        put64(&mut b, ATOUCH + 0x40, TOUCH);
        put64(&mut b, AINT + 0x40, INT);
        put64(&mut b, STATICS + 360, TOUCHES);
        put64(&mut b, STATICS + 368, IDS);
        for (o, a) in [(TOUCHES, TOUCH_ARRAY), (IDS, ID_ARRAY)] {
            put64(&mut b, o, LIST);
            put64(&mut b, o + 16, a);
            put32(&mut b, o + 24, 1);
            put32(&mut b, o + 28, 1);
        }
        put64(&mut b, TOUCH_ARRAY, ATOUCH);
        put64(&mut b, TOUCH_ARRAY + 24, 4);
        put64(&mut b, TOUCH_ARRAY + 32, T0);
        put64(&mut b, ID_ARRAY, AINT);
        put64(&mut b, ID_ARRAY + 24, 4);
        put32(&mut b, ID_ARRAY + 32, 42);
        for o in [T0, T1] {
            put64(&mut b, o, TOUCH);
            real(&mut b, o + 16, 3751.0386);
            real(&mut b, o + 20, 1411.8866);
            put64(&mut b, o + 48, CAMERA);
            put64(&mut b, o + 64, TARGET);
            put64(&mut b, o + 72, FOLLOW);
            put64(&mut b, o + 80, FOLLOW);
            b[o as usize + 101] = 1;
            b[o as usize + 102] = 1;
        }
        Reflection::new(Fake(RefCell::new(b))).unwrap()
    }
    fn sample(r: &mut Reflection<Fake>) -> Result<ContactObservation> {
        let p = ContactRegistryPlan::discover(r, UI)?;
        let mut c = vec![];
        let v = p.sample(r, FOLLOW, CANCEL, &mut c)?;
        check(r, &c)?;
        Ok(v)
    }
    #[test]
    fn owned_contact_joins_native_id_by_index_and_does_not_require_callback_current_touch() {
        let mut r = fixture();
        let v = sample(&mut r).unwrap();
        assert!(v.present);
        assert_eq!(v.native_id, 42);
        assert_eq!(v.object, T0);
        assert_eq!(v.contact_owner_id, T0);
        assert_eq!(v.current_go, TARGET);
        assert_eq!(v.pressed_camera, CAMERA);
        assert!(v.press_started && v.drag_started);
        assert!(!v.cancel_hit);
        assert!((v.raw_pos[0] - 3751.0386).abs() < 0.001);
        // currentTouch is deliberately null: it is scoped only to callbacks.
        assert_eq!(r.pointer(STATICS + 160).unwrap(), 0);
        put64(&mut r.memory.0.borrow_mut(), T0 + 64, CANCEL);
        assert!(sample(&mut r).unwrap().cancel_hit);
    }
    #[test]
    fn empty_or_foreign_contacts_do_not_publish_owned_raw_position() {
        let mut r = fixture();
        put32(&mut r.memory.0.borrow_mut(), TOUCHES + 24, 0);
        put32(&mut r.memory.0.borrow_mut(), IDS + 24, 0);
        let v = sample(&mut r).unwrap();
        assert_eq!(v, ContactObservation::default());
        let mut r = fixture();
        put64(&mut r.memory.0.borrow_mut(), T0 + 72, FOLLOW + 1);
        assert!(!sample(&mut r).unwrap().present);
        let mut r = fixture();
        put64(&mut r.memory.0.borrow_mut(), T0 + 80, FOLLOW + 1);
        assert!(!sample(&mut r).unwrap().present);
    }
    #[test]
    fn mismatched_counts_duplicate_ids_or_owned_contacts_are_unknown() {
        let mut r = fixture();
        put32(&mut r.memory.0.borrow_mut(), IDS + 24, 0);
        assert!(sample(&mut r).is_err());
        for id in [42, 901] {
            let mut r = fixture();
            {
                let mut b = r.memory.0.borrow_mut();
                put32(&mut b, TOUCHES + 24, 2);
                put32(&mut b, IDS + 24, 2);
                put64(&mut b, TOUCH_ARRAY + 40, T1);
                put32(&mut b, ID_ARRAY + 36, id);
            }
            assert!(sample(&mut r).is_err());
        }
    }
    #[test]
    fn cached_rebind_and_consumed_changes_revoke_the_receipt() {
        for (at, value) in [
            (STATICS + 360, TOUCHES + 8),
            (STATICS + 368, IDS + 8),
            (TOUCHES, TOUCH + 1),
            (ATOUCH + 0x40, INT),
        ] {
            let mut r = fixture();
            let p = ContactRegistryPlan::discover(&mut r, UI).unwrap();
            put64(&mut r.memory.0.borrow_mut(), at, value);
            assert!(p.validate(&r).is_err());
            assert!(p.sample(&r, FOLLOW, CANCEL, &mut vec![]).is_err());
        }
        let mut r = fixture();
        let p = ContactRegistryPlan::discover(&mut r, UI).unwrap();
        let mut c = vec![];
        p.sample(&r, FOLLOW, CANCEL, &mut c).unwrap();
        real(&mut r.memory.0.borrow_mut(), T0 + 16, 22.);
        assert!(check(&r, &c).is_err());
        let mut r = fixture();
        let p = ContactRegistryPlan::discover(&mut r, UI).unwrap();
        let mut c = vec![];
        p.sample(&r, FOLLOW, CANCEL, &mut c).unwrap();
        put32(&mut r.memory.0.borrow_mut(), IDS + 28, 2);
        assert!(check(&r, &c).is_err());
    }
    #[test]
    fn typed_array_growth_preserves_native_id_join_without_rediscovery() {
        let mut r = fixture();
        let p = ContactRegistryPlan::discover(&mut r, UI).unwrap();
        {
            let mut b = r.memory.0.borrow_mut();
            let ta = 0x11000;
            let ia = 0x11100;
            put64(&mut b, ta, ATOUCH);
            put64(&mut b, ta + 24, 8);
            put64(&mut b, ta + 32, T0);
            put64(&mut b, ta + 40, T1);
            put64(&mut b, ia, AINT);
            put64(&mut b, ia + 24, 8);
            put32(&mut b, ia + 32, 79);
            put32(&mut b, ia + 36, 4);
            put64(&mut b, T1 + 72, FOLLOW + 1);
            put64(&mut b, T1 + 80, FOLLOW + 1);
            put64(&mut b, TOUCHES + 16, ta);
            put64(&mut b, IDS + 16, ia);
            put32(&mut b, TOUCHES + 24, 2);
            put32(&mut b, IDS + 24, 2);
            put32(&mut b, TOUCHES + 28, 2);
            put32(&mut b, IDS + 28, 2);
        }
        let mut consumed = vec![];
        let v = p.sample(&r, FOLLOW, CANCEL, &mut consumed).unwrap();
        assert!(v.present);
        assert_eq!(v.native_id, 79);
        assert_eq!(v.contact_owner_id, T0);
        assert!(check(&r, &consumed).is_ok());
    }
    #[test]
    fn rejects_nonfinite_positions_flags_foreign_array_types_and_unbounded_counts() {
        for v in [f32::NAN, f32::INFINITY, 1_000_001.] {
            let mut r = fixture();
            real(&mut r.memory.0.borrow_mut(), T0 + 16, v);
            assert!(sample(&mut r).is_err());
        }
        let mut r = fixture();
        r.memory.0.borrow_mut()[T0 as usize + 101] = 2;
        assert!(sample(&mut r).is_err());
        for (at, v) in [
            (TOUCH_ARRAY, AINT),
            (TOUCH_ARRAY + 24, 257),
            (TOUCH_ARRAY + 24, 0),
        ] {
            let mut r = fixture();
            let p = ContactRegistryPlan::discover(&mut r, UI).unwrap();
            put64(&mut r.memory.0.borrow_mut(), at, v);
            assert!(p.sample(&r, FOLLOW, CANCEL, &mut vec![]).is_err());
        }
        let mut r = fixture();
        put32(&mut r.memory.0.borrow_mut(), TOUCHES + 24, 17);
        assert!(sample(&mut r).is_err());
    }
    fn camera_fixture() -> Reflection<Fake> {
        let r = fixture();
        {
            let mut b = r.memory.0.borrow_mut();
            let cam_native = 0x10000;
            let go_native = 0x10100;
            let ui_native = 0x10200;
            let table = 0x10300;
            let ui_object = 0x10400;
            put64(&mut b, CAMERA, CAM_CLASS);
            put64(&mut b, CAMERA + 16, cam_native);
            put64(&mut b, cam_native + 40, CAMERA);
            put64(&mut b, cam_native + 48, go_native);
            put64(&mut b, FOLLOW, GO_CLASS);
            put64(&mut b, FOLLOW + 16, go_native);
            put64(&mut b, go_native + 40, FOLLOW);
            put64(&mut b, go_native + 48, table);
            put64(&mut b, go_native + 64, 2);
            put64(&mut b, table + 8, cam_native);
            put64(&mut b, table + 24, ui_native);
            put64(&mut b, ui_native + 40, ui_object);
            put64(&mut b, ui_native + 48, go_native);
            put64(&mut b, ui_object, UI);
            put64(&mut b, ui_object + 16, ui_native);
            put64(&mut b, ui_object + 128, CAMERA);
        }
        r
    }
    #[test]
    fn cold_camera_discovery_requires_exact_reciprocal_component_and_camera_link() {
        let mut r = camera_fixture();
        assert_eq!(discover_ui_camera_class(&mut r, CAMERA).unwrap(), UI);
        for (at, value) in [
            (CAMERA + 16, 0),
            (0x10000 + 40, CAMERA + 1),
            (0x10000 + 48, 0),
            (0x10100 + 64, 33),
            (0x10200 + 48, 0),
            (0x10400 + 16, 0),
            (0x10400 + 128, CAMERA + 1),
        ] {
            let mut r = camera_fixture();
            put64(&mut r.memory.0.borrow_mut(), at, value);
            assert!(discover_ui_camera_class(&mut r, CAMERA).is_err(), "{at:#x}");
        }
        let mut r = camera_fixture();
        put64(&mut r.memory.0.borrow_mut(), 0x10100 + 64, 1);
        assert!(discover_ui_camera_class(&mut r, CAMERA).is_err());
        let mut r = camera_fixture();
        {
            let mut b = r.memory.0.borrow_mut();
            put64(&mut b, 0x10100 + 64, 3);
            put64(&mut b, 0x10300 + 40, 0x10200);
        }
        assert!(discover_ui_camera_class(&mut r, CAMERA).is_err());
    }
}
