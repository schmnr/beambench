//! One worker owns the native USB stream. Stop has a separate atomic signal,
//! so it interrupts arming/query waits without taking the service session lock.
use beambench_serial::{RealSerialTransport, SerialTransport};
use beambench_xtool::{
    D1_SERIAL_BAUD, D1CompiledJob, D1Identity, D1RuntimePhase, D1SerialIo, D1SerialRuntime,
    D1SerialSnapshot,
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

#[derive(Clone)]
pub(crate) struct D1SerialStop {
    requested: Arc<AtomicU64>,
    handled: Arc<AtomicU64>,
    result: Arc<Mutex<Result<(), String>>>,
}

impl D1SerialStop {
    pub fn interrupt(&self) -> u64 {
        self.requested.fetch_add(1, Ordering::AcqRel) + 1
    }
    pub fn stop(&self) -> Result<(), String> {
        let pending = self.requested.load(Ordering::Acquire);
        let target = if pending > self.handled.load(Ordering::Acquire) {
            pending
        } else {
            self.interrupt()
        };
        let since = Instant::now();
        while self.handled.load(Ordering::Acquire) < target {
            if since.elapsed() > Duration::from_secs(11) {
                return Err("D1 USB Stop timed out; use the physical stop".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        self.result.lock().map_err(|e| e.to_string())?.clone()
    }
}

struct SerialIo {
    transport: Box<dyn SerialTransport>,
    stop: D1SerialStop,
}
impl D1SerialIo for SerialIo {
    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        let n = self
            .transport
            .write_bytes(bytes)
            .map_err(|e| e.to_string())?;
        if n != bytes.len() {
            return Err("D1 USB short write; job will not be retried".into());
        }
        Ok(())
    }
    fn read_available(&mut self) -> Result<Vec<u8>, String> {
        self.transport.read_available().map_err(|e| e.to_string())
    }
    fn stop_requested(&self) -> bool {
        self.stop.requested.load(Ordering::Acquire) > self.stop.handled.load(Ordering::Acquire)
    }
}
enum Command {
    Arm(D1CompiledJob),
    Pause,
    Resume,
}
struct Request {
    command: Command,
    stop_generation: u64,
    reply: mpsc::SyncSender<Result<(), String>>,
}

pub(crate) struct D1SerialWorker {
    identity: D1Identity,
    snapshot: Arc<Mutex<D1SerialSnapshot>>,
    commands: mpsc::SyncSender<Request>,
    stop: D1SerialStop,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl D1SerialWorker {
    pub fn connect(path: &str) -> Result<Self, String> {
        // Native Studio opens without a DTR low/high reset pulse. Do not use
        // GRBL's reset-on-connect or any baud/reset fallback here.
        Self::from_transport(Box::new(RealSerialTransport::new_without_dtr(
            path,
            D1_SERIAL_BAUD,
        )))
    }
    pub(crate) fn from_transport(mut transport: Box<dyn SerialTransport>) -> Result<Self, String> {
        transport.open().map_err(|e| e.to_string())?;
        // Studio's bundled Windows binding enables DTR once (hupcl=true).
        // Rust's serialport defaults preserve/disable it on Windows instead.
        // Assert the native level without GRBL's low/high reset pulse.
        #[cfg(windows)]
        transport.set_dtr(true).map_err(|e| e.to_string())?;
        let stop = D1SerialStop {
            requested: Arc::new(AtomicU64::new(0)),
            handled: Arc::new(AtomicU64::new(0)),
            result: Arc::new(Mutex::new(Ok(()))),
        };
        let mut runtime = D1SerialRuntime::new(SerialIo {
            transport,
            stop: stop.clone(),
        });
        let identity = runtime.connect()?;
        let snapshot = Arc::new(Mutex::new(runtime.snapshot()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let (commands, rx) = mpsc::sync_channel::<Request>(1);
        let worker_snapshot = Arc::clone(&snapshot);
        let worker_shutdown = Arc::clone(&shutdown);
        let worker_stop = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut fault: Option<String> = None;
            loop {
                let generation = worker_stop.requested.load(Ordering::Acquire);
                if generation > worker_stop.handled.load(Ordering::Acquire) {
                    let result = runtime.stop();
                    fault = result.as_ref().err().cloned();
                    *worker_stop.result.lock().unwrap() = result;
                    *worker_snapshot.lock().unwrap() = runtime.snapshot();
                    worker_stop.handled.store(generation, Ordering::Release);
                }
                if worker_shutdown.load(Ordering::Acquire) {
                    break;
                }
                if let Ok(request) = rx.try_recv() {
                    let result = if request.stop_generation
                        != worker_stop.requested.load(Ordering::Acquire)
                    {
                        Err("D1 USB command was cancelled by Stop".into())
                    } else if fault.is_some() {
                        Err("D1 USB requires Stop/recovery before another command".into())
                    } else {
                        match request.command {
                            Command::Arm(job) => runtime.arm(&job),
                            Command::Pause => runtime.pause(),
                            Command::Resume => runtime.resume(),
                        }
                    };
                    if result.is_err() && runtime.has_pending_job() {
                        worker_stop.interrupt();
                    }
                    *worker_snapshot.lock().unwrap() = runtime.snapshot();
                    let _ = request.reply.try_send(result);
                }
                if worker_stop.requested.load(Ordering::Acquire)
                    > worker_stop.handled.load(Ordering::Acquire)
                {
                    continue;
                }
                let was_owned = runtime.has_pending_job();
                if fault.is_none()
                    && let Err(error) = runtime.tick()
                {
                    // A failed owned stream is never resumed. Attempt cancel
                    // and output-off, while retaining the failure for the UI.
                    fault = Some(if was_owned {
                        match runtime.stop() {
                            Ok(()) => format!("{error}. Stop confirmed idle."),
                            Err(stop) => {
                                format!("{error}. Stop unconfirmed; use the physical stop. {stop}")
                            }
                        }
                    } else {
                        error
                    });
                }
                let mut state = runtime.snapshot();
                if let Some(error) = &fault {
                    state.phase = D1RuntimePhase::RecoveryRequired;
                    state.error = Some(error.clone());
                }
                *worker_snapshot.lock().unwrap() = state;
                std::thread::sleep(Duration::from_millis(2));
            }
        });
        Ok(Self {
            identity,
            snapshot,
            commands,
            stop,
            shutdown,
            thread: Some(thread),
        })
    }
    pub fn identity(&self) -> &D1Identity {
        &self.identity
    }
    pub fn snapshot(&self) -> D1SerialSnapshot {
        self.snapshot.lock().unwrap().clone()
    }
    pub fn stop_handle(&self) -> D1SerialStop {
        self.stop.clone()
    }
    fn command(&self, command: Command) -> Result<(), String> {
        let (reply, rx) = mpsc::sync_channel(1);
        self.commands
            .try_send(Request {
                command,
                stop_generation: self.stop.requested.load(Ordering::Acquire),
                reply,
            })
            .map_err(|e| e.to_string())?;
        match rx.recv_timeout(Duration::from_secs(7)) {
            Ok(r) => r,
            Err(e) => {
                self.stop.interrupt();
                Err(e.to_string())
            }
        }
    }
    pub fn arm(&self, job: &D1CompiledJob) -> Result<(), String> {
        self.command(Command::Arm(job.clone()))
    }
    pub fn pause(&self) -> Result<(), String> {
        self.command(Command::Pause)
    }
    pub fn resume(&self) -> Result<(), String> {
        self.command(Command::Resume)
    }
    pub fn stop(&self) -> Result<(), String> {
        self.stop.stop()
    }
}
impl Drop for D1SerialWorker {
    fn drop(&mut self) {
        self.stop.interrupt();
        self.shutdown.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
