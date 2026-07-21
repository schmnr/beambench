import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useAutosave } from '../useAutosave';
import { useAppStore } from '../../stores/appStore';
import { useProjectStore } from '../../stores/projectStore';
import type { AppSettings } from '../../types/commands';
import { makeProject } from '../../test-utils/projectFixtures';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const mockAutosave = vi.fn().mockResolvedValue(undefined);
vi.mock('../../services/persistenceService', () => ({
  persistenceService: { autosave: () => mockAutosave() },
}));

const initialAppState = useAppStore.getState();
const initialProjectState = useProjectStore.getState();

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
    ...overrides,
  };
}

beforeEach(() => {
  vi.useFakeTimers();
  mockAutosave.mockClear();
});

afterEach(() => {
  vi.useRealTimers();
  useAppStore.setState(initialAppState, true);
  useProjectStore.setState(initialProjectState, true);
});

describe('useAutosave fallback defaults', () => {
  it('defaults to enabled=true and interval=120s when settings are null (matches AppSettings::default())', async () => {
    // Settings null — simulates pre-load or failed fetch
    useAppStore.setState({ settings: null });
    // the autosave hook only reads `project.dirty` + presence — the
    // full Project schema isn't exercised here. Use makeProject to build a
    // schema-valid Project and toggle dirty without a partial cast.
    useProjectStore.setState({ project: { ...makeProject(), dirty: true } });

    renderHook(() => useAutosave());

    // With enabled=true and dirty=true, a timer should be active.
    // Advance 120s (the backend default interval) and verify autosave is called.
    await vi.advanceTimersByTimeAsync(120_000);
    expect(mockAutosave).toHaveBeenCalledTimes(1);

    // Should NOT have been called at 60s (the old wrong default)
    mockAutosave.mockClear();
    await vi.advanceTimersByTimeAsync(60_000);
    expect(mockAutosave).toHaveBeenCalledTimes(0);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(mockAutosave).toHaveBeenCalledTimes(1);
  });

  it('does not start timer when settings are null and project is not dirty', () => {
    useAppStore.setState({ settings: null });
    useProjectStore.setState({ project: null });

    renderHook(() => useAutosave());

    vi.advanceTimersByTime(300_000);
    expect(mockAutosave).not.toHaveBeenCalled();
  });

  it('uses settings values when present', async () => {
    useAppStore.setState({
      settings: makeSettings({
        autosave_enabled: true,
        autosave_interval_secs: 60,
      }),
    });
    // the autosave hook only reads `project.dirty` + presence — the
    // full Project schema isn't exercised here. Use makeProject to build a
    // schema-valid Project and toggle dirty without a partial cast.
    useProjectStore.setState({ project: { ...makeProject(), dirty: true } });

    renderHook(() => useAutosave());

    await vi.advanceTimersByTimeAsync(60_000);
    expect(mockAutosave).toHaveBeenCalledTimes(1);
  });

  it('does not start timer when autosave_enabled is false', () => {
    useAppStore.setState({
      settings: makeSettings({
        autosave_enabled: false,
        autosave_interval_secs: 120,
      }),
    });
    // the autosave hook only reads `project.dirty` + presence — the
    // full Project schema isn't exercised here. Use makeProject to build a
    // schema-valid Project and toggle dirty without a partial cast.
    useProjectStore.setState({ project: { ...makeProject(), dirty: true } });

    renderHook(() => useAutosave());

    vi.advanceTimersByTime(300_000);
    expect(mockAutosave).not.toHaveBeenCalled();
  });

  it('does not start a second autosave while the previous one is still running', async () => {
    let finishAutosave!: () => void;
    mockAutosave.mockImplementationOnce(() => new Promise<void>((resolve) => {
      finishAutosave = () => resolve();
    }));

    useAppStore.setState({
      settings: makeSettings({
        autosave_enabled: true,
        autosave_interval_secs: 60,
      }),
    });
    // the autosave hook only reads `project.dirty` + presence — the
    // full Project schema isn't exercised here. Use makeProject to build a
    // schema-valid Project and toggle dirty without a partial cast.
    useProjectStore.setState({ project: { ...makeProject(), dirty: true } });

    renderHook(() => useAutosave());

    await vi.advanceTimersByTimeAsync(60_000);
    expect(mockAutosave).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(60_000);
    expect(mockAutosave).toHaveBeenCalledTimes(1);

    finishAutosave();
    await Promise.resolve();

    await vi.advanceTimersByTimeAsync(60_000);
    expect(mockAutosave).toHaveBeenCalledTimes(2);
  });
});
