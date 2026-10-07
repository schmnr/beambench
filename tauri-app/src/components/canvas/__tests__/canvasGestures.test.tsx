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

describe('cancelled canvas gestures', () => {
  async function setup(activeTool: 'select' | 'two_point_rotate_scale', selected: boolean) {
    const { makeProject, makeProjectObject, makeLayer } = await import('../../../test-utils/projectFixtures');
    const obj = makeProjectObject({ id: 'gesture-obj', layer_id: 'layer1', bounds: { min: { x: 10, y: 10 }, max: { x: 20, y: 20 } }, data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 } });
    useProjectStore.setState({ project: makeProject({ objects: [obj], layers: [makeLayer({ id: 'layer1' })] }), selectedObjectIds: selected ? [obj.id] : [] });
    useUiStore.setState({ activeTool, zoom: 100, gridVisible: false, snapToGrid: false, snapToObjects: false });
    const { container } = render(<Canvas />);
    act(() => useUiStore.setState({ viewportOffset: { x: 0, y: 0 }, zoom: 100 }));
    return { obj, overlay: container.querySelectorAll('canvas')[1] };
  }

  it('switching tools mid-drag puts the object back', async () => {
    const { obj, overlay } = await setup('select', false);
    const original = structuredClone(obj.bounds);
    pointer(overlay, 'pointerdown', { button: 0, buttons: 1, clientX: 430, clientY: 330 });
    pointer(overlay, 'pointermove', { buttons: 1, clientX: 450, clientY: 330 }); flush();
    pointer(overlay, 'pointermove', { buttons: 1, clientX: 470, clientY: 330 }); flush();
    expect(obj.bounds).not.toEqual(original);
    act(() => useUiStore.setState({ activeTool: 'rect' }));
    pointer(overlay, 'pointerup', { button: 0, buttons: 0, clientX: 470, clientY: 330 });
    expect(obj.bounds).toEqual(original);
  });

  it('pointercancel restores a two-point rotate preview', async () => {
    const { obj, overlay } = await setup('two_point_rotate_scale', true);
    const original = structuredClone({ bounds: obj.bounds, transform: obj.transform });
    pointer(overlay, 'pointerdown', { button: 0, buttons: 1, clientX: 400, clientY: 300 });
    pointer(overlay, 'pointerup', { button: 0, buttons: 0, clientX: 400, clientY: 300 });
    pointer(overlay, 'pointerdown', { button: 0, buttons: 1, clientX: 420, clientY: 300 });
    pointer(overlay, 'pointermove', { buttons: 1, clientX: 400, clientY: 320 }); flush();
    expect({ bounds: obj.bounds, transform: obj.transform }).not.toEqual(original);
    pointer(overlay, 'pointercancel', {});
    expect({ bounds: obj.bounds, transform: obj.transform }).toEqual(original);
  });
});
