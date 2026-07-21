//! GRBL session manager.
//! Owns transport + state machine + settings + last status.

use crate::commands;
use crate::error::GrblError;
use crate::identity::GrblFamilyIdentityDetector;
use crate::identity_probe::{
    GrblFamilyIdentityProbeConfig, GrblFamilyIdentityProbeResult, GrblFamilyIdentityProbeSequence,
};
use crate::parser::{self, GrblResponse};
use crate::settings::{GrblSettingId, GrblSettings};
use crate::state::SessionStateMachine;
use beambench_common::console::{ConsoleDirection, ConsoleEntry};
use beambench_common::grbl_family::GrblFamilyIdentity;
use beambench_common::machine::{MachineRunState, MachineStatus, SessionState};
use beambench_serial::SerialTransport;
use chrono::Utc;
use std::collections::{HashMap, VecDeque};
use std::time::Instant;
use tracing::{debug, warn};

const CONSOLE_LOG_CAPACITY: usize = 1000;

/// A GRBL session managing communication with a laser controller.
pub struct GrblSession {
    transport: Box<dyn SerialTransport>,
    state_machine: SessionStateMachine,
    settings: GrblSettings,
    controller_info: HashMap<String, String>,
    identity_detector: GrblFamilyIdentityDetector,
    last_identity_probe: Option<GrblFamilyIdentityProbeResult>,
    last_status: MachineStatus,
    console_log: VecDeque<ConsoleEntry>,
    saw_undecodable_data: bool,
    status_report_count: u64,
}

impl GrblSession {
    pub fn new(transport: Box<dyn SerialTransport>) -> Self {
        Self {
            transport,
            state_machine: SessionStateMachine::new(),
            settings: GrblSettings::new(),
            controller_info: HashMap::new(),
            identity_detector: GrblFamilyIdentityDetector::default(),
            last_identity_probe: None,
            last_status: MachineStatus::default(),
            console_log: VecDeque::with_capacity(CONSOLE_LOG_CAPACITY),
            saw_undecodable_data: false,
            status_report_count: 0,
        }
    }

    /// Number of status reports received since connect. Lets callers tell a
    /// FRESH status from a stale `last_status` snapshot (e.g. job completion
    /// must not trust an Idle that predates the job).
    pub fn status_report_count(&self) -> u64 {
        self.status_report_count
    }

    /// Whether any received line contained bytes that were not valid UTF-8
    /// (surfaced as U+FFFD by the transport's lossy conversion). During the
    /// connect handshake this is the signature of a baud-rate mismatch.
    pub fn saw_undecodable_data(&self) -> bool {
        self.saw_undecodable_data
    }

    /// Get the current session state.
    pub fn session_state(&self) -> SessionState {
        self.state_machine.state()
    }

    /// Get a reference to the current machine settings.
    pub fn settings(&self) -> &GrblSettings {
        &self.settings
    }

    /// Get the last known controller info fields.
    pub fn controller_info(&self) -> &HashMap<String, String> {
        &self.controller_info
    }

    /// Identity evidence accumulated from raw controller response lines.
    ///
    /// This is descriptive only: it does not select a driver, enable
    /// capabilities, or change the session lifecycle.
    pub fn grbl_family_identity(&self) -> GrblFamilyIdentity {
        self.identity_detector.identity()
    }

    /// Most recent explicit read-only identity-probe result for this
    /// connection, if one has been run.
    pub fn last_identity_probe(&self) -> Option<&GrblFamilyIdentityProbeResult> {
        self.last_identity_probe.as_ref()
    }

    pub fn port_name(&self) -> &str {
        self.transport.port_name()
    }

    /// Get the last known machine status.
    pub fn last_status(&self) -> &MachineStatus {
        &self.last_status
    }

    /// Connect to the machine: open transport, wait for banner, read settings.
    pub fn connect(&mut self) -> Result<(), GrblError> {
        self.state_machine.transition(SessionState::Connecting)?;
        // A reused session must never carry identity evidence across connection
        // attempts.
        self.identity_detector = GrblFamilyIdentityDetector::default();
        self.last_identity_probe = None;

        if let Err(e) = self.transport.open() {
            self.state_machine.force(SessionState::Error);
            return Err(e.into());
        }

        self.state_machine.transition(SessionState::TransportOpen)?;
        self.state_machine
            .transition(SessionState::WaitingForBanner)?;

        debug!("Transport open, waiting for banner");
        Ok(())
    }

