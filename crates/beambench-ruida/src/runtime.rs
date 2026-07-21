use std::time::Duration;

use thiserror::Error;

use crate::{
    KNOWN_MACHINE_STATUS_BITS, MACHINE_STATUS_JOB_RUNNING, MACHINE_STATUS_MOVING,
    MACHINE_STATUS_PART_END, RuidaCompiledJob, RuidaDatagramIo, RuidaJogAxis, RuidaProcessAction,
    RuidaStorageClient, RuidaStoredFile, RuidaTransferConfig, RuidaTransferError,
    RuidaUploadProgress, RuidaUploadReceipt, home_xy_command, jog_speed_command,
    process_control_command, relative_jog_command, select_document_command,
    transport::{RuidaUploadCursor, RuidaUploadStep},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuidaRuntimeConfig {
    pub status_confirmation_attempts: u8,
    pub status_confirmation_delay: Duration,
}

impl Default for RuidaRuntimeConfig {
    fn default() -> Self {
        Self {
            status_confirmation_attempts: 20,
            status_confirmation_delay: Duration::from_millis(100),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RuidaRuntimePhase {
    #[default]
    Disconnected,
    Ready,
    BusyUnowned,
    Uploading,
    Uploaded,
    Queued,
    Starting,
    Running,
    Pausing,
    Paused,
    Resuming,
    CompletionVerifying,
    Completed,
    ManualMotion,
    Stopping,
    Stopped,
    RecoveryRequired,
}

impl RuidaRuntimePhase {
    const fn owns_active_job(self) -> bool {
        matches!(
            self,
            Self::Starting
                | Self::Running
                | Self::Pausing
                | Self::Paused
                | Self::Resuming
                | Self::CompletionVerifying
                | Self::Stopping
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuidaRecoveryReason {
    AmbiguousCommand { action: &'static str },
    StatusUnavailable,
    UnknownStatusBits { raw: u64, unknown: u64 },
    UnexpectedIdle,
    UnexpectedControllerActivity,
    ConfirmationTimeout { action: &'static str },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuidaRuntimeSnapshot {
    pub phase: RuidaRuntimePhase,
    pub controller_card_id: Option<u64>,
    pub raw_machine_status: Option<u64>,
    pub file: Option<RuidaStoredFile>,
    /// True while controller-native state says an owned job may still emit.
    /// This deliberately does not claim the tube is firing at this instant and
    /// remains true when a previously issued start has no confirmed terminal state.
    pub output_may_be_active: bool,
    pub completion_confirmed: bool,
    pub stop_confirmed: bool,
    pub recovery_reason: Option<RuidaRecoveryReason>,
}

#[derive(Debug, Error)]
pub enum RuidaRuntimeError {
    #[error(transparent)]
    Transfer(#[from] RuidaTransferError),
    #[error("Ruida runtime status_confirmation_attempts must be at least one")]
    InvalidConfirmationLimit,
    #[error("Ruida cannot {action} while its runtime phase is {phase:?}")]
    InvalidPhase {
        action: &'static str,
        phase: RuidaRuntimePhase,
    },
    #[error("Ruida controller returned unknown machine-status bits {unknown:#x} in {raw:#x}")]
    UnknownStatusBits { raw: u64, unknown: u64 },
    #[error("Ruida controller became idle without a part-end or requested stop transition")]
    UnexpectedIdle,
    #[error("Ruida controller became active before Beam Bench issued a start command")]
    UnexpectedControllerActivity,
    #[error("Ruida controller did not confirm {action} within the bounded status window")]
    ConfirmationTimeout { action: &'static str },
    #[error("Ruida jog requires a finite non-zero motion distance")]
    InvalidJogVector,
}

#[derive(Debug)]
pub struct RuidaRuntime<I> {
    storage: RuidaStorageClient<I>,
    config: RuidaRuntimeConfig,
    phase: RuidaRuntimePhase,
    raw_machine_status: Option<u64>,
    receipt: Option<RuidaUploadReceipt>,
    observed_active: bool,
    observed_part_end: bool,
    owned_output_unresolved: bool,
    recovery_reason: Option<RuidaRecoveryReason>,
    active_upload: Option<ActiveUpload>,
}

#[derive(Debug)]
struct ActiveUpload {
    job: RuidaCompiledJob,
    cursor: RuidaUploadCursor,
}

/// Acknowledged data chunks per [`RuidaRuntime::advance_upload`] call. Each
/// chunk is one stop-and-wait exchange, so this bounds how long one advance
/// (and therefore one service tick holding the session lock) can take.
const UPLOAD_CHUNKS_PER_ADVANCE: usize = 2;

impl<I: RuidaDatagramIo> RuidaRuntime<I> {
    pub fn new(
        io: I,
        transfer_config: RuidaTransferConfig,
        runtime_config: RuidaRuntimeConfig,
    ) -> Result<Self, RuidaRuntimeError> {
        if runtime_config.status_confirmation_attempts == 0 {
            return Err(RuidaRuntimeError::InvalidConfirmationLimit);
        }
        Ok(Self {
            storage: RuidaStorageClient::new(io, transfer_config)?,
            config: runtime_config,
            phase: RuidaRuntimePhase::Disconnected,
            raw_machine_status: None,
            receipt: None,
            observed_active: false,
            observed_part_end: false,
            owned_output_unresolved: false,
            recovery_reason: None,
            active_upload: None,
        })
    }

    pub fn into_inner(self) -> I {
        self.storage.into_inner()
    }

    pub fn io(&self) -> &I {
        self.storage.io()
    }

    pub fn io_mut(&mut self) -> &mut I {
        self.storage.io_mut()
    }

    pub const fn phase(&self) -> RuidaRuntimePhase {
        self.phase
    }

    pub fn receipt(&self) -> Option<&RuidaUploadReceipt> {
        self.receipt.as_ref()
    }

    pub fn snapshot(&self) -> RuidaRuntimeSnapshot {
        let output_may_be_active = self.owned_output_unresolved
            || self
                .raw_machine_status
                .is_some_and(|status| status & MACHINE_STATUS_JOB_RUNNING != 0);
        RuidaRuntimeSnapshot {
            phase: self.phase,
            controller_card_id: self.storage.verified_card_id(),
            raw_machine_status: self.raw_machine_status,
            file: self.receipt.as_ref().map(|receipt| receipt.file().clone()),
            output_may_be_active,
            completion_confirmed: self.phase == RuidaRuntimePhase::Completed,
            stop_confirmed: self.phase == RuidaRuntimePhase::Stopped,
            recovery_reason: self.recovery_reason.clone(),
        }
    }

    pub fn connect(&mut self) -> Result<RuidaRuntimeSnapshot, RuidaRuntimeError> {
        self.require_phase("connect", &[RuidaRuntimePhase::Disconnected])?;
        self.storage.connect()?;
        let raw = self.read_status()?;
        validate_known_status(raw)?;
        self.phase = if machine_is_active(raw) || machine_is_part_end(raw) {
            RuidaRuntimePhase::BusyUnowned
        } else {
            RuidaRuntimePhase::Ready
        };
        Ok(self.snapshot())
    }

    pub fn upload_job(
        &mut self,
        job: &RuidaCompiledJob,
    ) -> Result<RuidaUploadReceipt, RuidaRuntimeError> {
        self.upload_job_with_progress(job, |_| {})
    }

    pub fn upload_job_with_progress(
        &mut self,
        job: &RuidaCompiledJob,
        progress: impl FnMut(RuidaUploadProgress),
    ) -> Result<RuidaUploadReceipt, RuidaRuntimeError> {
        self.require_phase("upload", &[RuidaRuntimePhase::Ready])?;
        match self.storage.upload_job_with_progress(job, progress) {
            Ok(receipt) => {
                self.receipt = Some(receipt.clone());
                self.phase = RuidaRuntimePhase::Uploaded;
                Ok(receipt)
            }
            Err(error) => {
                if self.storage.recovery_required() {
                    self.mark_recovery(RuidaRecoveryReason::AmbiguousCommand { action: "upload" });
                }
                Err(error.into())
            }
        }
    }

    /// Validate and stage a resumable upload. Nothing is sent until
    /// [`Self::advance_upload`] is called, so a tick loop can drive the
    /// transfer and keep stop/cancel/status reachable between ticks.
    pub fn begin_upload(
        &mut self,
        job: &RuidaCompiledJob,
    ) -> Result<RuidaRuntimeSnapshot, RuidaRuntimeError> {
        self.require_phase("upload", &[RuidaRuntimePhase::Ready])?;
        let cursor = self.storage.begin_upload(job)?;
        self.phase = RuidaRuntimePhase::Uploading;
        self.active_upload = Some(ActiveUpload {
            job: job.clone(),
            cursor,
        });
        Ok(self.snapshot())
    }

    /// Progress of the staged upload as (acknowledged, total) chunks.
    pub fn upload_progress(&self) -> Option<(usize, usize)> {
        self.active_upload.as_ref().map(|active| {
            (
                active.cursor.packets_acknowledged(),
                active.cursor.total_packets(),
            )
        })
    }

    /// Advance the staged upload by a bounded number of chunk exchanges. On
    /// completion the runtime enters Uploaded exactly like the synchronous
    /// path; error ambiguity rules match it too.
    pub fn advance_upload(&mut self) -> Result<RuidaRuntimeSnapshot, RuidaRuntimeError> {
        self.require_phase("advance an upload", &[RuidaRuntimePhase::Uploading])?;
        let Some(active) = self.active_upload.as_mut() else {
            return Err(RuidaRuntimeError::InvalidPhase {
                action: "advance an upload",
                phase: self.phase,
            });
        };
        let result =
            self.storage
                .advance_upload(&active.job, &mut active.cursor, UPLOAD_CHUNKS_PER_ADVANCE);
        match result {
            Ok(RuidaUploadStep::Pending) => Ok(self.snapshot()),
            Ok(RuidaUploadStep::Complete(receipt)) => {
                self.active_upload = None;
                self.receipt = Some(receipt);
                self.phase = RuidaRuntimePhase::Uploaded;
                Ok(self.snapshot())
            }
            Err(error) => {
                self.active_upload = None;
                if self.storage.recovery_required() {
                    self.mark_recovery(RuidaRecoveryReason::AmbiguousCommand { action: "upload" });
                } else {
                    // Nothing owned is executing: an upload never starts
                    // motion, so a failed upload returns to Ready.
                    self.phase = RuidaRuntimePhase::Ready;
                }
                Err(error.into())
            }
        }
    }

    /// Abort a staged upload. The upload never starts motion, so a verified
    /// abort (upload stream closed, truncated file confirmed deleted)
    /// returns the runtime to Ready. If the controller's upload state cannot
    /// be verified clean, the runtime fails closed into RecoveryRequired:
    /// while a transfer is pending the controller consumes every datagram as
    /// file data, so later commands would be silently swallowed.
    pub fn abort_upload(&mut self) -> Result<RuidaRuntimeSnapshot, RuidaRuntimeError> {
        self.require_phase("abort an upload", &[RuidaRuntimePhase::Uploading])?;
        let active = self
            .active_upload
            .take()
            .expect("uploading phase owns an active upload");
        match self.storage.abort_upload(&active.cursor) {
            Ok(()) => {
                self.phase = RuidaRuntimePhase::Ready;
                Ok(self.snapshot())
            }
            Err(error) => {
                self.mark_recovery(RuidaRecoveryReason::AmbiguousCommand {
                    action: "abort upload",
                });
                Err(error.into())
            }
        }
    }

    pub fn queue_uploaded(&mut self) -> Result<RuidaRuntimeSnapshot, RuidaRuntimeError> {
        self.require_phase("queue", &[RuidaRuntimePhase::Uploaded])?;
        let receipt = self
            .receipt
            .as_ref()
            .expect("uploaded phase owns a receipt")
            .clone();
        let file = self.storage.inspect_receipt(&receipt)?;
        let command = select_document_command(file.index).map_err(RuidaTransferError::from)?;
        if let Err(error) = self.storage.send_acknowledged_command(&command) {
            self.mark_recovery(RuidaRecoveryReason::AmbiguousCommand { action: "queue" });
            return Err(error.into());
        }
        // Selection must not itself start motion or output.
        if let Err(error) = self.poll() {
            if self.phase != RuidaRuntimePhase::RecoveryRequired {
                self.mark_recovery(RuidaRecoveryReason::StatusUnavailable);
            }
            return Err(error);
        }
        if self.phase != RuidaRuntimePhase::Uploaded {
            self.mark_recovery(RuidaRecoveryReason::UnexpectedControllerActivity);
            return Err(RuidaRuntimeError::UnexpectedControllerActivity);
        }
        self.phase = RuidaRuntimePhase::Queued;
        Ok(self.snapshot())
    }

    pub fn start(&mut self) -> Result<RuidaRuntimeSnapshot, RuidaRuntimeError> {
        self.require_phase("start", &[RuidaRuntimePhase::Queued])?;
        self.phase = RuidaRuntimePhase::Starting;
        self.observed_active = false;
        self.observed_part_end = false;
        self.owned_output_unresolved = true;
        self.send_process_command(RuidaProcessAction::Start, "start")?;
        self.wait_for_phase("start", RuidaRuntimePhase::Running)
    }

    pub fn pause(&mut self) -> Result<RuidaRuntimeSnapshot, RuidaRuntimeError> {
        self.require_phase("pause", &[RuidaRuntimePhase::Running])?;
        self.phase = RuidaRuntimePhase::Pausing;
        self.send_process_command(RuidaProcessAction::Pause, "pause")?;
        self.wait_for_phase("pause", RuidaRuntimePhase::Paused)
    }

    pub fn resume(&mut self) -> Result<RuidaRuntimeSnapshot, RuidaRuntimeError> {
        self.require_phase("resume", &[RuidaRuntimePhase::Paused])?;
        self.phase = RuidaRuntimePhase::Resuming;
        self.send_process_command(RuidaProcessAction::Resume, "resume")?;
        self.wait_for_phase("resume", RuidaRuntimePhase::Running)
    }

    pub fn stop(&mut self) -> Result<RuidaRuntimeSnapshot, RuidaRuntimeError> {
        self.require_phase(
            "stop",
            &[
                RuidaRuntimePhase::Starting,
                RuidaRuntimePhase::Running,
                RuidaRuntimePhase::Pausing,
                RuidaRuntimePhase::Paused,
                RuidaRuntimePhase::Resuming,
                RuidaRuntimePhase::CompletionVerifying,
            ],
        )?;
        self.phase = RuidaRuntimePhase::Stopping;
        self.send_process_command(RuidaProcessAction::Stop, "stop")?;
        self.wait_for_phase("stop", RuidaRuntimePhase::Stopped)
    }

    pub fn home_xy(&mut self) -> Result<RuidaRuntimeSnapshot, RuidaRuntimeError> {
        self.require_phase("home", &[RuidaRuntimePhase::Ready])?;
        self.phase = RuidaRuntimePhase::ManualMotion;
        self.send_manual_command(&home_xy_command(), "home")?;
        Ok(self.snapshot())
    }

    pub fn jog(
        &mut self,
        x_mm: f64,
        y_mm: f64,
        speed_mm_min: f64,
    ) -> Result<RuidaRuntimeSnapshot, RuidaRuntimeError> {
        self.require_phase("jog", &[RuidaRuntimePhase::Ready])?;
        if !x_mm.is_finite() || !y_mm.is_finite() || (x_mm == 0.0 && y_mm == 0.0) {
            return Err(RuidaRuntimeError::InvalidJogVector);
        }
        let speed = jog_speed_command(speed_mm_min).map_err(RuidaTransferError::from)?;
        let x = (x_mm != 0.0)
            .then(|| relative_jog_command(RuidaJogAxis::X, x_mm))
            .transpose()
            .map_err(RuidaTransferError::from)?;
        let y = (y_mm != 0.0)
            .then(|| relative_jog_command(RuidaJogAxis::Y, y_mm))
            .transpose()
            .map_err(RuidaTransferError::from)?;

        self.send_manual_command(&speed, "set jog speed")?;
        self.phase = RuidaRuntimePhase::ManualMotion;
        if let Some(command) = x {
            self.send_manual_command(&command, "jog X")?;
        }
        if let Some(command) = y {
            self.send_manual_command(&command, "jog Y")?;
        }
        Ok(self.snapshot())
    }

    /// Perform one bounded, output-disabled manual move on a single Ruida
    /// controller channel. Z and U are both used for lift tables in the field,
    /// so the machine profile chooses the correct channel.
    pub fn jog_axis(
        &mut self,
        axis: RuidaJogAxis,
        distance_mm: f64,
        speed_mm_min: f64,
    ) -> Result<RuidaRuntimeSnapshot, RuidaRuntimeError> {
        self.require_phase("jog", &[RuidaRuntimePhase::Ready])?;
        if !distance_mm.is_finite() || distance_mm == 0.0 {
            return Err(RuidaRuntimeError::InvalidJogVector);
        }
        let speed = jog_speed_command(speed_mm_min).map_err(RuidaTransferError::from)?;
        let command = relative_jog_command(axis, distance_mm).map_err(RuidaTransferError::from)?;
        let label = match axis {
            RuidaJogAxis::X => "jog X",
            RuidaJogAxis::Y => "jog Y",
            RuidaJogAxis::Z => "jog Z",
            RuidaJogAxis::U => "jog U",
        };

        self.send_manual_command(&speed, "set jog speed")?;
        self.phase = RuidaRuntimePhase::ManualMotion;
        self.send_manual_command(&command, label)?;
        Ok(self.snapshot())
    }

    pub fn poll(&mut self) -> Result<RuidaRuntimeSnapshot, RuidaRuntimeError> {
        let raw = self.read_status()?;
        self.apply_status(raw)?;
        Ok(self.snapshot())
    }

    pub fn delete_uploaded(&mut self) -> Result<RuidaRuntimeSnapshot, RuidaRuntimeError> {
        self.require_phase(
            "delete uploaded job",
            &[
                RuidaRuntimePhase::Uploaded,
                RuidaRuntimePhase::Queued,
                RuidaRuntimePhase::Completed,
                RuidaRuntimePhase::Stopped,
            ],
        )?;
        let receipt = self
            .receipt
            .as_ref()
            .expect("a deletable phase owns a receipt")
            .clone();
        self.storage.delete_receipt(&receipt)?;
        self.receipt = None;
        self.raw_machine_status = Some(0);
        self.observed_active = false;
        self.observed_part_end = false;
        self.owned_output_unresolved = false;
        self.phase = RuidaRuntimePhase::Ready;
        Ok(self.snapshot())
    }

    fn read_status(&mut self) -> Result<u64, RuidaRuntimeError> {
        match self.storage.read_machine_status() {
            Ok(raw) => {
                self.raw_machine_status = Some(raw);
                Ok(raw)
            }
            Err(error) => {
                if self.phase.owns_active_job() || self.phase == RuidaRuntimePhase::ManualMotion {
                    self.mark_recovery(RuidaRecoveryReason::StatusUnavailable);
                }
                Err(error.into())
            }
        }
    }

    fn apply_status(&mut self, raw: u64) -> Result<(), RuidaRuntimeError> {
        let unknown = raw & !KNOWN_MACHINE_STATUS_BITS;
        if unknown != 0 {
            self.mark_recovery(RuidaRecoveryReason::UnknownStatusBits { raw, unknown });
            return Err(RuidaRuntimeError::UnknownStatusBits { raw, unknown });
        }

        if machine_is_part_end(raw) {
            if self.phase == RuidaRuntimePhase::Stopping {
                return Ok(());
            }
            if self.phase == RuidaRuntimePhase::ManualMotion {
                self.mark_recovery(RuidaRecoveryReason::UnexpectedControllerActivity);
                return Err(RuidaRuntimeError::UnexpectedControllerActivity);
            }
            if self.phase.owns_active_job() {
                self.observed_part_end = true;
                self.phase = RuidaRuntimePhase::CompletionVerifying;
            } else {
                self.phase = RuidaRuntimePhase::BusyUnowned;
            }
            return Ok(());
        }

        if machine_is_active(raw) {
            match self.phase {
                RuidaRuntimePhase::Starting | RuidaRuntimePhase::Resuming
                    if raw & MACHINE_STATUS_MOVING != 0 =>
                {
                    self.observed_active = true;
                    self.phase = RuidaRuntimePhase::Running;
                }
                RuidaRuntimePhase::Starting | RuidaRuntimePhase::Resuming => {}
                RuidaRuntimePhase::Running if raw & MACHINE_STATUS_MOVING == 0 => {
                    self.phase = RuidaRuntimePhase::Paused;
                }
                RuidaRuntimePhase::Running => {
                    self.observed_active = true;
                }
                RuidaRuntimePhase::Pausing
                    if raw & MACHINE_STATUS_JOB_RUNNING != 0
                        && raw & MACHINE_STATUS_MOVING == 0 =>
                {
                    self.phase = RuidaRuntimePhase::Paused;
                }
                RuidaRuntimePhase::Pausing | RuidaRuntimePhase::Stopping => {}
                RuidaRuntimePhase::ManualMotion if raw & MACHINE_STATUS_JOB_RUNNING == 0 => {}
                RuidaRuntimePhase::ManualMotion => {
                    self.mark_recovery(RuidaRecoveryReason::UnexpectedControllerActivity);
                    return Err(RuidaRuntimeError::UnexpectedControllerActivity);
                }
                RuidaRuntimePhase::Paused if raw & MACHINE_STATUS_MOVING == 0 => {}
                RuidaRuntimePhase::Paused => {
                    self.phase = RuidaRuntimePhase::Running;
                }
                RuidaRuntimePhase::Disconnected
                | RuidaRuntimePhase::Ready
                | RuidaRuntimePhase::BusyUnowned
                | RuidaRuntimePhase::Completed
                | RuidaRuntimePhase::Stopped => {
                    self.phase = RuidaRuntimePhase::BusyUnowned;
                }
                RuidaRuntimePhase::Uploading
                | RuidaRuntimePhase::Uploaded
                | RuidaRuntimePhase::Queued
                | RuidaRuntimePhase::CompletionVerifying => {
                    self.mark_recovery(RuidaRecoveryReason::UnexpectedControllerActivity);
                    return Err(RuidaRuntimeError::UnexpectedControllerActivity);
                }
                RuidaRuntimePhase::RecoveryRequired => {}
            }
            return Ok(());
        }

        match self.phase {
            RuidaRuntimePhase::BusyUnowned => self.phase = RuidaRuntimePhase::Ready,
            RuidaRuntimePhase::ManualMotion => self.phase = RuidaRuntimePhase::Ready,
            RuidaRuntimePhase::CompletionVerifying
                if self.observed_active && self.observed_part_end =>
            {
                self.owned_output_unresolved = false;
                self.phase = RuidaRuntimePhase::Completed;
            }
            RuidaRuntimePhase::Stopping => {
                self.owned_output_unresolved = false;
                self.phase = RuidaRuntimePhase::Stopped;
            }
            RuidaRuntimePhase::Starting => {}
            phase if phase.owns_active_job() => {
                self.mark_recovery(RuidaRecoveryReason::UnexpectedIdle);
                return Err(RuidaRuntimeError::UnexpectedIdle);
            }
            _ => {}
        }
        Ok(())
    }

    fn send_process_command(
        &mut self,
        action: RuidaProcessAction,
        label: &'static str,
    ) -> Result<(), RuidaRuntimeError> {
        if let Err(error) = self
            .storage
            .send_acknowledged_command(&process_control_command(action))
        {
            self.mark_recovery(RuidaRecoveryReason::AmbiguousCommand { action: label });
            return Err(error.into());
        }
        Ok(())
    }

    fn send_manual_command(
        &mut self,
        command: &[u8],
        label: &'static str,
    ) -> Result<(), RuidaRuntimeError> {
        if let Err(error) = self.storage.send_acknowledged_command(command) {
            self.mark_recovery(RuidaRecoveryReason::AmbiguousCommand { action: label });
            return Err(error.into());
        }
        Ok(())
    }

    fn wait_for_phase(
        &mut self,
        action: &'static str,
        expected: RuidaRuntimePhase,
    ) -> Result<RuidaRuntimeSnapshot, RuidaRuntimeError> {
        for attempt in 1..=self.config.status_confirmation_attempts {
            let snapshot = self.poll()?;
            if snapshot.phase == expected {
                return Ok(snapshot);
            }
            if attempt < self.config.status_confirmation_attempts {
                std::thread::sleep(self.config.status_confirmation_delay);
            }
        }
        self.mark_recovery(RuidaRecoveryReason::ConfirmationTimeout { action });
        Err(RuidaRuntimeError::ConfirmationTimeout { action })
    }

    fn require_phase(
        &self,
        action: &'static str,
        allowed: &[RuidaRuntimePhase],
    ) -> Result<(), RuidaRuntimeError> {
        if allowed.contains(&self.phase) {
            Ok(())
        } else {
            Err(RuidaRuntimeError::InvalidPhase {
                action,
                phase: self.phase,
            })
        }
    }

    fn mark_recovery(&mut self, reason: RuidaRecoveryReason) {
        self.phase = RuidaRuntimePhase::RecoveryRequired;
        self.recovery_reason = Some(reason);
    }
}

fn validate_known_status(raw: u64) -> Result<(), RuidaRuntimeError> {
    let unknown = raw & !KNOWN_MACHINE_STATUS_BITS;
    if unknown == 0 {
        Ok(())
    } else {
        Err(RuidaRuntimeError::UnknownStatusBits { raw, unknown })
    }
}

const fn machine_is_active(raw: u64) -> bool {
    raw & (MACHINE_STATUS_MOVING | MACHINE_STATUS_JOB_RUNNING) != 0
}

const fn machine_is_part_end(raw: u64) -> bool {
    raw & MACHINE_STATUS_PART_END != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        RuidaCompilationConfig, RuidaVirtualExecutionState, RuidaVirtualIo,
        compile_ruida_storage_sentinel,
    };

    fn runtime() -> RuidaRuntime<RuidaVirtualIo> {
        RuidaRuntime::new(
            RuidaVirtualIo::rdc6442s(),
            RuidaTransferConfig::default(),
            RuidaRuntimeConfig {
                status_confirmation_attempts: 2,
                status_confirmation_delay: Duration::ZERO,
            },
        )
        .unwrap()
    }

    fn queued_runtime() -> RuidaRuntime<RuidaVirtualIo> {
        let mut runtime = runtime();
        assert_eq!(runtime.connect().unwrap().phase, RuidaRuntimePhase::Ready);
        let sentinel = compile_ruida_storage_sentinel(&RuidaCompilationConfig::default()).unwrap();
        runtime.upload_job(&sentinel).unwrap();
        runtime.queue_uploaded().unwrap();
        runtime
    }

    fn multi_chunk_job() -> crate::RuidaCompiledJob {
        use beambench_common::{Bounds, Point2D};
        use beambench_planner::{ExecutionPlan, PlanSegment};
        // Enough vertices that clear_bytes spans several UDP chunks.
        let polyline: Vec<Point2D> = (0..600)
            .map(|i| {
                let step = f64::from(i);
                Point2D::new(1.0 + (step % 97.0), 1.0 + ((step * 7.0) % 97.0))
            })
            .collect();
        let plan = ExecutionPlan {
            id: Default::default(),
            project_id: Default::default(),
            revision_hash: "ruida-multi-chunk-test".to_string(),
            created_at: Default::default(),
            bounds: Bounds::new(Point2D::new(1.0, 1.0), Point2D::new(98.0, 98.0)),
            total_distance_mm: 0.0,
            estimated_duration_secs: 0.0,
            segments: vec![PlanSegment::Vector {
                polyline,
                closed: false,
                power_percent: 40.0,
                speed_mm_min: 1_200.0,
                layer_id: "layer-1".to_string(),
                cut_entry_id: "cut-1".to_string(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            }],
            layer_order: vec!["layer-1".to_string()],
            warnings: Vec::new(),
            failed_entries: Vec::new(),
        };
        crate::compile_ruida_job(&plan, &RuidaCompilationConfig::default()).unwrap()
    }

    #[test]
    fn upload_is_resumable_and_abortable_for_stop_availability() {
        let mut runtime = runtime();
        runtime.connect().unwrap();
        let job = multi_chunk_job();
        assert!(
            job.clear_bytes.len() > 2 * crate::protocol::MAX_UDP_PAYLOAD_SIZE,
            "test needs a multi-chunk job, got {} bytes",
            job.clear_bytes.len()
        );

        let snapshot = runtime.begin_upload(&job).unwrap();
        assert_eq!(snapshot.phase, RuidaRuntimePhase::Uploading);

        // One bounded advance must not upload the whole file: the synchronous
        // upload held the service session lock for the full transfer.
        let snapshot = runtime.advance_upload().unwrap();
        assert_eq!(snapshot.phase, RuidaRuntimePhase::Uploading);

        // Abort between ticks must verifiably close the controller's upload
        // state: no pending upload, no leftover file, and later commands must
        // not be swallowed as file data.
        let aborted = runtime.abort_upload().unwrap();
        assert_eq!(aborted.phase, RuidaRuntimePhase::Ready);
        assert!(
            !runtime.io().controller().has_pending_upload(),
            "abort must close the controller's pending upload"
        );
        assert!(runtime.io().controller().files().is_empty());
        runtime.poll().unwrap();

        // The controller must accept a fresh upload after the abort.
        runtime.begin_upload(&job).unwrap();
        let mut guard = 0;
        while runtime.phase() == RuidaRuntimePhase::Uploading {
            runtime.advance_upload().unwrap();
            guard += 1;
            assert!(guard < 10_000, "post-abort upload did not complete");
        }
        assert_eq!(runtime.phase(), RuidaRuntimePhase::Uploaded);
    }

    #[test]
    fn resumable_upload_completes_with_a_valid_receipt() {
        let mut runtime = runtime();
        runtime.connect().unwrap();
        let job = multi_chunk_job();
        runtime.begin_upload(&job).unwrap();
        let mut guard = 0;
        while runtime.phase() == RuidaRuntimePhase::Uploading {
            runtime.advance_upload().unwrap();
            guard += 1;
            assert!(guard < 10_000, "upload did not complete");
        }
        assert_eq!(runtime.phase(), RuidaRuntimePhase::Uploaded);
        let receipt = runtime.receipt().expect("uploaded phase owns a receipt");
        assert_eq!(receipt.clear_byte_len(), job.clear_bytes.len());

        // The staged upload plugs into the existing native lifecycle.
        runtime.queue_uploaded().unwrap();
        assert_eq!(runtime.start().unwrap().phase, RuidaRuntimePhase::Running);
    }

    #[test]
    fn virtual_runtime_requires_controller_native_lifecycle_transitions() {
        let mut runtime = queued_runtime();
        assert_eq!(runtime.phase(), RuidaRuntimePhase::Queued);
        assert_eq!(
            runtime.io().controller().execution_state(),
            RuidaVirtualExecutionState::Idle
        );

        let running = runtime.start().unwrap();
        assert_eq!(running.phase, RuidaRuntimePhase::Running);
        assert!(running.output_may_be_active);
        assert!(!running.completion_confirmed);

        let paused = runtime.pause().unwrap();
        assert_eq!(paused.phase, RuidaRuntimePhase::Paused);
        assert!(!runtime.io().controller().output_active());

        let resumed = runtime.resume().unwrap();
        assert_eq!(resumed.phase, RuidaRuntimePhase::Running);
        assert!(runtime.io_mut().controller_mut().complete_execution());
        assert_eq!(
            runtime.poll().unwrap().phase,
            RuidaRuntimePhase::CompletionVerifying
        );
        assert!(runtime.io_mut().controller_mut().settle_completion());
        let completed = runtime.poll().unwrap();
        assert_eq!(completed.phase, RuidaRuntimePhase::Completed);
        assert!(completed.completion_confirmed);
        assert!(!completed.output_may_be_active);

        assert_eq!(
            runtime.delete_uploaded().unwrap().phase,
            RuidaRuntimePhase::Ready
        );
        assert!(runtime.io().controller().files().is_empty());
    }

    #[test]
    fn requested_stop_is_confirmed_but_never_reported_as_completion() {
        let mut runtime = queued_runtime();
        runtime.start().unwrap();
        let stopped = runtime.stop().unwrap();
        assert_eq!(stopped.phase, RuidaRuntimePhase::Stopped);
        assert!(stopped.stop_confirmed);
        assert!(!stopped.completion_confirmed);
        assert!(!stopped.output_may_be_active);
        assert!(!runtime.io().controller().output_active());
    }

    #[test]
    fn part_end_observed_while_stopping_cannot_become_completion() {
        let mut runtime = queued_runtime();
        runtime.start().unwrap();
        runtime.phase = RuidaRuntimePhase::Stopping;
        assert!(runtime.io_mut().controller_mut().complete_execution());

        let part_end = runtime.poll().unwrap();
        assert_eq!(part_end.phase, RuidaRuntimePhase::Stopping);
        assert!(!part_end.completion_confirmed);

        assert!(runtime.io_mut().controller_mut().settle_completion());
        let stopped = runtime.poll().unwrap();
        assert_eq!(stopped.phase, RuidaRuntimePhase::Stopped);
        assert!(stopped.stop_confirmed);
        assert!(!stopped.completion_confirmed);
    }

    #[test]
    fn unexpected_idle_after_running_requires_recovery() {
        let mut runtime = queued_runtime();
        runtime.start().unwrap();
        runtime.io_mut().controller_mut().set_machine_status(0);
        assert!(matches!(
            runtime.poll(),
            Err(RuidaRuntimeError::UnexpectedIdle)
        ));
        assert_eq!(runtime.phase(), RuidaRuntimePhase::RecoveryRequired);
        assert!(runtime.snapshot().output_may_be_active);
        assert_eq!(
            runtime.snapshot().recovery_reason,
            Some(RuidaRecoveryReason::UnexpectedIdle)
        );
    }

    #[test]
    fn ambiguous_start_timeout_is_not_retried_or_reported_running() {
        let mut runtime = queued_runtime();
        runtime.io_mut().drop_next_sends(1);
        assert!(matches!(
            runtime.start(),
            Err(RuidaRuntimeError::Transfer(
                RuidaTransferError::AcknowledgementTimeout { attempts: 1 }
            ))
        ));
        assert_eq!(runtime.phase(), RuidaRuntimePhase::RecoveryRequired);
        assert!(!runtime.snapshot().completion_confirmed);
        assert!(runtime.snapshot().output_may_be_active);
        assert_eq!(
            runtime.io().controller().execution_state(),
            RuidaVirtualExecutionState::Idle
        );
    }

    #[test]
    fn unknown_status_bits_and_unowned_activity_never_become_ready() {
        let mut busy = runtime();
        busy.io_mut()
            .controller_mut()
            .set_machine_status(MACHINE_STATUS_MOVING);
        assert_eq!(
            busy.connect().unwrap().phase,
            RuidaRuntimePhase::BusyUnowned
        );
        assert!(!busy.snapshot().output_may_be_active);

        let mut unknown = runtime();
        unknown.io_mut().controller_mut().set_machine_status(0x40);
        assert!(matches!(
            unknown.connect(),
            Err(RuidaRuntimeError::UnknownStatusBits {
                raw: 0x40,
                unknown: 0x40
            })
        ));
        assert_eq!(unknown.phase(), RuidaRuntimePhase::Disconnected);

        let mut active = queued_runtime();
        active.start().unwrap();
        active.io_mut().controller_mut().set_machine_status(0x40);
        assert!(matches!(
            active.poll(),
            Err(RuidaRuntimeError::UnknownStatusBits {
                raw: 0x40,
                unknown: 0x40
            })
        ));
        assert_eq!(active.phase(), RuidaRuntimePhase::RecoveryRequired);
        assert!(active.snapshot().output_may_be_active);
    }

    #[test]
    fn queue_requires_a_post_selection_status_confirmation() {
        let mut runtime = runtime();
        runtime.connect().unwrap();
        let sentinel = compile_ruida_storage_sentinel(&RuidaCompilationConfig::default()).unwrap();
        runtime.upload_job(&sentinel).unwrap();
        // Receipt inspection uses two sends, selection uses one, and the
        // scheduled fault drops the following machine-status query.
        runtime.io_mut().drop_send_after(3);

        assert!(matches!(
            runtime.queue_uploaded(),
            Err(RuidaRuntimeError::Transfer(
                RuidaTransferError::AcknowledgementTimeout { attempts: 1 }
            ))
        ));
        assert_eq!(runtime.phase(), RuidaRuntimePhase::RecoveryRequired);
        assert_eq!(
            runtime.snapshot().recovery_reason,
            Some(RuidaRecoveryReason::StatusUnavailable)
        );
    }

    #[test]
    fn explicit_start_nak_can_be_retried_but_status_lag_is_bounded() {
        let mut runtime = queued_runtime();
        runtime.io_mut().nak_next_sends(1);
        assert_eq!(runtime.start().unwrap().phase, RuidaRuntimePhase::Running);

        runtime.phase = RuidaRuntimePhase::Resuming;
        runtime
            .io_mut()
            .controller_mut()
            .set_machine_status(MACHINE_STATUS_JOB_RUNNING);
        assert_eq!(runtime.poll().unwrap().phase, RuidaRuntimePhase::Resuming);

        runtime.phase = RuidaRuntimePhase::Starting;
        runtime.io_mut().controller_mut().set_machine_status(0);
        assert!(matches!(
            runtime.wait_for_phase("start", RuidaRuntimePhase::Running),
            Err(RuidaRuntimeError::ConfirmationTimeout { action: "start" })
        ));
        assert_eq!(runtime.phase(), RuidaRuntimePhase::RecoveryRequired);
        assert!(runtime.snapshot().output_may_be_active);
    }

    #[test]
    fn home_and_jog_require_acknowledged_output_disabled_manual_motion() {
        let mut runtime = runtime();
        runtime.connect().unwrap();

        let home = runtime.home_xy().unwrap();
        assert_eq!(home.phase, RuidaRuntimePhase::ManualMotion);
        assert!(!home.output_may_be_active);
        assert_eq!(
            runtime.io().controller().execution_state(),
            RuidaVirtualExecutionState::ManualMotion
        );
        assert!(runtime.io_mut().controller_mut().settle_manual_motion());
        assert_eq!(runtime.poll().unwrap().phase, RuidaRuntimePhase::Ready);

        let jog = runtime.jog(2.5, -3.0, 6_000.0).unwrap();
        assert_eq!(jog.phase, RuidaRuntimePhase::ManualMotion);
        assert!(!jog.output_may_be_active);
        assert_eq!(
            runtime.io().controller().position_micrometres(),
            (2_500, -3_000)
        );
        assert_eq!(
            runtime.io().controller().jog_speed_micrometres_per_second(),
            Some(100_000)
        );
        assert!(runtime.io_mut().controller_mut().settle_manual_motion());
        assert_eq!(runtime.poll().unwrap().phase, RuidaRuntimePhase::Ready);
        assert!(!runtime.io().controller().output_active());
    }

    #[test]
    fn z_and_u_table_jogs_use_bounded_output_disabled_moves() {
        let mut runtime = runtime();
        runtime.connect().unwrap();

        let z = runtime.jog_axis(RuidaJogAxis::Z, 1.25, 300.0).unwrap();
        assert_eq!(z.phase, RuidaRuntimePhase::ManualMotion);
        assert!(!z.output_may_be_active);
        assert_eq!(
            runtime.io().controller().table_position_micrometres(),
            (1_250, 0)
        );
        assert!(runtime.io_mut().controller_mut().settle_manual_motion());
        assert_eq!(runtime.poll().unwrap().phase, RuidaRuntimePhase::Ready);

        let u = runtime.jog_axis(RuidaJogAxis::U, -2.0, 300.0).unwrap();
        assert_eq!(u.phase, RuidaRuntimePhase::ManualMotion);
        assert!(!u.output_may_be_active);
        assert_eq!(
            runtime.io().controller().table_position_micrometres(),
            (1_250, -2_000)
        );
        assert!(!runtime.io().controller().output_active());
    }

    #[test]
    fn invalid_or_partially_delivered_jog_never_claims_ready() {
        let mut runtime = runtime();
        runtime.connect().unwrap();
        assert!(matches!(
            runtime.jog(0.0, 0.0, 1_000.0),
            Err(RuidaRuntimeError::InvalidJogVector)
        ));
        assert_eq!(runtime.phase(), RuidaRuntimePhase::Ready);

        // Speed and X are acknowledged; the Y acknowledgement is lost.
        runtime.io_mut().drop_send_after(2);
        assert!(matches!(
            runtime.jog(1.0, 1.0, 1_000.0),
            Err(RuidaRuntimeError::Transfer(
                RuidaTransferError::AcknowledgementTimeout { attempts: 1 }
            ))
        ));
        assert_eq!(runtime.phase(), RuidaRuntimePhase::RecoveryRequired);
        assert_eq!(runtime.io().controller().position_micrometres(), (1_000, 0));
        assert_eq!(
            runtime.snapshot().recovery_reason,
            Some(RuidaRecoveryReason::AmbiguousCommand { action: "jog Y" })
        );
    }
}
