import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { ProjectObject } from '../types/project';
import type { EditablePath } from '../types/vector';
import type { ViewportParams } from './ViewportTransform';
import { worldToScreen } from './ViewportTransform';
import {
  buildObjectScreenMaskPath,
  drawEditableVectorPath,
  drawVectorPath,
  drawRasterImage,
  drawPolygon,
  drawShape,
  drawText,
  editablePathsToSvgD,
  computePathBBox,
  getVectorPathRenderInfoForObject,
  parsePathData,
  resetVectorPathCachesForTests,
  resetTransformedPath2DProbeForTests,
  supportsTransformedPath2DFastPath,
  type RasterMaskRenderContext,
} from './drawObjects';
import { makeProjectObject, makeTextObjectData } from '../test-utils/projectFixtures';

function createMockCtx() {
  return {
    save: vi.fn(),
    restore: vi.fn(),
    beginPath: vi.fn(),
    rect: vi.fn(),
    roundRect: vi.fn(),
    ellipse: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    quadraticCurveTo: vi.fn(),
    bezierCurveTo: vi.fn(),
    closePath: vi.fn(),
    fill: vi.fn(),
    stroke: vi.fn(),
    drawImage: vi.fn(),
    strokeRect: vi.fn(),
    setLineDash: vi.fn(),
    fillText: vi.fn(),
    transform: vi.fn(),
    translate: vi.fn(),
    strokeStyle: '',
    fillStyle: '',
    lineWidth: 1,
    font: '',
    textAlign: 'left' as CanvasTextAlign,
    textBaseline: 'alphabetic' as CanvasTextBaseline,
  } as unknown as CanvasRenderingContext2D;
}

type MockCanvasContext = CanvasRenderingContext2D & {
  operations: string[];
  globalCompositeOperation: GlobalCompositeOperation;
};

class MockPath2D {
  ops: unknown[][] = [];
  constructor(d?: string) {
    if (d) this.ops.push(['d', d]);
  }
  moveTo(x: number, y: number) { this.ops.push(['moveTo', x, y]); }
  lineTo(x: number, y: number) { this.ops.push(['lineTo', x, y]); }
  quadraticCurveTo(x1: number, y1: number, x: number, y: number) {
    this.ops.push(['quadraticCurveTo', x1, y1, x, y]);
  }
  bezierCurveTo(x1: number, y1: number, x2: number, y2: number, x: number, y: number) {
    this.ops.push(['bezierCurveTo', x1, y1, x2, y2, x, y]);
  }
  closePath() { this.ops.push(['closePath']); }
  addPath(path: Path2D, matrix?: DOMMatrix2DInit) { this.ops.push(['addPath', path, matrix]); }
}

function createRasterMockCtx(operations: string[] = []): MockCanvasContext {
  let composite: GlobalCompositeOperation = 'source-over';
  const ctx = {
    ...createMockCtx(),
    operations,
    clearRect: vi.fn(),
    fillRect: vi.fn(),
    clip: vi.fn(),
    fill: vi.fn(() => operations.push(`fill:${composite}`)),
    drawImage: vi.fn(() => operations.push(`drawImage:${composite}`)),
    getImageData: vi.fn(() => ({ data: new Uint8ClampedArray(4) })),
  } as unknown as MockCanvasContext;
  Object.defineProperty(ctx, 'globalCompositeOperation', {
    get: () => composite,
    set: (value: GlobalCompositeOperation) => {
      composite = value;
      operations.push(`composite:${value}`);
    },
    configurable: true,
  });
  return ctx;
}

function withMockedCanvases<T>(fn: (contexts: MockCanvasContext[]) => T): T {
  const originalCreateElement = document.createElement.bind(document);
  const contexts: MockCanvasContext[] = [];
  document.createElement = vi.fn((tagName: string) => {
    if (tagName === 'canvas') {
      const ctx = createRasterMockCtx();
      contexts.push(ctx);
      return {
        width: 0,
        height: 0,
        getContext: vi.fn(() => ctx),
      } as unknown as HTMLCanvasElement;
    }
    return originalCreateElement(tagName);
  }) as typeof document.createElement;
  try {
    return fn(contexts);
  } finally {
    document.createElement = originalCreateElement;
  }
}

