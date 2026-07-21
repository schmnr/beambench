//! Execution plan types for laser operations.

use beambench_common::geometry::{Bounds, Point2D};
use beambench_common::path::Polyline;
use beambench_core::layer::OperationType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// A complete execution plan for a laser engraving job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub id: Uuid,
    pub project_id: Uuid,
    pub revision_hash: String, // SHA-256 of serialized project (cache key)
    pub created_at: DateTime<Utc>,
    pub bounds: Bounds,
    pub total_distance_mm: f64,
    pub estimated_duration_secs: f64,
    pub segments: Vec<PlanSegment>,
    pub layer_order: Vec<String>, // layer IDs in execution order
    pub warnings: Vec<PlanWarning>,
    #[serde(default)]
    pub failed_entries: Vec<PlanEntryFailure>,
}

/// A segment of the execution plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlanSegment {
    Travel {
        start: Point2D,
        end: Point2D,
    },
    Vector {
        polyline: Vec<Point2D>,
        closed: bool,
        power_percent: f64,
        speed_mm_min: f64,
        layer_id: String,
        #[serde(default)]
        cut_entry_id: String,
        /// Whether perforation mode is active for this segment.
        #[serde(default)]
        perforation_enabled: bool,
        /// Laser-on time per perforation pulse, in milliseconds.
        #[serde(default)]
        perforation_on_ms: f64,
        /// Laser-off (dwell) time per perforation gap, in milliseconds.
        #[serde(default)]
        perforation_off_ms: f64,
        /// Source object ID for tab application (set during plan building).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_object_id: Option<String>,
        /// Source subpath index for tab application.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_subpath_index: Option<usize>,
    },
    Raster {
        scanlines: Vec<Scanline>,
        line_interval_mm: f64,
        direction_mode: DirectionMode,
        power_mode: PowerMode,
        speed_mm_min: f64,
        layer_id: String,
        #[serde(default)]
        cut_entry_id: String,
        /// Scan angle in degrees (0=horizontal, 90=vertical, arbitrary supported).
        /// For non-orthogonal angles, scanline coordinates are in a local
        /// origin-centered frame; the G-code emitter and preview rotate them
        /// back to world space using scan_origin.
        #[serde(default)]
        scan_angle_deg: f64,
        /// World-space center of rotation (object center). Used by the G-code
        /// emitter to rotate local scanline coordinates to machine coordinates.
        /// Only meaningful when scan_angle_deg is non-orthogonal.
        #[serde(default)]
        scan_origin: Point2D,
        #[serde(default)]
        overscan_mm: f64,
        /// Source shape outlines for preview rendering (clip path).
        /// Empty for image-based rasters; populated for vector fill.
        #[serde(default)]
        outlines: Vec<Polyline>,
        /// Whether scanlines run horizontally (default) or vertically.
        #[serde(default)]
        scan_axis: ScanAxis,
        /// Layer max power percentage (0-100). Default 100.0.
        #[serde(default = "default_power_max")]
        power_max_percent: f64,
        /// Layer min power percentage for grayscale mapping (0-100). Default 0.0.
        #[serde(default)]
        power_min_percent: f64,
        /// Per-layer dot-width correction in mm. Composed with machine-level
        /// DWC at the planner's single correction pass — total trim per side
        /// is `(machine_dot_width + layer_dot_width) / 2`.
        #[serde(default)]
        dot_width_correction_mm: f64,
        /// Ramp length in mm. When non-zero, threshold runs are expanded
        /// into a ramp-in / constant / ramp-out profile by the G-code
        /// emitter and preview distiller.
        ///
        /// The planner zeroes this for any mode other than
        /// `RasterMode::Threshold` (and for pass-through) — dithered,
        /// halftone, newsprint, sketch, and pass-through binary output
        /// already encode tone through their bit pattern and would be
        /// destroyed by per-run ramping. Grayscale runs carry
        /// `power_values` and ignore ramp length.
        #[serde(default)]
        ramp_length_mm: f64,
        /// Pixel pitch along the scan axis (X), in mm. For normal
        /// modes this equals `line_interval_mm`; for pass-through it
        /// can differ (native pixel spacing). The preview distiller
        /// uses this to size the preview bitmap correctly for
        /// non-square pixels. 0.0 is a sentinel meaning "fall back to
        /// line_interval_mm" — used by vector-fill raster segments
        /// that have no source ProcessedRaster.
        #[serde(default)]
        x_pixel_mm: f64,
    },
    Frame {
        path: Vec<Point2D>,
        power_percent: f64,
        speed_mm_min: f64,
    },
    OffsetFill {
        layer_id: String,
        object_id: String,
        offset_mm: f64,
        angle_deg: f64,
    },
}

