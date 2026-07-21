//! Travel segment insertion between disconnected path segments.

use crate::plan::{PlanSegment, ScanDirection};
use beambench_common::geometry::Point2D;

const POSITION_TOLERANCE_MM: f64 = 0.001;

/// Insert Travel segments between disconnected segments.
///
/// A travel is needed when the endpoint of one segment doesn't match the start of the next.
/// This function examines all consecutive segment pairs and inserts Travel segments where
/// there are gaps in the path continuity.
pub fn insert_travel_segments(segments: Vec<PlanSegment>) -> Vec<PlanSegment> {
    if segments.is_empty() {
        return vec![];
    }

    let mut result = Vec::new();

    for (i, segment) in segments.into_iter().enumerate() {
        // Add the current segment
        result.push(segment.clone());

        // Check if we need to insert travel after this segment
        if i < result.len() - 1 {
            // We'll check against the NEXT segment that will be added
            // But we need to wait until we have it
        }
    }

    // Now do a second pass to insert travel segments
    let mut final_result = Vec::new();
    let original_segments = result;

    for (i, segment) in original_segments.iter().enumerate() {
        final_result.push(segment.clone());

        // Check if there's a next segment
        if i + 1 < original_segments.len() {
            let next_segment = &original_segments[i + 1];

            if let (Some(current_end), Some(next_start)) =
                (segment_end(segment), segment_start(next_segment))
            {
                let distance = current_end.distance_to(&next_start);

                if distance > POSITION_TOLERANCE_MM {
                    // Insert a travel segment
                    final_result.push(PlanSegment::Travel {
                        start: current_end,
                        end: next_start,
                    });
                }
            }
        }
    }

    final_result
}

/// Transform a local-space raster point to world space using scan_angle + scan_origin.
fn local_to_world(local: Point2D, scan_angle_deg: f64, scan_origin: &Point2D) -> Point2D {
    if scan_angle_deg.abs() < 0.5 {
        return local;
    }
    let rad = scan_angle_deg.to_radians();
    let cos_a = rad.cos();
    let sin_a = rad.sin();
    Point2D::new(
        scan_origin.x + local.x * cos_a - local.y * sin_a,
        scan_origin.y + local.x * sin_a + local.y * cos_a,
    )
}