function extractPathPoints(ctx: CanvasRenderingContext2D): Array<{ x: number; y: number }> {
  const moveCalls = (ctx.moveTo as ReturnType<typeof vi.fn>).mock.calls.map((call) => ({
    x: call[0] as number,
    y: call[1] as number,
  }));
  const lineCalls = (ctx.lineTo as ReturnType<typeof vi.fn>).mock.calls.map((call) => ({
    x: call[0] as number,
    y: call[1] as number,
  }));
  return [...moveCalls, ...lineCalls];
}

beforeEach(() => {
  resetVectorPathCachesForTests();
  globalThis.Path2D = MockPath2D as unknown as typeof Path2D;
});

describe('drawPolygon', () => {
  it('keeps transformed polygons aligned to object bounds on canvas', () => {
    const ctx = createMockCtx();
    const vp: ViewportParams = {
      offset: { x: 160, y: 125 },
      zoom: 100,
      canvasWidth: 800,
      canvasHeight: 600,
    };

    const obj: ProjectObject = makeProjectObject({
      id: 'poly-1',
      name: 'Hexagon',
      transform: {
        a: 0.92,
        b: 0.28,
        c: -0.18,
        d: 1.08,
        tx: 14,
        ty: -9,
      },
      bounds: {
        min: { x: 100, y: 80 },
        max: { x: 220, y: 170 },
      },
      layer_id: 'layer-1',
      z_index: 0,
      data: {
        type: 'polygon',
        sides: 6,
        radius: 40,
      },
    });

    drawPolygon(ctx, obj, '#000000', vp, false);

    // After canvas-transform migration, transform is applied via ctx.transform
    expect(ctx.transform).toHaveBeenCalledTimes(1);
    expect(ctx.moveTo).toHaveBeenCalledTimes(1);
    expect(ctx.lineTo).toHaveBeenCalledTimes(5);
    expect(ctx.stroke).toHaveBeenCalledTimes(1);

    // Rendered points are in untransformed bounds space (canvas transform handles the visual rotation)
    const points = extractPathPoints(ctx);
    expect(points.length).toBe(6);

    const xs = points.map((p) => p.x);
    const ys = points.map((p) => p.y);
    const rendered = {
      minX: Math.min(...xs),
      minY: Math.min(...ys),
      maxX: Math.max(...xs),
      maxY: Math.max(...ys),
    };

    const expectedMin = worldToScreen(obj.bounds.min, vp);
    const expectedMax = worldToScreen(obj.bounds.max, vp);

    expect(rendered.minX).toBeCloseTo(expectedMin.x, 4);
    expect(rendered.minY).toBeCloseTo(expectedMin.y, 4);
    expect(rendered.maxX).toBeCloseTo(expectedMax.x, 4);
    expect(rendered.maxY).toBeCloseTo(expectedMax.y, 4);
  });
});

describe('drawShape', () => {
  it('draws zero-radius rectangles via a path instead of fillRect/strokeRect', () => {
    const ctx = {
      ...createMockCtx(),
      fillRect: vi.fn(),
      strokeRect: vi.fn(),
    } as unknown as CanvasRenderingContext2D;
    const vp: ViewportParams = {
      offset: { x: 0, y: 0 },
      zoom: 10,
      canvasWidth: 800,
      canvasHeight: 600,
    };
    const obj: ProjectObject = makeProjectObject({
      id: 'rect-1',
      name: 'Rectangle',
      bounds: { min: { x: 10, y: 20 }, max: { x: 30, y: 50 } },
      layer_id: 'layer-1',
      data: {
        type: 'shape',
        kind: 'rectangle',
        width: 20,
        height: 30,
        corner_radius: 0,
      },
    });

    drawShape(ctx, obj, '#000000', vp, true);

    expect(ctx.beginPath).toHaveBeenCalledTimes(1);
    expect(ctx.rect).toHaveBeenCalledTimes(1);
    expect(ctx.fill).toHaveBeenCalledTimes(1);
    expect(ctx.stroke).toHaveBeenCalledTimes(1);
    expect(ctx.fillRect).not.toHaveBeenCalled();
    expect(ctx.strokeRect).not.toHaveBeenCalled();
  });
});

