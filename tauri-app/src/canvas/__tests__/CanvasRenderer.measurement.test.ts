import { describe, expect, it, vi } from 'vitest';
import { CanvasRenderer } from '../CanvasRenderer';

vi.mock('../drawWorkspace', () => ({
  drawBed: vi.fn(),
  drawGrid: vi.fn(),
  drawOrigin: vi.fn(),
  drawRulers: vi.fn(),
}));

function mockContext(labels: string[]): CanvasRenderingContext2D {
  const target: Record<string, unknown> = { canvas: {}, globalAlpha: 1 };
  return new Proxy(target, {
    get(obj, prop) {
      if (prop in obj) return obj[prop as string];
      if (prop === 'measureText') return vi.fn((label: string) => ({ width: label.length * 6 }));
      if (prop === 'fillText') return vi.fn((label: string) => labels.push(label));
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

describe('CanvasRenderer measurement overlays', () => {
  it('keeps a completed linear measurement visible', () => {
    const labels: string[] = [];
    const renderer = new CanvasRenderer(mockContext(labels));

    expect(() => renderer.renderToolOverlay(baseParams({
      type: 'measure-inspection',
      result: {
        kind: 'linear',
        start: { x: 0, y: 0 },
        end: { x: 30, y: 40 },
        dxMm: 30,
        dyMm: 40,
        lengthMm: 50,
        angleDeg: 53.13,
      },
    }) as never)).not.toThrow();
  });

  it('draws the completed angle value', () => {
    const labels: string[] = [];
    const renderer = new CanvasRenderer(mockContext(labels));
    const first = { start: { x: 0, y: 0 }, end: { x: 10, y: 0 }, dxMm: 10, dyMm: 0, lengthMm: 10, angleDeg: 0, segmentIndex: 0, t: 0.5 };
    const second = { start: { x: 0, y: 0 }, end: { x: 0, y: 10 }, dxMm: 0, dyMm: 10, lengthMm: 10, angleDeg: 90, segmentIndex: 1, t: 0.5 };

    renderer.renderToolOverlay(baseParams({
      type: 'measure-inspection',
      result: { kind: 'angle', first, second, angleDeg: 90, labelPoint: { x: 2.5, y: 2.5 } },
    }) as never);

    expect(labels).toContain('90.0°');
  });

  it('draws radius and diameter for a circle', () => {
    const labels: string[] = [];
    const renderer = new CanvasRenderer(mockContext(labels));

    renderer.renderToolOverlay(baseParams({
      type: 'measure-inspection',
      result: {
        kind: 'radius',
        objectId: 'circle',
        objectName: 'Circle',
        center: { x: 10, y: 10 },
        edgePoint: { x: 20, y: 10 },
        radiusXmm: 10,
        radiusYmm: 10,
        diameterXmm: 20,
        diameterYmm: 20,
        circular: true,
      },
    }) as never);

    expect(labels.some((label) => label.includes('R 10.00 mm'))).toBe(true);
    expect(labels.some((label) => label.includes('Ø 20.00 mm'))).toBe(true);
  });
});
