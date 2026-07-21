import type { CameraAlignment, CameraCalibration } from './camera';
import type { CutEntry } from './project';
import { defaultRasterSettings, defaultVectorSettings } from './cutEntryDefaults';

/** Machine connection and job control types — mirrors beambench-common/machine.rs */

export type SessionState =
  | 'disconnected'
  | 'connecting'
  | 'transport_open'
  | 'waiting_for_banner'
  | 'validating'
  | 'ready'
  | 'running'
  | 'paused'
  | 'alarm'
  | 'error';

export type MachineRunState =
  | 'idle'
  | 'run'
  | 'hold'
  | 'jog'
  | 'home'
  | 'alarm'
  | 'door'
  | 'sleep'
  | 'check'
  | 'unknown';

export interface MachinePosition {
  x: number;
  y: number;
  z: number;
}

export interface MachineStatus {
  run_state: MachineRunState;
  machine_position: MachinePosition;
  work_position: MachinePosition;
  feed_rate: number;
  spindle_speed: number;
  feed_override: number;
  spindle_override: number;
  rapid_override: number;
  pin_states: string;
}

export type JobState =
  | 'idle'
  | 'preparing'
  | 'ready_to_run'
  | 'running'
  | 'paused'
  | 'completed'
  | 'failed'
  | 'cancelled';

export interface JobProgressBucket {
  layer_id: string;
  cut_entry_id: string;
  segment_count: number;
}

export interface JobProgress {
  state: JobState;
  total_lines: number;
  queued_lines: number;
  sent_lines: number;
  acknowledged_lines: number;
  elapsed_secs: number;
  estimated_remaining_secs: number;
  buffer_fill_bytes: number;
  error_message?: string | null;
  buckets?: JobProgressBucket[];
}

export interface PortInfo {
  port_name: string;
  description: string;
  manufacturer: string;
  vid: number | null;
  pid: number | null;
}

export type ControllerFamily = 'unknown' | 'gcode' | 'dsp' | 'galvo';

export type ControllerModel =
  | 'unknown'
  | 'grbl'
  | 'fluid_nc'
  | 'grbl_hal'
  | 'laser_pecker'
  | 'marlin'
  | 'snapmaker'
  | 'smoothieware'
  | 'ruida'
  | 'lihuiyu_m2_nano'
  | 'trocen'
  | 'topwisdom'
  | 'ezcad2'
  | 'ezcad2_lite'
  | 'bsl';

export type GrblFamilyDialect = 'unknown' | 'grbl' | 'fluid_nc' | 'grbl_hal';

export type GrblFamilyIdentityStatus =
  | 'unknown'
  | 'protocol_compatible'
  | 'provisional'
  | 'identified'
  | 'conflicting';

/** Redacted identity evidence categories; raw controller text is never carried here. */
export type GrblFamilyIdentityEvidence =
  | 'startup_banner'
  | 'protocol_signature'
  | 'controller_info_version'
  | 'firmware_identity_message';

export interface GrblFamilyIdentity {
  dialect: GrblFamilyDialect;
  status: GrblFamilyIdentityStatus;
  firmware_identity: string | null;
  firmware_version: string | null;
  evidence: GrblFamilyIdentityEvidence[];
}

export type ControllerProductTier =
  | 'unavailable'
  | 'internal'
  | 'experimental'
  | 'beta'
  | 'supported';

export type ControllerEvidenceState =
  | 'emulated'
  | 'hardware_observed'
  | 'community_validated'
  | 'lab_vendor_verified';

/** Known driver IDs; availability is reported separately by backend policy. */
export type ControllerDriverId =
  | 'grbl'
  | 'fluid_nc'
  | 'grbl_hal'
  | 'laser_pecker'
  | 'marlin'
  | 'snapmaker'
  | 'smoothieware'
  | 'ruida'
  | 'lihuiyu'
  | 'unknown';

export type ExplicitControllerSelection =
  | { mode: 'known_driver'; driver: ControllerDriverId }
  | { mode: 'generic_grbl_compatible' }
  | { mode: 'unknown' };

export type ControllerSelection = { mode: 'auto_detect' } | ExplicitControllerSelection;

