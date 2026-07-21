//! Quality-test request, settings, and response types for Material/Focus/Interval test workflows (M3).
//!
//! These types are persisted on `MachineProfile.quality_test_settings` and crossed over the IPC
//! boundary as `QualityTestRequest`. The transient quality-test pipeline lives in
//! `beambench-service::ops::quality_test`.

use crate::layer::{CutEntry, CutEntryId, OperationType};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// How Focus Test emits Z motions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FocusTestZMode {
    /// Emit each step as an absolute machine/work-coord move (`G1 Z<value> F<feed>`).
    #[default]
    AbsoluteWorkCoord,
    /// Wrap each step in a temporary relative block (`G91` / `G1 Z<delta> F<feed>` / `G90`).
    RelativeTemporary,
}

/// Parameter varied along one Material Test axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialTestAxisParam {
    Speed,
    Power,
    Interval,
    Passes,
}

/// Numeric sweep definition for one Material Test axis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialTestAxis {
    pub param: MaterialTestAxisParam,
    pub count: u32,
    pub min: f64,
    pub max: f64,
}

fn material_test_x_axis_default() -> MaterialTestAxis {
    MaterialTestAxis {
        param: MaterialTestAxisParam::Power,
        count: 5,
        min: 10.0,
        max: 100.0,
    }
}

fn material_test_y_axis_default() -> MaterialTestAxis {
    MaterialTestAxis {
        param: MaterialTestAxisParam::Speed,
        count: 5,
        min: 300.0,
        max: 3000.0,
    }
}

fn stable_cut_entry_id(offset: u128) -> CutEntryId {
    CutEntryId::from_uuid(Uuid::from_u128(offset))
}

fn material_test_sample_entry_default() -> CutEntry {
    let mut entry = CutEntry::new(OperationType::Fill);
    entry.id = stable_cut_entry_id(1);
    entry
}

fn material_test_text_entry_default() -> CutEntry {
    let mut entry = CutEntry::new(OperationType::Line);
    entry.id = stable_cut_entry_id(2);
    entry.power_percent = 25.0;
    entry
}

fn material_test_border_entry_default() -> CutEntry {
    let mut entry = CutEntry::new(OperationType::Line);
    entry.id = stable_cut_entry_id(3);
    entry.power_percent = 50.0;
    entry
}

fn material_test_cell_w_default() -> f64 {
    10.0
}

fn material_test_cell_h_default() -> f64 {
    10.0
}

fn material_test_cell_spacing_default() -> f64 {
    4.0
}

fn default_true() -> bool {
    true
}

fn focus_speed_default() -> f64 {
    1000.0
}

fn focus_power_default() -> f64 {
    50.0
}

fn focus_intervals_default() -> u32 {
    9
}

fn interval_speed_default() -> f64 {
    1000.0
}

fn interval_power_default() -> f64 {
    50.0
}

/// Persisted Material Test dialog state (per machine profile).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MaterialTestSettings {
    #[serde(default = "material_test_x_axis_default")]
    pub x_axis: MaterialTestAxis,
    #[serde(default = "material_test_y_axis_default")]
    pub y_axis: MaterialTestAxis,
    #[serde(default = "material_test_cell_w_default")]
    pub cell_w_mm: f64,
    #[serde(default = "material_test_cell_h_default")]
    pub cell_h_mm: f64,
    #[serde(default = "material_test_cell_spacing_default")]
    pub cell_spacing_mm: f64,
    #[serde(default = "material_test_sample_entry_default")]
    pub sample_entry: CutEntry,
    #[serde(default = "material_test_text_entry_default")]
    pub text_entry: CutEntry,
    #[serde(default = "material_test_border_entry_default")]
    pub border_entry: CutEntry,
    #[serde(default = "default_true")]
    pub enable_text: bool,
    #[serde(default = "default_true")]
    pub enable_border: bool,
    #[serde(default)]
    pub absolute_center_enabled: bool,
    #[serde(default)]
    pub x_center_mm: f64,
    #[serde(default)]
    pub y_center_mm: f64,
}

impl Default for MaterialTestSettings {
    fn default() -> Self {
        Self {
            x_axis: material_test_x_axis_default(),
            y_axis: material_test_y_axis_default(),
            cell_w_mm: material_test_cell_w_default(),
            cell_h_mm: material_test_cell_h_default(),
            cell_spacing_mm: material_test_cell_spacing_default(),
            sample_entry: material_test_sample_entry_default(),
            text_entry: material_test_text_entry_default(),
            border_entry: material_test_border_entry_default(),
            enable_text: true,
            enable_border: true,
            absolute_center_enabled: false,
            x_center_mm: 0.0,
            y_center_mm: 0.0,
        }
    }
}

