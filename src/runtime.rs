use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use crate::control::{
    Action, Controller, DebugView, Input, Kind, Light, LiveView, Mode, Screen, SheetView, UsbLink,
};
use crate::functionfs::{Session, SessionEvents};
use crate::gadget::{self, GadgetFs, Layout, PortLease};
use crate::settings::SettingsView;
use crate::system::{self, Backlight, Motor, Panel, ScreenHelper, StateDir};
use crate::xinput::{EndpointBudget, State};

const TICK: Duration = Duration::from_millis(20);
const SLOW_TICKS: u32 = 5;
const NOT_ATTACHED: &str = "not attached";
const DIM_LEVEL: u32 = 10;
const OFF_LEVEL: u32 = 0;

pub trait ActiveSession {
    fn budget(&self) -> EndpointBudget;
    fn submit(&self, state: &State);
    fn stop(self: Box<Self>, fs: &dyn GadgetFs, layout: &Layout, lease: &PortLease);
}

impl ActiveSession for Session {
    fn budget(&self) -> EndpointBudget {
        Session::budget(self)
    }

    fn submit(&self, state: &State) {
        Session::submit(self, state);
    }

    fn stop(self: Box<Self>, fs: &dyn GadgetFs, layout: &Layout, lease: &PortLease) {
        Session::stop(*self, fs, layout, lease);
    }
}

pub struct StartRequest<'a> {
    pub fs: &'a dyn GadgetFs,
    pub layout: &'a Layout,
    pub lease: &'a PortLease,
    pub kind: Kind,
    pub serial: &'a str,
    pub events: SessionEvents,
    pub known_budget: Option<EndpointBudget>,
}

pub trait SessionStarter {
    fn start(&self, request: StartRequest<'_>) -> io::Result<Box<dyn ActiveSession>>;
}

pub struct RealSessions;

impl SessionStarter for RealSessions {
    fn start(&self, request: StartRequest<'_>) -> io::Result<Box<dyn ActiveSession>> {
        let session = Session::start(
            request.fs,
            request.layout,
            request.lease,
            request.kind,
            request.serial,
            request.events,
            request.known_budget,
        )?;
        Ok(Box::new(session))
    }
}

pub trait Helper {
    fn show(&mut self, screen: Screen) -> io::Result<()>;
    fn hold(&mut self, progress: Option<u8>) -> io::Result<()>;
    fn busy(&mut self, busy: bool) -> io::Result<()>;
    fn live(&mut self, live: LiveView) -> io::Result<()>;
    fn sheet(&mut self, sheet: Option<SheetView>) -> io::Result<()>;
    fn settings(&mut self, view: Option<SettingsView>) -> io::Result<()>;
    fn debug(&mut self, view: Option<DebugView>) -> io::Result<()>;
    fn animate(&mut self) -> io::Result<()>;
    fn close(self: Box<Self>);
}

impl Helper for ScreenHelper {
    fn show(&mut self, screen: Screen) -> io::Result<()> {
        ScreenHelper::show(self, screen)
    }

    fn hold(&mut self, progress: Option<u8>) -> io::Result<()> {
        ScreenHelper::hold(self, progress)
    }

    fn busy(&mut self, busy: bool) -> io::Result<()> {
        ScreenHelper::busy(self, busy)
    }

    fn live(&mut self, live: LiveView) -> io::Result<()> {
        ScreenHelper::live(self, live)
    }

    fn sheet(&mut self, sheet: Option<SheetView>) -> io::Result<()> {
        ScreenHelper::sheet(self, sheet)
    }

    fn settings(&mut self, view: Option<SettingsView>) -> io::Result<()> {
        ScreenHelper::settings(self, view)
    }

    fn debug(&mut self, view: Option<DebugView>) -> io::Result<()> {
        ScreenHelper::debug(self, view)
    }

    fn animate(&mut self) -> io::Result<()> {
        ScreenHelper::animate(self)
    }

    fn close(self: Box<Self>) {
        ScreenHelper::close(*self);
    }
}

pub trait HelperStarter {
    fn start(
        &self,
        binary: &Path,
        font: &Path,
        assets: Option<&Path>,
        drawing: Option<&str>,
        screen: Screen,
    ) -> io::Result<Box<dyn Helper>>;
}

pub struct RealHelpers;

impl HelperStarter for RealHelpers {
    fn start(
        &self,
        binary: &Path,
        font: &Path,
        assets: Option<&Path>,
        drawing: Option<&str>,
        screen: Screen,
    ) -> io::Result<Box<dyn Helper>> {
        Ok(Box::new(ScreenHelper::start(
            binary, font, assets, drawing, screen,
        )?))
    }
}

pub trait Notice {
    fn show(&mut self, text: &str);
    fn close(&mut self);
}

impl Notice for system::Presenter {
    fn show(&mut self, text: &str) {
        system::Presenter::show(self, text);
    }

