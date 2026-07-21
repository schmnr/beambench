import { describe, it, expect, vi, beforeEach } from 'vitest';
import { CanvasRenderer } from './CanvasRenderer';
import type { RenderParams } from './CanvasRenderer';
import type { PreviewData } from '../types/preview';
import { DARK_THEME, LIGHT_THEME } from './constants';
import type { ProjectObject, Layer } from '../types/project';
import type { EditablePath } from '../types/vector';
import { invoke } from '@tauri-apps/api/core';
import { useNotificationStore } from '../stores/notificationStore';
import { makeLayer, makeProjectObject } from '../test-utils/projectFixtures';
import { resetTransformedPath2DProbeForTests } from './drawObjects';
import { renderOptionsFromViewStyle } from '../stores/uiStore';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

// Need to mock the canvas context before importing the renderer
// Mock ResizeObserver for jsdom
globalThis.ResizeObserver = vi.fn().mockImplementation(() => ({
  observe: vi.fn(),
  unobserve: vi.fn(),
  disconnect: vi.fn(),
}));

// Mock canvas 2D context
const mockGetContext = vi.fn();
HTMLCanvasElement.prototype.getContext = mockGetContext;

function createMockCtx(): CanvasRenderingContext2D {
  return {
    save: vi.fn(),
    restore: vi.fn(),
    beginPath: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    stroke: vi.fn(),
    fill: vi.fn(),
    closePath: vi.fn(),
    fillRect: vi.fn(),
    strokeRect: vi.fn(),
    clearRect: vi.fn(),
    rect: vi.fn(),
    clip: vi.fn(),
    arc: vi.fn(),
    arcTo: vi.fn(),
    ellipse: vi.fn(),
    bezierCurveTo: vi.fn(),
    quadraticCurveTo: vi.fn(),
    roundRect: vi.fn(),
    transform: vi.fn(),
    translate: vi.fn(),
    rotate: vi.fn(),
    scale: vi.fn(),
    setLineDash: vi.fn(),
    fillText: vi.fn(),
    strokeText: vi.fn(),
    measureText: vi.fn(() => ({ width: 0 })),
    setTransform: vi.fn(),
    drawImage: vi.fn(),
    getImageData: vi.fn(() => ({ data: new Uint8ClampedArray(4) })),
    strokeStyle: '',
    fillStyle: '',
    lineWidth: 1,
    globalAlpha: 1,
    imageSmoothingEnabled: true,
    lineDashOffset: 0,
    font: '',
    textAlign: '' as CanvasTextAlign,
    textBaseline: '' as CanvasTextBaseline,
    canvas: document.createElement('canvas'),
  } as unknown as CanvasRenderingContext2D;
}

const baseParams: RenderParams = {
  workspace: { bed_width_mm: 400, bed_height_mm: 300, origin: 'top_left' },
  objects: [],
  layers: [],
  selectedObjectIds: [],
  vp: { offset: { x: 200, y: 150 }, zoom: 100, canvasWidth: 800, canvasHeight: 600 },
  gridVisible: false,
  gridSpacingMm: 10,
  toolOverlay: { type: 'none' },
};

