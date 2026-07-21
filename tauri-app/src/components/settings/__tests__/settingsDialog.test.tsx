import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent, waitFor } from '@testing-library/react';
import { SettingsDialog } from '../SettingsDialog';
import { useAppStore } from '../../../stores/appStore';
import type { AppSettings } from '../../../types/commands';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const initialAppState = useAppStore.getState();

function makeSettings(overrides: Partial<AppSettings> = {}): AppSettings {
  return {
    display_unit: 'mm',
    speed_time_unit: 'minutes',
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

afterEach(() => {
  cleanup();
  useAppStore.setState(initialAppState, true);
});

describe('SettingsDialog', () => {
  it('waits for real settings instead of rendering editable fallback defaults', async () => {
    useAppStore.setState({ settings: null });

    render(<SettingsDialog onClose={vi.fn()} />);

    expect(screen.getByText('Loading settings...')).toBeDefined();
    expect((screen.getByText('Save') as HTMLButtonElement).disabled).toBe(true);

    useAppStore.setState({
      settings: makeSettings({
        display_unit: 'inches',
        autosave_enabled: false,
        autosave_interval_secs: 240,
        api_enabled: true,
        api_port: 6100,
        api_localhost_only: false,
        dark_mode: true,
        antialiasing: true,
        filled_rendering: true,
        reduce_motion: true,
        show_palette_labels: true,
        cursor_size: 'large',
        toolbar_icon_size: 'small',
        click_tolerance_px: 9,
        snap_threshold_px: 11,
        scroll_zoom: false,
      }),
    });

    await waitFor(() => {
      expect(screen.queryByText('Loading settings...')).toBeNull();
    });

    fireEvent.click(screen.getByText('Display'));
    expect(screen.getByTestId('toggle-dark-mode').getAttribute('aria-checked')).toBe('true');
    expect((screen.getByTestId('select-scroll-behavior') as HTMLSelectElement).value).toBe('pan');
    fireEvent.click(screen.getByText('Units & Grid'));
    expect((screen.getByTestId('input-snap-threshold') as HTMLInputElement).value).toBe('11');
  });

  it('renders display toggle controls', () => {
    useAppStore.setState({
      settings: makeSettings({
        display_unit: 'mm',
        autosave_enabled: false,
        autosave_interval_secs: 120,
        dark_mode: false,
        antialiasing: true,
        filled_rendering: false,
        reduce_motion: false,
      }),
    });

    render(<SettingsDialog onClose={vi.fn()} />);

    fireEvent.click(screen.getByText('Display'));
    expect(screen.getByText('Appearance')).toBeDefined();
    expect(screen.getByLabelText('App appearance')).toBeDefined();
    expect(screen.getByText('Controls menus, toolbars, panels, dialogs, and window chrome. The workspace background is set separately.')).toBeDefined();
    expect(screen.getByText('Dark workspace background')).toBeDefined();
    expect(screen.getByRole('switch', { name: 'Dark workspace background' })).toBeDefined();
    expect(screen.getByText('Antialiasing')).toBeDefined();
    expect(screen.getByText('Filled rendering')).toBeDefined();
    expect(screen.getByText('Reduce motion')).toBeDefined();
  });

  it('saves app appearance separately from the workspace background', async () => {
    useAppStore.setState({
      settings: makeSettings({ ui_theme: 'dark', dark_mode: false }),
    });
    document.documentElement.dataset.theme = 'dark';
    const spy = vi.spyOn(useAppStore.getState(), 'updateSettings').mockResolvedValue(undefined);

    render(<SettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByText('Display'));
    fireEvent.change(screen.getByLabelText('App appearance'), { target: { value: 'light' } });

    expect(document.documentElement.dataset.theme).toBe('dark');
    expect(screen.getByTestId('toggle-dark-mode').getAttribute('aria-checked')).toBe('false');

    fireEvent.click(screen.getByText('Save'));
    await waitFor(() => {
      expect(spy).toHaveBeenCalledWith(expect.objectContaining({
        ui_theme: 'light',
        dark_mode: false,
      }));
    });
    spy.mockRestore();
  });

  it('leaves the active theme unchanged when appearance saving fails', async () => {
    useAppStore.setState({
      settings: makeSettings({ ui_theme: 'dark' }),
    });
    document.documentElement.dataset.theme = 'dark';
    const spy = vi.spyOn(useAppStore.getState(), 'updateSettings').mockRejectedValue(new Error('save failed'));

    render(<SettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByText('Display'));
    fireEvent.change(screen.getByLabelText('App appearance'), { target: { value: 'light' } });
    fireEvent.click(screen.getByText('Save'));

    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('save failed'));
    expect(document.documentElement.dataset.theme).toBe('dark');
    spy.mockRestore();
  });

  it('closes on Escape without saving', () => {
    useAppStore.setState({
      settings: makeSettings(),
    });

    const onClose = vi.fn();
    const updateSpy = vi.spyOn(useAppStore.getState(), 'updateSettings').mockResolvedValue(undefined);
    render(<SettingsDialog onClose={onClose} />);

    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });

    expect(onClose).toHaveBeenCalled();
    expect(updateSpy).not.toHaveBeenCalled();
    updateSpy.mockRestore();
  });

  it('closes on backdrop click', () => {
    useAppStore.setState({
      settings: makeSettings(),
    });

    const onClose = vi.fn();
    render(<SettingsDialog onClose={onClose} />);

    fireEvent.click(screen.getByTestId('settings-dialog-backdrop'));

    expect(onClose).toHaveBeenCalled();
  });

  it('closes on Cancel without saving', () => {
    useAppStore.setState({
      settings: makeSettings(),
    });

    const onClose = vi.fn();
    const updateSpy = vi.spyOn(useAppStore.getState(), 'updateSettings').mockResolvedValue(undefined);
    render(<SettingsDialog onClose={onClose} />);

    fireEvent.click(screen.getByText('Cancel'));

    expect(onClose).toHaveBeenCalled();
    expect(updateSpy).not.toHaveBeenCalled();
    updateSpy.mockRestore();
  });

  it('renders backed preference tabs and omits deferred decorative controls', () => {
    useAppStore.setState({
      settings: makeSettings({
        display_unit: 'mm',
        autosave_enabled: false,
        autosave_interval_secs: 120,
        dark_mode: false,
        antialiasing: true,
        filled_rendering: false,
        reduce_motion: false,
        show_palette_labels: true,
        cursor_size: 'normal',
        toolbar_icon_size: 'normal',
        click_tolerance_px: 5,
        scroll_zoom: true,
      }),
    });

    render(<SettingsDialog onClose={vi.fn()} />);

    fireEvent.click(screen.getByText('Display'));
    expect(screen.queryByText('Show Palette Labels')).toBeNull();
    expect(screen.queryByText('Cursor / Nodes Size')).toBeNull();
    expect(screen.queryByText('Toolbar Icon Size')).toBeNull();
    expect(screen.getByText('Scroll wheel')).toBeDefined();
    expect(screen.getByTestId('select-scroll-behavior')).toBeDefined();

    fireEvent.click(screen.getByText('Units & Grid'));
    expect(screen.getByText('Speed Time Unit')).toBeDefined();
    expect(screen.getByText('Click tolerance (px)')).toBeDefined();
    expect(screen.getByTestId('input-grid-spacing')).toBeDefined();

    fireEvent.click(screen.getByText('File & Import'));
    expect(screen.getByText('Allow importing to tool layers')).toBeDefined();
    expect(screen.getByText('Include tool layers in job bounds')).toBeDefined();

    expect(screen.queryByText('Hotkeys')).toBeNull();
    expect(screen.queryByText('Edit Hotkeys')).toBeNull();
  });

  it('saves backed file and import fields', async () => {
    useAppStore.setState({
      settings: makeSettings({
        display_unit: 'mm',
        autosave_enabled: false,
        autosave_interval_secs: 120,
        dark_mode: false,
        antialiasing: true,
        filled_rendering: false,
        reduce_motion: false,
        show_palette_labels: true,
        cursor_size: 'normal',
        toolbar_icon_size: 'normal',
        click_tolerance_px: 5,
        scroll_zoom: true,
      }),
    });

    const spy = vi.spyOn(useAppStore.getState(), 'updateSettings').mockResolvedValue(undefined);
    render(<SettingsDialog onClose={vi.fn()} />);

    fireEvent.click(screen.getByText('File & Import'));
    fireEvent.click(screen.getByTestId('toggle-allow-import-tool-layers'));
    fireEvent.click(screen.getByText('Save'));

    await waitFor(() => {
      expect(spy).toHaveBeenCalledWith(
        expect.objectContaining({ allow_importing_to_tool_layers: true }),
      );
    });
    spy.mockRestore();
  });

  it('renders snap threshold input', () => {
    useAppStore.setState({
      settings: makeSettings({ autosave_enabled: false, snap_threshold_px: 8 }),
    });

    render(<SettingsDialog onClose={vi.fn()} />);

    fireEvent.click(screen.getByText('Units & Grid'));
    expect(screen.getByText('Object snap distance (px)')).toBeDefined();
    const input = screen.getByTestId('input-snap-threshold') as HTMLInputElement;
    expect(input.value).toBe('8');
  });

  it('save carries snap_threshold_px value', async () => {
    useAppStore.setState({
      settings: makeSettings({ autosave_enabled: false, snap_threshold_px: 8 }),
    });

    const spy = vi.spyOn(useAppStore.getState(), 'updateSettings').mockResolvedValue(undefined);
    render(<SettingsDialog onClose={vi.fn()} />);

    // Change snap threshold to 12
    fireEvent.click(screen.getByText('Units & Grid'));
    const input = screen.getByTestId('input-snap-threshold') as HTMLInputElement;
    fireEvent.change(input, { target: { value: '12' } });

    fireEvent.click(screen.getByText('Save'));

    await waitFor(() => {
      expect(spy).toHaveBeenCalledWith(
        expect.objectContaining({ snap_threshold_px: 12 }),
      );
    });
    spy.mockRestore();
  });

  it('saves speed time unit changes', async () => {
    useAppStore.setState({
      settings: makeSettings({ speed_time_unit: 'minutes' }),
    });

    const spy = vi.spyOn(useAppStore.getState(), 'updateSettings').mockResolvedValue(undefined);
    render(<SettingsDialog onClose={vi.fn()} />);

    fireEvent.click(screen.getByText('Units & Grid'));
    fireEvent.click(screen.getByText('sec'));
    fireEvent.click(screen.getByText('Save'));

    await waitFor(() => {
      expect(spy).toHaveBeenCalledWith(
        expect.objectContaining({ speed_time_unit: 'seconds' }),
      );
    });
    spy.mockRestore();
  });

  it('renders network access on by default and saves localhost-only when turned off', async () => {
    useAppStore.setState({
      settings: makeSettings({ api_enabled: true }),
    });

    const spy = vi.spyOn(useAppStore.getState(), 'updateSettings').mockResolvedValue(undefined);
    render(<SettingsDialog onClose={vi.fn()} />);

    expect(screen.getByText('Allow network devices to connect')).toBeDefined();
    expect(screen.getByText(/Off keeps the local API available only on this computer/)).toBeDefined();
    expect(screen.getByTestId('toggle-network-api-access').getAttribute('aria-checked')).toBe('true');

    fireEvent.click(screen.getByTestId('toggle-network-api-access'));
    fireEvent.click(screen.getByText('Save'));

    await waitFor(() => {
      expect(spy).toHaveBeenCalledWith(
        expect.objectContaining({ api_localhost_only: true }),
      );
    });
    spy.mockRestore();
  });

  it('renders backend defaults from the concrete settings payload', async () => {
    useAppStore.setState({
      settings: makeSettings(),
    });

    const spy = vi.spyOn(useAppStore.getState(), 'updateSettings').mockResolvedValue(undefined);
    render(<SettingsDialog onClose={vi.fn()} />);

    // Verify rendered values match backend defaults (settings.rs)
    fireEvent.click(screen.getByText('Display'));
    // dark_mode default: false
    const darkToggle = screen.getByTestId('toggle-dark-mode');
    expect(darkToggle.getAttribute('aria-checked')).toBe('false');
    // antialiasing default: false (was incorrectly true in old UI fallback)
    const aaToggle = screen.getByTestId('toggle-antialiasing');
    expect(aaToggle.getAttribute('aria-checked')).toBe('false');
    // filled_rendering default: false
    const frToggle = screen.getByTestId('toggle-filled-rendering');
    expect(frToggle.getAttribute('aria-checked')).toBe('false');
    // reduce_motion default: false
    const rmToggle = screen.getByTestId('toggle-reduce-motion');
    expect(rmToggle.getAttribute('aria-checked')).toBe('false');
    // Deferred controls are not rendered until backed by real effect sites.
    expect(screen.queryByTestId('toggle-palette-labels')).toBeNull();
    expect(screen.queryByTestId('select-cursor-size')).toBeNull();
    expect(screen.queryByTestId('select-toolbar-icon-size')).toBeNull();
    const scrollSelect = screen.getByTestId('select-scroll-behavior') as HTMLSelectElement;
    expect(scrollSelect.value).toBe('zoom');
    fireEvent.click(screen.getByText('Units & Grid'));
    // click_tolerance_px default: 5.0
    const clickInput = screen.getByTestId('input-click-tolerance') as HTMLInputElement;
    expect(clickInput.value).toBe('5');
    // snap_threshold_px default: 5.0
    const snapInput = screen.getByTestId('input-snap-threshold') as HTMLInputElement;
    expect(snapInput.value).toBe('5');

    // Save without changing anything — should echo backend defaults, not old UI fallbacks
    fireEvent.click(screen.getByText('Save'));

    await waitFor(() => {
      expect(spy).toHaveBeenCalledWith(
        expect.objectContaining({
          autosave_enabled: true,
          dark_mode: false,
          antialiasing: false,
          filled_rendering: false,
          reduce_motion: false,
          click_tolerance_px: 5,
          snap_threshold_px: 5,
          grid_spacing_mm: 10,
          nudge_step_mm: 5,
          nudge_step_fine_mm: 1,
          nudge_step_coarse_mm: 20,
          scroll_zoom: true,
        }),
      );
    });
    spy.mockRestore();
  });

  it('toggle calls updateSettings on save', async () => {
    useAppStore.setState({
      settings: makeSettings({
        autosave_enabled: false,
        dark_mode: false,
        antialiasing: true,
        filled_rendering: false,
        reduce_motion: false,
      }),
    });

    const spy = vi.spyOn(useAppStore.getState(), 'updateSettings').mockResolvedValue(undefined);
    const onClose = vi.fn();
    render(<SettingsDialog onClose={onClose} />);

    // Toggle dark mode
    fireEvent.click(screen.getByText('Display'));
    fireEvent.click(screen.getByTestId('toggle-dark-mode'));

    // Click Save
    fireEvent.click(screen.getByText('Save'));

    await waitFor(() => {
      expect(spy).toHaveBeenCalledWith(
        expect.objectContaining({ dark_mode: true }),
      );
    });
    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
    spy.mockRestore();
  });

  it('keeps the dialog open and shows the error when save fails', async () => {
    useAppStore.setState({
      settings: makeSettings({ autosave_enabled: false }),
    });

    const onClose = vi.fn();
    const spy = vi.spyOn(useAppStore.getState(), 'updateSettings').mockRejectedValue(new Error('save failed'));

    render(<SettingsDialog onClose={onClose} />);
    fireEvent.click(screen.getByText('Save'));

    await waitFor(() => {
      expect(screen.getByRole('alert').textContent).toContain('save failed');
    });
    expect(onClose).not.toHaveBeenCalled();
    spy.mockRestore();
  });

  it('resyncs untouched fields when store settings change underneath the dialog', async () => {
    useAppStore.setState({
      settings: makeSettings({ autosave_enabled: false }),
    });

    const spy = vi.spyOn(useAppStore.getState(), 'updateSettings').mockResolvedValue(undefined);
    render(<SettingsDialog onClose={vi.fn()} />);

    fireEvent.click(screen.getByText('Units & Grid'));
    useAppStore.setState({
      settings: makeSettings({ display_unit: 'inches', autosave_enabled: false }),
    });

    await waitFor(() => {
      expect(screen.getByText('inches').className).toContain('bg-bb-accent');
    });

    fireEvent.click(screen.getByText('Save'));

    await waitFor(() => {
      expect(spy).toHaveBeenCalledWith(
        expect.objectContaining({ display_unit: 'inches' }),
      );
    });
    spy.mockRestore();
  });
});
