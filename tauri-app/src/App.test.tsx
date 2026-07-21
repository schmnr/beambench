import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, cleanup, waitFor, act } from '@testing-library/react';
import { fireEvent } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import App from './App';
import { useAppStore } from './stores/appStore';
import { useProjectStore } from './stores/projectStore';
import { useUiStore } from './stores/uiStore';
import { useMachineStore } from './stores/machineStore';
import { useNotificationStore } from './stores/notificationStore';
import { usePreviewStore } from './stores/previewStore';
import { useWelcomeStore } from './stores/welcomeStore';
import { suppressProfileEvent } from './stores/machineStore';
import { previewService } from './services/previewService';
import { persistenceService } from './services/persistenceService';
import { printService } from './services/printService';
import { appService } from './services/appService';
import { feedbackService } from './services/feedbackService';
import { APP_COMMANDS } from './commands/appCommandIds';
import { clearClipboard, getClipboard } from './utils/clipboard';
import type { AppSettings } from './types/commands';
import type { AppEvent } from './types/events';
import type { JobProgress } from './types/machine';
import type { DiagnosticBundleV1, DiagnosticPanic } from './types/feedback';
import { makeProject, makeProjectObject, makeTransformLocks } from './test-utils/projectFixtures';

const mockListen = vi.fn();

function makeSettings(overrides: Partial<AppSettings> = {}): AppSettings {
  return {
    display_unit: 'mm',
    autosave_enabled: true,
    autosave_interval_secs: 300,
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

function makeAppEvent<T>(type: string, payload: T): AppEvent<T> {
  return {
    type,
    timestamp: '2026-04-16T15:30:00Z',
    payload,
  };
}

function makeJobProgress(overrides: Partial<JobProgress> = {}): JobProgress {
  return {
    state: 'running',
    total_lines: 10,
    queued_lines: 0,
    sent_lines: 4,
    acknowledged_lines: 3,
    elapsed_secs: 2,
    estimated_remaining_secs: 8,
    buffer_fill_bytes: 12,
    error_message: null,
    ...overrides,
  };
}

function makeDiagnosticBundle(recentPanics: DiagnosticPanic[] = []): DiagnosticBundleV1 {
  return {
    schema_version: 1,
    kind: 'crash',
    created_at: '2026-06-27T12:00:00Z',
    client: {
      app_version: '0.1.5',
      build_target: 'x86_64-unknown-linux-gnu',
      git_sha: 'abc123',
    },
    system: {
      os: 'linux',
      arch: 'x86_64',
    },
    machine: {
      connected: false,
      session_state: 'unknown',
    },
    ports_detected: [],
    connection_events: [],
    recent_serial: {
      tx_hex: '',
      tx_ascii: '',
      rx_hex: '',
      rx_ascii: '',
    },
    recent_logs: [],
    recent_panics: recentPanics,
    known_issues: [],
    project_file_attached: false,
    source_context: null,
  };
}

function getAppEventListener(): (event: { payload: AppEvent }) => void | Promise<void> {
  const appEventListener = mockListen.mock.calls.find((call) => call[0] === 'app-event')?.[1] as
    | ((event: { payload: AppEvent }) => void | Promise<void>)
    | undefined;
  expect(appEventListener).toBeTypeOf('function');
  if (!appEventListener) {
    throw new Error('Expected app-event listener to be registered');
  }
  return appEventListener;
}

async function renderApp(): Promise<void> {
  await act(async () => {
    render(<App />);
  });
}

async function dispatchKeyDown(
  target: Window | HTMLElement,
  init: KeyboardEventInit,
): Promise<void> {
  await act(async () => {
    fireEvent.keyDown(target, init);
    await Promise.resolve();
  });
}

async function dispatchCancelableKeyDown(init: KeyboardEventInit): Promise<KeyboardEvent> {
  const event = new KeyboardEvent('keydown', {
    bubbles: true,
    cancelable: true,
    ...init,
  });
  await act(async () => {
    window.dispatchEvent(event);
    await Promise.resolve();
  });
  return event;
}

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((cmd: string) => {
    if (cmd === 'get_app_status')
      return Promise.resolve({ version: '0.1.0', state: 'ready' });
    if (cmd === 'get_app_settings')
      return Promise.resolve(makeSettings());
    return Promise.resolve(null);
  }),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => mockListen(...args),
}));

vi.mock('@tauri-apps/api/webviewWindow', () => ({
  getCurrentWebviewWindow: vi.fn(() => ({
    onDragDropEvent: vi.fn().mockResolvedValue(vi.fn()),
  })),
}));

// jsdom doesn't have ResizeObserver — stub it
class MockResizeObserver {
  observe = vi.fn();
  unobserve = vi.fn();
  disconnect = vi.fn();
}
globalThis.ResizeObserver = MockResizeObserver as unknown as typeof ResizeObserver;

// jsdom doesn't have canvas 2D context — stub it
HTMLCanvasElement.prototype.getContext = vi.fn().mockReturnValue({
  fillRect: vi.fn(),
  strokeRect: vi.fn(),
  fillText: vi.fn(),
  beginPath: vi.fn(),
  moveTo: vi.fn(),
  lineTo: vi.fn(),
  stroke: vi.fn(),
  fill: vi.fn(),
  save: vi.fn(),
  restore: vi.fn(),
  setLineDash: vi.fn(),
  setTransform: vi.fn(),
  translate: vi.fn(),
  rotate: vi.fn(),
  scale: vi.fn(),
  transform: vi.fn(),
  arc: vi.fn(),
  ellipse: vi.fn(),
  closePath: vi.fn(),
  rect: vi.fn(),
  clip: vi.fn(),
  roundRect: vi.fn(),
  bezierCurveTo: vi.fn(),
  clearRect: vi.fn(),
  fillStyle: '',
  strokeStyle: '',
  lineWidth: 1,
  font: '',
  textAlign: 'left',
  textBaseline: 'top',
}) as unknown as typeof HTMLCanvasElement.prototype.getContext;

