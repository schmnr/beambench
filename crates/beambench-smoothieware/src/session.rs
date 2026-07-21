//! Live serial-session core for Smoothieware laser firmware.

use std::time::{Duration, Instant};

use beambench_gcode::{
    AckFlowConfig, AckFlowError, AckFlowProgress, AcknowledgedLineFlow, LineProtocolEvent,
    classify_smoothieware_response,
};
use beambench_serial::{SerialError, SerialTransport};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    SMOOTHIEWARE_CANCEL_COMMAND, SMOOTHIEWARE_FINISH_MOVES_COMMAND, SmoothiewareIdentity,
    SmoothiewareIdentityDetector, SmoothiewareIdentityProbeOutcome,
    SmoothiewareIdentityProbeResult, SmoothiewareIdentityProbeSequence, SmoothiewareIdentityStatus,
};

/// Upper bound on lines sent per tick so a burst of instant acknowledgements
/// cannot hold the service session lock for a whole job.
const MAX_LINES_PER_TICK: usize = 64;

pub const SMOOTHIEWARE_LASER_ENABLE_QUERY: &str = "M1000 config-get laser_module_enable";
pub const SMOOTHIEWARE_MAXIMUM_S_QUERY: &str = "M1000 config-get laser_module_maximum_s_value";
pub const SMOOTHIEWARE_PROPORTIONAL_POWER_QUERY: &str =
    "M1000 config-get laser_module_proportional_power";

const LASER_CONFIGURATION_QUERIES: [&str; 3] = [
    SMOOTHIEWARE_LASER_ENABLE_QUERY,
    SMOOTHIEWARE_MAXIMUM_S_QUERY,
    SMOOTHIEWARE_PROPORTIONAL_POWER_QUERY,
];

/// Live lifecycle owned by the Smoothieware serial adapter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmoothiewareSessionState {
    #[default]
    Disconnected,
    Validating,
    Ready,
    Running,
    RecoveryRequired,
    Error,
}

/// Terminal result of the most recent Smoothieware job attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmoothiewareJobOutcome {
    Completed,
    CancelCommandSentRecoveryRequired,
    FailedRecoveryRequired,
}

/// Timing and line limits for a conservative Smoothieware serial session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmoothiewareSerialSessionConfig {
    pub acknowledgement_flow: AckFlowConfig,
    pub probe_timeout: Duration,
    pub poll_interval: Duration,
}

impl Default for SmoothiewareSerialSessionConfig {
    fn default() -> Self {
        Self {
            acknowledgement_flow: AckFlowConfig::default(),
            probe_timeout: Duration::from_secs(5),
            poll_interval: Duration::from_millis(5),
        }
    }
}