describe('drawRasterImage image masks', () => {
  const vp: ViewportParams = {
    offset: { x: 0, y: 0 },
    zoom: 10,
    canvasWidth: 800,
    canvasHeight: 600,
  };

  function makeRasterObject(overrides: Partial<ProjectObject> = {}): ProjectObject {
    return makeProjectObject({
      id: 'raster-1',
      name: 'Raster',
      bounds: { min: { x: 10, y: 20 }, max: { x: 30, y: 40 } },
      data: {
        type: 'raster_image',
        asset_key: 'asset-1',
        original_width_px: 20,
        original_height_px: 20,
        masks: [],
      },
      ...overrides,
    });
  }

  function maskContext(overrides: Partial<RasterMaskRenderContext> = {}): RasterMaskRenderContext {
    return {
      inside: [],
      outside: [],
      ...overrides,
    };
  }

  function makeLoadedImage(): HTMLImageElement {
    const img = document.createElement('img');
    Object.defineProperty(img, 'complete', { value: true, configurable: true });
    Object.defineProperty(img, 'naturalWidth', { value: 20, configurable: true });
    Object.defineProperty(img, 'naturalHeight', { value: 20, configurable: true });
    return img;
  }

  it('uses the direct path for unmasked and all-invalid-mask rasters', () => {
    const ctx = createRasterMockCtx();
    const createElementSpy = vi.spyOn(document, 'createElement');

    drawRasterImage(ctx, makeRasterObject(), '#111111', vp, undefined, undefined, null);
    drawRasterImage(ctx, makeRasterObject(), '#111111', vp, undefined, undefined, maskContext());

    expect(createElementSpy).not.toHaveBeenCalled();
    expect(ctx.strokeRect).toHaveBeenCalledTimes(2);
    expect(ctx.fillText).toHaveBeenCalledWith('Loading...', expect.any(Number), expect.any(Number));
    createElementSpy.mockRestore();
  });

  it('draws a loaded image fallback instead of leaving refreshed rasters stuck loading', () => {
    const ctx = createRasterMockCtx();
    const imageCache = new Map<string, HTMLImageElement | HTMLCanvasElement>([['asset-1', makeLoadedImage()]]);

    drawRasterImage(ctx, makeRasterObject(), '#111111', vp, imageCache, undefined, null);

    expect(ctx.drawImage).toHaveBeenCalledTimes(1);
    expect(ctx.fillText).not.toHaveBeenCalledWith('Loading...', expect.any(Number), expect.any(Number));
  });

  it('uses offscreen compositing for masked loading placeholders', () => {
    const ctx = createRasterMockCtx();
    const inside = new Path2D();

    withMockedCanvases((contexts) => {
      drawRasterImage(ctx, makeRasterObject(), '#111111', vp, undefined, undefined, maskContext({ inside: [inside] }));

      expect(contexts.length).toBe(2);
      expect(contexts[0].strokeRect).toHaveBeenCalledTimes(1);
      expect(contexts[1].fill).toHaveBeenCalledWith(inside);
      expect(contexts[0].drawImage).toHaveBeenCalledTimes(1);
      expect(ctx.drawImage).toHaveBeenCalledTimes(1);
    });
  });

  it('unions multiple keep-inside masks on a separate alpha buffer', () => {
    const ctx = createRasterMockCtx();
    const insideA = new Path2D();
    const insideB = new Path2D();

    withMockedCanvases((contexts) => {
      drawRasterImage(ctx, makeRasterObject(), '#111111', vp, undefined, undefined, maskContext({ inside: [insideA, insideB] }));

      expect(contexts.length).toBe(2);
      expect(contexts[1].fill).toHaveBeenNthCalledWith(1, insideA);
      expect(contexts[1].fill).toHaveBeenNthCalledWith(2, insideB);
      expect(contexts[0].operations).toContain('composite:destination-in');
      expect(contexts[0].drawImage).toHaveBeenCalledWith(expect.anything(), 0, 0);
    });
  });

  it('subtracts overlapping keep-outside masks with destination-out', () => {
    const ctx = createRasterMockCtx();
    const outsideA = new Path2D();
    const outsideB = new Path2D();

    withMockedCanvases((contexts) => {
      drawRasterImage(ctx, makeRasterObject(), '#111111', vp, undefined, undefined, maskContext({ outside: [outsideA, outsideB] }));

      expect(contexts.length).toBe(1);
      expect(contexts[0].operations).toContain('composite:destination-out');
      expect(contexts[0].fill).toHaveBeenNthCalledWith(1, outsideA);
      expect(contexts[0].fill).toHaveBeenNthCalledWith(2, outsideB);
    });
  });

  it('applies inside masks before outside masks when both are present', () => {
    const ctx = createRasterMockCtx();
    const insideA = new Path2D();
    const insideB = new Path2D();
    const outside = new Path2D();

    withMockedCanvases((contexts) => {
      drawRasterImage(
        ctx,
        makeRasterObject(),
        '#111111',
        vp,
        undefined,
        undefined,
        maskContext({ inside: [insideA, insideB], outside: [outside] }),
      );

      expect(contexts.length).toBe(2);
      expect(contexts[0].operations).toEqual(
        expect.arrayContaining(['composite:destination-in', 'composite:destination-out']),
      );
      expect(contexts[0].operations.indexOf('composite:destination-in')).toBeLessThan(
        contexts[0].operations.indexOf('composite:destination-out'),
      );
      expect(contexts[1].fill).toHaveBeenCalledTimes(2);
      expect(contexts[0].fill).toHaveBeenCalledWith(outside);
    });
  });

  it('builds mask paths with mask transforms independent from image transforms', () => {
    const mask = makeProjectObject({
      id: 'mask-1',
      name: 'Mask',
      bounds: { min: { x: 10, y: 10 }, max: { x: 20, y: 20 } },
      transform: { a: 1, b: 0, c: 0, d: 1, tx: 5, ty: -3 },
      data: {
        type: 'vector_path',
        path_data: 'M 0 0 L 10 0 L 10 10 L 0 10 Z',
        closed: true,
      },
    });

    const path = buildObjectScreenMaskPath(mask, vp) as unknown as MockPath2D;
    const expectedFirst = worldToScreen({ x: 15, y: 7 }, vp);
    const expectedSecond = worldToScreen({ x: 25, y: 7 }, vp);

    expect(path).not.toBeNull();
    expect(path.ops[0]).toEqual(['moveTo', expectedFirst.x, expectedFirst.y]);
    expect(path.ops).toContainEqual(['lineTo', expectedSecond.x, expectedSecond.y]);
  });

  it('builds closed vector_path mask geometry for clipping', () => {
    const mask = makeProjectObject({
      id: 'vector-mask',
      name: 'Vector Mask',
      bounds: { min: { x: 5, y: 6 }, max: { x: 15, y: 16 } },
      data: {
        type: 'vector_path',
        path_data: 'M 0 0 L 20 0 L 20 20 L 0 20 Z',
        closed: true,
      },
    });

    const path = buildObjectScreenMaskPath(mask, vp) as unknown as MockPath2D;
    const expectedFirst = worldToScreen({ x: 5, y: 6 }, vp);
    const expectedSecond = worldToScreen({ x: 15, y: 6 }, vp);

    expect(path).not.toBeNull();
    expect(path.ops[0]).toEqual(['moveTo', expectedFirst.x, expectedFirst.y]);
    expect(path.ops).toContainEqual(['lineTo', expectedSecond.x, expectedSecond.y]);
    expect(path.ops).toContainEqual(['closePath']);
  });

  it('uses rounded rectangle geometry for rounded shape masks', () => {
    const mask = makeProjectObject({
      id: 'rounded-mask',
      name: 'Rounded Mask',
      bounds: { min: { x: 10, y: 10 }, max: { x: 30, y: 30 } },
      data: {
        type: 'shape',
        kind: 'rectangle',
        width: 20,
        height: 20,
        corner_radius: 5,
      },
    });

    const path = buildObjectScreenMaskPath(mask, vp) as unknown as MockPath2D;

    expect(path).not.toBeNull();
    expect(path.ops.filter((op) => op[0] === 'lineTo').length).toBeGreaterThan(20);
    expect(path.ops).toContainEqual(['closePath']);
  });

  it('renders with independent non-identity image and mask transforms', () => {
    const ctx = createRasterMockCtx();
    const image = makeRasterObject({
      transform: { a: 1.2, b: 0, c: 0, d: 0.8, tx: 3, ty: -2 },
    });
    const mask = makeProjectObject({
      id: 'mask-1',
      name: 'Mask',
      bounds: { min: { x: 12, y: 14 }, max: { x: 22, y: 24 } },
      transform: { a: 1, b: 0, c: 0, d: 1, tx: -4, ty: 6 },
      data: {
        type: 'vector_path',
        path_data: 'M 0 0 L 10 0 L 10 10 L 0 10 Z',
        closed: true,
      },
    });
    const maskPath = buildObjectScreenMaskPath(mask, vp) as unknown as MockPath2D;
    const expectedMaskFirst = worldToScreen({ x: 8, y: 20 }, vp);

    withMockedCanvases((contexts) => {
      drawRasterImage(ctx, image, '#111111', vp, undefined, undefined, maskContext({ inside: [maskPath as unknown as Path2D] }));

      expect(maskPath.ops[0]).toEqual(['moveTo', expectedMaskFirst.x, expectedMaskFirst.y]);
      expect(contexts[0].transform).toHaveBeenCalledWith(1.2, 0, 0, 0.8, 0.6000000000000001, -0.4);
      expect(contexts[0].translate).toHaveBeenCalledWith(-402, -304);
      expect(contexts[1].translate).toHaveBeenCalledWith(-402, -304);
    });
  });

  it('does not build paths for open or unsupported mask objects', () => {
    const openPath = makeProjectObject({
      data: { type: 'vector_path', path_data: 'M 0 0 L 10 10', closed: false },
    });
    const rasterMask = makeRasterObject();

    expect(buildObjectScreenMaskPath(openPath, vp)).toBeNull();
    expect(buildObjectScreenMaskPath(rasterMask, vp)).toBeNull();
  });
});

