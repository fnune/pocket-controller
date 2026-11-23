use crate::control::{Kind, Mode};
use crate::layer::FaceLayout;
use crate::xinput;

pub const PRINTED_A: u16 = xinput::B;
pub const PRINTED_B: u16 = xinput::A;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScreenWhilePlaying {
    #[default]
    DimAfter,
    AlwaysOn,
    OffAfter,
}

impl ScreenWhilePlaying {
    pub const ALL: [Self; 3] = [Self::DimAfter, Self::AlwaysOn, Self::OffAfter];

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|value| value.as_str() == text.trim())
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::DimAfter => "dim",
            Self::AlwaysOn => "on",
            Self::OffAfter => "off",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::DimAfter => "Dim",
            Self::AlwaysOn => "Stay on",
            Self::OffAfter => "Turn off",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub face: FaceLayout,
    pub rumble: bool,
    pub screen: ScreenWhilePlaying,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            face: FaceLayout::default(),
            rumble: true,
            screen: ScreenWhilePlaying::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Row {
    #[default]
    Face,
    Rumble,
    Screen,
    Mode,
    Debug,
}

impl Row {
    pub const ALL: [Self; 5] = [
        Self::Face,
        Self::Rumble,
        Self::Screen,
        Self::Mode,
        Self::Debug,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Face => "face",
            Self::Rumble => "rumble",
            Self::Screen => "screen",
            Self::Mode => "mode",
            Self::Debug => "debug",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|row| row.as_str() == text)
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|row| *row == self).unwrap_or(0)
    }
}

pub const FACE_VALUES: [FaceLayout; 2] = [FaceLayout::Label, FaceLayout::Position];
pub const RUMBLE_VALUES: [bool; 2] = [true, false];
pub const MODE_VALUES: [Mode; 3] = [Mode::Automatic, Mode::Xbox, Mode::Gamepad];

pub fn face_label(face: FaceLayout) -> &'static str {
    match face {
        FaceLayout::Label => "Nintendo layout",
        FaceLayout::Position => "Xbox layout",
    }
}

pub fn face_detail(face: FaceLayout) -> &'static str {
    match face {
        FaceLayout::Label => "A is on the right: the button labelled A sends A.",
        FaceLayout::Position => {
            "A is at the bottom: the bottom button sends A, whatever is printed on it."
        }
    }
}

pub fn rumble_label(on: bool) -> &'static str {
    if on { "On" } else { "Off" }
}

pub fn rumble_detail(on: bool) -> &'static str {
    if on {
        "The handheld vibrates when a game asks."
    } else {
        "The handheld never vibrates."
    }
}

pub fn screen_detail(screen: ScreenWhilePlaying) -> &'static str {
    match screen {
        ScreenWhilePlaying::DimAfter => "Dims 30 s after a computer connects.",
        ScreenWhilePlaying::AlwaysOn => "Stays at full brightness while connected.",
        ScreenWhilePlaying::OffAfter => {
            "Turns off 30 s after a computer connects. The controller keeps working."
        }
    }
}

pub fn mode_detail(mode: Mode) -> &'static str {
    match mode {
        Mode::Automatic => "Xbox-style when the computer supports it, otherwise Generic.",
        Mode::Xbox => {
            "Emulates an Xbox 360 controller. Linux and Steam recognise it without setup."
        }
        Mode::Gamepad => {
            "A standard USB gamepad that works with more devices but may need button mapping."
        }
    }
}

pub const RUMBLE_UNAVAILABLE: &str = "Not available in Generic mode";
pub const RUMBLE_UNAVAILABLE_DETAIL: &str =
    "Generic mode is a plain USB gamepad, which has no way to receive rumble.";
pub const DEBUG_VALUE: &str = "What the computer receives";
pub const DEBUG_DETAIL: &str =
    "Shows what the computer receives, live. Buttons still reach the computer while it is open.";
pub const MODE_CHANGE_WARNING: &str = "The computer will see the controller unplug and replug.";

