import { act, cleanup, fireEvent, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Canvas } from '../Canvas';
import { useUiStore } from '../../../stores/uiStore';
import { useProjectStore } from '../../../stores/projectStore';
import { NodeTool } from '../../../canvas/tools/NodeTool';

vi.mock('../../../hooks/useCanvasSize', () => ({ useCanvasSize: () => ({ width: 800, height: 600 }) }));
vi.mock('../../../canvas/CanvasRenderer', () => ({
  CanvasRenderer: class {
    invalidateBaseScene() {}
    renderBaseScene() {}
    renderToolOverlay() {}
    setRenderCallback() {}
    ensureCameraOverlayImage() {}
    clearPreviewBitmapCache() {}
    clearImageCache() {}
    dispose() {}
  },
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));

const initialUi = useUiStore.getState();
const initialProject = useProjectStore.getState();
let frames: Map<number, FrameRequestCallback>;
let frameId: number;
function flush() {
  act(() => {
    const queued = [...frames.values()];
    frames.clear();
    queued.forEach((callback) => callback(performance.now()));
  });
}
function pointer(target: Element, type: string, values: Record<string, number>) {
  const event = new MouseEvent(type, { bubbles: true, ...values });
  Object.defineProperty(event, 'pointerId', { value: 1 });
  fireEvent(target, event);
}

beforeEach(() => {
  frames = new Map(); frameId = 0;
  vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => { frames.set(++frameId, callback); return frameId; });
  vi.stubGlobal('cancelAnimationFrame', (id: number) => { frames.delete(id); });
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(new Proxy({}, { get: () => vi.fn() }) as CanvasRenderingContext2D);
  HTMLCanvasElement.prototype.setPointerCapture = vi.fn();
  HTMLCanvasElement.prototype.hasPointerCapture = vi.fn().mockReturnValue(false);
  HTMLCanvasElement.prototype.releasePointerCapture = vi.fn();
  useUiStore.setState({ activeTool: 'node', workspaceMode: 'design', viewportOffset: { x: 0, y: 0 } });
  useProjectStore.setState({ project: null, selectedObjectIds: [] });
});
afterEach(() => {
  cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals();
  useUiStore.setState(initialUi, true); useProjectStore.setState(initialProject, true);
});

describe('Canvas interrupted gestures', () => {
  it.each(['pointercancel', 'lostpointercapture', 'blur'])('ends panning on %s', (end) => {
    const { container } = render(<Canvas />);
    const overlay = container.querySelectorAll('canvas')[1];
    pointer(overlay, 'pointerdown', { button: 1, buttons: 4, clientX: 100, clientY: 100 });
    pointer(overlay, 'pointermove', { buttons: 4, clientX: 140, clientY: 120 });
    flush();
    const moved = useUiStore.getState().viewportOffset;
    expect(moved).not.toEqual({ x: 0, y: 0 });
    if (end === 'blur') fireEvent(window, new Event('blur')); else pointer(overlay, end, {});
    pointer(overlay, 'pointermove', { buttons: 0, clientX: 180, clientY: 140 });
    flush();
    expect(useUiStore.getState().viewportOffset).toEqual(moved);
  });

  it('clears a lost Space key-up on blur and cancels the owning node tool', () => {
    const down = vi.spyOn(NodeTool.prototype, 'onMouseDown').mockImplementation(() => {});
    const reset = vi.spyOn(NodeTool.prototype, 'reset');
    const { container } = render(<Canvas />);
    const overlay = container.querySelectorAll('canvas')[1];
    fireEvent.keyDown(window, { code: 'Space' });
    fireEvent(window, new Event('blur'));
    pointer(overlay, 'pointerdown', { button: 0, buttons: 1, clientX: 100, clientY: 100 });
    expect(down).toHaveBeenCalledOnce();
    reset.mockClear();
    // The OS reports no held button after a missed release outside the window.
    pointer(overlay, 'pointermove', { buttons: 0, clientX: 140, clientY: 120 });
    expect(reset).toHaveBeenCalledOnce();
  });
});
