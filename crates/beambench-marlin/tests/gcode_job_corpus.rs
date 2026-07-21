use beambench_common::geometry::{Bounds, Point2D};
use beambench_marlin::{
    MarlinGcodeConfig, MarlinLaserMode, MarlinPowerScale, generate_marlin_gcode,
};
use beambench_planner::{ExecutionPlan, PlanSegment};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Corpus {
    schema_version: u32,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    laser_mode: MarlinLaserMode,
    power_scale: MarlinPowerScale,
    #[serde(default)]
    start_gcode: String,
    #[serde(default)]
    end_gcode: String,
    segment: PlanSegment,
    expected_lines: Vec<String>,
}

fn plan(segment: PlanSegment) -> ExecutionPlan {
    ExecutionPlan {
        id: Default::default(),
        project_id: Default::default(),
        revision_hash: "marlin-golden".to_string(),
        created_at: Default::default(),
        bounds: Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
        total_distance_mm: 0.0,
        estimated_duration_secs: 0.0,
        segments: vec![segment],
        layer_order: vec![],
        warnings: vec![],
        failed_entries: vec![],
    }
}

#[test]
fn versioned_marlin_gcode_jobs_are_stable() {
    let corpus: Corpus = serde_json::from_str(include_str!("fixtures/gcode_jobs.json")).unwrap();
    assert_eq!(corpus.schema_version, 2);
    assert!(!corpus.cases.is_empty());

    for case in corpus.cases {
        let mut config = MarlinGcodeConfig {
            laser_mode: case.laser_mode,
            power_scale: case.power_scale,
            ..MarlinGcodeConfig::default()
        };
        config.gcode.gcode_prefix = case.start_gcode;
        config.gcode.gcode_suffix = case.end_gcode;

        let lines = generate_marlin_gcode(&plan(case.segment), &config)
            .unwrap_or_else(|error| panic!("{}: {error}", case.name));
        assert_eq!(lines, case.expected_lines, "case: {}", case.name);
    }
}
