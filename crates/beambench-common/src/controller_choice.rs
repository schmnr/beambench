//! Shared controller-selection and identity-resolution wire types.
//!
//! These types describe policy decisions only. A resolved controller choice does
//! not imply that a transport is open, a protocol handshake succeeded, runtime
//! capabilities are enabled, or a machine session is ready.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::machine::{ControllerFamily, ControllerModel, SessionState, TransportKind};

/// Stable identifier for a controller driver implementation.
///
/// GRBL-family IDs are named once their identity/dialect contract exists, even
/// when the matching runtime adapter is not available yet. Availability remains
/// a separate backend policy check. Unknown values deserialize fail-closed. Raw
/// unknown-ID preservation remains part of the profile-v2 persistence work;
/// these choice types are not yet stored in profiles.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControllerDriverId {
    Grbl,
    FluidNc,
    GrblHal,
    LaserPecker,
    Marlin,
    Snapmaker,
    Smoothieware,
    Ruida,
    Lihuiyu,
    #[default]
    #[serde(other)]
    Unknown,
}

/// Controller-selection mode chosen for a machine connection.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ControllerSelection {
    #[default]
    AutoDetect,
    KnownDriver {
        driver: ControllerDriverId,
    },
    GenericGrblCompatible,
    #[serde(other)]
    Unknown,
}

impl ControllerSelection {
    pub fn explicit(&self) -> Option<ExplicitControllerSelection> {
        match self {
            Self::AutoDetect => None,
            Self::KnownDriver { driver } => {
                Some(ExplicitControllerSelection::KnownDriver { driver: *driver })
            }
            Self::GenericGrblCompatible => Some(ExplicitControllerSelection::GenericGrblCompatible),
            Self::Unknown => Some(ExplicitControllerSelection::Unknown),
        }
    }
}

/// A controller selection that can be applied to a connection.
///
/// This separate type prevents an `AutoDetect` selection from being stored as a
/// remembered experimental override.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ExplicitControllerSelection {
    KnownDriver {
        driver: ControllerDriverId,
    },
    GenericGrblCompatible,
    #[serde(other)]
    Unknown,
}

impl ExplicitControllerSelection {
    /// Driver implementation used after this selection is authorized.
    ///
    /// Generic GRBL-compatible mode uses the GRBL protocol implementation, but
    /// remains a distinct Experimental selection with conservative capabilities.
    pub const fn driver(&self) -> ControllerDriverId {
        match self {
            Self::KnownDriver { driver } => *driver,
            Self::GenericGrblCompatible => ControllerDriverId::Grbl,
            Self::Unknown => ControllerDriverId::Unknown,
        }
    }

    pub const fn is_generic(&self) -> bool {
        matches!(self, Self::GenericGrblCompatible)
    }
}

/// Controller identity established by positive protocol or trusted passive evidence.
///
/// Numeric discovery confidence and free-form port labels are never positive
/// identity. `evidence` is explanatory and is not part of override matching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PositiveControllerIdentity {
    pub family: ControllerFamily,
    pub model: ControllerModel,
    #[serde(default)]
    pub firmware_identity: Option<String>,
    #[serde(default)]
    pub firmware_version: Option<String>,
    #[serde(default)]
    pub evidence: Vec<String>,
}

impl PositiveControllerIdentity {
    pub fn is_positive(&self) -> bool {
        !matches!(self.family, ControllerFamily::Unknown)
            && !matches!(self.model, ControllerModel::Unknown)
            && self.evidence.iter().any(|item| !item.trim().is_empty())
    }
}

/// Whether a device fingerprint is stable enough to remember authorization.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceFingerprintStrength {
    #[default]
    Weak,
    Strong,
}

/// Versioned, opaque digest of stable transport/device identity fields.
///
/// This value is local-only authorization state. It must be omitted from
/// diagnostics, feedback bundles, logs, and exported settings.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceFingerprint {
    pub schema_version: u16,
    pub strength: DeviceFingerprintStrength,
    pub value: String,
}

