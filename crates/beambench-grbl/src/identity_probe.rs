//! Bounded, read-only GRBL-family controller identity probing.
//!
//! The probe deliberately retains only normalized identity and per-command
//! outcomes. Raw controller lines continue through the session console and
//! identity detector; they are not duplicated into the result.

use std::time::Duration;

use beambench_common::{GrblFamilyIdentity, GrblFamilyIdentityStatus};

use crate::{commands, parser::GrblResponse};

/// Default maximum time to wait for each identity command's terminal `ok` or
/// `error:N` response.
pub const DEFAULT_IDENTITY_PROBE_COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

/// Default delay between non-blocking serial polls while an identity command
/// is in flight.
pub const DEFAULT_IDENTITY_PROBE_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Timing policy for one explicit identity-probe operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrblFamilyIdentityProbeConfig {
    pub command_timeout: Duration,
    pub poll_interval: Duration,
}

impl Default for GrblFamilyIdentityProbeConfig {
    fn default() -> Self {
        Self {
            command_timeout: DEFAULT_IDENTITY_PROBE_COMMAND_TIMEOUT,
            poll_interval: DEFAULT_IDENTITY_PROBE_POLL_INTERVAL,
        }
    }
}

/// Terminal outcome for one read-only command in the identity sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrblFamilyIdentityProbeOutcome {
    /// The command was not reached because an earlier command did not finish.
    NotAttempted,
    /// Existing evidence was already conclusive, so the extended query was
    /// unnecessary.
    NotNeeded,
    /// The controller acknowledged the command with `ok`.
    Succeeded,
    /// The controller rejected the command with `error:N`.
    Rejected(u8),
    /// No terminal response arrived before the configured deadline.
    TimedOut,
}

/// Normalized result of an explicit `$I`/`$I+` probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrblFamilyIdentityProbeResult {
    pub identity: GrblFamilyIdentity,
    pub controller_info: GrblFamilyIdentityProbeOutcome,
    pub extended_controller_info: GrblFamilyIdentityProbeOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IdentityProbeCommand {
    ControllerInfo,
    ExtendedControllerInfo,
}

