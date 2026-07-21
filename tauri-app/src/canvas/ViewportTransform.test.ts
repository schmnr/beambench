import { describe, it, expect } from 'vitest';
import {
  pxPerMm,
  worldToScreen,
  screenToWorld,
  worldToScreenDist,
  screenToWorldDist,
  visibleBounds,
  worldBoundsToScreenRect,
  zoomToFitBounds,
  snapToGrid,
  snapPointToGrid,
  type ViewportParams,
} from './ViewportTransform';

const defaultVp: ViewportParams = {
  offset: { x: 200, y: 200 },
  zoom: 100,
  canvasWidth: 800,
  canvasHeight: 600,
};

describe('pxPerMm', () => {
  it('returns BASE_PX_PER_MM at 100% zoom', () => {
    expect(pxPerMm(100)).toBe(2.0);
  });

  it('scales linearly with zoom', () => {
    expect(pxPerMm(200)).toBe(4.0);
    expect(pxPerMm(50)).toBe(1.0);
  });
});

describe('worldToScreen', () => {
  it('maps viewport center to canvas center', () => {
    const result = worldToScreen({ x: 200, y: 200 }, defaultVp);
    expect(result.x).toBe(400);
    expect(result.y).toBe(300);
  });

  it('maps origin (0,0) correctly', () => {
    const result = worldToScreen({ x: 0, y: 0 }, defaultVp);
    // (0 - 200) * 2 + 400 = -400 + 400 = 0
    expect(result.x).toBe(0);
    // (0 - 200) * 2 + 300 = -400 + 300 = -100
    expect(result.y).toBe(-100);
  });

  it('accounts for zoom', () => {
    const vp = { ...defaultVp, zoom: 200 };
    const result = worldToScreen({ x: 200, y: 200 }, vp);
    expect(result.x).toBe(400);
    expect(result.y).toBe(300);
  });

  it('can render world Y upward for machine-space previews', () => {
    const vp: ViewportParams = { ...defaultVp, offset: { x: 200, y: 150 }, yAxis: 'up' };
    expect(worldToScreen({ x: 200, y: 150 }, vp)).toEqual({ x: 400, y: 300 });
    expect(worldToScreen({ x: 200, y: 0 }, vp).y).toBe(600);
    expect(worldToScreen({ x: 200, y: 300 }, vp).y).toBe(0);
  });
});

describe('screenToWorld', () => {
  it('maps canvas center to viewport offset', () => {
    const result = screenToWorld({ x: 400, y: 300 }, defaultVp);
    expect(result.x).toBe(200);
    expect(result.y).toBe(200);
  });

  it('is the inverse of worldToScreen', () => {
    const worldPt = { x: 123.4, y: 56.7 };
    const screen = worldToScreen(worldPt, defaultVp);
    const back = screenToWorld(screen, defaultVp);
    expect(back.x).toBeCloseTo(worldPt.x);
    expect(back.y).toBeCloseTo(worldPt.y);
  });

  it('inverse relationship holds at different zoom levels', () => {
    const vp = { ...defaultVp, zoom: 250 };
    const worldPt = { x: 50, y: 300 };
    const screen = worldToScreen(worldPt, vp);
    const back = screenToWorld(screen, vp);
    expect(back.x).toBeCloseTo(worldPt.x);
    expect(back.y).toBeCloseTo(worldPt.y);
  });

  it('is the inverse of worldToScreen for Y-up viewports', () => {
    const vp: ViewportParams = { ...defaultVp, offset: { x: 200, y: 150 }, yAxis: 'up' };
    const worldPt = { x: 123.4, y: 56.7 };
    const screen = worldToScreen(worldPt, vp);
    const back = screenToWorld(screen, vp);
    expect(back.x).toBeCloseTo(worldPt.x);
    expect(back.y).toBeCloseTo(worldPt.y);
  });
});

describe('worldToScreenDist / screenToWorldDist', () => {
  it('converts mm to pixels at 100% zoom', () => {
    expect(worldToScreenDist(10, 100)).toBe(20);
  });

  it('screenToWorldDist is the inverse of worldToScreenDist', () => {
    const mm = 42;
    const px = worldToScreenDist(mm, 150);
    expect(screenToWorldDist(px, 150)).toBeCloseTo(mm);
  });
});

describe('visibleBounds', () => {
  it('returns the world-space bounds visible in the viewport', () => {
    const bounds = visibleBounds(defaultVp);
    // At 100% zoom, pxPerMm = 2
    // left edge: (0 - 400) / 2 + 200 = 0
    // right edge: (800 - 400) / 2 + 200 = 400
    expect(bounds.min.x).toBe(0);
    expect(bounds.max.x).toBe(400);
    // top edge: (0 - 300) / 2 + 200 = 50
    // bottom edge: (600 - 300) / 2 + 200 = 350
    expect(bounds.min.y).toBe(50);
    expect(bounds.max.y).toBe(350);
  });

  it('normalizes visible world-space bounds for Y-up viewports', () => {
    const vp: ViewportParams = { ...defaultVp, offset: { x: 200, y: 150 }, yAxis: 'up' };
    const bounds = visibleBounds(vp);
    expect(bounds.min.x).toBe(0);
    expect(bounds.max.x).toBe(400);
    expect(bounds.min.y).toBe(0);
    expect(bounds.max.y).toBe(300);
  });
});

describe('worldBoundsToScreenRect', () => {
  it('returns a positive rectangle for Y-down viewports', () => {
    expect(worldBoundsToScreenRect(
      { min: { x: 100, y: 100 }, max: { x: 200, y: 150 } },
      defaultVp,
    )).toEqual({ x: 200, y: 100, w: 200, h: 100 });
  });

  it('returns a positive rectangle for Y-up viewports', () => {
    const vp: ViewportParams = { ...defaultVp, offset: { x: 200, y: 150 }, yAxis: 'up' };
    expect(worldBoundsToScreenRect(
      { min: { x: 100, y: 50 }, max: { x: 200, y: 150 } },
      vp,
    )).toEqual({ x: 200, y: 300, w: 200, h: 200 });
  });
});

describe('zoomToFitBounds', () => {
  it('centers on the given bounds', () => {
    const result = zoomToFitBounds(
      { min: { x: 0, y: 0 }, max: { x: 400, y: 400 } },
      800,
      600,
    );
    expect(result.offset.x).toBe(200);
    expect(result.offset.y).toBe(200);
  });

  it('handles zero-size bounds gracefully', () => {
    const result = zoomToFitBounds(
      { min: { x: 100, y: 100 }, max: { x: 100, y: 100 } },
      800,
      600,
    );
    expect(result.offset.x).toBe(100);
    expect(result.zoom).toBe(100);
  });
});

describe('snapToGrid', () => {
  it('snaps to nearest grid line', () => {
    expect(snapToGrid(12, 10)).toBe(10);
    expect(snapToGrid(17, 10)).toBe(20);
    expect(snapToGrid(15, 10)).toBe(20);
  });

  it('snaps exactly on grid line', () => {
    expect(snapToGrid(30, 10)).toBe(30);
  });

  it('snaps negative values', () => {
    expect(snapToGrid(-3, 10)).toBe(0);
    expect(snapToGrid(-8, 10)).toBe(-10);
  });
});

describe('snapPointToGrid', () => {
  it('snaps both coordinates', () => {
    const result = snapPointToGrid({ x: 12, y: 28 }, 10);
    expect(result.x).toBe(10);
    expect(result.y).toBe(30);
  });
});