impl fmt::Debug for DeviceFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeviceFingerprint")
            .field("schema_version", &self.schema_version)
            .field("strength", &self.strength)
            .field("value", &"[redacted]")
            .finish()
    }
}

/// Minimal, privacy-reduced binding for the positively detected controller and
/// its firmware. The digest is computed from normalized identity/version fields;
/// explanatory evidence and raw banners are never persisted in an override.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControllerIdentityBinding {
    pub schema_version: u16,
    pub family: ControllerFamily,
    pub model: ControllerModel,
    pub firmware_fingerprint: String,
}

impl fmt::Debug for ControllerIdentityBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ControllerIdentityBinding")
            .field("schema_version", &self.schema_version)
            .field("family", &self.family)
            .field("model", &self.model)
            .field("firmware_fingerprint", &"[redacted]")
            .finish()
    }
}

impl DeviceFingerprint {
    /// Basic wire-level validity. The backend must additionally require the
    /// fingerprint schema version it generated for the current attempt.
    pub fn can_bind_remembered_override(&self) -> bool {
        matches!(self.strength, DeviceFingerprintStrength::Strong)
            && self.schema_version > 0
            && !self.value.trim().is_empty()
    }
}

/// A remembered user authorization to continue with an explicit Experimental choice.
/// This record is backend-local and must never be accepted from or returned to a
/// frontend as connection authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FingerprintBoundControllerOverride {
    pub selection: ExplicitControllerSelection,
    pub detected_identity: ControllerIdentityBinding,
    pub transport_kind: TransportKind,
    pub fingerprint: DeviceFingerprint,
}

/// User response when the detected controller and selected driver disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControllerMismatchDecision {
    UseDetected,
    ContinueSelectedExperimentally,
    Cancel,
}

/// Lifetime of an Experimental compatibility override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControllerOverrideScope {
    /// Valid only for the current connection attempt and never persisted.
    SessionOnly,
    /// May be persisted because it is bound to a strong device fingerprint.
    FingerprintBound,
}

/// Why the resolver selected the effective driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControllerChoiceSource {
    AutoDetected,
    KnownDriverSelection,
    DetectedDriverChoice,
    UserExperimentalOverride,
    RememberedOverride,
}

/// Material change that prevents reuse of a remembered override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControllerOverrideInvalidationReason {
    SelectionChanged,
    DetectedIdentityChanged,
    FirmwareIdentityChanged,
    TransportChanged,
    DeviceFingerprintChanged,
    FingerprintUnavailable,
    FingerprintTooWeak,
}

/// Persistence instruction emitted alongside every controller-choice outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ControllerOverrideUpdate {
    Keep,
    Clear {
        reason: ControllerOverrideInvalidationReason,
    },
    Replace,
}

/// Stable reason code for a fail-closed controller-choice outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControllerChoiceBlockReason {
    UnsupportedDriver,
    UnsupportedTransport,
    DetectedDriverUnavailable,
    InvalidDecision,
}

/// Policy result after a driver has been selected.
///
/// Callers must still perform transport, compatible-handshake, availability,
/// and runtime-capability checks before creating a live session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedControllerChoice {
    pub selection: ExplicitControllerSelection,
    pub driver: ControllerDriverId,
    pub source: ControllerChoiceSource,
    pub detected_identity: Option<PositiveControllerIdentity>,
    /// Requires the caller to apply Experimental policy as an additional
    /// restriction. It never promotes an unavailable/internal driver.
    pub requires_experimental_mode: bool,
    pub mismatch: bool,
    pub override_scope: Option<ControllerOverrideScope>,
    /// Generic or overridden selections need an extra compatibility check in
    /// addition to the normal driver handshake that every connection requires.
    pub requires_experimental_compatibility_handshake: bool,
}