export interface PositiveControllerIdentity {
  family: ControllerFamily;
  model: ControllerModel;
  firmware_identity?: string | null;
  firmware_version?: string | null;
  evidence: string[];
}

export type DeviceFingerprintStrength = 'weak' | 'strong';

export interface DeviceFingerprint {
  schema_version: number;
  strength: DeviceFingerprintStrength;
  /** Backend-local digest; never send it in diagnostics, logs, or exports. */
  value: string;
}

export interface ControllerIdentityBinding {
  schema_version: number;
  family: ControllerFamily;
  model: ControllerModel;
  firmware_fingerprint: string;
}

/** Backend-local authorization state; never accept this object as UI authority. */
export interface FingerprintBoundControllerOverride {
  selection: ExplicitControllerSelection;
  detected_identity: ControllerIdentityBinding;
  transport_kind: TransportKind;
  fingerprint: DeviceFingerprint;
}

export type ControllerMismatchDecision =
  | 'use_detected'
  | 'continue_selected_experimentally'
  | 'cancel';

export type ControllerOverrideScope = 'session_only' | 'fingerprint_bound';

export type ControllerChoiceSource =
  | 'auto_detected'
  | 'known_driver_selection'
  | 'detected_driver_choice'
  | 'user_experimental_override'
  | 'remembered_override';

export type ControllerOverrideInvalidationReason =
  | 'selection_changed'
  | 'detected_identity_changed'
  | 'firmware_identity_changed'
  | 'transport_changed'
  | 'device_fingerprint_changed'
  | 'fingerprint_unavailable'
  | 'fingerprint_too_weak';

export type ControllerChoiceBlockReason =
  | 'unsupported_driver'
  | 'unsupported_transport'
  | 'detected_driver_unavailable'
  | 'invalid_decision';

export type ControllerOverrideUpdate =
  | { action: 'keep' }
  | { action: 'clear'; reason: ControllerOverrideInvalidationReason }
  | { action: 'replace' };

export interface ResolvedControllerChoice {
  selection: ExplicitControllerSelection;
  driver: ControllerDriverId;
  source: ControllerChoiceSource;
  detected_identity: PositiveControllerIdentity | null;
  /** Additional restriction only; never promotes an unavailable/internal driver. */
  requires_experimental_mode: boolean;
  mismatch: boolean;
  override_scope: ControllerOverrideScope | null;
  /** Extra check beyond the normal driver handshake required for every connection. */
  requires_experimental_compatibility_handshake: boolean;
}

export type ControllerChoiceOutcome =
  | { outcome: 'resolved'; choice: ResolvedControllerChoice }
  | { outcome: 'selection_required' }
  | {
      outcome: 'mismatch_decision_required';
      selected: ExplicitControllerSelection;
      detected_identity: PositiveControllerIdentity;
      detected_driver: ControllerDriverId | null;
      can_remember_override: boolean;
      invalidated_override_reason: ControllerOverrideInvalidationReason | null;
      allowed_decisions: ControllerMismatchDecision[];
    }
  | { outcome: 'cancelled' }
  | {
      outcome: 'blocked';
      reason: ControllerChoiceBlockReason;
      message: string;
    };

export type ControllerChoiceResolution = ControllerChoiceOutcome & {
  override_update: ControllerOverrideUpdate;
};

export type ControllerConnectionEndpoint =
  | { type: 'serial'; port_name: string; baud_rate: number }
  | { type: 'tcp'; host: string; port: number }
  | { type: 'udp'; host: string; port: number }
  | { type: 'usb'; device_id: string; vendor_id: number; product_id: number };

export interface LihuiyuUsbDeviceInfo {
  bus_id: string;
  device_address: number;
  port_numbers: number[];
  vendor_id: number;
  product_id: number;
  manufacturer: string | null;
  product: string | null;
  serial_number: string | null;
  has_required_bulk_endpoints: boolean | null;
  driver: string | null;
}

export type ControllerConnectionResult =
  | {
      status: 'connected';
      session_state: SessionState;
      endpoint: ControllerConnectionEndpoint;
      choice: ResolvedControllerChoice;
    }
  | {
      status: 'challenge';
      attempt_id: string;
      endpoint: ControllerConnectionEndpoint;
      detected_identity: PositiveControllerIdentity | null;
      resolution: ControllerChoiceResolution;
    }
  | { status: 'cancelled' };

