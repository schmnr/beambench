import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent, waitFor } from '@testing-library/react';
import { ModifiersToolbar } from '../ModifiersToolbar';
import { useProjectStore } from '../../../stores/projectStore';
import { useUiStore } from '../../../stores/uiStore';
import { makeLayer, makeProject as makeProjectFixture, makeProjectObject, makeTextObjectData } from '../../../test-utils/projectFixtures';
import { clearPendingEdit, setPendingEdit, updatePendingContent } from '../../../canvas/textEditSession';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const makeProject = (locked = false) => ({
  ...makeProjectFixture({
    metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p1', project_name: 'Test', created_at: '', modified_at: '' },
    layers: [makeLayer({ id: 'l1', name: 'L1', operation: 'line', color_tag: '#ff0000' })],
    assets: [],
  }),
  objects: [
    makeProjectObject({ id: 'a', name: 'A', locked, layer_id: 'l1', data: { type: 'shape' as const, kind: 'rectangle' as const, width: 10, height: 10, corner_radius: 0 } }),
    makeProjectObject({ id: 'b', name: 'B', locked, transform: { a: 1, b: 0, c: 0, d: 1, tx: 20, ty: 0 }, bounds: { min: { x: 20, y: 0 }, max: { x: 30, y: 10 } }, layer_id: 'l1', z_index: 1, data: { type: 'shape' as const, kind: 'rectangle' as const, width: 10, height: 10, corner_radius: 0 }, created_at: '2026-01-01T00:00:01Z' }),
  ],
});

const initialState = useProjectStore.getState();
const initialUiState = useUiStore.getState();

afterEach(() => {
  cleanup();
  clearPendingEdit();
  useProjectStore.setState(initialState, true);
  useUiStore.setState(initialUiState, true);
});

describe('ModifiersToolbar', () => {
  it('renders all modifier buttons', () => {
    render(<ModifiersToolbar />);
    expect(screen.getByTitle('Offset')).toBeDefined();
    expect(screen.getByTitle('Grid Array')).toBeDefined();
    expect(screen.getByTitle('Circular Array')).toBeDefined();
    expect(screen.getByTitle('Set Start Point')).toBeDefined();
    expect(screen.getByTitle('Radius Tool')).toBeDefined();
  });

  it('does not duplicate Boolean operations from the Properties panel', () => {
    render(<ModifiersToolbar />);
    expect(screen.queryByTitle('Weld')).toBeNull();
    expect(screen.queryByTitle('Union')).toBeNull();
    expect(screen.queryByTitle('Subtract')).toBeNull();
    expect(screen.queryByTitle('Intersect')).toBeNull();
    expect(screen.queryByTitle('Exclude')).toBeNull();
  });

  it('all buttons disabled when selected objects are locked', () => {
    useProjectStore.setState({ project: makeProject(true), selectedObjectIds: ['a', 'b'] });
    render(<ModifiersToolbar />);
    expect(screen.getByTitle('Offset').closest('button')?.disabled).toBe(true);
    expect(screen.getByTitle('Grid Array').closest('button')?.disabled).toBe(true);
    expect(screen.getByTitle('Circular Array').closest('button')?.disabled).toBe(true);
  });

  it('Grid Array button opens a contextual Properties session instead of a dialog', () => {
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['a', 'b'] });
    render(<ModifiersToolbar />);
    fireEvent.click(screen.getByTitle('Grid Array'));
    expect(useUiStore.getState().modifierPropertiesSession).toEqual({
      kind: 'grid_array',
      objectIds: ['a', 'b'],
    });
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(screen.getByTitle('Grid Array').closest('button')?.className).toContain('bg-bb-accent/15');
  });

  it('Circular Array button opens a contextual Properties session instead of a dialog', () => {
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['a', 'b'] });
    render(<ModifiersToolbar />);
    fireEvent.click(screen.getByTitle('Circular Array'));
    expect(useUiStore.getState().modifierPropertiesSession).toEqual({
      kind: 'circular_array',
      objectIds: ['a', 'b'],
    });
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('opens a modifier session after an active inline text edit finishes committing', async () => {
    const project = makeProject();
    project.objects[0] = { ...project.objects[0], data: makeTextObjectData({ content: 'Hello' }) };
    let finishCommit!: (saved: boolean) => void;
    const updateObjectData = vi.fn().mockReturnValue(new Promise<boolean>((resolve) => {
      finishCommit = resolve;
    }));
    useProjectStore.setState({ project, selectedObjectIds: ['a'], updateObjectData });
    useUiStore.setState({ textEditObjectId: 'a', textEditMode: 'double-click' });
    setPendingEdit('a', 'Hello');
    updatePendingContent('Hello edited');

    render(<ModifiersToolbar />);
    fireEvent.click(screen.getByTitle('Offset'));
    expect(useUiStore.getState().modifierPropertiesSession).toBeNull();

    finishCommit(true);
    await waitFor(() => {
      expect(useUiStore.getState().modifierPropertiesSession).toEqual({
        kind: 'offset',
        objectIds: ['a'],
      });
    });
  });

  it('clicking an active modifier button closes its Properties session', () => {
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['a', 'b'] });
    useUiStore.setState({ modifierPropertiesSession: { kind: 'offset', objectIds: ['a', 'b'] } });
    render(<ModifiersToolbar />);

    fireEvent.click(screen.getByTitle('Offset'));

    expect(useUiStore.getState().modifierPropertiesSession).toBeNull();
  });

  it('Grid Array button does not open dialog when locked', () => {
    const gridArray = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeProject(true), selectedObjectIds: ['a', 'b'], gridArray });
    render(<ModifiersToolbar />);
    fireEvent.click(screen.getByTitle('Grid Array'));
    expect(useUiStore.getState().modifierPropertiesSession).toBeNull();
    expect(gridArray).not.toHaveBeenCalled();
  });
});
