use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use pocket_controller::control::{Action, Controller, Input, Kind, Mode, Screen, UsbLink};
use pocket_controller::functionfs::{Session, SessionEvents};
use pocket_controller::gadget::{self, Layout, PortLease};
use pocket_controller::interrupt;
use pocket_controller::system::{
    self, Backlight, Motor, Presenter, RealFs, ScreenHelper, StateDir,
};

const TICK: Duration = Duration::from_millis(20);
const SLOW_TICKS: u32 = 5;
const NOT_ATTACHED: &str = "not attached";
const DIM_LEVEL: u32 = 10;
const SAVED_BRIGHTNESS: &str = "/tmp/pocket-controller.brightness";

#[derive(Debug, Default)]
struct Options {
    mode: Option<Mode>,
    state_dir: Option<PathBuf>,
    presenter: Option<PathBuf>,
    screen: Option<PathBuf>,
    font: Option<PathBuf>,
    assets: Option<PathBuf>,
    motor: Option<PathBuf>,
    backlight: bool,
    recover_only: bool,
}

const USAGE: &str = "usage: pocket-controller [recover] [--mode automatic|xbox|gamepad] [--state-dir PATH] [--screen PATH --font PATH [--assets DIR]] [--presenter PATH] [--motor PATH] [--dim]";

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
            "--motor" => options.motor = Some(PathBuf::from(value("--motor")?)),
            "--dim" => options.backlight = true,
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
            Action::Picker(selected) => {
                if let Some(helper) = &mut self.helper
                    && let Err(error) = helper.picker(selected)
                {
                    self.helper_failed(&error);
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
            Action::Dim(dimmed) => {
                if let Some(backlight) = &mut self.backlight {
                    if dimmed {
                        backlight.dim(DIM_LEVEL);
                    } else {
                        backlight.undim();
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
            match ScreenHelper::start(&binary, &font, self.assets.as_deref(), screen.clone()) {
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

fn run_loop(
    hardware: &mut Hardware,
    controller: &mut Controller,
    receiver: &Receiver<(u64, Input)>,
) -> Result<(), String> {
    let mut was_attached = false;
    let mut last_link = None;
    let mut ticks: u32 = 0;
    loop {
        let inputs = match receiver.recv_timeout(TICK) {
            Ok((generation, input)) if generation == 0 || generation == hardware.generation => {
                vec![input]
            }
            Ok(_) => continue,
            Err(RecvTimeoutError::Timeout) => {
                ticks = ticks.wrapping_add(1);
                if !ticks.is_multiple_of(SLOW_TICKS) {
                    vec![Input::Tick]
                } else {
                    let state = hardware.controller().and_then(|controller| {
                        gadget::controller_state(&hardware.fs, &hardware.layout, controller)
                    });
                    let attached = state.as_deref().is_some_and(|state| state != NOT_ATTACHED);
                    let unplugged = was_attached && !attached;
                    was_attached = attached;
                    hardware.animate();
                    let link = UsbLink::classify(system::usb_power_online(), state.as_deref());
                    let mut inputs = Vec::new();
                    if last_link != Some(link) {
                        last_link = Some(link);
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
    let pad = devices
        .iter()
        .find(|device| device.is_gamepad())
        .cloned()
        .ok_or("no built-in controller found")?;
    eprintln!(
        "reading the built-in controller {:?} ({})",
        pad.name, pad.event_node
    );
    let state = options.state_dir.map(StateDir::new);
    let mode = options
        .mode
        .or_else(|| state.as_ref().and_then(StateDir::mode))
        .unwrap_or(Mode::Automatic);
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
        motor: options.motor.map(Motor::new),
        backlight: options
            .backlight
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
        if let Some(power) = devices
            .iter()
            .find(|device| device.has_power_key())
            .cloned()
            && let Err(error) = system::spawn_power_reader(power, sender.clone())
        {
            eprintln!("the power button will not quit: {error}");
        }
        let input = hardware
            .state
            .as_ref()
            .map(StateDir::input_settings)
            .unwrap_or_default();
        let (mut controller, actions) = Controller::with_input(mode, input, Instant::now());
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
