use crate::hid::{SetupReply, SetupRequest};

pub const DPAD_UP: u16 = 1 << 0;
pub const DPAD_DOWN: u16 = 1 << 1;
pub const DPAD_LEFT: u16 = 1 << 2;
pub const DPAD_RIGHT: u16 = 1 << 3;
pub const START: u16 = 1 << 4;
pub const BACK: u16 = 1 << 5;
pub const LEFT_THUMB: u16 = 1 << 6;
pub const RIGHT_THUMB: u16 = 1 << 7;
pub const LEFT_SHOULDER: u16 = 1 << 8;
pub const RIGHT_SHOULDER: u16 = 1 << 9;
pub const GUIDE: u16 = 1 << 10;
pub const A: u16 = 1 << 12;
pub const B: u16 = 1 << 13;
pub const X: u16 = 1 << 14;
pub const Y: u16 = 1 << 15;

pub const REPORT_LEN: usize = 20;

pub const VENDOR_ID: u16 = 0x045e;
pub const PRODUCT_ID: u16 = 0x028e;
pub const DEVICE_RELEASE: u16 = 0x0114;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub buttons: u16,
    pub left_trigger: u8,
    pub right_trigger: u8,
    pub left_x: i16,
    pub left_y: i16,
    pub right_x: i16,
    pub right_y: i16,
}

impl State {
    pub fn holds(&self, mask: u16) -> bool {
        self.buttons & mask == mask
    }
}

pub fn input_report(state: &State) -> [u8; REPORT_LEN] {
    let mut report = [0u8; REPORT_LEN];
    report[0] = 0x00;
    report[1] = REPORT_LEN as u8;
    report[2..4].copy_from_slice(&state.buttons.to_le_bytes());
    report[4] = state.left_trigger;
    report[5] = state.right_trigger;
    report[6..8].copy_from_slice(&state.left_x.to_le_bytes());
    report[8..10].copy_from_slice(&state.left_y.to_le_bytes());
    report[10..12].copy_from_slice(&state.right_x.to_le_bytes());
    report[12..14].copy_from_slice(&state.right_y.to_le_bytes());
    report
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rumble {
    pub strong: u8,
    pub weak: u8,
}

impl Rumble {
    pub fn level(&self) -> u8 {
        self.strong.max(self.weak)
    }
}

pub fn parse_output_report(report: &[u8]) -> Option<Rumble> {
    const RUMBLE_TYPE: u8 = 0x00;
    const RUMBLE_MIN_LEN: usize = 5;
    if report.len() < RUMBLE_MIN_LEN
        || report[0] != RUMBLE_TYPE
        || (report[1] as usize) < RUMBLE_MIN_LEN
    {
        return None;
    }
    Some(Rumble {
        strong: report[3],
        weak: report[4],
    })
}

const XUSB_DESCRIPTOR: [u8; 17] = [
    0x11, 0x21, 0x00, 0x01, 0x01, 0x25, 0x81, 0x14, 0x00, 0x00, 0x00, 0x00, 0x13, 0x01, 0x08, 0x00,
    0x00,
];

const INTERFACE_DESCRIPTOR: [u8; 9] = [9, 4, 0, 0, 2, 0xff, 0x5d, 0x01, 0];

const ENDPOINT_IN: u8 = 0x81;
const ENDPOINT_OUT: u8 = 0x02;
const ENDPOINT_INTERRUPT: u8 = 3;
const ENDPOINT_MAX_PACKET: u16 = 32;

const FULL_SPEED_INTERVAL_IN_MS: u8 = 1;
const FULL_SPEED_INTERVAL_OUT_MS: u8 = 8;
const HIGH_SPEED_INTERVAL_IN_EXPONENT: u8 = 4;
const HIGH_SPEED_INTERVAL_OUT_EXPONENT: u8 = 7;

const FUNCTIONFS_DESCRIPTORS_MAGIC_V2: u32 = 3;
const FUNCTIONFS_HAS_FS_DESC: u32 = 1;
const FUNCTIONFS_HAS_HS_DESC: u32 = 2;
const FUNCTIONFS_STRINGS_MAGIC: u32 = 2;

const ENDPOINT_BULK: u8 = 2;

fn endpoint(address: u8, transfer: u8, max_packet: u16, interval: u8) -> [u8; 7] {
    let [low, high] = max_packet.to_le_bytes();
    [7, 5, address, transfer, low, high, interval]
}

#[derive(Clone, Copy)]
enum OutTransfer {
    Interrupt(u8),
    Bulk,
}

fn interface_descriptors(include_xusb: bool, interval_in: u8, out: OutTransfer) -> Vec<u8> {
    let mut descriptors = INTERFACE_DESCRIPTOR.to_vec();
    if include_xusb {
        descriptors.extend_from_slice(&XUSB_DESCRIPTOR);
    }
    descriptors.extend_from_slice(&endpoint(
        ENDPOINT_IN,
        ENDPOINT_INTERRUPT,
        ENDPOINT_MAX_PACKET,
        interval_in,
    ));
    let out_endpoint = match out {
        OutTransfer::Interrupt(interval) => endpoint(
            ENDPOINT_OUT,
            ENDPOINT_INTERRUPT,
            ENDPOINT_MAX_PACKET,
            interval,
        ),
        OutTransfer::Bulk => endpoint(ENDPOINT_OUT, ENDPOINT_BULK, ENDPOINT_MAX_PACKET, 0),
    };
    descriptors.extend_from_slice(&out_endpoint);
    descriptors
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointBudget {
    TwoInterrupt,
    OneInterrupt,
}

impl EndpointBudget {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "two-interrupt" => Some(Self::TwoInterrupt),
            "one-interrupt" => Some(Self::OneInterrupt),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::TwoInterrupt => "two-interrupt",
            Self::OneInterrupt => "one-interrupt",
        }
    }
}

