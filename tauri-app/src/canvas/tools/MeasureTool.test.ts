import { beforeEach, describe, expect, it, vi } from 'vitest';
import { MeasureTool } from './MeasureTool';
import type { CanvasMouseEvent, ToolContext } from './types';
import type { ViewportParams } from '../ViewportTransform';
import { worldToScreen } from '../ViewportTransform';
import { useMeasurementStore } from '../../stores/measurementStore';
import { makeProjectObject } from '../../test-utils/projectFixtures';

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

describe('MeasureTool', () => {
  let tool: MeasureTool;

  beforeEach(() => {
    tool = new MeasureTool();
    useMeasurementStore.getState().clear();
    vi.clearAllMocks();
  });

  it('sets hover measurement for visible geometry and nearest segment', () => {
    const object = makeProjectObject({
      id: 'rect-1',
      name: 'Measured Rect',
      bounds: { min: { x: 0, y: 0 }, max: { x: 20, y: 10 } },
    });
    const ctx = makeToolContext({ objects: [object] });

    tool.onMouseMove(eventAtWorld({ x: 10, y: 0 }), ctx);

    const state = useMeasurementStore.getState().state;
    expect(state.type).toBe('hover');
    if (state.type === 'hover') {
      expect(state.objectId).toBe('rect-1');
      expect(state.objectMetrics.widthMm).toBeCloseTo(20);
      expect(state.objectMetrics.heightMm).toBeCloseTo(10);
      expect(state.objectMetrics.areaMm2).toBeCloseTo(200);
      expect(state.segment?.lengthMm).toBeCloseTo(20);
    }

    const overlay = tool.getOverlay();
    expect(overlay.type).toBe('measure-inspection');
    if (overlay.type === 'measure-inspection') {
      expect(overlay.hoverObjectId).toBe('rect-1');
      expect(overlay.hoverSegment?.lengthMm).toBeCloseTo(20);
    }
  });

  it('ignores objects on hidden layers', () => {
    const object = makeProjectObject({
      id: 'hidden-layer-object',
      bounds: { min: { x: 0, y: 0 }, max: { x: 20, y: 10 } },
    });
    const ctx = makeToolContext({
      objects: [object],
      layers: [{ id: 'layer-1', enabled: true, visible: false }],
    });

    tool.onMouseMove(eventAtWorld({ x: 10, y: 0 }), ctx);

    expect(useMeasurementStore.getState().state.type).toBe('idle');
  });

  it('measures locked but visible objects on hover', () => {
    const object = makeProjectObject({
      id: 'locked-rect',
      locked: true,
      bounds: { min: { x: 0, y: 0 }, max: { x: 20, y: 10 } },
    });
    const ctx = makeToolContext({ objects: [object] });

    tool.onMouseMove(eventAtWorld({ x: 10, y: 0 }), ctx);

    const state = useMeasurementStore.getState().state;
    expect(state.type).toBe('hover');
    if (state.type === 'hover') {
      expect(state.objectId).toBe('locked-rect');
      expect(state.objectMetrics.widthMm).toBeCloseTo(20);
    }
  });

  it('tracks drag distance and angle through the measurement store', () => {
    const ctx = makeToolContext();

    tool.onMouseDown(eventAtWorld({ x: 0, y: 0 }), ctx);
    tool.onMouseMove(eventAtWorld({ x: 30, y: 40 }), ctx);

    const state = useMeasurementStore.getState().state;
    expect(state.type).toBe('drag');
    if (state.type === 'drag') {
      expect(state.lengthMm).toBeCloseTo(50);
      expect(state.angleDeg).toBeCloseTo(53.13, 2);
    }

    const overlay = tool.getOverlay();
    expect(overlay.type).toBe('measure-inspection');
    if (overlay.type === 'measure-inspection') {
      expect(overlay.drag?.lengthMm).toBeCloseTo(50);
    }
  });

  it('uses snapped drag endpoints instead of raw cursor coordinates', () => {
    const ctx = makeToolContext();

    tool.onMouseDown(eventAtWorld({ x: 1.3, y: 1.2 }, { snappedX: 0, snappedY: 0 }), ctx);
    tool.onMouseMove(eventAtWorld({ x: 9.7, y: 0.4 }, { snappedX: 10, snappedY: 0 }), ctx);

    const state = useMeasurementStore.getState().state;
    expect(state.type).toBe('drag');
    if (state.type === 'drag') {
      expect(state.start).toEqual({ x: 0, y: 0 });
      expect(state.end).toEqual({ x: 10, y: 0 });
      expect(state.lengthMm).toBeCloseTo(10);
    }
  });

  it('shift constrains drag to the nearest 45-degree angle', () => {
    const ctx = makeToolContext();

    tool.onMouseDown(eventAtWorld({ x: 0, y: 0 }), ctx);
    tool.onMouseMove(eventAtWorld({ x: 13, y: 3 }, { shiftKey: true }), ctx);

    const state = useMeasurementStore.getState().state;
    expect(state.type).toBe('drag');
    if (state.type === 'drag') {
      expect(state.end.y).toBeCloseTo(0);
      expect(state.angleDeg).toBeCloseTo(0);
    }
  });

  it('clears a non-zero drag on mouse up', () => {
    const ctx = makeToolContext();

    tool.onMouseDown(eventAtWorld({ x: 0, y: 0 }), ctx);
    tool.onMouseMove(eventAtWorld({ x: 10, y: 0 }), ctx);
    tool.onMouseUp(eventAtWorld({ x: 10, y: 0 }), ctx);

    expect(useMeasurementStore.getState().state.type).toBe('idle');
  });

  it('zero-distance click resolves back to hover under the cursor', () => {
    const object = makeProjectObject({
      id: 'rect-1',
      bounds: { min: { x: 0, y: 0 }, max: { x: 20, y: 10 } },
    });
    const ctx = makeToolContext({ objects: [object] });
    const event = eventAtWorld({ x: 10, y: 0 });

    tool.onMouseDown(event, ctx);
    tool.onMouseUp(event, ctx);

    const state = useMeasurementStore.getState().state;
    expect(state.type).toBe('hover');
    if (state.type === 'hover') {
      expect(state.objectId).toBe('rect-1');
    }
  });

  it('reset clears measurement state', () => {
    const ctx = makeToolContext();
    tool.onMouseDown(eventAtWorld({ x: 0, y: 0 }), ctx);

    tool.reset();

    expect(useMeasurementStore.getState().state.type).toBe('idle');
  });
});
