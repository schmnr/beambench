//! Product and capability metadata for the Smoothieware serial adapter.

use beambench_common::{
    ControllerDriverId, ControllerEvidenceState, ControllerModel, ControllerProductTier,
    DeviceCapabilities, TransportKind,
};

/// Stable metadata for Smoothieware laser firmware.
#[derive(Debug, Clone, PartialEq)]
pub struct SmoothiewareAdapterDescriptor {
    pub driver: ControllerDriverId,
    pub controller_model: ControllerModel,
    pub product_tier: ControllerProductTier,
    pub evidence_state: ControllerEvidenceState,
    pub transport_kind: TransportKind,
    pub capabilities: DeviceCapabilities,
}

/// Exact Smoothieware serial adapter identity and maturity contract.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SmoothiewareSerialAdapter;

impl SmoothiewareSerialAdapter {
    pub const fn new() -> Self {
        Self
    }

    pub fn descriptor(self) -> SmoothiewareAdapterDescriptor {
        SmoothiewareAdapterDescriptor {
            driver: ControllerDriverId::Smoothieware,
            controller_model: ControllerModel::Smoothieware,
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
        let descriptor = SmoothiewareSerialAdapter::new().descriptor();
        assert_eq!(descriptor.driver, ControllerDriverId::Smoothieware);
        assert_eq!(descriptor.controller_model, ControllerModel::Smoothieware);
        assert_eq!(descriptor.product_tier, ControllerProductTier::Experimental);
        assert_eq!(descriptor.evidence_state, ControllerEvidenceState::Emulated);
        assert_eq!(descriptor.transport_kind, TransportKind::Serial);
        assert!(descriptor.capabilities.can_run_job);
        assert!(descriptor.capabilities.can_frame);
        assert!(!descriptor.capabilities.can_pause_resume);
        assert!(!descriptor.capabilities.can_jog);
    }
}