describe('drawText', () => {
  it('draws cached outline geometry in object-local coordinates', () => {
    const ctx = createMockCtx();
    const vp: ViewportParams = {
      offset: { x: 0, y: 0 },
      zoom: 10,
      canvasWidth: 800,
      canvasHeight: 600,
    };
    const obj: ProjectObject = makeProjectObject({
      id: 'text-1',
      name: 'Text',
      bounds: { min: { x: 100, y: 200 }, max: { x: 140, y: 220 } },
      layer_id: 'layer-1',
      data: makeTextObjectData({
        content: 'Hello',
        font_family: 'Arial',
        font_size_mm: 6,
        resolved_path_data: 'M 0 0 L 10 0 L 10 5 L 0 5 Z',
      }),
    });

    drawText(ctx, obj, '#000000', vp, true);

    const expectedOrigin = worldToScreen({ x: 100, y: 200 }, vp);
    expect(ctx.moveTo).toHaveBeenCalledWith(expectedOrigin.x, expectedOrigin.y);
    expect(ctx.fill).toHaveBeenCalledTimes(1);
    expect(ctx.stroke).toHaveBeenCalledTimes(1);
  });
});

describe('drawVectorPath', () => {
  const vp: ViewportParams = {
    offset: { x: 0, y: 0 },
    zoom: 100,
    canvasWidth: 800,
    canvasHeight: 600,
  };

  it('fills only closed subpaths when an object mixes open and closed contours', () => {
    const originalDOMMatrix = globalThis.DOMMatrix;
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    globalThis.DOMMatrix = undefined as unknown as typeof DOMMatrix;
    resetTransformedPath2DProbeForTests();

    const ctx = createMockCtx();
    const pathData = 'M 0 0 L 50 0 L 50 8 L 0 8 M 15 12 L 35 12 L 35 28 L 15 28 Z';
    const obj: ProjectObject = makeProjectObject({
      id: 'mixed-vector',
      name: 'Mixed Vector',
      bounds: { min: { x: 0, y: 0 }, max: { x: 50, y: 30 } },
      layer_id: 'layer-1',
      data: {
        type: 'vector_path',
        closed: true,
        path_data: pathData,
      },
    });

    try {
      drawVectorPath(ctx, obj, '#000000', vp, true);
    } finally {
      globalThis.DOMMatrix = originalDOMMatrix;
      resetTransformedPath2DProbeForTests();
      warn.mockRestore();
    }

    expect(ctx.fill).toHaveBeenCalledTimes(1);
    expect(ctx.fill).toHaveBeenCalledWith('evenodd');
    expect(ctx.stroke).toHaveBeenCalledTimes(1);

    const fillOrder = (ctx.fill as ReturnType<typeof vi.fn>).mock.invocationCallOrder[0];
    const moveToMock = ctx.moveTo as ReturnType<typeof vi.fn>;
    const movesBeforeFill = moveToMock.mock.calls
      .map((call, index) => ({ call, order: moveToMock.mock.invocationCallOrder[index] }))
      .filter(({ order }) => order < fillOrder);
    const bbox = computePathBBox(parsePathData(pathData));
    const closedStart = worldToScreen({
      x: obj.bounds.min.x + ((15 - bbox.minX) / bbox.width) * (obj.bounds.max.x - obj.bounds.min.x),
      y: obj.bounds.min.y + ((12 - bbox.minY) / bbox.height) * (obj.bounds.max.y - obj.bounds.min.y),
    }, vp);

    expect(movesBeforeFill).toHaveLength(1);
    expect(movesBeforeFill[0].call[0]).toBeCloseTo(closedStart.x, 4);
    expect(movesBeforeFill[0].call[1]).toBeCloseTo(closedStart.y, 4);
  });

  it('does not fill a single open contour just because vector metadata is closed', () => {
    const originalDOMMatrix = globalThis.DOMMatrix;
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    globalThis.DOMMatrix = undefined as unknown as typeof DOMMatrix;
    resetTransformedPath2DProbeForTests();

    const ctx = createMockCtx();
    const obj: ProjectObject = makeProjectObject({
      id: 'stale-closed-vector',
      name: 'Stale Closed Vector',
      bounds: { min: { x: 0, y: 0 }, max: { x: 50, y: 8 } },
      layer_id: 'layer-1',
      data: {
        type: 'vector_path',
        closed: true,
        path_data: 'M 0 0 L 50 0 L 50 8 L 0 8',
      },
    });

    try {
      drawVectorPath(ctx, obj, '#000000', vp, true);
    } finally {
      globalThis.DOMMatrix = originalDOMMatrix;
      resetTransformedPath2DProbeForTests();
      warn.mockRestore();
    }

    expect(ctx.fill).not.toHaveBeenCalled();
    expect(ctx.closePath).not.toHaveBeenCalled();
    expect(ctx.stroke).toHaveBeenCalledTimes(1);
  });
});

