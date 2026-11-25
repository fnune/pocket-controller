use std::ffi::CString;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::control::{Input, Kind};
use crate::gadget::{self, GadgetFs, Layout, PortLease};
use crate::hid::{SetupReply, SetupRequest};
use crate::interrupt::Interruptible;
use crate::personality::{self, Personality};
use crate::xinput::{self, State};

const EVENT_SIZE: usize = 12;
const EVENT_BIND: u8 = 0;
const EVENT_UNBIND: u8 = 1;
const EVENT_ENABLE: u8 = 2;
const EVENT_DISABLE: u8 = 3;
const EVENT_SETUP: u8 = 4;
const EVENT_SUSPEND: u8 = 5;
const EVENT_RESUME: u8 = 6;
const STOP_TIMEOUT: Duration = Duration::from_secs(3);
const RETRY_PAUSE: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionEvent {
    Bind,
    Unbind,
    Enable,
    Disable,
    Setup(SetupRequest),
    Suspend,
    Resume,
}

pub fn parse_events(bytes: &[u8]) -> Vec<FunctionEvent> {
    bytes
        .chunks_exact(EVENT_SIZE)
        .filter_map(|raw| {
            let mut setup = [0u8; 8];
            setup.copy_from_slice(&raw[..8]);
            match raw[8] {
                EVENT_BIND => Some(FunctionEvent::Bind),
                EVENT_UNBIND => Some(FunctionEvent::Unbind),
                EVENT_ENABLE => Some(FunctionEvent::Enable),
                EVENT_DISABLE => Some(FunctionEvent::Disable),
                EVENT_SETUP => Some(FunctionEvent::Setup(SetupRequest::parse(&setup))),
                EVENT_SUSPEND => Some(FunctionEvent::Suspend),
                EVENT_RESUME => Some(FunctionEvent::Resume),
                _ => None,
            }
        })
        .collect()
}

struct Outbox {
    report: Vec<u8>,
    generation: u64,
    connected: bool,
    stopping: bool,
}

struct Shared {
    outbox: Mutex<Outbox>,
    wake: Condvar,
    stopping: AtomicBool,
}

