use std::ffi::CString;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use crate::control::{DebugView, Input, LiveView, Mode, Saved, Screen, SheetView};
use crate::evdev::{self, AbsRange, Event, InputDevice, PadReader};
use crate::frame::Frame;
use crate::gadget::GadgetFs;
use crate::layer::{DpadMode, FaceLayout};
use crate::settings::{ScreenWhilePlaying, Settings, SettingsView};
use crate::xinput::EndpointBudget;

pub struct RealFs;

impl GadgetFs for RealFs {
    fn exists(&self, path: &Path) -> bool {
        path.symlink_metadata().is_ok()
    }

    fn read(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn write(&self, path: &Path, value: &str) -> io::Result<()> {
        std::fs::write(path, value)
    }

    fn create_dir(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir(path)
    }

    fn remove_dir(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_dir(path)
    }

    fn symlink(&self, target: &Path, link: &Path) -> io::Result<()> {
        std::os::unix::fs::symlink(target, link)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }

    fn list(&self, path: &Path) -> io::Result<Vec<String>> {
        std::fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
            .collect()
    }

    fn list_links(&self, path: &Path) -> io::Result<Vec<String>> {
        let mut links = Vec::new();
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            if entry.file_type()?.is_symlink() {
                links.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
        Ok(links)
    }

    fn unmount(&self, path: &Path) -> io::Result<()> {
        let target = CString::new(path.as_os_str().as_bytes()).map_err(io::Error::other)?;
        if unsafe { libc::umount2(target.as_ptr(), libc::MNT_DETACH) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

const fn ioc(direction: u64, number: u64, size: u64) -> u64 {
    direction << 30 | size << 16 | (b'E' as u64) << 8 | number
}
const IOC_WRITE: u64 = 1;
const IOC_READ: u64 = 2;
const ABS_INFO_SIZE: usize = 24;
const KEY_BITMAP_SIZE: usize = 96;
const EVIOCGRAB: u64 = ioc(IOC_WRITE, 0x90, 4);
const fn eviocgabs(axis: u16) -> u64 {
    ioc(IOC_READ, 0x40 + axis as u64, ABS_INFO_SIZE as u64)
}
const EVIOCGKEY: u64 = ioc(IOC_READ, 0x18, KEY_BITMAP_SIZE as u64);
const SWITCH_BITMAP_SIZE: usize = 8;
const EVIOCGSW: u64 = ioc(IOC_READ, 0x1b, SWITCH_BITMAP_SIZE as u64);

const DISP_LCD_SET_BRIGHTNESS: u64 = 0x102;
const DISP_LCD_GET_BRIGHTNESS: u64 = 0x103;
const DISPLAY_DEVICE: &str = "/dev/disp";

pub struct Backlight {
    saved: PathBuf,
    original: Option<u32>,
    session: Option<u32>,
    dimmed: bool,
}

fn display_ioctl(request: u64, value: u32) -> io::Result<i32> {
    let display = OpenOptions::new()
        .read(true)
        .write(true)
        .open(DISPLAY_DEVICE)?;
    let mut parameters: [libc::c_ulong; 4] = [0, value as libc::c_ulong, 0, 0];
    let result = unsafe { libc::ioctl(display.as_raw_fd(), request as _, parameters.as_mut_ptr()) };
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(result)
    }
}

pub fn read_brightness() -> io::Result<u32> {
    display_ioctl(DISP_LCD_GET_BRIGHTNESS, 0).map(|level| level as u32)
}

pub fn write_brightness(level: u32) -> io::Result<()> {
    display_ioctl(DISP_LCD_SET_BRIGHTNESS, level).map(drop)
}

impl Backlight {
    pub fn new(saved: PathBuf) -> Self {
        Self {
            saved,
            original: None,
            session: None,
            dimmed: false,
        }
    }

    fn remember_original(&mut self) -> Option<u32> {
        if let Some(original) = self.original {
            return Some(original);
        }
        match read_brightness() {
            Ok(original) => {
                if let Err(error) = std::fs::write(&self.saved, original.to_string()) {
                    eprintln!(
                        "could not save the brightness to {}: {error}",
                        self.saved.display()
                    );
                    return None;
                }
                self.original = Some(original);
                Some(original)
            }
            Err(error) => {
                eprintln!("could not read the brightness: {error}");
                None
            }
        }
    }

    fn apply(&self, level: u32) {
        if let Err(error) = write_brightness(level) {
            eprintln!("could not set the brightness to {level}: {error}");
        }
    }

    pub fn dim(&mut self, level: u32) {
        if self.remember_original().is_some() {
            self.dimmed = true;
            self.apply(level);
        }
    }

    pub fn undim(&mut self) {
        self.dimmed = false;
        if let Some(level) = self.session.or(self.original) {
            self.apply(level);
        }
    }

    pub fn restore(&mut self) {
        if let Some(original) = self.original.take() {
            match write_brightness(original) {
                Ok(()) => {
                    let _ = std::fs::remove_file(&self.saved);
                }
                Err(error) => eprintln!("could not restore the brightness: {error}"),
            }
        }
        self.session = None;
        self.dimmed = false;
    }
}

impl Drop for Backlight {
    fn drop(&mut self) {
        self.restore();
    }
}

pub fn restore_saved_brightness(saved: &Path) -> io::Result<()> {
    let Ok(text) = std::fs::read_to_string(saved) else {
        return Ok(());
    };
    let level: u32 = text.trim().parse().map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: {error}", saved.display()),
        )
    })?;
    write_brightness(level)?;
    std::fs::remove_file(saved)
}

pub fn usb_power_online() -> Option<bool> {
    let supplies = std::fs::read_dir("/sys/class/power_supply").ok()?;
    supplies.flatten().find_map(|supply| {
        let path = supply.path();
        let kind = std::fs::read_to_string(path.join("type")).ok()?;
        if kind.trim() != "USB" {
            return None;
        }
        let online = std::fs::read_to_string(path.join("online")).ok()?;
        Some(online.trim() == "1")
    })
}

pub fn find_devices() -> io::Result<Vec<InputDevice>> {
    Ok(evdev::parse_devices(&std::fs::read_to_string(
        "/proc/bus/input/devices",
    )?))
}

fn open_device(device: &InputDevice) -> io::Result<File> {
    let path = Path::new("/dev/input").join(&device.event_node);
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("could not open {}: {error}", path.display()),
            )
        })
}

