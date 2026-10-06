use std::time::Duration;
use thiserror::Error;

use crate::M1CompiledJob;
use crate::protocol::{
    M1HttpIo, M1Identity, M1ProtocolError, M1Status, probe_m1_identity, read_m1_status,
    require_ok_response,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum M1RuntimePhase {
    #[default]
    Disconnected,
    Ready,
    BusyUnowned,
    ReadyToRun,
    Running,
    Paused,
    Completed,
    Stopped,
    RecoveryRequired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct M1RuntimeSnapshot {
    pub phase: M1RuntimePhase,
    pub status: Option<M1Status>,
    pub identity: Option<M1Identity>,
    pub output_may_be_active: bool,
    pub recovery_reason: Option<String>,
}

#[derive(Debug, Error)]
pub enum M1RuntimeError {
    #[error(transparent)]
    Protocol(#[from] M1ProtocolError),
    #[error("xTool M1 cannot {action} while its runtime phase is {phase:?}")]
    InvalidPhase {
        action: &'static str,
        phase: M1RuntimePhase,
    },
    #[error("xTool M1 reported unknown status {0}")]
    UnknownStatus(String),
    #[error("xTool M1 is busy with a job that Beam Bench did not start")]
    BusyUnowned,
    #[error(
        "xTool M1 accepted the stop but still reports {0}; use the machine's button or power switch"
    )]
    StopUnconfirmed(String),
}

/// Status reads after a stop before it counts as unconfirmed (about two
/// seconds at the default interval).
pub(crate) const STOP_CONFIRM_ATTEMPTS: usize = 8;
const STOP_CONFIRM_INTERVAL: Duration = Duration::from_millis(250);
pub(crate) const STOPPABLE_PHASES: &[M1RuntimePhase] = &[
    M1RuntimePhase::ReadyToRun,
    M1RuntimePhase::Running,
    M1RuntimePhase::Paused,
    M1RuntimePhase::BusyUnowned,
    M1RuntimePhase::RecoveryRequired,
];

pub struct M1Runtime<I> {
    io: I,
    phase: M1RuntimePhase,
    status: Option<M1Status>,
    identity: Option<M1Identity>,
    recovery_reason: Option<String>,
    stop_confirm_interval: Duration,
}

impl<I: M1HttpIo> M1Runtime<I> {
    pub fn new(io: I) -> Self {
        Self {
            io,
            phase: M1RuntimePhase::Disconnected,
            status: None,
            identity: None,
            recovery_reason: None,
            stop_confirm_interval: STOP_CONFIRM_INTERVAL,
        }
    }

    pub fn into_inner(self) -> I {
        self.io
    }

    pub const fn phase(&self) -> M1RuntimePhase {
        self.phase
    }

    pub fn identity(&self) -> Option<&M1Identity> {
        self.identity.as_ref()
    }

    pub fn snapshot(&self) -> M1RuntimeSnapshot {
        M1RuntimeSnapshot {
            phase: self.phase,
            status: self.status.clone(),
            identity: self.identity.clone(),
            output_may_be_active: matches!(
                self.phase,
                M1RuntimePhase::Running | M1RuntimePhase::Paused | M1RuntimePhase::BusyUnowned
            ),
            recovery_reason: self.recovery_reason.clone(),
        }
    }

    pub fn connect(&mut self) -> Result<M1RuntimeSnapshot, M1RuntimeError> {
        self.require_phase("connect", &[M1RuntimePhase::Disconnected])?;
        let identity = probe_m1_identity(&mut self.io)?;
        self.identity = Some(identity);
        let status = read_m1_status(&mut self.io)?;
        self.apply_unowned_status(status)?;
        Ok(self.snapshot())
    }

    pub fn upload(&mut self, job: &M1CompiledJob) -> Result<M1RuntimeSnapshot, M1RuntimeError> {
        self.require_phase(
            "upload a job",
            &[
                M1RuntimePhase::Ready,
                M1RuntimePhase::Completed,
                M1RuntimePhase::Stopped,
            ],
        )?;
        let tool_path = "/setprintToolType?type=Laser";
        let response = self
            .io
            .post(tool_path, &[], "application/x-www-form-urlencoded")
            .map_err(M1ProtocolError::Transport)?;
        require_ok_response(response, tool_path)?;

        let upload_path = "/cnc/data?action=upload&zip=true&id=-1";
        let response = match self.io.post(
            upload_path,
            &job.archive,
            "application/x-www-form-urlencoded",
        ) {
            Ok(response) => response,
            Err(error) => {
                self.mark_recovery(format!(
                    "job upload result is ambiguous and was not retried: {error}"
                ));
                return Err(M1ProtocolError::Transport(error).into());
            }
        };
        if let Err(error) = require_ok_response(response, upload_path) {
            self.mark_recovery(format!("job upload was not confirmed: {error}"));
            return Err(error.into());
        }
        self.phase = M1RuntimePhase::ReadyToRun;
        self.status = Some(M1Status::ReadyToRun);
        Ok(self.snapshot())
    }

    pub fn poll(&mut self) -> Result<M1RuntimeSnapshot, M1RuntimeError> {
        let status = read_m1_status(&mut self.io)?;
        self.apply_status(status)?;
        Ok(self.snapshot())
    }

    pub fn pause(&mut self) -> Result<M1RuntimeSnapshot, M1RuntimeError> {
        self.require_phase("pause", &[M1RuntimePhase::Running])?;
        self.send_action("pause")?;
        self.phase = M1RuntimePhase::Paused;
        self.status = Some(M1Status::Paused);
        Ok(self.snapshot())
    }

    pub fn resume(&mut self) -> Result<M1RuntimeSnapshot, M1RuntimeError> {
        self.require_phase("resume", &[M1RuntimePhase::Paused])?;
        self.send_action("start")?;
        self.phase = M1RuntimePhase::Running;
        self.status = Some(M1Status::Working);
        Ok(self.snapshot())
    }

    /// Request a stop and confirm it from the machine's own status. An
    /// accepted request alone does not prove the machine stopped.
    pub fn stop(&mut self) -> Result<M1RuntimeSnapshot, M1RuntimeError> {
        self.require_phase("stop", STOPPABLE_PHASES)?;
        self.send_action("stop")?;
        for attempt in 0..STOP_CONFIRM_ATTEMPTS {
            if attempt > 0 {
                std::thread::sleep(self.stop_confirm_interval);
            }
            let status = read_m1_status(&mut self.io).map_err(|error| {
                self.mark_recovery(format!("stop could not be confirmed: {error}"));
                error
            })?;
            let idle = status.is_idle();
            self.status = Some(status);
            if idle {
                self.phase = M1RuntimePhase::Stopped;
                return Ok(self.snapshot());
            }
        }
        let reported = format!("{:?}", self.status.clone().unwrap_or(M1Status::Working));
        self.mark_recovery(format!(
            "stop was accepted but the machine still reports {reported}"
        ));
        Err(M1RuntimeError::StopUnconfirmed(reported))
    }

    pub fn disconnect(&mut self) {
        self.phase = M1RuntimePhase::Disconnected;
        self.status = None;
    }

    fn send_action(&mut self, action: &'static str) -> Result<(), M1RuntimeError> {
        let path = format!("/cnc/data?action={action}");
        let response = self.io.get(&path).map_err(|error| {
            self.mark_recovery(format!("{action} result is ambiguous: {error}"));
            M1ProtocolError::Transport(error)
        })?;
        require_ok_response(response, &path)?;
        Ok(())
    }

    fn apply_unowned_status(&mut self, status: M1Status) -> Result<(), M1RuntimeError> {
        self.status = Some(status.clone());
        self.phase = if status.is_idle() {
            M1RuntimePhase::Ready
        } else {
            match status {
                M1Status::Unknown(raw) => return Err(M1RuntimeError::UnknownStatus(raw)),
                M1Status::Error => M1RuntimePhase::RecoveryRequired,
                _ => M1RuntimePhase::BusyUnowned,
            }
        };
        Ok(())
    }

    fn apply_status(&mut self, status: M1Status) -> Result<(), M1RuntimeError> {
        self.status = Some(status.clone());
        self.phase = match status {
            M1Status::ReadyToRun if self.phase == M1RuntimePhase::ReadyToRun => {
                M1RuntimePhase::ReadyToRun
            }
            M1Status::Working
                if matches!(
                    self.phase,
                    M1RuntimePhase::ReadyToRun | M1RuntimePhase::Running | M1RuntimePhase::Paused
                ) =>
            {
                M1RuntimePhase::Running
            }
            M1Status::Paused
                if matches!(self.phase, M1RuntimePhase::Running | M1RuntimePhase::Paused) =>
            {
                M1RuntimePhase::Paused
            }
            M1Status::Finished
                if matches!(
                    self.phase,
                    M1RuntimePhase::ReadyToRun | M1RuntimePhase::Running | M1RuntimePhase::Paused
                ) =>
            {
                M1RuntimePhase::Completed
            }
            status
                if status.is_idle()
                    && matches!(
                        self.phase,
                        M1RuntimePhase::ReadyToRun
                            | M1RuntimePhase::Running
                            | M1RuntimePhase::Paused
                    ) =>
            {
                // Some observed firmware reports P_WORK_DONE briefly, while
                // other versions return directly to an idle state.
                M1RuntimePhase::Completed
            }
            M1Status::Error => {
                self.recovery_reason = Some("the machine reported an error".to_string());
                M1RuntimePhase::RecoveryRequired
            }
            M1Status::Unknown(raw) => {
                self.mark_recovery(format!("unknown machine status {raw}"));
                return Err(M1RuntimeError::UnknownStatus(raw));
            }
            status if status.is_idle() && matches!(self.phase, M1RuntimePhase::Stopped) => {
                M1RuntimePhase::Stopped
            }
            status if status.is_idle() && matches!(self.phase, M1RuntimePhase::Completed) => {
                M1RuntimePhase::Completed
            }
            status if status.is_idle() && matches!(self.phase, M1RuntimePhase::Ready) => {
                M1RuntimePhase::Ready
            }
            // Activity after Ready, Stopped or Completed is not ours to assume
            // finished: report it as busy so a later stop is actually sent.
            _ if matches!(
                self.phase,
                M1RuntimePhase::Ready | M1RuntimePhase::Stopped | M1RuntimePhase::Completed
            ) =>
            {
                M1RuntimePhase::BusyUnowned
            }
            _ => self.phase,
        };
        Ok(())
    }

    fn require_phase(
        &self,
        action: &'static str,
        allowed: &[M1RuntimePhase],
    ) -> Result<(), M1RuntimeError> {
        if allowed.contains(&self.phase) {
            Ok(())
        } else {
            Err(M1RuntimeError::InvalidPhase {
                action,
                phase: self.phase,
            })
        }
    }

    fn mark_recovery(&mut self, reason: String) {
        self.phase = M1RuntimePhase::RecoveryRequired;
        self.recovery_reason = Some(reason);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;
    use crate::M1HttpResponse;

    struct ScriptedIo {
        replies: VecDeque<Result<M1HttpResponse, String>>,
        requests: Vec<String>,
    }

    impl M1HttpIo for ScriptedIo {
        fn get(&mut self, path: &str) -> Result<M1HttpResponse, String> {
            self.requests.push(format!("GET {path}"));
            self.replies.pop_front().expect("scripted response")
        }

        fn post(
            &mut self,
            path: &str,
            _body: &[u8],
            _content_type: &str,
        ) -> Result<M1HttpResponse, String> {
            self.requests.push(format!("POST {path}"));
            self.replies.pop_front().expect("scripted response")
        }
    }

    fn ok(body: &str) -> Result<M1HttpResponse, String> {
        Ok(M1HttpResponse {
            status: 200,
            body: body.as_bytes().to_vec(),
        })
    }

    fn running_runtime(replies: Vec<Result<M1HttpResponse, String>>) -> M1Runtime<ScriptedIo> {
        let mut runtime = M1Runtime::new(ScriptedIo {
            replies: VecDeque::from(replies),
            requests: Vec::new(),
        });
        runtime.phase = M1RuntimePhase::Running;
        runtime.stop_confirm_interval = Duration::ZERO;
        runtime
    }

    #[test]
    fn stop_waits_for_an_observed_idle_status() {
        let mut runtime = running_runtime(vec![
            ok(r#"{"result":"ok"}"#),
            ok(r#"{"STATUS":"P_WORKING"}"#),
            ok(r#"{"STATUS":"P_IDLE"}"#),
        ]);
        let snapshot = runtime.stop().unwrap();
        assert_eq!(snapshot.phase, M1RuntimePhase::Stopped);
        assert_eq!(snapshot.status, Some(M1Status::Idle));
        assert_eq!(runtime.io.requests.len(), 3, "stop must poll until idle");
    }

    #[test]
    fn stop_is_unconfirmed_while_the_machine_keeps_working() {
        let mut replies = vec![ok(r#"{"result":"ok"}"#)];
        replies.extend((0..STOP_CONFIRM_ATTEMPTS).map(|_| ok(r#"{"STATUS":"P_WORKING"}"#)));
        let mut runtime = running_runtime(replies);
        assert!(
            runtime.stop().is_err(),
            "a stop the machine ignored must not report success"
        );
        assert_eq!(runtime.snapshot().phase, M1RuntimePhase::RecoveryRequired);
        // Recovery still allows another stop request.
        assert!(runtime.require_phase("stop", STOPPABLE_PHASES).is_ok());
    }

    #[test]
    fn a_stopped_machine_that_reports_working_is_active_again() {
        let mut runtime = running_runtime(vec![ok(r#"{"STATUS":"P_WORKING"}"#)]);
        runtime.phase = M1RuntimePhase::Stopped;
        let snapshot = runtime.poll().unwrap();
        assert_eq!(snapshot.phase, M1RuntimePhase::BusyUnowned);
        assert!(snapshot.output_may_be_active);
    }

    #[test]
    fn upload_waits_for_physical_button_then_tracks_completion() {
        let io = ScriptedIo {
            replies: VecDeque::from([
                ok(r#"{"STATUS":"P_SLEEP"}"#),
                ok("Shop M1"),
                ok("M1-10W"),
                ok(r#"{"package_version":"40.18.026.00"}"#),
                ok(r#"{"result":"10"}"#),
                ok(r#"{"STATUS":"P_SLEEP"}"#),
                ok(r#"{"result":"ok"}"#),
                ok(r#"{"result":"ok"}"#),
                ok(r#"{"STATUS":"P_ONLINE_READY_WORK"}"#),
                ok(r#"{"STATUS":"P_WORKING"}"#),
                ok(r#"{"STATUS":"P_FINISH"}"#),
            ]),
            requests: Vec::new(),
        };
        let mut runtime = M1Runtime::new(io);
        assert_eq!(runtime.connect().unwrap().phase, M1RuntimePhase::Ready);
        let job = M1CompiledJob {
            gcode: "M6 P1\n".to_string(),
            archive: vec![1, 2, 3],
            line_count: 1,
        };
        assert_eq!(
            runtime.upload(&job).unwrap().phase,
            M1RuntimePhase::ReadyToRun
        );
        assert_eq!(runtime.poll().unwrap().phase, M1RuntimePhase::ReadyToRun);
        assert_eq!(runtime.poll().unwrap().phase, M1RuntimePhase::Running);
        assert_eq!(runtime.poll().unwrap().phase, M1RuntimePhase::Completed);
    }

    #[test]
    fn idle_after_running_is_treated_as_completion() {
        let io = ScriptedIo {
            replies: VecDeque::from([ok(r#"{"STATUS":"P_SLEEP"}"#)]),
            requests: Vec::new(),
        };
        let mut runtime = M1Runtime::new(io);
        runtime.phase = M1RuntimePhase::Running;

        let snapshot = runtime.poll().unwrap();

        assert_eq!(snapshot.phase, M1RuntimePhase::Completed);
    }

    #[test]
    fn ambiguous_upload_enters_recovery_without_retry() {
        let io = ScriptedIo {
            replies: VecDeque::from([
                ok(r#"{"STATUS":"P_IDLE"}"#),
                ok("M1"),
                ok("M1"),
                ok(r#"{"package_version":"40.18.024.00"}"#),
                ok(r#"{"result":"5"}"#),
                ok(r#"{"STATUS":"P_IDLE"}"#),
                ok(r#"{"result":"ok"}"#),
                Err("timed out".to_string()),
            ]),
            requests: Vec::new(),
        };
        let mut runtime = M1Runtime::new(io);
        runtime.connect().unwrap();
        let job = M1CompiledJob {
            gcode: String::new(),
            archive: vec![1],
            line_count: 0,
        };
        assert!(runtime.upload(&job).is_err());
        assert_eq!(runtime.phase(), M1RuntimePhase::RecoveryRequired);
        let io = runtime.into_inner();
        assert_eq!(
            io.requests
                .iter()
                .filter(|request| request.contains("action=upload"))
                .count(),
            1
        );
    }
}