impl IdentityProbeCommand {
    pub(crate) fn wire_command(self) -> String {
        match self {
            Self::ControllerInfo => commands::controller_info(),
            Self::ExtendedControllerInfo => commands::extended_controller_info(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdentityProbePhase {
    NotStarted,
    WaitingForControllerInfo,
    WaitingForExtendedControllerInfo,
    Complete,
}

/// Pure command/response sequencer. Transport polling and deadlines are owned
/// by `GrblSession` so this state machine remains deterministic in replay tests.
pub(crate) struct GrblFamilyIdentityProbeSequence {
    phase: IdentityProbePhase,
    controller_info: GrblFamilyIdentityProbeOutcome,
    extended_controller_info: GrblFamilyIdentityProbeOutcome,
}

impl Default for GrblFamilyIdentityProbeSequence {
    fn default() -> Self {
        Self {
            phase: IdentityProbePhase::NotStarted,
            controller_info: GrblFamilyIdentityProbeOutcome::NotAttempted,
            extended_controller_info: GrblFamilyIdentityProbeOutcome::NotAttempted,
        }
    }
}

impl GrblFamilyIdentityProbeSequence {
    pub(crate) fn begin(&mut self) -> Option<IdentityProbeCommand> {
        if self.phase != IdentityProbePhase::NotStarted {
            return None;
        }

        self.phase = IdentityProbePhase::WaitingForControllerInfo;
        Some(IdentityProbeCommand::ControllerInfo)
    }

    pub(crate) fn observe_response(
        &mut self,
        response: &GrblResponse,
        identity: &GrblFamilyIdentity,
    ) -> Option<IdentityProbeCommand> {
        match (self.phase, response) {
            (IdentityProbePhase::WaitingForControllerInfo, GrblResponse::Ok) => {
                self.controller_info = GrblFamilyIdentityProbeOutcome::Succeeded;
                if identity_is_conclusive(identity) {
                    self.extended_controller_info = GrblFamilyIdentityProbeOutcome::NotNeeded;
                    self.phase = IdentityProbePhase::Complete;
                    None
                } else {
                    self.phase = IdentityProbePhase::WaitingForExtendedControllerInfo;
                    Some(IdentityProbeCommand::ExtendedControllerInfo)
                }
            }
            (IdentityProbePhase::WaitingForControllerInfo, GrblResponse::Error(code)) => {
                self.controller_info = GrblFamilyIdentityProbeOutcome::Rejected(*code);
                self.phase = IdentityProbePhase::Complete;
                None
            }
            (IdentityProbePhase::WaitingForExtendedControllerInfo, GrblResponse::Ok) => {
                self.extended_controller_info = GrblFamilyIdentityProbeOutcome::Succeeded;
                self.phase = IdentityProbePhase::Complete;
                None
            }
            (IdentityProbePhase::WaitingForExtendedControllerInfo, GrblResponse::Error(code)) => {
                self.extended_controller_info = GrblFamilyIdentityProbeOutcome::Rejected(*code);
                self.phase = IdentityProbePhase::Complete;
                None
            }
            _ => None,
        }
    }

    pub(crate) fn timeout_active_command(&mut self) {
        match self.phase {
            IdentityProbePhase::WaitingForControllerInfo => {
                self.controller_info = GrblFamilyIdentityProbeOutcome::TimedOut;
                self.phase = IdentityProbePhase::Complete;
            }
            IdentityProbePhase::WaitingForExtendedControllerInfo => {
                self.extended_controller_info = GrblFamilyIdentityProbeOutcome::TimedOut;
                self.phase = IdentityProbePhase::Complete;
            }
            IdentityProbePhase::NotStarted | IdentityProbePhase::Complete => {}
        }
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.phase == IdentityProbePhase::Complete
    }

    pub(crate) fn finish(
        self,
        identity: GrblFamilyIdentity,
    ) -> Option<GrblFamilyIdentityProbeResult> {
        self.is_complete().then_some(GrblFamilyIdentityProbeResult {
            identity,
            controller_info: self.controller_info,
            extended_controller_info: self.extended_controller_info,
        })
    }
}

fn identity_is_conclusive(identity: &GrblFamilyIdentity) -> bool {
    matches!(
        identity.status,
        GrblFamilyIdentityStatus::Identified | GrblFamilyIdentityStatus::Conflicting
    )
}

#[cfg(test)]
mod tests {
    use beambench_common::{GrblFamilyDialect, GrblFamilyIdentityEvidence};

    use super::*;

    fn identified_fluid_nc() -> GrblFamilyIdentity {
        GrblFamilyIdentity {
            dialect: GrblFamilyDialect::FluidNc,
            status: GrblFamilyIdentityStatus::Identified,
            firmware_identity: Some("FluidNC".to_string()),
            firmware_version: Some("4.0.3".to_string()),
            evidence: vec![GrblFamilyIdentityEvidence::ControllerInfoVersion],
        }
    }

    #[test]
    fn begins_with_controller_info_once() {
        let mut sequence = GrblFamilyIdentityProbeSequence::default();

        assert_eq!(sequence.begin(), Some(IdentityProbeCommand::ControllerInfo));
        assert_eq!(sequence.begin(), None);
        assert!(!sequence.is_complete());
    }

    #[test]
    fn conclusive_basic_identity_skips_extended_query() {
        let mut sequence = GrblFamilyIdentityProbeSequence::default();
        sequence.begin();

        let next = sequence.observe_response(&GrblResponse::Ok, &identified_fluid_nc());
        let result = sequence.finish(identified_fluid_nc()).unwrap();

        assert_eq!(next, None);
        assert_eq!(
            result.controller_info,
            GrblFamilyIdentityProbeOutcome::Succeeded
        );
        assert_eq!(
            result.extended_controller_info,
            GrblFamilyIdentityProbeOutcome::NotNeeded
        );
    }

    #[test]
    fn inconclusive_basic_identity_requests_extended_query() {
        let mut sequence = GrblFamilyIdentityProbeSequence::default();
        sequence.begin();

        let next = sequence.observe_response(&GrblResponse::Ok, &GrblFamilyIdentity::default());

        assert_eq!(next, Some(IdentityProbeCommand::ExtendedControllerInfo));
        assert!(!sequence.is_complete());
    }

    #[test]
    fn extended_rejection_is_a_completed_probe_result() {
        let mut sequence = GrblFamilyIdentityProbeSequence::default();
        sequence.begin();
        sequence.observe_response(&GrblResponse::Ok, &GrblFamilyIdentity::default());

        let next =
            sequence.observe_response(&GrblResponse::Error(3), &GrblFamilyIdentity::default());
        let result = sequence.finish(GrblFamilyIdentity::default()).unwrap();

        assert_eq!(next, None);
        assert_eq!(
            result.extended_controller_info,
            GrblFamilyIdentityProbeOutcome::Rejected(3)
        );
    }

    #[test]
    fn basic_timeout_prevents_extended_query() {
        let mut sequence = GrblFamilyIdentityProbeSequence::default();
        sequence.begin();

        sequence.timeout_active_command();
        let result = sequence.finish(GrblFamilyIdentity::default()).unwrap();

        assert_eq!(
            result.controller_info,
            GrblFamilyIdentityProbeOutcome::TimedOut
        );
        assert_eq!(
            result.extended_controller_info,
            GrblFamilyIdentityProbeOutcome::NotAttempted
        );
    }

    #[test]
    fn unrelated_responses_do_not_advance_the_sequence() {
        let mut sequence = GrblFamilyIdentityProbeSequence::default();
        sequence.begin();

        for response in [
            GrblResponse::Message("hello".to_string()),
            GrblResponse::Alarm(1),
            GrblResponse::Unknown("noise".to_string()),
        ] {
            assert_eq!(
                sequence.observe_response(&response, &GrblFamilyIdentity::default()),
                None
            );
        }

        assert!(!sequence.is_complete());
    }
}
