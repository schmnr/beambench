import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render, screen, cleanup, fireEvent, waitFor } from '@testing-library/react';
import { TraceImageDialog } from '../TraceImageDialog';
import { importService } from '../../../services/importService';

const mockInvoke = vi.fn().mockResolvedValue(null);
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => mockInvoke(...args) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const translateCalls: Array<[number, number]> = [];
const mockCtx = {
  clearRect: vi.fn(),
  drawImage: vi.fn(),
  fillRect: vi.fn(),
  save: vi.fn(),
  restore: vi.fn(),
  setTransform: vi.fn(),
  translate: vi.fn((x: number, y: number) => { translateCalls.push([x, y]); }),
  scale: vi.fn(),
  stroke: vi.fn(),
  strokeRect: vi.fn(),
  set fillStyle(_v: string) { /* noop */ },
  set strokeStyle(_v: string) { /* noop */ },
  set lineWidth(_v: number) { /* noop */ },
  set globalAlpha(_v: number) { /* noop */ },
};

beforeEach(() => {
  HTMLCanvasElement.prototype.getContext = vi.fn().mockReturnValue(mockCtx);
  globalThis.Path2D = vi.fn().mockImplementation(() => ({
    moveTo: vi.fn(), lineTo: vi.fn(), quadraticCurveTo: vi.fn(), bezierCurveTo: vi.fn(), closePath: vi.fn(),
  }));
  translateCalls.length = 0;
  mockCtx.stroke.mockClear();
});

afterEach(() => {
  cleanup();
  mockInvoke.mockClear();
  vi.restoreAllMocks();
});

describe('TraceImageDialog pan while zoomed', () => {
  it('space+left-drag pans the preview after zooming in', async () => {
    const traceSpy = vi.spyOn(importService, 'traceImagePreview').mockResolvedValue({
      paths: ['M10 10 L200 10 L200 200 L10 200 Z'],
      source_width: 256,
      source_height: 256,
    });

    render(<TraceImageDialog objectId="obj-1" onClose={vi.fn()} />);
    await waitFor(() => expect(traceSpy).toHaveBeenCalled(), { timeout: 3000 });
    await waitFor(() => expect(mockCtx.stroke).toHaveBeenCalled(), { timeout: 3000 });

    // Zoom in twice via the + button
    fireEvent.click(screen.getByTestId('trace-zoom-in'));
    fireEvent.click(screen.getByTestId('trace-zoom-in'));

    const frame = screen.getByTestId('trace-preview-frame');
    translateCalls.length = 0;

    // Hold space, drag on the preview
    fireEvent.keyDown(window, { code: 'Space', key: ' ' });
    fireEvent.mouseDown(frame, { button: 0, clientX: 100, clientY: 100 });
    fireEvent.mouseMove(window, { clientX: 150, clientY: 130 });
    fireEvent.mouseUp(window);
    fireEvent.keyUp(window, { code: 'Space', key: ' ' });

    await waitFor(() => {
      // First translate call per render is translate(centerX + pan.x, centerY + pan.y);
      // pan should now be (50, 30). jsdom viewport measures 1x1 so center is 0.5.
      const found = translateCalls.some(([x, y]) => Math.abs(x - 50.5) < 0.01 && Math.abs(y - 30.5) < 0.01);
      expect(found, `translate calls: ${JSON.stringify(translateCalls)}`).toBe(true);
    });
  });

  it('space+drag still pans when focus is inside a number input', async () => {
    const traceSpy = vi.spyOn(importService, 'traceImagePreview').mockResolvedValue({
      paths: ['M10 10 L200 10 L200 200 L10 200 Z'],
      source_width: 256,
      source_height: 256,
    });

    render(<TraceImageDialog objectId="obj-1" onClose={vi.fn()} />);
    await waitFor(() => expect(traceSpy).toHaveBeenCalled(), { timeout: 3000 });
    await waitFor(() => expect(mockCtx.stroke).toHaveBeenCalled(), { timeout: 3000 });

    const thresholdInput = screen.getByTestId('trace-threshold').querySelector('input')
      ?? screen.getByTestId('trace-threshold');
    (thresholdInput as HTMLElement).focus();

    const frame = screen.getByTestId('trace-preview-frame');
    translateCalls.length = 0;

    // Space keydown lands on the focused number input; number inputs don't take
    // spaces, so panning must still engage (regression: pan used to go dead here)
    fireEvent.keyDown(thresholdInput, { code: 'Space', key: ' ' });
    fireEvent.mouseDown(frame, { button: 0, clientX: 100, clientY: 100 });
    fireEvent.mouseMove(window, { clientX: 150, clientY: 130 });
    fireEvent.mouseUp(window);
    fireEvent.keyUp(window, { code: 'Space', key: ' ' });

    await waitFor(() => {
      const panned = translateCalls.some(([x, y]) => Math.abs(x - 50.5) < 0.01 && Math.abs(y - 30.5) < 0.01);
      expect(panned, `translate calls: ${JSON.stringify(translateCalls)}`).toBe(true);
    });
  });
});
