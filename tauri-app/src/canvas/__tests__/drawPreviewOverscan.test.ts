import { describe, expect, it, vi } from 'vitest';
import { drawPerRunOverscanMarkers } from '../drawPreview';
import { worldToScreen, type ViewportParams } from '../ViewportTransform';

function makeContext() {
  return {
    fillRect: vi.fn(),
    beginPath: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    stroke: vi.fn(),
    fillStyle: '',
    strokeStyle: '',
    globalAlpha: 1,
    lineWidth: 1,
    lineCap: 'butt',
  } as unknown as CanvasRenderingContext2D;
}

describe('drawPerRunOverscanMarkers', () => {
  it('draws the active right-to-left lead-in on the right side of the burn', () => {
    const ctx = makeContext();
    const vp: ViewportParams = {
      offset: { x: 0, y: 0 },
      zoom: 100,
      canvasWidth: 100,
      canvasHeight: 100,
    };

    drawPerRunOverscanMarkers(
      ctx,
      { scan_axis: 'horizontal', line_interval_mm: 1, overscan_mm: 1 },
      [{ y_mm: 0, start_x_mm: 0, end_x_mm: 10, direction: 'right_to_left' }],
      vp,
      false,
    );

    const calls = (ctx.fillRect as ReturnType<typeof vi.fn>).mock.calls;
    expect(calls).toHaveLength(1);
    const burnRight = worldToScreen({ x: 10, y: 0 }, vp).x;
    expect(calls[0][0]).toBeGreaterThanOrEqual(burnRight);
  });

  it('shows both sides after the right-to-left row is complete', () => {
    const ctx = makeContext();
    const vp: ViewportParams = {
      offset: { x: 0, y: 0 },
      zoom: 100,
      canvasWidth: 100,
      canvasHeight: 100,
    };

    drawPerRunOverscanMarkers(
      ctx,
      { scan_axis: 'horizontal', line_interval_mm: 1, overscan_mm: 1 },
      [{ y_mm: 0, start_x_mm: 0, end_x_mm: 10, direction: 'right_to_left' }],
      vp,
      true,
    );

    expect(ctx.fillRect).toHaveBeenCalledTimes(2);
  });
});
