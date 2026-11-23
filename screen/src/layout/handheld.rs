use super::*;

pub(super) const PLAIN_CONTROLS: [(u16, &str); 9] = [
    (xinput::DPAD_UP, "press-dpad-up"),
    (xinput::DPAD_DOWN, "press-dpad-down"),
    (xinput::DPAD_LEFT, "press-dpad-left"),
    (xinput::DPAD_RIGHT, "press-dpad-right"),
    (xinput::GUIDE, "press-menu"),
    (xinput::BACK, "press-select"),
    (xinput::START, "press-start"),
    (xinput::LEFT_THUMB, "press-l3"),
    (xinput::RIGHT_THUMB, "press-r3"),
];

pub(super) const DPAD_BUTTONS: u16 =
    xinput::DPAD_UP | xinput::DPAD_DOWN | xinput::DPAD_LEFT | xinput::DPAD_RIGHT;
pub(super) const DIAGONAL_PER_MILLE: i32 = 707;
pub(super) const CHOICE_GAP: i32 = 24;
pub(super) const CHOICE_HALF: i32 = 8;

pub fn anchor_text(anchor: &str) -> Option<(&'static str, Role, Paint)> {
    if let Some(shortcut) = SHORTCUTS.iter().find(|shortcut| shortcut.anchor == anchor) {
        return Some((shortcut.label, Role::SheetLabel, Paint::Text));
    }
    let caption = |text| Some((text, Role::Caption, Paint::Muted));
    match anchor {
        "label-menu" => Some(("Hold", Role::SheetLabel, Paint::Text)),
        "caption-dpad-mode" => caption("D-pad acts as"),
        "caption-l3" => caption("L3"),
        "caption-r3" => caption("R3"),
        _ => None,
    }
}

pub(super) fn dpad_direction(buttons: u16, negative: u16, positive: u16) -> i32 {
    i32::from(buttons & positive != 0) - i32::from(buttons & negative != 0)
}

pub(super) fn thumb_offset(buttons: u16, travel: i32) -> (i32, i32) {
    let x = dpad_direction(buttons, xinput::DPAD_LEFT, xinput::DPAD_RIGHT);
    let y = dpad_direction(buttons, xinput::DPAD_DOWN, xinput::DPAD_UP);
    let reach = if x != 0 && y != 0 {
        travel * DIAGONAL_PER_MILLE / 1000
    } else {
        travel
    };
    (x * reach, -y * reach)
}

impl Canvas<'_> {
    fn layer(&mut self, device: &Device, name: &str, shift: i32) {
        if let Some((x, y)) = device.image(name) {
            self.image(name, x + shift, y);
        }
    }
}

pub(super) fn handheld(
    canvas: &mut Canvas,
    device: &Device,
    live: &LiveView,
    labelled: bool,
    shift: i32,
) {
    canvas.layer(device, "body", shift);
    let analog = live.dpad != DpadMode::Dpad;
    for (bit, name) in PLAIN_CONTROLS {
        if live.buttons & bit != 0 && !(analog && bit & DPAD_BUTTONS != 0) {
            canvas.layer(device, name, shift);
        }
    }
    for (on, name) in [
        (live.volume_up, "press-volume-up"),
        (live.volume_down, "press-volume-down"),
        (live.fn_switch, "fn-on"),
    ] {
        if on {
            canvas.layer(device, name, shift);
        }
    }
    for shortcut in turbo_shortcuts() {
        let pressed = shortcut
            .key
            .active(live.buttons, live.left_trigger, live.right_trigger);
        let turbo = shortcut.key.active(
            live.turbo.buttons,
            live.turbo.left_trigger,
            live.turbo.right_trigger,
        );
        let state = match (pressed, turbo) {
            (false, false) => continue,
            (true, false) => "press",
            (false, true) => "turbo",
            (true, true) => "turbo-press",
        };
        canvas.layer(
            device,
            &format!("{state}-{}", shortcut.printed.to_lowercase()),
            shift,
        );
    }
    let badge = match live.dpad {
        DpadMode::Dpad => None,
        DpadMode::LeftStick => Some("stick-well-l"),
        DpadMode::RightStick => Some("stick-well-r"),
    };
    if let (Some(badge), Some(stick), Some((thumb_x, thumb_y))) =
        (badge, device.stick("dpad"), device.image("stick-thumb"))
    {
        canvas.layer(device, "stick-well", shift);
        canvas.layer(device, badge, shift);
        let (dx, dy) = thumb_offset(live.buttons, stick.travel);
        canvas.image("stick-thumb", thumb_x + dx + shift, thumb_y + dy);
    }
    if labelled {
        canvas.layer(device, "leaders", shift);
        let choice = for_dpad_mode(live.dpad);
        if let Some(row) = device.anchor(choice.anchor) {
            let label_width = canvas.design_width_of(Role::SheetLabel, choice.label);
            canvas.image(
                "choice",
                row.x - label_width - CHOICE_GAP,
                row.y - CHOICE_HALF,
            );
        }
        for (name, anchor) in device.anchors() {
            if let Some((text, role, paint)) = anchor_text(name) {
                canvas.text(role, text, anchor.x, anchor.y, anchor.align, paint);
            }
        }
    }
}
