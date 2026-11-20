//! Locations of the game's own chat buttons and input fields.
use crate::{
    account_chooser::named,
    optional_ui::button_widget,
    platform::Memory,
    reflection::{Reflection, Result},
    unity::{Widget, matrices, onscreen, ui_matrix_offset},
};

const DEFAULT_LIMIT: i32 = 60;
const LOBBY_TEAM_CHANNEL: i32 = 2;

#[derive(Clone, Copy, Debug)]
#[repr(u32)]
enum SendState {
    Unknown,
    Pending,
    Ready,
}

#[derive(Clone, Copy, Debug)]
pub struct Target {
    pub owner: u64,
    pub site: u32,
    pub channel: i32,
    pub editing: bool,
    pub point: [f64; 2],
    pub limit: i32,
    send_state: SendState,
}

impl Target {
    pub fn encode(self) -> String {
        let [x, y] = self.point.map(|v| (v * 1e6).round() as u32);
        format!(
            "{} {} {} {} {x} {y} {} {}",
            self.owner,
            self.site,
            self.channel,
            u32::from(self.editing),
            self.limit,
            self.send_state as u32
        )
    }
}

fn point<M: Memory>(r: &mut Reflection<M>, obj: u64) -> Result<[f64; 2]> {
    let widget = Widget::new(r, obj)?;
    if !widget.visible(r)? {
        return Err("Chat control hidden".into());
    }
    project(r, &widget)
}

fn project<M: Memory>(r: &mut Reflection<M>, widget: &Widget) -> Result<[f64; 2]> {
    let obj = widget.object;
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

fn input_point<M: Memory>(r: &mut Reflection<M>, label: u64, send_button: u64) -> Option<[f64; 2]> {
    let mut checks = Vec::new();
    let sprite = button_widget(r, &mut checks, send_button).ok()?;
    let widget = Widget::new(r, sprite).ok()?;
    if !widget.visible(r).ok()? {
        return None;
    }
    // Textless labels have no draw call even while the input is clickable.
    let label = Widget::new(r, label).ok()?;
    let picked = project(r, &label).ok()?;
    crate::platform::matches_reads(&r.memory, &checks)
        .ok()?
        .then_some(picked)
}

fn integer<M: Memory>(r: &mut Reflection<M>, obj: u64, field: &str) -> Option<i32> {
    let addr = r.field(obj, field).ok()?;
    r.integer(addr).ok()
}

fn configured_limit(value: Option<i32>) -> Option<i32> {
    value.filter(|n| (1..=4096).contains(n))
}

fn lobby_send_state<M: Memory>(r: &mut Reflection<M>, chat: u64, channel: i32) -> SendState {
    // The room's Team tab identifies one recipient. Other tabs can have
    // conversations within a tab that this source does not identify.
    if channel != LOBBY_TEAM_CHANNEL {
        return SendState::Unknown;
    }
    let value = r
        .link(chat, "currentInputValue")
        .and_then(|p| if p == 0 { Ok(0) } else { r.integer(p + 16) });
    match value {
        Ok(0) => SendState::Ready,
        Ok(1..) => SendState::Pending,
        _ => SendState::Unknown,
    }
}

fn battle_send_state<M: Memory>(
    r: &mut Reflection<M>,
    chat: u64,
    channel: Option<i32>,
) -> SendState {
    if !matches!(channel, Some(1 | 2)) {
        return SendState::Unknown;
    }
    let cooldown = r
        .link(chat, "sendMsgCdCountDown")
        .and_then(|p| r.field(p, "m_CanCountCd"))
        .and_then(|addr| r.boolean(addr));
    match cooldown {
        Ok(false) => SendState::Ready,
        Ok(true) => SendState::Pending,
        Err(_) => SendState::Unknown,
    }
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
        let send = r.link(ui, "m_button_Send").ok()?;
        let (editing, point) = if let Some(p) = input_point(r, field, send) {
            (true, p)
        } else {
            let icon = r.link(ui, "m_sprite_SendMessage1").ok()?;
            (false, point(r, icon).ok()?)
        };
        let controller = r.link(chat, "m_sendChat").ok().filter(|&p| p != 0);
        let limit = controller.and_then(|p| integer(r, p, "chatMaxInputNum"));
        let channel = controller.and_then(|p| integer(r, p, "m_currentChannel"));
        return Some(Target {
            owner,
            site: 1,
            channel: channel.unwrap_or(0),
            editing,
            point,
            limit: configured_limit(limit).unwrap_or(DEFAULT_LIMIT),
            send_state: battle_send_state(r, chat, channel),
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
            limit: DEFAULT_LIMIT,
            send_state: SendState::Unknown,
        });
    }
    let input = r.link(top, "localInput").ok()?;
    let label = r.link(input, "label").ok()?;
    let ui = r.link(top, "m_FrameUI").ok()?;
    let send = r.link(ui, "m_Button_Send").ok()?;
    let chat_limit = configured_limit(integer(r, top, "maxInputSize")).unwrap_or(DEFAULT_LIMIT);
    let limit = configured_limit(integer(r, input, "characterLimit"))
        .map_or(chat_limit, |n| n.min(chat_limit));
    let channel = integer(r, top, "_chosenTabType").unwrap_or(0);
    Some(Target {
        owner,
        site: 2,
        channel,
        editing: true,
        point: input_point(r, label, send)?,
        limit,
        send_state: lobby_send_state(r, top, channel),
    })
}