    /// Process incoming data. Returns parsed responses.
    pub fn poll(&mut self) -> Result<Vec<GrblResponse>, GrblError> {
        let mut responses = Vec::new();

        loop {
            match self.transport.read_line()? {
                Some(line) if !line.is_empty() => {
                    if line.contains('\u{FFFD}') {
                        self.saw_undecodable_data = true;
                    }
                    self.identity_detector.observe_line(&line);
                    let response = parser::parse_response(&line);
                    self.handle_response(&response, &line);
                    responses.push(response);
                }
                _ => break,
            }
        }

        Ok(responses)
    }

    fn handle_response(&mut self, response: &GrblResponse, raw_line: &str) {
        self.log_console_entry(ConsoleDirection::Received, raw_line);

        match response {
            GrblResponse::Banner(_) => {
                if let GrblResponse::Banner(banner) = response {
                    self.controller_info
                        .insert("Banner".to_string(), banner.clone());
                }
                if self.state_machine.state() == SessionState::WaitingForBanner {
                    let _ = self.state_machine.transition(SessionState::Validating);
                }
            }
            GrblResponse::Status(status) => {
                self.status_report_count = self.status_report_count.wrapping_add(1);
                // GRBL only includes `Ov:` (override fields) periodically, not in
                // every status report.  When Ov is absent, the parser returns 0 for
                // the override fields.  Preserve the previous override values in
                // that case so callers always see the real overrides.
                let mut merged = status.clone();
                if merged.feed_override == 0 {
                    merged.feed_override = self.last_status.feed_override;
                }
                if merged.rapid_override == 0 {
                    merged.rapid_override = self.last_status.rapid_override;
                }
                if merged.spindle_override == 0 {
                    merged.spindle_override = self.last_status.spindle_override;
                }
                if merged.run_state == MachineRunState::Alarm {
                    self.state_machine.force(SessionState::Alarm);
                } else if self.state_machine.state() == SessionState::Alarm
                    && merged.run_state == MachineRunState::Idle
                {
                    let _ = self.state_machine.transition(SessionState::Ready);
                }
                self.last_status = merged;
            }
            GrblResponse::Setting(num, val) => {
                self.settings.set(*num, *val);
            }
            GrblResponse::Message(message) => {
                self.controller_info
                    .insert("Message".to_string(), message.clone());
            }
            GrblResponse::Feedback(feedback) => {
                if let Some((key, value)) = feedback.split_once(':') {
                    self.controller_info
                        .insert(key.trim().to_string(), value.trim().to_string());
                } else {
                    self.controller_info
                        .insert("Feedback".to_string(), feedback.clone());
                }
            }
            GrblResponse::Alarm(code) => {
                warn!(code, "GRBL alarm received");
                self.state_machine.force(SessionState::Alarm);
            }
            _ => {}
        }
    }

    fn log_console_entry(&mut self, direction: ConsoleDirection, content: &str) {
        let entry = ConsoleEntry {
            timestamp: Utc::now(),
            direction,
            content: content.to_string(),
        };

        if self.console_log.len() >= CONSOLE_LOG_CAPACITY {
            self.console_log.pop_front();
        }

        self.console_log.push_back(entry);
    }

    /// Mark the session as ready (after validation is complete).
    pub fn mark_ready(&mut self) -> Result<(), GrblError> {
        self.state_machine.transition(SessionState::Ready)
    }

    /// Move a bannerless transport into validation only after it returned a
    /// fresh status report on this connection. Network GRBL streams commonly
    /// attach after firmware boot and therefore cannot be expected to replay a
    /// startup banner.
    pub fn begin_validation_from_fresh_status(
        &mut self,
        previous_status_report_count: u64,
    ) -> Result<(), GrblError> {
        if self.status_report_count == previous_status_report_count {
            return Err(GrblError::FreshStatusRequired);
        }
        match self.session_state() {
            SessionState::WaitingForBanner => {
                self.state_machine.transition(SessionState::Validating)
            }
            // A banner can arrive before the requested status report. An
            // Alarm status also moves the session directly into Alarm. Both
            // remain valid, read-only identity-probe states once freshness is
            // proven above.
            SessionState::Validating | SessionState::Alarm => Ok(()),
            state => Err(GrblError::InvalidState {
                action: "validate a bannerless transport",
                state: format!("{state:?}"),
            }),
        }
    }

