use beambench_common::StartFromMode;
use beambench_common::machine::MachineStatus;
use beambench_core::{
    FinishPosition, MachineProfile, Project, ProjectOptimization, RotaryAxis, ScanningOffsetEntry,
};
use beambench_grbl::{GcodeConfig, RotaryGcodeConfig};
use beambench_planner::{ExecutionPlan, PlanSegment, PlannerCalibration, ScanAxis};

/// Build a `GcodeConfig` from the persisted project optimization and the
/// active machine profile. This is the single source of truth for
/// constructing G-code emitter configuration.
///
/// The signature deliberately narrows to `&ProjectOptimization` rather
/// than widening to also take `&OptimizationRuntime`: runtime-only state is
/// consumed upstream by the planner's offset pass, not by G-code output.
pub fn build_gcode_config(_opt: &ProjectOptimization, profile: &MachineProfile) -> GcodeConfig {
    let mut scanning_offsets = profile.scanning_offsets.clone();
    normalize_scanning_offsets(&mut scanning_offsets);

    GcodeConfig {
        // The planner already materializes finish-position travel into the
        // execution plan so preview, bounds, and streaming all see the same
        // path. The G-code serializer should only serialize that plan, not add
        // a second finish move in the postamble.
        finish_position: FinishPosition::DontMove,
        finish_x: None,
        finish_y: None,
        use_constant_power: profile.use_constant_power,
        emit_s_every_g1: profile.emit_s_every_g1,
        s_value_max: profile.s_value_max,
        use_g0_for_overscan: profile.use_g0_for_overscan,
        gcode_prefix: profile.job_header_gcode.clone(),
        gcode_suffix: profile.job_footer_gcode.clone(),
        air_assist_on_gcode: profile.air_assist_on_gcode.clone(),
        air_assist_off_gcode: profile.air_assist_off_gcode.clone(),
        air_assist_on_delay_ms: profile.air_assist_on_delay_ms,
        transfer_mode: profile.transfer_mode,
        z_moves_enabled: profile.supports_z_moves,
        z_move_feed_mm_min: profile.z_move_feed_mm_min,
        scanning_offsets: scanning_offsets
            .iter()
            .map(|e| (e.speed_mm_min, e.offset_mm))
            .collect(),
        enable_scanning_offset: profile.enable_scanning_offset,
        ..GcodeConfig::default()
    }
}

/// Add project-scoped cut-entry metadata that is needed at G-code emission time.
pub fn apply_project_gcode_metadata(config: &mut GcodeConfig, project: &Project) {
    config.air_assist_cut_entry_ids = project
        .layers
        .iter()
        .flat_map(|layer| layer.entries.iter())
        .filter(|entry| entry.air_assist)
        .map(|entry| entry.id.to_string())
        .collect();
    config.z_base_mm = project.material_height_mm.unwrap_or(0.0);
    config.z_offset_cut_entry_ids = project
        .layers
        .iter()
        .flat_map(|layer| layer.entries.iter())
        .map(|entry| (entry.id.to_string(), entry.z_offset_mm))
        .collect();
}

/// Attach the live coordinate anchor required by rotary output. Rotary jobs
/// deliberately require Current Position mode so a dedicated Z rotary and an
/// X/Y substitution both start from the controller's reported work position
/// without rewriting persistent firmware or work offsets.
pub fn apply_rotary_runtime(
    config: &mut GcodeConfig,
    project: &Project,
    profile: &MachineProfile,
    status: &MachineStatus,
) -> Result<(), String> {
    if !profile.rotary_enabled {
        config.rotary = None;
        return Ok(());
    }
    if project.start_from != StartFromMode::CurrentPosition {
        return Err(
            "Rotary mode requires Start From Current Position so the attachment can be anchored safely"
                .to_string(),
        );
    }
    let command_scale = profile.rotary_command_scale().ok_or_else(|| {
        "Rotary calibration is invalid; check mm per rotation and diameter values".to_string()
    })?;
    if profile.rotary_axis == RotaryAxis::Z {
        let uses_job_z_offsets = config
            .z_offset_cut_entry_ids
            .iter()
            .any(|(_, offset)| offset.abs() > f64::EPSILON);
        if uses_job_z_offsets {
            return Err("Z-axis rotary mode cannot be combined with layer Z offsets".to_string());
        }
        config.z_moves_enabled = false;
    }
    let work = &status.work_position;
    config.rotary = Some(RotaryGcodeConfig {
        axis: profile.rotary_axis,
        command_scale,
        surface_origin_x_mm: work.x,
        surface_origin_y_mm: work.y,
        axis_origin_mm: match profile.rotary_axis {
            RotaryAxis::X => work.x,
            RotaryAxis::Y => work.y,
            RotaryAxis::Z => work.z,
        },
    });
    Ok(())
}

