use beambench_common::machine::{
    ControllerModel, DiscoveryCandidate, DiscoveryPhase, DiscoveryScanState, DiscoveryTcpTarget,
    DiscoveryUsbTarget, TransportKind,
};
use beambench_core::MachineProfile;
use beambench_discovery::{DiscoveryRequest, completed_scan_state};
use beambench_serial::list_available_ports;
use serde_json::json;

use crate::context::ServiceContext;
use crate::error::{ServiceError, ServiceResult};
use crate::ops::profiles;

#[derive(Debug, Clone, Default)]
pub struct StartDiscoveryInput {
    pub tcp_targets: Vec<DiscoveryTcpTarget>,
    pub usb_targets: Vec<DiscoveryUsbTarget>,
}

#[derive(Debug, Clone)]
pub struct BootstrapProfileInput {
    pub candidate_id: String,
    pub profile_name: Option<String>,
    pub activate: bool,
}

fn lock_err(name: &str, e: impl std::fmt::Display) -> ServiceError {
    ServiceError::internal(format!("Failed to lock {name}: {e}"))
}

pub fn get_discovery_state(ctx: &ServiceContext) -> ServiceResult<DiscoveryScanState> {
    let guard = ctx
        .discovery_state
        .lock()
        .map_err(|e| lock_err("discovery_state", e))?;
    Ok(guard.clone())
}

pub fn cancel_discovery(ctx: &ServiceContext) -> ServiceResult<DiscoveryScanState> {
    let state = {
        let mut guard = ctx
            .discovery_state
            .lock()
            .map_err(|e| lock_err("discovery_state", e))?;
        guard.phase = DiscoveryPhase::Cancelled;
        guard.status_text = "Waiting for connection...".to_string();
        guard.completed_at = Some(crate::events::timestamp());
        guard.clone()
    };
    ctx.emit_event(
        "machine.discovery.cancelled",
        json!({
            "phase": state.phase,
            "status_text": state.status_text,
        }),
    );
    Ok(state)
}

pub fn start_discovery(
    ctx: &ServiceContext,
    input: StartDiscoveryInput,
) -> ServiceResult<DiscoveryScanState> {
    let started_at = crate::events::timestamp();
    {
        let mut guard = ctx
            .discovery_state
            .lock()
            .map_err(|e| lock_err("discovery_state", e))?;
        *guard = DiscoveryScanState {
            phase: DiscoveryPhase::Scanning,
            status_text: "Waiting for connection...".to_string(),
            candidates: Vec::new(),
            scanned_serial_count: 0,
            scanned_tcp_count: input.tcp_targets.len(),
            scanned_usb_count: input.usb_targets.len(),
            started_at: Some(started_at.clone()),
            completed_at: None,
        };
    }
    ctx.emit_event(
        "machine.discovery.started",
        json!({
            "status_text": "Waiting for connection...",
        }),
    );

    let ports = match list_available_ports() {
        Ok(ports) => {
            ctx.push_connection_event(
                "port_scan",
                None,
                None,
                Some(format!("Discovery detected {} serial ports", ports.len())),
                None,
            );
            ports
        }
        Err(error) => {
            let message = error.to_string();
            ctx.push_connection_event("port_scan_failed", None, None, None, Some(message.clone()));
            return Err(ServiceError::machine(message));
        }
    };
    ctx.emit_event(
        "machine.discovery.progress",
        json!({
            "serial_ports": ports.len(),
            "tcp_targets": input.tcp_targets.len(),
            "usb_targets": input.usb_targets.len(),
        }),
    );

    let request = DiscoveryRequest {
        tcp_targets: input.tcp_targets,
        usb_targets: input.usb_targets,
    };
    let state = completed_scan_state(&ports, &request, Some(started_at));

    for candidate in &state.candidates {
        ctx.emit_event(
            "machine.discovery.candidate_found",
            json!({
                "candidate": candidate,
            }),
        );
    }

    {
        let mut guard = ctx
            .discovery_state
            .lock()
            .map_err(|e| lock_err("discovery_state", e))?;
        *guard = state.clone();
    }
    ctx.emit_event(
        "machine.discovery.completed",
        json!({
            "candidate_count": state.candidates.len(),
            "status_text": state.status_text,
        }),
    );
    Ok(state)
}

