//! Kerf must affect the shared execution plan, including preview and output.

use beambench_common::StartFromMode;
use beambench_common::geometry::{Bounds, Point2D};
use beambench_core::layer::{CutEntry, OperationType};
use beambench_core::object::{ObjectData, ProjectObject, TabAnchor};
use beambench_core::{Project, WorkspaceOrigin};
use beambench_grbl::{GcodeConfig, generate_gcode};
use beambench_planner::{
    ExecutionPlan, PlanSegment, PlannerError, build_plan, calculate_plan_bounds,
};
use beambench_preview::distill_preview;

fn project(offset: f64) -> Project {
    let mut project = Project::new("Kerf");
    project.workspace.origin = WorkspaceOrigin::TopLeft;
    project.start_from = StartFromMode::AbsoluteCoords;
    project.optimization.enabled = false;
    project.ensure_default_layer();
    let layer = &mut project.layers[0];
    layer.entries = vec![CutEntry::new(OperationType::Line)];
    layer.entries[0]
        .vector_settings
        .as_mut()
        .unwrap()
        .kerf_offset_mm = offset;
    project
}

fn add_path(project: &mut Project, points: &[(f64, f64)], closed: bool) -> String {
    let mut path_data = format!("M {} {}", points[0].0, points[0].1);
    for &(x, y) in &points[1..] {
        path_data.push_str(&format!(" L {x} {y}"));
    }
    if closed {
        path_data.push_str(" Z");
    }
    let bounds = Bounds::new(
        Point2D::new(
            points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min),
            points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min),
        ),
        Point2D::new(
            points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max),
            points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max),
        ),
    );
    let object = ProjectObject::new(
        "Contour",
        project.layers[0].id,
        bounds,
        ObjectData::VectorPath {
            path_data,
            closed,
            ruler_guide_axis: None,
        },
    );
    let id = object.id.to_string();
    project.objects.push(object);
    id
}

fn square(project: &mut Project, x: f64, y: f64, size: f64, reverse: bool) -> String {
    let mut points = vec![(x, y), (x + size, y), (x + size, y + size), (x, y + size)];
    if reverse {
        points.reverse();
    }
    add_path(project, &points, true)
}

fn contours<'a>(plan: &'a ExecutionPlan, object_id: &str) -> Vec<&'a PlanSegment> {
    plan.segments.iter().filter(|segment| {
        matches!(segment, PlanSegment::Vector { source_object_id: Some(id), .. } if id == object_id)
    }).collect()
}

fn bounds(segments: &[&PlanSegment]) -> Bounds {
    calculate_plan_bounds(&segments.iter().map(|s| (*s).clone()).collect::<Vec<_>>())
}

fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() <= 0.002, "{actual} != {expected}");
}

#[test]
fn signed_offsets_alternate_for_holes_and_islands_in_either_winding() {
    for offset in [-0.1, 0.1] {
        for reverse in [false, true] {
            for optimized in [false, true] {
                let mut project = project(offset);
                project.optimization.enabled = optimized;
                project.optimization.inner_first = optimized;
                // Separate objects intentionally have identical winding.
                let outer = square(&mut project, 10.0, 10.0, 20.0, reverse);
                let hole = square(&mut project, 15.0, 15.0, 10.0, reverse);
                let island = square(&mut project, 19.0, 19.0, 2.0, !reverse);
                let plan = build_plan(&project).unwrap();
                near(
                    bounds(&contours(&plan, &outer)).width(),
                    20.0 + 2.0 * offset,
                );
                near(bounds(&contours(&plan, &hole)).width(), 10.0 - 2.0 * offset);
                near(
                    bounds(&contours(&plan, &island)).width(),
                    2.0 + 2.0 * offset,
                );
            }
        }
    }
}

#[test]
fn zero_preserves_paths_and_nonzero_leaves_open_paths_unchanged() {
    let mut project = project(0.0);
    let open = add_path(
        &mut project,
        &[(10.0, 10.0), (20.0, 10.0), (20.0, 20.0)],
        false,
    );
    let closed_id = square(&mut project, 40.0, 40.0, 10.0, false);
    let baseline = build_plan(&project).unwrap();
    let PlanSegment::Vector {
        polyline, closed, ..
    } = contours(&baseline, &closed_id)[0]
    else {
        unreachable!()
    };
    assert!(*closed);
    assert_eq!(
        *polyline,
        vec![
            Point2D::new(40.0, 40.0),
            Point2D::new(50.0, 40.0),
            Point2D::new(50.0, 50.0),
            Point2D::new(40.0, 50.0),
        ]
    );
    project.layers[0].entries[0]
        .vector_settings
        .as_mut()
        .unwrap()
        .kerf_offset_mm = 0.1;
    let compensated = build_plan(&project).unwrap();
    assert_eq!(contours(&baseline, &open), contours(&compensated, &open));
}

