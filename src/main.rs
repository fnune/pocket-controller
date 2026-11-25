use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use pocket_controller::control::{Action, Controller, Input, Kind, Light, Mode, Screen, UsbLink};
use pocket_controller::evdev::InputDevice;
use pocket_controller::functionfs::{Session, SessionEvents};
use pocket_controller::gadget::{self, Layout, PortLease};
use pocket_controller::interrupt;
use pocket_controller::profile;
use pocket_controller::system::{
    self, Backlight, Motor, Presenter, RealFs, ScreenHelper, StateDir,
};

const TICK: Duration = Duration::from_millis(20);
const SLOW_TICKS: u32 = 5;
const NOT_ATTACHED: &str = "not attached";
const DIM_LEVEL: u32 = 10;
const OFF_LEVEL: u32 = 0;
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

struct Hardware {
    fs: RealFs,
    layout: Layout,
    lease: Option<PortLease>,
    session: Option<Session>,
    generation: u64,
    serial: String,
    events: Sender<(u64, Input)>,
    presenter: Option<Presenter>,
    helper: Option<ScreenHelper>,
    helper_paths: Option<(PathBuf, PathBuf)>,
    assets: Option<PathBuf>,
    drawing: Option<&'static str>,
    motor: Option<Motor>,
    backlight: Option<Backlight>,
    state: Option<StateDir>,
}

impl Hardware {
    fn controller(&self) -> Option<&str> {
        self.lease.as_ref().map(PortLease::controller)
    }

    fn start(&mut self, kind: Kind) -> Result<(), String> {
        let lease = self.lease.as_ref().ok_or("the USB port is not ours")?;
        self.generation += 1;
        let events = SessionEvents {
            sender: self.events.clone(),
            generation: self.generation,
        };
        let serial = lease.serial().unwrap_or(&self.serial).to_string();
        let known_budget = self.state.as_ref().and_then(StateDir::endpoint_budget);
        let session = Session::start(
            &self.fs,
            &self.layout,
            lease,
            kind,
            &serial,
            events,
            known_budget,
        )
        .map_err(|error| format!("could not start the {kind:?} controller: {error}"))?;
        eprintln!("controller ready as {kind:?} on {}", lease.controller());
        if kind == Kind::Xbox
            && let Some(state) = &self.state
            && known_budget != Some(session.budget())
        {
            state.save_endpoint_budget(session.budget());
        }
        let _ = self
            .events
            .send((self.generation, Input::Endpoints(session.budget())));
        self.session = Some(session);
        Ok(())
    }

    fn stop(&mut self) {
        if let (Some(session), Some(lease)) = (self.session.take(), &self.lease) {
            session.stop(&self.fs, &self.layout, lease);
        }
    }

    fn apply(&mut self, action: Action) -> Result<bool, String> {
        match action {
            Action::Start(kind) => self.start(kind)?,
            Action::Restart(kind) => {
                self.stop();
                self.start(kind)?;
            }
            Action::Report(state) => {
                if let Some(session) = &self.session {
                    session.submit(&state);
                }
            }
            Action::Show(screen) => {
                eprintln!("screen: {screen:?}");
                self.show(screen);
            }
            Action::Settings(view) => {
                eprintln!("settings: {view:?}");
                if let Some(helper) = &mut self.helper
                    && let Err(error) = helper.settings(view)
                {
                    self.helper_failed(&error);
                }
            }
            Action::Debug(view) => {
                if let Some(helper) = &mut self.helper
                    && let Err(error) = helper.debug(view)
                {
                    self.helper_failed(&error);
                }
            }
            Action::SaveSettings(settings) => {
                if let Some(state) = &self.state {
                    state.save_settings(settings);
                }
            }
            Action::Busy(busy) => {
                if let Some(helper) = &mut self.helper
                    && let Err(error) = helper.busy(busy)
                {
                    self.helper_failed(&error);
                }
            }
            Action::Hold(progress) => {
                eprintln!("hold: {progress:?}");
                if let Some(helper) = &mut self.helper
                    && let Err(error) = helper.hold(progress)
                {
                    self.helper_failed(&error);
                }
            }
            Action::Backlight(light) => {
                if let Some(backlight) = &mut self.backlight {
                    match light {
                        Light::Full => backlight.undim(),
                        Light::Dim => backlight.dim(DIM_LEVEL),
                        Light::Off => backlight.dim(OFF_LEVEL),
                    }
                }
            }
            Action::Sheet(sheet) => {
                eprintln!("sheet: {sheet:?}");
                if let Some(helper) = &mut self.helper
                    && let Err(error) = helper.sheet(sheet)
                {
                    self.helper_failed(&error);
                }
            }
            Action::Live(view) => {
                if let Some(helper) = &mut self.helper
                    && let Err(error) = helper.live(view)
                {
                    self.helper_failed(&error);
                }
            }
            Action::SaveDpadMode(mode) => {
                if let Some(state) = &self.state {
                    state.save_dpad_mode(mode);
                }
            }
            Action::Motor(running) => {
                if let Some(motor) = &mut self.motor {
                    motor.set(running);
                }
            }
            Action::SaveMode(mode) => {
                if let Some(state) = &self.state {
                    state.save_mode(mode);
                }
            }
            Action::Exit => return Ok(false),
        }
        Ok(true)
    }