fn rotary_feed_factor(dx: f64, dy: f64, profile: &MachineProfile, command_scale: f64) -> f64 {
    let surface_distance = dx.hypot(dy);
    if surface_distance <= f64::EPSILON {
        return 1.0;
    }
    let (mapped_dx, mapped_dy, mapped_dz) = match profile.rotary_axis {
        RotaryAxis::X => (dx * command_scale, dy, 0.0),
        RotaryAxis::Y => (dx, dy * command_scale, 0.0),
        RotaryAxis::Z => (dx, 0.0, dy * command_scale),
    };
    mapped_dx.hypot(mapped_dy).hypot(mapped_dz) / surface_distance
}

fn maximum_path_rotary_feed(
    points: &[beambench_common::geometry::Point2D],
    speed: f64,
    closed: bool,
    profile: &MachineProfile,
    command_scale: f64,
) -> f64 {
    let mut maximum = points.windows(2).fold(0.0_f64, |current, pair| {
        current.max(
            speed
                * rotary_feed_factor(
                    pair[1].x - pair[0].x,
                    pair[1].y - pair[0].y,
                    profile,
                    command_scale,
                ),
        )
    });
    if closed && points.len() > 1 {
        let first = points[0];
        let last = points[points.len() - 1];
        maximum = maximum.max(
            speed * rotary_feed_factor(first.x - last.x, first.y - last.y, profile, command_scale),
        );
    }
    maximum
}

/// Highest controller feed that rotary compensation will emit for this plan.
/// The plan stays in surface millimetres, while the mapped controller axis may
/// need to travel much farther for the same surface motion.
pub fn maximum_rotary_command_feed(plan: &ExecutionPlan, profile: &MachineProfile) -> Option<f64> {
    if !profile.rotary_enabled {
        return None;
    }
    let command_scale = profile.rotary_command_scale()?;
    let mut maximum = 0.0_f64;

    for segment in &plan.segments {
        match segment {
            PlanSegment::Vector {
                polyline,
                closed,
                speed_mm_min,
                ..
            } => {
                maximum = maximum.max(maximum_path_rotary_feed(
                    polyline,
                    *speed_mm_min,
                    *closed,
                    profile,
                    command_scale,
                ));
            }
            PlanSegment::Frame {
                path, speed_mm_min, ..
            } => {
                maximum = maximum.max(maximum_path_rotary_feed(
                    path,
                    *speed_mm_min,
                    false,
                    profile,
                    command_scale,
                ));
            }
            PlanSegment::Raster {
                scanlines,
                speed_mm_min,
                scan_angle_deg,
                scan_axis,
                ..
            } if scanlines.iter().any(|line| !line.runs.is_empty()) => {
                let orthogonal = scan_angle_deg.abs() < 0.5
                    || (scan_angle_deg.abs() - 90.0).abs() < 0.5
                    || (scan_angle_deg.abs() - 180.0).abs() < 0.5
                    || (scan_angle_deg.abs() - 270.0).abs() < 0.5
                    || (scan_angle_deg.abs() - 360.0).abs() < 0.5;
                let (dx, dy) = if orthogonal {
                    match scan_axis {
                        ScanAxis::Horizontal => (1.0, 0.0),
                        ScanAxis::Vertical => (0.0, 1.0),
                    }
                } else {
                    let radians = scan_angle_deg.to_radians();
                    (radians.cos(), radians.sin())
                };
                maximum =
                    maximum.max(speed_mm_min * rotary_feed_factor(dx, dy, profile, command_scale));
            }
            _ => {}
        }
    }

    Some(maximum)
}

pub fn validate_rotary_feed_limit(
    plan: &ExecutionPlan,
    profile: &MachineProfile,
) -> Result<(), String> {
    let Some(required_feed) = maximum_rotary_command_feed(plan, profile) else {
        return Ok(());
    };
    if required_feed <= profile.max_speed_mm_min + f64::EPSILON {
        return Ok(());
    }
    Err(format!(
        "Rotary feed compensation requires up to {required_feed:.0} mm/min, above the machine profile limit of {:.0} mm/min. Lower the artwork speed or correct the machine maximum speed.",
        profile.max_speed_mm_min
    ))
}