fn load_pad_state(file: &File, reader: &mut PadReader) {
    let fd = file.as_raw_fd();
    for axis in evdev::ABS_AXES {
        let mut info = [0i32; ABS_INFO_SIZE / 4];
        if unsafe { libc::ioctl(fd, eviocgabs(axis) as _, info.as_mut_ptr()) } == 0 {
            reader.set_axis(
                axis,
                AbsRange {
                    min: info[1],
                    max: info[2],
                },
                info[0],
            );
        }
    }
    let mut switches = [0u8; SWITCH_BITMAP_SIZE];
    if unsafe { libc::ioctl(fd, EVIOCGSW as _, switches.as_mut_ptr()) } >= 0 {
        let code = evdev::SW_FN as usize;
        reader.set_fn_switch(switches[code / 8] >> (code % 8) & 1 == 1);
    }
    let mut bitmap = [0u8; KEY_BITMAP_SIZE];
    if unsafe { libc::ioctl(fd, EVIOCGKEY as _, bitmap.as_mut_ptr()) } >= 0 {
        reader.set_held_keys(|code| {
            bitmap
                .get(code as usize / 8)
                .is_some_and(|byte| byte >> (code % 8) & 1 == 1)
        });
    }
}

pub fn spawn_pad_reader(device: InputDevice, events: Sender<(u64, Input)>) -> io::Result<()> {
    let file = open_device(&device)?;
    if unsafe { libc::ioctl(file.as_raw_fd(), EVIOCGRAB as _, 1 as libc::c_int) } != 0 {
        return Err(io::Error::other(format!(
            "could not take over {}: {}",
            device.name,
            io::Error::last_os_error()
        )));
    }
    let mut reader = PadReader::default();
    load_pad_state(&file, &mut reader);
    let _ = events.send((0, Input::Pad(reader.pad())));
    std::thread::Builder::new()
        .name("pad".into())
        .spawn(move || pad_loop(file, reader, &events))?;
    Ok(())
}

