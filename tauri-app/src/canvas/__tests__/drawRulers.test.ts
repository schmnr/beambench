import { describe, it, expect, vi } from 'vitest';
import { computeRulerTicks, drawOrigin, drawRulers } from '../drawWorkspace';
import type { ViewportParams } from '../ViewportTransform';
import { DARK_THEME, RULER_SIZE } from '../constants';

const makeVp = (zoom: number): ViewportParams => ({
  zoom,
  offset: { x: 0, y: 0 },
  canvasWidth: 800,
  canvasHeight: 600,
});

describe('computeRulerTicks', () => {
  it('returns ticks at grid intervals for x axis', () => {
    const ticks = computeRulerTicks(10, 400, makeVp(100), 'x');
    expect(ticks.length).toBe(41); // 0 to 400 in steps of 10
    expect(ticks[0].label).toBe('0');
    expect(ticks[0].isMajor).toBe(true);
    expect(ticks[1].isMajor).toBe(false);
    expect(ticks[5].isMajor).toBe(true);
    expect(ticks[5].label).toBe('50');
  });

  it('returns empty array if screen spacing too small', () => {
    // At zoom=1, 10mm spacing = 0.02px — way below MIN_GRID_SCREEN_PX
    const ticks = computeRulerTicks(10, 400, makeVp(1), 'x');
    expect(ticks.length).toBe(0);
  });

  it('computes y axis ticks', () => {
    const ticks = computeRulerTicks(10, 400, makeVp(100), 'y');
    expect(ticks.length).toBe(41);
    expect(ticks[10].isMajor).toBe(true);
    expect(ticks[10].label).toBe('100');
  });

  it('labels bottom-left Y rulers from the lower bed edge upward', () => {
    const ticks = computeRulerTicks(10, 100, makeVp(100), 'y', 'mm', 'bottom_left');
    expect(ticks[0]).toMatchObject({ label: '0', screenPos: 500, isMajor: true });
    expect(ticks[5]).toMatchObject({ label: '50', screenPos: 400, isMajor: true });
    expect(ticks[10]).toMatchObject({ label: '100', screenPos: 300, isMajor: true });
  });

  it('keeps X rulers unchanged for bottom-left workspaces', () => {
    const topLeftTicks = computeRulerTicks(10, 100, makeVp(100), 'x', 'mm', 'top_left');
    const bottomLeftTicks = computeRulerTicks(10, 100, makeVp(100), 'x', 'mm', 'bottom_left');
    expect(bottomLeftTicks).toEqual(topLeftTicks);
  });

  it('major ticks have labels, minor ticks do not', () => {
    const ticks = computeRulerTicks(10, 100, makeVp(100), 'x');
    const minors = ticks.filter((t) => !t.isMajor);
    const majors = ticks.filter((t) => t.isMajor);
    expect(minors.every((t) => t.label === '')).toBe(true);
    expect(majors.every((t) => t.label !== '')).toBe(true);
  });

  it('inch mode produces ticks at clean inch fractions', () => {
    // At zoom 100, 1/2" = 12.7mm → 25.4px screen, above MIN_INCH_GRID_SCREEN_PX
    const ticks = computeRulerTicks(10, 254, makeVp(100), 'x', 'inches');
    expect(ticks.length).toBeGreaterThan(0);
    // Major ticks should have clean inch labels (integer or short decimal)
    const majors = ticks.filter((t) => t.isMajor);
    expect(majors.length).toBeGreaterThan(0);
    for (const m of majors) {
      const inchVal = parseFloat(m.label);
      expect(Number.isFinite(inchVal)).toBe(true);
    }
  });

  it('inch mode ignores gridSpacingMm parameter', () => {
    // Regardless of gridSpacingMm, inch mode uses chooseInchInterval
    const ticksA = computeRulerTicks(5, 254, makeVp(100), 'x', 'inches');
    const ticksB = computeRulerTicks(20, 254, makeVp(100), 'x', 'inches');
    // Same tick count & positions — gridSpacingMm is irrelevant in inch mode
    expect(ticksA.length).toBe(ticksB.length);
    for (let i = 0; i < ticksA.length; i++) {
      expect(ticksA[i].screenPos).toBeCloseTo(ticksB[i].screenPos, 5);
      expect(ticksA[i].isMajor).toBe(ticksB[i].isMajor);
    }
  });
});

describe('drawOrigin', () => {
  function createMockCtx() {
    return {
      beginPath: vi.fn(),
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      stroke: vi.fn(),
      strokeStyle: '',
      lineWidth: 1,
    } as unknown as CanvasRenderingContext2D;
  }

  it('draws the origin marker at the upper-left for top-left origin workspaces', () => {
    const ctx = createMockCtx();
    const vp = makeVp(100);

    drawOrigin(ctx, { bed_width_mm: 100, bed_height_mm: 100, origin: 'top_left' }, vp);

    expect((ctx.moveTo as ReturnType<typeof vi.fn>).mock.calls[0]).toEqual([388, 300]);
    expect((ctx.lineTo as ReturnType<typeof vi.fn>).mock.calls[0]).toEqual([412, 300]);
    expect((ctx.moveTo as ReturnType<typeof vi.fn>).mock.calls[1]).toEqual([400, 288]);
    expect((ctx.lineTo as ReturnType<typeof vi.fn>).mock.calls[1]).toEqual([400, 312]);
  });

  it('draws the origin marker at the lower-left for bottom-left origin workspaces', () => {
    const ctx = createMockCtx();
    const vp = makeVp(100);

    drawOrigin(ctx, { bed_width_mm: 100, bed_height_mm: 100, origin: 'bottom_left' }, vp);

    expect((ctx.moveTo as ReturnType<typeof vi.fn>).mock.calls[0]).toEqual([388, 500]);
    expect((ctx.lineTo as ReturnType<typeof vi.fn>).mock.calls[0]).toEqual([412, 500]);
    expect((ctx.moveTo as ReturnType<typeof vi.fn>).mock.calls[1]).toEqual([400, 488]);
    expect((ctx.lineTo as ReturnType<typeof vi.fn>).mock.calls[1]).toEqual([400, 512]);
  });
});

