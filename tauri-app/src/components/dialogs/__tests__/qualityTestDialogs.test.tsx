import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';

import { FocusTestDialog } from '../FocusTestDialog';
import { IntervalTestDialog } from '../IntervalTestDialog';
import { MaterialTestDialog } from '../MaterialTestDialog';
import { useMachineStore } from '../../../stores/machineStore';
import { useProjectStore } from '../../../stores/projectStore';
import {
  DEFAULT_QUALITY_TEST_SETTINGS,
  type MachineRunState,
  type MachineProfile,
  type QualityTestRequest,
  type SessionState,
} from '../../../types/machine';

const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => mockInvoke(...args) }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockReturnValue(new Promise(() => {})),
}));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(), save: vi.fn() }));

// PreviewWindow uses canvas + Path2D; jsdom needs both mocked or the mount throws.
const mockCtx = {
  clearRect: vi.fn(),
  drawImage: vi.fn(),
  fillRect: vi.fn(),
  beginPath: vi.fn(),
  moveTo: vi.fn(),
  lineTo: vi.fn(),
  closePath: vi.fn(),
  stroke: vi.fn(),
  fill: vi.fn(),
  save: vi.fn(),
  restore: vi.fn(),
  translate: vi.fn(),
  scale: vi.fn(),
  rotate: vi.fn(),
  measureText: vi.fn().mockReturnValue({ width: 0 }),
  fillText: vi.fn(),
  setTransform: vi.fn(),
  getTransform: vi.fn().mockReturnValue({ a: 1, b: 0, c: 0, d: 1, e: 0, f: 0 }),
  set fillStyle(_v: string) {},
  set strokeStyle(_v: string) {},
  set lineWidth(_v: number) {},
  set font(_v: string) {},
  set textAlign(_v: string) {},
  set textBaseline(_v: string) {},
  set globalAlpha(_v: number) {},
};
HTMLCanvasElement.prototype.getContext = vi.fn().mockReturnValue(mockCtx);
globalThis.Path2D = vi
  .fn()
  .mockImplementation(() => ({ rect: vi.fn(), addPath: vi.fn() })) as never;
if (typeof globalThis.ResizeObserver === 'undefined') {
  globalThis.ResizeObserver = vi.fn().mockImplementation(() => ({
    observe: vi.fn(),
    unobserve: vi.fn(),
    disconnect: vi.fn(),
  })) as never;
}

const initialProjectState = useProjectStore.getState();

function makeProfile(overrides: Partial<MachineProfile> = {}): MachineProfile {
  return {
    id: 'profile-1',
    name: 'Test',
    bed_width_mm: 200,
    bed_height_mm: 200,
    max_speed_mm_min: 3000,
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
    use_g0_for_overscan: false,
    scanning_offsets: [],
    enable_scanning_offset: false,
    dot_width_mm: 0,
    enable_dot_width: false,
    supports_z_moves: false,
    z_move_feed_mm_min: 300,
    quality_test_settings: DEFAULT_QUALITY_TEST_SETTINGS,
    ...overrides,
  };
}

function seedProjectWithMaterialHeight(material_height_mm: number | null) {
  useProjectStore.setState({
    project: {
      metadata: {
        format_version: '1',
        app_version: '1',
        project_id: 'p',
        project_name: 'P',
        created_at: '',
        modified_at: '',
      },
      workspace: { bed_width_mm: 200, bed_height_mm: 200, origin: 'top_left' },
      layers: [],
      objects: [],
      assets: [],
      machine_profile_id: null,
      machine_profile_snapshot: null,
      notes: '',
      start_from: 'absolute_coords',
      job_origin: 'center',
      user_origin: null,
      transform_locks: { x: false, y: false, rotation: false, scale: false },
      material_height_mm,
    } as never,
  });
}

