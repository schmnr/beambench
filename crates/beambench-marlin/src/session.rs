//! Live serial-session core for standard Marlin laser firmware.

use std::time::{Duration, Instant};

use beambench_gcode::{
    AckFlowConfig, AckFlowError, AckFlowProgress, AcknowledgedLineFlow, LineProtocolEvent,
    classify_marlin_response,
};
use beambench_serial::{SerialError, SerialTransport};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    MARLIN_FINISH_MOVES_COMMAND, MarlinDialect, MarlinIdentity, MarlinIdentityDetector,
    MarlinIdentityProbeOutcome, MarlinIdentityProbeResult, MarlinIdentityProbeSequence,
    MarlinIdentityStatus,
};

/// Marlin's host-side full-shutdown command.
///
/// This is sent as a normal line, never as a GRBL realtime byte. An activated
/// Beam Bench Marlin session requires `EMERGENCY_PARSER` so firmware can act on
/// the command without waiting for queue space.
pub const MARLIN_CANCEL_COMMAND: &str = "M112";

/// Upper bound on lines sent per tick so a burst of instant acknowledgements
/// cannot hold the service session lock for a whole job.
const MAX_LINES_PER_TICK: usize = 64;

/// Live lifecycle owned by the standard-Marlin serial adapter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarlinSessionState {
    #[default]
    Disconnected,
    Validating,
    Ready,
    Running,
    RecoveryRequired,
    Error,
}

/// Terminal result of the most recent Marlin job attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarlinJobOutcome {
    Completed,
    CancelCommandSentRecoveryRequired,
    FailedRecoveryRequired,
}

/// Timing and line limits for a conservative Marlin serial session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarlinSerialSessionConfig {
    pub acknowledgement_flow: AckFlowConfig,
    pub identity_timeout: Duration,
    pub poll_interval: Duration,
}

impl Default for MarlinSerialSessionConfig {
    fn default() -> Self {
        Self {
            acknowledgement_flow: AckFlowConfig::default(),
            identity_timeout: Duration::from_secs(5),
            poll_interval: Duration::from_millis(5),
        }
    }
}

#[derive(Debug, Error)]
pub enum MarlinSessionError {
    #[error(transparent)]
    Serial(#[from] SerialError),
    #[error(transparent)]
    Acknowledgement(#[from] AckFlowError),
    #[error("cannot {action} while the Marlin session is {state:?}")]
    InvalidState {
        action: &'static str,
        state: MarlinSessionState,
    },
    #[error("the Marlin identity probe has not completed")]
    MissingIdentityProbe,
    #[error("the Marlin identity probe ended with {outcome:?}")]
    IdentityProbeFailed { outcome: MarlinIdentityProbeOutcome },
    #[error("the controller did not provide an exact standard Marlin identity")]
    UnverifiedMarlinIdentity,
    #[error(
        "the controller reports the {dialect:?} Marlin-derived dialect, which requires its dedicated adapter"
    )]
    DedicatedDialectRequired { dialect: MarlinDialect },
    #[error("expected the {expected:?} Marlin-derived dialect, but detected {detected:?}")]
    DialectMismatch {
        expected: MarlinDialect,
        detected: MarlinDialect,
    },
    #[error("the Marlin build must report Cap:EMERGENCY_PARSER:1 before live jobs can run")]
    EmergencyParserRequired,
    #[error("unsafe Marlin job boundary: {reason}")]
    UnsafeJobBoundary { reason: &'static str },
}

/// A standard-Marlin serial session with exact identity activation,
/// one-command-per-ack flow, controller-confirmed `M400` completion, and a
/// fail-closed `M112` cancellation contract.
pub struct MarlinSerialSession {
    transport: Box<dyn SerialTransport>,
    config: MarlinSerialSessionConfig,
    state: MarlinSessionState,
    identity_detector: MarlinIdentityDetector,
    last_identity_probe: Option<MarlinIdentityProbeResult>,
    job_flow: Option<AcknowledgedLineFlow>,
    job_outcome: Option<MarlinJobOutcome>,
}

impl MarlinSerialSession {
    pub fn new(transport: Box<dyn SerialTransport>, config: MarlinSerialSessionConfig) -> Self {
        Self {
            transport,
            config,
            state: MarlinSessionState::Disconnected,
            identity_detector: MarlinIdentityDetector::default(),
            last_identity_probe: None,
            job_flow: None,
            job_outcome: None,
        }
    }

