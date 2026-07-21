use beambench_common::machine::{
    ControllerEvidenceState, ControllerFamily, ControllerModel, ControllerProductTier,
    DeviceCapabilities, DeviceIdentity, DiscoveryCandidate, DiscoveryScanState, DiscoveryTcpTarget,
    DiscoveryUsbTarget, PortInfo, TransportKind, new_discovery_id,
};
use chrono::Utc;

#[derive(Debug, Clone, Default)]
pub struct DiscoveryRequest {
    pub tcp_targets: Vec<DiscoveryTcpTarget>,
    pub usb_targets: Vec<DiscoveryUsbTarget>,
}

const UNVERIFIED_SERIAL_REASON: &str = "Beam Bench can't identify a controller from serial-port metadata alone. Connect using this port and choose Auto-detect or a controller type on the Connection tab.";
const UNVERIFIED_TCP_REASON: &str = "Beam Bench can't identify a controller from a TCP address or port alone. Live TCP controller selection isn't available yet.";
const UNVERIFIED_USB_REASON: &str = "Beam Bench can't identify a controller from passive USB metadata alone. Live USB controller selection isn't available yet.";

fn capabilities_for_model(model: ControllerModel) -> DeviceCapabilities {
    match model {
        ControllerModel::Grbl => DeviceCapabilities::legacy_grbl(),
        ControllerModel::Unknown
        | ControllerModel::FluidNc
        | ControllerModel::GrblHal
        | ControllerModel::LaserPecker
        | ControllerModel::Marlin
        | ControllerModel::Snapmaker
        | ControllerModel::Smoothieware
        | ControllerModel::Ruida
        | ControllerModel::LihuiyuM2Nano
        | ControllerModel::Trocen
        | ControllerModel::Topwisdom
        | ControllerModel::Ezcad2
        | ControllerModel::Ezcad2Lite
        | ControllerModel::Bsl => DeviceCapabilities::default(),
    }
}

fn compatibility_for_model(
    model: ControllerModel,
) -> (
    Option<ControllerProductTier>,
    Option<ControllerEvidenceState>,
) {
    match model {
        // Positively identified legacy GRBL remains unnormalized until exact-row evidence exists.
        ControllerModel::Grbl => (None, None),
        ControllerModel::Unknown
        | ControllerModel::FluidNc
        | ControllerModel::GrblHal
        | ControllerModel::LaserPecker
        | ControllerModel::Marlin
        | ControllerModel::Snapmaker
        | ControllerModel::Smoothieware
        | ControllerModel::Ruida
        | ControllerModel::LihuiyuM2Nano
        | ControllerModel::Trocen
        | ControllerModel::Topwisdom
        | ControllerModel::Ezcad2
        | ControllerModel::Ezcad2Lite
        | ControllerModel::Bsl => (
            Some(ControllerProductTier::Unavailable),
            Some(ControllerEvidenceState::Emulated),
        ),
    }
}

pub fn candidate_from_serial(port: &PortInfo) -> DiscoveryCandidate {
    let family = ControllerFamily::Unknown;
    let model = ControllerModel::Unknown;
    let (product_tier, evidence_state) = compatibility_for_model(model);
    DiscoveryCandidate {
        id: new_discovery_id(),
        controller_family: family,
        controller_model: model,
        transport_kind: TransportKind::Serial,
        identity: DeviceIdentity {
            display_name: if port.description.is_empty() {
                port.port_name.clone()
            } else {
                port.description.clone()
            },
            manufacturer: if port.manufacturer.is_empty() {
                None
            } else {
                Some(port.manufacturer.clone())
            },
            description: if port.description.is_empty() {
                None
            } else {
                Some(port.description.clone())
            },
            product: None,
            serial_number: None,
            vendor_id: port.vid,
            product_id: port.pid,
            port_name: Some(port.port_name.clone()),
            host: None,
            tcp_port: None,
            udp_port: None,
            usb_path: None,
        },
        confidence: 0.0,
        capabilities: capabilities_for_model(model),
        product_tier,
        evidence_state,
        status_text: format!(
            "Detected serial port {} (controller unverified)",
            port.port_name
        ),
        unsupported_reason: Some(UNVERIFIED_SERIAL_REASON.to_string()),
    }
}