function seedActiveProfile(profile: MachineProfile) {
  useMachineStore.setState({
    profiles: [profile],
    activeProfileId: profile.id,
    // Live actions (Frame/Start) require a ready, idle machine.
    sessionState: 'ready',
    machineStatus: {
      run_state: 'idle',
      machine_position: { x: 0, y: 0, z: 0 },
      work_position: { x: 0, y: 0, z: 0 },
      feed_rate: 0,
      spindle_speed: 0,
      feed_override: 100,
      spindle_override: 100,
      rapid_override: 100,
      pin_states: '',
    },
  });
}

afterEach(() => {
  cleanup();
  mockInvoke.mockReset();
  // Default: every command returns null so MaterialPresetPicker's get_material_presets call (and
  // any other untargeted invoke) doesn't crash later tests.
  mockInvoke.mockResolvedValue(null);
  vi.restoreAllMocks();
  useMachineStore.setState({
    profiles: [],
    activeProfileId: null,
    jobProgress: null,
    sessionState: 'disconnected',
    machineStatus: null,
  });
  useProjectStore.setState(initialProjectState, true);
});

function makePreviewResponse(extra?: Partial<{ warnings: unknown[] }>) {
  return {
    preview: {
      plan_id: 'p',
      revision_hash: 'r',
      bounds: { min: { x: 0, y: 0 }, max: { x: 50, y: 50 } },
      layers: [],
      travel_moves: [],
      failed_entries: [],
      frame: null,
      stats: {
        segment_count: 25,
        total_distance_mm: 100,
        travel_distance_mm: 10,
        burn_distance_mm: 90,
        estimated_duration_secs: 5,
        raster_line_count: 0,
      },
      warnings: [],
    },
    warnings: extra?.warnings ?? [],
  };
}

/** Build an invoke router that returns sensible defaults so MaterialPresetPicker doesn't crash
 *  during unrelated tests. Tests can override individual commands via `overrides`. */
function setupInvokeRouter(overrides: Record<string, unknown> = {}) {
  mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd in overrides) return overrides[cmd];
    if (cmd === 'get_material_presets') return [];
    if (cmd === 'save_machine_profile') return (args as { profile?: MachineProfile }).profile;
    if (cmd === 'get_machine_profiles') return useMachineStore.getState().profiles;
    if (cmd === 'get_app_settings')
      return { active_profile_id: useMachineStore.getState().activeProfileId };
    return null;
  });
}

describe('MaterialTestDialog', () => {
  it('renders axis fields and routes Preview through quality_test_preview', async () => {
    seedActiveProfile(makeProfile());
    setupInvokeRouter({ quality_test_preview: makePreviewResponse() });
    render(<MaterialTestDialog onClose={vi.fn()} />);
    expect(screen.getByText('Material Test')).toBeDefined();
    expect(screen.getByText('X Axis')).toBeDefined();
    expect(screen.getByText('Y Axis')).toBeDefined();
    expect(screen.getByTestId('qt-material-dialog-drag-handle')).toBeDefined();
    expect(screen.getByTestId('qt-material-dialog-resize-handle')).toBeDefined();

    fireEvent.click(screen.getByTestId('qt-material-preview'));
    await waitFor(() => {
      const call = mockInvoke.mock.calls.find((c) => c[0] === 'quality_test_preview');
      expect(call).toBeTruthy();
      const req = call?.[1] as { request: QualityTestRequest };
      expect(req.request.kind).toBe('material');
      expect(req.request.x_axis).toMatchObject({ param: 'power', count: 5 });
      expect(req.request.y_axis).toMatchObject({ param: 'speed', count: 5 });
    });
  });

  it('switches interval axes to speed for vector operations with inline messaging', async () => {
    seedActiveProfile(makeProfile());
    setupInvokeRouter();
    render(<MaterialTestDialog onClose={vi.fn()} />);
    fireEvent.change(screen.getByTestId('qt-material-x_axis-param'), {
      target: { value: 'interval' },
    });
    fireEvent.change(screen.getByTestId('qt-material-operation'), { target: { value: 'line' } });
    await waitFor(() => {
      expect(screen.getByTestId('qt-material-axis-message').textContent ?? '').toMatch(
        /switched to Speed/,
      );
      expect((screen.getByTestId('qt-material-x_axis-param') as HTMLSelectElement).value).toBe(
        'speed',
      );
    });
  });

  it('hides Create on Canvas while the materialization workflow is shelved', async () => {
    seedActiveProfile(makeProfile());
    setupInvokeRouter();

    render(<MaterialTestDialog onClose={vi.fn()} />);

    expect(screen.queryByTestId('qt-material-create-canvas')).toBeNull();
    expect(
      mockInvoke.mock.calls.some((call) => call[0] === 'quality_test_create_material_on_canvas'),
    ).toBe(false);
  });
});