pub fn find_candidate(
    ctx: &ServiceContext,
    candidate_id: &str,
) -> ServiceResult<DiscoveryCandidate> {
    let guard = ctx
        .discovery_state
        .lock()
        .map_err(|e| lock_err("discovery_state", e))?;
    guard
        .candidates
        .iter()
        .find(|c| c.id == candidate_id)
        .cloned()
        .ok_or_else(|| ServiceError::not_found("Discovery candidate not found"))
}

fn firmware_type_for_model(model: ControllerModel) -> String {
    match model {
        ControllerModel::Unknown => "unknown",
        ControllerModel::Grbl => "grbl",
        ControllerModel::FluidNc => "fluid_nc",
        ControllerModel::GrblHal => "grbl_hal",
        ControllerModel::LaserPecker => "laser_pecker",
        ControllerModel::Marlin => "marlin",
        ControllerModel::Snapmaker => "snapmaker",
        ControllerModel::Smoothieware => "smoothieware",
        ControllerModel::Ruida => "ruida",
        ControllerModel::LihuiyuM2Nano => "lihuiyu_m2_nano",
        ControllerModel::Trocen => "trocen",
        ControllerModel::Topwisdom => "topwisdom",
        ControllerModel::Ezcad2 => "ezcad2",
        ControllerModel::Ezcad2Lite => "ezcad2_lite",
        ControllerModel::Bsl => "bsl",
    }
    .to_string()
}

