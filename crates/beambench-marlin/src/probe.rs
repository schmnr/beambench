//! Pure command/response sequencing for a read-only Marlin identity probe.

use beambench_gcode::LineProtocolEvent;

use crate::MarlinIdentity;

pub const MARLIN_IDENTITY_COMMAND: &str = "M115";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarlinIdentityProbeOutcome {
    Succeeded,
    Rejected,
    RetryRequested,
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarlinIdentityProbeResult {
    pub identity: MarlinIdentity,
    pub outcome: MarlinIdentityProbeOutcome,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum ProbePhase {
    #[default]
    NotStarted,
    WaitingForAcknowledgement,
    Complete(MarlinIdentityProbeOutcome),
}

/// Deterministic M115 probe state machine. Transport deadlines are owned by
/// the future Marlin session so transcript tests need no clock or hardware.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarlinIdentityProbeSequence {
    phase: ProbePhase,
}

impl MarlinIdentityProbeSequence {
    pub fn begin(&mut self) -> Option<&'static str> {
        if self.phase != ProbePhase::NotStarted {
            return None;
        }
        self.phase = ProbePhase::WaitingForAcknowledgement;
        Some(MARLIN_IDENTITY_COMMAND)
    }

    pub fn observe(&mut self, event: &LineProtocolEvent) {
        if self.phase != ProbePhase::WaitingForAcknowledgement {
            return;
        }
        self.phase = match event {
            LineProtocolEvent::Acknowledged { .. } => {
                ProbePhase::Complete(MarlinIdentityProbeOutcome::Succeeded)
            }
            LineProtocolEvent::CommandError { .. } => {
                ProbePhase::Complete(MarlinIdentityProbeOutcome::Rejected)
            }
            LineProtocolEvent::RetryRequested { .. } => {
                ProbePhase::Complete(MarlinIdentityProbeOutcome::RetryRequested)
            }
            LineProtocolEvent::Busy { .. } | LineProtocolEvent::Informational { .. } => return,
        };
    }

    pub fn timeout(&mut self) {
        if self.phase == ProbePhase::WaitingForAcknowledgement {
            self.phase = ProbePhase::Complete(MarlinIdentityProbeOutcome::TimedOut);
        }
    }

    pub fn is_complete(&self) -> bool {
        matches!(self.phase, ProbePhase::Complete(_))
    }

    pub fn finish(self, identity: MarlinIdentity) -> Option<MarlinIdentityProbeResult> {
        let ProbePhase::Complete(outcome) = self.phase else {
            return None;
        };
        Some(MarlinIdentityProbeResult { identity, outcome })
    }
}

#[cfg(test)]
mod tests {
    use beambench_gcode::classify_marlin_response;

    use super::*;

    #[test]
    fn m115_probe_waits_through_information_and_busy_lines() {
        let mut probe = MarlinIdentityProbeSequence::default();
        assert_eq!(probe.begin(), Some("M115"));
        assert_eq!(probe.begin(), None);

        probe.observe(&classify_marlin_response("FIRMWARE_NAME:Marlin 2.1.3"));
        probe.observe(&classify_marlin_response("busy: processing"));
        assert!(!probe.is_complete());

        probe.observe(&classify_marlin_response("ok"));
        assert!(probe.is_complete());
        assert_eq!(
            probe.finish(MarlinIdentity::default()).unwrap().outcome,
            MarlinIdentityProbeOutcome::Succeeded
        );
    }

    #[test]
    fn errors_resends_and_timeouts_have_distinct_outcomes() {
        let outcome_for = |line: &str| {
            let mut probe = MarlinIdentityProbeSequence::default();
            probe.begin();
            probe.observe(&classify_marlin_response(line));
            probe.finish(MarlinIdentity::default()).unwrap().outcome
        };

        assert_eq!(
            outcome_for("Error:Unknown command: M115"),
            MarlinIdentityProbeOutcome::Rejected
        );
        assert_eq!(
            outcome_for("Resend: 7"),
            MarlinIdentityProbeOutcome::RetryRequested
        );

        let mut timed_out = MarlinIdentityProbeSequence::default();
        timed_out.begin();
        timed_out.timeout();
        assert_eq!(
            timed_out.finish(MarlinIdentity::default()).unwrap().outcome,
            MarlinIdentityProbeOutcome::TimedOut
        );
    }
}
