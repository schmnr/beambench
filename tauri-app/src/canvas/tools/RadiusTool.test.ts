import { beforeEach, describe, expect, it, vi } from 'vitest';
import { RadiusTool } from './RadiusTool';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

describe('RadiusTool', () => {
  let tool: RadiusTool;

  beforeEach(() => {
    tool = new RadiusTool();
    vi.clearAllMocks();
  });

  it('reset clears state and overlay is none', () => {
    // @ts-expect-error accessing private field
    tool.candidates = {
      objectId: 'obj1',
      markers: [{ subpathIndex: 0, vertexIndex: 0, x: 10, y: 0, alreadyFilleted: false }],
    };
    // @ts-expect-error accessing private field
    tool.hoveredIndex = 0;

    tool.reset();

    // @ts-expect-error accessing private field
    expect(tool.candidates).toBeNull();
    // @ts-expect-error accessing private field
    expect(tool.hoveredIndex).toBeNull();
    expect(tool.getOverlay().type).toBe('none');
  });

  it('cursor is crosshair by default', () => {
    expect(tool.getCursor()).toBe('crosshair');
  });

  it('getOverlay with no candidates returns none', () => {
    const overlay = tool.getOverlay();
    expect(overlay.type).toBe('none');
  });

  it('clicking already-filleted corner sends radius 0 to toggle off', async () => {
    // Set up stores with a nonzero radius
    const { useUiStore } = await import('../../stores/uiStore');
    const { useProjectStore } = await import('../../stores/projectStore');
    useUiStore.setState({ radiusToolValue: 5 });

    const applyCornerRadius = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ applyCornerRadius } as any);

    // Set up tool state with an already-filleted candidate
    // @ts-expect-error accessing private field
    tool.candidates = {
      objectId: 'obj1',
      markers: [
        { subpathIndex: 0, vertexIndex: 1, x: 10, y: 0, alreadyFilleted: true },
      ],
    };
    // @ts-expect-error accessing private field
    tool.hoveredIndex = 0;

    const ctx = {
      selectedObjectIds: ['obj1'],
      requestRender: vi.fn(),
      vp: { zoom: 1, panX: 0, panY: 0 },
    } as any;

    tool.onMouseDown({ worldX: 10, worldY: 0 } as any, ctx);

    // Allow the async handler to run
    await vi.waitFor(() => {
      expect(applyCornerRadius).toHaveBeenCalled();
    });

    // Should have sent radius 0 (unfillet), not the tool's active radius of 5
    expect(applyCornerRadius).toHaveBeenCalledWith('obj1', 0, 1, 0);
  });

  it('clicking unfilleted corner sends the active radius', async () => {
    const { useUiStore } = await import('../../stores/uiStore');
    const { useProjectStore } = await import('../../stores/projectStore');
    useUiStore.setState({ radiusToolValue: 5 });

    const applyCornerRadius = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ applyCornerRadius } as any);

    // @ts-expect-error accessing private field
    tool.candidates = {
      objectId: 'obj1',
      markers: [
        { subpathIndex: 0, vertexIndex: 1, x: 10, y: 0, alreadyFilleted: false },
      ],
    };
    // @ts-expect-error accessing private field
    tool.hoveredIndex = 0;

    const ctx = {
      selectedObjectIds: ['obj1'],
      requestRender: vi.fn(),
      vp: { zoom: 1, panX: 0, panY: 0 },
    } as any;

    tool.onMouseDown({ worldX: 10, worldY: 0 } as any, ctx);

    await vi.waitFor(() => {
      expect(applyCornerRadius).toHaveBeenCalled();
    });

    // Should send radius 5 (the active tool value)
    expect(applyCornerRadius).toHaveBeenCalledWith('obj1', 0, 1, 5);
  });
});
