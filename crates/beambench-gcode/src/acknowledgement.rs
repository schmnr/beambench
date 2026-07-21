use std::time::{Duration, Instant};

use thiserror::Error;

use crate::LineProtocolEvent;

/// Limits for a conservative one-command-per-ack flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AckFlowConfig {
    /// Maximum encoded command size, including the terminating newline.
    pub max_line_bytes: usize,
    /// Maximum time without an acknowledgement or explicit busy response.
    pub acknowledgement_timeout: Duration,
}

impl Default for AckFlowConfig {
    fn default() -> Self {
        Self {
            max_line_bytes: 1_024,
            acknowledgement_timeout: Duration::from_secs(30),
        }
    }
}

/// A command that may be written to the adapter's transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadyLine<'a> {
    pub index: usize,
    pub line: &'a str,
}

/// Transport-neutral progress for diagnostics and adapter integration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AckFlowProgress {
    pub total_lines: usize,
    pub sent_lines: usize,
    pub acknowledged_lines: usize,
    pub in_flight_index: Option<usize>,
    pub cancelled: bool,
    pub failed: bool,
}

/// State change produced by one classified controller response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckFlowUpdate {
    Acknowledged { index: usize },
    Busy { index: usize },
    Ignored,
}

/// Fail-closed acknowledgement-flow errors.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AckFlowError {
    #[error("acknowledgement timeout must be greater than zero")]
    ZeroAcknowledgementTimeout,
    #[error("maximum line size must include at least a newline byte")]
    ZeroMaximumLineSize,
    #[error("command {index} contains an embedded line ending")]
    EmbeddedLineEnding { index: usize },
    #[error("command {index} is {actual_bytes} bytes including newline; limit is {max_bytes}")]
    LineTooLong {
        index: usize,
        actual_bytes: usize,
        max_bytes: usize,
    },
    #[error("command {index} is already waiting for acknowledgement")]
    CommandAlreadyInFlight { index: usize },
    #[error("no command is ready to send")]
    NoLineReady,
    #[error("received an acknowledgement with no command in flight")]
    UnexpectedAcknowledgement,
    #[error("controller requested an unsupported resend{line_suffix}: {message}", line_suffix = resend_suffix(*line_number))]
    UnsupportedResend {
        line_number: Option<u32>,
        message: String,
    },
    #[error("controller rejected the command: {message}")]
    ControllerRejected { message: String },
    #[error("command {index} timed out waiting for acknowledgement")]
    AcknowledgementTimeout { index: usize },
    #[error("acknowledgement flow was cancelled")]
    Cancelled,
    #[error("acknowledgement flow has failed: {message}")]
    Failed { message: String },
}

fn resend_suffix(line_number: Option<u32>) -> String {
    line_number
        .map(|number| format!(" for line {number}"))
        .unwrap_or_default()
}

#[derive(Debug, Clone, Copy)]
struct InFlightLine {
    index: usize,
    last_activity_at: Instant,
}

/// Conservative line flow shared by acknowledgement-based G-code adapters.
///
/// The caller writes [`ReadyLine::line`] to its transport and calls
/// [`mark_sent`](Self::mark_sent) only after that write succeeds. Until the
/// controller acknowledges the line, no later command becomes ready.
pub struct AcknowledgedLineFlow {
    commands: Vec<String>,
    next_index: usize,
    acknowledged_lines: usize,
    in_flight: Option<InFlightLine>,
    cancelled: bool,
    failure: Option<String>,
    config: AckFlowConfig,
}

impl AcknowledgedLineFlow {
    pub fn new(commands: Vec<String>, config: AckFlowConfig) -> Result<Self, AckFlowError> {
        if config.max_line_bytes == 0 {
            return Err(AckFlowError::ZeroMaximumLineSize);
        }
        if config.acknowledgement_timeout.is_zero() {
            return Err(AckFlowError::ZeroAcknowledgementTimeout);
        }

        for (index, line) in commands.iter().enumerate() {
            if line.contains(['\r', '\n']) {
                return Err(AckFlowError::EmbeddedLineEnding { index });
            }
            let actual_bytes = line.len().saturating_add(1);
            if actual_bytes > config.max_line_bytes {
                return Err(AckFlowError::LineTooLong {
                    index,
                    actual_bytes,
                    max_bytes: config.max_line_bytes,
                });
            }
        }

        Ok(Self {
            commands,
            next_index: 0,
            acknowledged_lines: 0,
            in_flight: None,
            cancelled: false,
            failure: None,
            config,
        })
    }

