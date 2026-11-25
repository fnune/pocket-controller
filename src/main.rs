use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::Instant;

use pocket_controller::control::{Controller, Mode};
use pocket_controller::evdev::InputDevice;
use pocket_controller::gadget::{self, Layout, PortLease};
use pocket_controller::interrupt;
use pocket_controller::profile;
use pocket_controller::runtime::{Hardware, run_loop};
use pocket_controller::system::{self, Backlight, Motor, Presenter, RealFs, StateDir};

const TRIMUI_LAUNCHER: &str = "/usr/trimui/bin/MainUI";
const SAVED_BRIGHTNESS: &str = "/tmp/pocket-controller.brightness";

#[derive(Debug, Default)]
struct Options {
    mode: Option<Mode>,
    state_dir: Option<PathBuf>,
    presenter: Option<PathBuf>,
    screen: Option<PathBuf>,
    font: Option<PathBuf>,
    assets: Option<PathBuf>,
    recover_only: bool,
}

const USAGE: &str = "usage: pocket-controller [recover] [--mode automatic|xbox|gamepad] [--state-dir PATH] [--screen PATH --font PATH [--assets DIR]] [--presenter PATH]";

fn parse_options(arguments: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut options = Options::default();
    let mut arguments = arguments.peekable();
    while let Some(argument) = arguments.next() {
        let mut value = |name: &str| {
            arguments
                .next()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match argument.as_str() {
            "recover" => options.recover_only = true,
            "--mode" => {
                let text = value("--mode")?;
                options.mode =
                    Some(Mode::parse(&text).ok_or_else(|| format!("unknown mode {text:?}"))?);
            }
            "--state-dir" => options.state_dir = Some(PathBuf::from(value("--state-dir")?)),
            "--presenter" => options.presenter = Some(PathBuf::from(value("--presenter")?)),
            "--screen" => options.screen = Some(PathBuf::from(value("--screen")?)),
            "--font" => options.font = Some(PathBuf::from(value("--font")?)),
            "--assets" => options.assets = Some(PathBuf::from(value("--assets")?)),
            "--help" | "-h" => return Err(USAGE.to_string()),
            other => return Err(format!("unknown argument {other:?}\n{USAGE}")),
        }
    }
    Ok(options)
}

fn pick_devices(
    devices: &[InputDevice],
) -> Result<(InputDevice, Option<InputDevice>), &'static str> {
    let pad = devices
        .iter()
        .find(|device| device.is_gamepad())
        .cloned()
        .ok_or("no built-in controller found")?;
    let power = devices
        .iter()
        .find(|device| device.has_power_key())
        .cloned();
    Ok((pad, power))
}

fn resolve_mode(requested: Option<Mode>, saved: Option<Mode>) -> Mode {
    requested.or(saved).unwrap_or(Mode::Automatic)
}

fn run(options: Options) -> Result<(), String> {
    let fs = RealFs;
    let layout = Layout::system();
    if options.recover_only {
        if let Err(error) = system::restore_saved_brightness(Path::new(SAVED_BRIGHTNESS)) {
            eprintln!("could not restore the brightness: {error}");
        }
        return gadget::recover_stale(&fs, &layout).map_err(|error| error.to_string());
    }
    interrupt::install_wake_handler()
        .map_err(|error| format!("could not install the wake handler: {error}"))?;
    let (sender, receiver) = mpsc::channel();
    system::spawn_quit_signal_waiter(sender.clone()).map_err(|error| error.to_string())?;
    let devices =
        system::find_devices().map_err(|error| format!("could not list input devices: {error}"))?;
    let (pad, power) = pick_devices(&devices)?;
    eprintln!(
        "reading the built-in controller {:?} ({})",
        pad.name, pad.event_node
    );
    let state = options.state_dir.map(StateDir::new);
    let mode = resolve_mode(options.mode, state.as_ref().and_then(StateDir::mode));
    let model = profile::detect(
        std::env::var("DEVICE").ok().as_deref(),
        std::fs::read(TRIMUI_LAUNCHER).ok().as_deref(),
    );
    let profile = model.profile();
    eprintln!("device: {model:?}");
    let mut hardware = Hardware {
        fs,
        layout,
        lease: None,
        session: None,
        generation: 0,
        serial: system::serial_number(),
        events: sender.clone(),
        presenter: options.presenter.map(Presenter::new),
        helper: None,
        helper_paths: options.screen.zip(options.font),
        assets: options.assets,
        drawing: profile.drawing,
        motor: profile
            .motor
            .map(PathBuf::from)
            .filter(|path| path.exists())
            .map(Motor::new),
        backlight: profile
            .sunxi_backlight
            .then(|| Backlight::new(PathBuf::from(SAVED_BRIGHTNESS))),
        state,
    };
    let lease = PortLease::take(&hardware.fs, &hardware.layout)
        .map_err(|error| format!("could not take the USB port: {error}"))?;
    let snapshot = lease.snapshot();
    eprintln!(
        "borrowed gadget {} on {} (created by us: {})",
        snapshot.gadget, snapshot.controller, snapshot.created_by_us
    );
    hardware.lease = Some(lease);
    let result = (|| {
        system::spawn_pad_reader(pad, sender.clone()).map_err(|error| error.to_string())?;
        if let Some(power) = power
            && let Err(error) = system::spawn_power_reader(power, sender.clone())
        {
            eprintln!("the power button will not quit: {error}");
        }
        let saved = hardware
            .state
            .as_ref()
            .map(StateDir::saved)
            .unwrap_or_default();
        let (mut controller, actions) = Controller::with_saved(mode, saved, Instant::now());
        for action in actions {
            hardware.apply(action)?;
        }
        run_loop(&mut hardware, &mut controller, &receiver)
    })();
    hardware.shut_down();
    result
}

