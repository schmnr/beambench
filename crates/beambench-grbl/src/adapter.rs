//! Controller-specific policy for GRBL-family adapters.
//!
//! The wire protocol remains shared with [`crate::GrblSession`]. This module
//! supplies the identity, maturity, transport, and capability contract that a
//! named family adapter must satisfy before the service can expose it.

use beambench_common::{
    ControllerDriverId, ControllerEvidenceState, ControllerModel, ControllerProductTier,
    DeviceCapabilities, GrblFamilyDialect, GrblFamilyIdentityEvidence, GrblFamilyIdentityStatus,
    TransportKind,
};
use thiserror::Error;

use crate::{
    GrblFamilyIdentityProbeOutcome, GrblFamilyIdentityProbeResult, GrblResponse, parse_response,
};

/// Stable metadata and fail-closed runtime policy for one GRBL-family adapter.
#[derive(Debug, Clone, PartialEq)]
pub struct GrblFamilyAdapterDescriptor {
    pub driver: ControllerDriverId,
    pub dialect: GrblFamilyDialect,
    pub controller_model: ControllerModel,
    pub product_tier: ControllerProductTier,
    pub evidence_state: ControllerEvidenceState,
    pub transport_kind: TransportKind,
    pub capabilities: DeviceCapabilities,
}

/// Why a named GRBL-family adapter refused a probe result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum GrblFamilyAdapterError {
    #[error("controller information probe did not complete successfully: {outcome:?}")]
    ControllerInfoProbeIncomplete {
        outcome: GrblFamilyIdentityProbeOutcome,
    },

    #[error("extended controller information probe did not complete successfully: {outcome:?}")]
    ExtendedControllerInfoProbeIncomplete {
        outcome: GrblFamilyIdentityProbeOutcome,
    },

    #[error("expected {expected:?} identity, detected {detected:?}")]
    IdentityMismatch {
        expected: GrblFamilyDialect,
        detected: GrblFamilyDialect,
    },

    #[error("controller identity is not conclusive: {status:?}")]
    IdentityNotConclusive { status: GrblFamilyIdentityStatus },

    #[error("controller identity lacks the required controller-information evidence")]
    MissingControllerInfoEvidence,

    #[error("controller identity lacks the required firmware-identity evidence")]
    MissingFirmwareIdentityEvidence,
}

/// Behavior shared by named adapters in the GRBL protocol family.
pub trait GrblFamilyAdapter {
    fn descriptor(&self) -> GrblFamilyAdapterDescriptor;

    /// Validate backend-owned identity evidence before activating this adapter.
    fn validate_probe(
        &self,
        probe: &GrblFamilyIdentityProbeResult,
    ) -> Result<(), GrblFamilyAdapterError>;

    /// Parse a response using the shared GRBL sender contract.
    ///
    /// Named adapters can override this when their dialect introduces a
    /// response that cannot be represented by the common parser.
    fn parse_response(&self, line: &str) -> GrblResponse {
        parse_response(line)
    }
}

/// Backward-compatible name retained for downstream serial-adapter imports.
pub use GrblFamilyAdapter as GrblFamilySerialAdapter;

/// FluidNC's serial adapter core.
///
/// FluidNC documents its normal sender protocol as GRBL-compatible, so the
/// adapter deliberately reuses the existing parser, command builders, session,
/// and acknowledgement flow. Activation still requires exact FluidNC evidence
/// from `$I`; a startup banner alone is insufficient.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FluidNcSerialAdapter;

impl FluidNcSerialAdapter {
    pub const fn new() -> Self {
        Self
    }
}

impl GrblFamilyAdapter for FluidNcSerialAdapter {
    fn descriptor(&self) -> GrblFamilyAdapterDescriptor {
        GrblFamilyAdapterDescriptor {
            driver: ControllerDriverId::FluidNc,
            dialect: GrblFamilyDialect::FluidNc,
            controller_model: ControllerModel::FluidNc,
            product_tier: ControllerProductTier::Experimental,
            evidence_state: ControllerEvidenceState::Emulated,
            transport_kind: TransportKind::Serial,
            capabilities: DeviceCapabilities::experimental_named_grbl_family(),
        }
    }

