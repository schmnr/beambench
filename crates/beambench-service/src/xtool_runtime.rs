use std::collections::HashMap;
use std::time::{Duration, Instant};

use beambench_common::controller_choice::{
    ControllerDriverId, ExplicitControllerSelection, PositiveControllerIdentity,
};
use beambench_common::machine::{
    ControllerEvidenceState, ControllerFamily, ControllerModel, ControllerProductTier,
    DeviceCapabilities, JobProgress, JobState, MachineRunState, MachineStatus, SessionState,
};
use beambench_xtool::{
    M1CompiledJob, M1HttpIo, M1HttpResponse, M1IdentityConfidence, M1Runtime, M1RuntimePhase,
};
use reqwest::blocking::Client;
use reqwest::redirect::Policy;

const RESPONSE_LIMIT_BYTES: usize = 1024 * 1024;

pub struct ReqwestM1HttpIo {
    client: Client,
    base_url: String,
}

impl ReqwestM1HttpIo {
    pub fn new(host: &str, port: u16) -> Result<Self, String> {
        let host = host.trim();
        if host.is_empty() {
            return Err("xTool M1 host cannot be empty".to_string());
        }
        let authority = if host.contains(':') && !(host.starts_with('[') && host.ends_with(']')) {
            format!("[{host}]:{port}")
        } else {
            format!("{host}:{port}")
        };
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(10))
            .redirect(Policy::none())
            .build()
            .map_err(|error| format!("could not create xTool M1 HTTP client: {error}"))?;
        Ok(Self {
            client,
            base_url: format!("http://{authority}"),
        })
    }

    fn response(response: reqwest::blocking::Response) -> Result<M1HttpResponse, String> {
        let status = response.status().as_u16();
        if response
            .content_length()
            .is_some_and(|length| length > RESPONSE_LIMIT_BYTES as u64)
        {
            return Err("xTool M1 response exceeded the 1 MiB safety limit".to_string());
        }
        let body = response
            .bytes()
            .map_err(|error| format!("could not read xTool M1 response: {error}"))?;
        if body.len() > RESPONSE_LIMIT_BYTES {
            return Err("xTool M1 response exceeded the 1 MiB safety limit".to_string());
        }
        Ok(M1HttpResponse {
            status,
            body: body.to_vec(),
        })
    }
}

impl M1HttpIo for ReqwestM1HttpIo {
    fn get(&mut self, path_and_query: &str) -> Result<M1HttpResponse, String> {
        let response = self
            .client
            .get(format!("{}{path_and_query}", self.base_url))
            .send()
            .map_err(|error| error.to_string())?;
        Self::response(response)
    }

    fn post(
        &mut self,
        path_and_query: &str,
        body: &[u8],
        content_type: &str,
    ) -> Result<M1HttpResponse, String> {
        let request = self
            .client
            .post(format!("{}{path_and_query}", self.base_url))
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(body.to_vec());
        let request = if path_and_query.contains("action=upload") {
            request.timeout(Duration::from_secs(60))
        } else {
            request
        };
        let response = request.send().map_err(|error| error.to_string())?;
        Self::response(response)
    }
}

pub struct XToolM1RuntimeSession {
    runtime: M1Runtime<ReqwestM1HttpIo>,
    host: String,
    port: u16,
    machine_status: MachineStatus,
}

impl XToolM1RuntimeSession {
    pub fn connect(host: &str, port: u16) -> Result<Self, String> {
        let io = ReqwestM1HttpIo::new(host, port)?;
        let mut runtime = M1Runtime::new(io);
        let snapshot = runtime.connect().map_err(|error| error.to_string())?;
        let identity = snapshot
            .identity
            .as_ref()
            .ok_or_else(|| "xTool M1 did not return device identity".to_string())?;
        if identity.confidence != M1IdentityConfidence::Confirmed {
            return Err(
                "The endpoint answered the xTool HTTP protocol, but did not identify as an original xTool M1. No controls were enabled."
                    .to_string(),
            );
        }
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
        ControllerDriverId::XToolM1
    }

