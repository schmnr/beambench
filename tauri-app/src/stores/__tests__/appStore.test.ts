import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useAppStore } from '../appStore';
import { useUiStore } from '../uiStore';
import { appService } from '../../services/appService';
import type { AppSettings } from '../../types/commands';
import { uiThemeController } from '../../theme';

vi.mock('../../services/appService', () => ({
  appService: {
    getStatus: vi.fn(),
    getSettings: vi.fn(),
    updateSettings: vi.fn(),
    updateDisplaySettings: vi.fn(),
  },
}));

const initialState = useAppStore.getState();
const initialUiState = useUiStore.getState();

function makeSettings(overrides: Partial<AppSettings> = {}): AppSettings {
  return {
    display_unit: 'mm',
    autosave_enabled: true,
    autosave_interval_secs: 120,
    machine_profiles: [],
    active_profile_id: null,
    recent_files: [],
    api_enabled: true,
    api_port: 5900,
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
    ...overrides,
  };
}

describe('appStore.updateSettings', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    uiThemeController.sync('dark');
    useAppStore.setState(initialState, true);
    useUiStore.setState(initialUiState, true);
  });

  it('sends mixed basic and display settings through one backend mutation', async () => {
    vi.mocked(appService.updateSettings).mockResolvedValue(makeSettings({
      display_unit: 'inches',
      autosave_enabled: false,
      autosave_interval_secs: 240,
      api_enabled: true,
      api_port: 6100,
      api_localhost_only: false,
      dark_mode: true,
      antialiasing: true,
      filled_rendering: true,
      reduce_motion: false,
      show_palette_labels: true,
      cursor_size: 'large',
      toolbar_icon_size: 'small',
      click_tolerance_px: 9,
      snap_threshold_px: 12,
      scroll_zoom: false,
    }));

    await useAppStore.getState().updateSettings({
      display_unit: 'inches',
      autosave_enabled: false,
      api_enabled: true,
      dark_mode: true,
      cursor_size: 'large',
      snap_threshold_px: 12,
    });

    expect(appService.updateSettings).toHaveBeenCalledTimes(1);
    expect(appService.updateSettings).toHaveBeenCalledWith({
      display_unit: 'inches',
      autosave_enabled: false,
      api_enabled: true,
      dark_mode: true,
      cursor_size: 'large',
      snap_threshold_px: 12,
    });
    expect(appService.updateDisplaySettings).not.toHaveBeenCalled();
  });

  it('rethrows failed saves after storing the error', async () => {
    vi.mocked(appService.updateSettings).mockRejectedValue(new Error('boom'));

    await expect(
      useAppStore.getState().updateSettings({ dark_mode: true }),
    ).rejects.toThrow('boom');

    expect(useAppStore.getState().error).toContain('boom');
  });

  it('hydrates persisted grid, nudge, and render settings into UI state', async () => {
    vi.mocked(appService.updateSettings).mockResolvedValue(makeSettings({
      antialiasing: true,
      filled_rendering: true,
      grid_spacing_mm: 2.5,
      nudge_step_mm: 0.5,
      nudge_step_fine_mm: 0.05,
      nudge_step_coarse_mm: 5,
    }));

    await useAppStore.getState().updateSettings({ grid_spacing_mm: 2.5 });
    await new Promise((resolve) => setTimeout(resolve, 0));

    const ui = useUiStore.getState();
    expect(ui.gridSpacingMm).toBe(2.5);
    expect(ui.nudgeStepMm).toBe(0.5);
    expect(ui.nudgeStepFineMm).toBe(0.05);
    expect(ui.nudgeStepCoarseMm).toBe(5);
    expect(ui.viewStyle).toBe('filled_smooth');
  });

  it('reconciles the authoritative theme after asynchronous settings hydration', async () => {
    document.documentElement.dataset.theme = 'dark';
    vi.mocked(appService.getSettings).mockResolvedValue(makeSettings({ ui_theme: 'light' }));

    await useAppStore.getState().fetchSettings();

    expect(document.documentElement.dataset.theme).toBe('light');
    expect(document.documentElement.style.colorScheme).toBe('light');
  });

  it('does not apply a draft theme when the settings save fails', async () => {
    uiThemeController.sync('dark');
    vi.mocked(appService.updateSettings).mockRejectedValue(new Error('save failed'));

    await expect(useAppStore.getState().updateSettings({ ui_theme: 'light' })).rejects.toThrow('save failed');

    expect(document.documentElement.dataset.theme).toBe('dark');
  });

  it('keeps app appearance independent from the workspace background setting', async () => {
    vi.mocked(appService.updateSettings).mockResolvedValue(makeSettings({
      ui_theme: 'light',
      dark_mode: true,
    }));

    await useAppStore.getState().updateSettings({ dark_mode: true });

    expect(document.documentElement.dataset.theme).toBe('light');
    expect(useAppStore.getState().settings?.dark_mode).toBe(true);
  });
});
