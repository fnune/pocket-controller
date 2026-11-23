use crate::xinput::{self, State};

pub const VENDOR_ID: u16 = 0x1d6b;
pub const PRODUCT_ID: u16 = 0x0105;
pub const DEVICE_RELEASE: u16 = 0x0100;

pub const REPORT_LEN: usize = 7;

const BUTTON_ORDER: [u16; 16] = [
    xinput::A,
    xinput::B,
    0,
    xinput::X,
    xinput::Y,
    0,
    xinput::LEFT_SHOULDER,
    xinput::RIGHT_SHOULDER,
    0,
    0,
    xinput::BACK,
    xinput::START,
    xinput::GUIDE,
    xinput::LEFT_THUMB,
    xinput::RIGHT_THUMB,
    0,
];
const LEFT_TRIGGER_BUTTON: u16 = 1 << 8;
const RIGHT_TRIGGER_BUTTON: u16 = 1 << 9;
const TRIGGER_PRESSED_ABOVE: u8 = 127;
const HAT_CENTERED: u8 = 8;

const REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x01, 0x09, 0x05, 0xa1, 0x01, 0x05, 0x09, 0x19, 0x01, 0x29, 0x10, 0x15, 0x00, 0x25, 0x01,
    0x75, 0x01, 0x95, 0x10, 0x81, 0x02, 0x05, 0x01, 0x09, 0x39, 0x15, 0x00, 0x25, 0x07, 0x35, 0x00,
    0x46, 0x3b, 0x01, 0x65, 0x14, 0x75, 0x04, 0x95, 0x01, 0x81, 0x42, 0x65, 0x00, 0x45, 0x00, 0x75,
    0x04, 0x95, 0x01, 0x81, 0x03, 0x09, 0x30, 0x09, 0x31, 0x09, 0x32, 0x09, 0x35, 0x15, 0x00, 0x26,
    0xff, 0x00, 0x75, 0x08, 0x95, 0x04, 0x81, 0x02, 0xc0,
];

pub fn report_descriptor() -> &'static [u8] {
    REPORT_DESCRIPTOR
}

pub fn hid_descriptor() -> [u8; 9] {
    let [low, high] = (REPORT_DESCRIPTOR.len() as u16).to_le_bytes();
    [9, 0x21, 0x11, 0x01, 0x00, 0x01, 0x22, low, high]
}

fn hat(buttons: u16) -> u8 {
    let up = buttons & xinput::DPAD_UP != 0;
    let down = buttons & xinput::DPAD_DOWN != 0;
    let left = buttons & xinput::DPAD_LEFT != 0;
    let right = buttons & xinput::DPAD_RIGHT != 0;
    match (up, right, down, left) {
        (true, true, _, _) => 1,
        (_, true, true, _) => 3,
        (_, _, true, true) => 5,
        (true, _, _, true) => 7,
        (true, _, _, _) => 0,
        (_, true, _, _) => 2,
        (_, _, true, _) => 4,
        (_, _, _, true) => 6,
        _ => HAT_CENTERED,
    }
}

fn axis_byte(value: i32) -> u8 {
    ((value + 32768) >> 8).clamp(0, 255) as u8
}

pub fn input_report(state: &State) -> [u8; REPORT_LEN] {
    let mut buttons = BUTTON_ORDER
        .iter()
        .enumerate()
        .filter(|&(_, &mask)| mask != 0 && state.buttons & mask != 0)
        .fold(0u16, |bits, (index, _)| bits | 1 << index);
    if state.left_trigger > TRIGGER_PRESSED_ABOVE {
        buttons |= LEFT_TRIGGER_BUTTON;
    }
    if state.right_trigger > TRIGGER_PRESSED_ABOVE {
        buttons |= RIGHT_TRIGGER_BUTTON;
    }
    let [low, high] = buttons.to_le_bytes();
    [
        low,
        high,
        hat(state.buttons),
        axis_byte(state.left_x as i32),
        axis_byte(-(state.left_y as i32)),
        axis_byte(state.right_x as i32),
        axis_byte(-(state.right_y as i32)),
    ]
}

const INTERFACE_DESCRIPTOR: [u8; 9] = [9, 4, 0, 0, 1, 3, 0, 0, 0];
const ENDPOINT_MAX_PACKET: u8 = 8;
const FULL_SPEED_INTERVAL_MS: u8 = 1;
const HIGH_SPEED_INTERVAL_EXPONENT: u8 = 4;