    fn validate_probe(
        &self,
        probe: &GrblFamilyIdentityProbeResult,
    ) -> Result<(), GrblFamilyAdapterError> {
        if probe.controller_info != GrblFamilyIdentityProbeOutcome::Succeeded {
            return Err(GrblFamilyAdapterError::ControllerInfoProbeIncomplete {
                outcome: probe.controller_info,
            });
        }
        if probe.identity.dialect != GrblFamilyDialect::FluidNc {
            return Err(GrblFamilyAdapterError::IdentityMismatch {
                expected: GrblFamilyDialect::FluidNc,
                detected: probe.identity.dialect,
            });
        }
        if probe.identity.status != GrblFamilyIdentityStatus::Identified {
            return Err(GrblFamilyAdapterError::IdentityNotConclusive {
                status: probe.identity.status,
            });
        }
        if !probe
            .identity
            .evidence
            .contains(&GrblFamilyIdentityEvidence::ControllerInfoVersion)
        {
            return Err(GrblFamilyAdapterError::MissingControllerInfoEvidence);
        }

        Ok(())
    }
}

/// FluidNC's TCP/Telnet adapter policy.
///
/// The network stream carries the same GRBL sender contract and requires the
/// same exact `$I` identity evidence as the serial adapter. Serial and network
/// remain separate compatibility rows through the descriptor transport kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FluidNcNetworkAdapter;

impl FluidNcNetworkAdapter {
    pub const fn new() -> Self {
        Self
    }
}

impl GrblFamilyAdapter for FluidNcNetworkAdapter {
    fn descriptor(&self) -> GrblFamilyAdapterDescriptor {
        let mut descriptor = FluidNcSerialAdapter::new().descriptor();
        descriptor.transport_kind = TransportKind::Tcp;
        descriptor
    }

    fn validate_probe(
        &self,
        probe: &GrblFamilyIdentityProbeResult,
    ) -> Result<(), GrblFamilyAdapterError> {
        FluidNcSerialAdapter::new().validate_probe(probe)
    }
}

/// grblHAL's serial adapter core.
///
/// grblHAL is based on the GRBL 1.1 sender contract but can expose additional
/// states, report fields, settings, and commands. The adapter reuses the
/// shared protocol session while retaining conservative capabilities.
/// Activation requires the standalone `[FIRMWARE:grblHAL]` evidence emitted
/// by extended controller information; a GRBL-compatible or rebranded banner
/// is not enough.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GrblHalSerialAdapter;

impl GrblHalSerialAdapter {
    pub const fn new() -> Self {
        Self
    }
}

impl GrblFamilyAdapter for GrblHalSerialAdapter {
    fn descriptor(&self) -> GrblFamilyAdapterDescriptor {
        GrblFamilyAdapterDescriptor {
            driver: ControllerDriverId::GrblHal,
            dialect: GrblFamilyDialect::GrblHal,
            controller_model: ControllerModel::GrblHal,
            product_tier: ControllerProductTier::Experimental,
            evidence_state: ControllerEvidenceState::Emulated,
            transport_kind: TransportKind::Serial,
            capabilities: DeviceCapabilities::experimental_named_grbl_family(),
        }
    }