pub fn functionfs_descriptors(include_xusb: bool, budget: EndpointBudget) -> Vec<u8> {
    let count: u32 = if include_xusb { 4 } else { 3 };
    let full_speed_out = match budget {
        EndpointBudget::TwoInterrupt => OutTransfer::Interrupt(FULL_SPEED_INTERVAL_OUT_MS),
        EndpointBudget::OneInterrupt => OutTransfer::Bulk,
    };
    functionfs_blob(
        &interface_descriptors(include_xusb, FULL_SPEED_INTERVAL_IN_MS, full_speed_out),
        &interface_descriptors(
            include_xusb,
            HIGH_SPEED_INTERVAL_IN_EXPONENT,
            OutTransfer::Interrupt(HIGH_SPEED_INTERVAL_OUT_EXPONENT),
        ),
        count,
    )
}

pub fn functionfs_blob(full_speed: &[u8], high_speed: &[u8], count: u32) -> Vec<u8> {
    const HEADER_LEN: usize = 20;
    let total = (HEADER_LEN + full_speed.len() + high_speed.len()) as u32;
    let mut blob = Vec::with_capacity(total as usize);
    for word in [
        FUNCTIONFS_DESCRIPTORS_MAGIC_V2,
        total,
        FUNCTIONFS_HAS_FS_DESC | FUNCTIONFS_HAS_HS_DESC,
        count,
        count,
    ] {
        blob.extend_from_slice(&word.to_le_bytes());
    }
    blob.extend_from_slice(full_speed);
    blob.extend_from_slice(high_speed);
    blob
}

pub fn functionfs_strings() -> Vec<u8> {
    const LEN: u32 = 16;
    let mut blob = Vec::with_capacity(LEN as usize);
    for word in [FUNCTIONFS_STRINGS_MAGIC, LEN, 0, 0] {
        blob.extend_from_slice(&word.to_le_bytes());
    }
    blob
}

const VENDOR_INTERFACE_IN: u8 = 0xc1;
const GET_INPUT_REPORT: u8 = 0x01;