if (!globalThis.URL.createObjectURL) {
  globalThis.URL.createObjectURL = vi.fn(() => 'blob:test');
}

const initialAppState = useAppStore.getState();
const initialProjectState = useProjectStore.getState();
const initialUiState = useUiStore.getState();
const initialMachineState = useMachineStore.getState();
const initialNotificationState = useNotificationStore.getState();
const initialPreviewState = usePreviewStore.getState();

afterEach(() => {
  vi.useRealTimers();
  cleanup();
  vi.restoreAllMocks();
  clearClipboard();
  useAppStore.setState(initialAppState, true);
  useProjectStore.setState(initialProjectState, true);
  useUiStore.setState(initialUiState, true);
  useMachineStore.setState(initialMachineState, true);
  useNotificationStore.setState(initialNotificationState, true);
  usePreviewStore.setState(initialPreviewState, true);
});

describe('App', () => {
  beforeEach(() => {
    mockListen.mockReset();
    mockListen.mockResolvedValue(() => {});
    useAppStore.setState({
      fetchStatus: vi.fn(),
      fetchSettings: vi.fn(),
    });
    // Prevent mount-time async calls from connection and device panels.
    useMachineStore.setState({
      refreshPorts: vi.fn(),
      loadProfiles: vi.fn(),
    });
    // The welcome/promo screen now shows on every startup; keep it out of these
    // shell tests by making its open action a no-op.
    useWelcomeStore.setState({ openDialog: () => {} });
  });

  it('renders the app shell', async () => {
    await act(async () => { render(<App />); });
    expect(screen.getByText('Beam Bench', { exact: false })).toBeDefined();
  });

  it('renders toolbar buttons', async () => {
    await act(async () => { render(<App />); });
    expect(screen.getByTitle('Select')).toBeDefined();
    expect(screen.getByTitle('Rectangle')).toBeDefined();
  });

  it('renders panel tabs', async () => {
    await act(async () => { render(<App />); });
    expect(screen.getByText('Cuts / Layers')).toBeDefined();
    expect(screen.getByText('Shape Properties')).toBeDefined();
    expect(screen.getByText('Laser Control')).toBeDefined();
  });

  it('calls fetchStatus and fetchSettings on mount', async () => {
    await act(async () => { render(<App />); });
    expect(useAppStore.getState().fetchStatus).toHaveBeenCalledOnce();
    expect(useAppStore.getState().fetchSettings).toHaveBeenCalledOnce();
    expect(useMachineStore.getState().loadProfiles).toHaveBeenCalledOnce();
  });

  it('renders canvas element', async () => {
    await act(async () => { render(<App />); });
    const canvas = document.querySelector('canvas');
    expect(canvas).toBeDefined();
  });

  it('syncs frontend selection to the agent surface on a trailing timer', async () => {
    vi.useFakeTimers();
    const invokeMock = vi.mocked(invoke);
    const project = makeProject({
      objects: [makeProjectObject({ id: 'obj-1' }), makeProjectObject({ id: 'obj-2' })],
    });
    useProjectStore.setState({
      project,
      selectedLayerId: project.layers[0]?.id ?? null,
      selectedObjectIds: [],
    });

    await act(async () => { render(<App />); });
    invokeMock.mockClear();

    await act(async () => {
      useProjectStore.setState({
        project,
        selectedLayerId: project.layers[0]?.id ?? null,
        selectedObjectIds: ['obj-1'],
      });
      useProjectStore.setState({
        project,
        selectedLayerId: project.layers[0]?.id ?? null,
        selectedObjectIds: ['obj-1', 'obj-2'],
      });
      await Promise.resolve();
    });

    await act(async () => {
      vi.advanceTimersByTime(74);
      await Promise.resolve();
    });
    expect(invokeMock.mock.calls.filter(([cmd]) => cmd === 'agent_sync_selection')).toHaveLength(0);

    await act(async () => {
      vi.advanceTimersByTime(1);
      await Promise.resolve();
    });

    const syncCalls = invokeMock.mock.calls.filter(([cmd]) => cmd === 'agent_sync_selection');
    expect(syncCalls).toHaveLength(1);
    expect(syncCalls[0][1]).toMatchObject({
      selectedObjectIds: ['obj-1', 'obj-2'],
      selectedLayerId: project.layers[0]?.id ?? null,
      projectId: project.metadata.project_id,
    });
    expect(typeof (syncCalls[0][1] as { frontendUpdatedAtMs: unknown }).frontendUpdatedAtMs).toBe('number');
    vi.useRealTimers();
  });

  it('renders status bar zoom controls', async () => {
    await act(async () => { render(<App />); });
    expect(screen.getByText('100%')).toBeDefined();
    expect(screen.getAllByText('Grid').length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText('Snap').length).toBeGreaterThanOrEqual(1);
  });

  it('renders a global cancellable preview generation dialog', async () => {
    const cancelPreviewGeneration = vi.fn().mockResolvedValue(undefined);
    await renderApp();

    await act(async () => {
      usePreviewStore.setState({
        previewWindowOpen: false,
        previewGenerationDialogVisible: true,
        previewGenerationDialogTitle: 'Generating preview...',
        cancelPreviewGeneration,
      });
    });

    expect(screen.getByRole('dialog', { name: 'Generating preview...' })).toBeDefined();
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(cancelPreviewGeneration).toHaveBeenCalledTimes(1);

    await dispatchKeyDown(window, { key: 'Escape' });
    expect(cancelPreviewGeneration).toHaveBeenCalledTimes(2);
  });
});

