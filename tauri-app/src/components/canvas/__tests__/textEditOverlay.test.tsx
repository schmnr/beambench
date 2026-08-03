import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render, screen, cleanup, fireEvent, waitFor, act } from '@testing-library/react';
import { useProjectStore } from '../../../stores/projectStore';
import { useUiStore } from '../../../stores/uiStore';
import {
  setPendingEdit, updatePendingContent, commitPendingTextEdit, getPendingContent, clearPendingEdit,
} from '../../../canvas/textEditSession';
import { makeProjectObject, makeTextObjectData } from '../../../test-utils/projectFixtures';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

// Must import after mocks
import { TextEditOverlay } from '../TextEditOverlay';

const initialProjectState = useProjectStore.getState();
const initialUiState = useUiStore.getState();

const makeTextObject = (id: string, content: string, overrides: Record<string, any> = {}) => ({
  ...makeProjectObject({
    id,
    name: 'Text',
    bounds: { min: { x: 10, y: 20 }, max: { x: 50, y: 30 } },
    layer_id: 'layer-1',
    data: makeTextObjectData({
      content,
      font_size_mm: 6,
      ...overrides,
    }),
  }),
});

const makeProject = (objects: any[]) => ({
  name: 'test',
  workspace: { width_mm: 400, height_mm: 400, origin: 'top_left' },
  layers: [{ id: 'layer-1', name: 'Layer 1', operation: 'line', enabled: true, order_index: 0, color_tag: '#ff0000', speed_mm_min: 100, power_percent: 50, raster_settings: null, vector_settings: null, visible: true }],
  objects,
});

const defaultVp = {
  offset: { x: 200, y: 200 },
  zoom: 100,
  canvasWidth: 800,
  canvasHeight: 600,
};

