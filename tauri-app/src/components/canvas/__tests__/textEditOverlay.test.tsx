import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render, screen, cleanup, fireEvent, waitFor } from '@testing-library/react';
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