pub fn candidate_from_tcp(target: &DiscoveryTcpTarget) -> DiscoveryCandidate {
    let family = ControllerFamily::Unknown;
    let model = ControllerModel::Unknown;
    let (product_tier, evidence_state) = compatibility_for_model(model);
    DiscoveryCandidate {
        id: new_discovery_id(),
        controller_family: family,
        controller_model: model,
        transport_kind: TransportKind::Tcp,
        identity: DeviceIdentity {
            display_name: target
                .label
                .clone()
                .unwrap_or_else(|| format!("{}:{}", target.host, target.port)),
            manufacturer: None,
            description: None,
            product: None,
            serial_number: None,
            vendor_id: None,
            product_id: None,
            port_name: None,
            host: Some(target.host.clone()),
            tcp_port: Some(target.port),
            udp_port: None,
            usb_path: None,
        },
        confidence: 0.0,
        capabilities: capabilities_for_model(model),
        product_tier,
        evidence_state,
        status_text: format!(
            "Added unverified TCP target {}:{}",
            target.host, target.port
        ),
        unsupported_reason: Some(UNVERIFIED_TCP_REASON.to_string()),
    }
}

pub fn candidate_from_usb(target: &DiscoveryUsbTarget) -> DiscoveryCandidate {
    let family = ControllerFamily::Unknown;
    let model = ControllerModel::Unknown;
    let (product_tier, evidence_state) = compatibility_for_model(model);
    DiscoveryCandidate {
        id: new_discovery_id(),
        controller_family: family,
        controller_model: model,
        transport_kind: TransportKind::UsbPacket,
        identity: DeviceIdentity {
            display_name: target
                .product
                .clone()
                .unwrap_or_else(|| target.device_path.clone()),
            manufacturer: target.manufacturer.clone(),
            description: None,
            product: target.product.clone(),
            serial_number: None,
            vendor_id: None,
            product_id: None,
            port_name: None,
            host: None,
            tcp_port: None,
            udp_port: None,
            usb_path: Some(target.device_path.clone()),
        },
        confidence: 0.0,
        capabilities: capabilities_for_model(model),
        product_tier,
        evidence_state,
        status_text: format!(
            "Added unverified USB target {}",
            target
                .product
                .clone()
                .unwrap_or_else(|| target.device_path.clone())
        ),
        unsupported_reason: Some(UNVERIFIED_USB_REASON.to_string()),
    }
}

pub fn scan_candidates(
    serial_ports: &[PortInfo],
    request: &DiscoveryRequest,
) -> Vec<DiscoveryCandidate> {
    let mut candidates = Vec::new();
    candidates.extend(serial_ports.iter().map(candidate_from_serial));
    candidates.extend(request.tcp_targets.iter().map(candidate_from_tcp));
    candidates.extend(request.usb_targets.iter().map(candidate_from_usb));
    candidates
}

