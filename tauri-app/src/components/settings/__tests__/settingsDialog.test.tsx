import { describe, it, expect, vi, afterEach } from 'vitest';
import { act, render, screen, cleanup, fireEvent, waitFor, within } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { SettingsDialog } from '../SettingsDialog';
import { useAppStore } from '../../../stores/appStore';
import type { AppSettings } from '../../../types/commands';
import { APP_COMMANDS } from '../../../commands/appCommandIds';
import { LibraryDrawer } from '../../layout/LibraryDrawer';
import { useConnectedLibraryStore as cg, usePrintcutcarveLibraryStore as pcc } from '../../../stores/connectedLibraryStore';
import { useUiStore } from '../../../stores/uiStore';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const initialAppState = useAppStore.getState();
const initialCgState = cg.getState();
const initialPccState = pcc.getState();
const initialUiState = useUiStore.getState();

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
    antialiasing: true,
    artwork_display_mode: 'by_layer',
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
  cg.setState(initialCgState, true);
  pcc.setState(initialPccState, true);
  useUiStore.setState(initialUiState, true);
});

describe('SettingsDialog', () => {
  it('shares remember choice and pending login cancellation with the Library', async () => {
    const disconnected = { account: null, remembered: false, walletUnavailable: false };
    cg.setState({ status: disconnected });
    pcc.setState({ status: disconnected });
    useAppStore.setState({ settings: makeSettings() });
    useUiStore.setState({ libraryDrawerOpen: true, libraryDrawerTab: 'craftgineer' });
    let finish!: (value: unknown) => void;
    vi.mocked(invoke).mockImplementation((command) => command === 'connected_library_connect'
      ? new Promise(resolve => { finish = resolve }) : Promise.resolve(null));
    render(<><LibraryDrawer /><SettingsDialog onClose={vi.fn()} /></>);
    const preferences = within(screen.getByTestId('settings-dialog'));
    fireEvent.click(preferences.getByRole('tab', { name: 'Connected accounts' }));
    const library = within(screen.getByTestId('library-drawer'));
    const settingsAccount = within(preferences.getByRole('region', { name: 'Craftgineer connection' }));
    fireEvent.click(library.getByRole('checkbox'));
    expect((settingsAccount.getByRole('checkbox') as HTMLInputElement).checked).toBe(false);
    fireEvent.click(library.getByRole('button', { name: 'Connect Craftgineer' }));
    await waitFor(() => expect(cg.getState().connecting).toBe(true));
    expect(vi.mocked(invoke)).toHaveBeenCalledWith('connected_library_connect', expect.objectContaining({ provider: 'craftgineer', remember: false }));
    expect((settingsAccount.getByRole('button', { name: 'Connect Craftgineer' }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(settingsAccount.getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(vi.mocked(invoke)).toHaveBeenCalledWith('connected_library_cancel', { provider: 'craftgineer' }));
    await act(async () => finish({ account: { account: { id: 'late', displayName: 'Late account' }, access: 'active' }, remembered: false, walletUnavailable: false }));
    expect(cg.getState().status?.account ?? null).toBeNull();
    expect(screen.queryByText(/Late account/)).toBeNull();
    expect(cg.getState().connecting).toBe(false);
  });

  it('connects and disconnects from Settings immediately without saving preferences', async () => {
    const disconnected = { account: null, remembered: false, walletUnavailable: false };
    const connected = { account: { account: { id: 'pcc-maker', displayName: 'PCC maker' }, access: 'active' }, remembered: true, walletUnavailable: false };
    cg.setState({ status: disconnected });
    pcc.setState({ status: disconnected });
    useAppStore.setState({ settings: makeSettings() });
    useUiStore.setState({ libraryDrawerOpen: true, libraryDrawerTab: 'printcutcarve' });
    const save = vi.spyOn(useAppStore.getState(), 'updateSettings').mockResolvedValue();
    vi.mocked(invoke).mockImplementation((command) => Promise.resolve(command === 'connected_library_connect' ? connected : null));
    render(<><LibraryDrawer /><SettingsDialog onClose={vi.fn()} /></>);
    const preferences = within(screen.getByTestId('settings-dialog'));
    fireEvent.click(preferences.getByRole('tab', { name: 'Connected accounts' }));
    const account = within(preferences.getByRole('region', { name: 'PrintCutCarve connection' }));
    expect(preferences.queryByRole('button', { name: 'Save' })).toBeNull();
    fireEvent.click(account.getByRole('button', { name: 'Connect PrintCutCarve' }));
    await account.findByText('PrintCutCarve · PCC maker');
    const library = within(screen.getByTestId('library-drawer'));
    expect(await library.findByText('PrintCutCarve · PCC maker')).toBeDefined();
    expect(account.getByText('Remembered on this computer.')).toBeDefined();
    expect(preferences.queryByRole('button', { name: 'Browse library' })).toBeNull();
    fireEvent.click(account.getByRole('button', { name: 'Disconnect' }));
    expect(await library.findByRole('button', { name: 'Connect PrintCutCarve' })).toBeDefined();
    expect(library.getByRole('tab', { name: 'PrintCutCarve' }).getAttribute('aria-selected')).toBe('true');
    expect(save).not.toHaveBeenCalled();
    expect(cg.getState().status).toEqual(disconnected);
  });

  it('keeps Save available for preference edits while viewing connected accounts', () => {
    const disconnected = { account: null, remembered: false, walletUnavailable: false };
    cg.setState({ status: disconnected });
    pcc.setState({ status: disconnected });
    useAppStore.setState({ settings: makeSettings() });
    render(<SettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByRole('switch', { name: 'Autosave' }));
    fireEvent.click(screen.getByRole('tab', { name: 'Connected accounts' }));
    expect(screen.getByRole('button', { name: 'Save' })).toBeDefined();
    fireEvent.click(screen.getByRole('tab', { name: 'General' }));
    expect(screen.getByRole('switch', { name: 'Autosave' }).getAttribute('aria-checked')).toBe('false');
  });

  it('waits for real settings instead of rendering editable fallback defaults', async () => {
    useAppStore.setState({ settings: null });

    render(<SettingsDialog onClose={vi.fn()} />);

    expect(screen.getByText('Loading settings...')).toBeDefined();
    expect((screen.getByText('Save') as HTMLButtonElement).disabled).toBe(true);

    act(() => {
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
    expect(screen.getByText('Smooth edges')).toBeDefined();
    expect((screen.getByTestId('select-artwork-display') as HTMLSelectElement).value).toBe('by_layer');
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

    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
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
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

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
    expect(screen.queryByText('Include tool layers in job bounds')).toBeNull();

    expect(screen.getByText('Edit Hotkeys')).toBeDefined();
  });

  it('lists, edits, and saves application shortcuts from Settings', async () => {
    useAppStore.setState({ settings: makeSettings() });
    const updateSpy = vi.spyOn(useAppStore.getState(), 'updateSettings').mockResolvedValue(undefined);

    render(<SettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByText('Edit Hotkeys'));

    expect(screen.getByTestId('hotkey-settings-panel')).toBeDefined();
    fireEvent.change(screen.getByPlaceholderText('Search commands'), { target: { value: 'Activate Layers' } });
    expect(screen.getByText('Activate Layers')).toBeDefined();

    fireEvent.click(screen.getByRole('button', { name: 'Assign' }));
    fireEvent.keyDown(window, { key: '4', altKey: true });
    expect(screen.getByText('(custom)')).toBeDefined();

    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => {
      expect(updateSpy).toHaveBeenCalledWith(expect.objectContaining({
        custom_hotkeys: expect.objectContaining({
          [APP_COMMANDS.WINDOW_ACTIVATE_LAYERS]: 'Alt+4',
        }),
      }));
    });
    updateSpy.mockRestore();
  });

  it('resets all shortcut customizations to their defaults', async () => {
    useAppStore.setState({
      settings: makeSettings({
        custom_hotkeys: { [APP_COMMANDS.WINDOW_ACTIVATE_LAYERS]: 'Alt+4' },
      }),
    });
    const updateSpy = vi.spyOn(useAppStore.getState(), 'updateSettings').mockResolvedValue(undefined);

    render(<SettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByText('Edit Hotkeys'));
    fireEvent.click(screen.getByRole('button', { name: 'Reset All' }));
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => {
      expect(updateSpy).toHaveBeenCalledWith(expect.objectContaining({ custom_hotkeys: {} }));
    });
    updateSpy.mockRestore();
  });

  it('keeps preference-file management in the File & Import settings tab', async () => {
    useAppStore.setState({ settings: makeSettings() });
    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === 'reset_preferences') return makeSettings({ autosave_enabled: false });
      return null;
    });

    render(<SettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByText('File & Import'));

    expect(screen.getByRole('button', { name: 'Import Prefs' })).toBeDefined();
    expect(screen.getByRole('button', { name: 'Export Prefs' })).toBeDefined();
    expect(screen.getByRole('button', { name: 'Open Prefs Folder' })).toBeDefined();
    fireEvent.click(screen.getByRole('button', { name: 'Reset Prefs to Defaults' }));
    fireEvent.click(screen.getByRole('button', { name: 'OK' }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('reset_preferences');
    });
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
    // smooth edge rendering ships enabled by default
    const aaToggle = screen.getByTestId('toggle-antialiasing');
    expect(aaToggle.getAttribute('aria-checked')).toBe('true');
    expect((screen.getByTestId('select-artwork-display') as HTMLSelectElement).value).toBe('by_layer');
    expect(screen.queryByTestId('toggle-filled-rendering')).toBeNull();
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
          antialiasing: true,
          artwork_display_mode: 'by_layer',
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

  it('display controls call updateSettings on save', async () => {
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
    fireEvent.change(screen.getByTestId('select-artwork-display'), { target: { value: 'filled' } });

    // Click Save
    fireEvent.click(screen.getByText('Save'));

    await waitFor(() => {
      expect(spy).toHaveBeenCalledWith(
        expect.objectContaining({ dark_mode: true, artwork_display_mode: 'filled' }),
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
    act(() => {
      useAppStore.setState({
        settings: makeSettings({ display_unit: 'inches', autosave_enabled: false }),
      });
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
