use super::*;

pub(super) const PLUS_SLOT: i32 = 35;
pub(super) const PILL_TEXT_GAP: i32 = 14;

pub(super) const HINT_LEFT: i32 = 40;
pub(super) const HINT_RIGHT: i32 = 984;

pub(super) fn quit_hint(canvas: &mut Canvas) {
    let (pill_width, pill_height) = BUTTON_PILL;
    let text_width = canvas.design_width_of(Role::Hint, QUIT_HINT);
    let total = pill_width + PLUS_SLOT + pill_width + PILL_TEXT_GAP + text_width;
    let mut x = HINT_RIGHT - total;
    canvas.image("btn-select", x, HINTS_Y - pill_height / 2);
    x += pill_width;
    canvas.text(
        Role::Hint,
        "+",
        x + PLUS_SLOT / 2,
        HINTS_Y,
        Align::Center,
        Paint::Muted,
    );
    x += PLUS_SLOT;
    canvas.image("btn-start", x, HINTS_Y - pill_height / 2);
    x += pill_width + PILL_TEXT_GAP;
    canvas.text(Role::Hint, QUIT_HINT, x, HINTS_Y, Align::Left, Paint::Text);
}

pub(super) fn shortcuts_hint(canvas: &mut Canvas) {
    let (pill_width, pill_height) = BUTTON_PILL;
    canvas.image("btn-menu", HINT_LEFT, HINTS_Y - pill_height / 2);
    canvas.text(
        Role::Hint,
        SHORTCUTS_HINT,
        HINT_LEFT + pill_width + PILL_TEXT_GAP,
        HINTS_Y,
        Align::Left,
        Paint::Text,
    );
}

pub(super) fn hint(canvas: &mut Canvas, letter: &str, label: &str, x: i32, y: i32) {
    let radius = 23;
    canvas.circle(x + radius, y, radius, Paint::Accent);
    canvas.text(
        Role::Hint,
        letter,
        x + radius,
        y,
        Align::Center,
        Paint::Text,
    );
    canvas.text(
        Role::Hint,
        label,
        x + 2 * radius + 12,
        y,
        Align::Left,
        Paint::Text,
    );
}

pub(super) const HOLD_TEXT: &str = "Keep holding to quit";
pub(super) const HOLD_TEXT_GAP: i32 = 24;
pub(super) const HOLD_BAR_HEIGHT: i32 = 16;

pub(super) fn hold_bar(canvas: &mut Canvas, progress: u8) {
    canvas.text(
        Role::Hint,
        HOLD_TEXT,
        HINT_LEFT,
        HINTS_Y,
        Align::Left,
        Paint::Text,
    );
    let left = HINT_LEFT + canvas.design_width_of(Role::Hint, HOLD_TEXT) + HOLD_TEXT_GAP;
    let width = HINT_RIGHT - left;
    let top = HINTS_Y - HOLD_BAR_HEIGHT / 2;
    canvas.fill(left, top, width, HOLD_BAR_HEIGHT, Paint::Track);
    canvas.fill(
        left,
        top,
        width * i32::from(progress) / 100,
        HOLD_BAR_HEIGHT,
        Paint::Accent,
    );
}