    /// Disconnect from the machine.
    pub fn disconnect(&mut self) -> Result<(), GrblError> {
        if self.transport.is_open() {
            let _ = self.transport.close();
        }
        self.identity_detector = GrblFamilyIdentityDetector::default();
        self.last_identity_probe = None;
        self.state_machine.force(SessionState::Disconnected);
        Ok(())
    }

    /// Send a G-code command line.
    pub fn send_command(&mut self, command: &str) -> Result<(), GrblError> {
        if !self.transport.is_open() {
            return Err(GrblError::NotConnected);
        }
        self.transport.write_line(command)?;
        self.log_console_entry(ConsoleDirection::Sent, command);
        Ok(())
    }

    /// Send a real-time command (single byte, no newline).
    pub fn send_realtime(&mut self, bytes: &[u8]) -> Result<(), GrblError> {
        if !self.transport.is_open() {
            return Err(GrblError::NotConnected);
        }
        self.transport.write_bytes(bytes)?;
        Ok(())
    }

    /// Query the machine status (?).
    pub fn poll_status(&mut self) -> Result<(), GrblError> {
        self.send_realtime(commands::status_query())
    }

    /// Unlock the machine ($X).
    pub fn unlock(&mut self) -> Result<(), GrblError> {
        self.send_command(&commands::unlock())
    }

    /// Home all axes ($H).
    pub fn home(&mut self) -> Result<(), GrblError> {
        self.send_command(&commands::home())
    }

    /// Jog to a relative position.
    pub fn jog(&mut self, x: f64, y: f64, z: Option<f64>, feed: f64) -> Result<(), GrblError> {
        self.send_command(&commands::jog(x, y, z, feed))
    }

    /// Cancel current jog.
    pub fn jog_cancel(&mut self) -> Result<(), GrblError> {
        self.send_realtime(commands::jog_cancel())
    }

    /// Request settings dump ($$).
    pub fn request_settings(&mut self) -> Result<(), GrblError> {
        self.send_command(&commands::settings_dump())
    }

    /// Feed hold (pause).
    pub fn feed_hold(&mut self) -> Result<(), GrblError> {
        self.send_realtime(commands::feed_hold())
    }

    /// Cycle start (resume).
    pub fn cycle_start(&mut self) -> Result<(), GrblError> {
        self.send_realtime(commands::cycle_start())
    }

    /// Soft reset.
    pub fn soft_reset(&mut self) -> Result<(), GrblError> {
        self.send_realtime(commands::soft_reset())?;
        self.identity_detector = GrblFamilyIdentityDetector::default();
        self.last_identity_probe = None;
        self.state_machine.force(SessionState::WaitingForBanner);
        Ok(())
    }

    /// Set work coordinate origin at current position (G92).
    pub fn set_origin(&mut self) -> Result<(), GrblError> {
        self.send_command(&commands::set_origin())
    }

    /// Reset work coordinate origin to machine coordinates (G92.1).
    pub fn reset_origin(&mut self) -> Result<(), GrblError> {
        self.send_command(&commands::reset_origin())
    }

    /// Transition session to Running state.
    pub fn start_running(&mut self) -> Result<(), GrblError> {
        self.state_machine.transition(SessionState::Running)
    }

    /// Transition session to Paused state.
    pub fn pause(&mut self) -> Result<(), GrblError> {
        self.state_machine.transition(SessionState::Paused)
    }

    /// Transition session from Paused back to Running.
    pub fn resume(&mut self) -> Result<(), GrblError> {
        self.state_machine.transition(SessionState::Running)
    }

    /// Transition session from Running/Paused back to Ready.
    pub fn stop(&mut self) -> Result<(), GrblError> {
        self.state_machine.transition(SessionState::Ready)
    }

    /// Get console log entries (newest first, up to limit).
    pub fn get_console_log(&self, limit: usize) -> Vec<ConsoleEntry> {
        self.console_log.iter().rev().take(limit).cloned().collect()
    }

    /// Clear all retained sent and received console entries.
    pub fn clear_console_log(&mut self) {
        self.console_log.clear();
    }

    /// Send a GRBL setting command ($N=V).
    pub fn send_setting(&mut self, key: GrblSettingId, value: f64) -> Result<(), GrblError> {
        self.send_command(&commands::set_setting(key, value))
    }

