import type {
  ObjectData,
  Layer,
  OperationType,
  Project,
  ProjectObject,
  RasterSettings,
  TransformLocks,
  VectorSettings,
  Workspace,
} from '../types/project';
import { DEFAULT_PROJECT_OPTIMIZATION } from '../types/project';
import type { AppSettings } from '../types/commands';

export function makeTransformLocks(overrides: Partial<TransformLocks> = {}): TransformLocks {
  return {
    move_enabled: true,
    size_enabled: true,
    rotate_enabled: true,
    shear_enabled: true,
    ...overrides,
  };
}

export function makeTextObjectData(overrides: Partial<Extract<ObjectData, { type: 'text' }>> = {}): Extract<ObjectData, { type: 'text' }> {
  return {
    type: 'text',
    content: 'Hello',
    font_family: 'sans-serif',
    font_size_mm: 10,
    alignment: 'left',
    alignment_v: 'top',
    bold: false,
    italic: false,
    upper_case: false,
    welded: false,
    h_spacing: 0,
    v_spacing: 0,
    on_path: false,
    path_offset: 0,
    distort: false,
    layout_mode: 'straight',
    rtl: false,
    bend_radius: 0,
    transform_style: 'none',
    transform_curve: 0,
    circle_placement: 'top_outside',
    max_width: null,
    squeeze: false,
    ignore_empty_vars: false,
    missing_font: false,
    missing_glyphs: [],
    ...overrides,
  };
}

export function makeStarObjectData(overrides: Partial<Extract<ObjectData, { type: 'star' }>> = {}): Extract<ObjectData, { type: 'star' }> {
  return {
    type: 'star',
    points: 5,
    bulge: 0,
    ratio: 0.5,
    dual_radius: false,
    ratio2: null,
    corner_radius: 0,
    corner_radii: [],
    ...overrides,
  };
}

export function makeWorkspace(overrides: Partial<Workspace> = {}): Workspace {
  return {
    bed_width_mm: 400,
    bed_height_mm: 400,
    origin: 'top_left',
    ...overrides,
  };
}

export function makeRasterSettings(overrides: Partial<RasterSettings> = {}): RasterSettings {
  return {
    dpi: 254,
    mode: 'floyd_steinberg',
    scan_angle: 0,
    bidirectional: true,
    overscan_mm: 2.5,
    passes: 1,
    line_interval_mm: 0.1,
    crosshatch: false,
    flood_fill: false,
    angle_passes: 1,
    angle_increment_deg: 90,
    pass_through: false,
    halftone_cells_per_inch: 10,
    halftone_angle_deg: 0,
    newsprint_angle_deg: 45,
    newsprint_frequency: 10,
    invert: false,
    dot_width_correction_mm: 0,
    ramp_length_mm: 0,
    ...overrides,
  };
}

export function makeVectorSettings(overrides: Partial<VectorSettings> = {}): VectorSettings {
  return {
    passes: 1,
    perforation_enabled: false,
    perforation_on_ms: 10,
    perforation_off_ms: 10,
    kerf_offset_mm: 0,
    tab_count: 0,
    tab_width_mm: 3,
    offset_overlap_mm: 0,
    offset_outward: false,
    offset_fill_grouping_mode: 'all_shapes_at_once',
    ...overrides,
  };
}

export type LayerFixtureOverrides = Partial<Layer> & {
  operation?: OperationType;
  speed_mm_min?: number;
  power_percent?: number;
  raster_settings?: RasterSettings | null;
  vector_settings?: VectorSettings | null;
  air_assist?: boolean;
  power_min_percent?: number;
  z_offset_mm?: number;
  gcode_prefix?: string;
  gcode_suffix?: string;
};

export function makeLayer(overrides: LayerFixtureOverrides = {}): Layer {
  const isToolLayer = overrides.is_tool_layer ?? overrides.operation === 'tool';
  const primaryEntry = {
    id: 'entry-1',
    operation: overrides.operation ?? (isToolLayer ? 'tool' : 'line'),
    speed_mm_min: overrides.speed_mm_min ?? (isToolLayer ? 0 : 1000),
    power_percent: overrides.power_percent ?? (isToolLayer ? 0 : 50),
    raster_settings: overrides.raster_settings ?? null,
    vector_settings: overrides.vector_settings ?? null,
    air_assist: overrides.air_assist ?? false,
    power_min_percent: overrides.power_min_percent ?? 0,
    z_offset_mm: overrides.z_offset_mm ?? 0,
    gcode_prefix: overrides.gcode_prefix ?? '',
    gcode_suffix: overrides.gcode_suffix ?? '',
    output_enabled: !isToolLayer,
  };
  return {
    id: 'layer-1',
    name: 'Layer 1',
    entries: overrides.entries ?? [primaryEntry],
    enabled: true,
    order_index: 0,
    color_tag: '#000000',
    visible: true,
    is_tool_layer: isToolLayer,
    ...overrides,
  };
}

