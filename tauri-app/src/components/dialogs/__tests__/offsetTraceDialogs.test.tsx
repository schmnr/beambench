import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent, waitFor, act } from '@testing-library/react';
import { OffsetDialog } from '../OffsetDialog';
import { TraceImageDialog } from '../TraceImageDialog';
import { BooleanAssistantDialog } from '../BooleanAssistantDialog';
import { useProjectStore } from '../../../stores/projectStore';
import { useNotificationStore } from '../../../stores/notificationStore';
import { useUiStore } from '../../../stores/uiStore';
import { importService } from '../../../services/importService';
import { vectorService } from '../../../services/vectorService';
import type { OffsetPreview } from '../../../types/vector';
import { makeProject, makeProjectObject } from '../../../test-utils/projectFixtures';

const mockInvoke = vi.fn().mockResolvedValue(null);
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => mockInvoke(...args) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

// Mock canvas 2D context for jsdom (avoids "not implemented" noise)
const mockCtx = {
  clearRect: vi.fn(),
  drawImage: vi.fn(),
  fillRect: vi.fn(),
  save: vi.fn(),
  restore: vi.fn(),
  setTransform: vi.fn(),
  translate: vi.fn(),
  scale: vi.fn(),
  stroke: vi.fn(),
  strokeRect: vi.fn(),
  set fillStyle(_v: string) { /* noop */ },
  set strokeStyle(_v: string) { /* noop */ },
  set lineWidth(_v: number) { /* noop */ },
  set globalAlpha(_v: number) { /* noop */ },
};
HTMLCanvasElement.prototype.getContext = vi.fn().mockReturnValue(mockCtx);

// Mock Path2D (not available in jsdom)
globalThis.Path2D = vi.fn().mockImplementation(() => ({
  moveTo: vi.fn(),
  lineTo: vi.fn(),
  quadraticCurveTo: vi.fn(),
  bezierCurveTo: vi.fn(),
  closePath: vi.fn(),
}));

afterEach(() => {
  cleanup();
  mockInvoke.mockClear();
  vi.restoreAllMocks();
});