impl Shared {
    fn lock(&self) -> std::sync::MutexGuard<'_, Outbox> {
        self.outbox
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

enum BuildError {
    Bind(io::Error),
    Other(io::Error),
}

impl From<io::Error> for BuildError {
    fn from(error: io::Error) -> Self {
        Self::Other(error)
    }
}

pub struct Session {
    personality: &'static dyn Personality,
    budget: xinput::EndpointBudget,
    shared: Arc<Shared>,
    threads: Vec<Interruptible>,
}

#[derive(Clone)]
pub struct SessionEvents {
    pub sender: Sender<(u64, Input)>,
    pub generation: u64,
}

impl SessionEvents {
    fn send(&self, input: Input) {
        let _ = self.sender.send((self.generation, input));
    }
}

fn mount_functionfs(mount_point: &Path) -> io::Result<()> {
    std::fs::create_dir_all(mount_point)?;
    let source = CString::new(gadget::FUNCTIONFS_INSTANCE).map_err(io::Error::other)?;
    let target = CString::new(mount_point.as_os_str().as_bytes()).map_err(io::Error::other)?;
    let kind = CString::new("functionfs").map_err(io::Error::other)?;
    let result = unsafe {
        libc::mount(
            source.as_ptr(),
            target.as_ptr(),
            kind.as_ptr(),
            0,
            std::ptr::null(),
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn open_endpoint(mount_point: &Path, name: &str) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(mount_point.join(name))
        .map_err(|error| io::Error::new(error.kind(), format!("could not open {name}: {error}")))
}

fn write_descriptors(
    ep0: &mut File,
    personality: &dyn Personality,
    budget: xinput::EndpointBudget,
) -> io::Result<()> {
    let mut candidates = personality.descriptors(budget).into_iter().peekable();
    while let Some(descriptors) = candidates.next() {
        match ep0.write_all(&descriptors) {
            Ok(()) => break,
            Err(error) if candidates.peek().is_some() => {
                eprintln!("functionfs refused the descriptors ({error}); trying the next layout")
            }
            Err(error) => return Err(error),
        }
    }
    ep0.write_all(&personality.strings())
}

impl Session {
    pub fn start(
        fs: &dyn GadgetFs,
        layout: &Layout,
        lease: &PortLease,
        kind: Kind,
        serial: &str,
        events: SessionEvents,
        known_budget: Option<xinput::EndpointBudget>,
    ) -> io::Result<Self> {
        let budgets = personality::for_kind(kind).budgets(known_budget);
        let mut last_error = None;
        for budget in budgets {
            let mut session = Self::new(kind, budget);
            match session.build(fs, layout, lease, serial, events.clone(), budget) {
                Ok(()) => return Ok(session),
                Err(BuildError::Bind(error)) => {
                    eprintln!(
                        "the USB controller could not bind the {kind:?} function with {budget:?} endpoints: {error}"
                    );
                    session.stop(fs, layout, lease);
                    last_error = Some(error);
                }
                Err(BuildError::Other(error)) => {
                    session.stop(fs, layout, lease);
                    return Err(error);
                }
            }
        }
        Err(last_error.unwrap_or_else(|| io::Error::other("no endpoint layout to try")))
    }

    fn new(kind: Kind, budget: xinput::EndpointBudget) -> Self {
        let personality = personality::for_kind(kind);
        Self {
            personality,
            budget,
            shared: Arc::new(Shared {
                outbox: Mutex::new(Outbox {
                    report: personality.encode(&State::default()),
                    generation: 1,
                    connected: false,
                    stopping: false,
                }),
                wake: Condvar::new(),
                stopping: AtomicBool::new(false),
            }),
            threads: Vec::new(),
        }
    }

    fn build(
        &mut self,
        fs: &dyn GadgetFs,
        layout: &Layout,
        lease: &PortLease,
        serial: &str,
        events: SessionEvents,
        budget: xinput::EndpointBudget,
    ) -> Result<(), BuildError> {
        lease.attach(fs, layout, &self.personality.identity(), serial)?;
        mount_functionfs(&layout.functionfs_mount).map_err(|error| {
            io::Error::new(error.kind(), format!("could not mount FunctionFS: {error}"))
        })?;
        let mut ep0 = open_endpoint(&layout.functionfs_mount, "ep0")?;
        write_descriptors(&mut ep0, self.personality, budget)?;
        let ep_in = open_endpoint(&layout.functionfs_mount, "ep1")?;
        let ep_out = if self.personality.has_out_endpoint() {
            Some(open_endpoint(&layout.functionfs_mount, "ep2")?)
        } else {
            None
        };
        lease.bind(fs, layout).map_err(BuildError::Bind)?;
        let personality = self.personality;
        let shared = self.shared.clone();
        let control_events = events.clone();
        self.threads
            .push(Interruptible::spawn("ep0", move |stopping| {
                control_loop(ep0, personality, &shared, &control_events, stopping)
            })?);
        let shared = self.shared.clone();
        self.threads
            .push(Interruptible::spawn("ep-in", move |stopping| {
                send_loop(ep_in, &shared, stopping)
            })?);
        if let Some(ep_out) = ep_out {
            self.threads
                .push(Interruptible::spawn("ep-out", move |stopping| {
                    receive_loop(ep_out, personality, &events, stopping)
                })?);
        }
        Ok(())
    }

    pub fn budget(&self) -> xinput::EndpointBudget {
        self.budget
    }

    pub fn submit(&self, state: &State) {
        let report = self.personality.encode(state);
        let mut outbox = self.shared.lock();
        if outbox.report != report {
            outbox.report = report;
            outbox.generation += 1;
            self.shared.wake.notify_all();
        }
    }

    pub fn stop(mut self, fs: &dyn GadgetFs, layout: &Layout, lease: &PortLease) {
        self.shared.stopping.store(true, Ordering::SeqCst);
        {
            let mut outbox = self.shared.lock();
            outbox.stopping = true;
            self.shared.wake.notify_all();
        }
        if let Err(error) = lease.unbind(fs, layout) {
            eprintln!("could not unbind the controller gadget: {error}");
        }
        let deadline = Instant::now() + STOP_TIMEOUT;
        for thread in self.threads.drain(..) {
            if let Err(name) = thread.join_until(deadline) {
                eprintln!("the {name} thread did not stop in time");
            }
        }
        if let Err(error) = lease.detach(fs, layout) {
            eprintln!("could not remove the controller function: {error}");
        }
    }
}

fn set_connected(shared: &Shared, connected: bool) {
    let mut outbox = shared.lock();
    if outbox.connected != connected {
        outbox.connected = connected;
        outbox.generation += 1;
        shared.wake.notify_all();
    }
}

fn answer_setup(
    ep0: &mut (impl Read + Write),
    personality: &dyn Personality,
    setup: &SetupRequest,
    shared: &Shared,
) {
    let report = shared.lock().report.clone();
    let reply = personality.answer_setup(setup, &report);
    let stalled = matches!(reply, SetupReply::Stall);
    let result = match reply {
        SetupReply::Data(bytes) => ep0.write(&bytes).map(drop),
        SetupReply::Acknowledge => ep0.read(&mut vec![0u8; setup.length as usize]).map(drop),
        SetupReply::Stall if setup.is_device_to_host() => ep0.read(&mut []).map(drop),
        SetupReply::Stall => ep0.write(&[]).map(drop),
    };
    if let Err(error) = result
        && error.kind() != io::ErrorKind::Interrupted
        && !stalled
    {
        eprintln!("could not answer setup request {setup:?}: {error}");
    }
}

fn control_loop(
    mut ep0: impl Read + Write,
    personality: &dyn Personality,
    shared: &Shared,
    events: &SessionEvents,
    stopping: &AtomicBool,
) {
    let mut buffer = [0u8; EVENT_SIZE * 4];
    while !stopping.load(Ordering::SeqCst) {
        let read = match ep0.read(&mut buffer) {
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => {
                std::thread::sleep(RETRY_PAUSE);
                continue;
            }
        };
        for event in parse_events(&buffer[..read]) {
            match event {
                FunctionEvent::Enable => {
                    set_connected(shared, true);
                    events.send(Input::HostEnabled);
                }
                FunctionEvent::Disable | FunctionEvent::Unbind => {
                    set_connected(shared, false);
                    events.send(Input::HostDisabled);
                }
                FunctionEvent::Suspend => {
                    set_connected(shared, false);
                    events.send(Input::HostSuspended);
                }
                FunctionEvent::Resume => {
                    set_connected(shared, true);
                    events.send(Input::HostResumed);
                }
                FunctionEvent::Setup(setup) => answer_setup(&mut ep0, personality, &setup, shared),
                FunctionEvent::Bind => {}
            }
        }
    }
}

fn send_loop(mut ep_in: impl Write, shared: &Shared, stopping: &AtomicBool) {
    let mut sent_generation = 0;
    loop {
        let report = {
            let mut outbox = shared.lock();
            let ready = |outbox: &Outbox| {
                outbox.stopping || (outbox.connected && outbox.generation != sent_generation)
            };
            while !ready(&outbox) {
                outbox = shared
                    .wake
                    .wait(outbox)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
            if outbox.stopping {
                return;
            }
            sent_generation = outbox.generation;
            outbox.report.clone()
        };
        if let Err(error) = ep_in.write(&report) {
            if stopping.load(Ordering::SeqCst) {
                return;
            }
            if error.kind() != io::ErrorKind::Interrupted {
                std::thread::sleep(RETRY_PAUSE);
            }
            sent_generation = 0;
        }
    }
}

fn receive_loop(
    mut ep_out: impl Read,
    personality: &dyn Personality,
    events: &SessionEvents,
    stopping: &AtomicBool,
) {
    let mut buffer = [0u8; 64];
    while !stopping.load(Ordering::SeqCst) {
        match ep_out.read(&mut buffer) {
            Ok(read) => {
                for input in personality.read_output(&buffer[..read]) {
                    events.send(input);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => std::thread::sleep(RETRY_PAUSE),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_event(kind: u8, setup: [u8; 8]) -> Vec<u8> {
        let mut raw = setup.to_vec();
        raw.extend_from_slice(&[kind, 0, 0, 0]);
        raw
    }

    #[test]
    fn events_are_parsed_in_order() {
        let mut bytes = raw_event(EVENT_BIND, [0; 8]);
        bytes.extend(raw_event(EVENT_ENABLE, [0; 8]));
        bytes.extend(raw_event(
            EVENT_SETUP,
            [0x81, 0x06, 0x00, 0x22, 0, 0, 0x49, 0],
        ));
        bytes.extend(raw_event(EVENT_SUSPEND, [0; 8]));
        let events = parse_events(&bytes);
        assert_eq!(events.len(), 4);
        assert_eq!(events[0], FunctionEvent::Bind);
        assert_eq!(events[1], FunctionEvent::Enable);
        assert!(
            matches!(events[2], FunctionEvent::Setup(setup) if setup.value == 0x2200 && setup.length == 0x49)
        );
        assert_eq!(events[3], FunctionEvent::Suspend);
    }

    #[test]
    fn unknown_event_types_are_skipped() {
        assert!(parse_events(&raw_event(42, [0; 8])).is_empty());
    }

    #[test]
    fn every_event_type_maps_to_its_variant() {
        let cases = [
            (EVENT_BIND, FunctionEvent::Bind),
            (EVENT_UNBIND, FunctionEvent::Unbind),
            (EVENT_ENABLE, FunctionEvent::Enable),
            (EVENT_DISABLE, FunctionEvent::Disable),
            (EVENT_SUSPEND, FunctionEvent::Suspend),
            (EVENT_RESUME, FunctionEvent::Resume),
        ];
        for (kind, expected) in cases {
            assert_eq!(parse_events(&raw_event(kind, [0; 8])), vec![expected]);
        }
    }

    #[test]
    fn a_truncated_trailing_event_is_dropped_without_losing_the_complete_ones() {
        let mut bytes = raw_event(EVENT_BIND, [0; 8]);
        bytes.extend_from_slice(&raw_event(EVENT_ENABLE, [0; 8])[..EVENT_SIZE - 1]);
        assert_eq!(parse_events(&bytes), vec![FunctionEvent::Bind]);
    }

    #[test]
    fn an_unknown_event_between_known_ones_does_not_shift_the_rest() {
        let mut bytes = raw_event(EVENT_BIND, [0; 8]);
        bytes.extend(raw_event(99, [0; 8]));
        bytes.extend(raw_event(EVENT_RESUME, [0; 8]));
        assert_eq!(
            parse_events(&bytes),
            vec![FunctionEvent::Bind, FunctionEvent::Resume]
        );
    }

    #[test]
    fn empty_input_yields_no_events() {
        assert!(parse_events(&[]).is_empty());
    }

    #[test]
    fn setup_bytes_of_a_non_setup_event_are_ignored() {
        let events = parse_events(&raw_event(EVENT_DISABLE, [0xff; 8]));
        assert_eq!(events, vec![FunctionEvent::Disable]);
    }

    use std::collections::VecDeque;
    use std::sync::mpsc;

    struct ScriptedEndpoint {
        reads: VecDeque<Vec<u8>>,
        stopping: Arc<AtomicBool>,
    }

    impl ScriptedEndpoint {
        fn new(reads: Vec<Vec<u8>>, stopping: &Arc<AtomicBool>) -> Self {
            Self {
                reads: reads.into(),
                stopping: stopping.clone(),
            }
        }
    }

    impl Read for ScriptedEndpoint {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            match self.reads.pop_front() {
                Some(bytes) => {
                    buffer[..bytes.len()].copy_from_slice(&bytes);
                    Ok(bytes.len())
                }
                None => {
                    self.stopping.store(true, Ordering::SeqCst);
                    Ok(0)
                }
            }
        }
    }

    fn shared_with(report: Vec<u8>, connected: bool) -> Arc<Shared> {
        Arc::new(Shared {
            outbox: Mutex::new(Outbox {
                report,
                generation: 1,
                connected,
                stopping: false,
            }),
            wake: Condvar::new(),
            stopping: AtomicBool::new(false),
        })
    }

    fn events() -> (SessionEvents, mpsc::Receiver<(u64, Input)>) {
        let (sender, receiver) = mpsc::channel();
        (
            SessionEvents {
                sender,
                generation: 7,
            },
            receiver,
        )
    }

    fn run_control(raw: Vec<Vec<u8>>, shared: &Shared) -> (Vec<Input>, Vec<Vec<u8>>) {
        let stopping = Arc::new(AtomicBool::new(false));
        let endpoint = ScriptedEndpoint::new(raw, &stopping);
        let (events, receiver) = events();
        let personality = personality::for_kind(Kind::Gamepad);
        let writes = Arc::new(Mutex::new(Vec::new()));
        struct Tee(ScriptedEndpoint, Arc<Mutex<Vec<Vec<u8>>>>);
        impl Read for Tee {
            fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
                self.0.read(buffer)
            }
        }
        impl Write for Tee {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.1.lock().unwrap().push(bytes.to_vec());
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        control_loop(
            Tee(endpoint, writes.clone()),
            personality,
            shared,
            &events,
            &stopping,
        );
        drop(events);
        let inputs = receiver.try_iter().map(|(_, input)| input).collect();
        let written = writes.lock().unwrap().clone();
        (inputs, written)
    }

    #[test]
    fn enabling_the_function_marks_the_host_connected_and_tells_the_controller() {
        let shared = shared_with(vec![0; 7], false);
        let (inputs, _) = run_control(vec![raw_event(EVENT_ENABLE, [0; 8])], &shared);
        assert_eq!(inputs, vec![Input::HostEnabled]);
        assert!(shared.lock().connected);
    }

    #[test]
    fn disable_and_unbind_both_mark_the_host_gone() {
        for kind in [EVENT_DISABLE, EVENT_UNBIND] {
            let shared = shared_with(vec![0; 7], true);
            let (inputs, _) = run_control(vec![raw_event(kind, [0; 8])], &shared);
            assert_eq!(inputs, vec![Input::HostDisabled]);
            assert!(!shared.lock().connected);
        }
    }

    #[test]
    fn suspend_disconnects_and_resume_reconnects() {
        let shared = shared_with(vec![0; 7], true);
        let (inputs, _) = run_control(
            vec![
                raw_event(EVENT_SUSPEND, [0; 8]),
                raw_event(EVENT_RESUME, [0; 8]),
            ],
            &shared,
        );
        assert_eq!(inputs, vec![Input::HostSuspended, Input::HostResumed]);
        assert!(shared.lock().connected);
    }

    #[test]
    fn a_bind_event_changes_nothing() {
        let shared = shared_with(vec![0; 7], false);
        let (inputs, written) = run_control(vec![raw_event(EVENT_BIND, [0; 8])], &shared);
        assert!(inputs.is_empty());
        assert!(written.is_empty());
        assert!(!shared.lock().connected);
    }

    #[test]
    fn repeating_the_same_connection_state_does_not_wake_the_sender_again() {
        let shared = shared_with(vec![0; 7], false);
        run_control(
            vec![
                raw_event(EVENT_ENABLE, [0; 8]),
                raw_event(EVENT_ENABLE, [0; 8]),
            ],
            &shared,
        );
        assert_eq!(shared.lock().generation, 2);
    }

    #[test]
    fn a_get_report_setup_request_is_answered_with_the_current_report() {
        let report = vec![1, 2, 3, 4, 5, 6, 7];
        let shared = shared_with(report.clone(), true);
        let setup = [0xa1, 0x01, 0, 0x01, 0, 0, 7, 0];
        let (_, written) = run_control(vec![raw_event(EVENT_SETUP, setup)], &shared);
        assert_eq!(written, vec![report]);
    }

    #[test]
    fn an_unsupported_device_to_host_setup_request_is_stalled_by_reading_nothing() {
        let shared = shared_with(vec![0; 7], true);
        let setup = [0x80, 0xff, 0, 0, 0, 0, 0, 0];
        let (_, written) = run_control(vec![raw_event(EVENT_SETUP, setup)], &shared);
        assert!(written.is_empty());
    }

    #[test]
    fn an_unsupported_host_to_device_setup_request_is_stalled_by_writing_nothing() {
        let shared = shared_with(vec![0; 7], true);
        let setup = [0x00, 0xff, 0, 0, 0, 0, 0, 0];
        let (_, written) = run_control(vec![raw_event(EVENT_SETUP, setup)], &shared);
        assert_eq!(written, vec![Vec::<u8>::new()]);
    }

    struct ChannelEndpoint {
        sent: mpsc::Sender<Vec<u8>>,
        fail_first: bool,
    }

    impl Write for ChannelEndpoint {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if std::mem::take(&mut self.fail_first) {
                return Err(io::ErrorKind::Interrupted.into());
            }
            let _ = self.sent.send(bytes.to_vec());
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn run_sender(
        shared: &Arc<Shared>,
        fail_first: bool,
    ) -> (mpsc::Receiver<Vec<u8>>, std::thread::JoinHandle<()>) {
        let (sent, received) = mpsc::channel();
        let shared = shared.clone();
        let handle = std::thread::spawn(move || {
            let stopping = AtomicBool::new(false);
            send_loop(ChannelEndpoint { sent, fail_first }, &shared, &stopping);
        });
        (received, handle)
    }

    fn stop_sender(shared: &Shared, handle: std::thread::JoinHandle<()>) {
        shared.lock().stopping = true;
        shared.wake.notify_all();
        handle.join().unwrap();
    }

    const WAIT: Duration = Duration::from_secs(2);

    #[test]
    fn nothing_is_sent_until_the_host_is_connected() {
        let shared = shared_with(vec![9; 7], false);
        let (received, handle) = run_sender(&shared, false);
        assert!(received.recv_timeout(Duration::from_millis(100)).is_err());
        set_connected(&shared, true);
        assert_eq!(received.recv_timeout(WAIT).unwrap(), vec![9; 7]);
        stop_sender(&shared, handle);
    }

    #[test]
    fn a_new_report_is_sent_once_and_an_identical_one_is_not() {
        let shared = shared_with(vec![1; 7], true);
        let (received, handle) = run_sender(&shared, false);
        assert_eq!(received.recv_timeout(WAIT).unwrap(), vec![1; 7]);
        assert!(received.recv_timeout(Duration::from_millis(100)).is_err());
        {
            let mut outbox = shared.lock();
            outbox.report = vec![2; 7];
            outbox.generation += 1;
            shared.wake.notify_all();
        }
        assert_eq!(received.recv_timeout(WAIT).unwrap(), vec![2; 7]);
        stop_sender(&shared, handle);
    }

    #[test]
    fn an_interrupted_write_is_retried_with_the_same_report() {
        let shared = shared_with(vec![4; 7], true);
        let (received, handle) = run_sender(&shared, true);
        assert_eq!(received.recv_timeout(WAIT).unwrap(), vec![4; 7]);
        stop_sender(&shared, handle);
    }

    #[test]
    fn stopping_ends_a_sender_that_has_nothing_to_send() {
        let shared = shared_with(vec![0; 7], false);
        let (_received, handle) = run_sender(&shared, false);
        stop_sender(&shared, handle);
    }

    #[test]
    fn host_output_reports_become_controller_inputs_tagged_with_the_session_generation() {
        let stopping = Arc::new(AtomicBool::new(false));
        let rumble = vec![0x00, 0x08, 0x00, 0xff, 0x00, 0x00, 0x00, 0x00];
        let endpoint = ScriptedEndpoint::new(vec![rumble], &stopping);
        let (events, receiver) = events();
        receive_loop(
            endpoint,
            personality::for_kind(Kind::Xbox),
            &events,
            &stopping,
        );
        let received: Vec<_> = receiver.try_iter().collect();
        assert_eq!(received.len(), 2);
        assert!(received.iter().all(|(generation, _)| *generation == 7));
        assert_eq!(received[0].1, Input::XboxDriverSpoke);
        assert!(matches!(received[1].1, Input::Rumble(_)));
    }

    #[test]
    fn output_the_personality_does_not_understand_is_dropped() {
        let stopping = Arc::new(AtomicBool::new(false));
        let endpoint = ScriptedEndpoint::new(vec![vec![0x00, 0x08, 0x00, 0xff]], &stopping);
        let (events, receiver) = events();
        receive_loop(
            endpoint,
            personality::for_kind(Kind::Gamepad),
            &events,
            &stopping,
        );
        assert_eq!(receiver.try_iter().count(), 0);
    }
}