trait PadSource: Read {
    fn resync(&self, reader: &mut PadReader);
}

impl PadSource for File {
    fn resync(&self, reader: &mut PadReader) {
        load_pad_state(self, reader);
    }
}

fn pad_loop(mut source: impl PadSource, mut reader: PadReader, events: &Sender<(u64, Input)>) {
    let mut buffer = [0u8; evdev::EVENT_SIZE * 64];
    let mut dropping = false;
    loop {
        let read = match source.read(&mut buffer) {
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                eprintln!("lost the built-in controller: {error}");
                let _ = events.send((0, Input::QuitRequested));
                return;
            }
        };
        let mut complete = false;
        for event in Event::decode(&buffer[..read]) {
            if event.kind == evdev::EV_SYN && event.code == evdev::SYN_DROPPED {
                dropping = true;
                continue;
            }
            if dropping {
                if event.kind == evdev::EV_SYN && event.code == evdev::SYN_REPORT {
                    dropping = false;
                    source.resync(&mut reader);
                    complete = true;
                }
                continue;
            }
            complete |= reader.feed(event);
        }
        if complete && events.send((0, Input::Pad(reader.pad()))).is_err() {
            return;
        }
    }
}

pub fn spawn_power_reader(device: InputDevice, events: Sender<(u64, Input)>) -> io::Result<()> {
    let file = open_device(&device)?;
    std::thread::Builder::new()
        .name("power".into())
        .spawn(move || power_loop(file, &events))?;
    Ok(())
}

