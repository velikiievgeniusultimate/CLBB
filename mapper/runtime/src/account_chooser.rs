//! Read the visible provider list so the host can cover it without touching game files.
use crate::{
    math,
    platform::Memory,
    reflection::{Reflection, Result, f32_at},
    unity::{Path, Widget, matrices, ui_matrix_offset},
};
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub struct Chooser {
    pub frame: u64,
    pub bounds: [f64; 4],
    pub moonton: Option<[f64; 2]>,
}

#[derive(Default)]
pub struct Reader {
    frames: Option<u64>,
    discovery: Option<Instant>,
}

pub(crate) fn named<M: Memory>(r: &mut Reflection<M>, obj: u64, field: &str, name: &str) -> bool {
    let Ok(s) = r.link(obj, field) else {
        return false;
    };
    if s == 0 || r.integer(s + 16).ok() != Some(name.len() as i32) {
        return false;
    }
    r.read(s + 20, name.len() * 2).is_ok_and(|bytes| {
        bytes
            .chunks_exact(2)
            .zip(name.bytes())
            .all(|(unit, ch)| unit == [ch, 0])
    })
}

impl Reader {
    pub(crate) fn active_frames<M: Memory>(&mut self, r: &mut Reflection<M>) -> Option<Vec<u64>> {
        if self.frames.is_none() {
            if self
                .discovery
                .is_some_and(|last| last.elapsed() < Duration::from_secs(3))
            {
                return None;
            }
            self.discovery = Some(Instant::now());
            r.discover(&["BaseFrame"]).ok()?;
            self.frames = r.static_field("BaseFrame", "m_ActiveFrameList").ok();
        }
        r.list(r.pointer(self.frames?).ok()?).ok()
    }

    pub fn locate<M: Memory>(r: &mut Reflection<M>, frames: &[u64]) -> Option<Chooser> {
        let frame = frames.iter().rev().copied().find(|&f| f != 0)?;
        if !named(r, frame, "name", "UI_AccountBinding") {
            return None;
        }
        let root = r.link(frame, "panel").ok()?;
        let panels = r.link(root, "list").ok()?;
        for panel in r.list(panels).ok()? {
            if panel == 0 || r.link(panel, "mParentPanel").ok() != Some(root) {
                continue;
            }
            let Some(rows) = r
                .link(panel, "widgets")
                .and_then(|w| r.link(w, "m_InnerList"))
                .and_then(|list| r.list(list))
                .ok()
            else {
                continue;
            };
            if let Some(icon) = rows
                .into_iter()
                .find(|&w| w != 0 && named(r, w, "mSpriteName", "Moonton"))
            {
                return geometry(r, frame, panel, icon).ok();
            }
        }
        None
    }
}

fn geometry<M: Memory>(
    r: &mut Reflection<M>,
    frame: u64,
    panel: u64,
    icon: u64,
) -> Result<Chooser> {
    let camera = r.link(panel, "mCam")?;
    if r.class_name(r.pointer(camera)?)? != "Camera" {
        return Err("Missing chooser camera".into());
    }
    let camera = r.link(camera, "m_CachedPtr")?;
    let block = r.read(camera, 4096)?;
    let offset = ui_matrix_offset(&block)?;
    let (view, projection) = matrices(&block[offset..offset + 128])?;

    // NGUI's clipping range stays fixed on screen when the list scrolls. Its offset
    // compensates for the panel transform; widget bounds alone include offscreen rows.
    let at = r.field(panel, "mClipRange")?;
    let range = r.read(at, 16)?;
    let at = r.field(panel, "mClipOffset")?;
    let scroll = r.read(at, 8)?;
    let transform = Path::new(r, panel, true)?.bytes(r)?;
    let corner = |dx: f64, dy: f64| {
        let local = [
            f32_at(&range, 0) + f32_at(&scroll, 0) + dx * f32_at(&range, 8) / 2.,
            f32_at(&range, 4) + f32_at(&scroll, 4) + dy * f32_at(&range, 12) / 2.,
            0.,
        ];
        math::trs_chain(&transform, Some(local))
            .and_then(|p| math::project(&view, &projection, p, false))
    };
    let [x1, y1] = corner(-1., 1.).ok_or("Missing chooser top edge")?;
    let [x2, y2] = corner(1., -1.).ok_or("Missing chooser bottom edge")?;
    if ![x1, y1, x2, y2].iter().all(|v| (0.0..=1.0).contains(v)) || x2 <= x1 || y2 <= y1 {
        return Err("Chooser is outside the viewport".into());
    }
    let widget = Widget::new(r, icon)?;
    let moonton = if widget.visible(r)? {
        let (p, _) = widget.geometry(r, &view, &projection, true)?;
        ((x1..x2).contains(&p[0]) && (y1..y2).contains(&p[1])).then_some(p)
    } else {
        None
    };
    Ok(Chooser {
        frame,
        bounds: [x1, y1, x2, y2],
        moonton,
    })
}
