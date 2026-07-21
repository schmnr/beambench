use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraBackendKind {
    MockSnapshot,
    Native,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraDeviceInfo {
    pub camera_id: String,
    pub display_name: String,
    pub backend_kind: CameraBackendKind,
    pub available: bool,
    pub width_px: u32,
    pub height_px: u32,
    pub status_text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimilarityTransform {
    pub scale: f64,
    pub rotation_deg: f64,
    pub translation_x: f64,
    pub translation_y: f64,
}

impl Default for SimilarityTransform {
    fn default() -> Self {
        Self {
            scale: 1.0,
            rotation_deg: 0.0,
            translation_x: 0.0,
            translation_y: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibrationPoint {
    pub image_x: f64,
    pub image_y: f64,
    pub reference_x: f64,
    pub reference_y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibrationPointSet {
    pub image_width_px: u32,
    pub image_height_px: u32,
    pub points: Vec<CalibrationPoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraCalibration {
    pub image_width_px: u32,
    pub image_height_px: u32,
    pub transform: SimilarityTransform,
    pub rmse_px: f64,
    pub quality_score: f64,
    pub solved_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibrationSolveResult {
    pub calibration: CameraCalibration,
    pub point_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlignmentPoint {
    pub camera_x: f64,
    pub camera_y: f64,
    pub workspace_x_mm: f64,
    pub workspace_y_mm: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlignmentPointSet {
    pub points: Vec<AlignmentPoint>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraAlignmentSource {
    #[default]
    SolvedPoints,
    ManualAdjust,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraAlignment {
    pub transform: SimilarityTransform,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_width_px: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_height_px: Option<u32>,
    pub rmse_mm: f64,
    pub quality_score: f64,
    pub solved_at: String,
    #[serde(default)]
    pub source: CameraAlignmentSource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraFrameHandle {
    pub handle_id: String,
    pub file_path: String,
    pub width_px: u32,
    pub height_px: u32,
    pub media_type: String,
    pub captured_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraOverlayState {
    pub selected_camera_id: Option<String>,
    pub frame: Option<CameraFrameHandle>,
    pub calibration: Option<CameraCalibration>,
    pub alignment: Option<CameraAlignment>,
    pub overlay_ready: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraOverlayStatus {
    NoFrame,
    UnsavedChanges,
    AdjustingOverlay,
    PreviewFittedToBed,
    SavedAlignment,
    NoAlignment,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraOverlayRuntimeState {
    pub overlay_visible: bool,
    pub overlay_opacity: f64,
    pub draft_overlay_transform: Option<SimilarityTransform>,
    pub draft_base_transform: Option<SimilarityTransform>,
    pub draft_dirty: bool,
    pub overlay_adjust_mode: bool,
}

impl Default for CameraOverlayRuntimeState {
    fn default() -> Self {
        Self {
            overlay_visible: true,
            overlay_opacity: 0.4,
            draft_overlay_transform: None,
            draft_base_transform: None,
            draft_dirty: false,
            overlay_adjust_mode: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraOverlayDisplayState {
    pub overlay_visible: bool,
    pub overlay_opacity: f64,
    pub draft_overlay_transform: Option<SimilarityTransform>,
    pub draft_base_transform: Option<SimilarityTransform>,
    pub draft_dirty: bool,
    pub overlay_adjust_mode: bool,
    pub effective_transform: Option<SimilarityTransform>,
    pub status: CameraOverlayStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraArtifactInfo {
    pub path: String,
    pub temporary: bool,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraAgentState {
    pub schema_version: u32,
    pub selected_camera_id: Option<String>,
    pub frame: Option<CameraFrameHandle>,
    pub calibration: Option<CameraCalibration>,
    pub alignment: Option<CameraAlignment>,
    pub overlay_ready: bool,
    pub frontend_bridge_connected: bool,
    pub display: CameraOverlayDisplayState,
    pub latest_capture: Option<CameraArtifactInfo>,
    pub latest_render: Option<CameraArtifactInfo>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraOverlayRenderView {
    #[default]
    Fit,
    Current,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraOverlayRenderOptions {
    pub output_path: String,
    pub view: CameraOverlayRenderView,
    pub format: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraOverlayRenderResult {
    pub schema_version: u32,
    pub path: String,
    pub view: CameraOverlayRenderView,
    pub format: String,
    pub temporary: bool,
    pub bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calibration_roundtrip() {
        let calibration = CameraCalibration {
            image_width_px: 1920,
            image_height_px: 1080,
            transform: SimilarityTransform::default(),
            rmse_px: 1.25,
            quality_score: 0.9,
            solved_at: "2026-03-19T12:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&calibration).unwrap();
        let restored: CameraCalibration = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.image_width_px, 1920);
        assert_eq!(restored.quality_score, 0.9);
    }

    #[test]
    fn legacy_alignment_defaults_to_solved_points_source() {
        let json = r#"{
            "transform": {
                "scale": 1.0,
                "rotation_deg": 0.0,
                "translation_x": 0.0,
                "translation_y": 0.0
            },
            "rmse_mm": 0.0,
            "quality_score": 1.0,
            "solved_at": "2026-03-19T12:00:00Z"
        }"#;
        let restored: CameraAlignment = serde_json::from_str(json).unwrap();
        assert_eq!(restored.source, CameraAlignmentSource::SolvedPoints);
        assert_eq!(restored.image_width_px, None);
        assert_eq!(restored.image_height_px, None);
    }

    #[test]
    fn manual_alignment_source_roundtrips() {
        let alignment = CameraAlignment {
            transform: SimilarityTransform::default(),
            image_width_px: Some(1920),
            image_height_px: Some(1080),
            rmse_mm: 0.0,
            quality_score: 1.0,
            solved_at: "2026-03-19T12:00:00Z".to_string(),
            source: CameraAlignmentSource::ManualAdjust,
        };
        let json = serde_json::to_string(&alignment).unwrap();
        let restored: CameraAlignment = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.source, CameraAlignmentSource::ManualAdjust);
        assert_eq!(restored.image_width_px, Some(1920));
        assert_eq!(restored.image_height_px, Some(1080));
    }
}
