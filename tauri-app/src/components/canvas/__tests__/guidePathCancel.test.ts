import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cancelPendingGuidePathSelection } from '../guidePathCancel';
import { useProjectStore } from '../../../stores/projectStore';
import { useUiStore } from '../../../stores/uiStore';
import { makeLayer, makeProject as makeFixtureProject, makeProjectObject, makeTextObjectData } from '../../../test-utils/projectFixtures';

const initialProjectState = useProjectStore.getState();
const initialUiState = useUiStore.getState();

// build via shared typed fixtures — no partial Project `as any`.
function makeProject() {
  return makeFixtureProject({
    layers: [makeLayer({ id: 'layer-1', color_tag: '#ff0000' })],
    objects: [makeProjectObject({
      id: 'text-1',
      name: 'Text',
      bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 5 } },
      layer_id: 'layer-1',
      data: makeTextObjectData({
        font_size_mm: 6,
        layout_mode: 'path',
        on_path: true,
      }),
    })],
  });
}

describe('cancelPendingGuidePathSelection', () => {
  beforeEach(() => {
    useProjectStore.setState({ project: makeProject() });
    useUiStore.setState({ pendingGuidePathTextId: 'text-1' });
  });

  afterEach(() => {
    useProjectStore.setState(initialProjectState, true);
    useUiStore.setState(initialUiState, true);
  });

  it('keeps guide-path pick mode active when the revert fails', async () => {
    vi.spyOn(useProjectStore.getState(), 'updateObjectData').mockResolvedValue(false);

    await expect(cancelPendingGuidePathSelection()).resolves.toBe(false);
    expect(useUiStore.getState().pendingGuidePathTextId).toBe('text-1');
  });

  it('clears guide-path pick mode after a successful revert', async () => {
    vi.spyOn(useProjectStore.getState(), 'updateObjectData').mockResolvedValue(true);

    await expect(cancelPendingGuidePathSelection()).resolves.toBe(true);
    expect(useUiStore.getState().pendingGuidePathTextId).toBeNull();
  });
});
