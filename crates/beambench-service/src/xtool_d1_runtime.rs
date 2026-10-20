//! Native D1 Wi-Fi transport. Status is read continuously, including during
//! HTTP uploads, so a lost socket or event queue cannot masquerade as completion.
use beambench_xtool::{
    D1_MAX_JOB_BYTES, D1_STATUS_PORT, D1NetworkIo, XToolHttpIo, XToolHttpResponse,
};
use reqwest::{blocking::Client, redirect::Policy};
use std::io::Read;
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tungstenite::{Message, protocol::WebSocketConfig};

pub struct ReqwestD1NetworkIo {
    client: Client,
    base_url: String,
    events: mpsc::Receiver<String>,
    initial_events: Vec<String>,
    barriers: mpsc::SyncSender<mpsc::SyncSender<()>>,
    failure: Arc<Mutex<Option<String>>>,
    shutdown: Arc<AtomicBool>,
    socket: TcpStream,
    worker: Option<JoinHandle<()>>,
}

fn authority(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("[{}]:{port}", host.trim_matches(['[', ']']))
    } else {
        format!("{host}:{port}")
    }
}

impl ReqwestD1NetworkIo {
    pub fn new(host: &str, port: u16) -> Result<Self, String> {
        Self::with_status_port(host, port, D1_STATUS_PORT)
    }

    pub(crate) fn with_status_port(
        host: &str,
        port: u16,
        status_port: u16,
    ) -> Result<Self, String> {
        let host = host.trim();
        if host.is_empty() || host.contains(['/', '?', '#', '@']) {
            return Err("xTool D1 requires a host name or IP address".into());
        }
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(3))
            .redirect(Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .build()
            .map_err(|e| e.to_string())?;
        let address = authority(host, status_port)
            .to_socket_addrs()
            .map_err(|e| e.to_string())?
            .next()
            .ok_or("xTool D1 status address did not resolve")?;
        let stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))
            .map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .map_err(|e| e.to_string())?;
        let config = WebSocketConfig::default()
            .read_buffer_size(4096)
            .write_buffer_size(0)
            .max_write_buffer_size(8192)
            .max_message_size(Some(4096))
            .max_frame_size(Some(4096));
        let (mut ws, _) = tungstenite::client::client_with_config(
            format!("ws://{}/", authority(host, status_port)),
            stream,
            Some(config),
        )
        .map_err(|e| e.to_string())?;
        ws.get_mut()
            .set_nonblocking(true)
            .map_err(|e| e.to_string())?;
        let socket = ws.get_ref().try_clone().map_err(|e| e.to_string())?;
        let (tx, events) = mpsc::sync_channel(64);
        let (barriers, barrier_rx) = mpsc::sync_channel::<mpsc::SyncSender<()>>(1);
        let failure = Arc::new(Mutex::new(None));
        let worker_failure = Arc::clone(&failure);
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutdown);
        let worker = std::thread::spawn(move || {
            let result = (|| -> Result<(), String> {
                let mut last_pong = Instant::now();
                let mut last_ping = Instant::now() - Duration::from_secs(2);
                let mut pending_barrier = None;
                while !worker_shutdown.load(Ordering::Acquire) {
                    if pending_barrier.is_none()
                        && let Ok(reply) = barrier_rx.try_recv()
                    {
                        ws.send(Message::Ping(b"barrier".to_vec().into()))
                            .map_err(|e| e.to_string())?;
                        pending_barrier = Some(reply);
                    }
                    if last_ping.elapsed() >= Duration::from_secs(2) {
                        ws.send(Message::Ping(b"heartbeat".to_vec().into()))
                            .map_err(|e| e.to_string())?;
                        last_ping = Instant::now();
                    }
                    if last_pong.elapsed() > Duration::from_secs(7) {
                        return Err("xTool D1 status heartbeat timed out".into());
                    }
                    match ws.read() {
                        Ok(Message::Text(text)) => tx.try_send(text.to_string()).map_err(|_| {
                            "xTool D1 status event queue overflowed or closed".to_string()
                        })?,
                        Ok(Message::Pong(payload)) => {
                            if payload.as_ref() == b"heartbeat" || payload.as_ref() == b"barrier" {
                                last_pong = Instant::now();
                            }
                            if payload.as_ref() == b"barrier"
                                && let Some(reply) = pending_barrier.take()
                            {
                                let _ = reply.try_send(());
                            }
                        }
                        Ok(Message::Ping(_)) => ws.flush().map_err(|e| e.to_string())?,
                        Ok(Message::Close(_)) => {
                            return Err("xTool D1 status connection closed".into());
                        }
                        Ok(_) => return Err("xTool D1 sent an unsupported status frame".into()),
                        Err(tungstenite::Error::Io(e))
                            if e.kind() == std::io::ErrorKind::WouldBlock =>
                        {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(e) => return Err(format!("xTool D1 status connection failed: {e}")),
                    }
                }
                Ok(())
            })();
            if let Err(error) = result
                && let Ok(mut failure) = worker_failure.lock()
            {
                *failure = Some(error);
            }
        });
        let mut io = Self {
            client,
            base_url: format!("http://{}", authority(host, port)),
            events,
            initial_events: Vec::new(),
            barriers,
            failure,
            shutdown,
            socket,
            worker: Some(worker),
        };
        // Confirm that the socket is actively reading before enabling uploads.
        io.initial_events = io.synchronize_status()?;
        Ok(io)
    }

    fn response(
        response: reqwest::blocking::Response,
        limit: usize,
    ) -> Result<XToolHttpResponse, String> {
        let status = response.status().as_u16();
        let mut body = Vec::new();
        response
            .take(limit as u64 + 1)
            .read_to_end(&mut body)
            .map_err(|e| e.to_string())?;
        if body.len() > limit {
            return Err("xTool D1 HTTP response exceeded its size limit".into());
        }
        Ok(XToolHttpResponse { status, body })
    }
}