describe('FocusTestDialog', () => {
  it('disables Frame and Start when the active profile lacks Z support', () => {
    seedActiveProfile(makeProfile({ supports_z_moves: false }));
    seedProjectWithMaterialHeight(5);
    render(<FocusTestDialog onClose={vi.fn()} />);
    expect((screen.getByTestId('qt-focus-frame') as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByTestId('qt-focus-start') as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByTestId('qt-focus-live-gate').textContent ?? '').toMatch(/Z support/);
  });

  it('disables all generated-output actions in absolute mode without material height', () => {
    seedActiveProfile(makeProfile({ supports_z_moves: true }));
    seedProjectWithMaterialHeight(null);
    render(<FocusTestDialog onClose={vi.fn()} />);
    expect((screen.getByTestId('qt-focus-preview') as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByTestId('qt-focus-save') as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByTestId('qt-focus-frame') as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByTestId('qt-focus-start') as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByTestId('qt-focus-live-gate').textContent ?? '').toMatch(/material height/);
  });

  it('allows preview/save in relative mode without an open project', () => {
    seedActiveProfile({
      ...makeProfile({ supports_z_moves: true }),
      quality_test_settings: {
        ...DEFAULT_QUALITY_TEST_SETTINGS,
        focus: {
          ...DEFAULT_QUALITY_TEST_SETTINGS.focus,
          mode: 'RelativeTemporary',
        },
      },
    });
    render(<FocusTestDialog onClose={vi.fn()} />);
    expect((screen.getByTestId('qt-focus-preview') as HTMLButtonElement).disabled).toBe(false);
    expect((screen.getByTestId('qt-focus-save') as HTMLButtonElement).disabled).toBe(false);
  });

  it('enables Frame/Start when Z support is on and material height is set', async () => {
    seedActiveProfile(makeProfile({ supports_z_moves: true }));
    seedProjectWithMaterialHeight(5);
    render(<FocusTestDialog onClose={vi.fn()} />);
    expect((screen.getByTestId('qt-focus-frame') as HTMLButtonElement).disabled).toBe(false);
    expect((screen.getByTestId('qt-focus-start') as HTMLButtonElement).disabled).toBe(false);
  });

  it('does not close on backdrop click', () => {
    const onClose = vi.fn();
    seedActiveProfile(makeProfile({ supports_z_moves: true }));
    seedProjectWithMaterialHeight(5);
    render(<FocusTestDialog onClose={onClose} />);

    fireEvent.click(screen.getByTestId('qt-focus-dialog-backdrop'));

    expect(onClose).not.toHaveBeenCalled();
  });
});

describe('Profile switching', () => {
  it('re-seeds local state when active profile changes (no clobber on switch)', async () => {
    setupInvokeRouter();
    const profileA = makeProfile({
      id: 'A',
      quality_test_settings: {
        ...DEFAULT_QUALITY_TEST_SETTINGS,
        material: {
          ...DEFAULT_QUALITY_TEST_SETTINGS.material,
          x_axis: { ...DEFAULT_QUALITY_TEST_SETTINGS.material.x_axis, count: 3 },
        },
      },
    });
    const profileB = makeProfile({
      id: 'B',
      quality_test_settings: {
        ...DEFAULT_QUALITY_TEST_SETTINGS,
        material: {
          ...DEFAULT_QUALITY_TEST_SETTINGS.material,
          x_axis: { ...DEFAULT_QUALITY_TEST_SETTINGS.material.x_axis, count: 7 },
        },
      },
    });
    useMachineStore.setState({ profiles: [profileA, profileB], activeProfileId: 'A' });

    const { rerender } = render(<MaterialTestDialog onClose={vi.fn()} />);
    // X-axis count input should reflect profile A.
    const colsInputA = screen.getByDisplayValue('3');
    expect(colsInputA).toBeDefined();

    // Switch active profile while the dialog stays mounted.
    useMachineStore.setState({ profiles: [profileA, profileB], activeProfileId: 'B' });
    rerender(<MaterialTestDialog onClose={vi.fn()} />);

    await waitFor(() => {
      // Local state must re-seed to profile B's count (7), not stay at profile A's (3).
      expect(screen.getByDisplayValue('7')).toBeDefined();
    });
    // And profile A's stored settings must not have been overwritten — verify by checking that
    // no save_machine_profile invoke has been called for profile A with x_axis.count === 7.
    const offendingCalls = mockInvoke.mock.calls.filter(
      (c) =>
        c[0] === 'save_machine_profile' &&
        (
          c[1] as {
            profile?: {
              id?: string;
              quality_test_settings?: { material?: { x_axis?: { count?: number } } };
            };
          }
        )?.profile?.id === 'A' &&
        (
          c[1] as {
            profile?: { quality_test_settings?: { material?: { x_axis?: { count?: number } } } };
          }
        )?.profile?.quality_test_settings?.material?.x_axis?.count === 7,
    );
    expect(offendingCalls).toHaveLength(0);
  });
});

describe('Material preset picker', () => {
  it('routes through apply_material_preset_to_seed and updates dialog defaults without project mutation', async () => {
    seedActiveProfile(makeProfile());
    // get_material_presets → returns one preset; apply returns updated entry.
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'get_material_presets') {
        return [
          {
            id: 'p1',
            name: '3mm ply / cut',
            material: 'plywood',
            thickness_mm: 3,
            operation: 'cut',
            speed_mm_min: 800,
            power_percent: 80,
            passes: 1,
            notes: '',
            category: '',
          },
        ];
      }
      if (cmd === 'apply_material_preset_to_seed') {
        const a = args as {
          presetId: string;
          seed: { operation: string; raster_settings: unknown };
        };
        // Must hit transient path, never the project-mutating one.
        expect(a.presetId).toBe('p1');
        expect(a.seed.operation).toBe('fill');
        // Seed for fill MUST carry raster_settings so the backend's pure helper has a bag to
        // mutate. A null here would silently drop preset interval/DPI/scan-angle (regression
        // for Interval Test picker).
        expect(a.seed.raster_settings).not.toBeNull();
        return [
          {
            id: crypto.randomUUID(),
            operation: 'fill',
            speed_mm_min: 800,
            power_percent: 80,
            raster_settings: { line_interval_mm: 0.1, dpi: 254 },
            vector_settings: null,
            air_assist: false,
            power_min_percent: 0,
            z_offset_mm: 0,
            gcode_prefix: '',
            gcode_suffix: '',
            output_enabled: true,
          },
          [],
        ];
      }
      return null;
    });

    render(<MaterialTestDialog onClose={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByTestId('qt-preset-picker')).toBeDefined();
    });
    fireEvent.change(screen.getByTestId('qt-preset-picker'), { target: { value: 'p1' } });

    await waitFor(() => {
      const calls = mockInvoke.mock.calls.filter((c) => c[0] === 'apply_material_preset_to_seed');
      expect(calls.length).toBe(1);
    });
    // Must NOT have called the project-mutating apply path.
    const layerMutating = mockInvoke.mock.calls.filter((c) => c[0] === 'apply_material_preset');
    expect(layerMutating).toHaveLength(0);

    // Speed/power axes should reflect derived defaults: speed 400..1200, power 50..100.
    await waitFor(() => {
      expect(screen.getByDisplayValue('400')).toBeDefined();
      expect(screen.getByDisplayValue('1200')).toBeDefined();
      expect(screen.getAllByDisplayValue('50').length).toBeGreaterThan(0);
      expect(screen.getAllByDisplayValue('100').length).toBeGreaterThan(0);
    });
  });

  it('IntervalTestDialog derives interval range from preset line_interval_mm', async () => {
    seedActiveProfile(makeProfile());
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'get_material_presets') {
        return [
          {
            id: 'p2',
            name: 'fill 0.1mm',
            material: 'paper',
            thickness_mm: 0.1,
            operation: 'fill',
            speed_mm_min: 6000,
            power_percent: 25,
            passes: 1,
            line_interval_mm: 0.1,
            notes: '',
            category: '',
          },
        ];
      }
      if (cmd === 'apply_material_preset_to_seed') {
        const a = args as { seed: { raster_settings: unknown } };
        // Real backend behavior: seed.raster_settings is populated → mutator runs → returns entry
        // with line_interval_mm set. The bug was: makeSeedCutEntry left this null, so the mutator
        // skipped and the dialog couldn't read line_interval_mm from the response.
        expect(a.seed.raster_settings).not.toBeNull();
        return [
          {
            id: crypto.randomUUID(),
            operation: 'fill',
            speed_mm_min: 6000,
            power_percent: 25,
            raster_settings: { line_interval_mm: 0.1, dpi: 254 },
            vector_settings: null,
            air_assist: false,
            power_min_percent: 0,
            z_offset_mm: 0,
            gcode_prefix: '',
            gcode_suffix: '',
            output_enabled: true,
          },
          [],
        ];
      }
      return null;
    });

    render(<IntervalTestDialog onClose={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByTestId('qt-preset-picker')).toBeDefined();
    });
    fireEvent.change(screen.getByTestId('qt-preset-picker'), { target: { value: 'p2' } });

    // Range derived as [interval * 0.5, interval * 1.5] → [0.05, 0.15].
    await waitFor(() => {
      expect(screen.getByDisplayValue('0.05')).toBeDefined();
      expect(screen.getByDisplayValue('0.15')).toBeDefined();
      expect(screen.getByText('508 DPI')).toBeDefined();
      expect(screen.getByText('169 DPI')).toBeDefined();
      expect(screen.getByDisplayValue('6000')).toBeDefined();
      expect(screen.getByDisplayValue('25')).toBeDefined();
    });
  });
});

