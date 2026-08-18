use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use beambench_common::StartFromMode;
use beambench_common::controller_choice::ControllerConnectionEndpoint;
use beambench_common::feedback::{
    ConnectionDiagnosticsSnapshot, DiagnosticBundleV1, DiagnosticClient, DiagnosticConnectionEvent,
    DiagnosticLogEntry, DiagnosticMachine, DiagnosticPanic, DiagnosticPort, DiagnosticProjectLayer,
    DiagnosticProjectMetadata, DiagnosticProjectObject, DiagnosticSessionState, DiagnosticSystem,
    FEEDBACK_HISTORY_MAX_ENTRIES, FEEDBACK_SCHEMA_VERSION, FeedbackHistoryEntry, FeedbackKind,
    FeedbackReportInput, KnownIssueWarning, MAX_FEEDBACK_DESCRIPTION_CHARS,
    MAX_FEEDBACK_REPLY_TO_EMAIL_CHARS, MAX_FEEDBACK_TITLE_CHARS, MAX_PROJECT_ATTACHMENT_RAW_BYTES,
    MAX_SUBMIT_BODY_BYTES, SavedReport, SubmitFeedbackRequest, scrub_and_serialize,
    scrub_deserialized,
};
use beambench_common::geometry::Bounds;
use beambench_common::machine::{PortInfo, SessionState};
use beambench_core::object::ObjectData;
use beambench_core::{MachineProfile, Project};
use beambench_project::save_project_to_bytes;
use beambench_serial::{list_available_ports, recent_serial_traffic};
use chrono::{DateTime, Utc};
use serde_json::json;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

use crate::context::{ActiveErrorEntry, ServiceContext};
use crate::error::{ServiceError, ServiceResult};
use crate::persist;

const MAX_PANIC_REPORT_FILE_BYTES: u64 = 256 * 1024;
const MAX_DIAGNOSTIC_PROJECT_ITEMS: usize = 10_000;
const MAX_PANIC_REPORT_BUNDLE_BYTES: usize = 512 * 1024;

pub const KNOWN_ISSUES: &[KnownIssueDefinition] = &[
    KnownIssueDefinition {
        code: "grbl_9600_baud",
        model_contains: "grbl",
        firmware_contains: None,
        configured_baud: Some(9_600),
        severity: "warning",
        message: "GRBL controllers normally use 115200 baud. If this machine is not responding, switch the profile baud rate to 115200 and reconnect.",
    },
    KnownIssueDefinition {
        code: "no_grbl_response",
        model_contains: "grbl",
        firmware_contains: None,
        configured_baud: None,
        severity: "warning",
        message: "No GRBL response has been detected yet. This usually points to the wrong port, wrong baud rate, a missing USB driver, or another app already holding the port.",
    },
];

#[derive(Debug, Clone, Copy)]
pub struct KnownIssueDefinition {
    pub code: &'static str,
    pub model_contains: &'static str,
    pub firmware_contains: Option<&'static str>,
    pub configured_baud: Option<u32>,
    pub severity: &'static str,
    pub message: &'static str,
}

#[derive(Debug, Clone)]
pub struct SubmitTransportRequest {
    pub body: Vec<u8>,
    pub bundle: DiagnosticBundleV1,
}

pub fn build_bundle(
    input: &FeedbackReportInput,
    ctx: &ServiceContext,
) -> ServiceResult<DiagnosticBundleV1> {
    validate_feedback_input(input)?;

    let project = ctx.project.lock().ok().and_then(|guard| guard.clone());
    let project_metadata = project
        .as_ref()
        .map(|project| build_project_metadata(ctx, project));
    let connection_events = ctx.recent_connection_events();
    let ports_detected = detect_ports(ctx);
    let machine = build_machine_diagnostics(ctx, &ports_detected, &connection_events);
    let known_issues = known_issues_for(&machine, &connection_events);
    let locale = ctx
        .settings
        .lock()
        .ok()
        .map(|settings| settings.display_language.clone());

    let mut bundle = DiagnosticBundleV1 {
        schema_version: FEEDBACK_SCHEMA_VERSION,
        kind: input.kind,
        created_at: Utc::now().to_rfc3339(),
        client: DiagnosticClient {
            app_version: beambench_buildinfo::APP_VERSION.to_owned(),
            tauri_version: None,
            rust_version: Some(beambench_buildinfo::RUSTC_VERSION.to_owned()),
            build_target: beambench_buildinfo::TARGET_TRIPLE.to_owned(),
            git_sha: beambench_buildinfo::GIT_SHA.to_owned(),
        },
        system: DiagnosticSystem {
            os: std::env::consts::OS.to_owned(),
            os_version: None,
            arch: std::env::consts::ARCH.to_owned(),
            locale,
        },
        machine,
        ports_detected,
        connection_events,
        recent_serial: recent_serial_traffic(),
        recent_logs: active_error_entries(ctx),
        recent_panics: recent_panic_reports_for_bundle(ctx),
        terminal_job: ctx
            .last_terminal_job
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
            .map(|job| contextualize_terminal_job(job, input.source_context.as_ref())),
        project_metadata,
        known_issues,
        project_file_attached: input.include_project_file,
        source_context: input.source_context.clone(),
    };

    if is_successful_job_compatibility_report(input) {
        minimize_successful_job_compatibility_bundle(&mut bundle);
    }

    Ok(bundle)
}

fn is_successful_job_compatibility_report(input: &FeedbackReportInput) -> bool {
    input.kind == FeedbackKind::Connectivity
        && input
            .source_context
            .as_ref()
            .and_then(|context| context.feature.as_deref())
            == Some("job_compatibility_success")
}

fn minimize_successful_job_compatibility_bundle(bundle: &mut DiagnosticBundleV1) {
    // Compatibility reports answer one narrow question: which Beam Bench build,
    // OS/controller family, and machine settings completed a job. Keep personal
    // profile/device identifiers and design/command content out of that report.
    bundle.system.locale = None;
    bundle.machine.model = None;
    bundle.machine.profile_id = None;
    bundle.machine.profile_name = None;
    bundle.machine.port_name = None;
    bundle.machine.port_vendor_id = None;
    bundle.machine.port_product_id = None;
    bundle.machine.handshake_message = None;
    bundle.machine.machine_position = None;
    bundle.machine.work_position = None;
    bundle.machine.machine_coordinates_valid = false;
    bundle.ports_detected.clear();
    bundle.connection_events.clear();
    bundle.recent_serial = Default::default();
    bundle.recent_logs.clear();
    bundle.recent_panics.clear();
    bundle.project_metadata = None;
    bundle.known_issues.clear();
    bundle.project_file_attached = false;

    if let Some(job) = bundle.terminal_job.as_mut() {
        job.error = None;
        job.job_console.clear();
        job.session_console.clear();
        if let Some(progress) = job.progress.as_mut() {
            progress.error_message = None;
            progress.buckets.clear();
        }
    }

    if let Some(context) = bundle.source_context.as_mut() {
        context.error_message = None;
        context.stack = None;
        context.correlation_ts = None;
        context.action = None;
        context.error_code = None;
        context.error_details = None;
        context.frame_mode = None;
        context.frame_laser_on = None;
        context.selected_object_count = None;
        context.selected_bounds = None;
        context.jog_request = None;
    }
}

