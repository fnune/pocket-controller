use super::*;

pub(super) fn waiting_lines(diagnosis: Diagnosis) -> (&'static str, &'static str) {
    match diagnosis {
        Diagnosis::None => (
            "Not connected",
            "Plug the bottom USB-C port into a USB-A port",
        ),
        Diagnosis::NoComputer => (
            "Connected, but not recognised",
            "Try a cable that carries data, or another USB-A port",
        ),
        Diagnosis::SetupStuck => ("USB setup did not finish", "Try another USB-A port"),
    }
}

pub(super) fn mode_chip(mode: Mode) -> (&'static str, &'static str) {
    match mode {
        Mode::Automatic => ("mode-auto", "Automatic"),
        Mode::Xbox => ("mode-xbox", "Xbox-style"),
        Mode::Gamepad => ("mode-gamepad", "Generic"),
    }
}

pub(super) const CHIP_TOP: i32 = 30;
pub(super) const CHIP_HEIGHT: i32 = 40;
pub(super) const CHIP_GLYPH: i32 = 28;
pub(super) const CHIP_LEFT_PADDING: i32 = 14;
pub(super) const CHIP_GLYPH_GAP: i32 = 8;
pub(super) const CHIP_RIGHT_PADDING: i32 = 18;
pub(super) const CHIP_RIGHT_EDGE: i32 = 984;

pub(super) fn chip(canvas: &mut Canvas, mode: Mode) {
    let (glyph, label) = mode_chip(mode);
    let label_width = canvas.design_width_of(Role::Small, label);
    let width = CHIP_LEFT_PADDING + CHIP_GLYPH + CHIP_GLYPH_GAP + label_width + CHIP_RIGHT_PADDING;
    let left = CHIP_RIGHT_EDGE - width;
    canvas.rounded(
        left,
        CHIP_TOP,
        width,
        CHIP_HEIGHT,
        CHIP_HEIGHT / 2,
        Paint::Track,
    );
    let middle = CHIP_TOP + CHIP_HEIGHT / 2;
    canvas.image(glyph, left + CHIP_LEFT_PADDING, middle - CHIP_GLYPH / 2);
    let label_left = left + CHIP_LEFT_PADDING + CHIP_GLYPH + CHIP_GLYPH_GAP;
    canvas.text(
        Role::Small,
        label,
        label_left,
        middle,
        Align::Left,
        Paint::Text,
    );
}

pub(super) const ICON_SIZE: i32 = 48;
pub(super) const ICON_GAP: i32 = 16;
pub(super) const DOT_RADIUS: i32 = 7;
pub(super) const DOT_SPACING: i32 = 22;

pub(super) fn status_icon(status: Status) -> &'static str {
    match status {
        Status::Waiting => "icon-waiting",
        Status::Warning => "icon-warning",
    }
}

pub(super) fn status_line(
    canvas: &mut Canvas,
    status: Status,
    headline: &str,
    cy: i32,
    busy: Option<u8>,
) {
    let text_width = canvas.design_width_of(Role::Headline, headline);
    let left = (DESIGN_WIDTH - (ICON_SIZE + ICON_GAP + text_width)) / 2;
    canvas.image(status_icon(status), left, cy - ICON_SIZE / 2);
    let text_left = left + ICON_SIZE + ICON_GAP;
    canvas.text(
        Role::Headline,
        headline,
        text_left,
        cy,
        Align::Left,
        Paint::Text,
    );
    if let Some(phase) = busy {
        let first = text_left + text_width + ICON_GAP + DOT_RADIUS;
        for index in 0..3u8 {
            let paint = if phase % 3 == index {
                Paint::Accent
            } else {
                Paint::Muted
            };
            canvas.circle(
                first + i32::from(index) * DOT_SPACING,
                cy,
                DOT_RADIUS,
                paint,
            );
        }
    }
}

pub(super) fn link_image(diagnosis: Diagnosis) -> &'static str {
    match diagnosis {
        Diagnosis::None => "link-unplugged",
        Diagnosis::NoComputer => "link-unrecognised",
        Diagnosis::SetupStuck => "link-stuck",
    }
}

pub(super) const LINK_POSITION: (i32, i32) = (192, 130);
pub(super) const WAITING_HEADLINE_Y: i32 = 430;
pub(super) const WAITING_INSTRUCTION_Y: i32 = 484;
