import { describe, it, expect, vi } from 'vitest';
import {
  drawSelectionHighlight,
  drawHoveredSegment,
  drawNodeHandles,
  drawPolygonPreview,
  drawShapePreview,
  drawStarPreview,
  getSelectionHandles,
} from './drawSelection';
import { DARK_THEME } from './constants';
import type { ProjectObject, Transform2D, TransformLocks } from '../types/project';
import type { ViewportParams } from './ViewportTransform';
import type { EditablePath } from '../types/vector';
import { makeProjectObject } from '../test-utils/projectFixtures';

const identity: Transform2D = { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 };

const defaultVp: ViewportParams = {
  offset: { x: 200, y: 200 },
  zoom: 100,
  canvasWidth: 800,
  canvasHeight: 600,
};

function makeObj(overrides?: Partial<ProjectObject>): ProjectObject {
  return makeProjectObject({
    id: 'obj-1',
    name: 'Rect',
    transform: identity,
    bounds: { min: { x: 150, y: 150 }, max: { x: 250, y: 250 } },
    layer_id: 'layer-1',
    data: { type: 'shape', kind: 'rectangle', width: 100, height: 100, corner_radius: 0 },
    ...overrides,
  });
}

function makeLineObj(overrides?: Partial<ProjectObject>): ProjectObject {
  return makeProjectObject({
    id: 'line-1',
    name: 'Line',
    transform: identity,
    bounds: { min: { x: 150, y: 100 }, max: { x: 250, y: 100 } },
    layer_id: 'layer-1',
    data: {
      type: 'vector_path',
      path_data: 'M 0 0 L 100 0',
      closed: false,
      ruler_guide_axis: null,
    },
    ...overrides,
  });
}

