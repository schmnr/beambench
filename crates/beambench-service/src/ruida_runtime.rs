use std::collections::HashMap;
use std::time::Instant;

use beambench_common::{
    ControllerDriverId, ControllerEvidenceState, ControllerFamily, ControllerModel,
    ControllerProductTier, DeviceCapabilities, JobProgress, JobState, MachinePosition,
    MachineRunState, MachineStatus, PositiveControllerIdentity, SessionState,
};
use beambench_ruida::{
    RDC6442S_CARD_ID, RuidaCompiledJob, RuidaEthernetAdapter, RuidaJogAxis, RuidaRuntime,
    RuidaRuntimeConfig, RuidaRuntimePhase, RuidaRuntimeSnapshot, RuidaTransferConfig, RuidaUdpIo,
};

pub struct RuidaRuntimeSession {
    runtime: RuidaRuntime<RuidaUdpIo>,
    endpoint: String,
    capabilities: DeviceCapabilities,
    machine_status: MachineStatus,
    connected: bool,
}

impl RuidaRuntimeSession {
    pub fn connect(io: RuidaUdpIo, endpoint: String) -> Result<Self, String> {
        let descriptor = RuidaEthernetAdapter::new().descriptor();
        let mut runtime = RuidaRuntime::new(
            io,
            RuidaTransferConfig::default(),
            RuidaRuntimeConfig::default(),
        )
        .map_err(|error| error.to_string())?;
        let snapshot = runtime.connect().map_err(|error| error.to_string())?;
        if snapshot.phase != RuidaRuntimePhase::Ready {
            return Err(format!(
                "Ruida controller is not idle after identity validation ({:?})",
                snapshot.phase
            ));
        }
        let machine_status = machine_status_from_snapshot(&snapshot);
        Ok(Self {
            runtime,
            endpoint,
            capabilities: descriptor.capabilities,
            machine_status,
            connected: true,
        })
    }

    pub const fn driver(&self) -> ControllerDriverId {
        ControllerDriverId::Ruida
    }

    pub const fn controller_model(&self) -> ControllerModel {
        ControllerModel::Ruida
    }

    pub const fn product_tier(&self) -> ControllerProductTier {
        ControllerProductTier::Experimental
    }

    pub const fn evidence_state(&self) -> ControllerEvidenceState {
        ControllerEvidenceState::Emulated
    }

    pub fn capabilities(&self) -> DeviceCapabilities {
        self.capabilities.clone()
    }

    pub fn detected_identity(&self) -> PositiveControllerIdentity {
        PositiveControllerIdentity {
            family: ControllerFamily::Dsp,
            model: ControllerModel::Ruida,
            firmware_identity: Some("RDC6442S".to_string()),
            firmware_version: None,
            evidence: vec![format!(
                "Read-only Ruida card ID matched {RDC6442S_CARD_ID:#x}"
            )],
        }
    }

    pub fn session_state(&self) -> SessionState {
        if !self.connected {
            return SessionState::Disconnected;
        }
        match self.runtime.phase() {
            RuidaRuntimePhase::Ready
            | RuidaRuntimePhase::Uploaded
            | RuidaRuntimePhase::Queued
            | RuidaRuntimePhase::Completed
            | RuidaRuntimePhase::Stopped => SessionState::Ready,
            RuidaRuntimePhase::Paused => SessionState::Paused,
            RuidaRuntimePhase::Disconnected => SessionState::Disconnected,
            RuidaRuntimePhase::RecoveryRequired => SessionState::Error,
            RuidaRuntimePhase::BusyUnowned => SessionState::Validating,
            RuidaRuntimePhase::Uploading
            | RuidaRuntimePhase::Starting
            | RuidaRuntimePhase::Running
            | RuidaRuntimePhase::Pausing
            | RuidaRuntimePhase::Resuming
            | RuidaRuntimePhase::CompletionVerifying
            | RuidaRuntimePhase::ManualMotion
            | RuidaRuntimePhase::Stopping => SessionState::Running,
        }
    }

    pub fn machine_status(&self) -> &MachineStatus {
        &self.machine_status
    }

