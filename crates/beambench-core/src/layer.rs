use beambench_common::markers::{CutEntryMarker, LayerMarker};
use beambench_common::{ColorTag, Id, RasterMode};
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_line_interval_mm() -> f64 {
    0.1
}

fn default_tab_width_mm() -> f64 {
    3.0
}

fn default_one() -> u32 {
    1
}

fn default_ninety() -> f64 {
    90.0
}

fn default_halftone_cpi() -> u32 {
    10
}

fn default_newsprint_angle() -> f64 {
    45.0
}

fn default_newsprint_frequency() -> f64 {
    10.0
}

pub type LayerId = Id<LayerMarker>;
pub type CutEntryId = Id<CutEntryMarker>;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationType {
    Image,
    #[default]
    Line,
    Fill,
    Score,
    Cut,
    OffsetFill,
    Tool,
}

impl OperationType {
    pub fn uses_raster_settings(self) -> bool {
        matches!(
            self,
            OperationType::Image | OperationType::Fill | OperationType::OffsetFill
        )
    }

    pub fn uses_vector_settings(self) -> bool {
        matches!(
            self,
            OperationType::Line
                | OperationType::Score
                | OperationType::Cut
                | OperationType::OffsetFill
        )
    }

    pub fn is_tool(self) -> bool {
        matches!(self, OperationType::Tool)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterSettings {
    pub dpi: u32,
    pub mode: RasterMode,
    pub scan_angle: f64,
    pub bidirectional: bool,
    pub overscan_mm: f64,
    pub passes: u32,
    #[serde(default = "default_line_interval_mm")]
    pub line_interval_mm: f64,
    #[serde(default)]
    pub crosshatch: bool,
    #[serde(default)]
    pub flood_fill: bool,
    #[serde(default = "default_one")]
    pub angle_passes: u32,
    #[serde(default = "default_ninety")]
    pub angle_increment_deg: f64,
    #[serde(default)]
    pub pass_through: bool,
    #[serde(default = "default_halftone_cpi")]
    pub halftone_cells_per_inch: u32,
    #[serde(default)]
    pub halftone_angle_deg: f64,
    #[serde(default = "default_newsprint_angle")]
    pub newsprint_angle_deg: f64,
    #[serde(default = "default_newsprint_frequency")]
    pub newsprint_frequency: f64,
    #[serde(default)]
    pub invert: bool,
    #[serde(default)]
    pub dot_width_correction_mm: f64,
    #[serde(default)]
    pub ramp_length_mm: f64,
}

impl Default for RasterSettings {
    fn default() -> Self {
        Self {
            dpi: 254,
            mode: RasterMode::FloydSteinberg,
            scan_angle: 0.0,
            bidirectional: true,
            overscan_mm: 2.5,
            passes: 1,
            line_interval_mm: 0.1,
            crosshatch: false,
            flood_fill: false,
            angle_passes: 1,
            angle_increment_deg: 90.0,
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
        }
    }
}

impl RasterSettings {
    pub fn effective_dpi(&self) -> u32 {
        if self.line_interval_mm > 0.0 {
            (25.4 / self.line_interval_mm).round() as u32
        } else if self.dpi > 0 {
            self.dpi
        } else {
            254
        }
    }

    pub fn effective_line_interval_mm(&self) -> f64 {
        if self.line_interval_mm > 0.0 {
            self.line_interval_mm
        } else if self.dpi > 0 {
            25.4 / self.dpi as f64
        } else {
            0.1
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorSettings {
    pub passes: u32,
    pub perforation_enabled: bool,
    pub perforation_on_ms: f64,
    pub perforation_off_ms: f64,
    #[serde(default)]
    pub kerf_offset_mm: f64,
    #[serde(default)]
    pub tab_count: u32,
    #[serde(default = "default_tab_width_mm")]
    pub tab_width_mm: f64,
    #[serde(default)]
    pub offset_overlap_mm: f64,
    #[serde(default)]
    pub offset_outward: bool,
    #[serde(default)]
    pub offset_fill_grouping_mode: OffsetFillGroupingMode,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OffsetFillGroupingMode {
    #[default]
    AllShapesAtOnce,
    GroupsTogether,
    ShapesIndividually,
}

impl Default for VectorSettings {
    fn default() -> Self {
        Self {
            passes: 1,
            perforation_enabled: false,
            perforation_on_ms: 10.0,
            perforation_off_ms: 10.0,
            kerf_offset_mm: 0.0,
            tab_count: 0,
            tab_width_mm: 3.0,
            offset_overlap_mm: 0.0,
            offset_outward: false,
            offset_fill_grouping_mode: OffsetFillGroupingMode::AllShapesAtOnce,
        }
    }
}

/// M4: batch-toggle mode for layer enabled/visible operations.
///
/// `OnlyThisOn` powers the row-menu "Disable/Hide all but this one" actions — the kept layer
/// stays on, every other layer is turned off in one undo snapshot. The other variants drive the
/// header-menu Enable all / Disable all / Invert actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LayerBatchToggle {
    AllOn,
    AllOff,
    Invert,
    OnlyThisOn { keep: LayerId },
}

/// M4: clipboard payload shape for `paste_layer_entries` — mirrors `CutEntry` minus `id`.
///
/// The backend mints a fresh `CutEntryId` for every entry built from a template, so a single
/// clipboard can be pasted onto N target layers without aliasing entry ids. Frontend never
/// supplies ids; the type is enforced structurally.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CutEntryTemplate {
    pub operation: OperationType,
    pub speed_mm_min: f64,
    pub power_percent: f64,
    pub raster_settings: Option<RasterSettings>,
    pub vector_settings: Option<VectorSettings>,
    #[serde(default)]
    pub air_assist: bool,
    #[serde(default)]
    pub power_min_percent: f64,
    #[serde(default)]
    pub z_offset_mm: f64,
    #[serde(default)]
    pub gcode_prefix: String,
    #[serde(default)]
    pub gcode_suffix: String,
    #[serde(default = "default_true")]
    pub output_enabled: bool,
}

impl CutEntryTemplate {
    /// Snapshot a `CutEntry` into a clipboard template (drops the id).
    pub fn from_entry(entry: &CutEntry) -> Self {
        Self {
            operation: entry.operation,
            speed_mm_min: entry.speed_mm_min,
            power_percent: entry.power_percent,
            raster_settings: entry.raster_settings.clone(),
            vector_settings: entry.vector_settings.clone(),
            air_assist: entry.air_assist,
            power_min_percent: entry.power_min_percent,
            z_offset_mm: entry.z_offset_mm,
            gcode_prefix: entry.gcode_prefix.clone(),
            gcode_suffix: entry.gcode_suffix.clone(),
            output_enabled: entry.output_enabled,
        }
    }

    /// Materialize this template into a fresh `CutEntry` with a newly minted id.
    pub fn into_entry(self) -> CutEntry {
        CutEntry {
            id: CutEntryId::new(),
            operation: self.operation,
            speed_mm_min: self.speed_mm_min,
            power_percent: self.power_percent,
            raster_settings: self.raster_settings,
            vector_settings: self.vector_settings,
            air_assist: self.air_assist,
            power_min_percent: self.power_min_percent,
            z_offset_mm: self.z_offset_mm,
            gcode_prefix: self.gcode_prefix,
            gcode_suffix: self.gcode_suffix,
            output_enabled: self.output_enabled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CutEntry {
    pub id: CutEntryId,
    pub operation: OperationType,
    pub speed_mm_min: f64,
    pub power_percent: f64,
    pub raster_settings: Option<RasterSettings>,
    pub vector_settings: Option<VectorSettings>,
    #[serde(default)]
    pub air_assist: bool,
    #[serde(default)]
    pub power_min_percent: f64,
    #[serde(default)]
    pub z_offset_mm: f64,
    #[serde(default)]
    pub gcode_prefix: String,
    #[serde(default)]
    pub gcode_suffix: String,
    #[serde(default = "default_true")]
    pub output_enabled: bool,
}

impl CutEntry {
    /// Canonical non-output marker used by T1/T2 tool layers.
    ///
    /// Tool layers intentionally carry no editable cut settings. Numeric fields stay neutral only
    /// so older serialization and summary surfaces can still deserialize/render the entry shape.
    pub fn tool_marker() -> Self {
        Self {
            id: CutEntryId::new(),
            operation: OperationType::Tool,
            speed_mm_min: 0.0,
            power_percent: 0.0,
            raster_settings: None,
            vector_settings: None,
            air_assist: false,
            power_min_percent: 0.0,
            z_offset_mm: 0.0,
            gcode_prefix: String::new(),
            gcode_suffix: String::new(),
            output_enabled: false,
        }
    }

    /// M4: canonical built-in default entry for the given operation.
    ///
    /// Single source of truth used by `CutEntry::new`, layer creation paths
    /// (`Layer::new_single_entry` → `add_layer` / `ensure_default_layer`), and the
    /// `reset_cut_entry_to_defaults` Tauri command. Frontend must not duplicate these
    /// values — call the backend reset op instead.
    pub fn defaults_for(operation: OperationType) -> Self {
        let (raster_settings, vector_settings) = match operation {
            OperationType::Image | OperationType::Fill => (Some(RasterSettings::default()), None),
            OperationType::OffsetFill => (
                Some(RasterSettings::default()),
                Some(VectorSettings::default()),
            ),
            OperationType::Tool => return Self::tool_marker(),
            _ => (None, Some(VectorSettings::default())),
        };

        Self {
            id: CutEntryId::new(),
            operation,
            speed_mm_min: 1000.0,
            power_percent: 50.0,
            raster_settings,
            vector_settings,
            air_assist: false,
            power_min_percent: 0.0,
            z_offset_mm: 0.0,
            gcode_prefix: String::new(),
            gcode_suffix: String::new(),
            output_enabled: true,
        }
    }

    pub fn new(operation: OperationType) -> Self {
        Self::defaults_for(operation)
    }

    pub fn ensure_raster_settings(&mut self) {
        if self.raster_settings.is_none() {
            self.raster_settings = Some(RasterSettings::default());
        }
    }

    /// M4: passes count read from the operation-appropriate settings bag.
    ///
    /// Used by `Layer::cut_strength` for Sort Cuts Last. Falls back to 1 when neither bag is
    /// populated (e.g., a freshly stubbed transient seed). Image/Fill use raster passes; vector
    /// ops use vector passes.
    pub fn passes_for_operation(&self) -> u32 {
        match self.operation {
            OperationType::Image | OperationType::Fill => {
                self.raster_settings.as_ref().map(|r| r.passes).unwrap_or(1)
            }
            OperationType::Tool => 0,
            _ => self.vector_settings.as_ref().map(|v| v.passes).unwrap_or(1),
        }
    }

    pub fn apply_patch(&mut self, patch: &CutEntryPatch) -> bool {
        let mut next = self.clone();
        let operation_changed = patch.operation.is_some();

        if let Some(id) = patch.id {
            next.id = id;
        }
        if let Some(operation) = patch.operation {
            next.operation = operation;
        }
        if let Some(speed_mm_min) = patch.speed_mm_min {
            next.speed_mm_min = speed_mm_min;
        }
        if let Some(power_percent) = patch.power_percent {
            next.power_percent = power_percent;
        }
        if let Some(raster_settings) = &patch.raster_settings {
            next.raster_settings = raster_settings.clone();
        }
        if let Some(vector_settings) = &patch.vector_settings {
            next.vector_settings = vector_settings.clone();
        }
        if let Some(air_assist) = patch.air_assist {
            next.air_assist = air_assist;
        }
        if let Some(power_min_percent) = patch.power_min_percent {
            next.power_min_percent = power_min_percent;
        }
        if let Some(z_offset_mm) = patch.z_offset_mm {
            next.z_offset_mm = z_offset_mm;
        }
        if let Some(gcode_prefix) = &patch.gcode_prefix {
            next.gcode_prefix = gcode_prefix.clone();
        }
        if let Some(gcode_suffix) = &patch.gcode_suffix {
            next.gcode_suffix = gcode_suffix.clone();
        }
        if let Some(output_enabled) = patch.output_enabled {
            next.output_enabled = output_enabled;
        }

        if operation_changed {
            if next.operation == OperationType::Tool {
                let id = next.id;
                next = CutEntry::tool_marker();
                next.id = id;
            } else if next.operation == OperationType::OffsetFill {
                if next.raster_settings.is_none() {
                    next.raster_settings = Some(RasterSettings::default());
                }
                if next.vector_settings.is_none() {
                    next.vector_settings = Some(VectorSettings::default());
                }
            } else if next.operation.uses_raster_settings() {
                next.vector_settings = None;
                if next.raster_settings.is_none() {
                    next.raster_settings = Some(RasterSettings::default());
                }
            } else {
                next.raster_settings = None;
                if next.vector_settings.is_none() {
                    next.vector_settings = Some(VectorSettings::default());
                }
            }
        }

        if *self == next {
            false
        } else {
            *self = next;
            true
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CutEntryPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<CutEntryId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<OperationType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed_mm_min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_percent: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raster_settings: Option<Option<RasterSettings>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vector_settings: Option<Option<VectorSettings>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub air_assist: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_min_percent: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z_offset_mm: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gcode_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gcode_suffix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LayerPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(default, alias = "color", skip_serializing_if = "Option::is_none")]
    pub color_tag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    pub enabled: bool,
    pub order_index: u32,
    pub color_tag: ColorTag,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default = "default_false")]
    pub is_tool_layer: bool,
    pub entries: Vec<CutEntry>,
}

impl Layer {
    pub fn new_single_entry(name: impl Into<String>, operation: OperationType) -> Self {
        Self {
            id: LayerId::new(),
            name: name.into(),
            enabled: true,
            order_index: 0,
            color_tag: ColorTag::default(),
            visible: true,
            is_tool_layer: false,
            entries: vec![CutEntry::new(operation)],
        }
    }

    pub fn new(name: impl Into<String>, operation: OperationType) -> Self {
        Self::new_single_entry(name, operation)
    }

    pub fn primary_entry(&self) -> &CutEntry {
        self.entries
            .first()
            .expect("Layer invariant violated: entries must not be empty")
    }

    pub fn primary_entry_mut(&mut self) -> &mut CutEntry {
        self.entries
            .first_mut()
            .expect("Layer invariant violated: entries must not be empty")
    }

    pub fn ensure_raster_settings(&mut self) {
        self.primary_entry_mut().ensure_raster_settings();
    }

    pub fn canonicalize_tool_layer(&mut self) {
        self.is_tool_layer = true;
        self.entries = vec![CutEntry::tool_marker()];
    }

    /// M4 Sort Cuts Last heuristic: higher score → sorts later (more cut-like).
    ///
    /// Score = energy_density (passes × power% / max(speed, 1)) + line_bias.
    /// Energy carries most of the signal so a high-power slow Cut sorts after a low-power fast
    /// Fill regardless of `OperationType`. The line-bias is a tiebreaker that pushes Score and
    /// Line/Cut/OffsetFill ops later when energy is comparable, so cut-class
    /// operations finish after engraving.
    pub fn cut_strength(&self) -> f64 {
        let e = self.primary_entry();
        let energy = (e.passes_for_operation() as f64) * e.power_percent / e.speed_mm_min.max(1.0);
        let line_bias = match e.operation {
            OperationType::Image | OperationType::Fill => 0.0,
            OperationType::Tool => 0.0,
            OperationType::Score => 1.0,
            OperationType::Line | OperationType::Cut | OperationType::OffsetFill => 2.0,
        };
        energy + line_bias
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_cut_entry_has_raster_settings() {
        let entry = CutEntry::new(OperationType::Image);
        assert!(entry.raster_settings.is_some());
        assert!(entry.vector_settings.is_none());
    }

    #[test]
    fn line_cut_entry_has_vector_settings() {
        let entry = CutEntry::new(OperationType::Line);
        assert!(entry.raster_settings.is_none());
        assert!(entry.vector_settings.is_some());
    }

    #[test]
    fn effective_dpi_prefers_canonical_line_interval() {
        let mut rs = RasterSettings::default();
        rs.dpi = 100;
        rs.line_interval_mm = 0.05;
        assert_eq!(rs.effective_dpi(), 508);
    }

    #[test]
    fn effective_dpi_falls_back_to_legacy_dpi() {
        let mut rs = RasterSettings::default();
        rs.dpi = 300;
        rs.line_interval_mm = 0.0;
        assert_eq!(rs.effective_dpi(), 300);
    }

    #[test]
    fn effective_dpi_falls_back_to_sensible_default() {
        let mut rs = RasterSettings::default();
        rs.dpi = 0;
        rs.line_interval_mm = 0.0;
        assert_eq!(rs.effective_dpi(), 254);
    }

    #[test]
    fn effective_line_interval_mm_prefers_canonical() {
        let mut rs = RasterSettings::default();
        rs.dpi = 100;
        rs.line_interval_mm = 0.08;
        assert!((rs.effective_line_interval_mm() - 0.08).abs() < 1e-9);
    }

    #[test]
    fn effective_line_interval_mm_falls_back_to_legacy_dpi() {
        let mut rs = RasterSettings::default();
        rs.dpi = 254;
        rs.line_interval_mm = 0.0;
        assert!((rs.effective_line_interval_mm() - 0.1).abs() < 1e-4);
    }

    #[test]
    fn raster_settings_defaults_have_new_image_fields_off() {
        let rs = RasterSettings::default();
        assert!(!rs.invert);
        assert_eq!(rs.dot_width_correction_mm, 0.0);
        assert_eq!(rs.ramp_length_mm, 0.0);
    }

    #[test]
    fn layer_new_has_single_entry() {
        let layer = Layer::new_single_entry("Test", OperationType::Cut);
        assert_eq!(layer.entries.len(), 1);
        assert_eq!(layer.primary_entry().operation, OperationType::Cut);
    }

    #[test]
    fn layer_roundtrips_through_json() {
        let layer = Layer::new_single_entry("Engrave", OperationType::Line);
        let json = serde_json::to_string(&layer).unwrap();
        let restored: Layer = serde_json::from_str(&json).unwrap();
        assert_eq!(layer, restored);
    }

    #[test]
    fn operation_type_serializes_snake_case() {
        let json = serde_json::to_string(&OperationType::Image).unwrap();
        assert_eq!(json, "\"image\"");
    }

    #[test]
    fn uses_raster_settings_classification() {
        assert!(OperationType::Image.uses_raster_settings());
        assert!(OperationType::Fill.uses_raster_settings());
        assert!(OperationType::OffsetFill.uses_raster_settings());
        assert!(!OperationType::Line.uses_raster_settings());
        assert!(!OperationType::Cut.uses_raster_settings());
        assert!(!OperationType::Score.uses_raster_settings());
        assert!(!OperationType::Tool.uses_raster_settings());
    }

    #[test]
    fn uses_vector_settings_classification() {
        assert!(OperationType::Line.uses_vector_settings());
        assert!(OperationType::Cut.uses_vector_settings());
        assert!(OperationType::Score.uses_vector_settings());
        assert!(OperationType::OffsetFill.uses_vector_settings());
        assert!(!OperationType::Image.uses_vector_settings());
        assert!(!OperationType::Fill.uses_vector_settings());
        assert!(!OperationType::Tool.uses_vector_settings());
    }

    #[test]
    fn tool_marker_has_no_output_settings() {
        let entry = CutEntry::tool_marker();
        assert_eq!(entry.operation, OperationType::Tool);
        assert_eq!(entry.speed_mm_min, 0.0);
        assert_eq!(entry.power_percent, 0.0);
        assert!(!entry.output_enabled);
        assert!(entry.raster_settings.is_none());
        assert!(entry.vector_settings.is_none());
    }

    #[test]
    fn canonicalize_tool_layer_replaces_cut_entries() {
        let mut layer = Layer::new_single_entry("T1", OperationType::Cut);
        layer.entries[0].speed_mm_min = 5000.0;
        layer.entries.push(CutEntry::new(OperationType::Image));

        layer.canonicalize_tool_layer();

        assert!(layer.is_tool_layer);
        assert_eq!(layer.entries.len(), 1);
        assert_eq!(layer.primary_entry().operation, OperationType::Tool);
        assert_eq!(layer.primary_entry().speed_mm_min, 0.0);
        assert!(!layer.primary_entry().output_enabled);
    }

    #[test]
    fn offset_fill_defaults_include_raster_and_vector_settings() {
        let entry = CutEntry::defaults_for(OperationType::OffsetFill);
        assert!(entry.raster_settings.is_some());
        assert!(entry.vector_settings.is_some());
        assert_eq!(
            entry.vector_settings.unwrap().offset_fill_grouping_mode,
            OffsetFillGroupingMode::AllShapesAtOnce
        );
    }

    #[test]
    fn new_layer_defaults_live_on_primary_entry() {
        let layer = Layer::new_single_entry("Test", OperationType::Line);
        let entry = layer.primary_entry();
        assert!(layer.visible);
        assert!(!layer.is_tool_layer);
        assert!(entry.output_enabled);
        assert!(!entry.air_assist);
        assert_eq!(entry.power_min_percent, 0.0);
        assert_eq!(entry.z_offset_mm, 0.0);
        assert_eq!(entry.gcode_prefix, "");
        assert_eq!(entry.gcode_suffix, "");
    }

    #[test]
    fn ensure_raster_settings_populates_primary_entry() {
        let mut layer = Layer::new_single_entry("Test", OperationType::Cut);
        assert!(layer.primary_entry().raster_settings.is_none());
        layer.ensure_raster_settings();
        assert!(layer.primary_entry().raster_settings.is_some());
    }

    #[test]
    fn cut_entry_patch_noop_returns_false() {
        let mut entry = CutEntry::new(OperationType::Line);
        let patch = CutEntryPatch::default();
        assert!(!entry.apply_patch(&patch));
    }

    #[test]
    fn cut_entry_patch_updates_fields_and_preserves_siblings() {
        let mut entry = CutEntry::new(OperationType::Cut);
        let id = entry.id;
        let vector_settings = entry.vector_settings.clone();
        let patch = CutEntryPatch {
            speed_mm_min: Some(2500.0),
            power_percent: Some(75.0),
            output_enabled: Some(false),
            ..Default::default()
        };

        assert!(entry.apply_patch(&patch));
        assert_eq!(entry.id, id);
        assert_eq!(entry.speed_mm_min, 2500.0);
        assert_eq!(entry.power_percent, 75.0);
        assert!(!entry.output_enabled);
        assert_eq!(entry.vector_settings, vector_settings);
    }

    #[test]
    fn cut_entry_patch_can_clear_nullable_settings() {
        let mut entry = CutEntry::new(OperationType::Image);
        assert!(entry.raster_settings.is_some());
        let patch = CutEntryPatch {
            raster_settings: Some(None),
            ..Default::default()
        };
        assert!(entry.apply_patch(&patch));
        assert!(entry.raster_settings.is_none());
    }
}
