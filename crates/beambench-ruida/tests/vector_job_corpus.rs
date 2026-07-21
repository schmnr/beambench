use beambench_common::{Bounds, Point2D};
use beambench_planner::{ExecutionPlan, PlanSegment};
use beambench_ruida::{RuidaCompilationConfig, compile_ruida_job};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Corpus {
    schema_version: u32,
    cases: Vec<VectorCase>,
}

#[derive(Debug, Deserialize)]
struct VectorCase {
    name: String,
    points_mm: Vec<[f64; 2]>,
    closed: bool,
    power_percent: f64,
    speed_mm_min: f64,
    expected_bounds_micrometres: [i32; 4],
    expected_file_sum: u64,
    expected_clear_hex: String,
    expected_rd_hex: String,
}

fn hex(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0, "hex fixture has an even length");
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).unwrap())
        .collect()
}

fn plan(case: &VectorCase) -> ExecutionPlan {
    let points: Vec<Point2D> = case
        .points_mm
        .iter()
        .map(|point| Point2D::new(point[0], point[1]))
        .collect();
    let bounds =
        points
            .iter()
            .skip(1)
            .fold(Bounds::new(points[0], points[0]), |mut bounds, point| {
                bounds.min.x = bounds.min.x.min(point.x);
                bounds.min.y = bounds.min.y.min(point.y);
                bounds.max.x = bounds.max.x.max(point.x);
                bounds.max.y = bounds.max.y.max(point.y);
                bounds
            });
    ExecutionPlan {
        id: Default::default(),
        project_id: Default::default(),
        revision_hash: case.name.clone(),
        created_at: Default::default(),
        bounds,
        total_distance_mm: 0.0,
        estimated_duration_secs: 0.0,
        segments: vec![PlanSegment::Vector {
            polyline: points,
            closed: case.closed,
            power_percent: case.power_percent,
            speed_mm_min: case.speed_mm_min,
            layer_id: "layer-1".to_string(),
            cut_entry_id: "cut-1".to_string(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        }],
        layer_order: vec!["layer-1".to_string()],
        warnings: Vec::new(),
        failed_entries: Vec::new(),
    }
}

#[test]
fn vector_jobs_match_independent_golden_outputs() {
    let corpus: Corpus = serde_json::from_str(include_str!("fixtures/vector_jobs.json")).unwrap();
    assert_eq!(corpus.schema_version, 1);
    for case in corpus.cases {
        let job = compile_ruida_job(&plan(&case), &RuidaCompilationConfig::default()).unwrap();
        assert_eq!(
            job.bounds_micrometres, case.expected_bounds_micrometres,
            "{} bounds",
            case.name
        );
        assert_eq!(
            job.clear_file_sum, case.expected_file_sum,
            "{} file sum",
            case.name
        );
        assert_eq!(
            job.clear_bytes,
            hex(&case.expected_clear_hex),
            "{} clear",
            case.name
        );
        assert_eq!(
            job.rd_file_bytes,
            hex(&case.expected_rd_hex),
            "{} rd",
            case.name
        );
    }
}