export type TransportKind = 'serial' | 'tcp' | 'udp' | 'usb_packet';

export interface DeviceIdentity {
  display_name: string;
  manufacturer?: string | null;
  description?: string | null;
  product?: string | null;
  serial_number?: string | null;
  vendor_id?: number | null;
  product_id?: number | null;
  port_name?: string | null;
  host?: string | null;
  tcp_port?: number | null;
  udp_port?: number | null;
  usb_path?: string | null;
}

export interface DeviceCapabilities {
  can_home: boolean;
  can_jog: boolean;
  /** Press-and-hold jogging with a cancel command; false means finite steps only. */
  can_jog_continuous: boolean;
  can_unlock: boolean;
  can_pause_resume: boolean;
  can_set_origin: boolean;
  can_frame: boolean;
  can_run_job: boolean;
  /** The controller reports a trustworthy absolute position. */
  reports_absolute_position: boolean;
  /** Manual laser fire pulses (GCode/GRBL protocol surface only). */
  can_manual_fire: boolean;
  /** Realtime feed/spindle/rapid overrides during a job. */
  can_adjust_overrides: boolean;
  supports_rotary: boolean;
  supports_cylinder: boolean;
  supports_camera_alignment: boolean;
}

export interface MachineRuntimeState {
  capabilities: DeviceCapabilities | null;
  session_state: SessionState;
}

export interface DiscoveryCandidate {
  id: string;
  controller_family: ControllerFamily;
  controller_model: ControllerModel;
  transport_kind: TransportKind;
  identity: DeviceIdentity;
  confidence: number;
  capabilities: DeviceCapabilities;
  /** `null` means legacy availability has not been normalized yet. */
  product_tier: ControllerProductTier | null;
  /** `null` means legacy evidence has not been normalized yet. */
  evidence_state: ControllerEvidenceState | null;
  status_text: string;
  unsupported_reason?: string | null;
}

export type DiscoveryPhase = 'idle' | 'scanning' | 'completed' | 'cancelled';

export interface DiscoveryTcpTarget {
  host: string;
  port: number;
  label?: string | null;
}

export interface DiscoveryUsbTarget {
  device_path: string;
  manufacturer?: string | null;
  product?: string | null;
}

export interface DiscoveryScanState {
  phase: DiscoveryPhase;
  status_text: string;
  candidates: DiscoveryCandidate[];
  scanned_serial_count: number;
  scanned_tcp_count: number;
  scanned_usb_count: number;
  started_at?: string | null;
  completed_at?: string | null;
}

export type PreflightOutcome = 'pass' | 'pass_with_warnings' | 'fail';
export type FrameMode = 'rectangular' | 'rubber_band';
export type OverrideAction = 'reset' | 'increase_10' | 'decrease_10' | 'increase_1' | 'decrease_1';

export interface PreflightCheck {
  category: string;
  description: string;
  passed: boolean;
  message: string;
}

export interface PreflightReport {
  outcome: PreflightOutcome;
  checks: PreflightCheck[];
}

export interface ScanningOffsetEntry {
  speed_mm_min: number;
  offset_mm: number;
}

