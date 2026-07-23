import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { usePreviewStore } from './previewStore';
import { useProjectStore } from './projectStore';
import type { PreviewData } from '../types/preview';
import {
  makeLayer,
  makeProject,
  makeProjectObject,
  makeRasterSettings,
} from '../test-utils/projectFixtures';

// Mock previewService
vi.mock('../services/previewService', () => ({
  previewService: {
    generatePreview: vi.fn(),
    generatePlan: vi.fn(),
    getPlanStats: vi.fn(),
    cancelPlanning: vi.fn(),
    exportGcode: vi.fn(),
  },
}));

import { previewService } from '../services/previewService';

function makeVectorPreview(overrides = {}) {
  return {
    points: [{ x: 0, y: 0 }, { x: 10, y: 0 }],
    closed: false,
    power_percent: 80,
    speed_mm_min: 600,
    sequence: 1,
    ...overrides,
  };
}

function makeRasterPreview(overrides = {}) {
  return {
    bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
    line_count: 10,
    line_interval_mm: 0.1,
    direction_mode: 'bidirectional' as const,
    power_mode: 'binary' as const,
    speed_mm_min: 1200,
    fill_density: 0.5,
    scan_angle_deg: 0,
    scan_origin: { x: 0, y: 0 },
    overscan_mm: 0,
    outlines: [],
    scan_axis: 'horizontal' as const,
    sequence: 2,
    duration_secs: 0,
    avg_power_normalized: 0,
    local_origin_mm: { x: 0, y: 0 },
    local_width_mm: 10,
    local_height_mm: 10,
    run_extents: [],
    overscan_run_extents: [],
    ...overrides,
  };
}

