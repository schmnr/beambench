import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useAppStore } from './appStore';
import { useNotificationStore } from './notificationStore';
import { useUpdateStore } from './updateStore';
import type { AppSettings } from '../types/commands';
import { checkForUpdate } from '../services/updateService';

vi.mock('../services/updateService', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../services/updateService')>();
  return {
    ...actual,
    checkForUpdate: vi.fn(),
    downloadAndInstallUpdate: vi.fn(),
    getUpdateInstallBlocker: vi.fn(() => null),
    getUpdateEnvironmentBlocker: vi.fn(async () => null),
  };
});

function makeSettings(overrides: Partial<AppSettings> = {}): AppSettings {
  return {
    display_unit: 'mm',
    autosave_enabled: true,
    autosave_interval_secs: 120,
    machine_profiles: [],
    active_profile_id: null,
    recent_files: [],
    api_enabled: false,
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
    check_for_updates_on_startup: true,
    update_snoozed_until: '',
    skipped_update_version: '',
    ...overrides,
  };
}

const updateInfo = {
  currentVersion: '1.0.0',
  version: '1.0.1',
  date: '2026-05-12T00:00:00Z',
  body: 'Bug fixes',
  rawJson: {},
};

const initialAppState = useAppStore.getState();
const initialNotificationState = useNotificationStore.getState();
const initialUpdateState = useUpdateStore.getState();

describe('updateStore', () => {
  beforeEach(async () => {
    const { getUpdateEnvironmentBlocker, downloadAndInstallUpdate } = await import(
      '../services/updateService'
    );
    vi.mocked(checkForUpdate).mockReset();
    vi.mocked(downloadAndInstallUpdate).mockReset();
    vi.mocked(getUpdateEnvironmentBlocker).mockReset();
    vi.mocked(getUpdateEnvironmentBlocker).mockResolvedValue(null);
    useAppStore.setState({
      ...initialAppState,
      settings: makeSettings(),
      updateSettings: vi.fn(async (updates) => {
        useAppStore.setState((state) => ({
          settings: {
            ...(state.settings ?? makeSettings()),
            ...updates,
          },
        }));
      }),
    }, true);
    useNotificationStore.setState(initialNotificationState, true);
    useUpdateStore.setState(initialUpdateState, true);
  });

  afterEach(() => {
    useAppStore.setState(initialAppState, true);
    useNotificationStore.setState(initialNotificationState, true);
    useUpdateStore.setState(initialUpdateState, true);
  });

  it('does not check on startup when the preference is disabled', async () => {
    useAppStore.setState({ settings: makeSettings({ check_for_updates_on_startup: false }) });

    const result = await useUpdateStore.getState().runStartupCheck();

    expect(result).toBeNull();
    expect(checkForUpdate).not.toHaveBeenCalled();
  });

  it('suppresses startup prompts for a skipped version', async () => {
    vi.mocked(checkForUpdate).mockResolvedValue(updateInfo);
    useAppStore.setState({ settings: makeSettings({ skipped_update_version: '1.0.1' }) });

    const result = await useUpdateStore.getState().runStartupCheck();

    expect(result).toEqual(updateInfo);
    expect(useUpdateStore.getState().dialogOpen).toBe(false);
    expect(useNotificationStore.getState().notifications).toHaveLength(0);
  });

  it('manual checks ignore skipped versions and open the update dialog', async () => {
    vi.mocked(checkForUpdate).mockResolvedValue(updateInfo);
    useAppStore.setState({ settings: makeSettings({ skipped_update_version: '1.0.1' }) });

    const result = await useUpdateStore.getState().checkForUpdates('manual');

    expect(result).toEqual(updateInfo);
    expect(useUpdateStore.getState().dialogOpen).toBe(true);
    expect(useUpdateStore.getState().status).toBe('available');
  });

  it('does not start a second update check while one is already running', async () => {
    useUpdateStore.setState({ status: 'checking' });

    const result = await useUpdateStore.getState().checkForUpdates('manual');

    expect(result).toBeNull();
    expect(checkForUpdate).not.toHaveBeenCalled();
    expect(useUpdateStore.getState().status).toBe('checking');
  });

  it('snoozes an available update for 24 hours and clears skipped version', async () => {
    vi.mocked(checkForUpdate).mockResolvedValue(updateInfo);
    await useUpdateStore.getState().checkForUpdates('manual');

    await useUpdateStore.getState().snoozeAvailableUpdate();

    const settings = useAppStore.getState().settings;
    expect(settings?.skipped_update_version).toBe('');
    expect(Date.parse(settings?.update_snoozed_until ?? '')).toBeGreaterThan(Date.now());
    expect(useUpdateStore.getState().dialogOpen).toBe(false);
  });

  it('blocks install with a remedy when the app cannot be swapped in place', async () => {
    const { getUpdateEnvironmentBlocker, downloadAndInstallUpdate } = await import(
      '../services/updateService'
    );
    vi.mocked(getUpdateEnvironmentBlocker).mockResolvedValue('not_in_applications');

    await useUpdateStore.getState().installAvailableUpdate();

    expect(useUpdateStore.getState().status).toBe('blocked');
    expect(useUpdateStore.getState().error).toBeTruthy();
    expect(downloadAndInstallUpdate).not.toHaveBeenCalled();
  });

  it('maps a cross-device install failure to the blocked remedy', async () => {
    const { getUpdateEnvironmentBlocker, downloadAndInstallUpdate } = await import(
      '../services/updateService'
    );
    vi.mocked(getUpdateEnvironmentBlocker).mockResolvedValue(null);
    vi.mocked(downloadAndInstallUpdate).mockRejectedValue(
      new Error('Cross-device link (os error 18)'),
    );

    await useUpdateStore.getState().installAvailableUpdate();

    const state = useUpdateStore.getState();
    expect(state.status).toBe('blocked');
    expect(state.error).not.toMatch(/os error 18/i);
  });
});
