//! Validation functions for execution plans.

use crate::{
    error::PlannerError,
    plan::{PlanSegment, ScanAxis},
};
use beambench_common::geometry::Point2D;

/// Validate that all segments fall within workspace bounds.
///
/// Travel segments are allowed to extend beyond bed bounds by the maximum
/// overscan distance found in any raster segment, since the laser head must
/// accelerate/decelerate outside the burn region.
pub fn validate_bounds(
    segments: &[PlanSegment],
    bed_width_mm: f64,
    bed_height_mm: f64,
) -> Result<(), PlannerError> {
    // Compute the maximum distance any raster travel endpoint might extend
    // past the bed boundary. This includes:
    // 1. The overscan (acceleration zone) itself
    // 2. The scanline start position may be slightly past the object bounds
    //    due to pixel-to-mm snapping
    // We compute this as the actual minimum/maximum raster travel position
    // to get an exact tolerance.
    let max_travel_overshoot =
        segments
            .iter()
            .fold(0.0_f64, |max_overshoot, segment| match segment {
                PlanSegment::Raster {
                    scanlines,
                    overscan_mm,
                    scan_axis,
                    ..
                } => {
                    // Run positions are X world coordinates for horizontal
                    // scans and Y world coordinates for vertical scans, so
                    // overshoot is measured against the matching bed dimension.
                    let run_limit = match scan_axis {
                        ScanAxis::Horizontal => bed_width_mm,
                        ScanAxis::Vertical => bed_height_mm,
                    };
                    let mut segment_overshoot = 0.0_f64;
                    for line in scanlines {
                        for run in &line.runs {
                            let min_x = run.start_x_mm.min(run.end_x_mm) - overscan_mm;
                            let max_x = run.start_x_mm.max(run.end_x_mm) + overscan_mm;
                            segment_overshoot = segment_overshoot.max((-min_x).max(0.0));
                            segment_overshoot = segment_overshoot.max((max_x - run_limit).max(0.0));
                        }
                    }
                    max_overshoot.max(segment_overshoot)
                }
                _ => max_overshoot,
            });

    for (idx, segment) in segments.iter().enumerate() {
        match segment {
            PlanSegment::Travel { start, end } => {
                check_point_with_tolerance(
                    start,
                    bed_width_mm,
                    bed_height_mm,
                    max_travel_overshoot,
                    idx,
                    "travel start",
                )?;
                check_point_with_tolerance(
                    end,
                    bed_width_mm,
                    bed_height_mm,
                    max_travel_overshoot,
                    idx,
                    "travel end",
                )?;
            }
            PlanSegment::Vector { polyline, .. } => {
                for (pt_idx, point) in polyline.iter().enumerate() {
                    check_point(
                        point,
                        bed_width_mm,
                        bed_height_mm,
                        idx,
                        &format!("vector point {}", pt_idx),
                    )?;
                }
            }
            PlanSegment::Frame { path, .. } => {
                for (pt_idx, point) in path.iter().enumerate() {
                    check_point(
                        point,
                        bed_width_mm,
                        bed_height_mm,
                        idx,
                        &format!("frame point {}", pt_idx),
                    )?;
                }
            }
            PlanSegment::Raster {
                scanlines,
                scan_angle_deg,
                scan_axis,
                ..
            } => {
                // Skip bounds checking for rotated raster segments — their
                // scanlines are in local space (centred at scan_origin) and
                // will be transformed to world coordinates by the G-code
                // emitter using scan_angle_deg / scan_origin.
                if scan_angle_deg.abs() > 0.5 {
                    continue;
                }

                // For horizontal scans, scanline.y_mm is the Y world
                // coordinate and run positions are X world coordinates.
                // For vertical scans the axes are swapped: scanline.y_mm
                // holds the X world coordinate (bounded by bed width) and
                // run positions hold Y (bounded by bed height).
                let (scanline_limit, scanline_dim, run_limit, run_dim) = match scan_axis {
                    ScanAxis::Horizontal => (bed_height_mm, "height", bed_width_mm, "width"),
                    ScanAxis::Vertical => (bed_width_mm, "width", bed_height_mm, "height"),
                };

                for (line_idx, scanline) in scanlines.iter().enumerate() {
                    // Check cross-axis coordinate
                    if scanline.y_mm < -FP_TOLERANCE_MM {
                        return Err(PlannerError::BoundsExceeded(format!(
                            "Segment {} raster scanline {} y={} is below bed origin 0",
                            idx, line_idx, scanline.y_mm
                        )));
                    }
                    if scanline.y_mm > scanline_limit + FP_TOLERANCE_MM {
                        return Err(PlannerError::BoundsExceeded(format!(
                            "Segment {} raster scanline {} y={} exceeds bed {} {}",
                            idx, line_idx, scanline.y_mm, scanline_dim, scanline_limit
                        )));
                    }

                    // Check scan-axis ranges
                    for (run_idx, run) in scanline.runs.iter().enumerate() {
                        if run.start_x_mm < -FP_TOLERANCE_MM {
                            return Err(PlannerError::BoundsExceeded(format!(
                                "Segment {} raster scanline {} run {} start_x={} is below bed origin 0",
                                idx, line_idx, run_idx, run.start_x_mm
                            )));
                        }
                        if run.start_x_mm > run_limit + FP_TOLERANCE_MM {
                            return Err(PlannerError::BoundsExceeded(format!(
                                "Segment {} raster scanline {} run {} start_x={} exceeds bed {} {}",
                                idx, line_idx, run_idx, run.start_x_mm, run_dim, run_limit
                            )));
                        }
                        if run.end_x_mm < -FP_TOLERANCE_MM {
                            return Err(PlannerError::BoundsExceeded(format!(
                                "Segment {} raster scanline {} run {} end_x={} is below bed origin 0",
                                idx, line_idx, run_idx, run.end_x_mm
                            )));
                        }
                        if run.end_x_mm > run_limit + FP_TOLERANCE_MM {
                            return Err(PlannerError::BoundsExceeded(format!(
                                "Segment {} raster scanline {} run {} end_x={} exceeds bed {} {}",
                                idx, line_idx, run_idx, run.end_x_mm, run_dim, run_limit
                            )));
                        }
                    }
                }
            }
            PlanSegment::OffsetFill { .. } => {}
        }
    }

    Ok(())
}

