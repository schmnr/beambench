import { beforeEach, describe, expect, it, vi } from 'vitest';
import { MeasureTool } from './MeasureTool';
import type { CanvasMouseEvent, ToolContext } from './types';
import type { ViewportParams } from '../ViewportTransform';
import { worldToScreen } from '../ViewportTransform';
import { useMeasurementStore } from '../../stores/measurementStore';
import { useUiStore } from '../../stores/uiStore';
import { makeProjectObject } from '../../test-utils/projectFixtures';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));

const defaultVp: ViewportParams = {
  offset: { x: 0, y: 0 },
  zoom: 100,
  canvasWidth: 100,
  canvasHeight: 100,
};

function makeMouseEvent(overrides: Partial<CanvasMouseEvent> = {}): CanvasMouseEvent {
  return {
    screenX: 0,
    screenY: 0,
    worldX: 0,
    worldY: 0,
    snappedX: 0,
    snappedY: 0,
    button: 0,
    shiftKey: false,
    ctrlKey: false,
    altKey: false,
    ...overrides,
  };
}

function makeToolContext(overrides: Partial<ToolContext> = {}): ToolContext {
  return {
    vp: defaultVp,
    objects: [],
    selectedObjectIds: [],
    selectedLayerId: 'layer-1',
    layers: [{ id: 'layer-1', enabled: true, visible: true }],
    transformLocks: { move_enabled: true, size_enabled: true, rotate_enabled: true, shear_enabled: true },
    snapEnabled: false,
    snapToObjects: false,
    gridSpacingMm: 10,
    selectObjects: vi.fn(),
    toggleObjectSelection: vi.fn(),
    addObject: vi.fn(),
    updateObject: vi.fn(),
    rotateObjects: vi.fn(),
    shearObjects: vi.fn(),
    updateObjectBoundsBatch: vi.fn(),
    setCursorWorldPos: vi.fn(),
    setStatusMessage: vi.fn(),
    requestRender: vi.fn(),
    ...overrides,
  };
}

function eventAtWorld(point: { x: number; y: number }, overrides: Partial<CanvasMouseEvent> = {}) {
  const screen = worldToScreen(point, defaultVp);
  return makeMouseEvent({
    worldX: point.x,
    worldY: point.y,
    snappedX: point.x,
    snappedY: point.y,
    screenX: screen.x,
    screenY: screen.y,
    ...overrides,
  });
}

function click(tool: MeasureTool, event: CanvasMouseEvent, ctx: ToolContext) {
  tool.onMouseDown(event, ctx);
  tool.onMouseUp(event, ctx);
}

