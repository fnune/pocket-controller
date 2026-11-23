use super::*;

pub(super) const PLUS_SLOT: i32 = 35;
pub(super) const PILL_TEXT_GAP: i32 = 14;

pub(super) const HINT_LEFT: i32 = 40;
pub(super) const HINT_RIGHT: i32 = 984;

pub(super) fn quit_hint(canvas: &mut Canvas) {
    combo_hint(canvas, "btn-select", "btn-start", QUIT_HINT);
}

pub(super) fn combo_hint(canvas: &mut Canvas, first: &str, second: &str, text: &str) {
    let (pill_width, pill_height) = BUTTON_PILL;
    let text_width = canvas.design_width_of(Role::Hint, text);
    let total = pill_width + PLUS_SLOT + pill_width + PILL_TEXT_GAP + text_width;
    let mut x = HINT_RIGHT - total;
    canvas.image(first, x, HINTS_Y - pill_height / 2);
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
    canvas.image(second, x, HINTS_Y - pill_height / 2);
    x += pill_width + PILL_TEXT_GAP;
    canvas.text(Role::Hint, text, x, HINTS_Y, Align::Left, Paint::Text);
}

pub(super) fn pill_hint(canvas: &mut Canvas, pill: &str, text: &str, x: i32) -> i32 {
    let (pill_width, pill_height) = BUTTON_PILL;
    canvas.image(pill, x, HINTS_Y - pill_height / 2);
    let text_left = x + pill_width + PILL_TEXT_GAP;
    canvas.text(
        Role::Hint,
        text,
        text_left,
        HINTS_Y,
        Align::Left,
        Paint::Text,
    );
    text_left + canvas.design_width_of(Role::Hint, text)
}

pub(super) const HINT_SPACING: i32 = 32;

pub(super) fn settings_hints(canvas: &mut Canvas, on_debug: bool) {
    let x = pill_hint(canvas, "btn-updown", "Move", HINT_LEFT) + HINT_SPACING;
    if on_debug {
        hint(canvas, "A", "OPEN", x, HINTS_Y);
    } else {
        pill_hint(canvas, "btn-leftright", "Change", x);
    }
    hint_right(canvas, "B", "CLOSE", HINTS_Y);
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

pub(super) const HINT_RADIUS: i32 = 23;
pub(super) const HINT_LABEL_GAP: i32 = 12;

pub(super) fn hint_right(canvas: &mut Canvas, letter: &str, label: &str, y: i32) {
    let width = 2 * HINT_RADIUS + HINT_LABEL_GAP + canvas.design_width_of(Role::Hint, label);
    hint(canvas, letter, label, HINT_RIGHT - width, y);
}

pub(super) fn hint(canvas: &mut Canvas, letter: &str, label: &str, x: i32, y: i32) {
    let radius = HINT_RADIUS;
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
        x + 2 * radius + HINT_LABEL_GAP,
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