/// Saved whole-dialog Material Test configuration. Deliberately separate from material-library
/// presets, which describe a single material/cut setting rather than a full test grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialTestRecipe {
    pub id: String,
    pub name: String,
    pub settings: MaterialTestSettings,
}

/// Persisted Focus Test dialog state (per machine profile).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FocusTestSettings {
    pub z_min_mm: f64,
    pub z_max_mm: f64,
    #[serde(default = "focus_speed_default")]
    pub speed_mm_min: f64,
    #[serde(default = "focus_power_default")]
    pub power_percent: f64,
    #[serde(default = "focus_intervals_default", alias = "steps")]
    pub intervals: u32,
    pub mode: FocusTestZMode,
    pub line_length_mm: f64,
    pub step_spacing_mm: f64,
    #[serde(default, alias = "high_power_labels")]
    pub perforated_labels: bool,
}

impl Default for FocusTestSettings {
    fn default() -> Self {
        Self {
            z_min_mm: -2.0,
            z_max_mm: 2.0,
            speed_mm_min: focus_speed_default(),
            power_percent: focus_power_default(),
            intervals: focus_intervals_default(),
            mode: FocusTestZMode::AbsoluteWorkCoord,
            line_length_mm: 30.0,
            step_spacing_mm: 5.0,
            perforated_labels: false,
        }
    }
}

/// Persisted Interval Test dialog state (per machine profile).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct IntervalTestSettings {
    pub interval_min_mm: f64,
    pub interval_max_mm: f64,
    #[serde(default = "interval_speed_default")]
    pub speed_mm_min: f64,
    #[serde(default = "interval_power_default")]
    pub power_percent: f64,
    pub steps: u32,
    pub cell_w_mm: f64,
    pub cell_h_mm: f64,
    pub cell_spacing_mm: f64,
}

impl Default for IntervalTestSettings {
    fn default() -> Self {
        Self {
            interval_min_mm: 0.05,
            interval_max_mm: 0.30,
            speed_mm_min: interval_speed_default(),
            power_percent: interval_power_default(),
            steps: 6,
            cell_w_mm: 15.0,
            cell_h_mm: 15.0,
            cell_spacing_mm: 4.0,
        }
    }
}

/// Aggregate of all per-tool settings, persisted on `MachineProfile`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct QualityTestSettings {
    #[serde(default)]
    pub material: MaterialTestSettings,
    #[serde(default)]
    pub focus: FocusTestSettings,
    #[serde(default)]
    pub interval: IntervalTestSettings,
    #[serde(default)]
    pub material_recipes: Vec<MaterialTestRecipe>,
    #[serde(default)]
    pub active_material_recipe_id: Option<String>,
}

/// IPC request shape for quality-test commands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QualityTestRequest {
    Material(MaterialTestSettings),
    Focus(FocusTestSettings),
    Interval(IntervalTestSettings),
}

/// Non-fatal warnings returned alongside preview/export responses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QualityTestWarning {
    /// Generated geometry exceeds the active machine profile's bed bounds.
    BoundsExceeded {
        bbox_w_mm: f64,
        bbox_h_mm: f64,
        bed_w_mm: f64,
        bed_h_mm: f64,
    },
    /// A label could not resolve its requested font and used the bundled fallback.
    FontFallback { requested_family: String },
}

/// Hard-reject conditions for `frame` and `start` paths.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QualityTestError {
    #[error(
        "generated geometry ({bbox_w_mm:.1}×{bbox_h_mm:.1} mm) exceeds bed ({bed_w_mm:.1}×{bed_h_mm:.1} mm)"
    )]
    BoundsExceeded {
        bbox_w_mm: f64,
        bbox_h_mm: f64,
        bed_w_mm: f64,
        bed_h_mm: f64,
    },
    #[error("active machine profile does not advertise Z support")]
    ZSupportRequired,
    #[error("absolute Z mode requires Project.material_height_mm to be set")]
    MaterialHeightRequired,
    #[error("Focus Test Z output is only supported for GRBL sessions")]
    UnsupportedZBackend,
    #[error("no active machine profile selected")]
    NoActiveMachineProfile,
    #[error("a job is already active; cancel or wait for it to finish")]
    JobInProgress,
    #[error("{message}")]
    Internal { message: String },
}

