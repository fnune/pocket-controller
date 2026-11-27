use super::*;
use pocket_controller::layer::FaceLayout;
use pocket_controller::settings::{
    DPAD_VALUES, FACE_VALUES, MODE_CHANGE_WARNING, MODE_VALUES, RUMBLE_UNAVAILABLE,
    RUMBLE_UNAVAILABLE_DETAIL, RUMBLE_VALUES, Row, SHOULDER_VALUES, ScreenWhilePlaying,
    SettingsView, dpad_detail, dpad_label, face_detail, face_label, mode_detail, rumble_available,
    rumble_detail, rumble_label, screen_detail, shoulder_detail, shoulder_label,
};

pub(super) const LIST_LEFT: i32 = 40;
pub(super) const LIST_WIDTH: i32 = 392;
pub(super) const ROWS_TOP: i32 = 72;
pub(super) const ROW_HEIGHT: i32 = 76;
pub(super) const ROW_PITCH: i32 = 84;
pub(super) const ROW_TEXT_LEFT: i32 = LIST_LEFT + 28;
pub(super) const ROW_LABEL_Y: i32 = 25;
pub(super) const ROW_VALUE_Y: i32 = 54;
pub(super) const VERSION_Y: i32 = ROWS_TOP + Row::ALL.len() as i32 * ROW_PITCH + 28;

pub(super) const PANEL_LEFT: i32 = 456;
pub(super) const PANEL_TOP: i32 = 72;
pub(super) const PANEL_WIDTH: i32 = 528;
pub(super) const PANEL_HEIGHT: i32 = Row::ALL.len() as i32 * ROW_PITCH - (ROW_PITCH - ROW_HEIGHT);
pub(super) const PANEL_PADDING: i32 = 32;
pub(super) const PANEL_TEXT_WIDTH: i32 = PANEL_WIDTH - 2 * PANEL_PADDING;
pub(super) const CARDS_TOP: i32 = PANEL_TOP + PANEL_PADDING;
pub(super) const CARD_GAP: i32 = 16;
pub(super) const CARD_HEIGHT_WITH_GLYPH: i32 = 116;
pub(super) const CARD_HEIGHT: i32 = 72;
pub(super) const CARD_GLYPH: i32 = 48;
pub(super) const CARD_RADIUS: i32 = 20;
pub(super) const CHEVRON: (i32, i32) = (16, 24);
pub(super) const DETAIL_GAP: i32 = 56;
pub(super) const DETAIL_LINE_HEIGHT: i32 = 28;
pub(super) const WARNING_GAP: i32 = 16;
pub(super) const EXTRA_GAP: i32 = 20;
pub(super) const FACE_DIAGRAM: i32 = 140;
pub(super) const FACE_CAPTION: &str = "What the computer receives";
pub(super) const KEY_PILL: (i32, i32) = (52, 32);
pub(super) const KEY_TEXT_GAP: i32 = 10;

pub(super) fn mode_glyph(mode: Mode, selected: bool) -> &'static str {
    match (mode, selected) {
        (Mode::Automatic, true) => "mode-auto-48-on",
        (Mode::Automatic, false) => "mode-auto-48-off",
        (Mode::Xbox, true) => "mode-xbox-48-on",
        (Mode::Xbox, false) => "mode-xbox-48-off",
        (Mode::Gamepad, true) => "mode-gamepad-48-on",
        (Mode::Gamepad, false) => "mode-gamepad-48-off",
    }
}

pub(super) fn wrap_to_width(
    canvas: &Canvas,
    role: Role,
    text: &str,
    max_width: i32,
) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(last) if canvas.design_width_of(role, &format!("{last} {word}")) <= max_width => {
                last.push(' ');
                last.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines
}

pub(super) fn row_label(row: Row) -> &'static str {
    match row {
        Row::Face => "Face buttons",
        Row::Shoulders => "Shoulder buttons",
        Row::Dpad => "D-pad",
        Row::Rumble => "Rumble",
        Row::Screen => "Screen while playing",
        Row::Mode => "Mode",
    }
}

fn row_value(view: &SettingsView, row: Row) -> (&'static str, bool) {
    match row {
        Row::Face => (face_label(view.settings.face), true),
        Row::Shoulders => (shoulder_label(view.settings.shoulders), true),
        Row::Dpad => (dpad_label(view.settings.dpad), true),
        Row::Rumble if !rumble_available(view.kind) => (RUMBLE_UNAVAILABLE, false),
        Row::Rumble => (rumble_label(view.settings.rumble), true),
        Row::Screen => (view.settings.screen.label(), true),
        Row::Mode => (view.mode.label(), true),
    }
}

