import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TrimTool } from './TrimTool';
import type { CanvasMouseEvent, ToolContext } from './types';
import type { ViewportParams } from '../ViewportTransform';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue({ objects: [], healFailed: false, openResult: false }),
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const defaultVp: ViewportParams = {
  offset: { x: 0, y: 0 },
  zoom: 100,
  canvasWidth: 800,
  canvasHeight: 600,
};

function makeMouseEvent(overrides: Partial<CanvasMouseEvent> = {}): CanvasMouseEvent {
  return {
    screenX: 0, screenY: 0,
    worldX: 0, worldY: 0,
    snappedX: 0, snappedY: 0,
    button: 0, shiftKey: false, ctrlKey: false, altKey: false,
    ...overrides,
  };
}

function makeToolContext(overrides: Partial<ToolContext> = {}): ToolContext {
  return {
    vp: defaultVp,
    objects: [],
    selectedObjectIds: [],
    selectedLayerId: 'layer1',
    layers: [{ id: 'layer1', enabled: true }],
    snapEnabled: false,
    snapToObjects: false,
    gridSpacingMm: 10,
    selectObjects: vi.fn(),
    toggleObjectSelection: vi.fn(),
    addObject: vi.fn(),
    updateObject: vi.fn(),
    rotateObjects: vi.fn(),
    shearObjects: vi.fn(),
    updateObjectBoundsBatch: vi.fn(),
    setCursorWorldPos: vi.fn(),
    setStatusMessage: vi.fn(),
    requestRender: vi.fn(),
    ...overrides,
  };
}