describe('drawEditableVectorPath', () => {
  const vp: ViewportParams = {
    offset: { x: 0, y: 0 },
    zoom: 100,
    canvasWidth: 800,
    canvasHeight: 600,
  };

  it('fills closed editable subpaths as one evenodd compound path', () => {
    const ctx = createMockCtx();
    const paths: EditablePath[] = [
      {
        closed: true,
        nodes: [
          { id: { subpath_idx: 0, command_idx: 0 }, position: { x: 0, y: 0 }, handle_in: null, handle_out: null, node_type: 'corner' },
          { id: { subpath_idx: 0, command_idx: 1 }, position: { x: 20, y: 0 }, handle_in: null, handle_out: null, node_type: 'corner' },
          { id: { subpath_idx: 0, command_idx: 2 }, position: { x: 20, y: 20 }, handle_in: null, handle_out: null, node_type: 'corner' },
          { id: { subpath_idx: 0, command_idx: 3 }, position: { x: 0, y: 20 }, handle_in: null, handle_out: null, node_type: 'corner' },
        ],
      },
      {
        closed: true,
        nodes: [
          { id: { subpath_idx: 1, command_idx: 0 }, position: { x: 5, y: 5 }, handle_in: null, handle_out: null, node_type: 'corner' },
          { id: { subpath_idx: 1, command_idx: 1 }, position: { x: 15, y: 5 }, handle_in: null, handle_out: null, node_type: 'corner' },
          { id: { subpath_idx: 1, command_idx: 2 }, position: { x: 15, y: 15 }, handle_in: null, handle_out: null, node_type: 'corner' },
          { id: { subpath_idx: 1, command_idx: 3 }, position: { x: 5, y: 15 }, handle_in: null, handle_out: null, node_type: 'corner' },
        ],
      },
    ];

    drawEditableVectorPath(ctx, paths, '#000000', vp, undefined, true);

    expect(ctx.fill).toHaveBeenCalledTimes(1);
    expect(ctx.fill).toHaveBeenCalledWith('evenodd');
    expect(ctx.stroke).toHaveBeenCalledTimes(1);
  });
});

