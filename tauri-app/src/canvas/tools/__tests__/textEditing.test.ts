import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SelectTool } from '../SelectTool';
import { TextTool } from '../TextTool';
import type { CanvasMouseEvent, ToolContext } from '../types';
import type { Bounds, ProjectObject, Transform2D } from '../../../types/project';
import type { ViewportParams } from '../../ViewportTransform';
import { useProjectStore } from '../../../stores/projectStore';
import { useUiStore } from '../../../stores/uiStore';
import { useAppStore } from '../../../stores/appStore';
import { makeProjectObject, makeTextObjectData } from '../../../test-utils/projectFixtures';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

// Mock canvas 2D context for textMeasure.ts (getCaretIndexFromClick)
HTMLCanvasElement.prototype.getContext = vi.fn().mockReturnValue({
  measureText: vi.fn().mockReturnValue({ width: 10 }),
  font: '',
}) as any;

const initialUiState = useUiStore.getState();
const initialAppState = useAppStore.getState();

afterEach(() => {
  useUiStore.setState(initialUiState, true);
  useAppStore.setState(initialAppState, true);
});

const identity: Transform2D = { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 };

const defaultVp: ViewportParams = {
  offset: { x: 0, y: 0 },
  zoom: 100,
  canvasWidth: 800,
  canvasHeight: 600,
};

function makeTextObject(id: string, content: string, bounds: Bounds, overrides: Partial<ProjectObject> = {}): ProjectObject {
  return makeProjectObject({
    id,
    name: 'Text',
    transform: { ...identity },
    bounds: { min: { ...bounds.min }, max: { ...bounds.max } },
    layer_id: 'layer1',
    data: makeTextObjectData({
      content,
      font_size_mm: 6,
    }),
    ...overrides,
  });
}

function makeVectorObject(id: string, bounds: Bounds, overrides: Partial<ProjectObject> = {}): ProjectObject {
  return makeProjectObject({
    id,
    name: id,
    transform: { ...identity },
    bounds: { min: { ...bounds.min }, max: { ...bounds.max } },
    layer_id: 'layer1',
    data: { type: 'vector_path', path_data: 'M 0 0 L 10 0 L 10 10 Z', closed: true },
    ...overrides,
  });
}

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
    addObject: vi.fn().mockResolvedValue(undefined),
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

describe('SelectTool double-click text editing', () => {
  let tool: SelectTool;

  // worldToScreen: screenX = worldX * 2 + 400, screenY = worldY * 2 + 300
  // textObj: world (10,10)-(50,20) → screen center ~(460,330)
  const textObj = makeTextObject('text-1', 'Hello', {
    min: { x: 10, y: 10 },
    max: { x: 50, y: 20 },
  });

  const vectorObj = makeVectorObject('vec-1', {
    min: { x: 10, y: 10 },
    max: { x: 50, y: 20 },
  });

  beforeEach(() => {
    tool = new SelectTool();
  });

  it('sets textEditObjectId on double-click of a text object with double-click mode', () => {
    const ctx = makeToolContext({
      objects: [textObj],
    });

    // screenX = 10*2+400=420, screenY = 10*2+300=320 → inside bounds
    const event = makeMouseEvent({
      screenX: 430,
      screenY: 325,
      worldX: 15,
      worldY: 12.5,
    });

    tool.onDoubleClick!(event, ctx);

    expect(ctx.selectObjects).toHaveBeenCalledWith(['text-1']);
    expect(useUiStore.getState().textEditObjectId).toBe('text-1');
    expect(useUiStore.getState().textEditMode).toBe('double-click');
  });

  it('does not set textEditObjectId on double-click of a non-text object', () => {
    const ctx = makeToolContext({
      objects: [vectorObj],
    });

    const event = makeMouseEvent({
      screenX: 430,
      screenY: 325,
      worldX: 15,
      worldY: 12.5,
    });

    tool.onDoubleClick!(event, ctx);

    // selectObjects not called because hit is a vector_path
    expect(useUiStore.getState().textEditObjectId).toBeNull();
  });

  it('does not set textEditObjectId when double-click hits nothing', () => {
    const ctx = makeToolContext({
      objects: [textObj],
    });

    // Click far away from any object
    const event = makeMouseEvent({
      screenX: 100,
      screenY: 100,
      worldX: -150,
      worldY: -100,
    });

    tool.onDoubleClick!(event, ctx);

    expect(useUiStore.getState().textEditObjectId).toBeNull();
  });

  it('clears textEditObjectId on mouse down (clicking away)', async () => {
    useUiStore.setState({ textEditObjectId: 'text-1' });

    const ctx = makeToolContext({
      objects: [textObj],
    });

    // Click on empty area
    const event = makeMouseEvent({
      screenX: 100,
      screenY: 100,
      worldX: -150,
      worldY: -100,
    });

    tool.onMouseDown(event, ctx);

    await vi.waitFor(() => {
      expect(useUiStore.getState().textEditObjectId).toBeNull();
    });
  });
});