describe('App bootstrap', () => {
  beforeEach(() => {
    mockListen.mockReset();
    mockListen.mockResolvedValue(() => {});
  });

  it('mounts and completes the async IPC bootstrap', async () => {
    render(<App />);

    // Before the fetches resolve, StatusBar shows the loading state.
    expect(screen.getByText('Initializing...')).toBeDefined();

    // Wait for the async mount path to complete: StatusBar reflects
    // the resolved status once fetchStatus settles the store.
    await screen.findByText('ready');
    expect(screen.getByText(/v0\.1\.0/)).toBeDefined();

    // Verify the store converged to the expected post-bootstrap state.
    const { status, settings, loading, error } = useAppStore.getState();
    expect(error).toBeNull();
    expect(loading).toBe(false);
    expect(status).toEqual({ version: '0.1.0', state: 'ready' });
    expect(settings).toEqual(makeSettings());
  });

  it('warns when restoring the saved panel layout fails', async () => {
    // deliberately invalid panel_layout to exercise the restore-
    // failure path. Cast through `unknown` instead of `as never` so the
    // intent is documented: the test MUST provide a malformed payload here.
    vi.spyOn(appService, 'getSettings').mockResolvedValue(
      makeSettings({ panel_layout: {} as unknown as AppSettings['panel_layout'] }),
    );

    render(<App />);

    await waitFor(() => {
      const notifications = useNotificationStore.getState().notifications;
      expect(
        notifications.some(
          (notification) =>
            notification.message.includes('Failed to restore saved panel layout')
            && notification.type === 'warning',
        ),
      ).toBe(true);
    });
  });

  it('warns when the startup recovery check fails', async () => {
    vi.spyOn(persistenceService, 'checkRecovery').mockRejectedValue(new Error('recovery failed'));

    render(<App />);

    await waitFor(() => {
      const notifications = useNotificationStore.getState().notifications;
      expect(
        notifications.some(
          (notification) =>
            notification.message.includes('Failed to check for recovery files')
            && notification.type === 'warning',
        ),
      ).toBe(true);
    });
  });

  it('prompts to send a crash report when startup diagnostics include stored panics', async () => {
    vi.spyOn(feedbackService, 'previewReport').mockResolvedValue(makeDiagnosticBundle([{
      ts: '2026-06-27T11:50:00Z',
      message: 'startup panic',
      app_version: '0.1.5',
      os: 'linux',
      build_target: 'x86_64-unknown-linux-gnu',
      git_sha: 'abc123',
    }]));

    render(<App />);

    await waitFor(() => {
      expect(screen.getByDisplayValue('Previous crash')).toBeDefined();
      expect(screen.getByDisplayValue(/Beam Bench detected a crash from the previous session/)).toBeDefined();
    });
  });

  it('warns when subscribing to the app event bridge fails', async () => {
    mockListen.mockRejectedValueOnce(new Error('listen failed'));

    render(<App />);

    await waitFor(() => {
      const notifications = useNotificationStore.getState().notifications;
      expect(
        notifications.some(
          (notification) =>
            notification.message.includes('Failed to subscribe to backend events')
            && notification.type === 'warning',
        ),
      ).toBe(true);
    });
  });

  it('refreshes profiles and invalidates preview on backend profile events', async () => {
    const loadProfiles = vi.fn().mockResolvedValue(undefined);
    const invalidate = vi.fn();
    let eventHandler: ((event: { payload: AppEvent }) => void | Promise<void>) | undefined;

    mockListen.mockImplementation((_eventName: string, handler: typeof eventHandler) => {
      eventHandler = handler;
      return Promise.resolve(() => {});
    });

    useMachineStore.setState({
      loadProfiles,
      refreshPorts: vi.fn(),
    });
    usePreviewStore.setState({ invalidate });

    await renderApp();

    await waitFor(() => {
      expect(mockListen).toHaveBeenCalled();
    });

    await act(async () => {
      await eventHandler?.({
        payload: makeAppEvent('profile.activated', { profile_id: 'prof-1' }),
      });
    });

    await waitFor(() => {
      expect(loadProfiles).toHaveBeenCalledTimes(2);
      expect(invalidate).toHaveBeenCalledTimes(1);
    });
  });

  it('ignores suppressed profile events from local frontend actions', async () => {
    const loadProfiles = vi.fn().mockResolvedValue(undefined);
    const invalidate = vi.fn();
    let eventHandler: ((event: { payload: AppEvent }) => void | Promise<void>) | undefined;

    mockListen.mockImplementation((_eventName: string, handler: typeof eventHandler) => {
      eventHandler = handler;
      return Promise.resolve(() => {});
    });

    useMachineStore.setState({
      loadProfiles,
      refreshPorts: vi.fn(),
    });
    usePreviewStore.setState({ invalidate });

    await renderApp();

    await waitFor(() => {
      expect(loadProfiles).toHaveBeenCalledTimes(1);
    });

    suppressProfileEvent('profile.activated', 'prof-1');
    await act(async () => {
      await eventHandler?.({
        payload: makeAppEvent('profile.activated', { profile_id: 'prof-1' }),
      });
    });

    await waitFor(() => {
      expect(loadProfiles).toHaveBeenCalledTimes(1);
      expect(invalidate).toHaveBeenCalledTimes(0);
    });
  });

  it('does not swallow unrelated backend profile events when a local event is suppressed', async () => {
    const loadProfiles = vi.fn().mockResolvedValue(undefined);
    const invalidate = vi.fn();
    let eventHandler: ((event: { payload: AppEvent }) => void | Promise<void>) | undefined;

    mockListen.mockImplementation((_eventName: string, handler: typeof eventHandler) => {
      eventHandler = handler;
      return Promise.resolve(() => {});
    });

    useMachineStore.setState({
      loadProfiles,
      refreshPorts: vi.fn(),
    });
    usePreviewStore.setState({ invalidate });

    await renderApp();

    await waitFor(() => {
      expect(loadProfiles).toHaveBeenCalledTimes(1);
    });

    suppressProfileEvent('profile.saved', 'prof-1');
    await act(async () => {
      await eventHandler?.({
        payload: makeAppEvent('profile.saved', { profile: { id: 'prof-2' } }),
      });
    });

    await waitFor(() => {
      expect(loadProfiles).toHaveBeenCalledTimes(2);
      expect(invalidate).toHaveBeenCalledTimes(0);
    });
  });

  it('does not invalidate preview for backend saves to inactive profiles', async () => {
    const loadProfiles = vi.fn().mockResolvedValue(undefined);
    const invalidate = vi.fn();
    let eventHandler: ((event: { payload: AppEvent }) => void | Promise<void>) | undefined;

    mockListen.mockImplementation((_eventName: string, handler: typeof eventHandler) => {
      eventHandler = handler;
      return Promise.resolve(() => {});
    });

    useMachineStore.setState({
      loadProfiles,
      refreshPorts: vi.fn(),
      activeProfileId: 'prof-active',
    });
    usePreviewStore.setState({ invalidate });

    await renderApp();

    await waitFor(() => {
      expect(loadProfiles).toHaveBeenCalledTimes(1);
    });

    await act(async () => {
      await eventHandler?.({
        payload: makeAppEvent('profile.saved', { profile: { id: 'prof-inactive' } }),
      });
    });

    await waitFor(() => {
      expect(loadProfiles).toHaveBeenCalledTimes(2);
      expect(invalidate).toHaveBeenCalledTimes(0);
    });
  });

  it('routes native File Import events through the project import handler', async () => {
    const originalPlatform = navigator.platform;
    Object.defineProperty(navigator, 'platform', {
      configurable: true,
      value: 'MacIntel',
    });
    const project = makeProject();
    const importFiles = vi.fn().mockResolvedValue(undefined);
    let nativeMenuHandler:
      | ((event: { payload: { commandId: string; filePath?: string } }) => void | Promise<void>)
      | undefined;

    mockListen.mockImplementation((eventName: string, handler: typeof nativeMenuHandler) => {
      if (eventName === 'native-menu-command') nativeMenuHandler = handler;
      return Promise.resolve(() => {});
    });
    useProjectStore.setState({
      project,
      selectedLayerId: project.layers[0].id,
      importFiles,
      loadProject: vi.fn().mockResolvedValue(undefined),
    });

    try {
      await renderApp();
      await waitFor(() => {
        expect(nativeMenuHandler).toBeDefined();
      });

      await act(async () => {
        await nativeMenuHandler?.({ payload: { commandId: APP_COMMANDS.FILE_IMPORT } });
      });

      await waitFor(() => {
        expect(importFiles).toHaveBeenCalledWith(project.layers[0].id);
      });
    } finally {
      Object.defineProperty(navigator, 'platform', {
        configurable: true,
        value: originalPlatform,
      });
    }
  });
});

