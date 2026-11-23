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
    ep0: &mut File,
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
    mut ep0: File,
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

fn send_loop(mut ep_in: File, shared: &Shared, stopping: &AtomicBool) {
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
    mut ep_out: File,
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
}