describe('editablePathsToSvgD', () => {
  function makeNode(
    spIdx: number,
    cmdIdx: number,
    x: number,
    y: number,
    handleIn: { x: number; y: number } | null = null,
    handleOut: { x: number; y: number } | null = null,
  ) {
    return {
      id: { subpath_idx: spIdx, command_idx: cmdIdx },
      position: { x, y },
      handle_in: handleIn,
      handle_out: handleOut,
      node_type: 'corner' as const,
    };
  }

  it('closed path with cubic seam emits C before Z', () => {
    const path: EditablePath = {
      closed: true,
      nodes: [
        makeNode(0, 0, 0, 0, { x: 5, y: -5 }, null),        // MoveTo with handle_in from seam
        makeNode(0, 1, 10, 0, null, null),                     // LineTo
        makeNode(0, 2, 0, 0, { x: 15, y: 5 }, { x: 5, y: 10 }), // CubicTo endpoint
      ],
    };
    const d = editablePathsToSvgD([path]);
    // Closing segment: last.handle_out (5,10) + first.handle_in (5,-5) → C before Z
    expect(d).toContain('C 5 10 5 -5 0 0');
    expect(d).toContain('Z');
  });

  it('closed path without seam handles emits bare Z', () => {
    const path: EditablePath = {
      closed: true,
      nodes: [
        makeNode(0, 0, 0, 0),
        makeNode(0, 1, 10, 0),
        makeNode(0, 2, 20, 0),
      ],
    };
    const d = editablePathsToSvgD([path]);
    // Should end with the last LineTo then Z, no curve before Z
    expect(d).not.toMatch(/[CQ] .* Z/);
    expect(d).toMatch(/L 20 0 Z$/);
  });

  it('closed path with one-sided seam handle emits Q before Z', () => {
    const path: EditablePath = {
      closed: true,
      nodes: [
        makeNode(0, 0, 0, 0, null, null),                     // No handle_in
        makeNode(0, 1, 10, 0, null, null),
        makeNode(0, 2, 5, 5, null, { x: 3, y: 8 }),           // handle_out only
      ],
    };
    const d = editablePathsToSvgD([path]);
    // Only last.handle_out exists → Q before Z
    expect(d).toContain('Q 3 8 0 0');
    expect(d).toContain('Z');
  });

  it('open path unchanged — no Z, no closing segment', () => {
    const path: EditablePath = {
      closed: false,
      nodes: [
        makeNode(0, 0, 0, 0, null, { x: 5, y: 10 }),
        makeNode(0, 1, 20, 0, { x: 15, y: 10 }, null),
      ],
    };
    const d = editablePathsToSvgD([path]);
    expect(d).not.toContain('Z');
    expect(d).toContain('C 5 10 15 10 20 0');
  });
});