    /// Return the only command currently eligible for transport write.
    pub fn ready_line(&self) -> Option<ReadyLine<'_>> {
        if self.cancelled || self.failure.is_some() || self.in_flight.is_some() {
            return None;
        }
        self.commands.get(self.next_index).map(|line| ReadyLine {
            index: self.next_index,
            line,
        })
    }

    /// Record a successful transport write for the current ready command.
    pub fn mark_sent(&mut self, now: Instant) -> Result<usize, AckFlowError> {
        self.ensure_active()?;
        if let Some(in_flight) = self.in_flight {
            return Err(AckFlowError::CommandAlreadyInFlight {
                index: in_flight.index,
            });
        }
        if self.next_index >= self.commands.len() {
            return Err(AckFlowError::NoLineReady);
        }

        let index = self.next_index;
        self.next_index += 1;
        self.in_flight = Some(InFlightLine {
            index,
            last_activity_at: now,
        });
        Ok(index)
    }

    /// Apply one dialect-classified controller response.
    pub fn observe(
        &mut self,
        event: &LineProtocolEvent,
        now: Instant,
    ) -> Result<AckFlowUpdate, AckFlowError> {
        self.ensure_active()?;
        match event {
            LineProtocolEvent::Acknowledged { .. } => {
                let Some(in_flight) = self.in_flight.take() else {
                    return self.fail(AckFlowError::UnexpectedAcknowledgement);
                };
                self.acknowledged_lines += 1;
                Ok(AckFlowUpdate::Acknowledged {
                    index: in_flight.index,
                })
            }
            LineProtocolEvent::Busy { .. } => {
                let Some(in_flight) = self.in_flight.as_mut() else {
                    return Ok(AckFlowUpdate::Ignored);
                };
                in_flight.last_activity_at = now;
                Ok(AckFlowUpdate::Busy {
                    index: in_flight.index,
                })
            }
            LineProtocolEvent::RetryRequested {
                line_number,
                message,
            } => self.fail(AckFlowError::UnsupportedResend {
                line_number: *line_number,
                message: message.clone(),
            }),
            LineProtocolEvent::CommandError { message } => {
                self.fail(AckFlowError::ControllerRejected {
                    message: message.clone(),
                })
            }
            LineProtocolEvent::Informational { .. } => Ok(AckFlowUpdate::Ignored),
        }
    }

    /// Fail when the command in flight has received no acknowledgement or busy
    /// activity within the configured timeout.
    pub fn check_timeout(&mut self, now: Instant) -> Result<(), AckFlowError> {
        self.ensure_active()?;
        let Some(in_flight) = self.in_flight else {
            return Ok(());
        };
        if now.saturating_duration_since(in_flight.last_activity_at)
            < self.config.acknowledgement_timeout
        {
            return Ok(());
        }
        self.fail(AckFlowError::AcknowledgementTimeout {
            index: in_flight.index,
        })
    }

    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.in_flight = None;
    }

    pub fn is_complete(&self) -> bool {
        !self.cancelled
            && self.failure.is_none()
            && self.in_flight.is_none()
            && self.acknowledged_lines == self.commands.len()
    }

    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    pub fn progress(&self) -> AckFlowProgress {
        AckFlowProgress {
            total_lines: self.commands.len(),
            sent_lines: self.next_index,
            acknowledged_lines: self.acknowledged_lines,
            in_flight_index: self.in_flight.map(|line| line.index),
            cancelled: self.cancelled,
            failed: self.failure.is_some(),
        }
    }

    fn ensure_active(&self) -> Result<(), AckFlowError> {
        if self.cancelled {
            return Err(AckFlowError::Cancelled);
        }
        if let Some(message) = &self.failure {
            return Err(AckFlowError::Failed {
                message: message.clone(),
            });
        }
        Ok(())
    }

    fn fail<T>(&mut self, error: AckFlowError) -> Result<T, AckFlowError> {
        self.failure = Some(error.to_string());
        self.in_flight = None;
        Err(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flow(commands: &[&str], timeout: Duration) -> AcknowledgedLineFlow {
        AcknowledgedLineFlow::new(
            commands.iter().map(|line| (*line).to_string()).collect(),
            AckFlowConfig {
                max_line_bytes: 64,
                acknowledgement_timeout: timeout,
            },
        )
        .unwrap()
    }

    #[test]
    fn exposes_only_one_line_until_acknowledged() {
        let started = Instant::now();
        let mut flow = flow(&["G0 X0", "G1 X10"], Duration::from_secs(5));

        assert_eq!(flow.ready_line().unwrap().line, "G0 X0");
        assert_eq!(flow.mark_sent(started), Ok(0));
        assert_eq!(flow.ready_line(), None);
        assert_eq!(
            flow.observe(&LineProtocolEvent::Acknowledged { detail: None }, started),
            Ok(AckFlowUpdate::Acknowledged { index: 0 })
        );
        assert_eq!(flow.ready_line().unwrap().line, "G1 X10");
        assert_eq!(flow.mark_sent(started), Ok(1));
        flow.observe(&LineProtocolEvent::Acknowledged { detail: None }, started)
            .unwrap();

        assert!(flow.is_complete());
        assert_eq!(
            flow.progress(),
            AckFlowProgress {
                total_lines: 2,
                sent_lines: 2,
                acknowledged_lines: 2,
                in_flight_index: None,
                cancelled: false,
                failed: false,
            }
        );
    }

    #[test]
    fn busy_activity_extends_the_acknowledgement_deadline() {
        let started = Instant::now();
        let mut flow = flow(&["M400"], Duration::from_secs(10));
        flow.mark_sent(started).unwrap();

        assert_eq!(
            flow.observe(
                &LineProtocolEvent::Busy {
                    message: "busy: processing".to_string(),
                },
                started + Duration::from_secs(9),
            ),
            Ok(AckFlowUpdate::Busy { index: 0 })
        );
        assert!(
            flow.check_timeout(started + Duration::from_secs(18))
                .is_ok()
        );
        assert_eq!(
            flow.check_timeout(started + Duration::from_secs(19)),
            Err(AckFlowError::AcknowledgementTimeout { index: 0 })
        );
    }

    #[test]
    fn controller_errors_and_resends_fail_closed() {
        let started = Instant::now();
        let mut rejected = flow(&["M3 S100"], Duration::from_secs(5));
        rejected.mark_sent(started).unwrap();
        assert_eq!(
            rejected.observe(
                &LineProtocolEvent::CommandError {
                    message: "Error:Unknown command".to_string(),
                },
                started,
            ),
            Err(AckFlowError::ControllerRejected {
                message: "Error:Unknown command".to_string(),
            })
        );
        assert!(rejected.failure().is_some());

        let mut resend = flow(&["G1 X10"], Duration::from_secs(5));
        resend.mark_sent(started).unwrap();
        assert_eq!(
            resend.observe(
                &LineProtocolEvent::RetryRequested {
                    line_number: Some(12),
                    message: "Resend: 12".to_string(),
                },
                started,
            ),
            Err(AckFlowError::UnsupportedResend {
                line_number: Some(12),
                message: "Resend: 12".to_string(),
            })
        );
        assert!(resend.failure().is_some());
    }

    #[test]
    fn invalid_commands_are_rejected_before_any_transport_write() {
        assert!(matches!(
            AcknowledgedLineFlow::new(vec!["G0 X0\nM3".to_string()], AckFlowConfig::default()),
            Err(AckFlowError::EmbeddedLineEnding { index: 0 })
        ));
        assert!(matches!(
            AcknowledgedLineFlow::new(
                vec!["G1 X123".to_string()],
                AckFlowConfig {
                    max_line_bytes: 4,
                    acknowledgement_timeout: Duration::from_secs(1),
                }
            ),
            Err(AckFlowError::LineTooLong { index: 0, .. })
        ));
    }

    #[test]
    fn unmatched_acknowledgement_fails_as_a_desynchronized_stream() {
        let now = Instant::now();
        let mut flow = flow(&["G0 X0"], Duration::from_secs(5));
        assert_eq!(
            flow.observe(&LineProtocolEvent::Acknowledged { detail: None }, now),
            Err(AckFlowError::UnexpectedAcknowledgement)
        );
        assert!(flow.failure().is_some());
    }

    #[test]
    fn cancellation_stops_the_flow() {
        let mut flow = flow(&["G0 X0"], Duration::from_secs(5));
        flow.cancel();
        assert_eq!(flow.ready_line(), None);
        assert_eq!(flow.mark_sent(Instant::now()), Err(AckFlowError::Cancelled));
        assert!(flow.progress().cancelled);
    }
}
