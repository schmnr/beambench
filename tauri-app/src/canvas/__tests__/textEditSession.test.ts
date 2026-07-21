import { describe, expect, it, vi, beforeEach } from 'vitest';
import {
  setPendingEdit, updatePendingContent, clearPendingEdit,
  commitPendingTextEdit, discardPendingTextEdit, getPendingContent,
} from '../textEditSession';
import { useProjectStore } from '../../stores/projectStore';
import { makeLayer, makeProject as makeFixtureProject, makeProjectObject, makeTextObjectData } from '../../test-utils/projectFixtures';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const initialProjectState = useProjectStore.getState();

beforeEach(() => {
  clearPendingEdit();
  useProjectStore.setState(initialProjectState, true);
});

// build via shared typed fixtures — no partial Project `as any`.
const makeProject = (content: string) => makeFixtureProject({
  layers: [makeLayer({ id: 'layer-1', color_tag: '#ff0000', speed_mm_min: 100 })],
  objects: [makeProjectObject({
    id: 'obj-1',
    name: 'Text',
    bounds: { min: { x: 10, y: 20 }, max: { x: 50, y: 30 } },
    layer_id: 'layer-1',
    data: makeTextObjectData({
      content,
      font_size_mm: 6,
    }),
  })],
});

describe('textEditSession', () => {
  it('commitPendingTextEdit is a no-op when no pending edit', async () => {
    // Should not throw
    await commitPendingTextEdit();
  });

  it('commitPendingTextEdit is a no-op when content unchanged', async () => {
    useProjectStore.setState({ project: makeProject('Hello') });
    const spy = vi.spyOn(useProjectStore.getState(), 'updateObjectData');

    setPendingEdit('obj-1', 'Hello');
    await commitPendingTextEdit();

    expect(spy).not.toHaveBeenCalled();
    spy.mockRestore();
  });

  it('commitPendingTextEdit saves changed content', async () => {
    useProjectStore.setState({ project: makeProject('Hello') });
    const spy = vi.spyOn(useProjectStore.getState(), 'updateObjectData').mockImplementation(() => Promise.resolve(true));

    setPendingEdit('obj-1', 'Hello');
    updatePendingContent('Hello World');
    await commitPendingTextEdit();

    expect(spy).toHaveBeenCalledTimes(1);
    const [objId, data] = spy.mock.calls[0];
    expect(objId).toBe('obj-1');
    // narrow the discriminated union properly instead of `as any`.
    if (data.type !== 'text') throw new Error('Expected text data');
    expect(data.content).toBe('Hello World');
    spy.mockRestore();
  });

  it('commitPendingTextEdit only fires once (clears pending on first call)', async () => {
    useProjectStore.setState({ project: makeProject('Hello') });
    const spy = vi.spyOn(useProjectStore.getState(), 'updateObjectData').mockImplementation(() => Promise.resolve(true));

    setPendingEdit('obj-1', 'Hello');
    updatePendingContent('Changed');
    await commitPendingTextEdit();
    await commitPendingTextEdit(); // second call should be no-op

    expect(spy).toHaveBeenCalledTimes(1);
    spy.mockRestore();
  });

  it('discardPendingTextEdit clears pending without saving', async () => {
    useProjectStore.setState({ project: makeProject('Hello') });
    const spy = vi.spyOn(useProjectStore.getState(), 'updateObjectData').mockImplementation(() => Promise.resolve(true));

    setPendingEdit('obj-1', 'Hello');
    updatePendingContent('Changed');
    discardPendingTextEdit();
    await commitPendingTextEdit(); // should be no-op after discard

    expect(spy).not.toHaveBeenCalled();
    spy.mockRestore();
  });

  it('clearPendingEdit prevents subsequent commit', async () => {
    useProjectStore.setState({ project: makeProject('Hello') });
    const spy = vi.spyOn(useProjectStore.getState(), 'updateObjectData').mockImplementation(() => Promise.resolve(true));

    setPendingEdit('obj-1', 'Hello');
    updatePendingContent('Changed');
    clearPendingEdit();
    await commitPendingTextEdit();

    expect(spy).not.toHaveBeenCalled();
    spy.mockRestore();
  });

  it('preserves pending content when save fails', async () => {
    useProjectStore.setState({ project: makeProject('Hello') });
    const spy = vi.spyOn(useProjectStore.getState(), 'updateObjectData').mockImplementation(() => Promise.resolve(false));

    setPendingEdit('obj-1', 'Hello');
    updatePendingContent('Changed');

    await expect(commitPendingTextEdit()).resolves.toBe(false);
    expect(getPendingContent()).toBe('Changed');

    spy.mockRestore();
  });
});

describe('getPendingContent', () => {
  it('returns null when no pending edit', () => {
    expect(getPendingContent()).toBeNull();
  });

  it('returns initial content after setPendingEdit', () => {
    setPendingEdit('obj-1', 'Hello');
    expect(getPendingContent()).toBe('Hello');
  });

  it('returns updated content after updatePendingContent', () => {
    setPendingEdit('obj-1', 'Hello');
    updatePendingContent('World');
    expect(getPendingContent()).toBe('World');
  });
});