fn interface_descriptors(interval: u8) -> Vec<u8> {
    let mut descriptors = INTERFACE_DESCRIPTOR.to_vec();
    descriptors.extend_from_slice(&hid_descriptor());
    descriptors.extend_from_slice(&[7, 5, 0x81, 3, ENDPOINT_MAX_PACKET, 0, interval]);
    descriptors
}

pub fn functionfs_descriptors() -> Vec<u8> {
    crate::xinput::functionfs_blob(
        &interface_descriptors(FULL_SPEED_INTERVAL_MS),
        &interface_descriptors(HIGH_SPEED_INTERVAL_EXPONENT),
        3,
    )
}

#[derive(Debug, PartialEq, Eq)]
pub enum SetupReply {
    Data(Vec<u8>),
    Acknowledge,
    Stall,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetupRequest {
    pub request_type: u8,
    pub request: u8,
    pub value: u16,
    pub index: u16,
    pub length: u16,
}

impl SetupRequest {
    pub fn parse(bytes: &[u8; 8]) -> Self {
        Self {
            request_type: bytes[0],
            request: bytes[1],
            value: u16::from_le_bytes([bytes[2], bytes[3]]),
            index: u16::from_le_bytes([bytes[4], bytes[5]]),
            length: u16::from_le_bytes([bytes[6], bytes[7]]),
        }
    }