pub fn answer_setup(setup: &SetupRequest, current_report: &[u8]) -> SetupReply {
    match (setup.request_type, setup.request) {
        (VENDOR_INTERFACE_IN, GET_INPUT_REPORT) => SetupReply::Data(
            current_report[..current_report.len().min(setup.length as usize)].to_vec(),
        ),
        _ => SetupReply::Stall,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_report_is_header_and_zeros() {
        let report = input_report(&State::default());
        assert_eq!(report[0], 0x00);
        assert_eq!(report[1], 20);
        assert!(report[2..].iter().all(|&byte| byte == 0));
    }

    #[test]
    fn buttons_land_in_bytes_two_and_three() {
        let report = input_report(&State {
            buttons: DPAD_UP | START | A | Y | GUIDE,
            ..State::default()
        });
        assert_eq!(report[2], 0b0001_0001);
        assert_eq!(report[3], 0b1001_0100);
    }

    #[test]
    fn triggers_and_sticks_are_little_endian() {
        let report = input_report(&State {
            left_trigger: 255,
            right_trigger: 7,
            left_x: -32768,
            left_y: 32767,
            right_x: 1,
            right_y: -1,
            ..State::default()
        });
        assert_eq!(&report[4..6], &[255, 7]);
        assert_eq!(&report[6..8], &[0x00, 0x80]);
        assert_eq!(&report[8..10], &[0xff, 0x7f]);
        assert_eq!(&report[10..12], &[0x01, 0x00]);
        assert_eq!(&report[12..14], &[0xff, 0xff]);
        assert!(report[14..].iter().all(|&byte| byte == 0));
    }

    #[test]
    fn rumble_report_gives_both_motors() {
        let rumble = parse_output_report(&[0x00, 0x08, 0x00, 0x80, 0x20, 0, 0, 0]).unwrap();
        assert_eq!(
            rumble,
            Rumble {
                strong: 0x80,
                weak: 0x20
            }
        );
        assert_eq!(rumble.level(), 0x80);
    }

    #[test]
    fn led_report_is_not_rumble() {
        assert_eq!(parse_output_report(&[0x01, 0x03, 0x06]), None);
    }

    #[test]
    fn short_report_is_not_rumble() {
        assert_eq!(parse_output_report(&[0x00, 0x08, 0x00]), None);
    }

    fn word(blob: &[u8], index: usize) -> u32 {
        u32::from_le_bytes(blob[index * 4..index * 4 + 4].try_into().unwrap())
    }

    #[test]
    fn descriptor_blob_header_matches_its_contents() {
        for include_xusb in [true, false] {
            let blob = functionfs_descriptors(include_xusb, EndpointBudget::TwoInterrupt);
            assert_eq!(word(&blob, 0), 3);
            assert_eq!(word(&blob, 1) as usize, blob.len());
            assert_eq!(word(&blob, 2), 3);
            let expected_count = if include_xusb { 4 } else { 3 };
            assert_eq!(word(&blob, 3), expected_count);
            assert_eq!(word(&blob, 4), expected_count);
        }
    }

    fn walk_descriptors(mut bytes: &[u8]) -> Vec<(u8, Vec<u8>)> {
        let mut found = Vec::new();
        while !bytes.is_empty() {
            let len = bytes[0] as usize;
            found.push((bytes[1], bytes[..len].to_vec()));
            bytes = &bytes[len..];
        }
        found
    }

    #[test]
    fn interface_is_xbox_360_vendor_class_with_interrupt_pipes() {
        let blob = functionfs_descriptors(false, EndpointBudget::TwoInterrupt);
        let descriptors = walk_descriptors(&blob[20..]);
        assert_eq!(descriptors.len(), 6);
        let (kind, interface) = &descriptors[0];
        assert_eq!(*kind, 4);
        assert_eq!(&interface[4..8], &[2, 0xff, 0x5d, 0x01]);
        let (_, endpoint_in) = &descriptors[1];
        assert_eq!(endpoint_in[2], 0x81);
        assert_eq!(endpoint_in[3], 3);
        let (_, endpoint_out) = &descriptors[2];
        assert_eq!(endpoint_out[2], 0x02);
    }

    #[test]
    fn xusb_descriptor_follows_the_interface_when_included() {
        let blob = functionfs_descriptors(true, EndpointBudget::TwoInterrupt);
        let descriptors = walk_descriptors(&blob[20..]);
        assert_eq!(descriptors.len(), 8);
        assert_eq!(descriptors[1].0, 0x21);
        assert_eq!(descriptors[1].1.len(), 17);
    }

    #[test]
    fn one_interrupt_budget_uses_bulk_out_at_full_speed_and_interrupt_out_at_high_speed() {
        let blob = functionfs_descriptors(false, EndpointBudget::OneInterrupt);
        let descriptors = walk_descriptors(&blob[20..]);
        let full_speed_out = &descriptors[2].1;
        let high_speed_out = &descriptors[5].1;
        assert_eq!((full_speed_out[2], full_speed_out[3]), (0x02, 2));
        assert_eq!((high_speed_out[2], high_speed_out[3]), (0x02, 3));
    }

    #[test]
    fn xpad_magic_request_gets_the_current_report() {
        let setup = SetupRequest::parse(&[0xc1, 0x01, 0x00, 0x01, 0x00, 0x00, 20, 0]);
        let report = input_report(&State::default());
        assert_eq!(
            answer_setup(&setup, &report),
            SetupReply::Data(report.to_vec())
        );
    }

    #[test]
    fn other_vendor_requests_stall() {
        let setup = SetupRequest::parse(&[0xc1, 0x02, 0x00, 0x01, 0x00, 0x00, 20, 0]);
        assert_eq!(answer_setup(&setup, &[0; REPORT_LEN]), SetupReply::Stall);
    }

    #[test]
    fn endpoint_budget_names_round_trip() {
        for budget in [EndpointBudget::TwoInterrupt, EndpointBudget::OneInterrupt] {
            assert_eq!(EndpointBudget::parse(budget.as_str()), Some(budget));
        }
    }

    #[test]
    fn strings_blob_declares_no_strings() {
        let blob = functionfs_strings();
        assert_eq!(blob.len(), 16);
        assert_eq!(word(&blob, 0), 2);
        assert_eq!(word(&blob, 1), 16);
        assert_eq!(word(&blob, 2), 0);
        assert_eq!(word(&blob, 3), 0);
    }
}