pub fn preview_feedback_report(
    ctx: &ServiceContext,
    input: FeedbackReportInput,
) -> ServiceResult<DiagnosticBundleV1> {
    let bundle = build_bundle(&input, ctx)?;
    scrub_deserialized(&bundle)
        .map_err(|e| ServiceError::internal(format!("Failed to scrub feedback preview: {e}")))
}

pub fn get_connection_diagnostics(
    ctx: &ServiceContext,
) -> ServiceResult<ConnectionDiagnosticsSnapshot> {
    let input = FeedbackReportInput {
        kind: FeedbackKind::Connectivity,
        title: None,
        description: None,
        notes: None,
        reply_to_email: None,
        include_project_file: false,
        source_context: None,
    };
    let bundle = preview_feedback_report(ctx, input)?;
    Ok(ConnectionDiagnosticsSnapshot {
        captured_at: bundle.created_at,
        ports_detected: bundle.ports_detected,
        machine: bundle.machine,
        connection_events: bundle.connection_events,
        recent_serial: bundle.recent_serial,
        known_issues: bundle.known_issues,
    })
}

pub fn save_feedback_report(
    ctx: &ServiceContext,
    input: FeedbackReportInput,
    path: &Path,
) -> ServiceResult<SavedReport> {
    let bundle = preview_feedback_report(ctx, input.clone())?;
    let project_bytes = if input.include_project_file {
        Some(project_attachment_bytes(ctx)?)
    } else {
        None
    };

    if let Some(bytes) = &project_bytes {
        validate_project_attachment_size(bytes.len())?;
    }

    let request = submit_request(input, bundle, None);
    let report_bytes = scrub_and_serialize(&request)
        .map_err(|e| ServiceError::internal(format!("Failed to serialize feedback report: {e}")))?;

    if let Some(project_bytes) = project_bytes {
        write_report_zip(path, &report_bytes, &project_bytes)?;
    } else {
        write_file_atomic(path, &report_bytes)?;
    }

    let size_bytes = fs::metadata(path)?.len();
    append_feedback_history(FeedbackHistoryEntry {
        recorded_at: Utc::now().to_rfc3339(),
        kind: request.kind,
        title: request.title.clone(),
        report_id: None,
        saved_path: Some(path.to_string_lossy().to_string()),
    })?;

    Ok(SavedReport {
        path: path.to_string_lossy().to_string(),
        size_bytes,
    })
}

pub fn build_submit_request_for_transport(
    ctx: &ServiceContext,
    input: FeedbackReportInput,
) -> ServiceResult<SubmitTransportRequest> {
    let mut bundle = preview_feedback_report(ctx, input.clone())?;
    let project_blob = if input.include_project_file {
        let project_bytes = project_attachment_bytes(ctx)?;
        validate_project_attachment_size(project_bytes.len())?;
        Some(BASE64_STANDARD.encode(project_bytes))
    } else {
        None
    };
    let mut request = submit_request(input.clone(), bundle.clone(), project_blob.clone());
    let mut bytes = scrub_and_serialize(&request)
        .map_err(|e| ServiceError::internal(format!("Failed to serialize feedback report: {e}")))?;

    if bytes.len() > MAX_SUBMIT_BODY_BYTES && !bundle.recent_panics.is_empty() {
        bundle.recent_panics.clear();
        request = submit_request(input, bundle.clone(), project_blob);
        bytes = scrub_and_serialize(&request).map_err(|e| {
            ServiceError::internal(format!("Failed to serialize feedback report: {e}"))
        })?;
    }

    if bytes.len() > MAX_SUBMIT_BODY_BYTES {
        return Err(ServiceError::invalid_input(format!(
            "Feedback report is too large to submit ({} bytes, limit {MAX_SUBMIT_BODY_BYTES})",
            bytes.len()
        ))
        .with_details(json!({
            "error": "too_large",
            "limit_bytes": MAX_SUBMIT_BODY_BYTES,
            "actual_bytes": bytes.len(),
        })));
    }

    Ok(SubmitTransportRequest {
        body: bytes,
        bundle,
    })
}

pub fn record_feedback_submission(
    input: &FeedbackReportInput,
    report_id: String,
) -> ServiceResult<()> {
    append_feedback_history(FeedbackHistoryEntry {
        recorded_at: Utc::now().to_rfc3339(),
        kind: input.kind,
        title: input.title.clone(),
        report_id: Some(report_id),
        saved_path: None,
    })
}

fn submit_request(
    input: FeedbackReportInput,
    bundle: DiagnosticBundleV1,
    project_file_blob: Option<String>,
) -> SubmitFeedbackRequest {
    SubmitFeedbackRequest {
        schema_version: FEEDBACK_SCHEMA_VERSION,
        kind: input.kind,
        title: input.title,
        description: input.description,
        notes: input.notes,
        reply_to_email: input.reply_to_email,
        bundle,
        project_file_attached: input.include_project_file,
        project_file_blob,
    }
}

fn validate_feedback_input(input: &FeedbackReportInput) -> ServiceResult<()> {
    if is_successful_job_compatibility_report(input) && input.include_project_file {
        return Err(ServiceError::invalid_input(
            "Project files cannot be attached to successful job compatibility reports",
        ));
    }
    if input
        .title
        .as_deref()
        .is_some_and(|title| title.chars().count() > MAX_FEEDBACK_TITLE_CHARS)
    {
        return Err(ServiceError::invalid_input(format!(
            "Title must be {MAX_FEEDBACK_TITLE_CHARS} characters or fewer"
        )));
    }

    let description_chars = input
        .description
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .chars()
        .count();
    if matches!(input.kind, FeedbackKind::Bug | FeedbackKind::Crash) && description_chars == 0 {
        return Err(ServiceError::invalid_input(
            "Description is required for bug and crash reports",
        ));
    }
    if description_chars > MAX_FEEDBACK_DESCRIPTION_CHARS {
        return Err(ServiceError::invalid_input(format!(
            "Description must be {MAX_FEEDBACK_DESCRIPTION_CHARS} characters or fewer"
        )));
    }

    if input
        .notes
        .as_deref()
        .is_some_and(|notes| notes.chars().count() > MAX_FEEDBACK_DESCRIPTION_CHARS)
    {
        return Err(ServiceError::invalid_input(format!(
            "Notes must be {MAX_FEEDBACK_DESCRIPTION_CHARS} characters or fewer"
        )));
    }

    if input
        .reply_to_email
        .as_deref()
        .is_some_and(|email| email.chars().count() > MAX_FEEDBACK_REPLY_TO_EMAIL_CHARS)
    {
        return Err(ServiceError::invalid_input(format!(
            "Reply-to email must be {MAX_FEEDBACK_REPLY_TO_EMAIL_CHARS} characters or fewer"
        )));
    }

    Ok(())
}

fn validate_project_attachment_size(size: usize) -> ServiceResult<()> {
    if size > MAX_PROJECT_ATTACHMENT_RAW_BYTES {
        return Err(ServiceError::invalid_input(format!(
            "Project file is too large to attach ({} bytes, limit {MAX_PROJECT_ATTACHMENT_RAW_BYTES})",
            size
        ))
        .with_details(json!({
            "error": "too_large",
            "limit_bytes": MAX_PROJECT_ATTACHMENT_RAW_BYTES,
            "actual_bytes": size,
        })));
    }
    Ok(())
}

