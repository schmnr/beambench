import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent } from '@testing-library/react';
import { ModifiersToolbar } from '../ModifiersToolbar';
import { useProjectStore } from '../../../stores/projectStore';
import { useUiStore } from '../../../stores/uiStore';
import { makeLayer, makeProject as makeProjectFixture, makeProjectObject } from '../../../test-utils/projectFixtures';

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
  useProjectStore.setState(initialState, true);
  useUiStore.setState(initialUiState, true);
});

describe('ModifiersToolbar', () => {
  it('renders all modifier buttons', () => {
    render(<ModifiersToolbar />);
    expect(screen.getByTitle('Offset')).toBeDefined();
    // Boolean ops are now in a submenu — the button shows the last-used op (default: Union)
    expect(screen.getByTitle('Union')).toBeDefined();
    expect(screen.getByTitle('Weld')).toBeDefined();
    expect(screen.getByTitle('Grid Array')).toBeDefined();
    expect(screen.getByTitle('Circular Array')).toBeDefined();
    expect(screen.getByTitle('Set Start Point')).toBeDefined();
    expect(screen.getByTitle('Radius Tool')).toBeDefined();
  });

  it('Boolean submenu button disabled when selection < 2', () => {
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['a'] });
    render(<ModifiersToolbar />);
    // The submenu button shows 'Union' (default last-used op) — it should be disabled
    expect(screen.getByTitle('Union').closest('button')?.disabled).toBe(true);
  });

  it('Boolean submenu button enabled when exactly 2 selected', () => {
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['a', 'b'] });
    render(<ModifiersToolbar />);
    expect(screen.getByTitle('Union').closest('button')?.disabled).toBe(false);
  });

  it('Click boolean submenu executes last-used op (union) with correct IDs', () => {
    const booleanUnion = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['a', 'b'], booleanUnion });
    render(<ModifiersToolbar />);
    // pointerDown + pointerUp = short click = execute last-used op
    const btn = screen.getByTitle('Union');
    fireEvent.pointerDown(btn);
    fireEvent.pointerUp(btn);
    expect(booleanUnion).toHaveBeenCalledWith('a', 'b');
  });

  it('Click boolean submenu executes last-used exclude op with correct IDs', () => {
    const booleanExclude = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['a', 'b'], booleanExclude });
    useUiStore.setState({ lastBooleanOp: 'exclude' });
    render(<ModifiersToolbar />);

    const btn = screen.getByTitle('Exclude');
    fireEvent.pointerDown(btn);
    fireEvent.pointerUp(btn);

    expect(booleanExclude).toHaveBeenCalledWith('a', 'b');
  });

  it('all buttons disabled when selected objects are locked', () => {
    useProjectStore.setState({ project: makeProject(true), selectedObjectIds: ['a', 'b'] });
    render(<ModifiersToolbar />);
    expect(screen.getByTitle('Offset').closest('button')?.disabled).toBe(true);
    // Boolean submenu button
    expect(screen.getByTitle('Union').closest('button')?.disabled).toBe(true);
    expect(screen.getByTitle('Weld').closest('button')?.disabled).toBe(true);
    expect(screen.getByTitle('Grid Array').closest('button')?.disabled).toBe(true);
    expect(screen.getByTitle('Circular Array').closest('button')?.disabled).toBe(true);
  });

  it('Boolean op not called when objects are locked and button clicked', () => {
    const booleanUnion = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeProject(true), selectedObjectIds: ['a', 'b'], booleanUnion });
    render(<ModifiersToolbar />);
    const btn = screen.getByTitle('Union');
    fireEvent.pointerDown(btn);
    fireEvent.pointerUp(btn);
    expect(booleanUnion).not.toHaveBeenCalled();
  });

  it('Grid Array button opens dialog instead of executing directly', () => {
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['a', 'b'] });
    render(<ModifiersToolbar />);
    fireEvent.click(screen.getByTitle('Grid Array'));
    // Dialog should render with its title
    expect(screen.getByText('Grid Array')).toBeDefined();
    expect(screen.getByText('Apply')).toBeDefined();
  });

  it('Circular Array button opens dialog instead of executing directly', () => {
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['a', 'b'] });
    render(<ModifiersToolbar />);
    fireEvent.click(screen.getByTitle('Circular Array'));
    expect(screen.getByText('Circular Array')).toBeDefined();
    expect(screen.getByText('Apply')).toBeDefined();
  });

  it('Grid Array button does not open dialog when locked', () => {
    const gridArray = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeProject(true), selectedObjectIds: ['a', 'b'], gridArray });
    render(<ModifiersToolbar />);
    fireEvent.click(screen.getByTitle('Grid Array'));
    // Dialog should NOT render — button is disabled
    expect(screen.queryByText('Apply')).toBeNull();
    expect(gridArray).not.toHaveBeenCalled();
  });
});