    pub fn controller_info(&self) -> HashMap<String, String> {
        HashMap::from([
            ("Controller".to_string(), "Ruida RDC6442S".to_string()),
            ("Card ID".to_string(), format!("{RDC6442S_CARD_ID:#x}")),
            ("Transport".to_string(), "Ethernet / UDP".to_string()),
            ("Endpoint".to_string(), self.endpoint.clone()),
            ("Support tier".to_string(), "Experimental".to_string()),
        ])
    }

    pub fn poll(&mut self) -> Result<RuidaRuntimeSnapshot, String> {
        let snapshot = self.runtime.poll().map_err(|error| error.to_string())?;
        self.machine_status = machine_status_from_snapshot(&snapshot);
        Ok(snapshot)
    }

    pub fn home(&mut self) -> Result<(), String> {
        let snapshot = self.runtime.home_xy().map_err(|error| error.to_string())?;
        self.machine_status = machine_status_from_snapshot(&snapshot);
        Ok(())
    }

    pub fn jog(&mut self, x_mm: f64, y_mm: f64, speed_mm_min: f64) -> Result<(), String> {
        let snapshot = self
            .runtime
            .jog(x_mm, y_mm, speed_mm_min)
            .map_err(|error| error.to_string())?;
        self.machine_status = machine_status_from_snapshot(&snapshot);
        Ok(())
    }

    pub fn jog_table(
        &mut self,
        axis: RuidaJogAxis,
        distance_mm: f64,
        speed_mm_min: f64,
    ) -> Result<(), String> {
        let snapshot = self
            .runtime
            .jog_axis(axis, distance_mm, speed_mm_min)
            .map_err(|error| error.to_string())?;
        self.machine_status = machine_status_from_snapshot(&snapshot);
        Ok(())
    }

    /// Stage the job for a tick-driven upload. Nothing is transferred here:
    /// the job tick loop advances the upload in bounded steps so stop,
    /// cancel, and status stay reachable between ticks; queueing and the
    /// start command follow automatically once the upload is confirmed.
    pub fn start_job(
        &mut self,
        compiled: &RuidaCompiledJob,
        frame: bool,
    ) -> Result<RuidaRuntimeJob, String> {
        let snapshot = self
            .runtime
            .begin_upload(compiled)
            .map_err(|error| error.to_string())?;
        self.machine_status = machine_status_from_snapshot(&snapshot);
        Ok(RuidaRuntimeJob {
            // Preparing keeps Stop available but Pause disabled in the UI:
            // the runtime cannot pause an upload, only a running job.
            state: JobState::Preparing,
            started_at: Instant::now(),
            acknowledged_phases: 0,
            upload_progress: self.runtime.upload_progress(),
            error_message: None,
            frame,
        })
    }

    pub fn is_uploading(&self) -> bool {
        self.runtime.phase() == RuidaRuntimePhase::Uploading
    }

    pub fn upload_progress(&self) -> Option<(usize, usize)> {
        self.runtime.upload_progress()
    }

    pub fn advance_upload(&mut self) -> Result<(), String> {
        let snapshot = self
            .runtime
            .advance_upload()
            .map_err(|error| error.to_string())?;
        self.machine_status = machine_status_from_snapshot(&snapshot);
        Ok(())
    }

    /// Queue the confirmed upload and issue the start command (bounded phase
    /// confirmations, a couple of seconds worst case).
    pub fn queue_and_start(&mut self) -> Result<(), String> {
        self.runtime
            .queue_uploaded()
            .map_err(|error| error.to_string())?;
        let snapshot = self.runtime.start().map_err(|error| error.to_string())?;
        self.machine_status = machine_status_from_snapshot(&snapshot);
        Ok(())
    }

    fn abort_upload(&mut self) -> Result<(), String> {
        let snapshot = self
            .runtime
            .abort_upload()
            .map_err(|error| error.to_string())?;
        self.machine_status = machine_status_from_snapshot(&snapshot);
        Ok(())
    }

    pub fn pause_job(&mut self) -> Result<(), String> {
        let snapshot = self.runtime.pause().map_err(|error| error.to_string())?;
        self.machine_status = machine_status_from_snapshot(&snapshot);
        Ok(())
    }

    pub fn resume_job(&mut self) -> Result<(), String> {
        let snapshot = self.runtime.resume().map_err(|error| error.to_string())?;
        self.machine_status = machine_status_from_snapshot(&snapshot);
        Ok(())
    }