fn project_attachment_bytes(ctx: &ServiceContext) -> ServiceResult<Vec<u8>> {
    let project = ctx
        .project
        .lock()
        .map_err(|e| ServiceError::internal(format!("Failed to lock project: {e}")))?
        .clone()
        .ok_or_else(|| ServiceError::not_found("No project is open to attach"))?;
    save_project_to_bytes(&project)
        .map_err(|e| ServiceError::persistence(format!("Failed to serialize project: {e}")))
}

fn bounds_for_objects<'a>(
    objects: impl Iterator<Item = &'a beambench_core::ProjectObject>,
) -> Option<Bounds> {
    objects
        .map(|object| object.bounds)
        .reduce(|bounds, next| bounds.union(&next))
}

fn object_type(object: &ObjectData) -> &'static str {
    match object {
        ObjectData::RasterImage { .. } => "raster_image",
        ObjectData::VectorPath { .. } => "vector_path",
        ObjectData::Shape { .. } => "shape",
        ObjectData::Star { .. } => "star",
        ObjectData::Text { .. } => "text",
        ObjectData::Polygon { .. } => "polygon",
        ObjectData::Barcode { .. } => "barcode",
        ObjectData::Group { .. } => "group",
        ObjectData::VirtualClone { .. } => "virtual_clone",
    }
}

fn contextualize_terminal_job(
    mut job: beambench_common::feedback::DiagnosticTerminalJob,
    source_context: Option<&beambench_common::feedback::FeedbackSourceContext>,
) -> beambench_common::feedback::DiagnosticTerminalJob {
    let report_time = source_context
        .and_then(|context| context.correlation_ts.as_deref())
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);
    let captured_at = DateTime::parse_from_rfc3339(&job.captured_at)
        .ok()
        .map(|value| value.with_timezone(&Utc));
    let age_ms = captured_at.map(|captured| {
        report_time
            .signed_duration_since(captured)
            .num_milliseconds()
            .unsigned_abs()
    });
    let relevant = age_ms.is_some_and(|age| age <= 5 * 60 * 1_000);
    job.age_ms = age_ms;
    job.relevant_to_report = Some(relevant);
    if !relevant {
        job.job_console.clear();
        job.session_console.clear();
    }
    job
}

fn build_project_metadata(ctx: &ServiceContext, project: &Project) -> DiagnosticProjectMetadata {
    let mut has_raster = false;
    let mut has_vector = false;
    let mut has_text = false;
    for object in &project.objects {
        match &object.data {
            ObjectData::RasterImage { .. } => has_raster = true,
            ObjectData::Text { .. } => has_text = true,
            ObjectData::VectorPath { .. }
            | ObjectData::Shape { .. }
            | ObjectData::Star { .. }
            | ObjectData::Polygon { .. }
            | ObjectData::Barcode { .. }
            | ObjectData::Group { .. }
            | ObjectData::VirtualClone { .. } => has_vector = true,
        }
    }

    let size_bytes = save_project_to_bytes(project)
        .ok()
        .map(|bytes| bytes.len() as u64);

    let output_layer_ids = project
        .layers
        .iter()
        .filter(|layer| layer.enabled && !layer.is_tool_layer)
        .map(|layer| layer.id)
        .collect::<std::collections::HashSet<_>>();
    let artwork_bounds = bounds_for_objects(project.objects.iter());
    let output_bounds = bounds_for_objects(
        project
            .objects
            .iter()
            .filter(|object| object.visible && output_layer_ids.contains(&object.layer_id)),
    );
    let runtime_current_position = ctx
        .optimization_runtime
        .lock()
        .ok()
        .and_then(|runtime| runtime.current_position);
    let placement_target = match project.start_from {
        StartFromMode::AbsoluteCoords => None,
        StartFromMode::CurrentPosition => runtime_current_position,
        StartFromMode::UserOrigin => project.user_origin,
    };
    let cached_plan_bounds = ctx
        .plan_cache
        .lock()
        .ok()
        .and_then(|plan| plan.as_ref().map(|plan| plan.bounds));
    let layers = project
        .layers
        .iter()
        .take(MAX_DIAGNOSTIC_PROJECT_ITEMS)
        .map(|layer| DiagnosticProjectLayer {
            layer_id: layer.id.to_string(),
            enabled: layer.enabled,
            visible: layer.visible,
            is_tool_layer: layer.is_tool_layer,
            operations: layer
                .entries
                .iter()
                .filter_map(|entry| serialized_enum_string(entry.operation))
                .collect(),
        })
        .collect();
    let objects = project
        .objects
        .iter()
        .take(MAX_DIAGNOSTIC_PROJECT_ITEMS)
        .map(|object| DiagnosticProjectObject {
            object_id: object.id.to_string(),
            object_type: object_type(&object.data).to_owned(),
            layer_id: object.layer_id.to_string(),
            visible: object.visible,
            locked: object.locked,
            bounds: object.bounds,
        })
        .collect();

    DiagnosticProjectMetadata {
        object_count: project.objects.len(),
        layer_count: project.layers.len(),
        size_bytes,
        has_raster,
        has_vector,
        has_text,
        workspace_width_mm: project.workspace.bed_width_mm,
        workspace_height_mm: project.workspace.bed_height_mm,
        workspace_origin: serialized_enum_string(project.workspace.origin)
            .unwrap_or_else(|| "unknown".to_owned()),
        start_from: serialized_enum_string(project.start_from)
            .unwrap_or_else(|| "unknown".to_owned()),
        job_origin: serialized_enum_string(project.job_origin)
            .unwrap_or_else(|| "unknown".to_owned()),
        user_origin: project.user_origin,
        runtime_current_position,
        placement_target,
        artwork_bounds,
        output_bounds,
        cached_plan_bounds,
        layers,
        objects,
        project_path: None,
    }
}

fn detect_ports(ctx: &ServiceContext) -> Vec<DiagnosticPort> {
    let owned_port = beambench_owned_serial_port(ctx);

    list_available_ports()
        .unwrap_or_default()
        .into_iter()
        .map(|port| diagnostic_port(port, owned_port.as_deref()))
        .collect()
}

fn serial_port_from_endpoint(endpoint: &ControllerConnectionEndpoint) -> Option<&str> {
    match endpoint {
        ControllerConnectionEndpoint::Serial { port_name, .. } => Some(port_name),
        ControllerConnectionEndpoint::Tcp { .. }
        | ControllerConnectionEndpoint::Udp { .. }
        | ControllerConnectionEndpoint::Usb { .. } => None,
    }
}

fn beambench_owned_serial_port(ctx: &ServiceContext) -> Option<String> {
    let active_port = ctx
        .session
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().and_then(|session| session.port_name()));
    if active_port.is_some() {
        return active_port;
    }

    ctx.pending_controller_connection
        .lock()
        .ok()
        .and_then(|guard| {
            guard
                .as_ref()
                .and_then(|pending| serial_port_from_endpoint(&pending.endpoint).map(str::to_owned))
        })
}

fn diagnostic_port(port: PortInfo, owned_port: Option<&str>) -> DiagnosticPort {
    DiagnosticPort {
        name: port.port_name.clone(),
        description: non_empty_string(port.description),
        manufacturer: non_empty_string(port.manufacturer),
        vendor_id: port.vid.map(|vid| format!("0x{vid:04x}")),
        product_id: port.pid.map(|pid| format!("0x{pid:04x}")),
        in_use_by_beambench: owned_port.is_some_and(|owned| owned == port.port_name),
        available: owned_port.is_none_or(|owned| owned != port.port_name),
    }
}