pub fn rumble_available(kind: Kind) -> bool {
    kind == Kind::Xbox
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettingsView {
    pub cursor: Row,
    pub settings: Settings,
    pub mode: Mode,
    pub current_mode: Mode,
    pub kind: Kind,
    pub connected: bool,
}

impl SettingsView {
    pub fn mode_change_warning(&self) -> bool {
        self.cursor == Row::Mode
            && self.connected
            && self.mode != self.current_mode
            && self.mode.first_kind() != self.kind
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuOutcome {
    Nothing,
    Moved,
    Changed,
    Close,
    OpenDebug,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Menu {
    pub cursor: Row,
    pub mode: Mode,
}

fn step<T: Copy + PartialEq>(values: &[T], current: T, forward: bool) -> T {
    let index = values
        .iter()
        .position(|value| *value == current)
        .unwrap_or(0);
    let next = if forward {
        (index + 1).min(values.len() - 1)
    } else {
        index.saturating_sub(1)
    };
    values[next]
}

impl Menu {
    pub fn new(mode: Mode) -> Self {
        Self {
            cursor: Row::default(),
            mode,
        }
    }

    pub fn press(&mut self, button: u16, settings: &mut Settings, kind: Kind) -> MenuOutcome {
        match button {
            xinput::DPAD_UP | xinput::DPAD_DOWN => {
                let index = self.cursor.index();
                let next = if button == xinput::DPAD_DOWN {
                    (index + 1).min(Row::ALL.len() - 1)
                } else {
                    index.saturating_sub(1)
                };
                if next == index {
                    return MenuOutcome::Nothing;
                }
                self.cursor = Row::ALL[next];
                MenuOutcome::Moved
            }
            xinput::DPAD_LEFT | xinput::DPAD_RIGHT => {
                let forward = button == xinput::DPAD_RIGHT;
                let before = (*settings, self.mode);
                match self.cursor {
                    Row::Face => settings.face = step(&FACE_VALUES, settings.face, forward),
                    Row::Rumble if rumble_available(kind) => {
                        settings.rumble = step(&RUMBLE_VALUES, settings.rumble, forward)
                    }
                    Row::Screen => {
                        settings.screen = step(&ScreenWhilePlaying::ALL, settings.screen, forward)
                    }
                    Row::Mode => self.mode = step(&MODE_VALUES, self.mode, forward),
                    Row::Rumble | Row::Debug => {}
                }
                if before == (*settings, self.mode) {
                    MenuOutcome::Nothing
                } else {
                    MenuOutcome::Changed
                }
            }
            PRINTED_A if self.cursor == Row::Debug => MenuOutcome::OpenDebug,
            PRINTED_B => MenuOutcome::Close,
            _ => MenuOutcome::Nothing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_and_down_move_the_cursor_and_stop_at_the_ends() {
        let mut settings = Settings::default();
        let mut menu = Menu::new(Mode::Automatic);
        assert_eq!(
            menu.press(xinput::DPAD_UP, &mut settings, Kind::Xbox),
            MenuOutcome::Nothing
        );
        for _ in 0..10 {
            menu.press(xinput::DPAD_DOWN, &mut settings, Kind::Xbox);
        }
        assert_eq!(menu.cursor, Row::Debug);
    }

    #[test]
    fn left_and_right_step_through_values_without_wrapping() {
        let mut settings = Settings::default();
        let mut menu = Menu::new(Mode::Automatic);
        menu.cursor = Row::Screen;
        menu.press(xinput::DPAD_RIGHT, &mut settings, Kind::Xbox);
        assert_eq!(settings.screen, ScreenWhilePlaying::AlwaysOn);
        menu.press(xinput::DPAD_RIGHT, &mut settings, Kind::Xbox);
        assert_eq!(
            menu.press(xinput::DPAD_RIGHT, &mut settings, Kind::Xbox),
            MenuOutcome::Nothing
        );
        assert_eq!(settings.screen, ScreenWhilePlaying::OffAfter);
    }

    #[test]
    fn rumble_cannot_change_in_generic_mode() {
        let mut settings = Settings::default();
        let mut menu = Menu::new(Mode::Gamepad);
        menu.cursor = Row::Rumble;
        assert_eq!(
            menu.press(xinput::DPAD_RIGHT, &mut settings, Kind::Gamepad),
            MenuOutcome::Nothing
        );
        assert!(settings.rumble);
    }

    #[test]
    fn the_mode_row_changes_only_the_menu_until_it_closes() {
        let mut settings = Settings::default();
        let mut menu = Menu::new(Mode::Automatic);
        menu.cursor = Row::Mode;
        menu.press(xinput::DPAD_RIGHT, &mut settings, Kind::Xbox);
        assert_eq!(menu.mode, Mode::Xbox);
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn printed_a_opens_debug_only_on_its_row_and_printed_b_closes() {
        let mut settings = Settings::default();
        let mut menu = Menu::new(Mode::Automatic);
        assert_eq!(
            menu.press(PRINTED_A, &mut settings, Kind::Xbox),
            MenuOutcome::Nothing
        );
        menu.cursor = Row::Debug;
        assert_eq!(
            menu.press(PRINTED_A, &mut settings, Kind::Xbox),
            MenuOutcome::OpenDebug
        );
        assert_eq!(
            menu.press(PRINTED_B, &mut settings, Kind::Xbox),
            MenuOutcome::Close
        );
    }

    #[test]
    fn stored_names_round_trip() {
        for value in ScreenWhilePlaying::ALL {
            assert_eq!(ScreenWhilePlaying::parse(value.as_str()), Some(value));
        }
        for row in Row::ALL {
            assert_eq!(Row::parse(row.as_str()), Some(row));
        }
    }
}