    pub const fn state(&self) -> MarlinSessionState {
        self.state
    }

    pub fn port_name(&self) -> &str {
        self.transport.port_name()
    }

    pub fn identity(&self) -> MarlinIdentity {
        self.identity_detector.identity()
    }

    pub fn last_identity_probe(&self) -> Option<&MarlinIdentityProbeResult> {
        self.last_identity_probe.as_ref()
    }

    pub const fn job_outcome(&self) -> Option<MarlinJobOutcome> {
        self.job_outcome
    }

    pub fn job_progress(&self) -> Option<AckFlowProgress> {
        self.job_flow.as_ref().map(AcknowledgedLineFlow::progress)
    }

    pub fn connect(&mut self) -> Result<(), MarlinSessionError> {
        self.require_state("connect", MarlinSessionState::Disconnected)?;
        if let Err(error) = self.transport.open() {
            self.state = MarlinSessionState::Error;
            return Err(error.into());
        }
        self.identity_detector = MarlinIdentityDetector::default();
        self.last_identity_probe = None;
        self.job_flow = None;
        self.job_outcome = None;
        self.state = MarlinSessionState::Validating;
        Ok(())
    }

    /// Run the bounded, read-only `M115` identity sequence.
    pub fn probe_identity(&mut self) -> Result<MarlinIdentityProbeResult, MarlinSessionError> {
        self.require_state("probe identity", MarlinSessionState::Validating)?;
        self.identity_detector = MarlinIdentityDetector::default();
        self.last_identity_probe = None;

        let result = self.probe_identity_inner();
        if result.is_err() {
            self.state = MarlinSessionState::Error;
        }
        result
    }

    fn probe_identity_inner(&mut self) -> Result<MarlinIdentityProbeResult, MarlinSessionError> {
        let mut sequence = MarlinIdentityProbeSequence::default();
        let command = sequence
            .begin()
            .expect("a new identity sequence must have one initial command");
        self.transport.write_line(command)?;
        let started = Instant::now();

        while !sequence.is_complete() {
            let mut received_line = false;
            while let Some(line) = self.transport.read_line()? {
                received_line = true;
                self.identity_detector.observe_line(&line);
                sequence.observe(&classify_marlin_response(&line));
                if sequence.is_complete() {
                    break;
                }
            }

            if sequence.is_complete() {
                break;
            }
            if started.elapsed() >= self.config.identity_timeout {
                sequence.timeout();
                break;
            }
            if !received_line && !self.config.poll_interval.is_zero() {
                std::thread::sleep(self.config.poll_interval);
            }
        }

        let result = sequence
            .finish(self.identity())
            .expect("identity sequence must be terminal before returning");
        self.last_identity_probe = Some(result.clone());
        Ok(result)
    }

    /// Activate only the exact standard-Marlin contract. Recognized vendor
    /// dialects remain available to their own adapters instead of inheriting
    /// generic Marlin commands.
    pub fn activate(&mut self) -> Result<(), MarlinSessionError> {
        self.activate_dialect(MarlinDialect::Generic, true)
    }

    /// Activate the exact Snapmaker 2.0 dialect through its dedicated command
    /// contract. Snapmaker's documented `M115` response reports its emergency
    /// parser disabled, so cancellation remains best-effort `M112` followed by
    /// mandatory reconnect instead of claiming an immediate physical stop.
    pub fn activate_snapmaker(&mut self) -> Result<(), MarlinSessionError> {
        self.activate_dialect(MarlinDialect::Snapmaker, false)
    }

    fn activate_dialect(
        &mut self,
        expected_dialect: MarlinDialect,
        require_emergency_parser: bool,
    ) -> Result<(), MarlinSessionError> {
        self.require_state("activate", MarlinSessionState::Validating)?;
        let probe = self
            .last_identity_probe
            .as_ref()
            .ok_or(MarlinSessionError::MissingIdentityProbe)?;
        if probe.outcome != MarlinIdentityProbeOutcome::Succeeded {
            return Err(MarlinSessionError::IdentityProbeFailed {
                outcome: probe.outcome,
            });
        }

        let identity = &probe.identity;
        if identity.status != MarlinIdentityStatus::Identified
            || identity.firmware_identity.as_deref() != Some("Marlin")
        {
            return Err(MarlinSessionError::UnverifiedMarlinIdentity);
        }
        if identity.dialect != expected_dialect {
            if expected_dialect == MarlinDialect::Generic
                && identity.dialect == MarlinDialect::Snapmaker
            {
                return Err(MarlinSessionError::DedicatedDialectRequired {
                    dialect: identity.dialect,
                });
            }
            return Err(MarlinSessionError::DialectMismatch {
                expected: expected_dialect,
                detected: identity.dialect,
            });
        }
        if require_emergency_parser && identity.capabilities.get("EMERGENCY_PARSER") != Some(&true)
        {
            return Err(MarlinSessionError::EmergencyParserRequired);
        }

        self.state = MarlinSessionState::Ready;
        Ok(())
    }

