import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { useProjectStore } from '../../../stores/projectStore';
import { useUiStore } from '../../../stores/uiStore';
import { makeLayer, makeProject, makeProjectObject } from '../../../test-utils/projectFixtures';
import {
  OutlinerPanel,
  resolveLayerDropIndex,
  resolveObjectDropBeforeId,
  topLevelObjectsForLayer,
} from '../OutlinerPanel';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));

const initialProjectState = useProjectStore.getState();
const initialUiState = useUiStore.getState();

afterEach(() => {
  cleanup();
  useProjectStore.setState(initialProjectState, true);
  useUiStore.setState(initialUiState, true);
});

function makeOutlinerProject() {
  const black = makeLayer({ id: 'black', name: 'Black Line', color_tag: '#000000', order_index: 0 });
  const red = makeLayer({ id: 'red', name: 'Red Fill', color_tag: '#ff0000', order_index: 1, operation: 'fill' });
  const child = makeProjectObject({ id: 'child', name: 'Grouped Circle', layer_id: 'black', z_index: 1 });
  const group = makeProjectObject({
    id: 'group',
    name: 'Logo Group',
    layer_id: 'black',
    z_index: 2,
    data: { type: 'group', children: ['child'] },
  });
  const loose = makeProjectObject({ id: 'loose', name: 'Loose Rectangle', layer_id: 'black', z_index: 0 });
  const target = makeProjectObject({ id: 'target', name: 'Red Circle', layer_id: 'red', z_index: 0 });
  return makeProject({ layers: [black, red], objects: [child, group, loose, target] });
}

describe('OutlinerPanel', () => {
  it('shows layers, top-level objects, and expandable group contents', () => {
    useProjectStore.setState({ project: makeOutlinerProject(), selectedLayerId: 'black', selectedObjectIds: [] });

    render(<OutlinerPanel />);

    expect(screen.getByText('2 layers · 4 objects')).toBeDefined();
    expect(screen.getByText('Black Line')).toBeDefined();
    expect(screen.getByText('Logo Group')).toBeDefined();
    expect(screen.queryByText('Grouped Circle')).toBeNull();

    fireEvent.click(screen.getByLabelText('Expand group'));
    expect(screen.getByText('Grouped Circle')).toBeDefined();
  });

  it('keeps canvas selection synchronized from object rows', () => {
    const selectObjects = vi.fn();
    const selectLayer = vi.fn();
    useProjectStore.setState({
      project: makeOutlinerProject(),
      selectedLayerId: 'black',
      selectedObjectIds: [],
      selectObjects,
      selectLayer,
    });

    render(<OutlinerPanel />);
    fireEvent.click(screen.getByTestId('outliner-object-loose'));

    expect(selectLayer).toHaveBeenCalledWith('black');
    expect(selectObjects).toHaveBeenCalledWith(['loose']);
  });

  it('drops an object onto another layer in one outliner move', async () => {
    const moveObjectsInOutliner = vi.fn().mockResolvedValue(true);
    useUiStore.setState({ workspaceMode: 'design' });
    useProjectStore.setState({
      project: makeOutlinerProject(),
      selectedLayerId: 'black',
      selectedObjectIds: [],
      moveObjectsInOutliner,
    });
    render(<OutlinerPanel />);
    const dataTransfer = { effectAllowed: '', dropEffect: '', setData: vi.fn() };

    await act(async () => {
      fireEvent.dragStart(screen.getByTestId('outliner-object-loose'), { dataTransfer });
    });
    expect(dataTransfer.setData).toHaveBeenCalled();
    await act(async () => {
      fireEvent.dragOver(screen.getByTestId('outliner-layer-red'), { dataTransfer });
    });
    await act(async () => {
      fireEvent.drop(screen.getByTestId('outliner-layer-red'), { dataTransfer });
    });

    expect(moveObjectsInOutliner).toHaveBeenCalledWith(['loose'], 'red', 'target');
  });

  it('reorders processing layers by dragging a layer row', async () => {
    const reorderLayer = vi.fn().mockResolvedValue(undefined);
    useUiStore.setState({ workspaceMode: 'design' });
    useProjectStore.setState({
      project: makeOutlinerProject(),
      selectedLayerId: 'black',
      selectedObjectIds: [],
      reorderLayer,
    });
    render(<OutlinerPanel />);
    const dataTransfer = { effectAllowed: '', dropEffect: '', setData: vi.fn() };
    const target = screen.getByTestId('outliner-layer-red');

    await act(async () => {
      fireEvent.dragStart(screen.getByTestId('outliner-layer-black'), { dataTransfer });
    });
    await act(async () => {
      fireEvent.dragOver(target, { dataTransfer });
      fireEvent.drop(target, { dataTransfer });
    });

    expect(reorderLayer).toHaveBeenCalledWith('black', 1);
  });

  it('derives stable layer and object drop positions', () => {
    const project = makeOutlinerProject();
    expect(resolveLayerDropIndex(project.layers, 'black', 'red', 'after')).toBe(1);
    expect(resolveLayerDropIndex(project.layers, 'red', 'black', 'before')).toBe(0);
    expect(resolveObjectDropBeforeId(project, ['loose'], 'group', 'after')).toBeNull();
    expect(topLevelObjectsForLayer(project, 'black').map((object) => object.id)).toEqual(['group', 'loose']);
  });
});