impl Drop for ReqwestD1NetworkIo {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        let _ = self.socket.shutdown(Shutdown::Both);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl XToolHttpIo for ReqwestD1NetworkIo {
    fn get(&mut self, path: &str) -> Result<XToolHttpResponse, String> {
        let limit = if path == "/tmp.gcode" {
            D1_MAX_JOB_BYTES
        } else {
            65536
        };
        let request = self.client.get(format!("{}{path}", self.base_url));
        let request = if path == "/tmp.gcode" {
            request.timeout(Duration::from_secs(60))
        } else {
            request
        };
        Self::response(request.send().map_err(|e| e.to_string())?, limit)
    }

    fn post(
        &mut self,
        path: &str,
        body: &[u8],
        content_type: &str,
    ) -> Result<XToolHttpResponse, String> {
        Self::response(
            self.client
                .post(format!("{}{path}", self.base_url))
                .timeout(Duration::from_secs(60))
                .header(reqwest::header::CONTENT_TYPE, content_type)
                .body(body.to_vec())
                .send()
                .map_err(|e| e.to_string())?,
            65536,
        )
    }
}
impl D1NetworkIo for ReqwestD1NetworkIo {
    fn poll_status_events(&mut self) -> Result<Vec<String>, String> {
        if let Some(error) = self.failure.lock().map_err(|e| e.to_string())?.clone() {
            return Err(error);
        }
        let mut events = std::mem::take(&mut self.initial_events);
        loop {
            match self.events.try_recv() {
                Ok(event) => {
                    events.push(event);
                    if events.len() > 64 {
                        return Err("xTool D1 status queue overflow".into());
                    }
                }
                Err(mpsc::TryRecvError::Empty) => return Ok(events),
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("xTool D1 status connection is unavailable".into());
                }
            }
        }
    }
    fn synchronize_status(&mut self) -> Result<Vec<String>, String> {
        let (reply, ready) = mpsc::sync_channel(1);
        self.barriers.try_send(reply).map_err(|e| e.to_string())?;
        ready
            .recv_timeout(Duration::from_secs(3))
            .map_err(|_| "xTool D1 status synchronization timed out".to_string())?;
        self.poll_status_events()
    }
}

