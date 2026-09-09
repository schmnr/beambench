//! Streaming engine with bounded send window.
//! Respects GRBL's 128-byte RX buffer by tracking bytes in flight.

use crate::error::StreamerError;
use crate::progress::ProgressTracker;
use beambench_common::console::{ConsoleDirection, ConsoleEntry};
use beambench_core::TransferMode;
use beambench_grbl::GrblSession;
use beambench_grbl::parser::GrblResponse;
use chrono::Utc;
use std::collections::VecDeque;
use std::time::Duration;
use tracing::{debug, warn};

/// Usable GRBL RX buffer in bytes. The firmware buffer is 128, but its ring
/// implementation keeps one byte unusable — planning against the full 128
/// overflows by one byte when the window is exactly full, silently dropping
/// a character, corrupting a line, and deadlocking the ack accounting
/// (field report: ACMER S1 stalling after the first window of short lines).
/// Every major GRBL sender plans against 127.
const GRBL_RX_BUFFER_SIZE: usize = 127;

fn is_air_assist_command(command: &str) -> bool {
    let command = command.split([';', '(']).next().unwrap_or_default().trim();
    matches!(
        command.to_ascii_uppercase().as_str(),
        "M7" | "M07" | "M8" | "M08" | "M9" | "M09"
    )
}

/// Streaming engine that manages the flow of G-code commands to GRBL.
pub struct StreamingEngine {
    commands: Vec<String>,
    next_index: usize,
    bytes_in_flight: usize,
    sent_sizes: VecDeque<usize>,
    paused: bool,
    cancelled: bool,
    failed: bool,
    transfer_mode: TransferMode,
    error_message: Option<String>,
    console_log: Vec<ConsoleEntry>,
    pause_position: Option<(f64, f64)>,
}

impl StreamingEngine {
    /// Reject unsendable lines before any machine output occurs.
    pub fn validate_commands(commands: &[String]) -> Result<(), StreamerError> {
        for (index, command) in commands.iter().enumerate() {
            if command.len() >= GRBL_RX_BUFFER_SIZE || command.contains(['\r', '\n']) {
                return Err(StreamerError::JobFailed(format!(
                    "G-code line {} cannot fit the GRBL receive buffer. Each line must be at most {} bytes and contain no embedded newline.",
                    index + 1,
                    GRBL_RX_BUFFER_SIZE - 1
                )));
            }
        }
        Ok(())
    }

    pub fn new(commands: Vec<String>) -> Self {
        Self::new_with_transfer_mode(commands, TransferMode::Buffered)
    }

    pub fn new_with_transfer_mode(commands: Vec<String>, transfer_mode: TransferMode) -> Self {
        Self {
            commands,
            next_index: 0,
            bytes_in_flight: 0,
            sent_sizes: VecDeque::new(),
            paused: false,
            cancelled: false,
            failed: false,
            transfer_mode,
            error_message: None,
            console_log: Vec::new(),
            pause_position: None,
        }
    }

    pub fn total_commands(&self) -> usize {
        self.commands.len()
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub fn is_failed(&self) -> bool {
        self.failed
    }

    pub fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
    }

    pub fn fail(&mut self, message: impl Into<String>, progress: &mut ProgressTracker) {
        let message = message.into();
        self.failed = true;
        self.error_message = Some(message.clone());
        progress.set_failed(message);
    }

    pub fn bytes_in_flight(&self) -> usize {
        self.bytes_in_flight
    }