/// Build `PlannerCalibration` from the active machine profile.
pub fn build_planner_calibration(profile: &MachineProfile) -> PlannerCalibration {
    PlannerCalibration {
        dot_width_mm: profile.dot_width_mm,
        enable_dot_width: profile.enable_dot_width,
    }
}

/// Normalize scanning-offset entries: remove invalid speeds, sort by speed, deduplicate.
pub fn normalize_scanning_offsets(entries: &mut Vec<ScanningOffsetEntry>) {
    entries.retain(|e| e.speed_mm_min > 0.0);
    entries.sort_by(|a, b| {
        a.speed_mm_min
            .partial_cmp(&b.speed_mm_min)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    entries.dedup_by(|a, b| (a.speed_mm_min - b.speed_mm_min).abs() < 0.001);
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::geometry::{Bounds, Point2D};
    use beambench_common::machine::{MachinePosition, MachineRunState};
    use beambench_core::{FinishPosition, RotaryType};
    use chrono::Utc;
    use uuid::Uuid;

    fn plan_with_segments(segments: Vec<PlanSegment>) -> ExecutionPlan {
        ExecutionPlan {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            revision_hash: "test".to_string(),
            created_at: Utc::now(),
            bounds: Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            total_distance_mm: 0.0,
            estimated_duration_secs: 0.0,
            segments,
            layer_order: vec![],
            warnings: vec![],
            failed_entries: vec![],
        }
    }

    #[test]
    fn build_gcode_config_maps_profile_fields() {
        let opt = ProjectOptimization::default();
        let mut profile = MachineProfile::default();
        profile.use_constant_power = true;
        profile.emit_s_every_g1 = true;
        profile.s_value_max = 500;
        profile.use_g0_for_overscan = false;
        profile.supports_z_moves = true;
        profile.z_move_feed_mm_min = 333.0;
        profile.air_assist_on_gcode = "M8".to_string();
        profile.air_assist_on_delay_ms = 300;
        profile.scanning_offsets = vec![
            ScanningOffsetEntry {
                speed_mm_min: 1000.0,
                offset_mm: 0.1,
            },
            ScanningOffsetEntry {
                speed_mm_min: 2000.0,
                offset_mm: 0.2,
            },
        ];
        profile.enable_scanning_offset = true;

        let config = build_gcode_config(&opt, &profile);

        assert!(config.use_constant_power);
        assert!(config.emit_s_every_g1);
        assert_eq!(config.s_value_max, 500);
        assert!(!config.use_g0_for_overscan);
        assert!(config.z_moves_enabled);
        assert_eq!(config.z_move_feed_mm_min, 333.0);
        assert_eq!(config.air_assist_on_gcode, "M8");
        assert_eq!(config.air_assist_on_delay_ms, 300);
        assert!(config.enable_scanning_offset);
        assert_eq!(config.scanning_offsets.len(), 2);
        assert_eq!(config.scanning_offsets[0], (1000.0, 0.1));
        assert_eq!(config.scanning_offsets[1], (2000.0, 0.2));
    }

    #[test]
    fn apply_rotary_runtime_anchors_z_axis_to_live_work_position() {
        let mut profile = MachineProfile {
            rotary_enabled: true,
            rotary_axis: RotaryAxis::Z,
            rotary_mm_per_rotation: 50.0,
            rotary_roller_diameter_mm: 10.0,
            ..MachineProfile::default()
        };
        profile.supports_z_moves = true;
        let mut project = Project::new("Rotary");
        project.start_from = StartFromMode::CurrentPosition;
        let status = MachineStatus {
            run_state: MachineRunState::Idle,
            machine_position: MachinePosition {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            work_position: MachinePosition {
                x: 12.0,
                y: 34.0,
                z: 5.0,
            },
            ..MachineStatus::default()
        };
        let mut config = build_gcode_config(&project.optimization, &profile);

        apply_rotary_runtime(&mut config, &project, &profile, &status).unwrap();

        let rotary = config.rotary.expect("rotary mapping");
        assert_eq!(rotary.axis, RotaryAxis::Z);
        assert_eq!(rotary.surface_origin_x_mm, 12.0);
        assert_eq!(rotary.surface_origin_y_mm, 34.0);
        assert_eq!(rotary.axis_origin_mm, 5.0);
        assert!(!config.z_moves_enabled);
    }

    #[test]
    fn apply_rotary_runtime_requires_current_position_mode() {
        let profile = MachineProfile {
            rotary_enabled: true,
            ..MachineProfile::default()
        };
        let project = Project::new("Rotary");
        let status = MachineStatus::default();
        let mut config = GcodeConfig::default();

        let error = apply_rotary_runtime(&mut config, &project, &profile, &status).unwrap_err();
        assert!(error.contains("Start From Current Position"));
    }

    #[test]
    fn rotary_feed_limit_checks_the_compensated_controller_rate() {
        let profile = MachineProfile {
            rotary_enabled: true,
            rotary_type: RotaryType::Chuck,
            rotary_axis: RotaryAxis::Y,
            rotary_mm_per_rotation: 20.0 * std::f64::consts::PI,
            rotary_object_diameter_mm: 10.0,
            max_speed_mm_min: 1500.0,
            ..MachineProfile::default()
        };
        let plan = plan_with_segments(vec![PlanSegment::Vector {
            polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(0.0, 10.0)],
            closed: false,
            power_percent: 50.0,
            speed_mm_min: 1000.0,
            layer_id: "layer".to_string(),
            cut_entry_id: "entry".to_string(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        }]);

        assert_eq!(maximum_rotary_command_feed(&plan, &profile), Some(2000.0));
        let error = validate_rotary_feed_limit(&plan, &profile).unwrap_err();
        assert!(error.contains("2000 mm/min"));
        assert!(error.contains("1500 mm/min"));
    }

    #[test]
    fn apply_project_gcode_metadata_maps_air_assist_entries() {
        use beambench_core::layer::{CutEntry, Layer, OperationType};

        let mut project = beambench_core::Project::new("Air");
        let mut layer = Layer::new("Air", OperationType::Line);
        let mut air_entry = CutEntry::new(OperationType::Line);
        air_entry.air_assist = true;
        let air_id = air_entry.id.to_string();
        layer.entries = vec![air_entry, CutEntry::new(OperationType::Line)];
        project.layers.push(layer);

        let mut config = GcodeConfig::default();
        apply_project_gcode_metadata(&mut config, &project);

        assert_eq!(config.air_assist_cut_entry_ids, vec![air_id]);
    }

    #[test]
    fn planner_segments_with_real_cut_entry_ids_emit_profile_air_command() {
        use beambench_common::geometry::{Bounds, Point2D};
        use beambench_core::layer::{Layer, OperationType};
        use beambench_core::object::{ObjectData, ProjectObject, ShapeKind};
        use beambench_grbl::generate_gcode;
        use beambench_planner::build_plan;

        let mut project = beambench_core::Project::new("Air E2E");
        let mut layer = Layer::new("Air", OperationType::Line);
        layer.primary_entry_mut().air_assist = true;
        let layer_id = layer.id;
        project.layers.push(layer);
        project.add_object(ProjectObject::new(
            "rect",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(20.0, 20.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));

        let plan = build_plan(&project).expect("plan");
        assert!(
            plan.segments.iter().any(|segment| matches!(
                segment,
                beambench_planner::PlanSegment::Vector { cut_entry_id, .. }
                    if !cut_entry_id.is_empty()
            )),
            "planner should populate real cut-entry ids"
        );

        let mut profile = MachineProfile::default();
        profile.air_assist_on_gcode = "M8".to_string();
        let mut config = build_gcode_config(&project.optimization, &profile);
        apply_project_gcode_metadata(&mut config, &project);
        let gcode = generate_gcode(&plan, &config).expect("gcode");

        assert!(gcode.iter().any(|line| line == "M8"));
    }

    #[test]
    fn bottom_left_workspace_gcode_uses_machine_y_coordinates() {
        use beambench_common::geometry::{Bounds, Point2D};
        use beambench_core::WorkspaceOrigin;
        use beambench_core::layer::{Layer, OperationType};
        use beambench_core::object::{ObjectData, ProjectObject, ShapeKind};
        use beambench_grbl::generate_gcode;
        use beambench_planner::build_plan;

        let mut project = beambench_core::Project::new("Bottom-left G-code");
        project.workspace.origin = WorkspaceOrigin::BottomLeft;
        project.workspace.bed_height_mm = 300.0;

        let layer = Layer::new("Line", OperationType::Line);
        let layer_id = layer.id;
        project.layers.push(layer);
        project.add_object(ProjectObject::new(
            "lower-visual-rect",
            layer_id,
            Bounds::new(Point2D::new(10.0, 250.0), Point2D::new(20.0, 260.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));

        let profile = MachineProfile::default();
        let plan = build_plan(&project).expect("plan");
        let mut config = build_gcode_config(&project.optimization, &profile);
        apply_project_gcode_metadata(&mut config, &project);
        let gcode = generate_gcode(&plan, &config).expect("gcode");
        let y_lines = gcode
            .iter()
            .filter(|line| line.contains('Y'))
            .cloned()
            .collect::<Vec<_>>();

        assert!(
            y_lines
                .iter()
                .any(|line| line.contains("Y40.000") || line.contains("Y50.000")),
            "expected machine-space Y around 40..50, got {y_lines:?}",
        );
        assert!(
            !y_lines
                .iter()
                .any(|line| line.contains("Y250.000") || line.contains("Y260.000")),
            "canvas-space Y leaked into G-code: {y_lines:?}",
        );
    }

    #[test]
    fn normal_layer_z_offsets_emit_profile_feed_once_per_target_change() {
        use beambench_common::geometry::{Bounds, Point2D};
        use beambench_core::layer::{Layer, OperationType};
        use beambench_core::object::{ObjectData, ProjectObject, ShapeKind};
        use beambench_grbl::generate_gcode;
        use beambench_planner::build_plan;

        let mut project = beambench_core::Project::new("Layer Z");

        let mut offset_layer = Layer::new("Offset", OperationType::Line);
        offset_layer.primary_entry_mut().z_offset_mm = 5.0;
        offset_layer
            .primary_entry_mut()
            .vector_settings
            .as_mut()
            .expect("vector settings")
            .passes = 2;
        let offset_layer_id = offset_layer.id;
        project.layers.push(offset_layer);
        project.add_object(ProjectObject::new(
            "offset-rect",
            offset_layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(20.0, 20.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));

        let base_layer = Layer::new("Base", OperationType::Line);
        let base_layer_id = base_layer.id;
        project.layers.push(base_layer);
        project.add_object(ProjectObject::new(
            "base-rect",
            base_layer_id,
            Bounds::new(Point2D::new(30.0, 10.0), Point2D::new(40.0, 20.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));

        let mut profile = MachineProfile::default();
        profile.supports_z_moves = true;
        profile.z_move_feed_mm_min = 333.0;

        let plan = build_plan(&project).expect("plan");
        let mut config = build_gcode_config(&project.optimization, &profile);
        apply_project_gcode_metadata(&mut config, &project);
        let gcode = generate_gcode(&plan, &config).expect("gcode");
        let z_lines: Vec<&str> = gcode
            .iter()
            .filter(|line| line.starts_with("G1 Z"))
            .map(String::as_str)
            .collect();

        assert_eq!(z_lines, vec!["G1 Z5.000 F333", "G1 Z0.000 F333"]);
    }

    #[test]
    fn non_z_profiles_ignore_layer_z_offsets() {
        use beambench_common::geometry::{Bounds, Point2D};
        use beambench_core::layer::{Layer, OperationType};
        use beambench_core::object::{ObjectData, ProjectObject, ShapeKind};
        use beambench_grbl::generate_gcode;
        use beambench_planner::build_plan;

        let mut project = beambench_core::Project::new("No Z");
        let mut layer = Layer::new("Offset", OperationType::Line);
        layer.primary_entry_mut().z_offset_mm = 5.0;
        let layer_id = layer.id;
        project.layers.push(layer);
        project.add_object(ProjectObject::new(
            "rect",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(20.0, 20.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));

        let mut profile = MachineProfile::default();
        profile.supports_z_moves = false;
        profile.z_move_feed_mm_min = 333.0;

        let plan = build_plan(&project).expect("plan");
        let mut config = build_gcode_config(&project.optimization, &profile);
        apply_project_gcode_metadata(&mut config, &project);
        let gcode = generate_gcode(&plan, &config).expect("gcode");

        assert!(!gcode.iter().any(|line| line.starts_with("G1 Z")));
    }

    #[test]
    fn all_zero_layer_z_offsets_do_not_emit_z_moves() {
        use beambench_grbl::generate_gcode;
        use beambench_planner::{OptimizationRuntime, PlannerInput, build_plan_with_input};

        let mut project = build_regression_project();
        project.material_height_mm = Some(7.0);
        let mut profile = MachineProfile::default();
        profile.supports_z_moves = true;
        profile.z_move_feed_mm_min = 333.0;

        let input = PlannerInput::new(
            project.optimization.clone(),
            OptimizationRuntime::default(),
            PlannerCalibration::default(),
        );
        let plan = build_plan_with_input(&project, &input).expect("plan");
        let mut config = build_gcode_config(&project.optimization, &profile);
        apply_project_gcode_metadata(&mut config, &project);
        let gcode = generate_gcode(&plan, &config).expect("gcode");

        assert!(!gcode.iter().any(|line| line.starts_with("G1 Z")));
    }

    #[test]
    fn build_gcode_config_leaves_finish_position_to_planner() {
        let mut opt = ProjectOptimization::default();
        opt.finish_position = FinishPosition::CustomXY;
        opt.finish_x = Some(42.0);
        opt.finish_y = Some(99.0);
        let profile = MachineProfile::default();

        let config = build_gcode_config(&opt, &profile);

        assert_eq!(config.finish_position, FinishPosition::DontMove);
        assert_eq!(config.finish_x, None);
        assert_eq!(config.finish_y, None);
    }

    #[test]
    fn build_gcode_config_normalizes_scanning_offsets_from_profile() {
        let opt = ProjectOptimization::default();
        let mut profile = MachineProfile::default();
        profile.enable_scanning_offset = true;
        profile.scanning_offsets = vec![
            ScanningOffsetEntry {
                speed_mm_min: 3000.0,
                offset_mm: 0.3,
            },
            ScanningOffsetEntry {
                speed_mm_min: 0.0,
                offset_mm: 0.9,
            },
            ScanningOffsetEntry {
                speed_mm_min: 1000.0,
                offset_mm: 0.1,
            },
            ScanningOffsetEntry {
                speed_mm_min: 1000.0,
                offset_mm: 0.2,
            },
        ];

        let config = build_gcode_config(&opt, &profile);

        assert_eq!(config.scanning_offsets, vec![(1000.0, 0.1), (3000.0, 0.3)]);
    }

    #[test]
    fn build_planner_calibration_from_profile() {
        let mut profile = MachineProfile::default();
        profile.dot_width_mm = 0.15;
        profile.enable_dot_width = true;

        let cal = build_planner_calibration(&profile);

        assert_eq!(cal.dot_width_mm, 0.15);
        assert!(cal.enable_dot_width);
    }

    #[test]
    fn build_planner_calibration_default_disabled() {
        let profile = MachineProfile::default();
        let cal = build_planner_calibration(&profile);

        assert_eq!(cal.dot_width_mm, 0.0);
        assert!(!cal.enable_dot_width);
    }

    #[test]
    fn normalize_scanning_offsets_sorts_and_deduplicates() {
        let mut entries = vec![
            ScanningOffsetEntry {
                speed_mm_min: 3000.0,
                offset_mm: 0.3,
            },
            ScanningOffsetEntry {
                speed_mm_min: 1000.0,
                offset_mm: 0.1,
            },
            ScanningOffsetEntry {
                speed_mm_min: 1000.0,
                offset_mm: 0.15,
            },
            ScanningOffsetEntry {
                speed_mm_min: 2000.0,
                offset_mm: 0.2,
            },
        ];

        normalize_scanning_offsets(&mut entries);

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].speed_mm_min, 1000.0);
        assert_eq!(entries[1].speed_mm_min, 2000.0);
        assert_eq!(entries[2].speed_mm_min, 3000.0);
    }

    #[test]
    fn normalize_scanning_offsets_removes_invalid_speeds() {
        let mut entries = vec![
            ScanningOffsetEntry {
                speed_mm_min: 0.0,
                offset_mm: 0.1,
            },
            ScanningOffsetEntry {
                speed_mm_min: -100.0,
                offset_mm: 0.2,
            },
            ScanningOffsetEntry {
                speed_mm_min: 500.0,
                offset_mm: 0.05,
            },
        ];

        normalize_scanning_offsets(&mut entries);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].speed_mm_min, 500.0);
    }

    #[test]
    fn normalize_scanning_offsets_empty_table() {
        let mut entries = vec![];
        normalize_scanning_offsets(&mut entries);
        assert!(entries.is_empty());
    }

    /// Build a simple project with vector segments for regression testing.
    fn build_regression_project() -> beambench_core::Project {
        use beambench_common::geometry::{Bounds, Point2D};
        use beambench_core::layer::{Layer, OperationType};
        use beambench_core::object::{ObjectData, ProjectObject, ShapeKind};

        let mut project = beambench_core::Project::new("RegressionTest");
        let layer = Layer::new("Lines", OperationType::Line);
        let layer_id = layer.id;
        project.layers.push(layer);

        for (i, (x, y)) in [(10.0, 10.0), (100.0, 50.0)].iter().enumerate() {
            project.add_object(ProjectObject::new(
                &format!("rect{i}"),
                layer_id,
                Bounds::new(Point2D::new(*x, *y), Point2D::new(x + 20.0, y + 20.0)),
                ObjectData::Shape {
                    kind: ShapeKind::Rectangle,
                    width: 20.0,
                    height: 20.0,
                    corner_radius: 0.0,
                },
            ));
        }
        project
    }

    #[test]
    fn helper_config_does_not_duplicate_planner_finish_move() {
        use beambench_grbl::generate_gcode;
        use beambench_planner::{OptimizationRuntime, PlannerInput, build_plan_with_input};

        let project = build_regression_project();
        let opt = ProjectOptimization::default();
        let profile = MachineProfile::default();

        let input = PlannerInput::new(
            opt.clone(),
            OptimizationRuntime::default(),
            PlannerCalibration::default(),
        );
        let plan = build_plan_with_input(&project, &input).expect("plan build");

        let config_from_helper = build_gcode_config(&opt, &profile);
        let gcode = generate_gcode(&plan, &config_from_helper).expect("gcode");
        let origin_returns = gcode
            .iter()
            .filter(|line| line.starts_with("G0") && line.contains("X0") && line.contains("Y0"))
            .count();

        assert_eq!(
            origin_returns,
            1,
            "Planner already includes the finish move; helper config must not add a postamble duplicate:\n{}",
            gcode.join("\n")
        );
    }

    #[test]
    fn default_calibration_produces_identical_plan() {
        use beambench_planner::{OptimizationRuntime, PlannerInput, build_plan_with_input};

        let project = build_regression_project();
        let opt = ProjectOptimization::default();

        let plan_no_cal = build_plan_with_input(
            &project,
            &PlannerInput::new(
                opt.clone(),
                OptimizationRuntime::default(),
                PlannerCalibration::default(),
            ),
        )
        .expect("plan no cal");
        let plan_with_cal = build_plan_with_input(
            &project,
            &PlannerInput::new(
                opt.clone(),
                OptimizationRuntime::default(),
                PlannerCalibration {
                    dot_width_mm: 0.0,
                    enable_dot_width: false,
                },
            ),
        )
        .expect("plan with cal");

        assert_eq!(
            plan_no_cal.segments.len(),
            plan_with_cal.segments.len(),
            "Default calibration should produce same number of segments"
        );

        // Verify G-code is identical
        use beambench_grbl::generate_gcode;
        let config = GcodeConfig::default();
        let gcode_a = generate_gcode(&plan_no_cal, &config).expect("gcode A");
        let gcode_b = generate_gcode(&plan_with_cal, &config).expect("gcode B");
        assert_eq!(
            gcode_a, gcode_b,
            "Default calibration should produce identical G-code"
        );
    }

    #[test]
    fn default_config_matches_gcode_config_default() {
        let opt = ProjectOptimization::default();
        let profile = MachineProfile::default();
        let config = build_gcode_config(&opt, &profile);
        let default_config = GcodeConfig::default();

        assert_eq!(config.use_constant_power, default_config.use_constant_power);
        assert_eq!(config.emit_s_every_g1, default_config.emit_s_every_g1);
        assert_eq!(config.s_value_max, default_config.s_value_max);
        assert_eq!(
            config.use_g0_for_overscan,
            default_config.use_g0_for_overscan
        );
        assert_eq!(
            config.enable_scanning_offset,
            default_config.enable_scanning_offset
        );
        assert_eq!(
            config.scanning_offsets.len(),
            default_config.scanning_offsets.len()
        );
        assert_eq!(config.finish_position, FinishPosition::DontMove);
        assert_eq!(config.finish_x, None);
        assert_eq!(config.finish_y, None);
    }
}
