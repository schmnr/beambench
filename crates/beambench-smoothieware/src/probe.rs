//! Pure command/response sequencing for a read-only Smoothieware identity probe.

use beambench_gcode::LineProtocolEvent;

use crate::SmoothiewareIdentity;

pub const SMOOTHIEWARE_IDENTITY_COMMAND: &str = "M115";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmoothiewareIdentityProbeOutcome {
    Succeeded,
    Rejected,
    TimedOut,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SmoothiewareIdentityProbeResult {
    pub identity: SmoothiewareIdentity,
    pub outcome: SmoothiewareIdentityProbeOutcome,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum ProbePhase {
    #[default]
    NotStarted,
    WaitingForAcknowledgement,
    Complete(SmoothiewareIdentityProbeOutcome),
}

/// Deterministic M115 probe state machine. Transport deadlines are owned by
/// the future serial session so transcript tests need no clock or hardware.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SmoothiewareIdentityProbeSequence {
    phase: ProbePhase,
}

impl SmoothiewareIdentityProbeSequence {
    pub fn begin(&mut self) -> Option<&'static str> {
        if self.phase != ProbePhase::NotStarted {
            return None;
        }
        self.phase = ProbePhase::WaitingForAcknowledgement;
        Some(SMOOTHIEWARE_IDENTITY_COMMAND)
    }

    pub fn observe(&mut self, event: &LineProtocolEvent) {
        if self.phase != ProbePhase::WaitingForAcknowledgement {
            return;
        }
        self.phase = match event {
            LineProtocolEvent::Acknowledged { .. } => {
                ProbePhase::Complete(SmoothiewareIdentityProbeOutcome::Succeeded)
            }
            LineProtocolEvent::CommandError { .. } | LineProtocolEvent::RetryRequested { .. } => {
                ProbePhase::Complete(SmoothiewareIdentityProbeOutcome::Rejected)
            }
            LineProtocolEvent::Busy { .. } | LineProtocolEvent::Informational { .. } => return,
        };
    }

    pub fn timeout(&mut self) {
        if self.phase == ProbePhase::WaitingForAcknowledgement {
            self.phase = ProbePhase::Complete(SmoothiewareIdentityProbeOutcome::TimedOut);
        }
    }

    pub fn is_complete(&self) -> bool {
        matches!(self.phase, ProbePhase::Complete(_))
    }

    pub fn finish(self, identity: SmoothiewareIdentity) -> Option<SmoothiewareIdentityProbeResult> {
        let ProbePhase::Complete(outcome) = self.phase else {
            return None;
        };
        Some(SmoothiewareIdentityProbeResult { identity, outcome })
    }
}

#[cfg(test)]
mod tests {
    use beambench_gcode::classify_smoothieware_response;

    use super::*;

    #[test]
    fn m115_probe_waits_through_identity_and_banner_lines() {
        let mut probe = SmoothiewareIdentityProbeSequence::default();
        assert_eq!(probe.begin(), Some("M115"));
        assert_eq!(probe.begin(), None);

        probe.observe(&classify_smoothieware_response("Smoothie ok"));
        probe.observe(&classify_smoothieware_response(
            "FIRMWARE_NAME:Smoothieware, FIRMWARE_VERSION:edge",
        ));
        assert!(!probe.is_complete());

        probe.observe(&classify_smoothieware_response("ok"));
        assert!(probe.is_complete());
        assert_eq!(
            probe
                .finish(SmoothiewareIdentity::default())
                .unwrap()
                .outcome,
            SmoothiewareIdentityProbeOutcome::Succeeded
        );
    }

    #[test]
    fn errors_and_timeouts_have_distinct_outcomes() {
        let mut rejected = SmoothiewareIdentityProbeSequence::default();
        rejected.begin();
        rejected.observe(&classify_smoothieware_response("ok - Invalid M115"));
        assert_eq!(
            rejected
                .finish(SmoothiewareIdentity::default())
                .unwrap()
                .outcome,
            SmoothiewareIdentityProbeOutcome::Rejected
        );

        let mut timed_out = SmoothiewareIdentityProbeSequence::default();
        timed_out.begin();
        timed_out.timeout();
        assert_eq!(
            timed_out
                .finish(SmoothiewareIdentity::default())
                .unwrap()
                .outcome,
            SmoothiewareIdentityProbeOutcome::TimedOut
        );
    }
}
