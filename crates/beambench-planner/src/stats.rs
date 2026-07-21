//! Statistics calculation for execution plans.

use crate::plan::PlanSegment;

const RAPID_SPEED_MM_MIN: f64 = 10000.0;

/// Calculate total travel+work distance in mm.
pub fn calculate_distance(segments: &[PlanSegment]) -> f64 {
    segments.iter().map(segment_distance).sum()
}

/// Calculate estimated duration in seconds.
pub fn calculate_duration(segments: &[PlanSegment]) -> f64 {
    segments.iter().map(segment_duration).sum()
}

fn segment_distance(segment: &PlanSegment) -> f64 {
    match segment {
        PlanSegment::Travel { start, end } => start.distance_to(end),
        PlanSegment::Vector { polyline, .. } => {
            polyline.windows(2).map(|w| w[0].distance_to(&w[1])).sum()
        }
        PlanSegment::Frame { path, .. } => path.windows(2).map(|w| w[0].distance_to(&w[1])).sum(),
        PlanSegment::Raster {
            scanlines,
            overscan_mm,
            ..
        } => {
            let (feed, rapid) = raster_distance_components(scanlines, *overscan_mm);
            feed + rapid
        }
        PlanSegment::OffsetFill { .. } => 0.0,
    }
}

/// Split raster motion into (feed_distance, rapid_distance).
///
/// Feed distance = each non-empty row's full burn envelope plus one lead-in
/// and lead-out. Internal white gaps remain part of the continuous row sweep
/// at engraving speed with the laser at zero power.
/// Rapid distance = travel between completed scanlines.
fn raster_distance_components(scanlines: &[crate::plan::Scanline], os: f64) -> (f64, f64) {
    use crate::plan::ScanDirection;

    let mut feed_dist = 0.0;
    let mut rapid_dist = 0.0;

    // Track the emitter's position after the previous row completes so the
    // inter-scanline rapid matches the G-code emitter.
    let mut prev_end: Option<(f64, f64)> = None; // (x, y) in planner coords

    for scanline in scanlines {
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        for run in &scanline.runs {
            min_x = min_x.min(run.start_x_mm.min(run.end_x_mm));
            max_x = max_x.max(run.start_x_mm.max(run.end_x_mm));
        }
        if !min_x.is_finite() {
            continue;
        }

        let (start_pos, end_pos, direction) = match scanline.direction {
            ScanDirection::LeftToRight => (min_x, max_x, 1.0),
            ScanDirection::RightToLeft => (max_x, min_x, -1.0),
        };
        let os_start = start_pos - direction * os;
        let os_end = end_pos + direction * os;

        feed_dist += max_x - min_x + 2.0 * os;
        if let Some((prev_x, prev_y)) = prev_end {
            let dx = os_start - prev_x;
            let dy = scanline.y_mm - prev_y;
            rapid_dist += (dx * dx + dy * dy).sqrt();
        }
        prev_end = Some((os_end, scanline.y_mm));
    }

    (feed_dist, rapid_dist)
}

