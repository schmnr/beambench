//! Controller-choice policy, device-fingerprint construction, and the explicit
//! GRBL-family probe-to-policy bridge.
//!
//! This module deliberately does not open transports or create machine sessions.
//! A `Resolved` result selects the next driver policy; it is not proof that a
//! controller is compatible, safe, connected, or ready.

use beambench_common::{
    ControllerChoiceBlockReason, ControllerChoiceOutcome, ControllerChoiceResolution,
    ControllerChoiceSource, ControllerDriverId, ControllerFamily, ControllerIdentityBinding,
    ControllerMismatchDecision, ControllerModel, ControllerOverrideInvalidationReason,
    ControllerOverrideScope, ControllerOverrideUpdate, ControllerSelection, DeviceFingerprint,
    DeviceFingerprintStrength, DeviceIdentity, DiscoveryCandidate, ExplicitControllerSelection,
    FingerprintBoundControllerOverride, GrblFamilyDialect, GrblFamilyIdentityStatus,
    PositiveControllerIdentity, ResolvedControllerChoice, TransportKind,
};
use beambench_grbl::{
    FluidNcNetworkAdapter, FluidNcSerialAdapter, GrblError, GrblFamilyAdapter,
    GrblFamilyIdentityProbeConfig, GrblFamilyIdentityProbeResult, GrblHalNetworkAdapter,
    GrblHalSerialAdapter, GrblSession,
};
use beambench_marlin::{MarlinIdentityProbeResult, MarlinSerialAdapter, SnapmakerSerialAdapter};
use beambench_ruida::RuidaEthernetAdapter;
use beambench_smoothieware::{SmoothiewareIdentityProbeResult, SmoothiewareSerialAdapter};
use serde::Serialize;
use sha2::{Digest, Sha256};

const FINGERPRINT_SCHEMA_VERSION: u16 = 1;
const IDENTITY_BINDING_SCHEMA_VERSION: u16 = 1;

/// Inputs to the backend-owned controller-choice resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveControllerChoiceInput {
    pub selection: ControllerSelection,
    pub detected_identity: Option<PositiveControllerIdentity>,
    /// The probe proved the shared GRBL wire contract but not an exact named
    /// firmware identity. This can validate an explicit GRBL choice, but it is
    /// intentionally insufficient for Auto-detect.
    pub protocol_compatible: bool,
    pub transport_kind: TransportKind,
    pub fingerprint: Option<DeviceFingerprint>,
    pub remembered_override: Option<FingerprintBoundControllerOverride>,
    pub decision: Option<ControllerMismatchDecision>,
}

/// Backend-owned inputs surrounding a GRBL-family probe result.
///
/// The frontend may choose `selection` and answer a decision prompt, but it
/// must never provide detected identity, device fingerprints, or remembered
/// authorization as connection authority. Those values are produced or loaded
/// by the backend before this input is constructed.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolveGrblFamilyProbeChoiceInput {
    pub selection: ControllerSelection,
    pub device_identity: DeviceIdentity,
    pub transport_kind: TransportKind,
    pub remembered_override: Option<FingerprintBoundControllerOverride>,
    pub decision: Option<ControllerMismatchDecision>,
}

/// Probe evidence paired with its controller-choice policy result.
///
/// A `Resolved` policy outcome still does not create or ready a live machine
/// session. The caller must enforce driver availability, compatibility, runtime
/// capabilities, and any backend-owned prompt challenge before doing so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbedControllerChoiceResolution {
    pub probe: GrblFamilyIdentityProbeResult,
    pub resolution: ControllerChoiceResolution,
}

/// Backend-owned inputs surrounding a standard-Marlin identity probe.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolveMarlinProbeChoiceInput {
    pub selection: ControllerSelection,
    pub device_identity: DeviceIdentity,
    pub remembered_override: Option<FingerprintBoundControllerOverride>,
    pub decision: Option<ControllerMismatchDecision>,
}

/// Standard-Marlin identity evidence paired with controller-choice policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbedMarlinChoiceResolution {
    pub probe: MarlinIdentityProbeResult,
    pub resolution: ControllerChoiceResolution,
}

/// Backend-owned inputs surrounding a Smoothieware identity/config probe.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolveSmoothiewareProbeChoiceInput {
    pub selection: ControllerSelection,
    pub device_identity: DeviceIdentity,
    pub remembered_override: Option<FingerprintBoundControllerOverride>,
    pub decision: Option<ControllerMismatchDecision>,
}

/// Smoothieware evidence paired with controller-choice policy.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbedSmoothiewareChoiceResolution {
    pub probe: SmoothiewareIdentityProbeResult,
    pub resolution: ControllerChoiceResolution,
}

#[derive(Serialize)]
struct FingerprintMaterial<'a> {
    schema_version: u16,
    transport_kind: TransportKind,
    locator_kind: &'a str,
    locator: &'a str,
    vendor_id: Option<u16>,
    product_id: Option<u16>,
}

#[derive(Serialize)]
struct FirmwareIdentityMaterial<'a> {
    schema_version: u16,
    family: ControllerFamily,
    model: ControllerModel,
    firmware_identity: Option<&'a str>,
    firmware_version: Option<&'a str>,
}