/// Delivers stop independently of a session lock or a slow upload. The
/// session subsequently confirms idle and removes only its own pending file.
pub fn send_independent_stop(host: &str, port: u16) -> Result<(), String> {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(3))
        .redirect(Policy::none())
        .retry(reqwest::retry::never())
        .no_proxy()
        .build()
        .map_err(|e| e.to_string())?;
    let base = format!("http://{}", authority(host, port));
    // Even a rejected stop must not prevent the explicit output-off attempt.
    let stop = client.get(format!("{base}/cnc/data?action=stop")).send();
    let off = client
        .get(format!("{base}/cmd?cmd=M9+S0+N0"))
        .send()
        .map_err(|e| e.to_string())?;
    beambench_xtool::require_d1_ack(
        off.status().as_u16(),
        &ReqwestD1NetworkIo::response(off, 65536)?.body,
    )
    .map_err(|e| e.to_string())?;
    stop.map(|_| ()).map_err(|e| e.to_string())
}

use beambench_common::{
    ControllerDriverId, ControllerEvidenceState, ControllerFamily, ControllerModel,
    ControllerProductTier, DeviceCapabilities, ExplicitControllerSelection, JobProgress, JobState,
    MachineRunState, MachineStatus, PositiveControllerIdentity, SessionState,
};
use beambench_xtool::{D1CompiledJob, D1Model, D1Runtime, D1RuntimePhase};
use std::collections::HashMap;

pub struct XToolD1RuntimeSession {
    runtime: D1Runtime<ReqwestD1NetworkIo>,
    host: String,
    port: u16,
    machine_status: MachineStatus,
}

impl XToolD1RuntimeSession {
    pub fn connect(host: &str, port: u16) -> Result<Self, String> {
        Self::from_io(host, port, ReqwestD1NetworkIo::new(host, port)?)
    }

    pub(crate) fn from_io(host: &str, port: u16, io: ReqwestD1NetworkIo) -> Result<Self, String> {
        let mut runtime = D1Runtime::new(io);
        runtime.connect().map_err(|error| error.to_string())?;
        let mut session = Self {
            runtime,
            host: host.trim().to_string(),
            port,
            machine_status: MachineStatus::default(),
        };
        session.refresh_machine_status();
        Ok(session)
    }

    pub const fn driver(&self) -> ControllerDriverId {
        ControllerDriverId::XToolD1
    }

    /// Host and port, for stops sent outside the session.
    pub fn endpoint(&self) -> (String, u16) {
        (self.host.clone(), self.port)
    }

    pub fn controller_model(&self) -> ControllerModel {
        match self.runtime.identity().map(|identity| identity.model) {
            Some(D1Model::D1Pro) => ControllerModel::XToolD1Pro,
            _ => ControllerModel::XToolD1,
        }
    }

    pub const fn product_tier(&self) -> ControllerProductTier {
        ControllerProductTier::Experimental
    }

    pub const fn evidence_state(&self) -> ControllerEvidenceState {
        ControllerEvidenceState::Emulated
    }

    pub fn capabilities(&self) -> DeviceCapabilities {
        DeviceCapabilities {
            can_pause_resume: true,
            can_run_job: true,
            can_frame: true,
            ..DeviceCapabilities::disabled()
        }
    }

    pub fn selection(&self) -> ExplicitControllerSelection {
        ExplicitControllerSelection::KnownDriver {
            driver: ControllerDriverId::XToolD1,
        }
    }

    pub fn detected_identity(&self) -> PositiveControllerIdentity {
        let identity = self.runtime.identity();
        PositiveControllerIdentity {
            family: ControllerFamily::Gcode,
            model: self.controller_model(),
            firmware_identity: identity
                .map(|identity| format!("{:?} {}W", identity.model, identity.power_watts)),
            firmware_version: identity.and_then(|identity| identity.firmware_version.clone()),
            evidence: vec![
                "xTool D1 / D1 Pro identity confirmed through its HTTP controller API".to_string(),
            ],
        }
    }

