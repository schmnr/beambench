//! Integration test: import a potrace SVG and verify fill rasterization.
//!
//! This test uses the actual monotrace_26212e07.svg file to reproduce
//! the "solid black rectangle" fill bug.

use std::time::Instant;

use beambench_common::{Bounds, PathCommand, Point2D, Polyline, SubPath, VecPath};
use beambench_core::import::import_svg;
use beambench_core::layer::OperationType;
use beambench_core::object::{ObjectData, ProjectObject, ShapeKind};
use beambench_core::project::Project;
use beambench_core::vector::boolean::path_exclude;
use beambench_core::vector::normalize::normalize_object;
use beambench_core::vector::{
    OFFSET_FILL_BOOLEAN_TOLERANCE_MM, normalize_subject_evenodd_with_tolerance, weld_shapes,
};
use beambench_planner::{
    PlanEntryFailureReason, PlanSegment, build_plan, fill_raster::rasterize_fill,
    offset_fill_boolean_tolerances_mm,
};
use beambench_preview::distill_preview;

const SVG_BYTES: &[u8] = include_bytes!("fixtures/monotrace_26212e07.svg");
const INTERVIEWER_SVG_BYTES: &[u8] = include_bytes!("fixtures/interviewer.svg");

#[test]
fn potrace_svg_import_fill_not_solid() {
    // 1. Import the SVG
    let mut project = Project::new("Fill Test");
    let layer_id = project.ensure_default_layer();
    let ids = import_svg(SVG_BYTES, &mut project, layer_id).unwrap();
    assert!(!ids.is_empty(), "SVG should import at least one object");

    let obj = project.find_object(ids[0]).unwrap();
    let bounds = obj.bounds;
    eprintln!(
        "Object bounds: ({:.2}, {:.2}) -> ({:.2}, {:.2})  [w={:.2}, h={:.2}]",
        bounds.min.x,
        bounds.min.y,
        bounds.max.x,
        bounds.max.y,
        bounds.width(),
        bounds.height()
    );

    // Check path_data structure
    if let ObjectData::VectorPath {
        path_data, closed, ..
    } = &obj.data
    {
        let subpath_count = path_data.matches('M').count();
        let close_count = path_data.matches('Z').count();
        eprintln!(
            "path_data: {} chars, {} sub-paths (M), {} closes (Z), closed={}",
            path_data.len(),
            subpath_count,
            close_count,
            closed
        );
    }

    // 2. Normalize the object (parse → flatten → map to bounds)
    let normalized = normalize_object(obj).expect("Should normalize the potrace VectorPath");
    eprintln!("Normalized: {} polylines", normalized.polylines.len());

    let mut closed_count = 0;
    let mut open_count = 0;
    for poly in &normalized.polylines {
        if poly.closed {
            closed_count += 1;
        } else {
            open_count += 1;
        }
    }
    eprintln!("  closed: {}, open: {}", closed_count, open_count);

    // Print first few polyline bounds for debugging
    for (i, poly) in normalized.polylines.iter().take(5).enumerate() {
        let min_x = poly
            .points
            .iter()
            .map(|p| p.x)
            .fold(f64::INFINITY, f64::min);
        let max_x = poly
            .points
            .iter()
            .map(|p| p.x)
            .fold(f64::NEG_INFINITY, f64::max);
        let min_y = poly
            .points
            .iter()
            .map(|p| p.y)
            .fold(f64::INFINITY, f64::min);
        let max_y = poly
            .points
            .iter()
            .map(|p| p.y)
            .fold(f64::NEG_INFINITY, f64::max);
        eprintln!(
            "  polyline[{}]: {} pts, closed={}, bounds=({:.1},{:.1})->({:.1},{:.1})",
            i,
            poly.points.len(),
            poly.closed,
            min_x,
            min_y,
            max_x,
            max_y
        );
    }

    // 3. Rasterize the fill
    let line_interval = 0.1;
    let processed = rasterize_fill(&normalized.polylines, &bounds, line_interval)
        .expect("Should produce a raster fill");

    eprintln!(
        "Raster: {}x{} px, format={:?}",
        processed.width_px, processed.height_px, processed.format
    );

    // 4. Count burn pixels
    let row_bytes = (processed.width_px as usize).div_ceil(8);
    let mut burn_count = 0u64;
    let total = processed.width_px as u64 * processed.height_px as u64;

    for row in 0..processed.height_px {
        for col in 0..processed.width_px {
            let byte_idx = row as usize * row_bytes + col as usize / 8;
            let bit_idx = 7 - (col as usize % 8);
            if (processed.data[byte_idx] & (1 << bit_idx)) == 0 {
                burn_count += 1;
            }
        }
    }

    let fill_ratio = burn_count as f64 / total as f64;
    eprintln!(
        "Fill: {} burn / {} total = {:.1}%",
        burn_count,
        total,
        fill_ratio * 100.0
    );

    // A solid fill would be ~100%. The text "BEAM BENCH" should fill
    // maybe 30-60% of the bounding box, not 90%+.
    assert!(
        fill_ratio < 0.90,
        "Fill ratio is {:.1}% — this looks like a solid fill bug! Expected <90%",
        fill_ratio * 100.0
    );
    assert!(
        fill_ratio > 0.01,
        "Fill ratio is {:.1}% — fill seems empty",
        fill_ratio * 100.0
    );
}

