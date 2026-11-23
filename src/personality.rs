use crate::control::{Input, Kind};
use crate::gadget::Identity;
use crate::hid::{self, SetupReply, SetupRequest};
use crate::xinput::{self, EndpointBudget, State};

const MANUFACTURER: &str = "Pocket Controller";
const XBOX_LED_REPORT_TYPE: u8 = 0x01;

pub trait Personality: Sync {
    fn identity(&self) -> Identity;
    fn descriptors(&self, budget: EndpointBudget) -> Vec<Vec<u8>>;
    fn budgets(&self, known: Option<EndpointBudget>) -> Vec<EndpointBudget>;
    fn has_out_endpoint(&self) -> bool;
    fn encode(&self, state: &State) -> Vec<u8>;
    fn answer_setup(&self, setup: &SetupRequest, report: &[u8]) -> SetupReply;
    fn read_output(&self, report: &[u8]) -> Vec<Input>;

    fn strings(&self) -> Vec<u8> {
        xinput::functionfs_strings()
    }
}

pub struct XboxStyle;
pub struct Generic;

pub fn for_kind(kind: Kind) -> &'static dyn Personality {
    match kind {
        Kind::Xbox => &XboxStyle,
        Kind::Gamepad => &Generic,
    }
}

impl Personality for XboxStyle {
    fn identity(&self) -> Identity {
        Identity {
            vendor: xinput::VENDOR_ID,
            product: xinput::PRODUCT_ID,
            release: xinput::DEVICE_RELEASE,
            device_class: 0xff,
            manufacturer: MANUFACTURER,
            product_name: "Pocket Controller",
        }
    }

    fn descriptors(&self, budget: EndpointBudget) -> Vec<Vec<u8>> {
        vec![
            xinput::functionfs_descriptors(true, budget),
            xinput::functionfs_descriptors(false, budget),
        ]
    }

    fn budgets(&self, known: Option<EndpointBudget>) -> Vec<EndpointBudget> {
        match known {
            Some(budget) => vec![budget],
            None => vec![EndpointBudget::TwoInterrupt, EndpointBudget::OneInterrupt],
        }
    }

    fn has_out_endpoint(&self) -> bool {
        true
    }

    fn encode(&self, state: &State) -> Vec<u8> {
        xinput::input_report(state).to_vec()
    }

    fn answer_setup(&self, setup: &SetupRequest, report: &[u8]) -> SetupReply {
        xinput::answer_setup(setup, report)
    }

    fn read_output(&self, report: &[u8]) -> Vec<Input> {
        if report.first() == Some(&XBOX_LED_REPORT_TYPE) {
            return vec![Input::XboxDriverSpoke];
        }
        match xinput::parse_output_report(report) {
            Some(rumble) => vec![Input::XboxDriverSpoke, Input::Rumble(rumble.level())],
            None => Vec::new(),
        }
    }
}

impl Personality for Generic {
    fn identity(&self) -> Identity {
        Identity {
            vendor: hid::VENDOR_ID,
            product: hid::PRODUCT_ID,
            release: hid::DEVICE_RELEASE,
            device_class: 0x00,
            manufacturer: MANUFACTURER,
            product_name: "Gamepad",
        }
    }

    fn descriptors(&self, _budget: EndpointBudget) -> Vec<Vec<u8>> {
        vec![hid::functionfs_descriptors()]
    }

    fn budgets(&self, _known: Option<EndpointBudget>) -> Vec<EndpointBudget> {
        vec![EndpointBudget::TwoInterrupt]
    }

    fn has_out_endpoint(&self) -> bool {
        false
    }

    fn encode(&self, state: &State) -> Vec<u8> {
        hid::input_report(state).to_vec()
    }

    fn answer_setup(&self, setup: &SetupRequest, report: &[u8]) -> SetupReply {
        let mut current = [0u8; hid::REPORT_LEN];
        if report.len() == hid::REPORT_LEN {
            current.copy_from_slice(report);
        }
        hid::answer_setup(setup, &current)
    }

    fn read_output(&self, _report: &[u8]) -> Vec<Input> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xinput::EndpointBudget::{OneInterrupt, TwoInterrupt};

    #[test]
    fn a_known_endpoint_budget_is_the_only_one_tried() {
        assert_eq!(XboxStyle.budgets(None), vec![TwoInterrupt, OneInterrupt]);
        assert_eq!(XboxStyle.budgets(Some(OneInterrupt)), vec![OneInterrupt]);
        assert_eq!(Generic.budgets(Some(OneInterrupt)), vec![TwoInterrupt]);
    }

    #[test]
    fn xbox_and_gamepad_identities_differ_so_hosts_do_not_reuse_a_cached_driver() {
        let xbox = for_kind(Kind::Xbox).identity();
        let gamepad = for_kind(Kind::Gamepad).identity();
        assert_eq!((xbox.vendor, xbox.product), (0x045e, 0x028e));
        assert_ne!(
            (xbox.vendor, xbox.product),
            (gamepad.vendor, gamepad.product)
        );
    }

    #[test]
    fn reports_are_encoded_per_personality() {
        assert_eq!(
            XboxStyle.encode(&State::default()).len(),
            xinput::REPORT_LEN
        );
        assert_eq!(Generic.encode(&State::default()).len(), hid::REPORT_LEN);
    }

    #[test]
    fn the_xbox_style_falls_back_to_descriptors_without_the_xusb_block() {
        let candidates = XboxStyle.descriptors(TwoInterrupt);
        assert_eq!(candidates.len(), 2);
        assert!(candidates[0].len() > candidates[1].len());
    }

    #[test]
    fn an_led_report_or_rumble_means_the_xbox_driver_is_there() {
        assert_eq!(
            XboxStyle.read_output(&[0x01, 0x03, 0x02]),
            vec![Input::XboxDriverSpoke]
        );
        let rumble = XboxStyle.read_output(&[0x00, 0x08, 0x00, 0xff, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(rumble.first(), Some(&Input::XboxDriverSpoke));
        assert!(matches!(rumble.get(1), Some(Input::Rumble(_))));
        assert!(Generic.read_output(&[0x00, 0x08, 0x00, 0xff]).is_empty());
    }
}