    pub fn session_state(&self) -> SessionState {
        match self.runtime.phase() {
            D1RuntimePhase::Disconnected => SessionState::Disconnected,
            D1RuntimePhase::Ready | D1RuntimePhase::ReadyToRun | D1RuntimePhase::Completed => {
                SessionState::Ready
            }
            D1RuntimePhase::Running
            | D1RuntimePhase::BusyUnowned
            | D1RuntimePhase::PauseRequested
            | D1RuntimePhase::ResumeRequested => SessionState::Running,
            D1RuntimePhase::Paused => SessionState::Paused,
            D1RuntimePhase::Stopped => SessionState::Ready,
            D1RuntimePhase::RecoveryRequired => SessionState::Error,
        }
    }

    pub fn controller_info(&self) -> HashMap<String, String> {
        let mut info = HashMap::new();
        info.insert("transport".to_string(), "xTool D1 HTTP".to_string());
        info.insert("host".to_string(), self.host.clone());
        info.insert("port".to_string(), self.port.to_string());
        if let Some(identity) = self.runtime.identity() {
            info.insert("machine_type".into(), format!("{:?}", identity.model));
            info.insert("laser_power_watts".into(), identity.power_watts.to_string());
            if let Some(version) = &identity.firmware_version {
                info.insert("firmware_version".into(), version.clone());
            }
        }
        info
    }

    pub fn endpoint_name(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn machine_status(&self) -> &MachineStatus {
        &self.machine_status
    }

    pub fn upload_job(&mut self, job: &D1CompiledJob) -> Result<(), String> {
        let result = self
            .runtime
            .upload(job, &format!("BeamBench-{}", uuid::Uuid::new_v4()))
            .map_err(|error| error.to_string());
        let result = if let Err(error) = result {
            if self.runtime.has_pending_file() {
                let stopped = match self.runtime.stop() {
                    Ok(()) => "Stop confirmed idle.".to_string(),
                    Err(stop) => {
                        format!("Stop could not be confirmed. Use the physical stop now. {stop}")
                    }
                };
                Err(format!("{error}. {stopped}"))
            } else {
                Err(error)
            }
        } else {
            result
        };
        self.refresh_machine_status();
        result
    }

    pub fn poll_job(&mut self) -> Result<D1RuntimePhase, String> {
        let phase = self.runtime.poll().map_err(|error| error.to_string());
        self.refresh_machine_status();
        phase
    }

    pub fn pause_job(&mut self) -> Result<(), String> {
        self.runtime.pause().map_err(|error| error.to_string())?;
        self.refresh_machine_status();
        Ok(())
    }

    pub fn resume_job(&mut self) -> Result<(), String> {
        self.runtime.resume().map_err(|error| error.to_string())?;
        self.refresh_machine_status();
        Ok(())
    }

    pub fn cancel_job(&mut self) -> Result<(), String> {
        self.runtime.stop().map_err(|error| error.to_string())?;
        self.refresh_machine_status();
        Ok(())
    }

    pub fn emergency_stop(&mut self) -> Result<(), String> {
        // The physical button can start work after our last idle poll. Always
        // send stop/output-off and confirm fresh idle, even from cached Ready.
        self.cancel_job()
    }

    pub fn disconnect(&mut self) -> Result<(), String> {
        let result = self.runtime.stop().map_err(|e| e.to_string());
        self.runtime.disconnect();
        self.refresh_machine_status();
        result
    }

    pub fn has_pending_file(&self) -> bool {
        self.runtime.has_pending_file()
    }

    pub fn stock_workspace_mm(&self) -> (f64, f64) {
        self.runtime
            .identity()
            .expect("connected D1 identity")
            .stock_workspace_mm()
    }

    fn refresh_machine_status(&mut self) {
        self.machine_status.run_state = match self.runtime.phase() {
            D1RuntimePhase::Running
            | D1RuntimePhase::BusyUnowned
            | D1RuntimePhase::PauseRequested
            | D1RuntimePhase::ResumeRequested => MachineRunState::Run,
            D1RuntimePhase::Paused => MachineRunState::Hold,
            D1RuntimePhase::RecoveryRequired => MachineRunState::Alarm,
            D1RuntimePhase::Disconnected
            | D1RuntimePhase::Ready
            | D1RuntimePhase::ReadyToRun
            | D1RuntimePhase::Completed
            | D1RuntimePhase::Stopped => MachineRunState::Idle,
        };
    }
}

pub struct XToolD1RuntimeJob {
    total_lines: usize,
    started_at: Instant,
    state: JobState,
    error_message: Option<String>,
    planned_duration_secs: Option<f64>,
}

impl XToolD1RuntimeJob {
    pub fn start(
        job: &D1CompiledJob,
        session: &mut XToolD1RuntimeSession,
        planned_duration_secs: Option<f64>,
    ) -> Result<Self, String> {
        session.upload_job(job)?;
        Ok(Self {
            total_lines: job.line_count,
            started_at: Instant::now(),
            state: JobState::ReadyToRun,
            error_message: None,
            planned_duration_secs: planned_duration_secs.filter(|value| *value > 0.0),
        })
    }

