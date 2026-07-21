import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  clearClipboard,
  clipboardCopy,
  clipboardCut,
  getClipboard,
  clipboardPaste,
  clipboardPasteInPlace,
} from '../clipboard';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { projectService } from '../../services/projectService';
import { makeLayer, makeProject, makeProjectObject } from '../../test-utils/projectFixtures';

const initialProjectState = useProjectStore.getState();
const initialUiState = useUiStore.getState();

describe('clipboardCut', () => {
  beforeEach(() => {
    clearClipboard();
    useUiStore.setState({ hasClipboard: false });
  });

  afterEach(() => {
    vi.restoreAllMocks();
    clearClipboard();
    useProjectStore.setState(initialProjectState, true);
    useUiStore.setState(initialUiState, true);
  });

  it('does not publish clipboard state when removeObjects fails', async () => {
    const object = makeProjectObject({ id: 'obj-1' });
    const removeObjects = vi.fn().mockResolvedValue(false);
    useProjectStore.setState({ project: makeProject({ objects: [object] }), removeObjects } as never);

    await clipboardCut(['obj-1']);

    expect(removeObjects).toHaveBeenCalledWith(['obj-1']);
    expect(getClipboard()).toBeNull();
    expect(useUiStore.getState().hasClipboard).toBe(false);
  });

  it('publishes clipboard state after removeObjects succeeds', async () => {
    const objectA = makeProjectObject({ id: 'obj-1' });
    const objectB = makeProjectObject({ id: 'obj-2' });
    const removeObjects = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({
      project: makeProject({ objects: [objectA, objectB] }),
      removeObjects,
    } as never);

    await clipboardCut(['obj-1', 'obj-2']);

    expect(removeObjects).toHaveBeenCalledWith(['obj-1', 'obj-2']);
    expect(getClipboard()?.map((object) => object.id)).toEqual(['obj-1', 'obj-2']);
    expect(useUiStore.getState().hasClipboard).toBe(true);
  });

  it('clipboardCopy still updates clipboard state immediately', () => {
    const object = makeProjectObject({ id: 'obj-1' });
    useProjectStore.setState({ project: makeProject({ objects: [object] }) });

    clipboardCopy(['obj-1']);

    expect(getClipboard()?.map((stored) => stored.id)).toEqual(['obj-1']);
    expect(useUiStore.getState().hasClipboard).toBe(true);
  });

  it('clipboardCopy includes grouped children with the group snapshot', () => {
    const child = makeProjectObject({ id: 'child-1' });
    const group = makeProjectObject({
      id: 'group-1',
      data: { type: 'group', children: ['child-1'] },
    });
    useProjectStore.setState({ project: makeProject({ objects: [child, group] }) });

    clipboardCopy(['group-1']);

    expect(getClipboard()?.map((stored) => stored.id)).toEqual(['group-1', 'child-1']);
    expect(useUiStore.getState().hasClipboard).toBe(true);
  });

  it('clipboardCut removes grouped children with the group', async () => {
    const child = makeProjectObject({ id: 'child-1' });
    const group = makeProjectObject({
      id: 'group-1',
      data: { type: 'group', children: ['child-1'] },
    });
    const removeObjects = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({
      project: makeProject({ objects: [child, group] }),
      removeObjects,
    } as never);

    await clipboardCut(['group-1']);

    expect(removeObjects).toHaveBeenCalledWith(['group-1', 'child-1']);
    expect(getClipboard()?.map((stored) => stored.id)).toEqual(['group-1', 'child-1']);
  });

  it('pastes object snapshots instead of requiring live source ids', async () => {
    const object = makeProjectObject({ id: 'obj-1' });
    const pasteObjects = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project: makeProject({ objects: [object] }),
      pasteObjects,
    } as never);
    clipboardCopy(['obj-1']);
    useProjectStore.setState({
      project: makeProject({ objects: [] }),
      pasteObjects,
    } as never);

    await clipboardPaste();

    expect(pasteObjects).toHaveBeenCalledWith(
      [expect.objectContaining({ id: 'obj-1' })],
      false,
    );
  });

  it('recreates a missing clipboard source layer before pasting', async () => {
    const sourceLayer = makeLayer({
      id: 'source-layer',
      name: 'Red Layer',
      color_tag: '#FF0000',
      speed_mm_min: 1200,
      power_percent: 35,
    });
    const object = makeProjectObject({ id: 'obj-1', layer_id: 'source-layer' });
    const pasteObjects = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project: makeProject({ layers: [sourceLayer], objects: [object] }),
      pasteObjects,
    } as never);
    clipboardCopy(['obj-1']);

    const createdLayer = makeLayer({
      id: 'recreated-layer',
      name: 'Red Layer',
      color_tag: '#000000',
    });
    const updatedLayer = makeLayer({
      ...sourceLayer,
      id: 'recreated-layer',
    });
    const addLayer = vi.spyOn(projectService, 'addLayer').mockResolvedValue(createdLayer);
    const updateLayer = vi.spyOn(projectService, 'updateLayer').mockResolvedValue(updatedLayer);
    const pasteLayerEntries = vi.spyOn(projectService, 'pasteLayerEntries').mockResolvedValue(updatedLayer);

    useProjectStore.setState({
      project: makeProject({ layers: [], objects: [] }),
      pasteObjects,
    } as never);

    await clipboardPaste();

    expect(addLayer).toHaveBeenCalledWith('Red Layer', 'line');
    expect(updateLayer).toHaveBeenCalledWith('recreated-layer', {
      name: 'Red Layer',
      enabled: true,
      visible: true,
      color_tag: '#FF0000',
    });
    expect(pasteLayerEntries).toHaveBeenCalledWith('recreated-layer', [
      expect.objectContaining({
        operation: 'line',
        speed_mm_min: 1200,
        power_percent: 35,
      }),
    ]);
    expect(pasteObjects).toHaveBeenCalledWith(
      [expect.objectContaining({ id: 'obj-1', layer_id: 'recreated-layer' })],
      false,
    );
  });

  it('pastes in place from object snapshots', async () => {
    const object = makeProjectObject({ id: 'obj-1' });
    const pasteObjects = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project: makeProject({ objects: [object] }),
      pasteObjects,
    } as never);
    clipboardCopy(['obj-1']);

    await clipboardPasteInPlace();

    expect(pasteObjects).toHaveBeenCalledWith(
      [expect.objectContaining({ id: 'obj-1' })],
      true,
    );
  });
});