/// A single scanline in a raster operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scanline {
    pub y_mm: f64,
    pub runs: Vec<ScanRun>,
    pub direction: ScanDirection,
}

/// A run of pixels within a scanline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanRun {
    pub start_x_mm: f64,
    pub end_x_mm: f64,
    pub power_values: Vec<u8>, // 1 value per pixel for grayscale, empty for binary (implied 100%)
}

/// Direction of a scanline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanDirection {
    LeftToRight,
    RightToLeft,
}

/// Raster scanning direction mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectionMode {
    Bidirectional,
    Unidirectional,
}

/// Raster power mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerMode {
    Binary,
    Grayscale,
}

/// Raster scan axis — whether scanlines run horizontally or vertically.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanAxis {
    #[default]
    Horizontal,
    Vertical,
}

/// A warning message generated during planning.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanWarning {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanEntryFailure {
    pub layer_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut_entry_id: Option<String>,
    pub operation: OperationType,
    pub reason: PlanEntryFailureReason,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlanEntryFailureReason {
    ImageMaskSkipped { object_id: String, message: String },
    OffsetFillEmergencyCeiling { iterations: usize },
    OffsetFillTimedOut { iterations: usize, elapsed_ms: u64 },
}

/// Stats summary for quick queries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanStats {
    pub distance_mm: f64,
    pub duration_secs: f64,
    pub segment_count: usize,
    pub warning_count: usize,
}

/// Calibration parameters passed from the machine profile to the planner.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlannerCalibration {
    /// Beam diameter in mm (0.0 = disabled).
    pub dot_width_mm: f64,
    /// Whether dot-width correction is active.
    pub enable_dot_width: bool,
}

fn default_power_max() -> f64 {
    100.0
}

impl ExecutionPlan {
    /// Get statistics summary for this plan.
    pub fn stats(&self) -> PlanStats {
        PlanStats {
            distance_mm: self.total_distance_mm,
            duration_secs: self.estimated_duration_secs,
            segment_count: self.segments.len(),
            warning_count: self.warnings.len(),
        }
    }

    /// Stable digest of emitted segments only.
    ///
    /// This intentionally excludes volatile plan envelope fields such as
    /// `revision_hash`, `id`, and `created_at` so single-entry regression tests
    /// can compare emitted work across schema refactors.
    pub fn segments_digest(&self) -> String {
        let encoded = serde_json::to_vec(&self.segments).expect("segments serialize");
        let mut hasher = Sha256::new();
        hasher.update(&encoded);
        format!("{:x}", hasher.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::geometry::Point2D;
    use beambench_core::FinishPosition;

    #[test]
    fn execution_plan_serialization_roundtrip() {
        let plan = ExecutionPlan {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            revision_hash: "abc123".to_string(),
            created_at: Utc::now(),
            bounds: Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 100.0)),
            total_distance_mm: 500.0,
            estimated_duration_secs: 120.0,
            segments: vec![],
            layer_order: vec!["layer1".to_string()],
            warnings: vec![],
            failed_entries: vec![],
        };

        let json = serde_json::to_string(&plan).unwrap();
        let restored: ExecutionPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(plan, restored);
    }

