//! Fixture-load validation tests.
//!
//! For each benchmark fixture: load → build plan → assert plan structure → generate G-code.

use beambench_core::{Project, WorkspaceOrigin};
use beambench_grbl::{GcodeConfig, generate_gcode};
use beambench_planner::ExecutionPlan;
use beambench_planner::{PlanSegment, build_plan};
use beambench_project::load_project;
use std::path::PathBuf;

const GCODE_BOUNDS_EPSILON_MM: f64 = 0.001;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/benchmarks")
        .join(format!("{name}.lzrproj"))
}

fn default_gcode_config() -> GcodeConfig {
    GcodeConfig::default()
}

/// Helper: assert that a plan has at least one segment of a given type.
fn has_raster_segment(plan: &beambench_planner::ExecutionPlan) -> bool {
    plan.segments
        .iter()
        .any(|s| matches!(s, PlanSegment::Raster { .. }))
}

fn has_vector_segment(plan: &beambench_planner::ExecutionPlan) -> bool {
    plan.segments
        .iter()
        .any(|s| matches!(s, PlanSegment::Vector { .. }))
}

fn expected_machine_angle(project: &Project, design_angle: f64) -> f64 {
    if project.workspace.origin == WorkspaceOrigin::BottomLeft {
        -design_angle
    } else {
        design_angle
    }
}

fn assert_plan_bounds_fit_workspace(project: &Project, plan: &ExecutionPlan) {
    assert!(
        plan.bounds.min.x >= -GCODE_BOUNDS_EPSILON_MM
            && plan.bounds.min.y >= -GCODE_BOUNDS_EPSILON_MM
            && plan.bounds.max.x <= project.workspace.bed_width_mm + GCODE_BOUNDS_EPSILON_MM
            && plan.bounds.max.y <= project.workspace.bed_height_mm + GCODE_BOUNDS_EPSILON_MM,
        "Plan bounds should fit within {}x{}mm workspace, got min=({}, {}) max=({}, {})",
        project.workspace.bed_width_mm,
        project.workspace.bed_height_mm,
        plan.bounds.min.x,
        plan.bounds.min.y,
        plan.bounds.max.x,
        plan.bounds.max.y,
    );
}

fn parse_gcode_word(line: &str, prefix: char) -> Option<f64> {
    line.split_whitespace()
        .find_map(|part| part.strip_prefix(prefix)?.parse::<f64>().ok())
}

fn assert_laser_on_moves_within_workspace(project: &Project, gcode: &[String], fixture_name: &str) {
    let mut laser_enabled = false;
    let mut s_value = 0.0_f64;
    let mut current_x: Option<f64> = None;
    let mut current_y: Option<f64> = None;

    for line in gcode {
        if line.split_whitespace().any(|part| part == "M5") {
            laser_enabled = false;
            s_value = 0.0;
        }

        if line
            .split_whitespace()
            .any(|part| part == "M3" || part == "M4")
        {
            laser_enabled = true;
        }

        if let Some(s) = parse_gcode_word(line, 'S') {
            s_value = s;
        }

        if let Some(x) = parse_gcode_word(line, 'X') {
            current_x = Some(x);
        }
        if let Some(y) = parse_gcode_word(line, 'Y') {
            current_y = Some(y);
        }

        let is_motion = line.starts_with("G0 ") || line.starts_with("G1 ");
        if is_motion
            && laser_enabled
            && s_value > 0.0
            && let (Some(x), Some(y)) = (current_x, current_y)
        {
            assert!(
                x >= -GCODE_BOUNDS_EPSILON_MM
                    && x <= project.workspace.bed_width_mm + GCODE_BOUNDS_EPSILON_MM
                    && y >= -GCODE_BOUNDS_EPSILON_MM
                    && y <= project.workspace.bed_height_mm + GCODE_BOUNDS_EPSILON_MM,
                "{fixture_name} laser-on move outside {}x{}mm workspace: {line}",
                project.workspace.bed_width_mm,
                project.workspace.bed_height_mm,
            );
        }
    }
}

// ---------- Fixture 1: Grayscale Wedge ----------

