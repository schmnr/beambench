//! Product and capability metadata for the Ruida Ethernet adapter.

use beambench_common::{
    ControllerDriverId, ControllerEvidenceState, ControllerModel, ControllerProductTier,
    DeviceCapabilities, TransportKind,
};

use crate::RuidaCompatibilityTarget;

#[derive(Debug, Clone, PartialEq)]
pub struct RuidaAdapterDescriptor {
    pub driver: ControllerDriverId,
    pub controller_model: ControllerModel,
    pub product_tier: ControllerProductTier,
    pub evidence_state: ControllerEvidenceState,
    pub transport_kind: TransportKind,
    pub capabilities: DeviceCapabilities,
}

/// Capability contract for a Ruida Ethernet target accepted by the read-only
/// compatibility probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuidaEthernetAdapter;

impl Default for RuidaEthernetAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RuidaEthernetAdapter {
    pub const fn new() -> Self {
        Self
    }

    pub const fn for_target(_target: RuidaCompatibilityTarget) -> Self {
        Self
    }

    pub fn descriptor(self) -> RuidaAdapterDescriptor {
        let capabilities = DeviceCapabilities {
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
    use crate::RDC6442S_ETHERNET_TARGET;

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
    fn experimental_target_uses_the_shared_ruida_capabilities() {
        let mut unknown = RDC6442S_ETHERNET_TARGET;
        unknown.card_id = 0x1234_5678;
        let descriptor = RuidaEthernetAdapter::for_target(unknown).descriptor();
        assert!(descriptor.capabilities.can_run_job);
        assert!(descriptor.capabilities.can_frame);
        assert!(descriptor.capabilities.can_home);
        assert!(descriptor.capabilities.can_jog);
    }
}