#[test]
fn potrace_svg_switch_line_to_fill_builds_preview() {
    let mut project = Project::new("Fill Preview Switch");
    // The fixture is 520x406 mm at its declared SVG size. This test exercises
    // line-to-fill planning, so give it a workspace that contains the import.
    project.workspace.bed_width_mm = 1000.0;
    project.workspace.bed_height_mm = 1000.0;
    let layer_id = project.ensure_default_layer();
    let ids = import_svg(SVG_BYTES, &mut project, layer_id).unwrap();
    assert_eq!(ids.len(), 1, "Expected imported SVG to create one object");

    let layer = project
        .find_layer_mut(layer_id)
        .expect("default layer should exist");
    layer.primary_entry_mut().operation = OperationType::Fill;
    layer.primary_entry_mut().raster_settings = Some(Default::default());
    layer.primary_entry_mut().vector_settings = None;

    let plan = build_plan(&project).expect("line->fill switch should still build a plan");
    assert!(
        plan.segments
            .iter()
            .any(|s| matches!(s, PlanSegment::Raster { .. })),
        "fill preview should produce raster segments"
    );

    let preview = distill_preview(&plan);
    let raster_outline_points: usize = preview
        .layers
        .iter()
        .flat_map(|l| &l.raster_regions)
        .flat_map(|r| &r.outlines)
        .map(|o| o.points.len())
        .sum();
    let vector_preview_points: usize = preview
        .layers
        .iter()
        .flat_map(|l| &l.vector_paths)
        .map(|v| v.points.len())
        .sum();
    let json = serde_json::to_vec(&preview).expect("preview should serialize");
    eprintln!(
        "preview payload: {} bytes, raster_regions={}, raster_outline_points={}, vector_preview_points={}",
        json.len(),
        preview
            .layers
            .iter()
            .map(|l| l.raster_regions.len())
            .sum::<usize>(),
        raster_outline_points,
        vector_preview_points
    );
    assert!(
        preview.layers.iter().any(|l| !l.raster_regions.is_empty()),
        "fill preview should include raster preview data"
    );
}

#[test]
fn many_closed_shapes_fill_preview_stress() {
    let mut project = Project::new("Fill Preview Stress");
    let layer_id = project.ensure_default_layer();
    let layer = project
        .find_layer_mut(layer_id)
        .expect("default layer should exist");
    layer.primary_entry_mut().operation = OperationType::Fill;
    layer.primary_entry_mut().raster_settings = Some(Default::default());
    layer.primary_entry_mut().vector_settings = None;

    let cols = 40;
    let rows = 25;
    let size = 4.0;
    let gap = 1.0;
    for row in 0..rows {
        for col in 0..cols {
            let x = col as f64 * (size + gap);
            let y = row as f64 * (size + gap);
            project.add_object(ProjectObject::new(
                &format!("rect-{}-{}", row, col),
                layer_id,
                Bounds::new(Point2D::new(x, y), Point2D::new(x + size, y + size)),
                ObjectData::Shape {
                    kind: ShapeKind::Rectangle,
                    width: size,
                    height: size,
                    corner_radius: 0.0,
                },
            ));
        }
    }

    let start = std::time::Instant::now();
    let plan = build_plan(&project).expect("stress fill should still build a plan");
    let plan_elapsed = start.elapsed();

    let preview_start = std::time::Instant::now();
    let preview = distill_preview(&plan);
    let preview_elapsed = preview_start.elapsed();
    let json = serde_json::to_vec(&preview).expect("preview should serialize");
    let raster_outline_points: usize = preview
        .layers
        .iter()
        .flat_map(|l| &l.raster_regions)
        .flat_map(|r| &r.outlines)
        .map(|o| o.points.len())
        .sum();

    eprintln!(
        "stress preview: objects={}, plan_ms={}, preview_ms={}, payload_bytes={}, raster_regions={}, raster_outline_points={}",
        cols * rows,
        plan_elapsed.as_millis(),
        preview_elapsed.as_millis(),
        json.len(),
        preview
            .layers
            .iter()
            .map(|l| l.raster_regions.len())
            .sum::<usize>(),
        raster_outline_points
    );

    assert!(
        preview.layers.iter().any(|l| !l.raster_regions.is_empty()),
        "stress preview should include raster preview data"
    );
}

