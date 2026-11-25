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
    use crate::gadget::fake::{FakeFs, brick, test_layout};
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::mpsc;

    use crate::control::Diagnosis;

    type Log = Rc<RefCell<Vec<String>>>;

    #[derive(Default)]
    struct Script {
        fail_start: bool,
        fail_helper_start: bool,
        fail_helper_call: Option<&'static str>,
        budget_on_start: Option<EndpointBudget>,
        requests: Vec<(Kind, String, Option<EndpointBudget>, u64)>,
        helper_starts: Vec<HelperStart>,
        quit_on_animate: Option<(u32, Sender<(u64, Input)>)>,
        animations: u32,
        power_probes: u32,
    }

    type Shared = Rc<RefCell<Script>>;
    type Events = (u64, Input);
    type HelperStart = (PathBuf, PathBuf, Option<PathBuf>, Option<String>, Screen);

    struct FakeSession {
        log: Log,
        budget: EndpointBudget,
    }

    impl ActiveSession for FakeSession {
        fn budget(&self) -> EndpointBudget {
            self.budget
        }

        fn submit(&self, state: &State) {
            self.log
                .borrow_mut()
                .push(format!("submit {}", state.buttons));
        }

        fn stop(self: Box<Self>, _: &dyn GadgetFs, _: &Layout, _: &PortLease) {
            self.log.borrow_mut().push("session.stop".into());
        }
    }

    struct FakeStarter {
        log: Log,
        script: Shared,
    }

    impl SessionStarter for FakeStarter {
        fn start(&self, request: StartRequest<'_>) -> io::Result<Box<dyn ActiveSession>> {
            let mut script = self.script.borrow_mut();
            script.requests.push((
                request.kind,
                request.serial.to_string(),
                request.known_budget,
                request.events.generation,
            ));
            self.log
                .borrow_mut()
                .push(format!("session.start {:?}", request.kind));
            if script.fail_start {
                return Err(io::Error::other("no endpoint layout to try"));
            }
            Ok(Box::new(FakeSession {
                log: self.log.clone(),
                budget: script
                    .budget_on_start
                    .unwrap_or(EndpointBudget::TwoInterrupt),
            }))
        }
    }

    struct FakeHelper {
        log: Log,
        script: Shared,
    }

    impl FakeHelper {
        fn call(&self, name: &'static str) -> io::Result<()> {
            self.log.borrow_mut().push(format!("helper.{name}"));
            if self.script.borrow().fail_helper_call == Some(name) {
                Err(io::Error::from(io::ErrorKind::BrokenPipe))
            } else {
                Ok(())
            }
        }
    }

    impl Helper for FakeHelper {
        fn show(&mut self, _: Screen) -> io::Result<()> {
            self.call("show")
        }

        fn hold(&mut self, _: Option<u8>) -> io::Result<()> {
            self.call("hold")
        }

        fn busy(&mut self, _: bool) -> io::Result<()> {
            self.call("busy")
        }

        fn live(&mut self, _: LiveView) -> io::Result<()> {
            self.call("live")
        }

        fn sheet(&mut self, _: Option<SheetView>) -> io::Result<()> {
            self.call("sheet")
        }

        fn settings(&mut self, _: Option<SettingsView>) -> io::Result<()> {
            self.call("settings")
        }

        fn debug(&mut self, _: Option<DebugView>) -> io::Result<()> {
            self.call("debug")
        }

        fn animate(&mut self) -> io::Result<()> {
            let mut script = self.script.borrow_mut();
            script.animations += 1;
            if let Some((after, sender)) = &script.quit_on_animate
                && script.animations >= *after
            {
                let _ = sender.send((0, Input::QuitRequested));
            }
            drop(script);
            self.call("animate")
        }

        fn close(self: Box<Self>) {
            self.log.borrow_mut().push("helper.close".into());
        }
    }

    struct FakeHelpers {
        log: Log,
        script: Shared,
    }

    impl HelperStarter for FakeHelpers {
        fn start(
            &self,
            binary: &Path,
            font: &Path,
            assets: Option<&Path>,
            drawing: Option<&str>,
            screen: Screen,
        ) -> io::Result<Box<dyn Helper>> {
            let mut script = self.script.borrow_mut();
            script.helper_starts.push((
                binary.to_path_buf(),
                font.to_path_buf(),
                assets.map(Path::to_path_buf),
                drawing.map(str::to_string),
                screen,
            ));
            self.log.borrow_mut().push("helper.start".into());
            if script.fail_helper_start {
                return Err(io::Error::other("no such binary"));
            }
            Ok(Box::new(FakeHelper {
                log: self.log.clone(),
                script: self.script.clone(),
            }))
        }
    }

    struct FakeNotice {
        log: Log,
    }

    impl Notice for FakeNotice {
        fn show(&mut self, text: &str) {
            self.log.borrow_mut().push(format!("notice.show {text}"));
        }

        fn close(&mut self) {
            self.log.borrow_mut().push("notice.close".into());
        }
    }

    struct FakePanel(Rc<RefCell<u32>>);

    impl Panel for FakePanel {
        fn read(&self) -> io::Result<u32> {
            Ok(*self.0.borrow())
        }

        fn write(&self, level: u32) -> io::Result<()> {
            *self.0.borrow_mut() = level;
            Ok(())
        }
    }

    fn scratch(label: &str) -> PathBuf {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "pocket-controller-runtime-{}-{}-{label}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    struct Rig {
        hardware: Hardware,
        log: Log,
        script: Shared,
        fs: Rc<FakeFs>,
        events: mpsc::Receiver<(u64, Input)>,
        directory: PathBuf,
        panel: Rc<RefCell<u32>>,
    }

    impl Rig {
        fn new(label: &str) -> Self {
            let directory = scratch(label);
            let log: Log = Rc::default();
            let script: Shared = Rc::default();
            let fs = Rc::new(brick());
            let layout = test_layout();
            let lease = PortLease::take(&*fs, &layout).unwrap();
            let (sender, events) = mpsc::channel();
            let panel = Rc::new(RefCell::new(80));
            let hardware = Hardware {
                fs: fs.clone(),
                layout,
                lease: Some(lease),
                session: None,
                sessions: Box::new(FakeStarter {
                    log: log.clone(),
                    script: script.clone(),
                }),
                generation: 0,
                serial: "fallback".into(),
                events: sender,
                presenter: Some(Box::new(FakeNotice { log: log.clone() })),
                helper: None,
                helpers: Box::new(FakeHelpers {
                    log: log.clone(),
                    script: script.clone(),
                }),
                helper_paths: None,
                assets: None,
                drawing: None,
                motor: None,
                backlight: None,
                usb_power: Box::new({
                    let script = script.clone();
                    move || {
                        script.borrow_mut().power_probes += 1;
                        Some(true)
                    }
                }),
                state: None,
            };
            Self {
                hardware,
                log,
                script,
                fs,
                events,
                directory,
                panel,
            }
        }

        fn with_state(mut self) -> Self {
            self.hardware.state = Some(StateDir::new(self.directory.join("state")));
            self
        }

        fn with_helper(mut self) -> Self {
            self.hardware.helper_paths = Some(("/bin/screen".into(), "/fonts/a.ttf".into()));
            self.hardware.assets = Some("/assets".into());
            self.hardware.drawing = Some("brick");
            self
        }

        fn with_motor(mut self) -> Self {
            self.hardware.motor = Some(Motor::new(self.directory.join("motor")));
            self
        }

        fn with_backlight(mut self) -> Self {
            let panel: Box<dyn Panel> = Box::new(FakePanel(self.panel.clone()));
            self.hardware.backlight = Some(Backlight::with_panel(
                panel,
                self.directory.join("brightness"),
            ));
            self
        }

        fn entries(&self) -> Vec<String> {
            self.log.borrow().clone()
        }

        fn state_file(&self, name: &str) -> Option<String> {
            std::fs::read_to_string(self.directory.join("state").join(name)).ok()
        }
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }

    #[test]
    fn starting_without_the_port_is_refused_and_tries_nothing() {
        let mut rig = Rig::new("no-port");
        rig.hardware.lease = None;
        let error = rig.hardware.start(Kind::Xbox).unwrap_err();
        assert_eq!(error, "the USB port is not ours");
        assert!(rig.entries().is_empty());
        assert_eq!(rig.hardware.generation, 0);
    }

    #[test]
    fn starting_a_session_uses_the_firmware_serial_and_a_new_generation() {
        let mut rig = Rig::new("serial");
        rig.hardware.start(Kind::Gamepad).unwrap();
        let requests = rig.script.borrow().requests.clone();
        assert_eq!(
            requests,
            vec![(Kind::Gamepad, "2c000c81b5c28711c9d".into(), None, 1)]
        );
        assert!(rig.hardware.session.is_some());
    }

    #[test]
    fn each_start_gets_its_own_generation_so_stale_input_can_be_told_apart() {
        let mut rig = Rig::new("generations");
        rig.hardware.start(Kind::Xbox).unwrap();
        rig.hardware.start(Kind::Xbox).unwrap();
        let generations: Vec<u64> = rig
            .script
            .borrow()
            .requests
            .iter()
            .map(|request| request.3)
            .collect();
        assert_eq!(generations, vec![1, 2]);
    }

    #[test]
    fn the_controller_is_told_which_endpoint_budget_the_session_got() {
        let mut rig = Rig::new("budget-event");
        rig.script.borrow_mut().budget_on_start = Some(EndpointBudget::OneInterrupt);
        rig.hardware.start(Kind::Xbox).unwrap();
        assert_eq!(
            rig.events.try_recv().unwrap(),
            (1, Input::Endpoints(EndpointBudget::OneInterrupt))
        );
    }

    #[test]
    fn an_xbox_session_that_needed_the_fallback_budget_remembers_it() {
        let mut rig = Rig::new("budget-saved").with_state();
        rig.script.borrow_mut().budget_on_start = Some(EndpointBudget::OneInterrupt);
        rig.hardware.start(Kind::Xbox).unwrap();
        assert_eq!(
            rig.state_file("endpoints").as_deref(),
            Some("one-interrupt")
        );
    }

    #[test]
    fn the_remembered_budget_is_offered_to_the_next_session() {
        let mut rig = Rig::new("budget-known").with_state();
        rig.hardware
            .state
            .as_ref()
            .unwrap()
            .save_endpoint_budget(EndpointBudget::OneInterrupt);
        rig.hardware.start(Kind::Xbox).unwrap();
        assert_eq!(
            rig.script.borrow().requests[0].2,
            Some(EndpointBudget::OneInterrupt)
        );
    }

    #[test]
    fn a_generic_gamepad_session_never_writes_the_endpoint_budget() {
        let mut rig = Rig::new("gamepad-budget").with_state();
        rig.hardware.start(Kind::Gamepad).unwrap();
        assert_eq!(rig.state_file("endpoints"), None);
    }

    #[test]
    fn a_session_that_cannot_start_names_the_kind_and_the_cause_and_leaves_none_running() {
        let mut rig = Rig::new("start-fails");
        rig.script.borrow_mut().fail_start = true;
        let error = rig.hardware.start(Kind::Xbox).unwrap_err();
        assert_eq!(
            error,
            "could not start the Xbox controller: no endpoint layout to try"
        );
        assert!(rig.hardware.session.is_none());
    }

    #[test]
    fn restarting_stops_the_old_session_before_starting_the_new_one() {
        let mut rig = Rig::new("restart");
        rig.hardware.apply(Action::Start(Kind::Xbox)).unwrap();
        rig.hardware.apply(Action::Restart(Kind::Gamepad)).unwrap();
        assert_eq!(
            rig.entries(),
            [
                "session.start Xbox",
                "session.stop",
                "session.start Gamepad"
            ]
        );
    }

    #[test]
    fn a_restart_that_fails_leaves_the_port_without_a_session() {
        let mut rig = Rig::new("restart-fails");
        rig.hardware.apply(Action::Start(Kind::Xbox)).unwrap();
        rig.script.borrow_mut().fail_start = true;
        assert!(rig.hardware.apply(Action::Restart(Kind::Gamepad)).is_err());
        assert!(rig.hardware.session.is_none());
    }

    #[test]
    fn a_report_reaches_the_running_session_and_is_dropped_without_one() {
        let mut rig = Rig::new("report");
        let state = State {
            buttons: 7,
            ..State::default()
        };
        rig.hardware.apply(Action::Report(state)).unwrap();
        assert!(rig.entries().is_empty());
        rig.hardware.apply(Action::Start(Kind::Xbox)).unwrap();
        rig.hardware.apply(Action::Report(state)).unwrap();
        assert_eq!(rig.entries().last().unwrap(), "submit 7");
    }

    #[test]
    fn exit_is_the_only_action_that_ends_the_loop() {
        let mut rig = Rig::new("exit");
        assert!(rig.hardware.apply(Action::Busy(true)).unwrap());
        assert!(!rig.hardware.apply(Action::Exit).unwrap());
    }

    #[test]
    fn the_first_screen_starts_the_helper_with_its_paths_and_the_screen() {
        let mut rig = Rig::new("helper-start").with_helper();
        let screen = Screen::Connected(Kind::Xbox);
        rig.hardware.apply(Action::Show(screen.clone())).unwrap();
        assert_eq!(
            rig.script.borrow().helper_starts,
            vec![(
                PathBuf::from("/bin/screen"),
                PathBuf::from("/fonts/a.ttf"),
                Some(PathBuf::from("/assets")),
                Some("brick".to_string()),
                screen
            )]
        );
        assert!(rig.hardware.helper.is_some());
        assert!(
            rig.entries()
                .iter()
                .all(|entry| !entry.starts_with("notice"))
        );
    }

    #[test]
    fn later_screens_go_to_the_running_helper() {
        let mut rig = Rig::new("helper-show").with_helper();
        rig.hardware
            .apply(Action::Show(Screen::Connected(Kind::Xbox)))
            .unwrap();
        rig.hardware
            .apply(Action::Show(Screen::Connected(Kind::Gamepad)))
            .unwrap();
        assert_eq!(rig.entries(), ["helper.start", "helper.show"]);
    }

    #[test]
    fn a_helper_that_cannot_start_is_not_retried_and_the_presenter_shows_the_text() {
        let mut rig = Rig::new("helper-fails").with_helper();
        rig.script.borrow_mut().fail_helper_start = true;
        let screen = Screen::Waiting(Mode::Automatic, Diagnosis::None);
        rig.hardware.apply(Action::Show(screen.clone())).unwrap();
        rig.hardware.apply(Action::Show(screen.clone())).unwrap();
        let text = screen.text();
        assert_eq!(
            rig.entries(),
            [
                "helper.start".to_string(),
                format!("notice.show {text}"),
                format!("notice.show {text}")
            ]
        );
    }

    #[test]
    fn without_a_helper_path_the_presenter_shows_the_screen_text() {
        let mut rig = Rig::new("presenter-only");
        let screen = Screen::Connected(Kind::Gamepad);
        rig.hardware.apply(Action::Show(screen.clone())).unwrap();
        assert_eq!(rig.entries(), [format!("notice.show {}", screen.text())]);
    }

    #[test]
    fn a_screen_with_neither_helper_nor_presenter_is_dropped_quietly() {
        let mut rig = Rig::new("nothing");
        rig.hardware.presenter = None;
        rig.hardware
            .apply(Action::Show(Screen::Connected(Kind::Xbox)))
            .unwrap();
        assert!(rig.entries().is_empty());
    }

    #[test]
    fn a_helper_that_dies_while_showing_is_closed_and_the_presenter_takes_over() {
        let mut rig = Rig::new("helper-dies").with_helper();
        let screen = Screen::Connected(Kind::Xbox);
        rig.hardware.apply(Action::Show(screen.clone())).unwrap();
        rig.script.borrow_mut().fail_helper_call = Some("show");
        rig.hardware.apply(Action::Show(screen.clone())).unwrap();
        assert!(rig.hardware.helper.is_none());
        assert_eq!(
            rig.entries(),
            [
                "helper.start".to_string(),
                "helper.show".to_string(),
                "helper.close".to_string(),
                format!("notice.show {}", screen.text())
            ]
        );
    }

    #[test]
    fn every_view_action_goes_to_the_helper() {
        let mut rig = Rig::new("views").with_helper();
        rig.hardware
            .apply(Action::Show(Screen::Connected(Kind::Xbox)))
            .unwrap();
        for action in [
            Action::Settings(None),
            Action::Debug(None),
            Action::Busy(true),
            Action::Hold(Some(50)),
            Action::Sheet(None),
            Action::Live(LiveView::default()),
        ] {
            rig.hardware.apply(action).unwrap();
        }
        assert_eq!(
            rig.entries()[1..],
            [
                "helper.settings",
                "helper.debug",
                "helper.busy",
                "helper.hold",
                "helper.sheet",
                "helper.live"
            ]
        );
    }

    #[test]
    fn a_failing_view_call_drops_the_helper_and_later_views_are_ignored() {
        for name in ["settings", "debug", "busy", "hold", "sheet", "live"] {
            let mut rig = Rig::new("view-fails").with_helper();
            rig.hardware
                .apply(Action::Show(Screen::Connected(Kind::Xbox)))
                .unwrap();
            rig.script.borrow_mut().fail_helper_call = Some(name);
            let action = match name {
                "settings" => Action::Settings(None),
                "debug" => Action::Debug(None),
                "busy" => Action::Busy(true),
                "hold" => Action::Hold(None),
                "sheet" => Action::Sheet(None),
                _ => Action::Live(LiveView::default()),
            };
            rig.hardware.apply(action).unwrap();
            assert!(rig.hardware.helper.is_none(), "{name}");
            let before = rig.entries().len();
            rig.hardware.apply(Action::Busy(false)).unwrap();
            assert_eq!(rig.entries().len(), before, "{name}");
        }
    }

    #[test]
    fn animating_reaches_the_helper_and_a_failure_drops_it() {
        let mut rig = Rig::new("animate").with_helper();
        rig.hardware.animate();
        assert!(rig.entries().is_empty());
        rig.hardware
            .apply(Action::Show(Screen::Connected(Kind::Xbox)))
            .unwrap();
        rig.hardware.animate();
        rig.script.borrow_mut().fail_helper_call = Some("animate");
        rig.hardware.animate();
        assert!(rig.hardware.helper.is_none());
    }

    #[test]
    fn mode_settings_and_dpad_choices_are_saved_to_the_state_directory() {
        let mut rig = Rig::new("saves").with_state();
        let settings = crate::settings::Settings {
            face: crate::layer::FaceLayout::Position,
            rumble: false,
            screen: crate::settings::ScreenWhilePlaying::AlwaysOn,
        };
        rig.hardware.apply(Action::SaveMode(Mode::Gamepad)).unwrap();
        rig.hardware.apply(Action::SaveSettings(settings)).unwrap();
        rig.hardware
            .apply(Action::SaveDpadMode(crate::layer::DpadMode::LeftStick))
            .unwrap();
        let saved = rig.hardware.state.as_ref().unwrap();
        assert_eq!(saved.mode(), Some(Mode::Gamepad));
        assert_eq!(saved.saved().settings, settings);
        assert_eq!(saved.saved().dpad, crate::layer::DpadMode::LeftStick);
    }

    #[test]
    fn saving_without_a_state_directory_does_nothing() {
        let mut rig = Rig::new("no-state");
        assert!(rig.hardware.apply(Action::SaveMode(Mode::Xbox)).unwrap());
        assert!(!rig.directory.join("state").exists());
    }

    #[test]
    fn the_backlight_follows_the_light_actions() {
        let mut rig = Rig::new("light").with_backlight();
        rig.hardware.apply(Action::Backlight(Light::Dim)).unwrap();
        assert_eq!(*rig.panel.borrow(), DIM_LEVEL);
        rig.hardware.apply(Action::Backlight(Light::Off)).unwrap();
        assert_eq!(*rig.panel.borrow(), OFF_LEVEL);
        rig.hardware.apply(Action::Backlight(Light::Full)).unwrap();
        assert_eq!(*rig.panel.borrow(), 80);
    }

    #[test]
    fn the_motor_action_drives_the_motor() {
        let mut rig = Rig::new("motor").with_motor();
        rig.hardware.apply(Action::Motor(true)).unwrap();
        assert_eq!(
            std::fs::read_to_string(rig.directory.join("motor")).unwrap(),
            "1"
        );
        rig.hardware.apply(Action::Motor(false)).unwrap();
        assert_eq!(
            std::fs::read_to_string(rig.directory.join("motor")).unwrap(),
            "0"
        );
    }

    #[test]
    fn hardware_that_is_absent_ignores_its_actions() {
        let mut rig = Rig::new("absent");
        for action in [
            Action::Motor(true),
            Action::Backlight(Light::Dim),
            Action::Hold(None),
            Action::Settings(None),
        ] {
            assert!(rig.hardware.apply(action).unwrap());
        }
    }

    #[test]
    fn the_controller_name_comes_from_the_lease() {
        let mut rig = Rig::new("controller-name");
        assert_eq!(rig.hardware.controller(), Some("5100000.udc-controller"));
        rig.hardware.lease = None;
        assert_eq!(rig.hardware.controller(), None);
    }

    #[test]
    fn shutting_down_undoes_everything_in_order_and_gives_the_port_back() {
        let mut rig = Rig::new("shutdown")
            .with_helper()
            .with_motor()
            .with_backlight();
        rig.hardware
            .apply(Action::Show(Screen::Connected(Kind::Xbox)))
            .unwrap();
        rig.hardware.apply(Action::Start(Kind::Xbox)).unwrap();
        rig.hardware.apply(Action::Motor(true)).unwrap();
        rig.hardware.apply(Action::Backlight(Light::Off)).unwrap();
        rig.log.borrow_mut().clear();

        rig.hardware.shut_down();

        assert_eq!(
            rig.entries(),
            ["session.stop", "helper.close", "notice.close"]
        );
        assert_eq!(
            std::fs::read_to_string(rig.directory.join("motor")).unwrap(),
            "0"
        );
        assert_eq!(*rig.panel.borrow(), 80);
        assert!(rig.hardware.lease.is_none());
        assert!(rig.hardware.session.is_none());
        assert!(rig.hardware.helper.is_none());
        assert!(!rig.fs.has(&test_layout().marker.to_string_lossy()));
    }

    #[test]
    fn shutting_down_twice_does_nothing_the_second_time() {
        let mut rig = Rig::new("shutdown-twice").with_helper();
        rig.hardware
            .apply(Action::Show(Screen::Connected(Kind::Xbox)))
            .unwrap();
        rig.hardware.shut_down();
        rig.log.borrow_mut().clear();
        rig.hardware.shut_down();
        assert_eq!(rig.entries(), ["notice.close"]);
    }

    fn queue(inputs: &[Input]) -> (Sender<Events>, Receiver<Events>) {
        let (sender, receiver) = mpsc::channel();
        for input in inputs {
            sender.send((0, input.clone())).unwrap();
        }
        (sender, receiver)
    }

    #[test]
    fn serving_starts_the_first_kind_shows_the_screen_and_exits_on_a_quit_request() {
        let mut rig = Rig::new("serve").with_helper();
        let (_sender, receiver) = queue(&[Input::QuitRequested]);
        serve(&mut rig.hardware, Mode::Xbox, &receiver, || Ok(())).unwrap();
        let entries = rig.entries();
        assert_eq!(entries[0], "session.start Xbox");
        assert_eq!(entries[1], "helper.start");
        assert!(entries.contains(&"session.stop".to_string()));
        assert!(rig.hardware.lease.is_none());
    }

    #[test]
    fn serving_starts_the_saved_settings_not_the_defaults() {
        let mut rig = Rig::new("serve-saved").with_state();
        rig.hardware
            .state
            .as_ref()
            .unwrap()
            .save_dpad_mode(crate::layer::DpadMode::RightStick);
        let (_sender, receiver) = queue(&[Input::QuitRequested]);
        serve(&mut rig.hardware, Mode::Automatic, &receiver, || Ok(())).unwrap();
        assert_eq!(rig.script.borrow().requests[0].0, Kind::Xbox);
    }

    #[test]
    fn a_reader_that_cannot_start_ends_serving_with_its_error_and_still_shuts_down() {
        let mut rig = Rig::new("serve-reader");
        let (_sender, receiver) = queue(&[]);
        let result = serve(&mut rig.hardware, Mode::Xbox, &receiver, || {
            Err("could not take over the pad".to_string())
        });
        assert_eq!(result.unwrap_err(), "could not take over the pad");
        assert!(rig.hardware.lease.is_none());
        assert!(rig.script.borrow().requests.is_empty());
    }

    #[test]
    fn a_first_session_that_cannot_start_ends_serving_and_still_gives_the_port_back() {
        let mut rig = Rig::new("serve-start-fails");
        rig.script.borrow_mut().fail_start = true;
        let (_sender, receiver) = queue(&[Input::QuitRequested]);
        let result = serve(&mut rig.hardware, Mode::Xbox, &receiver, || Ok(()));
        assert_eq!(
            result.unwrap_err(),
            "could not start the Xbox controller: no endpoint layout to try"
        );
        assert!(rig.hardware.lease.is_none());
        assert!(!rig.fs.has(&test_layout().marker.to_string_lossy()));
    }

    #[test]
    fn the_loop_ends_cleanly_when_every_sender_is_gone() {
        let mut rig = Rig::new("serve-disconnected");
        let (sender, receiver) = queue(&[]);
        drop(sender);
        assert!(serve(&mut rig.hardware, Mode::Xbox, &receiver, || Ok(())).is_ok());
    }

    fn connected_screens_shown(generation: u64) -> usize {
        let mut rig = Rig::new("serve-generation");
        let (sender, receiver) = queue(&[]);
        sender.send((generation, Input::HostEnabled)).unwrap();
        sender.send((0, Input::QuitRequested)).unwrap();
        serve(&mut rig.hardware, Mode::Xbox, &receiver, || Ok(())).unwrap();
        rig.entries()
            .iter()
            .filter(|entry| entry.contains("Connected as"))
            .count()
    }

    #[test]
    fn input_from_a_stopped_session_does_not_reach_the_controller() {
        assert_eq!(connected_screens_shown(0), 1);
        assert_eq!(connected_screens_shown(1), 1);
        assert_eq!(connected_screens_shown(99), 0);
    }

    #[test]
    fn idle_time_animates_the_helper_and_probes_the_usb_power_until_told_to_quit() {
        let mut rig = Rig::new("serve-idle").with_helper();
        let (sender, receiver) = queue(&[]);
        rig.script.borrow_mut().quit_on_animate = Some((2, sender.clone()));
        serve(&mut rig.hardware, Mode::Xbox, &receiver, || Ok(())).unwrap();
        assert_eq!(rig.script.borrow().animations, 2);
        assert_eq!(rig.script.borrow().power_probes, 2);
    }
}