describe('getSelectionHandles', () => {
  it('generates 15 handles for unlocked object with no lock restrictions', () => {
    const obj = makeObj();
    const handles = getSelectionHandles([obj], defaultVp);
    // 8 resize + 1 center + 4 rotate + 2 shear = 15
    expect(handles.length).toBe(15);

    const ids = handles.map((h) => h.id);
    // Resize handles
    expect(ids).toContain('nw');
    expect(ids).toContain('n');
    expect(ids).toContain('ne');
    expect(ids).toContain('w');
    expect(ids).toContain('e');
    expect(ids).toContain('sw');
    expect(ids).toContain('s');
    expect(ids).toContain('se');
    // Center handle
    expect(ids).toContain('center');
    // Rotation handles
    expect(ids).toContain('rotate_nw');
    expect(ids).toContain('rotate_ne');
    expect(ids).toContain('rotate_sw');
    expect(ids).toContain('rotate_se');
    // Shear handles
    expect(ids).toContain('shear_n');
    expect(ids).toContain('shear_e');
  });

  it('hides resize handles when size_enabled is false', () => {
    const obj = makeObj();
    const locks: TransformLocks = { move_enabled: true, size_enabled: false, rotate_enabled: true, shear_enabled: true };
    const handles = getSelectionHandles([obj], defaultVp, locks);
    // Without resize: 1 center + 4 rotate + 2 shear = 7
    expect(handles.length).toBe(7);

    const ids = handles.map((h) => h.id);
    expect(ids).not.toContain('nw');
    expect(ids).not.toContain('n');
    expect(ids).not.toContain('ne');
    expect(ids).not.toContain('w');
    expect(ids).not.toContain('e');
    expect(ids).not.toContain('sw');
    expect(ids).not.toContain('s');
    expect(ids).not.toContain('se');
    expect(ids).toContain('center');
    expect(ids).toContain('rotate_nw');
    expect(ids).toContain('shear_n');
  });

  it('hides rotation handles when rotate_enabled is false', () => {
    const obj = makeObj();
    const locks: TransformLocks = { move_enabled: true, size_enabled: true, rotate_enabled: false, shear_enabled: true };
    const handles = getSelectionHandles([obj], defaultVp, locks);
    // Without rotation: 8 resize + 1 center + 2 shear = 11
    expect(handles.length).toBe(11);

    const ids = handles.map((h) => h.id);
    expect(ids).not.toContain('rotate_nw');
    expect(ids).not.toContain('rotate_ne');
    expect(ids).not.toContain('rotate_sw');
    expect(ids).not.toContain('rotate_se');
    expect(ids).toContain('nw');
    expect(ids).toContain('center');
    expect(ids).toContain('shear_n');
  });

  it('hides shear handles when shear_enabled is false', () => {
    const obj = makeObj();
    const locks: TransformLocks = { move_enabled: true, size_enabled: true, rotate_enabled: true, shear_enabled: false };
    const handles = getSelectionHandles([obj], defaultVp, locks);
    // Without shear: 8 resize + 1 center + 4 rotate = 13
    expect(handles.length).toBe(13);

    const ids = handles.map((h) => h.id);
    expect(ids).not.toContain('shear_n');
    expect(ids).not.toContain('shear_e');
    expect(ids).toContain('nw');
    expect(ids).toContain('center');
    expect(ids).toContain('rotate_nw');
  });

  it('hides center handle when move_enabled is false', () => {
    const obj = makeObj();
    const locks: TransformLocks = { move_enabled: false, size_enabled: true, rotate_enabled: true, shear_enabled: true };
    const handles = getSelectionHandles([obj], defaultVp, locks);
    // Without center: 8 resize + 4 rotate + 2 shear = 14
    expect(handles.length).toBe(14);

    const ids = handles.map((h) => h.id);
    expect(ids).not.toContain('center');
    expect(ids).toContain('nw');
    expect(ids).toContain('rotate_nw');
    expect(ids).toContain('shear_n');
  });

  it('uses computeVisualBoundsWorld for selection box — rotated object extends handles', () => {
    // A 10x10 rectangle at (100,100)-(110,110) rotated 45 degrees
    const cos45 = Math.cos(Math.PI / 4);
    const sin45 = Math.sin(Math.PI / 4);
    const rotatedObj = makeObj({
      bounds: { min: { x: 100, y: 100 }, max: { x: 110, y: 110 } },
      transform: { a: cos45, b: sin45, c: -sin45, d: cos45, tx: 0, ty: 0 },
      data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 },
    });

    // Unrotated version for comparison
    const unrotatedObj = makeObj({
      bounds: { min: { x: 100, y: 100 }, max: { x: 110, y: 110 } },
      transform: identity,
      data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 },
    });

    const rotatedHandles = getSelectionHandles([rotatedObj], defaultVp);
    const unrotatedHandles = getSelectionHandles([unrotatedObj], defaultVp);

    // Find the NW handle in each set — rotated should be further out
    const rotNw = rotatedHandles.find((h) => h.id === 'nw')!;
    const unrotNw = unrotatedHandles.find((h) => h.id === 'nw')!;

    // Rotated NW handle should be at a smaller screenX and screenY (further top-left)
    // because the visual bounds are larger
    expect(rotNw.screenX).toBeLessThan(unrotNw.screenX);
    expect(rotNw.screenY).toBeLessThan(unrotNw.screenY);
  });

  it('returns empty array for no objects', () => {
    const handles = getSelectionHandles([], defaultVp);
    expect(handles.length).toBe(0);
  });

  it('keeps flat-line resize handles off the line while center stays on it', () => {
    const line = makeLineObj();
    const handles = getSelectionHandles([line], defaultVp);
    const center = handles.find((h) => h.id === 'center')!;
    const north = handles.find((h) => h.id === 'n')!;
    const south = handles.find((h) => h.id === 's')!;
    const west = handles.find((h) => h.id === 'w')!;
    const east = handles.find((h) => h.id === 'e')!;

    expect(center.screenX).toBe(400);
    expect(center.screenY).toBe(100);
    expect(north.screenY).toBeLessThan(center.screenY);
    expect(south.screenY).toBeGreaterThan(center.screenY);
    expect(west.screenX).toBeLessThan(center.screenX);
    expect(east.screenX).toBeGreaterThan(center.screenX);
  });

  it('suppresses resize/rotate/shear handles for a single ruler guide', () => {
    const guide = makeObj({
      bounds: { min: { x: 150, y: 150 }, max: { x: 150, y: 250 } },
      data: {
        type: 'vector_path',
        path_data: 'M 0 0 L 0 100',
        closed: false,
        ruler_guide_axis: 'vertical',
      },
    });

    const handles = getSelectionHandles([guide], defaultVp);
    expect(handles.map((h) => h.id)).toEqual(['center']);
  });

  it('draws outlines but no handles when a mixed selection contains a locked object', () => {
    const unlocked = makeObj({ id: 'unlocked' });
    const locked = makeObj({
      id: 'locked',
      locked: true,
      bounds: { min: { x: 260, y: 150 }, max: { x: 300, y: 190 } },
    });
    const ctx = {
      globalAlpha: 1,
      strokeStyle: '',
      fillStyle: '',
      lineWidth: 1,
      lineDashOffset: 0,
      save: vi.fn(),
      restore: vi.fn(),
      setLineDash: vi.fn(),
      strokeRect: vi.fn(),
      fillRect: vi.fn(),
    } as unknown as CanvasRenderingContext2D;

    drawSelectionHighlight(ctx, [unlocked, locked], defaultVp, DARK_THEME);

    expect(ctx.strokeRect).toHaveBeenCalledTimes(2);
    expect(ctx.fillRect).not.toHaveBeenCalled();
  });
});