fn main() -> ExitCode {
    let options = match parse_options(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    match run(options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("pocket-controller: {message}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(arguments: &[&str]) -> Result<Options, String> {
        parse_options(arguments.iter().map(|argument| argument.to_string()))
    }

    const BRICK_DEVICES: &str = include_str!("../tests/fixtures/brick-input-devices.txt");

    #[test]
    fn the_brick_yields_its_gamepad_and_a_separate_power_key_device() {
        let devices = pocket_controller::evdev::parse_devices(BRICK_DEVICES);
        let (pad, power) = pick_devices(&devices).unwrap();
        assert_eq!(pad.event_node, "event3");
        assert_eq!(power.unwrap().event_node, "event1");
    }

    #[test]
    fn a_machine_with_no_gamepad_is_refused_with_a_message() {
        assert_eq!(
            pick_devices(&[]).unwrap_err(),
            "no built-in controller found"
        );
    }

    #[test]
    fn a_missing_power_key_device_does_not_block_startup() {
        let without_power: String = BRICK_DEVICES
            .split("\n\n")
            .filter(|block| !block.contains("N: Name=\"axp"))
            .collect::<Vec<_>>()
            .join("\n\n");
        let devices = pocket_controller::evdev::parse_devices(&without_power);
        assert!(devices.iter().any(InputDevice::is_gamepad));
        assert!(pick_devices(&devices).unwrap().1.is_none());
    }

    #[test]
    fn the_flag_beats_the_saved_mode_which_beats_automatic() {
        assert_eq!(
            resolve_mode(Some(Mode::Xbox), Some(Mode::Gamepad)),
            Mode::Xbox
        );
        assert_eq!(resolve_mode(None, Some(Mode::Gamepad)), Mode::Gamepad);
        assert_eq!(resolve_mode(None, None), Mode::Automatic);
    }

    #[test]
    fn no_arguments_leave_every_option_unset() {
        let options = parse(&[]).unwrap();
        assert_eq!(options.mode, None);
        assert_eq!(options.state_dir, None);
        assert!(!options.recover_only);
    }

    #[test]
    fn recover_selects_recovery_only() {
        assert!(parse(&["recover"]).unwrap().recover_only);
    }

    #[test]
    fn every_mode_name_is_accepted() {
        for (name, mode) in [
            ("automatic", Mode::Automatic),
            ("xbox", Mode::Xbox),
            ("gamepad", Mode::Gamepad),
        ] {
            assert_eq!(parse(&["--mode", name]).unwrap().mode, Some(mode));
        }
    }

    #[test]
    fn an_unknown_mode_is_rejected_with_its_text() {
        let error = parse(&["--mode", "ps5"]).unwrap_err();
        assert!(error.contains("ps5"));
    }

    #[test]
    fn path_options_are_stored_as_given() {
        let options = parse(&[
            "--state-dir",
            "/state",
            "--presenter",
            "/bin/presenter",
            "--screen",
            "/bin/screen",
            "--font",
            "/fonts/a.ttf",
            "--assets",
            "/assets",
        ])
        .unwrap();
        assert_eq!(options.state_dir, Some(PathBuf::from("/state")));
        assert_eq!(options.presenter, Some(PathBuf::from("/bin/presenter")));
        assert_eq!(options.screen, Some(PathBuf::from("/bin/screen")));
        assert_eq!(options.font, Some(PathBuf::from("/fonts/a.ttf")));
        assert_eq!(options.assets, Some(PathBuf::from("/assets")));
    }

    #[test]
    fn an_option_missing_its_value_names_the_option() {
        for name in [
            "--mode",
            "--state-dir",
            "--presenter",
            "--screen",
            "--font",
            "--assets",
        ] {
            let error = parse(&[name]).unwrap_err();
            assert_eq!(error, format!("{name} needs a value"));
        }
    }

    #[test]
    fn an_unknown_argument_is_rejected_and_followed_by_the_usage() {
        let error = parse(&["--turbo"]).unwrap_err();
        assert!(error.contains("--turbo"));
        assert!(error.ends_with(USAGE));
    }

    #[test]
    fn help_in_either_spelling_returns_the_usage() {
        assert_eq!(parse(&["--help"]).unwrap_err(), USAGE);
        assert_eq!(parse(&["-h"]).unwrap_err(), USAGE);
    }

    #[test]
    fn a_later_occurrence_of_an_option_wins() {
        let options = parse(&["--mode", "xbox", "--mode", "gamepad"]).unwrap();
        assert_eq!(options.mode, Some(Mode::Gamepad));
    }
}