describe('TextEditOverlay', () => {
  beforeEach(() => {
    useProjectStore.setState(initialProjectState, true);
    useUiStore.setState(initialUiState, true);
  });

  afterEach(() => {
    cleanup();
    clearPendingEdit();
    useProjectStore.setState(initialProjectState, true);
    useUiStore.setState(initialUiState, true);
  });

  it('does not render when textEditObjectId is null', () => {
    useUiStore.setState({ textEditObjectId: null });
    render(<TextEditOverlay vp={defaultVp} />);
    expect(screen.queryByTestId('text-edit-overlay')).toBeNull();
  });

  it('renders textarea when textEditObjectId matches a text object', () => {
    const obj = makeTextObject('obj-1', 'Hello');
    useProjectStore.setState({ project: makeProject([obj]) as any });
    useUiStore.setState({ textEditObjectId: 'obj-1' });

    render(<TextEditOverlay vp={defaultVp} />);
    const textarea = screen.getByTestId('text-edit-overlay');
    expect(textarea).toBeTruthy();
    expect((textarea as HTMLTextAreaElement).value).toBe('Hello');
  });

  it('accepts buffered text before the textarea receives focus', () => {
    const obj = makeTextObject('obj-1', '');
    useProjectStore.setState({ project: makeProject([obj]) as any });
    useUiStore.setState({ textEditObjectId: 'obj-1', textEditMode: 'new' });
    setPendingEdit('obj-1', '');

    render(<TextEditOverlay vp={defaultVp} />);
    act(() => updatePendingContent('P'));

    expect((screen.getByTestId('text-edit-overlay') as HTMLTextAreaElement).value).toBe('P');
  });

  it('does not render when textEditObjectId points to a non-text object', () => {
    const obj = {
      id: 'obj-vec',
      name: 'Path',
      visible: true,
      locked: false,
      transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
      bounds: { min: { x: 10, y: 20 }, max: { x: 50, y: 30 } },
      layer_id: 'layer-1',
      z_index: 0,
      data: { type: 'vector_path' as const, path_data: 'M 0 0 L 10 10', closed: false },
    };
    useProjectStore.setState({ project: makeProject([obj]) as any });
    useUiStore.setState({ textEditObjectId: 'obj-vec' });

    render(<TextEditOverlay vp={defaultVp} />);
    expect(screen.queryByTestId('text-edit-overlay')).toBeNull();
  });

  it('renders with correct initial content value', () => {
    const obj = makeTextObject('obj-1', 'Hello World');
    useProjectStore.setState({ project: makeProject([obj]) as any });
    useUiStore.setState({ textEditObjectId: 'obj-1' });

    render(<TextEditOverlay vp={defaultVp} />);
    const textarea = screen.getByTestId('text-edit-overlay') as HTMLTextAreaElement;

    expect(textarea.value).toBe('Hello World');
  });

  it('clears textEditObjectId on Escape', async () => {
    const obj = makeTextObject('obj-1', 'Hello');
    useProjectStore.setState({ project: makeProject([obj]) as any });
    useUiStore.setState({ textEditObjectId: 'obj-1' });

    render(<TextEditOverlay vp={defaultVp} />);
    const textarea = screen.getByTestId('text-edit-overlay');

    fireEvent.keyDown(textarea, { key: 'Escape' });

    await waitFor(() => {
      expect(useUiStore.getState().textEditObjectId).toBeNull();
    });
  });

  it('Enter does not close overlay (inserts newline)', () => {
    const obj = makeTextObject('obj-1', 'Hello');
    useProjectStore.setState({ project: makeProject([obj]) as any });
    useUiStore.setState({ textEditObjectId: 'obj-1' });

    render(<TextEditOverlay vp={defaultVp} />);
    const textarea = screen.getByTestId('text-edit-overlay');

    fireEvent.keyDown(textarea, { key: 'Enter' });

    // Overlay should still be open — Enter inserts newline, does NOT commit
    expect(useUiStore.getState().textEditObjectId).toBe('obj-1');
  });

  it('Cmd/Ctrl+Enter commits and closes the overlay', async () => {
    const obj = makeTextObject('obj-1', 'Hello');
    useProjectStore.setState({ project: makeProject([obj]) as any });
    useUiStore.setState({ textEditObjectId: 'obj-1', textEditMode: 'double-click' });

    render(<TextEditOverlay vp={defaultVp} />);
    fireEvent.keyDown(screen.getByTestId('text-edit-overlay'), { key: 'Enter', metaKey: true });

    await waitFor(() => {
      expect(useUiStore.getState().textEditObjectId).toBeNull();
    });
  });

  it('keeps keyboard zoom available while editing text', () => {
    const obj = makeTextObject('obj-1', 'Hello');
    useProjectStore.setState({ project: makeProject([obj]) as any });
    useUiStore.setState({ textEditObjectId: 'obj-1', zoom: 100 });

    render(<TextEditOverlay vp={defaultVp} />);
    const textarea = screen.getByTestId('text-edit-overlay');

    fireEvent.keyDown(textarea, { key: '=', metaKey: true });
    expect(useUiStore.getState().zoom).toBeGreaterThan(100);

    const zoomedIn = useUiStore.getState().zoom;
    fireEvent.keyDown(textarea, { key: '-', ctrlKey: true });
    expect(useUiStore.getState().zoom).toBeLessThan(zoomedIn);
    expect(useUiStore.getState().textEditObjectId).toBe('obj-1');
  });

  it('shows a subtle resizable frame only for Box text', () => {
    const obj = {
      ...makeTextObject('obj-1', 'A wrapped line', { max_width: 30, v_spacing: 1.5 }),
      bounds: { min: { x: 10, y: 20 }, max: { x: 40, y: 50 } },
    };
    expect((obj.data as any).max_width).toBe(30);
    useProjectStore.setState({ project: makeProject([obj]) as any });
    useUiStore.setState({ textEditObjectId: 'obj-1' });

    render(<TextEditOverlay vp={defaultVp} />);
    const textarea = screen.getByTestId('text-edit-overlay') as HTMLTextAreaElement;
    const frame = screen.getByTestId('text-area-frame');

    expect(textarea.className).toContain('border-none');
    expect(textarea.className).toContain('text-edit-overlay');
    expect(frame.className).toContain('border-dashed');
    expect(frame.className).toContain('border-bb-accent/70');
    expect(frame.style.width).toBe('60px');
    expect(frame.style.height).toBe('60px');
    expect(screen.getByTestId('text-area-resize-handle')).toBeTruthy();
    expect(textarea.style.padding).toBe('0px');
    expect(textarea.wrap).toBe('soft');
    expect(textarea.style.width).toBe('100%');
    expect(textarea.style.height).toBe('100%');
    expect(textarea.style.overflow).toBe('auto');
    expect(textarea.style.whiteSpace).toBe('pre-wrap');
    expect(textarea.style.lineHeight).toBe('15px');
  });

  it('keeps Point text borderless without an area frame', () => {
    const obj = makeTextObject('obj-1', 'Point text', { max_width: null });
    useProjectStore.setState({ project: makeProject([obj]) as any });
    useUiStore.setState({ textEditObjectId: 'obj-1' });

    render(<TextEditOverlay vp={defaultVp} />);

    expect(screen.queryByTestId('text-area-frame')).toBeNull();
    expect(screen.getByTestId('text-edit-overlay').className).toContain('border-none');
  });

  it('resizes the Box frame without using generic text scaling', async () => {
    const obj = {
      ...makeTextObject('obj-1', 'Area text', { max_width: 30 }),
      bounds: { min: { x: 10, y: 20 }, max: { x: 40, y: 50 } },
    };
    const resizeTextArea = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({
      project: makeProject([obj]) as any,
      resizeTextArea,
    });
    useUiStore.setState({ textEditObjectId: 'obj-1' });

    render(<TextEditOverlay vp={defaultVp} />);
    const handle = screen.getByTestId('text-area-resize-handle');
    Object.defineProperty(handle, 'setPointerCapture', { value: vi.fn() });
    Object.defineProperty(handle, 'releasePointerCapture', { value: vi.fn() });

    const dispatchPointer = (type: string, clientX: number, clientY: number) => {
      const event = new MouseEvent(type, { bubbles: true, clientX, clientY });
      Object.defineProperty(event, 'pointerId', { value: 1 });
      fireEvent(handle, event);
    };
    dispatchPointer('pointerdown', 100, 100);
    dispatchPointer('pointermove', 140, 120);
    dispatchPointer('pointerup', 140, 120);

    await waitFor(() => {
      expect(resizeTextArea).toHaveBeenCalledWith('obj-1', {
        min: { x: 10, y: 20 },
        max: { x: 60, y: 60 },
      });
    });
  });

  it('places the caret at the supplied canvas index', async () => {
    const obj = makeTextObject('obj-1', 'Hello world');
    useProjectStore.setState({ project: makeProject([obj]) as any });
    useUiStore.setState({
      textEditObjectId: 'obj-1',
      textEditMode: 'double-click',
      textEditCaretIndex: 6,
    });

    render(<TextEditOverlay vp={defaultVp} />);
    const textarea = screen.getByTestId('text-edit-overlay') as HTMLTextAreaElement;

    await waitFor(() => {
      expect(textarea.selectionStart).toBe(6);
      expect(textarea.selectionEnd).toBe(6);
    });
  });

  it('center-aligned overlay has translateX(-50%) transform', () => {
    const obj = makeTextObject('obj-1', 'Hello', { alignment: 'center' });
    useProjectStore.setState({ project: makeProject([obj]) as any });
    useUiStore.setState({ textEditObjectId: 'obj-1', textEditMode: 'tool-click' });

    render(<TextEditOverlay vp={defaultVp} />);
    const textarea = screen.getByTestId('text-edit-overlay') as HTMLTextAreaElement;

    expect(textarea.style.transform).toContain('translateX(-50%)');
  });

  it('textarea textAlign matches object alignment', () => {
    const obj = makeTextObject('obj-1', 'Hello', { alignment: 'center' });
    useProjectStore.setState({ project: makeProject([obj]) as any });
    useUiStore.setState({ textEditObjectId: 'obj-1' });

    render(<TextEditOverlay vp={defaultVp} />);
    const textarea = screen.getByTestId('text-edit-overlay') as HTMLTextAreaElement;

    expect(textarea.style.textAlign).toBe('center');
  });

  it('right-aligned overlay has translateX(-100%) transform', () => {
    const obj = makeTextObject('obj-1', 'Hello', { alignment: 'right' });
    useProjectStore.setState({ project: makeProject([obj]) as any });
    useUiStore.setState({ textEditObjectId: 'obj-1', textEditMode: 'tool-click' });

    render(<TextEditOverlay vp={defaultVp} />);
    const textarea = screen.getByTestId('text-edit-overlay') as HTMLTextAreaElement;

    expect(textarea.style.transform).toContain('translateX(-100%)');
  });

  it('Escape on brand-new empty text commits and removes the object', async () => {
    const obj = makeTextObject('obj-1', '');
    useProjectStore.setState({ project: makeProject([obj]) as any });
    useUiStore.setState({
      textEditObjectId: 'obj-1',
      textEditMode: 'new',
      textEditClickPos: { x: 10, y: 20 },
    });
    const removeSpy = vi.spyOn(useProjectStore.getState(), 'removeObject').mockImplementation(() => Promise.resolve());

    render(<TextEditOverlay vp={defaultVp} />);
    const textarea = screen.getByTestId('text-edit-overlay');

    // Press Escape without typing anything → should delete the empty new text
    fireEvent.keyDown(textarea, { key: 'Escape' });

    await waitFor(() => {
      expect(useUiStore.getState().textEditObjectId).toBeNull();
      expect(removeSpy).toHaveBeenCalledWith('obj-1');
    });
    removeSpy.mockRestore();
  });

  it('Escape on existing text (double-click mode) commits but does NOT remove the object', async () => {
    const obj = makeTextObject('obj-1', 'Hello');
    useProjectStore.setState({ project: makeProject([obj]) as any });
    useUiStore.setState({ textEditObjectId: 'obj-1', textEditMode: 'double-click' });
    const removeSpy = vi.spyOn(useProjectStore.getState(), 'removeObject').mockImplementation(() => Promise.resolve());

    render(<TextEditOverlay vp={defaultVp} />);
    const textarea = screen.getByTestId('text-edit-overlay');

    fireEvent.keyDown(textarea, { key: 'Escape' });

    await waitFor(() => {
      expect(useUiStore.getState().textEditObjectId).toBeNull();
    });
    expect(removeSpy).not.toHaveBeenCalled();
    removeSpy.mockRestore();
  });

  it('Escape on brand-new text with typed content commits but does NOT remove the object', async () => {
    const obj = makeTextObject('obj-1', '');
    useProjectStore.setState({ project: makeProject([obj]) as any });
    useUiStore.setState({
      textEditObjectId: 'obj-1',
      textEditMode: 'new',
      textEditClickPos: { x: 10, y: 20 },
    });
    const removeSpy = vi.spyOn(useProjectStore.getState(), 'removeObject').mockImplementation(() => Promise.resolve());
    // Mock updateObjectData so the fire-and-forget commit doesn't resolve
    // outside act() (content changed from '' to 'Hello' triggers a real commit).
    const updateSpy = vi.spyOn(useProjectStore.getState(), 'updateObjectData').mockImplementation(() => Promise.resolve(true));

    render(<TextEditOverlay vp={defaultVp} />);
    const textarea = screen.getByTestId('text-edit-overlay') as HTMLTextAreaElement;

    // Type some content then Escape
    fireEvent.change(textarea, { target: { value: 'Hello' } });
    fireEvent.keyDown(textarea, { key: 'Escape' });

    await waitFor(() => {
      expect(useUiStore.getState().textEditObjectId).toBeNull();
    });
    expect(removeSpy).not.toHaveBeenCalled();
    removeSpy.mockRestore();
    updateSpy.mockRestore();
  });

  it('Escape commits changed content via updateObjectData', async () => {
    const obj = makeTextObject('obj-1', 'Hello');
    useProjectStore.setState({ project: makeProject([obj]) as any });
    useUiStore.setState({ textEditObjectId: 'obj-1', textEditMode: 'double-click' });
    const updateSpy = vi.spyOn(useProjectStore.getState(), 'updateObjectData').mockImplementation(() => Promise.resolve(true));

    render(<TextEditOverlay vp={defaultVp} />);
    const textarea = screen.getByTestId('text-edit-overlay') as HTMLTextAreaElement;

    // Change content
    fireEvent.change(textarea, { target: { value: 'World' } });

    // Press Escape — should commit the new content
    fireEvent.keyDown(textarea, { key: 'Escape' });

    await waitFor(() => {
      expect(useUiStore.getState().textEditObjectId).toBeNull();
      expect(updateSpy).toHaveBeenCalledWith(
        'obj-1',
        expect.objectContaining({ content: 'World' }),
      );
    });
    updateSpy.mockRestore();
  });

  it('keeps the overlay open when inline save fails', async () => {
    const obj = makeTextObject('obj-1', 'Hello');
    useProjectStore.setState({ project: makeProject([obj]) as any });
    useUiStore.setState({ textEditObjectId: 'obj-1', textEditMode: 'double-click' });
    const updateSpy = vi.spyOn(useProjectStore.getState(), 'updateObjectData').mockImplementation(() => Promise.resolve(false));

    render(<TextEditOverlay vp={defaultVp} />);
    const textarea = screen.getByTestId('text-edit-overlay') as HTMLTextAreaElement;

    fireEvent.change(textarea, { target: { value: 'World' } });
    fireEvent.keyDown(textarea, { key: 'Escape' });

    await waitFor(() => {
      expect(useUiStore.getState().textEditObjectId).toBe('obj-1');
      expect((screen.getByTestId('text-edit-overlay') as HTMLTextAreaElement).value).toBe('World');
    });

    updateSpy.mockRestore();
  });
});