function makeTravelMove(overrides = {}) {
  return {
    from: { x: 0, y: 0 },
    to: { x: 5, y: 5 },
    sequence: 0,
    ...overrides,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

async function flushPreviewGenerationStart(): Promise<void> {
  await vi.dynamicImportSettled();
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}

const mockPreviewData: PreviewData = {
  plan_id: 'test-plan-id',
  revision_hash: 'abc123',
  bounds: { min: { x: 0, y: 0 }, max: { x: 100, y: 100 } },
  layers: [{
    layer_id: 'layer-1',
    vector_paths: [makeVectorPreview()],
    raster_regions: [makeRasterPreview()],
  }],
  travel_moves: [makeTravelMove()],
  frame: null,
  stats: {
    total_distance_mm: 100,
    travel_distance_mm: 20,
    burn_distance_mm: 80,
    estimated_duration_secs: 60,
    segment_count: 5,
    raster_line_count: 0,
  },
  warnings: [],
  failed_entries: [],
};

describe('previewStore', () => {
  beforeEach(() => {
    // Reset store to initial state
    usePreviewStore.setState({
      state: 'idle',
      data: null,
      revisionHash: null,
      error: null,
      showPreview: false,
      canvasPreviewActive: false,
      previewWindowOpen: false,
      previewGenerationDialogVisible: false,
      previewGenerationDialogTitle: 'Generating preview...',
      manualRefreshRequired: false,
      interactionActive: false,
      lastSuccessfulDurationMs: null,
      pendingInteractionRefresh: false,
    });
    useProjectStore.setState({ project: null });
    vi.clearAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('generatePreview transitions idle → generating → current', async () => {
    vi.mocked(previewService.generatePreview).mockResolvedValue(mockPreviewData);

    const promise = usePreviewStore.getState().generatePreview();

    // Should be generating
    expect(usePreviewStore.getState().state).toBe('generating');

    await expect(promise).resolves.toBe(true);

    // Should be current with data
    expect(usePreviewStore.getState().state).toBe('current');
    expect(usePreviewStore.getState().data).toEqual(mockPreviewData);
    expect(usePreviewStore.getState().revisionHash).toBe('abc123');
  });

  it('shows the long offset fill dialog only after the reveal delay', async () => {
    const pending = deferred<PreviewData>();
    vi.mocked(previewService.generatePreview).mockReturnValue(pending.promise);
    useProjectStore.setState({
      project: makeProject({
        layers: [makeLayer({
          operation: 'offset_fill',
          raster_settings: makeRasterSettings({ line_interval_mm: 0.1 }),
        })],
        objects: [makeProjectObject({
          bounds: { min: { x: 0, y: 0 }, max: { x: 60, y: 60 } },
        })],
      }),
    });

    const promise = usePreviewStore.getState().generatePreview();
    await flushPreviewGenerationStart();
    expect(previewService.generatePreview).toHaveBeenCalled();

    expect(usePreviewStore.getState().previewGenerationDialogVisible).toBe(false);
    await vi.advanceTimersByTimeAsync(299);
    expect(usePreviewStore.getState().previewGenerationDialogVisible).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    expect(usePreviewStore.getState().previewGenerationDialogVisible).toBe(true);
    expect(usePreviewStore.getState().previewGenerationDialogTitle).toBe('Generating offset fills...');

    pending.resolve(mockPreviewData);
    await expect(promise).resolves.toBe(true);
    expect(usePreviewStore.getState().previewGenerationDialogVisible).toBe(false);
  });

  it('shows the general preview generation dialog after the reveal delay for non-offset work', async () => {
    const pending = deferred<PreviewData>();
    vi.mocked(previewService.generatePreview).mockReturnValue(pending.promise);

    const promise = usePreviewStore.getState().generatePreview();
    await flushPreviewGenerationStart();
    expect(previewService.generatePreview).toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(300);

    expect(usePreviewStore.getState().previewGenerationDialogVisible).toBe(true);
    expect(usePreviewStore.getState().previewGenerationDialogTitle).toBe('Generating preview...');

    pending.resolve(mockPreviewData);
    await expect(promise).resolves.toBe(true);
    expect(usePreviewStore.getState().previewGenerationDialogVisible).toBe(false);
  });

  it('does not flash the long offset fill dialog for quick resolved previews', async () => {
    vi.mocked(previewService.generatePreview).mockResolvedValue(mockPreviewData);
    useProjectStore.setState({
      project: makeProject({
        layers: [makeLayer({
          operation: 'offset_fill',
          raster_settings: makeRasterSettings({ line_interval_mm: 0.1 }),
        })],
        objects: [makeProjectObject({
          bounds: { min: { x: 0, y: 0 }, max: { x: 60, y: 60 } },
        })],
      }),
    });

    await expect(usePreviewStore.getState().generatePreview()).resolves.toBe(true);
    await vi.advanceTimersByTimeAsync(300);

    expect(usePreviewStore.getState().previewGenerationDialogVisible).toBe(false);
  });

  it('clears a pending long offset fill reveal when cancelled before the delay', async () => {
    const pending = deferred<PreviewData>();
    vi.mocked(previewService.generatePreview).mockReturnValue(pending.promise);
    vi.mocked(previewService.cancelPlanning).mockResolvedValue();
    useProjectStore.setState({
      project: makeProject({
        layers: [makeLayer({
          operation: 'offset_fill',
          raster_settings: makeRasterSettings({ line_interval_mm: 0.1 }),
        })],
        objects: [makeProjectObject({
          bounds: { min: { x: 0, y: 0 }, max: { x: 60, y: 60 } },
        })],
      }),
    });

    const promise = usePreviewStore.getState().generatePreview();
    await flushPreviewGenerationStart();
    expect(previewService.generatePreview).toHaveBeenCalled();

    await usePreviewStore.getState().cancelPreviewGeneration();
    await vi.advanceTimersByTimeAsync(300);

    expect(usePreviewStore.getState().previewGenerationDialogVisible).toBe(false);
    pending.resolve(mockPreviewData);
    await expect(promise).resolves.toBe(false);
  });

  it('rapid back-to-back previews do not leak a stale long offset fill reveal timer', async () => {
    const first = deferred<PreviewData>();
    const second = deferred<PreviewData>();
    vi.mocked(previewService.generatePreview)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    useProjectStore.setState({
      project: makeProject({
        layers: [makeLayer({
          operation: 'offset_fill',
          raster_settings: makeRasterSettings({ line_interval_mm: 0.1 }),
        })],
        objects: [makeProjectObject({
          bounds: { min: { x: 0, y: 0 }, max: { x: 60, y: 60 } },
        })],
      }),
    });

    const firstCall = usePreviewStore.getState().generatePreview();
    await flushPreviewGenerationStart();
    expect(previewService.generatePreview).toHaveBeenCalledTimes(1);

    const secondCall = usePreviewStore.getState().generatePreview();
    await flushPreviewGenerationStart();
    expect(previewService.generatePreview).toHaveBeenCalledTimes(2);

    await vi.advanceTimersByTimeAsync(299);
    expect(usePreviewStore.getState().previewGenerationDialogVisible).toBe(false);
    first.resolve({ ...mockPreviewData, plan_id: 'stale-plan' });
    await expect(firstCall).resolves.toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    expect(usePreviewStore.getState().previewGenerationDialogVisible).toBe(true);

    second.resolve({ ...mockPreviewData, plan_id: 'fresh-plan' });
    await expect(secondCall).resolves.toBe(true);
    expect(usePreviewStore.getState().previewGenerationDialogVisible).toBe(false);
  });

  it('generatePreview failure transitions to error', async () => {
    vi.mocked(previewService.generatePreview).mockRejectedValue(new Error('Plan failed'));

    await expect(usePreviewStore.getState().generatePreview()).resolves.toBe(false);

    expect(usePreviewStore.getState().state).toBe('error');
    expect(usePreviewStore.getState().error).toContain('Plan failed');
  });

  it('empty project preview failure quietly clears preview mode instead of erroring', async () => {
    vi.mocked(previewService.generatePreview).mockRejectedValue(
      new Error('Plan generation failed: Cannot build plan from empty project'),
    );

    usePreviewStore.setState({
      state: 'stale',
      data: mockPreviewData,
      revisionHash: mockPreviewData.revision_hash,
      showPreview: true,
      previewWindowOpen: true,
      manualRefreshRequired: false,
      interactionActive: false,
      lastSuccessfulDurationMs: null,
      pendingInteractionRefresh: false,
    });

    await expect(usePreviewStore.getState().generatePreview()).resolves.toBe(false);

    expect(usePreviewStore.getState().state).toBe('idle');
    expect(usePreviewStore.getState().data).toBeNull();
    expect(usePreviewStore.getState().error).toBeNull();
    expect(usePreviewStore.getState().showPreview).toBe(false);
    expect(usePreviewStore.getState().previewWindowOpen).toBe(false);
  });

  it('cancelled preview generation is a quiet stale state', async () => {
    vi.mocked(previewService.generatePreview).mockRejectedValue(
      new Error('Plan generation cancelled'),
    );

    await expect(usePreviewStore.getState().generatePreview()).resolves.toBe(false);

    expect(usePreviewStore.getState().state).toBe('stale');
    expect(usePreviewStore.getState().error).toBeNull();
  });

  it('cancelPreviewGeneration calls the backend cancellation command', async () => {
    vi.mocked(previewService.cancelPlanning).mockResolvedValue();

    await expect(usePreviewStore.getState().cancelPreviewGeneration()).resolves.toBeUndefined();

    expect(previewService.cancelPlanning).toHaveBeenCalled();
    expect(usePreviewStore.getState().state).toBe('stale');
  });

  it('invalidate sets stale when not idle', () => {
    // Set state to current first
    usePreviewStore.setState({ state: 'current', data: mockPreviewData });

    usePreviewStore.getState().invalidate();

    expect(usePreviewStore.getState().state).toBe('stale');
  });

  it('invalidate does nothing when idle', () => {
    usePreviewStore.getState().invalidate();
    expect(usePreviewStore.getState().state).toBe('idle');
  });

  it('togglePreview opens immediately when preview data already exists', () => {
    expect(usePreviewStore.getState().previewWindowOpen).toBe(false);
    usePreviewStore.setState({ state: 'current', data: mockPreviewData });
    usePreviewStore.getState().togglePreview();
    expect(usePreviewStore.getState().previewWindowOpen).toBe(true);
    usePreviewStore.getState().togglePreview();
    expect(usePreviewStore.getState().previewWindowOpen).toBe(false);
  });

  it('togglePreview waits to open the preview window until first generation succeeds', async () => {
    vi.mocked(previewService.generatePreview).mockResolvedValue(mockPreviewData);

    usePreviewStore.getState().togglePreview();

    expect(usePreviewStore.getState().previewWindowOpen).toBe(false);
    await vi.waitFor(() => {
      expect(usePreviewStore.getState().previewWindowOpen).toBe(true);
    });
    expect(usePreviewStore.getState().data).toEqual(mockPreviewData);
  });

  it('togglePreview waits for regeneration when existing preview data is stale', async () => {
    const freshPreview = {
      ...mockPreviewData,
      plan_id: 'fresh-plan-id',
      revision_hash: 'fresh-revision',
    };
    vi.mocked(previewService.generatePreview).mockResolvedValue(freshPreview);
    usePreviewStore.setState({
      state: 'stale',
      data: mockPreviewData,
      revisionHash: mockPreviewData.revision_hash,
      previewWindowOpen: false,
    });

    usePreviewStore.getState().togglePreview();

    expect(usePreviewStore.getState().previewWindowOpen).toBe(false);
    await vi.waitFor(() => {
      expect(usePreviewStore.getState().previewWindowOpen).toBe(true);
    });
    expect(usePreviewStore.getState().data?.plan_id).toBe('fresh-plan-id');
  });

  it('togglePreview keeps the preview window closed when first generation is cancelled', async () => {
    vi.mocked(previewService.generatePreview).mockRejectedValue(
      new Error('Plan generation cancelled'),
    );

    usePreviewStore.getState().togglePreview();
    await flushPreviewGenerationStart();

    await vi.waitFor(() => {
      expect(usePreviewStore.getState().state).toBe('stale');
    });
    expect(usePreviewStore.getState().previewWindowOpen).toBe(false);
  });

  it('clearPreview resets to idle', () => {
    usePreviewStore.setState({
      state: 'current',
      data: mockPreviewData,
      revisionHash: 'abc',
      error: null,
      previewWindowOpen: true,
      previewGenerationDialogVisible: true,
    });

    usePreviewStore.getState().clearPreview();

    expect(usePreviewStore.getState().state).toBe('idle');
    expect(usePreviewStore.getState().data).toBeNull();
    expect(usePreviewStore.getState().revisionHash).toBeNull();
    expect(usePreviewStore.getState().showPreview).toBe(false);
    expect(usePreviewStore.getState().previewGenerationDialogVisible).toBe(false);
    expect(usePreviewStore.getState().previewGenerationDialogTitle).toBe('Generating preview...');
  });

  it('auto-regenerates when the preview window is open and stale', async () => {
    vi.mocked(previewService.generatePreview).mockResolvedValue(mockPreviewData);

    // Set to current with preview visible
    usePreviewStore.setState({
      state: 'current',
      data: mockPreviewData,
      revisionHash: mockPreviewData.revision_hash,
      previewWindowOpen: true,
    });

    // Invalidate — should debounce
    usePreviewStore.getState().invalidate();
    expect(usePreviewStore.getState().state).toBe('stale');

    // Fast-forward past debounce
    await vi.advanceTimersByTimeAsync(500);

    expect(previewService.generatePreview).toHaveBeenCalled();
  });

  it('no auto-regenerate when hidden and stale', async () => {
    vi.mocked(previewService.generatePreview).mockResolvedValue(mockPreviewData);

    usePreviewStore.setState({
      state: 'current',
      data: mockPreviewData,
      showPreview: false,
    });

    usePreviewStore.getState().invalidate();

    await vi.advanceTimersByTimeAsync(500);

    expect(previewService.generatePreview).not.toHaveBeenCalled();
  });

  it('debounce: rapid mutations only trigger one regeneration', async () => {
    vi.mocked(previewService.generatePreview).mockResolvedValue(mockPreviewData);

    usePreviewStore.setState({
      state: 'current',
      data: mockPreviewData,
      previewWindowOpen: true,
    });

    // Rapid invalidations
    usePreviewStore.getState().invalidate();
    await vi.advanceTimersByTimeAsync(100);
    usePreviewStore.getState().invalidate();
    await vi.advanceTimersByTimeAsync(100);
    usePreviewStore.getState().invalidate();

    // Only one regeneration should fire (after 500ms from last invalidation)
    await vi.advanceTimersByTimeAsync(500);

    expect(previewService.generatePreview).toHaveBeenCalledTimes(1);
  });

  it('togglePreview on when idle triggers generation', async () => {
    vi.mocked(previewService.generatePreview).mockResolvedValue(mockPreviewData);

    usePreviewStore.getState().togglePreview();
    await flushPreviewGenerationStart();

    // Should have triggered generatePreview
    expect(previewService.generatePreview).toHaveBeenCalled();
    await vi.waitFor(() => {
      expect(usePreviewStore.getState().previewWindowOpen).toBe(true);
    });
  });

  it('togglePreview on when error retries generation', async () => {
    vi.mocked(previewService.generatePreview).mockResolvedValue(mockPreviewData);

    usePreviewStore.setState({
      state: 'error',
      error: 'preview failed',
      previewWindowOpen: false,
    });

    usePreviewStore.getState().togglePreview();
    await flushPreviewGenerationStart();

    expect(previewService.generatePreview).toHaveBeenCalledTimes(1);
    await vi.waitFor(() => {
      expect(usePreviewStore.getState().previewWindowOpen).toBe(true);
    });
  });

  it('failed refresh keeps cached preview data intact', async () => {
    vi.mocked(previewService.generatePreview).mockRejectedValue(new Error('preview failed'));

    usePreviewStore.setState({
      state: 'current',
      data: mockPreviewData,
      revisionHash: mockPreviewData.revision_hash,
      previewWindowOpen: true,
    });

    await expect(usePreviewStore.getState().generatePreview()).resolves.toBe(false);

    expect(usePreviewStore.getState().state).toBe('error');
    expect(usePreviewStore.getState().data).toEqual(mockPreviewData);
    expect(usePreviewStore.getState().revisionHash).toBe(mockPreviewData.revision_hash);
  });

  it('discards stale in-flight preview results after invalidate', async () => {
    const first = deferred<PreviewData>();
    const second = deferred<PreviewData>();
    vi.mocked(previewService.generatePreview)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);

    usePreviewStore.setState({
      state: 'current',
      data: mockPreviewData,
      previewWindowOpen: true,
    });

    const firstCall = usePreviewStore.getState().generatePreview();
    expect(usePreviewStore.getState().state).toBe('generating');

    usePreviewStore.getState().invalidate();
    expect(usePreviewStore.getState().state).toBe('stale');

    first.resolve({
      ...mockPreviewData,
      plan_id: 'stale-plan',
      revision_hash: 'stale-hash',
    });
    await firstCall;

    expect(usePreviewStore.getState().state).toBe('stale');
    expect(usePreviewStore.getState().data).toEqual(mockPreviewData);

    await vi.advanceTimersByTimeAsync(500);
    expect(previewService.generatePreview).toHaveBeenCalledTimes(2);

    second.resolve({
      ...mockPreviewData,
      plan_id: 'fresh-plan',
      revision_hash: 'fresh-hash',
    });
    await vi.runAllTimersAsync();

    expect(usePreviewStore.getState().state).toBe('current');
    expect(usePreviewStore.getState().revisionHash).toBe('fresh-hash');
    expect(usePreviewStore.getState().data?.plan_id).toBe('fresh-plan');
  });

  it('requires manual refresh after invalidate when the last preview was slow', async () => {
    vi.mocked(previewService.generatePreview).mockResolvedValue(mockPreviewData);

    usePreviewStore.setState({
      state: 'current',
      data: mockPreviewData,
      previewWindowOpen: true,
      lastSuccessfulDurationMs: 300,
    });

    usePreviewStore.getState().invalidate();

    expect(usePreviewStore.getState().state).toBe('stale');
    expect(usePreviewStore.getState().manualRefreshRequired).toBe(true);

    await vi.advanceTimersByTimeAsync(500);
    expect(previewService.generatePreview).not.toHaveBeenCalled();
  });

  it('defers auto-refresh until interaction ends when the last preview was fast', async () => {
    vi.mocked(previewService.generatePreview).mockResolvedValue(mockPreviewData);

    usePreviewStore.setState({
      state: 'current',
      data: mockPreviewData,
      previewWindowOpen: true,
      interactionActive: true,
      lastSuccessfulDurationMs: 100,
    });

    usePreviewStore.getState().invalidate();

    expect(usePreviewStore.getState().state).toBe('stale');
    expect(usePreviewStore.getState().pendingInteractionRefresh).toBe(true);
    expect(usePreviewStore.getState().manualRefreshRequired).toBe(false);

    await vi.advanceTimersByTimeAsync(500);
    expect(previewService.generatePreview).not.toHaveBeenCalled();

    usePreviewStore.getState().setInteractionActive(false);

    await vi.advanceTimersByTimeAsync(500);
    expect(previewService.generatePreview).toHaveBeenCalledTimes(1);
  });

  it('resumes Run-canvas auto-refresh when interaction ends', async () => {
    vi.mocked(previewService.generatePreview).mockResolvedValue(mockPreviewData);
    usePreviewStore.setState({
      state: 'current',
      data: mockPreviewData,
      canvasPreviewActive: true,
      interactionActive: true,
      lastSuccessfulDurationMs: 100,
    });

    usePreviewStore.getState().invalidate();
    expect(usePreviewStore.getState().pendingInteractionRefresh).toBe(true);

    usePreviewStore.getState().setInteractionActive(false);
    await vi.advanceTimersByTimeAsync(500);

    expect(previewService.generatePreview).toHaveBeenCalledTimes(1);
  });
});