/// Discriminated policy outcome returned by the resolver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ControllerChoiceOutcome {
    Resolved {
        choice: ResolvedControllerChoice,
    },
    SelectionRequired,
    MismatchDecisionRequired {
        selected: ExplicitControllerSelection,
        detected_identity: PositiveControllerIdentity,
        detected_driver: Option<ControllerDriverId>,
        can_remember_override: bool,
        invalidated_override_reason: Option<ControllerOverrideInvalidationReason>,
        allowed_decisions: Vec<ControllerMismatchDecision>,
    },
    Cancelled,
    Blocked {
        reason: ControllerChoiceBlockReason,
        message: String,
    },
}

/// Result of the pure controller-choice policy resolver.
///
/// `override_update` is uniform so callers never need to infer persistence
/// behavior from a particular outcome variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControllerChoiceResolution {
    #[serde(flatten)]
    pub outcome: ControllerChoiceOutcome,
    pub override_update: ControllerOverrideUpdate,
    /// Backend-local replacement value. Serde intentionally omits it so a stable
    /// device digest cannot leak through a UI/diagnostic response.
    #[serde(skip)]
    pub replacement_override: Option<FingerprintBoundControllerOverride>,
}

/// Transport endpoint retained for a controller-choice connection attempt.
/// Serial-only fields never appear on a TCP endpoint and vice versa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControllerConnectionEndpoint {
    Serial {
        port_name: String,
        baud_rate: u32,
    },
    Tcp {
        host: String,
        port: u16,
    },
    Udp {
        host: String,
        port: u16,
    },
    Usb {
        device_id: String,
        vendor_id: u16,
        product_id: u16,
    },
}

impl ControllerConnectionEndpoint {
    pub const fn transport_kind(&self) -> TransportKind {
        match self {
            Self::Serial { .. } => TransportKind::Serial,
            Self::Tcp { .. } => TransportKind::Tcp,
            Self::Udp { .. } => TransportKind::Udp,
            Self::Usb { .. } => TransportKind::UsbPacket,
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            Self::Serial { port_name, .. } => port_name.clone(),
            Self::Tcp { host, port } | Self::Udp { host, port } => {
                let host = host.trim();
                if host.contains(':') && !(host.starts_with('[') && host.ends_with(']')) {
                    format!("[{host}]:{port}")
                } else {
                    format!("{host}:{port}")
                }
            }
            Self::Usb {
                device_id,
                vendor_id,
                product_id,
            } => format!("{device_id} ({vendor_id:04x}:{product_id:04x})"),
        }
    }

    pub const fn baud_rate(&self) -> Option<u32> {
        match self {
            Self::Serial { baud_rate, .. } => Some(*baud_rate),
            Self::Tcp { .. } | Self::Udp { .. } | Self::Usb { .. } => None,
        }
    }
}