#[test]
fn primitive_rectangle_fill_preview_keeps_outlines_and_runs() {
    let mut project = Project::new("Primitive Rectangle Fill Preview");
    let layer_id = project.ensure_default_layer();
    let layer = project
        .find_layer_mut(layer_id)
        .expect("default layer should exist");
    layer.primary_entry_mut().operation = OperationType::Fill;
    layer.primary_entry_mut().raster_settings = Some(Default::default());
    layer.primary_entry_mut().vector_settings = None;

    project.add_object(ProjectObject::new(
        "rect",
        layer_id,
        Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(40.0, 30.0)),
        ObjectData::Shape {
            kind: ShapeKind::Rectangle,
            width: 30.0,
            height: 20.0,
            corner_radius: 0.0,
        },
    ));

    let plan = build_plan(&project).expect("primitive rectangle fill should build a plan");
    let preview = distill_preview(&plan);
    let raster = preview
        .layers
        .iter()
        .flat_map(|layer| &layer.raster_regions)
        .next()
        .expect("expected one raster preview region");

    assert!(
        !raster.outlines.is_empty(),
        "primitive rectangle fill preview should include outlines"
    );
    assert!(
        !raster.run_extents.is_empty(),
        "primitive rectangle fill preview should include run extents"
    );
}

#[test]
fn dense_fill_preview_outlines_are_budgeted() {
    let mut project = Project::new("Dense Fill Preview");
    let layer_id = project.ensure_default_layer();
    let layer = project
        .find_layer_mut(layer_id)
        .expect("default layer should exist");
    layer.primary_entry_mut().operation = OperationType::Fill;
    layer.primary_entry_mut().raster_settings = Some(Default::default());
    layer.primary_entry_mut().vector_settings = None;

    let point_count = 20_000usize;
    let radius_x = 60.0;
    let radius_y = 40.0;
    let mut path_data = String::new();
    for i in 0..point_count {
        let t = (i as f64 / point_count as f64) * std::f64::consts::TAU;
        let x = radius_x + radius_x * t.cos();
        let y = radius_y + radius_y * t.sin();
        if i == 0 {
            path_data.push_str(&format!("M {x:.6} {y:.6} "));
        } else {
            path_data.push_str(&format!("L {x:.6} {y:.6} "));
        }
    }
    path_data.push('Z');

    project.add_object(ProjectObject::new(
        "dense-ring",
        layer_id,
        Bounds::new(
            Point2D::new(0.0, 0.0),
            Point2D::new(radius_x * 2.0, radius_y * 2.0),
        ),
        ObjectData::VectorPath {
            path_data,
            closed: true,
            ruler_guide_axis: None,
        },
    ));

    let plan = build_plan(&project).expect("dense fill should still build a plan");
    let preview = distill_preview(&plan);
    let raster_outline_points: usize = preview
        .layers
        .iter()
        .flat_map(|l| &l.raster_regions)
        .flat_map(|r| &r.outlines)
        .map(|o| o.points.len())
        .sum();

    assert!(
        raster_outline_points <= 12_000,
        "preview outlines should be budgeted for dense fill paths, got {} points",
        raster_outline_points
    );
}

