//! Shared identity vocabulary for GRBL-protocol controller families.
//!
//! This module describes identity evidence only. It does not select a driver,
//! enable capabilities, open a transport, or imply that a controller is ready.

use serde::{Deserialize, Serialize};

use crate::controller_choice::PositiveControllerIdentity;
use crate::machine::{ControllerFamily, ControllerModel};

/// GRBL-protocol dialect positively named by controller responses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrblFamilyDialect {
    #[default]
    Unknown,
    Grbl,
    FluidNc,
    GrblHal,
}

/// Maturity of the identity accumulated from controller responses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrblFamilyIdentityStatus {
    #[default]
    Unknown,
    ProtocolCompatible,
    Provisional,
    Identified,
    Conflicting,
}

/// Redacted evidence categories used to establish GRBL-family identity.
///
/// Variants intentionally carry no controller-provided text. Raw banners and
/// messages belong in bounded protocol diagnostics, not this shared identity
/// contract or its user-visible evidence descriptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrblFamilyIdentityEvidence {
    StartupBanner,
    ProtocolSignature,
    ControllerInfoVersion,
    FirmwareIdentityMessage,
}

impl GrblFamilyIdentityEvidence {
    /// Stable, non-raw explanation suitable for controller-choice UI.
    pub const fn evidence_description(self) -> &'static str {
        match self {
            Self::StartupBanner => "Parsed a named GRBL-family startup banner",
            Self::ProtocolSignature => "Parsed a GRBL-compatible protocol signature",
            Self::ControllerInfoVersion => "Parsed a controller information version response",
            Self::FirmwareIdentityMessage => "Parsed a firmware identity message",
        }
    }
}

/// Identity accumulated from startup and controller-information responses.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrblFamilyIdentity {
    pub dialect: GrblFamilyDialect,
    pub status: GrblFamilyIdentityStatus,
    #[serde(default)]
    pub firmware_identity: Option<String>,
    #[serde(default)]
    pub firmware_version: Option<String>,
    #[serde(default)]
    pub evidence: Vec<GrblFamilyIdentityEvidence>,
}

impl GrblFamilyIdentity {
    /// Map the candidate protocol dialect to the shared controller-model vocabulary.
    ///
    /// Identity status and exact evidence still govern whether this candidate is
    /// strong enough to become a positive controller identity.
    pub const fn controller_model(&self) -> ControllerModel {
        match self.dialect {
            GrblFamilyDialect::Unknown => ControllerModel::Unknown,
            GrblFamilyDialect::Grbl => ControllerModel::Grbl,
            GrblFamilyDialect::FluidNc => ControllerModel::FluidNc,
            GrblFamilyDialect::GrblHal => ControllerModel::GrblHal,
        }
    }