export function makeProjectObject(overrides: Partial<ProjectObject> = {}): ProjectObject {
  return {
    id: 'obj-1',
    name: 'Object 1',
    visible: true,
    locked: false,
    transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
    bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
    layer_id: 'layer-1',
    z_index: 0,
    data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 },
    lock_aspect_ratio: false,
    power_scale: 1,
    priority: 0,
    created_at: '2026-01-01T00:00:00Z',
    ...overrides,
  };
}

export function makeProject(overrides: Partial<Project> = {}): Project {
  return {
    metadata: {
      format_version: '1',
      app_version: '0.1.0',
      project_id: 'project-1',
      project_name: 'Test Project',
      created_at: '',
      modified_at: '',
    },
    workspace: makeWorkspace(),
    layers: [makeLayer()],
    objects: [makeProjectObject()],
    assets: [],
    machine_profile_id: null,
    machine_profile_snapshot: null,
    notes: '',
    start_from: 'absolute_coords',
    job_origin: 'top_left',
    user_origin: null,
    transform_locks: makeTransformLocks(),
    ...overrides,
    optimization: overrides.optimization ?? DEFAULT_PROJECT_OPTIMIZATION,
  };
}

export function makeMachineStatus(overrides: Partial<import('../types/machine').MachineStatus> = {}): import('../types/machine').MachineStatus {
  return {
    run_state: 'idle',
    machine_position: { x: 0, y: 0, z: 0 },
    work_position: { x: 0, y: 0, z: 0 },
    feed_rate: 0,
    spindle_speed: 0,
    feed_override: 100,
    spindle_override: 100,
    rapid_override: 100,
    pin_states: '',
    ...overrides,
  };
}

export function makeJobProgress(overrides: Partial<import('../types/machine').JobProgress> = {}): import('../types/machine').JobProgress {
  return {
    state: 'idle',
    total_lines: 0,
    queued_lines: 0,
    sent_lines: 0,
    acknowledged_lines: 0,
    elapsed_secs: 0,
    estimated_remaining_secs: 0,
    buffer_fill_bytes: 0,
    error_message: null,
    buckets: [],
    ...overrides,
  };
}

export function makeMachineProfile(overrides: Partial<import('../types/machine').MachineProfile> = {}): import('../types/machine').MachineProfile {
  return {
    id: 'profile-1',
    name: 'Test Profile',
    bed_width_mm: 400,
    bed_height_mm: 400,
    max_speed_mm_min: 6000,
    max_power_percent: 100,
    s_value_max: 1000,
    homing_enabled: false,
    default_baud_rate: 115200,
    firmware_type: 'grbl',
    notes: '',
    origin: 'top_left',
    laser_offset_x: 0,
    laser_offset_y: 0,
    enable_laser_offset: false,
    swap_xy: false,
    selected_camera_id: null,
    camera_calibration: null,
    camera_alignment: null,
    job_checklist: false,
    frame_continuously: false,
    laser_on_when_framing: false,
    tab_pulse_width_ms: 0,
    cnc_machine: false,
    use_constant_power: false,
    emit_s_every_g1: false,
    use_g0_for_overscan: true,
    scanning_offsets: [],
    enable_scanning_offset: false,
    dot_width_mm: 0,
    enable_dot_width: false,
    ...overrides,
  };
}

export function makePortInfo(overrides: Partial<import('../types/machine').PortInfo> = {}): import('../types/machine').PortInfo {
  return {
    port_name: '/dev/ttyUSB0',
    description: '',
    manufacturer: '',
    vid: null,
    pid: null,
    ...overrides,
  };
}

// Shared AppSettings fixture — per-test helpers duplicated this; centralized
// here so test files can import instead of redefining.
export function makeAppSettings(overrides: Partial<AppSettings> = {}): AppSettings {
  return {
    display_unit: 'mm',
    speed_time_unit: 'minutes',
    autosave_enabled: true,
    autosave_interval_secs: 300,
    machine_profiles: [],
    active_profile_id: null,
    recent_files: [],
    api_enabled: false,
    api_port: 8080,
    api_localhost_only: false,
    ui_theme: 'dark',
    dark_mode: false,
    antialiasing: false,
    filled_rendering: false,
    reduce_motion: false,
    show_palette_labels: false,
    cursor_size: 'normal',
    toolbar_icon_size: 'normal',
    click_tolerance_px: 5,
    snap_threshold_px: 5,
    grid_spacing_mm: 10,
    nudge_step_mm: 5,
    nudge_step_fine_mm: 1,
    nudge_step_coarse_mm: 20,
    scroll_zoom: true,
    debug_log_enabled: false,
    panel_layout: null,
    saved_positions: [],
    last_radius_mm: 5,
    image_presets: [],
    custom_hotkeys: {},
    export_settings: {
      last_directory: null,
      last_format: 'svg',
      filename_stem: null,
    },
    allow_importing_to_tool_layers: false,
    include_tool_layers_in_job_bounds: true,
    ...overrides,
  };
}