describe('OffsetDialog', () => {
  it('renders distance, direction, corner style, and delete original fields', () => {
    render(<OffsetDialog objectIds={['obj-1']} onClose={vi.fn()} />);
    expect(screen.getByText('Distance (mm)')).toBeDefined();
    expect(screen.getByText('Direction')).toBeDefined();
    expect(screen.getByText('Corner Style')).toBeDefined();
    expect(screen.getByText('Delete original')).toBeDefined();
  });

  it('submit calls offsetShapes with correct default params', async () => {
    const spy = vi.spyOn(useProjectStore.getState(), 'offsetShapes').mockResolvedValue(undefined);
    const onClose = vi.fn();
    render(<OffsetDialog objectIds={['obj-1']} onClose={onClose} />);

    fireEvent.click(screen.getByTestId('offset-submit'));

    await waitFor(() => {
      expect(spy).toHaveBeenCalledWith(['obj-1'], 1, 'outward', 'miter', false);
    });
    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
    spy.mockRestore();
  });

  it('corner style dropdown passes selected value', async () => {
    const spy = vi.spyOn(useProjectStore.getState(), 'offsetShapes').mockResolvedValue(undefined);
    render(<OffsetDialog objectIds={['obj-1']} onClose={vi.fn()} />);

    // Find corner style select by its label
    const cornerLabel = screen.getByText('Corner Style');
    const cornerSelect = cornerLabel.closest('label')!.querySelector('select')!;
    fireEvent.change(cornerSelect, { target: { value: 'round' } });

    fireEvent.click(screen.getByTestId('offset-submit'));

    await waitFor(() => {
      expect(spy).toHaveBeenCalledWith(['obj-1'], 1, 'outward', 'round', false);
    });
    spy.mockRestore();
  });

  it('corner style dropdown default option shows "Miter" label', () => {
    render(<OffsetDialog objectIds={['obj-1']} onClose={vi.fn()} />);
    const cornerLabel = screen.getByText('Corner Style');
    const cornerSelect = cornerLabel.closest('label')!.querySelector('select')!;
    // The selected option text should be "Miter", not "Corner"
    const selectedOption = cornerSelect.options[cornerSelect.selectedIndex];
    expect(selectedOption.text).toBe('Miter');
  });

  it('submits multi-object offset in a single call', async () => {
    const spy = vi.spyOn(useProjectStore.getState(), 'offsetShapes').mockResolvedValue(undefined);
    render(<OffsetDialog objectIds={['obj-1', 'obj-2', 'obj-3']} onClose={vi.fn()} />);

    fireEvent.click(screen.getByTestId('offset-submit'));

    await waitFor(() => {
      // All object IDs should be sent in a single call, not one call per object
      expect(spy).toHaveBeenCalledTimes(1);
      expect(spy).toHaveBeenCalledWith(['obj-1', 'obj-2', 'obj-3'], 1, 'outward', 'miter', false);
    });
    spy.mockRestore();
  });

  it('delete original checkbox passes value', async () => {
    const spy = vi.spyOn(useProjectStore.getState(), 'offsetShapes').mockResolvedValue(undefined);
    render(<OffsetDialog objectIds={['obj-1']} onClose={vi.fn()} />);

    // Find the delete original checkbox
    const deleteLabel = screen.getByText('Delete original');
    const checkbox = deleteLabel.closest('label')!.querySelector('input')!;
    fireEvent.click(checkbox);

    fireEvent.click(screen.getByTestId('offset-submit'));

    await waitFor(() => {
      expect(spy).toHaveBeenCalledWith(['obj-1'], 1, 'outward', 'miter', true);
    });
    spy.mockRestore();
  });

  it('stays open when offsetShapes rejects', async () => {
    const spy = vi.spyOn(useProjectStore.getState(), 'offsetShapes').mockRejectedValue(new Error('offset failed'));
    const onClose = vi.fn();
    render(<OffsetDialog objectIds={['obj-1']} onClose={onClose} />);

    fireEvent.click(screen.getByTestId('offset-submit'));

    await waitFor(() => {
      expect(spy).toHaveBeenCalled();
    });
    // Dialog should NOT close on error
    expect(onClose).not.toHaveBeenCalled();
    spy.mockRestore();
  });

  it('refuses to submit after the active project changes', async () => {
    useProjectStore.setState({
      project: {
        metadata: { project_id: 'p1', project_name: 'Project A', created_at: '', modified_at: '' },
        workspace: { bed_width_mm: 400, bed_height_mm: 400 },
        layers: [],
        objects: [{
          id: 'obj-1',
          name: 'Shape A',
          visible: true,
          locked: false,
          transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
          bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
          layer_id: 'l1',
          z_index: 0,
          data: { type: 'shape', shape_type: 'rect' },
        }],
      } as never,
    });

    const spy = vi.spyOn(useProjectStore.getState(), 'offsetShapes').mockResolvedValue(undefined);
    const onClose = vi.fn();
    render(<OffsetDialog objectIds={['obj-1']} onClose={onClose} />);

    act(() => {
      useProjectStore.setState({
        project: {
          metadata: { project_id: 'p2', project_name: 'Project B', created_at: '', modified_at: '' },
          workspace: { bed_width_mm: 400, bed_height_mm: 400 },
          layers: [],
          objects: [],
        } as never,
      });
    });

    fireEvent.click(screen.getByTestId('offset-submit'));

    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
    expect(spy).not.toHaveBeenCalled();
    spy.mockRestore();
  });

  it('relabels direction to side options and defaults to Both sides for an all-open selection', async () => {
    const spy = vi.spyOn(vectorService, 'previewOffsetShapes').mockResolvedValue({
      paths: [{ points: [{ x: 0, y: -2 }, { x: 10, y: -2 }], closed: false }],
      source_all_open: true,
    });
    render(<OffsetDialog objectIds={['line-1']} onClose={vi.fn()} />);

    // Field relabels to "Side" and options become Side A / Side B / Both sides.
    await waitFor(() => {
      expect(screen.getByText('Side')).toBeDefined();
    });
    const sideSelect = screen.getByText('Side').closest('label')!.querySelector('select')!;
    expect(Array.from(sideSelect.options).map((o) => o.text)).toEqual([
      'Side A',
      'Side B',
      'Both sides',
    ]);
    // Default applied once for the open selection.
    await waitFor(() => expect(sideSelect.value).toBe('both'));
    spy.mockRestore();
  });

  it('publishes the ghost preview to the store and clears it on unmount', async () => {
    const paths = [{ points: [{ x: 0, y: -2 }, { x: 10, y: -2 }], closed: false }];
    const spy = vi.spyOn(vectorService, 'previewOffsetShapes').mockResolvedValue({
      paths,
      source_all_open: true,
    });
    const { unmount } = render(<OffsetDialog objectIds={['line-1']} onClose={vi.fn()} />);

    await waitFor(() => {
      expect(useUiStore.getState().offsetPreview).toEqual(paths);
    });
    unmount();
    expect(useUiStore.getState().offsetPreview).toBeNull();
    spy.mockRestore();
  });

  it('does not publish a one-sided ghost while auto-defaulting open selections to Both sides', async () => {
    const oneSided = [{ points: [{ x: 0, y: -2 }, { x: 10, y: -2 }], closed: false }];
    const bothSides = [
      { points: [{ x: 0, y: -2 }, { x: 10, y: -2 }], closed: false },
      { points: [{ x: 0, y: 2 }, { x: 10, y: 2 }], closed: false },
    ];
    const resolvers: Array<(v: OffsetPreview) => void> = [];
    const spy = vi
      .spyOn(vectorService, 'previewOffsetShapes')
      .mockImplementation(() => new Promise<OffsetPreview>((res) => { resolvers.push(res); }));
    render(<OffsetDialog objectIds={['line-1']} onClose={vi.fn()} />);

    await waitFor(() => expect(resolvers.length).toBe(1));
    await act(async () => { resolvers[0]({ paths: oneSided, source_all_open: true }); });

    const sideSelect = screen.getByText('Side').closest('label')!.querySelector('select')!;
    expect(sideSelect.value).toBe('both');
    expect(useUiStore.getState().offsetPreview).toBeNull();

    await act(async () => { await new Promise((r) => setTimeout(r, 160)); });
    await waitFor(() => expect(resolvers.length).toBe(2));
    await act(async () => { resolvers[1]({ paths: bothSides, source_all_open: true }); });

    expect(spy).toHaveBeenNthCalledWith(1, ['line-1'], 1, 'outward', 'miter');
    expect(spy).toHaveBeenNthCalledWith(2, ['line-1'], 1, 'both', 'miter');
    expect(useUiStore.getState().offsetPreview).toEqual(bothSides);
    spy.mockRestore();
  });

  it('does not re-apply the Both-sides default after the user picks a side', async () => {
    const spy = vi.spyOn(vectorService, 'previewOffsetShapes').mockResolvedValue({
      paths: [],
      source_all_open: true,
    });
    render(<OffsetDialog objectIds={['line-1']} onClose={vi.fn()} />);

    const sideSelect = await waitFor(() => {
      const s = screen.getByText('Side').closest('label')!.querySelector('select')!;
      expect(s.value).toBe('both');
      return s;
    });
    // User switches to Side B; a later preview run must not snap back to both.
    fireEvent.change(sideSelect, { target: { value: 'inward' } });
    expect(sideSelect.value).toBe('inward');
    const distance = screen.getByText('Distance (mm)').closest('label')!.querySelector('input')!;
    fireEvent.change(distance, { target: { value: '3' } });
    // Let the debounced preview run (past the 120ms debounce) inside act.
    await act(async () => { await new Promise((r) => setTimeout(r, 160)); });
    expect(sideSelect.value).toBe('inward');
    spy.mockRestore();
  });

  it('ignores a stale preview response that resolves after a newer one', async () => {
    const resolvers: Array<(v: OffsetPreview) => void> = [];
    const spy = vi
      .spyOn(vectorService, 'previewOffsetShapes')
      .mockImplementation(() => new Promise<OffsetPreview>((res) => { resolvers.push(res); }));
    render(<OffsetDialog objectIds={['line-1']} onClose={vi.fn()} />);

    await waitFor(() => expect(resolvers.length).toBe(1));
    // User-picked sides publish immediately, so this isolates stale-response handling
    // from the open-selection auto-default to Both sides.
    const direction = screen.getByText('Direction').closest('label')!.querySelector('select')!;
    fireEvent.change(direction, { target: { value: 'inward' } });
    await act(async () => { await new Promise((r) => setTimeout(r, 160)); });
    await waitFor(() => expect(resolvers.length).toBe(2));

    const newer = [{ points: [{ x: 0, y: -4 }, { x: 10, y: -4 }], closed: false }];
    const stale = [{ points: [{ x: 0, y: 9 }, { x: 10, y: 9 }], closed: false }];
    // Resolve newer first, then the superseded (stale) request.
    await act(async () => { resolvers[1]({ paths: newer, source_all_open: true }); });
    await act(async () => { resolvers[0]({ paths: stale, source_all_open: false }); });

    expect(useUiStore.getState().offsetPreview).toEqual(newer);
    spy.mockRestore();
  });
});