/// Reorder segments using a greedy nearest-neighbor heuristic to minimize travel distance.
///
/// Starting from `start_pos`, picks the segment whose start point is closest to the current position,
/// then advances to that segment's end point. Segments without a computable start point (e.g. OffsetFill)
/// are appended at the end in their original order.
///
/// When `reduce_direction_changes` is true, the cost function is biased by
/// the angle between the previous exit heading and the candidate's entry
/// direction. A π turn (180°) adds [`ANGLE_PENALTY_MM_PER_RAD`] × π to the
/// distance cost; small-angle turns are close to free. The first pick
/// uses pure distance since there's no prior heading.
pub fn reorder_segments_nearest_neighbor(
    segments: Vec<PlanSegment>,
    start_pos: Point2D,
    reduce_direction_changes: bool,
) -> Vec<PlanSegment> {
    if segments.len() <= 1 {
        return segments;
    }

    let mut remaining: Vec<(usize, PlanSegment)> = segments.into_iter().enumerate().collect();
    let mut result = Vec::with_capacity(remaining.len());
    let mut deferred = Vec::new();
    let mut current_pos = start_pos;
    let mut prev_heading: Option<Point2D> = None;

    // Separate segments without a start point (OffsetFill)
    remaining.retain(|(_, seg)| {
        if segment_start(seg).is_none() {
            deferred.push(seg.clone());
            false
        } else {
            true
        }
    });

    while !remaining.is_empty() {
        let (best_idx, _) = remaining
            .iter()
            .enumerate()
            .min_by(|(_, (_, a)), (_, (_, b))| {
                let ca = candidate_cost(a, current_pos, prev_heading, reduce_direction_changes);
                let cb = candidate_cost(b, current_pos, prev_heading, reduce_direction_changes);
                ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();

        let (_, seg) = remaining.remove(best_idx);
        // Advance current position and capture this segment's exit heading
        // for the next round's direction-change penalty.
        if reduce_direction_changes {
            prev_heading = exit_heading(&seg);
        }
        if let Some(end) = segment_end(&seg) {
            current_pos = end;
        }
        result.push(seg);
    }

    // Append deferred segments (OffsetFill) at end
    result.extend(deferred);
    result
}

/// Cost penalty per radian of direction change, expressed in millimeters.
/// A π-radian (180°) turn contributes π × `ANGLE_PENALTY_MM_PER_RAD`
/// (~31 mm at the current value) to the candidate cost. Tuned so small
/// direction changes are nearly free but reversals meaningfully dominate
/// short-distance ties.
const ANGLE_PENALTY_MM_PER_RAD: f64 = 10.0;

/// Compute a candidate segment's selection cost given the current state.
///
/// Returns `f64::INFINITY` for segments with no start point (treated as
/// unreachable under distance-based selection; these are pre-filtered).
fn candidate_cost(
    segment: &PlanSegment,
    current_pos: Point2D,
    prev_heading: Option<Point2D>,
    reduce_direction_changes: bool,
) -> f64 {
    let Some(start) = segment_start(segment) else {
        return f64::INFINITY;
    };
    let distance = current_pos.distance_to(&start);
    if !reduce_direction_changes {
        return distance;
    }
    let Some(prev) = prev_heading else {
        return distance;
    };
    let Some(entry) = entry_heading(segment) else {
        return distance;
    };
    let angle = angle_between(prev, entry);
    distance + ANGLE_PENALTY_MM_PER_RAD * angle
}

/// Unit vector pointing from the candidate's start to its next point —
/// the heading the tool would travel when the segment begins cutting.
/// Returns `None` when no interior point exists to define a direction.
fn entry_heading(segment: &PlanSegment) -> Option<Point2D> {
    match segment {
        PlanSegment::Vector { polyline, .. } if polyline.len() >= 2 => {
            let a = polyline[0];
            let b = polyline[1];
            unit_vector(a, b)
        }
        PlanSegment::Frame { path, .. } if path.len() >= 2 => {
            let a = path[0];
            let b = path[1];
            unit_vector(a, b)
        }
        // For Raster/Travel/OffsetFill we don't have a meaningful entry
        // direction for this heuristic; fall back to distance-only.
        _ => None,
    }
}

/// Unit vector pointing from the segment's second-to-last point to its
/// last — the tool's heading as it leaves the segment. Returns `None`
/// when the segment has no defined exit direction.
fn exit_heading(segment: &PlanSegment) -> Option<Point2D> {
    match segment {
        PlanSegment::Vector { polyline, .. } if polyline.len() >= 2 => {
            let n = polyline.len();
            unit_vector(polyline[n - 2], polyline[n - 1])
        }
        PlanSegment::Frame { path, .. } if path.len() >= 2 => {
            let n = path.len();
            unit_vector(path[n - 2], path[n - 1])
        }
        _ => None,
    }
}

fn unit_vector(from: Point2D, to: Point2D) -> Option<Point2D> {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let mag = (dx * dx + dy * dy).sqrt();
    if mag < 1e-9 {
        None
    } else {
        Some(Point2D::new(dx / mag, dy / mag))
    }
}

/// Unsigned angle between two unit vectors in the range `[0, π]`.
fn angle_between(a: Point2D, b: Point2D) -> f64 {
    let dot = (a.x * b.x + a.y * b.y).clamp(-1.0, 1.0);
    dot.acos()
}

/// Get the starting point of a segment.
///
/// For raster segments, returns the overscan-adjusted start position
/// (os_start) matching where the G-code emitter actually rapids to.
pub(crate) fn segment_start(segment: &PlanSegment) -> Option<Point2D> {
    match segment {
        PlanSegment::Travel { start, .. } => Some(*start),
        PlanSegment::Vector { polyline, .. } => polyline.first().copied(),
        PlanSegment::Frame { path, .. } => path.first().copied(),
        PlanSegment::Raster {
            scanlines,
            scan_angle_deg,
            scan_origin,
            overscan_mm,
            ..
        } => scanlines.first().and_then(|line| {
            line.runs.first().map(|run| {
                let (start_pos, end_pos) = match line.direction {
                    ScanDirection::LeftToRight => (run.start_x_mm, run.end_x_mm),
                    ScanDirection::RightToLeft => (run.end_x_mm, run.start_x_mm),
                };
                let dir_sign = if start_pos <= end_pos { 1.0 } else { -1.0 };
                let os_start = start_pos - dir_sign * overscan_mm;
                local_to_world(
                    Point2D::new(os_start, line.y_mm),
                    *scan_angle_deg,
                    scan_origin,
                )
            })
        }),
        PlanSegment::OffsetFill { .. } => None,
    }
}

/// Get the ending point of a segment.
///
/// For raster segments, returns the overscan-adjusted end position
/// (os_end) matching where the G-code emitter actually finishes.
pub(crate) fn segment_end(segment: &PlanSegment) -> Option<Point2D> {
    match segment {
        PlanSegment::Travel { end, .. } => Some(*end),
        PlanSegment::Vector { polyline, .. } => polyline.last().copied(),
        PlanSegment::Frame { path, .. } => path.last().copied(),
        PlanSegment::Raster {
            scanlines,
            scan_angle_deg,
            scan_origin,
            overscan_mm,
            ..
        } => scanlines.last().and_then(|line| {
            line.runs.last().map(|run| {
                let (start_pos, end_pos) = match line.direction {
                    ScanDirection::LeftToRight => (run.start_x_mm, run.end_x_mm),
                    ScanDirection::RightToLeft => (run.end_x_mm, run.start_x_mm),
                };
                let dir_sign = if start_pos <= end_pos { 1.0 } else { -1.0 };
                let os_end = end_pos + dir_sign * overscan_mm;
                local_to_world(
                    Point2D::new(os_end, line.y_mm),
                    *scan_angle_deg,
                    scan_origin,
                )
            })
        }),
        PlanSegment::OffsetFill { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{DirectionMode, PowerMode, ScanAxis, ScanDirection, ScanRun, Scanline};

    #[test]
    fn no_segments_returns_empty() {
        let result = insert_travel_segments(vec![]);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn single_segment_no_travel_added() {
        let segments = vec![PlanSegment::Vector {
            polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)],
            closed: false,
            power_percent: 80.0,
            speed_mm_min: 1000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        }];

        let result = insert_travel_segments(segments);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn connected_segments_no_travel() {
        let segments = vec![
            PlanSegment::Vector {
                polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)],
                closed: false,
                power_percent: 80.0,
                speed_mm_min: 1000.0,
                layer_id: "layer1".to_string(),
                cut_entry_id: String::new(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
            PlanSegment::Vector {
                polyline: vec![Point2D::new(10.0, 10.0), Point2D::new(20.0, 20.0)],
                closed: false,
                power_percent: 80.0,
                speed_mm_min: 1000.0,
                layer_id: "layer1".to_string(),
                cut_entry_id: String::new(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
        ];

        let result = insert_travel_segments(segments);
        assert_eq!(result.len(), 2); // No travel inserted
    }

    #[test]
    fn disconnected_segments_travel_inserted() {
        let segments = vec![
            PlanSegment::Vector {
                polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)],
                closed: false,
                power_percent: 80.0,
                speed_mm_min: 1000.0,
                layer_id: "layer1".to_string(),
                cut_entry_id: String::new(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
            PlanSegment::Vector {
                polyline: vec![Point2D::new(50.0, 50.0), Point2D::new(60.0, 60.0)],
                closed: false,
                power_percent: 80.0,
                speed_mm_min: 1000.0,
                layer_id: "layer1".to_string(),
                cut_entry_id: String::new(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
        ];

        let result = insert_travel_segments(segments);

        assert_eq!(result.len(), 3); // 2 vectors + 1 travel
        match &result[1] {
            PlanSegment::Travel { start, end } => {
                assert_eq!(start.x, 10.0);
                assert_eq!(start.y, 10.0);
                assert_eq!(end.x, 50.0);
                assert_eq!(end.y, 50.0);
            }
            _ => panic!("Expected Travel segment"),
        }
    }

    #[test]
    fn mixed_connected_and_disconnected() {
        let segments = vec![
            PlanSegment::Vector {
                polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)],
                closed: false,
                power_percent: 80.0,
                speed_mm_min: 1000.0,
                layer_id: "layer1".to_string(),
                cut_entry_id: String::new(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
            PlanSegment::Vector {
                polyline: vec![Point2D::new(10.0, 0.0), Point2D::new(20.0, 0.0)],
                closed: false,
                power_percent: 80.0,
                speed_mm_min: 1000.0,
                layer_id: "layer1".to_string(),
                cut_entry_id: String::new(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
            PlanSegment::Vector {
                polyline: vec![Point2D::new(100.0, 0.0), Point2D::new(110.0, 0.0)],
                closed: false,
                power_percent: 80.0,
                speed_mm_min: 1000.0,
                layer_id: "layer1".to_string(),
                cut_entry_id: String::new(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
        ];

        let result = insert_travel_segments(segments);

        assert_eq!(result.len(), 4); // 3 vectors + 1 travel (between second and third)
    }

    #[test]
    fn tolerance_within_threshold() {
        let segments = vec![
            PlanSegment::Vector {
                polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)],
                closed: false,
                power_percent: 80.0,
                speed_mm_min: 1000.0,
                layer_id: "layer1".to_string(),
                cut_entry_id: String::new(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
            PlanSegment::Vector {
                polyline: vec![Point2D::new(10.0001, 10.0001), Point2D::new(20.0, 20.0)],
                closed: false,
                power_percent: 80.0,
                speed_mm_min: 1000.0,
                layer_id: "layer1".to_string(),
                cut_entry_id: String::new(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
        ];

        let result = insert_travel_segments(segments);
        // Distance is ~0.00014mm, which is less than 0.001mm tolerance
        assert_eq!(result.len(), 2); // No travel inserted
    }

    #[test]
    fn travel_segment_endpoints() {
        let segments = vec![
            PlanSegment::Travel {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(10.0, 10.0),
            },
            PlanSegment::Travel {
                start: Point2D::new(50.0, 50.0),
                end: Point2D::new(60.0, 60.0),
            },
        ];

        let result = insert_travel_segments(segments);

        assert_eq!(result.len(), 3); // 2 travels + 1 inserted
    }

    #[test]
    fn frame_segment_endpoints() {
        let segments = vec![
            PlanSegment::Frame {
                path: vec![
                    Point2D::new(0.0, 0.0),
                    Point2D::new(10.0, 0.0),
                    Point2D::new(10.0, 10.0),
                ],
                power_percent: 10.0,
                speed_mm_min: 3000.0,
            },
            PlanSegment::Frame {
                path: vec![Point2D::new(50.0, 50.0), Point2D::new(60.0, 60.0)],
                power_percent: 10.0,
                speed_mm_min: 3000.0,
            },
        ];

        let result = insert_travel_segments(segments);

        assert_eq!(result.len(), 3); // 2 frames + 1 travel
    }

    #[test]
    fn raster_segment_endpoints() {
        let segments = vec![
            PlanSegment::Raster {
                scanlines: vec![Scanline {
                    y_mm: 0.0,
                    runs: vec![ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 10.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::LeftToRight,
                }],
                line_interval_mm: 0.1,
                direction_mode: DirectionMode::Bidirectional,
                power_mode: PowerMode::Binary,
                speed_mm_min: 2000.0,
                layer_id: "layer1".to_string(),
                cut_entry_id: String::new(),
                scan_angle_deg: 0.0,
                scan_origin: Point2D::new(0.0, 0.0),
                overscan_mm: 0.0,
                outlines: vec![],
                scan_axis: ScanAxis::default(),
                power_max_percent: 100.0,
                power_min_percent: 0.0,
                dot_width_correction_mm: 0.0,
                ramp_length_mm: 0.0,
                x_pixel_mm: 0.0,
            },
            PlanSegment::Vector {
                polyline: vec![Point2D::new(50.0, 50.0), Point2D::new(60.0, 60.0)],
                closed: false,
                power_percent: 80.0,
                speed_mm_min: 1000.0,
                layer_id: "layer1".to_string(),
                cut_entry_id: String::new(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
        ];

        let result = insert_travel_segments(segments);

        assert_eq!(result.len(), 3); // Raster + Travel + Vector
        match &result[1] {
            PlanSegment::Travel { start, end } => {
                assert_eq!(start.x, 10.0); // End of raster run
                assert_eq!(start.y, 0.0); // Y of scanline
                assert_eq!(end.x, 50.0);
                assert_eq!(end.y, 50.0);
            }
            _ => panic!("Expected Travel segment"),
        }
    }

    #[test]
    fn empty_polyline_handled() {
        let segments = vec![
            PlanSegment::Vector {
                polyline: vec![],
                closed: false,
                power_percent: 80.0,
                speed_mm_min: 1000.0,
                layer_id: "layer1".to_string(),
                cut_entry_id: String::new(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
            PlanSegment::Vector {
                polyline: vec![Point2D::new(10.0, 10.0)],
                closed: false,
                power_percent: 80.0,
                speed_mm_min: 1000.0,
                layer_id: "layer1".to_string(),
                cut_entry_id: String::new(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
        ];

        let result = insert_travel_segments(segments);
        // Empty polyline has no start/end, so no travel can be computed
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn raster_multiple_scanlines() {
        let segments = vec![PlanSegment::Raster {
            scanlines: vec![
                Scanline {
                    y_mm: 0.0,
                    runs: vec![ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 10.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::LeftToRight,
                },
                Scanline {
                    y_mm: 0.1,
                    runs: vec![ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 10.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::RightToLeft,
                },
            ],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }];

        // Get the end point — last scanline is RTL, so head exits at start_x_mm (left edge)
        let end = segment_end(&segments[0]);
        assert!(end.is_some());
        let end = end.unwrap();
        assert_eq!(end.x, 0.0); // RTL: exits at start_x_mm (left edge)
        assert_eq!(end.y, 0.1); // Y of last scanline
    }

    #[test]
    fn raster_rtl_scanline_endpoints_respect_direction() {
        // RTL scanline: head starts at end_x (right) and exits at start_x (left)
        let rtl_scanline = Scanline {
            y_mm: 5.0,
            runs: vec![ScanRun {
                start_x_mm: 2.0,
                end_x_mm: 18.0,
                power_values: vec![],
            }],
            direction: ScanDirection::RightToLeft,
        };

        let segments = vec![PlanSegment::Raster {
            scanlines: vec![rtl_scanline],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }];

        // RTL: start is at end_x_mm (rightmost), end is at start_x_mm (leftmost)
        let start = segment_start(&segments[0]).unwrap();
        assert_eq!(
            start.x, 18.0,
            "RTL start should be at right edge (end_x_mm)"
        );

        let end = segment_end(&segments[0]).unwrap();
        assert_eq!(end.x, 2.0, "RTL end should be at left edge (start_x_mm)");
    }

    #[test]
    fn raster_ltr_scanline_endpoints_unchanged() {
        let ltr_scanline = Scanline {
            y_mm: 5.0,
            runs: vec![ScanRun {
                start_x_mm: 2.0,
                end_x_mm: 18.0,
                power_values: vec![],
            }],
            direction: ScanDirection::LeftToRight,
        };

        let segments = vec![PlanSegment::Raster {
            scanlines: vec![ltr_scanline],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Unidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }];

        let start = segment_start(&segments[0]).unwrap();
        assert_eq!(
            start.x, 2.0,
            "LTR start should be at left edge (start_x_mm)"
        );

        let end = segment_end(&segments[0]).unwrap();
        assert_eq!(end.x, 18.0, "LTR end should be at right edge (end_x_mm)");
    }

    #[test]
    fn reorder_empty_returns_empty() {
        let result = reorder_segments_nearest_neighbor(vec![], Point2D::new(0.0, 0.0), false);
        assert!(result.is_empty());
    }

    #[test]
    fn reorder_single_segment_unchanged() {
        let seg = PlanSegment::Vector {
            polyline: vec![Point2D::new(5.0, 5.0), Point2D::new(10.0, 10.0)],
            closed: false,
            power_percent: 80.0,
            speed_mm_min: 1000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        };
        let result =
            reorder_segments_nearest_neighbor(vec![seg.clone()], Point2D::new(0.0, 0.0), false);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn reorder_picks_nearest_first() {
        // Seg A starts at (0,0), Seg B starts at (90,90)
        let seg_a = PlanSegment::Vector {
            polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)],
            closed: false,
            power_percent: 80.0,
            speed_mm_min: 1000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        };
        let seg_b = PlanSegment::Vector {
            polyline: vec![Point2D::new(90.0, 90.0), Point2D::new(100.0, 100.0)],
            closed: false,
            power_percent: 80.0,
            speed_mm_min: 1000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        };
        // Start at (0,0) => seg_a should come first
        let result =
            reorder_segments_nearest_neighbor(vec![seg_b, seg_a], Point2D::new(0.0, 0.0), false);
        // First segment should start at (0,0)
        match &result[0] {
            PlanSegment::Vector { polyline, .. } => assert_eq!(polyline[0], Point2D::new(0.0, 0.0)),
            _ => panic!("Expected Vector segment"),
        }
    }

    #[test]
    fn reorder_with_custom_start() {
        let seg_a = PlanSegment::Vector {
            polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)],
            closed: false,
            power_percent: 80.0,
            speed_mm_min: 1000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        };
        let seg_b = PlanSegment::Vector {
            polyline: vec![Point2D::new(90.0, 90.0), Point2D::new(100.0, 100.0)],
            closed: false,
            power_percent: 80.0,
            speed_mm_min: 1000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        };
        // Start at (95,95) => seg_b should come first
        let result =
            reorder_segments_nearest_neighbor(vec![seg_a, seg_b], Point2D::new(95.0, 95.0), false);
        match &result[0] {
            PlanSegment::Vector { polyline, .. } => {
                assert_eq!(polyline[0], Point2D::new(90.0, 90.0))
            }
            _ => panic!("Expected Vector segment"),
        }
    }

    #[test]
    fn raster_ltr_overscan_adjusts_start_and_end() {
        // LTR run: burn 2→18, overscan 3mm
        // os_start = 2 - 1*3 = -1, os_end = 18 + 1*3 = 21
        let segments = vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 5.0,
                runs: vec![ScanRun {
                    start_x_mm: 2.0,
                    end_x_mm: 18.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Unidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 3.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }];

        let start = segment_start(&segments[0]).unwrap();
        assert!(
            (start.x - (-1.0)).abs() < 1e-10,
            "LTR os_start should be 2 - 3 = -1, got {}",
            start.x
        );

        let end = segment_end(&segments[0]).unwrap();
        assert!(
            (end.x - 21.0).abs() < 1e-10,
            "LTR os_end should be 18 + 3 = 21, got {}",
            end.x
        );
    }

    #[test]
    fn raster_rtl_overscan_adjusts_start_and_end() {
        // RTL run: burn 5→25, start_pos=25, end_pos=5, dir_sign=-1
        // os_start = 25 - (-1)*3 = 28, os_end = 5 + (-1)*3 = 2
        let segments = vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 25.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::RightToLeft,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 3.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }];

        let start = segment_start(&segments[0]).unwrap();
        assert!(
            (start.x - 28.0).abs() < 1e-10,
            "RTL os_start should be 25 + 3 = 28, got {}",
            start.x
        );

        let end = segment_end(&segments[0]).unwrap();
        assert!(
            (end.x - 2.0).abs() < 1e-10,
            "RTL os_end should be 5 - 3 = 2, got {}",
            end.x
        );
    }

    #[test]
    fn travel_inserted_to_overscan_position() {
        // Vector ends at (50,50), raster starts LTR with overscan 5mm at x=10
        // Travel should go to os_start = 10 - 5 = 5, not burn start 10
        let segments = vec![
            PlanSegment::Vector {
                polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(50.0, 50.0)],
                closed: false,
                power_percent: 80.0,
                speed_mm_min: 1000.0,
                layer_id: "l1".to_string(),
                cut_entry_id: String::new(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
            PlanSegment::Raster {
                scanlines: vec![Scanline {
                    y_mm: 0.0,
                    runs: vec![ScanRun {
                        start_x_mm: 10.0,
                        end_x_mm: 90.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::LeftToRight,
                }],
                line_interval_mm: 0.1,
                direction_mode: DirectionMode::Unidirectional,
                power_mode: PowerMode::Binary,
                speed_mm_min: 2000.0,
                layer_id: "l1".to_string(),
                cut_entry_id: String::new(),
                scan_angle_deg: 0.0,
                scan_origin: Point2D::new(0.0, 0.0),
                overscan_mm: 5.0,
                outlines: vec![],
                scan_axis: ScanAxis::default(),
                power_max_percent: 100.0,
                power_min_percent: 0.0,
                dot_width_correction_mm: 0.0,
                ramp_length_mm: 0.0,
                x_pixel_mm: 0.0,
            },
        ];

        let result = insert_travel_segments(segments);
        assert_eq!(result.len(), 3); // Vector + Travel + Raster
        match &result[1] {
            PlanSegment::Travel { start, end } => {
                assert_eq!(start.x, 50.0);
                assert_eq!(start.y, 50.0);
                assert!(
                    (end.x - 5.0).abs() < 1e-10,
                    "Travel should target os_start=5, got {}",
                    end.x
                );
                assert_eq!(end.y, 0.0);
            }
            _ => panic!("Expected Travel segment"),
        }
    }

    fn vec_seg(pts: Vec<Point2D>) -> PlanSegment {
        PlanSegment::Vector {
            polyline: pts,
            closed: false,
            power_percent: 80.0,
            speed_mm_min: 1000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        }
    }

    #[test]
    fn reduce_direction_changes_prefers_straight_continuation() {
        // Three horizontal segments end-to-end. The "straight" continuation of
        // the first (heading +X) is the rightward segment. A reversing segment
        // with identical distance should lose to the straight one under the
        // angle penalty.
        // seg_lead: ends at (10,0) heading +X
        // seg_straight: starts at (11,0) heading +X  — straight ahead, distance 1mm
        // seg_reverse: starts at (11,0) heading -X  — reversal, distance 1mm
        let lead = vec_seg(vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)]);
        let straight = vec_seg(vec![Point2D::new(11.0, 0.0), Point2D::new(21.0, 0.0)]);
        let reverse = vec_seg(vec![Point2D::new(11.0, 0.0), Point2D::new(1.5, 0.0)]);

        // Without direction bias, tie-break is by iteration order; we put
        // `reverse` first to show the bias flips the choice.
        let biased = reorder_segments_nearest_neighbor(
            vec![lead.clone(), reverse.clone(), straight.clone()],
            Point2D::new(0.0, 0.0),
            true,
        );
        // After `lead` at position 0, the biased picker should prefer
        // `straight` (same heading) over `reverse` (180° turn).
        match &biased[1] {
            PlanSegment::Vector { polyline, .. } => {
                assert_eq!(
                    polyline[1],
                    Point2D::new(21.0, 0.0),
                    "expected straight continuation, got {:?}",
                    polyline
                );
            }
            _ => panic!("expected Vector"),
        }
    }

    #[test]
    fn reduce_direction_changes_off_matches_pure_distance() {
        // When the flag is off, the picker collapses to pure distance —
        // identical to the default behavior exercised by
        // `reorder_picks_nearest_first`.
        let seg_a = vec_seg(vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)]);
        let seg_b = vec_seg(vec![Point2D::new(90.0, 90.0), Point2D::new(100.0, 100.0)]);
        let result =
            reorder_segments_nearest_neighbor(vec![seg_b, seg_a], Point2D::new(0.0, 0.0), false);
        match &result[0] {
            PlanSegment::Vector { polyline, .. } => assert_eq!(polyline[0], Point2D::new(0.0, 0.0)),
            _ => panic!("expected Vector"),
        }
    }

    #[test]
    fn reduce_direction_changes_first_pick_uses_distance() {
        // First pick has no prior heading → falls back to pure distance.
        // Nearest-to-origin should still win even with the flag on.
        let near = vec_seg(vec![Point2D::new(1.0, 0.0), Point2D::new(5.0, 0.0)]);
        let far = vec_seg(vec![Point2D::new(50.0, 0.0), Point2D::new(60.0, 0.0)]);
        let result =
            reorder_segments_nearest_neighbor(vec![far, near], Point2D::new(0.0, 0.0), true);
        match &result[0] {
            PlanSegment::Vector { polyline, .. } => assert_eq!(polyline[0], Point2D::new(1.0, 0.0)),
            _ => panic!("expected Vector"),
        }
    }

    #[test]
    fn angle_between_is_zero_for_parallel() {
        let a = Point2D::new(1.0, 0.0);
        let b = Point2D::new(1.0, 0.0);
        assert!(angle_between(a, b).abs() < 1e-9);
    }

    #[test]
    fn angle_between_is_pi_for_opposite() {
        let a = Point2D::new(1.0, 0.0);
        let b = Point2D::new(-1.0, 0.0);
        assert!((angle_between(a, b) - std::f64::consts::PI).abs() < 1e-9);
    }

    #[test]
    fn angle_between_is_half_pi_for_perpendicular() {
        let a = Point2D::new(1.0, 0.0);
        let b = Point2D::new(0.0, 1.0);
        assert!((angle_between(a, b) - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    }

    #[test]
    fn reorder_preserves_offset_fill() {
        let seg_v = PlanSegment::Vector {
            polyline: vec![Point2D::new(5.0, 5.0), Point2D::new(10.0, 10.0)],
            closed: false,
            power_percent: 80.0,
            speed_mm_min: 1000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        };
        let seg_of = PlanSegment::OffsetFill {
            layer_id: "l1".to_string(),
            object_id: "obj1".to_string(),
            offset_mm: 0.5,
            angle_deg: 45.0,
        };
        let result = reorder_segments_nearest_neighbor(
            vec![seg_of.clone(), seg_v],
            Point2D::new(0.0, 0.0),
            false,
        );
        // Vector should come first (it has a start point), OffsetFill last
        assert_eq!(result.len(), 2);
        assert!(matches!(&result[0], PlanSegment::Vector { .. }));
        assert!(matches!(&result[1], PlanSegment::OffsetFill { .. }));
    }
}