export interface MachineProfile {
  id: string;
  name: string;
  preset_id?: string | null;
  preset_version?: number | null;
  bed_width_mm: number;
  bed_height_mm: number;
  max_speed_mm_min: number;
  max_power_percent: number;
  s_value_max: number;
  homing_enabled: boolean;
  default_baud_rate: number;
  firmware_type: string;
  notes: string;
  origin: 'top_left' | 'bottom_left';
  laser_offset_x: number;
  laser_offset_y: number;
  enable_laser_offset: boolean;
  swap_xy: boolean;
  selected_camera_id: string | null;
  camera_calibration: CameraCalibration | null;
  camera_alignment: CameraAlignment | null;
  job_checklist: boolean;
  frame_continuously: boolean;
  laser_on_when_framing: boolean;
  tab_pulse_width_ms: number;
  cnc_machine: boolean;
  use_constant_power: boolean;
  emit_s_every_g1: boolean;
  use_g0_for_overscan: boolean;
  air_assist_on_gcode?: string;
  air_assist_off_gcode?: string;
  air_assist_on_delay_ms?: number;
  job_header_gcode?: string;
  job_footer_gcode?: string;
  transfer_mode?: 'buffered' | 'synchronous';
  preferred_default_origin?: 'top_left' | 'bottom_left' | null;
  scanning_offsets: ScanningOffsetEntry[];
  enable_scanning_offset: boolean;
  dot_width_mm: number;
  enable_dot_width: boolean;
  /** M3: whether the machine supports Z-axis motions (gates Focus Test live actions). */
  supports_z_moves?: boolean;
  /** Controlled Z move feed rate for GRBL Z offsets and Focus Test sweeps. */
  z_move_feed_mm_min?: number;
  /** Ruida controller channel used for finite manual lift-table jogging. */
  ruida_table_axis?: 'disabled' | 'z' | 'u';
  /** Manual fire button is hidden unless this is explicitly enabled. */
  enable_laser_fire_button?: boolean;
  /** Manual fire power in 0-100 percent units. 1 means 1%. */
  default_fire_power_percent?: number;
  /** M3: persisted Material/Focus/Interval Test dialog state. */
  quality_test_settings?: QualityTestSettings;
}

export interface MachineProfilePreset {
  id: string;
  version: number;
  name: string;
  description: string;
  advisory_text: string | null;
  firmware_type: string;
  default_baud_rate: number;
  bed_width_mm: number;
  bed_height_mm: number;
  max_speed_mm_min: number;
  max_power_percent: number;
  s_value_max: number;
  homing_enabled: boolean;
  origin: 'top_left' | 'bottom_left';
  use_constant_power: boolean;
  emit_s_every_g1: boolean;
  use_g0_for_overscan: boolean;
  air_assist_on_gcode: string;
  air_assist_off_gcode: string;
  air_assist_on_delay_ms: number;
  job_header_gcode: string;
  job_footer_gcode: string;
  transfer_mode: 'buffered' | 'synchronous';
  preferred_default_origin: 'top_left' | 'bottom_left' | null;
}

export interface ProfileFieldDiff {
  field: string;
  old: unknown;
  new: unknown;
}

export interface ApplyPresetResult {
  applied: boolean;
  profile: MachineProfile;
  diff: ProfileFieldDiff[];
}

export interface PresetSuggestion {
  suggestion: string | null;
  reason: 'matched' | 'no_match' | 'not_connected' | 'unknown_firmware';
}

// ---------------------------------------------------------------------------
// M3 — Quality Test types (mirrors beambench-core::quality_test)
// ---------------------------------------------------------------------------

export type FocusTestZMode = 'AbsoluteWorkCoord' | 'RelativeTemporary';
export type MaterialTestAxisParam = 'speed' | 'power' | 'interval' | 'passes';

export interface MaterialTestAxis {
  param: MaterialTestAxisParam;
  count: number;
  min: number;
  max: number;
}

export interface MaterialTestSettings {
  x_axis: MaterialTestAxis;
  y_axis: MaterialTestAxis;
  cell_w_mm: number;
  cell_h_mm: number;
  cell_spacing_mm: number;
  sample_entry: CutEntry;
  text_entry: CutEntry;
  border_entry: CutEntry;
  enable_text: boolean;
  enable_border: boolean;
  absolute_center_enabled: boolean;
  x_center_mm: number;
  y_center_mm: number;
}

export interface MaterialTestRecipe {
  id: string;
  name: string;
  settings: MaterialTestSettings;
}

export interface FocusTestSettings {
  z_min_mm: number;
  z_max_mm: number;
  speed_mm_min: number;
  power_percent: number;
  intervals: number;
  mode: FocusTestZMode;
  line_length_mm: number;
  step_spacing_mm: number;
  perforated_labels: boolean;
}

export interface IntervalTestSettings {
  interval_min_mm: number;
  interval_max_mm: number;
  speed_mm_min: number;
  power_percent: number;
  steps: number;
  cell_w_mm: number;
  cell_h_mm: number;
  cell_spacing_mm: number;
}