    pub const fn controller_model(&self) -> ControllerModel {
        ControllerModel::XToolM1
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
            ..DeviceCapabilities::disabled()
        }
    }

    pub fn selection(&self) -> ExplicitControllerSelection {
        ExplicitControllerSelection::KnownDriver {
            driver: ControllerDriverId::XToolM1,
        }
    }

    pub fn detected_identity(&self) -> PositiveControllerIdentity {
        let identity = self.runtime.identity();
        PositiveControllerIdentity {
            family: ControllerFamily::Gcode,
            model: ControllerModel::XToolM1,
            firmware_identity: identity.and_then(|identity| {
                identity
                    .machine_type
                    .clone()
                    .or_else(|| identity.device_name.clone())
            }),
            firmware_version: identity.and_then(|identity| identity.firmware_version.clone()),
            evidence: vec![
                "Original xTool M1 identity confirmed through its HTTP controller API".to_string(),
            ],
        }
    }

    pub fn session_state(&self) -> SessionState {
        match self.runtime.phase() {
            M1RuntimePhase::Disconnected => SessionState::Disconnected,
            M1RuntimePhase::Ready | M1RuntimePhase::ReadyToRun | M1RuntimePhase::Completed => {
                SessionState::Ready
            }
            M1RuntimePhase::Running | M1RuntimePhase::BusyUnowned => SessionState::Running,
            M1RuntimePhase::Paused => SessionState::Paused,
            M1RuntimePhase::Stopped => SessionState::Ready,
            M1RuntimePhase::RecoveryRequired => SessionState::Error,
        }
    }

    pub fn controller_info(&self) -> HashMap<String, String> {
        let mut info = HashMap::new();
        info.insert("transport".to_string(), "xTool M1 HTTP".to_string());
        info.insert("host".to_string(), self.host.clone());
        info.insert("port".to_string(), self.port.to_string());
        if let Some(identity) = self.runtime.identity() {
            if let Some(value) = &identity.device_name {
                info.insert("device_name".to_string(), value.clone());
            }
            if let Some(value) = &identity.machine_type {
                info.insert("machine_type".to_string(), value.clone());
            }
            if let Some(value) = &identity.firmware_version {
                info.insert("firmware_version".to_string(), value.clone());
            }
            if let Some(value) = identity.laser_power_watts {
                info.insert("laser_power_watts".to_string(), value.to_string());
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

    pub fn upload_job(&mut self, job: &M1CompiledJob) -> Result<(), String> {
        self.runtime
            .upload(job)
            .map_err(|error| error.to_string())?;
        self.refresh_machine_status();
        Ok(())
    }

    pub fn poll_job(&mut self) -> Result<M1RuntimePhase, String> {
        let phase = self
            .runtime
            .poll()
            .map_err(|error| error.to_string())?
            .phase;
        self.refresh_machine_status();
        Ok(phase)
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
        if matches!(
            self.runtime.phase(),
            M1RuntimePhase::Ready | M1RuntimePhase::Completed | M1RuntimePhase::Stopped
        ) {
            return Ok(());
        }
        self.cancel_job()
    }

    pub fn disconnect(&mut self) {
        self.runtime.disconnect();
        self.refresh_machine_status();
    }

    fn refresh_machine_status(&mut self) {
        self.machine_status.run_state = match self.runtime.phase() {
            M1RuntimePhase::Running | M1RuntimePhase::BusyUnowned => MachineRunState::Run,
            M1RuntimePhase::Paused => MachineRunState::Hold,
            M1RuntimePhase::RecoveryRequired => MachineRunState::Alarm,
            M1RuntimePhase::Disconnected
            | M1RuntimePhase::Ready
            | M1RuntimePhase::ReadyToRun
            | M1RuntimePhase::Completed
            | M1RuntimePhase::Stopped => MachineRunState::Idle,
        };
    }
}

pub struct XToolM1RuntimeJob {
    total_lines: usize,
    started_at: Instant,
    state: JobState,
    error_message: Option<String>,
    planned_duration_secs: Option<f64>,
}

impl XToolM1RuntimeJob {
    pub fn start(
        job: &M1CompiledJob,
        session: &mut XToolM1RuntimeSession,
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

    pub fn tick(&mut self, session: &mut XToolM1RuntimeSession) -> JobProgress {
        if matches!(
            self.state,
            JobState::Completed | JobState::Failed | JobState::Cancelled
        ) {
            return self.progress();
        }
        match session.poll_job() {
            Ok(M1RuntimePhase::ReadyToRun) => self.state = JobState::ReadyToRun,
            Ok(M1RuntimePhase::Running) => self.state = JobState::Running,
            Ok(M1RuntimePhase::Paused) => self.state = JobState::Paused,
            Ok(M1RuntimePhase::Completed) => self.state = JobState::Completed,
            Ok(M1RuntimePhase::Stopped) => self.state = JobState::Cancelled,
            Ok(M1RuntimePhase::RecoveryRequired | M1RuntimePhase::BusyUnowned) => {
                self.state = JobState::Failed;
                self.error_message = Some("xTool M1 entered a recovery-required state".to_string());
            }
            Ok(M1RuntimePhase::Disconnected | M1RuntimePhase::Ready) => {}
            Err(error) => {
                self.state = JobState::Failed;
                self.error_message = Some(error);
            }
        }
        self.progress()
    }

    pub fn pause(&mut self, session: &mut XToolM1RuntimeSession) -> Result<JobProgress, String> {
        session.pause_job()?;
        self.state = JobState::Paused;
        Ok(self.progress())
    }

    pub fn resume(&mut self, session: &mut XToolM1RuntimeSession) -> Result<JobProgress, String> {
        session.resume_job()?;
        self.state = JobState::Running;
        Ok(self.progress())
    }

    pub fn cancel(&mut self, session: &mut XToolM1RuntimeSession) -> Result<JobProgress, String> {
        session.cancel_job()?;
        self.state = JobState::Cancelled;
        Ok(self.progress())
    }
}