describe('drawShapePreview', () => {
  it('uses a rounded rectangle preview when a corner radius is provided', () => {
    const ctx = {
      fillStyle: '',
      strokeStyle: '',
      lineWidth: 1,
      fillRect: vi.fn(),
      strokeRect: vi.fn(),
      roundRect: vi.fn(),
      beginPath: vi.fn(),
      fill: vi.fn(),
      stroke: vi.fn(),
      ellipse: vi.fn(),
    } as unknown as CanvasRenderingContext2D;

    drawShapePreview(
      ctx,
      { x: 10, y: 20 },
      { x: 110, y: 70 },
      'rectangle',
      8,
    );

    expect(ctx.roundRect).toHaveBeenCalledWith(10, 20, 100, 50, 8);
    expect(ctx.fillRect).not.toHaveBeenCalled();
    expect(ctx.strokeRect).not.toHaveBeenCalled();
  });
});

function makePathPreviewContext() {
  return {
    fillStyle: '',
    strokeStyle: '',
    lineWidth: 1,
    beginPath: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    quadraticCurveTo: vi.fn(),
    closePath: vi.fn(),
    fill: vi.fn(),
    stroke: vi.fn(),
    save: vi.fn(),
    restore: vi.fn(),
  };
}

function collectPathPoints(ctx: ReturnType<typeof makePathPreviewContext>): { x: number; y: number }[] {
  return [
    ...ctx.moveTo.mock.calls.map(([x, y]) => ({ x, y })),
    ...ctx.lineTo.mock.calls.map(([x, y]) => ({ x, y })),
    ...ctx.quadraticCurveTo.mock.calls.flatMap(([qx, qy, x, y]) => [
      { x: qx, y: qy },
      { x, y },
    ]),
  ];
}

function expectPointExtentsToMatch(points: { x: number; y: number }[], minX: number, minY: number, maxX: number, maxY: number) {
  expect(Math.min(...points.map((p) => p.x))).toBeCloseTo(minX, 5);
  expect(Math.min(...points.map((p) => p.y))).toBeCloseTo(minY, 5);
  expect(Math.max(...points.map((p) => p.x))).toBeCloseTo(maxX, 5);
  expect(Math.max(...points.map((p) => p.y))).toBeCloseTo(maxY, 5);
}

describe('drawPolygonPreview', () => {
  it.each([3, 5])('normalizes %i-sided polygon path bounds to the dragged preview bounds', (sides) => {
    const ctx = makePathPreviewContext();

    drawPolygonPreview(
      ctx as unknown as CanvasRenderingContext2D,
      { x: 10, y: 20 },
      { x: 110, y: 120 },
      sides,
    );

    expectPointExtentsToMatch(collectPathPoints(ctx), 10, 20, 110, 120);
  });
});