    pub fn stop_job(&mut self) -> Result<(), String> {
        if self.is_uploading() {
            return self.abort_upload();
        }
        let snapshot = self.runtime.stop().map_err(|error| error.to_string())?;
        self.machine_status = machine_status_from_snapshot(&snapshot);
        Ok(())
    }

    pub fn emergency_stop(&mut self) -> Result<(), String> {
        match self.runtime.phase() {
            RuidaRuntimePhase::Starting
            | RuidaRuntimePhase::Running
            | RuidaRuntimePhase::Pausing
            | RuidaRuntimePhase::Paused
            | RuidaRuntimePhase::Resuming
            | RuidaRuntimePhase::CompletionVerifying => self.stop_job(),
            // A staged upload never starts motion; abort it and clean up.
            RuidaRuntimePhase::Uploading => self.abort_upload(),
            RuidaRuntimePhase::Uploaded
            | RuidaRuntimePhase::Queued
            | RuidaRuntimePhase::Completed
            | RuidaRuntimePhase::Stopped => self.delete_terminal_job(),
            RuidaRuntimePhase::Ready | RuidaRuntimePhase::Disconnected => Ok(()),
            RuidaRuntimePhase::ManualMotion => Err(
                "Ruida manual motion has no verified software abort command; use the machine's physical stop"
                    .to_string(),
            ),
            RuidaRuntimePhase::BusyUnowned => Err(
                "Ruida reports controller activity that Beam Bench did not start; use the machine's physical stop"
                    .to_string(),
            ),
            RuidaRuntimePhase::RecoveryRequired | RuidaRuntimePhase::Stopping => Err(
                "Ruida could not confirm a software stop; use the machine's physical stop".to_string(),
            ),
        }
    }

    fn delete_terminal_job(&mut self) -> Result<(), String> {
        let snapshot = self
            .runtime
            .delete_uploaded()
            .map_err(|error| error.to_string())?;
        self.machine_status = machine_status_from_snapshot(&snapshot);
        Ok(())
    }

    pub fn disconnect(&mut self) -> Result<(), String> {
        self.emergency_stop()?;
        self.connected = false;
        self.machine_status = MachineStatus::default();
        Ok(())
    }
}

pub struct RuidaRuntimeJob {
    state: JobState,
    started_at: Instant,
    acknowledged_phases: usize,
    upload_progress: Option<(usize, usize)>,
    error_message: Option<String>,
    frame: bool,
}

impl RuidaRuntimeJob {
    const TOTAL_PHASES: usize = 4;

    pub fn tick(&mut self, session: &mut RuidaRuntimeSession) -> JobProgress {
        if !matches!(
            self.state,
            JobState::Preparing | JobState::Running | JobState::Paused
        ) {
            return self.progress();
        }
        // While the upload is staged, each tick advances it by a bounded
        // number of chunk exchanges; once controller storage confirms the
        // file, the same tick queues it and issues the start command.
        if session.is_uploading() {
            if let Err(error) = session.advance_upload() {
                self.state = JobState::Failed;
                self.error_message = Some(error);
                return self.progress();
            }
            self.upload_progress = session.upload_progress();
            if !session.is_uploading() {
                if let Err(error) = session.queue_and_start() {
                    self.state = JobState::Failed;
                    self.error_message = Some(error);
                } else {
                    self.state = JobState::Running;
                    self.acknowledged_phases = 3;
                    self.upload_progress = None;
                }
            }
            return self.progress();
        }
        match session.poll() {
            Ok(snapshot) => match snapshot.phase {
                RuidaRuntimePhase::Completed => {
                    self.state = JobState::Completed;
                    self.acknowledged_phases = Self::TOTAL_PHASES;
                    if let Err(error) = session.delete_terminal_job() {
                        self.error_message = Some(format!(
                            "Job completed but controller-file cleanup failed: {error}"
                        ));
                    }
                }
                RuidaRuntimePhase::Paused => self.state = JobState::Paused,
                RuidaRuntimePhase::Starting
                | RuidaRuntimePhase::Running
                | RuidaRuntimePhase::Pausing
                | RuidaRuntimePhase::Resuming
                | RuidaRuntimePhase::CompletionVerifying => self.state = JobState::Running,
                RuidaRuntimePhase::RecoveryRequired => {
                    self.state = JobState::Failed;
                    self.error_message = snapshot
                        .recovery_reason
                        .map(|reason| format!("Ruida recovery required: {reason:?}"));
                }
                phase => {
                    self.state = JobState::Failed;
                    self.error_message = Some(format!(
                        "Ruida job entered unexpected runtime phase {phase:?}"
                    ));
                }
            },
            Err(error) => {
                self.state = JobState::Failed;
                self.error_message = Some(error);
            }
        }
        self.progress()
    }

