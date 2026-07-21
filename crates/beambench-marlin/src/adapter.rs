//! Product and capability metadata for the standard-Marlin serial adapter.

use beambench_common::{
    ControllerDriverId, ControllerEvidenceState, ControllerModel, ControllerProductTier,
    DeviceCapabilities, TransportKind,
};

/// Stable metadata for standard Marlin laser firmware.
#[derive(Debug, Clone, PartialEq)]
pub struct MarlinAdapterDescriptor {
    pub driver: ControllerDriverId,
    pub controller_model: ControllerModel,
    pub product_tier: ControllerProductTier,
    pub evidence_state: ControllerEvidenceState,
    pub transport_kind: TransportKind,
    pub capabilities: DeviceCapabilities,
}

/// Standard-Marlin serial adapter identity and maturity contract.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MarlinSerialAdapter;

impl MarlinSerialAdapter {
    pub const fn new() -> Self {
        Self
    }

    pub fn descriptor(self) -> MarlinAdapterDescriptor {
        MarlinAdapterDescriptor {
            driver: ControllerDriverId::Marlin,
            controller_model: ControllerModel::Marlin,
            product_tier: ControllerProductTier::Experimental,
            evidence_state: ControllerEvidenceState::Emulated,
            transport_kind: TransportKind::Serial,
            capabilities: DeviceCapabilities::experimental_acknowledged_gcode(),
        }
    }
}

/// Exact Snapmaker 2.0 serial adapter identity and maturity contract.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SnapmakerSerialAdapter;

impl SnapmakerSerialAdapter {
    pub const fn new() -> Self {
        Self
    }

    pub fn descriptor(self) -> MarlinAdapterDescriptor {
        MarlinAdapterDescriptor {
            driver: ControllerDriverId::Snapmaker,
            controller_model: ControllerModel::Snapmaker,
            product_tier: ControllerProductTier::Experimental,
            evidence_state: ControllerEvidenceState::Emulated,
            transport_kind: TransportKind::Serial,
            capabilities: DeviceCapabilities::experimental_acknowledged_gcode(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_exposes_only_implemented_live_actions() {
        let descriptor = MarlinSerialAdapter::new().descriptor();

        assert_eq!(descriptor.driver, ControllerDriverId::Marlin);
        assert_eq!(descriptor.controller_model, ControllerModel::Marlin);
        assert_eq!(descriptor.product_tier, ControllerProductTier::Experimental);
        assert_eq!(descriptor.evidence_state, ControllerEvidenceState::Emulated);
        assert_eq!(descriptor.transport_kind, TransportKind::Serial);
        assert!(descriptor.capabilities.can_run_job);
        assert!(descriptor.capabilities.can_frame);
        assert!(!descriptor.capabilities.can_pause_resume);
        assert!(!descriptor.capabilities.can_jog);
    }

    #[test]
    fn snapmaker_descriptor_stays_distinct_from_standard_marlin() {
        let descriptor = SnapmakerSerialAdapter::new().descriptor();

        assert_eq!(descriptor.driver, ControllerDriverId::Snapmaker);
        assert_eq!(descriptor.controller_model, ControllerModel::Snapmaker);
        assert_eq!(descriptor.product_tier, ControllerProductTier::Experimental);
        assert_eq!(descriptor.evidence_state, ControllerEvidenceState::Emulated);
        assert!(descriptor.capabilities.can_run_job);
        assert!(!descriptor.capabilities.can_pause_resume);
    }
}