/// Small tolerance for floating-point drift from geometric operations
/// (undo/redo, transform round-trips, boolean ops, etc.).
const FP_TOLERANCE_MM: f64 = 0.5;

fn check_point(
    point: &Point2D,
    bed_width_mm: f64,
    bed_height_mm: f64,
    segment_idx: usize,
    label: &str,
) -> Result<(), PlannerError> {
    check_point_with_tolerance(
        point,
        bed_width_mm,
        bed_height_mm,
        FP_TOLERANCE_MM,
        segment_idx,
        label,
    )
}

fn check_point_with_tolerance(
    point: &Point2D,
    bed_width_mm: f64,
    bed_height_mm: f64,
    tolerance: f64,
    segment_idx: usize,
    label: &str,
) -> Result<(), PlannerError> {
    if point.x < -tolerance {
        return Err(PlannerError::BoundsExceeded(format!(
            "Segment {} {} x={} is below bed origin 0",
            segment_idx, label, point.x
        )));
    }
    if point.x > bed_width_mm + tolerance {
        return Err(PlannerError::BoundsExceeded(format!(
            "Segment {} {} x={} exceeds bed width {}",
            segment_idx, label, point.x, bed_width_mm
        )));
    }
    if point.y < -tolerance {
        return Err(PlannerError::BoundsExceeded(format!(
            "Segment {} {} y={} is below bed origin 0",
            segment_idx, label, point.y
        )));
    }
    if point.y > bed_height_mm + tolerance {
        return Err(PlannerError::BoundsExceeded(format!(
            "Segment {} {} y={} exceeds bed height {}",
            segment_idx, label, point.y, bed_height_mm
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{DirectionMode, PowerMode, ScanAxis, ScanDirection, ScanRun, Scanline};

    fn horizontal_raster_segment(y_mm: f64, start_x_mm: f64, end_x_mm: f64) -> PlanSegment {
        PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm,
                runs: vec![ScanRun {
                    start_x_mm,
                    end_x_mm,
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
            scan_axis: ScanAxis::Horizontal,
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.1,
        }
    }

    #[test]
    fn validate_empty_segments_ok() {
        let result = validate_bounds(&[], 400.0, 300.0);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_segments_within_bounds_ok() {
        let segments = vec![
            PlanSegment::Travel {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(100.0, 100.0),
            },
            PlanSegment::Vector {
                polyline: vec![Point2D::new(50.0, 50.0), Point2D::new(150.0, 150.0)],
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
            PlanSegment::Frame {
                path: vec![
                    Point2D::new(0.0, 0.0),
                    Point2D::new(400.0, 0.0),
                    Point2D::new(400.0, 300.0),
                    Point2D::new(0.0, 300.0),
                ],
                power_percent: 10.0,
                speed_mm_min: 3000.0,
            },
        ];

        let result = validate_bounds(&segments, 400.0, 300.0);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_vector_segment_exceeds_width() {
        let segments = vec![PlanSegment::Vector {
            polyline: vec![Point2D::new(50.0, 50.0), Point2D::new(450.0, 100.0)],
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

        let result = validate_bounds(&segments, 400.0, 300.0);
        assert!(result.is_err());
        match result {
            Err(PlannerError::BoundsExceeded(msg)) => {
                assert!(msg.contains("exceeds bed width"));
                assert!(msg.contains("450"));
            }
            _ => panic!("Expected BoundsExceeded error"),
        }
    }

    #[test]
    fn validate_vector_segment_exceeds_height() {
        let segments = vec![PlanSegment::Vector {
            polyline: vec![Point2D::new(50.0, 50.0), Point2D::new(100.0, 350.0)],
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

        let result = validate_bounds(&segments, 400.0, 300.0);
        assert!(result.is_err());
        match result {
            Err(PlannerError::BoundsExceeded(msg)) => {
                assert!(msg.contains("exceeds bed height"));
                assert!(msg.contains("350"));
            }
            _ => panic!("Expected BoundsExceeded error"),
        }
    }

    #[test]
    fn validate_vector_segment_below_origin_reports_lower_bound() {
        let segments = vec![PlanSegment::Vector {
            polyline: vec![Point2D::new(50.0, 50.0), Point2D::new(100.0, -1.0)],
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

        let result = validate_bounds(&segments, 400.0, 300.0);
        assert!(result.is_err());
        match result {
            Err(PlannerError::BoundsExceeded(msg)) => {
                assert!(msg.contains("below bed origin"));
                assert!(msg.contains("-1"));
            }
            _ => panic!("Expected BoundsExceeded error"),
        }
    }

    #[test]
    fn validate_travel_segment_exceeds_bounds() {
        let segments = vec![PlanSegment::Travel {
            start: Point2D::new(0.0, 0.0),
            end: Point2D::new(500.0, 100.0),
        }];

        let result = validate_bounds(&segments, 400.0, 300.0);
        assert!(result.is_err());
        match result {
            Err(PlannerError::BoundsExceeded(msg)) => {
                assert!(msg.contains("travel end"));
                assert!(msg.contains("exceeds bed width"));
            }
            _ => panic!("Expected BoundsExceeded error"),
        }
    }

    #[test]
    fn validate_frame_segment_exceeds_bounds() {
        let segments = vec![PlanSegment::Frame {
            path: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(450.0, 0.0),
                Point2D::new(450.0, 300.0),
            ],
            power_percent: 10.0,
            speed_mm_min: 3000.0,
        }];

        let result = validate_bounds(&segments, 400.0, 300.0);
        assert!(result.is_err());
    }

    #[test]
    fn validate_raster_segment_within_bounds() {
        let segments = vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 100.0,
                runs: vec![ScanRun {
                    start_x_mm: 50.0,
                    end_x_mm: 350.0,
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
        }];

        let result = validate_bounds(&segments, 400.0, 300.0);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_raster_coordinates_within_geometric_tolerance_ok() {
        let segments = vec![horizontal_raster_segment(
            -0.010_688_649_809_765_138,
            -0.25,
            400.25,
        )];

        let result = validate_bounds(&segments, 400.0, 300.0);

        assert!(result.is_ok());
    }

    #[test]
    fn validate_raster_scanline_materially_below_origin_reports_lower_bound() {
        let segments = vec![horizontal_raster_segment(-0.500_1, 50.0, 100.0)];

        let result = validate_bounds(&segments, 400.0, 300.0);

        match result {
            Err(PlannerError::BoundsExceeded(msg)) => {
                assert!(msg.contains("raster scanline"));
                assert!(msg.contains("below bed origin"));
                assert!(!msg.contains("exceeds bed height"));
            }
            _ => panic!("Expected BoundsExceeded error"),
        }
    }

    #[test]
    fn validate_raster_run_endpoints_materially_below_origin_report_lower_bound() {
        let cases = [
            (horizontal_raster_segment(100.0, -0.500_1, 50.0), "start_x"),
            (horizontal_raster_segment(100.0, 50.0, -0.500_1), "end_x"),
        ];

        for (segment, expected_label) in cases {
            let result = validate_bounds(&[segment], 400.0, 300.0);

            match result {
                Err(PlannerError::BoundsExceeded(msg)) => {
                    assert!(msg.contains(expected_label));
                    assert!(msg.contains("below bed origin"));
                    assert!(!msg.contains("exceeds bed width"));
                }
                _ => panic!("Expected BoundsExceeded error"),
            }
        }
    }

    #[test]
    fn validate_raster_segment_y_exceeds_bounds() {
        let segments = vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 350.0,
                runs: vec![ScanRun {
                    start_x_mm: 50.0,
                    end_x_mm: 100.0,
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
        }];

        let result = validate_bounds(&segments, 400.0, 300.0);
        assert!(result.is_err());
        match result {
            Err(PlannerError::BoundsExceeded(msg)) => {
                assert!(msg.contains("scanline"));
                assert!(msg.contains("exceeds bed height"));
            }
            _ => panic!("Expected BoundsExceeded error"),
        }
    }

    #[test]
    fn validate_raster_segment_x_exceeds_bounds() {
        let segments = vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 100.0,
                runs: vec![ScanRun {
                    start_x_mm: 50.0,
                    end_x_mm: 450.0,
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
        }];

        let result = validate_bounds(&segments, 400.0, 300.0);
        assert!(result.is_err());
        match result {
            Err(PlannerError::BoundsExceeded(msg)) => {
                assert!(msg.contains("end_x"));
                assert!(msg.contains("exceeds bed width"));
            }
            _ => panic!("Expected BoundsExceeded error"),
        }
    }

    #[test]
    fn validate_vertical_raster_cross_axis_x_within_bed_width_ok() {
        // Vertical scan axis: scanline.y_mm holds the X world coordinate.
        // On a 400x300 bed, x=350 is valid (within bed width) even though
        // it would exceed bed height — the horizontal-axis check must not
        // apply here.
        let segments = vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 350.0,
                runs: vec![ScanRun {
                    start_x_mm: 50.0,
                    end_x_mm: 250.0,
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
            scan_axis: ScanAxis::Vertical,
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }];

        let result = validate_bounds(&segments, 400.0, 300.0);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_vertical_raster_run_y_exceeds_bed_height() {
        // Vertical scan axis: run start/end hold Y world coordinates.
        // On a 400x300 bed, a run end of 350 exceeds bed height even
        // though it would fit within bed width.
        let segments = vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 100.0,
                runs: vec![ScanRun {
                    start_x_mm: 50.0,
                    end_x_mm: 350.0,
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
            scan_axis: ScanAxis::Vertical,
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }];

        let result = validate_bounds(&segments, 400.0, 300.0);
        assert!(result.is_err());
        match result {
            Err(PlannerError::BoundsExceeded(msg)) => {
                assert!(msg.contains("end_x"));
                assert!(msg.contains("exceeds bed height"));
            }
            _ => panic!("Expected BoundsExceeded error"),
        }
    }

    #[test]
    fn validate_travel_allows_right_edge_raster_overscan() {
        let segments = vec![
            PlanSegment::Raster {
                scanlines: vec![Scanline {
                    y_mm: 100.0,
                    runs: vec![ScanRun {
                        start_x_mm: 395.0,
                        end_x_mm: 400.0,
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
                overscan_mm: 5.0,
                outlines: vec![],
                scan_axis: ScanAxis::default(),
                power_max_percent: 100.0,
                power_min_percent: 0.0,
                dot_width_correction_mm: 0.0,
                ramp_length_mm: 0.0,
                x_pixel_mm: 0.0,
            },
            PlanSegment::Travel {
                start: Point2D::new(405.0, 100.0),
                end: Point2D::new(405.0, 110.0),
            },
        ];

        let result = validate_bounds(&segments, 400.0, 300.0);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_negative_coordinates_fail() {
        let segments = vec![PlanSegment::Travel {
            start: Point2D::new(-10.0, 50.0),
            end: Point2D::new(100.0, 100.0),
        }];

        let result = validate_bounds(&segments, 400.0, 300.0);
        assert!(result.is_err());
    }

    #[test]
    fn validate_vector_fp_drift_within_tolerance_ok() {
        // Floating-point drift from geometric operations (undo/redo, transforms)
        // can produce coordinates like -0.134mm. These should be accepted.
        let segments = vec![PlanSegment::Vector {
            polyline: vec![Point2D::new(-0.134, 50.0), Point2D::new(400.25, 50.0)],
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

        let result = validate_bounds(&segments, 400.0, 300.0);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_vector_clearly_outside_bounds_fails() {
        // 1mm beyond bed should still fail (past the tolerance)
        let segments = vec![PlanSegment::Vector {
            polyline: vec![Point2D::new(-1.0, 50.0)],
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

        let result = validate_bounds(&segments, 400.0, 300.0);
        assert!(result.is_err());
    }
}