#[test]
fn preview_and_gcode_share_compensated_geometry_in_both_coordinate_origins() {
    for origin in [WorkspaceOrigin::TopLeft, WorkspaceOrigin::BottomLeft] {
        let mut project = project(0.1);
        project.workspace.origin = origin;
        let id = square(&mut project, 10.0, 10.0, 20.0, false);
        let artwork = project.objects.clone();
        let plan = build_plan(&project).unwrap();
        assert_eq!(project.objects, artwork);
        near(bounds(&contours(&plan, &id)).width(), 20.2);
        let preview = distill_preview(&plan);
        let PlanSegment::Vector { polyline, .. } = contours(&plan, &id)[0] else {
            unreachable!()
        };
        assert_eq!(preview.layers[0].vector_paths[0].points, *polyline);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        for point in polyline {
            assert!(
                gcode.iter().any(|line| {
                    let word = |prefix: char| {
                        line.split_whitespace()
                            .find_map(|s| s.strip_prefix(prefix)?.parse::<f64>().ok())
                    };
                    line.starts_with("G1 ")
                        && word('X').is_some_and(|x| (x - point.x).abs() < 0.002)
                        && word('Y').is_some_and(|y| (y - point.y).abs() < 0.002)
                }),
                "missing compensated machine point {point:?}: {gcode:?}"
            );
        }
    }
}

#[test]
fn each_sublayer_and_pass_uses_its_own_offset_without_accumulating() {
    let mut project = project(0.0);
    let id = square(&mut project, 10.0, 10.0, 20.0, false);
    let mut second = CutEntry::new(OperationType::Line);
    second.power_percent = 77.0;
    let settings = second.vector_settings.as_mut().unwrap();
    settings.kerf_offset_mm = 0.1;
    settings.passes = 2;
    let second_id = second.id.to_string();
    project.layers[0].entries.push(second);
    let plan = build_plan(&project).unwrap();
    let paths = contours(&plan, &id);
    assert_eq!(paths.len(), 3);
    near(bounds(&paths[..1]).width(), 20.0);
    for segment in &paths[1..] {
        near(bounds(&[segment]).width(), 20.2);
        assert!(
            matches!(segment, PlanSegment::Vector { cut_entry_id, power_percent, .. } if cut_entry_id == &second_id && *power_percent == 77.0)
        );
    }
}

#[test]
fn tabs_are_split_on_the_compensated_contour_with_their_original_identity() {
    for positioned in [false, true] {
        let mut project = project(0.1);
        let id = square(&mut project, 10.0, 10.0, 20.0, false);
        let settings = project.layers[0].entries[0]
            .vector_settings
            .as_mut()
            .unwrap();
        settings.tab_width_mm = 2.0;
        settings.tab_count = if positioned { 0 } else { 4 };
        if positioned {
            project.objects[0].tabs.push(TabAnchor {
                subpath_index: 0,
                position: 0.125,
            });
        }
        let plan = build_plan(&project).unwrap();
        let paths = contours(&plan, &id);
        assert!(!paths.is_empty());
        near(bounds(&paths).width(), 20.2);
        let mut burn_length = 0.0;
        for path in paths {
            let PlanSegment::Vector {
                polyline,
                closed,
                source_subpath_index,
                cut_entry_id,
                ..
            } = path
            else {
                unreachable!()
            };
            assert!(!closed);
            assert_eq!(*source_subpath_index, Some(0));
            assert_eq!(cut_entry_id, &project.layers[0].entries[0].id.to_string());
            burn_length += polyline
                .windows(2)
                .map(|w| w[0].distance_to(&w[1]))
                .sum::<f64>();
        }
        near(burn_length, 80.8 - if positioned { 2.0 } else { 8.0 });
    }
}