/// Result of one backend-owned desktop controller connection attempt.
///
/// A challenge token refers to an open, probed session retained only by the
/// backend. The frontend may choose a selection or one of the returned
/// decisions, but it cannot supply controller identity or fingerprint claims.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ControllerConnectionResult {
    Connected {
        session_state: SessionState,
        endpoint: ControllerConnectionEndpoint,
        choice: ResolvedControllerChoice,
    },
    Challenge {
        attempt_id: String,
        endpoint: ControllerConnectionEndpoint,
        detected_identity: Option<PositiveControllerIdentity>,
        resolution: Box<ControllerChoiceResolution>,
    },
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_selection_wire_shapes_are_stable() {
        assert_eq!(
            serde_json::to_value(ControllerSelection::AutoDetect).unwrap(),
            serde_json::json!({ "mode": "auto_detect" })
        );
        assert_eq!(
            serde_json::to_value(ControllerSelection::KnownDriver {
                driver: ControllerDriverId::Grbl,
            })
            .unwrap(),
            serde_json::json!({ "mode": "known_driver", "driver": "grbl" })
        );
        assert_eq!(
            serde_json::to_value(ControllerSelection::KnownDriver {
                driver: ControllerDriverId::FluidNc,
            })
            .unwrap(),
            serde_json::json!({ "mode": "known_driver", "driver": "fluid_nc" })
        );
        assert_eq!(
            serde_json::to_value(ControllerSelection::KnownDriver {
                driver: ControllerDriverId::GrblHal,
            })
            .unwrap(),
            serde_json::json!({ "mode": "known_driver", "driver": "grbl_hal" })
        );
        assert_eq!(
            serde_json::to_value(ControllerSelection::KnownDriver {
                driver: ControllerDriverId::LaserPecker,
            })
            .unwrap(),
            serde_json::json!({ "mode": "known_driver", "driver": "laser_pecker" })
        );
        assert_eq!(
            serde_json::to_value(ControllerSelection::KnownDriver {
                driver: ControllerDriverId::Marlin,
            })
            .unwrap(),
            serde_json::json!({ "mode": "known_driver", "driver": "marlin" })
        );
        assert_eq!(
            serde_json::to_value(ControllerSelection::KnownDriver {
                driver: ControllerDriverId::Snapmaker,
            })
            .unwrap(),
            serde_json::json!({ "mode": "known_driver", "driver": "snapmaker" })
        );
        assert_eq!(
            serde_json::to_value(ControllerSelection::KnownDriver {
                driver: ControllerDriverId::Smoothieware,
            })
            .unwrap(),
            serde_json::json!({ "mode": "known_driver", "driver": "smoothieware" })
        );
        assert_eq!(
            serde_json::to_value(ControllerSelection::KnownDriver {
                driver: ControllerDriverId::Ruida,
            })
            .unwrap(),
            serde_json::json!({ "mode": "known_driver", "driver": "ruida" })
        );
        assert_eq!(
            serde_json::to_value(ControllerSelection::KnownDriver {
                driver: ControllerDriverId::Lihuiyu,
            })
            .unwrap(),
            serde_json::json!({ "mode": "known_driver", "driver": "lihuiyu" })
        );
        assert_eq!(
            serde_json::to_value(ControllerSelection::GenericGrblCompatible).unwrap(),
            serde_json::json!({ "mode": "generic_grbl_compatible" })
        );
    }

    #[test]
    fn future_driver_ids_deserialize_fail_closed() {
        let driver: ControllerDriverId = serde_json::from_str("\"future_driver\"").unwrap();
        assert_eq!(driver, ControllerDriverId::Unknown);

        let selection: ControllerSelection =
            serde_json::from_str(r#"{"mode":"future_mode","driver":"grbl"}"#).unwrap();
        assert_eq!(selection, ControllerSelection::Unknown);
    }

    #[test]
    fn positive_identity_requires_backend_evidence() {
        let mut identity = PositiveControllerIdentity {
            family: ControllerFamily::Gcode,
            model: ControllerModel::Grbl,
            firmware_identity: Some("Grbl".to_string()),
            firmware_version: Some("1.1h".to_string()),
            evidence: Vec::new(),
        };
        assert!(!identity.is_positive());
        identity.evidence.push("  ".to_string());
        assert!(!identity.is_positive());
        identity.evidence.push("Parsed startup banner".to_string());
        assert!(identity.is_positive());
    }

    #[test]
    fn resolved_wire_shape_does_not_imply_session_readiness() {
        let resolution = ControllerChoiceResolution {
            outcome: ControllerChoiceOutcome::Resolved {
                choice: ResolvedControllerChoice {
                    selection: ExplicitControllerSelection::KnownDriver {
                        driver: ControllerDriverId::Grbl,
                    },
                    driver: ControllerDriverId::Grbl,
                    source: ControllerChoiceSource::KnownDriverSelection,
                    detected_identity: None,
                    requires_experimental_mode: false,
                    mismatch: false,
                    override_scope: None,
                    requires_experimental_compatibility_handshake: false,
                },
            },
            override_update: ControllerOverrideUpdate::Keep,
            replacement_override: None,
        };
        let value = serde_json::to_value(resolution).unwrap();
        assert_eq!(value["outcome"], "resolved");
        assert_eq!(value["choice"]["driver"], "grbl");
        assert_eq!(
            value["choice"]["requires_experimental_compatibility_handshake"],
            false
        );
        assert!(value.get("session_state").is_none());
        assert!(value.get("capabilities").is_none());
        assert_eq!(value["override_update"]["action"], "keep");
    }

    #[test]
    fn controller_connection_challenge_wire_shape_keeps_attempt_authority_backend_owned() {
        let result = ControllerConnectionResult::Challenge {
            attempt_id: "attempt-1".to_string(),
            endpoint: ControllerConnectionEndpoint::Serial {
                port_name: "/dev/ttyUSB0".to_string(),
                baud_rate: 115_200,
            },
            detected_identity: None,
            resolution: Box::new(ControllerChoiceResolution {
                outcome: ControllerChoiceOutcome::SelectionRequired,
                override_update: ControllerOverrideUpdate::Keep,
                replacement_override: None,
            }),
        };
        let value = serde_json::to_value(result).unwrap();
        assert_eq!(value["status"], "challenge");
        assert_eq!(value["attempt_id"], "attempt-1");
        assert_eq!(value["endpoint"]["type"], "serial");
        assert_eq!(value["endpoint"]["port_name"], "/dev/ttyUSB0");
        assert_eq!(value["endpoint"]["baud_rate"], 115_200);
        assert_eq!(value["resolution"]["outcome"], "selection_required");
        assert!(value["resolution"].get("replacement_override").is_none());
        assert!(value.get("session_state").is_none());
    }

    #[test]
    fn tcp_connection_endpoint_has_no_fake_serial_fields() {
        let endpoint = ControllerConnectionEndpoint::Tcp {
            host: "2001:db8::10".to_string(),
            port: 23,
        };
        assert_eq!(endpoint.transport_kind(), TransportKind::Tcp);
        assert_eq!(endpoint.baud_rate(), None);
        assert_eq!(endpoint.display_name(), "[2001:db8::10]:23");

        let value = serde_json::to_value(endpoint).unwrap();
        assert_eq!(value["type"], "tcp");
        assert_eq!(value["host"], "2001:db8::10");
        assert_eq!(value["port"], 23);
        assert!(value.get("port_name").is_none());
        assert!(value.get("baud_rate").is_none());
    }

    #[test]
    fn udp_connection_endpoint_is_distinct_from_tcp() {
        let endpoint = ControllerConnectionEndpoint::Udp {
            host: "192.168.1.50".to_string(),
            port: 50_200,
        };
        assert_eq!(endpoint.transport_kind(), TransportKind::Udp);
        assert_eq!(endpoint.display_name(), "192.168.1.50:50200");
        assert_eq!(endpoint.baud_rate(), None);

        let value = serde_json::to_value(endpoint).unwrap();
        assert_eq!(value["type"], "udp");
        assert_eq!(value["host"], "192.168.1.50");
        assert_eq!(value["port"], 50_200);
    }

    #[test]
    fn usb_connection_endpoint_preserves_the_physical_device_identity() {
        let endpoint = ControllerConnectionEndpoint::Usb {
            device_id: "usb-bus-20-ports-1.3".to_string(),
            vendor_id: 0x1a86,
            product_id: 0x5512,
        };
        assert_eq!(endpoint.transport_kind(), TransportKind::UsbPacket);
        assert_eq!(endpoint.baud_rate(), None);
        assert_eq!(endpoint.display_name(), "usb-bus-20-ports-1.3 (1a86:5512)");

        let value = serde_json::to_value(endpoint).unwrap();
        assert_eq!(value["type"], "usb");
        assert_eq!(value["device_id"], "usb-bus-20-ports-1.3");
        assert_eq!(value["vendor_id"], 0x1a86);
        assert_eq!(value["product_id"], 0x5512);
        assert!(value.get("port_name").is_none());
        assert!(value.get("host").is_none());
    }
}
