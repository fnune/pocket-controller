use super::*;
use pocket_controller::control::{DebugView, UsbLink};
use pocket_controller::xinput::EndpointBudget;

pub(super) const DEBUG_TITLE_Y: i32 = 44;
pub(super) const KEY_LEFT: i32 = 40;
pub(super) const VALUE_LEFT: i32 = 200;
pub(super) const DEBUG_ROWS_TOP: i32 = 88;
pub(super) const DEBUG_ROW_PITCH: i32 = 44;
pub(super) const DIVIDER_WIDTH: i32 = 440;
pub(super) const DIVIDER_GAP: i32 = 16;
pub(super) const DRAWING_SHIFT: i32 = 250;
pub(super) const DEBUG_HINT: &str = "Buttons still reach the computer";

const BUTTON_NAMES: [(u16, &str); 15] = [
    (xinput::A, "A"),
    (xinput::B, "B"),
    (xinput::X, "X"),
    (xinput::Y, "Y"),
    (xinput::LEFT_SHOULDER, "LB"),
    (xinput::RIGHT_SHOULDER, "RB"),
    (xinput::BACK, "Back"),
    (xinput::START, "Start"),
    (xinput::GUIDE, "Guide"),
    (xinput::LEFT_THUMB, "LS"),
    (xinput::RIGHT_THUMB, "RS"),
    (xinput::DPAD_UP, "Up"),
    (xinput::DPAD_DOWN, "Down"),
    (xinput::DPAD_LEFT, "Left"),
    (xinput::DPAD_RIGHT, "Right"),
];

fn link_text(link: UsbLink) -> &'static str {
    match link {
        UsbLink::Unknown => "Unknown",
        UsbLink::NoPower => "No power",
        UsbLink::Powered => "Powered, no computer",
        UsbLink::Enumerating => "Setting up",
        UsbLink::Configured => "Configured",
    }
}

fn endpoints_text(view: &DebugView) -> &'static str {
    match (view.kind, view.budget) {
        (Kind::Gamepad, _) => "Interrupt IN",
        (Kind::Xbox, Some(EndpointBudget::TwoInterrupt)) => "Interrupt IN and OUT",
        (Kind::Xbox, Some(EndpointBudget::OneInterrupt)) => "Interrupt IN, bulk OUT",
        (Kind::Xbox, None) => "Not set up yet",
    }
}

pub(super) fn button_names(buttons: u16) -> String {
    let names: Vec<&str> = BUTTON_NAMES
        .iter()
        .filter(|(bit, _)| buttons & bit != 0)
        .map(|(_, name)| *name)
        .collect();
    if names.is_empty() {
        "None".to_string()
    } else {
        names.join("  ")
    }
}

type DebugRows = Vec<(&'static str, String)>;

pub(super) fn debug_rows(view: &DebugView) -> (DebugRows, DebugRows) {
    let kind = match (view.connected, view.kind) {
        (false, _) => "Not connected",
        (true, Kind::Xbox) => "Xbox-style",
        (true, Kind::Gamepad) => "Generic",
    };
    let driver = match (view.kind, view.xbox_driver) {
        (Kind::Gamepad, _) => "Not used",
        (Kind::Xbox, true) => "Seen",
        (Kind::Xbox, false) => "Not seen",
    };
    let sent = view.sent;
    (
        vec![
            ("USB link", link_text(view.link).to_string()),
            ("Connected as", kind.to_string()),
            ("Endpoints", endpoints_text(view).to_string()),
            ("Xbox driver", driver.to_string()),
            ("Last rumble", format!("{} of 255", view.rumble)),
        ],
        vec![
            ("Buttons", button_names(sent.buttons)),
            (
                "Triggers",
                format!("LT {}   RT {}", sent.left_trigger, sent.right_trigger),
            ),
            (
                "Left stick",
                format!("X {}   Y {}", sent.left_x, sent.left_y),
            ),
            (
                "Right stick",
                format!("X {}   Y {}", sent.right_x, sent.right_y),
            ),
        ],
    )
}

pub(super) fn debug_screen(
    canvas: &mut Canvas,
    device: &Device,
    view: &DebugView,
    live: &LiveView,
    hold: Option<u8>,
) {
    handheld(canvas, device, live, false, DRAWING_SHIFT);
    canvas.text(
        Role::Small,
        "Debug",
        KEY_LEFT,
        DEBUG_TITLE_Y,
        Align::Left,
        Paint::Text,
    );
    let (link, sent) = debug_rows(view);
    let mut y = DEBUG_ROWS_TOP;
    let row = |canvas: &mut Canvas, key: &str, value: &str, y: i32| {
        canvas.text(Role::Value, key, KEY_LEFT, y, Align::Left, Paint::Muted);
        canvas.text(Role::Detail, value, VALUE_LEFT, y, Align::Left, Paint::Text);
    };
    for (key, value) in &link {
        row(canvas, key, value, y);
        y += DEBUG_ROW_PITCH;
    }
    let divider = y - DEBUG_ROW_PITCH / 2 + DIVIDER_GAP / 2;
    canvas.fill(KEY_LEFT, divider, DIVIDER_WIDTH, 2, Paint::Track);
    y += DIVIDER_GAP;
    for (key, value) in &sent {
        row(canvas, key, value, y);
        y += DEBUG_ROW_PITCH;
    }
    match hold {
        Some(progress) => hold_bar(canvas, progress),
        None => {
            canvas.text(
                Role::Value,
                DEBUG_HINT,
                HINT_LEFT,
                HINTS_Y,
                Align::Left,
                Paint::Muted,
            );
            combo_hint(canvas, "btn-menu", "btn-start", "Close");
        }
    }
}