    pub fn is_device_to_host(&self) -> bool {
        self.request_type & 0x80 != 0
    }
}

const STANDARD_INTERFACE_IN: u8 = 0x81;
const CLASS_INTERFACE_IN: u8 = 0xa1;
const CLASS_INTERFACE_OUT: u8 = 0x21;
const GET_DESCRIPTOR: u8 = 0x06;
const DESCRIPTOR_TYPE_HID: u8 = 0x21;
const DESCRIPTOR_TYPE_REPORT: u8 = 0x22;
const GET_REPORT: u8 = 0x01;
const GET_IDLE: u8 = 0x02;
const GET_PROTOCOL: u8 = 0x03;
const SET_REPORT: u8 = 0x09;
const SET_IDLE: u8 = 0x0a;
const SET_PROTOCOL: u8 = 0x0b;
const REPORT_PROTOCOL: u8 = 1;

pub fn answer_setup(setup: &SetupRequest, current_report: &[u8; REPORT_LEN]) -> SetupReply {
    let truncated =
        |bytes: &[u8]| SetupReply::Data(bytes[..bytes.len().min(setup.length as usize)].to_vec());
    let descriptor_type = (setup.value >> 8) as u8;
    match (setup.request_type, setup.request) {
        (STANDARD_INTERFACE_IN, GET_DESCRIPTOR) if descriptor_type == DESCRIPTOR_TYPE_REPORT => {
            truncated(report_descriptor())
        }
        (STANDARD_INTERFACE_IN, GET_DESCRIPTOR) if descriptor_type == DESCRIPTOR_TYPE_HID => {
            truncated(&hid_descriptor())
        }
        (CLASS_INTERFACE_IN, GET_REPORT) => truncated(current_report),
        (CLASS_INTERFACE_IN, GET_IDLE) => truncated(&[0]),
        (CLASS_INTERFACE_IN, GET_PROTOCOL) => truncated(&[REPORT_PROTOCOL]),
        (CLASS_INTERFACE_OUT, SET_REPORT | SET_IDLE | SET_PROTOCOL) => SetupReply::Acknowledge,
        _ => SetupReply::Stall,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_descriptor_ends_its_collection() {
        let descriptor = report_descriptor();
        assert_eq!(descriptor.len(), 73);
        assert_eq!(&descriptor[..6], &[0x05, 0x01, 0x09, 0x05, 0xa1, 0x01]);
        assert_eq!(descriptor.last(), Some(&0xc0));
    }

    #[test]
    fn report_descriptor_declares_seven_byte_reports() {
        let descriptor = report_descriptor();
        let mut bits = 0usize;
        let mut size = 0usize;
        let mut count = 0usize;
        let mut at = 0;
        while at < descriptor.len() {
            let prefix = descriptor[at];
            let data_len = match prefix & 0x03 {
                3 => 4,
                n => n as usize,
            };
            let data = descriptor[at + 1..at + 1 + data_len]
                .iter()
                .rev()
                .fold(0usize, |value, &byte| value << 8 | byte as usize);
            match prefix & 0xfc {
                0x74 => size = data,
                0x94 => count = data,
                0x80 => bits += size * count,
                _ => {}
            }
            at += 1 + data_len;
        }
        assert_eq!(bits, REPORT_LEN * 8);
    }

    #[test]
    fn hid_descriptor_names_the_report_descriptor_length() {
        let descriptor = hid_descriptor();
        assert_eq!(descriptor[1], 0x21);
        assert_eq!(descriptor[6], 0x22);
        assert_eq!(u16::from_le_bytes([descriptor[7], descriptor[8]]), 73);
    }

    #[test]
    fn idle_report_is_centered() {
        assert_eq!(
            input_report(&State::default()),
            [0, 0, 8, 128, 128, 128, 128]
        );
    }

    #[test]
    fn face_buttons_follow_the_linux_gamepad_order() {
        let report = input_report(&State {
            buttons: xinput::A | xinput::B | xinput::X | xinput::Y,
            ..State::default()
        });
        assert_eq!(u16::from_le_bytes([report[0], report[1]]), 0b11011);
    }

    #[test]
    fn select_start_and_guide_are_buttons_eleven_to_thirteen() {
        let report = input_report(&State {
            buttons: xinput::BACK | xinput::START | xinput::GUIDE,
            ..State::default()
        });
        assert_eq!(
            u16::from_le_bytes([report[0], report[1]]),
            0b1_1100_0000_0000
        );
    }

    #[test]
    fn pressed_triggers_become_buttons_nine_and_ten() {
        let report = input_report(&State {
            left_trigger: 255,
            right_trigger: 128,
            ..State::default()
        });
        assert_eq!(u16::from_le_bytes([report[0], report[1]]), 0b11_0000_0000);
    }

    #[test]
    fn dpad_becomes_a_clockwise_hat() {
        let cases = [
            (xinput::DPAD_UP, 0),
            (xinput::DPAD_UP | xinput::DPAD_RIGHT, 1),
            (xinput::DPAD_RIGHT, 2),
            (xinput::DPAD_DOWN | xinput::DPAD_RIGHT, 3),
            (xinput::DPAD_DOWN, 4),
            (xinput::DPAD_DOWN | xinput::DPAD_LEFT, 5),
            (xinput::DPAD_LEFT, 6),
            (xinput::DPAD_UP | xinput::DPAD_LEFT, 7),
        ];
        for (buttons, expected) in cases {
            let report = input_report(&State {
                buttons,
                ..State::default()
            });
            assert_eq!(report[2], expected, "buttons {buttons:#06x}");
        }
    }

    #[test]
    fn stick_up_is_hid_low() {
        let report = input_report(&State {
            left_y: 32767,
            right_x: -32768,
            ..State::default()
        });
        assert_eq!(report[4], 0);
        assert_eq!(report[5], 0);
    }

    fn request(request_type: u8, request: u8, value: u16, length: u16) -> SetupRequest {
        SetupRequest {
            request_type,
            request,
            value,
            index: 0,
            length,
        }
    }

    #[test]
    fn host_gets_the_report_descriptor_truncated_to_its_length() {
        let report = [0; REPORT_LEN];
        assert_eq!(
            answer_setup(&request(0x81, 0x06, 0x2200, 200), &report),
            SetupReply::Data(report_descriptor().to_vec())
        );
        assert_eq!(
            answer_setup(&request(0x81, 0x06, 0x2200, 4), &report),
            SetupReply::Data(report_descriptor()[..4].to_vec())
        );
    }

    #[test]
    fn host_gets_the_current_report() {
        let report = [1, 2, 3, 4, 5, 6, 7];
        assert_eq!(
            answer_setup(&request(0xa1, 0x01, 0x0100, 7), &report),
            SetupReply::Data(report.to_vec())
        );
    }

    #[test]
    fn set_idle_is_acknowledged_and_unknown_requests_stall() {
        let report = [0; REPORT_LEN];
        assert_eq!(
            answer_setup(&request(0x21, 0x0a, 0, 0), &report),
            SetupReply::Acknowledge
        );
        assert_eq!(
            answer_setup(&request(0xc1, 0x01, 0, 4), &report),
            SetupReply::Stall
        );
    }

    #[test]
    fn setup_request_parses_little_endian_fields() {
        let setup = SetupRequest::parse(&[0x81, 0x06, 0x00, 0x22, 0x00, 0x00, 0x49, 0x00]);
        assert_eq!(setup, request(0x81, 0x06, 0x2200, 0x49));
        assert!(setup.is_device_to_host());
    }
}
