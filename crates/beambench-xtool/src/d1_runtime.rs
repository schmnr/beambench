use std::time::{Duration, Instant};

use serde_json::Value;
use thiserror::Error;

use crate::{
    D1_JOB_UPLOAD_PATH, D1CompiledJob, D1Model, D1Progress, D1ProtocolError, D1StatusEvent,
    D1WorkingState, XToolHttpIo, encode_d1_upload, parse_d1_model, parse_d1_progress,
    parse_d1_status_event, parse_d1_working_state, require_d1_ack,
};

/// The status channel must return an error on close, heartbeat timeout,
/// oversized messages or queue overflow. Reconnecting it during an owned job
/// could hide a stop event and is deliberately outside this contract.
pub trait D1NetworkIo: XToolHttpIo {
    fn poll_status_events(&mut self) -> Result<Vec<String>, String>;
    /// Flush ordered events through a ping/pong barrier before a new upload.
    fn synchronize_status(&mut self) -> Result<Vec<String>, String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct D1Identity {
    pub model: D1Model,
    pub power_watts: u16,
    pub firmware_version: Option<String>,
}

impl D1Identity {
    pub fn stock_workspace_mm(&self) -> (f64, f64) {
        match (self.model, self.power_watts) {
            (D1Model::D1, _) => (432.0, 406.0),
            (D1Model::D1Pro, 40) => (430.0, 348.0),
            (D1Model::D1Pro, 20) => (430.0, 390.0),
            (D1Model::D1Pro, _) => (430.0, 400.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum D1RuntimePhase {
    Disconnected,
    Ready,
    BusyUnowned,
    ReadyToRun,
    Running,
    PauseRequested,
    Paused,
    ResumeRequested,
    Completed,
    Stopped,
    RecoveryRequired,
}

#[derive(Debug, Error)]
pub enum D1RuntimeError {
    #[error(transparent)]
    Protocol(#[from] D1ProtocolError),
    #[error("xTool D1 communication failed: {0}")]
    Transport(String),
    #[error("xTool D1 cannot {action} while its runtime phase is {phase:?}")]
    InvalidPhase {
        action: &'static str,
        phase: D1RuntimePhase,
    },
    #[error("xTool D1 requires an inserted SD card")]
    MissingSdCard,
    #[error("this xTool D1 laser module is outside the supported blue-diode scope")]
    UnsupportedModule,
    #[error("xTool D1 did not return the exact uploaded file; the upload was not retried")]
    UploadNotVerified,
    #[error("xTool D1 control result could not be confirmed: {0}")]
    Unconfirmed(&'static str),
}

pub struct D1Runtime<I> {
    io: I,
    phase: D1RuntimePhase,
    identity: Option<D1Identity>,
    owned_file: Option<String>,
    saw_owned_running: bool,
    last_progress: Option<D1Progress>,
    pending_control_since: Option<Instant>,
    pending_http_transition_since: Option<Instant>,
    recovery_reason: Option<String>,
}

impl<I: D1NetworkIo> D1Runtime<I> {
    pub fn new(io: I) -> Self {
        Self {
            io,
            phase: D1RuntimePhase::Disconnected,
            identity: None,
            owned_file: None,
            saw_owned_running: false,
            last_progress: None,
            pending_control_since: None,
            pending_http_transition_since: None,
            recovery_reason: None,
        }
    }

    pub const fn phase(&self) -> D1RuntimePhase {
        self.phase
    }
    pub fn identity(&self) -> Option<&D1Identity> {
        self.identity.as_ref()
    }
    pub fn progress(&self) -> Option<D1Progress> {
        self.last_progress
    }
    pub fn has_pending_file(&self) -> bool {
        self.owned_file.is_some()
    }
    pub fn recovery_reason(&self) -> Option<&str> {
        self.recovery_reason.as_deref()
    }
    pub fn into_inner(self) -> I {
        self.io
    }

    fn require_phase(
        &self,
        action: &'static str,
        allowed: &[D1RuntimePhase],
    ) -> Result<(), D1RuntimeError> {
        if allowed.contains(&self.phase) {
            Ok(())
        } else {
            Err(D1RuntimeError::InvalidPhase {
                action,
                phase: self.phase,
            })
        }
    }

    fn get(&mut self, path: &str) -> Result<crate::XToolHttpResponse, D1RuntimeError> {
        self.io.get(path).map_err(D1RuntimeError::Transport)
    }

    fn ack_get(&mut self, path: &str) -> Result<(), D1RuntimeError> {
        let response = self.get(path)?;
        require_d1_ack(response.status, &response.body)?;
        Ok(())
    }

    fn checked_json(&mut self, path: &str) -> Result<Value, D1RuntimeError> {
        let response = self.get(path)?;
        require_d1_ack(response.status, &response.body)?;
        serde_json::from_slice(&response.body)
            .map_err(|_| D1ProtocolError::InvalidResponse("expected JSON").into())
    }

    fn working_state(&mut self) -> Result<D1WorkingState, D1RuntimeError> {
        let response = self.get("/system?action=get_working_sta")?;
        Ok(parse_d1_working_state(response.status, &response.body)?)
    }

    pub fn connect(&mut self) -> Result<(), D1RuntimeError> {
        self.require_phase("connect", &[D1RuntimePhase::Disconnected])?;
        self.ack_get("/ping")?;
        let response = self.get("/getmachinetype")?;
        let model = parse_d1_model(response.status, &response.body)?;
        let power = self.checked_json("/getlaserpowerinfo")?;
        let watts = power
            .get("power")
            .and_then(Value::as_u64)
            .filter(|watts| {
                matches!(
                    (model, *watts),
                    (D1Model::D1, 5 | 10) | (D1Model::D1Pro, 5 | 10 | 20 | 40)
                )
            })
            .ok_or(D1RuntimeError::UnsupportedModule)? as u16;
        if power.get("type").and_then(Value::as_u64) != Some(0) {
            return Err(D1RuntimeError::UnsupportedModule);
        }
        let version = self.checked_json("/system?action=version")?;
        let firmware_version = version
            .get("version")
            .and_then(Value::as_str)
            .filter(|text| text.len() <= 100 && !text.chars().any(char::is_control))
            .map(str::to_string);
        let peripherals = self.checked_json("/peripherystatus")?;
        if peripherals.get("sdCard").and_then(Value::as_u64) != Some(1) {
            return Err(D1RuntimeError::MissingSdCard);
        }
        self.identity = Some(D1Identity {
            model,
            power_watts: watts,
            firmware_version,
        });
        self.phase = if self.working_state()? == D1WorkingState::Idle {
            D1RuntimePhase::Ready
        } else {
            D1RuntimePhase::BusyUnowned
        };
        self.poll()?;
        Ok(())
    }

    /// Upload once, then compare readback before prompting for a physical
    /// start. The device's upload `ok` alone does not prove it stored the file.
    pub fn upload(&mut self, job: &D1CompiledJob, boundary: &str) -> Result<(), D1RuntimeError> {
        self.poll()?;
        self.require_phase(
            "upload",
            &[
                D1RuntimePhase::Ready,
                D1RuntimePhase::Completed,
                D1RuntimePhase::Stopped,
            ],
        )?;
        let prior = self
            .io
            .synchronize_status()
            .map_err(D1RuntimeError::Transport)?;
        for frame in prior {
            if parse_d1_status_event(&frame)? != D1StatusEvent::Idle {
                self.phase = D1RuntimePhase::BusyUnowned;
                return Err(D1RuntimeError::Unconfirmed("status changed before upload"));
            }
        }
        if self.working_state()? != D1WorkingState::Idle {
            self.phase = D1RuntimePhase::BusyUnowned;
            return Err(D1RuntimeError::Unconfirmed("controller is busy"));
        }
        let body = encode_d1_upload(&job.gcode, boundary)?;
        self.last_progress = None;
        self.pending_http_transition_since = None;
        self.saw_owned_running = false;
        // Remember the possible file before the first write. An ambiguous
        // write may have stored it and must be recoverable by explicit Stop.
        self.owned_file = Some(job.gcode.clone());
        let result = (|| {
            let response = self
                .io
                .post(
                    D1_JOB_UPLOAD_PATH,
                    &body,
                    &format!("multipart/form-data; boundary={boundary}"),
                )
                .map_err(D1RuntimeError::Transport)?;
            require_d1_ack(response.status, &response.body)?;
            // `/read?file=...` executes a file. Only the static file route is
            // used here. A failed or transformed readback is not retried.
            let response = self.get("/tmp.gcode")?;
            if response.status != 200 || response.body != job.gcode.as_bytes() {
                return Err(D1RuntimeError::UploadNotVerified);
            }
            // A button press during transfer could run the previous file.
            // Reject any motion predating verified readiness rather than
            // attribute its later Idle to this new program.
            for frame in self
                .io
                .synchronize_status()
                .map_err(D1RuntimeError::Transport)?
            {
                if !matches!(
                    parse_d1_status_event(&frame)?,
                    D1StatusEvent::Idle | D1StatusEvent::AwaitingButton
                ) {
                    return Err(D1RuntimeError::Unconfirmed(
                        "motion or an alarm arrived before upload verification",
                    ));
                }
            }
            if self.working_state()? != D1WorkingState::Idle {
                return Err(D1RuntimeError::Unconfirmed(
                    "controller started before upload verification",
                ));
            }
            self.phase = D1RuntimePhase::ReadyToRun;
            self.poll()?;
            Ok(())
        })();
        if let Err(error) = &result {
            self.recover(error.to_string());
        }
        result
    }

    pub fn poll(&mut self) -> Result<D1RuntimePhase, D1RuntimeError> {
        if self.phase == D1RuntimePhase::Disconnected {
            return Ok(self.phase);
        }
        let result = self.poll_inner();
        if let Err(error) = &result {
            self.recover(error.to_string());
        }
        result
    }

    fn poll_inner(&mut self) -> Result<D1RuntimePhase, D1RuntimeError> {
        let frames = self
            .io
            .poll_status_events()
            .map_err(D1RuntimeError::Transport)?;
        // Loss of ordered status evidence is sticky, even if later frames
        // look idle. Only an explicit confirmed stop can leave recovery.
        if self.phase == D1RuntimePhase::RecoveryRequired {
            return Ok(self.phase);
        }
        let mut ended = false;
        let mut stopped = false;
        for frame in frames {
            let event = parse_d1_status_event(&frame)?;
            match event {
                D1StatusEvent::FlameAlarm
                | D1StatusEvent::TiltAlarm
                | D1StatusEvent::MovementAlarm
                | D1StatusEvent::LimitAlarm
                | D1StatusEvent::Error => {
                    return Err(D1RuntimeError::Unconfirmed("controller reported an alarm"));
                }
                D1StatusEvent::Stopped => {
                    stopped = true;
                    ended = false;
                }
                D1StatusEvent::Idle => {
                    ended = self.saw_owned_running && self.owned_file.is_some() && !stopped;
                }
                D1StatusEvent::RunningOnline | D1StatusEvent::RunningOffline => {
                    ended = false;
                    if self.owns_active_job() {
                        self.saw_owned_running = true;
                        self.phase = D1RuntimePhase::Running;
                        self.pending_control_since = None;
                    } else {
                        self.phase = D1RuntimePhase::BusyUnowned;
                    }
                }
                D1StatusEvent::Paused => {
                    if self.owns_active_job() && self.saw_owned_running {
                        self.phase = D1RuntimePhase::Paused;
                        self.pending_control_since = None;
                    } else {
                        self.phase = D1RuntimePhase::BusyUnowned;
                    }
                }
                D1StatusEvent::AwaitingButton => {
                    if self.owned_file.is_none() {
                        return Err(D1RuntimeError::Unconfirmed(
                            "another application has a file awaiting its start button",
                        ));
                    }
                }
                D1StatusEvent::Framing | D1StatusEvent::FrameReady => {
                    // Beam Bench frames using a regular, zero-output program.
                    // Native vendor framing mode therefore belongs to another
                    // application and cannot complete an owned program.
                    return Err(D1RuntimeError::Unconfirmed(
                        "another framing workflow is active",
                    ));
                }
            }
        }
        let state = self.working_state()?;
        if self.phase == D1RuntimePhase::RecoveryRequired {
            return Ok(self.phase);
        }
        if stopped {
            if state != D1WorkingState::Idle {
                return Err(D1RuntimeError::Unconfirmed("stop is not idle"));
            }
            self.phase = D1RuntimePhase::Stopped;
            self.owned_file = None;
        } else if ended {
            if state != D1WorkingState::Idle {
                return Err(D1RuntimeError::Unconfirmed("completion is not idle"));
            }
            self.phase = D1RuntimePhase::Completed;
            self.owned_file = None;
        } else if self.owned_file.is_none() {
            self.phase = if state == D1WorkingState::Idle {
                if matches!(
                    self.phase,
                    D1RuntimePhase::Completed | D1RuntimePhase::Stopped
                ) {
                    self.phase
                } else {
                    D1RuntimePhase::Ready
                }
            } else {
                D1RuntimePhase::BusyUnowned
            };
        }
        // HTTP can report a transition before the separate WebSocket
        // delivers its event. Wait briefly for ordered evidence, never infer
        // start/completion from the coarse HTTP state alone.
        let awaiting_event = (matches!(
            self.phase,
            D1RuntimePhase::Running
                | D1RuntimePhase::Paused
                | D1RuntimePhase::PauseRequested
                | D1RuntimePhase::ResumeRequested
        ) && state == D1WorkingState::Idle)
            || (self.phase == D1RuntimePhase::ReadyToRun && state != D1WorkingState::Idle);
        if awaiting_event {
            let since = self
                .pending_http_transition_since
                .get_or_insert_with(Instant::now);
            if since.elapsed() >= Duration::from_secs(5) {
                return Err(D1RuntimeError::Unconfirmed(
                    "HTTP changed without its ordered status event",
                ));
            }
        } else {
            self.pending_http_transition_since = None;
        }
        if self
            .pending_control_since
            .is_some_and(|since| since.elapsed() >= Duration::from_secs(5))
        {
            return Err(D1RuntimeError::Unconfirmed(
                "pause or resume status did not arrive",
            ));
        }
        if matches!(self.phase, D1RuntimePhase::Running | D1RuntimePhase::Paused) {
            let response = self.get("/progress")?;
            self.last_progress = Some(parse_d1_progress(response.status, &response.body)?);
        }
        Ok(self.phase)
    }

    pub fn pause(&mut self) -> Result<(), D1RuntimeError> {
        self.require_phase("pause", &[D1RuntimePhase::Running])?;
        if let Err(error) = self.ack_get("/cnc/data?action=pause") {
            self.recover(error.to_string());
            return Err(error);
        }
        self.phase = D1RuntimePhase::PauseRequested;
        self.pending_control_since = Some(Instant::now());
        Ok(())
    }

    pub fn resume(&mut self) -> Result<(), D1RuntimeError> {
        self.require_phase("resume", &[D1RuntimePhase::Paused])?;
        if let Err(error) = self.ack_get("/cnc/data?action=resume") {
            self.recover(error.to_string());
            return Err(error);
        }
        self.phase = D1RuntimePhase::ResumeRequested;
        self.pending_control_since = Some(Instant::now());
        Ok(())
    }

    /// Stop is confirmed from HTTP independently of the status stream, which
    /// may already have failed. Only an explicitly owned file can be removed.
    pub fn stop(&mut self) -> Result<(), D1RuntimeError> {
        self.require_phase(
            "stop",
            &[
                D1RuntimePhase::Ready,
                D1RuntimePhase::BusyUnowned,
                D1RuntimePhase::ReadyToRun,
                D1RuntimePhase::Running,
                D1RuntimePhase::PauseRequested,
                D1RuntimePhase::Paused,
                D1RuntimePhase::ResumeRequested,
                D1RuntimePhase::Completed,
                D1RuntimePhase::Stopped,
                D1RuntimePhase::RecoveryRequired,
            ],
        )?;
        let _stop_ack = self.ack_get("/cnc/data?action=stop");
        let result = (|| {
            self.ack_get("/cmd?cmd=M9+S0+N0")?;
            let mut idle = false;
            for attempt in 0..8 {
                if self.working_state()? == D1WorkingState::Idle {
                    idle = true;
                    break;
                }
                if attempt < 7 {
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
            if !idle {
                return Err(D1RuntimeError::Unconfirmed(
                    "controller is still active after stop",
                ));
            }
            if let Some(owned) = self.owned_file.clone() {
                let file = self.get("/tmp.gcode")?;
                if file.status == 200 && file.body == owned.as_bytes() {
                    self.ack_get("/delete?file=/tmp.gcode")?;
                } else if file.status != 404 && file.status != 200 {
                    return Err(D1RuntimeError::Unconfirmed(
                        "could not check pending file ownership",
                    ));
                }
                self.owned_file = None;
            }
            self.phase = D1RuntimePhase::Stopped;
            self.pending_control_since = None;
            self.recovery_reason = None;
            Ok(())
        })();
        if let Err(error) = &result {
            self.recover(error.to_string());
        }
        result
    }

    fn owns_active_job(&self) -> bool {
        self.owned_file.is_some()
            && matches!(
                self.phase,
                D1RuntimePhase::ReadyToRun
                    | D1RuntimePhase::Running
                    | D1RuntimePhase::PauseRequested
                    | D1RuntimePhase::Paused
                    | D1RuntimePhase::ResumeRequested
            )
    }

    fn recover(&mut self, reason: String) {
        self.phase = D1RuntimePhase::RecoveryRequired;
        self.recovery_reason = Some(reason);
    }

    pub fn disconnect(&mut self) {
        self.phase = D1RuntimePhase::Disconnected;
    }
}