describe('TextTool hit-test and stay-active', () => {
  let tool: TextTool;

  const textObj = makeTextObject('text-1', 'Hello', {
    min: { x: 10, y: 10 },
    max: { x: 50, y: 20 },
  });

  beforeEach(() => {
    tool = new TextTool();
    useUiStore.setState({ activeTool: 'text' });
  });

  it('enters edit mode on existing text, activeTool stays text', () => {
    const ctx = makeToolContext({ objects: [textObj] });

    const event = makeMouseEvent({
      screenX: 430,
      screenY: 325,
      worldX: 15,
      worldY: 12.5,
    });

    tool.onMouseDown(event, ctx);

    expect(ctx.selectObjects).toHaveBeenCalledWith(['text-1']);
    expect(useUiStore.getState().textEditObjectId).toBe('text-1');
    expect(useUiStore.getState().textEditMode).toBe('tool-click');
    expect(useUiStore.getState().activeTool).toBe('text');
  });

  it('ignores non-text objects and creates new text', async () => {
    const vectorObj = makeVectorObject('vec-1', {
      min: { x: 10, y: 10 },
      max: { x: 50, y: 20 },
    });
    const ctx = makeToolContext({ objects: [vectorObj] });

    const event = makeMouseEvent({
      screenX: 430,
      screenY: 325,
      worldX: 15,
      worldY: 12.5,
      snappedX: 15,
      snappedY: 12.5,
    });

    tool.onMouseDown(event, ctx);

    // Non-text hit → creates new text object
    await vi.waitFor(() => {
      expect(ctx.addObject).toHaveBeenCalledTimes(1);
    });
    expect(ctx.selectObjects).not.toHaveBeenCalled();
  });

  it('does nothing on locked text', () => {
    const lockedText = makeTextObject('text-locked', 'Locked', {
      min: { x: 10, y: 10 },
      max: { x: 50, y: 20 },
    }, { locked: true });
    const ctx = makeToolContext({ objects: [lockedText] });

    const event = makeMouseEvent({
      screenX: 430,
      screenY: 325,
      worldX: 15,
      worldY: 12.5,
    });

    tool.onMouseDown(event, ctx);

    // Neither addObject nor textEditObjectId should be set
    expect(ctx.addObject).not.toHaveBeenCalled();
    expect(useUiStore.getState().textEditObjectId).toBeNull();
  });

  it('stays in text tool after creating new text', async () => {
    const ctx = makeToolContext({
      objects: [],
      addObject: vi.fn().mockResolvedValue(makeTextObject('new-text-1', '', {
        min: { x: 40, y: 55 },
        max: { x: 60, y: 65 },
      })),
    });

    const event = makeMouseEvent({
      snappedX: 50,
      snappedY: 60,
      worldX: 50,
      worldY: 60,
    });

    tool.onMouseDown(event, ctx);

    // Wait for the promise to resolve
    await vi.waitFor(() => {
      expect(useUiStore.getState().textEditObjectId).toBe('new-text-1');
    });

    expect(useUiStore.getState().activeTool).toBe('text');
    expect(useUiStore.getState().textEditMode).toBe('new');
  });

  it('does not open inline editing on a stale selected object when create fails', async () => {
    useProjectStore.setState({ selectedObjectIds: ['existing-text'] });
    const ctx = makeToolContext({
      objects: [],
      addObject: vi.fn().mockResolvedValue(null),
    });

    const event = makeMouseEvent({
      snappedX: 50,
      snappedY: 60,
      worldX: 50,
      worldY: 60,
    });

    tool.onMouseDown(event, ctx);

    await vi.waitFor(() => {
      expect(ctx.addObject).toHaveBeenCalledTimes(1);
    });
    expect(useUiStore.getState().textEditObjectId).toBeNull();
  });
});

describe('uiStore textEditObjectId', () => {
  it('clears textEditObjectId and mode fields when tool changes', () => {
    useUiStore.setState({
      textEditObjectId: 'obj-1',
      textEditMode: 'tool-click',
      textEditCaretIndex: 3,
    });
    useUiStore.getState().setActiveTool('rect');
    expect(useUiStore.getState().textEditObjectId).toBeNull();
    expect(useUiStore.getState().textEditMode).toBeNull();
    expect(useUiStore.getState().textEditCaretIndex).toBeNull();
  });

  it('sets and clears textEditObjectId', () => {
    useUiStore.getState().setTextEditObjectId('obj-1');
    expect(useUiStore.getState().textEditObjectId).toBe('obj-1');

    useUiStore.getState().setTextEditObjectId(null);
    expect(useUiStore.getState().textEditObjectId).toBeNull();
  });
});
