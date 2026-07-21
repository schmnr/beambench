use beambench_common::{Bounds, Point2D};
use beambench_lihuiyu::{LihuiyuCompilationConfig, compile_lihuiyu_job};
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
    points_mm: Vec<[f64; 2]>,
    closed: bool,
    power_percent: f64,
    speed_mm_min: f64,
    reference_egv_ascii: String,
    expected_controller_ascii: String,
    expected_bounds_steps: [i32; 4],
    expected_final_position_steps: [i32; 2],
}

#[test]
fn pinned_job_corpus_matches_compiler() {
    let corpus: Corpus = serde_json::from_str(include_str!("fixtures/job_corpus.json")).unwrap();
    assert_eq!(corpus.schema_version, 1);
    assert!(!corpus.cases.is_empty());

    for case in corpus.cases {
        let points = case
            .points_mm
            .iter()
            .map(|point| Point2D::new(point[0], point[1]))
            .collect();
        let plan = ExecutionPlan {
            id: Default::default(),
            project_id: Default::default(),
            revision_hash: format!("lihuiyu-corpus:{}", case.name),
            created_at: Default::default(),
            bounds: Bounds::new(Point2D::zero(), Point2D::new(100.0, 100.0)),
            total_distance_mm: 0.0,
            estimated_duration_secs: 0.0,
            segments: vec![PlanSegment::Vector {
                polyline: points,
                closed: case.closed,
                power_percent: case.power_percent,
                speed_mm_min: case.speed_mm_min,
                layer_id: "corpus".to_string(),
                cut_entry_id: case.name.clone(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            }],
            layer_order: vec!["corpus".to_string()],
            warnings: Vec::new(),
            failed_entries: Vec::new(),
        };
        let job = compile_lihuiyu_job(&plan, &LihuiyuCompilationConfig::default())
            .unwrap_or_else(|error| panic!("{} failed to compile: {error}", case.name));
        assert_eq!(
            job.egv_bytes,
            case.reference_egv_ascii.as_bytes(),
            "{} EGV transcript",
            case.name
        );
        assert_eq!(
            job.controller_bytes,
            case.expected_controller_ascii.as_bytes(),
            "{} controller stream",
            case.name
        );
        assert!(job.waits_for_completion, "{} completion wait", case.name);
        assert_eq!(
            job.bounds_steps, case.expected_bounds_steps,
            "{} bounds",
            case.name
        );
        assert_eq!(
            job.final_position_steps, case.expected_final_position_steps,
            "{} final position",
            case.name
        );
    }
}