    /// G4 blocks its acknowledgement while the controller may report Idle.
    /// Read the oldest pending block, including compact/custom G-code forms.
    pub(crate) fn pending_dwell_duration(&self) -> Option<Duration> {
        if self.sent_sizes.is_empty() {
            return None;
        }
        let command = &self.commands[self.next_index - self.sent_sizes.len()];
        let mut block = String::new();
        let mut comment = false;
        for byte in command.bytes() {
            match byte {
                b'(' => comment = true,
                b')' => comment = false,
                b';' if !comment => break,
                byte if !comment && !byte.is_ascii_whitespace() => {
                    block.push(byte.to_ascii_uppercase() as char);
                }
                _ => {}
            }
        }
        let mut dwell = false;
        let mut seconds = None;
        for (index, letter) in block
            .char_indices()
            .filter(|(_, ch)| ch.is_ascii_alphabetic())
        {
            let value = block[index + 1..]
                .split(|ch: char| ch.is_ascii_alphabetic())
                .next()?;
            match letter {
                'G' if value.parse::<f64>().ok() == Some(4.0) => dwell = true,
                'P' => seconds = value.parse::<f64>().ok(),
                _ => {}
            }
        }
        dwell
            .then_some(seconds)
            .flatten()
            .and_then(|seconds| Duration::try_from_secs_f64(seconds).ok())
    }

    /// Check if all commands have been sent.
    pub fn all_sent(&self) -> bool {
        self.next_index >= self.commands.len()
    }

    /// Check if all commands have been acknowledged.
    pub fn all_acknowledged(&self) -> bool {
        self.all_sent() && self.sent_sizes.is_empty()
    }

    /// Send as many commands as fit within the GRBL RX buffer.
    pub fn send_tick(
        &mut self,
        session: &mut GrblSession,
        progress: &mut ProgressTracker,
    ) -> Result<usize, StreamerError> {
        if self.paused || self.cancelled || self.failed {
            return Ok(0);
        }
        // Optional coolant commands differ across GRBL controllers. Wait for
        // their acknowledgement before putting following motion on the wire.
        if !self.sent_sizes.is_empty() && is_air_assist_command(&self.commands[self.next_index - 1])
        {
            return Ok(0);
        }

        let mut sent_count = 0;

        while self.next_index < self.commands.len() {
            if self.transfer_mode == TransferMode::Synchronous && self.bytes_in_flight > 0 {
                break;
            }
            let cmd = &self.commands[self.next_index];
            let cmd_size = cmd.len() + 1; // +1 for \n
            let wait_for_ack = is_air_assist_command(cmd);
            if wait_for_ack && self.bytes_in_flight > 0 {
                break;
            }

            if cmd_size > GRBL_RX_BUFFER_SIZE || cmd.contains(['\r', '\n']) {
                let message = format!(
                    "G-code line {} exceeds the GRBL line limit",
                    self.next_index + 1
                );
                self.fail(message.clone(), progress);
                return Err(StreamerError::JobFailed(message));
            }

            if self.bytes_in_flight + cmd_size > GRBL_RX_BUFFER_SIZE {
                break;
            }

            session.send_command(cmd)?;
            self.bytes_in_flight += cmd_size;
            self.sent_sizes.push_back(cmd_size);
            self.next_index += 1;
            progress.record_sent();
            sent_count += 1;

            // Log sent command
            self.console_log.push(ConsoleEntry {
                timestamp: Utc::now(),
                direction: ConsoleDirection::Sent,
                content: cmd.clone(),
            });

            debug!(
                cmd_index = self.next_index - 1,
                bytes_in_flight = self.bytes_in_flight,
                "Sent command"
            );
            if wait_for_ack {
                break;
            }
        }

        progress.set_buffer_fill(self.bytes_in_flight);
        Ok(sent_count)
    }

