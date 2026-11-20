//! Locations of the game's own chat buttons and input fields.
use crate::{
    account_chooser::named,
    platform::Memory,
    reflection::{Reflection, Result},
    unity::{Widget, matrices, onscreen, ui_matrix_offset},
};

#[derive(Clone, Copy, Debug)]
pub struct Target {
    pub owner: u64,
    pub site: u32,
    pub channel: i32,
    pub editing: bool,
    pub point: [f64; 2],
}

impl Target {
    pub fn encode(self) -> String {
        let [x, y] = self.point.map(|v| (v * 1e6).round() as u32);
        format!(
            "{} {} {} {} {x} {y}",
            self.owner,
            self.site,
            self.channel,
            u32::from(self.editing)
        )
    }
}

fn point<M: Memory>(r: &mut Reflection<M>, obj: u64) -> Result<[f64; 2]> {
    let widget = Widget::new(r, obj)?;
    if !widget.visible(r)? {
        return Err("Chat control hidden".into());
    }
    let camera = r.link(obj, "mCam")?;
    let native = r.link(camera, "m_CachedPtr")?;
    let bytes = r.read(native, 4096)?;
    let offset = ui_matrix_offset(&bytes)?;
    let (view, projection) = matrices(&bytes[offset..offset + 128])?;
    let (p, _) = widget.geometry(r, &view, &projection, true)?;
    if !onscreen(p) {
        return Err("Chat control outside the game surface".into());
    }
    Ok(p)
}

fn integer<M: Memory>(r: &mut Reflection<M>, obj: u64, field: &str) -> Option<i32> {
    let addr = r.field(obj, field).ok()?;
    r.integer(addr).ok()
}

fn lobby_bar<M: Memory>(r: &mut Reflection<M>, frame: u64) -> Option<[f64; 2]> {
    let root = r.link(frame, "panel").ok()?;
    let panels = r.link(root, "list").and_then(|list| r.list(list)).ok()?;
    for panel in panels.into_iter().filter(|p| *p != 0) {
        let mut parent = panel;
        for _ in 0..8 {
            if parent == root || parent == 0 {
                break;
            }
            parent = r.link(parent, "mParentPanel").unwrap_or(0);
        }
        if parent != root {
            continue;
        }
        let Ok(rows) = r
            .link(panel, "widgets")
            .and_then(|w| r.link(w, "m_InnerList"))
            .and_then(|list| r.list(list))
        else {
            continue;
        };
        let channel = rows
            .iter()
            .copied()
            .filter(|&w| w != 0)
            .find(|&w| named(r, w, "mSpriteName", "ChatChannel_team"));
        let Some([cx, cy]) = channel.and_then(|w| point(r, w).ok()) else {
            continue;
        };
        for obj in rows.into_iter().filter(|w| *w != 0) {
            if named(r, obj, "mSpriteName", "com_pic_baifangkuai8px") {
                if let Ok([x, y]) = point(r, obj) {
                    // The chat bar is the background immediately to the right of
                    // the team's channel icon. Other room widgets share this sprite.
                    if x > cx && x - cx < 0.20 && (y - cy).abs() < 0.012 {
                        return Some([x, y]);
                    }
                }
            }
        }
    }
    None
}

pub fn locate<M: Memory>(r: &mut Reflection<M>, frames: &[u64], in_match: bool) -> Option<Target> {
    let top = frames.iter().rev().copied().find(|v| *v != 0)?;
    if in_match {
        // A full-screen modal, the scoreboard or hero selection must own its input.
        let top_class = r.class_name(r.pointer(top).ok()?).ok()?;
        if !matches!(
            top_class,
            "UIBattleInfo"
                | "UIBattleInfoTips"
                | "UITrainGM"
                | "UIMiniMap"
                | "UIMiniMapToolButton"
                | "UIBattleSpecialEffPanelNew"
                | "UIJoyStick"
        ) {
            return None;
        }
        let owner = frames
            .iter()
            .rev()
            .copied()
            .filter(|p| *p != 0)
            .find(|&p| named(r, p, "name", "UI_BattleInfo"))?;
        let chat = r.link(owner, "uiBattleChatInfo").ok()?;
        let ui = r.link(chat, "m_UIFrameInfo").ok()?;
        let field = r.link(ui, "m_zLabel_InputChat").ok()?;
        let (editing, point) = if let Ok(p) = point(r, field) {
            (true, p)
        } else {
            let icon = r.link(ui, "m_sprite_SendMessage1").ok()?;
            (false, point(r, icon).ok()?)
        };
        return Some(Target {
            owner,
            site: 1,
            channel: integer(r, chat, "m_eCurrentMessageTab").unwrap_or(0),
            editing,
            point,
        });
    }
    let open = named(r, top, "name", "UI_Chat");
    if !open && !named(r, top, "name", "UI_MatchRoom") {
        return None;
    }
    let owner = frames
        .iter()
        .rev()
        .copied()
        .filter(|p| *p != 0)
        .find(|&p| named(r, p, "name", "UI_MatchRoom"))?;
    if !open {
        return Some(Target {
            owner,
            site: 2,
            channel: 0,
            editing: false,
            point: lobby_bar(r, owner)?,
        });
    }
    let input = r.link(top, "localInput").ok()?;
    let label = r.link(input, "label").ok()?;
    Some(Target {
        owner,
        site: 2,
        channel: integer(r, top, "_chosenTabType").unwrap_or(0),
        editing: true,
        point: point(r, label).ok()?,
    })
}