fn non_empty_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn build_machine_diagnostics(
    ctx: &ServiceContext,
    ports: &[DiagnosticPort],
    connection_events: &[DiagnosticConnectionEvent],
) -> DiagnosticMachine {
    let profile = ctx.settings.lock().ok().and_then(|settings| {
        settings.active_profile_id.and_then(|id| {
            settings
                .machine_profiles
                .iter()
                .find(|p| p.id == id)
                .cloned()
        })
    });
    let profile_baud_rate = profile.as_ref().map(|profile| profile.default_baud_rate);
    let profile_model = profile.as_ref().map(|profile| profile.name.clone());
    let profile_fields = MachineProfileDiagnosticFields::from_profile(profile.as_ref());

    let session = ctx.session.lock().ok();
    let Some(session) = session.as_deref().and_then(|guard| guard.as_ref()) else {
        drop(session);
        let pending_endpoint = ctx
            .pending_controller_connection
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|pending| pending.endpoint.clone()));
        let last_port_event = connection_events
            .iter()
            .rev()
            .find(|event| event.port_name.is_some());
        let port_name = pending_endpoint
            .as_ref()
            .and_then(serial_port_from_endpoint)
            .map(str::to_owned)
            .or_else(|| last_port_event.and_then(|event| event.port_name.clone()));
        let port = port_name
            .as_deref()
            .and_then(|attempted| ports.iter().find(|port| port.name == attempted));
        let failure = latest_connection_failure(connection_events);
        let waiting_for_choice = pending_endpoint.is_some();
        return DiagnosticMachine {
            connected: false,
            model: profile_model,
            profile_id: profile_fields.profile_id,
            profile_name: profile_fields.profile_name,
            profile_preset_id: profile_fields.profile_preset_id,
            profile_preset_version: profile_fields.profile_preset_version,
            firmware_type: profile_fields.firmware_type,
            controller_family: None,
            controller_model: None,
            transport_kind: last_port_event.and_then(|event| event.transport_kind.clone()),
            transfer_mode: profile_fields.transfer_mode,
            s_value_max: profile_fields.s_value_max,
            homing_enabled: profile_fields.homing_enabled,
            use_constant_power: profile_fields.use_constant_power,
            emit_s_every_g1: profile_fields.emit_s_every_g1,
            use_g0_for_overscan: profile_fields.use_g0_for_overscan,
            bed_width_mm: profile_fields.bed_width_mm,
            bed_height_mm: profile_fields.bed_height_mm,
            workspace_origin: profile_fields.workspace_origin,
            max_speed_mm_min: profile_fields.max_speed_mm_min,
            controller_travel_x_mm: None,
            controller_travel_y_mm: None,
            run_state: None,
            machine_position: None,
            work_position: None,
            machine_coordinates_valid: false,
            firmware_version: None,
            baud_rate: pending_endpoint
                .as_ref()
                .and_then(ControllerConnectionEndpoint::baud_rate)
                .or_else(|| last_port_event.and_then(|event| event.baud_rate))
                .or(profile_baud_rate),
            port_name,
            port_vendor_id: port
                .and_then(|port| port.vendor_id.clone())
                .or_else(|| last_port_event.and_then(|event| event.vendor_id.clone())),
            port_product_id: port
                .and_then(|port| port.product_id.clone())
                .or_else(|| last_port_event.and_then(|event| event.product_id.clone())),
            session_state: if waiting_for_choice {
                DiagnosticSessionState::Connecting
            } else if failure.is_some() {
                DiagnosticSessionState::HandshakeFailed
            } else {
                DiagnosticSessionState::Disconnected
            },
            handshake_message: if waiting_for_choice {
                Some("Controller detected; waiting for controller selection".to_owned())
            } else {
                failure
                    .map(connection_failure_summary)
                    .or_else(|| Some("Disconnected".to_owned()))
            },
        };
    };

    let controller_info = session.controller_info().unwrap_or_default();
    let firmware_version = controller_info
        .get("VER")
        .or_else(|| controller_info.get("Banner"))
        .cloned();
    let port_name = session.port_name();
    let port = port_name
        .as_deref()
        .and_then(|active| ports.iter().find(|port| port.name == active));
    let state = session_state_to_diagnostic(session.session_state());
    let status = session.machine_status();
    let controller_settings = session.controller_settings().unwrap_or_default();
    let controller_travel_x_mm = controller_settings
        .get("$130")
        .and_then(|value| value.parse::<f64>().ok());
    let controller_travel_y_mm = controller_settings
        .get("$131")
        .and_then(|value| value.parse::<f64>().ok());

    DiagnosticMachine {
        connected: !matches!(session.session_state(), SessionState::Disconnected),
        model: profile_model
            .clone()
            .or_else(|| Some(format!("{:?}", session.controller_model()))),
        profile_id: profile_fields.profile_id,
        profile_name: profile_fields.profile_name,
        profile_preset_id: profile_fields.profile_preset_id,
        profile_preset_version: profile_fields.profile_preset_version,
        firmware_type: profile_fields.firmware_type,
        controller_family: serialized_enum_string(session.controller_family()),
        controller_model: serialized_enum_string(session.controller_model()),
        transport_kind: serialized_enum_string(session.transport_kind()),
        transfer_mode: profile_fields.transfer_mode,
        s_value_max: profile_fields.s_value_max,
        homing_enabled: profile_fields.homing_enabled,
        use_constant_power: profile_fields.use_constant_power,
        emit_s_every_g1: profile_fields.emit_s_every_g1,
        use_g0_for_overscan: profile_fields.use_g0_for_overscan,
        bed_width_mm: profile_fields.bed_width_mm,
        bed_height_mm: profile_fields.bed_height_mm,
        workspace_origin: profile_fields.workspace_origin,
        max_speed_mm_min: profile_fields.max_speed_mm_min,
        controller_travel_x_mm,
        controller_travel_y_mm,
        run_state: Some(status.run_state),
        machine_position: Some(status.machine_position),
        work_position: Some(status.work_position),
        machine_coordinates_valid: ctx
            .machine_coordinates_valid
            .load(std::sync::atomic::Ordering::Acquire),
        firmware_version,
        baud_rate: profile_baud_rate,
        port_name,
        port_vendor_id: port.and_then(|port| port.vendor_id.clone()),
        port_product_id: port.and_then(|port| port.product_id.clone()),
        session_state: state,
        handshake_message: handshake_message(session.session_state(), &controller_info),
    }
}

#[derive(Default)]
struct MachineProfileDiagnosticFields {
    profile_id: Option<String>,
    profile_name: Option<String>,
    profile_preset_id: Option<String>,
    profile_preset_version: Option<u32>,
    firmware_type: Option<String>,
    transfer_mode: Option<String>,
    s_value_max: Option<u32>,
    homing_enabled: Option<bool>,
    use_constant_power: Option<bool>,
    emit_s_every_g1: Option<bool>,
    use_g0_for_overscan: Option<bool>,
    bed_width_mm: Option<f64>,
    bed_height_mm: Option<f64>,
    workspace_origin: Option<String>,
    max_speed_mm_min: Option<f64>,
}

