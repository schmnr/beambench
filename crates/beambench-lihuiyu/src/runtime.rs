use std::time::Duration;

use thiserror::Error;

use crate::{
    LihuiyuCompiledJob, LihuiyuStatus, M2_MILLIMETRES_PER_STEP, encode_distance,
    transport::{
        LihuiyuJobTransfer, LihuiyuTransferConfig, LihuiyuTransferError, LihuiyuTransferPhase,
        LihuiyuTransferProgress, LihuiyuTransferReceipt, LihuiyuTransferStep, LihuiyuTransport,
        LihuiyuUsbIo,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LihuiyuRuntimeConfig {
    pub completion_attempts: u16,
    pub completion_delay: Duration,
}

impl Default for LihuiyuRuntimeConfig {
    fn default() -> Self {
        Self {
            completion_attempts: 1_000,
            completion_delay: Duration::from_millis(5),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LihuiyuRuntimePhase {
    #[default]
    Disconnected,
    Ready,
    BusyUnowned,
    Transferring,
    Running,
    Paused,
    Completed,
    Stopped,
    PowerFault,
    RecoveryRequired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LihuiyuRecoveryReason {
    AmbiguousTransfer { action: &'static str },
    UnknownStatus(u8),
    PowerFaultDuringOwnedJob,
    ChecksumStateOutsideTransfer,
    CompletionTimeout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LihuiyuRuntimeSnapshot {
    pub phase: LihuiyuRuntimePhase,
    pub identity_verified: bool,
    pub last_status: Option<LihuiyuStatus>,
    pub packets_acknowledged: usize,
    pub total_packets: usize,
    pub commanded_position_steps: [i32; 2],
    pub output_may_be_active: bool,
    pub completion_confirmed: bool,
    pub stop_confirmed: bool,
    pub recovery_reason: Option<LihuiyuRecoveryReason>,
}

#[derive(Debug, Error)]
pub enum LihuiyuRuntimeError {
    #[error(transparent)]
    Transfer(#[from] LihuiyuTransferError),
    #[error("Lihuiyu runtime completion_attempts must be at least one")]
    InvalidCompletionLimit,
    #[error("Lihuiyu cannot {action} while its runtime phase is {phase:?}")]
    InvalidPhase {
        action: &'static str,
        phase: LihuiyuRuntimePhase,
    },
    #[error("Lihuiyu jog requires a finite non-zero X or Y distance")]
    InvalidJogVector,
    #[error("Lihuiyu controller did not report terminal completion in time")]
    CompletionTimeout,
}

#[derive(Debug)]
pub struct LihuiyuRuntime<I> {
    transport: LihuiyuTransport<I>,
    config: LihuiyuRuntimeConfig,
    phase: LihuiyuRuntimePhase,
    identity_verified: bool,
    last_status: Option<LihuiyuStatus>,
    packets_acknowledged: usize,
    total_packets: usize,
    commanded_position_steps: [i32; 2],
    owned_output_unresolved: bool,
    recovery_reason: Option<LihuiyuRecoveryReason>,
    active_transfer: Option<ActiveTransfer>,
}

#[derive(Debug)]
struct ActiveTransfer {
    job: LihuiyuCompiledJob,
    cursor: LihuiyuJobTransfer,
}

/// Status polls consumed per [`LihuiyuRuntime::advance_transfer`] call: with
/// the 2ms poll delay this bounds one advance (and therefore one service tick
/// holding the session lock) to roughly 16ms.
const TRANSFER_POLLS_PER_ADVANCE: u32 = 8;

enum TransferAdvance {
    InProgress,
    Complete(LihuiyuTransferReceipt),
}

impl<I: LihuiyuUsbIo> LihuiyuRuntime<I> {
    pub fn new(
        io: I,
        transfer_config: LihuiyuTransferConfig,
        runtime_config: LihuiyuRuntimeConfig,
    ) -> Result<Self, LihuiyuRuntimeError> {
        if runtime_config.completion_attempts == 0 {
            return Err(LihuiyuRuntimeError::InvalidCompletionLimit);
        }
        Ok(Self {
            transport: LihuiyuTransport::new(io, transfer_config)?,
            config: runtime_config,
            phase: LihuiyuRuntimePhase::Disconnected,
            identity_verified: false,
            last_status: None,
            packets_acknowledged: 0,
            total_packets: 0,
            commanded_position_steps: [0, 0],
            owned_output_unresolved: false,
            recovery_reason: None,
            active_transfer: None,
        })
    }

    pub fn into_inner(self) -> I {
        self.transport.into_inner()
    }

    pub fn io(&self) -> &I {
        self.transport.io()
    }

    pub fn io_mut(&mut self) -> &mut I {
        self.transport.io_mut()
    }

    pub const fn phase(&self) -> LihuiyuRuntimePhase {
        self.phase
    }

    pub fn snapshot(&self) -> LihuiyuRuntimeSnapshot {
        LihuiyuRuntimeSnapshot {
            phase: self.phase,
            identity_verified: self.identity_verified,
            last_status: self.last_status,
            packets_acknowledged: self.packets_acknowledged,
            total_packets: self.total_packets,
            commanded_position_steps: self.commanded_position_steps,
            output_may_be_active: self.owned_output_unresolved,
            completion_confirmed: self.phase == LihuiyuRuntimePhase::Completed,
            stop_confirmed: self.phase == LihuiyuRuntimePhase::Stopped,
            recovery_reason: self.recovery_reason.clone(),
        }
    }

    pub fn connect(&mut self) -> Result<LihuiyuRuntimeSnapshot, LihuiyuRuntimeError> {
        self.require_phase("connect", &[LihuiyuRuntimePhase::Disconnected])?;
        let status = self.transport.connect()?;
        self.identity_verified = true;
        self.last_status = Some(status);
        self.phase = match status {
            LihuiyuStatus::Ready
            | LihuiyuStatus::Finished
            | LihuiyuStatus::SerialCorrectOrM3Finished => LihuiyuRuntimePhase::Ready,
            LihuiyuStatus::Busy => LihuiyuRuntimePhase::BusyUnowned,
            LihuiyuStatus::Power => LihuiyuRuntimePhase::PowerFault,
            LihuiyuStatus::ChecksumError => {
                self.mark_recovery(LihuiyuRecoveryReason::ChecksumStateOutsideTransfer);
                LihuiyuRuntimePhase::RecoveryRequired
            }
            LihuiyuStatus::Unknown(code) => {
                self.mark_recovery(LihuiyuRecoveryReason::UnknownStatus(code));
                LihuiyuRuntimePhase::RecoveryRequired
            }
        };
        Ok(self.snapshot())
    }

    pub fn begin_job(
        &mut self,
        job: &LihuiyuCompiledJob,
    ) -> Result<LihuiyuTransferReceipt, LihuiyuRuntimeError> {
        self.begin_job_with_progress(job, |_| {})
    }

    pub fn begin_job_with_progress(
        &mut self,
        job: &LihuiyuCompiledJob,
        mut progress: impl FnMut(LihuiyuTransferProgress),
    ) -> Result<LihuiyuTransferReceipt, LihuiyuRuntimeError> {
        self.begin_job_transfer(job)?;
        progress(LihuiyuTransferProgress {
            phase: LihuiyuTransferPhase::Preparing,
            packets_acknowledged: 0,
            total_packets: job.packets.len(),
            send_attempts: 0,
        });
        loop {
            let step = self.advance_transfer_step(u32::MAX)?;
            progress(LihuiyuTransferProgress {
                phase: if matches!(step, TransferAdvance::Complete(_)) {
                    LihuiyuTransferPhase::Complete
                } else {
                    LihuiyuTransferPhase::Transferring
                },
                packets_acknowledged: self.packets_acknowledged,
                total_packets: self.total_packets,
                send_attempts: 0,
            });
            if let TransferAdvance::Complete(receipt) = step {
                return Ok(receipt);
            }
        }
    }

    /// Validate and stage a resumable job transfer. Nothing is sent to the
    /// controller until [`Self::advance_transfer`] is called, so the caller
    /// can drive the upload from a tick loop and keep stop/cancel reachable.
    pub fn begin_job_transfer(
        &mut self,
        job: &LihuiyuCompiledJob,
    ) -> Result<LihuiyuRuntimeSnapshot, LihuiyuRuntimeError> {
        self.require_phase(
            "run a job",
            &[
                LihuiyuRuntimePhase::Ready,
                LihuiyuRuntimePhase::Completed,
                LihuiyuRuntimePhase::Stopped,
            ],
        )?;
        let cursor = self.transport.begin_job_transfer(job)?;
        self.phase = LihuiyuRuntimePhase::Transferring;
        self.packets_acknowledged = 0;
        self.total_packets = job.packets.len();
        self.owned_output_unresolved = job.summary.uses_beam;
        self.active_transfer = Some(ActiveTransfer {
            job: job.clone(),
            cursor,
        });
        Ok(self.snapshot())
    }

    /// Advance the staged transfer by one bounded step (at most one packet
    /// acknowledgement and a small number of status polls). On completion the
    /// runtime transitions to Running/Completed exactly like the synchronous
    /// path; on error the ambiguity rules match it too.
    pub fn advance_transfer(&mut self) -> Result<LihuiyuRuntimeSnapshot, LihuiyuRuntimeError> {
        self.advance_transfer_step(TRANSFER_POLLS_PER_ADVANCE)?;
        Ok(self.snapshot())
    }

    /// Abort a staged transfer for an emergency stop or cancel. Acknowledged
    /// packets may still be executing, so the runtime treats the job as an
    /// owned running job and confirms the stop through the cancel path.
    pub fn abort_transfer(&mut self) -> Result<LihuiyuRuntimeSnapshot, LihuiyuRuntimeError> {
        self.require_phase("abort a transfer", &[LihuiyuRuntimePhase::Transferring])?;
        self.active_transfer = None;
        self.phase = LihuiyuRuntimePhase::Running;
        self.cancel()
    }

    fn advance_transfer_step(
        &mut self,
        max_polls: u32,
    ) -> Result<TransferAdvance, LihuiyuRuntimeError> {
        self.require_phase("advance a transfer", &[LihuiyuRuntimePhase::Transferring])?;
        let Some(active) = self.active_transfer.as_mut() else {
            return Err(LihuiyuRuntimeError::InvalidPhase {
                action: "advance a transfer",
                phase: self.phase,
            });
        };
        let result =
            self.transport
                .advance_job_transfer(&active.job, &mut active.cursor, max_polls);
        match result {
            Ok(LihuiyuTransferStep::Pending) => {
                self.packets_acknowledged = active.cursor.packets_acknowledged();
                Ok(TransferAdvance::InProgress)
            }
            Ok(LihuiyuTransferStep::PacketAcknowledged) => {
                self.packets_acknowledged = active.cursor.packets_acknowledged();
                Ok(TransferAdvance::InProgress)
            }
            Ok(LihuiyuTransferStep::Complete(receipt)) => {
                let final_position = active.job.final_position_steps;
                self.active_transfer = None;
                self.packets_acknowledged = receipt.packets_acknowledged;
                self.commanded_position_steps = final_position;
                self.phase = if receipt.observed_terminal_status {
                    self.owned_output_unresolved = false;
                    LihuiyuRuntimePhase::Completed
                } else {
                    LihuiyuRuntimePhase::Running
                };
                Ok(TransferAdvance::Complete(receipt))
            }
            Err(error) => {
                self.packets_acknowledged = active.cursor.packets_acknowledged();
                self.active_transfer = None;
                if self.transport.recovery_required() {
                    self.mark_recovery(LihuiyuRecoveryReason::AmbiguousTransfer {
                        action: "run job",
                    });
                } else if matches!(error, LihuiyuTransferError::PowerFault) {
                    if self.owned_output_unresolved && self.packets_acknowledged > 0 {
                        self.mark_recovery(LihuiyuRecoveryReason::PowerFaultDuringOwnedJob);
                    } else {
                        self.phase = LihuiyuRuntimePhase::PowerFault;
                        self.owned_output_unresolved = false;
                    }
                } else if self.packets_acknowledged > 0 {
                    // The controller accepted part of the job and may still be
                    // executing it; never report Ready while that is possible.
                    self.mark_recovery(LihuiyuRecoveryReason::AmbiguousTransfer {
                        action: "run job",
                    });
                } else {
                    self.phase = LihuiyuRuntimePhase::Ready;
                    self.owned_output_unresolved = false;
                }
                Err(error.into())
            }
        }
    }

    pub fn poll(&mut self) -> Result<LihuiyuRuntimeSnapshot, LihuiyuRuntimeError> {
        if matches!(
            self.phase,
            LihuiyuRuntimePhase::Disconnected | LihuiyuRuntimePhase::RecoveryRequired
        ) {
            return Err(LihuiyuRuntimeError::InvalidPhase {
                action: "poll",
                phase: self.phase,
            });
        }
        let status = match self.transport.read_status() {
            Ok(status) => status,
            Err(error) => {
                if self.owned_output_unresolved {
                    self.mark_recovery(LihuiyuRecoveryReason::AmbiguousTransfer {
                        action: "poll job status",
                    });
                }
                return Err(error.into());
            }
        };
        self.last_status = Some(status);
        match status {
            LihuiyuStatus::Ready | LihuiyuStatus::Busy => {}
            LihuiyuStatus::Finished | LihuiyuStatus::SerialCorrectOrM3Finished => {
                if matches!(
                    self.phase,
                    LihuiyuRuntimePhase::Running | LihuiyuRuntimePhase::Paused
                ) {
                    self.phase = LihuiyuRuntimePhase::Completed;
                    self.owned_output_unresolved = false;
                }
            }
            LihuiyuStatus::Power => {
                if self.owned_output_unresolved {
                    self.mark_recovery(LihuiyuRecoveryReason::PowerFaultDuringOwnedJob);
                } else {
                    self.phase = LihuiyuRuntimePhase::PowerFault;
                }
            }
            LihuiyuStatus::ChecksumError => {
                self.mark_recovery(LihuiyuRecoveryReason::ChecksumStateOutsideTransfer);
            }
            LihuiyuStatus::Unknown(code) => {
                self.mark_recovery(LihuiyuRecoveryReason::UnknownStatus(code));
            }
        }
        Ok(self.snapshot())
    }

    pub fn wait_until_complete(&mut self) -> Result<LihuiyuRuntimeSnapshot, LihuiyuRuntimeError> {
        self.require_phase(
            "wait for completion",
            &[LihuiyuRuntimePhase::Running, LihuiyuRuntimePhase::Paused],
        )?;
        for attempt in 1..=self.config.completion_attempts {
            let snapshot = self.poll()?;
            if snapshot.completion_confirmed {
                return Ok(snapshot);
            }
            if attempt < self.config.completion_attempts && !self.config.completion_delay.is_zero()
            {
                std::thread::sleep(self.config.completion_delay);
            }
        }
        self.mark_recovery(LihuiyuRecoveryReason::CompletionTimeout);
        Err(LihuiyuRuntimeError::CompletionTimeout)
    }

    pub fn pause(&mut self) -> Result<LihuiyuRuntimeSnapshot, LihuiyuRuntimeError> {
        self.require_phase("pause", &[LihuiyuRuntimePhase::Running])?;
        let receipt = self.send_owned_command(b"PN", "pause")?;
        if receipt.observed_terminal_status {
            self.phase = LihuiyuRuntimePhase::Completed;
            self.owned_output_unresolved = false;
        } else {
            self.phase = LihuiyuRuntimePhase::Paused;
        }
        Ok(self.snapshot())
    }

    pub fn resume(&mut self) -> Result<LihuiyuRuntimeSnapshot, LihuiyuRuntimeError> {
        self.require_phase("resume", &[LihuiyuRuntimePhase::Paused])?;
        let receipt = self.send_owned_command(b"PN", "resume")?;
        if receipt.observed_terminal_status {
            self.phase = LihuiyuRuntimePhase::Completed;
            self.owned_output_unresolved = false;
        } else {
            self.phase = LihuiyuRuntimePhase::Running;
        }
        Ok(self.snapshot())
    }

    pub fn cancel(&mut self) -> Result<LihuiyuRuntimeSnapshot, LihuiyuRuntimeError> {
        self.require_phase(
            "cancel",
            &[LihuiyuRuntimePhase::Running, LihuiyuRuntimePhase::Paused],
        )?;
        let receipt = self.send_owned_command(b"IUS1P", "cancel")?;
        self.phase = if receipt.observed_terminal_status {
            LihuiyuRuntimePhase::Completed
        } else {
            LihuiyuRuntimePhase::Stopped
        };
        self.owned_output_unresolved = false;
        Ok(self.snapshot())
    }

    pub fn output_off(&mut self) -> Result<LihuiyuRuntimeSnapshot, LihuiyuRuntimeError> {
        self.require_phase(
            "turn output off",
            &[
                LihuiyuRuntimePhase::Ready,
                LihuiyuRuntimePhase::Completed,
                LihuiyuRuntimePhase::Stopped,
                LihuiyuRuntimePhase::Running,
                LihuiyuRuntimePhase::Paused,
            ],
        )?;
        self.send_owned_command(b"IUS1P", "turn output off")?;
        self.phase = LihuiyuRuntimePhase::Ready;
        self.owned_output_unresolved = false;
        Ok(self.snapshot())
    }

    pub fn home(&mut self) -> Result<LihuiyuRuntimeSnapshot, LihuiyuRuntimeError> {
        self.require_idle_phase("home")?;
        self.send_idle_command(b"IPP", "home")?;
        self.commanded_position_steps = [0, 0];
        self.phase = LihuiyuRuntimePhase::Ready;
        Ok(self.snapshot())
    }

    pub fn unlock(&mut self) -> Result<LihuiyuRuntimeSnapshot, LihuiyuRuntimeError> {
        self.require_idle_phase("unlock")?;
        self.send_idle_command(b"IS2P", "unlock")?;
        self.phase = LihuiyuRuntimePhase::Ready;
        Ok(self.snapshot())
    }

    pub fn jog(
        &mut self,
        dx_mm: f64,
        dy_mm: f64,
    ) -> Result<LihuiyuRuntimeSnapshot, LihuiyuRuntimeError> {
        self.require_idle_phase("jog")?;
        if !dx_mm.is_finite() || !dy_mm.is_finite() {
            return Err(LihuiyuRuntimeError::InvalidJogVector);
        }
        let dx = millimetres_to_delta_steps(dx_mm)?;
        let dy = millimetres_to_delta_steps(dy_mm)?;
        if dx == 0 && dy == 0 {
            return Err(LihuiyuRuntimeError::InvalidJogVector);
        }
        let mut command = Vec::from(&b"IU"[..]);
        if dx != 0 {
            command.push(if dx > 0 { b'B' } else { b'T' });
            command.extend(encode_distance(dx.unsigned_abs()).map_err(LihuiyuTransferError::from)?);
        }
        if dy != 0 {
            command.push(if dy > 0 { b'R' } else { b'L' });
            command.extend(encode_distance(dy.unsigned_abs()).map_err(LihuiyuTransferError::from)?);
        }
        command.extend_from_slice(b"S1P");
        self.send_idle_command(&command, "jog")?;
        self.commanded_position_steps[0] = self.commanded_position_steps[0]
            .checked_add(dx)
            .ok_or(LihuiyuRuntimeError::InvalidJogVector)?;
        self.commanded_position_steps[1] = self.commanded_position_steps[1]
            .checked_add(dy)
            .ok_or(LihuiyuRuntimeError::InvalidJogVector)?;
        self.phase = LihuiyuRuntimePhase::Ready;
        Ok(self.snapshot())
    }

    fn send_owned_command(
        &mut self,
        command: &[u8],
        action: &'static str,
    ) -> Result<LihuiyuTransferReceipt, LihuiyuRuntimeError> {
        match self.transport.send_command(command) {
            Ok(receipt) => Ok(receipt),
            Err(error) => {
                if self.transport.recovery_required() || self.owned_output_unresolved {
                    self.mark_recovery(LihuiyuRecoveryReason::AmbiguousTransfer { action });
                }
                Err(error.into())
            }
        }
    }

    fn send_idle_command(
        &mut self,
        command: &[u8],
        action: &'static str,
    ) -> Result<(), LihuiyuRuntimeError> {
        if let Err(error) = self.transport.send_command(command) {
            if self.transport.recovery_required() {
                self.mark_recovery(LihuiyuRecoveryReason::AmbiguousTransfer { action });
            }
            return Err(error.into());
        }
        Ok(())
    }

    fn require_idle_phase(&self, action: &'static str) -> Result<(), LihuiyuRuntimeError> {
        self.require_phase(
            action,
            &[
                LihuiyuRuntimePhase::Ready,
                LihuiyuRuntimePhase::Completed,
                LihuiyuRuntimePhase::Stopped,
            ],
        )
    }

    fn require_phase(
        &self,
        action: &'static str,
        allowed: &[LihuiyuRuntimePhase],
    ) -> Result<(), LihuiyuRuntimeError> {
        if allowed.contains(&self.phase) {
            Ok(())
        } else {
            Err(LihuiyuRuntimeError::InvalidPhase {
                action,
                phase: self.phase,
            })
        }
    }

    fn mark_recovery(&mut self, reason: LihuiyuRecoveryReason) {
        self.phase = LihuiyuRuntimePhase::RecoveryRequired;
        self.recovery_reason = Some(reason);
    }
}

fn millimetres_to_delta_steps(value: f64) -> Result<i32, LihuiyuRuntimeError> {
    let steps = value / M2_MILLIMETRES_PER_STEP;
    if !steps.is_finite() || steps < f64::from(i32::MIN) || steps > f64::from(i32::MAX) {
        return Err(LihuiyuRuntimeError::InvalidJogVector);
    }
    Ok(steps.round() as i32)
}

#[cfg(test)]
mod tests {
    use beambench_common::{Bounds, Point2D};
    use beambench_planner::{ExecutionPlan, PlanSegment};

    use crate::{LihuiyuCompilationConfig, LihuiyuVirtualUsbIo, compile_lihuiyu_job};

    use super::*;

    fn runtime() -> LihuiyuRuntime<LihuiyuVirtualUsbIo> {
        LihuiyuRuntime::new(
            LihuiyuVirtualUsbIo::m2_nano(),
            LihuiyuTransferConfig {
                io_timeout: Duration::ZERO,
                status_attempts: 8,
                busy_status_attempts: 64,
                status_delay: Duration::ZERO,
                checksum_attempts: 3,
            },
            LihuiyuRuntimeConfig {
                completion_attempts: 8,
                completion_delay: Duration::ZERO,
            },
        )
        .unwrap()
    }

    fn compiled_job() -> LihuiyuCompiledJob {
        let plan = ExecutionPlan {
            id: Default::default(),
            project_id: Default::default(),
            revision_hash: "runtime-test".to_string(),
            created_at: Default::default(),
            bounds: Bounds::new(Point2D::zero(), Point2D::new(20.0, 20.0)),
            total_distance_mm: 10.0,
            estimated_duration_secs: 1.0,
            segments: vec![PlanSegment::Vector {
                polyline: vec![Point2D::new(1.0, 1.0), Point2D::new(10.0, 1.0)],
                closed: false,
                power_percent: 100.0,
                speed_mm_min: 900.0,
                layer_id: "layer".to_string(),
                cut_entry_id: "entry".to_string(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            }],
            layer_order: vec!["layer".to_string()],
            warnings: Vec::new(),
            failed_entries: Vec::new(),
        };
        compile_lihuiyu_job(&plan, &LihuiyuCompilationConfig::default()).unwrap()
    }

    #[test]
    fn positive_status_identity_precedes_every_mutation() {
        let mut runtime = runtime();
        let snapshot = runtime.connect().unwrap();
        assert_eq!(snapshot.phase, LihuiyuRuntimePhase::Ready);
        assert!(snapshot.identity_verified);
        assert!(runtime.io().controller().accepted_packets().is_empty());
    }

    #[test]
    fn unknown_identity_status_is_rejected_before_mutation() {
        let mut runtime = runtime();
        runtime
            .io_mut()
            .controller_mut()
            .return_unknown_status_once(0xaa);
        assert!(runtime.connect().is_err());
        assert_eq!(runtime.phase(), LihuiyuRuntimePhase::Disconnected);
        assert!(runtime.io().controller().accepted_packets().is_empty());
    }

    #[test]
    fn job_pause_resume_cancel_and_output_off_share_the_real_transport() {
        let mut runtime = runtime();
        runtime.connect().unwrap();
        let job = compiled_job();
        let receipt = runtime.begin_job(&job).unwrap();
        assert_eq!(receipt.packets_acknowledged, job.packets.len());
        assert_eq!(runtime.phase(), LihuiyuRuntimePhase::Running);
        assert!(runtime.snapshot().output_may_be_active);

        runtime.pause().unwrap();
        assert_eq!(runtime.phase(), LihuiyuRuntimePhase::Paused);
        assert!(runtime.io().controller().paused());
        runtime.resume().unwrap();
        assert_eq!(runtime.phase(), LihuiyuRuntimePhase::Running);
        assert!(!runtime.io().controller().paused());
        let stopped = runtime.cancel().unwrap();
        assert!(stopped.stop_confirmed);
        assert!(!stopped.output_may_be_active);
        assert!(!runtime.io().controller().output_active());
    }

    #[test]
    fn terminal_status_is_required_for_completion() {
        let mut runtime = runtime();
        runtime.connect().unwrap();
        runtime.begin_job(&compiled_job()).unwrap();
        assert!(!runtime.poll().unwrap().completion_confirmed);
        runtime.io_mut().controller_mut().complete_job();
        let snapshot = runtime.wait_until_complete().unwrap();
        assert!(snapshot.completion_confirmed);
        assert!(!snapshot.output_may_be_active);
    }

    #[test]
    fn stale_terminal_status_before_first_packet_does_not_complete_new_job() {
        let mut runtime = runtime();
        runtime.connect().unwrap();
        runtime
            .io_mut()
            .controller_mut()
            .report_status_once(LihuiyuStatus::Finished);
        runtime.begin_job(&compiled_job()).unwrap();
        assert_eq!(runtime.phase(), LihuiyuRuntimePhase::Running);
        assert!(runtime.snapshot().output_may_be_active);
    }

    #[test]
    fn explicit_checksum_rejection_retries_without_recovery() {
        let mut runtime = runtime();
        runtime.connect().unwrap();
        runtime.io_mut().controller_mut().reject_next_packets(1);
        let receipt = runtime.begin_job(&compiled_job()).unwrap();
        assert_eq!(receipt.checksum_rejections, 1);
        assert_eq!(receipt.send_attempts, receipt.packets_acknowledged + 1);
        assert_eq!(runtime.phase(), LihuiyuRuntimePhase::Running);
    }

    #[test]
    fn ambiguous_packet_write_never_retries_and_requires_reconnect() {
        let mut runtime = runtime();
        runtime.connect().unwrap();
        runtime.io_mut().fail_next_packet_writes(1);
        assert!(runtime.begin_job(&compiled_job()).is_err());
        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.phase, LihuiyuRuntimePhase::RecoveryRequired);
        assert!(snapshot.output_may_be_active);
        assert!(runtime.begin_job(&compiled_job()).is_err());
    }

    #[test]
    fn lost_ack_after_packet_write_is_ambiguous_and_never_retried() {
        let mut runtime = runtime();
        runtime.connect().unwrap();
        runtime.io_mut().fail_next_packet_ack_reads(1);
        assert!(runtime.begin_job(&compiled_job()).is_err());
        assert_eq!(runtime.phase(), LihuiyuRuntimePhase::RecoveryRequired);
        assert_eq!(runtime.io().controller().accepted_packets().len(), 1);
    }

    #[test]
    fn transfer_is_resumable_and_abortable_for_emergency_stop() {
        let mut runtime = runtime();
        runtime.connect().unwrap();
        let job = compiled_job();
        let total = job.packets.len();
        assert!(total >= 2, "test needs a multi-packet job, got {total}");

        let snapshot = runtime.begin_job_transfer(&job).unwrap();
        assert_eq!(snapshot.phase, LihuiyuRuntimePhase::Transferring);
        // Starting a transfer must not perform the upload: the synchronous
        // upload held the service session lock for the whole burn, making
        // emergency stop unreachable.
        assert!(runtime.io().controller().accepted_packets().is_empty());

        // One bounded advance makes partial progress only.
        let snapshot = runtime.advance_transfer().unwrap();
        assert_eq!(snapshot.phase, LihuiyuRuntimePhase::Transferring);
        assert!(snapshot.packets_acknowledged >= 1);
        assert!(runtime.io().controller().accepted_packets().len() < total);

        // Emergency stop between ticks: abort and confirm the stop through
        // the owned-job cancel path.
        let stopped = runtime.abort_transfer().unwrap();
        assert!(stopped.stop_confirmed);
        assert!(!stopped.output_may_be_active);
        assert!(!runtime.io().controller().output_active());
        assert!(runtime.io().controller().accepted_packets().len() < total + 1);
    }

    #[test]
    fn resumable_transfer_completes_and_matches_the_synchronous_contract() {
        let mut runtime = runtime();
        runtime.connect().unwrap();
        let job = compiled_job();
        runtime.begin_job_transfer(&job).unwrap();
        let mut guard = 0;
        loop {
            let snapshot = runtime.advance_transfer().unwrap();
            if snapshot.phase != LihuiyuRuntimePhase::Transferring {
                assert_eq!(snapshot.phase, LihuiyuRuntimePhase::Running);
                assert_eq!(snapshot.packets_acknowledged, job.packets.len());
                assert!(snapshot.output_may_be_active);
                break;
            }
            guard += 1;
            assert!(guard < 10_000, "transfer did not complete");
        }
        assert_eq!(
            runtime.io().controller().controller_bytes(),
            &job.controller_bytes[..]
        );
    }

    #[test]
    fn mid_transfer_failure_after_acknowledged_packets_requires_reconnect() {
        let mut runtime = runtime();
        runtime.connect().unwrap();
        runtime
            .io_mut()
            .controller_mut()
            .reject_packets_after_accepted(1);
        let error = runtime.begin_job(&compiled_job()).unwrap_err();
        assert!(matches!(
            error,
            LihuiyuRuntimeError::Transfer(LihuiyuTransferError::ChecksumRejected { .. })
        ));
        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.phase, LihuiyuRuntimePhase::RecoveryRequired);
        assert!(
            snapshot.output_may_be_active,
            "acknowledged packets may still be executing with the beam on"
        );
        assert!(runtime.begin_job(&compiled_job()).is_err());
    }

    #[test]
    fn sustained_busy_before_a_packet_does_not_abort_the_job() {
        let mut runtime = runtime();
        runtime.connect().unwrap();
        for _ in 0..20 {
            runtime
                .io_mut()
                .controller_mut()
                .report_status_once(LihuiyuStatus::Busy);
        }
        let job = compiled_job();
        let receipt = runtime.begin_job(&job).unwrap();
        assert_eq!(receipt.packets_acknowledged, job.packets.len());
    }

    #[test]
    fn wedged_busy_controller_still_times_out_within_the_busy_budget() {
        let mut runtime = LihuiyuRuntime::new(
            LihuiyuVirtualUsbIo::m2_nano(),
            LihuiyuTransferConfig {
                io_timeout: Duration::ZERO,
                status_attempts: 1_000,
                busy_status_attempts: 4,
                status_delay: Duration::ZERO,
                checksum_attempts: 3,
            },
            LihuiyuRuntimeConfig {
                completion_attempts: 8,
                completion_delay: Duration::ZERO,
            },
        )
        .unwrap();
        runtime.connect().unwrap();
        for _ in 0..10 {
            runtime
                .io_mut()
                .controller_mut()
                .report_status_once(LihuiyuStatus::Busy);
        }
        let error = runtime.begin_job(&compiled_job()).unwrap_err();
        assert!(matches!(
            error,
            LihuiyuRuntimeError::Transfer(LihuiyuTransferError::ReadyTimeout)
        ));
        assert_eq!(
            runtime.phase(),
            LihuiyuRuntimePhase::Ready,
            "nothing was acknowledged, so the runtime may safely return to Ready"
        );
    }

    #[test]
    fn power_fault_before_first_packet_does_not_claim_job_activity() {
        let mut runtime = runtime();
        runtime.connect().unwrap();
        runtime
            .io_mut()
            .controller_mut()
            .report_status_once(LihuiyuStatus::Power);
        assert!(runtime.begin_job(&compiled_job()).is_err());
        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.phase, LihuiyuRuntimePhase::PowerFault);
        assert!(!snapshot.output_may_be_active);
        assert!(runtime.io().controller().accepted_packets().is_empty());
    }

    #[test]
    fn completion_timeout_preserves_uncertainty_and_requires_reconnect() {
        let mut runtime = runtime();
        runtime.connect().unwrap();
        runtime.begin_job(&compiled_job()).unwrap();
        assert!(matches!(
            runtime.wait_until_complete(),
            Err(LihuiyuRuntimeError::CompletionTimeout)
        ));
        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.phase, LihuiyuRuntimePhase::RecoveryRequired);
        assert!(snapshot.output_may_be_active);
    }

    #[test]
    fn altered_compiled_artifact_is_rejected_before_usb_mutation() {
        let mut runtime = runtime();
        runtime.connect().unwrap();
        let mut job = compiled_job();
        job.controller_bytes.push(b'X');
        assert!(matches!(
            runtime.begin_job(&job),
            Err(LihuiyuRuntimeError::Transfer(
                LihuiyuTransferError::InvalidCompiledJob
            ))
        ));
        assert_eq!(runtime.phase(), LihuiyuRuntimePhase::Ready);
        assert!(runtime.io().controller().accepted_packets().is_empty());
    }

    #[test]
    fn home_unlock_and_finite_jog_are_output_disabled() {
        let mut runtime = runtime();
        runtime.connect().unwrap();
        assert_eq!(runtime.home().unwrap().commanded_position_steps, [0, 0]);
        assert!(runtime.io().controller().homed());
        runtime.unlock().unwrap();
        assert!(runtime.io().controller().unlocked());
        let snapshot = runtime.jog(2.54, -1.27).unwrap();
        assert_eq!(snapshot.commanded_position_steps, [100, -50]);
        assert!(!snapshot.output_may_be_active);
        assert!(!runtime.io().controller().output_active());
        assert!(runtime.jog(f64::NAN, 1.0).is_err());
        assert!(runtime.jog(0.0, 0.0).is_err());
    }
}
