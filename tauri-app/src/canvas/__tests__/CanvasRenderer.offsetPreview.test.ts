import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CanvasRenderer } from '../CanvasRenderer';
import { worldToScreen } from '../ViewportTransform';

vi.mock('../drawWorkspace', () => ({
  drawBed: vi.fn(),
  drawGrid: vi.fn(),
  drawOrigin: vi.fn(),
  drawRulers: vi.fn(),
}));

/** Minimal 2D context that records stroke calls and no-ops everything else. */
function mockContext(counters: { strokes: number; moves?: Array<[number, number]> }): CanvasRenderingContext2D {
  const target: Record<string, unknown> = { canvas: {}, globalAlpha: 1 };
  return new Proxy(target, {
    get(obj, prop) {
      if (prop in obj) return obj[prop as string];
      if (prop === 'stroke') return vi.fn(() => { counters.strokes += 1; });
      if (prop === 'moveTo') return vi.fn((x: number, y: number) => { counters.moves?.push([x, y]); });
      return vi.fn();
    },
    set(obj, prop, value) {
      obj[prop as string] = value;
      return true;
    },
  }) as unknown as CanvasRenderingContext2D;
}

const vp = { offset: { x: 0, y: 0 }, zoom: 100, canvasWidth: 800, canvasHeight: 600 };

function baseParams(toolOverlay: unknown) {
  return {
    workspace: { bed_width_mm: 300, bed_height_mm: 200, origin: 'top_left' as const },
    objects: [],
    layers: [],
    selectedObjectIds: [],
    vp,
    gridVisible: true,
    gridSpacingMm: 10,
    toolOverlay,
  };
}

describe('CanvasRenderer offset preview overlay', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('strokes one dashed ghost per preview path', () => {
    const counters = { strokes: 0 };
    const ctx = mockContext(counters);
    const renderer = new CanvasRenderer(ctx);

    renderer.renderToolOverlay(baseParams({
      type: 'offset-preview',
      paths: [
        { points: [{ x: 0, y: -2 }, { x: 10, y: -2 }], closed: false },
        { points: [{ x: 0, y: 2 }, { x: 10, y: 2 }], closed: false },
      ],
    }) as never);

    expect(counters.strokes).toBe(2);
  });

  it('skips degenerate single-point paths without throwing', () => {
    const counters = { strokes: 0 };
    const ctx = mockContext(counters);
    const renderer = new CanvasRenderer(ctx);

    expect(() =>
      renderer.renderToolOverlay(baseParams({
        type: 'offset-preview',
        paths: [{ points: [{ x: 0, y: 0 }], closed: false }],
      }) as never),
    ).not.toThrow();
    expect(counters.strokes).toBe(0);
  });

  it('translates the ghost along with a live source-object drag', () => {
    const counters = { strokes: 0, moves: [] as Array<[number, number]> };
    const ctx = mockContext(counters);
    const renderer = new CanvasRenderer(ctx);

    renderer.renderToolOverlay(baseParams({
      type: 'offset-preview',
      paths: [{ points: [{ x: 2, y: 3 }, { x: 4, y: 3 }], closed: false }],
      translation: { x: 5, y: -2 },
    }) as never);

    const translatedStart = worldToScreen({ x: 7, y: 1 }, vp);
    expect(counters.moves).toContainEqual([translatedStart.x, translatedStart.y]);
  });
});