describe('drawStarPreview', () => {
  it('normalizes star path bounds to the dragged preview bounds', () => {
    const ctx = makePathPreviewContext();

    drawStarPreview(
      ctx as unknown as CanvasRenderingContext2D,
      { x: 10, y: 20 },
      { x: 110, y: 120 },
      5,
    );

    expectPointExtentsToMatch(collectPathPoints(ctx), 10, 20, 110, 120);
  });
});

describe('drawNodeHandles', () => {
  it('draws a selected handle even when its node is not selected', () => {
    const ctx = {
      fillStyle: '',
      strokeStyle: '',
      lineWidth: 1,
      beginPath: vi.fn(),
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      stroke: vi.fn(),
      arc: vi.fn(),
      fill: vi.fn(),
      fillRect: vi.fn(),
      strokeRect: vi.fn(),
    } as unknown as CanvasRenderingContext2D;
    const paths: EditablePath[] = [{
      closed: false,
      nodes: [{
        id: { subpath_idx: 0, command_idx: 0 },
        position: { x: 0, y: 0 },
        handle_in: null,
        handle_out: { x: 10, y: 0 },
        node_type: 'corner',
      }],
    }];

    drawNodeHandles(
      ctx,
      paths,
      [{ kind: 'handle', nodeId: { subpath_idx: 0, command_idx: 0 }, handleType: 'out' }],
      { offset: { x: 0, y: 0 }, zoom: 100, canvasWidth: 100, canvasHeight: 100 },
    );

    expect(ctx.lineTo).toHaveBeenCalled();
    expect(ctx.arc).toHaveBeenCalled();
  });
});

describe('drawHoveredSegment', () => {
  it('draws a hovered closing segment on the matching disconnected subpath', () => {
    const ctx = {
      fillStyle: '',
      strokeStyle: '',
      lineWidth: 1,
      beginPath: vi.fn(),
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      bezierCurveTo: vi.fn(),
      stroke: vi.fn(),
      arc: vi.fn(),
      fill: vi.fn(),
    } as unknown as CanvasRenderingContext2D;
    const paths: EditablePath[] = [
      {
        closed: true,
        nodes: [
          { id: { subpath_idx: 0, command_idx: 0 }, position: { x: 0, y: 0 }, handle_in: null, handle_out: null, node_type: 'corner' },
          { id: { subpath_idx: 0, command_idx: 1 }, position: { x: 30, y: 0 }, handle_in: null, handle_out: null, node_type: 'corner' },
          { id: { subpath_idx: 0, command_idx: 2 }, position: { x: 30, y: 10 }, handle_in: null, handle_out: null, node_type: 'corner' },
          { id: { subpath_idx: 0, command_idx: 3 }, position: { x: 0, y: 10 }, handle_in: null, handle_out: null, node_type: 'corner' },
        ],
      },
      {
        closed: true,
        nodes: [
          { id: { subpath_idx: 1, command_idx: 0 }, position: { x: 100, y: 100 }, handle_in: null, handle_out: null, node_type: 'corner' },
          { id: { subpath_idx: 1, command_idx: 1 }, position: { x: 130, y: 100 }, handle_in: null, handle_out: null, node_type: 'corner' },
          { id: { subpath_idx: 1, command_idx: 2 }, position: { x: 130, y: 120 }, handle_in: null, handle_out: null, node_type: 'corner' },
          { id: { subpath_idx: 1, command_idx: 3 }, position: { x: 100, y: 120 }, handle_in: null, handle_out: null, node_type: 'corner' },
        ],
      },
    ];
    const vp = { offset: { x: 0, y: 0 }, zoom: 100, canvasWidth: 0, canvasHeight: 0 };

    drawHoveredSegment(ctx, paths, { subpath_idx: 1, command_idx: 4 }, 0.5, vp);

    expect(ctx.moveTo).toHaveBeenCalledWith(200, 240);
    expect(ctx.lineTo).toHaveBeenCalledWith(200, 200);
    expect(ctx.moveTo).not.toHaveBeenCalledWith(0, 20);
  });
});
