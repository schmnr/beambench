export type CameraBackendKind = 'mock_snapshot' | 'native';

export interface CameraDeviceInfo {
  camera_id: string;
  display_name: string;
  backend_kind: CameraBackendKind;
  available: boolean;
  width_px: number;
  height_px: number;
  status_text: string;
}

export interface SimilarityTransform {
  scale: number;
  rotation_deg: number;
  translation_x: number;
  translation_y: number;
}

export interface CalibrationPoint {
  image_x: number;
  image_y: number;
  reference_x: number;
  reference_y: number;
}

export interface CalibrationPointSet {
  image_width_px: number;
  image_height_px: number;
  points: CalibrationPoint[];
}

export interface CameraCalibration {
  image_width_px: number;
  image_height_px: number;
  transform: SimilarityTransform;
  rmse_px: number;
  quality_score: number;
  solved_at: string;
}

export interface CalibrationSolveResult {
  calibration: CameraCalibration;
  point_count: number;
}

export interface AlignmentPoint {
  camera_x: number;
  camera_y: number;
  workspace_x_mm: number;
  workspace_y_mm: number;
}

export interface AlignmentPointSet {
  points: AlignmentPoint[];
}

export type CameraAlignmentSource = 'solved_points' | 'manual_adjust';

export interface CameraAlignment {
  transform: SimilarityTransform;
  image_width_px?: number;
  image_height_px?: number;
  rmse_mm: number;
  quality_score: number;
  solved_at: string;
  source?: CameraAlignmentSource;
}

export interface CameraFrameHandle {
  handle_id: string;
  file_path: string;
  width_px: number;
  height_px: number;
  media_type: string;
  captured_at: string;
}

export interface CameraOverlayState {
  selected_camera_id: string | null;
  frame: CameraFrameHandle | null;
  calibration: CameraCalibration | null;
  alignment: CameraAlignment | null;
  overlay_ready: boolean;
}

export type CameraOverlayStatus =
  | 'no_frame'
  | 'unsaved_changes'
  | 'adjusting_overlay'
  | 'preview_fitted_to_bed'
  | 'saved_alignment'
  | 'no_alignment';

export interface CameraOverlayDisplayState {
  overlay_visible: boolean;
  overlay_opacity: number;
  draft_overlay_transform: SimilarityTransform | null;
  draft_base_transform: SimilarityTransform | null;
  draft_dirty: boolean;
  overlay_adjust_mode: boolean;
  effective_transform: SimilarityTransform | null;
  status: CameraOverlayStatus;
}

export interface CameraArtifactInfo {
  path: string;
  temporary: boolean;
  bytes: number;
}

export interface CameraAgentState {
  schema_version: number;
  selected_camera_id: string | null;
  frame: CameraFrameHandle | null;
  calibration: CameraCalibration | null;
  alignment: CameraAlignment | null;
  overlay_ready: boolean;
  frontend_bridge_connected?: boolean;
  display: CameraOverlayDisplayState;
  latest_capture: CameraArtifactInfo | null;
  latest_render: CameraArtifactInfo | null;
}