fn normalized(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// Build a versioned opaque device fingerprint without using free-form labels,
/// confidence, status text, or the discovery candidate UUID.
///
/// USB serial number plus VID/PID is considered strong. Endpoint/path-only
/// identities are weak: they can scope the current prompt but cannot authorize a
/// remembered override.
pub fn fingerprint_device(
    transport_kind: TransportKind,
    identity: &DeviceIdentity,
) -> Option<DeviceFingerprint> {
    let serial_number = normalized(identity.serial_number.as_deref());
    let port_name = normalized(identity.port_name.as_deref());
    let host = normalized(identity.host.as_deref()).map(|host| host.to_ascii_lowercase());
    let usb_path = normalized(identity.usb_path.as_deref());

    let (locator_kind, locator, strength) = match transport_kind {
        TransportKind::Serial => {
            if let (Some(serial), Some(_), Some(_)) = (
                serial_number.as_deref(),
                identity.vendor_id,
                identity.product_id,
            ) {
                (
                    "usb_serial",
                    serial.to_owned(),
                    DeviceFingerprintStrength::Strong,
                )
            } else if let Some(port) = port_name {
                ("serial_port", port, DeviceFingerprintStrength::Weak)
            } else if let Some(serial) = serial_number {
                ("serial_number", serial, DeviceFingerprintStrength::Weak)
            } else {
                return None;
            }
        }
        TransportKind::Tcp => {
            let host = host?;
            let port = identity.tcp_port?;
            (
                "tcp_endpoint",
                format!("{host}:{port}"),
                DeviceFingerprintStrength::Weak,
            )
        }
        TransportKind::Udp => {
            let host = host?;
            let port = identity.udp_port?;
            (
                "udp_endpoint",
                format!("{host}:{port}"),
                DeviceFingerprintStrength::Weak,
            )
        }
        TransportKind::UsbPacket => {
            if let (Some(serial), Some(_), Some(_)) = (
                serial_number.as_deref(),
                identity.vendor_id,
                identity.product_id,
            ) {
                (
                    "usb_serial",
                    serial.to_owned(),
                    DeviceFingerprintStrength::Strong,
                )
            } else if let Some(path) = usb_path {
                ("usb_path", path, DeviceFingerprintStrength::Weak)
            } else if let Some(serial) = serial_number {
                ("serial_number", serial, DeviceFingerprintStrength::Weak)
            } else {
                return None;
            }
        }
    };

    let material = FingerprintMaterial {
        schema_version: FINGERPRINT_SCHEMA_VERSION,
        transport_kind,
        locator_kind,
        locator: &locator,
        vendor_id: identity.vendor_id,
        product_id: identity.product_id,
    };
    let bytes = serde_json::to_vec(&material).ok()?;
    let digest = Sha256::digest(bytes);
    Some(DeviceFingerprint {
        schema_version: FINGERPRINT_SCHEMA_VERSION,
        strength,
        value: format!("{digest:x}"),
    })
}

/// Fingerprint a discovery candidate using only its structured device identity.
pub fn fingerprint_discovery_candidate(
    candidate: &DiscoveryCandidate,
) -> Option<DeviceFingerprint> {
    fingerprint_device(candidate.transport_kind, &candidate.identity)
}

fn driver_for_identity(identity: &PositiveControllerIdentity) -> Option<ControllerDriverId> {
    match (identity.family, identity.model) {
        (ControllerFamily::Gcode, ControllerModel::Grbl) => Some(ControllerDriverId::Grbl),
        (ControllerFamily::Gcode, ControllerModel::FluidNc) => Some(ControllerDriverId::FluidNc),
        (ControllerFamily::Gcode, ControllerModel::GrblHal) => Some(ControllerDriverId::GrblHal),
        (ControllerFamily::Gcode, ControllerModel::LaserPecker) => {
            Some(ControllerDriverId::LaserPecker)
        }
        (ControllerFamily::Gcode, ControllerModel::Marlin) => Some(ControllerDriverId::Marlin),
        (ControllerFamily::Gcode, ControllerModel::Snapmaker) => {
            Some(ControllerDriverId::Snapmaker)
        }
        (ControllerFamily::Gcode, ControllerModel::Smoothieware) => {
            Some(ControllerDriverId::Smoothieware)
        }
        (ControllerFamily::Dsp, ControllerModel::Ruida) => Some(ControllerDriverId::Ruida),
        (ControllerFamily::Dsp, ControllerModel::LihuiyuM2Nano) => {
            Some(ControllerDriverId::Lihuiyu)
        }
        _ => None,
    }
}

fn driver_is_available(driver: ControllerDriverId) -> bool {
    match driver {
        ControllerDriverId::Grbl => true,
        ControllerDriverId::LaserPecker => true,
        ControllerDriverId::FluidNc => {
            FluidNcSerialAdapter::new().descriptor().transport_kind == TransportKind::Serial
        }
        ControllerDriverId::GrblHal => {
            GrblHalSerialAdapter::new().descriptor().transport_kind == TransportKind::Serial
        }
        ControllerDriverId::Marlin => {
            MarlinSerialAdapter::new().descriptor().transport_kind == TransportKind::Serial
        }
        ControllerDriverId::Snapmaker => {
            SnapmakerSerialAdapter::new().descriptor().transport_kind == TransportKind::Serial
        }
        ControllerDriverId::Smoothieware => {
            SmoothiewareSerialAdapter::new().descriptor().transport_kind == TransportKind::Serial
        }
        ControllerDriverId::Ruida => {
            RuidaEthernetAdapter::new().descriptor().transport_kind == TransportKind::Udp
        }
        ControllerDriverId::Lihuiyu => true,
        ControllerDriverId::Unknown => false,
    }
}

fn driver_runs_experimentally(driver: ControllerDriverId) -> bool {
    matches!(
        driver,
        ControllerDriverId::LaserPecker
            | ControllerDriverId::FluidNc
            | ControllerDriverId::GrblHal
            | ControllerDriverId::Marlin
            | ControllerDriverId::Snapmaker
            | ControllerDriverId::Smoothieware
            | ControllerDriverId::Ruida
            | ControllerDriverId::Lihuiyu
    )
}

fn driver_supports_transport(driver: ControllerDriverId, transport: TransportKind) -> bool {
    match (driver, transport) {
        (
            ControllerDriverId::Grbl
            | ControllerDriverId::LaserPecker
            | ControllerDriverId::Marlin
            | ControllerDriverId::Snapmaker
            | ControllerDriverId::Smoothieware,
            TransportKind::Serial,
        ) => true,
        (ControllerDriverId::FluidNc, TransportKind::Serial) => {
            FluidNcSerialAdapter::new().descriptor().transport_kind == TransportKind::Serial
        }
        (ControllerDriverId::FluidNc, TransportKind::Tcp) => {
            FluidNcNetworkAdapter::new().descriptor().transport_kind == TransportKind::Tcp
        }
        (ControllerDriverId::GrblHal, TransportKind::Serial) => {
            GrblHalSerialAdapter::new().descriptor().transport_kind == TransportKind::Serial
        }
        (ControllerDriverId::GrblHal, TransportKind::Tcp) => {
            GrblHalNetworkAdapter::new().descriptor().transport_kind == TransportKind::Tcp
        }
        (ControllerDriverId::LaserPecker, TransportKind::Tcp) => true,
        (ControllerDriverId::Ruida, TransportKind::Udp) => {
            RuidaEthernetAdapter::new().descriptor().transport_kind == TransportKind::Udp
        }
        (ControllerDriverId::Lihuiyu, TransportKind::UsbPacket) => true,
        _ => false,
    }
}

fn fingerprint_can_bind_override(fingerprint: &DeviceFingerprint) -> bool {
    fingerprint.schema_version == FINGERPRINT_SCHEMA_VERSION
        && fingerprint.can_bind_remembered_override()
        && !fingerprint.value.trim().is_empty()
}

fn normalize_firmware_value(value: Option<&str>) -> Option<String> {
    value.and_then(|value| {
        let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
        (!normalized.is_empty()).then_some(normalized)
    })
}

fn identity_binding(identity: &PositiveControllerIdentity) -> Option<ControllerIdentityBinding> {
    if !identity.is_positive() {
        return None;
    }
    let firmware_identity = normalize_firmware_value(identity.firmware_identity.as_deref());
    let firmware_version = normalize_firmware_value(identity.firmware_version.as_deref());
    // A generic family string (for example just "Grbl") cannot detect a later
    // firmware upgrade. Remembered authorization therefore requires an exact
    // version/build value; identity-only detections remain session-only.
    firmware_version.as_ref()?;
    let material = FirmwareIdentityMaterial {
        schema_version: IDENTITY_BINDING_SCHEMA_VERSION,
        family: identity.family,
        model: identity.model,
        firmware_identity: firmware_identity.as_deref(),
        firmware_version: firmware_version.as_deref(),
    };
    let bytes = serde_json::to_vec(&material).ok()?;
    Some(ControllerIdentityBinding {
        schema_version: IDENTITY_BINDING_SCHEMA_VERSION,
        family: identity.family,
        model: identity.model,
        firmware_fingerprint: format!("{:x}", Sha256::digest(bytes)),
    })
}

fn resolution(
    outcome: ControllerChoiceOutcome,
    override_update: ControllerOverrideUpdate,
) -> ControllerChoiceResolution {
    ControllerChoiceResolution {
        outcome,
        override_update,
        replacement_override: None,
    }
}

fn resolution_with_replacement(
    outcome: ControllerChoiceOutcome,
    replacement_override: FingerprintBoundControllerOverride,
) -> ControllerChoiceResolution {
    ControllerChoiceResolution {
        outcome,
        override_update: ControllerOverrideUpdate::Replace,
        replacement_override: Some(replacement_override),
    }
}

fn keep_override() -> ControllerOverrideUpdate {
    ControllerOverrideUpdate::Keep
}

fn update_for_invalidation(
    reason: Option<ControllerOverrideInvalidationReason>,
) -> ControllerOverrideUpdate {
    match reason {
        Some(reason) => ControllerOverrideUpdate::Clear { reason },
        None => keep_override(),
    }
}

fn blocked(
    reason: ControllerChoiceBlockReason,
    message: impl Into<String>,
    override_update: ControllerOverrideUpdate,
) -> ControllerChoiceResolution {
    resolution(
        ControllerChoiceOutcome::Blocked {
            reason,
            message: message.into(),
        },
        override_update,
    )
}

fn invalid_decision(
    message: impl Into<String>,
    override_update: ControllerOverrideUpdate,
) -> ControllerChoiceResolution {
    blocked(
        ControllerChoiceBlockReason::InvalidDecision,
        message,
        override_update,
    )
}

fn identity_material_matches(
    remembered: &ControllerIdentityBinding,
    current: Option<&PositiveControllerIdentity>,
) -> Result<(), ControllerOverrideInvalidationReason> {
    let Some(current) = current else {
        return Err(ControllerOverrideInvalidationReason::DetectedIdentityChanged);
    };
    if remembered.family != current.family || remembered.model != current.model {
        return Err(ControllerOverrideInvalidationReason::DetectedIdentityChanged);
    }
    let Some(current_binding) = identity_binding(current) else {
        return Err(ControllerOverrideInvalidationReason::FirmwareIdentityChanged);
    };
    if *remembered != current_binding {
        return Err(ControllerOverrideInvalidationReason::FirmwareIdentityChanged);
    }
    Ok(())
}

fn override_invalidation_reason(
    remembered: &FingerprintBoundControllerOverride,
    selection: &ExplicitControllerSelection,
    detected_identity: Option<&PositiveControllerIdentity>,
    transport_kind: TransportKind,
    fingerprint: Option<&DeviceFingerprint>,
) -> Option<ControllerOverrideInvalidationReason> {
    if remembered.selection != *selection {
        return Some(ControllerOverrideInvalidationReason::SelectionChanged);
    }
    if let Err(reason) = identity_material_matches(&remembered.detected_identity, detected_identity)
    {
        return Some(reason);
    }
    if remembered.transport_kind != transport_kind {
        return Some(ControllerOverrideInvalidationReason::TransportChanged);
    }
    let Some(fingerprint) = fingerprint else {
        return Some(ControllerOverrideInvalidationReason::FingerprintUnavailable);
    };
    if !fingerprint_can_bind_override(fingerprint)
        || !fingerprint_can_bind_override(&remembered.fingerprint)
    {
        return Some(ControllerOverrideInvalidationReason::FingerprintTooWeak);
    }
    if remembered.fingerprint != *fingerprint {
        return Some(ControllerOverrideInvalidationReason::DeviceFingerprintChanged);
    }
    None
}

fn selection_matches_driver(
    selection: &ExplicitControllerSelection,
    driver: Option<ControllerDriverId>,
) -> bool {
    matches!(
        (selection, driver),
        (
            ExplicitControllerSelection::KnownDriver { driver: selected_driver },
            Some(detected_driver)
        ) if *selected_driver == detected_driver
    )
}

fn resolved_normal(
    selection: ExplicitControllerSelection,
    source: ControllerChoiceSource,
    detected_identity: Option<PositiveControllerIdentity>,
    mismatch: bool,
    override_update: ControllerOverrideUpdate,
) -> ControllerChoiceResolution {
    let driver = selection.driver();
    resolution(
        ControllerChoiceOutcome::Resolved {
            choice: ResolvedControllerChoice {
                driver,
                selection,
                source,
                detected_identity,
                requires_experimental_mode: driver_runs_experimentally(driver),
                mismatch,
                override_scope: None,
                requires_experimental_compatibility_handshake: false,
            },
        },
        override_update,
    )
}

fn resolved_experimental(
    selection: ExplicitControllerSelection,
    detected_identity: Option<PositiveControllerIdentity>,
    transport_kind: TransportKind,
    fingerprint: Option<&DeviceFingerprint>,
    mismatch: bool,
    stale_override_update: ControllerOverrideUpdate,
) -> ControllerChoiceResolution {
    let remembered = detected_identity
        .as_ref()
        .and_then(identity_binding)
        .and_then(|detected_identity| {
            let fingerprint = fingerprint.filter(|value| fingerprint_can_bind_override(value))?;
            Some(FingerprintBoundControllerOverride {
                selection: selection.clone(),
                detected_identity,
                transport_kind,
                fingerprint: fingerprint.clone(),
            })
        });
    let outcome = |override_scope| ControllerChoiceOutcome::Resolved {
        choice: ResolvedControllerChoice {
            driver: selection.driver(),
            selection,
            source: ControllerChoiceSource::UserExperimentalOverride,
            detected_identity,
            requires_experimental_mode: true,
            mismatch,
            override_scope: Some(override_scope),
            requires_experimental_compatibility_handshake: true,
        },
    };
    match remembered {
        Some(value) => {
            resolution_with_replacement(outcome(ControllerOverrideScope::FingerprintBound), value)
        }
        None => resolution(
            outcome(ControllerOverrideScope::SessionOnly),
            stale_override_update,
        ),
    }
}

fn resolved_remembered(
    remembered: &FingerprintBoundControllerOverride,
    current_identity: &PositiveControllerIdentity,
) -> ControllerChoiceResolution {
    let mismatch =
        !selection_matches_driver(&remembered.selection, driver_for_identity(current_identity));
    resolution(
        ControllerChoiceOutcome::Resolved {
            choice: ResolvedControllerChoice {
                selection: remembered.selection.clone(),
                driver: remembered.selection.driver(),
                source: ControllerChoiceSource::RememberedOverride,
                detected_identity: Some(current_identity.clone()),
                requires_experimental_mode: true,
                mismatch,
                override_scope: Some(ControllerOverrideScope::FingerprintBound),
                requires_experimental_compatibility_handshake: true,
            },
        },
        keep_override(),
    )
}

fn resolve_auto_detect(
    input: &ResolveControllerChoiceInput,
    detected_identity: Option<&PositiveControllerIdentity>,
) -> ControllerChoiceResolution {
    let override_update = if input.remembered_override.is_some() {
        ControllerOverrideUpdate::Clear {
            reason: ControllerOverrideInvalidationReason::SelectionChanged,
        }
    } else {
        keep_override()
    };
    let Some(identity) = detected_identity else {
        if input.decision.is_some() {
            return invalid_decision("Auto-detect did not request a decision", override_update);
        }
        return resolution(ControllerChoiceOutcome::SelectionRequired, override_update);
    };
    let Some(driver) = driver_for_identity(identity) else {
        return blocked(
            ControllerChoiceBlockReason::DetectedDriverUnavailable,
            "Beam Bench identified the controller, but no available driver matches it",
            override_update,
        );
    };
    if !driver_is_available(driver) {
        return blocked(
            ControllerChoiceBlockReason::DetectedDriverUnavailable,
            "Beam Bench identified the controller, but its driver is not available in this build",
            override_update,
        );
    }
    if !driver_supports_transport(driver, input.transport_kind) {
        return blocked(
            ControllerChoiceBlockReason::UnsupportedTransport,
            "The detected controller driver is not available over this transport",
            override_update,
        );
    }

    if input.decision.is_some() {
        return invalid_decision("Auto-detect did not request a decision", override_update);
    }
    resolved_normal(
        ExplicitControllerSelection::KnownDriver { driver },
        ControllerChoiceSource::AutoDetected,
        Some(identity.clone()),
        false,
        override_update,
    )
}

fn resolve_explicit_selection(
    input: &ResolveControllerChoiceInput,
    selected: ExplicitControllerSelection,
    detected_identity: Option<&PositiveControllerIdentity>,
) -> ControllerChoiceResolution {
    let invalidated_override_reason = input.remembered_override.as_ref().and_then(|remembered| {
        override_invalidation_reason(
            remembered,
            &selected,
            detected_identity,
            input.transport_kind,
            input.fingerprint.as_ref(),
        )
    });
    let stale_override_update = update_for_invalidation(invalidated_override_reason);
    let selected_driver = selected.driver();
    if matches!(selected_driver, ControllerDriverId::Unknown) {
        return blocked(
            ControllerChoiceBlockReason::UnsupportedDriver,
            "The selected controller driver is not available in this build",
            stale_override_update,
        );
    }
    if !driver_is_available(selected_driver) {
        return blocked(
            ControllerChoiceBlockReason::UnsupportedDriver,
            "The selected controller driver is not available in this build",
            stale_override_update,
        );
    }
    if !driver_supports_transport(selected_driver, input.transport_kind) {
        return blocked(
            ControllerChoiceBlockReason::UnsupportedTransport,
            "The selected controller driver is not available over this transport",
            stale_override_update,
        );
    }

    let remembered_matches =
        input.remembered_override.is_some() && invalidated_override_reason.is_none();
    if remembered_matches {
        if input.decision.is_some() {
            return invalid_decision(
                "A remembered controller override did not request a decision",
                keep_override(),
            );
        }
        return resolved_remembered(
            input
                .remembered_override
                .as_ref()
                .expect("remembered override checked above"),
            detected_identity.expect("matching override requires current positive identity"),
        );
    }

    let can_remember_override = input
        .fingerprint
        .as_ref()
        .is_some_and(fingerprint_can_bind_override)
        && detected_identity.and_then(identity_binding).is_some();

    let Some(identity) = detected_identity else {
        if input.protocol_compatible
            && matches!(
                selected,
                ExplicitControllerSelection::KnownDriver {
                    driver: ControllerDriverId::Grbl
                }
            )
        {
            if input.decision.is_some() {
                return invalid_decision(
                    "The explicit GRBL selection already passed its compatibility check",
                    stale_override_update,
                );
            }
            return resolved_normal(
                selected,
                ControllerChoiceSource::KnownDriverSelection,
                None,
                false,
                stale_override_update,
            );
        }
        if input.decision.is_some() {
            return invalid_decision(
                "The explicit controller selection did not request a decision",
                stale_override_update,
            );
        }
        return resolved_experimental(
            selected,
            None,
            input.transport_kind,
            input.fingerprint.as_ref(),
            false,
            stale_override_update,
        );
    };

    let detected_driver = driver_for_identity(identity);
    if selection_matches_driver(&selected, detected_driver) {
        if input.decision.is_some() {
            return invalid_decision(
                "The selected and detected controller drivers already match",
                stale_override_update,
            );
        }
        return resolved_normal(
            selected,
            ControllerChoiceSource::KnownDriverSelection,
            Some(identity.clone()),
            false,
            stale_override_update,
        );
    }

    let detected_driver_available = detected_driver.is_some_and(|driver| {
        driver_is_available(driver) && driver_supports_transport(driver, input.transport_kind)
    });
    let mut allowed_decisions = Vec::with_capacity(3);
    if detected_driver_available {
        allowed_decisions.push(ControllerMismatchDecision::UseDetected);
    }
    allowed_decisions.push(ControllerMismatchDecision::ContinueSelectedExperimentally);
    allowed_decisions.push(ControllerMismatchDecision::Cancel);

    match input.decision {
        None => resolution(
            ControllerChoiceOutcome::MismatchDecisionRequired {
                selected,
                detected_identity: identity.clone(),
                detected_driver,
                can_remember_override,
                invalidated_override_reason,
                allowed_decisions,
            },
            stale_override_update,
        ),
        Some(ControllerMismatchDecision::UseDetected) => {
            let Some(driver) = detected_driver.filter(|driver| {
                driver_is_available(*driver)
                    && driver_supports_transport(*driver, input.transport_kind)
            }) else {
                return blocked(
                    ControllerChoiceBlockReason::DetectedDriverUnavailable,
                    "The detected controller does not have an available driver for this transport",
                    stale_override_update,
                );
            };
            resolved_normal(
                ExplicitControllerSelection::KnownDriver { driver },
                ControllerChoiceSource::DetectedDriverChoice,
                Some(identity.clone()),
                true,
                stale_override_update,
            )
        }
        Some(ControllerMismatchDecision::ContinueSelectedExperimentally) => resolved_experimental(
            selected,
            Some(identity.clone()),
            input.transport_kind,
            input.fingerprint.as_ref(),
            true,
            stale_override_update,
        ),
        Some(ControllerMismatchDecision::Cancel) => {
            resolution(ControllerChoiceOutcome::Cancelled, keep_override())
        }
    }
}

/// Resolve controller selection, detected identity, and any remembered or current
/// mismatch decision without opening a transport or mutating runtime state.
///
/// The detected identity and fingerprint must be produced by backend-owned
/// discovery/handshake code. This input is intentionally not deserializable from a
/// frontend request.
pub fn resolve_controller_choice(
    input: &ResolveControllerChoiceInput,
) -> ControllerChoiceResolution {
    // Cancelling is always safe and never mutates remembered authorization.
    if input.decision == Some(ControllerMismatchDecision::Cancel) {
        return resolution(ControllerChoiceOutcome::Cancelled, keep_override());
    }
    let detected_identity = input
        .detected_identity
        .as_ref()
        .filter(|identity| identity.is_positive());
    match input.selection.explicit() {
        None => resolve_auto_detect(input, detected_identity),
        Some(selected) => resolve_explicit_selection(input, selected, detected_identity),
    }
}

/// Feed a backend-generated GRBL-family probe into the controller-choice
/// resolver without accepting identity or fingerprint claims from the UI.
pub fn resolve_grbl_family_probe_choice(
    input: &ResolveGrblFamilyProbeChoiceInput,
    probe: GrblFamilyIdentityProbeResult,
) -> ProbedControllerChoiceResolution {
    let detected_identity = probe.identity.positive_identity();
    let protocol_compatible = matches!(
        (probe.identity.dialect, probe.identity.status),
        (
            GrblFamilyDialect::Grbl,
            GrblFamilyIdentityStatus::ProtocolCompatible
        )
    );
    let fingerprint = fingerprint_device(input.transport_kind, &input.device_identity);
    let resolution = resolve_controller_choice(&ResolveControllerChoiceInput {
        selection: input.selection.clone(),
        detected_identity,
        protocol_compatible,
        transport_kind: input.transport_kind,
        fingerprint,
        remembered_override: input.remembered_override.clone(),
        decision: input.decision,
    });

    ProbedControllerChoiceResolution { probe, resolution }
}

/// Feed a backend-generated Marlin probe into the common controller-choice
/// resolver without accepting identity claims from the frontend.
pub fn resolve_marlin_probe_choice(
    input: &ResolveMarlinProbeChoiceInput,
    probe: MarlinIdentityProbeResult,
) -> ProbedMarlinChoiceResolution {
    let detected_identity = probe.identity.positive_identity();
    let fingerprint = fingerprint_device(TransportKind::Serial, &input.device_identity);
    let resolution = resolve_controller_choice(&ResolveControllerChoiceInput {
        selection: input.selection.clone(),
        detected_identity,
        protocol_compatible: false,
        transport_kind: TransportKind::Serial,
        fingerprint,
        remembered_override: input.remembered_override.clone(),
        decision: input.decision,
    });

    ProbedMarlinChoiceResolution { probe, resolution }
}

/// Feed backend-generated Smoothieware firmware/config evidence into the
/// common controller-choice resolver.
pub fn resolve_smoothieware_probe_choice(
    input: &ResolveSmoothiewareProbeChoiceInput,
    probe: SmoothiewareIdentityProbeResult,
) -> ProbedSmoothiewareChoiceResolution {
    let detected_identity = probe.identity.positive_identity();
    let fingerprint = fingerprint_device(TransportKind::Serial, &input.device_identity);
    let resolution = resolve_controller_choice(&ResolveControllerChoiceInput {
        selection: input.selection.clone(),
        detected_identity,
        protocol_compatible: false,
        transport_kind: TransportKind::Serial,
        fingerprint,
        remembered_override: input.remembered_override.clone(),
        decision: input.decision,
    });

    ProbedSmoothiewareChoiceResolution { probe, resolution }
}

/// Run the explicit read-only probe against a caller-owned open session and
/// immediately resolve its normalized evidence through controller-choice
/// policy. This does not create, register, or mark a runtime session Ready.
pub fn probe_and_resolve_grbl_family_choice(
    session: &mut GrblSession,
    config: GrblFamilyIdentityProbeConfig,
    input: &ResolveGrblFamilyProbeChoiceInput,
) -> Result<ProbedControllerChoiceResolution, GrblError> {
    let probe = session.probe_grbl_family_identity(config)?;
    Ok(resolve_grbl_family_probe_choice(input, probe))
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::{
        ControllerEvidenceState, ControllerProductTier, DeviceCapabilities, DiscoveryCandidate,
        GrblFamilyDialect, GrblFamilyIdentity, GrblFamilyIdentityEvidence,
        GrblFamilyIdentityStatus,
    };
    use beambench_grbl::GrblFamilyIdentityProbeOutcome;

    fn grbl_identity(version: &str) -> PositiveControllerIdentity {
        PositiveControllerIdentity {
            family: ControllerFamily::Gcode,
            model: ControllerModel::Grbl,
            firmware_identity: Some("Grbl".to_string()),
            firmware_version: Some(version.to_string()),
            evidence: vec![format!("Grbl {version} startup banner")],
        }
    }

    fn ruida_identity() -> PositiveControllerIdentity {
        PositiveControllerIdentity {
            family: ControllerFamily::Dsp,
            model: ControllerModel::Ruida,
            firmware_identity: Some("RDC6445G".to_string()),
            firmware_version: Some("1.0".to_string()),
            evidence: vec!["Controller-native identity response".to_string()],
        }
    }

    fn unavailable_dsp_identity() -> PositiveControllerIdentity {
        PositiveControllerIdentity {
            family: ControllerFamily::Dsp,
            model: ControllerModel::Topwisdom,
            firmware_identity: Some("TopWisdom".to_string()),
            firmware_version: Some("1.0".to_string()),
            evidence: vec!["Controller-native identity response".to_string()],
        }
    }

    fn fluidnc_identity() -> PositiveControllerIdentity {
        PositiveControllerIdentity {
            family: ControllerFamily::Gcode,
            model: ControllerModel::FluidNc,
            firmware_identity: Some("FluidNC".to_string()),
            firmware_version: Some("4.0.3".to_string()),
            evidence: vec!["FluidNC controller information response".to_string()],
        }
    }

    fn grblhal_identity() -> PositiveControllerIdentity {
        PositiveControllerIdentity {
            family: ControllerFamily::Gcode,
            model: ControllerModel::GrblHal,
            firmware_identity: Some("grblHAL".to_string()),
            firmware_version: Some("1.1f.20241019".to_string()),
            evidence: vec!["grblHAL firmware response".to_string()],
        }
    }

    fn marlin_probe() -> MarlinIdentityProbeResult {
        use beambench_marlin::{
            MarlinDialect, MarlinIdentity, MarlinIdentityEvidence, MarlinIdentityProbeOutcome,
            MarlinIdentityStatus,
        };

        MarlinIdentityProbeResult {
            identity: MarlinIdentity {
                status: MarlinIdentityStatus::Identified,
                dialect: MarlinDialect::Generic,
                firmware_identity: Some("Marlin".to_string()),
                firmware_version: Some("2.1.3".to_string()),
                capabilities: [("EMERGENCY_PARSER".to_string(), true)]
                    .into_iter()
                    .collect(),
                evidence: vec![
                    MarlinIdentityEvidence::FirmwareInfo,
                    MarlinIdentityEvidence::ExtendedCapabilityReport,
                ],
                ..MarlinIdentity::default()
            },
            outcome: MarlinIdentityProbeOutcome::Succeeded,
        }
    }

    fn snapmaker_probe() -> MarlinIdentityProbeResult {
        use beambench_marlin::{MarlinDialect, MarlinIdentityEvidence};

        let mut probe = marlin_probe();
        probe.identity.dialect = MarlinDialect::Snapmaker;
        probe.identity.firmware_version = Some("SM2-4.7.2".to_string());
        probe
            .identity
            .capabilities
            .insert("EMERGENCY_PARSER".to_string(), false);
        probe
            .identity
            .evidence
            .push(MarlinIdentityEvidence::SnapmakerFirmwareSignature);
        probe
    }

    fn smoothieware_probe() -> SmoothiewareIdentityProbeResult {
        use beambench_smoothieware::{
            SmoothiewareIdentity, SmoothiewareIdentityEvidence, SmoothiewareIdentityProbeOutcome,
            SmoothiewareIdentityStatus,
        };

        SmoothiewareIdentityProbeResult {
            identity: SmoothiewareIdentity {
                status: SmoothiewareIdentityStatus::Identified,
                firmware_identity: Some("Smoothieware".to_string()),
                firmware_version: Some("edge-6ce309b".to_string()),
                protocol_version: Some("1.0".to_string()),
                grbl_mode: Some(false),
                supports_arcs: Some(true),
                laser_module_enabled: Some(true),
                laser_maximum_s_value: Some(1.0),
                laser_proportional_power: Some(true),
                evidence: vec![
                    SmoothiewareIdentityEvidence::FirmwareInfo,
                    SmoothiewareIdentityEvidence::ExtendedFirmwareReport,
                    SmoothiewareIdentityEvidence::LaserConfiguration,
                ],
                ..SmoothiewareIdentity::default()
            },
            outcome: SmoothiewareIdentityProbeOutcome::Succeeded,
        }
    }

    fn fingerprint(value: &str, strength: DeviceFingerprintStrength) -> DeviceFingerprint {
        DeviceFingerprint {
            schema_version: FINGERPRINT_SCHEMA_VERSION,
            strength,
            value: value.to_string(),
        }
    }

    fn known_grbl() -> ControllerSelection {
        ControllerSelection::KnownDriver {
            driver: ControllerDriverId::Grbl,
        }
    }

    fn input(selection: ControllerSelection) -> ResolveControllerChoiceInput {
        ResolveControllerChoiceInput {
            selection,
            detected_identity: None,
            protocol_compatible: false,
            transport_kind: TransportKind::Serial,
            fingerprint: Some(fingerprint("device-a", DeviceFingerprintStrength::Strong)),
            remembered_override: None,
            decision: None,
        }
    }

    fn probe_choice_input(selection: ControllerSelection) -> ResolveGrblFamilyProbeChoiceInput {
        ResolveGrblFamilyProbeChoiceInput {
            selection,
            device_identity: DeviceIdentity {
                port_name: Some("/dev/tty.usbmodem1".to_string()),
                ..DeviceIdentity::default()
            },
            transport_kind: TransportKind::Serial,
            remembered_override: None,
            decision: None,
        }
    }

    fn probe_result(identity: GrblFamilyIdentity) -> GrblFamilyIdentityProbeResult {
        GrblFamilyIdentityProbeResult {
            identity,
            controller_info: GrblFamilyIdentityProbeOutcome::Succeeded,
            extended_controller_info: GrblFamilyIdentityProbeOutcome::NotNeeded,
        }
    }

    fn exact_grbl_family_probe(dialect: GrblFamilyDialect) -> GrblFamilyIdentityProbeResult {
        let (firmware_identity, firmware_version, evidence) = match dialect {
            GrblFamilyDialect::FluidNc => (
                "FluidNC",
                "4.0.3",
                GrblFamilyIdentityEvidence::ControllerInfoVersion,
            ),
            GrblFamilyDialect::GrblHal => (
                "grblHAL",
                "1.1f.20260709",
                GrblFamilyIdentityEvidence::FirmwareIdentityMessage,
            ),
            _ => panic!("helper only creates exact FluidNC or grblHAL probes"),
        };
        probe_result(GrblFamilyIdentity {
            dialect,
            status: GrblFamilyIdentityStatus::Identified,
            firmware_identity: Some(firmware_identity.to_string()),
            firmware_version: Some(firmware_version.to_string()),
            evidence: vec![evidence],
        })
    }

    fn expect_resolved(
        resolution: ControllerChoiceResolution,
    ) -> (ResolvedControllerChoice, ControllerOverrideUpdate) {
        let ControllerChoiceResolution {
            outcome,
            override_update,
            ..
        } = resolution;
        match outcome {
            ControllerChoiceOutcome::Resolved { choice } => (choice, override_update),
            other => panic!("expected resolved controller choice, got {other:?}"),
        }
    }

    fn invalidation_reason(
        resolution: &ControllerChoiceResolution,
    ) -> Option<ControllerOverrideInvalidationReason> {
        match &resolution.outcome {
            ControllerChoiceOutcome::MismatchDecisionRequired {
                invalidated_override_reason,
                ..
            } => *invalidated_override_reason,
            _ => None,
        }
    }

    #[test]
    fn fingerprints_ignore_volatile_discovery_fields_and_candidate_id() {
        let identity = DeviceIdentity {
            display_name: "First label".to_string(),
            manufacturer: Some("Ignored vendor label".to_string()),
            description: Some("GRBL-ish text is not identity".to_string()),
            product: Some("Ignored product label".to_string()),
            serial_number: Some("SERIAL-123".to_string()),
            vendor_id: Some(0x1234),
            product_id: Some(0x5678),
            port_name: Some("/dev/tty.usbmodem1".to_string()),
            host: None,
            tcp_port: None,
            udp_port: None,
            usb_path: None,
        };
        let candidate = |id: &str, display_name: &str| DiscoveryCandidate {
            id: id.to_string(),
            controller_family: ControllerFamily::Unknown,
            controller_model: ControllerModel::Unknown,
            transport_kind: TransportKind::Serial,
            identity: DeviceIdentity {
                display_name: display_name.to_string(),
                ..identity.clone()
            },
            confidence: if id == "candidate-a" { 0.0 } else { 0.99 },
            capabilities: DeviceCapabilities::disabled(),
            product_tier: Some(ControllerProductTier::Unavailable),
            evidence_state: Some(ControllerEvidenceState::Emulated),
            status_text: format!("volatile status for {id}"),
            unsupported_reason: Some(format!("volatile reason for {id}")),
        };
        let first = fingerprint_discovery_candidate(&candidate("candidate-a", "First label"));
        let second = fingerprint_discovery_candidate(&candidate("candidate-b", "Other label"));
        assert_eq!(first, second);
        assert_eq!(first.unwrap().strength, DeviceFingerprintStrength::Strong);
    }

    #[test]
    fn endpoint_only_fingerprints_are_weak_and_material_changes_change_digest() {
        let serial = DeviceIdentity {
            display_name: "Serial".to_string(),
            port_name: Some("/dev/tty.usbserial-a".to_string()),
            ..DeviceIdentity::default()
        };
        let first = fingerprint_device(TransportKind::Serial, &serial).unwrap();
        assert_eq!(first.strength, DeviceFingerprintStrength::Weak);
        let changed = fingerprint_device(
            TransportKind::Serial,
            &DeviceIdentity {
                port_name: Some("/dev/tty.usbserial-b".to_string()),
                ..serial.clone()
            },
        )
        .unwrap();
        assert_ne!(first.value, changed.value);

        let tcp = fingerprint_device(
            TransportKind::Tcp,
            &DeviceIdentity {
                display_name: "TCP".to_string(),
                host: Some("LASER.LOCAL".to_string()),
                tcp_port: Some(5000),
                ..DeviceIdentity::default()
            },
        )
        .unwrap();
        assert_eq!(tcp.strength, DeviceFingerprintStrength::Weak);
        assert_eq!(tcp.value.len(), 64);
    }

    #[test]
    fn probe_bridge_auto_selects_exact_fluidnc_without_confirmation() {
        let probe = exact_grbl_family_probe(GrblFamilyDialect::FluidNc);
        let result = resolve_grbl_family_probe_choice(
            &probe_choice_input(ControllerSelection::AutoDetect),
            probe.clone(),
        );

        assert_eq!(result.probe, probe);
        let (choice, _) = expect_resolved(result.resolution);
        assert_eq!(choice.driver, ControllerDriverId::FluidNc);
        assert_eq!(choice.source, ControllerChoiceSource::AutoDetected);
        assert!(choice.requires_experimental_mode);
        assert!(!choice.requires_experimental_compatibility_handshake);
    }

    #[test]
    fn probe_bridge_auto_selects_exact_grblhal_without_confirmation() {
        let probe = exact_grbl_family_probe(GrblFamilyDialect::GrblHal);
        let result = resolve_grbl_family_probe_choice(
            &probe_choice_input(ControllerSelection::AutoDetect),
            probe.clone(),
        );

        assert_eq!(result.probe, probe);
        let (choice, _) = expect_resolved(result.resolution);
        assert_eq!(choice.driver, ControllerDriverId::GrblHal);
        assert_eq!(choice.source, ControllerChoiceSource::AutoDetected);
        assert!(choice.requires_experimental_mode);
        assert!(!choice.requires_experimental_compatibility_handshake);
    }

    #[test]
    fn probe_bridge_preserves_tcp_transport_for_exact_network_adapters() {
        for (dialect, expected_driver) in [
            (GrblFamilyDialect::FluidNc, ControllerDriverId::FluidNc),
            (GrblFamilyDialect::GrblHal, ControllerDriverId::GrblHal),
        ] {
            let mut input = probe_choice_input(ControllerSelection::AutoDetect);
            input.transport_kind = TransportKind::Tcp;
            input.device_identity = DeviceIdentity {
                display_name: "laser.local:23".to_string(),
                host: Some("laser.local".to_string()),
                tcp_port: Some(23),
                ..DeviceIdentity::default()
            };
            let result = resolve_grbl_family_probe_choice(&input, exact_grbl_family_probe(dialect));
            let (choice, _) = expect_resolved(result.resolution);
            assert_eq!(choice.driver, expected_driver);
            assert_eq!(choice.source, ControllerChoiceSource::AutoDetected);
            assert!(choice.requires_experimental_mode);
            assert!(!choice.requires_experimental_compatibility_handshake);
        }
    }

    #[test]
    fn tcp_transport_blocks_serial_only_controller_choices() {
        let mut request = input(ControllerSelection::KnownDriver {
            driver: ControllerDriverId::Marlin,
        });
        request.transport_kind = TransportKind::Tcp;
        assert!(matches!(
            resolve_controller_choice(&request).outcome,
            ControllerChoiceOutcome::Blocked {
                reason: ControllerChoiceBlockReason::UnsupportedTransport,
                ..
            }
        ));
    }

    #[test]
    fn marlin_probe_bridge_auto_selects_exact_standard_firmware() {
        let probe = marlin_probe();
        let result = resolve_marlin_probe_choice(
            &ResolveMarlinProbeChoiceInput {
                selection: ControllerSelection::AutoDetect,
                device_identity: DeviceIdentity {
                    port_name: Some("/dev/tty.usbmodem-marlin".to_string()),
                    ..DeviceIdentity::default()
                },
                remembered_override: None,
                decision: None,
            },
            probe.clone(),
        );

        assert_eq!(result.probe, probe);
        let (choice, _) = expect_resolved(result.resolution);
        assert_eq!(choice.driver, ControllerDriverId::Marlin);
        assert_eq!(choice.source, ControllerChoiceSource::AutoDetected);
        assert!(choice.requires_experimental_mode);
        assert!(!choice.requires_experimental_compatibility_handshake);
        assert_eq!(
            choice.detected_identity.unwrap().model,
            ControllerModel::Marlin
        );
    }

    #[test]
    fn marlin_probe_bridge_auto_selects_exact_snapmaker_2_firmware() {
        let result = resolve_marlin_probe_choice(
            &ResolveMarlinProbeChoiceInput {
                selection: ControllerSelection::AutoDetect,
                device_identity: DeviceIdentity {
                    port_name: Some("/dev/tty.usbmodem-snapmaker".to_string()),
                    ..DeviceIdentity::default()
                },
                remembered_override: None,
                decision: None,
            },
            snapmaker_probe(),
        );

        let (choice, _) = expect_resolved(result.resolution);
        assert_eq!(choice.driver, ControllerDriverId::Snapmaker);
        assert_eq!(choice.source, ControllerChoiceSource::AutoDetected);
        assert!(choice.requires_experimental_mode);
        assert_eq!(
            choice.detected_identity.unwrap().model,
            ControllerModel::Snapmaker
        );
    }

    #[test]
    fn smoothieware_probe_bridge_auto_selects_exact_laser_firmware() {
        let result = resolve_smoothieware_probe_choice(
            &ResolveSmoothiewareProbeChoiceInput {
                selection: ControllerSelection::AutoDetect,
                device_identity: DeviceIdentity {
                    port_name: Some("/dev/tty.usbmodem-smoothie".to_string()),
                    ..DeviceIdentity::default()
                },
                remembered_override: None,
                decision: None,
            },
            smoothieware_probe(),
        );

        let (choice, _) = expect_resolved(result.resolution);
        assert_eq!(choice.driver, ControllerDriverId::Smoothieware);
        assert_eq!(choice.source, ControllerChoiceSource::AutoDetected);
        assert!(choice.requires_experimental_mode);
        assert_eq!(
            choice.detected_identity.unwrap().model,
            ControllerModel::Smoothieware
        );
    }

    #[test]
    fn probe_bridge_exposes_named_mismatch_and_backend_fingerprint_strength() {
        let mut input = probe_choice_input(known_grbl());
        input.device_identity.serial_number = Some("SERIAL-123".to_string());
        input.device_identity.vendor_id = Some(0x1234);
        input.device_identity.product_id = Some(0x5678);

        let result = resolve_grbl_family_probe_choice(
            &input,
            exact_grbl_family_probe(GrblFamilyDialect::FluidNc),
        );

        assert!(matches!(
            result.resolution.outcome,
            ControllerChoiceOutcome::MismatchDecisionRequired {
                detected_driver: Some(ControllerDriverId::FluidNc),
                can_remember_override: true,
                ref allowed_decisions,
                ..
            } if allowed_decisions == &vec![
                ControllerMismatchDecision::UseDetected,
                ControllerMismatchDecision::ContinueSelectedExperimentally,
                ControllerMismatchDecision::Cancel,
            ]
        ));
    }

    #[test]
    fn inconclusive_probe_requires_auto_selection_but_honors_an_explicit_generic_choice() {
        let probe = GrblFamilyIdentityProbeResult {
            identity: GrblFamilyIdentity::default(),
            controller_info: GrblFamilyIdentityProbeOutcome::Succeeded,
            extended_controller_info: GrblFamilyIdentityProbeOutcome::Rejected(3),
        };
        let auto = resolve_grbl_family_probe_choice(
            &probe_choice_input(ControllerSelection::AutoDetect),
            probe.clone(),
        );
        assert_eq!(
            auto.resolution.outcome,
            ControllerChoiceOutcome::SelectionRequired
        );

        let generic = resolve_grbl_family_probe_choice(
            &probe_choice_input(ControllerSelection::GenericGrblCompatible),
            probe,
        );
        let (choice, _) = expect_resolved(generic.resolution);
        assert_eq!(choice.driver, ControllerDriverId::Grbl);
        assert!(choice.requires_experimental_mode);
        assert_eq!(
            choice.override_scope,
            Some(ControllerOverrideScope::SessionOnly)
        );
        assert!(choice.requires_experimental_compatibility_handshake);
    }

    #[test]
    fn auto_detect_requires_positive_identity() {
        let request = input(ControllerSelection::AutoDetect);
        let resolution = resolve_controller_choice(&request);
        assert_eq!(
            resolution.outcome,
            ControllerChoiceOutcome::SelectionRequired
        );
        assert_eq!(resolution.override_update, ControllerOverrideUpdate::Keep);

        let detected = grbl_identity("1.1h");
        let mut with_stale_override = request;
        with_stale_override.remembered_override = Some(FingerprintBoundControllerOverride {
            selection: ExplicitControllerSelection::GenericGrblCompatible,
            detected_identity: identity_binding(&detected).unwrap(),
            transport_kind: TransportKind::Serial,
            fingerprint: fingerprint("device-a", DeviceFingerprintStrength::Strong),
        });
        let resolution = resolve_controller_choice(&with_stale_override);
        assert_eq!(
            resolution.outcome,
            ControllerChoiceOutcome::SelectionRequired
        );
        assert_eq!(
            resolution.override_update,
            ControllerOverrideUpdate::Clear {
                reason: ControllerOverrideInvalidationReason::SelectionChanged,
            }
        );
    }

    #[test]
    fn auto_detect_selects_available_grbl_driver_without_claiming_experimental_override() {
        let mut request = input(ControllerSelection::AutoDetect);
        request.detected_identity = Some(grbl_identity("1.1h"));
        let (choice, override_update) = expect_resolved(resolve_controller_choice(&request));
        assert_eq!(choice.driver, ControllerDriverId::Grbl);
        assert_eq!(choice.source, ControllerChoiceSource::AutoDetected);
        assert!(!choice.requires_experimental_mode);
        assert!(!choice.mismatch);
        assert!(!choice.requires_experimental_compatibility_handshake);
        assert_eq!(override_update, ControllerOverrideUpdate::Keep);
    }

    #[test]
    fn auto_detect_blocks_unavailable_driver_and_unsupported_transport() {
        let mut unavailable = input(ControllerSelection::AutoDetect);
        unavailable.detected_identity = Some(unavailable_dsp_identity());
        assert!(matches!(
            resolve_controller_choice(&unavailable).outcome,
            ControllerChoiceOutcome::Blocked {
                reason: ControllerChoiceBlockReason::DetectedDriverUnavailable,
                ..
            }
        ));

        let mut wrong_transport = input(ControllerSelection::AutoDetect);
        wrong_transport.detected_identity = Some(grbl_identity("1.1h"));
        wrong_transport.transport_kind = TransportKind::Tcp;
        assert!(matches!(
            resolve_controller_choice(&wrong_transport).outcome,
            ControllerChoiceOutcome::Blocked {
                reason: ControllerChoiceBlockReason::UnsupportedTransport,
                ..
            }
        ));
    }

    #[test]
    fn auto_detect_selects_exact_grblhal_without_grbl_fallback_or_confirmation() {
        let identity = grblhal_identity();
        assert_eq!(
            driver_for_identity(&identity),
            Some(ControllerDriverId::GrblHal)
        );
        assert!(driver_is_available(ControllerDriverId::GrblHal));
        let mut request = input(ControllerSelection::AutoDetect);
        request.detected_identity = Some(identity);

        let (choice, override_update) = expect_resolved(resolve_controller_choice(&request));
        assert_eq!(choice.driver, ControllerDriverId::GrblHal);
        assert_eq!(choice.source, ControllerChoiceSource::AutoDetected);
        assert!(choice.requires_experimental_mode);
        assert!(!choice.requires_experimental_compatibility_handshake);
        assert_eq!(override_update, ControllerOverrideUpdate::Keep);
    }

    #[test]
    fn explicit_exact_grblhal_selection_resolves_as_experimental() {
        let mut request = input(ControllerSelection::KnownDriver {
            driver: ControllerDriverId::GrblHal,
        });
        request.detected_identity = Some(grblhal_identity());

        let (choice, _) = expect_resolved(resolve_controller_choice(&request));
        assert_eq!(choice.driver, ControllerDriverId::GrblHal);
        assert_eq!(choice.source, ControllerChoiceSource::KnownDriverSelection);
        assert!(choice.requires_experimental_mode);
        assert!(!choice.requires_experimental_compatibility_handshake);
    }

    #[test]
    fn explicit_laserpecker_selection_is_available_over_serial_and_tcp() {
        for transport_kind in [TransportKind::Serial, TransportKind::Tcp] {
            let mut request = input(ControllerSelection::KnownDriver {
                driver: ControllerDriverId::LaserPecker,
            });
            request.transport_kind = transport_kind;

            let (choice, _) = expect_resolved(resolve_controller_choice(&request));
            assert_eq!(choice.driver, ControllerDriverId::LaserPecker);
            assert_eq!(
                choice.source,
                ControllerChoiceSource::UserExperimentalOverride
            );
            assert!(choice.requires_experimental_mode);
            assert!(choice.requires_experimental_compatibility_handshake);
        }
    }

    #[test]
    fn explicit_exact_fluidnc_selection_resolves_as_experimental() {
        let mut request = input(ControllerSelection::KnownDriver {
            driver: ControllerDriverId::FluidNc,
        });
        request.detected_identity = Some(fluidnc_identity());

        let (choice, _) = expect_resolved(resolve_controller_choice(&request));
        assert_eq!(choice.driver, ControllerDriverId::FluidNc);
        assert_eq!(choice.source, ControllerChoiceSource::KnownDriverSelection);
        assert!(choice.requires_experimental_mode);
        assert!(!choice.requires_experimental_compatibility_handshake);
    }

    #[test]
    fn explicit_grbl_without_firmware_identity_is_session_only() {
        let request = input(known_grbl());
        let (choice, override_update) = expect_resolved(resolve_controller_choice(&request));
        assert!(choice.requires_experimental_mode);
        assert_eq!(
            choice.override_scope,
            Some(ControllerOverrideScope::SessionOnly)
        );
        assert_eq!(override_update, ControllerOverrideUpdate::Keep);
        assert!(choice.requires_experimental_compatibility_handshake);
    }

    #[test]
    fn weak_generic_choice_is_session_only_and_reprompts_next_time() {
        let mut request = input(ControllerSelection::GenericGrblCompatible);
        request.detected_identity = Some(grbl_identity("1.1h"));
        request.fingerprint = Some(fingerprint("weak-port", DeviceFingerprintStrength::Weak));
        assert!(matches!(
            resolve_controller_choice(&request).outcome,
            ControllerChoiceOutcome::MismatchDecisionRequired {
                can_remember_override: false,
                ..
            }
        ));
        request.decision = Some(ControllerMismatchDecision::ContinueSelectedExperimentally);
        let (choice, override_update) = expect_resolved(resolve_controller_choice(&request));
        assert_eq!(
            choice.override_scope,
            Some(ControllerOverrideScope::SessionOnly)
        );
        assert_eq!(override_update, ControllerOverrideUpdate::Keep);
        assert_eq!(choice.driver, ControllerDriverId::Grbl);
        assert!(choice.requires_experimental_compatibility_handshake);
    }

    #[test]
    fn strong_device_without_firmware_binding_is_still_session_only() {
        let mut request = input(ControllerSelection::GenericGrblCompatible);
        request.detected_identity = Some(PositiveControllerIdentity {
            family: ControllerFamily::Gcode,
            model: ControllerModel::Grbl,
            firmware_identity: Some("Grbl".to_string()),
            firmware_version: None,
            evidence: vec!["Trusted passive identity rule".to_string()],
        });
        let prompt = resolve_controller_choice(&request);
        assert!(matches!(
            prompt.outcome,
            ControllerChoiceOutcome::MismatchDecisionRequired {
                can_remember_override: false,
                ..
            }
        ));
        request.decision = Some(ControllerMismatchDecision::ContinueSelectedExperimentally);
        let (choice, override_update) = expect_resolved(resolve_controller_choice(&request));
        assert_eq!(
            choice.override_scope,
            Some(ControllerOverrideScope::SessionOnly)
        );
        assert_eq!(override_update, ControllerOverrideUpdate::Keep);
    }

    #[test]
    fn matching_known_driver_resolves_without_override() {
        let mut request = input(known_grbl());
        request.detected_identity = Some(grbl_identity("1.1h"));
        let (choice, override_update) = expect_resolved(resolve_controller_choice(&request));
        assert_eq!(choice.source, ControllerChoiceSource::KnownDriverSelection);
        assert!(!choice.requires_experimental_mode);
        assert!(!choice.mismatch);
        assert!(choice.override_scope.is_none());
        assert_eq!(override_update, ControllerOverrideUpdate::Keep);
    }

    #[test]
    fn detected_mismatch_requires_a_choice_and_supports_all_three_outcomes() {
        let mut request = input(ControllerSelection::GenericGrblCompatible);
        request.detected_identity = Some(grbl_identity("1.1h"));
        assert!(matches!(
            resolve_controller_choice(&request).outcome,
            ControllerChoiceOutcome::MismatchDecisionRequired {
                detected_driver: Some(ControllerDriverId::Grbl),
                ref allowed_decisions,
                ..
            } if allowed_decisions == &vec![
                ControllerMismatchDecision::UseDetected,
                ControllerMismatchDecision::ContinueSelectedExperimentally,
                ControllerMismatchDecision::Cancel,
            ]
        ));

        request.decision = Some(ControllerMismatchDecision::UseDetected);
        let (detected_choice, detected_update) =
            expect_resolved(resolve_controller_choice(&request));
        assert_eq!(
            detected_choice.source,
            ControllerChoiceSource::DetectedDriverChoice
        );
        assert!(detected_choice.mismatch);
        assert!(!detected_choice.requires_experimental_mode);
        assert_eq!(detected_update, ControllerOverrideUpdate::Keep);

        request.decision = Some(ControllerMismatchDecision::ContinueSelectedExperimentally);
        let selected_resolution = resolve_controller_choice(&request);
        let replacement = selected_resolution
            .replacement_override
            .as_ref()
            .expect("strong device and exact firmware should create a local replacement");
        let serialized_resolution = serde_json::to_string(&selected_resolution).unwrap();
        assert!(!serialized_resolution.contains(&replacement.fingerprint.value));
        let debug_resolution = format!("{selected_resolution:?}");
        assert!(!debug_resolution.contains(&replacement.fingerprint.value));
        assert!(debug_resolution.contains("[redacted]"));
        let persisted = serde_json::to_string(replacement).unwrap();
        assert!(!persisted.contains("startup banner"));
        assert!(!persisted.contains("1.1h"));
        let (selected_choice, selected_update) = expect_resolved(selected_resolution);
        assert!(selected_choice.requires_experimental_mode);
        assert!(selected_choice.mismatch);
        assert_eq!(selected_update, ControllerOverrideUpdate::Replace);

        request.decision = Some(ControllerMismatchDecision::Cancel);
        let cancelled = resolve_controller_choice(&request);
        assert_eq!(cancelled.outcome, ControllerChoiceOutcome::Cancelled);
        assert_eq!(cancelled.override_update, ControllerOverrideUpdate::Keep);
    }

    #[test]
    fn exact_remembered_override_is_reused_without_a_new_prompt() {
        let selected = ExplicitControllerSelection::GenericGrblCompatible;
        let detected = grbl_identity("1.1h");
        let strong = fingerprint("device-a", DeviceFingerprintStrength::Strong);
        let remembered = FingerprintBoundControllerOverride {
            selection: selected.clone(),
            detected_identity: identity_binding(&detected).unwrap(),
            transport_kind: TransportKind::Serial,
            fingerprint: strong.clone(),
        };
        let mut current = detected.clone();
        current.evidence = vec!["Fresh handshake evidence".to_string()];
        let request = ResolveControllerChoiceInput {
            selection: ControllerSelection::GenericGrblCompatible,
            detected_identity: Some(current),
            protocol_compatible: false,
            transport_kind: TransportKind::Serial,
            fingerprint: Some(strong),
            remembered_override: Some(remembered),
            decision: None,
        };
        let (choice, override_update) = expect_resolved(resolve_controller_choice(&request));
        assert_eq!(choice.source, ControllerChoiceSource::RememberedOverride);
        assert_eq!(
            choice.detected_identity.unwrap().evidence,
            vec!["Fresh handshake evidence".to_string()]
        );
        assert_eq!(
            choice.override_scope,
            Some(ControllerOverrideScope::FingerprintBound)
        );
        assert_eq!(override_update, ControllerOverrideUpdate::Keep);
    }

    #[test]
    fn material_changes_invalidate_a_remembered_override_and_reprompt() {
        let selected = ExplicitControllerSelection::GenericGrblCompatible;
        let detected = grbl_identity("1.1h");
        let strong = fingerprint("device-a", DeviceFingerprintStrength::Strong);
        let remembered = FingerprintBoundControllerOverride {
            selection: selected.clone(),
            detected_identity: identity_binding(&detected).unwrap(),
            transport_kind: TransportKind::Serial,
            fingerprint: strong.clone(),
        };
        let base = ResolveControllerChoiceInput {
            selection: ControllerSelection::GenericGrblCompatible,
            detected_identity: Some(detected.clone()),
            protocol_compatible: false,
            transport_kind: TransportKind::Serial,
            fingerprint: Some(strong.clone()),
            remembered_override: Some(remembered.clone()),
            decision: None,
        };

        let mut firmware_changed = base.clone();
        firmware_changed.detected_identity = Some(grbl_identity("1.1i"));
        let firmware_resolution = resolve_controller_choice(&firmware_changed);
        assert_eq!(
            invalidation_reason(&firmware_resolution),
            Some(ControllerOverrideInvalidationReason::FirmwareIdentityChanged)
        );
        assert_eq!(
            firmware_resolution.override_update,
            ControllerOverrideUpdate::Clear {
                reason: ControllerOverrideInvalidationReason::FirmwareIdentityChanged,
            }
        );

        let detected_uppercase = grbl_identity("1.1A");
        let remembered_uppercase = FingerprintBoundControllerOverride {
            selection: selected.clone(),
            detected_identity: identity_binding(&detected_uppercase).unwrap(),
            transport_kind: TransportKind::Serial,
            fingerprint: strong.clone(),
        };
        assert_eq!(
            override_invalidation_reason(
                &remembered_uppercase,
                &selected,
                Some(&grbl_identity("1.1a")),
                TransportKind::Serial,
                Some(&strong),
            ),
            Some(ControllerOverrideInvalidationReason::FirmwareIdentityChanged)
        );

        let mut transport_changed = base.clone();
        transport_changed.transport_kind = TransportKind::Tcp;
        let transport_resolution = resolve_controller_choice(&transport_changed);
        assert!(matches!(
            transport_resolution.outcome,
            ControllerChoiceOutcome::Blocked {
                reason: ControllerChoiceBlockReason::UnsupportedTransport,
                ..
            }
        ));
        assert_eq!(
            override_invalidation_reason(
                &remembered,
                &selected,
                Some(&detected),
                TransportKind::Tcp,
                Some(&strong),
            ),
            Some(ControllerOverrideInvalidationReason::TransportChanged)
        );

        let mut fingerprint_changed = base.clone();
        fingerprint_changed.fingerprint =
            Some(fingerprint("device-b", DeviceFingerprintStrength::Strong));
        assert_eq!(
            invalidation_reason(&resolve_controller_choice(&fingerprint_changed)),
            Some(ControllerOverrideInvalidationReason::DeviceFingerprintChanged)
        );

        let mut identity_changed = base.clone();
        identity_changed.detected_identity = Some(ruida_identity());
        assert_eq!(
            invalidation_reason(&resolve_controller_choice(&identity_changed)),
            Some(ControllerOverrideInvalidationReason::DetectedIdentityChanged)
        );

        let mut selection_changed = base;
        selection_changed.selection = known_grbl();
        assert_eq!(
            override_invalidation_reason(
                &remembered,
                &ExplicitControllerSelection::KnownDriver {
                    driver: ControllerDriverId::Grbl,
                },
                Some(&detected),
                TransportKind::Serial,
                Some(&strong),
            ),
            Some(ControllerOverrideInvalidationReason::SelectionChanged)
        );
        let (safe_matching_choice, safe_matching_update) =
            expect_resolved(resolve_controller_choice(&selection_changed));
        assert!(!safe_matching_choice.requires_experimental_mode);
        assert_eq!(
            safe_matching_update,
            ControllerOverrideUpdate::Clear {
                reason: ControllerOverrideInvalidationReason::SelectionChanged,
            }
        );

        let detected_ruida = ruida_identity();
        let remembered_known = FingerprintBoundControllerOverride {
            selection: ExplicitControllerSelection::KnownDriver {
                driver: ControllerDriverId::Grbl,
            },
            detected_identity: identity_binding(&detected_ruida).unwrap(),
            transport_kind: TransportKind::Serial,
            fingerprint: strong.clone(),
        };
        let reprompt_after_selection_change = ResolveControllerChoiceInput {
            selection: ControllerSelection::GenericGrblCompatible,
            detected_identity: Some(detected_ruida),
            protocol_compatible: false,
            transport_kind: TransportKind::Serial,
            fingerprint: Some(strong),
            remembered_override: Some(remembered_known),
            decision: None,
        };
        assert_eq!(
            invalidation_reason(&resolve_controller_choice(&reprompt_after_selection_change)),
            Some(ControllerOverrideInvalidationReason::SelectionChanged)
        );
    }

    #[test]
    fn weak_or_missing_fingerprint_never_reuses_remembered_authorization() {
        let detected = grbl_identity("1.1h");
        let remembered = FingerprintBoundControllerOverride {
            selection: ExplicitControllerSelection::GenericGrblCompatible,
            detected_identity: identity_binding(&detected).unwrap(),
            transport_kind: TransportKind::Serial,
            fingerprint: fingerprint("device-a", DeviceFingerprintStrength::Strong),
        };
        let mut request = input(ControllerSelection::GenericGrblCompatible);
        request.detected_identity = Some(detected);
        request.remembered_override = Some(remembered);
        request.fingerprint = Some(fingerprint("device-a", DeviceFingerprintStrength::Weak));
        assert_eq!(
            invalidation_reason(&resolve_controller_choice(&request)),
            Some(ControllerOverrideInvalidationReason::FingerprintTooWeak)
        );
        request.fingerprint = None;
        assert_eq!(
            invalidation_reason(&resolve_controller_choice(&request)),
            Some(ControllerOverrideInvalidationReason::FingerprintUnavailable)
        );
    }

    #[test]
    fn unavailable_detected_driver_cannot_be_selected_but_experimental_continue_remains_a_choice() {
        let mut request = input(known_grbl());
        request.detected_identity = Some(unavailable_dsp_identity());
        assert!(matches!(
            resolve_controller_choice(&request).outcome,
            ControllerChoiceOutcome::MismatchDecisionRequired {
                detected_driver: None,
                ref allowed_decisions,
                ..
            } if allowed_decisions == &vec![
                ControllerMismatchDecision::ContinueSelectedExperimentally,
                ControllerMismatchDecision::Cancel,
            ]
        ));
        request.decision = Some(ControllerMismatchDecision::UseDetected);
        assert!(matches!(
            resolve_controller_choice(&request).outcome,
            ControllerChoiceOutcome::Blocked {
                reason: ControllerChoiceBlockReason::DetectedDriverUnavailable,
                ..
            }
        ));
    }

    #[test]
    fn explicit_grbl_can_switch_to_detected_grblhal_adapter() {
        let identity = grblhal_identity();
        let mut request = input(known_grbl());
        request.detected_identity = Some(identity.clone());

        let prompt = resolve_controller_choice(&request);
        assert!(matches!(
            prompt.outcome,
            ControllerChoiceOutcome::MismatchDecisionRequired {
                detected_driver: Some(ControllerDriverId::GrblHal),
                ref allowed_decisions,
                ..
            } if allowed_decisions == &vec![
                ControllerMismatchDecision::UseDetected,
                ControllerMismatchDecision::ContinueSelectedExperimentally,
                ControllerMismatchDecision::Cancel,
            ]
        ));

        request.decision = Some(ControllerMismatchDecision::UseDetected);
        let (choice, _) = expect_resolved(resolve_controller_choice(&request));
        assert_eq!(choice.driver, ControllerDriverId::GrblHal);
        assert_eq!(choice.source, ControllerChoiceSource::DetectedDriverChoice);
        assert!(choice.mismatch);
        assert!(choice.requires_experimental_mode);
        assert!(!choice.requires_experimental_compatibility_handshake);
        assert_eq!(choice.detected_identity, Some(identity));
    }

    #[test]
    fn explicit_grbl_can_switch_to_detected_fluidnc_adapter() {
        let mut request = input(known_grbl());
        request.detected_identity = Some(fluidnc_identity());

        assert!(matches!(
            resolve_controller_choice(&request).outcome,
            ControllerChoiceOutcome::MismatchDecisionRequired {
                detected_driver: Some(ControllerDriverId::FluidNc),
                ref allowed_decisions,
                ..
            } if allowed_decisions == &vec![
                ControllerMismatchDecision::UseDetected,
                ControllerMismatchDecision::ContinueSelectedExperimentally,
                ControllerMismatchDecision::Cancel,
            ]
        ));

        request.decision = Some(ControllerMismatchDecision::UseDetected);
        let (choice, _) = expect_resolved(resolve_controller_choice(&request));
        assert_eq!(choice.driver, ControllerDriverId::FluidNc);
        assert_eq!(choice.source, ControllerChoiceSource::DetectedDriverChoice);
        assert!(choice.requires_experimental_mode);
        assert!(!choice.requires_experimental_compatibility_handshake);
    }

    #[test]
    fn protocol_compatibility_validates_only_an_explicit_grbl_choice() {
        let mut explicit = input(known_grbl());
        explicit.protocol_compatible = true;
        let (choice, _) = expect_resolved(resolve_controller_choice(&explicit));
        assert_eq!(choice.driver, ControllerDriverId::Grbl);
        assert_eq!(choice.source, ControllerChoiceSource::KnownDriverSelection);
        assert!(!choice.requires_experimental_mode);

        let mut auto = input(ControllerSelection::AutoDetect);
        auto.protocol_compatible = true;
        assert_eq!(
            resolve_controller_choice(&auto).outcome,
            ControllerChoiceOutcome::SelectionRequired
        );

        let mut generic = input(ControllerSelection::GenericGrblCompatible);
        generic.protocol_compatible = true;
        let (choice, _) = expect_resolved(resolve_controller_choice(&generic));
        assert_eq!(choice.driver, ControllerDriverId::Grbl);
        assert!(choice.requires_experimental_mode);
        assert!(choice.requires_experimental_compatibility_handshake);
    }

    #[test]
    fn decision_without_matching_prompt_is_rejected() {
        let mut auto = input(ControllerSelection::AutoDetect);
        auto.decision = Some(ControllerMismatchDecision::UseDetected);
        assert!(matches!(
            resolve_controller_choice(&auto).outcome,
            ControllerChoiceOutcome::Blocked {
                reason: ControllerChoiceBlockReason::InvalidDecision,
                ..
            }
        ));

        let mut explicit = input(known_grbl());
        explicit.decision = Some(ControllerMismatchDecision::UseDetected);
        assert!(matches!(
            resolve_controller_choice(&explicit).outcome,
            ControllerChoiceOutcome::Blocked {
                reason: ControllerChoiceBlockReason::InvalidDecision,
                ..
            }
        ));

        let mut cancel_after_transport_change = input(known_grbl());
        cancel_after_transport_change.transport_kind = TransportKind::Tcp;
        cancel_after_transport_change.decision = Some(ControllerMismatchDecision::Cancel);
        let cancelled = resolve_controller_choice(&cancel_after_transport_change);
        assert_eq!(cancelled.outcome, ControllerChoiceOutcome::Cancelled);
        assert_eq!(cancelled.override_update, ControllerOverrideUpdate::Keep);
    }
}