    /// Query all settings ($$).
    pub fn query_settings(&mut self) -> Result<(), GrblError> {
        self.send_command(&commands::query_all_settings())
    }

    /// Get controller firmware info ($I).
    pub fn get_controller_info(&mut self) -> Result<(), GrblError> {
        self.send_command(&commands::controller_info())
    }

    /// Run the bounded read-only GRBL-family identity sequence.
    ///
    /// The caller must own the line-command channel: do not run this while a
    /// job or another request/response transaction is in flight. The method is
    /// state-gated accordingly and never sends motion, reset, homing, or
    /// setting-write commands.
    pub fn probe_grbl_family_identity(
        &mut self,
        config: GrblFamilyIdentityProbeConfig,
    ) -> Result<GrblFamilyIdentityProbeResult, GrblError> {
        let state = self.session_state();
        if !matches!(
            state,
            SessionState::Validating | SessionState::Ready | SessionState::Alarm
        ) {
            return Err(GrblError::InvalidState {
                action: "probe controller identity",
                state: format!("{state:?}"),
            });
        }

        // A failed retry must not leave a prior successful result looking
        // current for this connection.
        self.last_identity_probe = None;
        let mut sequence = GrblFamilyIdentityProbeSequence::default();
        let mut next_command = sequence.begin();

        while let Some(command) = next_command.take() {
            let wire_command = command.wire_command();
            self.send_command(&wire_command)?;
            let command_started = Instant::now();

            loop {
                let responses = self.poll()?;
                let identity = self.grbl_family_identity();

                for response in &responses {
                    if let Some(command) = sequence.observe_response(response, &identity) {
                        next_command = Some(command);
                        break;
                    }
                    if sequence.is_complete() {
                        break;
                    }
                }

                if next_command.is_some() || sequence.is_complete() {
                    break;
                }

                if command_started.elapsed() >= config.command_timeout {
                    sequence.timeout_active_command();
                    break;
                }

                std::thread::sleep(config.poll_interval);
            }
        }

        let result = sequence
            .finish(self.grbl_family_identity())
            .expect("identity probe sequence must finish before returning");
        self.last_identity_probe = Some(result.clone());
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GrblFamilyIdentityProbeOutcome;
    use beambench_common::{GrblFamilyDialect, GrblFamilyIdentityStatus};
    use beambench_serial::{MockSerialTransport, SerialError};

    #[test]
    fn bannerless_validation_requires_a_fresh_status_report() {
        let transport = MockSerialTransport::new("network-mock");
        let handle = transport.handle();
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        let baseline = session.status_report_count();

        assert!(matches!(
            session.begin_validation_from_fresh_status(baseline),
            Err(GrblError::FreshStatusRequired)
        ));
        handle.enqueue_response("<Idle|MPos:0.000,0.000,0.000|FS:0,0>");
        session.poll().unwrap();
        session
            .begin_validation_from_fresh_status(baseline)
            .unwrap();
        assert_eq!(session.session_state(), SessionState::Validating);
    }

    #[test]
    fn fresh_alarm_status_is_a_valid_bannerless_probe_state() {
        let transport = MockSerialTransport::new("network-alarm-mock");
        let handle = transport.handle();
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        let baseline = session.status_report_count();
        handle.enqueue_response("<Alarm|MPos:0.000,0.000,0.000|FS:0,0>");
        session.poll().unwrap();

        session
            .begin_validation_from_fresh_status(baseline)
            .unwrap();
        assert_eq!(session.session_state(), SessionState::Alarm);
    }

    struct ScriptedIdentityTransport {
        open: bool,
        rx: VecDeque<String>,
        controller_info_responses: VecDeque<String>,
        extended_info_responses: VecDeque<String>,
    }

    impl ScriptedIdentityTransport {
        fn new(initial: &[&str], controller_info: &[&str], extended_info: &[&str]) -> Self {
            Self {
                open: false,
                rx: initial.iter().map(|line| (*line).to_string()).collect(),
                controller_info_responses: controller_info
                    .iter()
                    .map(|line| (*line).to_string())
                    .collect(),
                extended_info_responses: extended_info
                    .iter()
                    .map(|line| (*line).to_string())
                    .collect(),
            }
        }
    }

    impl SerialTransport for ScriptedIdentityTransport {
        fn open(&mut self) -> Result<(), SerialError> {
            if self.open {
                return Err(SerialError::AlreadyOpen);
            }
            self.open = true;
            Ok(())
        }

        fn close(&mut self) -> Result<(), SerialError> {
            if !self.open {
                return Err(SerialError::NotOpen);
            }
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

            let mut responses = match line {
                "$I" => std::mem::take(&mut self.controller_info_responses),
                "$I+" => std::mem::take(&mut self.extended_info_responses),
                _ => VecDeque::new(),
            };
            self.rx.append(&mut responses);
            Ok(())
        }

        fn read_available(&mut self) -> Result<Vec<u8>, SerialError> {
            if !self.open {
                return Err(SerialError::NotOpen);
            }
            let mut data = Vec::new();
            while let Some(line) = self.rx.pop_front() {
                data.extend_from_slice(line.as_bytes());
                data.push(b'\n');
            }
            Ok(data)
        }

        fn read_line(&mut self) -> Result<Option<String>, SerialError> {
            if !self.open {
                return Err(SerialError::NotOpen);
            }
            Ok(self.rx.pop_front())
        }

        fn flush(&mut self) -> Result<(), SerialError> {
            if !self.open {
                return Err(SerialError::NotOpen);
            }
            Ok(())
        }

        fn port_name(&self) -> &str {
            "scripted-identity"
        }
    }

    fn make_session() -> GrblSession {
        let transport = MockSerialTransport::new("mock");
        GrblSession::new(Box::new(transport))
    }

    fn scripted_identity_session(
        banner: &str,
        controller_info: &[&str],
        extended_info: &[&str],
    ) -> GrblSession {
        let transport = ScriptedIdentityTransport::new(&[banner], controller_info, extended_info);
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();
        assert!(matches!(
            session.session_state(),
            SessionState::Validating | SessionState::Alarm
        ));
        session
    }

    fn sent_probe_commands(session: &GrblSession) -> Vec<String> {
        session
            .get_console_log(100)
            .into_iter()
            .rev()
            .filter(|entry| entry.direction == ConsoleDirection::Sent)
            .map(|entry| entry.content)
            .collect()
    }

    #[test]
    fn session_starts_disconnected() {
        let session = make_session();
        assert_eq!(session.session_state(), SessionState::Disconnected);
    }

    #[test]
    fn connect_opens_transport() {
        let mut session = make_session();
        session.connect().unwrap();
        assert_eq!(session.session_state(), SessionState::WaitingForBanner);
    }

    #[test]
    fn full_connect_lifecycle() {
        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("Grbl 1.1h ['$' for help]");
        transport.enqueue_response("$110=1000.000");
        transport.enqueue_response("$111=2000.000");
        transport.enqueue_response("$255=1.000");
        transport.enqueue_response("$256=2.000");
        transport.enqueue_response("$376=3.000");
        transport.enqueue_response("$65535=4.000");
        transport.enqueue_response("$32=1");

        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        assert_eq!(session.session_state(), SessionState::WaitingForBanner);

        // Poll should process banner and transition to Validating
        let responses = session.poll().unwrap();
        assert!(
            responses
                .iter()
                .any(|r| matches!(r, GrblResponse::Banner(_)))
        );
        assert_eq!(session.session_state(), SessionState::Validating);

        // Settings should be captured
        assert_eq!(session.settings().get(110), Some(1000.0));
        assert_eq!(session.settings().get(111), Some(2000.0));
        assert_eq!(session.settings().get(255), Some(1.0));
        assert_eq!(session.settings().get(256), Some(2.0));
        assert_eq!(session.settings().get(376), Some(3.0));
        assert_eq!(session.settings().get(u16::MAX), Some(4.0));
        let setting_map = session.settings().as_string_map();
        assert_eq!(setting_map.get("$255"), Some(&"1".to_string()));
        assert_eq!(setting_map.get("$256"), Some(&"2".to_string()));
        assert_eq!(setting_map.get("$376"), Some(&"3".to_string()));
        assert_eq!(setting_map.get("$65535"), Some(&"4".to_string()));
        assert!(session.settings().laser_mode());

        // Mark ready
        session.mark_ready().unwrap();
        assert_eq!(session.session_state(), SessionState::Ready);
    }

    #[test]
    fn disconnect_resets_state() {
        let mut session = make_session();
        session.connect().unwrap();
        session.disconnect().unwrap();
        assert_eq!(session.session_state(), SessionState::Disconnected);
    }

    #[test]
    fn send_command_when_disconnected_fails() {
        let mut session = make_session();
        assert!(matches!(
            session.send_command("G0 X10"),
            Err(GrblError::NotConnected)
        ));
    }

    #[test]
    fn alarm_sets_alarm_state() {
        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("Grbl 1.1h ['$' for help]");
        transport.enqueue_response("ALARM:1");

        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();
        assert_eq!(session.session_state(), SessionState::Alarm);
    }

    #[test]
    fn alarm_status_sets_alarm_state() {
        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("Grbl 1.1h ['$' for help]");
        transport.enqueue_response("<Alarm|MPos:0.000,0.000,0.000|WPos:0.000,0.000,0.000>");

        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();

        assert_eq!(session.session_state(), SessionState::Alarm);
        assert_eq!(session.last_status().run_state, MachineRunState::Alarm);
    }

    #[test]
    fn idle_status_recovers_from_alarm_state() {
        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("<Idle|MPos:0.000,0.000,0.000|WPos:0.000,0.000,0.000>");

        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.state_machine.force(SessionState::Alarm);
        assert_eq!(session.session_state(), SessionState::Alarm);

        session.poll().unwrap();

        assert_eq!(session.session_state(), SessionState::Ready);
        assert_eq!(session.last_status().run_state, MachineRunState::Idle);
    }

    #[test]
    fn mark_ready_is_idempotent_after_alarm_recovers_to_idle() {
        let mut transport = MockSerialTransport::new("mock");
        let handle = transport.handle();
        transport.enqueue_response("ALARM:1");

        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();

        session.poll().unwrap();
        assert_eq!(session.session_state(), SessionState::Alarm);

        handle.enqueue_response("<Idle|MPos:0.000,0.000,0.000|WPos:0.000,0.000,0.000>");
        session.poll().unwrap();
        assert_eq!(session.session_state(), SessionState::Ready);

        session.mark_ready().unwrap();
        assert_eq!(session.session_state(), SessionState::Ready);
    }

    #[test]
    fn stop_is_idempotent_after_alarm_recovers_to_idle() {
        let mut transport = MockSerialTransport::new("mock");
        let handle = transport.handle();
        transport.enqueue_response("ALARM:1");

        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();

        session.poll().unwrap();
        assert_eq!(session.session_state(), SessionState::Alarm);

        handle.enqueue_response("<Idle|MPos:0.000,0.000,0.000|WPos:0.000,0.000,0.000>");
        session.poll().unwrap();
        assert_eq!(session.session_state(), SessionState::Ready);

        session.stop().unwrap();
        assert_eq!(session.session_state(), SessionState::Ready);
    }

    #[test]
    fn garbage_lines_set_undecodable_flag() {
        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("Grbl 1.1h");
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();
        assert!(
            !session.saw_undecodable_data(),
            "clean banner must not set the flag"
        );

        // A line carrying U+FFFD is what the transport produces for bytes
        // that were not valid UTF-8 (e.g. baud-rate mismatch noise).
        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("\u{FFFD}\u{FFFD}x\u{FFFD}");
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();
        assert!(session.saw_undecodable_data());
    }

    #[test]
    fn status_poll_updates_last_status() {
        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("Grbl 1.1h");
        transport.enqueue_response("<Run|MPos:10.000,20.000,0.000|FS:1000,500>");

        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();

        let status = session.last_status();
        assert_eq!(status.machine_position.x, 10.0);
        assert_eq!(status.machine_position.y, 20.0);
        assert_eq!(status.feed_rate, 1000.0);
    }

    #[test]
    fn console_log_captures_sent_commands() {
        let mut session = make_session();
        session.connect().unwrap();
        session.send_command("G0 X10 Y20").unwrap();

        let log = session.get_console_log(10);
        assert!(!log.is_empty());
        assert!(
            log.iter()
                .any(|e| e.direction == ConsoleDirection::Sent && e.content.contains("G0 X10 Y20"))
        );
    }

    #[test]
    fn console_log_captures_received_responses() {
        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("Grbl 1.1h");
        let mut session = GrblSession::new(Box::new(transport));

        session.connect().unwrap();
        session.poll().unwrap();

        let log = session.get_console_log(10);
        assert!(
            log.iter()
                .any(|e| e.direction == ConsoleDirection::Received && e.content == "Grbl 1.1h")
        );
    }

    #[test]
    fn clear_console_log_removes_sent_and_received_entries() {
        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("ok");
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.send_command("T10M6").unwrap();
        session.poll().unwrap();
        assert!(!session.get_console_log(10).is_empty());

        session.clear_console_log();

        assert!(session.get_console_log(10).is_empty());
    }

    #[test]
    fn console_log_limited_to_capacity() {
        let mut session = make_session();
        session.connect().unwrap();

        // Send more than capacity
        for i in 0..1500 {
            let _ = session.send_command(&format!("G0 X{i}"));
        }

        let log = session.get_console_log(2000);
        assert!(log.len() <= CONSOLE_LOG_CAPACITY);
    }

    #[test]
    fn get_console_log_returns_newest_first() {
        let mut session = make_session();
        session.connect().unwrap();

        session.send_command("G0 X10").unwrap();
        session.send_command("G0 X20").unwrap();

        let log = session.get_console_log(2);
        assert_eq!(log.len(), 2);
        // Newest should be first
        assert!(log[0].content.contains("G0 X20"));
    }

    #[test]
    fn send_setting_formats_correctly() {
        let mut session = make_session();
        session.connect().unwrap();
        session.send_setting(376, 1.0).unwrap();

        let log = session.get_console_log(1);
        assert_eq!(log[0].content, "$376=1");
    }

    #[test]
    fn query_settings_sends_double_dollar() {
        let mut session = make_session();
        session.connect().unwrap();
        session.query_settings().unwrap();

        let log = session.get_console_log(1);
        assert!(log[0].content.contains("$$"));
    }

    #[test]
    fn get_controller_info_sends_dollar_i() {
        let mut session = make_session();
        session.connect().unwrap();
        session.get_controller_info().unwrap();

        let log = session.get_console_log(1);
        assert!(log[0].content.contains("$I"));
    }

    #[test]
    fn identity_probe_identifies_fluid_nc_without_unnecessary_extended_query() {
        let mut session = scripted_identity_session(
            "Grbl 4.0 [FluidNC v4.0.3 (esp32-wifi) '$' for help]",
            &[
                "[VER:4.0 FluidNC v4.0.3 (esp32-wifi) :]",
                "[OPT:VN,16,128]",
                "ok",
            ],
            &[],
        );

        let result = session
            .probe_grbl_family_identity(GrblFamilyIdentityProbeConfig::default())
            .unwrap();

        assert_eq!(result.identity.dialect, GrblFamilyDialect::FluidNc);
        assert_eq!(result.identity.status, GrblFamilyIdentityStatus::Identified);
        assert_eq!(
            result.controller_info,
            GrblFamilyIdentityProbeOutcome::Succeeded
        );
        assert_eq!(
            result.extended_controller_info,
            GrblFamilyIdentityProbeOutcome::NotNeeded
        );
        assert_eq!(sent_probe_commands(&session), ["$I"]);
        assert_eq!(session.last_identity_probe(), Some(&result));
    }

    #[test]
    fn identity_probe_uses_extended_query_to_identify_compatibility_mode_grbl_hal() {
        let mut session = scripted_identity_session(
            "Grbl 1.1f ['$' for help]",
            &["[VER:1.1f.20260709:]", "[OPT:VN,16,128]", "ok"],
            &[
                "[VER:1.1f.20260709:]",
                "[OPT:VN,16,128,3,0]",
                "[FIRMWARE:grblHAL]",
                "[COMPATIBILITY LEVEL:1]",
                "ok",
            ],
        );

        let result = session
            .probe_grbl_family_identity(GrblFamilyIdentityProbeConfig::default())
            .unwrap();

        assert_eq!(result.identity.dialect, GrblFamilyDialect::GrblHal);
        assert_eq!(result.identity.status, GrblFamilyIdentityStatus::Identified);
        assert_eq!(
            result.identity.firmware_version.as_deref(),
            Some("1.1f.20260709")
        );
        assert_eq!(
            result.extended_controller_info,
            GrblFamilyIdentityProbeOutcome::Succeeded
        );
        assert_eq!(sent_probe_commands(&session), ["$I", "$I+"]);
    }

    #[test]
    fn identity_probe_treats_unsupported_extended_query_as_nonfatal() {
        let mut session = scripted_identity_session(
            "Grbl 1.1h ['$' for help]",
            &["[VER:1.1h.20200101:]", "[OPT:VL,15,128]", "ok"],
            &["error:3"],
        );

        let result = session
            .probe_grbl_family_identity(GrblFamilyIdentityProbeConfig::default())
            .unwrap();

        assert_eq!(result.identity.dialect, GrblFamilyDialect::Grbl);
        assert_eq!(
            result.identity.status,
            GrblFamilyIdentityStatus::ProtocolCompatible
        );
        assert_eq!(
            result.extended_controller_info,
            GrblFamilyIdentityProbeOutcome::Rejected(3)
        );
        assert_eq!(sent_probe_commands(&session), ["$I", "$I+"]);
        assert_eq!(session.session_state(), SessionState::Validating);
    }

    #[test]
    fn identity_probe_timeout_stops_before_extended_query() {
        let mut session = scripted_identity_session(
            "Grbl 1.1h ['$' for help]",
            &[],
            &["[FIRMWARE:grblHAL]", "ok"],
        );

        let result = session
            .probe_grbl_family_identity(GrblFamilyIdentityProbeConfig {
                command_timeout: std::time::Duration::ZERO,
                poll_interval: std::time::Duration::ZERO,
            })
            .unwrap();

        assert_eq!(
            result.controller_info,
            GrblFamilyIdentityProbeOutcome::TimedOut
        );
        assert_eq!(
            result.extended_controller_info,
            GrblFamilyIdentityProbeOutcome::NotAttempted
        );
        assert_eq!(sent_probe_commands(&session), ["$I"]);
    }

    #[test]
    fn identity_probe_rejects_unsafe_session_states_without_writing() {
        let mut session = make_session();

        let error = session
            .probe_grbl_family_identity(GrblFamilyIdentityProbeConfig::default())
            .unwrap_err();

        assert!(matches!(error, GrblError::InvalidState { .. }));
        assert!(sent_probe_commands(&session).is_empty());
        assert!(session.last_identity_probe().is_none());
    }

    #[test]
    fn controller_info_caches_feedback_fields() {
        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("Grbl 1.1h ['$' for help]");
        transport.enqueue_response("[VER:1.1h.20200101:]");
        transport.enqueue_response("[OPT:VL,15,128]");

        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();

        assert_eq!(
            session.controller_info().get("Banner"),
            Some(&"Grbl 1.1h ['$' for help]".to_string())
        );
        assert_eq!(
            session.controller_info().get("VER"),
            Some(&"1.1h.20200101:".to_string())
        );
        assert_eq!(
            session.controller_info().get("OPT"),
            Some(&"VL,15,128".to_string())
        );
    }

    #[test]
    fn identity_observation_is_descriptive_and_resets_at_session_boundaries() {
        let mut transport = MockSerialTransport::new("mock");
        let handle = transport.handle();
        transport.enqueue_response("Grbl 4.0 [FluidNC v4.0.3 (esp32-wifi) '$' for help]");
        transport.enqueue_response("[VER:4.0 FluidNC v4.0.3 (esp32-wifi) :]");

        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();

        let identity = session.grbl_family_identity();
        assert_eq!(
            identity.status,
            beambench_common::GrblFamilyIdentityStatus::Identified
        );
        assert_eq!(
            identity.controller_model(),
            beambench_common::ControllerModel::FluidNc
        );
        assert!(identity.positive_identity().is_some());
        assert_eq!(session.session_state(), SessionState::Validating);

        // A rejected connect call does not erase evidence for the still-active
        // session.
        assert!(session.connect().is_err());
        assert_eq!(session.grbl_family_identity(), identity);

        // A successful reset starts a fresh handshake and must not retain the
        // previous controller's evidence.
        session.soft_reset().unwrap();
        assert_eq!(
            session.grbl_family_identity(),
            beambench_common::GrblFamilyIdentity::default()
        );

        handle.enqueue_response("Grbl 4.0 [FluidNC v4.0.3 (esp32-wifi) '$' for help]");
        handle.enqueue_response("[VER:4.0 FluidNC v4.0.3 (esp32-wifi) :]");
        session.poll().unwrap();
        assert!(session.grbl_family_identity().positive_identity().is_some());

        session.disconnect().unwrap();
        assert_eq!(
            session.grbl_family_identity(),
            beambench_common::GrblFamilyIdentity::default()
        );
    }
}
