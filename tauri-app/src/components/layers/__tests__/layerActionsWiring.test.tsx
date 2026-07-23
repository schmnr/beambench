import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent } from '@testing-library/react';
import { LayerList } from '../LayerList';
import { useProjectStore } from '../../../stores/projectStore';
import { useUiStore } from '../../../stores/uiStore';
import { makeLayer, makeProject, makeProjectObject } from '../../../test-utils/projectFixtures';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const initialProject = useProjectStore.getState();
const initialUi = useUiStore.getState();
afterEach(() => {
  cleanup();
  useProjectStore.setState(initialProject, true);
  useUiStore.setState(initialUi, true);
});

const setup = () => {
  const layer = makeLayer({ id: 'l1', name: 'Cut', operation: 'line', color_tag: '#FF0000' });
  const obj = makeProjectObject({
    id: 'o1', name: 'R', layer_id: 'l1',
    data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 },
  });
  return { layer, obj };
};

describe('layer card action buttons', () => {
  it('select-all selects the layer objects', () => {
    const { layer, obj } = setup();
    const selectObjects = vi.fn();
    useProjectStore.setState({ project: makeProject({ layers: [layer], objects: [obj] }), selectObjects });
    render(<LayerList />);
    fireEvent.click(screen.getByTestId('select-all-on-layer'));
    expect(selectObjects).toHaveBeenCalledWith(['o1']);
  });

  it('lock button locks unlocked layer objects', () => {
    const { layer, obj } = setup();
    const lockObjects = vi.fn();
    useProjectStore.setState({ project: makeProject({ layers: [layer], objects: [obj] }), lockObjects });
    render(<LayerList />);
    fireEvent.click(screen.getByTestId('lock-layer'));
    expect(lockObjects).toHaveBeenCalledWith(['o1']);
  });

  it('select-all and lock are disabled on an empty layer', () => {
    const { layer } = setup();
    useProjectStore.setState({ project: makeProject({ layers: [layer], objects: [] }) });
    render(<LayerList />);
    expect((screen.getByTestId('select-all-on-layer') as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByTestId('lock-layer') as HTMLButtonElement).disabled).toBe(true);
  });

  it('copy stores settings and paste applies them', () => {
    const { layer, obj } = setup();
    const copyLayerSettings = vi.fn();
    const pasteLayerSettings = vi.fn();
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [obj] }),
      copyLayerSettings,
      pasteLayerSettings,
    });
    useUiStore.setState({ layerSettingsClipboard: [{ operation: 'line' }] as never });
    render(<LayerList />);
    fireEvent.click(screen.getByTestId('copy-layer-settings'));
    expect(copyLayerSettings).toHaveBeenCalledWith('l1');
    fireEvent.click(screen.getByTestId('paste-layer-settings'));
    expect(pasteLayerSettings).toHaveBeenCalledWith('l1');
  });

  it('delete layer calls removeLayer', () => {
    const { layer, obj } = setup();
    const removeLayer = vi.fn();
    useProjectStore.setState({ project: makeProject({ layers: [layer], objects: [obj] }), removeLayer });
    render(<LayerList />);
    fireEvent.click(screen.getByTestId('delete-layer'));
    expect(removeLayer).toHaveBeenCalledWith('l1');
  });
});