impl MachineProfileDiagnosticFields {
    fn from_profile(profile: Option<&MachineProfile>) -> Self {
        let Some(profile) = profile else {
            return Self::default();
        };
        Self {
            profile_id: Some(profile.id.to_string()),
            profile_name: Some(profile.name.clone()),
            profile_preset_id: profile.preset_id.clone(),
            profile_preset_version: profile.preset_version,
            firmware_type: Some(profile.firmware_type.clone()),
            transfer_mode: serialized_enum_string(profile.transfer_mode),
            s_value_max: Some(profile.s_value_max),
            homing_enabled: Some(profile.homing_enabled),
            use_constant_power: Some(profile.use_constant_power),
            emit_s_every_g1: Some(profile.emit_s_every_g1),
            use_g0_for_overscan: Some(profile.use_g0_for_overscan),
            bed_width_mm: Some(profile.bed_width_mm),
            bed_height_mm: Some(profile.bed_height_mm),
            workspace_origin: serialized_enum_string(profile.origin),
            max_speed_mm_min: Some(profile.max_speed_mm_min),
        }
    }
}

fn serialized_enum_string<T: serde::Serialize>(value: T) -> Option<String> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
}

fn session_state_to_diagnostic(state: SessionState) -> DiagnosticSessionState {
    match state {
        SessionState::Disconnected => DiagnosticSessionState::Disconnected,
        SessionState::Connecting | SessionState::TransportOpen | SessionState::WaitingForBanner => {
            DiagnosticSessionState::Connecting
        }
        SessionState::Ready | SessionState::Validating | SessionState::Alarm => {
            DiagnosticSessionState::Connected
        }
        SessionState::Running | SessionState::Paused => DiagnosticSessionState::Streaming,
        SessionState::Error => DiagnosticSessionState::Error,
    }
}

fn handshake_message(
    state: SessionState,
    controller_info: &std::collections::HashMap<String, String>,
) -> Option<String> {
    if let Some(banner) = controller_info.get("Banner") {
        return Some(format!("Detected GRBL response: {banner}"));
    }

    match state {
        SessionState::WaitingForBanner | SessionState::TransportOpen | SessionState::Connecting => {
            Some("No GRBL response yet - possible baud rate or driver issue".to_owned())
        }
        SessionState::Error => Some("Connection error".to_owned()),
        _ => None,
    }
}

fn latest_connection_failure(
    events: &[DiagnosticConnectionEvent],
) -> Option<&DiagnosticConnectionEvent> {
    for event in events.iter().rev() {
        let failed_stage = event.stage.ends_with("_failed")
            || event.stage.ends_with("_timeout")
            || event.stage == "banner_timeout";
        if event.error.is_some() || failed_stage {
            return Some(event);
        }
        if matches!(
            event.stage.as_str(),
            "open_attempt" | "transport_open" | "ready"
        ) {
            return None;
        }
    }
    None
}

fn connection_failure_summary(event: &DiagnosticConnectionEvent) -> String {
    if event.error_code.as_deref() == Some("lihuiyu_incompatible_windows_driver") {
        let driver = event
            .usb_driver
            .as_deref()
            .unwrap_or("an incompatible driver");
        return format!(
            "This Lihuiyu controller is using Windows USB driver {driver}. Beam Bench requires WinUSB for USB device 1a86:5512. Reconnect the controller and refresh the USB list after changing the driver."
        );
    }
    if event.error_code.as_deref() == Some("serial_port_unavailable")
        || event
            .error
            .as_deref()
            .is_some_and(|error| error.contains("[serial_port_unavailable]"))
    {
        return match event.port_name.as_deref() {
            Some(port) => format!(
                "Beam Bench could not open {port}. Another application may be using the port, or the controller may have been disconnected."
            ),
            None => "Beam Bench could not open the serial port. Another application may be using it, or the controller may have been disconnected.".to_owned(),
        };
    }

    event
        .error
        .clone()
        .or_else(|| event.message.clone())
        .unwrap_or_else(|| "Connection failed".to_owned())
}

fn active_error_entries(ctx: &ServiceContext) -> Vec<DiagnosticLogEntry> {
    ctx.active_errors
        .lock()
        .map(|errors| {
            errors
                .iter()
                .rev()
                .take(200)
                .map(log_entry_from_active_error)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect()
        })
        .unwrap_or_default()
}

fn log_entry_from_active_error(entry: &ActiveErrorEntry) -> DiagnosticLogEntry {
    let (level, rest) = entry
        .line
        .split_once(' ')
        .map(|(level, rest)| (level.to_owned(), rest))
        .unwrap_or_else(|| ("warn".to_owned(), entry.line.as_str()));
    let (target, message) = rest
        .split_once(": ")
        .map(|(target, message)| (target.to_owned(), message.to_owned()))
        .unwrap_or_else(|| ("unknown".to_owned(), rest.to_owned()));

    DiagnosticLogEntry {
        ts: entry.ts.to_rfc3339(),
        level,
        target,
        message,
    }
}

fn recent_panic_reports_for_bundle(ctx: &ServiceContext) -> Vec<DiagnosticPanic> {
    let mut total_bytes = 2usize;
    let mut reports = Vec::new();

    for stored in ctx.recent_panic_reports() {
        let Ok(report_bytes) = serde_json::to_vec(&stored.report) else {
            continue;
        };
        let separator_bytes = usize::from(!reports.is_empty());
        let next_total = total_bytes + separator_bytes + report_bytes.len();
        if next_total > MAX_PANIC_REPORT_BUNDLE_BYTES {
            break;
        }
        total_bytes = next_total;
        reports.push(stored.report);
    }

    reports
}

fn known_issues_for(
    machine: &DiagnosticMachine,
    connection_events: &[DiagnosticConnectionEvent],
) -> Vec<KnownIssueWarning> {
    let issue_match_text = [
        machine.model.as_deref(),
        machine.firmware_type.as_deref(),
        machine.controller_family.as_deref(),
        machine.controller_model.as_deref(),
        machine.firmware_version.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ")
    .to_lowercase();
    let firmware = machine
        .firmware_version
        .as_deref()
        .unwrap_or_default()
        .to_lowercase();

    let mut issues = KNOWN_ISSUES
        .iter()
        .filter(|issue| issue_match_text.contains(issue.model_contains))
        .filter(|issue| {
            issue
                .firmware_contains
                .is_none_or(|needle| firmware.contains(needle))
        })
        .filter(|issue| {
            issue
                .configured_baud
                .is_none_or(|baud| machine.baud_rate == Some(baud))
        })
        .filter(|issue| issue.code != "no_grbl_response" || machine.firmware_version.is_none())
        .map(|issue| KnownIssueWarning {
            code: issue.code.to_owned(),
            severity: issue.severity.to_owned(),
            message: issue.message.to_owned(),
        })
        .collect::<Vec<_>>();

    if let Some(failure) = latest_connection_failure(connection_events)
        && failure.error_code.as_deref() == Some("lihuiyu_incompatible_windows_driver")
    {
        issues.retain(|issue| issue.code != "no_grbl_response");
        issues.insert(
            0,
            KnownIssueWarning {
                code: "lihuiyu_incompatible_windows_driver".to_owned(),
                severity: "warning".to_owned(),
                message: connection_failure_summary(failure),
            },
        );
    } else if let Some(failure) = latest_connection_failure(connection_events)
        && failure.stage == "open_failed"
        && failure.transport_kind.as_deref() != Some("usb")
    {
        issues.retain(|issue| issue.code != "no_grbl_response");
        issues.insert(
            0,
            KnownIssueWarning {
                code: "serial_port_unavailable".to_owned(),
                severity: "warning".to_owned(),
                message: connection_failure_summary(failure),
            },
        );
    }

    issues
}

fn write_report_zip(path: &Path, report_bytes: &[u8], project_bytes: &[u8]) -> ServiceResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temp = tempfile::NamedTempFile::new_in(parent)?;
    {
        let file = temp.as_file();
        let mut zip = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        zip.start_file("report.json", options)
            .map_err(|e| ServiceError::persistence(format!("Failed to create report zip: {e}")))?;
        zip.write_all(report_bytes)?;
        zip.start_file("project.lzrproj", options)
            .map_err(|e| ServiceError::persistence(format!("Failed to create report zip: {e}")))?;
        zip.write_all(project_bytes)?;
        zip.finish()
            .map_err(|e| ServiceError::persistence(format!("Failed to finish report zip: {e}")))?;
    }
    temp.persist(path)
        .map_err(|e| ServiceError::persistence(format!("Failed to save report: {}", e.error)))?;
    Ok(())
}

