//! Golden snapshot baselines for benchmark fixtures.
//!
//! Each fixture's plan stats and G-code hash are compared against saved baselines.
//! Run with `UPDATE_SNAPSHOTS=1` to regenerate baselines.

use beambench_grbl::{GcodeConfig, generate_gcode};
use beambench_planner::{PlanSegment, build_plan};
use beambench_preview::distill_preview;
use beambench_project::load_project;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/benchmarks")
        .join(format!("{name}.lzrproj"))
}

fn snapshot_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/snapshots")
}

fn snapshot_path(name: &str) -> PathBuf {
    snapshot_dir().join(name)
}

fn should_update() -> bool {
    std::env::var("UPDATE_SNAPSHOTS").is_ok()
}

fn check_or_update_snapshot(name: &str, actual: &str) {
    let path = snapshot_path(name);
    if should_update() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "Snapshot missing: {}. Run with UPDATE_SNAPSHOTS=1",
            path.display()
        )
    });
    assert_eq!(
        actual, expected,
        "Snapshot mismatch for {name}. Run with UPDATE_SNAPSHOTS=1 to update."
    );
}

// ---------- Plan stats snapshot ----------

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct PlanStatsSnapshot {
    segment_count: usize,
    raster_segments: usize,
    vector_segments: usize,
    travel_segments: usize,
    total_distance_mm: f64,
    estimated_duration_secs: f64,
}

fn capture_plan_stats(plan: &beambench_planner::ExecutionPlan) -> PlanStatsSnapshot {
    let raster_segments = plan
        .segments
        .iter()
        .filter(|s| matches!(s, PlanSegment::Raster { .. }))
        .count();
    let vector_segments = plan
        .segments
        .iter()
        .filter(|s| matches!(s, PlanSegment::Vector { .. }))
        .count();
    let travel_segments = plan
        .segments
        .iter()
        .filter(|s| matches!(s, PlanSegment::Travel { .. }))
        .count();

    PlanStatsSnapshot {
        segment_count: plan.segments.len(),
        raster_segments,
        vector_segments,
        travel_segments,
        total_distance_mm: (plan.total_distance_mm * 100.0).round() / 100.0,
        estimated_duration_secs: (plan.estimated_duration_secs * 100.0).round() / 100.0,
    }
}

// ---------- G-code hash snapshot ----------

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct GcodeHashSnapshot {
    line_count: usize,
    sha256: String,
}

fn capture_gcode_hash(gcode: &[String]) -> GcodeHashSnapshot {
    let full = gcode.join("\n");
    let mut hasher = Sha256::new();
    hasher.update(full.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    GcodeHashSnapshot {
        line_count: gcode.len(),
        sha256: hash,
    }
}

// ---------- Preview stats snapshot ----------

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct PreviewStatsSnapshot {
    layer_count: usize,
    total_distance_mm: f64,
    travel_distance_mm: f64,
    burn_distance_mm: f64,
    segment_count: usize,
    raster_line_count: usize,
    travel_move_count: usize,
}

fn capture_preview_stats(preview: &beambench_preview::PreviewData) -> PreviewStatsSnapshot {
    PreviewStatsSnapshot {
        layer_count: preview.layers.len(),
        total_distance_mm: (preview.stats.total_distance_mm * 100.0).round() / 100.0,
        travel_distance_mm: (preview.stats.travel_distance_mm * 100.0).round() / 100.0,
        burn_distance_mm: (preview.stats.burn_distance_mm * 100.0).round() / 100.0,
        segment_count: preview.stats.segment_count,
        raster_line_count: preview.stats.raster_line_count,
        travel_move_count: preview.travel_moves.len(),
    }
}

// ---------- Tests ----------

const FIXTURE_NAMES: &[&str] = &[
    "grayscale-wedge",
    "binary-raster-overscan",
    "dense-short-segment-vector",
    "scan-angle-fill",
    "angle-pass-multipass",
    "fixture-origin-placement",
];

#[test]
fn plan_stats_snapshots() {
    for name in FIXTURE_NAMES {
        let project = load_project(&fixture_path(name)).unwrap();
        let plan = build_plan(&project).unwrap();
        let stats = capture_plan_stats(&plan);
        let json = serde_json::to_string_pretty(&stats).unwrap();
        check_or_update_snapshot(&format!("{name}.plan-stats.json"), &json);
    }
}

#[test]
fn gcode_hash_snapshots() {
    for name in FIXTURE_NAMES {
        let project = load_project(&fixture_path(name)).unwrap();
        let plan = build_plan(&project).unwrap();
        let config = GcodeConfig::default();
        let gcode = generate_gcode(&plan, &config).unwrap();
        let hash = capture_gcode_hash(&gcode);
        let json = serde_json::to_string_pretty(&hash).unwrap();
        check_or_update_snapshot(&format!("{name}.gcode-hash.json"), &json);
    }
}

#[test]
fn preview_stats_snapshots() {
    for name in FIXTURE_NAMES {
        let project = load_project(&fixture_path(name)).unwrap();
        let plan = build_plan(&project).unwrap();
        let preview = distill_preview(&plan);
        let stats = capture_preview_stats(&preview);
        let json = serde_json::to_string_pretty(&stats).unwrap();
        check_or_update_snapshot(&format!("{name}.preview.json"), &json);
    }
}
