use std::time::{Duration, Instant};

use crate::evdev::Pad;
use crate::xinput::{self, State};

pub const TAP_LIMIT: Duration = Duration::from_millis(300);
pub const GUIDE_PULSE: Duration = Duration::from_millis(100);
pub const TURBO_HALF_PERIOD: Duration = Duration::from_millis(40);
const STICK_FULL: i16 = 32767;
const STICK_DIAGONAL: i16 = 23170;
pub const TRIGGER_PRESSED_ABOVE: u8 = 127;

const FACE: u16 = xinput::A | xinput::B | xinput::X | xinput::Y;
const SHOULDERS: u16 = xinput::LEFT_SHOULDER | xinput::RIGHT_SHOULDER;
const DPAD: u16 = xinput::DPAD_UP | xinput::DPAD_DOWN | xinput::DPAD_LEFT | xinput::DPAD_RIGHT;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DpadMode {
    #[default]
    Dpad,
    LeftStick,
    RightStick,
}

impl DpadMode {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "dpad" => Some(Self::Dpad),
            "left-stick" => Some(Self::LeftStick),
            "right-stick" => Some(Self::RightStick),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dpad => "dpad",
            Self::LeftStick => "left-stick",
            Self::RightStick => "right-stick",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Dpad => "D-pad",
            Self::LeftStick => "Left stick",
            Self::RightStick => "Right stick",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FaceLayout {
    #[default]
    Label,
    Position,
}

impl FaceLayout {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "label" => Some(Self::Label),
            "position" => Some(Self::Position),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Label => "label",
            Self::Position => "position",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Turbo {
    pub buttons: u16,
    pub left_trigger: bool,
    pub right_trigger: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayerEvent {
    Sheet(bool),
    Note(String),
    TurboChanged(Turbo),
    DpadModeChanged(DpadMode),
    OpenSettings,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayerOutput {
    pub sent: State,
    pub events: Vec<LayerEvent>,
}

#[derive(Debug)]
pub struct Layer {
    pub face: FaceLayout,
    pub dpad_mode: DpadMode,
    pub turbo: Turbo,
    epoch: Instant,
    previous: Pad,
    menu_since: Option<Instant>,
    menu_chorded: bool,
    sheet_shown: bool,
    withheld: u16,
    triggers_withheld: (bool, bool),
    guide_until: Option<Instant>,
}

fn printed_name(button: u16) -> &'static str {
    match button {
        xinput::A => "B",
        xinput::B => "A",
        xinput::X => "Y",
        xinput::Y => "X",
        xinput::LEFT_SHOULDER => "L1",
        xinput::RIGHT_SHOULDER => "R1",
        _ => "?",
    }
}

fn bits(mask: u16) -> impl Iterator<Item = u16> {
    (0..16)
        .map(|bit| 1u16 << bit)
        .filter(move |bit| mask & bit != 0)
}

fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

impl Layer {
    pub fn new(face: FaceLayout, dpad_mode: DpadMode, now: Instant) -> Self {
        Self {
            face,
            dpad_mode,
            turbo: Turbo::default(),
            epoch: now,
            previous: Pad::default(),
            menu_since: None,
            menu_chorded: false,
            sheet_shown: false,
            withheld: 0,
            triggers_withheld: (false, false),
            guide_until: None,
        }
    }

    pub fn sheet_shown(&self) -> bool {
        self.sheet_shown
    }

    pub fn menu_held(&self) -> bool {
        self.menu_since.is_some()
    }

    fn chord(&mut self, pad: &Pad, now: Instant, events: &mut Vec<LayerEvent>) {
        let held = pad.state.buttons;
        let pressed = held & !self.previous.state.buttons & !xinput::GUIDE;
        for button in bits(pressed) {
            self.menu_chorded = true;
            self.withheld |= button;
            if button == xinput::START {
                self.menu_chorded = true;
                events.push(LayerEvent::OpenSettings);
            } else if button & (FACE | SHOULDERS) != 0 {
                self.menu_chorded = true;
                self.turbo.buttons ^= button;
                let on = self.turbo.buttons & button != 0;
                events.push(LayerEvent::TurboChanged(self.turbo));
                events.push(LayerEvent::Note(format!(
                    "Turbo {}: {}",
                    printed_name(button),
                    on_off(on)
                )));
            } else if let Some(mode) = match button {
                xinput::DPAD_UP => Some(DpadMode::Dpad),
                xinput::DPAD_LEFT => Some(DpadMode::LeftStick),
                xinput::DPAD_RIGHT => Some(DpadMode::RightStick),
                _ => None,
            } {
                self.menu_chorded = true;
                self.dpad_mode = mode;
                events.push(LayerEvent::DpadModeChanged(mode));
                events.push(LayerEvent::Note(format!("D-pad: {}", mode.label())));
            }
        }
        let left_now = pad.state.left_trigger > TRIGGER_PRESSED_ABOVE;
        let right_now = pad.state.right_trigger > TRIGGER_PRESSED_ABOVE;
        let left_before = self.previous.state.left_trigger > TRIGGER_PRESSED_ABOVE;
        let right_before = self.previous.state.right_trigger > TRIGGER_PRESSED_ABOVE;
        for (pressed, name, is_left) in [
            (left_now && !left_before, "L2", true),
            (right_now && !right_before, "R2", false),
        ] {
            if !pressed {
                continue;
            }
            self.menu_chorded = true;
            let on = if is_left {
                self.triggers_withheld.0 = true;
                self.turbo.left_trigger = !self.turbo.left_trigger;
                self.turbo.left_trigger
            } else {
                self.triggers_withheld.1 = true;
                self.turbo.right_trigger = !self.turbo.right_trigger;
                self.turbo.right_trigger
            };
            events.push(LayerEvent::TurboChanged(self.turbo));
            events.push(LayerEvent::Note(format!("Turbo {name}: {}", on_off(on))));
        }
        let held_long = self
            .menu_since
            .is_some_and(|since| now.duration_since(since) >= TAP_LIMIT);
        if !self.sheet_shown && (held_long || self.menu_chorded) {
            self.sheet_shown = true;
            events.push(LayerEvent::Sheet(true));
        }
    }

    pub fn turbo_phase_on(&self, now: Instant) -> bool {
        (now.duration_since(self.epoch).as_millis() / TURBO_HALF_PERIOD.as_millis())
            .is_multiple_of(2)
    }

    fn apply_dpad_mode(&self, sent: &mut State) {
        let mode = self.dpad_mode;
        if mode == DpadMode::Dpad {
            return;
        }
        let direction = |negative: u16, positive: u16| match (
            sent.buttons & negative != 0,
            sent.buttons & positive != 0,
        ) {
            (true, false) => -1,
            (false, true) => 1,
            _ => 0,
        };
        let x = direction(xinput::DPAD_LEFT, xinput::DPAD_RIGHT);
        let y = direction(xinput::DPAD_DOWN, xinput::DPAD_UP);
        let amount = if x != 0 && y != 0 {
            STICK_DIAGONAL
        } else {
            STICK_FULL
        };
        let (stick_x, stick_y) = (x as i16 * amount, y as i16 * amount);
        match mode {
            DpadMode::LeftStick => (sent.left_x, sent.left_y) = (stick_x, stick_y),
            DpadMode::RightStick => (sent.right_x, sent.right_y) = (stick_x, stick_y),
            DpadMode::Dpad => {}
        }
        sent.buttons &= !DPAD;
    }

    fn apply_face_layout(&self, sent: &mut State) {
        if self.face == FaceLayout::Position {
            return;
        }
        let swap = |buttons: u16, a: u16, b: u16| {
            let mut swapped = buttons & !(a | b);
            if buttons & a != 0 {
                swapped |= b;
            }
            if buttons & b != 0 {
                swapped |= a;
            }
            swapped
        };
        sent.buttons = swap(
            swap(sent.buttons, xinput::A, xinput::B),
            xinput::X,
            xinput::Y,
        );
    }

    pub fn update(&mut self, pad: Pad, now: Instant) -> LayerOutput {
        let mut events = Vec::new();
        let menu = pad.state.buttons & xinput::GUIDE != 0;
        if menu {
            if self.menu_since.is_none() {
                self.menu_since = Some(now);
                self.menu_chorded = false;
            }
            self.chord(&pad, now, &mut events);
        } else if let Some(since) = self.menu_since.take() {
            if !self.menu_chorded && now.duration_since(since) < TAP_LIMIT {
                self.guide_until = Some(now + GUIDE_PULSE);
            }
            if self.sheet_shown {
                self.sheet_shown = false;
                events.push(LayerEvent::Sheet(false));
            }
        }
        self.withheld &= pad.state.buttons;
        if pad.state.left_trigger <= TRIGGER_PRESSED_ABOVE {
            self.triggers_withheld.0 = false;
        }
        if pad.state.right_trigger <= TRIGGER_PRESSED_ABOVE {
            self.triggers_withheld.1 = false;
        }
        let mut sent = pad.state;
        sent.buttons &= !xinput::GUIDE;
        if menu {
            sent = State::default();
        } else {
            sent.buttons &= !self.withheld;
            if self.triggers_withheld.0 {
                sent.left_trigger = 0;
            }
            if self.triggers_withheld.1 {
                sent.right_trigger = 0;
            }
        }
        if self.guide_until.is_some_and(|until| now < until) {
            sent.buttons |= xinput::GUIDE;
        } else {
            self.guide_until = None;
        }
        if !self.turbo_phase_on(now) {
            sent.buttons &= !self.turbo.buttons;
            if self.turbo.left_trigger {
                sent.left_trigger = 0;
            }
            if self.turbo.right_trigger {
                sent.right_trigger = 0;
            }
        }
        self.apply_dpad_mode(&mut sent);
        self.apply_face_layout(&mut sent);
        self.previous = pad;
        LayerOutput { sent, events }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evdev::Extras;

    fn pad(buttons: u16) -> Pad {
        Pad {
            state: State {
                buttons,
                ..State::default()
            },
            extras: Extras::default(),
        }
    }

    fn layer(start: Instant) -> Layer {
        Layer::new(FaceLayout::Position, DpadMode::Dpad, start)
    }

    fn ms(start: Instant, millis: u64) -> Instant {
        start + Duration::from_millis(millis)
    }

    #[test]
    fn a_quick_menu_tap_sends_a_short_guide_pulse_on_release() {
        let start = Instant::now();
        let mut layer = layer(start);
        assert_eq!(layer.update(pad(xinput::GUIDE), start).sent.buttons, 0);
        assert_eq!(
            layer.update(pad(0), ms(start, 150)).sent.buttons,
            xinput::GUIDE
        );
        assert_eq!(
            layer.update(pad(0), ms(start, 200)).sent.buttons,
            xinput::GUIDE
        );
        assert_eq!(layer.update(pad(0), ms(start, 260)).sent.buttons, 0);
    }

    #[test]
    fn holding_menu_shows_the_sheet_and_sends_nothing_on_release() {
        let start = Instant::now();
        let mut layer = layer(start);
        layer.update(pad(xinput::GUIDE), start);
        let held = layer.update(pad(xinput::GUIDE), ms(start, 320));
        assert!(held.events.contains(&LayerEvent::Sheet(true)));
        let released = layer.update(pad(0), ms(start, 900));
        assert!(released.events.contains(&LayerEvent::Sheet(false)));
        assert_eq!(released.sent.buttons, 0);
        assert_eq!(layer.update(pad(0), ms(start, 950)).sent.buttons, 0);
    }

    #[test]
    fn menu_plus_a_toggles_turbo_without_leaking_a_to_the_host() {
        let start = Instant::now();
        let mut layer = layer(start);
        layer.update(pad(xinput::GUIDE), start);
        let chord = layer.update(pad(xinput::GUIDE | xinput::B), ms(start, 50));
        assert_eq!(chord.sent.buttons, 0);
        assert!(chord.events.contains(&LayerEvent::Sheet(true)));
        assert!(
            chord
                .events
                .contains(&LayerEvent::Note("Turbo A: on".into()))
        );
        assert_eq!(layer.turbo.buttons, xinput::B);
        let menu_released_first = layer.update(pad(xinput::B), ms(start, 100));
        assert_eq!(
            menu_released_first.sent.buttons, 0,
            "A stays withheld until it is released"
        );
        assert_eq!(
            layer.update(pad(0), ms(start, 150)).sent.buttons,
            0,
            "a chord never sends Guide"
        );
    }

    #[test]
    fn turbo_buttons_alternate_every_forty_milliseconds_while_held() {
        let start = Instant::now();
        let mut layer = layer(start);
        layer.turbo.buttons = xinput::A;
        let at = |layer: &mut Layer, millis| {
            layer.update(pad(xinput::A), ms(start, millis)).sent.buttons
        };
        assert_eq!(at(&mut layer, 10), xinput::A);
        assert_eq!(at(&mut layer, 50), 0);
        assert_eq!(at(&mut layer, 90), xinput::A);
        assert_eq!(at(&mut layer, 130), 0);
    }

    #[test]
    fn menu_plus_a_direction_sets_the_dpad_mode() {
        let start = Instant::now();
        let mut layer = layer(start);
        layer.update(pad(xinput::GUIDE), start);
        let output = layer.update(pad(xinput::GUIDE | xinput::DPAD_LEFT), ms(start, 30));
        assert!(
            output
                .events
                .contains(&LayerEvent::DpadModeChanged(DpadMode::LeftStick))
        );
        assert_eq!(layer.dpad_mode, DpadMode::LeftStick);
    }

    #[test]
    fn left_stick_mode_turns_the_dpad_into_stick_values_with_scaled_diagonals() {
        let start = Instant::now();
        let mut layer = Layer::new(FaceLayout::Position, DpadMode::LeftStick, start);
        let sent = layer
            .update(pad(xinput::DPAD_UP | xinput::DPAD_RIGHT | xinput::A), start)
            .sent;
        assert_eq!(sent.buttons, xinput::A);
        assert_eq!((sent.left_x, sent.left_y), (STICK_DIAGONAL, STICK_DIAGONAL));
        let sent = layer.update(pad(xinput::DPAD_DOWN), ms(start, 10)).sent;
        assert_eq!((sent.left_x, sent.left_y), (0, -STICK_FULL));
    }

    #[test]
    fn the_label_layout_sends_what_is_printed_on_the_button() {
        let start = Instant::now();
        let mut layer = Layer::new(FaceLayout::Label, DpadMode::Dpad, start);
        assert_eq!(
            layer.update(pad(xinput::B), start).sent.buttons,
            xinput::A,
            "printed A is on the right"
        );
        assert_eq!(
            layer.update(pad(xinput::X), ms(start, 1)).sent.buttons,
            xinput::Y,
            "printed Y is on the left"
        );
    }

    #[test]
    fn a_button_without_a_shortcut_under_menu_sends_nothing() {
        let start = Instant::now();
        let mut layer = layer(start);
        layer.update(pad(xinput::GUIDE), start);
        let chord = layer.update(pad(xinput::GUIDE | xinput::BACK), ms(start, 20));
        assert_eq!(chord.sent.buttons, 0);
        assert!(chord.events.contains(&LayerEvent::Sheet(true)));
        assert_eq!(
            layer.update(pad(0), ms(start, 60)).sent.buttons,
            0,
            "no Guide tap after a chord"
        );
    }

    #[test]
    fn menu_plus_start_asks_for_settings() {
        let start = Instant::now();
        let mut layer = layer(start);
        layer.update(pad(xinput::GUIDE), start);
        assert!(
            layer
                .update(pad(xinput::GUIDE | xinput::START), ms(start, 20))
                .events
                .contains(&LayerEvent::OpenSettings)
        );
    }

    #[test]
    fn settings_names_round_trip() {
        for mode in [DpadMode::Dpad, DpadMode::LeftStick, DpadMode::RightStick] {
            assert_eq!(DpadMode::parse(mode.as_str()), Some(mode));
        }
        for face in [FaceLayout::Label, FaceLayout::Position] {
            assert_eq!(FaceLayout::parse(face.as_str()), Some(face));
        }
    }
}