fn write_file_atomic(path: &Path, bytes: &[u8]) -> ServiceResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temp = tempfile::NamedTempFile::new_in(parent)?;
    {
        let mut file = temp.as_file();
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    temp.persist(path)
        .map_err(|e| ServiceError::persistence(format!("Failed to save report: {}", e.error)))?;
    Ok(())
}

pub fn default_report_filename(kind: FeedbackKind, attached_project: bool) -> String {
    let stamp = Utc::now().format("%Y%m%d-%H%M%S-%3f");
    let suffix = if attached_project { "zip" } else { "json" };
    format!("beambench-report-{}-{stamp}.{suffix}", kind.as_str())
}

fn feedback_history_path() -> ServiceResult<PathBuf> {
    persist::data_dir()
        .map(|path| path.join("feedback_history.jsonl"))
        .ok_or_else(|| ServiceError::persistence("Could not determine app data directory"))
}

fn append_feedback_history(entry: FeedbackHistoryEntry) -> ServiceResult<()> {
    let path = feedback_history_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut entries = read_feedback_history(&path);
    entries.push(entry);
    if entries.len() > FEEDBACK_HISTORY_MAX_ENTRIES {
        entries.drain(0..entries.len() - FEEDBACK_HISTORY_MAX_ENTRIES);
    }

    let mut payload = String::new();
    for entry in entries {
        let line = serde_json::to_string(&entry).map_err(|e| {
            ServiceError::internal(format!("Failed to serialize feedback history: {e}"))
        })?;
        payload.push_str(&line);
        payload.push('\n');
    }
    write_file_atomic(&path, payload.as_bytes())
}

fn read_feedback_history(path: &Path) -> Vec<FeedbackHistoryEntry> {
    fs::read_to_string(path)
        .ok()
        .map(|content| {
            content
                .lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect()
        })
        .unwrap_or_default()
}

pub fn delete_included_panic_files(ctx: &ServiceContext, panics: &[DiagnosticPanic]) {
    let panic_keys: std::collections::HashSet<(&str, &str)> = panics
        .iter()
        .map(|panic| (panic.ts.as_str(), panic.message.as_str()))
        .collect();
    let remaining = ctx
        .recent_panic_reports()
        .into_iter()
        .filter(|stored| {
            let included =
                panic_keys.contains(&(stored.report.ts.as_str(), stored.report.message.as_str()));
            if included {
                let _ = fs::remove_file(&stored.path);
            }
            !included
        })
        .collect();
    ctx.set_panic_reports(remaining);
}

