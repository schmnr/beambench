import type { MachineProfile } from './machine';
import type { RasterAdjustments } from './project';

// `CommandResult<T>` was a documented-but-never-used shape. Tauri
// commands return the data type directly and throw on error; no wrapper is
// used anywhere in the app. Removed to prevent future code from adopting a
// contract the backend doesn't honor.

/** App version and state. */
export interface AppStatus {
  version: string;
  state: string;
}

/** Build metadata for the About dialog and bug reports. */
export interface BuildInfo {
  version: string;
  git_sha: string;
  build_timestamp: string;
  target_triple: string;
  rustc_version: string;
  channel: string;
}

/** A recently opened/saved project file. */
export interface RecentFile {
  path: string;
  name: string;
  opened_at: string;
}

export interface SavedPosition {
  id: string;
  name: string;
  x: number;
  y: number;
  z?: number | null;
}

export interface ImagePreset {
  name: string;
  adjustments: RasterAdjustments;
}

export type ArtworkExportFormat = 'svg' | 'dxf' | 'pdf' | 'eps' | 'ai' | 'png' | 'jpg' | 'bmp';

export interface ExportSettings {
  last_directory: string | null;
  last_format: ArtworkExportFormat;
  filename_stem: string | null;
}

export type UiTheme = 'system' | 'light' | 'dark';

/** User-configurable application settings. */
export interface AppSettings {
  display_unit: 'mm' | 'inches';
  speed_time_unit?: 'minutes' | 'seconds';
  autosave_enabled: boolean;
  autosave_interval_secs: number;
  machine_profiles: MachineProfile[];
  active_profile_id: string | null;
  recent_files: RecentFile[];
  api_enabled: boolean;
  api_port: number;
  api_localhost_only: boolean;
  ui_theme: UiTheme;
  dark_mode: boolean;
  antialiasing: boolean;
  filled_rendering: boolean;
  reduce_motion: boolean;
  show_palette_labels: boolean;
  cursor_size: 'small' | 'normal' | 'large';
  toolbar_icon_size: 'small' | 'normal' | 'large';
  click_tolerance_px: number;
  snap_threshold_px: number;
  grid_spacing_mm: number;
  nudge_step_mm: number;
  nudge_step_fine_mm: number;
  nudge_step_coarse_mm: number;
  scroll_zoom: boolean;
  debug_log_enabled: boolean;
  panel_layout: {
    zones: Record<string, { panel_ids: string[]; active_tab: string }>;
    hidden_panel_ids: string[];
    upper_split_ratio: number;
    right_panel_width: number;
    left_panel_width: number;
    bottom_panel_height: number;
    side_panels_visible: boolean;
    toolbar_visibility?: Record<string, boolean>;
    floating_panels: Array<{
      panel_id: string;
      x: number;
      y: number;
      width: number;
      height: number;
      z_index: number;
      origin_zone: string | null;
      origin_index: number | null;
    }>;
  } | null;
  saved_positions: SavedPosition[];
  last_radius_mm: number;
  image_presets: ImagePreset[];
  custom_hotkeys: Record<string, string>;
  export_settings: ExportSettings;
  allow_importing_to_tool_layers?: boolean;
  include_tool_layers_in_job_bounds?: boolean;
  check_for_updates_on_startup?: boolean;
  update_snoozed_until?: string;
  skipped_update_version?: string;
  display_language?: string;
}
