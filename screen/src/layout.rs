use pocket_controller::control::{Diagnosis, Kind, LiveView, Mode, Screen};
use pocket_controller::frame::Frame;
use pocket_controller::layer::{DpadMode, Turbo};
use pocket_controller::xinput;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Headline,
    Body,
    Small,
    Hint,
    PickerTitle,
    PickerLine,
    Tag,
    SheetLabel,
    Caption,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paint {
    Background,
    Text,
    Muted,
    Accent,
    Track,
    Ink,
    InkMuted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Fill {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        paint: Paint,
    },
    Circle {
        cx: i32,
        cy: i32,
        radius: i32,
        paint: Paint,
    },
    Text {
        role: Role,
        text: String,
        x: i32,
        y: i32,
        align: Align,
        paint: Paint,
    },
    Image {
        name: &'static str,
        x: i32,
        y: i32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    Waiting,
    Warning,
}

pub const IMAGES: &[&str] = &[
    "mode-auto",
    "mode-xbox",
    "mode-gamepad",
    "mode-auto-48-on",
    "mode-auto-48-off",
    "mode-xbox-48-on",
    "mode-xbox-48-off",
    "mode-gamepad-48-on",
    "mode-gamepad-48-off",
    "link-unplugged",
    "link-unrecognised",
    "link-stuck",
    "icon-waiting",
    "icon-warning",
    "icon-connected",
    "btn-menu",
    "btn-select",
    "btn-start",
    "brick",
    "brick-leaders",
    "brick-press-x",
    "brick-turbo-x",
    "brick-turbo-press-x",
    "brick-press-a",
    "brick-turbo-a",
    "brick-turbo-press-a",
    "brick-press-b",
    "brick-turbo-b",
    "brick-turbo-press-b",
    "brick-press-y",
    "brick-turbo-y",
    "brick-turbo-press-y",
    "brick-press-l1",
    "brick-turbo-l1",
    "brick-turbo-press-l1",
    "brick-press-l2",
    "brick-turbo-l2",
    "brick-turbo-press-l2",
    "brick-press-r2",
    "brick-turbo-r2",
    "brick-turbo-press-r2",
    "brick-press-r1",
    "brick-turbo-r1",
    "brick-turbo-press-r1",
    "brick-press-menu",
    "brick-press-select",
    "brick-press-start",
    "brick-press-stick",
    "brick-press-dpad-up",
    "brick-press-dpad-down",
    "brick-press-dpad-left",
    "brick-press-dpad-right",
    "brick-press-volume-up",
    "brick-press-volume-down",
    "brick-fn-on",
    "brick-gate",
    "brick-gate-l",
    "brick-gate-r",
    "brick-stick-dot",
    "brick-choice",
];
const QUIT_HINT: &str = "Hold to quit";
const SHORTCUTS_HINT: &str = "Hold for shortcuts";
const HINTS_Y: i32 = 715;
const BUTTON_PILL: (i32, i32) = (52, 32);
const DESIGN_WIDTH: i32 = 1024;

pub type Measure<'a> = &'a dyn Fn(Role, &str) -> i32;

struct Canvas<'a> {
    ops: Vec<Op>,
    width: i32,
    height: i32,
    measure: Measure<'a>,
}

impl Canvas<'_> {
    fn s(&self, design: i32) -> i32 {
        design * self.width / DESIGN_WIDTH
    }

    fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, paint: Paint) {
        let (x, y, w, h) = (self.s(x), self.s(y), self.s(w), self.s(h));
        self.ops.push(Op::Fill { x, y, w, h, paint });
    }

    fn circle(&mut self, cx: i32, cy: i32, radius: i32, paint: Paint) {
        let (cx, cy, radius) = (self.s(cx), self.s(cy), self.s(radius));
        self.ops.push(Op::Circle {
            cx,
            cy,
            radius,
            paint,
        });
    }

    fn rounded(&mut self, x: i32, y: i32, w: i32, h: i32, radius: i32, paint: Paint) {
        self.fill(x + radius, y, w - 2 * radius, h, paint);
        self.fill(x, y + radius, w, h - 2 * radius, paint);
        for (cx, cy) in [
            (x + radius, y + radius),
            (x + w - radius, y + radius),
            (x + radius, y + h - radius),
            (x + w - radius, y + h - radius),
        ] {
            self.circle(cx, cy, radius, paint);
        }
    }

    fn text(&mut self, role: Role, text: &str, x: i32, y: i32, align: Align, paint: Paint) {
        let (x, y) = (self.s(x), self.s(y));
        self.ops.push(Op::Text {
            role,
            text: text.to_string(),
            x,
            y,
            align,
            paint,
        });
    }

    fn image(&mut self, name: &'static str, x: i32, y: i32) {
        let (x, y) = (self.s(x), self.s(y));
        self.ops.push(Op::Image { name, x, y });
    }

    fn design_width_of(&self, role: Role, text: &str) -> i32 {
        (self.measure)(role, text) * DESIGN_WIDTH / self.width
    }
}