#[test]
fn concave_contours_work_and_crossing_edges_do_not_create_false_holes() {
    let mut project = project(0.1);
    let u = add_path(
        &mut project,
        &[
            (10.0, 10.0),
            (20.0, 10.0),
            (20.0, 20.0),
            (16.0, 20.0),
            (16.0, 14.0),
            (14.0, 14.0),
            (14.0, 20.0),
            (10.0, 20.0),
        ],
        true,
    );
    // Every triangle vertex is inside the U, but its edge crosses the notch.
    let triangle = add_path(
        &mut project,
        &[(12.0, 18.0), (15.0, 12.0), (18.0, 18.0)],
        true,
    );
    let first = square(&mut project, 40.0, 10.0, 10.0, false);
    let duplicate = square(&mut project, 40.0, 10.0, 10.0, true);
    let hole = square(&mut project, 42.0, 12.0, 6.0, false);
    let plan = build_plan(&project).unwrap();
    near(bounds(&contours(&plan, &u)).width(), 10.2);
    assert!(bounds(&contours(&plan, &triangle)).width() > 6.0);
    near(bounds(&contours(&plan, &first)).width(), 10.2);
    near(bounds(&contours(&plan, &duplicate)).width(), 10.2);
    near(bounds(&contours(&plan, &hole)).width(), 5.8);
}

#[test]
fn acute_corners_use_a_bounded_miter() {
    let mut project = project(0.1);
    let source = [(10.0, 30.0), (20.0, 10.0), (20.1, 30.0)];
    let id = add_path(&mut project, &source, true);
    let plan = build_plan(&project).unwrap();
    let PlanSegment::Vector { polyline, .. } = contours(&plan, &id)[0] else {
        unreachable!()
    };
    for point in polyline {
        assert!(
            source
                .iter()
                .any(|&(x, y)| point.distance_to(&Point2D::new(x, y)) < 0.21),
            "unbounded miter at {point:?}"
        );
    }
}

#[test]
fn collapsed_and_split_contours_stop_planning() {
    let mut collapsed = project(-0.1);
    let id = square(&mut collapsed, 10.0, 10.0, 0.1, false);
    let error = build_plan(&collapsed).unwrap_err();
    assert!(matches!(error, PlannerError::InvalidSettings(_)));
    assert!(error.to_string().contains(&id));
    let mut split = project(-0.6);
    add_path(
        &mut split,
        &[
            (10.0, 10.0),
            (20.0, 10.0),
            (20.0, 14.5),
            (30.0, 14.5),
            (30.0, 10.0),
            (40.0, 10.0),
            (40.0, 20.0),
            (30.0, 20.0),
            (30.0, 15.5),
            (20.0, 15.5),
            (20.0, 20.0),
            (10.0, 20.0),
        ],
        true,
    );
    assert!(matches!(
        build_plan(&split),
        Err(PlannerError::InvalidSettings(_))
    ));
}

#[test]
fn compensated_paths_are_checked_against_machine_bounds() {
    let mut project = project(1.0);
    square(&mut project, 0.05, 0.05, 20.0, false);
    assert!(matches!(
        build_plan(&project),
        Err(PlannerError::BoundsExceeded(_))
    ));
}

#[test]
fn kerf_does_not_modify_offset_fill_paths() {
    let mut project = project(0.0);
    project.layers[0].entries = vec![CutEntry::new(OperationType::OffsetFill)];
    square(&mut project, 10.0, 10.0, 4.0, false);
    let baseline = build_plan(&project).unwrap();
    project.layers[0].entries[0]
        .vector_settings
        .as_mut()
        .unwrap()
        .kerf_offset_mm = 0.1;
    assert_eq!(baseline.segments, build_plan(&project).unwrap().segments);
}

#[test]
fn legacy_cut_and_score_settings_also_compensate_closed_paths() {
    for operation in [OperationType::Cut, OperationType::Score] {
        let mut project = project(0.1);
        project.layers[0].entries[0].operation = operation;
        let id = square(&mut project, 10.0, 10.0, 20.0, false);
        let plan = build_plan(&project).unwrap();
        near(bounds(&contours(&plan, &id)).width(), 20.2);
    }
}

#[test]
fn invalid_offsets_stop_planning() {
    for offset in [f64::NAN, f64::INFINITY, 1e10] {
        let mut project = project(offset);
        square(&mut project, 10.0, 10.0, 20.0, false);
        assert!(matches!(
            build_plan(&project),
            Err(PlannerError::InvalidSettings(_))
        ));
    }
}