    fn validate_probe(
        &self,
        probe: &GrblFamilyIdentityProbeResult,
    ) -> Result<(), GrblFamilyAdapterError> {
        if probe.controller_info != GrblFamilyIdentityProbeOutcome::Succeeded {
            return Err(GrblFamilyAdapterError::ControllerInfoProbeIncomplete {
                outcome: probe.controller_info,
            });
        }
        if !matches!(
            probe.extended_controller_info,
            GrblFamilyIdentityProbeOutcome::Succeeded | GrblFamilyIdentityProbeOutcome::NotNeeded
        ) {
            return Err(
                GrblFamilyAdapterError::ExtendedControllerInfoProbeIncomplete {
                    outcome: probe.extended_controller_info,
                },
            );
        }
        if probe.identity.dialect != GrblFamilyDialect::GrblHal {
            return Err(GrblFamilyAdapterError::IdentityMismatch {
                expected: GrblFamilyDialect::GrblHal,
                detected: probe.identity.dialect,
            });
        }
        if probe.identity.status != GrblFamilyIdentityStatus::Identified {
            return Err(GrblFamilyAdapterError::IdentityNotConclusive {
                status: probe.identity.status,
            });
        }
        if !probe
            .identity
            .evidence
            .contains(&GrblFamilyIdentityEvidence::FirmwareIdentityMessage)
        {
            return Err(GrblFamilyAdapterError::MissingFirmwareIdentityEvidence);
        }

        Ok(())
    }
}

/// grblHAL's TCP/Telnet adapter policy.
///
/// Network capability depends on the controller build's networking plugin,
/// while activation still requires grblHAL's exact extended `$I+` evidence.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GrblHalNetworkAdapter;

impl GrblHalNetworkAdapter {
    pub const fn new() -> Self {
        Self
    }
}

impl GrblFamilyAdapter for GrblHalNetworkAdapter {
    fn descriptor(&self) -> GrblFamilyAdapterDescriptor {
        let mut descriptor = GrblHalSerialAdapter::new().descriptor();
        descriptor.transport_kind = TransportKind::Tcp;
        descriptor
    }

    fn validate_probe(
        &self,
        probe: &GrblFamilyIdentityProbeResult,
    ) -> Result<(), GrblFamilyAdapterError> {
        GrblHalSerialAdapter::new().validate_probe(probe)
    }
}

#[cfg(test)]
mod tests {
    use beambench_common::{GrblFamilyIdentity, MachineRunState};

    use super::*;

    fn exact_probe() -> GrblFamilyIdentityProbeResult {
        GrblFamilyIdentityProbeResult {
            identity: GrblFamilyIdentity {
                dialect: GrblFamilyDialect::FluidNc,
                status: GrblFamilyIdentityStatus::Identified,
                firmware_identity: Some("FluidNC".to_string()),
                firmware_version: Some("4.0.3".to_string()),
                evidence: vec![GrblFamilyIdentityEvidence::ControllerInfoVersion],
            },
            controller_info: GrblFamilyIdentityProbeOutcome::Succeeded,
            extended_controller_info: GrblFamilyIdentityProbeOutcome::NotNeeded,
        }
    }

    fn exact_grbl_hal_probe() -> GrblFamilyIdentityProbeResult {
        GrblFamilyIdentityProbeResult {
            identity: GrblFamilyIdentity {
                dialect: GrblFamilyDialect::GrblHal,
                status: GrblFamilyIdentityStatus::Identified,
                firmware_identity: Some("grblHAL".to_string()),
                firmware_version: Some("1.1f.20260712".to_string()),
                evidence: vec![GrblFamilyIdentityEvidence::FirmwareIdentityMessage],
            },
            controller_info: GrblFamilyIdentityProbeOutcome::Succeeded,
            extended_controller_info: GrblFamilyIdentityProbeOutcome::Succeeded,
        }
    }

    #[test]
    fn descriptor_is_explicit_experimental_and_fail_closed() {
        let descriptor = FluidNcSerialAdapter::new().descriptor();

        assert_eq!(descriptor.driver, ControllerDriverId::FluidNc);
        assert_eq!(descriptor.dialect, GrblFamilyDialect::FluidNc);
        assert_eq!(descriptor.controller_model, ControllerModel::FluidNc);
        assert_eq!(descriptor.product_tier, ControllerProductTier::Experimental);
        assert_eq!(descriptor.evidence_state, ControllerEvidenceState::Emulated);
        assert_eq!(descriptor.transport_kind, TransportKind::Serial);
        assert!(descriptor.capabilities.can_home);
        assert!(descriptor.capabilities.can_jog);
        assert!(descriptor.capabilities.can_pause_resume);
        assert!(descriptor.capabilities.can_frame);
        assert!(descriptor.capabilities.can_run_job);
    }