/// Compute the estimated duration of a single plan segment in seconds.
pub fn segment_duration(segment: &PlanSegment) -> f64 {
    match segment {
        PlanSegment::Travel { start, end } => {
            let dist = start.distance_to(end);
            if RAPID_SPEED_MM_MIN == 0.0 {
                return 0.0;
            }
            (dist / RAPID_SPEED_MM_MIN) * 60.0
        }
        PlanSegment::Vector {
            polyline,
            speed_mm_min,
            ..
        } => {
            let dist: f64 = polyline.windows(2).map(|w| w[0].distance_to(&w[1])).sum();
            if *speed_mm_min == 0.0 {
                return 0.0;
            }
            (dist / speed_mm_min) * 60.0
        }
        PlanSegment::Frame {
            path, speed_mm_min, ..
        } => {
            let dist: f64 = path.windows(2).map(|w| w[0].distance_to(&w[1])).sum();
            if *speed_mm_min == 0.0 {
                return 0.0;
            }
            (dist / speed_mm_min) * 60.0
        }
        PlanSegment::Raster {
            scanlines,
            overscan_mm,
            speed_mm_min,
            ..
        } => {
            let (feed, rapid) = raster_distance_components(scanlines, *overscan_mm);
            if *speed_mm_min == 0.0 {
                return 0.0;
            }
            let feed_secs = (feed / speed_mm_min) * 60.0;
            let rapid_secs = if RAPID_SPEED_MM_MIN > 0.0 {
                (rapid / RAPID_SPEED_MM_MIN) * 60.0
            } else {
                0.0
            };
            feed_secs + rapid_secs
        }
        PlanSegment::OffsetFill { .. } => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{DirectionMode, PowerMode, ScanAxis, ScanDirection, ScanRun, Scanline};
    use beambench_common::geometry::Point2D;

    #[test]
    fn calculate_distance_empty_segments() {
        let segments = vec![];
        assert_eq!(calculate_distance(&segments), 0.0);
    }

    #[test]
    fn calculate_duration_empty_segments() {
        let segments = vec![];
        assert_eq!(calculate_duration(&segments), 0.0);
    }

    #[test]
    fn calculate_distance_single_travel() {
        let segments = vec![PlanSegment::Travel {
            start: Point2D::new(0.0, 0.0),
            end: Point2D::new(3.0, 4.0),
        }];

        let distance = calculate_distance(&segments);
        assert!((distance - 5.0).abs() < 1e-10);
    }

    #[test]
    fn calculate_duration_single_travel() {
        let segments = vec![PlanSegment::Travel {
            start: Point2D::new(0.0, 0.0),
            end: Point2D::new(3.0, 4.0),
        }];

        let duration = calculate_duration(&segments);
        // Distance = 5mm, speed = 10000 mm/min
        // Duration = (5 / 10000) * 60 = 0.03 seconds
        assert!((duration - 0.03).abs() < 1e-10);
    }

    #[test]
    fn calculate_distance_vector_segment() {
        let segments = vec![PlanSegment::Vector {
            polyline: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
            ],
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

        let distance = calculate_distance(&segments);
        // 0→10 = 10mm, 10→(10,10) = 10mm, total = 20mm
        assert!((distance - 20.0).abs() < 1e-10);
    }

    #[test]
    fn calculate_duration_vector_segment() {
        let segments = vec![PlanSegment::Vector {
            polyline: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
            ],
            closed: false,
            power_percent: 80.0,
            speed_mm_min: 1200.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        }];

        let duration = calculate_duration(&segments);
        // Distance = 20mm, speed = 1200 mm/min
        // Duration = (20 / 1200) * 60 = 1.0 seconds
        assert!((duration - 1.0).abs() < 1e-10);
    }

    #[test]
    fn calculate_distance_frame_segment() {
        let segments = vec![PlanSegment::Frame {
            path: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(100.0, 0.0),
                Point2D::new(100.0, 50.0),
            ],
            power_percent: 10.0,
            speed_mm_min: 3000.0,
        }];

        let distance = calculate_distance(&segments);
        // 100 + 50 = 150mm
        assert!((distance - 150.0).abs() < 1e-10);
    }

    #[test]
    fn calculate_distance_raster_segment() {
        let segments = vec![PlanSegment::Raster {
            scanlines: vec![
                Scanline {
                    y_mm: 0.0,
                    runs: vec![ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 100.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::LeftToRight,
                },
                Scanline {
                    y_mm: 0.1,
                    runs: vec![ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 100.0,
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

        let distance = calculate_distance(&segments);
        // 2 scanlines of 100mm each = 200mm
        // Plus 1 vertical move of 0.1mm = 0.1mm
        // Total = 200.1mm
        assert!((distance - 200.1).abs() < 1e-10);
    }

    #[test]
    fn calculate_duration_raster_segment() {
        let segments = vec![PlanSegment::Raster {
            scanlines: vec![
                Scanline {
                    y_mm: 0.0,
                    runs: vec![ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 100.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::LeftToRight,
                },
                Scanline {
                    y_mm: 0.1,
                    runs: vec![ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 100.0,
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

        let duration = calculate_duration(&segments);
        // Feed = 200mm burn at 2000 mm/min = (200/2000)*60 = 6.0s
        // Rapid = 0.1mm inter-scanline at 10000 mm/min = (0.1/10000)*60 = 0.0006s
        // Total ≈ 6.0006s
        let expected = (200.0 / 2000.0) * 60.0 + (0.1 / RAPID_SPEED_MM_MIN) * 60.0;
        assert!((duration - expected).abs() < 1e-10);
    }

    #[test]
    fn calculate_distance_mixed_segments() {
        let segments = vec![
            PlanSegment::Travel {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(3.0, 4.0),
            },
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
            PlanSegment::Frame {
                path: vec![Point2D::new(0.0, 0.0), Point2D::new(20.0, 0.0)],
                power_percent: 10.0,
                speed_mm_min: 3000.0,
            },
        ];

        let distance = calculate_distance(&segments);
        // Travel: 5mm, Vector: 10mm, Frame: 20mm = 35mm
        assert!((distance - 35.0).abs() < 1e-10);
    }

    #[test]
    fn calculate_duration_mixed_segments_different_speeds() {
        let segments = vec![
            PlanSegment::Travel {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(100.0, 0.0),
            },
            PlanSegment::Vector {
                polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(60.0, 0.0)],
                closed: false,
                power_percent: 80.0,
                speed_mm_min: 1200.0,
                layer_id: "layer1".to_string(),
                cut_entry_id: String::new(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
        ];

        let duration = calculate_duration(&segments);
        // Travel: 100mm at 10000 mm/min = (100/10000)*60 = 0.6s
        // Vector: 60mm at 1200 mm/min = (60/1200)*60 = 3.0s
        // Total = 3.6s
        assert!((duration - 3.6).abs() < 1e-10);
    }

    #[test]
    fn calculate_distance_raster_multiple_runs_per_scanline() {
        let segments = vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 0.0,
                runs: vec![
                    ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 50.0,
                        power_values: vec![],
                    },
                    ScanRun {
                        start_x_mm: 60.0,
                        end_x_mm: 100.0,
                        power_values: vec![],
                    },
                ],
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

        let distance = calculate_distance(&segments);
        // One continuous 0..100mm feed sweep; the 50..60 gap runs at S0.
        assert!((distance - 100.0).abs() < 1e-10);
    }

    #[test]
    fn calculate_distance_empty_polyline() {
        let segments = vec![PlanSegment::Vector {
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
        }];

        let distance = calculate_distance(&segments);
        assert_eq!(distance, 0.0);
    }

    #[test]
    fn calculate_distance_single_point_polyline() {
        let segments = vec![PlanSegment::Vector {
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
        }];

        let distance = calculate_distance(&segments);
        assert_eq!(distance, 0.0);
    }

    #[test]
    fn calculate_distance_raster_with_overscan() {
        let segments = vec![PlanSegment::Raster {
            scanlines: vec![
                Scanline {
                    y_mm: 0.0,
                    runs: vec![ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 100.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::LeftToRight,
                },
                Scanline {
                    y_mm: 0.1,
                    runs: vec![ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 100.0,
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
            overscan_mm: 5.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }];

        let distance = calculate_distance(&segments);
        // LTR line 0: burn=100 + os=10 = 110mm feed. os_end=105.
        // RTL line 1: burn=100 + os=10 = 110mm feed. os_start=105.
        // Rapid: (105,0)→(105,0.1) = 0.1mm
        // Total = 220 feed + 0.1 rapid = 220.1mm
        assert!((distance - 220.1).abs() < 1e-10);
    }

    #[test]
    fn calculate_distance_raster_overscan_multi_run() {
        // Two nearby runs share one continuous row sweep and one overscan pair.
        let segments = vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 0.0,
                runs: vec![
                    ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 4.0,
                        power_values: vec![],
                    },
                    ScanRun {
                        start_x_mm: 6.0,
                        end_x_mm: 10.0,
                        power_values: vec![],
                    },
                ],
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
        }];

        let distance = calculate_distance(&segments);
        // Row envelope 0..10 plus 5mm lead-in and lead-out = 20mm.
        // The 4..6 white gap is traversed once at engraving speed with S0.
        assert!((distance - 20.0).abs() < 1e-10);
    }

    #[test]
    fn calculate_duration_raster_with_overscan() {
        // Verify duration splits feed (at engrave speed) and rapid (at rapid speed)
        let segments = vec![PlanSegment::Raster {
            scanlines: vec![
                Scanline {
                    y_mm: 0.0,
                    runs: vec![ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 100.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::LeftToRight,
                },
                Scanline {
                    y_mm: 0.1,
                    runs: vec![ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 100.0,
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
            overscan_mm: 5.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }];

        let duration = calculate_duration(&segments);
        // Feed = 220mm at 2000 mm/min = (220/2000)*60 = 6.6s
        // Rapid = 0.1mm at 10000 mm/min = 0.0006s
        let expected = (220.0 / 2000.0) * 60.0 + (0.1 / RAPID_SPEED_MM_MIN) * 60.0;
        assert!((duration - expected).abs() < 1e-10);
    }

    #[test]
    fn calculate_distance_raster_unidirectional_return_travel() {
        // Unidirectional: both scanlines LTR, so emitter must rapid back
        let segments = vec![PlanSegment::Raster {
            scanlines: vec![
                Scanline {
                    y_mm: 0.0,
                    runs: vec![ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 100.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::LeftToRight,
                },
                Scanline {
                    y_mm: 0.1,
                    runs: vec![ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 100.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::LeftToRight,
                },
            ],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Unidirectional,
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

        let distance = calculate_distance(&segments);
        // Feed = 200mm burn. Rapid from (100,0)→(0,0.1) = sqrt(100^2 + 0.1^2) ≈ 100.00005
        let rapid = (100.0_f64.powi(2) + 0.1_f64.powi(2)).sqrt();
        let expected = 200.0 + rapid;
        assert!((distance - expected).abs() < 1e-6);
    }

    #[test]
    fn calculate_duration_zero_speed() {
        let segments = vec![PlanSegment::Vector {
            polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(100.0, 0.0)],
            closed: false,
            power_percent: 80.0,
            speed_mm_min: 0.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        }];

        let duration = calculate_duration(&segments);
        assert_eq!(duration, 0.0);
    }
}