    fn shut_down(&mut self) {
        self.stop();
        if let Some(motor) = &mut self.motor {
            motor.set(false);
        }
        if let Some(backlight) = &mut self.backlight {
            backlight.restore();
        }
        if let Some(lease) = self.lease.take() {
            match lease.give_back(&self.fs, &self.layout) {
                Ok(()) => eprintln!("gave the USB port back"),
                Err(error) => eprintln!("could not give the USB port back cleanly: {error}"),
            }
        }
        if let Some(helper) = self.helper.take() {
            helper.close();
        }
        if let Some(presenter) = &mut self.presenter {
            presenter.close();
        }
    }

    fn show(&mut self, screen: Screen) {
        if self.helper.is_none()
            && let Some((binary, font)) = self.helper_paths.take()
        {
            match ScreenHelper::start(
                &binary,
                &font,
                self.assets.as_deref(),
                self.drawing,
                screen.clone(),
            ) {
                Ok(helper) => {
                    self.helper = Some(helper);
                    return;
                }
                Err(error) => eprintln!("could not start {}: {error}", binary.display()),
            }
        }
        if let Some(helper) = &mut self.helper {
            match helper.show(screen.clone()) {
                Ok(()) => return,
                Err(error) => self.helper_failed(&error),
            }
        }
        if let Some(presenter) = &mut self.presenter {
            presenter.show(&screen.text());
        }
    }

    fn animate(&mut self) {
        if let Some(helper) = &mut self.helper
            && let Err(error) = helper.animate()
        {
            self.helper_failed(&error);
        }
    }

    fn helper_failed(&mut self, error: &std::io::Error) {
        eprintln!("the screen helper stopped ({error}); falling back to the presenter");
        if let Some(helper) = self.helper.take() {
            helper.close();
        }
    }
}

fn is_current(generation: u64, current: u64) -> bool {
    generation == 0 || generation == current
}

#[derive(Default)]
struct Pulse {
    ticks: u32,
}

impl Pulse {
    fn next_is_slow(&mut self) -> bool {
        self.ticks = self.ticks.wrapping_add(1);
        self.ticks.is_multiple_of(SLOW_TICKS)
    }
}

#[derive(Default)]
struct LinkWatch {
    was_attached: bool,
    last_link: Option<UsbLink>,
}

impl LinkWatch {
    fn observe(&mut self, power: Option<bool>, controller_state: Option<&str>) -> Vec<Input> {
        let attached = controller_state.is_some_and(|state| state != NOT_ATTACHED);
        let unplugged = self.was_attached && !attached;
        self.was_attached = attached;
        let link = UsbLink::classify(power, controller_state);
        let mut inputs = Vec::new();
        if self.last_link != Some(link) {
            self.last_link = Some(link);
            inputs.push(Input::Link(link));
        }
        inputs.push(if unplugged {
            Input::CableUnplugged
        } else {
            Input::Tick
        });
        inputs
    }
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

fn run_loop(
    hardware: &mut Hardware,
    controller: &mut Controller,
    receiver: &Receiver<(u64, Input)>,
) -> Result<(), String> {
    let mut link_watch = LinkWatch::default();
    let mut pulse = Pulse::default();
    loop {
        let inputs = match receiver.recv_timeout(TICK) {
            Ok((generation, input)) if is_current(generation, hardware.generation) => {
                vec![input]
            }
            Ok(_) => continue,
            Err(RecvTimeoutError::Timeout) => {
                if !pulse.next_is_slow() {
                    vec![Input::Tick]
                } else {
                    let state = hardware.controller().and_then(|controller| {
                        gadget::controller_state(&hardware.fs, &hardware.layout, controller)
                    });
                    hardware.animate();
                    link_watch.observe(system::usb_power_online(), state.as_deref())
                }
            }
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        };
        for input in inputs {
            if !handle(hardware, controller, input)? {
                return Ok(());
            }
        }
    }
}

fn handle(
    hardware: &mut Hardware,
    controller: &mut Controller,
    input: Input,
) -> Result<bool, String> {
    if !matches!(input, Input::Tick | Input::Pad(_)) {
        eprintln!("input: {input:?}");
    }
    for action in controller.handle(input, Instant::now()) {
        if !hardware.apply(action)? {
            return Ok(false);
        }
    }
    Ok(true)
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