describe('computePathBBox', () => {
  it('uses curve geometry bounds instead of control-point bounds for cubics', () => {
    const commands = parsePathData('M 0 0 C 50 100 100 100 100 0');
    const bbox = computePathBBox(commands);

    expect(bbox.minX).toBeCloseTo(0, 4);
    expect(bbox.maxX).toBeCloseTo(100, 4);
    expect(bbox.minY).toBeCloseTo(0, 4);
    // Actual cubic peak is about 75, not 100 from the control points.
    expect(bbox.maxY).toBeGreaterThan(70);
    expect(bbox.maxY).toBeLessThan(80);
  });

  it('reuses parsed commands and sampled bounds for identical path data', () => {
    const d = 'M 0 0 C 50 100 100 100 100 0';
    const commandsA = parsePathData(d);
    const commandsB = parsePathData(d);
    const bboxA = computePathBBox(commandsA);
    const bboxB = computePathBBox(commandsB);

    expect(commandsB).toBe(commandsA);
    expect(bboxB).toBe(bboxA);
  });

  it('reuses cached vector geometry across refetch-style data identity changes', () => {
    const pathData = 'M 0 0 C 50 100 100 100 100 0';
    const objA = makeProjectObject({
      id: 'vector-a',
      name: 'Vector A',
      bounds: { min: { x: 0, y: 0 }, max: { x: 100, y: 100 } },
      layer_id: 'layer-1',
      data: { type: 'vector_path', path_data: pathData, closed: false },
    });
    const objB = makeProjectObject({
      id: 'vector-b',
      name: 'Vector B',
      bounds: { min: { x: 10, y: 10 }, max: { x: 110, y: 110 } },
      layer_id: 'layer-1',
      data: { type: 'vector_path', path_data: pathData, closed: false },
    });

    const infoA = getVectorPathRenderInfoForObject(objA);
    const infoARepeat = getVectorPathRenderInfoForObject(objA);
    const infoB = getVectorPathRenderInfoForObject(objB);
    const infoBRepeat = getVectorPathRenderInfoForObject(objB);

    expect(infoA).not.toBeNull();
    expect(infoB).not.toBeNull();
    expect(infoARepeat).toBe(infoA);
    expect(infoBRepeat).toBe(infoB);
    expect(infoB!.commands).toBe(infoA!.commands);
    expect(infoB!.bbox).toBe(infoA!.bbox);
    expect(infoB!.path2d).toBe(infoA!.path2d);
  });
});