describe('drawRulers 4-sided', () => {
  function createMockCtx() {
    const fillRectCalls: { x: number; y: number; w: number; h: number }[] = [];
    return {
      ctx: {
        save: vi.fn(),
        restore: vi.fn(),
        fillRect: vi.fn((x: number, y: number, w: number, h: number) => fillRectCalls.push({ x, y, w, h })),
        beginPath: vi.fn(),
        moveTo: vi.fn(),
        lineTo: vi.fn(),
        stroke: vi.fn(),
        fillText: vi.fn(),
        translate: vi.fn(),
        rotate: vi.fn(),
        measureText: vi.fn(() => ({ width: 20 })),
        fillStyle: '',
        strokeStyle: '',
        lineWidth: 1,
        font: '',
        textBaseline: '',
        textAlign: '',
      } as unknown as CanvasRenderingContext2D,
      fillRectCalls,
    };
  }

  it('draws 4 ruler backgrounds and 4 corner squares', () => {
    const { ctx, fillRectCalls } = createMockCtx();
    const vp = makeVp(100);
    const workspace = { bed_width_mm: 400, bed_height_mm: 400, origin: 'top_left' };
    const R = RULER_SIZE;

    drawRulers(ctx, workspace as never, vp, 10, DARK_THEME);

    // Should have:
    // 1 corner square (top-left)
    // 1 top ruler bg
    // 1 left ruler bg
    // 1 bottom ruler bg
    // 1 right ruler bg
    // 3 corner squares (top-right, bottom-left, bottom-right)
    // = 8 total fillRect calls for backgrounds (plus tick label fillText calls go through fillText)

    // Filter for the ruler background rects (large enough to be ruler backgrounds)
    const bgRects = fillRectCalls.filter((r) =>
      (r.w >= R || r.h >= R) // at least one dimension should be >= RULER_SIZE
    );

    // Top-left corner: (0, 0, R, R)
    expect(bgRects.some((r) => r.x === 0 && r.y === 0 && r.w === R && r.h === R)).toBe(true);

    // Top ruler: (R, 0, canvasWidth-R, R)
    expect(bgRects.some((r) => r.x === R && r.y === 0 && r.w === vp.canvasWidth - R && r.h === R)).toBe(true);

    // Left ruler: (0, R, R, canvasHeight-R)
    expect(bgRects.some((r) => r.x === 0 && r.y === R && r.w === R && r.h === vp.canvasHeight - R)).toBe(true);

    // Bottom ruler: (R, canvasHeight-R, canvasWidth-R, R)
    expect(bgRects.some((r) => r.x === R && r.y === vp.canvasHeight - R && r.w === vp.canvasWidth - R && r.h === R)).toBe(true);

    // Right ruler: (canvasWidth-R, R, R, canvasHeight-R)
    expect(bgRects.some((r) => r.x === vp.canvasWidth - R && r.y === R && r.w === R && r.h === vp.canvasHeight - R)).toBe(true);

    // Top-right corner: (canvasWidth-R, 0, R, R)
    expect(bgRects.some((r) => r.x === vp.canvasWidth - R && r.y === 0 && r.w === R && r.h === R)).toBe(true);

    // Bottom-left corner: (0, canvasHeight-R, R, R)
    expect(bgRects.some((r) => r.x === 0 && r.y === vp.canvasHeight - R && r.w === R && r.h === R)).toBe(true);

    // Bottom-right corner: (canvasWidth-R, canvasHeight-R, R, R)
    expect(bgRects.some((r) => r.x === vp.canvasWidth - R && r.y === vp.canvasHeight - R && r.w === R && r.h === R)).toBe(true);
  });

  it('draws ticks on all 4 edges', () => {
    const { ctx } = createMockCtx();
    const vp = makeVp(100);
    const workspace = { bed_width_mm: 100, bed_height_mm: 100, origin: 'top_left' };

    drawRulers(ctx, workspace as never, vp, 10, DARK_THEME);

    // moveTo/lineTo should be called many times (4 edges * ticks per edge)
    const moveToCount = (ctx.moveTo as ReturnType<typeof vi.fn>).mock.calls.length;
    const lineToCount = (ctx.lineTo as ReturnType<typeof vi.fn>).mock.calls.length;

    // At zoom 100, 10mm spacing = 2px screen spacing, each edge ~11 ticks (0..100 step 10)
    // 4 edges * 11 ticks = 44 tick lines minimum
    expect(moveToCount).toBeGreaterThanOrEqual(40);
    expect(lineToCount).toBeGreaterThanOrEqual(40);
  });
});