#[derive(Debug, Error)]
pub enum SmoothiewareSessionError {
    #[error(transparent)]
    Serial(#[from] SerialError),
    #[error(transparent)]
    Acknowledgement(#[from] AckFlowError),
    #[error("cannot {action} while the Smoothieware session is {state:?}")]
    InvalidState {
        action: &'static str,
        state: SmoothiewareSessionState,
    },
    #[error("the Smoothieware identity probe has not completed")]
    MissingIdentityProbe,
    #[error("the Smoothieware identity probe ended with {outcome:?}")]
    IdentityProbeFailed {
        outcome: SmoothiewareIdentityProbeOutcome,
    },
    #[error("the controller did not provide an exact Smoothieware identity")]
    UnverifiedSmoothiewareIdentity,
    #[error("Smoothieware rejected the configuration query `{command}`: {message}")]
    ConfigurationProbeRejected {
        command: &'static str,
        message: String,
    },
    #[error("Smoothieware did not acknowledge the configuration query `{command}`")]
    ConfigurationProbeTimedOut { command: &'static str },
    #[error("the effective Smoothieware configuration does not enable the laser module")]
    LaserModuleRequired,
    #[error("the effective Smoothieware laser power configuration could not be verified")]
    UnverifiedLaserPowerConfiguration,
    #[error("unsafe Smoothieware job boundary: {reason}")]
    UnsafeJobBoundary { reason: &'static str },
}

/// Smoothieware serial execution with exact firmware/config activation,
/// one-command-per-ack flow, terminal `M400` completion, and reconnect-required
/// `M112` cancellation.
pub struct SmoothiewareSerialSession {
    transport: Box<dyn SerialTransport>,
    config: SmoothiewareSerialSessionConfig,
    state: SmoothiewareSessionState,
    identity_detector: SmoothiewareIdentityDetector,
    last_identity_probe: Option<SmoothiewareIdentityProbeResult>,
    job_flow: Option<AcknowledgedLineFlow>,
    job_outcome: Option<SmoothiewareJobOutcome>,
}

impl SmoothiewareSerialSession {
    pub fn new(
        transport: Box<dyn SerialTransport>,
        config: SmoothiewareSerialSessionConfig,
    ) -> Self {
        Self {
            transport,
            config,
            state: SmoothiewareSessionState::Disconnected,
            identity_detector: SmoothiewareIdentityDetector::default(),
            last_identity_probe: None,
            job_flow: None,
            job_outcome: None,
        }
    }

    pub const fn state(&self) -> SmoothiewareSessionState {
        self.state
    }

    pub fn port_name(&self) -> &str {
        self.transport.port_name()
    }

    pub fn identity(&self) -> SmoothiewareIdentity {
        self.identity_detector.identity()
    }

    pub fn last_identity_probe(&self) -> Option<&SmoothiewareIdentityProbeResult> {
        self.last_identity_probe.as_ref()
    }

    pub const fn job_outcome(&self) -> Option<SmoothiewareJobOutcome> {
        self.job_outcome
    }

    pub fn job_progress(&self) -> Option<AckFlowProgress> {
        self.job_flow.as_ref().map(AcknowledgedLineFlow::progress)
    }

    pub fn connect(&mut self) -> Result<(), SmoothiewareSessionError> {
        self.require_state("connect", SmoothiewareSessionState::Disconnected)?;
        if let Err(error) = self.transport.open() {
            self.state = SmoothiewareSessionState::Error;
            return Err(error.into());
        }
        self.identity_detector = SmoothiewareIdentityDetector::default();
        self.last_identity_probe = None;
        self.job_flow = None;
        self.job_outcome = None;
        self.state = SmoothiewareSessionState::Validating;
        Ok(())
    }

    /// Run the bounded read-only `M115` identity and effective laser-config probes.
    pub fn probe_identity(
        &mut self,
    ) -> Result<SmoothiewareIdentityProbeResult, SmoothiewareSessionError> {
        self.require_state("probe identity", SmoothiewareSessionState::Validating)?;
        self.identity_detector = SmoothiewareIdentityDetector::default();
        self.last_identity_probe = None;

        let result = self.probe_identity_inner();
        if result.is_err() {
            self.state = SmoothiewareSessionState::Error;
        }
        result
    }

    fn probe_identity_inner(
        &mut self,
    ) -> Result<SmoothiewareIdentityProbeResult, SmoothiewareSessionError> {
        let mut sequence = SmoothiewareIdentityProbeSequence::default();
        let command = sequence
            .begin()
            .expect("a new identity sequence must have one initial command");
        self.transport.write_line(command)?;
        let started = Instant::now();

        while !sequence.is_complete() {
            let received_line = self.observe_available_identity_lines(&mut sequence)?;
            if sequence.is_complete() {
                break;
            }
            if started.elapsed() >= self.config.probe_timeout {
                sequence.timeout();
                break;
            }
            self.pause_if_idle(received_line);
        }

        let preliminary = sequence
            .finish(self.identity())
            .expect("identity sequence must be terminal before returning");
        if preliminary.outcome == SmoothiewareIdentityProbeOutcome::Succeeded
            && preliminary.identity.status == SmoothiewareIdentityStatus::Identified
        {
            for command in LASER_CONFIGURATION_QUERIES {
                self.probe_configuration(command)?;
            }
        }

        let result = SmoothiewareIdentityProbeResult {
            identity: self.identity(),
            outcome: preliminary.outcome,
        };
        self.last_identity_probe = Some(result.clone());
        Ok(result)
    }

    fn observe_available_identity_lines(
        &mut self,
        sequence: &mut SmoothiewareIdentityProbeSequence,
    ) -> Result<bool, SmoothiewareSessionError> {
        let mut received_line = false;
        while let Some(line) = self.transport.read_line()? {
            received_line = true;
            self.identity_detector.observe_line(&line);
            sequence.observe(&classify_smoothieware_response(&line));
            if sequence.is_complete() {
                break;
            }
        }
        Ok(received_line)
    }

    fn probe_configuration(
        &mut self,
        command: &'static str,
    ) -> Result<(), SmoothiewareSessionError> {
        self.transport.write_line(command)?;
        let started = Instant::now();
        loop {
            let mut received_line = false;
            while let Some(line) = self.transport.read_line()? {
                received_line = true;
                self.identity_detector.observe_line(&line);
                match classify_smoothieware_response(&line) {
                    LineProtocolEvent::Acknowledged { .. } => return Ok(()),
                    LineProtocolEvent::CommandError { message }
                    | LineProtocolEvent::RetryRequested { message, .. } => {
                        return Err(SmoothiewareSessionError::ConfigurationProbeRejected {
                            command,
                            message,
                        });
                    }
                    LineProtocolEvent::Busy { .. } | LineProtocolEvent::Informational { .. } => {}
                }
            }
            if started.elapsed() >= self.config.probe_timeout {
                return Err(SmoothiewareSessionError::ConfigurationProbeTimedOut { command });
            }
            self.pause_if_idle(received_line);
        }
    }

    fn pause_if_idle(&self, received_line: bool) {
        if !received_line && !self.config.poll_interval.is_zero() {
            std::thread::sleep(self.config.poll_interval);
        }
    }

    pub fn activate(&mut self) -> Result<(), SmoothiewareSessionError> {
        self.require_state("activate", SmoothiewareSessionState::Validating)?;
        let probe = self
            .last_identity_probe
            .as_ref()
            .ok_or(SmoothiewareSessionError::MissingIdentityProbe)?;
        if probe.outcome != SmoothiewareIdentityProbeOutcome::Succeeded {
            return Err(SmoothiewareSessionError::IdentityProbeFailed {
                outcome: probe.outcome,
            });
        }
        if probe.identity.status != SmoothiewareIdentityStatus::Identified
            || probe.identity.firmware_identity.as_deref() != Some("Smoothieware")
        {
            return Err(SmoothiewareSessionError::UnverifiedSmoothiewareIdentity);
        }
        if probe.identity.laser_module_enabled != Some(true) {
            return Err(SmoothiewareSessionError::LaserModuleRequired);
        }
        if probe.identity.laser_maximum_s_value.is_none()
            || probe.identity.laser_proportional_power.is_none()
        {
            return Err(SmoothiewareSessionError::UnverifiedLaserPowerConfiguration);
        }

        self.state = SmoothiewareSessionState::Ready;
        Ok(())
    }

    pub fn start_job(&mut self, commands: Vec<String>) -> Result<(), SmoothiewareSessionError> {
        self.require_state("start a job", SmoothiewareSessionState::Ready)?;
        validate_job_completion_contract(&commands)?;
        self.job_flow = Some(AcknowledgedLineFlow::new(
            commands,
            self.config.acknowledgement_flow,
        )?);
        self.job_outcome = None;
        self.state = SmoothiewareSessionState::Running;
        Ok(())
    }

    /// Advance one live job tick. At most one new command is written, and no
    /// later command becomes eligible until its predecessor is acknowledged.
    pub fn tick(
        &mut self,
        now: Instant,
    ) -> Result<Vec<LineProtocolEvent>, SmoothiewareSessionError> {
        self.require_state("advance a job", SmoothiewareSessionState::Running)?;
        let result = self.tick_inner(now);
        if result.is_err() {
            self.state = SmoothiewareSessionState::RecoveryRequired;
            self.job_outcome = Some(SmoothiewareJobOutcome::FailedRecoveryRequired);
        }
        result
    }

    fn tick_inner(
        &mut self,
        now: Instant,
    ) -> Result<Vec<LineProtocolEvent>, SmoothiewareSessionError> {
        let flow = self
            .job_flow
            .as_mut()
            .expect("running Smoothieware session must own a job flow");
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
                let event = classify_smoothieware_response(&line);
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
            self.state = SmoothiewareSessionState::Ready;
            self.job_outcome = Some(SmoothiewareJobOutcome::Completed);
        }
        Ok(events)
    }

    pub fn cancel_job(&mut self) -> Result<(), SmoothiewareSessionError> {
        self.require_state("cancel a job", SmoothiewareSessionState::Running)?;
        self.emergency_shutdown()
    }

    /// Send Smoothieware's normal-line emergency-stop command and require a
    /// reset/reconnect before any further command. A successful write is not
    /// reported as a controller-confirmed physical stop.
    pub fn emergency_shutdown(&mut self) -> Result<(), SmoothiewareSessionError> {
        if !matches!(
            self.state,
            SmoothiewareSessionState::Ready | SmoothiewareSessionState::Running
        ) {
            return Err(SmoothiewareSessionError::InvalidState {
                action: "request emergency shutdown",
                state: self.state,
            });
        }
        if let Some(flow) = self.job_flow.as_mut() {
            flow.cancel();
        }
        self.state = SmoothiewareSessionState::RecoveryRequired;

        match self.transport.write_line(SMOOTHIEWARE_CANCEL_COMMAND) {
            Ok(()) => {
                self.job_outcome = Some(SmoothiewareJobOutcome::CancelCommandSentRecoveryRequired);
                Ok(())
            }
            Err(error) => {
                self.job_outcome = Some(SmoothiewareJobOutcome::FailedRecoveryRequired);
                Err(error.into())
            }
        }
    }

    pub fn disconnect(&mut self) -> Result<(), SmoothiewareSessionError> {
        if self.state == SmoothiewareSessionState::Running {
            return Err(SmoothiewareSessionError::InvalidState {
                action: "disconnect before cancelling the active job",
                state: self.state,
            });
        }
        if self.transport.is_open() {
            self.transport.close()?;
        }
        self.identity_detector = SmoothiewareIdentityDetector::default();
        self.last_identity_probe = None;
        self.job_flow = None;
        self.state = SmoothiewareSessionState::Disconnected;
        Ok(())
    }

    fn require_state(
        &self,
        action: &'static str,
        expected: SmoothiewareSessionState,
    ) -> Result<(), SmoothiewareSessionError> {
        if self.state == expected {
            return Ok(());
        }
        Err(SmoothiewareSessionError::InvalidState {
            action,
            state: self.state,
        })
    }
}

fn validate_job_completion_contract(commands: &[String]) -> Result<(), SmoothiewareSessionError> {
    if commands.last().map(|line| line.trim()) != Some(SMOOTHIEWARE_FINISH_MOVES_COMMAND) {
        return Err(SmoothiewareSessionError::UnsafeJobBoundary {
            reason: "M400 must be the final command",
        });
    }

    for line in commands {
        let uncommented = line.split(';').next().unwrap_or_default().trim();
        if uncommented.is_empty() || uncommented.starts_with('(') {
            continue;
        }
        let mut tokens = uncommented.split_ascii_whitespace();
        let command = tokens.next().unwrap_or_default();
        if matches!(command.to_ascii_uppercase().as_str(), "G1" | "G2" | "G3")
            && !tokens.any(|token| token.to_ascii_uppercase().starts_with('S'))
        {
            return Err(SmoothiewareSessionError::UnsafeJobBoundary {
                reason: "every G1, G2, and G3 move must carry an explicit S value",
            });
        }
        let is_wrapped_fire = command.eq_ignore_ascii_case("M1000")
            && tokens
                .next()
                .is_some_and(|token| token.eq_ignore_ascii_case("fire"));
        if command.eq_ignore_ascii_case("fire") || is_wrapped_fire {
            return Err(SmoothiewareSessionError::UnsafeJobBoundary {
                reason: "manual fire commands are not allowed inside streamed jobs",
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Debug, Default)]
    struct ScriptState {
        rx: VecDeque<String>,
        tx: Vec<String>,
        auto_ack_jobs: bool,
        laser_enabled: bool,
    }

    struct ScriptedTransport {
        open: bool,
        state: Arc<Mutex<ScriptState>>,
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
            match line {
                "M115" => state.rx.extend([
                    "FIRMWARE_NAME:Smoothieware, FIRMWARE_VERSION:edge, PROTOCOL_VERSION:1.0, X-GRBL_MODE:0, X-ARCS:1".to_string(),
                    "ok".to_string(),
                ]),
                SMOOTHIEWARE_LASER_ENABLE_QUERY => {
                    let enabled = state.laser_enabled;
                    state.rx.extend([
                        format!("cached: laser_module_enable is set to {enabled}"),
                        "ok".to_string(),
                    ]);
                }
                SMOOTHIEWARE_MAXIMUM_S_QUERY => state.rx.extend([
                    "cached: laser_module_maximum_s_value is not in config".to_string(),
                    "ok".to_string(),
                ]),
                SMOOTHIEWARE_PROPORTIONAL_POWER_QUERY => state.rx.extend([
                    "cached: laser_module_proportional_power is set to true".to_string(),
                    "ok".to_string(),
                ]),
                SMOOTHIEWARE_CANCEL_COMMAND => {}
                _ if state.auto_ack_jobs => state.rx.push_back("ok".to_string()),
                _ => {}
            }
            Ok(())
        }

        fn read_available(&mut self) -> Result<Vec<u8>, SerialError> {
            if !self.open {
                return Err(SerialError::NotOpen);
            }
            let mut state = self.state.lock().unwrap();
            let mut bytes = Vec::new();
            while let Some(line) = state.rx.pop_front() {
                bytes.extend_from_slice(line.as_bytes());
                bytes.push(b'\n');
            }
            Ok(bytes)
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
            "scripted-smoothieware"
        }
    }

    fn session(
        auto_ack_jobs: bool,
        laser_enabled: bool,
    ) -> (SmoothiewareSerialSession, Arc<Mutex<ScriptState>>) {
        let state = Arc::new(Mutex::new(ScriptState {
            auto_ack_jobs,
            laser_enabled,
            ..ScriptState::default()
        }));
        let transport = ScriptedTransport {
            open: false,
            state: Arc::clone(&state),
        };
        let config = SmoothiewareSerialSessionConfig {
            acknowledgement_flow: AckFlowConfig {
                max_line_bytes: 256,
                acknowledgement_timeout: Duration::from_secs(1),
            },
            probe_timeout: Duration::from_millis(50),
            poll_interval: Duration::ZERO,
        };
        (
            SmoothiewareSerialSession::new(Box::new(transport), config),
            state,
        )
    }

    fn connect_and_activate(session: &mut SmoothiewareSerialSession) {
        session.connect().unwrap();
        let identity = session.probe_identity().unwrap().identity;
        assert_eq!(identity.laser_module_enabled, Some(true));
        assert_eq!(identity.laser_maximum_s_value, Some(1.0));
        session.activate().unwrap();
    }

    #[test]
    fn exact_laser_configuration_runs_one_line_per_ack_to_final_m400() {
        let (mut session, state) = session(true, true);
        connect_and_activate(&mut session);
        session
            .start_job(vec![
                "G90".to_string(),
                "G1 X10 S0.5".to_string(),
                "M400".to_string(),
            ])
            .unwrap();

        // Every write is acknowledged synchronously, so the ack-gated pump
        // streams the whole job in one tick while preserving strict ordering.
        session.tick(Instant::now()).unwrap();
        assert_eq!(session.state(), SmoothiewareSessionState::Ready);
        assert_eq!(
            session.job_outcome(),
            Some(SmoothiewareJobOutcome::Completed)
        );
        assert_eq!(session.job_progress().unwrap().acknowledged_lines, 3);
        assert_eq!(
            state.lock().unwrap().tx,
            [
                "M115",
                SMOOTHIEWARE_LASER_ENABLE_QUERY,
                SMOOTHIEWARE_MAXIMUM_S_QUERY,
                SMOOTHIEWARE_PROPORTIONAL_POWER_QUERY,
                "G90",
                "G1 X10 S0.5",
                "M400"
            ]
        );
    }

    #[test]
    fn disabled_laser_module_cannot_activate() {
        let (mut session, _) = session(true, false);
        session.connect().unwrap();
        session.probe_identity().unwrap();
        assert!(matches!(
            session.activate(),
            Err(SmoothiewareSessionError::LaserModuleRequired)
        ));
    }

    #[test]
    fn cancellation_sends_m112_and_requires_reconnect() {
        let (mut session, state) = session(false, true);
        connect_and_activate(&mut session);
        session
            .start_job(vec!["G1 X10 S0.5".to_string(), "M400".to_string()])
            .unwrap();
        session.tick(Instant::now()).unwrap();
        session.cancel_job().unwrap();

        assert_eq!(session.state(), SmoothiewareSessionState::RecoveryRequired);
        assert_eq!(
            session.job_outcome(),
            Some(SmoothiewareJobOutcome::CancelCommandSentRecoveryRequired)
        );
        assert_eq!(state.lock().unwrap().tx.last().unwrap(), "M112");
    }

    #[test]
    fn unsafe_modal_power_and_missing_completion_are_rejected() {
        let (mut session, _) = session(true, true);
        connect_and_activate(&mut session);
        assert!(matches!(
            session.start_job(vec!["G1 X10".to_string(), "M400".to_string()]),
            Err(SmoothiewareSessionError::UnsafeJobBoundary { .. })
        ));
        assert!(matches!(
            session.start_job(vec!["G1 X10 S0".to_string()]),
            Err(SmoothiewareSessionError::UnsafeJobBoundary { .. })
        ));
        assert!(matches!(
            session.start_job(vec!["fire 10".to_string(), "M400".to_string()]),
            Err(SmoothiewareSessionError::UnsafeJobBoundary { .. })
        ));
        assert!(matches!(
            session.start_job(vec!["M1000 fire 10".to_string(), "M400".to_string()]),
            Err(SmoothiewareSessionError::UnsafeJobBoundary { .. })
        ));
    }
}