pub fn load_panic_reports_from_dir(dir: &Path) -> Vec<crate::context::StoredPanicReport> {
    let cutoff = Utc::now() - chrono::Duration::days(7);
    let mut entries = fs::read_dir(dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter_map(|entry| {
            let path = entry.path();
            if fs::metadata(&path)
                .ok()
                .is_some_and(|metadata| metadata.len() > MAX_PANIC_REPORT_FILE_BYTES)
            {
                return None;
            }
            let content = fs::read_to_string(&path).ok()?;
            let report: DiagnosticPanic = serde_json::from_str(&content).ok()?;
            let parsed = DateTime::parse_from_rfc3339(&report.ts)
                .ok()
                .map(|ts| ts.with_timezone(&Utc))?;
            if parsed < cutoff {
                let _ = fs::remove_file(&path);
                return None;
            }
            let report = scrub_deserialized(&report).ok()?;
            Some(crate::context::StoredPanicReport { report, path })
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| b.report.ts.cmp(&a.report.ts));
    entries.truncate(50);
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::feedback::{DiagnosticTerminalJob, FeedbackSourceContext};
    use beambench_common::machine::{
        JobProgress, JobProgressBucket, JobState, MachineRunState, SessionState,
    };
    use beambench_common::{ConsoleDirection, ConsoleEntry};
    use beambench_core::Project;
    use std::io::Read;
    use tempfile::TempDir;

    fn bug_input() -> FeedbackReportInput {
        FeedbackReportInput {
            kind: FeedbackKind::Bug,
            title: Some("Bug".to_owned()),
            description: Some("Something failed".to_owned()),
            notes: None,
            reply_to_email: Some("support@example.com".to_owned()),
            include_project_file: false,
            source_context: Some(FeedbackSourceContext {
                source: "help_menu".to_owned(),
                error_message: None,
                stack: None,
                feature: None,
                correlation_ts: None,
                ..FeedbackSourceContext::default()
            }),
        }
    }

    fn successful_job_compatibility_input() -> FeedbackReportInput {
        FeedbackReportInput {
            kind: FeedbackKind::Connectivity,
            title: Some("Successful job compatibility check".to_owned()),
            description: None,
            notes: None,
            reply_to_email: None,
            include_project_file: false,
            source_context: Some(FeedbackSourceContext {
                source: "post_job_prompt".to_owned(),
                error_message: None,
                stack: None,
                feature: Some("job_compatibility_success".to_owned()),
                correlation_ts: Some("2026-05-14T12:34:56Z".to_owned()),
                ..FeedbackSourceContext::default()
            }),
        }
    }

    #[test]
    fn preview_builds_scrubbed_bundle_from_active_errors_and_panics() {
        let ctx = ServiceContext::new();
        let active_error_ts = DateTime::parse_from_rfc3339("2026-05-14T12:34:56Z")
            .unwrap()
            .with_timezone(&Utc);
        ctx.push_error_at(
            "WARN test: opened /Users/alice/Documents/a.lzrproj".to_owned(),
            active_error_ts,
        );
        ctx.push_connection_event(
            "open_failed",
            Some("/Users/alice/dev/cu.usbserial".to_owned()),
            Some(115200),
            None,
            Some("failed opening /Users/alice/dev/cu.usbserial".to_owned()),
        );
        ctx.set_panic_reports(vec![crate::context::StoredPanicReport {
            report: DiagnosticPanic {
                ts: Utc::now().to_rfc3339(),
                thread: Some("main".to_owned()),
                message: "panic at /Users/alice/Documents/a.lzrproj".to_owned(),
                location: None,
                backtrace: None,
                app_version: "0.1.0".to_owned(),
                os: "macOS".to_owned(),
                build_target: "aarch64-apple-darwin".to_owned(),
                git_sha: "abc123".to_owned(),
            },
            path: PathBuf::from("/tmp/panic.json"),
        }]);
        *ctx.project.lock().unwrap() = Some(Project::new("Example"));
        *ctx.project_path.lock().unwrap() = Some(PathBuf::from("/Users/alice/Documents/a.lzrproj"));

        let bundle = preview_feedback_report(&ctx, bug_input()).unwrap();
        let json = serde_json::to_string(&bundle).unwrap();

        assert!(!json.contains("alice"));
        assert!(json.contains("<userhome>/Documents/a.lzrproj"));
        assert_eq!(
            bundle
                .project_metadata
                .as_ref()
                .and_then(|metadata| metadata.project_path.as_deref()),
            None
        );
        assert_eq!(bundle.recent_logs.len(), 1);
        assert_eq!(bundle.recent_logs[0].ts, "2026-05-14T12:34:56+00:00");
    }

    #[test]
    fn preview_preserves_attempted_port_locale_and_unavailable_port_diagnostics() {
        let ctx = ServiceContext::new();
        ctx.settings.lock().unwrap().display_language = "fr".to_owned();
        ctx.push_connection_event(
            "open_failed",
            Some("COM5".to_owned()),
            Some(115_200),
            None,
            Some(
                "[serial_port_unavailable] Could not open COM5: Accès refusé. The port may be in use by another application or the controller may have been disconnected."
                    .to_owned(),
            ),
        );

        let bundle = preview_feedback_report(&ctx, bug_input()).unwrap();

        assert_eq!(bundle.system.locale.as_deref(), Some("fr"));
        assert_eq!(bundle.machine.port_name.as_deref(), Some("COM5"));
        assert_eq!(bundle.machine.baud_rate, Some(115_200));
        assert_eq!(
            bundle.machine.session_state,
            DiagnosticSessionState::HandshakeFailed
        );
        assert!(
            bundle
                .machine
                .handshake_message
                .as_deref()
                .is_some_and(|message| message.contains("could not open COM5"))
        );
        assert_eq!(
            bundle.connection_events[0].error_code.as_deref(),
            Some("serial_port_unavailable")
        );
        assert!(
            bundle
                .known_issues
                .iter()
                .any(|issue| issue.code == "serial_port_unavailable")
        );
    }

    #[test]
    fn preview_preserves_incompatible_lihuiyu_windows_driver_diagnostics() {
        let ctx = ServiceContext::new();
        ctx.push_usb_connection_event(
            "open_failed",
            "usb-bus-PCIROOT(0)-ports-1".to_owned(),
            0x1a86,
            0x5512,
            Some("CH341PAR".to_owned()),
            None,
            Some("lihuiyu_incompatible_windows_driver".to_owned()),
            Some("[lihuiyu_incompatible_windows_driver] Beam Bench requires WinUSB".to_owned()),
        );

        let bundle = preview_feedback_report(&ctx, bug_input()).unwrap();

        assert_eq!(bundle.machine.transport_kind.as_deref(), Some("usb"));
        assert_eq!(bundle.machine.port_vendor_id.as_deref(), Some("0x1a86"));
        assert_eq!(bundle.machine.port_product_id.as_deref(), Some("0x5512"));
        assert_eq!(
            bundle.connection_events[0].usb_driver.as_deref(),
            Some("CH341PAR")
        );
        assert_eq!(
            bundle.connection_events[0].error_code.as_deref(),
            Some("lihuiyu_incompatible_windows_driver")
        );
        assert!(
            bundle
                .known_issues
                .iter()
                .any(|issue| issue.code == "lihuiyu_incompatible_windows_driver"
                    && issue.message.contains("CH341PAR")
                    && issue.message.contains("WinUSB"))
        );
    }

    #[test]
    fn preview_includes_retained_terminal_job_diagnostic() {
        let ctx = ServiceContext::new();
        let mut progress = JobProgress::default();
        progress.state = JobState::Failed;
        progress.error_message =
            Some("Controller reported Idle while bytes were pending".to_string());
        *ctx.last_terminal_job.lock().unwrap() = Some(DiagnosticTerminalJob {
            captured_at: Utc::now().to_rfc3339(),
            age_ms: None,
            relevant_to_report: None,
            reason: "fatal_tick_error".to_string(),
            progress: Some(progress),
            error: Some("serial read failed".to_string()),
            job_tick_loop_running: true,
            session_state: Some(SessionState::Running),
            machine_run_state: Some(MachineRunState::Run),
            job_console: vec![ConsoleEntry {
                timestamp: Utc::now(),
                direction: ConsoleDirection::Sent,
                content: "G1 X10".to_string(),
            }],
            session_console: vec![ConsoleEntry {
                timestamp: Utc::now(),
                direction: ConsoleDirection::Received,
                content: "error:2".to_string(),
            }],
        });

        let bundle = preview_feedback_report(&ctx, bug_input()).unwrap();
        let retained = bundle.terminal_job.expect("retained terminal job included");

        assert_eq!(retained.reason, "fatal_tick_error");
        assert_eq!(
            retained.progress.as_ref().map(|progress| progress.state),
            Some(JobState::Failed)
        );
        assert_eq!(retained.error.as_deref(), Some("serial read failed"));
        assert_eq!(retained.job_console[0].content, "G1 X10");
        assert_eq!(retained.session_console[0].content, "error:2");
    }

    #[test]
    fn preview_omits_console_content_from_stale_terminal_jobs() {
        let ctx = ServiceContext::new();
        *ctx.last_terminal_job.lock().unwrap() = Some(DiagnosticTerminalJob {
            captured_at: "2026-01-01T00:00:00Z".to_owned(),
            age_ms: None,
            relevant_to_report: None,
            reason: "completed".to_owned(),
            progress: Some(JobProgress::default()),
            error: None,
            job_tick_loop_running: false,
            session_state: Some(SessionState::Ready),
            machine_run_state: Some(MachineRunState::Idle),
            job_console: vec![ConsoleEntry {
                timestamp: Utc::now(),
                direction: ConsoleDirection::Sent,
                content: "G1 X10".to_owned(),
            }],
            session_console: vec![ConsoleEntry {
                timestamp: Utc::now(),
                direction: ConsoleDirection::Received,
                content: "ok".to_owned(),
            }],
        });

        let bundle = preview_feedback_report(&ctx, bug_input()).unwrap();
        let retained = bundle.terminal_job.expect("retained terminal job summary");

        assert_eq!(retained.relevant_to_report, Some(false));
        assert!(retained.age_ms.is_some_and(|age| age > 5 * 60 * 1_000));
        assert!(retained.job_console.is_empty());
        assert!(retained.session_console.is_empty());
    }

    #[test]
    fn successful_job_compatibility_preview_excludes_design_and_trace_data() {
        let ctx = ServiceContext::new();
        ctx.machine_coordinates_valid
            .store(true, std::sync::atomic::Ordering::Release);
        ctx.settings.lock().unwrap().display_language = "fr".to_owned();
        ctx.push_error("WARN test: project /Users/alice/secret.lzrproj".to_owned());
        ctx.push_connection_event(
            "ready",
            Some("/dev/cu.unique-device".to_owned()),
            Some(115_200),
            None,
            None,
        );
        *ctx.project.lock().unwrap() = Some(Project::new("Private artwork"));
        let mut progress = JobProgress::default();
        progress.state = JobState::Completed;
        progress.total_lines = 42;
        progress.buckets.push(JobProgressBucket {
            layer_id: "private-layer-id".to_owned(),
            cut_entry_id: "private-cut-id".to_owned(),
            segment_count: 12,
        });
        *ctx.last_terminal_job.lock().unwrap() = Some(DiagnosticTerminalJob {
            captured_at: "2026-05-14T12:34:56Z".to_owned(),
            age_ms: None,
            relevant_to_report: None,
            reason: "terminal_progress".to_owned(),
            progress: Some(progress),
            error: None,
            job_tick_loop_running: false,
            session_state: Some(SessionState::Ready),
            machine_run_state: Some(MachineRunState::Idle),
            job_console: vec![ConsoleEntry {
                timestamp: Utc::now(),
                direction: ConsoleDirection::Sent,
                content: "G1 X10 Y20".to_owned(),
            }],
            session_console: vec![],
        });

        let bundle = preview_feedback_report(&ctx, successful_job_compatibility_input()).unwrap();
        let terminal = bundle.terminal_job.as_ref().expect("completed job summary");

        assert!(bundle.system.locale.is_none());
        assert!(bundle.ports_detected.is_empty());
        assert!(bundle.connection_events.is_empty());
        assert_eq!(bundle.recent_serial, Default::default());
        assert!(bundle.recent_logs.is_empty());
        assert!(bundle.recent_panics.is_empty());
        assert!(bundle.project_metadata.is_none());
        assert!(!bundle.machine.machine_coordinates_valid);
        assert!(terminal.job_console.is_empty());
        assert!(terminal.session_console.is_empty());
        assert_eq!(
            terminal
                .progress
                .as_ref()
                .map(|progress| progress.total_lines),
            Some(42)
        );
        assert!(terminal.progress.as_ref().unwrap().buckets.is_empty());
        assert!(
            bundle
                .source_context
                .as_ref()
                .unwrap()
                .correlation_ts
                .is_none()
        );
    }

    #[test]
    fn successful_job_compatibility_report_rejects_project_attachment() {
        let ctx = ServiceContext::new();
        let mut input = successful_job_compatibility_input();
        input.include_project_file = true;

        let error = preview_feedback_report(&ctx, input).unwrap_err();

        assert!(error.to_string().contains("cannot be attached"));
    }

    #[test]
    fn saved_report_writes_json_without_project_attachment() {
        let _guard = crate::test_support::PersistTestGuard::new();
        let ctx = ServiceContext::new();
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("report.json");

        let saved = save_feedback_report(&ctx, bug_input(), &path).unwrap();

        assert_eq!(saved.path, path.to_string_lossy());
        let json: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).expect("valid report json");
        assert!(json.get("project_file_blob").is_none());
    }

    #[test]
    fn saved_report_writes_zip_with_project_sibling_when_attached() {
        let _guard = crate::test_support::PersistTestGuard::new();
        let ctx = ServiceContext::new();
        *ctx.project.lock().unwrap() = Some(Project::new("Attached"));
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("report.zip");
        let mut input = bug_input();
        input.include_project_file = true;

        save_feedback_report(&ctx, input, &path).unwrap();

        let file = fs::File::open(&path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let mut report_json = String::new();
        archive
            .by_name("report.json")
            .unwrap()
            .read_to_string(&mut report_json)
            .unwrap();
        assert!(archive.by_name("project.lzrproj").is_ok());
        assert!(!report_json.contains("project_file_blob"));
    }

    #[test]
    fn boundary_submit_body_fits_under_server_cap_without_attachment() {
        let ctx = ServiceContext::new();
        let transport = build_submit_request_for_transport(&ctx, bug_input()).unwrap();
        assert!(transport.body.len() <= MAX_SUBMIT_BODY_BYTES);
        assert_eq!(transport.bundle.kind, FeedbackKind::Bug);
    }

    #[test]
    fn panic_payload_is_capped_before_submit_serialization() {
        let ctx = ServiceContext::new();
        let base_ts = DateTime::parse_from_rfc3339("2026-05-14T12:34:56Z")
            .unwrap()
            .with_timezone(&Utc);
        let reports = (0..50)
            .map(|index| crate::context::StoredPanicReport {
                report: DiagnosticPanic {
                    ts: (base_ts - chrono::Duration::seconds(index)).to_rfc3339(),
                    thread: Some("main".to_owned()),
                    message: format!("panic {index}"),
                    location: None,
                    backtrace: Some("x".repeat(64 * 1024)),
                    app_version: "0.1.0".to_owned(),
                    os: "macOS".to_owned(),
                    build_target: "aarch64-apple-darwin".to_owned(),
                    git_sha: "abc123".to_owned(),
                },
                path: PathBuf::from(format!("/tmp/panic-{index}.json")),
            })
            .collect();
        ctx.set_panic_reports(reports);

        let bundle = preview_feedback_report(&ctx, bug_input()).unwrap();
        let panic_bytes = serde_json::to_vec(&bundle.recent_panics).unwrap();
        assert!(panic_bytes.len() <= MAX_PANIC_REPORT_BUNDLE_BYTES);
        assert!(bundle.recent_panics.len() < 50);

        let transport = build_submit_request_for_transport(&ctx, bug_input()).unwrap();
        assert!(transport.body.len() <= MAX_SUBMIT_BODY_BYTES);
        assert_eq!(
            transport.bundle.recent_panics.len(),
            bundle.recent_panics.len()
        );
    }

    #[test]
    fn connecting_session_states_remain_in_progress() {
        assert_eq!(
            session_state_to_diagnostic(SessionState::Connecting),
            DiagnosticSessionState::Connecting
        );
        assert_eq!(
            session_state_to_diagnostic(SessionState::TransportOpen),
            DiagnosticSessionState::Connecting
        );
        assert_eq!(
            session_state_to_diagnostic(SessionState::WaitingForBanner),
            DiagnosticSessionState::Connecting
        );
    }

    #[test]
    fn known_issues_match_profile_firmware_type() {
        let issues = known_issues_for(
            &DiagnosticMachine {
                connected: false,
                model: Some("Preset Test".to_owned()),
                profile_id: None,
                profile_name: Some("Preset Test".to_owned()),
                profile_preset_id: None,
                profile_preset_version: None,
                firmware_type: Some("grbl".to_owned()),
                controller_family: None,
                controller_model: None,
                transport_kind: None,
                transfer_mode: None,
                s_value_max: None,
                homing_enabled: None,
                use_constant_power: None,
                emit_s_every_g1: None,
                use_g0_for_overscan: None,
                bed_width_mm: None,
                bed_height_mm: None,
                workspace_origin: None,
                max_speed_mm_min: None,
                controller_travel_x_mm: None,
                controller_travel_y_mm: None,
                run_state: None,
                machine_position: None,
                work_position: None,
                machine_coordinates_valid: false,
                firmware_version: None,
                baud_rate: Some(115_200),
                port_name: None,
                port_vendor_id: None,
                port_product_id: None,
                session_state: DiagnosticSessionState::Disconnected,
                handshake_message: Some("Disconnected".to_owned()),
            },
            &[],
        );

        assert!(issues.iter().any(|issue| issue.code == "no_grbl_response"));
    }

    #[test]
    fn load_panic_reports_skips_oversized_files() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("large.json");
        fs::write(path, vec![b'x'; MAX_PANIC_REPORT_FILE_BYTES as usize + 1]).unwrap();

        let reports = load_panic_reports_from_dir(dir.path());

        assert!(reports.is_empty());
    }
}