    #[test]
    fn plan_segment_tagged_serialization_travel() {
        let segment = PlanSegment::Travel {
            start: Point2D::new(0.0, 0.0),
            end: Point2D::new(10.0, 10.0),
        };

        let json = serde_json::to_string(&segment).unwrap();
        assert!(json.contains(r#""type":"travel"#));

        let restored: PlanSegment = serde_json::from_str(&json).unwrap();
        assert_eq!(segment, restored);
    }

    #[test]
    fn plan_segment_tagged_serialization_vector() {
        let segment = PlanSegment::Vector {
            polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)],
            closed: false,
            power_percent: 80.0,
            speed_mm_min: 1000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: "entry1".to_string(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        };

        let json = serde_json::to_string(&segment).unwrap();
        assert!(json.contains(r#""type":"vector"#));

        let restored: PlanSegment = serde_json::from_str(&json).unwrap();
        assert_eq!(segment, restored);
    }

    #[test]
    fn plan_segment_tagged_serialization_raster() {
        let segment = PlanSegment::Raster {
            scanlines: vec![],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: "entry1".to_string(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 2.5,
            outlines: vec![],
            scan_axis: ScanAxis::Horizontal,
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        };

        let json = serde_json::to_string(&segment).unwrap();
        assert!(json.contains(r#""type":"raster"#));
        assert!(json.contains(r#""direction_mode":"bidirectional"#));
        assert!(json.contains(r#""power_mode":"binary"#));

        let restored: PlanSegment = serde_json::from_str(&json).unwrap();
        assert_eq!(segment, restored);
    }

    #[test]
    fn plan_stats_from_execution_plan() {
        let plan = ExecutionPlan {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            revision_hash: "test".to_string(),
            created_at: Utc::now(),
            bounds: Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 100.0)),
            total_distance_mm: 750.0,
            estimated_duration_secs: 180.0,
            segments: vec![
                PlanSegment::Travel {
                    start: Point2D::new(0.0, 0.0),
                    end: Point2D::new(10.0, 10.0),
                },
                PlanSegment::Vector {
                    polyline: vec![Point2D::new(0.0, 0.0)],
                    closed: false,
                    power_percent: 80.0,
                    speed_mm_min: 1000.0,
                    layer_id: "layer1".to_string(),
                    cut_entry_id: "entry1".to_string(),
                    perforation_enabled: false,
                    perforation_on_ms: 0.0,
                    perforation_off_ms: 0.0,
                    source_object_id: None,
                    source_subpath_index: None,
                },
            ],
            layer_order: vec!["layer1".to_string()],
            warnings: vec![
                PlanWarning {
                    message: "Test warning 1".to_string(),
                },
                PlanWarning {
                    message: "Test warning 2".to_string(),
                },
            ],
            failed_entries: vec![],
        };

        let stats = plan.stats();
        assert_eq!(stats.distance_mm, 750.0);
        assert_eq!(stats.duration_secs, 180.0);
        assert_eq!(stats.segment_count, 2);
        assert_eq!(stats.warning_count, 2);
    }

    #[test]
    fn scanline_serialization_roundtrip() {
        let scanline = Scanline {
            y_mm: 10.5,
            runs: vec![ScanRun {
                start_x_mm: 5.0,
                end_x_mm: 15.0,
                power_values: vec![100, 150, 200],
            }],
            direction: ScanDirection::LeftToRight,
        };

        let json = serde_json::to_string(&scanline).unwrap();
        let restored: Scanline = serde_json::from_str(&json).unwrap();
        assert_eq!(scanline, restored);
    }

    #[test]
    fn cut_entry_id_roundtrips_through_plan_segment_json() {
        let vector = PlanSegment::Vector {
            polyline: vec![Point2D::new(1.0, 2.0), Point2D::new(3.0, 4.0)],
            closed: false,
            power_percent: 50.0,
            speed_mm_min: 900.0,
            layer_id: "layer-a".to_string(),
            cut_entry_id: "entry-a".to_string(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        };

        let json = serde_json::to_string(&vector).unwrap();
        let restored: PlanSegment = serde_json::from_str(&json).unwrap();
        match restored {
            PlanSegment::Vector { cut_entry_id, .. } => assert_eq!(cut_entry_id, "entry-a"),
            _ => panic!("expected vector segment"),
        }
    }

    #[test]
    fn segments_digest_ignores_plan_metadata() {
        let mut plan = ExecutionPlan {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            revision_hash: "hash-a".to_string(),
            created_at: Utc::now(),
            bounds: Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            total_distance_mm: 10.0,
            estimated_duration_secs: 1.0,
            segments: vec![PlanSegment::Vector {
                polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)],
                closed: false,
                power_percent: 50.0,
                speed_mm_min: 1000.0,
                layer_id: "layer-1".to_string(),
                cut_entry_id: "entry-1".to_string(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            }],
            layer_order: vec!["layer-1".to_string()],
            warnings: vec![],
            failed_entries: vec![],
        };
        let original = plan.segments_digest();

        plan.id = Uuid::new_v4();
        plan.revision_hash = "hash-b".to_string();
        plan.created_at = Utc::now();
        assert_eq!(original, plan.segments_digest());
    }

    #[test]
    fn segments_digest_changes_when_emitted_segments_change() {
        let mut plan = ExecutionPlan {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            revision_hash: "hash-a".to_string(),
            created_at: Utc::now(),
            bounds: Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            total_distance_mm: 10.0,
            estimated_duration_secs: 1.0,
            segments: vec![PlanSegment::Vector {
                polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)],
                closed: false,
                power_percent: 50.0,
                speed_mm_min: 1000.0,
                layer_id: "layer-1".to_string(),
                cut_entry_id: "entry-1".to_string(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            }],
            layer_order: vec!["layer-1".to_string()],
            warnings: vec![],
            failed_entries: vec![],
        };
        let original = plan.segments_digest();

        if let PlanSegment::Vector {
            speed_mm_min,
            cut_entry_id,
            ..
        } = &mut plan.segments[0]
        {
            *speed_mm_min = 1200.0;
            *cut_entry_id = "entry-2".to_string();
        }

        assert_ne!(original, plan.segments_digest());
    }

    #[test]
    fn scan_direction_serialization() {
        let left = ScanDirection::LeftToRight;
        let json = serde_json::to_string(&left).unwrap();
        assert!(json.contains(r#""left_to_right"#));

        let right = ScanDirection::RightToLeft;
        let json = serde_json::to_string(&right).unwrap();
        assert!(json.contains(r#""right_to_left"#));
    }

    #[test]
    fn direction_mode_serialization() {
        let bi = DirectionMode::Bidirectional;
        let json = serde_json::to_string(&bi).unwrap();
        assert!(json.contains(r#""bidirectional"#));

        let uni = DirectionMode::Unidirectional;
        let json = serde_json::to_string(&uni).unwrap();
        assert!(json.contains(r#""unidirectional"#));
    }

    #[test]
    fn power_mode_serialization() {
        let binary = PowerMode::Binary;
        let json = serde_json::to_string(&binary).unwrap();
        assert!(json.contains(r#""binary"#));

        let grayscale = PowerMode::Grayscale;
        let json = serde_json::to_string(&grayscale).unwrap();
        assert!(json.contains(r#""grayscale"#));
    }

    // Pre-M1 `OptimizationSettings` / `CutOrderStrategy` tests removed
    // in Phase 5b along with the types themselves. The post-M1 equivalent
    // coverage lives in `beambench_core::optimization::tests` for the
    // persisted DTO round-trip + patch semantics, and in
    // `tests/phase4b_flag_effects.rs` / `tests/phase4c_flag_effects.rs`
    // for planner-level flag behavior.

    #[test]
    fn finish_position_default_is_origin() {
        assert_eq!(FinishPosition::default(), FinishPosition::Origin);
    }

    #[test]
    fn planner_calibration_default_is_disabled() {
        let cal = PlannerCalibration::default();
        assert_eq!(cal.dot_width_mm, 0.0);
        assert!(!cal.enable_dot_width);
    }

    #[test]
    fn raster_power_fields_backward_compat_deserialization() {
        // Old JSON without power fields should deserialize with defaults
        let json = r#"{
            "type":"raster",
            "scanlines":[],
            "line_interval_mm":0.1,
            "direction_mode":"bidirectional",
            "power_mode":"binary",
            "speed_mm_min":2000.0,
            "layer_id":"layer1",
            "overscan_mm":0.0
        }"#;
        let segment: PlanSegment = serde_json::from_str(json).unwrap();
        if let PlanSegment::Raster {
            power_max_percent,
            power_min_percent,
            ..
        } = &segment
        {
            assert_eq!(*power_max_percent, 100.0);
            assert_eq!(*power_min_percent, 0.0);
        } else {
            panic!("Expected Raster variant");
        }
    }

    #[test]
    fn raster_power_fields_roundtrip_non_default() {
        let segment = PlanSegment::Raster {
            scanlines: vec![],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: "entry1".to_string(),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::Horizontal,
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            power_max_percent: 80.0,
            power_min_percent: 15.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        };

        let json = serde_json::to_string(&segment).unwrap();
        let restored: PlanSegment = serde_json::from_str(&json).unwrap();

        if let PlanSegment::Raster {
            power_max_percent,
            power_min_percent,
            ..
        } = &restored
        {
            assert_eq!(*power_max_percent, 80.0);
            assert_eq!(*power_min_percent, 15.0);
        } else {
            panic!("Expected Raster variant");
        }
    }

    #[test]
    fn offset_fill_segment_creation() {
        let segment = PlanSegment::OffsetFill {
            layer_id: "layer1".to_string(),
            object_id: "obj1".to_string(),
            offset_mm: 0.5,
            angle_deg: 45.0,
        };

        if let PlanSegment::OffsetFill {
            offset_mm,
            angle_deg,
            ..
        } = &segment
        {
            assert_eq!(*offset_mm, 0.5);
            assert_eq!(*angle_deg, 45.0);
        } else {
            panic!("Expected OffsetFill variant");
        }
    }

    #[test]
    fn offset_fill_segment_serialization_roundtrip() {
        let segment = PlanSegment::OffsetFill {
            layer_id: "layer1".to_string(),
            object_id: "obj1".to_string(),
            offset_mm: 1.0,
            angle_deg: 90.0,
        };

        let json = serde_json::to_string(&segment).unwrap();
        assert!(json.contains(r#""type":"offset_fill"#));

        let restored: PlanSegment = serde_json::from_str(&json).unwrap();
        assert_eq!(segment, restored);
    }
}
