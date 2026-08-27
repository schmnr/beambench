import { describe, expect, it, vi } from 'vitest';
import { prepareCanvasSurface, presentCanvasBuffer } from './canvasSurface';

function mockContext(): CanvasRenderingContext2D {
  return {
    clearRect: vi.fn(),
    drawImage: vi.fn(),
    globalCompositeOperation: 'source-over',
    restore: vi.fn(),
    save: vi.fn(),
    setTransform: vi.fn(),
  } as unknown as CanvasRenderingContext2D;
}

function mockCanvas(ctx: CanvasRenderingContext2D): HTMLCanvasElement {
  let width = 300;
  let height = 150;
  return {
    get width() {
      return width;
    },
    set width(value: number) {
      width = Math.trunc(value);
    },
    get height() {
      return height;
    },
    set height(value: number) {
      height = Math.trunc(value);
    },
    getContext: vi.fn(() => ctx),
  } as unknown as HTMLCanvasElement;
}

describe('canvas surfaces', () => {
  it.each([1.25, 1.5, 1.75])('keeps a fractional-DPR backing size stable at %s', (dpr) => {
    const ctx = mockContext();
    const canvas = mockCanvas(ctx);

    const first = prepareCanvasSurface(canvas, 1001, 701, dpr);
    const firstWidth = canvas.width;
    const firstHeight = canvas.height;
    const second = prepareCanvasSurface(canvas, 1001, 701, dpr);

    expect(first).toMatchObject({ resized: true });
    expect(second).toMatchObject({ resized: false });
    expect(canvas.width).toBe(firstWidth);
    expect(canvas.height).toBe(firstHeight);
    expect(canvas.width).toBe(Math.round(1001 * dpr));
    expect(canvas.height).toBe(Math.round(701 * dpr));
  });

  it('changes only the backing dimension that actually changed', () => {
    const ctx = mockContext();
    const canvas = mockCanvas(ctx);
    prepareCanvasSurface(canvas, 800, 600, 1);
    const widthSetter = vi.spyOn(canvas, 'width', 'set');
    const heightSetter = vi.spyOn(canvas, 'height', 'set');

    const result = prepareCanvasSurface(canvas, 801, 600, 1);

    expect(result).toMatchObject({ resized: true });
    expect(widthSetter).toHaveBeenCalledOnce();
    expect(heightSetter).not.toHaveBeenCalled();
  });

  it('replaces a visible overlay without clearing it', () => {
    const target = mockContext();
    const buffer = {} as HTMLCanvasElement;

    presentCanvasBuffer(target, buffer);

    expect(target.save).toHaveBeenCalledOnce();
    expect(target.setTransform).toHaveBeenCalledWith(1, 0, 0, 1, 0, 0);
    expect(target.drawImage).toHaveBeenCalledWith(buffer, 0, 0);
    expect(target.globalCompositeOperation).toBe('copy');
    expect(target.clearRect).not.toHaveBeenCalled();
    expect(target.restore).toHaveBeenCalledOnce();
  });
});
