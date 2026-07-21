import { beforeEach, describe, expect, it, vi } from 'vitest';
import { LaserPositionTool } from '../LaserPositionTool';
import type { CanvasMouseEvent, ToolContext } from '../types';
import type { ViewportParams } from '../../ViewportTransform';
import { useNotificationStore } from '../../../stores/notificationStore';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

// Mock machineStore
vi.mock('../../../stores/machineStore', () => ({
  useMachineStore: {
    getState: vi.fn().mockReturnValue({ sessionState: 'idle' }),
  },
}));

// Mock projectStore
vi.mock('../../../stores/projectStore', () => ({
  useProjectStore: {
    getState: vi.fn().mockReturnValue({ project: null }),
  },
}));

vi.mock('../../../stores/uiStore', () => ({
  useUiStore: {
    getState: vi.fn().mockReturnValue({ moveWindowJogFeedRateMmMin: 1500 }),
  },
}));

const defaultVp: ViewportParams = {
  offset: { x: 200, y: 200 },
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
    transformLocks: { move_enabled: true, size_enabled: true, rotate_enabled: true, shear_enabled: true },
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

function makeLaserProject(overrides: Record<string, unknown> = {}) {
  return {
    workspace: { bed_width_mm: 400, bed_height_mm: 300, origin: 'top_left' as const },
    start_from: 'absolute_coords',
    user_origin: null,
    ...overrides,
  } as never;
}

describe('LaserPositionTool', () => {
  let tool: LaserPositionTool;

  beforeEach(() => {
    tool = new LaserPositionTool();
    vi.clearAllMocks();
  });

  it('calls machineService.moveLaserTo on mouseDown', async () => {
    const { invoke } = await import('@tauri-apps/api/core');

    const ctx = makeToolContext();

    tool.onMouseDown(makeMouseEvent({ worldX: 50, worldY: 75 }), ctx);

    expect(invoke).toHaveBeenCalledWith('move_laser_to', {
      x: 50,
      y: 75,
      feedRate: 1500,
    });
  });

  it('surfaces move failures as an operator-visible error', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    const push = vi.fn();
    useNotificationStore.setState({ push });
    vi.mocked(invoke).mockRejectedValueOnce(new Error('move failed'));

    const setStatusMessage = vi.fn();
    tool.onMouseDown(makeMouseEvent({ worldX: 50, worldY: 75 }), makeToolContext({ setStatusMessage }));

    await vi.waitFor(() => {
      expect(setStatusMessage).toHaveBeenCalledWith('Error: move failed');
      expect(push).toHaveBeenCalledWith('Error: move failed', 'error');
    });
  });

  it('shows status message when disconnected', async () => {
    const { useMachineStore } = await import('../../../stores/machineStore');
    vi.mocked(useMachineStore.getState).mockReturnValue({
      ...useMachineStore.getState(),
      sessionState: 'disconnected',
    });

    const setStatusMessage = vi.fn();
    const ctx = makeToolContext({ setStatusMessage });

    tool.onMouseDown(makeMouseEvent({ worldX: 50, worldY: 75 }), ctx);

    expect(setStatusMessage).toHaveBeenCalledWith('Machine not connected');
  });

  it('applies user_origin offset in UserOrigin mode', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    const { useMachineStore } = await import('../../../stores/machineStore');
    const { useProjectStore } = await import('../../../stores/projectStore');

    vi.mocked(useMachineStore.getState).mockReturnValue({
      ...useMachineStore.getState(),
      sessionState: 'ready',
    });
    vi.mocked(useProjectStore.getState).mockReturnValue({
      ...useProjectStore.getState(),
      project: makeLaserProject({ start_from: 'user_origin', user_origin: [200, 100] }),
    });

    tool.onMouseDown(makeMouseEvent({ worldX: 50, worldY: 30 }), makeToolContext());

    expect(invoke).toHaveBeenCalledWith('move_laser_to', {
      x: 250,
      y: 130,
      feedRate: 1500,
    });
  });

  it('applies work_position offset in CurrentPosition mode', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    const { useMachineStore } = await import('../../../stores/machineStore');
    const { useProjectStore } = await import('../../../stores/projectStore');

    vi.mocked(useMachineStore.getState).mockReturnValue({
      ...useMachineStore.getState(),
      sessionState: 'ready',
      machineStatus: { work_position: { x: 100, y: 50, z: 0 } } as never,
    });
    vi.mocked(useProjectStore.getState).mockReturnValue({
      ...useProjectStore.getState(),
      project: makeLaserProject({ start_from: 'current_position' }),
    });

    tool.onMouseDown(makeMouseEvent({ worldX: 20, worldY: 10 }), makeToolContext());

    expect(invoke).toHaveBeenCalledWith('move_laser_to', {
      x: 120,
      y: 60,
      feedRate: 1500,
    });
  });

  it('no offset when user_origin is null in UserOrigin mode', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    const { useMachineStore } = await import('../../../stores/machineStore');
    const { useProjectStore } = await import('../../../stores/projectStore');

    vi.mocked(useMachineStore.getState).mockReturnValue({
      ...useMachineStore.getState(),
      sessionState: 'ready',
    });
    vi.mocked(useProjectStore.getState).mockReturnValue({
      ...useProjectStore.getState(),
      project: makeLaserProject({ start_from: 'user_origin', user_origin: null }),
    });

    tool.onMouseDown(makeMouseEvent({ worldX: 50, worldY: 75 }), makeToolContext());

    expect(invoke).toHaveBeenCalledWith('move_laser_to', {
      x: 50,
      y: 75,
      feedRate: 1500,
    });
  });

  it('no offset in AbsoluteCoords mode', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    const { useMachineStore } = await import('../../../stores/machineStore');
    const { useProjectStore } = await import('../../../stores/projectStore');

    vi.mocked(useMachineStore.getState).mockReturnValue({
      ...useMachineStore.getState(),
      sessionState: 'ready',
    });
    vi.mocked(useProjectStore.getState).mockReturnValue({
      ...useProjectStore.getState(),
      project: makeLaserProject({ start_from: 'absolute_coords' }),
    });

    tool.onMouseDown(makeMouseEvent({ worldX: 50, worldY: 75 }), makeToolContext());

    expect(invoke).toHaveBeenCalledWith('move_laser_to', {
      x: 50,
      y: 75,
      feedRate: 1500,
    });
  });

  it('converts bottom-left workspace clicks to machine Y coordinates', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    const { useMachineStore } = await import('../../../stores/machineStore');
    const { useProjectStore } = await import('../../../stores/projectStore');

    vi.mocked(useMachineStore.getState).mockReturnValue({
      ...useMachineStore.getState(),
      sessionState: 'ready',
    });
    vi.mocked(useProjectStore.getState).mockReturnValue({
      ...useProjectStore.getState(),
      project: makeLaserProject({
        workspace: { bed_width_mm: 400, bed_height_mm: 300, origin: 'bottom_left' as const },
        start_from: 'absolute_coords',
      }),
    });

    tool.onMouseDown(makeMouseEvent({ worldX: 400, worldY: 0 }), makeToolContext());
    tool.onMouseDown(makeMouseEvent({ worldX: 400, worldY: 300 }), makeToolContext());

    expect(invoke).toHaveBeenNthCalledWith(1, 'move_laser_to', {
      x: 400,
      y: 300,
      feedRate: 1500,
    });
    expect(invoke).toHaveBeenNthCalledWith(2, 'move_laser_to', {
      x: 400,
      y: 0,
      feedRate: 1500,
    });
  });

  it('converts bottom-left clicks before applying user-origin offsets', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    const { useMachineStore } = await import('../../../stores/machineStore');
    const { useProjectStore } = await import('../../../stores/projectStore');

    vi.mocked(useMachineStore.getState).mockReturnValue({
      ...useMachineStore.getState(),
      sessionState: 'ready',
    });
    vi.mocked(useProjectStore.getState).mockReturnValue({
      ...useProjectStore.getState(),
      project: makeLaserProject({
        workspace: { bed_width_mm: 400, bed_height_mm: 300, origin: 'bottom_left' as const },
        start_from: 'user_origin',
        user_origin: [200, 100],
      }),
    });

    tool.onMouseDown(makeMouseEvent({ worldX: 50, worldY: 30 }), makeToolContext());

    expect(invoke).toHaveBeenCalledWith('move_laser_to', {
      x: 250,
      y: 370,
      feedRate: 1500,
    });
  });

  it('does not send a move for clicks outside the workspace', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    const { useMachineStore } = await import('../../../stores/machineStore');
    const { useProjectStore } = await import('../../../stores/projectStore');

    vi.mocked(useMachineStore.getState).mockReturnValue({
      ...useMachineStore.getState(),
      sessionState: 'ready',
    });
    vi.mocked(useProjectStore.getState).mockReturnValue({
      ...useProjectStore.getState(),
      project: makeLaserProject({
        workspace: { bed_width_mm: 400, bed_height_mm: 300, origin: 'bottom_left' as const },
      }),
    });

    const setStatusMessage = vi.fn();
    tool.onMouseDown(
      makeMouseEvent({ worldX: 401, worldY: 150 }),
      makeToolContext({ setStatusMessage }),
    );
    tool.onMouseDown(
      makeMouseEvent({ worldX: 200, worldY: 301 }),
      makeToolContext({ setStatusMessage }),
    );

    expect(invoke).not.toHaveBeenCalled();
    expect(setStatusMessage).toHaveBeenCalledWith('Click inside the workspace to move the laser');
  });
});