pub(super) struct Choice {
    pub label: &'static str,
    pub glyph: Option<&'static str>,
    pub selected: bool,
}

pub(super) fn choices(view: &SettingsView) -> Vec<Choice> {
    let plain = |label, selected| Choice {
        label,
        glyph: None,
        selected,
    };
    match view.cursor {
        Row::Face => FACE_VALUES
            .iter()
            .map(|face| plain(face_label(*face), *face == view.settings.face))
            .collect(),
        Row::Shoulders => SHOULDER_VALUES
            .iter()
            .map(|shoulders| {
                plain(
                    shoulder_label(*shoulders),
                    *shoulders == view.settings.shoulders,
                )
            })
            .collect(),
        Row::Dpad => DPAD_VALUES
            .iter()
            .map(|dpad| plain(dpad_label(*dpad), *dpad == view.settings.dpad))
            .collect(),
        Row::Rumble if rumble_available(view.kind) => RUMBLE_VALUES
            .iter()
            .map(|on| plain(rumble_label(*on), *on == view.settings.rumble))
            .collect(),
        Row::Screen => ScreenWhilePlaying::ALL
            .iter()
            .map(|screen| plain(screen.label(), *screen == view.settings.screen))
            .collect(),
        Row::Mode => MODE_VALUES
            .iter()
            .map(|mode| Choice {
                label: mode.label(),
                glyph: Some(mode_glyph(*mode, *mode == view.mode)),
                selected: *mode == view.mode,
            })
            .collect(),
        Row::Rumble => Vec::new(),
    }
}

pub(super) fn detail(view: &SettingsView) -> &'static str {
    match view.cursor {
        Row::Face => face_detail(view.settings.face),
        Row::Shoulders => shoulder_detail(view.settings.shoulders),
        Row::Dpad => dpad_detail(view.settings.dpad),
        Row::Rumble if !rumble_available(view.kind) => RUMBLE_UNAVAILABLE_DETAIL,
        Row::Rumble => rumble_detail(view.settings.rumble),
        Row::Screen => screen_detail(view.settings.screen),
        Row::Mode => mode_detail(view.mode),
    }
}

fn list(canvas: &mut Canvas, view: &SettingsView) {
    for (index, row) in Row::ALL.into_iter().enumerate() {
        let top = ROWS_TOP + index as i32 * ROW_PITCH;
        let on = row == view.cursor;
        let (value, available) = row_value(view, row);
        if on {
            canvas.rounded(LIST_LEFT, top, LIST_WIDTH, ROW_HEIGHT, 24, Paint::Text);
        }
        let (label_paint, value_paint) = match (on, available) {
            (true, _) => (Paint::Ink, Paint::InkMuted),
            (false, true) => (Paint::Text, Paint::Muted),
            (false, false) => (Paint::Faint, Paint::Faint),
        };
        canvas.text(
            Role::Small,
            row_label(row),
            ROW_TEXT_LEFT,
            top + ROW_LABEL_Y,
            Align::Left,
            label_paint,
        );
        canvas.text(
            Role::Value,
            value,
            ROW_TEXT_LEFT,
            top + ROW_VALUE_Y,
            Align::Left,
            value_paint,
        );
    }
    let version = format!("Pocket Controller {}", pocket_controller::VERSION);
    canvas.text(
        Role::Tag,
        &version,
        ROW_TEXT_LEFT,
        VERSION_Y,
        Align::Left,
        Paint::Faint,
    );
}