    /// Handle a response from GRBL.
    pub fn handle_response(
        &mut self,
        response: &GrblResponse,
        progress: &mut ProgressTracker,
    ) -> Result<(), StreamerError> {
        // Log received response
        self.console_log.push(ConsoleEntry {
            timestamp: Utc::now(),
            direction: ConsoleDirection::Received,
            content: format!("{:?}", response),
        });

        // A later acknowledgement or error must not overwrite the first
        // failure or change the command accounting retained for diagnostics.
        if self.failed {
            return Ok(());
        }

        match response {
            GrblResponse::Ok => {
                if let Some(size) = self.sent_sizes.pop_front() {
                    self.bytes_in_flight = self.bytes_in_flight.saturating_sub(size);
                    progress.record_acknowledged();
                    progress.set_buffer_fill(self.bytes_in_flight);
                } else {
                    warn!("Received ok but no commands in flight");
                }
            }
            GrblResponse::Error(code) => {
                let msg = beambench_grbl::parser::error_message(*code);
                let mut message = format!("GRBL error {code}: {msg}");
                if !self.sent_sizes.is_empty() {
                    let index = self.next_index - self.sent_sizes.len();
                    let command = &self.commands[index];
                    message.push_str(&format!(" at G-code line {}: {command}", index + 1));
                    if *code == 20 && is_air_assist_command(command) {
                        message.push_str(
                            ". The controller rejected an air-assist command. Check the machine profile's air-assist commands and custom G-code against the controller documentation before running again.",
                        );
                    }
                }
                self.fail(message.clone(), progress);
                return Err(StreamerError::JobFailed(message));
            }
            GrblResponse::Alarm(code) => {
                self.fail(format!("GRBL alarm {code}"), progress);
                return Err(StreamerError::AlarmDuringJob(*code));
            }
            GrblResponse::Banner(_) => {
                let message = "The controller restarted during the job. Streaming stopped because its queued commands and position can no longer be trusted.";
                self.fail(message, progress);
                return Err(StreamerError::JobFailed(message.to_owned()));
            }
            GrblResponse::Status(status)
                if status.run_state == beambench_common::machine::MachineRunState::Alarm =>
            {
                let message = "The controller reported Alarm during the job. Streaming stopped.";
                self.fail(message, progress);
                return Err(StreamerError::JobFailed(message.to_owned()));
            }
            _ => {
                // Status reports, messages, etc. — handled elsewhere
            }
        }
        Ok(())
    }

    /// Pause the engine.
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Resume the engine.
    pub fn resume(&mut self) {
        self.paused = false;
    }

    /// Cancel the engine.
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    /// Get console entries from this streaming session.
    pub fn get_console_entries(&self, limit: usize) -> Vec<ConsoleEntry> {
        self.console_log.iter().rev().take(limit).cloned().collect()
    }

    /// Record the last known position for pause indicator.
    pub fn set_pause_position(&mut self, x: f64, y: f64) {
        self.pause_position = Some((x, y));
    }

