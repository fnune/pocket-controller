use crate::layer::DpadMode;
use crate::xinput;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Button(u16),
    LeftTrigger,
    RightTrigger,
}

impl Key {
    pub fn active(self, buttons: u16, left_trigger: bool, right_trigger: bool) -> bool {
        match self {
            Key::Button(bit) => buttons & bit != 0,
            Key::LeftTrigger => left_trigger,
            Key::RightTrigger => right_trigger,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Turbo,
    DpadMode(DpadMode),
    OpenSettings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub key: Key,
    pub printed: &'static str,
    pub effect: Effect,
    pub label: &'static str,
    pub anchor: &'static str,
}

const fn turbo(key: Key, printed: &'static str, anchor: &'static str) -> Shortcut {
    Shortcut {
        key,
        printed,
        effect: Effect::Turbo,
        label: "Turbo",
        anchor,
    }
}

const fn dpad(button: u16, mode: DpadMode, label: &'static str, anchor: &'static str) -> Shortcut {
    Shortcut {
        key: Key::Button(button),
        printed: "",
        effect: Effect::DpadMode(mode),
        label,
        anchor,
    }
}

pub const SHORTCUTS: [Shortcut; 12] = [
    turbo(Key::Button(xinput::B), "A", "label-turbo-face"),
    turbo(Key::Button(xinput::A), "B", "label-turbo-face"),
    turbo(Key::Button(xinput::Y), "X", "label-turbo-face"),
    turbo(Key::Button(xinput::X), "Y", "label-turbo-face"),
    turbo(
        Key::Button(xinput::LEFT_SHOULDER),
        "L1",
        "label-turbo-shoulders-left",
    ),
    turbo(Key::LeftTrigger, "L2", "label-turbo-shoulders-left"),
    turbo(
        Key::Button(xinput::RIGHT_SHOULDER),
        "R1",
        "label-turbo-shoulders-right",
    ),
    turbo(Key::RightTrigger, "R2", "label-turbo-shoulders-right"),
    dpad(
        xinput::DPAD_UP,
        DpadMode::Dpad,
        "D-pad",
        "label-dpad-mode-dpad",
    ),
    dpad(
        xinput::DPAD_LEFT,
        DpadMode::LeftStick,
        "Left analog stick",
        "label-dpad-mode-left-stick",
    ),
    dpad(
        xinput::DPAD_RIGHT,
        DpadMode::RightStick,
        "Right analog stick",
        "label-dpad-mode-right-stick",
    ),
    Shortcut {
        key: Key::Button(xinput::START),
        printed: "START",
        effect: Effect::OpenSettings,
        label: "Settings",
        anchor: "label-start",
    },
];

pub fn for_key(key: Key) -> Option<&'static Shortcut> {
    SHORTCUTS.iter().find(|shortcut| shortcut.key == key)
}

pub fn for_dpad_mode(mode: DpadMode) -> &'static Shortcut {
    SHORTCUTS
        .iter()
        .find(|shortcut| shortcut.effect == Effect::DpadMode(mode))
        .unwrap_or(&SHORTCUTS[8])
}

pub fn turbo_shortcuts() -> impl Iterator<Item = &'static Shortcut> {
    SHORTCUTS
        .iter()
        .filter(|shortcut| shortcut.effect == Effect::Turbo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_dpad_mode_has_exactly_one_shortcut() {
        for mode in [DpadMode::Dpad, DpadMode::LeftStick, DpadMode::RightStick] {
            let matching = SHORTCUTS
                .iter()
                .filter(|shortcut| shortcut.effect == Effect::DpadMode(mode))
                .count();
            assert_eq!(matching, 1, "{mode:?}");
            assert_eq!(for_dpad_mode(mode).effect, Effect::DpadMode(mode));
        }
    }

    #[test]
    fn no_key_has_two_shortcuts() {
        for (index, shortcut) in SHORTCUTS.iter().enumerate() {
            assert!(
                SHORTCUTS[index + 1..]
                    .iter()
                    .all(|other| other.key != shortcut.key),
                "{shortcut:?}"
            );
        }
    }

    #[test]
    fn the_readme_lists_every_shortcut() {
        let readme = include_str!("../README.md").to_lowercase();
        let turbo: Vec<&str> = turbo_shortcuts().map(|shortcut| shortcut.printed).collect();
        let turbo_line = format!("menu + {}: turbo", turbo.join(", ")).to_lowercase();
        assert!(readme.contains(&turbo_line), "{turbo_line}");
        for mode in [DpadMode::Dpad, DpadMode::LeftStick, DpadMode::RightStick] {
            let label = for_dpad_mode(mode).label.to_lowercase();
            assert!(readme.contains(&label), "{label}");
        }
        assert!(readme.contains("menu + start: settings"));
    }
}