export interface QualityTestSettings {
  material: MaterialTestSettings;
  focus: FocusTestSettings;
  interval: IntervalTestSettings;
  material_recipes: MaterialTestRecipe[];
  active_material_recipe_id: string | null;
}

export type QualityTestRequest =
  | ({ kind: 'material'; [k: string]: unknown } & MaterialTestSettings)
  | ({ kind: 'focus'; [k: string]: unknown } & FocusTestSettings)
  | ({ kind: 'interval'; [k: string]: unknown } & IntervalTestSettings);

export type QualityTestWarning =
  | {
      kind: 'bounds_exceeded';
      bbox_w_mm: number;
      bbox_h_mm: number;
      bed_w_mm: number;
      bed_h_mm: number;
    }
  | { kind: 'font_fallback'; requested_family: string };

export type QualityTestError =
  | {
      kind: 'bounds_exceeded';
      bbox_w_mm: number;
      bbox_h_mm: number;
      bed_w_mm: number;
      bed_h_mm: number;
    }
  | { kind: 'z_support_required' }
  | { kind: 'material_height_required' }
  | { kind: 'unsupported_z_backend' }
  | { kind: 'no_active_machine_profile' }
  | { kind: 'job_in_progress' }
  | { kind: 'internal'; message?: string; value?: string; 0?: string };

const DEFAULT_SAMPLE_ENTRY: CutEntry = {
  id: '00000000-0000-0000-0000-000000000001',
  operation: 'fill',
  speed_mm_min: 1000,
  power_percent: 50,
  raster_settings: defaultRasterSettings(),
  vector_settings: null,
  air_assist: false,
  power_min_percent: 0,
  z_offset_mm: 0,
  gcode_prefix: '',
  gcode_suffix: '',
  output_enabled: true,
};

const DEFAULT_TEXT_ENTRY: CutEntry = {
  id: '00000000-0000-0000-0000-000000000002',
  operation: 'line',
  speed_mm_min: 1000,
  power_percent: 25,
  raster_settings: null,
  vector_settings: defaultVectorSettings(),
  air_assist: false,
  power_min_percent: 0,
  z_offset_mm: 0,
  gcode_prefix: '',
  gcode_suffix: '',
  output_enabled: true,
};

const DEFAULT_BORDER_ENTRY: CutEntry = {
  ...DEFAULT_TEXT_ENTRY,
  id: '00000000-0000-0000-0000-000000000003',
  power_percent: 50,
};

export const DEFAULT_MATERIAL_TEST_SETTINGS: MaterialTestSettings = {
  x_axis: { param: 'power', count: 5, min: 10, max: 100 },
  y_axis: { param: 'speed', count: 5, min: 300, max: 3000 },
  cell_w_mm: 10,
  cell_h_mm: 10,
  cell_spacing_mm: 4,
  sample_entry: DEFAULT_SAMPLE_ENTRY,
  text_entry: DEFAULT_TEXT_ENTRY,
  border_entry: DEFAULT_BORDER_ENTRY,
  enable_text: true,
  enable_border: true,
  absolute_center_enabled: false,
  x_center_mm: 0,
  y_center_mm: 0,
};

export const DEFAULT_FOCUS_TEST_SETTINGS: FocusTestSettings = {
  z_min_mm: -2,
  z_max_mm: 2,
  speed_mm_min: 1000,
  power_percent: 50,
  intervals: 9,
  mode: 'AbsoluteWorkCoord',
  line_length_mm: 30,
  step_spacing_mm: 5,
  perforated_labels: false,
};

export const DEFAULT_INTERVAL_TEST_SETTINGS: IntervalTestSettings = {
  interval_min_mm: 0.05,
  interval_max_mm: 0.3,
  speed_mm_min: 1000,
  power_percent: 50,
  steps: 6,
  cell_w_mm: 15,
  cell_h_mm: 15,
  cell_spacing_mm: 4,
};

export const DEFAULT_QUALITY_TEST_SETTINGS: QualityTestSettings = {
  material: DEFAULT_MATERIAL_TEST_SETTINGS,
  focus: DEFAULT_FOCUS_TEST_SETTINGS,
  interval: DEFAULT_INTERVAL_TEST_SETTINGS,
  material_recipes: [],
  active_material_recipe_id: null,
};