describe('TrimTool', () => {
  let tool: TrimTool;

  beforeEach(() => {
    vi.clearAllMocks();
    tool = new TrimTool();
  });

  it('always shows crosshair cursor', () => {
    expect(tool.getCursor()).toBe('crosshair');
    const ctx = makeToolContext();
    tool.onMouseMove(makeMouseEvent({ screenX: 600, screenY: 500 }), ctx);
    expect(tool.getCursor()).toBe('crosshair');
  });

  it('mouseDown calls trim_shape with click coordinates, threshold and heal', async () => {
    const { invoke } = await import('@tauri-apps/api/core');

    const ctx = makeToolContext();

    tool.onMouseDown(makeMouseEvent({
      screenX: 600, screenY: 500,
      worldX: 100, worldY: 100,
    }), ctx);

    await new Promise((r) => setTimeout(r, 10));

    expect(invoke).toHaveBeenCalledWith('trim_shape', {
      clickX: 100,
      clickY: 100,
      edgeThresholdMm: expect.any(Number),
      heal: true,
    });
  });

  it('alt+click sends heal=false', async () => {
    const { invoke } = await import('@tauri-apps/api/core');

    const ctx = makeToolContext();

    tool.onMouseDown(makeMouseEvent({
      worldX: 50, worldY: 50,
      altKey: true,
    }), ctx);

    await new Promise((r) => setTimeout(r, 10));

    expect(invoke).toHaveBeenCalledWith('trim_shape', expect.objectContaining({
      heal: false,
    }));
  });

  it('shows warning notification on error', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockRejectedValueOnce('No intersections found');

    const { useNotificationStore } = await import('../../stores/notificationStore');
    useNotificationStore.setState({ notifications: [] });

    const ctx = makeToolContext();

    tool.onMouseDown(makeMouseEvent({
      worldX: 50, worldY: 50,
    }), ctx);

    await new Promise((r) => setTimeout(r, 50));

    expect(invoke).toHaveBeenCalledWith('trim_shape', expect.any(Object));
    const notifications = useNotificationStore.getState().notifications;
    expect(notifications.length).toBeGreaterThanOrEqual(1);
    const warning = notifications.find((n) => n.type === 'warning');
    expect(warning).toBeDefined();
    expect(warning!.message).toContain('No intersections found');
  });

  it('shows info notification when heal fails', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      objects: [{ id: 'a' }, { id: 'b' }],
      healFailed: true,
    });

    const { useNotificationStore } = await import('../../stores/notificationStore');
    useNotificationStore.setState({ notifications: [] });

    const ctx = makeToolContext();

    tool.onMouseDown(makeMouseEvent({
      worldX: 50, worldY: 50,
    }), ctx);

    await new Promise((r) => setTimeout(r, 50));

    const notifications = useNotificationStore.getState().notifications;
    const info = notifications.find((n) => n.type === 'info');
    expect(info).toBeDefined();
    expect(info!.message).toContain('Close & Join');
  });

  it('shows info notification when result is open', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      objects: [{ id: 'a' }],
      healFailed: false,
      openResult: true,
    });

    const { useNotificationStore } = await import('../../stores/notificationStore');
    useNotificationStore.setState({ notifications: [] });

    const ctx = makeToolContext();

    tool.onMouseDown(makeMouseEvent({
      worldX: 50, worldY: 50,
    }), ctx);

    await new Promise((r) => setTimeout(r, 50));

    const notifications = useNotificationStore.getState().notifications;
    const info = notifications.find((n) => n.type === 'info');
    expect(info).toBeDefined();
    expect(info!.message).toContain('not fill-ready');
  });

  it('reset does not throw', () => {
    expect(() => tool.reset()).not.toThrow();
  });

  it('getOverlay returns none initially', () => {
    expect(tool.getOverlay()).toEqual({ type: 'none' });
  });

  it('shows trim-preview overlay after hover', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      segmentPoints: [[10, 20], [30, 40]],
    });

    const requestRender = vi.fn();
    const ctx = makeToolContext({ requestRender });

    tool.onMouseMove(makeMouseEvent({ worldX: 15, worldY: 25 }), ctx);

    // Wait for debounce (50ms) + async
    await new Promise((r) => setTimeout(r, 100));

    expect(invoke).toHaveBeenCalledWith('preview_trim_segment', expect.any(Object));
    const overlay = tool.getOverlay();
    expect(overlay.type).toBe('trim-preview');
    if (overlay.type === 'trim-preview') {
      expect(overlay.segmentScreenPoints.length).toBe(2);
    }
    expect(requestRender).toHaveBeenCalled();
  });

  it('stale preview is discarded', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    const resolvers: Array<(v: unknown) => void> = [];
    (invoke as ReturnType<typeof vi.fn>)
      .mockImplementation(() => new Promise((resolve) => { resolvers.push(resolve); }));

    const requestRender = vi.fn();
    const ctx = makeToolContext({ requestRender });

    // First move
    tool.onMouseMove(makeMouseEvent({ worldX: 10, worldY: 10 }), ctx);
    await new Promise((r) => setTimeout(r, 60));

    // Second move (overwrites first)
    tool.onMouseMove(makeMouseEvent({ worldX: 20, worldY: 20 }), ctx);
    await new Promise((r) => setTimeout(r, 60));

    // Resolve first (stale) — should be discarded
    resolvers[0]?.({ segmentPoints: [[1, 1], [2, 2]] });
    await new Promise((r) => setTimeout(r, 10));

    // Resolve second (current) — should be applied
    resolvers[1]?.({ segmentPoints: [[3, 3], [4, 4]] });
    await new Promise((r) => setTimeout(r, 10));

    const overlay = tool.getOverlay();
    expect(overlay.type).toBe('trim-preview');
    if (overlay.type === 'trim-preview') {
      // Should use second response, not first
      expect(overlay.segmentScreenPoints.length).toBe(2);
    }
  });

  it('clears preview and triggers render on fetch error', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    // First succeed to populate preview
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      segmentPoints: [[10, 20], [30, 40]],
    });

    const requestRender = vi.fn();
    const ctx = makeToolContext({ requestRender });

    tool.onMouseMove(makeMouseEvent({ worldX: 15, worldY: 25 }), ctx);
    await new Promise((r) => setTimeout(r, 100));

    expect(tool.getOverlay().type).toBe('trim-preview');
    requestRender.mockClear();

    // Now fail on next preview fetch
    (invoke as ReturnType<typeof vi.fn>).mockRejectedValueOnce('backend error');
    tool.onMouseMove(makeMouseEvent({ worldX: 20, worldY: 30 }), ctx);
    await new Promise((r) => setTimeout(r, 100));

    expect(tool.getOverlay()).toEqual({ type: 'none' });
    expect(requestRender).toHaveBeenCalled();
  });

  it('clears preview on reset', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      segmentPoints: [[10, 20], [30, 40]],
    });

    const requestRender = vi.fn();
    const ctx = makeToolContext({ requestRender });

    tool.onMouseMove(makeMouseEvent({ worldX: 15, worldY: 25 }), ctx);
    await new Promise((r) => setTimeout(r, 100));

    // Should have preview
    expect(tool.getOverlay().type).toBe('trim-preview');

    // Reset clears it
    tool.reset();
    expect(tool.getOverlay()).toEqual({ type: 'none' });
  });
});
