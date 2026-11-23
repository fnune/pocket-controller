mod connected;
mod debug;
mod handheld;
mod hints;
mod settings;
mod waiting;

use connected::*;
use debug::*;
use handheld::*;
use hints::*;
use settings::*;
use waiting::*;

use crate::device::Device;
use pocket_controller::control::{Diagnosis, Kind, LiveView, Mode, Screen};
use pocket_controller::frame::Frame;
use pocket_controller::layer::{DpadMode, Turbo};
use pocket_controller::shortcuts::{SHORTCUTS, for_dpad_mode, turbo_shortcuts};
use pocket_controller::xinput;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Headline,
    Body,
    Small,
    Hint,
    Detail,
    Value,
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
    Faint,
    Card,
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
        name: String,
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
    "btn-updown",
    "btn-leftright",
    "chevron-left",
    "chevron-right",
    "face-nintendo",
    "face-xbox",
    "key-menu",
    "key-power",
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

    fn image(&mut self, name: &str, x: i32, y: i32) {
        let (x, y) = (self.s(x), self.s(y));
        self.ops.push(Op::Image {
            name: name.to_string(),
            x,
            y,
        });
    }

    fn design_width_of(&self, role: Role, text: &str) -> i32 {
        (self.measure)(role, text) * DESIGN_WIDTH / self.width
    }
}

pub fn layout(
    frame: &Frame,
    width: i32,
    height: i32,
    measure: Measure,
    device: &Device,
) -> Vec<Op> {
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
    if let Some(view) = &frame.settings {
        settings_screen(&mut canvas, view, frame.hold);
        return canvas.ops;
    }
    if let Some(view) = &frame.debug {
        let live = frame.live.unwrap_or_default();
        debug_screen(&mut canvas, device, view, &live, frame.hold);
        return canvas.ops;
    }
    let design_height = canvas.height * DESIGN_WIDTH / canvas.width;
    if let Some(view) = &frame.sheet {
        let live = LiveView {
            turbo: view.turbo,
            dpad: view.dpad,
            ..frame.live.unwrap_or_default()
        };
        handheld(&mut canvas, device, &live, true, 0);
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
            if frame.hold.is_none() {
                let hints_y = design_height - 53;
                hint(&mut canvas, "A", "SETTINGS", HINT_LEFT, hints_y);
                hint_right(&mut canvas, "B", "QUIT", hints_y);
            }
        }
        Screen::Connected(kind) => {
            let live = frame.live.unwrap_or_default();
            handheld(&mut canvas, device, &live, false, 0);
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
mod tests;