describe('BooleanAssistantDialog', () => {
  const previewResult = {
    operation: 'union',
    result: makeProjectObject({
      id: 'preview-union',
      name: 'Union',
      bounds: { min: { x: 0, y: 0 }, max: { x: 15, y: 15 } },
      data: { type: 'vector_path' as const, path_data: 'M0 0 L15 0 L15 15 L0 15 Z', closed: true },
    }),
    sources: [
      {
        id: 'shape-a',
        name: 'Shape A',
        bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
        pathData: 'M0 0 L10 0 L10 10 L0 10 Z',
      },
      {
        id: 'shape-b',
        name: 'Shape B',
        bounds: { min: { x: 5, y: 5 }, max: { x: 15, y: 15 } },
        pathData: 'M5 5 L15 5 L15 15 L5 15 Z',
      },
    ],
  };

  it('loads a non-destructive preview and commits the selected operation', async () => {
    const project = makeProject({
      objects: [
        makeProjectObject({ id: 'shape-a' }),
        makeProjectObject({ id: 'shape-b', bounds: { min: { x: 5, y: 5 }, max: { x: 15, y: 15 } } }),
      ],
    });
    useProjectStore.setState({ project });
    mockInvoke.mockResolvedValueOnce(previewResult);
    const booleanUnion = vi.spyOn(useProjectStore.getState(), 'booleanUnion').mockResolvedValue(undefined);
    const onClose = vi.fn();

    render(<BooleanAssistantDialog objectIds={['shape-a', 'shape-b']} onClose={onClose} />);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('boolean_assistant_preview', {
        objectIds: ['shape-a', 'shape-b'],
        operation: 'union',
      });
    });
    expect(await screen.findByTestId('boolean-assistant-preview')).toBeDefined();

    fireEvent.click(screen.getByTestId('boolean-assistant-apply'));

    await waitFor(() => {
      expect(booleanUnion).toHaveBeenCalledWith('shape-a', 'shape-b');
      expect(onClose).toHaveBeenCalled();
    });
  });

  it('refreshes preview and commits subtract when that operation is selected', async () => {
    const project = makeProject({
      objects: [
        makeProjectObject({ id: 'shape-a' }),
        makeProjectObject({ id: 'shape-b', bounds: { min: { x: 5, y: 5 }, max: { x: 15, y: 15 } } }),
      ],
    });
    useProjectStore.setState({ project });
    mockInvoke
      .mockResolvedValueOnce(previewResult)
      .mockResolvedValueOnce({ ...previewResult, operation: 'subtract' });
    const booleanSubtract = vi.spyOn(useProjectStore.getState(), 'booleanSubtract').mockResolvedValue(undefined);

    render(<BooleanAssistantDialog objectIds={['shape-a', 'shape-b']} onClose={vi.fn()} />);

    await screen.findByTestId('boolean-assistant-preview');
    fireEvent.click(screen.getByText('Subtract'));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenLastCalledWith('boolean_assistant_preview', {
        objectIds: ['shape-a', 'shape-b'],
        operation: 'subtract',
      });
    });
    fireEvent.click(screen.getByTestId('boolean-assistant-apply'));

    await waitFor(() => {
      expect(booleanSubtract).toHaveBeenCalledWith('shape-a', 'shape-b');
    });
  });

  it('refreshes preview and commits exclude when that operation is selected', async () => {
    const project = makeProject({
      objects: [
        makeProjectObject({ id: 'shape-a' }),
        makeProjectObject({ id: 'shape-b', bounds: { min: { x: 5, y: 5 }, max: { x: 15, y: 15 } } }),
      ],
    });
    useProjectStore.setState({ project });
    mockInvoke
      .mockResolvedValueOnce(previewResult)
      .mockResolvedValueOnce({ ...previewResult, operation: 'exclude' });
    const booleanExclude = vi.spyOn(useProjectStore.getState(), 'booleanExclude').mockResolvedValue(undefined);

    render(<BooleanAssistantDialog objectIds={['shape-a', 'shape-b']} onClose={vi.fn()} />);

    await screen.findByTestId('boolean-assistant-preview');
    fireEvent.click(screen.getByText('Exclude'));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenLastCalledWith('boolean_assistant_preview', {
        objectIds: ['shape-a', 'shape-b'],
        operation: 'exclude',
      });
    });
    fireEvent.click(screen.getByTestId('boolean-assistant-apply'));

    await waitFor(() => {
      expect(booleanExclude).toHaveBeenCalledWith('shape-a', 'shape-b');
    });
  });
});

