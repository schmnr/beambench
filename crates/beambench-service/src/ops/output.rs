use beambench_core::{
    FinishPosition, MachineProfile, Project, ProjectOptimization, ScanningOffsetEntry,
};
use beambench_grbl::GcodeConfig;
use beambench_planner::PlannerCalibration;

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
    use beambench_core::FinishPosition;

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