fn waiting_lines(diagnosis: Diagnosis) -> (&'static str, &'static str) {
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

fn mode_description(mode: Mode) -> &'static str {
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

const PICKER_ROWS: [(Mode, i32); 3] = [
    (Mode::Automatic, 356),
    (Mode::Xbox, 448),
    (Mode::Gamepad, 540),
];
const PICKER_LEFT: i32 = 160;
const PICKER_WIDTH: i32 = 704;
const PICKER_ROW_HEIGHT: i32 = 84;
const PICKER_TEXT_LEFT: i32 = 236;
const PICKER_TEXT_WIDTH: i32 = 608;
const PICKER_LINE_HEIGHT: i32 = 21;
const RECOMMENDED: &str = "Recommended";

const BRICK: (i32, i32) = (296, 52);

const BRICK_LABELS: [(&str, i32, i32, Align); 9] = [
    ("Turbo", 290, 58, Align::Right),
    ("Turbo", 734, 58, Align::Left),
    ("D-pad", 286, 376, Align::Right),
    ("Left analog stick", 286, 416, Align::Right),
    ("Right analog stick", 286, 456, Align::Right),
    ("Turbo", 734, 412, Align::Left),
    ("Hold", 412, 526, Align::Right),
    ("Guide + Select", 512, 552, Align::Center),
    ("Settings", 612, 526, Align::Left),
];

const BRICK_CAPTIONS: [(&str, i32, i32, Align); 3] = [
    ("D-pad acts as", 290, 340, Align::Right),
    ("L3", 490, 438, Align::Center),
    ("R3", 534, 438, Align::Center),
];

#[derive(Clone, Copy)]
enum Control {
    Button(u16),
    LeftTrigger,
    RightTrigger,
}

impl Control {
    fn in_mask(self, buttons: u16, left_trigger: bool, right_trigger: bool) -> bool {
        match self {
            Control::Button(bit) => buttons & bit != 0,
            Control::LeftTrigger => left_trigger,
            Control::RightTrigger => right_trigger,
        }
    }
}

struct TurboButton {
    control: Control,
    pressed: &'static str,
    turbo: &'static str,
    turbo_pressed: &'static str,
    x: i32,
    y: i32,
}

const fn turbo_button(control: Control, names: [&'static str; 3], x: i32, y: i32) -> TurboButton {
    TurboButton {
        control,
        pressed: names[0],
        turbo: names[1],
        turbo_pressed: names[2],
        x,
        y,
    }
}

const TURBO_BUTTONS: [TurboButton; 8] = [
    turbo_button(
        Control::Button(xinput::Y),
        ["brick-press-x", "brick-turbo-x", "brick-turbo-press-x"],
        596,
        366,
    ),
    turbo_button(
        Control::Button(xinput::B),
        ["brick-press-a", "brick-turbo-a", "brick-turbo-press-a"],
        626,
        396,
    ),
    turbo_button(
        Control::Button(xinput::A),
        ["brick-press-b", "brick-turbo-b", "brick-turbo-press-b"],
        596,
        426,
    ),
    turbo_button(
        Control::Button(xinput::X),
        ["brick-press-y", "brick-turbo-y", "brick-turbo-press-y"],
        566,
        396,
    ),
    turbo_button(
        Control::Button(xinput::LEFT_SHOULDER),
        ["brick-press-l1", "brick-turbo-l1", "brick-turbo-press-l1"],
        348,
        68,
    ),
    turbo_button(
        Control::LeftTrigger,
        ["brick-press-l2", "brick-turbo-l2", "brick-turbo-press-l2"],
        416,
        68,
    ),
    turbo_button(
        Control::RightTrigger,
        ["brick-press-r2", "brick-turbo-r2", "brick-turbo-press-r2"],
        548,
        68,
    ),
    turbo_button(
        Control::Button(xinput::RIGHT_SHOULDER),
        ["brick-press-r1", "brick-turbo-r1", "brick-turbo-press-r1"],
        616,
        68,
    ),
];

const PLAIN_BUTTONS: [(u16, &str, i32, i32); 9] = [
    (xinput::DPAD_UP, "brick-press-dpad-up", 403, 377),
    (xinput::DPAD_DOWN, "brick-press-dpad-down", 403, 425),
    (xinput::DPAD_LEFT, "brick-press-dpad-left", 377, 403),
    (xinput::DPAD_RIGHT, "brick-press-dpad-right", 425, 403),
    (xinput::GUIDE, "brick-press-menu", 442, 473),
    (xinput::BACK, "brick-press-select", 492, 473),
    (xinput::START, "brick-press-start", 542, 473),
    (xinput::LEFT_THUMB, "brick-press-stick", 478, 407),
    (xinput::RIGHT_THUMB, "brick-press-stick", 522, 407),
];

const DPAD_BUTTONS: u16 =
    xinput::DPAD_UP | xinput::DPAD_DOWN | xinput::DPAD_LEFT | xinput::DPAD_RIGHT;
const GATE: (i32, i32) = (369, 369);
const GATE_BADGE: (i32, i32) = (388, 388);
const GATE_CENTRE: (i32, i32) = (412, 412);
const STICK_TRAVEL: i32 = 30;
const STICK_TRAVEL_DIAGONAL: i32 = 21;
const STICK_DOT_HALF: i32 = 10;
const CHOICE_GAP: i32 = 24;
const CHOICE_HALF: i32 = 8;
const DPAD_LABEL_RIGHT: i32 = 286;

fn dpad_direction(buttons: u16, negative: u16, positive: u16) -> i32 {
    i32::from(buttons & positive != 0) - i32::from(buttons & negative != 0)
}

fn stick_dot(buttons: u16) -> (i32, i32) {
    let x = dpad_direction(buttons, xinput::DPAD_LEFT, xinput::DPAD_RIGHT);
    let y = dpad_direction(buttons, xinput::DPAD_DOWN, xinput::DPAD_UP);
    let travel = if x != 0 && y != 0 {
        STICK_TRAVEL_DIAGONAL
    } else {
        STICK_TRAVEL
    };
    (
        GATE_CENTRE.0 + x * travel - STICK_DOT_HALF,
        GATE_CENTRE.1 - y * travel - STICK_DOT_HALF,
    )
}

fn dpad_choice_row(mode: DpadMode) -> (&'static str, i32) {
    match mode {
        DpadMode::Dpad => ("D-pad", 376),
        DpadMode::LeftStick => ("Left analog stick", 416),
        DpadMode::RightStick => ("Right analog stick", 456),
    }
}

fn brick(canvas: &mut Canvas, live: &LiveView, labelled: bool) {
    canvas.image("brick", BRICK.0, BRICK.1);
    let analog = live.dpad != DpadMode::Dpad;
    for (bit, name, x, y) in PLAIN_BUTTONS {
        if live.buttons & bit != 0 && !(analog && bit & DPAD_BUTTONS != 0) {
            canvas.image(name, x, y);
        }
    }
    for (on, name, x, y) in [
        (live.volume_up, "brick-press-volume-up", 324, 160),
        (live.volume_down, "brick-press-volume-down", 324, 197),
        (live.fn_switch, "brick-fn-on", 692, 160),
    ] {
        if on {
            canvas.image(name, x, y);
        }
    }
    for button in &TURBO_BUTTONS {
        let pressed = button
            .control
            .in_mask(live.buttons, live.left_trigger, live.right_trigger);
        let turbo = button.control.in_mask(
            live.turbo.buttons,
            live.turbo.left_trigger,
            live.turbo.right_trigger,
        );
        let name = match (pressed, turbo) {
            (false, false) => continue,
            (true, false) => button.pressed,
            (false, true) => button.turbo,
            (true, true) => button.turbo_pressed,
        };
        canvas.image(name, button.x, button.y);
    }
    let badge = match live.dpad {
        DpadMode::Dpad => None,
        DpadMode::LeftStick => Some("brick-gate-l"),
        DpadMode::RightStick => Some("brick-gate-r"),
    };
    if let Some(badge) = badge {
        canvas.image("brick-gate", GATE.0, GATE.1);
        canvas.image(badge, GATE_BADGE.0, GATE_BADGE.1);
        let (dot_x, dot_y) = stick_dot(live.buttons);
        canvas.image("brick-stick-dot", dot_x, dot_y);
    }
    if labelled {
        canvas.image("brick-leaders", BRICK.0, BRICK.1);
        let (label, row_y) = dpad_choice_row(live.dpad);
        let label_left = DPAD_LABEL_RIGHT - canvas.design_width_of(Role::SheetLabel, label);
        canvas.image("brick-choice", label_left - CHOICE_GAP, row_y - CHOICE_HALF);
        for (label, x, y, align) in BRICK_LABELS {
            canvas.text(Role::SheetLabel, label, x, y, align, Paint::Text);
        }
        for (caption, x, y, align) in BRICK_CAPTIONS {
            canvas.text(Role::Caption, caption, x, y, align, Paint::Muted);
        }
    }
}

fn picker_glyph(mode: Mode, selected: bool) -> &'static str {
    match (mode, selected) {
        (Mode::Automatic, true) => "mode-auto-48-on",
        (Mode::Automatic, false) => "mode-auto-48-off",
        (Mode::Xbox, true) => "mode-xbox-48-on",
        (Mode::Xbox, false) => "mode-xbox-48-off",
        (Mode::Gamepad, true) => "mode-gamepad-48-on",
        (Mode::Gamepad, false) => "mode-gamepad-48-off",
    }
}

fn split_to_width<'t>(canvas: &Canvas, role: Role, text: &'t str, max_width: i32) -> Vec<&'t str> {
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

