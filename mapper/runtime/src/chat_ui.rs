//! Locations of the battle chat buttons and input field.
use crate::{
    optional_ui::button_widget,
    platform::Memory,
    reflection::{Reflection, Result},
    unity::{Widget, matrices, onscreen, ui_matrix_offset},
};

const DEFAULT_LIMIT: i32 = 60;

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
            "{} 1 {} {} {x} {y} {} {}",
            self.owner,
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

fn battle_hud(class: &str) -> bool {
    matches!(class, "UIBattleInfo" | "UIBattleInfo_EGC")
}

pub fn locate<M: Memory>(r: &mut Reflection<M>, frames: &[u64]) -> Option<Target> {
    let top = frames.iter().rev().copied().find(|v| *v != 0)?;
    // A full-screen modal, the scoreboard or hero selection must own its input.
    let top_class = r.class_name(r.pointer(top).ok()?).ok()?;
    if !battle_hud(top_class)
        && !matches!(
            top_class,
            "UIBattleInfoTips"
                | "UITrainGM"
                | "UIMiniMap"
                | "UIMiniMap_EGC"
                | "UIMiniMapToolButton"
                | "UIBattleSpecialEffPanelNew"
                | "UIJoyStick"
        )
    {
        return None;
    }
    let owner = frames
        .iter()
        .rev()
        .copied()
        .filter(|p| *p != 0)
        .find(|&p| {
            r.pointer(p)
                .ok()
                .and_then(|class| r.class_name(class).ok())
                .is_some_and(battle_hud)
        })?;
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
    Some(Target {
        owner,
        channel: channel.unwrap_or(0),
        editing,
        point,
        limit: configured_limit(limit).unwrap_or(DEFAULT_LIMIT),
        send_state: battle_send_state(r, chat, channel),
    })
}