describe('TraceImageDialog', () => {
  it('renders all controls including cutoff, trace transparency, sketch trace', () => {
    render(<TraceImageDialog objectId="obj-1" onClose={vi.fn()} />);
    expect(screen.getByText('Cutoff')).toBeDefined();
    expect(screen.getByText('Threshold')).toBeDefined();
    expect(screen.getByText('Ignore Less Than')).toBeDefined();
    expect(screen.getByText('Smoothness')).toBeDefined();
    expect(screen.getByText('Optimize')).toBeDefined();
    expect(screen.getByText('Trace Transparency')).toBeDefined();
    expect(screen.getByText('Sketch Trace')).toBeDefined();
    expect(screen.getByText('Delete image after trace')).toBeDefined();
    expect(screen.getByText('Fade Image')).toBeDefined();
    expect(screen.getByText('Show Points')).toBeDefined();
    expect(screen.getByText('Clear Boundary')).toBeDefined();
    expect(screen.getByTestId('trace-zoom-in')).toBeDefined();
    expect(screen.getByTestId('trace-zoom-out')).toBeDefined();
    expect(screen.getByTestId('trace-zoom-reset')).toBeDefined();
    expect(screen.getByTestId('trace-dialog-drag-handle')).toBeDefined();
    expect(screen.getByTestId('trace-dialog-resize-handle')).toBeDefined();
  });

  it('zoom controls update the preview zoom label', () => {
    render(<TraceImageDialog objectId="obj-1" onClose={vi.fn()} />);

    expect(screen.getByTestId('trace-zoom-label').textContent).toBe('100%');

    fireEvent.click(screen.getByTestId('trace-zoom-in'));
    expect(screen.getByTestId('trace-zoom-label').textContent).toBe('120%');

    fireEvent.click(screen.getByTestId('trace-zoom-reset'));
    expect(screen.getByTestId('trace-zoom-label').textContent).toBe('100%');
  });

  it('submit selects the returned traced objects after reload', async () => {
    const traced = [
      makeProjectObject({ id: 'trace-1' }),
      makeProjectObject({ id: 'trace-2' }),
    ];
    const spy = vi.spyOn(importService, 'traceImage').mockResolvedValue(traced);
    const loadSpy = vi.spyOn(useProjectStore.getState(), 'loadProject').mockResolvedValue(undefined);
    const selectSpy = vi.spyOn(useProjectStore.getState(), 'selectObjects');
    const onClose = vi.fn();
    render(<TraceImageDialog objectId="obj-1" onClose={onClose} />);

    fireEvent.click(screen.getByTestId('trace-submit'));

    await waitFor(() => {
      // threshold=128, cutoff=0, turdsize=2, alphamax=1.0, opttolerance=0.2,
      // traceAlpha=false, sketchTrace=false, deleteSource=true (default checked)
      expect(spy).toHaveBeenCalledWith('obj-1', 128, 0, 2, 1.0, 0.2, false, false, true, null);
    });
    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
    expect(loadSpy).toHaveBeenCalledWith({ invalidatePreview: true });
    expect(selectSpy).toHaveBeenCalledWith(['trace-1', 'trace-2']);
    spy.mockRestore();
    loadSpy.mockRestore();
    selectSpy.mockRestore();
  });

  it('surfaces trace submit failures and keeps the dialog open', async () => {
    const spy = vi.spyOn(importService, 'traceImage').mockRejectedValue(new Error('trace failed'));
    const loadSpy = vi.spyOn(useProjectStore.getState(), 'loadProject').mockResolvedValue(undefined);
    const onClose = vi.fn();
    useProjectStore.setState({
      project: {
        metadata: { project_id: 'p1', project_name: 'Trace Test', created_at: '', modified_at: '' },
        workspace: { bed_width_mm: 400, bed_height_mm: 400 },
        layers: [],
        objects: [{
          id: 'obj-1',
          name: 'Image',
          visible: true,
          locked: false,
          transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
          bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
          layer_id: 'l1',
          z_index: 0,
          data: { type: 'raster_image', asset_key: 'asset-1', original_width_px: 100, original_height_px: 100 },
        }],
        assets: [],
      } as never,
      loadAssetData: vi.fn().mockResolvedValue(null),
    });
    render(<TraceImageDialog objectId="obj-1" onClose={onClose} />);

    fireEvent.click(screen.getByTestId('trace-submit'));

    await waitFor(() => {
      const notifications = useNotificationStore.getState().notifications;
      expect(notifications[notifications.length - 1]?.message).toContain('Failed to trace image');
      expect(notifications[notifications.length - 1]?.message).toContain('trace failed');
      expect(notifications[notifications.length - 1]?.type).toBe('error');
    });

    expect(loadSpy).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();

    spy.mockRestore();
    loadSpy.mockRestore();
  });

  it('closes when its source object disappears', async () => {
    const onClose = vi.fn();
    useProjectStore.setState({
      project: {
        metadata: { project_id: 'p1', project_name: 'Trace Test', created_at: '', modified_at: '' },
        workspace: { bed_width_mm: 400, bed_height_mm: 400 },
        layers: [],
        objects: [{
          id: 'obj-1',
          name: 'Image',
          visible: true,
          locked: false,
          transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
          bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
          layer_id: 'l1',
          z_index: 0,
          data: { type: 'raster_image', asset_key: 'asset-1', original_width_px: 100, original_height_px: 100 },
        }],
        assets: [],
      } as never,
      loadAssetData: vi.fn().mockResolvedValue(null),
    });

    render(<TraceImageDialog objectId="obj-1" onClose={onClose} />);

    act(() => {
      useProjectStore.setState({
        project: {
          ...useProjectStore.getState().project!,
          objects: [],
        } as never,
      });
    });

    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
  });

  it('renders preview canvas and calls traceImagePreview on mount', async () => {
    const previewSpy = vi.spyOn(importService, 'traceImagePreview').mockResolvedValue({
      paths: ['M0 0L10 10'],
      source_width: 100,
      source_height: 100,
    });
    render(<TraceImageDialog objectId="obj-1" onClose={vi.fn()} />);

    expect(screen.getByTestId('trace-preview-canvas')).toBeDefined();

    await waitFor(() => {
      // threshold=128, cutoff=0, turdsize=2, alphamax=1.0, opttolerance=0.2, traceAlpha=false, sketchTrace=false
      expect(previewSpy).toHaveBeenCalledWith('obj-1', 128, 0, 2, 1.0, 0.2, false, false, expect.any(Number), null);
    });

    // Should show path count after preview resolves
    await waitFor(() => {
      expect(screen.getByText('1 path found')).toBeDefined();
    });

    previewSpy.mockRestore();
  });

  it('left-dragging in the preview commits a boundary for preview and submit', async () => {
    vi.useFakeTimers();
    const boundary = { x: 25, y: 25, width: 50, height: 50 };
    const previewSpy = vi.spyOn(importService, 'traceImagePreview').mockResolvedValue({
      paths: ['M25 25L75 75'],
      source_width: 100,
      source_height: 100,
    });
    const traceSpy = vi.spyOn(importService, 'traceImage').mockResolvedValue([]);
    vi.spyOn(useProjectStore.getState(), 'loadProject').mockResolvedValue(undefined);
    render(<TraceImageDialog objectId="obj-1" onClose={vi.fn()} />);

    await act(async () => { vi.advanceTimersByTime(500); });
    await act(async () => { await Promise.resolve(); });

    const frame = screen.getByTestId('trace-preview-frame');
    const dialog = screen.getByRole('dialog');
    await act(async () => {
      fireEvent.mouseDown(frame, { button: 0, clientX: 0.25, clientY: 0.25 });
      fireEvent.mouseMove(dialog, { clientX: 0.75, clientY: 0.75 });
      fireEvent.mouseUp(dialog);
    });

    await act(async () => { vi.advanceTimersByTime(500); });
    await act(async () => { await Promise.resolve(); });

    expect(previewSpy.mock.calls[previewSpy.mock.calls.length - 1]).toEqual([
      'obj-1', 128, 0, 2, 1.0, 0.2, false, false, expect.any(Number), boundary,
    ]);

    fireEvent.click(screen.getByTestId('trace-submit'));
    await act(async () => { await Promise.resolve(); });
    expect(traceSpy).toHaveBeenCalledWith('obj-1', 128, 0, 2, 1.0, 0.2, false, false, true, boundary);

    previewSpy.mockRestore();
    traceSpy.mockRestore();
    vi.useRealTimers();
  });

  it('discards stale preview response when inputs change before it resolves', async () => {
    vi.useFakeTimers();

    type PreviewResult = { paths: string[]; source_width: number; source_height: number };
    let resolveFirst: ((v: PreviewResult) => void) | null = null;
    let callCount = 0;

    const previewSpy = vi.spyOn(importService, 'traceImagePreview').mockImplementation(() => {
      callCount++;
      if (callCount === 1) {
        // First call — stays pending, resolved manually later
        return new Promise<PreviewResult>((resolve) => { resolveFirst = resolve; });
      }
      // Second call — resolves immediately with 2 paths
      return Promise.resolve({ paths: ['M0 0L20 20', 'M5 5L15 15'], source_width: 200, source_height: 200 });
    });

    render(<TraceImageDialog objectId="obj-1" onClose={vi.fn()} />);

    // Advance past 500ms debounce — first preview request fires
    await act(async () => { vi.advanceTimersByTime(500); });
    expect(previewSpy).toHaveBeenCalledTimes(1);

    // Simulate user changing threshold — find via data-testid
    const thresholdInput = screen.getByTestId('trace-threshold');
    await act(async () => {
      fireEvent.change(thresholdInput, { target: { value: '200' } });
    });

    // Advance past 500ms debounce — second preview request fires
    await act(async () => { vi.advanceTimersByTime(500); });
    expect(previewSpy).toHaveBeenCalledTimes(2);

    // Let second response process (it resolved immediately)
    await act(async () => { await Promise.resolve(); });

    // UI should show second response (2 paths)
    expect(screen.getByText('2 paths found')).toBeDefined();

    // Now resolve the first (stale) response — it should be discarded
    await act(async () => {
      resolveFirst!({ paths: ['M99 99L100 100'], source_width: 50, source_height: 50 });
      await Promise.resolve();
    });

    // UI should still show second response, not the stale first
    expect(screen.getByText('2 paths found')).toBeDefined();
    expect(screen.queryByText('1 path found')).toBeNull();

    previewSpy.mockRestore();
    vi.useRealTimers();
  });

  it('clears preview data on error so stale geometry is removed', async () => {
    vi.useFakeTimers();

    let callCount = 0;
    const previewSpy = vi.spyOn(importService, 'traceImagePreview').mockImplementation(() => {
      callCount++;
      if (callCount === 1) {
        return Promise.resolve({ paths: ['M0 0L10 10'], source_width: 100, source_height: 100 });
      }
      return Promise.reject(new Error('decode failed'));
    });

    render(<TraceImageDialog objectId="obj-1" onClose={vi.fn()} />);

    // First call succeeds
    await act(async () => { vi.advanceTimersByTime(500); });
    await act(async () => { await Promise.resolve(); });
    expect(screen.getByText('1 path found')).toBeDefined();

    // Change input to trigger second call that fails
    const thresholdInput = screen.getByTestId('trace-threshold');
    await act(async () => {
      fireEvent.change(thresholdInput, { target: { value: '200' } });
    });
    await act(async () => { vi.advanceTimersByTime(500); });
    await act(async () => { await Promise.resolve(); });

    // Preview data should be cleared — no path count shown
    expect(screen.queryByText('1 path found')).toBeNull();

    previewSpy.mockRestore();
    vi.useRealTimers();
  });

  it('Clear Boundary removes the selected boundary and refreshes the full preview', async () => {
    vi.useFakeTimers();
    const previewSpy = vi.spyOn(importService, 'traceImagePreview').mockResolvedValue({
      paths: ['M0 0L10 10'], source_width: 100, source_height: 100,
    });

    render(<TraceImageDialog objectId="obj-1" onClose={vi.fn()} />);

    await act(async () => { vi.advanceTimersByTime(500); });
    await act(async () => { await Promise.resolve(); });
    expect(screen.getByText('1 path found')).toBeDefined();

    fireEvent.click(screen.getByTestId('trace-clear-boundary'));

    expect(screen.queryByText('1 path found')).toBeNull();
    await act(async () => { vi.advanceTimersByTime(500); });
    await act(async () => { await Promise.resolve(); });
    expect(previewSpy.mock.calls[previewSpy.mock.calls.length - 1][9]).toBeNull();

    previewSpy.mockRestore();
    vi.useRealTimers();
  });

  it('Clear Boundary resets submit calls to full-image tracing', async () => {
    vi.useFakeTimers();
    const previewSpy = vi.spyOn(importService, 'traceImagePreview').mockResolvedValue({
      paths: ['M0 0L10 10'], source_width: 100, source_height: 100,
    });
    const traceSpy = vi.spyOn(importService, 'traceImage').mockResolvedValue([]);
    vi.spyOn(useProjectStore.getState(), 'loadProject').mockResolvedValue(undefined);
    render(<TraceImageDialog objectId="obj-1" onClose={vi.fn()} />);

    await act(async () => { vi.advanceTimersByTime(500); });
    await act(async () => { await Promise.resolve(); });

    const frame = screen.getByTestId('trace-preview-frame');
    const dialog = screen.getByRole('dialog');
    await act(async () => {
      fireEvent.mouseDown(frame, { button: 0, clientX: 0.25, clientY: 0.25 });
      fireEvent.mouseMove(dialog, { clientX: 0.75, clientY: 0.75 });
      fireEvent.mouseUp(dialog);
    });

    fireEvent.click(screen.getByTestId('trace-clear-boundary'));
    fireEvent.click(screen.getByTestId('trace-submit'));

    await act(async () => { await Promise.resolve(); });
    expect(traceSpy).toHaveBeenCalledWith('obj-1', 128, 0, 2, 1.0, 0.2, false, false, true, null);

    previewSpy.mockRestore();
    traceSpy.mockRestore();
    vi.useRealTimers();
  });

  it('middle mouse and Space+left drag pan without creating a boundary', async () => {
    const previewSpy = vi.spyOn(importService, 'traceImagePreview').mockResolvedValue({
      paths: ['M0 0L10 10'], source_width: 100, source_height: 100,
    });
    const traceSpy = vi.spyOn(importService, 'traceImage').mockResolvedValue([]);
    vi.spyOn(useProjectStore.getState(), 'loadProject').mockResolvedValue(undefined);
    render(<TraceImageDialog objectId="obj-1" onClose={vi.fn()} />);

    const frame = screen.getByTestId('trace-preview-frame');
    const dialog = screen.getByRole('dialog');
    fireEvent.mouseDown(frame, { button: 1, clientX: 150, clientY: 80 });
    fireEvent.mouseMove(dialog, { clientX: 180, clientY: 110 });
    fireEvent.mouseUp(dialog);

    fireEvent.keyDown(window, { key: ' ', code: 'Space' });
    fireEvent.mouseDown(frame, { button: 0, clientX: 150, clientY: 80 });
    fireEvent.mouseMove(dialog, { clientX: 180, clientY: 110 });
    fireEvent.mouseUp(dialog);
    fireEvent.keyUp(window, { key: ' ', code: 'Space' });

    fireEvent.click(screen.getByTestId('trace-submit'));
    await waitFor(() => {
      expect(traceSpy).toHaveBeenCalledWith('obj-1', 128, 0, 2, 1.0, 0.2, false, false, true, null);
    });

    previewSpy.mockRestore();
    traceSpy.mockRestore();
  });

  it('toggling Trace Transparency changes traceAlpha in submit call', async () => {
    const spy = vi.spyOn(importService, 'traceImage').mockResolvedValue([]);
    vi.spyOn(useProjectStore.getState(), 'loadProject').mockResolvedValue(undefined);
    render(<TraceImageDialog objectId="obj-1" onClose={vi.fn()} />);

    // Toggle Trace Transparency ON
    const toggle = screen.getByText('Trace Transparency');
    fireEvent.click(toggle.closest('label')!.querySelector('input') as Element); // click the checkbox input

    fireEvent.click(screen.getByTestId('trace-submit'));

    await waitFor(() => {
      // traceAlpha should now be true (7th arg)
      expect(spy).toHaveBeenCalledWith('obj-1', 128, 0, 2, 1.0, 0.2, true, false, true, null);
    });
    spy.mockRestore();
  });

  it('toggling Sketch Trace changes sketchTrace in submit call', async () => {
    const spy = vi.spyOn(importService, 'traceImage').mockResolvedValue([]);
    vi.spyOn(useProjectStore.getState(), 'loadProject').mockResolvedValue(undefined);
    render(<TraceImageDialog objectId="obj-1" onClose={vi.fn()} />);

    // Toggle Sketch Trace ON
    const toggle = screen.getByText('Sketch Trace');
    fireEvent.click(toggle.closest('label')!.querySelector('input') as Element);

    fireEvent.click(screen.getByTestId('trace-submit'));

    await waitFor(() => {
      // sketchTrace should now be true (8th arg)
      expect(spy).toHaveBeenCalledWith('obj-1', 128, 0, 2, 1.0, 0.2, false, true, true, null);
    });
    spy.mockRestore();
  });

  it('unchecking Delete image after trace changes deleteSource in submit call', async () => {
    const spy = vi.spyOn(importService, 'traceImage').mockResolvedValue([]);
    vi.spyOn(useProjectStore.getState(), 'loadProject').mockResolvedValue(undefined);
    render(<TraceImageDialog objectId="obj-1" onClose={vi.fn()} />);

    // Delete image is checked by default — toggle it OFF
    const toggle = screen.getByText('Delete image after trace');
    fireEvent.click(toggle.closest('label')!.querySelector('input') as Element);

    fireEvent.click(screen.getByTestId('trace-submit'));

    await waitFor(() => {
      // deleteSource should now be false (9th arg)
      expect(spy).toHaveBeenCalledWith('obj-1', 128, 0, 2, 1.0, 0.2, false, false, false, null);
    });
    spy.mockRestore();
  });

  it('Trace Transparency changes preview call params', async () => {
    vi.useFakeTimers();
    const previewSpy = vi.spyOn(importService, 'traceImagePreview').mockResolvedValue({
      paths: ['M0 0L10 10'], source_width: 100, source_height: 100,
    });

    render(<TraceImageDialog objectId="obj-1" onClose={vi.fn()} />);

    // Wait for initial preview call
    await act(async () => { vi.advanceTimersByTime(500); });
    await act(async () => { await Promise.resolve(); });
    expect(previewSpy).toHaveBeenCalledWith('obj-1', 128, 0, 2, 1.0, 0.2, false, false, expect.any(Number), null);

    // Toggle Trace Transparency ON
    const toggle = screen.getByText('Trace Transparency');
    await act(async () => {
      fireEvent.click(toggle.closest('label')!.querySelector('input') as Element);
    });

    // Wait for debounced preview with updated param
    await act(async () => { vi.advanceTimersByTime(500); });
    await act(async () => { await Promise.resolve(); });

    // Should have been called with traceAlpha=true
    const lastCall = previewSpy.mock.calls[previewSpy.mock.calls.length - 1];
    expect(lastCall[6]).toBe(true); // traceAlpha is 7th arg (index 6)

    previewSpy.mockRestore();
    vi.useRealTimers();
  });
});