    #[test]
    fn fluidnc_network_descriptor_is_a_separate_tcp_row() {
        let descriptor = FluidNcNetworkAdapter::new().descriptor();
        assert_eq!(descriptor.driver, ControllerDriverId::FluidNc);
        assert_eq!(descriptor.transport_kind, TransportKind::Tcp);
        assert_eq!(descriptor.product_tier, ControllerProductTier::Experimental);
        assert_eq!(
            FluidNcNetworkAdapter::new().validate_probe(&exact_probe()),
            Ok(())
        );
    }

    #[test]
    fn exact_controller_info_identity_is_required() {
        let adapter = FluidNcSerialAdapter::new();
        assert_eq!(adapter.validate_probe(&exact_probe()), Ok(()));

        let mut provisional = exact_probe();
        provisional.identity.status = GrblFamilyIdentityStatus::Provisional;
        assert!(matches!(
            adapter.validate_probe(&provisional),
            Err(GrblFamilyAdapterError::IdentityNotConclusive {
                status: GrblFamilyIdentityStatus::Provisional
            })
        ));

        let mut wrong_dialect = exact_probe();
        wrong_dialect.identity.dialect = GrblFamilyDialect::GrblHal;
        assert!(matches!(
            adapter.validate_probe(&wrong_dialect),
            Err(GrblFamilyAdapterError::IdentityMismatch {
                expected: GrblFamilyDialect::FluidNc,
                detected: GrblFamilyDialect::GrblHal
            })
        ));

        let mut timed_out = exact_probe();
        timed_out.controller_info = GrblFamilyIdentityProbeOutcome::TimedOut;
        assert!(matches!(
            adapter.validate_probe(&timed_out),
            Err(GrblFamilyAdapterError::ControllerInfoProbeIncomplete {
                outcome: GrblFamilyIdentityProbeOutcome::TimedOut
            })
        ));

        let mut missing_evidence = exact_probe();
        missing_evidence.identity.evidence.clear();
        assert_eq!(
            adapter.validate_probe(&missing_evidence),
            Err(GrblFamilyAdapterError::MissingControllerInfoEvidence)
        );
    }

    #[test]
    fn shared_parser_tolerates_fluidnc_status_extensions() {
        let response = FluidNcSerialAdapter::new().parse_response(
            "<Run|MPos:12.500,8.250,0.000|Bf:14,120|FS:900,250|WCO:1.000,2.000,0.000|Ov:100,100,100|A:SM|SD:42.5,job.nc>",
        );

        let GrblResponse::Status(status) = response else {
            panic!("expected status response");
        };
        assert_eq!(status.run_state, MachineRunState::Run);
        assert_eq!(status.machine_position.x, 12.5);
        assert_eq!(status.machine_position.y, 8.25);
        assert_eq!(status.work_position.x, 11.5);
        assert_eq!(status.work_position.y, 6.25);
        assert_eq!(status.feed_rate, 900.0);
        assert_eq!(status.spindle_speed, 250.0);
        assert_eq!(status.feed_override, 100);
    }

    #[test]
    fn grbl_hal_descriptor_is_explicit_experimental_and_fail_closed() {
        let descriptor = GrblHalSerialAdapter::new().descriptor();

        assert_eq!(descriptor.driver, ControllerDriverId::GrblHal);
        assert_eq!(descriptor.dialect, GrblFamilyDialect::GrblHal);
        assert_eq!(descriptor.controller_model, ControllerModel::GrblHal);
        assert_eq!(descriptor.product_tier, ControllerProductTier::Experimental);
        assert_eq!(descriptor.evidence_state, ControllerEvidenceState::Emulated);
        assert_eq!(descriptor.transport_kind, TransportKind::Serial);
        assert!(descriptor.capabilities.can_home);
        assert!(descriptor.capabilities.can_jog);
        assert!(descriptor.capabilities.can_pause_resume);
        assert!(descriptor.capabilities.can_frame);
        assert!(descriptor.capabilities.can_run_job);
    }