    pub fn start_job(&mut self, commands: Vec<String>) -> Result<(), MarlinSessionError> {
        self.require_state("start a job", MarlinSessionState::Ready)?;
        validate_job_completion_contract(&commands)?;
        self.job_flow = Some(AcknowledgedLineFlow::new(
            commands,
            self.config.acknowledgement_flow,
        )?);
        self.job_outcome = None;
        self.state = MarlinSessionState::Running;
        Ok(())
    }

    /// Advance one live job tick. Sends stay acknowledgement-gated (one line
    /// in flight), but the tick pumps as many rounds as buffered
    /// acknowledgements allow so throughput is bounded by the controller's
    /// ack latency, not the service tick cadence. The tick never blocks
    /// waiting for the controller.
    pub fn tick(&mut self, now: Instant) -> Result<Vec<LineProtocolEvent>, MarlinSessionError> {
        self.require_state("advance a job", MarlinSessionState::Running)?;
        let result = self.tick_inner(now);
        if result.is_err() {
            self.state = MarlinSessionState::RecoveryRequired;
            self.job_outcome = Some(MarlinJobOutcome::FailedRecoveryRequired);
        }
        result
    }

    fn tick_inner(&mut self, now: Instant) -> Result<Vec<LineProtocolEvent>, MarlinSessionError> {
        let flow = self
            .job_flow
            .as_mut()
            .expect("running Marlin session must own a job flow");
        flow.check_timeout(now)?;

        let mut events = Vec::new();
        let mut sent = 0;
        loop {
            // Ack-gated send: ready_line is None while a line is in flight.
            let mut progressed = false;
            if sent < MAX_LINES_PER_TICK
                && let Some(line) = flow.ready_line().map(|ready| ready.line.to_string())
            {
                self.transport.write_line(&line)?;
                flow.mark_sent(now)?;
                sent += 1;
                progressed = true;
            }
            while let Some(line) = self.transport.read_line()? {
                let event = classify_marlin_response(&line);
                flow.observe(&event, now)?;
                events.push(event);
                progressed = true;
            }
            // Pump until the job completes or this tick can make no further
            // progress (awaiting an acknowledgement that has not arrived).
            if flow.is_complete() || !progressed {
                break;
            }
        }

        if flow.is_complete() {
            self.state = MarlinSessionState::Ready;
            self.job_outcome = Some(MarlinJobOutcome::Completed);
        }
        Ok(events)
    }

    /// Request Marlin's immediate full shutdown and require a reconnect before
    /// any further command. Sending the command is not reported as a confirmed
    /// physical emergency stop.
    pub fn cancel_job(&mut self) -> Result<(), MarlinSessionError> {
        self.require_state("cancel a job", MarlinSessionState::Running)?;
        self.emergency_shutdown()
    }

    /// Send Marlin's full-shutdown command from either Ready or Running and
    /// require reconnect/recovery before any further command.
    pub fn emergency_shutdown(&mut self) -> Result<(), MarlinSessionError> {
        if !matches!(
            self.state,
            MarlinSessionState::Ready | MarlinSessionState::Running
        ) {
            return Err(MarlinSessionError::InvalidState {
                action: "request emergency shutdown",
                state: self.state,
            });
        }
        if let Some(flow) = self.job_flow.as_mut() {
            flow.cancel();
        }
        self.state = MarlinSessionState::RecoveryRequired;

        match self.transport.write_line(MARLIN_CANCEL_COMMAND) {
            Ok(()) => {
                self.job_outcome = Some(MarlinJobOutcome::CancelCommandSentRecoveryRequired);
                Ok(())
            }
            Err(error) => {
                self.job_outcome = Some(MarlinJobOutcome::FailedRecoveryRequired);
                Err(error.into())
            }
        }
    }

