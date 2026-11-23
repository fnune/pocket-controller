use std::time::{Duration, Instant};

use crate::chord::ExitChord;
use crate::evdev::{Extras, Pad};
use crate::layer::{DpadMode, FaceLayout, Layer, LayerEvent, TRIGGER_PRESSED_ABOVE, Turbo};
use crate::xinput::{self, State};

pub const XBOX_DRIVER_WAIT: Duration = Duration::from_secs(2);
pub const IDLE_LIMIT: Duration = Duration::from_secs(10 * 60);

pub const DIAGNOSIS_AFTER: Duration = Duration::from_secs(3);
pub const DIM_AFTER: Duration = Duration::from_secs(30);
pub const PICKER_CLOSES_AFTER: Duration = Duration::from_secs(3);

pub const LOCAL_QUIT: u16 = xinput::A;
pub const LOCAL_CHANGE_MODE: u16 = xinput::X;

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

    fn next(self) -> Self {
        match self {
            Self::Automatic => Self::Xbox,
            Self::Xbox => Self::Gamepad,
            Self::Gamepad => Self::Automatic,
        }
    }

    fn first_kind(self) -> Kind {
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
    Dim(bool),
    Hold(Option<u8>),
    Busy(bool),
    Picker(Option<Mode>),
    Sheet(Option<SheetView>),
    Live(LiveView),
    SaveMode(Mode),
    SaveDpadMode(DpadMode),
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputSettings {
    pub face: FaceLayout,
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
    picker: Option<(Mode, Instant)>,
    layer: Layer,
    extras: Extras,
    last_sent: Option<State>,
    sheet: Option<SheetView>,
    live_shown: Option<LiveView>,
    screen: Screen,
}

impl Controller {
    pub fn new(mode: Mode, now: Instant) -> (Self, Vec<Action>) {
        Self::with_input(mode, InputSettings::default(), now)
    }

    pub fn with_input(mode: Mode, input: InputSettings, now: Instant) -> (Self, Vec<Action>) {
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
            picker: None,
            layer: Layer::new(input.face, input.dpad, now),
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

    fn run_layer(&mut self, chord_sent: State, now: Instant, actions: &mut Vec<Action>) {
        let output = self.layer.update(
            Pad {
                state: chord_sent,
                extras: self.extras,
            },
            now,
        );
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
                LayerEvent::OpenSettings => {
                    if let Some(view) = sheet.as_mut() {
                        view.note = Some("Settings are not available yet".to_string());
                    }
                }
            }
        }
        if sheet != self.sheet {
            self.sheet = sheet.clone();
            actions.insert(0, Action::Sheet(sheet));
        }
        if self.connected() && self.last_sent != Some(output.sent) {
            self.last_sent = Some(output.sent);
            actions.push(Action::Report(output.sent));
        }
    }

    fn close_picker(&mut self, selected: Mode, actions: &mut Vec<Action>) {
        self.picker = None;
        actions.push(Action::Picker(None));
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
            actions.push(Action::Dim(false));
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
                self.run_layer(outcome.sent, now, &mut actions);
                if !self.connected() && !menu_was_held && !self.layer.menu_held() {
                    let pressed = pad.state.buttons & !previous.buttons;
                    if pressed & LOCAL_QUIT != 0 {
                        actions.push(Action::Exit);
                        return actions;
                    }
                    if pressed & LOCAL_CHANGE_MODE != 0 {
                        let selected = match self.picker {
                            Some((selected, _)) => selected.next(),
                            None => self.mode,
                        };
                        self.picker = Some((selected, now + PICKER_CLOSES_AFTER));
                        actions.insert(0, Action::Picker(Some(selected)));
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
                let output = self.layer.update(
                    Pad {
                        state: sent,
                        extras: self.extras,
                    },
                    now,
                );
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
                if self.connected() {
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
            Input::Tick => {
                if let Some((selected, closes_at)) = self.picker
                    && now >= closes_at
                {
                    self.close_picker(selected, &mut actions);
                }
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
                if self.connected() && !self.dimmed && self.dim_at.is_some_and(|at| now >= at) {
                    self.dimmed = true;
                    actions.push(Action::Dim(true));
                }
                if !self.connected() && now.duration_since(self.idle_since) >= IDLE_LIMIT {
                    actions.push(Action::Exit);
                    return actions;
                }
            }
        }
        self.refresh_screen(now, &mut actions);
        self.refresh_live(now, &mut actions);
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord::EXIT_CHORD;

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

    fn pick(controller: &mut Controller, at: Instant) -> Vec<Action> {
        let actions = controller.handle(holding(LOCAL_CHANGE_MODE), at);
        controller.handle(holding(0), at);
        actions
    }

    #[test]
    fn the_first_press_opens_the_picker_on_the_current_mode_without_changing_it() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Automatic, start);
        let actions = pick(&mut controller, start);
        assert_eq!(actions, vec![Action::Picker(Some(Mode::Automatic))]);
    }

    #[test]
    fn further_presses_move_the_selection_and_closing_applies_it() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Automatic, start);
        pick(&mut controller, start);
        assert_eq!(
            pick(&mut controller, start + Duration::from_millis(500)),
            vec![Action::Picker(Some(Mode::Xbox))]
        );
        assert_eq!(
            pick(&mut controller, start + Duration::from_secs(1)),
            vec![Action::Picker(Some(Mode::Gamepad))]
        );
        assert!(
            controller
                .handle(Input::Tick, start + Duration::from_millis(3900))
                .is_empty()
        );
        let actions = controller.handle(Input::Tick, start + Duration::from_secs(4));
        assert_eq!(
            actions.first(),
            Some(&Action::Show(Screen::Waiting(
                Mode::Gamepad,
                Diagnosis::None
            )))
        );
        assert!(actions.contains(&Action::Picker(None)));
        assert!(actions.contains(&Action::SaveMode(Mode::Gamepad)));
        assert_eq!(restarts(&actions), vec![Kind::Gamepad]);
    }

    #[test]
    fn closing_on_the_same_mode_saves_and_rebuilds_nothing() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        pick(&mut controller, start);
        let actions = controller.handle(Input::Tick, start + PICKER_CLOSES_AFTER);
        assert_eq!(actions, vec![Action::Picker(None)]);
    }

    #[test]
    fn picking_between_automatic_and_xbox_style_keeps_the_usb_controller() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Automatic, start);
        pick(&mut controller, start);
        pick(&mut controller, start);
        let actions = controller.handle(Input::Tick, start + PICKER_CLOSES_AFTER);
        assert!(actions.contains(&Action::SaveMode(Mode::Xbox)));
        assert!(restarts(&actions).is_empty());
    }

    #[test]
    fn holding_the_button_does_not_cycle_modes_repeatedly() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Automatic, start);
        controller.handle(holding(LOCAL_CHANGE_MODE), start);
        let actions = controller.handle(holding(LOCAL_CHANGE_MODE | xinput::DPAD_UP), start);
        assert!(actions.is_empty(), "{actions:?}");
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
    fn opening_the_picker_comes_before_any_other_work() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        let actions = controller.handle(holding(LOCAL_CHANGE_MODE), start);
        assert!(
            matches!(actions.first(), Some(Action::Picker(_))),
            "{actions:?}"
        );
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
                .contains(&Action::Dim(true))
        );
        assert!(
            controller
                .handle(Input::Tick, start + DIM_AFTER)
                .contains(&Action::Dim(true))
        );
        assert!(
            !controller
                .handle(Input::Tick, start + Duration::from_secs(31))
                .contains(&Action::Dim(true))
        );
    }

    #[test]
    fn power_wakes_a_dimmed_screen_instead_of_quitting() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        controller.handle(Input::HostEnabled, start);
        controller.handle(Input::Tick, start + DIM_AFTER);
        let actions = controller.handle(Input::PowerPressed, start + DIM_AFTER);
        assert_eq!(actions, vec![Action::Dim(false)]);
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
                .contains(&Action::Dim(false))
        );
    }

    #[test]
    fn the_screen_never_dims_without_a_computer() {
        let start = Instant::now();
        let (mut controller, _) = Controller::new(Mode::Xbox, start);
        assert!(
            !controller
                .handle(Input::Tick, start + Duration::from_secs(120))
                .contains(&Action::Dim(true))
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
