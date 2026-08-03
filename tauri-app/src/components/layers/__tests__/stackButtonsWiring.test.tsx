import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent } from '@testing-library/react';
import { LayerList } from '../LayerList';
import { useProjectStore } from '../../../stores/projectStore';
import { makeLayer, makeProject } from '../../../test-utils/projectFixtures';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const initial = useProjectStore.getState();
afterEach(() => { cleanup(); useProjectStore.setState(initial, true); });

describe('pass stack buttons in the panel', () => {
  it('delete and reorder call the store actions', () => {
    const base = makeLayer({ id: 'l1', name: 'Cut', operation: 'line', color_tag: '#FF0000' });
    const layer = { ...base, entries: [base.entries[0], { ...base.entries[0], id: 'e2' }] };
    const removeCutEntry = vi.fn();
    const reorderCutEntry = vi.fn();
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
      selectedLayerId: 'l1',
      removeCutEntry,
      reorderCutEntry,
    });

    render(<LayerList />);

    fireEvent.click(screen.getByTestId('sub-layer-down-' + layer.entries[0].id));
    expect(reorderCutEntry).toHaveBeenCalledWith('l1', layer.entries[0].id, 1);

    // Switch to the second pass's tab, then delete it.
    fireEvent.click(screen.getByTestId('sub-layer-tab-1'));
    fireEvent.click(screen.getByTestId('sub-layer-delete-e2'));
    expect(removeCutEntry).toHaveBeenCalledWith('l1', 'e2');
  });
});
