use crate::xinput::{self, State};

pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_ABS: u16 = 0x03;
pub const SYN_REPORT: u16 = 0;
pub const SYN_DROPPED: u16 = 3;

pub const KEY_POWER: u16 = 116;
pub const KEY_VOLUMEDOWN: u16 = 114;
pub const KEY_VOLUMEUP: u16 = 115;
pub const EV_SW: u16 = 0x05;
pub const SW_FN: u16 = 0x01;
pub const BTN_A: u16 = 0x130;
pub const BTN_B: u16 = 0x131;
pub const BTN_X: u16 = 0x133;
pub const BTN_Y: u16 = 0x134;
pub const BTN_TL: u16 = 0x136;
pub const BTN_TR: u16 = 0x137;
pub const BTN_TL2: u16 = 0x138;
pub const BTN_TR2: u16 = 0x139;
pub const BTN_SELECT: u16 = 0x13a;
pub const BTN_START: u16 = 0x13b;
pub const BTN_MODE: u16 = 0x13c;
pub const BTN_THUMBL: u16 = 0x13d;
pub const BTN_THUMBR: u16 = 0x13e;
pub const BTN_DPAD_UP: u16 = 0x220;
pub const BTN_DPAD_DOWN: u16 = 0x221;
pub const BTN_DPAD_LEFT: u16 = 0x222;
pub const BTN_DPAD_RIGHT: u16 = 0x223;

pub const ABS_X: u16 = 0x00;
pub const ABS_Y: u16 = 0x01;
pub const ABS_Z: u16 = 0x02;
pub const ABS_RX: u16 = 0x03;
pub const ABS_RY: u16 = 0x04;
pub const ABS_RZ: u16 = 0x05;
pub const ABS_HAT0X: u16 = 0x10;
pub const ABS_HAT0Y: u16 = 0x11;
pub const ABS_AXES: [u16; 8] = [
    ABS_X, ABS_Y, ABS_Z, ABS_RX, ABS_RY, ABS_RZ, ABS_HAT0X, ABS_HAT0Y,
];
const ABS_SLOTS: usize = ABS_HAT0Y as usize + 1;

pub const EVENT_SIZE: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    pub kind: u16,
    pub code: u16,
    pub value: i32,
}

impl Event {
    pub fn key(code: u16, pressed: bool) -> Self {
        Self {
            kind: EV_KEY,
            code,
            value: pressed as i32,
        }
    }

    pub fn abs(code: u16, value: i32) -> Self {
        Self {
            kind: EV_ABS,
            code,
            value,
        }
    }

    pub fn sync() -> Self {
        Self {
            kind: EV_SYN,
            code: SYN_REPORT,
            value: 0,
        }
    }

    pub fn decode(bytes: &[u8]) -> Vec<Event> {
        bytes
            .chunks_exact(EVENT_SIZE)
            .map(|raw| Event {
                kind: u16::from_ne_bytes([raw[16], raw[17]]),
                code: u16::from_ne_bytes([raw[18], raw[19]]),
                value: i32::from_ne_bytes([raw[20], raw[21], raw[22], raw[23]]),
            })
            .collect()
    }

