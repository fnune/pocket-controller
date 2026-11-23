use std::time::{Duration, Instant};

use crate::chord::ExitChord;
use crate::evdev::{Extras, Pad};
use crate::layer::{DpadMode, Layer, LayerEvent, TRIGGER_PRESSED_ABOVE, Turbo};
use crate::settings::{
    Menu, MenuOutcome, PRINTED_A, PRINTED_B, ScreenWhilePlaying, Settings, SettingsView,
    rumble_available,
};
use crate::xinput::{self, EndpointBudget, State};

pub const XBOX_DRIVER_WAIT: Duration = Duration::from_secs(2);
pub const IDLE_LIMIT: Duration = Duration::from_secs(10 * 60);

pub const DIAGNOSIS_AFTER: Duration = Duration::from_secs(3);
pub const DIM_AFTER: Duration = Duration::from_secs(30);

pub const LOCAL_QUIT: u16 = PRINTED_B;
pub const LOCAL_OPEN_SETTINGS: u16 = PRINTED_A;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Automatic,
    Xbox,
    Gamepad,
}

impl Mode {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "automatic" => Some(Self::Automatic),
            "xbox" => Some(Self::Xbox),
            "gamepad" => Some(Self::Gamepad),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Xbox => "xbox",
            Self::Gamepad => "gamepad",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Automatic => "Automatic",
            Self::Xbox => "Xbox-style",
            Self::Gamepad => "Generic",
        }
    }

    pub fn first_kind(self) -> Kind {
        match self {
            Self::Automatic | Self::Xbox => Kind::Xbox,
            Self::Gamepad => Kind::Gamepad,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Xbox,
    Gamepad,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsbLink {
    Unknown,
    NoPower,
    Powered,
    Enumerating,
    Configured,
}

impl UsbLink {
    pub const ALL: [Self; 5] = [
        Self::Unknown,
        Self::NoPower,
        Self::Powered,
        Self::Enumerating,
        Self::Configured,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::NoPower => "no-power",
            Self::Powered => "powered",
            Self::Enumerating => "enumerating",
            Self::Configured => "configured",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|link| link.as_str() == text)
    }

    pub fn classify(power: Option<bool>, controller_state: Option<&str>) -> Self {
        match controller_state.map(str::trim) {
            Some("configured" | "suspended") => Self::Configured,
            Some(
                "attached" | "powered" | "reconnecting" | "unauthenticated" | "default"
                | "addressed",
            ) => Self::Enumerating,
            _ => match power {
                Some(false) => Self::NoPower,
                Some(true) => Self::Powered,
                None => Self::Unknown,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Diagnosis {
    None,
    NoComputer,
    SetupStuck,
}

impl Diagnosis {
    pub fn text(self) -> &'static str {
        match self {
            Self::None => "Not connected. Plug the bottom USB-C port into a USB-A port",
            Self::NoComputer => {
                "Connected, but not recognised. Try a cable that carries data, or another USB-A port"
            }
            Self::SetupStuck => "USB setup did not finish. Try another USB-A port",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    Pad(Pad),
    PowerPressed,
    QuitRequested,
    HostEnabled,
    HostDisabled,
    HostSuspended,
    HostResumed,
    XboxDriverSpoke,
    Rumble(u8),
    CableUnplugged,
    Link(UsbLink),
    Endpoints(EndpointBudget),
    Tick,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Screen {
    Waiting(Mode, Diagnosis),
    Connected(Kind),
}

impl Screen {
    pub fn text(&self) -> String {
        let quit_hint = "Hold SELECT + START to quit";
        match self {
            Self::Waiting(mode, diagnosis) => format!(
                "Pocket Controller\\n\\n{}\\n\\nMode: {}\\n\\nY: change mode    B: quit",
                diagnosis.text(),
                mode.label()
            ),
            Self::Connected(Kind::Xbox) => {
                format!(
                    "Pocket Controller\\n\\nConnected as an Xbox 360 controller\\n\\n{quit_hint}"
                )
            }
            Self::Connected(Kind::Gamepad) => {
                format!("Pocket Controller\\n\\nConnected as a generic gamepad\\n\\n{quit_hint}")
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Start(Kind),
    Restart(Kind),
    Report(State),
    Show(Screen),
    Motor(bool),
    Backlight(Light),
    Hold(Option<u8>),
    Busy(bool),
    Sheet(Option<SheetView>),
    Live(LiveView),
    Settings(Option<SettingsView>),
    Debug(Option<DebugView>),
    SaveMode(Mode),
    SaveDpadMode(DpadMode),
    SaveSettings(Settings),
    Exit,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SheetView {
    pub note: Option<String>,
    pub turbo: Turbo,
    pub dpad: DpadMode,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LiveView {
    pub buttons: u16,
    pub left_trigger: bool,
    pub right_trigger: bool,
    pub volume_up: bool,
    pub volume_down: bool,
    pub fn_switch: bool,
    pub turbo: Turbo,
    pub dpad: DpadMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Light {
    Full,
    Dim,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DebugView {
    pub link: UsbLink,
    pub kind: Kind,
    pub connected: bool,
    pub budget: Option<EndpointBudget>,
    pub xbox_driver: bool,
    pub rumble: u8,
    pub sent: State,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Overlay {
    Settings(Menu),
    Debug,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Saved {
    pub settings: Settings,
    pub dpad: DpadMode,
}

#[derive(Debug)]
pub struct Controller {
    mode: Mode,
    kind: Kind,
    enabled: bool,
    suspended: bool,
    xbox_driver_confirmed: bool,
    xbox_driver_deadline: Option<Instant>,
    held: State,
    chord: ExitChord,
    idle_since: Instant,
    link: UsbLink,
    link_since: Instant,
    dim_at: Option<Instant>,
    dimmed: bool,
    hold: Option<u8>,
    showing_busy: bool,
    overlay: Option<Overlay>,
    settings: Settings,
    settings_shown: Option<SettingsView>,
    debug_shown: Option<DebugView>,
    last_rumble: u8,
    budget: Option<EndpointBudget>,
    layer: Layer,
    extras: Extras,
    last_sent: Option<State>,
    sheet: Option<SheetView>,
    live_shown: Option<LiveView>,
    screen: Screen,
}

impl Controller {
    pub fn new(mode: Mode, now: Instant) -> (Self, Vec<Action>) {
        Self::with_saved(mode, Saved::default(), now)
    }

    pub fn with_saved(mode: Mode, saved: Saved, now: Instant) -> (Self, Vec<Action>) {
        let kind = mode.first_kind();
        let controller = Self {
            mode,
            kind,
            enabled: false,
            suspended: false,
            xbox_driver_confirmed: false,
            xbox_driver_deadline: None,
            held: State::default(),
            chord: ExitChord::default(),
            idle_since: now,
            link: UsbLink::Unknown,
            link_since: now,
            dim_at: None,
            dimmed: false,
            hold: None,
            showing_busy: false,
            overlay: None,
            settings: saved.settings,
            settings_shown: None,
            debug_shown: None,
            last_rumble: 0,
            budget: None,
            layer: Layer::new(saved.settings.face, saved.dpad, now),
            extras: Extras::default(),
            last_sent: None,
            sheet: None,
            live_shown: None,
            screen: Screen::Waiting(mode, Diagnosis::None),
        };
        let screen = controller.screen.clone();
        (controller, vec![Action::Start(kind), Action::Show(screen)])
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    fn connected(&self) -> bool {
        self.enabled && !self.suspended
    }

    fn diagnosis(&self, now: Instant) -> Diagnosis {
        let settled = now.duration_since(self.link_since) >= DIAGNOSIS_AFTER;
        match self.link {
            UsbLink::Powered if settled => Diagnosis::NoComputer,
            UsbLink::Enumerating if settled => Diagnosis::SetupStuck,
            _ => Diagnosis::None,
        }
    }

    fn current_screen(&self, now: Instant) -> Screen {
        match (self.connected(), self.xbox_driver_deadline) {
            (false, _) => Screen::Waiting(self.mode, self.diagnosis(now)),
            (true, Some(_)) => self.screen.clone(),
            (true, None) => Screen::Connected(self.kind),
        }
    }

    fn busy(&self, now: Instant) -> bool {
        let settled = now.duration_since(self.link_since) >= DIAGNOSIS_AFTER;
        let waiting_for_xbox_driver = self.connected() && self.xbox_driver_deadline.is_some();
        let link_in_flux = !self.connected()
            && matches!(self.link, UsbLink::Powered | UsbLink::Enumerating)
            && !settled;
        waiting_for_xbox_driver || link_in_flux
    }

    fn refresh_screen(&mut self, now: Instant, actions: &mut Vec<Action>) {
        let busy = self.busy(now);
        if busy != self.showing_busy {
            self.showing_busy = busy;
            actions.insert(0, Action::Busy(busy));
        }
        let screen = self.current_screen(now);
        if screen != self.screen {
            self.screen = screen.clone();
            actions.insert(0, Action::Show(screen));
        }
    }

    fn switch_to(&mut self, kind: Kind, actions: &mut Vec<Action>) {
        self.kind = kind;
        self.enabled = false;
        self.suspended = false;
        self.xbox_driver_deadline = None;
        self.xbox_driver_confirmed = false;
        actions.push(Action::Motor(false));
        actions.push(Action::Restart(kind));
    }

    fn lose_host(&mut self, now: Instant, actions: &mut Vec<Action>) {
        self.last_sent = None;
        if self.connected() {
            self.idle_since = now;
        }
        actions.push(Action::Motor(false));
        self.dim_at = None;
        self.wake(actions);
    }

    fn show_hold(&mut self, progress: Option<u8>, actions: &mut Vec<Action>) {
        if progress != self.hold {
            self.hold = progress;
            actions.push(Action::Hold(progress));
        }
    }

    fn live_view(&self, now: Instant) -> LiveView {
        let turbo = self.layer.turbo;
        let firing = self.layer.turbo_phase_on(now);
        let pressed = |held: bool, turbo_on: bool| held && (firing || !turbo_on);
        let mut buttons = self.held.buttons;
        if !firing {
            buttons &= !turbo.buttons;
        }
        LiveView {
            buttons,
            left_trigger: pressed(
                self.held.left_trigger > TRIGGER_PRESSED_ABOVE,
                turbo.left_trigger,
            ),
            right_trigger: pressed(
                self.held.right_trigger > TRIGGER_PRESSED_ABOVE,
                turbo.right_trigger,
            ),
            volume_up: self.extras.volume_up,
            volume_down: self.extras.volume_down,
            fn_switch: self.extras.fn_switch,
            turbo,
            dpad: self.layer.dpad_mode,
        }
    }

    fn refresh_live(&mut self, now: Instant, actions: &mut Vec<Action>) {
        if !self.connected() {
            self.live_shown = None;
            return;
        }
        let view = self.live_view(now);
        if self.live_shown != Some(view) {
            self.live_shown = Some(view);
            actions.push(Action::Live(view));
        }
    }

    fn settings_open(&self) -> bool {
        matches!(self.overlay, Some(Overlay::Settings(_)))
    }

    fn layer_input(&self, chord_sent: State) -> Pad {
        Pad {
            state: if self.settings_open() {
                State::default()
            } else {
                chord_sent
            },
            extras: self.extras,
        }
    }

    fn held_pad(&self) -> Pad {
        Pad {
            state: self.held,
            extras: self.extras,
        }
    }

    fn open_settings(&mut self, actions: &mut Vec<Action>) {
        self.overlay = Some(Overlay::Settings(Menu::new(self.mode)));
        self.wake(actions);
    }

    fn open_debug(&mut self, now: Instant) {
        self.overlay = Some(Overlay::Debug);
        self.layer.absorb(self.held_pad(), now);
    }

    fn close_overlay(&mut self, now: Instant, actions: &mut Vec<Action>) {
        if let Some(Overlay::Settings(menu)) = self.overlay.take() {
            self.layer.absorb(self.held_pad(), now);
            self.apply_mode(menu.mode, actions);
        }
        if self.connected() {
            self.dim_at = Some(now + DIM_AFTER);
        }
    }

    fn settings_changed(&mut self, actions: &mut Vec<Action>) {
        self.layer.face = self.settings.face;
        if !self.settings.rumble {
            actions.push(Action::Motor(false));
        }
        actions.push(Action::SaveSettings(self.settings));
    }

    fn navigate_settings(&mut self, pressed: u16, now: Instant, actions: &mut Vec<Action>) {
        if pressed & xinput::START != 0 && self.held.buttons & xinput::GUIDE != 0 {
            self.close_overlay(now, actions);
            return;
        }
        for button in (0..16)
            .map(|bit| 1u16 << bit)
            .filter(|bit| pressed & bit != 0)
        {
            let Some(Overlay::Settings(mut menu)) = self.overlay else {
                return;
            };
            let outcome = menu.press(button, &mut self.settings, self.kind);
            self.overlay = Some(Overlay::Settings(menu));
            match outcome {
                MenuOutcome::Nothing | MenuOutcome::Moved => {}
                MenuOutcome::Changed => self.settings_changed(actions),
                MenuOutcome::Close => self.close_overlay(now, actions),
                MenuOutcome::OpenDebug => self.open_debug(now),
            }
        }
    }

    fn settings_view(&self) -> Option<SettingsView> {
        match self.overlay {
            Some(Overlay::Settings(menu)) => Some(SettingsView {
                cursor: menu.cursor,
                settings: self.settings,
                mode: menu.mode,
                current_mode: self.mode,
                kind: self.kind,
                connected: self.connected(),
            }),
            _ => None,
        }
    }

    fn debug_view(&self) -> Option<DebugView> {
        (self.overlay == Some(Overlay::Debug)).then(|| DebugView {
            link: self.link,
            kind: self.kind,
            connected: self.connected(),
            budget: self.budget,
            xbox_driver: self.xbox_driver_confirmed,
            rumble: self.last_rumble,
            sent: self.last_sent.unwrap_or_default(),
        })
    }

    fn refresh_overlay(&mut self, actions: &mut Vec<Action>) {
        let settings = self.settings_view();
        if settings != self.settings_shown {
            self.settings_shown = settings;
            actions.push(Action::Settings(settings));
        }
        let debug = self.debug_view();
        if debug != self.debug_shown {
            self.debug_shown = debug;
            actions.push(Action::Debug(debug));
        }
    }

    fn run_layer(&mut self, chord_sent: State, now: Instant, actions: &mut Vec<Action>) {
        let output = self.layer.update(self.layer_input(chord_sent), now);
        let mut toggle_overlay = false;
        let mut sheet = self.sheet.clone();
        for event in output.events {
            match event {
                LayerEvent::Sheet(true) => {
                    sheet = Some(SheetView {
                        note: None,
                        turbo: self.layer.turbo,
                        dpad: self.layer.dpad_mode,
                    });
                    if self.dimmed {
                        self.dim_at = Some(now + DIM_AFTER);
                        self.wake(actions);
                    }
                }
                LayerEvent::Sheet(false) => sheet = None,
                LayerEvent::Note(note) => {
                    if let Some(view) = sheet.as_mut() {
                        view.note = Some(note);
                    }
                }
                LayerEvent::TurboChanged(turbo) => {
                    if let Some(view) = sheet.as_mut() {
                        view.turbo = turbo;
                    }
                }
                LayerEvent::DpadModeChanged(mode) => {
                    actions.push(Action::SaveDpadMode(mode));
                    if let Some(view) = sheet.as_mut() {
                        view.dpad = mode;
                    }
                }
                LayerEvent::OpenSettings => toggle_overlay = true,
            }
        }
        if toggle_overlay {
            sheet = None;
        }
        if sheet != self.sheet {
            self.sheet = sheet.clone();
            actions.insert(0, Action::Sheet(sheet));
        }
        if self.connected() && self.last_sent != Some(output.sent) {
            self.last_sent = Some(output.sent);
            actions.push(Action::Report(output.sent));
        }
        if toggle_overlay {
            match self.overlay {
                None => self.open_settings(actions),
                Some(_) => self.close_overlay(now, actions),
            }
        }
    }

    fn apply_mode(&mut self, selected: Mode, actions: &mut Vec<Action>) {
        if selected == self.mode {
            return;
        }
        self.mode = selected;
        actions.push(Action::SaveMode(selected));
        let kind = selected.first_kind();
        if kind != self.kind {
            self.switch_to(kind, actions);
        }
    }

    fn wake(&mut self, actions: &mut Vec<Action>) {
        if self.dimmed {
            self.dimmed = false;
            actions.push(Action::Backlight(Light::Full));
        }
    }

    pub fn handle(&mut self, input: Input, now: Instant) -> Vec<Action> {
        let mut actions = Vec::new();
        match input {
            Input::Pad(pad) => {
                let previous = self.held;
                self.held = pad.state;
                self.extras = pad.extras;
                let outcome = self.chord.update(pad.state, now);
                if outcome.exit {
                    actions.push(Action::Exit);
                    return actions;
                }
                self.show_hold(outcome.progress, &mut actions);
                let menu_was_held = self.layer.menu_held();
                let overlay_was_open = self.overlay.is_some();
                let settings_were_open = self.settings_open();
                self.run_layer(outcome.sent, now, &mut actions);
                let pressed = pad.state.buttons & !previous.buttons;
                if settings_were_open {
                    self.navigate_settings(pressed, now, &mut actions);
                } else if !self.connected()
                    && !overlay_was_open
                    && self.overlay.is_none()
                    && !menu_was_held
                    && !self.layer.menu_held()
                {
                    if pressed & LOCAL_QUIT != 0 {
                        actions.push(Action::Exit);
                        return actions;
                    }
                    if pressed & LOCAL_OPEN_SETTINGS != 0 {
                        self.open_settings(&mut actions);
                    }
                }
            }
            Input::PowerPressed if self.dimmed => {
                self.dim_at = Some(now + DIM_AFTER);
                self.wake(&mut actions);
            }
            Input::PowerPressed | Input::QuitRequested => {
                actions.push(Action::Exit);
                return actions;
            }
            Input::HostEnabled => {
                self.dim_at = Some(now + DIM_AFTER);
                self.enabled = true;
                self.suspended = false;
                if self.mode == Mode::Automatic
                    && self.kind == Kind::Xbox
                    && !self.xbox_driver_confirmed
                {
                    self.xbox_driver_deadline = Some(now + XBOX_DRIVER_WAIT);
                }
                let sent = self.chord.update(self.held, now).sent;
                let output = self.layer.update(self.layer_input(sent), now);
                self.last_sent = Some(output.sent);
                actions.push(Action::Report(output.sent));
            }
            Input::HostDisabled => {
                self.lose_host(now, &mut actions);
                self.enabled = false;
                self.xbox_driver_deadline = None;
            }
            Input::HostSuspended => {
                self.lose_host(now, &mut actions);
                self.suspended = true;
            }
            Input::HostResumed => {
                self.suspended = false;
            }
            Input::XboxDriverSpoke => {
                self.xbox_driver_confirmed = true;
                self.xbox_driver_deadline = None;
            }
            Input::Rumble(level) => {
                self.last_rumble = level;
                if self.connected() && self.settings.rumble && rumble_available(self.kind) {
                    actions.push(Action::Motor(level > 0));
                }
            }
            Input::CableUnplugged => {
                self.lose_host(now, &mut actions);
                self.enabled = false;
                self.suspended = false;
                self.xbox_driver_deadline = None;
                if self.mode == Mode::Automatic && self.kind == Kind::Gamepad {
                    self.switch_to(Kind::Xbox, &mut actions);
                }
            }
            Input::Link(link) => {
                if link != self.link {
                    self.link = link;
                    self.link_since = now;
                }
            }
            Input::Endpoints(budget) => self.budget = Some(budget),
            Input::Tick => {
                let outcome = self.chord.update(self.held, now);
                if outcome.exit {
                    actions.push(Action::Exit);
                    return actions;
                }
                self.show_hold(outcome.progress, &mut actions);
                self.run_layer(outcome.sent, now, &mut actions);
                if self
                    .xbox_driver_deadline
                    .is_some_and(|deadline| now >= deadline)
                {
                    self.switch_to(Kind::Gamepad, &mut actions);
                }
                if self.connected()
                    && self.overlay.is_none()
                    && !self.dimmed
                    && self.dim_at.is_some_and(|at| now >= at)
                {
                    let light = match self.settings.screen {
                        ScreenWhilePlaying::DimAfter => Some(Light::Dim),
                        ScreenWhilePlaying::OffAfter => Some(Light::Off),
                        ScreenWhilePlaying::AlwaysOn => None,
                    };
                    if let Some(light) = light {
                        self.dimmed = true;
                        actions.push(Action::Backlight(light));
                    }
                }
                if !self.connected() && now.duration_since(self.idle_since) >= IDLE_LIMIT {
                    actions.push(Action::Exit);
                    return actions;
                }
            }
        }
        self.refresh_screen(now, &mut actions);
        self.refresh_live(now, &mut actions);
        self.refresh_overlay(&mut actions);
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord::EXIT_CHORD;
    use crate::layer::FaceLayout;
    use crate::settings::Row;

    fn holding(buttons: u16) -> Input {
        Input::Pad(Pad {
            state: State {
                buttons,
                ..State::default()
            },
            extras: Extras::default(),
        })
    }

    fn without_live(actions: Vec<Action>) -> Vec<Action> {
        actions
            .into_iter()
            .filter(|action| !matches!(action, Action::Live(_)))
            .collect()
    }

    fn restarts(actions: &[Action]) -> Vec<Kind> {
        actions
            .iter()
            .filter_map(|action| match action {
                Action::Restart(kind) => Some(*kind),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn automatic_mode_starts_as_an_xbox_controller_and_waits() {
        let (_, actions) = Controller::new(Mode::Automatic, Instant::now());
        assert_eq!(
            actions,
            vec![
                Action::Start(Kind::Xbox),
                Action::Show(Screen::Waiting(Mode::Automatic, Diagnosis::None))
            ]
        );
    }

    #[test]
    fn gamepad_mode_starts_as_a_gamepad() {
        let (_, actions) = Controller::new(Mode::Gamepad, Instant::now());
        assert_eq!(actions[0], Action::Start(Kind::Gamepad));
    }

    #[test]
    fn automatic_mode_stays_xbox_when_the_xbox_driver_speaks() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Automatic, start);
        let actions = without_live(controller.handle(Input::HostEnabled, start));
        assert_eq!(
            actions,
            vec![Action::Busy(true), Action::Report(State::default())]
        );
        let actions = controller.handle(Input::XboxDriverSpoke, start + Duration::from_millis(50));
        assert!(actions.contains(&Action::Show(Screen::Connected(Kind::Xbox))));
        assert!(actions.contains(&Action::Busy(false)));
        let actions = controller.handle(Input::Tick, start + Duration::from_secs(5));
        assert!(restarts(&actions).is_empty());
        assert_eq!(controller.kind(), Kind::Xbox);
    }

    #[test]
    fn automatic_mode_falls_back_to_a_gamepad_when_no_xbox_driver_speaks() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Automatic, start);
        controller.handle(Input::HostEnabled, start);
        assert!(
            restarts(&controller.handle(Input::Tick, start + Duration::from_millis(1999)))
                .is_empty()
        );
        let actions = controller.handle(Input::Tick, start + XBOX_DRIVER_WAIT);
        assert_eq!(restarts(&actions), vec![Kind::Gamepad]);
        assert_eq!(controller.kind(), Kind::Gamepad);
    }

    #[test]
    fn a_late_xbox_driver_does_not_switch_back() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Automatic, start);
        controller.handle(Input::HostEnabled, start);
        controller.handle(Input::Tick, start + XBOX_DRIVER_WAIT);
        let actions = controller.handle(Input::XboxDriverSpoke, start + Duration::from_secs(3));
        assert!(restarts(&actions).is_empty());
        assert_eq!(controller.kind(), Kind::Gamepad);
    }

    #[test]
    fn unplugging_in_automatic_mode_tries_xbox_again_for_the_next_computer() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Automatic, start);
        controller.handle(Input::HostEnabled, start);
        controller.handle(Input::Tick, start + XBOX_DRIVER_WAIT);
        let actions = controller.handle(Input::CableUnplugged, start + Duration::from_secs(10));
        assert_eq!(restarts(&actions), vec![Kind::Xbox]);
    }

    #[test]
    fn fixed_xbox_mode_never_falls_back() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        controller.handle(Input::HostEnabled, start);
        let actions = controller.handle(Input::Tick, start + Duration::from_secs(10));
        assert!(restarts(&actions).is_empty());
        assert!(actions.is_empty());
    }

    #[test]
    fn buttons_reach_the_host_only_while_connected() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        assert!(
            !controller
                .handle(holding(xinput::Y), start)
                .iter()
                .any(|a| matches!(a, Action::Report(_)))
        );
        controller.handle(Input::HostEnabled, start);
        let actions = without_live(controller.handle(holding(xinput::B), start));
        assert_eq!(
            actions,
            vec![Action::Report(State {
                buttons: xinput::A,
                ..State::default()
            })],
            "the button printed A, on the right, sends A by default"
        );
    }

    #[test]
    fn holding_menu_shows_the_cheat_sheet_with_the_current_settings() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        controller.handle(Input::HostEnabled, start);
        controller.handle(holding(xinput::GUIDE), start);
        let actions = controller.handle(Input::Tick, start + Duration::from_millis(320));
        assert!(
            actions.contains(&Action::Sheet(Some(SheetView::default()))),
            "{actions:?}"
        );
        let released = controller.handle(holding(0), start + Duration::from_millis(600));
        assert!(released.contains(&Action::Sheet(None)));
    }

    #[test]
    fn menu_and_a_direction_saves_the_dpad_mode() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        controller.handle(holding(xinput::GUIDE), start);
        let left = controller.handle(
            holding(xinput::GUIDE | xinput::DPAD_LEFT),
            start + Duration::from_millis(40),
        );
        assert!(left.contains(&Action::SaveDpadMode(DpadMode::LeftStick)));
    }

    #[test]
    fn menu_chords_never_trigger_local_controls_while_waiting() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        controller.handle(holding(xinput::GUIDE), start);
        let actions = controller.handle(
            holding(xinput::GUIDE | LOCAL_QUIT),
            start + Duration::from_millis(20),
        );
        assert!(!actions.contains(&Action::Exit), "{actions:?}");
    }

    #[test]
    fn the_live_view_shows_raw_presses_only_while_connected() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        let waiting = controller.handle(holding(xinput::B), start);
        assert!(!waiting.iter().any(|a| matches!(a, Action::Live(_))));
        controller.handle(Input::HostEnabled, start);
        let pressed = controller.handle(
            holding(xinput::B | xinput::GUIDE),
            start + Duration::from_millis(20),
        );
        assert!(pressed.contains(&Action::Live(LiveView {
            buttons: xinput::B | xinput::GUIDE,
            ..LiveView::default()
        })));
    }

    #[test]
    fn a_held_turbo_button_blinks_on_screen_as_it_does_for_the_host() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        controller.handle(Input::HostEnabled, start);
        controller.handle(holding(xinput::GUIDE), start);
        controller.handle(
            holding(xinput::GUIDE | xinput::B),
            start + Duration::from_millis(20),
        );
        controller.handle(holding(0), start + Duration::from_millis(40));
        let shown = |actions: Vec<Action>| {
            actions.into_iter().find_map(|action| match action {
                Action::Live(view) => Some(view.buttons & xinput::B != 0),
                _ => None,
            })
        };
        assert_eq!(
            shown(controller.handle(holding(xinput::B), start + Duration::from_millis(80))),
            Some(true)
        );
        assert_eq!(
            shown(controller.handle(Input::Tick, start + Duration::from_millis(120))),
            Some(false)
        );
        assert_eq!(
            shown(controller.handle(Input::Tick, start + Duration::from_millis(160))),
            Some(true)
        );
    }

    #[test]
    fn an_unchanged_report_is_not_sent_again() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        controller.handle(Input::HostEnabled, start);
        controller.handle(holding(xinput::B), start);
        let again = controller.handle(Input::Tick, start + Duration::from_millis(20));
        assert!(
            !again.iter().any(|a| matches!(a, Action::Report(_))),
            "{again:?}"
        );
    }

    #[test]
    fn local_quit_works_only_without_a_computer() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        assert_eq!(
            controller.handle(holding(LOCAL_QUIT), start),
            vec![Action::Exit]
        );
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        controller.handle(Input::HostEnabled, start);
        assert!(
            !controller
                .handle(holding(LOCAL_QUIT), start)
                .contains(&Action::Exit)
        );
    }

    fn tap(controller: &mut Controller, buttons: u16, at: Instant) -> Vec<Action> {
        let mut actions = controller.handle(holding(buttons), at);
        actions.extend(controller.handle(holding(0), at + Duration::from_millis(30)));
        actions
    }

    fn settings_shown(actions: &[Action]) -> Option<Option<SettingsView>> {
        actions.iter().rev().find_map(|action| match action {
            Action::Settings(view) => Some(*view),
            _ => None,
        })
    }

    fn reports(actions: &[Action]) -> Vec<State> {
        actions
            .iter()
            .filter_map(|action| match action {
                Action::Report(state) => Some(*state),
                _ => None,
            })
            .collect()
    }

    fn open_with_menu_and_start(controller: &mut Controller, at: Instant) -> Vec<Action> {
        controller.handle(holding(xinput::GUIDE), at);
        let mut actions = controller.handle(holding(xinput::GUIDE | xinput::START), at);
        actions.extend(controller.handle(holding(0), at + Duration::from_millis(30)));
        actions
    }

    fn move_to(controller: &mut Controller, row: Row, at: Instant) {
        let steps = Row::ALL.iter().position(|each| *each == row).unwrap_or(0);
        for _ in 0..steps {
            tap(controller, xinput::DPAD_DOWN, at);
        }
    }

    #[test]
    fn a_on_the_waiting_screen_opens_settings_on_the_first_row() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Automatic, start);
        let view = settings_shown(&tap(&mut controller, PRINTED_A, start)).flatten();
        assert_eq!(view.map(|view| view.cursor), Some(Row::Face));
    }

    #[test]
    fn b_closes_settings_on_the_waiting_screen_without_quitting() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Automatic, start);
        tap(&mut controller, PRINTED_A, start);
        let actions = tap(&mut controller, PRINTED_B, start);
        assert!(!actions.contains(&Action::Exit));
        assert_eq!(settings_shown(&actions), Some(None));
    }

    #[test]
    fn menu_and_start_open_settings_and_the_computer_gets_only_a_neutral_report() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        controller.handle(Input::HostEnabled, start);
        let actions = open_with_menu_and_start(&mut controller, start);
        assert!(matches!(settings_shown(&actions), Some(Some(_))));
        let actions = controller.handle(holding(xinput::Y | xinput::DPAD_DOWN), start);
        assert!(
            reports(&actions)
                .iter()
                .all(|state| *state == State::default()),
            "{actions:?}"
        );
        let actions = open_with_menu_and_start(&mut controller, start);
        assert_eq!(settings_shown(&actions), Some(None));
    }

    #[test]
    fn a_changed_setting_applies_and_is_saved_at_once() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Automatic, start);
        tap(&mut controller, PRINTED_A, start);
        let actions = tap(&mut controller, xinput::DPAD_RIGHT, start);
        assert!(actions.contains(&Action::SaveSettings(Settings {
            face: FaceLayout::Position,
            ..Settings::default()
        })));
    }

    #[test]
    fn the_mode_applies_when_settings_close() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Automatic, start);
        tap(&mut controller, PRINTED_A, start);
        move_to(&mut controller, Row::Mode, start);
        tap(&mut controller, xinput::DPAD_RIGHT, start);
        let actions = tap(&mut controller, xinput::DPAD_RIGHT, start);
        assert!(!actions.contains(&Action::SaveMode(Mode::Gamepad)));
        let actions = tap(&mut controller, PRINTED_B, start);
        assert!(actions.contains(&Action::SaveMode(Mode::Gamepad)));
        assert_eq!(restarts(&actions), vec![Kind::Gamepad]);
    }

    #[test]
    fn switching_between_automatic_and_xbox_style_keeps_the_usb_controller() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Automatic, start);
        tap(&mut controller, PRINTED_A, start);
        move_to(&mut controller, Row::Mode, start);
        tap(&mut controller, xinput::DPAD_RIGHT, start);
        let actions = tap(&mut controller, PRINTED_B, start);
        assert!(actions.contains(&Action::SaveMode(Mode::Xbox)));
        assert!(restarts(&actions).is_empty());
    }

    #[test]
    fn a_mode_that_rebuilds_the_controller_warns_while_connected() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        controller.handle(Input::HostEnabled, start);
        open_with_menu_and_start(&mut controller, start);
        move_to(&mut controller, Row::Mode, start);
        let view = settings_shown(&tap(&mut controller, xinput::DPAD_RIGHT, start)).flatten();
        assert!(view.is_some_and(|view| view.mode_change_warning()));
    }

    #[test]
    fn buttons_held_when_settings_close_wait_for_release() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        controller.handle(Input::HostEnabled, start);
        open_with_menu_and_start(&mut controller, start);
        controller.handle(holding(PRINTED_B | xinput::Y), start);
        let actions = controller.handle(holding(PRINTED_B | xinput::Y), start);
        assert!(reports(&actions).iter().all(|state| state.buttons == 0));
        controller.handle(holding(xinput::Y), start);
        controller.handle(holding(0), start);
        let actions = controller.handle(holding(xinput::Y), start);
        assert!(reports(&actions).iter().any(|state| state.buttons != 0));
    }

    #[test]
    fn rumble_turned_off_keeps_the_motor_still() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        controller.handle(Input::HostEnabled, start);
        open_with_menu_and_start(&mut controller, start);
        move_to(&mut controller, Row::Rumble, start);
        tap(&mut controller, xinput::DPAD_RIGHT, start);
        tap(&mut controller, PRINTED_B, start);
        assert!(
            !controller
                .handle(Input::Rumble(255), start)
                .contains(&Action::Motor(true))
        );
    }

    #[test]
    fn the_debug_row_opens_a_view_that_lets_buttons_through() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        controller.handle(Input::HostEnabled, start);
        open_with_menu_and_start(&mut controller, start);
        move_to(&mut controller, Row::Debug, start);
        let actions = tap(&mut controller, PRINTED_A, start);
        assert_eq!(settings_shown(&actions), Some(None));
        assert!(
            actions
                .iter()
                .any(|action| matches!(action, Action::Debug(Some(_))))
        );
        let actions = controller.handle(holding(xinput::Y), start);
        assert!(
            reports(&actions)
                .iter()
                .any(|state| state.buttons == xinput::X)
        );
        controller.handle(holding(0), start);
        let actions = open_with_menu_and_start(&mut controller, start);
        assert!(actions.contains(&Action::Debug(None)));
    }

    #[test]
    fn the_screen_setting_picks_dim_off_or_nothing() {
        for (screen, expected) in [
            (ScreenWhilePlaying::DimAfter, Some(Light::Dim)),
            (ScreenWhilePlaying::OffAfter, Some(Light::Off)),
            (ScreenWhilePlaying::AlwaysOn, None),
        ] {
            let start = Instant::now();
            let saved = Saved {
                settings: Settings {
                    screen,
                    ..Settings::default()
                },
                dpad: DpadMode::Dpad,
            };
            let (mut controller, _) = Controller::with_saved(Mode::Xbox, saved, start);
            controller.handle(Input::HostEnabled, start);
            let actions = controller.handle(Input::Tick, start + DIM_AFTER);
            let light = actions.iter().find_map(|action| match action {
                Action::Backlight(light) => Some(*light),
                _ => None,
            });
            assert_eq!(light, expected, "{screen:?}");
        }
    }

    #[test]
    fn the_screen_never_dims_while_settings_are_open() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        controller.handle(Input::HostEnabled, start);
        open_with_menu_and_start(&mut controller, start);
        let actions = controller.handle(Input::Tick, start + DIM_AFTER * 3);
        assert!(!actions.contains(&Action::Backlight(Light::Dim)));
    }

    #[test]
    fn exit_chord_held_two_seconds_quits_even_while_connected() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        controller.handle(Input::HostEnabled, start);
        let actions = without_live(controller.handle(holding(EXIT_CHORD), start));
        assert_eq!(
            actions,
            vec![Action::Hold(Some(0))],
            "the chord itself never reaches the host"
        );
        assert_eq!(
            controller.handle(Input::Tick, start + Duration::from_secs(2)),
            vec![Action::Exit]
        );
    }

    #[test]
    fn holding_the_exit_chord_reports_progress_and_letting_go_clears_it() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        assert!(
            controller
                .handle(holding(EXIT_CHORD), start)
                .contains(&Action::Hold(Some(0)))
        );
        assert!(
            controller
                .handle(Input::Tick, start + Duration::from_millis(850))
                .contains(&Action::Hold(Some(50)))
        );
        assert!(
            controller
                .handle(Input::Tick, start + Duration::from_millis(870))
                .is_empty()
        );
        assert!(
            controller
                .handle(holding(0), start + Duration::from_millis(900))
                .contains(&Action::Hold(None))
        );
    }

    #[test]
    fn power_button_quits() {
        let (mut controller, _) = Controller::new(Mode::Xbox, Instant::now());
        assert_eq!(
            controller.handle(Input::PowerPressed, Instant::now()),
            vec![Action::Exit]
        );
    }

    #[test]
    fn rumble_drives_the_motor_and_losing_the_host_stops_it() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        controller.handle(Input::HostEnabled, start);
        assert_eq!(
            controller.handle(Input::Rumble(200), start),
            vec![Action::Motor(true)]
        );
        assert_eq!(
            controller.handle(Input::Rumble(0), start),
            vec![Action::Motor(false)]
        );
        let actions = controller.handle(Input::HostSuspended, start);
        assert!(actions.contains(&Action::Motor(false)));
    }

    #[test]
    fn ten_idle_minutes_without_a_computer_quit() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        assert!(
            controller
                .handle(Input::Tick, start + Duration::from_secs(599))
                .is_empty()
        );
        assert_eq!(
            controller.handle(Input::Tick, start + IDLE_LIMIT),
            vec![Action::Exit]
        );
    }

    #[test]
    fn a_connected_computer_keeps_it_running() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        controller.handle(Input::HostEnabled, start);
        assert!(
            !controller
                .handle(Input::Tick, start + Duration::from_secs(3600))
                .contains(&Action::Exit)
        );
    }

    fn waiting_diagnosis(actions: &[Action]) -> Option<Diagnosis> {
        actions.iter().find_map(|action| match action {
            Action::Show(Screen::Waiting(_, diagnosis)) => Some(*diagnosis),
            _ => None,
        })
    }

    #[test]
    fn no_power_stays_a_neutral_wait_because_an_unplugged_cable_reads_the_same() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        controller.handle(Input::Link(UsbLink::NoPower), start);
        let actions = controller.handle(Input::Tick, start + Duration::from_secs(60));
        assert_eq!(waiting_diagnosis(&actions), None);
    }

    #[test]
    fn a_link_in_flux_keeps_the_screen_and_shows_loading_until_it_settles() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        let actions = controller.handle(Input::Link(UsbLink::Powered), start);
        assert_eq!(actions, vec![Action::Busy(true)]);
        let actions = controller.handle(
            Input::Link(UsbLink::Enumerating),
            start + Duration::from_millis(300),
        );
        assert!(actions.is_empty(), "{actions:?}");
        let actions = controller.handle(Input::HostEnabled, start + Duration::from_millis(400));
        assert!(actions.contains(&Action::Busy(false)));
        assert!(actions.contains(&Action::Show(Screen::Connected(Kind::Xbox))));
    }

    #[test]
    fn power_without_a_computer_is_reported_after_three_seconds() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        assert_eq!(
            waiting_diagnosis(&controller.handle(Input::Link(UsbLink::Powered), start)),
            None
        );
        assert_eq!(
            waiting_diagnosis(&controller.handle(Input::Tick, start + Duration::from_millis(2900))),
            None
        );
        let actions = controller.handle(Input::Tick, start + DIAGNOSIS_AFTER);
        assert_eq!(waiting_diagnosis(&actions), Some(Diagnosis::NoComputer));
    }

    #[test]
    fn setup_that_never_finishes_is_reported() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        controller.handle(Input::Link(UsbLink::Enumerating), start);
        let actions = controller.handle(Input::Tick, start + DIAGNOSIS_AFTER);
        assert_eq!(waiting_diagnosis(&actions), Some(Diagnosis::SetupStuck));
    }

    #[test]
    fn a_link_change_restarts_the_wait() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        controller.handle(Input::Link(UsbLink::Powered), start);
        controller.handle(
            Input::Link(UsbLink::Enumerating),
            start + Duration::from_secs(2),
        );
        let actions = controller.handle(Input::Tick, start + Duration::from_secs(4));
        assert_eq!(waiting_diagnosis(&actions), None);
    }

    #[test]
    fn usb_link_is_classified_from_power_and_controller_state() {
        assert_eq!(
            UsbLink::classify(Some(false), Some("not attached")),
            UsbLink::NoPower
        );
        assert_eq!(
            UsbLink::classify(Some(true), Some("not attached")),
            UsbLink::Powered
        );
        assert_eq!(
            UsbLink::classify(Some(true), Some("default")),
            UsbLink::Enumerating
        );
        assert_eq!(
            UsbLink::classify(Some(true), Some("configured\n")),
            UsbLink::Configured
        );
        assert_eq!(
            UsbLink::classify(None, Some("not attached")),
            UsbLink::Unknown
        );
    }

    #[test]
    fn the_screen_dims_thirty_seconds_after_a_computer_connects() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        controller.handle(Input::HostEnabled, start);
        assert!(
            !controller
                .handle(Input::Tick, start + Duration::from_secs(29))
                .contains(&Action::Backlight(Light::Dim))
        );
        assert!(
            controller
                .handle(Input::Tick, start + DIM_AFTER)
                .contains(&Action::Backlight(Light::Dim))
        );
        assert!(
            !controller
                .handle(Input::Tick, start + Duration::from_secs(31))
                .contains(&Action::Backlight(Light::Dim))
        );
    }

    #[test]
    fn power_wakes_a_dimmed_screen_instead_of_quitting() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        controller.handle(Input::HostEnabled, start);
        controller.handle(Input::Tick, start + DIM_AFTER);
        let actions = controller.handle(Input::PowerPressed, start + DIM_AFTER);
        assert_eq!(actions, vec![Action::Backlight(Light::Full)]);
        assert_eq!(
            controller.handle(Input::PowerPressed, start + DIM_AFTER),
            vec![Action::Exit]
        );
    }

    #[test]
    fn losing_the_computer_wakes_the_screen() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        controller.handle(Input::HostEnabled, start);
        controller.handle(Input::Tick, start + DIM_AFTER);
        assert!(
            controller
                .handle(Input::CableUnplugged, start + DIM_AFTER)
                .contains(&Action::Backlight(Light::Full))
        );
    }

    #[test]
    fn the_screen_never_dims_without_a_computer() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        assert!(
            !controller
                .handle(Input::Tick, start + Duration::from_secs(120))
                .contains(&Action::Backlight(Light::Dim))
        );
    }

    #[test]
    fn mode_names_round_trip() {
        for mode in [Mode::Automatic, Mode::Xbox, Mode::Gamepad] {
            assert_eq!(Mode::parse(mode.as_str()), Some(mode));
        }
        assert_eq!(Mode::parse("bogus"), None);
    }

    #[test]
    fn waiting_screen_names_the_mode() {
        assert!(
            Screen::Waiting(Mode::Gamepad, Diagnosis::None)
                .text()
                .contains("Mode: Generic")
        );
    }
}