pub fn completed_scan_state(
    serial_ports: &[PortInfo],
    request: &DiscoveryRequest,
    started_at: Option<String>,
) -> DiscoveryScanState {
    let candidates = scan_candidates(serial_ports, request);
    let status_text = if candidates.is_empty() {
        "No connection candidates available".to_string()
    } else {
        format!("Listed {} connection candidate(s)", candidates.len())
    };
    let completed_at = Utc::now().to_rfc3339();
    DiscoveryScanState {
        phase: beambench_common::machine::DiscoveryPhase::Completed,
        status_text,
        candidates,
        scanned_serial_count: serial_ports.len(),
        scanned_tcp_count: request.tcp_targets.len(),
        scanned_usb_count: request.usb_targets.len(),
        started_at: started_at.or_else(|| Some(completed_at.clone())),
        completed_at: Some(completed_at),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passive_scan_does_not_infer_identity_from_names_or_ports() {
        let serial = vec![PortInfo {
            port_name: "/dev/tty.usbmodem1".to_string(),
            description: "GRBL Controller".to_string(),
            manufacturer: "OpenBuilds".to_string(),
            vid: Some(0x1a86),
            pid: Some(0x7523),
        }];
        let request = DiscoveryRequest {
            tcp_targets: vec![DiscoveryTcpTarget {
                host: "192.168.0.50".to_string(),
                port: 50200,
                label: Some("Ruida".to_string()),
            }],
            usb_targets: vec![DiscoveryUsbTarget {
                device_path: "/dev/usb/galvo0".to_string(),
                manufacturer: Some("BJJCZ".to_string()),
                product: Some("LMC2-Lite".to_string()),
            }],
        };

        let state = completed_scan_state(&serial, &request, None);
        assert_eq!(state.candidates.len(), 3);
        assert!(state.candidates.iter().all(|candidate| {
            candidate.controller_family == ControllerFamily::Unknown
                && candidate.controller_model == ControllerModel::Unknown
                && candidate.confidence == 0.0
                && candidate.capabilities == DeviceCapabilities::default()
                && candidate.product_tier == Some(ControllerProductTier::Unavailable)
                && candidate.evidence_state == Some(ControllerEvidenceState::Emulated)
                && candidate.unsupported_reason.is_some()
        }));
        assert_eq!(state.candidates[0].transport_kind, TransportKind::Serial);
        assert_eq!(state.candidates[1].transport_kind, TransportKind::Tcp);
        assert_eq!(state.candidates[2].transport_kind, TransportKind::UsbPacket);
        assert_eq!(
            state.candidates[0].unsupported_reason.as_deref(),
            Some(UNVERIFIED_SERIAL_REASON)
        );
        assert_eq!(
            state.candidates[1].unsupported_reason.as_deref(),
            Some(UNVERIFIED_TCP_REASON)
        );
        assert_eq!(
            state.candidates[2].unsupported_reason.as_deref(),
            Some(UNVERIFIED_USB_REASON)
        );
        assert_eq!(state.candidates[0].identity.display_name, "GRBL Controller");
        assert_eq!(
            state.candidates[0].identity.port_name.as_deref(),
            Some("/dev/tty.usbmodem1")
        );
        assert_eq!(state.candidates[0].identity.vendor_id, Some(0x1a86));
        assert_eq!(state.candidates[0].identity.product_id, Some(0x7523));
        assert_eq!(
            state.candidates[1].identity.host.as_deref(),
            Some("192.168.0.50")
        );
        assert_eq!(state.candidates[1].identity.tcp_port, Some(50200));
        assert_eq!(
            state.candidates[2].identity.usb_path.as_deref(),
            Some("/dev/usb/galvo0")
        );
        assert_eq!(
            state.candidates[2].identity.manufacturer.as_deref(),
            Some("BJJCZ")
        );
        assert_eq!(
            state.candidates[2].identity.product.as_deref(),
            Some("LMC2-Lite")
        );
        assert_eq!(state.status_text, "Listed 3 connection candidate(s)");
    }

    #[test]
    fn completed_state_preserves_original_started_at() {
        let request = DiscoveryRequest::default();
        let started_at = "2026-04-15T12:00:00Z".to_string();

        let state = completed_scan_state(&[], &request, Some(started_at.clone()));

        assert_eq!(state.started_at, Some(started_at));
        assert!(state.completed_at.is_some());
        assert_eq!(state.status_text, "No connection candidates available");
    }

    #[test]
    fn compatibility_metadata_distinguishes_legacy_grbl_from_placeholders() {
        assert_eq!(compatibility_for_model(ControllerModel::Grbl), (None, None));
        for model in [
            ControllerModel::FluidNc,
            ControllerModel::GrblHal,
            ControllerModel::Marlin,
            ControllerModel::Snapmaker,
            ControllerModel::Ruida,
        ] {
            assert_eq!(
                compatibility_for_model(model),
                (
                    Some(ControllerProductTier::Unavailable),
                    Some(ControllerEvidenceState::Emulated)
                ),
                "{model:?} must remain unavailable and emulated until its adapter lands"
            );
        }
    }

    #[test]
    fn unknown_controller_capabilities_are_disabled() {
        assert_eq!(
            capabilities_for_model(ControllerModel::Unknown),
            DeviceCapabilities::default()
        );
    }

    #[test]
    fn grbl_capabilities_are_explicitly_enabled() {
        assert_eq!(
            capabilities_for_model(ControllerModel::Grbl),
            DeviceCapabilities::legacy_grbl()
        );
    }

    #[test]
    fn unavailable_placeholder_capabilities_are_disabled() {
        for model in [
            ControllerModel::FluidNc,
            ControllerModel::GrblHal,
            ControllerModel::Marlin,
            ControllerModel::Snapmaker,
            ControllerModel::Ruida,
            ControllerModel::Trocen,
            ControllerModel::Topwisdom,
            ControllerModel::Ezcad2,
            ControllerModel::Ezcad2Lite,
            ControllerModel::Bsl,
        ] {
            assert_eq!(
                capabilities_for_model(model),
                DeviceCapabilities::default(),
                "{model:?} must not advertise unfinished capabilities"
            );
        }
    }
}
