use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const WAKE_SIGNAL: libc::c_int = libc::SIGUSR1;
const KICK_INTERVAL: Duration = Duration::from_millis(50);

extern "C" fn ignore_signal(_: libc::c_int) {}

pub fn install_wake_handler() -> io::Result<()> {
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    action.sa_sigaction = ignore_signal as extern "C" fn(libc::c_int) as usize;
    action.sa_flags = 0;
    unsafe { libc::sigemptyset(&mut action.sa_mask) };
    if unsafe { libc::sigaction(WAKE_SIGNAL, &action, std::ptr::null_mut()) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

pub struct Interruptible {
    name: &'static str,
    stopping: Arc<AtomicBool>,
    thread_id: Arc<AtomicI32>,
    handle: JoinHandle<()>,
}

fn current_thread_id() -> i32 {
    unsafe { libc::syscall(libc::SYS_gettid) as i32 }
}

impl Interruptible {
    pub fn spawn<F>(name: &'static str, body: F) -> io::Result<Self>
    where
        F: FnOnce(&AtomicBool) + Send + 'static,
    {
        let stopping = Arc::new(AtomicBool::new(false));
        let thread_id = Arc::new(AtomicI32::new(0));
        let handle = {
            let stopping = stopping.clone();
            let thread_id = thread_id.clone();
            std::thread::Builder::new()
                .name(name.to_string())
                .spawn(move || {
                    thread_id.store(current_thread_id(), Ordering::SeqCst);
                    body(&stopping);
                    thread_id.store(0, Ordering::SeqCst);
                })?
        };
        Ok(Self {
            name,
            stopping,
            thread_id,
            handle,
        })
    }

    fn kick(&self) {
        let thread_id = self.thread_id.load(Ordering::SeqCst);
        if thread_id != 0 {
            unsafe { libc::syscall(libc::SYS_tgkill, libc::getpid(), thread_id, WAKE_SIGNAL) };
        }
    }

    pub fn join_until(self, deadline: Instant) -> Result<(), &'static str> {
        self.stopping.store(true, Ordering::SeqCst);
        while !self.handle.is_finished() {
            if Instant::now() >= deadline {
                return Err(self.name);
            }
            self.kick();
            std::thread::sleep(KICK_INTERVAL);
        }
        self.handle.join().map_err(|_| self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::os::unix::net::UnixStream;

    #[test]
    fn a_thread_blocked_in_read_is_woken_and_joined() {
        install_wake_handler().unwrap();
        let (mut reader, _writer) = UnixStream::pair().unwrap();
        let thread = Interruptible::spawn("reader", move |stopping| {
            let mut buffer = [0u8; 1];
            while !stopping.load(Ordering::SeqCst) {
                let _ = reader.read(&mut buffer);
            }
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(50));
        assert!(
            thread
                .join_until(Instant::now() + Duration::from_secs(2))
                .is_ok()
        );
    }

    #[test]
    fn a_body_that_never_returns_is_reported_by_name_at_the_deadline() {
        let release = Arc::new(AtomicBool::new(false));
        let hold = release.clone();
        let thread = Interruptible::spawn("stuck", move |_| {
            while !hold.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(5));
            }
        })
        .unwrap();
        let result = thread.join_until(Instant::now() + Duration::from_millis(150));
        release.store(true, Ordering::SeqCst);
        assert_eq!(result, Err("stuck"));
    }

    #[test]
    fn a_body_that_panics_is_reported_by_name() {
        let thread = Interruptible::spawn("crasher", |_| panic!("boom")).unwrap();
        let result = thread.join_until(Instant::now() + Duration::from_secs(2));
        assert_eq!(result, Err("crasher"));
    }

    #[test]
    fn the_stop_flag_is_set_before_the_body_is_waited_on() {
        let thread = Interruptible::spawn("polite", |stopping| {
            while !stopping.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(5));
            }
        })
        .unwrap();
        assert!(
            thread
                .join_until(Instant::now() + Duration::from_secs(2))
                .is_ok()
        );
    }

    #[test]
    fn the_thread_carries_the_name_it_was_spawned_with() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let thread = Interruptible::spawn("named", move |_| {
            let _ = sender.send(std::thread::current().name().map(str::to_string));
        })
        .unwrap();
        assert_eq!(receiver.recv().unwrap().as_deref(), Some("named"));
        assert!(
            thread
                .join_until(Instant::now() + Duration::from_secs(2))
                .is_ok()
        );
    }
}