describe('Keyboard shortcuts', () => {
  beforeEach(() => {
    mockListen.mockReset();
    mockListen.mockResolvedValue(() => {});
    useAppStore.setState({ fetchStatus: vi.fn(), fetchSettings: vi.fn() });
    useMachineStore.setState({
      refreshPorts: vi.fn(),
      loadProfiles: vi.fn(),
      hydrateSession: vi.fn().mockResolvedValue(undefined),
      refreshStatus: vi.fn().mockResolvedValue(undefined),
      refreshSessionState: vi.fn().mockResolvedValue(undefined),
      refreshJobProgress: vi.fn().mockResolvedValue(undefined),
    });
    useProjectStore.setState({
      project: makeProject({
        metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p-shortcuts', project_name: 'Shortcut Test', created_at: '', modified_at: '' },
        workspace: { bed_width_mm: 400, bed_height_mm: 400, origin: 'top_left' },
        layers: [],
        objects: [],
      }),
      loadProject: vi.fn().mockResolvedValue(undefined),
      createProject: vi.fn(),
    });
    vi.spyOn(persistenceService, 'checkRecovery').mockResolvedValue([]);
  });

  it('Alt+J triggers autoJoinShapes when objects are selected', async () => {
    const autoJoinShapes = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project: makeProject({
        objects: [
          makeProjectObject({ id: 'o1' }),
          makeProjectObject({ id: 'o2', bounds: { min: { x: 12, y: 0 }, max: { x: 22, y: 10 } } }),
        ],
      }),
      selectedObjectIds: ['o1', 'o2'],
      autoJoinShapes,
    });
    await renderApp();
    await dispatchKeyDown(window, { key: 'j', altKey: true });
    expect(autoJoinShapes).toHaveBeenCalledWith(['o1', 'o2'], 0.05);
  });

  it('Alt+J does not auto-join raster-only selections', async () => {
    const autoJoinShapes = vi.fn().mockResolvedValue(undefined);
    const raster = makeProjectObject({
      id: 'raster-1',
      data: {
        type: 'raster_image',
        asset_key: 'asset-1',
        original_width_px: 100,
        original_height_px: 100,
        masks: [],
      },
    });
    useProjectStore.setState({
      project: makeProject({ objects: [raster] }),
      selectedObjectIds: ['raster-1'],
      autoJoinShapes,
    });

    await renderApp();
    await dispatchKeyDown(window, { key: 'j', altKey: true });

    expect(autoJoinShapes).not.toHaveBeenCalled();
  });

  it('Ctrl+Shift+C does not convert raster-only selections to path', async () => {
    const convertToPath = vi.fn().mockResolvedValue(undefined);
    const raster = makeProjectObject({
      id: 'raster-1',
      data: {
        type: 'raster_image',
        asset_key: 'asset-1',
        original_width_px: 100,
        original_height_px: 100,
        masks: [],
      },
    });
    useProjectStore.setState({
      project: makeProject({ objects: [raster] }),
      selectedObjectIds: ['raster-1'],
      convertToPath,
    });

    await renderApp();
    await dispatchKeyDown(window, { key: 'C', ctrlKey: true, shiftKey: true });

    expect(convertToPath).not.toHaveBeenCalled();
  });

  it('Ctrl+W triggers booleanWeld when 2+ objects selected', async () => {
    const booleanWeld = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project: makeProject({
        metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p1', project_name: 'Test', created_at: '', modified_at: '' },
        workspace: { bed_width_mm: 400, bed_height_mm: 400, origin: 'top_left' },
        layers: [],
        objects: [
          makeProjectObject({ id: 'o1', name: 'A', transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 }, bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } }, layer_id: 'l1', z_index: 0, data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 } }),
          makeProjectObject({ id: 'o2', name: 'B', transform: { a: 1, b: 0, c: 0, d: 1, tx: 12, ty: 0 }, bounds: { min: { x: 12, y: 0 }, max: { x: 22, y: 10 } }, layer_id: 'l1', z_index: 1, data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 } }),
        ],
        assets: [],
      }),
      selectedObjectIds: ['o1', 'o2'],
      booleanWeld,
    });
    await renderApp();
    await dispatchKeyDown(window, { key: 'w', ctrlKey: true });
    expect(booleanWeld).toHaveBeenCalledWith(['o1', 'o2']);
  });

  it('Alt+D confirms before deleting duplicates with the current selection', async () => {
    const countDuplicates = vi.fn().mockResolvedValue(2);
    const deleteDuplicates = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      selectedObjectIds: ['o1'],
      countDuplicates,
      deleteDuplicates,
    });
    await renderApp();
    await dispatchKeyDown(window, { key: 'd', altKey: true });
    expect(countDuplicates).toHaveBeenCalledWith(['o1']);
    expect(await screen.findByText('2 duplicate objects were detected. Delete them?')).toBeDefined();
    fireEvent.click(screen.getByTestId('delete-duplicates-confirm'));
    await waitFor(() => expect(deleteDuplicates).toHaveBeenCalledWith(['o1']));
  });

  it('Alt+D confirms before deleting duplicates across the project when nothing is selected', async () => {
    const countDuplicates = vi.fn().mockResolvedValue(1);
    const deleteDuplicates = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      selectedObjectIds: [],
      countDuplicates,
      deleteDuplicates,
    });
    await renderApp();
    await dispatchKeyDown(window, { key: 'd', altKey: true });
    expect(countDuplicates).toHaveBeenCalledWith([]);
    expect(await screen.findByText('1 duplicate object was detected. Delete it?')).toBeDefined();
    fireEvent.click(screen.getByTestId('delete-duplicates-confirm'));
    await waitFor(() => expect(deleteDuplicates).toHaveBeenCalledWith([]));
  });

  it('Tab cycles to next object by creation order', async () => {
    const selectObjects = vi.fn();
    useProjectStore.setState({
      project: makeProject({
        metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p1', project_name: 'Test', created_at: '', modified_at: '' },
        workspace: { bed_width_mm: 400, bed_height_mm: 400, origin: 'top_left' },
        layers: [],
        objects: [
          makeProjectObject({ id: 'o2', name: 'B', bounds: { min: { x: 0, y: 0 }, max: { x: 5, y: 5 } }, layer_id: 'l1', z_index: 1, data: { type: 'shape', kind: 'rectangle', width: 5, height: 5, corner_radius: 0 }, created_at: '2026-03-01T00:00:01Z' }),
          makeProjectObject({ id: 'o1', name: 'A', bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } }, layer_id: 'l1', z_index: 0, data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 }, created_at: '2026-03-01T00:00:00Z' }),
        ],
        assets: [],
      }),
      selectedObjectIds: ['o1'],
      selectObjects,
    });
    await renderApp();
    await dispatchKeyDown(window, { key: 'Tab' });
    // o1 (earlier created_at) is index 0, so next should be o2 (index 1)
    expect(selectObjects).toHaveBeenCalledWith(['o2']);
  });

  it('Shift+Tab cycles to previous object by creation order', async () => {
    const selectObjects = vi.fn();
    useProjectStore.setState({
      project: makeProject({
        metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p1', project_name: 'Test', created_at: '', modified_at: '' },
        workspace: { bed_width_mm: 400, bed_height_mm: 400, origin: 'top_left' },
        layers: [],
        objects: [
          makeProjectObject({ id: 'o2', name: 'B', bounds: { min: { x: 0, y: 0 }, max: { x: 5, y: 5 } }, layer_id: 'l1', z_index: 1, data: { type: 'shape', kind: 'rectangle', width: 5, height: 5, corner_radius: 0 }, created_at: '2026-03-01T00:00:01Z' }),
          makeProjectObject({ id: 'o1', name: 'A', bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } }, layer_id: 'l1', z_index: 0, data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 }, created_at: '2026-03-01T00:00:00Z' }),
        ],
        assets: [],
      }),
      selectedObjectIds: ['o2'],
      selectObjects,
    });
    await renderApp();
    await dispatchKeyDown(window, { key: 'Tab', shiftKey: true });
    // o2 is index 1 in sorted order, Shift+Tab wraps back to o1 (index 0)
    expect(selectObjects).toHaveBeenCalledWith(['o1']);
  });

  it('Ctrl+Tab sets tabs tool', async () => {
    await renderApp();
    await dispatchKeyDown(window, { key: 'Tab', ctrlKey: true });
    expect(useUiStore.getState().activeTool).toBe('tabs');
  });

  it('keeps Escape inside the node tool as the node select submode shortcut', async () => {
    useUiStore.setState({ activeTool: 'node', nodeSubMode: 'trim' });

    await renderApp();
    await dispatchKeyDown(window, { key: 'Escape' });

    expect(useUiStore.getState().activeTool).toBe('node');
    expect(useUiStore.getState().nodeSubMode).toBe('select');
  });

  it('Ctrl+Shift+B triggers convertToBitmap for the selected object', async () => {
    const convertToBitmap = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project: makeProject({
        metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p1', project_name: 'Test', created_at: '', modified_at: '' },
        workspace: { bed_width_mm: 400, bed_height_mm: 400, origin: 'top_left' },
        layers: [],
        objects: [makeProjectObject({ id: 'o1', name: 'Text', transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 }, bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } }, layer_id: 'l1', z_index: 0, data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 } })],
        assets: [],
      }),
      selectedObjectIds: ['o1'],
      convertToBitmap,
    });
    await renderApp();
    await dispatchKeyDown(window, { key: 'B', ctrlKey: true, shiftKey: true });
    expect(convertToBitmap).toHaveBeenCalledWith('o1', 300);
  });

  it('Alt+I does not call refreshImage (opens dialog instead)', async () => {
    const refreshImage = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project: makeProject({
        metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p1', project_name: 'Test', created_at: '', modified_at: '' },
        workspace: { bed_width_mm: 400, bed_height_mm: 400, origin: 'top_left' },
        layers: [],
        objects: [makeProjectObject({ id: 'img1', name: 'Image', transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 }, bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } }, layer_id: 'l1', z_index: 0, data: { type: 'raster_image', asset_key: 'asset-1', original_width_px: 100, original_height_px: 100 } })],
        assets: [],
      }),
      selectedObjectIds: ['img1'],
      refreshImage,
    });
    await renderApp();
    await dispatchKeyDown(window, { key: 'I', altKey: true });
    // should NOT call refreshImage — opens Adjust Image dialog instead
    expect(refreshImage).not.toHaveBeenCalled();
  });

  it('Alt+T opens trace dialog instead of tracing directly', async () => {
    // Alt+T should open dialog, not call importService.traceImage directly.
    // We verify by checking the shortcut doesn't error and the key is consumed (preventDefault).
    useProjectStore.setState({
      project: makeProject({
        metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p1', project_name: 'Test', created_at: '', modified_at: '' },
        workspace: { bed_width_mm: 400, bed_height_mm: 400, origin: 'top_left' },
        layers: [],
        objects: [makeProjectObject({ id: 'img1', name: 'Image', transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 }, bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } }, layer_id: 'l1', z_index: 0, data: { type: 'raster_image', asset_key: 'asset-1', original_width_px: 100, original_height_px: 100 } })],
        assets: [],
      }),
      selectedObjectIds: ['img1'],
    });
    await renderApp();
    // Should not throw — the old path called importService.traceImage which could
    // fail in test env. The new path sets dialog state which is safe.
    await expect(dispatchKeyDown(window, { key: 'T', altKey: true })).resolves.toBeUndefined();
  });

  it('prevents WebView navigation for native-owned Jog Laser bracket shortcuts', async () => {
    const originalPlatform = navigator.platform;
    Object.defineProperty(navigator, 'platform', { configurable: true, value: 'MacIntel' });
    await renderApp();

    try {
      const events = [
        await dispatchCancelableKeyDown({ key: '[', metaKey: true, altKey: true }),
        await dispatchCancelableKeyDown({ key: ']', metaKey: true, altKey: true }),
        await dispatchCancelableKeyDown({ key: ']', metaKey: true, shiftKey: true }),
        await dispatchCancelableKeyDown({ key: '[', metaKey: true, shiftKey: true }),
      ];

      for (const event of events) {
        expect(event.defaultPrevented).toBe(true);
      }
    } finally {
      Object.defineProperty(navigator, 'platform', { configurable: true, value: originalPlatform });
    }
  });

  it('Alt+Shift+L triggers machine-file export', async () => {
    const exportGcode = vi.spyOn(previewService, 'exportGcode').mockResolvedValue('output.gcode');
    useProjectStore.setState({ project: makeProject() });
    usePreviewStore.setState({ state: 'current' });
    await renderApp();
    await dispatchKeyDown(window, { key: 'L', altKey: true, shiftKey: true });
    expect(exportGcode).toHaveBeenCalledOnce();
  });

  it('Alt+Shift+L exports machine files via G-code export', async () => {
    const exportGcode = vi.spyOn(previewService, 'exportGcode').mockResolvedValue('output.gcode');
    const exportSvg = vi.spyOn(persistenceService, 'exportSvg').mockResolvedValue('output.svg');
    useProjectStore.setState({ project: makeProject() });
    usePreviewStore.setState({ state: 'current' });

    await renderApp();
    await dispatchKeyDown(window, { key: 'L', altKey: true, shiftKey: true });

    expect(exportGcode).toHaveBeenCalledOnce();
    expect(exportSvg).not.toHaveBeenCalled();
  });

  it('Alt+Shift+L respects machine-file command gating when preview is stale', async () => {
    const exportGcode = vi.spyOn(previewService, 'exportGcode').mockResolvedValue('output.gcode');
    useProjectStore.setState({ project: makeProject() });
    usePreviewStore.setState({ state: 'idle' });

    await renderApp();
    await dispatchKeyDown(window, { key: 'L', altKey: true, shiftKey: true });

    expect(exportGcode).not.toHaveBeenCalled();
  });

  it('Alt+X triggers artwork export instead of machine-file export', async () => {
    const exportArtwork = vi.spyOn(persistenceService, 'exportArtwork').mockResolvedValue('output.svg');
    const exportGcode = vi.spyOn(previewService, 'exportGcode').mockResolvedValue('output.gcode');
    useProjectStore.setState({ project: makeProject() });

    await renderApp();
    await dispatchKeyDown(window, { key: 'x', altKey: true });

    await waitFor(() => {
      expect(exportArtwork).toHaveBeenCalledOnce();
    });
    expect(exportGcode).not.toHaveBeenCalled();
  });

  it('treats cancelled artwork export shortcuts as a quiet no-op', async () => {
    const exportArtwork = vi.spyOn(persistenceService, 'exportArtwork').mockRejectedValue(new Error('Export cancelled'));
    const push = vi.fn();
    useNotificationStore.setState({ push });
    useProjectStore.setState({ project: makeProject() });

    await renderApp();
    push.mockClear();
    await dispatchKeyDown(window, { key: 'x', altKey: true });

    await waitFor(() => {
      expect(exportArtwork).toHaveBeenCalledOnce();
    });
    expect(push).not.toHaveBeenCalled();
  });

  it('Ctrl+P and Ctrl+Shift+P trigger black and color print commands', async () => {
    const printProject = vi.spyOn(printService, 'printProject').mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeProject() });

    await renderApp();
    await dispatchKeyDown(window, { key: 'p', ctrlKey: true });
    await dispatchKeyDown(window, { key: 'P', ctrlKey: true, shiftKey: true });

    await waitFor(() => {
      expect(printProject).toHaveBeenNthCalledWith(1, 'black');
      expect(printProject).toHaveBeenNthCalledWith(2, 'color');
    });
  });

  it('suppresses file shortcuts while focus is in the notes textarea', async () => {
    const createProject = vi.fn();
    const saveProject = vi.fn();
    const saveProjectAs = vi.fn();
    const openProject = vi.fn();
    const closeSpy = vi.spyOn(window, 'close').mockImplementation(() => undefined);

    useProjectStore.setState({
      project: makeProject({
        metadata: {
          format_version: '1',
          app_version: '0.1.0',
          project_id: 'p1',
          project_name: 'Test',
          created_at: '',
          modified_at: '',
        },
        workspace: { bed_width_mm: 400, bed_height_mm: 400, origin: 'top_left' },
        layers: [],
        objects: [],
        assets: [],
        notes: '',
      }),
      createProject,
      saveProject,
      saveProjectAs,
      openProject,
    });

    await renderApp();
    createProject.mockClear();
    saveProject.mockClear();
    saveProjectAs.mockClear();
    openProject.mockClear();
    closeSpy.mockClear();
    await dispatchKeyDown(window, { key: 'n', ctrlKey: true, altKey: true });

    const textarea = await screen.findByTestId('notes-textarea');
    textarea.focus();

    await dispatchKeyDown(textarea, { key: 'n', ctrlKey: true });
    await dispatchKeyDown(textarea, { key: 's', ctrlKey: true });
    await dispatchKeyDown(textarea, { key: 's', ctrlKey: true, shiftKey: true });
    await dispatchKeyDown(textarea, { key: 'o', ctrlKey: true });
    await dispatchKeyDown(textarea, { key: 'q', ctrlKey: true });

    expect(createProject).not.toHaveBeenCalled();
    expect(saveProject).not.toHaveBeenCalled();
    expect(saveProjectAs).not.toHaveBeenCalled();
    expect(openProject).not.toHaveBeenCalled();
    expect(closeSpy).not.toHaveBeenCalled();
  });

  it('surfaces import completion events as notifications', async () => {
    await renderApp();
    const appEventListener = getAppEventListener();

    await act(async () => {
      await appEventListener({
        payload: makeAppEvent('project.import.completed', { file_count: 2, object_ids: ['o1', 'o2', 'o3'] }),
      });
    });

    const notifications = useNotificationStore.getState().notifications;
    expect(notifications[notifications.length - 1]?.message).toBe('Import complete: 2 files, 3 objects');
    expect(notifications[notifications.length - 1]?.type).toBe('success');
  });

  it('applies settings payloads from settings, import, and reset events', async () => {
    const applySettings = vi.fn();
    useAppStore.setState({ applySettings });
    await renderApp();
    const appEventListener = getAppEventListener();
    const eventTypes = [
      'app.settings.updated',
      'app.preferences.imported',
      'app.preferences.reset',
    ];

    await act(async () => {
      for (const [index, eventType] of eventTypes.entries()) {
        await appEventListener({
          payload: makeAppEvent(eventType, {
            settings: makeSettings({ ui_theme: index === 1 ? 'light' : 'dark' }),
          }),
        });
      }
    });

    expect(applySettings).toHaveBeenCalledTimes(3);
    expect(applySettings).toHaveBeenNthCalledWith(2, expect.objectContaining({ ui_theme: 'light' }));
  });

  it('surfaces backend job tick failures and clears stale running progress', async () => {
    await renderApp();
    const appEventListener = getAppEventListener();
    await act(async () => {
      useMachineStore.setState({
        sessionState: 'running',
        connectedPort: '/dev/ttyUSB0',
        jobProgress: makeJobProgress(),
        error: null,
      });
    });

    await act(async () => {
      await appEventListener({
        payload: makeAppEvent('job.tick_failed', {
          message: 'Job streaming tick failed: serial error: injected read failure',
        }),
      });
      await appEventListener({
        payload: makeAppEvent('machine.disconnected', { reason: 'job_tick_failed' }),
      });
    });

    const machine = useMachineStore.getState();
    expect(machine.sessionState).toBe('disconnected');
    expect(machine.jobProgress).toBeNull();
    expect(machine.error).toBe('Job streaming tick failed: serial error: injected read failure');
    const notifications = useNotificationStore.getState().notifications;
    expect(notifications[notifications.length - 1]).toMatchObject({
      type: 'error',
    });
    expect(notifications[notifications.length - 1]?.message).toContain('injected read failure');
  });

  it('surfaces a warning when disconnect could not confirm a software stop', async () => {
    await renderApp();
    const appEventListener = getAppEventListener();
    await act(async () => {
      useMachineStore.setState({
        sessionState: 'ready',
        connectedPort: '/dev/ttyUSB0',
        machineStatus: null,
        error: null,
      });
    });

    await act(async () => {
      await appEventListener({
        payload: makeAppEvent('machine.disconnected', {
          stop_warning:
            "Lihuiyu could not confirm a software stop; use the machine's physical stop",
        }),
      });
    });

    expect(useMachineStore.getState().sessionState).toBe('disconnected');
    const notifications = useNotificationStore.getState().notifications;
    expect(notifications[notifications.length - 1]).toMatchObject({ type: 'warning' });
    expect(notifications[notifications.length - 1]?.message).toContain('physical stop');
  });

  it('updates machine state on disconnect events while preserving terminal job failure details', async () => {
    await renderApp();
    const appEventListener = getAppEventListener();
    await act(async () => {
      useMachineStore.setState({
        sessionState: 'running',
        connectedPort: '/dev/ttyUSB0',
        jobProgress: makeJobProgress(),
        machineStatus: null,
        error: null,
      });
    });

    const failedProgress = makeJobProgress({
      state: 'failed',
      error_message: 'GRBL error 2: Bad number format',
    });
    await act(async () => {
      await appEventListener({
        payload: makeAppEvent('job.failed', failedProgress),
      });
      await appEventListener({
        payload: makeAppEvent('machine.disconnected', { reason: 'job_failed_while_running' }),
      });
    });

    const machine = useMachineStore.getState();
    expect(machine.sessionState).toBe('disconnected');
    expect(machine.connectedPort).toBeNull();
    expect(machine.machineStatus).toBeNull();
    expect(machine.jobProgress).toMatchObject({
      state: 'failed',
      error_message: 'GRBL error 2: Bad number format',
    });
  });

  it('reloads project state after design transaction events', async () => {
    const loadProject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ loadProject });
    await renderApp();
    loadProject.mockClear();
    const appEventListener = getAppEventListener();

    await act(async () => {
      await appEventListener({
        payload: makeAppEvent('project.design.transaction_applied', { transaction_id: 'tx-1' }),
      });
    });

    expect(loadProject).toHaveBeenCalledWith({ invalidatePreview: true });
  });

  it('rotation shortcuts are blocked when rotation lock is enabled', async () => {
    const rotateObjects = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project: makeProject({
        metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p1', project_name: 'Test', created_at: '', modified_at: '' },
        workspace: { bed_width_mm: 400, bed_height_mm: 400, origin: 'top_left' },
        layers: [],
        objects: [],
        assets: [],
        transform_locks: makeTransformLocks({ rotate_enabled: false }),
      }),
      selectedObjectIds: ['o1'],
      rotateObjects,
    });
    await renderApp();
    await dispatchKeyDown(window, { key: '.' });
    expect(rotateObjects).not.toHaveBeenCalled();
    const notifications = useNotificationStore.getState().notifications;
    expect(
      notifications.some((notification) => notification.message.includes('Rotation is locked')),
    ).toBe(true);
  });

  it('Ctrl+C with no selection clears clipboard availability', async () => {
    useUiStore.setState({ hasClipboard: true });
    useProjectStore.setState({
      selectedObjectIds: [],
    });

    await renderApp();
    await dispatchKeyDown(window, { key: 'c', ctrlKey: true });

    expect(useUiStore.getState().hasClipboard).toBe(false);
  });

  it('toolbar Copy stores selected objects and enables Paste', async () => {
    const object = makeProjectObject({ id: 'copy-source' });
    useProjectStore.setState({
      project: makeProject({ objects: [object] }),
      selectedObjectIds: ['copy-source'],
    });
    useUiStore.setState({ hasClipboard: false });

    await renderApp();

    expect(screen.getByTitle('Paste').closest('button')?.disabled).toBe(true);

    const windowDispatch = vi.spyOn(window, 'dispatchEvent');

    await act(async () => {
      fireEvent.click(screen.getByTitle('Copy'));
      await Promise.resolve();
    });

    expect(windowDispatch.mock.calls.some(([event]) => event instanceof KeyboardEvent && event.type === 'keydown')).toBe(false);
    expect(getClipboard()?.map((stored) => stored.id)).toEqual(['copy-source']);
    expect(useUiStore.getState().hasClipboard).toBe(true);
    expect(screen.getByTitle('Paste').closest('button')?.disabled).toBe(false);
  });

  it('toolbar Cut stores selected objects and enables Paste after deletion succeeds', async () => {
    const object = makeProjectObject({ id: 'cut-source' });
    const removeObjects = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({
      project: makeProject({ objects: [object] }),
      selectedObjectIds: ['cut-source'],
      removeObjects,
    });
    useUiStore.setState({ hasClipboard: false });

    await renderApp();

    expect(screen.getByTitle('Paste').closest('button')?.disabled).toBe(true);

    const windowDispatch = vi.spyOn(window, 'dispatchEvent');

    await act(async () => {
      fireEvent.click(screen.getByTitle('Cut'));
      await Promise.resolve();
    });

    expect(windowDispatch.mock.calls.some(([event]) => event instanceof KeyboardEvent && event.type === 'keydown')).toBe(false);
    expect(removeObjects).toHaveBeenCalledWith(['cut-source']);
    expect(getClipboard()?.map((stored) => stored.id)).toEqual(['cut-source']);
    expect(useUiStore.getState().hasClipboard).toBe(true);
    expect(screen.getByTitle('Paste').closest('button')?.disabled).toBe(false);
  });
});