    /// Get the pause position (if paused).
    pub fn get_pause_position(&self) -> Option<(f64, f64)> {
        self.pause_position
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_serial::MockSerialTransport;

    fn make_session_and_engine(
        commands: Vec<String>,
    ) -> (GrblSession, StreamingEngine, ProgressTracker) {
        let transport = MockSerialTransport::new("mock");
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        let total = commands.len();
        let engine = StreamingEngine::new(commands);
        let progress = ProgressTracker::new(total);
        (session, engine, progress)
    }

    #[test]
    fn send_tick_sends_commands_within_buffer() {
        let commands: Vec<String> = (0..5).map(|i| format!("G0 X{i}")).collect();
        let (mut session, mut engine, mut progress) = make_session_and_engine(commands);

        let sent = engine.send_tick(&mut session, &mut progress).unwrap();
        assert_eq!(sent, 5); // Each "G0 X0" is 5+1=6 bytes, 5*6=30 < 128
    }

    #[test]
    fn send_tick_respects_buffer_limit() {
        // Each command is ~70 chars + 1 = ~71 bytes, so only 1 fits in 128 bytes
        let long_cmd = "G1 X999.999 Y999.999 F9999.999 S999".to_string(); // 35 chars
        let commands = vec![long_cmd.clone(); 10];
        let (mut session, mut engine, mut progress) = make_session_and_engine(commands);

        let sent = engine.send_tick(&mut session, &mut progress).unwrap();
        // 35+1=36 bytes each, 128/36=3 commands fit
        assert_eq!(sent, 3);
        assert_eq!(engine.bytes_in_flight(), 36 * 3);
    }

    #[test]
    fn handle_ok_frees_buffer_space() {
        let commands = vec!["G0 X10".to_string(); 5];
        let (mut session, mut engine, mut progress) = make_session_and_engine(commands);

        engine.send_tick(&mut session, &mut progress).unwrap();
        let initial_bytes = engine.bytes_in_flight();

        engine
            .handle_response(&GrblResponse::Ok, &mut progress)
            .unwrap();
        assert!(engine.bytes_in_flight() < initial_bytes);
    }

    #[test]
    fn handle_error_fails_job() {
        let commands = vec!["G0 X10".to_string()];
        let (mut session, mut engine, mut progress) = make_session_and_engine(commands);

        engine.send_tick(&mut session, &mut progress).unwrap();
        let result = engine.handle_response(&GrblResponse::Error(2), &mut progress);
        assert!(result.is_err());
        assert!(engine.is_failed());
    }

    #[test]
    fn reported_m7_rejection_stops_before_any_motion_is_sent() {
        // Feedback r-a0533762f5574d739968947e81195ab3: the first three
        // commands were acknowledged before M7 was echoed and rejected.
        let mut commands = [
            "G90",
            "G21",
            "M5",
            "M7",
            "G0 X58.465 Y117.604",
            "M4 S300",
            "G1 X58.465 Y117.660 F500",
            "G1 X103.573 Y117.660 F500",
            "G1 X103.573 Y117.604 F500",
        ]
        .map(str::to_owned)
        .to_vec();
        // The unsent project was not attached. Use placeholder motion lines
        // to reproduce the report's queue length, not its missing geometry.
        commands.extend(std::iter::repeat_n("G1 X0 Y0 F500".to_owned(), 1522));
        let (mut session, mut engine, mut progress) = make_session_and_engine(commands);

        assert_eq!(engine.send_tick(&mut session, &mut progress).unwrap(), 3);
        assert_eq!(engine.bytes_in_flight(), 11);
        for _ in 0..3 {
            assert_eq!(engine.send_tick(&mut session, &mut progress).unwrap(), 0);
            engine
                .handle_response(&GrblResponse::Ok, &mut progress)
                .unwrap();
        }
        assert_eq!(engine.send_tick(&mut session, &mut progress).unwrap(), 1);
        assert_eq!(engine.send_tick(&mut session, &mut progress).unwrap(), 0);
        engine
            .handle_response(
                &GrblResponse::Feedback("echo: M7".to_owned()),
                &mut progress,
            )
            .unwrap();
        assert!(
            engine
                .handle_response(&GrblResponse::Error(20), &mut progress)
                .is_err()
        );

        let failed = progress.snapshot();
        assert_eq!(failed.state, beambench_common::machine::JobState::Failed);
        assert_eq!(failed.sent_lines, 4);
        assert_eq!(failed.acknowledged_lines, 3);
        assert_eq!(failed.queued_lines, 1527);
        assert_eq!(failed.buffer_fill_bytes, 3);
        let message = failed.error_message.as_deref().unwrap();
        assert!(message.contains("G-code line 4: M7"));
        assert!(message.contains("air-assist"));
        // Firmware can send more errors and acknowledgements in the same
        // receive batch. Keep the first rejection and its queue snapshot.
        engine
            .handle_response(&GrblResponse::Ok, &mut progress)
            .unwrap();
        engine
            .handle_response(&GrblResponse::Error(2), &mut progress)
            .unwrap();
        assert_eq!(progress.snapshot(), failed);
        assert_eq!(engine.send_tick(&mut session, &mut progress).unwrap(), 0);
        assert_eq!(progress.snapshot().sent_lines, 4);
        assert!(!engine.get_console_entries(100).iter().any(|entry| {
            entry.direction == ConsoleDirection::Sent
                && (entry.content.starts_with("G0")
                    || entry.content.starts_with("G1")
                    || entry.content.starts_with("M4"))
        }));
    }

    #[test]
    fn acknowledged_air_assist_commands_allow_buffered_motion_to_continue() {
        for command in ["M7", "M8", "M9", "M07", "m8 ; air", "M9 (off)"] {
            let commands = ["G90", command, "G0 X10", "M4 S300", "G1 X20 F500"]
                .map(str::to_owned)
                .to_vec();
            let (mut session, mut engine, mut progress) = make_session_and_engine(commands);
            assert_eq!(engine.send_tick(&mut session, &mut progress).unwrap(), 1);
            engine
                .handle_response(&GrblResponse::Ok, &mut progress)
                .unwrap();
            assert_eq!(engine.send_tick(&mut session, &mut progress).unwrap(), 1);
            assert_eq!(engine.send_tick(&mut session, &mut progress).unwrap(), 0);
            engine
                .handle_response(
                    &GrblResponse::Feedback("echo: command".to_owned()),
                    &mut progress,
                )
                .unwrap();
            assert_eq!(engine.send_tick(&mut session, &mut progress).unwrap(), 0);
            engine
                .handle_response(&GrblResponse::Ok, &mut progress)
                .unwrap();
            assert_eq!(engine.send_tick(&mut session, &mut progress).unwrap(), 3);
        }
    }

    #[test]
    fn errors_identify_oldest_pending_command_after_window_refill() {
        let commands = vec!["G1 X123.456 Y789.012 F500".to_owned(); 20];
        for mode in [TransferMode::Buffered, TransferMode::Synchronous] {
            let (mut session, mut engine, mut progress) = make_session_and_engine(commands.clone());
            engine.transfer_mode = mode;
            engine.send_tick(&mut session, &mut progress).unwrap();
            engine
                .handle_response(&GrblResponse::Ok, &mut progress)
                .unwrap();
            engine.send_tick(&mut session, &mut progress).unwrap();
            let error = engine
                .handle_response(&GrblResponse::Error(2), &mut progress)
                .unwrap_err();
            let message = error.to_string();
            assert!(message.contains("G-code line 2: G1 X123.456 Y789.012 F500"));
            assert!(!message.contains("air-assist"));
        }
    }

    #[test]
    fn unsupported_coolant_commands_explain_profile_configuration() {
        for command in ["M7", "M8", "M9", " m7 "] {
            let (mut session, mut engine, mut progress) =
                make_session_and_engine(vec![command.to_owned()]);
            engine.send_tick(&mut session, &mut progress).unwrap();
            engine
                .handle_response(&GrblResponse::Error(20), &mut progress)
                .unwrap_err();
            assert!(
                engine
                    .error_message()
                    .unwrap()
                    .contains("air-assist commands and custom G-code")
            );
        }
    }

    #[test]
    fn errors_without_pending_commands_do_not_invent_a_line() {
        let (_, mut engine, mut progress) = make_session_and_engine(vec!["M7".to_owned()]);
        engine
            .handle_response(&GrblResponse::Error(20), &mut progress)
            .unwrap_err();
        assert_eq!(
            engine.error_message(),
            Some("GRBL error 20: Unsupported command")
        );
    }

    #[test]
    fn pending_dwell_uses_only_the_oldest_unacknowledged_block() {
        for (command, seconds) in [
            ("G4 P10", Some(10.0)),
            ("g04p.5", Some(0.5)),
            ("(pump; start) N10 G90 G04.0 P+2 ; wait", Some(2.0)),
            ("G4 (pump) P1.25", Some(1.25)),
            ("G40 P5", None),
            ("G1 X0 (G4 P10)", None),
            ("G4 P-1", None),
            ("G4 PNaN", None),
            ("G4", None),
        ] {
            let (mut session, mut engine, mut progress) =
                make_session_and_engine(["G90", command, "G1 X10"].map(str::to_owned).to_vec());
            engine.send_tick(&mut session, &mut progress).unwrap();
            assert_eq!(engine.pending_dwell_duration(), None);
            engine
                .handle_response(&GrblResponse::Ok, &mut progress)
                .unwrap();
            assert_eq!(
                engine.pending_dwell_duration().map(|d| d.as_secs_f64()),
                seconds,
                "{command}"
            );
            engine
                .handle_response(&GrblResponse::Ok, &mut progress)
                .unwrap();
            assert_eq!(engine.pending_dwell_duration(), None);
        }
    }

    #[test]
    fn pause_and_resume() {
        let commands = vec!["G0 X10".to_string(); 5];
        let (mut session, mut engine, mut progress) = make_session_and_engine(commands);

        engine.pause();
        let sent = engine.send_tick(&mut session, &mut progress).unwrap();
        assert_eq!(sent, 0);

        engine.resume();
        let sent = engine.send_tick(&mut session, &mut progress).unwrap();
        assert!(sent > 0);
    }

    #[test]
    fn cancel_stops_sending() {
        let commands = vec!["G0 X10".to_string(); 5];
        let (mut session, mut engine, mut progress) = make_session_and_engine(commands);

        engine.cancel();
        let sent = engine.send_tick(&mut session, &mut progress).unwrap();
        assert_eq!(sent, 0);
        assert!(engine.is_cancelled());
    }

    #[test]
    fn all_acknowledged_after_all_ok() {
        let commands = vec!["G0 X10".to_string(), "G0 X20".to_string()];
        let (mut session, mut engine, mut progress) = make_session_and_engine(commands);

        engine.send_tick(&mut session, &mut progress).unwrap();
        assert!(!engine.all_acknowledged());

        engine
            .handle_response(&GrblResponse::Ok, &mut progress)
            .unwrap();
        engine
            .handle_response(&GrblResponse::Ok, &mut progress)
            .unwrap();
        assert!(engine.all_acknowledged());
    }

    #[test]
    fn console_log_captures_sent_commands() {
        let commands = vec!["G0 X10".to_string(), "G0 X20".to_string()];
        let (mut session, mut engine, mut progress) = make_session_and_engine(commands);

        engine.send_tick(&mut session, &mut progress).unwrap();

        let log = engine.get_console_entries(10);
        assert!(!log.is_empty());
        assert!(
            log.iter()
                .any(|e| e.direction == ConsoleDirection::Sent && e.content.contains("G0"))
        );
    }

    #[test]
    fn console_log_captures_received_responses() {
        let commands = vec!["G0 X10".to_string()];
        let (mut session, mut engine, mut progress) = make_session_and_engine(commands);

        engine.send_tick(&mut session, &mut progress).unwrap();
        engine
            .handle_response(&GrblResponse::Ok, &mut progress)
            .unwrap();

        let log = engine.get_console_entries(10);
        assert!(
            log.iter()
                .any(|e| e.direction == ConsoleDirection::Received)
        );
    }

    #[test]
    fn pause_position_can_be_set_and_retrieved() {
        let commands = vec!["G0 X10".to_string()];
        let (_, mut engine, _) = make_session_and_engine(commands);

        assert!(engine.get_pause_position().is_none());

        engine.set_pause_position(10.5, 20.5);
        assert_eq!(engine.get_pause_position(), Some((10.5, 20.5)));
    }

    #[test]
    fn console_entries_returned_newest_first() {
        let commands = vec!["G0 X10".to_string(), "G0 X20".to_string()];
        let (mut session, mut engine, mut progress) = make_session_and_engine(commands);

        engine.send_tick(&mut session, &mut progress).unwrap();

        let log = engine.get_console_entries(2);
        // Newest should be first (X20 before X10)
        assert!(log[0].content.contains("G0 X20"));
    }

    #[test]
    fn console_entries_limit_works() {
        let commands = vec![
            "G0 X10".to_string(),
            "G0 X20".to_string(),
            "G0 X30".to_string(),
        ];
        let (mut session, mut engine, mut progress) = make_session_and_engine(commands);

        engine.send_tick(&mut session, &mut progress).unwrap();

        let log = engine.get_console_entries(2);
        assert_eq!(log.len(), 2);
    }
}