    pub fn pause(&mut self, session: &mut RuidaRuntimeSession) -> Result<JobProgress, String> {
        if session.is_uploading() {
            return Err(
                "The job file is still uploading to the controller; stop the job instead of pausing"
                    .to_string(),
            );
        }
        session.pause_job()?;
        self.state = JobState::Paused;
        Ok(self.progress())
    }

    pub fn resume(&mut self, session: &mut RuidaRuntimeSession) -> Result<JobProgress, String> {
        session.resume_job()?;
        self.state = JobState::Running;
        Ok(self.progress())
    }

    pub fn cancel(&mut self, session: &mut RuidaRuntimeSession) -> Result<JobProgress, String> {
        let was_uploading = session.is_uploading();
        session.stop_job()?;
        self.state = JobState::Cancelled;
        self.acknowledged_phases = Self::TOTAL_PHASES;
        self.upload_progress = None;
        // A mid-upload abort already deleted the truncated file and returned
        // the runtime to Ready; a second delete would be an invalid phase.
        if !was_uploading && let Err(error) = session.delete_terminal_job() {
            self.error_message = Some(format!(
                "Job stopped but controller-file cleanup failed: {error}"
            ));
        }
        Ok(self.progress())
    }

    pub fn progress(&self) -> JobProgress {
        // During the staged upload, report chunk-level progress; afterwards
        // fall back to the coarse lifecycle phases.
        let (total, acknowledged) = match self.upload_progress {
            Some((acknowledged, total)) => (total.max(1), acknowledged),
            None => (Self::TOTAL_PHASES, self.acknowledged_phases),
        };
        JobProgress {
            state: self.state,
            total_lines: total,
            queued_lines: total.saturating_sub(acknowledged),
            sent_lines: acknowledged,
            acknowledged_lines: acknowledged,
            elapsed_secs: self.started_at.elapsed().as_secs_f64(),
            estimated_remaining_secs: 0.0,
            buffer_fill_bytes: 0,
            error_message: self.error_message.clone(),
            buckets: Vec::new(),
        }
    }

    pub const fn is_frame(&self) -> bool {
        self.frame
    }
}

fn machine_status_from_snapshot(snapshot: &RuidaRuntimeSnapshot) -> MachineStatus {
    let run_state = match snapshot.phase {
        RuidaRuntimePhase::Ready
        | RuidaRuntimePhase::Uploaded
        | RuidaRuntimePhase::Queued
        | RuidaRuntimePhase::Completed
        | RuidaRuntimePhase::Stopped => MachineRunState::Idle,
        RuidaRuntimePhase::Paused => MachineRunState::Hold,
        RuidaRuntimePhase::ManualMotion => MachineRunState::Jog,
        RuidaRuntimePhase::RecoveryRequired => MachineRunState::Alarm,
        RuidaRuntimePhase::Uploading => MachineRunState::Run,
        RuidaRuntimePhase::BusyUnowned => MachineRunState::Unknown,
        RuidaRuntimePhase::Disconnected => MachineRunState::Sleep,
        RuidaRuntimePhase::Starting
        | RuidaRuntimePhase::Running
        | RuidaRuntimePhase::Pausing
        | RuidaRuntimePhase::Resuming
        | RuidaRuntimePhase::CompletionVerifying
        | RuidaRuntimePhase::Stopping => MachineRunState::Run,
    };
    MachineStatus {
        run_state,
        machine_position: MachinePosition::default(),
        work_position: MachinePosition::default(),
        feed_rate: 0.0,
        spindle_speed: if snapshot.output_may_be_active {
            1.0
        } else {
            0.0
        },
        feed_override: 100,
        spindle_override: 100,
        rapid_override: 100,
        pin_states: String::new(),
    }
}