describe('MeasureTool', () => {
  let tool: MeasureTool;

  beforeEach(() => {
    tool = new MeasureTool();
    useMeasurementStore.setState({ mode: 'linear' });
    useMeasurementStore.getState().clear();
    useUiStore.setState({ activeTool: 'measure', workspaceMode: 'design' });
    vi.clearAllMocks();
  });

  it('keeps hover inspection separate from the persistent result', () => {
    const object = makeProjectObject({
      id: 'rect-1',
      name: 'Measured Rect',
      bounds: { min: { x: 0, y: 0 }, max: { x: 20, y: 10 } },
    });
    const ctx = makeToolContext({ objects: [object] });

    tool.onMouseMove(eventAtWorld({ x: 10, y: 0 }), ctx);

    const hover = useMeasurementStore.getState().hover;
    expect(hover?.objectId).toBe('rect-1');
    expect(hover?.objectMetrics.areaMm2).toBeCloseTo(200);
    expect(hover?.segment?.lengthMm).toBeCloseTo(20);
    expect(useMeasurementStore.getState().result).toBeNull();
  });

  it('ignores hidden geometry but measures locked visible objects', () => {
    const hidden = makeProjectObject({ id: 'hidden', visible: false });
    const locked = makeProjectObject({
      id: 'locked',
      locked: true,
      bounds: { min: { x: 0, y: 0 }, max: { x: 20, y: 10 } },
    });
    const ctx = makeToolContext({ objects: [hidden, locked] });

    tool.onMouseMove(eventAtWorld({ x: 10, y: 0 }), ctx);

    expect(useMeasurementStore.getState().hover?.objectId).toBe('locked');
  });

  it('persists a click-drag measurement after mouse release', () => {
    const ctx = makeToolContext();

    tool.onMouseDown(eventAtWorld({ x: 0, y: 0 }), ctx);
    tool.onMouseMove(eventAtWorld({ x: 30, y: 40 }), ctx);
    expect(useMeasurementStore.getState().draft?.lengthMm).toBeCloseTo(50);
    tool.onMouseUp(eventAtWorld({ x: 30, y: 40 }), ctx);

    const result = useMeasurementStore.getState().result;
    expect(result?.kind).toBe('linear');
    if (result?.kind === 'linear') {
      expect(result.lengthMm).toBeCloseTo(50);
      expect(result.angleDeg).toBeCloseTo(53.13, 2);
    }
    expect(useMeasurementStore.getState().draft).toBeNull();
  });

  it('supports click-first-point then click-second-point', () => {
    const ctx = makeToolContext();

    click(tool, eventAtWorld({ x: 5, y: 5 }), ctx);
    expect(useMeasurementStore.getState().pending).toEqual({ kind: 'linear', start: { x: 5, y: 5 } });
    tool.onMouseMove(eventAtWorld({ x: 15, y: 5 }), ctx);
    expect(useMeasurementStore.getState().draft?.lengthMm).toBeCloseTo(10);
    click(tool, eventAtWorld({ x: 15, y: 5 }), ctx);

    const result = useMeasurementStore.getState().result;
    expect(result?.kind).toBe('linear');
    if (result?.kind === 'linear') expect(result.lengthMm).toBeCloseTo(10);
  });

  it('uses snapped endpoints and Shift constrains to 45-degree increments', () => {
    const ctx = makeToolContext();

    tool.onMouseDown(eventAtWorld({ x: 1.3, y: 1.2 }, { snappedX: 0, snappedY: 0 }), ctx);
    tool.onMouseMove(eventAtWorld({ x: 13, y: 3 }, { snappedX: 13, snappedY: 3, shiftKey: true }), ctx);
    tool.onMouseUp(eventAtWorld({ x: 13, y: 3 }, { snappedX: 13, snappedY: 3, shiftKey: true }), ctx);

    const result = useMeasurementStore.getState().result;
    expect(result?.kind).toBe('linear');
    if (result?.kind === 'linear') {
      expect(result.start).toEqual({ x: 0, y: 0 });
      expect(result.end.y).toBeCloseTo(0);
      expect(result.angleDeg).toBeCloseTo(0);
    }
  });

  it('measures the angle between two clicked segments', () => {
    const horizontal = makeProjectObject({
      id: 'horizontal',
      bounds: { min: { x: 0, y: 0 }, max: { x: 20, y: 10 } },
    });
    const vertical = makeProjectObject({
      id: 'vertical',
      bounds: { min: { x: 30, y: 0 }, max: { x: 40, y: 20 } },
    });
    const ctx = makeToolContext({ objects: [horizontal, vertical] });
    useMeasurementStore.getState().setMode('angle');

    click(tool, eventAtWorld({ x: 10, y: 0 }), ctx);
    expect(useMeasurementStore.getState().pending?.kind).toBe('angle');
    click(tool, eventAtWorld({ x: 30, y: 10 }), ctx);

    const result = useMeasurementStore.getState().result;
    expect(result?.kind).toBe('angle');
    if (result?.kind === 'angle') expect(result.angleDeg).toBeCloseTo(90);
  });

  it('reports radius and diameter for an ellipse object', () => {
    const circle = makeProjectObject({
      id: 'circle',
      data: { type: 'shape', kind: 'ellipse', width: 20, height: 20, corner_radius: 0 },
      bounds: { min: { x: 0, y: 0 }, max: { x: 20, y: 20 } },
    });
    const ctx = makeToolContext({ objects: [circle] });
    useMeasurementStore.getState().setMode('radius');

    click(tool, eventAtWorld({ x: 10, y: 10 }), ctx);

    const result = useMeasurementStore.getState().result;
    expect(result?.kind).toBe('radius');
    if (result?.kind === 'radius') {
      expect(result.radiusXmm).toBeCloseTo(10);
      expect(result.diameterXmm).toBeCloseTo(20);
      expect(result.circular).toBe(true);
    }
  });

  it('measures the closest gap between two objects', () => {
    const first = makeProjectObject({
      id: 'first',
      name: 'First',
      bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
    });
    const second = makeProjectObject({
      id: 'second',
      name: 'Second',
      bounds: { min: { x: 20, y: 0 }, max: { x: 30, y: 10 } },
    });
    const ctx = makeToolContext({ objects: [first, second] });
    useMeasurementStore.getState().setMode('gap');

    click(tool, eventAtWorld({ x: 5, y: 5 }), ctx);
    click(tool, eventAtWorld({ x: 25, y: 5 }), ctx);

    const result = useMeasurementStore.getState().result;
    expect(result?.kind).toBe('gap');
    if (result?.kind === 'gap') {
      expect(result.lengthMm).toBeCloseTo(10);
      expect(result.horizontalGapMm).toBeCloseTo(10);
      expect(result.verticalGapMm).toBeCloseTo(0);
    }
  });

  it('Escape clears a result first, then returns to Select', () => {
    const ctx = makeToolContext();
    tool.onMouseDown(eventAtWorld({ x: 0, y: 0 }), ctx);
    tool.onMouseMove(eventAtWorld({ x: 10, y: 0 }), ctx);
    tool.onMouseUp(eventAtWorld({ x: 10, y: 0 }), ctx);

    tool.onKeyDown?.(new KeyboardEvent('keydown', { key: 'Escape' }), ctx);
    expect(useMeasurementStore.getState().result).toBeNull();
    expect(useUiStore.getState().activeTool).toBe('measure');

    tool.onKeyDown?.(new KeyboardEvent('keydown', { key: 'Escape' }), ctx);
    expect(useUiStore.getState().activeTool).toBe('select');
  });

  it('reset clears transient and persistent measurement state', () => {
    const ctx = makeToolContext();
    tool.onMouseDown(eventAtWorld({ x: 0, y: 0 }), ctx);
    tool.onMouseMove(eventAtWorld({ x: 10, y: 0 }), ctx);
    tool.onMouseUp(eventAtWorld({ x: 10, y: 0 }), ctx);

    tool.reset();

    const state = useMeasurementStore.getState();
    expect(state.hover).toBeNull();
    expect(state.draft).toBeNull();
    expect(state.pending).toBeNull();
    expect(state.result).toBeNull();
  });
});
