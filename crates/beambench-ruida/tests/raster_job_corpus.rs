use beambench_common::{Bounds, Point2D};
use beambench_planner::{
    DirectionMode, ExecutionPlan, PlanSegment, PowerMode, ScanAxis, ScanDirection, ScanRun,
    Scanline,
};
use beambench_ruida::{RuidaCompilationConfig, compile_ruida_job};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Corpus {
    schema_version: u32,
    cases: Vec<RasterCase>,
}

#[derive(Debug, Deserialize)]
struct RasterCase {
    name: String,
    scanline_y_mm: f64,
    run_start_x_mm: f64,
    run_end_x_mm: f64,
    overscan_mm: f64,
    min_power_percent: f64,
    max_power_percent: f64,
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

fn plan(case: &RasterCase) -> ExecutionPlan {
    ExecutionPlan {
        id: Default::default(),
        project_id: Default::default(),
        revision_hash: case.name.clone(),
        created_at: Default::default(),
        bounds: Bounds::new(
            Point2D::new(case.run_start_x_mm, case.scanline_y_mm),
            Point2D::new(case.run_end_x_mm, case.scanline_y_mm),
        ),
        total_distance_mm: 0.0,
        estimated_duration_secs: 0.0,
        segments: vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: case.scanline_y_mm,
                runs: vec![ScanRun {
                    start_x_mm: case.run_start_x_mm,
                    end_x_mm: case.run_end_x_mm,
                    power_values: Vec::new(),
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: case.speed_mm_min,
            layer_id: "layer-1".to_string(),
            cut_entry_id: "cut-1".to_string(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: case.overscan_mm,
            outlines: Vec::new(),
            scan_axis: ScanAxis::Horizontal,
            power_max_percent: case.max_power_percent,
            power_min_percent: case.min_power_percent,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.1,
        }],
        layer_order: vec!["layer-1".to_string()],
        warnings: Vec::new(),
        failed_entries: Vec::new(),
    }
}

#[test]
fn raster_jobs_match_independent_golden_outputs() {
    let corpus: Corpus = serde_json::from_str(include_str!("fixtures/raster_jobs.json")).unwrap();
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