fn power_loop(mut source: impl Read, events: &Sender<(u64, Input)>) {
    let mut buffer = [0u8; evdev::EVENT_SIZE * 16];
    loop {
        match source.read(&mut buffer) {
            Ok(read) => {
                let pressed = Event::decode(&buffer[..read]).iter().any(|event| {
                    event.kind == evdev::EV_KEY
                        && event.code == evdev::KEY_POWER
                        && event.value == 1
                });
                if pressed && events.send((0, Input::PowerPressed)).is_err() {
                    return;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
}

pub fn spawn_quit_signal_waiter(events: Sender<(u64, Input)>) -> io::Result<()> {
    let mut set: libc::sigset_t = unsafe { std::mem::zeroed() };
    unsafe {
        libc::sigemptyset(&mut set);
        for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            libc::sigaddset(&mut set, signal);
        }
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
    }
    std::thread::Builder::new()
        .name("signals".into())
        .spawn(move || {
            let mut received: libc::c_int = 0;
            if unsafe { libc::sigwait(&set, &mut received) } == 0 {
                eprintln!("received signal {received}");
                let _ = events.send((0, Input::QuitRequested));
            }
        })?;
    Ok(())
}

pub struct Presenter {
    binary: PathBuf,
    child: Option<Child>,
}

impl Presenter {
    pub fn new(binary: PathBuf) -> Self {
        Self {
            binary,
            child: None,
        }
    }

    pub fn show(&mut self, text: &str) {
        self.close();
        let mut command = Command::new(&self.binary);
        command
            .args(["--message", text, "--timeout", "-1"])
            .stdout(std::process::Stdio::null());
        match spawn_with_default_signals(&mut command) {
            Ok(child) => self.child = Some(child),
            Err(error) => eprintln!(
                "could not show {:?} with {}: {error}",
                text,
                self.binary.display()
            ),
        }
    }

    pub fn close(&mut self) {
        if let Some(child) = self.child.take() {
            stop_child(child, PRESENTER_GRACE);
        }
    }
}

pub struct ScreenHelper {
    child: Child,
    input: Option<std::process::ChildStdin>,
    frame: Frame,
}

const HELPER_CLEAR_GRACE: Duration = Duration::from_secs(1);

impl ScreenHelper {
    pub fn start(
        binary: &Path,
        font: &Path,
        assets: Option<&Path>,
        drawing: Option<&str>,
        screen: Screen,
    ) -> io::Result<Self> {
        let mut command = Command::new(binary);
        command.arg("--font").arg(font);
        if let Some(assets) = assets {
            command.arg("--assets").arg(assets);
        }
        if let Some(drawing) = drawing {
            command.arg("--device").arg(drawing);
        }
        command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null());
        let mut child = spawn_with_default_signals(&mut command)?;
        let input = child.stdin.take();
        let mut helper = Self {
            child,
            input,
            frame: Frame {
                screen,
                hold: None,
                busy: None,
                settings: None,
                debug: None,
                sheet: None,
                live: None,
            },
        };
        helper.send()?;
        Ok(helper)
    }

    fn send(&mut self) -> io::Result<()> {
        let input = self.input.as_mut().ok_or_else(|| {
            io::Error::new(io::ErrorKind::BrokenPipe, "the screen helper is closed")
        })?;
        input.write_all(self.frame.serialize().as_bytes())?;
        input.flush()
    }

    pub fn show(&mut self, screen: Screen) -> io::Result<()> {
        self.frame.screen = screen;
        self.send()
    }

    pub fn hold(&mut self, progress: Option<u8>) -> io::Result<()> {
        self.frame.hold = progress;
        self.send()
    }

    pub fn busy(&mut self, busy: bool) -> io::Result<()> {
        self.frame.busy = busy.then_some(0);
        self.send()
    }

    pub fn live(&mut self, live: LiveView) -> io::Result<()> {
        self.frame.live = Some(live);
        self.send()
    }

    pub fn sheet(&mut self, sheet: Option<SheetView>) -> io::Result<()> {
        self.frame.sheet = sheet;
        self.send()
    }

    pub fn settings(&mut self, view: Option<SettingsView>) -> io::Result<()> {
        self.frame.settings = view;
        self.send()
    }

    pub fn debug(&mut self, view: Option<DebugView>) -> io::Result<()> {
        self.frame.debug = view;
        self.send()
    }

    pub fn animate(&mut self) -> io::Result<()> {
        match self.frame.busy {
            Some(phase) => {
                self.frame.busy = Some(phase.wrapping_add(1));
                self.send()
            }
            None => Ok(()),
        }
    }

    pub fn close(mut self) {
        drop(self.input.take());
        let deadline = Instant::now() + HELPER_CLEAR_GRACE;
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(CHILD_POLL);
        }
        stop_child(self.child, PRESENTER_GRACE);
    }
}

const PRESENTER_GRACE: Duration = Duration::from_millis(500);
const CHILD_POLL: Duration = Duration::from_millis(20);

pub fn spawn_with_default_signals(command: &mut Command) -> io::Result<Child> {
    unsafe {
        command.pre_exec(|| {
            let mut all: libc::sigset_t = std::mem::zeroed();
            libc::sigfillset(&mut all);
            libc::pthread_sigmask(libc::SIG_UNBLOCK, &all, std::ptr::null_mut());
            Ok(())
        });
    }
    command.spawn()
}

pub fn stop_child(mut child: Child, grace: Duration) {
    unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) };
    let deadline = Instant::now() + grace;
    while Instant::now() < deadline {
        if let Ok(Some(_)) = child.try_wait() {
            return;
        }
        std::thread::sleep(CHILD_POLL);
    }
    eprintln!("{} did not stop after SIGTERM; killing it", child.id());
    let _ = child.kill();
    let _ = child.wait();
}

impl Drop for Presenter {
    fn drop(&mut self) {
        self.close();
    }
}

pub struct Motor {
    path: PathBuf,
    running: bool,
}

