//! Product and capability metadata for the exact Ruida Ethernet target.

use beambench_common::{
    ControllerDriverId, ControllerEvidenceState, ControllerModel, ControllerProductTier,
    DeviceCapabilities, TransportKind,
};

use crate::{RDC6442S_ETHERNET_TARGET, RuidaCompatibilityTarget};

#[derive(Debug, Clone, PartialEq)]
pub struct RuidaAdapterDescriptor {
    pub driver: ControllerDriverId,
    pub controller_model: ControllerModel,
    pub product_tier: ControllerProductTier,
    pub evidence_state: ControllerEvidenceState,
    pub transport_kind: TransportKind,
    pub capabilities: DeviceCapabilities,
}

/// Capability contract for one verified Ruida Ethernet target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuidaEthernetAdapter {
    target: RuidaCompatibilityTarget,
}

impl Default for RuidaEthernetAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RuidaEthernetAdapter {
    pub const fn new() -> Self {
        Self::for_target(RDC6442S_ETHERNET_TARGET)
    }

    pub const fn for_target(target: RuidaCompatibilityTarget) -> Self {
        Self { target }
    }

    pub fn descriptor(self) -> RuidaAdapterDescriptor {
        let capabilities = if self.target == RDC6442S_ETHERNET_TARGET {
            DeviceCapabilities {
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
            }
        } else {
            DeviceCapabilities::disabled()
        };
        RuidaAdapterDescriptor {
            driver: ControllerDriverId::Ruida,
            controller_model: ControllerModel::Ruida,
            product_tier: ControllerProductTier::Experimental,
            evidence_state: ControllerEvidenceState::Emulated,
            transport_kind: TransportKind::Udp,
            capabilities,
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

    #[test]
    fn unregistered_target_capabilities_fail_closed() {
        let mut unknown = RDC6442S_ETHERNET_TARGET;
        unknown.card_id = 0x1234_5678;
        let descriptor = RuidaEthernetAdapter::for_target(unknown).descriptor();
        assert_eq!(descriptor.capabilities, DeviceCapabilities::disabled());
    }
}