#[test]
fn dense_logo_offset_fill_characterization() {
    // This characterization guards runtime/cap/bounds on a dense traced SVG,
    // but it does not catch the detail-loss/artifact regression covered by
    // interviewer.svg below.
    let mut project = Project::new("Dense OffsetFill Characterization");
    project.workspace.bed_width_mm = 1000.0;
    project.workspace.bed_height_mm = 1000.0;
    let layer_id = project.ensure_default_layer();
    let ids = import_svg(SVG_BYTES, &mut project, layer_id).expect("dense SVG should import");
    assert_eq!(
        ids.len(),
        1,
        "expected the fixture to import as one vector object"
    );

    let object_bounds = project
        .find_object(ids[0])
        .expect("imported object should exist")
        .bounds;

    let layer = project
        .find_layer_mut(layer_id)
        .expect("default layer should exist");
    layer.primary_entry_mut().operation = OperationType::OffsetFill;
    layer.primary_entry_mut().raster_settings = Some(Default::default());
    layer.primary_entry_mut().vector_settings = Some(Default::default());
    if let Some(raster_settings) = layer.primary_entry_mut().raster_settings.as_mut() {
        raster_settings.line_interval_mm = 0.1;
    }

    let normalized = normalize_object(
        project
            .find_object(ids[0])
            .expect("imported dense-logo object should exist"),
    )
    .expect("dense-logo fixture should normalize");
    let normalized_closed: Vec<Polyline> = normalized
        .polylines
        .iter()
        .filter(|poly| poly.closed && poly.points.len() >= 3)
        .cloned()
        .collect();
    let (boolean_flatten_tolerance_mm, boolean_simplify_tolerance_mm) =
        offset_fill_boolean_tolerances_mm();
    let composite = normalize_subject_evenodd_with_tolerance(
        &normalized_closed
            .iter()
            .map(polyline_to_vecpath)
            .collect::<Vec<_>>(),
        boolean_flatten_tolerance_mm,
        boolean_simplify_tolerance_mm,
    );
    let composite_polylines = flatten_closed(&composite);
    let composite_metrics = stage_metrics(&composite_polylines);
    let composite_unit_count = inferred_offset_fill_unit_count(&composite_polylines);
    let legacy_composite = legacy_offset_fill_composite(&normalized_closed);
    let legacy_composite_polylines = flatten_closed(&legacy_composite);
    let legacy_composite_metrics = stage_metrics(&legacy_composite_polylines);
    let legacy_unit_count = inferred_offset_fill_unit_count(&legacy_composite_polylines);

    let started_at = Instant::now();
    let plan = build_plan(&project).expect("dense offset_fill should build a plan");
    let elapsed = started_at.elapsed();

    let vector_segments: Vec<&PlanSegment> = plan
        .segments
        .iter()
        .filter(|segment| matches!(segment, PlanSegment::Vector { .. }))
        .collect();
    let failed_entries: Vec<String> = plan
        .failed_entries
        .iter()
        .map(|failure| match &failure.reason {
            PlanEntryFailureReason::ImageMaskSkipped { object_id, message } => {
                format!("image-mask:{object_id}:{message}")
            }
            PlanEntryFailureReason::OffsetFillTimedOut {
                iterations,
                elapsed_ms,
            } => format!(
                "timeout:{}:{}:{}",
                failure.cut_entry_id.as_deref().unwrap_or("unknown"),
                iterations,
                elapsed_ms
            ),
            PlanEntryFailureReason::OffsetFillEmergencyCeiling { iterations } => format!(
                "ceiling:{}:{}",
                failure.cut_entry_id.as_deref().unwrap_or("unknown"),
                iterations
            ),
        })
        .collect();

    let all_points: Vec<Point2D> = vector_segments
        .iter()
        .flat_map(|segment| match segment {
            PlanSegment::Vector { polyline, .. } => polyline.iter().copied().collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect();
    let plan_bounds = (!all_points.is_empty()).then(|| {
        Bounds::new(
            Point2D::new(
                all_points
                    .iter()
                    .map(|point| point.x)
                    .fold(f64::INFINITY, f64::min),
                all_points
                    .iter()
                    .map(|point| point.y)
                    .fold(f64::INFINITY, f64::min),
            ),
            Point2D::new(
                all_points
                    .iter()
                    .map(|point| point.x)
                    .fold(f64::NEG_INFINITY, f64::max),
                all_points
                    .iter()
                    .map(|point| point.y)
                    .fold(f64::NEG_INFINITY, f64::max),
            ),
        )
    });

    eprintln!(
        "dense-logo offset_fill baseline_ms={:.1} normalized_closed={} normalized_points={} composite_polylines={} composite_points={} composite_max_edge={:.3} composite_units={} legacy_polylines={} legacy_points={} legacy_max_edge={:.3} legacy_units={} vector_segments={} total_segments={} warnings={} failed_entries={:?} bounds={}",
        elapsed.as_secs_f64() * 1000.0,
        normalized_closed.len(),
        stage_metrics(&normalized_closed).total_points,
        composite_metrics.polyline_count,
        composite_metrics.total_points,
        composite_metrics.max_edge_mm,
        composite_unit_count,
        legacy_composite_metrics.polyline_count,
        legacy_composite_metrics.total_points,
        legacy_composite_metrics.max_edge_mm,
        legacy_unit_count,
        vector_segments.len(),
        plan.segments.len(),
        plan.warnings.len(),
        failed_entries,
        plan_bounds
            .map(|bounds| format!(
                "({:.2},{:.2})->({:.2},{:.2})",
                bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y
            ))
            .unwrap_or_else(|| "<none>".to_string()),
    );

    assert!(
        plan.failed_entries.is_empty(),
        "dense-logo offset_fill should complete without failed entries: {:?}",
        plan.failed_entries
    );
    let plan_bounds = plan_bounds.expect("successful dense-logo offset_fill should have bounds");
    assert!(
        !vector_segments.is_empty(),
        "successful dense-logo offset_fill should emit vector segments"
    );
    assert!(
        plan_bounds.width() >= object_bounds.width() * 0.80
            && plan_bounds.height() >= object_bounds.height() * 0.80,
        "dense-logo offset_fill should preserve broad bounds coverage; object={:?}, plan={:?}",
        object_bounds,
        plan_bounds
    );
    assert!(
        plan_bounds.min.x >= object_bounds.min.x - 1.0
            && plan_bounds.min.y >= object_bounds.min.y - 1.0
            && plan_bounds.max.x <= object_bounds.max.x + 1.0
            && plan_bounds.max.y <= object_bounds.max.y + 1.0,
        "dense-logo offset_fill bounds should stay near the source art; object={:?}, plan={:?}",
        object_bounds,
        plan_bounds
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArtifactStage {
    Normalization,
    Composite,
    Emission,
    Clean,
}

#[derive(Debug, Clone)]
struct StageMetrics {
    polyline_count: usize,
    total_points: usize,
    max_edge_mm: f64,
}

#[derive(Debug, Clone)]
struct InterviewerOffsetFillAnalysis {
    object_bounds: Bounds,
    normalized: StageMetrics,
    legacy_composite: StageMetrics,
    current_composite: StageMetrics,
    emitted: StageMetrics,
    normalized_polylines: Vec<Polyline>,
    legacy_composite_polylines: Vec<Polyline>,
    current_composite_polylines: Vec<Polyline>,
    emitted_polylines: Vec<Polyline>,
    vector_segment_count: usize,
    warnings: Vec<String>,
    failed_entries: Vec<String>,
    stage: ArtifactStage,
}

#[test]
fn interviewer_offset_fill_regression() {
    let analysis = analyze_interviewer_offset_fill(0.25);
    let diagonal = analysis
        .object_bounds
        .width()
        .hypot(analysis.object_bounds.height());
    let source_scaled_limit = diagonal * 0.12;
    let allowed_emitted_edge = (analysis.normalized.max_edge_mm * 2.5).min(source_scaled_limit);

    eprintln!(
        "interviewer offset_fill: stage={:?} normalized={:?} legacy={:?} current={:?} emitted={:?} vector_segments={} warnings={:?} failed_entries={:?}",
        analysis.stage,
        analysis.normalized,
        analysis.legacy_composite,
        analysis.current_composite,
        analysis.emitted,
        analysis.vector_segment_count,
        analysis.warnings,
        analysis.failed_entries,
    );

    dump_debug_svg_if_requested("interviewer-normalized.svg", &analysis.normalized_polylines);
    dump_debug_svg_if_requested(
        "interviewer-legacy-composite.svg",
        &analysis.legacy_composite_polylines,
    );
    dump_debug_svg_if_requested(
        "interviewer-current-composite.svg",
        &analysis.current_composite_polylines,
    );
    dump_debug_svg_if_requested("interviewer-emitted.svg", &analysis.emitted_polylines);

    assert_eq!(
        analysis.stage,
        ArtifactStage::Clean,
        "the current offset_fill pipeline should keep normalized, composite, and emitted edges below the artifact threshold"
    );
    assert!(
        analysis.failed_entries.is_empty(),
        "fast interviewer regression should not fail closed: {:?}",
        analysis.failed_entries
    );
    assert!(
        analysis.vector_segment_count > 0,
        "offset_fill should emit vector segments"
    );
    assert!(
        analysis.emitted.max_edge_mm <= analysis.normalized.max_edge_mm * 2.5,
        "emitted max edge {:.3}mm should stay within 2.5x normalized {:.3}mm",
        analysis.emitted.max_edge_mm,
        analysis.normalized.max_edge_mm
    );
    assert!(
        analysis.emitted.max_edge_mm <= source_scaled_limit,
        "emitted max edge {:.3}mm should stay below {:.3}mm (12% of fixture diagonal)",
        analysis.emitted.max_edge_mm,
        source_scaled_limit
    );

    assert!(
        analysis.emitted.max_edge_mm <= allowed_emitted_edge,
        "emitted max edge {:.3}mm exceeds allowed {:.3}mm",
        analysis.emitted.max_edge_mm,
        allowed_emitted_edge
    );
}

#[test]
#[ignore = "manual production-spacing gate for offset_fill fidelity"]
fn interviewer_offset_fill_regression_production_spacing() {
    // Manual ship-gate command:
    // cargo test -p beambench-planner --test fill_integration interviewer_offset_fill_regression_production_spacing -- --ignored --nocapture
    //
    // Compare against the saved raster reference using these ROIs:
    // - left eye / brow: x 0.22..0.47, y 0.18..0.44
    // - right eye / brow: x 0.53..0.80, y 0.18..0.44
    // - nose bridge / nostril: x 0.40..0.63, y 0.37..0.73
    //
    // Record the closed-ring counts for these ROIs and require Beam Bench to
    // retain at least 80% in each region.
    let analysis = analyze_interviewer_offset_fill(0.1);
    let diagonal = analysis
        .object_bounds
        .width()
        .hypot(analysis.object_bounds.height());
    let source_scaled_limit = diagonal * 0.12;

    eprintln!(
        "interviewer offset_fill production spacing: stage={:?} emitted={:?} vector_segments={} warnings={:?} failed_entries={:?}",
        analysis.stage,
        analysis.emitted,
        analysis.vector_segment_count,
        analysis.warnings,
        analysis.failed_entries,
    );

    assert!(
        analysis.failed_entries.is_empty(),
        "production-spacing interviewer offset_fill should complete: {:?}",
        analysis.failed_entries
    );
    assert_eq!(
        analysis.stage,
        ArtifactStage::Clean,
        "production-spacing offset_fill should keep normalized, composite, and emitted edges below the artifact threshold"
    );
    assert!(
        analysis.vector_segment_count > 0,
        "successful production-spacing offset_fill should emit vector segments"
    );
    assert!(
        analysis.emitted.max_edge_mm <= analysis.normalized.max_edge_mm * 2.5,
        "production-spacing emitted max edge {:.3}mm should stay within 2.5x normalized {:.3}mm",
        analysis.emitted.max_edge_mm,
        analysis.normalized.max_edge_mm
    );
    assert!(
        analysis.emitted.max_edge_mm <= source_scaled_limit,
        "production-spacing emitted max edge {:.3}mm should stay below {:.3}mm (12% of fixture diagonal)",
        analysis.emitted.max_edge_mm,
        source_scaled_limit
    );
}

#[test]
fn offset_fill_production_uses_expected_boolean_tolerances() {
    let (flatten_tolerance_mm, simplify_tolerance_mm) = offset_fill_boolean_tolerances_mm();
    assert_eq!(flatten_tolerance_mm, OFFSET_FILL_BOOLEAN_TOLERANCE_MM);
    assert_eq!(simplify_tolerance_mm, OFFSET_FILL_BOOLEAN_TOLERANCE_MM);
}

fn analyze_interviewer_offset_fill(line_interval_mm: f64) -> InterviewerOffsetFillAnalysis {
    let mut project = Project::new("Interviewer OffsetFill");
    let layer_id = project.ensure_default_layer();
    let ids = import_svg(INTERVIEWER_SVG_BYTES, &mut project, layer_id)
        .expect("interviewer fixture should import");
    assert_eq!(
        ids.len(),
        1,
        "expected interviewer fixture to import as one object"
    );

    let object = project
        .find_object(ids[0])
        .expect("imported interviewer object should exist")
        .clone();
    let object_bounds = object.bounds;

    let normalized = normalize_object(&object).expect("interviewer fixture should normalize");
    let normalized_polylines: Vec<Polyline> = normalized
        .polylines
        .iter()
        .filter(|poly| poly.closed && poly.points.len() >= 3)
        .cloned()
        .collect();

    let normalized_metrics = stage_metrics(&normalized_polylines);
    let legacy_composite_path = legacy_offset_fill_composite(&normalized_polylines);
    let legacy_composite_polylines = flatten_closed(&legacy_composite_path);
    let legacy_composite_metrics = stage_metrics(&legacy_composite_polylines);

    let (boolean_flatten_tolerance_mm, boolean_simplify_tolerance_mm) =
        offset_fill_boolean_tolerances_mm();
    let current_composite_path = normalize_subject_evenodd_with_tolerance(
        &normalized_polylines
            .iter()
            .map(polyline_to_vecpath)
            .collect::<Vec<_>>(),
        boolean_flatten_tolerance_mm,
        boolean_simplify_tolerance_mm,
    );
    let current_composite_polylines = flatten_closed(&current_composite_path);
    let current_composite_metrics = stage_metrics(&current_composite_polylines);

    let layer = project
        .find_layer_mut(layer_id)
        .expect("interviewer layer should exist");
    layer.primary_entry_mut().operation = OperationType::OffsetFill;
    layer.primary_entry_mut().raster_settings = Some(Default::default());
    layer.primary_entry_mut().vector_settings = Some(Default::default());
    if let Some(raster_settings) = layer.primary_entry_mut().raster_settings.as_mut() {
        raster_settings.line_interval_mm = line_interval_mm;
    }

    let plan = build_plan(&project).expect("interviewer offset_fill should build");
    let emitted_polylines: Vec<Polyline> = plan
        .segments
        .iter()
        .filter_map(|segment| match segment {
            PlanSegment::Vector {
                polyline, closed, ..
            } if polyline.len() >= 2 => Some(Polyline::new(polyline.clone(), *closed)),
            _ => None,
        })
        .collect();
    let emitted_metrics = stage_metrics(&emitted_polylines);
    let vector_segment_count = emitted_polylines.len();
    let failed_entries = plan
        .failed_entries
        .iter()
        .map(|failure| match &failure.reason {
            PlanEntryFailureReason::ImageMaskSkipped { object_id, message } => {
                format!("image-mask:{object_id}:{message}")
            }
            PlanEntryFailureReason::OffsetFillTimedOut {
                iterations,
                elapsed_ms,
            } => format!(
                "timeout:{}:{}:{}",
                failure.cut_entry_id.as_deref().unwrap_or("unknown"),
                iterations,
                elapsed_ms
            ),
            PlanEntryFailureReason::OffsetFillEmergencyCeiling { iterations } => format!(
                "ceiling:{}:{}",
                failure.cut_entry_id.as_deref().unwrap_or("unknown"),
                iterations
            ),
        })
        .collect();

    let diagonal = object_bounds.width().hypot(object_bounds.height());
    let phase_threshold = diagonal * 0.12;
    let stage = if normalized_metrics.max_edge_mm > phase_threshold {
        ArtifactStage::Normalization
    } else if current_composite_metrics.max_edge_mm > phase_threshold {
        ArtifactStage::Composite
    } else if emitted_metrics.max_edge_mm > phase_threshold {
        ArtifactStage::Emission
    } else {
        ArtifactStage::Clean
    };

    InterviewerOffsetFillAnalysis {
        object_bounds,
        normalized: normalized_metrics,
        legacy_composite: legacy_composite_metrics,
        current_composite: current_composite_metrics,
        emitted: emitted_metrics,
        normalized_polylines,
        legacy_composite_polylines,
        current_composite_polylines,
        emitted_polylines,
        vector_segment_count,
        warnings: plan
            .warnings
            .iter()
            .map(|warning| warning.message.clone())
            .collect(),
        failed_entries,
        stage,
    }
}

fn stage_metrics(polylines: &[Polyline]) -> StageMetrics {
    StageMetrics {
        polyline_count: polylines.len(),
        total_points: polylines.iter().map(|poly| poly.points.len()).sum(),
        max_edge_mm: max_edge_length(polylines),
    }
}

fn max_edge_length(polylines: &[Polyline]) -> f64 {
    let mut max_edge: f64 = 0.0;
    for poly in polylines {
        for pair in poly.points.windows(2) {
            max_edge = max_edge.max(pair[0].distance_to(&pair[1]));
        }
        if poly.closed && poly.points.len() >= 3 {
            max_edge =
                max_edge.max(poly.points[poly.points.len() - 1].distance_to(&poly.points[0]));
        }
    }
    max_edge
}

fn flatten_closed(path: &VecPath) -> Vec<Polyline> {
    beambench_core::vector::flatten::flatten_vecpath(path, OFFSET_FILL_BOOLEAN_TOLERANCE_MM)
        .into_iter()
        .filter(|poly| poly.closed && poly.points.len() >= 3)
        .collect()
}

fn legacy_offset_fill_composite(original_polylines: &[Polyline]) -> VecPath {
    // Intentionally frozen reference for the pre-PR1 offset_fill composite
    // path. Keep this inlined here so future production refactors do not
    // accidentally "update" the regression baseline.
    let depths = legacy_nesting_depths(original_polylines);
    let nested_indices: Vec<usize> = depths
        .iter()
        .enumerate()
        .filter_map(|(i, depth)| (*depth > 0).then_some(i))
        .collect();
    let vecpaths: Vec<VecPath> = original_polylines.iter().map(polyline_to_vecpath).collect();
    let mut composite = weld_shapes(&vecpaths);
    for idx in nested_indices {
        composite = path_exclude(&composite, &polyline_to_vecpath(&original_polylines[idx]));
    }
    composite
}

fn inferred_offset_fill_unit_count(polylines: &[Polyline]) -> usize {
    let depths = legacy_nesting_depths(polylines);
    depths.into_iter().filter(|depth| depth % 2 == 0).count()
}

fn legacy_nesting_depths(polylines: &[Polyline]) -> Vec<usize> {
    let bounds: Vec<Option<Bounds>> = polylines.iter().map(polyline_bounds).collect();
    polylines
        .iter()
        .enumerate()
        .map(|(idx, poly)| {
            polylines
                .iter()
                .enumerate()
                .filter(|(other_idx, other)| {
                    idx != *other_idx
                        && legacy_polyline_contained_in(
                            poly,
                            other,
                            bounds[idx],
                            bounds[*other_idx],
                        )
                })
                .count()
        })
        .collect()
}

fn legacy_polyline_contained_in(
    inner: &Polyline,
    outer: &Polyline,
    inner_bounds: Option<Bounds>,
    outer_bounds: Option<Bounds>,
) -> bool {
    let Some(inner_bounds) = inner_bounds else {
        return false;
    };
    let Some(outer_bounds) = outer_bounds else {
        return false;
    };
    if !bounds_contains_bounds(outer_bounds, inner_bounds) {
        return false;
    }
    inner
        .points
        .first()
        .is_some_and(|pt| point_in_polygon(*pt, outer))
}

fn polyline_bounds(poly: &Polyline) -> Option<Bounds> {
    if poly.points.is_empty() {
        return None;
    }
    Some(Bounds::new(
        Point2D::new(
            poly.points
                .iter()
                .map(|p| p.x)
                .fold(f64::INFINITY, f64::min),
            poly.points
                .iter()
                .map(|p| p.y)
                .fold(f64::INFINITY, f64::min),
        ),
        Point2D::new(
            poly.points
                .iter()
                .map(|p| p.x)
                .fold(f64::NEG_INFINITY, f64::max),
            poly.points
                .iter()
                .map(|p| p.y)
                .fold(f64::NEG_INFINITY, f64::max),
        ),
    ))
}

fn bounds_contains_bounds(outer: Bounds, inner: Bounds) -> bool {
    outer.min.x <= inner.min.x
        && outer.min.y <= inner.min.y
        && outer.max.x >= inner.max.x
        && outer.max.y >= inner.max.y
}

fn point_in_polygon(point: Point2D, poly: &Polyline) -> bool {
    if poly.points.len() < 3 {
        return false;
    }
    let mut inside = false;
    let n = poly.points.len();
    for i in 0..n {
        let a = poly.points[i];
        let b = poly.points[(i + 1) % n];
        let dy = b.y - a.y;
        let denom = if dy.abs() < f64::EPSILON {
            f64::EPSILON.copysign(dy)
        } else {
            dy
        };
        let crosses = ((a.y > point.y) != (b.y > point.y))
            && (point.x < (b.x - a.x) * (point.y - a.y) / denom + a.x);
        if crosses {
            inside = !inside;
        }
    }
    inside
}

fn polyline_to_vecpath(poly: &Polyline) -> VecPath {
    if poly.points.is_empty() {
        return VecPath { subpaths: vec![] };
    }
    let mut commands = vec![PathCommand::MoveTo {
        x: poly.points[0].x,
        y: poly.points[0].y,
    }];
    for pt in &poly.points[1..] {
        commands.push(PathCommand::LineTo { x: pt.x, y: pt.y });
    }
    if poly.closed {
        commands.push(PathCommand::Close);
    }
    VecPath {
        subpaths: vec![SubPath {
            commands,
            closed: poly.closed,
        }],
    }
}

fn dump_debug_svg_if_requested(file_name: &str, polylines: &[Polyline]) {
    let Ok(dir) = std::env::var("BEAMBENCH_OFFSET_FILL_DEBUG_DIR") else {
        return;
    };
    if polylines.is_empty() {
        return;
    }

    let bounds = Bounds::new(
        Point2D::new(
            polylines
                .iter()
                .flat_map(|poly| poly.points.iter().map(|pt| pt.x))
                .fold(f64::INFINITY, f64::min),
            polylines
                .iter()
                .flat_map(|poly| poly.points.iter().map(|pt| pt.y))
                .fold(f64::INFINITY, f64::min),
        ),
        Point2D::new(
            polylines
                .iter()
                .flat_map(|poly| poly.points.iter().map(|pt| pt.x))
                .fold(f64::NEG_INFINITY, f64::max),
            polylines
                .iter()
                .flat_map(|poly| poly.points.iter().map(|pt| pt.y))
                .fold(f64::NEG_INFINITY, f64::max),
        ),
    );

    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" fill=\"none\" stroke=\"black\" stroke-width=\"0.2\">",
        bounds.min.x,
        bounds.min.y,
        bounds.width(),
        bounds.height()
    );
    for poly in polylines {
        if poly.points.is_empty() {
            continue;
        }
        svg.push_str("<path d=\"");
        svg.push_str(&format!("M {} {}", poly.points[0].x, poly.points[0].y));
        for pt in &poly.points[1..] {
            svg.push_str(&format!(" L {} {}", pt.x, pt.y));
        }
        if poly.closed {
            svg.push_str(" Z");
        }
        svg.push_str("\"/>");
    }
    svg.push_str("</svg>");

    let path = std::path::Path::new(&dir).join(file_name);
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(path, svg);
}
