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
use tracing::{debug, warn};

/// Usable GRBL RX buffer in bytes. The firmware buffer is 128, but its ring
/// implementation keeps one byte unusable — planning against the full 128
/// overflows by one byte when the window is exactly full, silently dropping
/// a character, corrupting a line, and deadlocking the ack accounting
/// (field report: ACMER S1 stalling after the first window of short lines).
/// Every major GRBL sender plans against 127.
const GRBL_RX_BUFFER_SIZE: usize = 127;

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

        let mut sent_count = 0;

        while self.next_index < self.commands.len() {
            if self.transfer_mode == TransferMode::Synchronous && self.bytes_in_flight > 0 {
                break;
            }
            let cmd = &self.commands[self.next_index];
            let cmd_size = cmd.len() + 1; // +1 for \n

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
                let message = format!("GRBL error {code}: {msg}");
                self.fail(message.clone(), progress);
                return Err(StreamerError::JobFailed(message));
            }
            GrblResponse::Alarm(code) => {
                self.fail(format!("GRBL alarm {code}"), progress);
                return Err(StreamerError::AlarmDuringJob(*code));
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