    fn close(&mut self) {
        system::Presenter::close(self);
    }
}

pub struct Hardware {
    pub fs: Rc<dyn GadgetFs>,
    pub layout: Layout,
    pub lease: Option<PortLease>,
    pub session: Option<Box<dyn ActiveSession>>,
    pub sessions: Box<dyn SessionStarter>,
    pub generation: u64,
    pub serial: String,
    pub events: Sender<(u64, Input)>,
    pub presenter: Option<Box<dyn Notice>>,
    pub helper: Option<Box<dyn Helper>>,
    pub helpers: Box<dyn HelperStarter>,
    pub helper_paths: Option<(PathBuf, PathBuf)>,
    pub assets: Option<PathBuf>,
    pub drawing: Option<&'static str>,
    pub motor: Option<Motor>,
    pub backlight: Option<Backlight<Box<dyn Panel>>>,
    pub usb_power: Box<dyn Fn() -> Option<bool>>,
    pub state: Option<StateDir>,
}

impl Hardware {
    pub fn controller(&self) -> Option<&str> {
        self.lease.as_ref().map(PortLease::controller)
    }

    pub fn start(&mut self, kind: Kind) -> Result<(), String> {
        let lease = self.lease.as_ref().ok_or("the USB port is not ours")?;
        self.generation += 1;
        let events = SessionEvents {
            sender: self.events.clone(),
            generation: self.generation,
        };
        let serial = lease.serial().unwrap_or(&self.serial).to_string();
        let known_budget = self.state.as_ref().and_then(StateDir::endpoint_budget);
        let session = self
            .sessions
            .start(StartRequest {
                fs: &*self.fs,
                layout: &self.layout,
                lease,
                kind,
                serial: &serial,
                events,
                known_budget,
            })
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

    pub fn stop(&mut self) {
        if let (Some(session), Some(lease)) = (self.session.take(), &self.lease) {
            session.stop(&*self.fs, &self.layout, lease);
        }
    }

    pub fn apply(&mut self, action: Action) -> Result<bool, String> {
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

    pub fn shut_down(&mut self) {
        self.stop();
        if let Some(motor) = &mut self.motor {
            motor.set(false);
        }
        if let Some(backlight) = &mut self.backlight {
            backlight.restore();
        }
        if let Some(lease) = self.lease.take() {
            match lease.give_back(&*self.fs, &self.layout) {
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

    pub fn show(&mut self, screen: Screen) {
        if self.helper.is_none()
            && let Some((binary, font)) = self.helper_paths.take()
        {
            match self.helpers.start(
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

    pub fn animate(&mut self) {
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

pub fn run_loop(
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
                        gadget::controller_state(&*hardware.fs, &hardware.layout, controller)
                    });
                    hardware.animate();
                    link_watch.observe((hardware.usb_power)(), state.as_deref())
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

pub fn serve(
    hardware: &mut Hardware,
    mode: Mode,
    receiver: &Receiver<(u64, Input)>,
    spawn_readers: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let result = (|| {
        spawn_readers()?;
        let saved = hardware
            .state
            .as_ref()
            .map(StateDir::saved)
            .unwrap_or_default();
        let (mut controller, actions) = Controller::with_saved(mode, saved, Instant::now());
        for action in actions {
            hardware.apply(action)?;
        }
        run_loop(hardware, &mut controller, receiver)
    })();
    hardware.shut_down();
    result
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_from_the_current_session_and_from_system_threads_is_accepted() {
        assert!(is_current(0, 5));
        assert!(is_current(5, 5));
    }

    #[test]
    fn input_from_a_stopped_session_is_ignored() {
        assert!(!is_current(4, 5));
    }

    #[test]
    fn only_every_fifth_tick_is_a_slow_one() {
        let mut pulse = Pulse::default();
        let slow: Vec<bool> = (0..10).map(|_| pulse.next_is_slow()).collect();
        assert_eq!(
            slow,
            [
                false, false, false, false, true, false, false, false, false, true
            ]
        );
    }

    #[test]
    fn the_first_observation_announces_the_link_and_later_identical_ones_do_not() {
        let mut watch = LinkWatch::default();
        assert_eq!(
            watch.observe(Some(true), None),
            vec![Input::Link(UsbLink::Powered), Input::Tick]
        );
        assert_eq!(watch.observe(Some(true), None), vec![Input::Tick]);
    }

    #[test]
    fn a_change_of_link_is_announced_when_it_happens() {
        let mut watch = LinkWatch::default();
        watch.observe(Some(true), Some("configured"));
        assert_eq!(
            watch.observe(Some(true), Some("addressed")),
            vec![Input::Link(UsbLink::Enumerating), Input::Tick]
        );
    }

    #[test]
    fn leaving_the_attached_state_reports_the_cable_unplugged_once() {
        let mut watch = LinkWatch::default();
        watch.observe(Some(true), Some("configured"));
        let inputs = watch.observe(Some(false), Some(NOT_ATTACHED));
        assert_eq!(
            inputs,
            vec![Input::Link(UsbLink::NoPower), Input::CableUnplugged]
        );
        assert_eq!(
            watch.observe(Some(false), Some(NOT_ATTACHED)),
            vec![Input::Tick]
        );
    }

    #[test]
    fn an_unreadable_controller_state_after_attaching_counts_as_unplugged() {
        let mut watch = LinkWatch::default();
        watch.observe(Some(true), Some("configured"));
        let inputs = watch.observe(None, None);
        assert!(inputs.contains(&Input::CableUnplugged));
    }

    #[test]
    fn never_having_been_attached_is_not_an_unplug() {
        let mut watch = LinkWatch::default();
        let inputs = watch.observe(Some(false), Some(NOT_ATTACHED));
        assert!(!inputs.contains(&Input::CableUnplugged));
    }
}