#[test]
fn grayscale_wedge_loads_and_plans() {
    let project = load_project(&fixture_path("grayscale-wedge")).unwrap();
    assert_eq!(project.layers.len(), 1);
    assert_eq!(project.objects.len(), 1);
    assert_eq!(project.assets.len(), 1);

    let plan = build_plan(&project).unwrap();
    assert!(!plan.segments.is_empty(), "Plan should have segments");
    assert!(
        has_raster_segment(&plan),
        "Grayscale wedge should produce raster segments"
    );
    assert_plan_bounds_fit_workspace(&project, &plan);

    let gcode = generate_gcode(&plan, &default_gcode_config()).unwrap();
    assert!(!gcode.is_empty(), "G-code should be non-empty");
    assert!(
        gcode.iter().any(|l| l.contains("M4")),
        "Should contain M4 (laser on)"
    );
    assert!(
        gcode.iter().any(|l| l.contains("G1")),
        "Should contain G1 (linear move)"
    );
}

// ---------- Fixture 2: Binary Raster Overscan ----------

#[test]
fn binary_raster_overscan_loads_and_plans() {
    let project = load_project(&fixture_path("binary-raster-overscan")).unwrap();
    assert_eq!(project.layers.len(), 1);
    assert_eq!(project.objects.len(), 1);

    let plan = build_plan(&project).unwrap();
    assert!(!plan.segments.is_empty());
    assert!(
        has_raster_segment(&plan),
        "Binary raster should produce raster segments"
    );

    // Check overscan is reflected in plan bounds being wider than object bounds
    let raster_seg = plan
        .segments
        .iter()
        .find(|s| matches!(s, PlanSegment::Raster { .. }));
    assert!(raster_seg.is_some(), "Should have a Raster segment");
    if let Some(PlanSegment::Raster { overscan_mm, .. }) = raster_seg {
        assert!(*overscan_mm > 0.0, "Overscan should be positive");
    }

    let gcode = generate_gcode(&plan, &default_gcode_config()).unwrap();
    assert!(!gcode.is_empty());
}

// ---------- Fixture 3: Dense Short-Segment Vector ----------

#[test]
fn dense_short_segment_vector_loads_and_plans() {
    let project = load_project(&fixture_path("dense-short-segment-vector")).unwrap();
    assert_eq!(project.layers.len(), 1);
    assert_eq!(
        project.objects.len(),
        50,
        "Should have 50 rectangle objects"
    );

    let plan = build_plan(&project).unwrap();
    assert!(!plan.segments.is_empty());
    assert!(
        has_vector_segment(&plan),
        "Dense vector should produce vector segments"
    );

    // Count vector segments — should be at least 50 (one per rectangle, possibly more due to sides)
    let vector_count = plan
        .segments
        .iter()
        .filter(|s| matches!(s, PlanSegment::Vector { .. }))
        .count();
    assert!(
        vector_count >= 50,
        "Should have at least 50 vector segments, got {vector_count}"
    );

    let gcode = generate_gcode(&plan, &default_gcode_config()).unwrap();
    assert!(!gcode.is_empty());
    assert!(
        gcode.len() > 100,
        "Dense vector should produce substantial G-code"
    );
}

// ---------- Fixture 4: Scan-Angle Fill ----------

#[test]
fn scan_angle_fill_loads_and_plans() {
    let project = load_project(&fixture_path("scan-angle-fill")).unwrap();
    assert_eq!(project.layers.len(), 1);
    assert_eq!(project.objects.len(), 1);
    let layer = &project.layers[0];
    let raster_settings = layer
        .primary_entry()
        .raster_settings
        .as_ref()
        .expect("scan-angle-fill fixture should have raster settings");
    assert!(
        (raster_settings.scan_angle - 45.0).abs() < 0.5,
        "scan-angle-fill fixture should exercise a non-zero angle, got {}",
        raster_settings.scan_angle,
    );

    let plan = build_plan(&project).unwrap();
    assert!(!plan.segments.is_empty());
    // Fill operations produce raster segments (scanline fill)
    assert!(
        has_raster_segment(&plan),
        "Fill should produce raster segments"
    );
    let expected_angle = expected_machine_angle(&project, 45.0);
    assert!(
        plan.segments.iter().any(|segment| matches!(
            segment,
            PlanSegment::Raster { scan_angle_deg, .. } if (*scan_angle_deg - expected_angle).abs() < 1.0
        )),
        "scan-angle-fill plan should preserve reflected machine-space scan angle {expected_angle}",
    );

    let gcode = generate_gcode(&plan, &default_gcode_config()).unwrap();
    assert!(!gcode.is_empty());
    assert_laser_on_moves_within_workspace(&project, &gcode, "scan-angle-fill");
    assert!(gcode.len() > 100, "Fill should produce substantial G-code");
}