fn picker(canvas: &mut Canvas, selected: Mode) {
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

fn mode_chip(mode: Mode) -> (&'static str, &'static str) {
    match mode {
        Mode::Automatic => ("mode-auto", "Automatic"),
        Mode::Xbox => ("mode-xbox", "Xbox-style"),
        Mode::Gamepad => ("mode-gamepad", "Generic"),
    }
}

const CHIP_TOP: i32 = 30;
const CHIP_HEIGHT: i32 = 40;
const CHIP_GLYPH: i32 = 28;
const CHIP_LEFT_PADDING: i32 = 14;
const CHIP_GLYPH_GAP: i32 = 8;
const CHIP_RIGHT_PADDING: i32 = 18;
const CHIP_RIGHT_EDGE: i32 = 984;

fn chip(canvas: &mut Canvas, mode: Mode) {
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

fn kind_title(kind: Kind) -> (&'static str, &'static str) {
    match kind {
        Kind::Xbox => ("Xbox-style", "mode-xbox-48-off"),
        Kind::Gamepad => ("Generic", "mode-gamepad-48-off"),
    }
}

const CONNECTED_AS: &str = "as";
const WORD_GAP: i32 = 14;
const MODE_GLYPH_GAP: i32 = 12;

fn connected_headline(canvas: &mut Canvas, kind: Kind, cy: i32) {
    let (title, glyph) = kind_title(kind);
    let headline = "Connected";
    let headline_width = canvas.design_width_of(Role::Headline, headline);
    let title_width = canvas.design_width_of(Role::Headline, title);
    let as_width = canvas.design_width_of(Role::Headline, CONNECTED_AS);
    let total = ICON_SIZE
        + ICON_GAP
        + headline_width
        + WORD_GAP
        + as_width
        + WORD_GAP
        + ICON_SIZE
        + MODE_GLYPH_GAP
        + title_width;
    let mut x = (DESIGN_WIDTH - total) / 2;
    canvas.image("icon-connected", x, cy - ICON_SIZE / 2);
    x += ICON_SIZE + ICON_GAP;
    canvas.text(Role::Headline, headline, x, cy, Align::Left, Paint::Text);
    x += headline_width + WORD_GAP;
    canvas.text(
        Role::Headline,
        CONNECTED_AS,
        x,
        cy,
        Align::Left,
        Paint::Muted,
    );
    x += as_width + WORD_GAP;
    canvas.image(glyph, x, cy - ICON_SIZE / 2);
    x += ICON_SIZE + MODE_GLYPH_GAP;
    canvas.text(Role::Headline, title, x, cy, Align::Left, Paint::Muted);
}

fn printed_turbo_names(turbo: Turbo) -> Vec<&'static str> {
    let mut names: Vec<&str> = [
        (xinput::B, "A"),
        (xinput::A, "B"),
        (xinput::Y, "X"),
        (xinput::X, "Y"),
        (xinput::LEFT_SHOULDER, "L1"),
        (xinput::RIGHT_SHOULDER, "R1"),
    ]
    .into_iter()
    .filter(|(bit, _)| turbo.buttons & bit != 0)
    .map(|(_, name)| name)
    .collect();
    if turbo.left_trigger {
        names.push("L2");
    }
    if turbo.right_trigger {
        names.push("R2");
    }
    names
}

fn options_line(live: &LiveView) -> Option<String> {
    let mut parts = Vec::new();
    let turbo = printed_turbo_names(live.turbo);
    if !turbo.is_empty() {
        parts.push(format!("Turbo {}", turbo.join(", ")));
    }
    match live.dpad {
        DpadMode::Dpad => {}
        DpadMode::LeftStick => parts.push("D-pad as left analog stick".to_string()),
        DpadMode::RightStick => parts.push("D-pad as right analog stick".to_string()),
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

const PLUS_SLOT: i32 = 35;
const PILL_TEXT_GAP: i32 = 14;

const HINT_LEFT: i32 = 40;
const HINT_RIGHT: i32 = 984;

fn quit_hint(canvas: &mut Canvas) {
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

fn shortcuts_hint(canvas: &mut Canvas) {
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

const ICON_SIZE: i32 = 48;
const ICON_GAP: i32 = 16;
const DOT_RADIUS: i32 = 7;
const DOT_SPACING: i32 = 22;

fn status_icon(status: Status) -> &'static str {
    match status {
        Status::Waiting => "icon-waiting",
        Status::Warning => "icon-warning",
    }
}

fn status_line(canvas: &mut Canvas, status: Status, headline: &str, cy: i32, busy: Option<u8>) {
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

fn link_image(diagnosis: Diagnosis) -> &'static str {
    match diagnosis {
        Diagnosis::None => "link-unplugged",
        Diagnosis::NoComputer => "link-unrecognised",
        Diagnosis::SetupStuck => "link-stuck",
    }
}

const LINK_POSITION: (i32, i32) = (192, 130);
const WAITING_HEADLINE_Y: i32 = 430;
const WAITING_INSTRUCTION_Y: i32 = 484;
const CONNECTED_HEADLINE_Y: i32 = 604;
const OPTIONS_Y: i32 = 650;

fn hint(canvas: &mut Canvas, letter: &str, label: &str, x: i32, y: i32) {
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

const HOLD_TEXT: &str = "Keep holding to quit";
const HOLD_TEXT_GAP: i32 = 24;
const HOLD_BAR_HEIGHT: i32 = 16;

fn hold_bar(canvas: &mut Canvas, progress: u8) {
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

pub fn layout(frame: &Frame, width: i32, height: i32, measure: Measure) -> Vec<Op> {
    let mut canvas = Canvas {
        ops: vec![Op::Fill {
            x: 0,
            y: 0,
            w: width,
            h: height,
            paint: Paint::Background,
        }],
        width,
        height,
        measure,
    };
    let center = DESIGN_WIDTH / 2;
    let design_height = canvas.height * DESIGN_WIDTH / canvas.width;
    if let Some(view) = &frame.sheet {
        let live = LiveView {
            turbo: view.turbo,
            dpad: view.dpad,
            ..frame.live.unwrap_or_default()
        };
        brick(&mut canvas, &live, true);
        if let Some(note) = &view.note {
            canvas.text(
                Role::Body,
                note,
                center,
                CONNECTED_HEADLINE_Y,
                Align::Center,
                Paint::Text,
            );
        }
        match frame.hold {
            Some(progress) => hold_bar(&mut canvas, progress),
            None => quit_hint(&mut canvas),
        }
        return canvas.ops;
    }
    match &frame.screen {
        Screen::Waiting(mode, diagnosis) => {
            canvas.image(link_image(*diagnosis), LINK_POSITION.0, LINK_POSITION.1);
            let (headline, instruction) = waiting_lines(*diagnosis);
            match frame.picker {
                Some(selected) => picker(&mut canvas, selected),
                None => {
                    let status = if *diagnosis == Diagnosis::None {
                        Status::Waiting
                    } else {
                        Status::Warning
                    };
                    status_line(
                        &mut canvas,
                        status,
                        headline,
                        WAITING_HEADLINE_Y,
                        frame.busy,
                    );
                    canvas.text(
                        Role::Body,
                        instruction,
                        center,
                        WAITING_INSTRUCTION_Y,
                        Align::Center,
                        Paint::Muted,
                    );
                    chip(&mut canvas, *mode);
                }
            }
            if frame.hold.is_none() {
                let hints_y = design_height - 53;
                hint(&mut canvas, "Y", "MODE", 40, hints_y);
                hint(&mut canvas, "B", "QUIT", DESIGN_WIDTH - 180, hints_y);
            }
        }
        Screen::Connected(kind) => {
            let live = frame.live.unwrap_or_default();
            brick(&mut canvas, &live, false);
            connected_headline(&mut canvas, *kind, CONNECTED_HEADLINE_Y);
            if let Some(options) = options_line(&live) {
                canvas.text(
                    Role::Body,
                    &options,
                    center,
                    OPTIONS_Y,
                    Align::Center,
                    Paint::Muted,
                );
            }
            if frame.hold.is_none() {
                shortcuts_hint(&mut canvas);
                quit_hint(&mut canvas);
            }
        }
    }
    if let Some(progress) = frame.hold {
        hold_bar(&mut canvas, progress);
    }
    canvas.ops
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocket_controller::control::SheetView;

    const WIDTH: i32 = 1024;
    const HEIGHT: i32 = 768;

    fn fake_measure(role: Role, text: &str) -> i32 {
        let size = match role {
            Role::Headline => 42,
            Role::Body => 30,
            Role::Small | Role::Hint => 24,
            Role::PickerTitle => 26,
            Role::PickerLine => 18,
            Role::Tag => 16,
            Role::SheetLabel => 22,
            Role::Caption => 18,
        };
        text.chars().count() as i32 * size * 55 / 100
    }

    fn draw(screen: Screen, hold: Option<u8>, busy: Option<u8>) -> Vec<Op> {
        draw_with_picker(screen, hold, busy, None)
    }

    fn draw_with_picker(
        screen: Screen,
        hold: Option<u8>,
        busy: Option<u8>,
        picker: Option<Mode>,
    ) -> Vec<Op> {
        let frame = Frame {
            screen,
            hold,
            busy,
            picker,
            sheet: None,
            live: None,
        };
        layout(&frame, WIDTH, HEIGHT, &fake_measure)
    }

    fn texts(ops: &[Op]) -> Vec<String> {
        ops.iter()
            .filter_map(|op| match op {
                Op::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    fn circles_painted(ops: &[Op], paint: Paint) -> usize {
        ops.iter()
            .filter(|op| matches!(op, Op::Circle { paint: p, .. } if *p == paint))
            .count()
    }

    const DIAGNOSES: [Diagnosis; 3] = [
        Diagnosis::None,
        Diagnosis::NoComputer,
        Diagnosis::SetupStuck,
    ];

    #[test]
    fn every_diagnosis_gets_its_own_headline_and_instruction() {
        for diagnosis in DIAGNOSES {
            let all = texts(&draw(
                Screen::Waiting(Mode::Automatic, diagnosis),
                None,
                None,
            ));
            let (headline, instruction) = waiting_lines(diagnosis);
            assert!(all.contains(&headline.to_string()) && all.contains(&instruction.to_string()));
        }
    }

    fn images(ops: &[Op]) -> Vec<&'static str> {
        ops.iter()
            .filter_map(|op| match op {
                Op::Image { name, .. } => Some(*name),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn each_waiting_state_has_its_own_illustration_and_problems_show_a_warning() {
        let expected = [
            (Diagnosis::None, "link-unplugged", "icon-waiting"),
            (Diagnosis::NoComputer, "link-unrecognised", "icon-warning"),
            (Diagnosis::SetupStuck, "link-stuck", "icon-warning"),
        ];
        for (diagnosis, link, icon) in expected {
            let shown = images(&draw(Screen::Waiting(Mode::Xbox, diagnosis), None, None));
            assert!(
                shown.contains(&link) && shown.contains(&icon),
                "{diagnosis:?}: {shown:?}"
            );
        }
    }

    #[test]
    fn every_image_the_layout_uses_is_loaded_by_the_helper() {
        let screens = [
            Screen::Waiting(Mode::Automatic, Diagnosis::None),
            Screen::Waiting(Mode::Xbox, Diagnosis::NoComputer),
            Screen::Waiting(Mode::Gamepad, Diagnosis::SetupStuck),
            Screen::Connected(Kind::Xbox),
        ];
        for screen in screens {
            for name in images(&draw(screen, None, None)) {
                assert!(IMAGES.contains(&name), "{name} is not in IMAGES");
            }
        }
    }

    #[test]
    fn every_listed_image_ships_in_the_assets_folder() {
        for name in IMAGES {
            let path = format!("{}/assets/{name}.png", env!("CARGO_MANIFEST_DIR"));
            assert!(std::path::Path::new(&path).exists(), "missing {path}");
        }
    }

    #[test]
    fn the_mode_chip_names_the_mode_with_one_word_and_its_glyph() {
        for mode in [Mode::Automatic, Mode::Xbox, Mode::Gamepad] {
            let ops = draw(Screen::Waiting(mode, Diagnosis::None), None, None);
            let (glyph, label) = mode_chip(mode);
            assert!(texts(&ops).contains(&label.to_string()), "{mode:?}");
            assert!(
                ops.iter()
                    .any(|op| matches!(op, Op::Image { name, .. } if *name == glyph))
            );
            assert!(!texts(&ops).contains(&mode_description(mode).to_string()));
        }
    }

    #[test]
    fn the_picker_lists_every_mode_and_highlights_only_the_selection() {
        let ops = draw_with_picker(
            Screen::Waiting(Mode::Automatic, Diagnosis::None),
            None,
            None,
            Some(Mode::Xbox),
        );
        let shown = images(&ops);
        assert!(shown.contains(&"mode-xbox-48-on"));
        assert!(shown.contains(&"mode-auto-48-off") && shown.contains(&"mode-gamepad-48-off"));
        let all = texts(&ops);
        for mode in [Mode::Automatic, Mode::Xbox, Mode::Gamepad] {
            assert!(all.contains(&mode.label().to_string()), "{mode:?}");
        }
        assert!(all.contains(&RECOMMENDED.to_string()));
    }

    #[test]
    fn the_picker_hides_the_chip_and_the_status_line() {
        let ops = draw_with_picker(
            Screen::Waiting(Mode::Automatic, Diagnosis::None),
            None,
            None,
            Some(Mode::Automatic),
        );
        let shown = images(&ops);
        assert!(!shown.contains(&"mode-auto") && !shown.contains(&"icon-waiting"));
        assert!(!texts(&ops).contains(&waiting_lines(Diagnosis::None).0.to_string()));
    }

    fn draw_sheet(view: SheetView) -> Vec<Op> {
        let frame = Frame {
            screen: Screen::Connected(Kind::Xbox),
            hold: None,
            busy: None,
            picker: None,
            sheet: Some(view),
            live: None,
        };
        layout(&frame, WIDTH, HEIGHT, &fake_measure)
    }

    #[test]
    fn holding_menu_labels_the_same_brick_and_colours_every_turbo_button() {
        use pocket_controller::layer::Turbo;
        let ops = draw_sheet(SheetView {
            note: Some("Turbo A: on".to_string()),
            turbo: Turbo {
                buttons: xinput::B | xinput::RIGHT_SHOULDER,
                left_trigger: true,
                right_trigger: false,
            },
            dpad: DpadMode::LeftStick,
        });
        let all = texts(&ops);
        assert!(all.contains(&"Turbo A: on".to_string()));
        assert!(
            !all.contains(&"Connected".to_string()),
            "the sheet replaces the headline row"
        );
        let rings: Vec<(i32, i32)> = ops
            .iter()
            .filter_map(|op| match op {
                Op::Image { name, x, y } if name.starts_with("brick-turbo") => Some((*x, *y)),
                _ => None,
            })
            .collect();
        assert_eq!(
            rings,
            vec![(626, 396), (416, 68), (616, 68)],
            "printed A, L2 and R1"
        );
        assert!(images(&ops).contains(&"brick-gate-l"));
        assert!(images(&ops).contains(&"brick-choice"));
        assert!(images(&ops).contains(&"brick-leaders"));
        assert!(all.contains(&"Left analog stick".to_string()));
    }

    fn draw_connected(live: LiveView) -> Vec<Op> {
        let frame = Frame {
            screen: Screen::Connected(Kind::Gamepad),
            hold: None,
            busy: None,
            picker: None,
            sheet: None,
            live: Some(live),
        };
        layout(&frame, WIDTH, HEIGHT, &fake_measure)
    }

    fn placed(ops: &[Op], prefix: &str) -> Vec<(&'static str, i32, i32)> {
        ops.iter()
            .filter_map(|op| match op {
                Op::Image { name, x, y } if name.starts_with(prefix) => Some((*name, *x, *y)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_connected_screen_names_the_mode_and_lights_the_pressed_buttons() {
        let ops = draw_connected(LiveView {
            buttons: xinput::B | xinput::DPAD_UP | xinput::GUIDE,
            left_trigger: true,
            ..LiveView::default()
        });
        let all = texts(&ops);
        let headline: Vec<&String> = all
            .iter()
            .filter(|text| ["Connected", "as", "Generic"].contains(&text.as_str()))
            .collect();
        assert_eq!(
            headline,
            ["Connected", "as", "Generic"],
            "Connected as Generic, in that order"
        );
        assert!(images(&ops).contains(&"mode-gamepad-48-off"));
        assert_eq!(
            placed(&ops, "brick-press"),
            vec![
                ("brick-press-dpad-up", 403, 377),
                ("brick-press-menu", 442, 473),
                ("brick-press-a", 626, 396),
                ("brick-press-l2", 416, 68),
            ],
            "printed A, D-pad up, MENU and L2"
        );
    }

    #[test]
    fn the_side_controls_show_volume_presses_and_the_fn_position() {
        let ops = draw_connected(LiveView {
            volume_down: true,
            fn_switch: true,
            ..LiveView::default()
        });
        assert!(images(&ops).contains(&"brick-press-volume-down"));
        assert!(images(&ops).contains(&"brick-fn-on"));
        assert!(!images(&ops).contains(&"brick-press-volume-up"));
    }

    #[test]
    fn the_stick_dot_sits_where_the_emulated_stick_points() {
        assert_eq!(stick_dot(0), (402, 402), "centred at rest");
        assert_eq!(stick_dot(xinput::DPAD_UP), (402, 372));
        assert_eq!(stick_dot(xinput::DPAD_LEFT), (372, 402));
        assert_eq!(
            stick_dot(xinput::DPAD_UP | xinput::DPAD_RIGHT),
            (423, 381),
            "diagonals stay inside the gate"
        );
    }

    #[test]
    fn analog_mode_shows_the_stick_instead_of_dpad_presses() {
        let ops = draw_connected(LiveView {
            buttons: xinput::DPAD_UP,
            dpad: DpadMode::LeftStick,
            ..LiveView::default()
        });
        assert!(placed(&ops, "brick-press-dpad").is_empty());
        assert!(!placed(&ops, "brick-stick-dot").is_empty());
    }

    #[test]
    fn plain_dpad_mode_draws_no_gate() {
        let ops = draw_connected(LiveView::default());
        assert!(
            placed(&ops, "brick-gate").is_empty() && placed(&ops, "brick-stick-dot").is_empty()
        );
    }

    #[test]
    fn active_turbo_and_dpad_mode_are_named_under_the_headline() {
        let ops = draw_connected(LiveView {
            turbo: Turbo {
                buttons: xinput::B | xinput::RIGHT_SHOULDER,
                left_trigger: false,
                right_trigger: true,
            },
            dpad: DpadMode::LeftStick,
            ..LiveView::default()
        });
        assert!(texts(&ops).contains(&"Turbo A, R1, R2 · D-pad as left analog stick".to_string()));
        assert_eq!(
            placed(&ops, "brick-turbo"),
            vec![
                ("brick-turbo-a", 626, 396),
                ("brick-turbo-r2", 548, 68),
                ("brick-turbo-r1", 616, 68),
            ]
        );
        assert!(placed(&ops, "brick-gate").contains(&("brick-gate-l", 388, 388)));
        assert!(images(&ops).contains(&"btn-select") && images(&ops).contains(&"btn-start"));
    }

    #[test]
    fn long_mode_descriptions_wrap_into_two_lines_that_fit_the_row() {
        let ops = draw_with_picker(
            Screen::Waiting(Mode::Automatic, Diagnosis::None),
            None,
            None,
            Some(Mode::Gamepad),
        );
        let lines: Vec<String> = ops
            .iter()
            .filter_map(|op| match op {
                Op::Text {
                    role: Role::PickerLine,
                    text,
                    ..
                } => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert!(lines.len() > 3, "{lines:?}");
        for line in &lines {
            assert!(
                fake_measure(Role::PickerLine, line) <= PICKER_TEXT_WIDTH,
                "{line:?}"
            );
        }
        assert_eq!(lines.join(" ").matches("button mapping").count(), 1);
    }

    #[test]
    fn waiting_offers_only_the_buttons_that_work() {
        let all = texts(&draw(
            Screen::Waiting(Mode::Gamepad, Diagnosis::None),
            None,
            None,
        ));
        assert!(all.contains(&"QUIT".to_string()) && all.contains(&"MODE".to_string()));
        assert!(!all.contains(&"SETTINGS".to_string()));
    }

    #[test]
    fn loading_dots_highlight_one_dot_per_phase() {
        let screen = Screen::Waiting(Mode::Xbox, Diagnosis::None);
        let without = circles_painted(&draw(screen.clone(), None, None), Paint::Accent);
        for phase in 0..6 {
            let with = circles_painted(&draw(screen.clone(), None, Some(phase)), Paint::Accent);
            assert_eq!(with, without + 1, "phase {phase}");
        }
    }

    #[test]
    fn connected_names_the_mode_and_how_to_quit() {
        let all = texts(&draw(Screen::Connected(Kind::Xbox), None, None));
        assert!(all.contains(&"Connected".to_string()) && all.contains(&"Xbox-style".to_string()));
        assert!(all.contains(&QUIT_HINT.to_string()));
    }

    #[test]
    fn hold_progress_fills_its_share_of_the_track() {
        let ops = draw(Screen::Connected(Kind::Xbox), Some(60), None);
        let width_of = |wanted: Paint| {
            ops.iter()
                .find_map(|op| match op {
                    Op::Fill { w, paint, .. } if *paint == wanted && *w > 100 => Some(*w),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(width_of(Paint::Accent), width_of(Paint::Track) * 60 / 100);
        assert!(texts(&ops).contains(&"Keep holding to quit".to_string()));
    }

    #[test]
    fn the_hold_bar_takes_the_hint_row_so_it_never_covers_the_drawing() {
        for screen in [
            Screen::Connected(Kind::Xbox),
            Screen::Waiting(Mode::Xbox, Diagnosis::None),
        ] {
            let ops = draw(screen, Some(40), None);
            let all = texts(&ops);
            assert!(!all.contains(&QUIT_HINT.to_string()) && !all.contains(&"QUIT".to_string()));
            let bar_top = ops
                .iter()
                .filter_map(|op| match op {
                    Op::Fill {
                        y,
                        w,
                        paint: Paint::Track,
                        ..
                    } => Some((*w, *y)),
                    _ => None,
                })
                .max()
                .map(|(_, y)| y);
            assert!(bar_top.is_some_and(|y| y > 680), "{bar_top:?}");
        }
    }

    const ONE_LINE_BUDGET: usize = 40;

    #[test]
    fn headlines_fit_on_one_line_so_nothing_wraps_into_the_next_row() {
        let mut lines: Vec<&str> = DIAGNOSES
            .into_iter()
            .map(|diagnosis| waiting_lines(diagnosis).0)
            .collect();
        lines.extend([QUIT_HINT, SHORTCUTS_HINT]);
        for line in lines {
            assert!(
                line.len() <= ONE_LINE_BUDGET,
                "{line:?} is {} characters",
                line.len()
            );
        }
    }

    #[test]
    fn everything_fits_on_screen() {
        let screens = [
            Screen::Waiting(Mode::Automatic, Diagnosis::SetupStuck),
            Screen::Waiting(Mode::Automatic, Diagnosis::NoComputer),
            Screen::Connected(Kind::Gamepad),
        ];
        for screen in screens {
            for hold in [None, Some(100)] {
                for op in draw(screen.clone(), hold, Some(1)) {
                    match op {
                        Op::Fill { x, y, w, h, .. } => {
                            assert!(
                                x >= 0 && y >= 0 && w >= 0 && x + w <= WIDTH && y + h <= HEIGHT,
                                "{screen:?}"
                            )
                        }
                        Op::Circle { cx, cy, radius, .. } => assert!(
                            cx - radius >= 0
                                && cx + radius <= WIDTH
                                && cy - radius >= 0
                                && cy + radius <= HEIGHT,
                            "{screen:?}"
                        ),
                        Op::Text { x, y, .. } | Op::Image { x, y, .. } => {
                            assert!((0..=WIDTH).contains(&x) && (0..=HEIGHT).contains(&y))
                        }
                    }
                }
            }
        }
    }
}