fn cards(canvas: &mut Canvas, choices: &[Choice]) -> i32 {
    let count = choices.len() as i32;
    let width = (PANEL_TEXT_WIDTH - (count - 1) * CARD_GAP) / count;
    let height = if choices.iter().any(|choice| choice.glyph.is_some()) {
        CARD_HEIGHT_WITH_GLYPH
    } else {
        CARD_HEIGHT
    };
    let left = PANEL_LEFT + PANEL_PADDING;
    let middle = CARDS_TOP + height / 2;
    let selected = choices.iter().position(|choice| choice.selected);
    let (chevron_width, chevron_height) = CHEVRON;
    if selected.is_some_and(|index| index > 0) {
        canvas.image(
            "chevron-left",
            left - PANEL_PADDING / 2 - chevron_width / 2,
            middle - chevron_height / 2,
        );
    }
    if selected.is_some_and(|index| index + 1 < choices.len()) {
        canvas.image(
            "chevron-right",
            left + PANEL_TEXT_WIDTH + PANEL_PADDING / 2 - chevron_width / 2,
            middle - chevron_height / 2,
        );
    }
    for (index, choice) in choices.iter().enumerate() {
        let x = left + index as i32 * (width + CARD_GAP);
        let (card, text) = if choice.selected {
            (Paint::Text, Paint::Ink)
        } else {
            (Paint::Card, Paint::Muted)
        };
        canvas.rounded(x, CARDS_TOP, width, height, CARD_RADIUS, card);
        let center = x + width / 2;
        let text_y = match choice.glyph {
            Some(glyph) => {
                canvas.image(glyph, center - CARD_GLYPH / 2, CARDS_TOP + 18);
                CARDS_TOP + 88
            }
            None => middle,
        };
        canvas.text(
            Role::Detail,
            choice.label,
            center,
            text_y,
            Align::Center,
            text,
        );
    }
    CARDS_TOP + height
}

fn panel(canvas: &mut Canvas, view: &SettingsView) {
    canvas.rounded(
        PANEL_LEFT,
        PANEL_TOP,
        PANEL_WIDTH,
        PANEL_HEIGHT,
        24,
        Paint::Track,
    );
    let choices = choices(view);
    let mut y = if choices.is_empty() {
        CARDS_TOP + DETAIL_LINE_HEIGHT / 2
    } else {
        cards(canvas, &choices) + DETAIL_GAP - DETAIL_LINE_HEIGHT / 2
    };
    let text_left = PANEL_LEFT + PANEL_PADDING;
    for line in wrap_to_width(canvas, Role::Detail, detail(view), PANEL_TEXT_WIDTH) {
        canvas.text(Role::Detail, &line, text_left, y, Align::Left, Paint::Text);
        y += DETAIL_LINE_HEIGHT;
    }
    match view.cursor {
        Row::Face => {
            let top = y - DETAIL_LINE_HEIGHT / 2 + EXTRA_GAP;
            let diagram = match view.settings.face {
                FaceLayout::Label => "face-nintendo",
                FaceLayout::Position => "face-xbox",
            };
            let center = PANEL_LEFT + PANEL_WIDTH / 2;
            canvas.image(diagram, center - FACE_DIAGRAM / 2, top);
            canvas.text(
                Role::Value,
                FACE_CAPTION,
                center,
                top + FACE_DIAGRAM + EXTRA_GAP,
                Align::Center,
                Paint::Muted,
            );
        }
        Row::Screen if view.settings.screen != ScreenWhilePlaying::AlwaysOn => {
            wake_keys(canvas, y + EXTRA_GAP);
        }
        _ => {}
    }
    if view.mode_change_warning() {
        y += WARNING_GAP;
        for line in wrap_to_width(canvas, Role::Detail, MODE_CHANGE_WARNING, PANEL_TEXT_WIDTH) {
            canvas.text(Role::Detail, &line, text_left, y, Align::Left, Paint::Muted);
            y += DETAIL_LINE_HEIGHT;
        }
    }
}

enum Piece {
    Words(&'static str),
    Key(&'static str),
}

const WAKE_HINT: [Piece; 4] = [
    Piece::Words("To wake it, hold"),
    Piece::Key("key-menu"),
    Piece::Words("or press"),
    Piece::Key("key-power"),
];

fn wake_keys(canvas: &mut Canvas, y: i32) {
    let (pill_width, pill_height) = KEY_PILL;
    let mut x = PANEL_LEFT + PANEL_PADDING;
    for piece in &WAKE_HINT {
        match piece {
            Piece::Words(words) => {
                canvas.text(Role::Detail, words, x, y, Align::Left, Paint::Text);
                x += canvas.design_width_of(Role::Detail, words) + KEY_TEXT_GAP;
            }
            Piece::Key(key) => {
                canvas.image(key, x, y - pill_height / 2);
                x += pill_width + KEY_TEXT_GAP;
            }
        }
    }
}

pub(super) fn settings_screen(canvas: &mut Canvas, view: &SettingsView, hold: Option<u8>) {
    list(canvas, view);
    panel(canvas, view);
    match hold {
        Some(progress) => hold_bar(canvas, progress),
        None => settings_hints(canvas),
    }
}