/// Ordering policy for the planner. Quality-test jobs use `RowMajor` to bypass optimization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum QualityTestOrdering {
    /// Apply normal `ProjectOptimization` passes.
    #[default]
    Normal,
    /// Bypass all optimization; emit segments in the order they were appended.
    RowMajor,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_test_settings_default_roundtrips() {
        let s = QualityTestSettings::default();
        let json = serde_json::to_string(&s).unwrap();
        let back: QualityTestSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn focus_test_z_mode_defaults_to_absolute() {
        assert_eq!(FocusTestZMode::default(), FocusTestZMode::AbsoluteWorkCoord);
    }

    #[test]
    fn quality_test_request_tagged_serialization() {
        let req = QualityTestRequest::Material(MaterialTestSettings::default());
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"kind\":\"material\""));
    }

    #[test]
    fn quality_test_error_internal_serializes_to_tagged_object() {
        let err = QualityTestError::Internal {
            message: "x".into(),
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains(r#""kind":"internal""#));
        assert!(json.contains(r#""message":"x""#));
    }

    #[test]
    fn empty_settings_object_deserializes_to_defaults() {
        let s: QualityTestSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, QualityTestSettings::default());
    }

    #[test]
    fn material_settings_defaults_match_plan() {
        let m = MaterialTestSettings::default();
        assert_eq!(m.x_axis.param, MaterialTestAxisParam::Power);
        assert_eq!(m.x_axis.count, 5);
        assert_eq!(m.x_axis.min, 10.0);
        assert_eq!(m.x_axis.max, 100.0);
        assert_eq!(m.y_axis.param, MaterialTestAxisParam::Speed);
        assert_eq!(m.y_axis.count, 5);
        assert_eq!(m.y_axis.min, 300.0);
        assert_eq!(m.y_axis.max, 3000.0);
        assert!(m.enable_text);
        assert!(m.enable_border);
        assert!(!m.absolute_center_enabled);
        assert_eq!(m.sample_entry, material_test_sample_entry_default());
    }

    #[test]
    fn old_material_settings_payload_deserializes_to_new_defaults() {
        let json = r#"{
            "material": {
                "grid_w": 9,
                "grid_h": 8,
                "speed_min_mm_min": 111,
                "speed_max_mm_min": 222,
                "power_min_percent": 33,
                "power_max_percent": 44,
                "cell_w_mm": 20
            }
        }"#;
        let s: QualityTestSettings = serde_json::from_str(json).unwrap();
        assert_eq!(s.material.x_axis, material_test_x_axis_default());
        assert_eq!(s.material.y_axis, material_test_y_axis_default());
        assert_eq!(s.material.cell_w_mm, 20.0);
        assert!(s.material_recipes.is_empty());
        assert_eq!(s.active_material_recipe_id, None);
    }

    #[test]
    fn old_focus_settings_payload_deserializes_to_new_defaults() {
        let json = r#"{
            "z_min_mm": -1.5,
            "z_max_mm": 1.5,
            "steps": 7,
            "mode": "RelativeTemporary",
            "line_length_mm": 25.0,
            "step_spacing_mm": 4.0
        }"#;

        let s: FocusTestSettings = serde_json::from_str(json).unwrap();

        assert_eq!(s.z_min_mm, -1.5);
        assert_eq!(s.z_max_mm, 1.5);
        assert_eq!(s.intervals, 7);
        assert_eq!(s.mode, FocusTestZMode::RelativeTemporary);
        assert_eq!(s.line_length_mm, 25.0);
        assert_eq!(s.step_spacing_mm, 4.0);
        assert_eq!(s.speed_mm_min, FocusTestSettings::default().speed_mm_min);
        assert_eq!(s.power_percent, FocusTestSettings::default().power_percent);
        assert!(!s.perforated_labels);
    }

    #[test]
    fn old_focus_high_power_labels_aliases_to_perforated_labels() {
        let json = r#"{
            "z_min_mm": -1.5,
            "z_max_mm": 1.5,
            "high_power_labels": true
        }"#;

        let s: FocusTestSettings = serde_json::from_str(json).unwrap();

        assert!(s.perforated_labels);
    }

    #[test]
    fn old_interval_settings_payload_deserializes_to_new_defaults() {
        let json = r#"{
            "interval_min_mm": 0.08,
            "interval_max_mm": 0.24,
            "steps": 5,
            "cell_w_mm": 12.0
        }"#;

        let s: IntervalTestSettings = serde_json::from_str(json).unwrap();

        assert_eq!(s.interval_min_mm, 0.08);
        assert_eq!(s.interval_max_mm, 0.24);
        assert_eq!(s.speed_mm_min, IntervalTestSettings::default().speed_mm_min);
        assert_eq!(
            s.power_percent,
            IntervalTestSettings::default().power_percent
        );
        assert_eq!(s.steps, 5);
        assert_eq!(s.cell_w_mm, 12.0);
        assert_eq!(s.cell_h_mm, IntervalTestSettings::default().cell_h_mm);
        assert_eq!(
            s.cell_spacing_mm,
            IntervalTestSettings::default().cell_spacing_mm
        );
    }
}