    #[test]
    fn grblhal_network_descriptor_is_a_separate_tcp_row() {
        let descriptor = GrblHalNetworkAdapter::new().descriptor();
        assert_eq!(descriptor.driver, ControllerDriverId::GrblHal);
        assert_eq!(descriptor.transport_kind, TransportKind::Tcp);
        assert_eq!(descriptor.product_tier, ControllerProductTier::Experimental);
        assert_eq!(
            GrblHalNetworkAdapter::new().validate_probe(&exact_grbl_hal_probe()),
            Ok(())
        );
    }

    #[test]
    fn grbl_hal_requires_exact_firmware_identity_evidence() {
        let adapter = GrblHalSerialAdapter::new();
        assert_eq!(adapter.validate_probe(&exact_grbl_hal_probe()), Ok(()));

        let mut exact_from_basic_info = exact_grbl_hal_probe();
        exact_from_basic_info.extended_controller_info = GrblFamilyIdentityProbeOutcome::NotNeeded;
        assert_eq!(adapter.validate_probe(&exact_from_basic_info), Ok(()));

        let mut provisional = exact_grbl_hal_probe();
        provisional.identity.status = GrblFamilyIdentityStatus::Provisional;
        assert!(matches!(
            adapter.validate_probe(&provisional),
            Err(GrblFamilyAdapterError::IdentityNotConclusive {
                status: GrblFamilyIdentityStatus::Provisional
            })
        ));

        let mut wrong_dialect = exact_grbl_hal_probe();
        wrong_dialect.identity.dialect = GrblFamilyDialect::FluidNc;
        assert!(matches!(
            adapter.validate_probe(&wrong_dialect),
            Err(GrblFamilyAdapterError::IdentityMismatch {
                expected: GrblFamilyDialect::GrblHal,
                detected: GrblFamilyDialect::FluidNc
            })
        ));

        let mut timed_out = exact_grbl_hal_probe();
        timed_out.controller_info = GrblFamilyIdentityProbeOutcome::TimedOut;
        assert!(matches!(
            adapter.validate_probe(&timed_out),
            Err(GrblFamilyAdapterError::ControllerInfoProbeIncomplete {
                outcome: GrblFamilyIdentityProbeOutcome::TimedOut
            })
        ));

        let mut missing_evidence = exact_grbl_hal_probe();
        missing_evidence.identity.evidence.clear();
        assert_eq!(
            adapter.validate_probe(&missing_evidence),
            Err(GrblFamilyAdapterError::MissingFirmwareIdentityEvidence)
        );

        let mut rejected_extension = exact_grbl_hal_probe();
        rejected_extension.extended_controller_info = GrblFamilyIdentityProbeOutcome::Rejected(3);
        assert!(matches!(
            adapter.validate_probe(&rejected_extension),
            Err(
                GrblFamilyAdapterError::ExtendedControllerInfoProbeIncomplete {
                    outcome: GrblFamilyIdentityProbeOutcome::Rejected(3)
                }
            )
        ));
    }

    #[test]
    fn shared_parser_tolerates_grbl_hal_status_extensions() {
        let response = GrblHalSerialAdapter::new().parse_response(
            "<Run:2|MPos:12.500,8.250,0.000|Bf:34,1024|FS:900,250|WCO:1.000,2.000,0.000|Ov:100,100,100|A:SM|FW:grblHAL>",
        );

        let GrblResponse::Status(status) = response else {
            panic!("expected status response");
        };
        assert_eq!(status.run_state, MachineRunState::Run);
        assert_eq!(status.machine_position.x, 12.5);
        assert_eq!(status.machine_position.y, 8.25);
        assert_eq!(status.work_position.x, 11.5);
        assert_eq!(status.work_position.y, 6.25);
        assert_eq!(status.feed_rate, 900.0);
        assert_eq!(status.spindle_speed, 250.0);
        assert_eq!(status.feed_override, 100);
    }
}