impl Motor {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            running: false,
        }
    }

    pub fn set(&mut self, running: bool) {
        if running == self.running {
            return;
        }
        match std::fs::write(&self.path, if running { "1" } else { "0" }) {
            Ok(()) => self.running = running,
            Err(error) => eprintln!(
                "could not drive the motor at {}: {error}",
                self.path.display()
            ),
        }
    }
}

impl Drop for Motor {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.path, "0");
    }
}

pub struct StateDir {
    path: PathBuf,
}

impl StateDir {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn read(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(self.path.join(name)).ok()
    }

    fn write(&self, name: &str, value: &str) {
        let target = self.path.join(name);
        let result =
            std::fs::create_dir_all(&self.path).and_then(|()| std::fs::write(&target, value));
        if let Err(error) = result {
            eprintln!("could not save {}: {error}", target.display());
        }
    }

    pub fn mode(&self) -> Option<Mode> {
        self.read("mode").and_then(|text| Mode::parse(&text))
    }

    pub fn save_mode(&self, mode: Mode) {
        self.write("mode", mode.as_str());
    }

    pub fn saved(&self) -> Saved {
        let defaults = Settings::default();
        Saved {
            settings: Settings {
                face: self
                    .read("face")
                    .and_then(|text| FaceLayout::parse(&text))
                    .unwrap_or(defaults.face),
                rumble: self
                    .read("rumble")
                    .map_or(defaults.rumble, |text| text.trim() != "off"),
                screen: self
                    .read("screen")
                    .and_then(|text| ScreenWhilePlaying::parse(&text))
                    .unwrap_or(defaults.screen),
            },
            dpad: self
                .read("dpad")
                .and_then(|text| DpadMode::parse(&text))
                .unwrap_or_default(),
        }
    }

    pub fn save_settings(&self, settings: Settings) {
        self.write("face", settings.face.as_str());
        self.write("rumble", if settings.rumble { "on" } else { "off" });
        self.write("screen", settings.screen.as_str());
    }

    pub fn save_dpad_mode(&self, mode: DpadMode) {
        self.write("dpad", mode.as_str());
    }

    pub fn endpoint_budget(&self) -> Option<EndpointBudget> {
        self.read("endpoints")
            .and_then(|text| EndpointBudget::parse(&text))
    }

    pub fn save_endpoint_budget(&self, budget: EndpointBudget) {
        self.write("endpoints", budget.as_str());
    }
}