    /// Convert a conclusively identified dialect to controller-choice evidence.
    ///
    /// Provisional, merely protocol-compatible, conflicting, and evidence-free
    /// classifications remain non-positive and therefore cannot drive Auto-detect.
    pub fn positive_identity(&self) -> Option<PositiveControllerIdentity> {
        if self.status != GrblFamilyIdentityStatus::Identified {
            return None;
        }

        let (model, required_evidence) = match self.dialect {
            GrblFamilyDialect::FluidNc => (
                ControllerModel::FluidNc,
                GrblFamilyIdentityEvidence::ControllerInfoVersion,
            ),
            GrblFamilyDialect::GrblHal => (
                ControllerModel::GrblHal,
                GrblFamilyIdentityEvidence::FirmwareIdentityMessage,
            ),
            GrblFamilyDialect::Unknown | GrblFamilyDialect::Grbl => return None,
        };
        if !self.evidence.contains(&required_evidence) {
            return None;
        }

        Some(PositiveControllerIdentity {
            family: ControllerFamily::Gcode,
            model,
            firmware_identity: self.firmware_identity.clone(),
            firmware_version: self.firmware_version.clone(),
            evidence: self
                .evidence
                .iter()
                .map(|evidence| evidence.evidence_description().to_string())
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_wire_shape_uses_snake_case_and_redacted_evidence() {
        let identity = GrblFamilyIdentity {
            dialect: GrblFamilyDialect::FluidNc,
            status: GrblFamilyIdentityStatus::Identified,
            firmware_identity: Some("FluidNC".to_string()),
            firmware_version: Some("3.9.1".to_string()),
            evidence: vec![GrblFamilyIdentityEvidence::ControllerInfoVersion],
        };

        assert_eq!(
            serde_json::to_value(identity).unwrap(),
            serde_json::json!({
                "dialect": "fluid_nc",
                "status": "identified",
                "firmware_identity": "FluidNC",
                "firmware_version": "3.9.1",
                "evidence": ["controller_info_version"]
            })
        );
    }

    #[test]
    fn all_identity_enum_variants_have_stable_wire_names() {
        assert_eq!(
            serde_json::to_value([
                GrblFamilyDialect::Unknown,
                GrblFamilyDialect::Grbl,
                GrblFamilyDialect::FluidNc,
                GrblFamilyDialect::GrblHal,
            ])
            .unwrap(),
            serde_json::json!(["unknown", "grbl", "fluid_nc", "grbl_hal"])
        );
        assert_eq!(
            serde_json::to_value([
                GrblFamilyIdentityStatus::Unknown,
                GrblFamilyIdentityStatus::ProtocolCompatible,
                GrblFamilyIdentityStatus::Provisional,
                GrblFamilyIdentityStatus::Identified,
                GrblFamilyIdentityStatus::Conflicting,
            ])
            .unwrap(),
            serde_json::json!([
                "unknown",
                "protocol_compatible",
                "provisional",
                "identified",
                "conflicting"
            ])
        );
        assert_eq!(
            serde_json::to_value([
                GrblFamilyIdentityEvidence::StartupBanner,
                GrblFamilyIdentityEvidence::ProtocolSignature,
                GrblFamilyIdentityEvidence::ControllerInfoVersion,
                GrblFamilyIdentityEvidence::FirmwareIdentityMessage,
            ])
            .unwrap(),
            serde_json::json!([
                "startup_banner",
                "protocol_signature",
                "controller_info_version",
                "firmware_identity_message"
            ])
        );
    }

    #[test]
    fn dialects_map_to_controller_models() {
        for (dialect, model) in [
            (GrblFamilyDialect::Unknown, ControllerModel::Unknown),
            (GrblFamilyDialect::Grbl, ControllerModel::Grbl),
            (GrblFamilyDialect::FluidNc, ControllerModel::FluidNc),
            (GrblFamilyDialect::GrblHal, ControllerModel::GrblHal),
        ] {
            assert_eq!(
                GrblFamilyIdentity {
                    dialect,
                    ..GrblFamilyIdentity::default()
                }
                .controller_model(),
                model
            );
        }
    }

    #[test]
    fn only_identified_dialects_become_positive_identity() {
        for status in [
            GrblFamilyIdentityStatus::Unknown,
            GrblFamilyIdentityStatus::ProtocolCompatible,
            GrblFamilyIdentityStatus::Provisional,
            GrblFamilyIdentityStatus::Conflicting,
        ] {
            let identity = GrblFamilyIdentity {
                dialect: GrblFamilyDialect::FluidNc,
                status,
                evidence: vec![GrblFamilyIdentityEvidence::ControllerInfoVersion],
                ..GrblFamilyIdentity::default()
            };
            assert_eq!(identity.positive_identity(), None);
        }

        let identity = GrblFamilyIdentity {
            dialect: GrblFamilyDialect::GrblHal,
            status: GrblFamilyIdentityStatus::Identified,
            firmware_identity: Some("grblHAL".to_string()),
            firmware_version: Some("1.1f".to_string()),
            evidence: vec![GrblFamilyIdentityEvidence::FirmwareIdentityMessage],
        };
        let positive = identity.positive_identity().unwrap();
        assert_eq!(positive.family, ControllerFamily::Gcode);
        assert_eq!(positive.model, ControllerModel::GrblHal);
        assert_eq!(positive.firmware_identity.as_deref(), Some("grblHAL"));
        assert_eq!(positive.firmware_version.as_deref(), Some("1.1f"));
        assert_eq!(
            positive.evidence,
            vec!["Parsed a firmware identity message"]
        );
        assert!(positive.is_positive());
    }

    #[test]
    fn identified_unknown_or_evidence_free_identity_is_not_positive() {
        let identified = |dialect, evidence| GrblFamilyIdentity {
            dialect,
            status: GrblFamilyIdentityStatus::Identified,
            evidence,
            ..GrblFamilyIdentity::default()
        };

        assert!(
            identified(
                GrblFamilyDialect::Unknown,
                vec![GrblFamilyIdentityEvidence::ProtocolSignature]
            )
            .positive_identity()
            .is_none()
        );
        assert!(
            identified(GrblFamilyDialect::Grbl, Vec::new())
                .positive_identity()
                .is_none()
        );
    }

    #[test]
    fn positive_identity_requires_the_exact_named_firmware_evidence() {
        let identified = |dialect, evidence| GrblFamilyIdentity {
            dialect,
            status: GrblFamilyIdentityStatus::Identified,
            evidence: vec![evidence],
            ..GrblFamilyIdentity::default()
        };

        assert!(
            identified(
                GrblFamilyDialect::FluidNc,
                GrblFamilyIdentityEvidence::FirmwareIdentityMessage
            )
            .positive_identity()
            .is_none()
        );
        assert!(
            identified(
                GrblFamilyDialect::GrblHal,
                GrblFamilyIdentityEvidence::ControllerInfoVersion
            )
            .positive_identity()
            .is_none()
        );
        assert!(
            identified(
                GrblFamilyDialect::Grbl,
                GrblFamilyIdentityEvidence::StartupBanner
            )
            .positive_identity()
            .is_none()
        );
    }
}
