import { describe, it, expect } from 'vitest';
import type { MachineProfile } from '../machine';
import type { AppSettings } from '../commands';
import type { CameraAlignment, CameraCalibration } from '../camera';

describe('types/machine extensions', () => {
  it('current MachineProfile objects are valid', () => {
    const profile: MachineProfile = {
      id: 'p1',
      name: 'Test Laser',
      bed_width_mm: 400,
      bed_height_mm: 400,
      max_speed_mm_min: 6000,
      max_power_percent: 100,
      s_value_max: 1000,
      homing_enabled: true,
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
      use_g0_for_overscan: false,
      scanning_offsets: [],
      enable_scanning_offset: false,
      dot_width_mm: 0,
      enable_dot_width: false,
    };
    expect(profile.id).toBe('p1');
    expect(profile.origin).toBe('top_left');
    expect(profile.swap_xy).toBe(false);
  });

  it('machine profile device policy fields are accepted', () => {
    const profile: MachineProfile = {
      id: 'p2',
      name: 'Offset Laser',
      bed_width_mm: 300,
      bed_height_mm: 200,
      max_speed_mm_min: 5000,
      max_power_percent: 100,
      s_value_max: 1000,
      homing_enabled: true,
      default_baud_rate: 115200,
      firmware_type: 'grbl',
      notes: '',
      origin: 'top_left',
      laser_offset_x: 5.0,
      laser_offset_y: 3.0,
      enable_laser_offset: true,
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
      use_g0_for_overscan: false,
      scanning_offsets: [],
      enable_scanning_offset: false,
      dot_width_mm: 0,
      enable_dot_width: false,
    };
    expect(profile.origin).toBe('top_left');
    expect(profile.enable_laser_offset).toBe(true);
  });

  it('camera calibration and alignment fields preserve their concrete payload shapes', () => {
    const calibration: CameraCalibration = {
      image_width_px: 1920,
      image_height_px: 1080,
      transform: { scale: 1, rotation_deg: 0, translation_x: 1.5, translation_y: -2.5 },
      rmse_px: 0.3,
      quality_score: 0.98,
      solved_at: '2026-04-16T12:00:00Z',
    };
    const alignment: CameraAlignment = {
      transform: { scale: 1, rotation_deg: 0, translation_x: 0.5, translation_y: 0.25 },
      rmse_mm: 0.08,
      quality_score: 0.97,
      solved_at: '2026-04-16T12:30:00Z',
    };
    const profile: MachineProfile = {
      id: 'p3',
      name: 'Camera Laser',
      bed_width_mm: 300,
      bed_height_mm: 200,
      max_speed_mm_min: 5000,
      max_power_percent: 100,
      s_value_max: 1000,
      homing_enabled: true,
      default_baud_rate: 115200,
      firmware_type: 'grbl',
      notes: '',
      selected_camera_id: 'cam-1',
      camera_calibration: calibration,
      camera_alignment: alignment,
      origin: 'top_left',
      laser_offset_x: 0,
      laser_offset_y: 0,
      enable_laser_offset: false,
      swap_xy: false,
      job_checklist: false,
      frame_continuously: false,
      laser_on_when_framing: false,
      tab_pulse_width_ms: 0,
      cnc_machine: false,
      use_constant_power: false,
      emit_s_every_g1: false,
      use_g0_for_overscan: false,
      scanning_offsets: [],
      enable_scanning_offset: false,
      dot_width_mm: 0,
      enable_dot_width: false,
    };

    expect(profile.camera_calibration?.transform.translation_x).toBe(1.5);
    expect(profile.camera_alignment?.rmse_mm).toBe(0.08);
  });
});

describe('types/commands extensions', () => {
  it('existing AppSettings objects still valid', () => {
    const settings: AppSettings = {
      display_unit: 'mm',
      autosave_enabled: true,
      autosave_interval_secs: 300,
      machine_profiles: [],
      active_profile_id: 'profile-1',
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
      export_settings: { last_directory: null, last_format: 'svg', filename_stem: null },
    };
    expect(settings.display_unit).toBe('mm');
    expect(settings.dark_mode).toBe(false);
  });

  it('new optional AppSettings fields accepted', () => {
    const settings: AppSettings = {
      display_unit: 'mm',
      autosave_enabled: true,
      autosave_interval_secs: 300,
      machine_profiles: [],
      active_profile_id: 'profile-1',
      recent_files: [],
      api_enabled: false,
      api_port: 8080,
      api_localhost_only: false,
      ui_theme: 'light',
      dark_mode: true,
      antialiasing: true,
      filled_rendering: false,
      reduce_motion: false,
      show_palette_labels: true,
      cursor_size: 'normal',
      toolbar_icon_size: 'large',
      click_tolerance_px: 5,
      snap_threshold_px: 6,
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
      export_settings: { last_directory: '/tmp', last_format: 'pdf', filename_stem: 'job' },
    };
    expect(settings.dark_mode).toBe(true);
    expect(settings.ui_theme).toBe('light');
    expect(settings.toolbar_icon_size).toBe('large');
    expect(settings.active_profile_id).toBe('profile-1');
  });
});