pub fn serial_number() -> String {
    std::fs::read_to_string("/etc/machine-id")
        .ok()
        .map(|text| {
            text.chars()
                .filter(char::is_ascii_hexdigit)
                .take(12)
                .collect::<String>()
        })
        .filter(|serial| !serial.is_empty())
        .unwrap_or_else(|| "000000000001".to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    fn scratch_dir(label: &str) -> PathBuf {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "pocket-controller-test-{}-{}-{label}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn a_state_dir_that_does_not_exist_yields_defaults() {
        let state = StateDir::new(scratch_dir("missing"));
        assert_eq!(state.mode(), None);
        assert_eq!(state.saved(), Saved::default());
        assert_eq!(state.endpoint_budget(), None);
    }

    #[test]
    fn saving_creates_the_state_dir_and_round_trips_every_field() {
        let path = scratch_dir("round-trip");
        let state = StateDir::new(path.clone());
        let settings = Settings {
            face: FaceLayout::Position,
            rumble: false,
            screen: ScreenWhilePlaying::OffAfter,
        };
        state.save_mode(Mode::Gamepad);
        state.save_settings(settings);
        state.save_dpad_mode(DpadMode::RightStick);
        state.save_endpoint_budget(EndpointBudget::OneInterrupt);

        let reloaded = StateDir::new(path.clone());
        assert_eq!(reloaded.mode(), Some(Mode::Gamepad));
        assert_eq!(
            reloaded.saved(),
            Saved {
                settings,
                dpad: DpadMode::RightStick
            }
        );
        assert_eq!(
            reloaded.endpoint_budget(),
            Some(EndpointBudget::OneInterrupt)
        );
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn unrecognised_saved_values_fall_back_to_defaults_instead_of_failing() {
        let path = scratch_dir("garbage");
        std::fs::create_dir_all(&path).unwrap();
        for name in ["mode", "face", "screen", "dpad", "endpoints"] {
            std::fs::write(path.join(name), "nonsense\n").unwrap();
        }
        let state = StateDir::new(path.clone());
        assert_eq!(state.mode(), None);
        assert_eq!(state.saved(), Saved::default());
        assert_eq!(state.endpoint_budget(), None);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn rumble_is_on_unless_the_file_says_off() {
        let path = scratch_dir("rumble");
        std::fs::create_dir_all(&path).unwrap();
        let state = StateDir::new(path.clone());
        std::fs::write(path.join("rumble"), "off\n").unwrap();
        assert!(!state.saved().settings.rumble);
        std::fs::write(path.join("rumble"), "garbage").unwrap();
        assert!(state.saved().settings.rumble);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn the_motor_is_only_written_when_its_state_changes() {
        let path = scratch_dir("motor");
        std::fs::create_dir_all(&path).unwrap();
        let file = path.join("motor");
        let mut motor = Motor::new(file.clone());
        motor.set(false);
        assert!(!file.exists());
        motor.set(true);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "1");
        std::fs::remove_file(&file).unwrap();
        motor.set(true);
        assert!(!file.exists());
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn dropping_the_motor_switches_it_off() {
        let path = scratch_dir("motor-drop");
        std::fs::create_dir_all(&path).unwrap();
        let file = path.join("motor");
        let mut motor = Motor::new(file.clone());
        motor.set(true);
        drop(motor);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "0");
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn a_motor_that_cannot_be_written_stays_marked_off_so_the_next_call_retries() {
        let path = scratch_dir("motor-retry");
        let file = path.join("missing-dir").join("motor");
        let mut motor = Motor::new(file.clone());
        motor.set(true);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        motor.set(true);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "1");
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn restoring_a_saved_brightness_with_no_saved_file_does_nothing() {
        let path = scratch_dir("no-brightness");
        assert!(restore_saved_brightness(&path.join("absent")).is_ok());
    }

    #[test]
    fn a_corrupt_saved_brightness_is_an_error_naming_the_file_and_is_kept() {
        let path = scratch_dir("bad-brightness");
        std::fs::create_dir_all(&path).unwrap();
        let file = path.join("brightness");
        std::fs::write(&file, "bright").unwrap();
        let error = restore_saved_brightness(&file).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("brightness"));
        assert!(file.exists());
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn an_unrestored_backlight_that_never_dimmed_leaves_no_saved_file() {
        let path = scratch_dir("backlight");
        std::fs::create_dir_all(&path).unwrap();
        let file = path.join("brightness");
        let mut backlight = Backlight::new(file.clone());
        backlight.undim();
        backlight.restore();
        assert!(!file.exists());
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn the_serial_number_is_twelve_hex_digits() {
        let serial = serial_number();
        assert_eq!(serial.len(), 12);
        assert!(serial.chars().all(|c| c.is_ascii_hexdigit()));
    }

    use super::*;

    fn block_quit_signals() {
        unsafe {
            let mut set: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut set);
            libc::sigaddset(&mut set, libc::SIGTERM);
            libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
        }
    }

    #[test]
    fn a_child_spawned_while_sigterm_is_blocked_still_stops_on_sigterm() {
        block_quit_signals();
        let child = spawn_with_default_signals(Command::new("sleep").arg("30")).unwrap();
        let started = Instant::now();
        stop_child(child, Duration::from_secs(5));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn a_child_that_ignores_sigterm_is_killed_after_the_grace_period() {
        let child = Command::new("sh")
            .args(["-c", "trap '' TERM; sleep 30"])
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let started = Instant::now();
        stop_child(child, Duration::from_millis(200));
        let elapsed = started.elapsed();
        assert!(elapsed >= Duration::from_millis(180) && elapsed < Duration::from_secs(2));
    }
}
