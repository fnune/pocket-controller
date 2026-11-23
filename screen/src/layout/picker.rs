use super::*;

pub(super) fn mode_description(mode: Mode) -> &'static str {
    match mode {
        Mode::Automatic => "Xbox-style when the host supports it, otherwise Generic",
        Mode::Xbox => {
            "Emulates an Xbox 360 controller, recognised without setup on Linux and SteamOS"
        }
        Mode::Gamepad => {
            "A standard USB gamepad that works with more devices but may need button mapping"
        }
    }
}

pub(super) const PICKER_ROWS: [(Mode, i32); 3] = [
    (Mode::Automatic, 356),
    (Mode::Xbox, 448),
    (Mode::Gamepad, 540),
];
pub(super) const PICKER_LEFT: i32 = 160;
pub(super) const PICKER_WIDTH: i32 = 704;
pub(super) const PICKER_ROW_HEIGHT: i32 = 84;
pub(super) const PICKER_TEXT_LEFT: i32 = 236;
pub(super) const PICKER_TEXT_WIDTH: i32 = 608;
pub(super) const PICKER_LINE_HEIGHT: i32 = 21;
pub(super) const RECOMMENDED: &str = "Recommended";

pub(super) fn picker_glyph(mode: Mode, selected: bool) -> &'static str {
    match (mode, selected) {
        (Mode::Automatic, true) => "mode-auto-48-on",
        (Mode::Automatic, false) => "mode-auto-48-off",
        (Mode::Xbox, true) => "mode-xbox-48-on",
        (Mode::Xbox, false) => "mode-xbox-48-off",
        (Mode::Gamepad, true) => "mode-gamepad-48-on",
        (Mode::Gamepad, false) => "mode-gamepad-48-off",
    }
}

pub(super) fn split_to_width<'t>(
    canvas: &Canvas,
    role: Role,
    text: &'t str,
    max_width: i32,
) -> Vec<&'t str> {
    if canvas.design_width_of(role, text) <= max_width {
        return vec![text];
    }
    let breaks: Vec<usize> = text.match_indices(' ').map(|(index, _)| index).collect();
    let fits = |end: usize| canvas.design_width_of(role, &text[..end]) <= max_width;
    match breaks.iter().rev().find(|&&end| fits(end)) {
        Some(&end) => vec![&text[..end], &text[end + 1..]],
        None => vec![text],
    }
}

pub(super) fn picker(canvas: &mut Canvas, selected: Mode) {
    for (mode, top) in PICKER_ROWS {
        let chosen = mode == selected;
        if chosen {
            canvas.rounded(
                PICKER_LEFT,
                top,
                PICKER_WIDTH,
                PICKER_ROW_HEIGHT,
                24,
                Paint::Text,
            );
        }
        canvas.image(picker_glyph(mode, chosen), PICKER_LEFT + 16, top + 18);
        let (title_paint, line_paint) = if chosen {
            (Paint::Ink, Paint::InkMuted)
        } else {
            (Paint::Muted, Paint::Muted)
        };
        let lines = split_to_width(
            canvas,
            Role::PickerLine,
            mode_description(mode),
            PICKER_TEXT_WIDTH,
        );
        let title_y = if lines.len() > 1 { top + 20 } else { top + 28 };
        let title = mode.label();
        canvas.text(
            Role::PickerTitle,
            title,
            PICKER_TEXT_LEFT,
            title_y,
            Align::Left,
            title_paint,
        );
        if mode == Mode::Automatic {
            let tag_left = PICKER_TEXT_LEFT + canvas.design_width_of(Role::PickerTitle, title) + 14;
            let tag_width = canvas.design_width_of(Role::Tag, RECOMMENDED) + 24;
            canvas.rounded(tag_left, title_y - 14, tag_width, 28, 14, Paint::Accent);
            canvas.text(
                Role::Tag,
                RECOMMENDED,
                tag_left + 12,
                title_y,
                Align::Left,
                Paint::Text,
            );
        }
        let first_line_y = if lines.len() > 1 { top + 46 } else { top + 58 };
        for (index, line) in lines.into_iter().enumerate() {
            let y = first_line_y + index as i32 * PICKER_LINE_HEIGHT;
            canvas.text(
                Role::PickerLine,
                line,
                PICKER_TEXT_LEFT,
                y,
                Align::Left,
                line_paint,
            );
        }
    }
}