pub fn bootstrap_profile(
    ctx: &ServiceContext,
    input: BootstrapProfileInput,
) -> ServiceResult<MachineProfile> {
    let candidate = find_candidate(ctx, &input.candidate_id)?;
    let profile_name = input
        .profile_name
        .unwrap_or_else(|| candidate.identity.display_name.clone());
    let profile_notes = if candidate.controller_model == ControllerModel::Unknown {
        let transport_label = match candidate.transport_kind {
            TransportKind::Serial => "serial",
            TransportKind::Tcp => "tcp",
            TransportKind::Udp => "udp",
            TransportKind::UsbPacket => "USB",
        };
        format!(
            "Draft profile from unverified {} target '{}'. Review all machine settings before use.",
            transport_label, candidate.identity.display_name
        )
    } else {
        format!(
            "Bootstrapped from {} via {:?}",
            candidate.identity.display_name, candidate.transport_kind
        )
    };
    let profile = profiles::save_profile(
        ctx,
        profiles::SaveProfileInput {
            profile_id: None,
            name: profile_name,
            preset_id: None,
            preset_version: None,
            bed_width_mm: if candidate.controller_family
                == beambench_common::machine::ControllerFamily::Galvo
            {
                110.0
            } else {
                400.0
            },
            bed_height_mm: if candidate.controller_family
                == beambench_common::machine::ControllerFamily::Galvo
            {
                110.0
            } else {
                300.0
            },
            max_speed_mm_min: match candidate.controller_model {
                ControllerModel::Ezcad2 | ControllerModel::Ezcad2Lite | ControllerModel::Bsl => {
                    12000.0
                }
                _ => 6000.0,
            },
            max_power_percent: 100.0,
            s_value_max: 1000,
            homing_enabled: candidate.capabilities.can_home,
            default_baud_rate: 115200,
            firmware_type: firmware_type_for_model(candidate.controller_model),
            notes: profile_notes,
            selected_camera_id: None,
            camera_calibration: None,
            camera_alignment: None,
            origin: beambench_core::WorkspaceOrigin::default(),
            laser_offset_x: 0.0,
            laser_offset_y: 0.0,
            enable_laser_offset: false,
            swap_xy: false,
            job_checklist: false,
            frame_continuously: false,
            laser_on_when_framing: false,
            tab_pulse_width_ms: 0.0,
            cnc_machine: false,
            use_constant_power: false,
            emit_s_every_g1: false,
            use_g0_for_overscan: true,
            air_assist_on_gcode: "M7".to_string(),
            air_assist_off_gcode: "M9".to_string(),
            air_assist_on_delay_ms: 0,
            job_header_gcode: String::new(),
            job_footer_gcode: String::new(),
            transfer_mode: beambench_core::TransferMode::Buffered,
            preferred_default_origin: None,
            scanning_offsets: Vec::new(),
            enable_scanning_offset: false,
            dot_width_mm: 0.0,
            enable_dot_width: false,
            supports_z_moves: false,
            z_move_feed_mm_min: 300.0,
            ruida_table_axis: beambench_core::RuidaTableAxis::Disabled,
            enable_laser_fire_button: false,
            default_fire_power_percent: 1.0,
            quality_test_settings: Default::default(),
        },
    )?;
    if input.activate {
        profiles::set_active_profile(ctx, Some(profile.id))?;
    }
    ctx.emit_event(
        "profile.bootstrap.completed",
        json!({
            "candidate_id": input.candidate_id,
            "profile_id": profile.id,
            "activated": input.activate,
        }),
    );
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::machine::{ControllerFamily, MachineConnectionTarget, PortInfo};

    #[test]
    fn grbl_family_firmware_type_names_are_stable() {
        assert_eq!(firmware_type_for_model(ControllerModel::Grbl), "grbl");
        assert_eq!(
            firmware_type_for_model(ControllerModel::FluidNc),
            "fluid_nc"
        );
        assert_eq!(
            firmware_type_for_model(ControllerModel::GrblHal),
            "grbl_hal"
        );
        assert_eq!(firmware_type_for_model(ControllerModel::Marlin), "marlin");
        assert_eq!(
            firmware_type_for_model(ControllerModel::Snapmaker),
            "snapmaker"
        );
        assert_eq!(
            firmware_type_for_model(ControllerModel::Smoothieware),
            "smoothieware"
        );
    }

    #[test]
    fn bootstrap_profile_from_discovery_candidate() {
        let ctx = ServiceContext::new();
        let state = start_discovery(
            &ctx,
            StartDiscoveryInput {
                tcp_targets: vec![DiscoveryTcpTarget {
                    host: "192.168.0.20".to_string(),
                    port: 50200,
                    label: Some("Ruida".to_string()),
                }],
                usb_targets: vec![],
            },
        )
        .unwrap();
        let candidate = state
            .candidates
            .iter()
            .find(|candidate| candidate.identity.host.as_deref() == Some("192.168.0.20"))
            .cloned()
            .expect("expected TCP candidate");
        assert_eq!(candidate.controller_family, ControllerFamily::Unknown);
        assert_eq!(candidate.controller_model, ControllerModel::Unknown);
        assert_eq!(
            candidate.product_tier,
            Some(beambench_common::machine::ControllerProductTier::Unavailable)
        );
        assert!(candidate.unsupported_reason.is_some());
        let profile = bootstrap_profile(
            &ctx,
            BootstrapProfileInput {
                candidate_id: candidate.id,
                profile_name: None,
                activate: true,
            },
        )
        .unwrap();
        assert_eq!(profile.firmware_type, "unknown");
        assert!(!profile.homing_enabled);
        assert!(
            profile
                .notes
                .contains("Draft profile from unverified tcp target")
        );
        assert_eq!(
            profiles::get_active_profile_id(&ctx).unwrap(),
            Some(profile.id)
        );
    }

    #[test]
    fn generated_passive_candidate_is_rejected_before_session_creation() {
        let candidate = beambench_discovery::candidate_from_serial(&PortInfo {
            port_name: "/dev/tty.test".to_string(),
            description: "GRBL Controller".to_string(),
            manufacturer: "Test Vendor".to_string(),
            vid: Some(0x1234),
            pid: Some(0x5678),
        });
        let candidate_id = candidate.id.clone();
        let expected_reason = candidate
            .unsupported_reason
            .clone()
            .expect("passive candidate must explain why it is unavailable");
        let ctx = ServiceContext::new();
        ctx.discovery_state.lock().unwrap().candidates = vec![candidate];
        let mut events = ctx.events.subscribe();

        let error = crate::ops::machine::connect_machine(
            &ctx,
            crate::ops::machine::ConnectMachineInput {
                target: MachineConnectionTarget::DiscoveryCandidate { candidate_id },
            },
        )
        .unwrap_err();

        assert_eq!(error.code, crate::error::ServiceErrorCode::InvalidState);
        assert!(error.message.contains(&expected_reason));
        assert!(ctx.session.lock().unwrap().is_none());
        assert!(ctx.job.lock().unwrap().is_none());
        while let Ok(raw) = events.try_recv() {
            let event: serde_json::Value = serde_json::from_str(&raw).unwrap();
            assert_ne!(event["type"].as_str(), Some("machine.connected"));
        }
    }
}