/**
 * Canvas.tsx window-level Escape safety-net regression tests.
 *
 * The Canvas component registers a `keydown` listener on `window` that commits
 * pending text edits when Escape is pressed. This exercises that exact code path
 * (commitPendingTextEdit + setTextEditObjectId(null) + optional delete) without
 * rendering the full Canvas component — instead we replicate the safety-net logic
 * directly, since the handler reads from the same stores and session module.
 */
describe('Canvas Escape safety-net (window keydown)', () => {
  beforeEach(() => {
    useProjectStore.setState(initialProjectState, true);
    useUiStore.setState(initialUiState, true);
    clearPendingEdit();
  });

  afterEach(() => {
    useProjectStore.setState(initialProjectState, true);
    useUiStore.setState(initialUiState, true);
    clearPendingEdit();
  });

  /**
   * Replicate the Canvas.tsx safety-net handler (lines 567-581) exactly,
   * so the test exercises the same commit+cleanup path that fires when
   * the textarea doesn't catch the Escape event.
   */
  async function simulateCanvasSafetyNet() {
    const objId = useUiStore.getState().textEditObjectId;
    if (!objId) return;
    const mode = useUiStore.getState().textEditMode;
    const content = getPendingContent();
    const shouldDelete = mode === 'new' && (content == null || content.trim() === '');
    const committed = await commitPendingTextEdit();
    if (!committed) return;
    useUiStore.setState({
      textEditObjectId: null,
      textEditClickPos: null,
      textEditMode: null,
      textEditCaretIndex: null,
    });
    if (shouldDelete) {
      void useProjectStore.getState().removeObject(objId);
    }
  }

  it('commits changed content through the window-level Escape path', async () => {
    const obj = makeTextObject('obj-1', 'Hello');
    useProjectStore.setState({ project: makeProject([obj]) as any });
    useUiStore.setState({ textEditObjectId: 'obj-1', textEditMode: 'double-click' });
    // Set up pending edit session (normally done by TextEditOverlay mount)
    setPendingEdit('obj-1', 'Hello');
    updatePendingContent('Changed');

    const updateSpy = vi.spyOn(useProjectStore.getState(), 'updateObjectData')
      .mockImplementation(() => Promise.resolve(true));

    await simulateCanvasSafetyNet();

    expect(useUiStore.getState().textEditObjectId).toBeNull();
    expect(updateSpy).toHaveBeenCalledWith(
      'obj-1',
      expect.objectContaining({ content: 'Changed' }),
    );
    updateSpy.mockRestore();
  });

  it('deletes brand-new empty text through the window-level Escape path', async () => {
    const obj = makeTextObject('obj-1', '');
    useProjectStore.setState({ project: makeProject([obj]) as any });
    useUiStore.setState({ textEditObjectId: 'obj-1', textEditMode: 'new' });
    setPendingEdit('obj-1', '');
    // Content stays empty — no updatePendingContent call

    const removeSpy = vi.spyOn(useProjectStore.getState(), 'removeObject')
      .mockImplementation(() => Promise.resolve());

    await simulateCanvasSafetyNet();

    expect(useUiStore.getState().textEditObjectId).toBeNull();
    expect(removeSpy).toHaveBeenCalledWith('obj-1');
    removeSpy.mockRestore();
  });

  it('does NOT delete existing text through the window-level Escape path', async () => {
    const obj = makeTextObject('obj-1', 'Hello');
    useProjectStore.setState({ project: makeProject([obj]) as any });
    useUiStore.setState({ textEditObjectId: 'obj-1', textEditMode: 'double-click' });
    setPendingEdit('obj-1', 'Hello');
    // No content change — commit is a no-op for unchanged content

    const removeSpy = vi.spyOn(useProjectStore.getState(), 'removeObject')
      .mockImplementation(() => Promise.resolve());

    await simulateCanvasSafetyNet();

    expect(useUiStore.getState().textEditObjectId).toBeNull();
    expect(removeSpy).not.toHaveBeenCalled();
    removeSpy.mockRestore();
  });

  it('keeps the window-level session open when commit fails', async () => {
    const obj = makeTextObject('obj-1', 'Hello');
    useProjectStore.setState({ project: makeProject([obj]) as any });
    useUiStore.setState({ textEditObjectId: 'obj-1', textEditMode: 'double-click' });
    setPendingEdit('obj-1', 'Hello');
    updatePendingContent('Changed');

    const updateSpy = vi.spyOn(useProjectStore.getState(), 'updateObjectData')
      .mockImplementation(() => Promise.resolve(false));

    await simulateCanvasSafetyNet();

    expect(useUiStore.getState().textEditObjectId).toBe('obj-1');
    updateSpy.mockRestore();
  });
});