function makeVectorPreview(overrides = {}) {
  return {
    points: [
      { x: 0, y: 0 },
      { x: 10, y: 10 },
    ],
    closed: false,
    power_percent: 80,
    speed_mm_min: 1000,
    sequence: 1,
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

const mockPreviewData: PreviewData = {
  plan_id: 'test-plan',
  revision_hash: 'hash',
  bounds: { min: { x: 0, y: 0 }, max: { x: 100, y: 100 } },
  layers: [
    {
      layer_id: 'layer1',
      vector_paths: [makeVectorPreview()],
      raster_regions: [],
    },
  ],
  failed_entries: [],
  travel_moves: [makeTravelMove()],
  frame: null,
  stats: {
    total_distance_mm: 50,
    travel_distance_mm: 10,
    burn_distance_mm: 40,
    estimated_duration_secs: 30,
    segment_count: 3,
    raster_line_count: 0,
  },
  warnings: [],
};

describe('CanvasRenderer', () => {
  let ctx: CanvasRenderingContext2D;
  let renderer: CanvasRenderer;
  const initialNotificationState = useNotificationStore.getState();

  beforeEach(() => {
    mockGetContext.mockImplementation(() => createMockCtx());
    resetTransformedPath2DProbeForTests();
    ctx = createMockCtx();
    renderer = new CanvasRenderer(ctx);
    delete (globalThis as typeof globalThis & { __BB_FORCE_FULL_BASE_REDRAW?: boolean })
      .__BB_FORCE_FULL_BASE_REDRAW;
    useNotificationStore.setState(initialNotificationState, true);
    vi.mocked(invoke).mockReset();
  });

  it('renders without errors when no preview data', () => {
    expect(() => renderer.render(baseParams)).not.toThrow();
    expect(ctx.fillRect).toHaveBeenCalled(); // Background cleared
  });

  it('shows a loading badge while keeping cached preview geometry visible', () => {
    renderer.render({
      ...baseParams,
      previewData: mockPreviewData,
      showPreview: true,
      previewState: 'generating',
    });

    expect(ctx.fillText).toHaveBeenCalledWith('PREVIEW (loading)', expect.any(Number), expect.any(Number));
    expect(ctx.lineTo).toHaveBeenCalled();
  });

  it('shows a stale badge while keeping cached preview geometry visible', () => {
    renderer.render({
      ...baseParams,
      previewData: mockPreviewData,
      showPreview: true,
      previewState: 'stale',
    });

    expect(ctx.fillText).toHaveBeenCalledWith('PREVIEW (stale)', expect.any(Number), expect.any(Number));
    expect(ctx.lineTo).toHaveBeenCalled();
  });

  it('shows a failure badge while keeping cached preview geometry visible', () => {
    renderer.render({
      ...baseParams,
      previewData: mockPreviewData,
      showPreview: true,
      previewState: 'error',
    });

    expect(ctx.fillText).toHaveBeenCalledWith('PREVIEW (failed)', expect.any(Number), expect.any(Number));
    expect(ctx.lineTo).toHaveBeenCalled();
  });

  it('shows a refresh-required badge while keeping cached preview geometry visible', () => {
    renderer.render({
      ...baseParams,
      previewData: mockPreviewData,
      showPreview: true,
      previewState: 'stale',
      previewManualRefreshRequired: true,
    });

    expect(ctx.fillText).toHaveBeenCalledWith('PREVIEW (refresh)', expect.any(Number), expect.any(Number));
    expect(ctx.lineTo).toHaveBeenCalled();
  });

  it('shows an empty preview badge when no cached preview data exists', () => {
    renderer.render({
      ...baseParams,
      showPreview: true,
      previewState: 'current',
      previewData: null,
    });

    expect(ctx.fillText).toHaveBeenCalledWith('PREVIEW (empty)', expect.any(Number), expect.any(Number));
  });

  it('renders tool overlays without repainting the base scene background', () => {
    renderer.renderToolOverlay({
      ...baseParams,
      toolOverlay: {
        type: 'measure-line',
        startScreen: { x: 10, y: 10 },
        endScreen: { x: 40, y: 40 },
        distanceMm: 12,
        angleDeg: 45,
      },
    });

    expect(ctx.clearRect).toHaveBeenCalledWith(0, 0, baseParams.vp.canvasWidth, baseParams.vp.canvasHeight);
    expect(ctx.fillRect).not.toHaveBeenCalled();
  });

  it('renders selection highlights on the overlay without repainting the base scene background', () => {
    const selected: ProjectObject = makeProjectObject({
      id: 'selected',
      name: 'Selected',
      bounds: { min: { x: 10, y: 10 }, max: { x: 60, y: 40 } },
      layer_id: 'layer1',
      data: {
        type: 'shape',
        kind: 'rectangle',
        width: 50,
        height: 30,
        corner_radius: 0,
      },
    });

    renderer.renderToolOverlay({
      ...baseParams,
      objects: [selected],
      selectedObjectIds: ['selected'],
      layers: [makeLayer({ id: 'layer1', name: 'Layer 1' })],
    });

    expect(ctx.clearRect).toHaveBeenCalledWith(0, 0, baseParams.vp.canvasWidth, baseParams.vp.canvasHeight);
    expect(ctx.fillRect).not.toHaveBeenCalledWith(
      0,
      0,
      baseParams.vp.canvasWidth,
      baseParams.vp.canvasHeight,
    );
    expect(ctx.strokeRect).toHaveBeenCalled();
  });

  it('redraws only a bounded dirty region for base-scene object movement', () => {
    const obj: ProjectObject = makeProjectObject({
      id: 'dirty-object',
      name: 'Dirty Object',
      bounds: { min: { x: 10, y: 10 }, max: { x: 30, y: 30 } },
      layer_id: 'layer1',
      data: {
        type: 'shape',
        kind: 'rectangle',
        width: 20,
        height: 20,
        corner_radius: 0,
      },
    });
    const layer: Layer = makeLayer({ id: 'layer1', name: 'Layer 1' });

    renderer.renderBaseScene({
      ...baseParams,
      objects: [obj],
      layers: [layer],
    });

    (ctx.clearRect as unknown as { mockClear: () => void }).mockClear();
    (ctx.rect as unknown as { mockClear: () => void }).mockClear();
    (ctx.clip as unknown as { mockClear: () => void }).mockClear();

    obj.bounds.min.x += 40;
    obj.bounds.max.x += 40;

    renderer.renderBaseScene({
      ...baseParams,
      objects: [obj],
      layers: [layer],
    });

    expect(ctx.clearRect).toHaveBeenCalledTimes(1);
    const [x, y, w, h] = (ctx.clearRect as unknown as { mock: { calls: number[][] } }).mock.calls[0];
    expect([x, y, w, h]).not.toEqual([0, 0, baseParams.vp.canvasWidth, baseParams.vp.canvasHeight]);
    expect(w).toBeLessThan(baseParams.vp.canvasWidth);
    expect(h).toBeLessThan(baseParams.vp.canvasHeight);
    expect(ctx.clip).toHaveBeenCalled();
  });

  it('falls back to a full base redraw when the debug toggle is enabled', () => {
    const obj: ProjectObject = makeProjectObject({
      id: 'force-full-redraw',
      name: 'Force Full Redraw',
      bounds: { min: { x: 10, y: 10 }, max: { x: 30, y: 30 } },
      layer_id: 'layer1',
      data: {
        type: 'shape',
        kind: 'rectangle',
        width: 20,
        height: 20,
        corner_radius: 0,
      },
    });
    const layer: Layer = makeLayer({ id: 'layer1', name: 'Layer 1' });

    renderer.renderBaseScene({
      ...baseParams,
      objects: [obj],
      layers: [layer],
    });

    (ctx.clearRect as unknown as { mockClear: () => void }).mockClear();
    (ctx.fillRect as unknown as { mockClear: () => void }).mockClear();
    (ctx.rect as unknown as { mockClear: () => void }).mockClear();
    (ctx.clip as unknown as { mockClear: () => void }).mockClear();

    (globalThis as typeof globalThis & { __BB_FORCE_FULL_BASE_REDRAW?: boolean })
      .__BB_FORCE_FULL_BASE_REDRAW = true;
    obj.bounds.min.x += 40;
    obj.bounds.max.x += 40;

    renderer.renderBaseScene({
      ...baseParams,
      objects: [obj],
      layers: [layer],
    });

    expect(ctx.clearRect).not.toHaveBeenCalled();
    expect(ctx.clip).not.toHaveBeenCalled();
    expect(ctx.fillRect).toHaveBeenCalledWith(0, 0, baseParams.vp.canvasWidth, baseParams.vp.canvasHeight);

    delete (globalThis as typeof globalThis & { __BB_FORCE_FULL_BASE_REDRAW?: boolean })
      .__BB_FORCE_FULL_BASE_REDRAW;
  });

  it('dirties the old region when an object is removed', () => {
    const obj: ProjectObject = makeProjectObject({
      id: 'remove-object',
      name: 'Remove Object',
      bounds: { min: { x: 40, y: 40 }, max: { x: 70, y: 70 } },
      layer_id: 'layer1',
      data: {
        type: 'shape',
        kind: 'rectangle',
        width: 30,
        height: 30,
        corner_radius: 0,
      },
    });
    const layer: Layer = makeLayer({ id: 'layer1', name: 'Layer 1' });

    renderer.renderBaseScene({
      ...baseParams,
      objects: [obj],
      layers: [layer],
    });

    (ctx.clearRect as unknown as { mockClear: () => void }).mockClear();
    (ctx.clip as unknown as { mockClear: () => void }).mockClear();

    renderer.renderBaseScene({
      ...baseParams,
      objects: [],
      layers: [layer],
    });

    expect(ctx.clearRect).toHaveBeenCalledTimes(1);
    const [, , w, h] = (ctx.clearRect as unknown as { mock: { calls: number[][] } }).mock.calls[0];
    expect(w).toBeLessThan(baseParams.vp.canvasWidth);
    expect(h).toBeLessThan(baseParams.vp.canvasHeight);
    expect(ctx.clip).toHaveBeenCalled();
  });

  it('dirties the new region when an object is added', () => {
    const obj: ProjectObject = makeProjectObject({
      id: 'add-object',
      name: 'Add Object',
      bounds: { min: { x: 60, y: 60 }, max: { x: 90, y: 90 } },
      layer_id: 'layer1',
      data: {
        type: 'shape',
        kind: 'rectangle',
        width: 30,
        height: 30,
        corner_radius: 0,
      },
    });
    const layer: Layer = makeLayer({ id: 'layer1', name: 'Layer 1' });

    renderer.renderBaseScene({
      ...baseParams,
      objects: [],
      layers: [layer],
    });

    (ctx.clearRect as unknown as { mockClear: () => void }).mockClear();
    (ctx.clip as unknown as { mockClear: () => void }).mockClear();

    renderer.renderBaseScene({
      ...baseParams,
      objects: [obj],
      layers: [layer],
    });

    expect(ctx.clearRect).toHaveBeenCalledTimes(1);
    const [, , w, h] = (ctx.clearRect as unknown as { mock: { calls: number[][] } }).mock.calls[0];
    expect(w).toBeLessThan(baseParams.vp.canvasWidth);
    expect(h).toBeLessThan(baseParams.vp.canvasHeight);
    expect(ctx.clip).toHaveBeenCalled();
  });

  it('dirties the overlapping region when only z-index changes', () => {
    const objA: ProjectObject = makeProjectObject({
      id: 'z-a',
      name: 'Z A',
      bounds: { min: { x: 80, y: 80 }, max: { x: 130, y: 130 } },
      z_index: 1,
      layer_id: 'layer1',
      data: { type: 'shape', kind: 'rectangle', width: 50, height: 50, corner_radius: 0 },
    });
    const objB: ProjectObject = makeProjectObject({
      id: 'z-b',
      name: 'Z B',
      bounds: { min: { x: 90, y: 90 }, max: { x: 140, y: 140 } },
      z_index: 2,
      layer_id: 'layer1',
      data: { type: 'shape', kind: 'rectangle', width: 50, height: 50, corner_radius: 0 },
    });
    const layer: Layer = makeLayer({ id: 'layer1', name: 'Layer 1' });

    renderer.renderBaseScene({
      ...baseParams,
      objects: [objA, objB],
      layers: [layer],
    });

    (ctx.clearRect as unknown as { mockClear: () => void }).mockClear();
    (ctx.clip as unknown as { mockClear: () => void }).mockClear();

    objA.z_index = 3;
    objB.z_index = 1;

    renderer.renderBaseScene({
      ...baseParams,
      objects: [objA, objB],
      layers: [layer],
    });

    expect(ctx.clearRect).toHaveBeenCalledTimes(1);
    const [, , w, h] = (ctx.clearRect as unknown as { mock: { calls: number[][] } }).mock.calls[0];
    expect(w).toBeLessThan(baseParams.vp.canvasWidth);
    expect(h).toBeLessThan(baseParams.vp.canvasHeight);
    expect(ctx.clip).toHaveBeenCalled();
  });

  it('forces a full base redraw when layer visibility changes', () => {
    const obj: ProjectObject = makeProjectObject({
      id: 'layer-visible',
      name: 'Layer Visible',
      bounds: { min: { x: 20, y: 20 }, max: { x: 50, y: 50 } },
      layer_id: 'layer1',
      data: { type: 'shape', kind: 'rectangle', width: 30, height: 30, corner_radius: 0 },
    });
    const layer: Layer = makeLayer({ id: 'layer1', name: 'Layer 1', visible: true });

    renderer.renderBaseScene({
      ...baseParams,
      objects: [obj],
      layers: [layer],
    });

    (ctx.clearRect as unknown as { mockClear: () => void }).mockClear();
    (ctx.fillRect as unknown as { mockClear: () => void }).mockClear();
    (ctx.clip as unknown as { mockClear: () => void }).mockClear();

    renderer.renderBaseScene({
      ...baseParams,
      objects: [obj],
      layers: [{ ...layer, visible: false }],
    });

    expect(ctx.clearRect).not.toHaveBeenCalled();
    expect(ctx.clip).not.toHaveBeenCalled();
    expect(ctx.fillRect).toHaveBeenCalledWith(0, 0, baseParams.vp.canvasWidth, baseParams.vp.canvasHeight);
  });

  it('forces a full base redraw when interaction state changes so proxies can settle back to crisp vectors', () => {
    const heavyPath = `M 0 0 ${Array.from({ length: 6000 }, (_, index) => `L ${index + 1} ${index % 11}`).join(' ')}`;
    const heavyObject: ProjectObject = makeProjectObject({
      id: 'interaction-heavy',
      name: 'Interaction Heavy',
      bounds: { min: { x: 10, y: 10 }, max: { x: 110, y: 60 } },
      layer_id: 'layer1',
      data: { type: 'vector_path', path_data: heavyPath, closed: false },
    });
    const layer: Layer = makeLayer({
      id: 'layer1',
      name: 'Layer 1',
      operation: 'cut',
      color_tag: '#000000',
      power_percent: 100,
    });
    const proxyCtx = createMockCtx();
    mockGetContext.mockImplementation(() => proxyCtx);

    renderer.renderBaseScene({
      ...baseParams,
      objects: [heavyObject],
      layers: [layer],
      interactionState: { active: true, kind: 'zoom' },
    });

    (ctx.clearRect as unknown as { mockClear: () => void }).mockClear();
    (ctx.fillRect as unknown as { mockClear: () => void }).mockClear();
    (ctx.clip as unknown as { mockClear: () => void }).mockClear();

    renderer.renderBaseScene({
      ...baseParams,
      objects: [heavyObject],
      layers: [layer],
      interactionState: { active: false, kind: 'none' },
    });

    expect(ctx.clearRect).not.toHaveBeenCalled();
    expect(ctx.clip).not.toHaveBeenCalled();
    expect(ctx.fillRect).toHaveBeenCalledWith(0, 0, baseParams.vp.canvasWidth, baseParams.vp.canvasHeight);
  });

  it('uses a bitmap proxy for heavy vectors during idle renders and reuses it across frames', () => {
    const heavyPath = `M 0 0 ${Array.from({ length: 6000 }, (_, index) => `L ${index + 1} ${index % 11}`).join(' ')}`;
    const heavyObject: ProjectObject = makeProjectObject({
      id: 'heavy-path',
      name: 'Heavy Path',
      bounds: { min: { x: 10, y: 10 }, max: { x: 110, y: 60 } },
      layer_id: 'layer1',
      data: { type: 'vector_path', path_data: heavyPath, closed: false },
    });
    const layer: Layer = makeLayer({
      id: 'layer1',
      name: 'Layer 1',
      operation: 'cut',
      color_tag: '#000000',
      power_percent: 100,
    });
    const proxyCtx = createMockCtx();
    mockGetContext.mockImplementation(() => proxyCtx);

    renderer.render({
      ...baseParams,
      objects: [heavyObject],
      layers: [layer],
      interactionState: { active: false, kind: 'none' },
    });
    renderer.render({
      ...baseParams,
      objects: [heavyObject],
      layers: [layer],
      interactionState: { active: false, kind: 'none' },
    });

    expect(ctx.drawImage).toHaveBeenCalledTimes(2);
    expect(proxyCtx.stroke).toHaveBeenCalledTimes(1);
  });

  it('uses bitmap proxies for all visible heavy vectors during object drag, not just the drag target', () => {
    const heavyPathA = `M 0 0 ${Array.from({ length: 6000 }, (_, index) => `L ${index + 1} ${index % 13}`).join(' ')}`;
    const heavyPathB = `M 0 0 ${Array.from({ length: 6000 }, (_, index) => `L ${index + 1} ${(index * 3) % 17}`).join(' ')}`;
    const heavyA: ProjectObject = makeProjectObject({
      id: 'heavy-a',
      name: 'Heavy A',
      bounds: { min: { x: 10, y: 10 }, max: { x: 110, y: 60 } },
      layer_id: 'layer1',
      data: { type: 'vector_path', path_data: heavyPathA, closed: false },
    });
    const heavyB: ProjectObject = makeProjectObject({
      id: 'heavy-b',
      name: 'Heavy B',
      bounds: { min: { x: 140, y: 10 }, max: { x: 240, y: 60 } },
      layer_id: 'layer1',
      data: { type: 'vector_path', path_data: heavyPathB, closed: false },
    });
    const layer: Layer = makeLayer({
      id: 'layer1',
      name: 'Layer 1',
      operation: 'cut',
      color_tag: '#000000',
      power_percent: 100,
    });
    const proxyCtx = createMockCtx();
    mockGetContext.mockImplementation(() => proxyCtx);

    renderer.render({
      ...baseParams,
      objects: [heavyA, heavyB],
      layers: [layer],
      interactionState: { active: true, kind: 'object-drag', objectIds: ['heavy-a'] },
    });

    expect(ctx.drawImage).toHaveBeenCalledTimes(2);
    expect(proxyCtx.stroke).toHaveBeenCalledTimes(2);
  });

  it('does not proxy moderately complex vectors below the 5000-command threshold', () => {
    const mediumHeavyPath = `M 0 0 ${Array.from({ length: 3000 }, (_, index) => `L ${index + 1} ${index % 9}`).join(' ')}`;
    const mediumHeavyObject: ProjectObject = makeProjectObject({
      id: 'medium-heavy',
      name: 'Medium Heavy',
      bounds: { min: { x: 10, y: 10 }, max: { x: 110, y: 60 } },
      layer_id: 'layer1',
      data: { type: 'vector_path', path_data: mediumHeavyPath, closed: false },
    });
    const layer: Layer = makeLayer({
      id: 'layer1',
      name: 'Layer 1',
      operation: 'cut',
      color_tag: '#000000',
      power_percent: 100,
    });
    const proxyCtx = createMockCtx();
    mockGetContext.mockImplementation(() => proxyCtx);

    renderer.render({
      ...baseParams,
      objects: [mediumHeavyObject],
      layers: [layer],
      interactionState: { active: true, kind: 'zoom' },
    });

    expect(ctx.drawImage).not.toHaveBeenCalled();
    expect(proxyCtx.stroke).not.toHaveBeenCalled();
    expect(ctx.stroke).toHaveBeenCalled();
  });

  it('does not use bitmap proxies for non-heavy vectors', () => {
    const vectorObject: ProjectObject = makeProjectObject({
      id: 'light-path',
      name: 'Light Path',
      bounds: { min: { x: 10, y: 10 }, max: { x: 30, y: 30 } },
      layer_id: 'layer1',
      data: { type: 'vector_path', path_data: 'M 0 0 L 10 0 L 10 10 Z', closed: true },
    });
    const layer: Layer = makeLayer({
      id: 'layer1',
      name: 'Layer 1',
      operation: 'cut',
      color_tag: '#000000',
      power_percent: 100,
    });

    renderer.render({
      ...baseParams,
      objects: [vectorObject],
      layers: [layer],
      interactionState: { active: true, kind: 'zoom' },
    });

    expect(ctx.drawImage).not.toHaveBeenCalled();
    expect(ctx.stroke).toHaveBeenCalled();
  });

  it('culls fully off-screen objects before drawing', () => {
    const offscreenObject: ProjectObject = makeProjectObject({
      id: 'offscreen',
      name: 'Offscreen',
      bounds: { min: { x: 2000, y: 2000 }, max: { x: 2100, y: 2100 } },
      layer_id: 'layer1',
      data: { type: 'shape', kind: 'rectangle', width: 100, height: 100, corner_radius: 0 },
    });
    const layer: Layer = makeLayer({
      id: 'layer1',
      name: 'Layer 1',
      operation: 'cut',
      color_tag: '#000000',
      power_percent: 100,
    });

    renderer.render({
      ...baseParams,
      objects: [offscreenObject],
      layers: [layer],
    });

    expect(ctx.rect).not.toHaveBeenCalled();
    expect(ctx.drawImage).not.toHaveBeenCalled();
  });

  it('renders preview overlay when showPreview=true and data present', () => {
    renderer.render({
      ...baseParams,
      previewData: mockPreviewData,
      showPreview: true,
    });

    // Should have called save/restore for preview drawing (via drawVectorPreview etc.)
    expect(ctx.save).toHaveBeenCalled();
    expect(ctx.stroke).toHaveBeenCalled();
  });

  it('does not draw live objects on layers represented by the current preview overlay', () => {
    const strokeRect = vi.fn();
    ctx.strokeRect = strokeRect;

    const layer: Layer = makeLayer({
      id: 'layer1',
      name: 'Line',
      operation: 'line',
      color_tag: '#000000',
      power_percent: 100,
    });

    const obj: ProjectObject = makeProjectObject({
      id: 'rect1',
      name: 'Rect',
      bounds: { min: { x: 10, y: 10 }, max: { x: 40, y: 40 } },
      layer_id: 'layer1',
      data: { type: 'shape', kind: 'rectangle', width: 30, height: 30, corner_radius: 0 },
    });

    renderer.render({
      ...baseParams,
      objects: [obj],
      layers: [layer],
      previewData: {
        ...mockPreviewData,
        layers: [
          {
            layer_id: 'layer1',
            vector_paths: [
              makeVectorPreview({
                points: [
                  { x: 10, y: 10 },
                  { x: 40, y: 10 },
                  { x: 40, y: 40 },
                  { x: 10, y: 40 },
                ],
                closed: true,
                power_percent: 100,
              }),
            ],
            raster_regions: [],
          },
        ],
      },
      showPreview: true,
    });

    expect(strokeRect).not.toHaveBeenCalledWith(20, 20, 60, 60);
  });

  it('does not render preview overlay when showPreview=false', () => {
    const saveCalls = vi.fn();
    ctx.save = saveCalls;

    renderer.render({
      ...baseParams,
      previewData: mockPreviewData,
      showPreview: false,
    });

    // save should be called fewer times (no preview save/restore)
    const saveCallCount = saveCalls.mock.calls.length;

    const saveCalls2 = vi.fn();
    const ctx2 = createMockCtx();
    ctx2.save = saveCalls2;
    const renderer2 = new CanvasRenderer(ctx2);

    renderer2.render({
      ...baseParams,
      previewData: mockPreviewData,
      showPreview: true,
    });

    // With preview on, should have more save calls
    expect(saveCalls2.mock.calls.length).toBeGreaterThan(saveCallCount);
  });

  it('does not render preview overlay when data is null', () => {
    const strokeCalls = vi.fn();
    ctx.stroke = strokeCalls;

    renderer.render({
      ...baseParams,
      previewData: null,
      showPreview: true,
    });

    const countWithoutPreview = strokeCalls.mock.calls.length;

    const ctx2 = createMockCtx();
    const strokeCalls2 = vi.fn();
    ctx2.stroke = strokeCalls2;
    const renderer2 = new CanvasRenderer(ctx2);

    renderer2.render({
      ...baseParams,
      previewData: mockPreviewData,
      showPreview: true,
    });

    // With preview, should have more stroke calls
    expect(strokeCalls2.mock.calls.length).toBeGreaterThan(countWithoutPreview);
  });

  it('keeps preview overlay visible during idle node edit', () => {
    const strokeCalls = vi.fn();
    ctx.stroke = strokeCalls;

    renderer.render({
      ...baseParams,
      previewData: mockPreviewData,
      showPreview: true,
      toolOverlay: {
        type: 'node-edit',
        objectId: 'path1',
        paths: [],
        selectedTargets: [],
        primaryTarget: null,
        suspendPreview: false,
      },
    });

    expect(strokeCalls.mock.calls.length).toBeGreaterThan(0);
  });

  it('suppresses preview overlay only while node edit drag is active', () => {
    const strokeCalls = vi.fn();
    ctx.stroke = strokeCalls;

    renderer.render({
      ...baseParams,
      previewData: mockPreviewData,
      showPreview: true,
      toolOverlay: {
        type: 'node-edit',
        objectId: 'path1',
        paths: [],
        selectedTargets: [],
        primaryTarget: null,
        suspendPreview: true,
      },
    });

    const countWithNodeEdit = strokeCalls.mock.calls.length;

    const ctx2 = createMockCtx();
    const strokeCalls2 = vi.fn();
    ctx2.stroke = strokeCalls2;
    const renderer2 = new CanvasRenderer(ctx2);
    renderer2.render({
      ...baseParams,
      previewData: mockPreviewData,
      showPreview: true,
      toolOverlay: { type: 'none' },
    });

    expect(strokeCalls2.mock.calls.length).toBeGreaterThan(countWithNodeEdit);
  });

  it('draws in-flight node edits from nodeToWorld mapping instead of rebasing temp path data', () => {
    const obj: ProjectObject = makeProjectObject({
      id: 'path1',
      name: 'Path',
      bounds: { min: { x: 10, y: 10 }, max: { x: 50, y: 50 } },
      layer_id: 'layer1',
      data: { type: 'vector_path', path_data: 'M 0 0 L 10 0 L 10 10 Z', closed: true },
    });
    const layer: Layer = makeLayer({
      id: 'layer1',
      name: 'Layer 1',
      operation: 'cut',
      color_tag: '#000000',
      power_percent: 100,
    });
    const paths: EditablePath[] = [
      {
        closed: true,
        nodes: [
          {
            id: { subpath_idx: 0, command_idx: 0 },
            position: { x: 0, y: 0 },
            handle_in: null,
            handle_out: null,
            node_type: 'corner',
          },
          {
            id: { subpath_idx: 0, command_idx: 1 },
            position: { x: 10, y: 0 },
            handle_in: null,
            handle_out: null,
            node_type: 'corner',
          },
          {
            id: { subpath_idx: 0, command_idx: 2 },
            position: { x: 10, y: 10 },
            handle_in: null,
            handle_out: null,
            node_type: 'corner',
          },
        ],
      },
    ];

    renderer.render({
      ...baseParams,
      objects: [obj],
      layers: [layer],
      toolOverlay: {
        type: 'node-edit',
        objectId: 'path1',
        paths,
        selectedTargets: [],
        primaryTarget: null,
        nodeToWorld: (p) => ({ x: p.x + 100, y: p.y + 200 }),
      },
    });

    const moveToCalls = (ctx.moveTo as ReturnType<typeof vi.fn>).mock.calls;
    expect(moveToCalls.length).toBeGreaterThan(0);
    const hasMappedStart = moveToCalls.some(
      (call) =>
        Math.abs((call[0] as number) - 200) < 0.001 && Math.abs((call[1] as number) - 400) < 0.001,
    );
    expect(hasMappedStart).toBe(true);
  });

  // --- F7: Theme system tests ---

  it('dark mode uses DARK_THEME canvasBg, light mode uses LIGHT_THEME canvasBg', () => {
    // Track fillStyle assignments
    const darkFillStyles: string[] = [];
    let darkCurrentFill = '';
    Object.defineProperty(ctx, 'fillStyle', {
      get: () => darkCurrentFill,
      set: (v: string) => {
        darkCurrentFill = v;
        darkFillStyles.push(v);
      },
    });

    renderer.render({ ...baseParams, theme: DARK_THEME });
    // First fillStyle assignment should be the background color
    expect(darkFillStyles[0]).toBe(DARK_THEME.canvasBg);

    // Light mode
    const ctx2 = createMockCtx();
    const lightFillStyles: string[] = [];
    let lightCurrentFill = '';
    Object.defineProperty(ctx2, 'fillStyle', {
      get: () => lightCurrentFill,
      set: (v: string) => {
        lightCurrentFill = v;
        lightFillStyles.push(v);
      },
    });
    const renderer2 = new CanvasRenderer(ctx2);
    renderer2.render({ ...baseParams, theme: LIGHT_THEME });
    expect(lightFillStyles[0]).toBe(LIGHT_THEME.canvasBg);
  });

  // --- F7: Antialiasing tests ---

  it('antialiasing toggles imageSmoothingEnabled', () => {
    renderer.render({ ...baseParams, antialiasing: false });
    expect(ctx.imageSmoothingEnabled).toBe(false);

    const ctx2 = createMockCtx();
    const renderer2 = new CanvasRenderer(ctx2);
    renderer2.render({ ...baseParams, antialiasing: true });
    expect(ctx2.imageSmoothingEnabled).toBe(true);
  });

  // --- F7: Filled rendering tests ---

  it('filled mode fills shapes', () => {
    const testObj: ProjectObject = makeProjectObject({
      id: 'obj1',
      name: 'rect',
      bounds: { min: { x: 10, y: 10 }, max: { x: 50, y: 50 } },
      layer_id: 'layer1',
      data: { type: 'shape', kind: 'rectangle', corner_radius: 0, width: 40, height: 40 },
    });
    const testLayer: Layer = makeLayer({
      id: 'layer1',
      name: 'Layer 1',
      operation: 'cut',
      color_tag: '#ff0000',
      power_percent: 100,
    });

    // Without filled: shape should not issue a fill() call
    const fillRectSpy = vi.fn();
    const fillSpy = vi.fn();
    ctx.fillRect = fillRectSpy;
    ctx.fill = fillSpy;
    renderer.render({
      ...baseParams,
      objects: [testObj],
      layers: [testLayer],
      filledRendering: false,
    });
    const fillCountWithout = fillSpy.mock.calls.length;

    // With filled: the shape should issue an actual fill() draw
    const ctx2 = createMockCtx();
    const fillSpy2 = vi.fn();
    ctx2.fill = fillSpy2;
    const renderer2 = new CanvasRenderer(ctx2);
    renderer2.render({
      ...baseParams,
      objects: [testObj],
      layers: [testLayer],
      filledRendering: true,
    });
    expect(fillSpy2.mock.calls.length).toBeGreaterThan(fillCountWithout);
  });

  // --- F7: Locked objects tests ---

  it('locked objects get dimmed selection with no handles', () => {
    const lockedObj: ProjectObject = makeProjectObject({
      id: 'locked1',
      name: 'locked rect',
      locked: true,
      bounds: { min: { x: 10, y: 10 }, max: { x: 50, y: 50 } },
      layer_id: 'layer1',
      data: { type: 'shape', kind: 'rectangle', corner_radius: 0, width: 40, height: 40 },
    });
    const testLayer: Layer = makeLayer({
      id: 'layer1',
      name: 'Layer 1',
      operation: 'cut',
      color_tag: '#ff0000',
      power_percent: 100,
    });

    // Track globalAlpha assignments
    const alphaValues: number[] = [];
    const originalCtx = createMockCtx();
    let currentAlpha = 1;
    Object.defineProperty(originalCtx, 'globalAlpha', {
      get: () => currentAlpha,
      set: (v: number) => {
        currentAlpha = v;
        alphaValues.push(v);
      },
    });
    const lockedRenderer = new CanvasRenderer(originalCtx);
    lockedRenderer.render({
      ...baseParams,
      objects: [lockedObj],
      layers: [testLayer],
      selectedObjectIds: ['locked1'],
    });

    // globalAlpha should have been set to 0.5 during selection drawing
    expect(alphaValues).toContain(0.5);

    // Handles should NOT be drawn for locked-only selection (no fillRect for handle squares)
    // Compare with unlocked version
    const unlockedObj = { ...lockedObj, locked: false };
    const ctx3 = createMockCtx();
    const renderer3 = new CanvasRenderer(ctx3);
    renderer3.render({
      ...baseParams,
      objects: [unlockedObj],
      layers: [testLayer],
      selectedObjectIds: ['locked1'],
    });
    // Unlocked should have more fillRect calls (handles drawn)
    expect((ctx3.fillRect as ReturnType<typeof vi.fn>).mock.calls.length).toBeGreaterThan(
      (originalCtx.fillRect as ReturnType<typeof vi.fn>).mock.calls.length,
    );
  });

  // --- F7: Tool layer tests ---

  it('tool layer uses dashed stroke and reduced alpha', () => {
    const toolObj: ProjectObject = makeProjectObject({
      id: 'tool1',
      name: 'tool rect',
      bounds: { min: { x: 10, y: 10 }, max: { x: 50, y: 50 } },
      layer_id: 'tool-layer',
      data: { type: 'shape', kind: 'rectangle', corner_radius: 0, width: 40, height: 40 },
    });
    const toolLayer: Layer = makeLayer({
      id: 'tool-layer',
      name: 'Tool Layer',
      operation: 'cut',
      color_tag: '#00ff00',
      power_percent: 100,
      is_tool_layer: true,
    });

    // Track globalAlpha and setLineDash
    const alphaValues: number[] = [];
    let currentAlpha = 1;
    Object.defineProperty(ctx, 'globalAlpha', {
      get: () => currentAlpha,
      set: (v: number) => {
        currentAlpha = v;
        alphaValues.push(v);
      },
    });

    renderer.render({
      ...baseParams,
      objects: [toolObj],
      layers: [toolLayer],
    });

    // Should have set globalAlpha to 0.6 for tool layer
    expect(alphaValues).toContain(0.6);

    // Should have called setLineDash with tool-layer pattern [8, 4]
    const dashCalls = (ctx.setLineDash as ReturnType<typeof vi.fn>).mock.calls;
    const hasToolDash = dashCalls.some(
      (call: number[][]) => call[0]?.length === 2 && call[0][0] === 8 && call[0][1] === 4,
    );
    expect(hasToolDash).toBe(true);
  });

  // --- View style rendering settings derivation test ---

  it('rendering settings derive from view style', () => {
    expect(renderOptionsFromViewStyle('wireframe_coarse')).toEqual({
      antialiasing: false,
      filledRendering: false,
    });
    expect(renderOptionsFromViewStyle('wireframe_smooth')).toEqual({
      antialiasing: true,
      filledRendering: false,
    });
    expect(renderOptionsFromViewStyle('filled_coarse')).toEqual({
      antialiasing: false,
      filledRendering: true,
    });
    expect(renderOptionsFromViewStyle('filled_smooth')).toEqual({
      antialiasing: true,
      filledRendering: true,
    });
  });

  // --- Barcode rendering test ---

  it('renders polygon objects on the canvas', () => {
    const polygonObj: ProjectObject = makeProjectObject({
      id: 'poly1',
      name: 'Polygon',
      bounds: { min: { x: 10, y: 10 }, max: { x: 50, y: 50 } },
      layer_id: 'layer1',
      data: { type: 'polygon', sides: 6, radius: 20 },
    });
    const testLayer: Layer = makeLayer({
      id: 'layer1',
      name: 'Layer 1',
      operation: 'cut',
      color_tag: '#ff0000',
      power_percent: 100,
    });

    expect(() =>
      renderer.render({
        ...baseParams,
        objects: [polygonObj],
        layers: [testLayer],
      }),
    ).not.toThrow();

    expect(ctx.lineTo).toHaveBeenCalled();
    expect(ctx.stroke).toHaveBeenCalled();
  });

  it('renders barcode object with cached path data', () => {
    const barcodeObj: ProjectObject = makeProjectObject({
      id: 'barcode1',
      name: 'QR Code',
      bounds: { min: { x: 10, y: 10 }, max: { x: 50, y: 50 } },
      layer_id: 'layer1',
      data: { type: 'barcode', barcode_type: 'qr_code', data: 'hello', width: 40, height: 40 },
    });
    const testLayer: Layer = makeLayer({
      id: 'layer1',
      name: 'Layer 1',
      operation: 'cut',
      color_tag: '#ff0000',
      power_percent: 100,
    });

    // Pre-populate barcode path cache
    renderer.barcodePathCache.set(
      'qr_code:hello:40:40:{"show_text":false,"qr_error_correction":"medium","data_matrix_force_square":false}',
      'M 0 0 L 10 0 L 10 10 L 0 10 Z',
    );

    expect(() =>
      renderer.render({
        ...baseParams,
        objects: [barcodeObj],
        layers: [testLayer],
      }),
    ).not.toThrow();

    // Stroke should have been called (for the barcode path)
    expect(ctx.stroke).toHaveBeenCalled();
    // Fill should have been called (barcodes are filled)
    expect(ctx.fill).toHaveBeenCalled();
  });

  it('renders barcode placeholder when path not cached', () => {
    const barcodeObj: ProjectObject = makeProjectObject({
      id: 'barcode2',
      name: 'QR Code',
      bounds: { min: { x: 10, y: 10 }, max: { x: 50, y: 50 } },
      layer_id: 'layer1',
      data: { type: 'barcode', barcode_type: 'qr_code', data: 'test', width: 40, height: 40 },
    });
    const testLayer: Layer = makeLayer({
      id: 'layer1',
      name: 'Layer 1',
      operation: 'cut',
      color_tag: '#ff0000',
      power_percent: 100,
    });

    // Don't populate cache — should render placeholder
    expect(() =>
      renderer.render({
        ...baseParams,
        objects: [barcodeObj],
        layers: [testLayer],
      }),
    ).not.toThrow();

    // fillText should have been called with "Barcode" placeholder
    expect(ctx.fillText).toHaveBeenCalledWith('Barcode', expect.any(Number), expect.any(Number));
  });

  it('caches barcode generation failures and does not retry the same broken barcode', async () => {
    const barcodeObj: ProjectObject = makeProjectObject({
      id: 'barcode3',
      name: 'Broken QR',
      bounds: { min: { x: 10, y: 10 }, max: { x: 50, y: 50 } },
      layer_id: 'layer1',
      data: { type: 'barcode', barcode_type: 'qr_code', data: 'broken', width: 40, height: 40 },
    });
    const testLayer: Layer = makeLayer({
      id: 'layer1',
      name: 'Layer 1',
      operation: 'cut',
      color_tag: '#ff0000',
      power_percent: 100,
    });
    const push = vi.fn();
    useNotificationStore.setState({ push });
    vi.mocked(invoke).mockRejectedValue(new Error('bad barcode'));

    renderer.render({
      ...baseParams,
      objects: [barcodeObj],
      layers: [testLayer],
    });
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(vi.mocked(invoke)).toHaveBeenCalledTimes(1);
    expect(push).toHaveBeenCalledWith('Failed to render barcode: Error: bad barcode', 'error');

    renderer.render({
      ...baseParams,
      objects: [barcodeObj],
      layers: [testLayer],
    });
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(vi.mocked(invoke)).toHaveBeenCalledTimes(1);
  });

  it('dispose clears raster and barcode caches', () => {
    (
      renderer as unknown as {
        imageCache: Map<string, unknown>;
        imageErrorCache: Map<string, string>;
        barcodeLoading: Set<string>;
      }
    ).imageCache.set('asset-1', document.createElement('canvas'));
    (
      renderer as unknown as {
        imageCache: Map<string, unknown>;
        imageErrorCache: Map<string, string>;
        barcodeLoading: Set<string>;
      }
    ).imageErrorCache.set('asset-2', 'missing');
    renderer.barcodePathCache.set('barcode-key', 'M 0 0 Z');
    renderer.barcodeErrorCache.set('barcode-error', 'bad barcode');
    (
      renderer as unknown as {
        imageCache: Map<string, unknown>;
        imageErrorCache: Map<string, string>;
        barcodeLoading: Set<string>;
      }
    ).barcodeLoading.add('barcode-loading');

    renderer.dispose();

    expect((renderer as unknown as { imageCache: Map<string, unknown> }).imageCache.size).toBe(0);
    expect(
      (renderer as unknown as { imageErrorCache: Map<string, string> }).imageErrorCache.size,
    ).toBe(0);
    expect(renderer.barcodePathCache.size).toBe(0);
    expect(renderer.barcodeErrorCache.size).toBe(0);
    expect((renderer as unknown as { barcodeLoading: Set<string> }).barcodeLoading.size).toBe(0);
  });

  // --- Pen preview overlay test ---

  it('pen-preview overlay renders without error', () => {
    expect(() =>
      renderer.render({
        ...baseParams,
        toolOverlay: {
          type: 'pen-preview',
          points: [
            { anchor: { x: 10, y: 10 }, handleOut: { x: 20, y: 10 } },
            { anchor: { x: 50, y: 50 }, handleIn: { x: 40, y: 50 } },
          ],
          screenPoints: [
            { anchor: { x: 100, y: 100 }, handleOut: { x: 150, y: 100 } },
            { anchor: { x: 300, y: 300 }, handleIn: { x: 250, y: 300 } },
          ],
          currentScreen: { x: 400, y: 200 },
          dragging: false,
          closed: false,
        },
      }),
    ).not.toThrow();

    // bezierCurveTo should have been called for the curve segments
    expect(ctx.beginPath).toHaveBeenCalled();
    expect(ctx.stroke).toHaveBeenCalled();
  });

  // --- F7: Reduce motion test ---

  it('reduce motion keeps dashOffset at zero', () => {
    // When reduce_motion is true, the Canvas.tsx animation loop should not run
    // and dashOffset stays 0. Test that renderer correctly applies dashOffset=0.
    const testObj: ProjectObject = makeProjectObject({
      id: 'obj1',
      name: 'rect',
      bounds: { min: { x: 10, y: 10 }, max: { x: 50, y: 50 } },
      layer_id: 'layer1',
      data: { type: 'shape', kind: 'rectangle', corner_radius: 0, width: 40, height: 40 },
    });
    const testLayer: Layer = makeLayer({
      id: 'layer1',
      name: 'Layer 1',
      operation: 'cut',
      color_tag: '#ff0000',
      power_percent: 100,
    });

    // Track lineDashOffset assignments
    const offsetValues: number[] = [];
    let currentOffset = 0;
    Object.defineProperty(ctx, 'lineDashOffset', {
      get: () => currentOffset,
      set: (v: number) => {
        currentOffset = v;
        offsetValues.push(v);
      },
    });

    renderer.render({
      ...baseParams,
      objects: [testObj],
      layers: [testLayer],
      selectedObjectIds: ['obj1'],
      selectionDashOffset: 0,
    });

    // All lineDashOffset assignments should be 0 (no animation)
    for (const v of offsetValues) {
      expect(v).toBe(0);
    }
  });
});
