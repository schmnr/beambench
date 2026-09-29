//! Idle-sleep protection. Acquire and release on one dedicated thread because
//! Windows execution-state assertions belong to the calling thread.
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub(crate) struct SleepGuard {
    stop: Option<mpsc::Sender<()>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl SleepGuard {
    pub(crate) fn acquire() -> Result<Self, String> {
        #[cfg(test)]
        return Ok(Self {
            stop: None,
            worker: None,
        });
        #[cfg(not(test))]
        Self::spawn(native_guard)
    }

    fn spawn<G: 'static>(
        create: impl FnOnce() -> Result<G, String> + Send + 'static,
    ) -> Result<Self, String> {
        let (stop, stopped) = mpsc::channel();
        let (ready, result) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("job-sleep-protection".into())
            .spawn(move || match create() {
                Ok(guard) => {
                    if ready.send(Ok(())).is_ok() {
                        let _ = stopped.recv();
                    }
                    drop(guard);
                }
                Err(error) => {
                    let _ = ready.send(Err(error));
                }
            })
            .map_err(|e| e.to_string())?;
        result
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())??;
        Ok(Self {
            stop: Some(stop),
            worker: Some(worker),
        })
    }
}

impl Drop for SleepGuard {
    fn drop(&mut self) {
        self.stop.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn native_guard() -> Result<keepawake::KeepAwake, String> {
    keepawake::Builder::default()
        .idle(true)
        .reason("Beam Bench laser job")
        .app_name("Beam Bench")
        .app_reverse_domain("com.beambench.app")
        .create()
        .map_err(|e| e.to_string())
}

// Cinnamon/GNOME manage their own idle timer, independently of logind's.
// Keep the session connection alive for the cookie's entire lifetime.
#[cfg(target_os = "linux")]
enum LinuxGuard {
    Desktop(zbus::blocking::Connection, u32),
    Logind { _guard: keepawake::KeepAwake },
}

#[cfg(target_os = "linux")]
impl Drop for LinuxGuard {
    fn drop(&mut self) {
        if let Self::Desktop(connection, cookie) = self {
            let _ = connection.call_method(
                Some("org.gnome.SessionManager"),
                "/org/gnome/SessionManager",
                Some("org.gnome.SessionManager"),
                "Uninhibit",
                &(*cookie,),
            );
        }
    }
}

#[cfg(target_os = "linux")]
fn native_guard() -> Result<LinuxGuard, String> {
    let desktop = (|| -> Result<LinuxGuard, zbus::Error> {
        let connection = zbus::blocking::connection::Builder::session()?
            .method_timeout(Duration::from_secs(2))
            .build()?;
        let cookie: u32 = connection
            .call_method(
                Some("org.gnome.SessionManager"),
                "/org/gnome/SessionManager",
                Some("org.gnome.SessionManager"),
                "Inhibit",
                &("Beam Bench", 0u32, "Laser job in progress", 8u32),
            )?
            .body()
            .deserialize()?;
        Ok(LinuxGuard::Desktop(connection, cookie))
    })();
    if desktop.is_ok() {
        return desktop.map_err(|e| e.to_string());
    }
    let session = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_lowercase();
    if session.contains("gnome") || session.contains("cinnamon") {
        return desktop.map_err(|e| e.to_string());
    }
    keepawake::Builder::default()
        .idle(true)
        .app_name("Beam Bench")
        .reason("Laser job in progress")
        .create()
        .map(|guard| LinuxGuard::Logind { _guard: guard })
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn releases_on_acquiring_thread_even_when_owner_moves() {
        struct Probe(mpsc::Sender<thread::ThreadId>);
        impl Drop for Probe {
            fn drop(&mut self) {
                self.0.send(thread::current().id()).unwrap();
            }
        }
        let (tx, rx) = mpsc::channel();
        let guard = SleepGuard::spawn(move || {
            tx.send(thread::current().id()).unwrap();
            Ok(Probe(tx))
        })
        .unwrap();
        let owner = rx.recv().unwrap();
        assert!(rx.try_recv().is_err());
        thread::spawn(move || drop(guard)).join().unwrap();
        assert_eq!(rx.recv().unwrap(), owner);
    }

    #[test]
    fn acquisition_failure_is_reported() {
        assert!(
            SleepGuard::spawn(|| Err::<(), _>("denied".into()))
                .err()
                .unwrap()
                .contains("denied")
        );
    }

    #[test]
    #[ignore = "requires a native desktop power manager"]
    fn native_idle_sleep_assertion_can_be_acquired_and_released() {
        #[cfg(target_os = "macos")]
        fn assertions() -> usize {
            let output = std::process::Command::new("pmset")
                .args(["-g", "assertions"])
                .output()
                .unwrap();
            assert!(output.status.success());
            String::from_utf8_lossy(&output.stdout)
                .matches("Beam Bench laser job")
                .count()
        }
        #[cfg(target_os = "macos")]
        let before = assertions();
        let guard = SleepGuard::spawn(native_guard).unwrap();
        #[cfg(target_os = "macos")]
        assert_eq!(assertions(), before + 1);
        drop(guard);
        #[cfg(target_os = "macos")]
        assert_eq!(assertions(), before);
    }
}