// ---------- Fixture 5: Explicit Angle-Pass Multipass Fill ----------

#[test]
fn angle_pass_multipass_loads_and_plans() {
    let project = load_project(&fixture_path("angle-pass-multipass")).unwrap();
    assert_eq!(project.layers.len(), 1);
    assert_eq!(project.objects.len(), 1);
    let layer = &project.layers[0];
    let raster_settings = layer
        .primary_entry()
        .raster_settings
        .as_ref()
        .expect("angle-pass-multipass fixture should have raster settings");
    assert_eq!(raster_settings.angle_passes, 3);
    assert!((raster_settings.angle_increment_deg - 45.0).abs() < 0.5);
    assert!((raster_settings.scan_angle - 15.0).abs() < 0.5);
    assert!(
        !raster_settings.crosshatch,
        "fixture should exercise explicit angle passes, not legacy crosshatch compat",
    );

    let plan = build_plan(&project).unwrap();
    assert!(!plan.segments.is_empty());
    assert!(
        has_raster_segment(&plan),
        "Multipass fill should produce raster segments"
    );
    let raster_count = plan
        .segments
        .iter()
        .filter(|segment| matches!(segment, PlanSegment::Raster { .. }))
        .count();
    assert_eq!(
        raster_count, 3,
        "Explicit angle_passes=3 should produce 3 raster segments"
    );
    let expected_angle = expected_machine_angle(&project, 15.0);
    assert!(
        plan.segments.iter().any(|segment| matches!(
            segment,
            PlanSegment::Raster { scan_angle_deg, .. } if (*scan_angle_deg - expected_angle).abs() < 1.0
        )),
        "Plan should preserve the first reflected machine-space scan angle {expected_angle}",
    );

    let gcode = generate_gcode(&plan, &default_gcode_config()).unwrap();
    assert!(!gcode.is_empty());
    assert_laser_on_moves_within_workspace(&project, &gcode, "angle-pass-multipass");
}

// ---------- Fixture 6: Origin Placement ----------

#[test]
fn fixture_origin_placement_loads_and_plans() {
    let project = load_project(&fixture_path("fixture-origin-placement")).unwrap();
    assert_eq!(
        project.layers.len(),
        2,
        "Should have 2 layers (raster + vector)"
    );
    assert_eq!(project.objects.len(), 2, "Should have 2 objects");

    let plan = build_plan(&project).unwrap();
    assert!(!plan.segments.is_empty());
    assert!(
        has_raster_segment(&plan),
        "Should have raster segment from image layer"
    );
    assert!(
        has_vector_segment(&plan),
        "Should have vector segment from line layer"
    );

    // Benchmark fixtures are saved projects with concrete workspace origins.
    // These fixtures use bottom-left, so plan bounds are machine-space.
    assert!(
        plan.bounds.min.x <= 11.0 && plan.bounds.min.y >= 360.0,
        "Plan bounds min should cover offset raster in bottom-left machine space, got ({}, {})",
        plan.bounds.min.x,
        plan.bounds.min.y,
    );
    assert!(
        plan.bounds.max.x >= 79.0 && plan.bounds.max.y <= project.workspace.bed_height_mm,
        "Plan bounds max should cover offset rect in bottom-left machine space, got ({}, {})",
        plan.bounds.max.x,
        plan.bounds.max.y,
    );
    assert_plan_bounds_fit_workspace(&project, &plan);

    let gcode = generate_gcode(&plan, &default_gcode_config()).unwrap();
    assert!(!gcode.is_empty());
}