describe('supportsTransformedPath2DFastPath', () => {
  it('enables the fast path when transformed addPath renders at the expected location', () => {
    const originalPath2D = globalThis.Path2D;
    const originalDOMMatrix = globalThis.DOMMatrix;
    const originalCreateElement = document.createElement.bind(document);
    const getImageData = vi.fn((x: number, y: number) => ({
      data: new Uint8ClampedArray([
        0,
        0,
        0,
        x === 18 && y === 12 ? 255 : 0,
      ]),
    }));
    const offscreenCtx = {
      clearRect: vi.fn(),
      strokeStyle: '',
      lineWidth: 0,
      stroke: vi.fn(),
      getImageData,
    };
    class MockPath2D {
      constructor(_d?: string) {}
      addPath = vi.fn();
    }
    class MockDOMMatrix {
      constructor(_values?: number[]) {}
      translateSelf() {
        return this;
      }
      rotateSelf() {
        return this;
      }
      scaleSelf() {
        return this;
      }
    }
    globalThis.Path2D = MockPath2D as unknown as typeof Path2D;
    globalThis.DOMMatrix = MockDOMMatrix as unknown as typeof DOMMatrix;
    document.createElement = vi.fn((tagName: string) => {
      if (tagName === 'canvas') {
        return {
          width: 0,
          height: 0,
          getContext: vi.fn(() => offscreenCtx),
        } as unknown as HTMLCanvasElement;
      }
      return originalCreateElement(tagName);
    }) as typeof document.createElement;

    resetTransformedPath2DProbeForTests();
    expect(supportsTransformedPath2DFastPath()).toBe(true);

    document.createElement = originalCreateElement;
    globalThis.Path2D = originalPath2D;
    globalThis.DOMMatrix = originalDOMMatrix;
  });

  it('falls back when transformed addPath does not render the transformed stroke', () => {
    const originalPath2D = globalThis.Path2D;
    const originalDOMMatrix = globalThis.DOMMatrix;
    const originalCreateElement = document.createElement.bind(document);
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const offscreenCtx = {
      clearRect: vi.fn(),
      strokeStyle: '',
      lineWidth: 0,
      stroke: vi.fn(),
      getImageData: vi.fn(() => ({ data: new Uint8ClampedArray([0, 0, 0, 255]) })),
    };
    class MockPath2D {
      constructor(_d?: string) {}
      addPath = vi.fn();
    }
    class MockDOMMatrix {
      constructor(_values?: number[]) {}
      translateSelf() {
        return this;
      }
      rotateSelf() {
        return this;
      }
      scaleSelf() {
        return this;
      }
    }
    globalThis.Path2D = MockPath2D as unknown as typeof Path2D;
    globalThis.DOMMatrix = MockDOMMatrix as unknown as typeof DOMMatrix;
    document.createElement = vi.fn((tagName: string) => {
      if (tagName === 'canvas') {
        return {
          width: 0,
          height: 0,
          getContext: vi.fn(() => offscreenCtx),
        } as unknown as HTMLCanvasElement;
      }
      return originalCreateElement(tagName);
    }) as typeof document.createElement;

    resetTransformedPath2DProbeForTests();
    expect(supportsTransformedPath2DFastPath()).toBe(false);
    expect(warnSpy).toHaveBeenCalled();

    warnSpy.mockRestore();
    document.createElement = originalCreateElement;
    globalThis.Path2D = originalPath2D;
    globalThis.DOMMatrix = originalDOMMatrix;
  });
});