    pub fn encode(&self) -> [u8; EVENT_SIZE] {
        let mut raw = [0u8; EVENT_SIZE];
        raw[16..18].copy_from_slice(&self.kind.to_ne_bytes());
        raw[18..20].copy_from_slice(&self.code.to_ne_bytes());
        raw[20..24].copy_from_slice(&self.value.to_ne_bytes());
        raw
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Extras {
    pub volume_up: bool,
    pub volume_down: bool,
    pub fn_switch: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pad {
    pub state: State,
    pub extras: Extras,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AbsRange {
    pub min: i32,
    pub max: i32,
}

impl AbsRange {
    fn is_valid(&self) -> bool {
        self.max > self.min
    }

    fn stick(&self, value: i32) -> i16 {
        if !self.is_valid() {
            return 0;
        }
        let center = (self.min as f64 + self.max as f64) / 2.0;
        let half_span = (self.max as f64 - self.min as f64) / 2.0;
        let unit = ((value as f64 - center) / half_span).clamp(-1.0, 1.0);
        (unit * 32767.0).round() as i16
    }

    fn trigger(&self, value: i32) -> u8 {
        if !self.is_valid() {
            return 0;
        }
        let unit = ((value - self.min) as f64 / (self.max - self.min) as f64).clamp(0.0, 1.0);
        (unit * 255.0).round() as u8
    }
}

fn button_for_key(code: u16) -> u16 {
    match code {
        BTN_A => xinput::A,
        BTN_B => xinput::B,
        BTN_X => xinput::X,
        BTN_Y => xinput::Y,
        BTN_TL => xinput::LEFT_SHOULDER,
        BTN_TR => xinput::RIGHT_SHOULDER,
        BTN_SELECT => xinput::BACK,
        BTN_START => xinput::START,
        BTN_MODE => xinput::GUIDE,
        BTN_THUMBL => xinput::LEFT_THUMB,
        BTN_THUMBR => xinput::RIGHT_THUMB,
        BTN_DPAD_UP => xinput::DPAD_UP,
        BTN_DPAD_DOWN => xinput::DPAD_DOWN,
        BTN_DPAD_LEFT => xinput::DPAD_LEFT,
        BTN_DPAD_RIGHT => xinput::DPAD_RIGHT,
        _ => 0,
    }
}

#[derive(Debug, Default)]
pub struct PadReader {
    extras: Extras,
    buttons: u16,
    left_trigger_key: bool,
    right_trigger_key: bool,
    values: [i32; ABS_SLOTS],
    ranges: [AbsRange; ABS_SLOTS],
}

impl PadReader {
    pub fn set_axis(&mut self, axis: u16, range: AbsRange, value: i32) {
        if let Some(slot) = self.ranges.get_mut(axis as usize) {
            *slot = range;
            self.values[axis as usize] = value;
        }
    }

    pub fn set_held_keys(&mut self, held: impl Fn(u16) -> bool) {
        self.buttons = (BTN_A..=BTN_DPAD_RIGHT)
            .filter(|&code| held(code))
            .fold(0, |bits, code| bits | button_for_key(code));
        self.left_trigger_key = held(BTN_TL2);
        self.right_trigger_key = held(BTN_TR2);
    }

    pub fn set_fn_switch(&mut self, on: bool) {
        self.extras.fn_switch = on;
    }

    pub fn pad(&self) -> Pad {
        Pad {
            state: self.state(),
            extras: self.extras,
        }
    }

    pub fn feed(&mut self, event: Event) -> bool {
        match event.kind {
            EV_SW if event.code == SW_FN => {
                self.extras.fn_switch = event.value != 0;
                false
            }
            EV_KEY => {
                let pressed = event.value != 0;
                match event.code {
                    KEY_VOLUMEUP => self.extras.volume_up = pressed,
                    KEY_VOLUMEDOWN => self.extras.volume_down = pressed,
                    BTN_TL2 => self.left_trigger_key = pressed,
                    BTN_TR2 => self.right_trigger_key = pressed,
                    code => {
                        let bit = button_for_key(code);
                        if pressed {
                            self.buttons |= bit;
                        } else {
                            self.buttons &= !bit;
                        }
                    }
                }
                false
            }
            EV_ABS => {
                if let Some(slot) = self.values.get_mut(event.code as usize) {
                    *slot = event.value;
                }
                false
            }
            EV_SYN => event.code == SYN_REPORT,
            _ => false,
        }
    }

    pub fn state(&self) -> State {
        let mut buttons = self.buttons;
        match self.values[ABS_HAT0X as usize].signum() {
            -1 => buttons |= xinput::DPAD_LEFT,
            1 => buttons |= xinput::DPAD_RIGHT,
            _ => {}
        }
        match self.values[ABS_HAT0Y as usize].signum() {
            -1 => buttons |= xinput::DPAD_UP,
            1 => buttons |= xinput::DPAD_DOWN,
            _ => {}
        }
        let axis = |code: u16| (self.ranges[code as usize], self.values[code as usize]);
        let stick = |code: u16| {
            let (range, value) = axis(code);
            range.stick(value)
        };
        let trigger = |code: u16, key: bool| {
            let (range, value) = axis(code);
            if key { 255 } else { range.trigger(value) }
        };
        State {
            buttons,
            left_trigger: trigger(ABS_Z, self.left_trigger_key),
            right_trigger: trigger(ABS_RZ, self.right_trigger_key),
            left_x: stick(ABS_X),
            left_y: stick(ABS_Y).saturating_neg(),
            right_x: stick(ABS_RX),
            right_y: stick(ABS_RY).saturating_neg(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputDevice {
    pub name: String,
    pub event_node: String,
    keys: Vec<u64>,
}

impl InputDevice {
    pub fn has_key(&self, code: u16) -> bool {
        let word = code as usize / 64;
        self.keys
            .get(word)
            .is_some_and(|bits| bits >> (code % 64) & 1 == 1)
    }

    pub fn is_gamepad(&self) -> bool {
        self.has_key(BTN_A) && self.has_key(BTN_START)
    }

    pub fn has_power_key(&self) -> bool {
        self.has_key(KEY_POWER)
    }
}

fn parse_bitmap(words: &str) -> Vec<u64> {
    words
        .split_whitespace()
        .rev()
        .map(|word| u64::from_str_radix(word, 16).unwrap_or(0))
        .collect()
}

pub fn parse_devices(text: &str) -> Vec<InputDevice> {
    text.split("\n\n")
        .filter_map(|block| {
            let mut name = None;
            let mut event_node = None;
            let mut keys = Vec::new();
            for line in block.lines() {
                if let Some(rest) = line.strip_prefix("N: Name=") {
                    name = Some(rest.trim_matches('"').to_string());
                } else if let Some(rest) = line.strip_prefix("H: Handlers=") {
                    event_node = rest
                        .split_whitespace()
                        .find(|handler| handler.starts_with("event"))
                        .map(String::from);
                } else if let Some(rest) = line.strip_prefix("B: KEY=") {
                    keys = parse_bitmap(rest);
                }
            }
            Some(InputDevice {
                name: name?,
                event_node: event_node?,
                keys,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BRICK_DEVICES: &str = include_str!("../tests/fixtures/brick-input-devices.txt");

    #[test]
    fn brick_gamepad_and_power_key_are_found() {
        let devices = parse_devices(BRICK_DEVICES);
        let pads: Vec<_> = devices
            .iter()
            .filter(|device| device.is_gamepad())
            .collect();
        assert_eq!(pads.len(), 1);
        assert_eq!(pads[0].name, "TRIMUI Player1");
        assert_eq!(pads[0].event_node, "event3");
        let power: Vec<_> = devices
            .iter()
            .filter(|device| device.has_power_key())
            .collect();
        assert_eq!(power.len(), 1);
        assert_eq!(power[0].event_node, "event1");
    }

    #[test]
    fn brick_gamepad_has_the_xpad_button_set() {
        let pad = parse_devices(BRICK_DEVICES)
            .into_iter()
            .find(InputDevice::is_gamepad)
            .unwrap();
        for code in [
            BTN_A, BTN_B, BTN_X, BTN_Y, BTN_TL, BTN_TR, BTN_SELECT, BTN_START, BTN_MODE,
        ] {
            assert!(pad.has_key(code), "missing {code:#x}");
        }
        assert!(!pad.has_key(BTN_TL2));
    }

    #[test]
    fn events_round_trip_through_bytes() {
        let events = [
            Event::key(BTN_A, true),
            Event::abs(ABS_HAT0X, -1),
            Event::sync(),
        ];
        let bytes: Vec<u8> = events.iter().flat_map(Event::encode).collect();
        assert_eq!(Event::decode(&bytes), events);
    }

    fn reader_with_brick_axes() -> PadReader {
        let mut reader = PadReader::default();
        for axis in [ABS_X, ABS_Y, ABS_RX, ABS_RY] {
            reader.set_axis(
                axis,
                AbsRange {
                    min: -32768,
                    max: 32767,
                },
                0,
            );
        }
        for axis in [ABS_Z, ABS_RZ] {
            reader.set_axis(axis, AbsRange { min: 0, max: 255 }, 0);
        }
        for axis in [ABS_HAT0X, ABS_HAT0Y] {
            reader.set_axis(axis, AbsRange { min: -1, max: 1 }, 0);
        }
        reader
    }

    #[test]
    fn state_is_ready_only_at_the_end_of_a_report() {
        let mut reader = reader_with_brick_axes();
        assert!(!reader.feed(Event::key(BTN_A, true)));
        assert!(reader.feed(Event::sync()));
        assert_eq!(reader.state().buttons, xinput::A);
    }

    #[test]
    fn release_clears_the_button() {
        let mut reader = reader_with_brick_axes();
        reader.feed(Event::key(BTN_START, true));
        reader.feed(Event::key(BTN_START, false));
        assert_eq!(reader.state().buttons, 0);
    }

    #[test]
    fn hat_becomes_dpad_buttons() {
        let mut reader = reader_with_brick_axes();
        reader.feed(Event::abs(ABS_HAT0X, -1));
        reader.feed(Event::abs(ABS_HAT0Y, 1));
        assert_eq!(
            reader.state().buttons,
            xinput::DPAD_LEFT | xinput::DPAD_DOWN
        );
    }

    #[test]
    fn dpad_keys_become_dpad_buttons() {
        let mut reader = reader_with_brick_axes();
        reader.feed(Event::key(BTN_DPAD_UP, true));
        assert_eq!(reader.state().buttons, xinput::DPAD_UP);
    }

    #[test]
    fn analog_triggers_scale_to_a_byte() {
        let mut reader = reader_with_brick_axes();
        reader.feed(Event::abs(ABS_Z, 255));
        reader.feed(Event::abs(ABS_RZ, 0));
        let state = reader.state();
        assert_eq!((state.left_trigger, state.right_trigger), (255, 0));
    }

    #[test]
    fn trigger_keys_press_the_trigger_fully() {
        let mut reader = PadReader::default();
        reader.feed(Event::key(BTN_TR2, true));
        assert_eq!(reader.state().right_trigger, 255);
    }

    #[test]
    fn stick_down_is_negative_like_xinput() {
        let mut reader = reader_with_brick_axes();
        reader.feed(Event::abs(ABS_Y, 32767));
        reader.feed(Event::abs(ABS_X, -32768));
        let state = reader.state();
        assert_eq!(state.left_y, -32767);
        assert_eq!(state.left_x, -32767);
    }

    #[test]
    fn volume_keys_and_the_fn_switch_are_kept_beside_the_buttons() {
        let mut reader = PadReader::default();
        reader.feed(Event::key(KEY_VOLUMEUP, true));
        reader.feed(Event {
            kind: EV_SW,
            code: SW_FN,
            value: 1,
        });
        reader.feed(Event::sync());
        let pad = reader.pad();
        assert!(pad.extras.volume_up && !pad.extras.volume_down && pad.extras.fn_switch);
        assert_eq!(pad.state, State::default());
    }

    #[test]
    fn unknown_axis_range_reads_as_rest() {
        let mut reader = PadReader::default();
        reader.feed(Event::abs(ABS_X, 1234));
        assert_eq!(reader.state(), State::default());
    }

    #[test]
    fn held_keys_are_loaded_from_a_snapshot() {
        let mut reader = PadReader::default();
        reader.set_held_keys(|code| code == BTN_SELECT || code == BTN_TL2);
        let state = reader.state();
        assert_eq!(state.buttons, xinput::BACK);
        assert_eq!(state.left_trigger, 255);
    }
}