    pub fn progress(&self) -> JobProgress {
        let elapsed_secs = self.started_at.elapsed().as_secs_f64();
        let completed = self.state == JobState::Completed;
        let estimated_remaining_secs = if matches!(self.state, JobState::ReadyToRun) {
            self.planned_duration_secs.unwrap_or_default()
        } else if self.state == JobState::Running {
            self.planned_duration_secs
                .map(|duration| (duration - elapsed_secs).max(0.0))
                .unwrap_or_default()
        } else {
            0.0
        };
        JobProgress {
            state: self.state,
            total_lines: self.total_lines,
            queued_lines: self.total_lines,
            sent_lines: self.total_lines,
            acknowledged_lines: if completed { self.total_lines } else { 0 },
            elapsed_secs,
            estimated_remaining_secs,
            buffer_fill_bytes: 0,
            error_message: self.error_message.clone(),
            buckets: Vec::new(),
        }
    }

    pub fn tick(&mut self, session: &mut XToolD1RuntimeSession) -> JobProgress {
        if matches!(
            self.state,
            JobState::Completed | JobState::Failed | JobState::Cancelled
        ) {
            return self.progress();
        }
        match session.poll_job() {
            Ok(D1RuntimePhase::ReadyToRun) => self.state = JobState::ReadyToRun,
            Ok(D1RuntimePhase::Running) => self.state = JobState::Running,
            Ok(D1RuntimePhase::Paused) => self.state = JobState::Paused,
            Ok(D1RuntimePhase::Completed) => self.state = JobState::Completed,
            Ok(D1RuntimePhase::Stopped) => self.state = JobState::Cancelled,
            Ok(D1RuntimePhase::RecoveryRequired | D1RuntimePhase::BusyUnowned) => {
                self.state = JobState::Failed;
                self.error_message = Some("xTool D1 entered a recovery-required state".to_string());
            }
            Ok(D1RuntimePhase::Disconnected | D1RuntimePhase::Ready) => {}
            Ok(D1RuntimePhase::PauseRequested | D1RuntimePhase::ResumeRequested) => {}
            Err(error) => {
                self.state = JobState::Failed;
                self.error_message = Some(format!(
                    "D1 status was lost; use Stop to confirm the machine is idle. {error}"
                ));
            }
        }
        if self.state == JobState::Failed {
            let stop = session.cancel_job();
            let detail = match stop {
                Ok(()) => " Stop confirmed idle.".to_string(),
                Err(error) => {
                    format!(" Stop could not be confirmed. Use the physical stop now. {error}")
                }
            };
            self.error_message
                .get_or_insert_with(String::new)
                .push_str(&detail);
        }
        self.progress()
    }

    pub fn pause(&mut self, session: &mut XToolD1RuntimeSession) -> Result<JobProgress, String> {
        session.pause_job()?;
        // Remain running until PAUSING confirms the change.
        Ok(self.progress())
    }

    pub fn resume(&mut self, session: &mut XToolD1RuntimeSession) -> Result<JobProgress, String> {
        session.resume_job()?;
        // Remain paused until the ordered running event confirms resume.
        Ok(self.progress())
    }

    pub fn cancel(&mut self, session: &mut XToolD1RuntimeSession) -> Result<JobProgress, String> {
        session.cancel_job()?;
        self.state = JobState::Cancelled;
        Ok(self.progress())
    }
}