    /// Close a non-running session. A running job must first take the explicit
    /// cancellation path so closing a serial port never masquerades as a stop.
    pub fn disconnect(&mut self) -> Result<(), MarlinSessionError> {
        if self.state == MarlinSessionState::Running {
            return Err(MarlinSessionError::InvalidState {
                action: "disconnect before cancelling the active job",
                state: self.state,
            });
        }
        if self.transport.is_open() {
            self.transport.close()?;
        }
        self.identity_detector = MarlinIdentityDetector::default();
        self.last_identity_probe = None;
        self.job_flow = None;
        self.state = MarlinSessionState::Disconnected;
        Ok(())
    }

    fn require_state(
        &self,
        action: &'static str,
        expected: MarlinSessionState,
    ) -> Result<(), MarlinSessionError> {
        if self.state == expected {
            return Ok(());
        }
        Err(MarlinSessionError::InvalidState {
            action,
            state: self.state,
        })
    }
}

fn validate_job_completion_contract(commands: &[String]) -> Result<(), MarlinSessionError> {
    if commands.last().map(|line| line.trim()) != Some(MARLIN_FINISH_MOVES_COMMAND) {
        return Err(MarlinSessionError::UnsafeJobBoundary {
            reason: "M400 must be the final command",
        });
    }

    let before_barrier = &commands[..commands.len() - 1];
    let Some(final_off_index) = before_barrier
        .iter()
        .rposition(|line| matches!(line.trim(), "M5" | "M5 I"))
    else {
        return Err(MarlinSessionError::UnsafeJobBoundary {
            reason: "a final M5 or M5 I is required before M400",
        });
    };

    if before_barrier[final_off_index + 1..]
        .iter()
        .any(|line| is_laser_activation(line))
    {
        return Err(MarlinSessionError::UnsafeJobBoundary {
            reason: "laser activation appears after the final laser-off command",
        });
    }
    Ok(())
}

fn is_laser_activation(line: &str) -> bool {
    let command = line.split_ascii_whitespace().next().unwrap_or_default();
    command.eq_ignore_ascii_case("M3")
        || command.eq_ignore_ascii_case("M4")
        || command.eq_ignore_ascii_case("M3I")
        || command.eq_ignore_ascii_case("M4I")
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use beambench_serial::SerialTransport;

    use super::*;

    #[derive(Debug, Default)]
    struct ScriptState {
        rx: VecDeque<String>,
        tx: Vec<String>,
        auto_ack_jobs: bool,
    }

    #[derive(Clone)]
    struct ScriptHandle {
        state: Arc<Mutex<ScriptState>>,
    }

    impl ScriptHandle {
        fn sent_lines(&self) -> Vec<String> {
            self.state.lock().unwrap().tx.clone()
        }
    }

    struct ScriptedTransport {
        open: bool,
        state: Arc<Mutex<ScriptState>>,
        identity_lines: Vec<String>,
    }

    impl ScriptedTransport {
        fn new(identity_lines: &[&str], auto_ack_jobs: bool) -> (Self, ScriptHandle) {
            let state = Arc::new(Mutex::new(ScriptState {
                auto_ack_jobs,
                ..ScriptState::default()
            }));
            (
                Self {
                    open: false,
                    state: Arc::clone(&state),
                    identity_lines: identity_lines
                        .iter()
                        .map(|line| (*line).to_string())
                        .collect(),
                },
                ScriptHandle { state },
            )
        }
    }

    impl SerialTransport for ScriptedTransport {
        fn open(&mut self) -> Result<(), SerialError> {
            self.open = true;
            Ok(())
        }

        fn close(&mut self) -> Result<(), SerialError> {
            self.open = false;
            Ok(())
        }

        fn is_open(&self) -> bool {
            self.open
        }

        fn write_bytes(&mut self, data: &[u8]) -> Result<usize, SerialError> {
            if !self.open {
                return Err(SerialError::NotOpen);
            }
            Ok(data.len())
        }

        fn write_line(&mut self, line: &str) -> Result<(), SerialError> {
            if !self.open {
                return Err(SerialError::NotOpen);
            }
            let mut state = self.state.lock().unwrap();
            state.tx.push(line.to_string());
            if line == "M115" {
                state.rx.extend(self.identity_lines.iter().cloned());
            } else if line != MARLIN_CANCEL_COMMAND && state.auto_ack_jobs {
                state.rx.push_back("ok".to_string());
            }
            Ok(())
        }

        fn read_available(&mut self) -> Result<Vec<u8>, SerialError> {
            if !self.open {
                return Err(SerialError::NotOpen);
            }
            let mut state = self.state.lock().unwrap();
            let mut data = Vec::new();
            while let Some(line) = state.rx.pop_front() {
                data.extend_from_slice(line.as_bytes());
                data.push(b'\n');
            }
            Ok(data)
        }

        fn read_line(&mut self) -> Result<Option<String>, SerialError> {
            if !self.open {
                return Err(SerialError::NotOpen);
            }
            Ok(self.state.lock().unwrap().rx.pop_front())
        }

        fn flush(&mut self) -> Result<(), SerialError> {
            if !self.open {
                return Err(SerialError::NotOpen);
            }
            Ok(())
        }

        fn port_name(&self) -> &str {
            "scripted-marlin"
        }
    }

    fn standard_identity(emergency_parser: bool) -> Vec<String> {
        vec![
            "FIRMWARE_NAME:Marlin 2.1.3 SOURCE_CODE_URL:github.com/MarlinFirmware/Marlin PROTOCOL_VERSION:1.0 MACHINE_TYPE:Laser Cutter".to_string(),
            format!(
                "Cap:EMERGENCY_PARSER:{}",
                if emergency_parser { 1 } else { 0 }
            ),
            "ok".to_string(),
        ]
    }

    fn snapmaker_identity() -> Vec<String> {
        vec![
            "FIRMWARE_NAME:Marlin SM2-4.7.2 SOURCE_CODE_URL:https://github.com/whimsycwd/SnapmakerMarlin PROTOCOL_VERSION:1.0 MACHINE_TYPE:GD32F305VGT6".to_string(),
            "Cap:EMERGENCY_PARSER:0".to_string(),
            "ok".to_string(),
        ]
    }

    fn session(
        identity_lines: &[String],
        auto_ack_jobs: bool,
    ) -> (MarlinSerialSession, ScriptHandle) {
        let refs: Vec<_> = identity_lines.iter().map(String::as_str).collect();
        let (transport, handle) = ScriptedTransport::new(&refs, auto_ack_jobs);
        let config = MarlinSerialSessionConfig {
            acknowledgement_flow: AckFlowConfig {
                max_line_bytes: 256,
                acknowledgement_timeout: Duration::from_secs(1),
            },
            identity_timeout: Duration::from_millis(50),
            poll_interval: Duration::ZERO,
        };
        (
            MarlinSerialSession::new(Box::new(transport), config),
            handle,
        )
    }

    fn connect_and_activate(session: &mut MarlinSerialSession) {
        session.connect().unwrap();
        let probe = session.probe_identity().unwrap();
        assert_eq!(probe.outcome, MarlinIdentityProbeOutcome::Succeeded);
        session.activate().unwrap();
    }

    #[test]
    fn exact_standard_marlin_runs_one_line_per_ack_to_a_final_m400() {
        let (mut session, handle) = session(&standard_identity(true), true);
        connect_and_activate(&mut session);
        session
            .start_job(vec![
                "M5".to_string(),
                "G0 X10 Y10".to_string(),
                "M400".to_string(),
            ])
            .unwrap();

        // Every write is acknowledged synchronously, so the ack-gated pump
        // streams the whole job in one tick while preserving strict ordering.
        session.tick(Instant::now()).unwrap();

        assert_eq!(session.state(), MarlinSessionState::Ready);
        assert_eq!(session.job_outcome(), Some(MarlinJobOutcome::Completed));
        assert_eq!(session.job_progress().unwrap().acknowledged_lines, 3);
        assert_eq!(handle.sent_lines(), ["M115", "M5", "G0 X10 Y10", "M400"]);
    }

    #[test]
    fn tick_pumps_every_buffered_acknowledgement_instead_of_one_line_per_tick() {
        let (mut session, handle) = session(&standard_identity(true), true);
        connect_and_activate(&mut session);
        let mut commands: Vec<String> = (0..30).map(|i| format!("G1 X{i}")).collect();
        commands.push("M5".to_string());
        commands.push("M400".to_string());
        session.start_job(commands).unwrap();

        // Every write is acknowledged synchronously by the transport, so a
        // single tick must stream the whole job. One-line-per-tick throttling
        // capped real jobs at the service tick cadence (~20 lines/s), starving
        // the motion planner with the laser energized at every dwell.
        session.tick(Instant::now()).unwrap();

        assert_eq!(session.state(), MarlinSessionState::Ready);
        assert_eq!(session.job_outcome(), Some(MarlinJobOutcome::Completed));
        assert_eq!(handle.sent_lines().len(), 33); // M115 + 30 moves + M5 + M400
    }

    #[test]
    fn cancellation_sends_m112_even_with_a_command_in_flight() {
        let (mut session, handle) = session(&standard_identity(true), false);
        connect_and_activate(&mut session);
        session
            .start_job(vec!["M5".to_string(), "M400".to_string()])
            .unwrap();
        session.tick(Instant::now()).unwrap();

        session.cancel_job().unwrap();

        assert_eq!(session.state(), MarlinSessionState::RecoveryRequired);
        assert_eq!(
            session.job_outcome(),
            Some(MarlinJobOutcome::CancelCommandSentRecoveryRequired)
        );
        assert_eq!(handle.sent_lines(), ["M115", "M5", "M112"]);
        assert!(session.start_job(vec![]).is_err());
    }

    #[test]
    fn emergency_shutdown_is_available_before_a_job_starts() {
        let (mut session, handle) = session(&standard_identity(true), true);
        connect_and_activate(&mut session);

        session.emergency_shutdown().unwrap();

        assert_eq!(session.state(), MarlinSessionState::RecoveryRequired);
        assert_eq!(handle.sent_lines(), ["M115", "M112"]);
    }

    #[test]
    fn missing_emergency_parser_cannot_activate_live_jobs() {
        let (mut session, _) = session(&standard_identity(false), true);
        session.connect().unwrap();
        session.probe_identity().unwrap();

        assert!(matches!(
            session.activate(),
            Err(MarlinSessionError::EmergencyParserRequired)
        ));
        assert_eq!(session.state(), MarlinSessionState::Validating);
    }

    #[test]
    fn snapmaker_identity_is_reserved_for_its_dedicated_adapter() {
        let lines = snapmaker_identity();
        let (mut session, _) = session(&lines, true);
        session.connect().unwrap();
        session.probe_identity().unwrap();

        assert!(matches!(
            session.activate(),
            Err(MarlinSessionError::DedicatedDialectRequired {
                dialect: MarlinDialect::Snapmaker
            })
        ));
    }

    #[test]
    fn exact_snapmaker_2_activates_without_claiming_an_emergency_parser() {
        let (mut session, handle) = session(&snapmaker_identity(), false);
        session.connect().unwrap();
        let probe = session.probe_identity().unwrap();
        assert_eq!(probe.identity.dialect, MarlinDialect::Snapmaker);
        session.activate_snapmaker().unwrap();
        session
            .start_job(vec!["M5".to_string(), "M400".to_string()])
            .unwrap();
        session.tick(Instant::now()).unwrap();

        session.cancel_job().unwrap();

        assert_eq!(session.state(), MarlinSessionState::RecoveryRequired);
        assert_eq!(handle.sent_lines(), ["M115", "M5", "M112"]);
    }

    #[test]
    fn jobs_without_a_terminal_off_and_completion_barrier_are_rejected() {
        let (mut session, _) = session(&standard_identity(true), true);
        connect_and_activate(&mut session);

        assert!(matches!(
            session.start_job(vec!["M5".to_string(), "G0 X0".to_string()]),
            Err(MarlinSessionError::UnsafeJobBoundary { .. })
        ));
        assert!(matches!(
            session.start_job(vec!["M3 S10".to_string(), "M400".to_string()]),
            Err(MarlinSessionError::UnsafeJobBoundary { .. })
        ));
        assert!(matches!(
            session.start_job(vec![
                "M5".to_string(),
                "M3 S10".to_string(),
                "M400".to_string(),
            ]),
            Err(MarlinSessionError::UnsafeJobBoundary { .. })
        ));
    }

    #[test]
    fn acknowledgement_timeout_requires_recovery() {
        let (mut session, _) = session(&standard_identity(true), false);
        connect_and_activate(&mut session);
        session
            .start_job(vec!["M5".to_string(), "M400".to_string()])
            .unwrap();
        let started = Instant::now();
        session.tick(started).unwrap();

        assert!(matches!(
            session.tick(started + Duration::from_secs(1)),
            Err(MarlinSessionError::Acknowledgement(
                AckFlowError::AcknowledgementTimeout { index: 0 }
            ))
        ));
        assert_eq!(session.state(), MarlinSessionState::RecoveryRequired);
        assert_eq!(
            session.job_outcome(),
            Some(MarlinJobOutcome::FailedRecoveryRequired)
        );
    }
}
