//! Product and capability metadata for the exact Ruida Ethernet target.

use beambench_common::{
    ControllerDriverId, ControllerEvidenceState, ControllerModel, ControllerProductTier,
    DeviceCapabilities, TransportKind,
};

#[derive(Debug, Clone, PartialEq)]
pub struct RuidaAdapterDescriptor {
    pub driver: ControllerDriverId,
    pub controller_model: ControllerModel,
    pub product_tier: ControllerProductTier,
    pub evidence_state: ControllerEvidenceState,
    pub transport_kind: TransportKind,
    pub capabilities: DeviceCapabilities,
}

/// Exact RDC6442S Ethernet/UDP adapter contract.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RuidaEthernetAdapter;

impl RuidaEthernetAdapter {
    pub const fn new() -> Self {
        Self
    }

    pub fn descriptor(self) -> RuidaAdapterDescriptor {
        RuidaAdapterDescriptor {
            driver: ControllerDriverId::Ruida,
            controller_model: ControllerModel::Ruida,
            product_tier: ControllerProductTier::Experimental,
            evidence_state: ControllerEvidenceState::Emulated,
            transport_kind: TransportKind::Udp,
            capabilities: DeviceCapabilities {
                can_home: true,
                can_jog: true,
                can_jog_continuous: false,
                can_unlock: false,
                can_pause_resume: true,
                can_set_origin: false,
                can_frame: true,
                can_run_job: true,
                reports_absolute_position: false,
                can_manual_fire: false,
                can_adjust_overrides: false,
                supports_rotary: false,
                supports_cylinder: false,
                supports_camera_alignment: false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_exposes_only_implemented_actions() {
        let descriptor = RuidaEthernetAdapter::new().descriptor();
        assert_eq!(descriptor.driver, ControllerDriverId::Ruida);
        assert_eq!(descriptor.controller_model, ControllerModel::Ruida);
        assert_eq!(descriptor.product_tier, ControllerProductTier::Experimental);
        assert_eq!(descriptor.evidence_state, ControllerEvidenceState::Emulated);
        assert_eq!(descriptor.transport_kind, TransportKind::Udp);
        assert!(descriptor.capabilities.can_run_job);
        assert!(descriptor.capabilities.can_frame);
        assert!(descriptor.capabilities.can_pause_resume);
        assert!(descriptor.capabilities.can_home);
        assert!(descriptor.capabilities.can_jog);
        assert!(!descriptor.capabilities.can_set_origin);
        assert!(!descriptor.capabilities.can_unlock);
    }
}