describe('Material Test recipes and job controls', () => {
  it('saves, loads, deletes, exports, and imports recipes without material preset APIs', async () => {
    const profile = makeProfile();
    seedActiveProfile(profile);
    const { open, save } = await import('@tauri-apps/plugin-dialog');
    (save as unknown as { mockResolvedValueOnce: (v: unknown) => void }).mockResolvedValueOnce(
      '/tmp/recipes.json',
    );
    (open as unknown as { mockResolvedValueOnce: (v: unknown) => void }).mockResolvedValueOnce(
      '/tmp/recipes.json',
    );
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'get_material_presets') return [];
      if (cmd === 'save_machine_profile') {
        const next = (args as { profile: MachineProfile }).profile;
        useMachineStore.setState({ profiles: [next], activeProfileId: next.id });
        return next;
      }
      if (cmd === 'get_machine_profiles') return useMachineStore.getState().profiles;
      if (cmd === 'get_app_settings')
        return { active_profile_id: useMachineStore.getState().activeProfileId };
      if (cmd === 'export_material_test_recipes') return null;
      if (cmd === 'import_material_test_recipes') {
        return [
          {
            id: 'recipe-imported',
            name: 'Imported',
            settings: DEFAULT_QUALITY_TEST_SETTINGS.material,
          },
        ];
      }
      return null;
    });

    render(<MaterialTestDialog onClose={vi.fn()} />);
    fireEvent.change(screen.getByTestId('qt-material-recipe-name'), {
      target: { value: 'Plywood grid' },
    });
    fireEvent.click(screen.getByTestId('qt-material-recipe-save'));

    await waitFor(() => {
      expect(
        useMachineStore.getState().profiles[0].quality_test_settings?.material_recipes?.[0]?.name,
      ).toBe('Plywood grid');
    });

    fireEvent.change(screen.getByTestId('qt-material-recipe-select'), {
      target: {
        value:
          useMachineStore.getState().profiles[0].quality_test_settings?.material_recipes?.[0]?.id,
      },
    });
    fireEvent.click(screen.getByTestId('qt-material-recipe-load'));
    fireEvent.click(screen.getByTestId('qt-material-recipe-export'));
    await waitFor(() => {
      expect(mockInvoke.mock.calls.some((c) => c[0] === 'export_material_test_recipes')).toBe(true);
    });

    fireEvent.click(screen.getByTestId('qt-material-recipe-import'));
    await waitFor(() => {
      expect(mockInvoke.mock.calls.some((c) => c[0] === 'import_material_test_recipes')).toBe(true);
    });

    fireEvent.change(screen.getByTestId('qt-material-recipe-select'), {
      target: { value: 'recipe-imported' },
    });
    fireEvent.click(screen.getByTestId('qt-material-recipe-delete'));
    await waitFor(() => {
      const calls = mockInvoke.mock.calls.map((c) => c[0]);
      expect(calls).not.toContain('apply_material_preset');
    });
  });

  it('renders idle/running/paused controls and calls Start/Pause/Resume/Stop services', async () => {
    seedActiveProfile(makeProfile());
    setupInvokeRouter({
      quality_test_start: {
        state: 'running',
        total_lines: 10,
        queued_lines: 0,
        sent_lines: 1,
        acknowledged_lines: 0,
        elapsed_secs: 0,
        estimated_remaining_secs: 5,
        buffer_fill_bytes: 0,
      },
    });

    render(<MaterialTestDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('qt-material-start'));
    await waitFor(() => {
      expect(mockInvoke.mock.calls.some((c) => c[0] === 'quality_test_start')).toBe(true);
      expect(screen.getByTestId('qt-material-pause')).toBeDefined();
      expect(screen.getByTestId('qt-material-stop')).toBeDefined();
    });

    fireEvent.click(screen.getByTestId('qt-material-pause'));
    await waitFor(() => {
      expect(mockInvoke.mock.calls.some((c) => c[0] === 'pause_job')).toBe(true);
      expect(screen.getByTestId('qt-material-resume')).toBeDefined();
    });

    fireEvent.click(screen.getByTestId('qt-material-resume'));
    await waitFor(() => {
      expect(mockInvoke.mock.calls.some((c) => c[0] === 'resume_job')).toBe(true);
      expect(screen.getByTestId('qt-material-pause')).toBeDefined();
    });

    fireEvent.click(screen.getByTestId('qt-material-stop'));
    await waitFor(() => {
      expect(mockInvoke.mock.calls.some((c) => c[0] === 'cancel_job')).toBe(true);
      expect(screen.getByTestId('qt-material-start')).toBeDefined();
    });
  });

  it('disables Frame and Start while disconnected, keeping Preview and Save available', async () => {
    seedActiveProfile(makeProfile());
    useMachineStore.setState({ sessionState: 'disconnected' });

    render(<MaterialTestDialog onClose={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByTestId('qt-material-live-gate')).toBeDefined();
    });
    expect((screen.getByTestId('qt-material-frame') as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByTestId('qt-material-start') as HTMLButtonElement).disabled).toBe(true);
    // Offline actions stay usable.
    expect((screen.getByTestId('qt-material-preview') as HTMLButtonElement).disabled).toBe(false);
    expect((screen.getByTestId('qt-material-save') as HTMLButtonElement).disabled).toBe(false);
  });

  it('disables Frame and Start in every non-ready session state', async () => {
    const nonReadyStates: SessionState[] = [
      'disconnected',
      'connecting',
      'transport_open',
      'waiting_for_banner',
      'validating',
      'running',
      'paused',
      'alarm',
      'error',
    ];
    seedActiveProfile(makeProfile());
    setupInvokeRouter();

    await act(async () => {
      render(<MaterialTestDialog onClose={vi.fn()} />);
      await Promise.resolve();
    });

    for (const sessionState of nonReadyStates) {
      act(() => useMachineStore.setState({ sessionState }));

      expect(
        (screen.getByTestId('qt-material-frame') as HTMLButtonElement).disabled,
        `Frame should be disabled while the session is ${sessionState}`,
      ).toBe(true);
      expect(
        (screen.getByTestId('qt-material-start') as HTMLButtonElement).disabled,
        `Start should be disabled while the session is ${sessionState}`,
      ).toBe(true);
      expect((screen.getByTestId('qt-material-preview') as HTMLButtonElement).disabled).toBe(false);
      expect((screen.getByTestId('qt-material-save') as HTMLButtonElement).disabled).toBe(false);
    }
  });

  it('disables Frame and Start for every non-idle machine state', async () => {
    const nonIdleStates: Exclude<MachineRunState, 'idle'>[] = [
      'run',
      'hold',
      'jog',
      'home',
      'alarm',
      'door',
      'sleep',
      'check',
      'unknown',
    ];
    seedActiveProfile(makeProfile());
    setupInvokeRouter();

    await act(async () => {
      render(<MaterialTestDialog onClose={vi.fn()} />);
      await Promise.resolve();
    });

    for (const runState of nonIdleStates) {
      act(() => {
        useMachineStore.setState((state) => ({
          machineStatus: state.machineStatus
            ? { ...state.machineStatus, run_state: runState }
            : null,
        }));
      });

      expect(
        (screen.getByTestId('qt-material-frame') as HTMLButtonElement).disabled,
        `Frame should be disabled while the machine is ${runState}`,
      ).toBe(true);
      expect(
        (screen.getByTestId('qt-material-start') as HTMLButtonElement).disabled,
        `Start should be disabled while the machine is ${runState}`,
      ).toBe(true);
    }
  });

  it('disables Frame and Start when no machine profile is active', () => {
    useMachineStore.setState({
      profiles: [],
      activeProfileId: null,
      sessionState: 'ready',
    });

    render(<MaterialTestDialog onClose={vi.fn()} />);

    expect((screen.getByTestId('qt-material-frame') as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByTestId('qt-material-start') as HTMLButtonElement).disabled).toBe(true);
  });

  it('shows Stop (not Start or Pause) while a staged transfer is Preparing', async () => {
    seedActiveProfile(makeProfile());
    useMachineStore.setState({
      jobProgress: {
        state: 'preparing',
        total_lines: 10,
        queued_lines: 6,
        sent_lines: 4,
        acknowledged_lines: 4,
        elapsed_secs: 0,
        estimated_remaining_secs: 0,
        buffer_fill_bytes: 0,
        error_message: null,
        buckets: [],
      },
    });

    render(<MaterialTestDialog onClose={vi.fn()} />);

    // A Preparing job is live: Stop must be reachable (Lihuiyu packets may
    // already be executing), while Start and Pause must not appear.
    await waitFor(() => {
      expect(screen.getByTestId('qt-material-stop')).toBeDefined();
    });
    expect(screen.queryByTestId('qt-material-start')).toBeNull();
    expect(screen.queryByTestId('qt-material-pause')).toBeNull();
  });
});

describe('IntervalTestDialog', () => {
  it('builds an interval-kind request when Save is clicked', async () => {
    seedActiveProfile(makeProfile());
    const { save } = await import('@tauri-apps/plugin-dialog');
    (save as unknown as { mockResolvedValueOnce: (v: unknown) => void }).mockResolvedValueOnce(
      '/tmp/quality.gcode',
    );
    setupInvokeRouter({
      quality_test_export_gcode: { path: '/tmp/quality.gcode', warnings: [] },
    });
    render(<IntervalTestDialog onClose={vi.fn()} />);
    expect(screen.getByText('Samples')).toBeDefined();
    fireEvent.click(screen.getByTestId('qt-interval-save'));
    await waitFor(() => {
      const call = mockInvoke.mock.calls.find((c) => c[0] === 'quality_test_export_gcode');
      expect(call).toBeTruthy();
      const req = call?.[1] as { request: QualityTestRequest; path: string };
      expect(req.request.kind).toBe('interval');
      expect(req.path).toBe('/tmp/quality.gcode');
    });
  });
});
