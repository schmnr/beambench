import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent, waitFor } from '@testing-library/react';
import { MainToolbar } from '../MainToolbar';
import { CreationToolbar } from '../CreationToolbar';
import { NodeSubToolbar } from '../NodeSubToolbar';
import { useUndoStore } from '../../../stores/undoStore';
import { useProjectStore } from '../../../stores/projectStore';
import { useUiStore } from '../../../stores/uiStore';
import { useNotificationStore } from '../../../stores/notificationStore';
import { useMacroStore } from '../../../stores/macroStore';
import { projectService } from '../../../services/projectService';
import { makeLayer, makeProject, makeProjectObject, makeTransformLocks } from '../../../test-utils/projectFixtures';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const initialUndoState = useUndoStore.getState();
const initialProjectState = useProjectStore.getState();
const initialUiState = useUiStore.getState();
const initialNotificationState = useNotificationStore.getState();
const initialMacroState = useMacroStore.getState();

afterEach(() => {
  cleanup();
  useUndoStore.setState(initialUndoState, true);
  useProjectStore.setState(initialProjectState, true);
  useUiStore.setState(initialUiState, true);
  useNotificationStore.setState(initialNotificationState, true);
  useMacroStore.setState(initialMacroState, true);
});

function showArrangeLongToolbar() {
  useUiStore.setState({
    panelLayout: {
      ...useUiStore.getState().panelLayout,
      toolbarVisibility: {
        ...useUiStore.getState().panelLayout.toolbarVisibility,
        arrangeLong: true,
      },
    },
  });
}


// The arrange cluster lives in a popover; open it (requires a selection).
const openArrange = () => {
  fireEvent.click(screen.getByTitle('Arrange'));
};

describe('MainToolbar', () => {
  it('renders file operation buttons', () => {
    render(<MainToolbar />);
    expect(screen.getByTitle('New')).toBeDefined();
    expect(screen.getByTitle('Open')).toBeDefined();
    expect(screen.getByTitle('Save')).toBeDefined();
    expect(screen.getByTitle('Undo')).toBeDefined();
    expect(screen.getByTitle('Redo')).toBeDefined();
  });

  it('Import button uses the selected project layer', () => {
    const project = makeProject();
    const importFiles = vi.fn();
    useProjectStore.setState({
      project,
      selectedLayerId: project.layers[0].id,
      importFiles,
    });

    render(<MainToolbar />);
    fireEvent.click(screen.getByTitle('Import'));

    expect(importFiles).toHaveBeenCalledWith(project.layers[0].id);
  });

  it('zoom fit options live in the fit dropdown', () => {
    useProjectStore.setState({ project: makeProject() });

    render(<MainToolbar />);
    fireEvent.click(screen.getByTitle('Zoom to fit'));

    expect(screen.getByText('Fit Page')).toBeTruthy();
    expect(screen.getByText('Fit Selection')).toBeTruthy();
  });

  it('Undo disabled when canUndo false', () => {
    useUndoStore.setState({ canUndo: false });
    render(<MainToolbar />);
    const undoBtn = screen.getByTitle('Undo');
    expect(undoBtn.closest('button')?.disabled).toBe(true);
  });

  it('renders arrange buttons', () => {
    render(<MainToolbar />);
    openArrange();
    expect(screen.getByTitle('Group')).toBeDefined();
    expect(screen.getByTitle('Ungroup')).toBeDefined();
    expect(screen.getByTitle('Flip Horizontal')).toBeDefined();
    expect(screen.getByTitle('Mirror Across Line')).toBeDefined();
    expect(screen.getByTitle('Align Left')).toBeDefined();
    expect(screen.getByTitle('Dock')).toBeDefined();
    expect(screen.queryByTitle('Make Same Width')).toBeNull();
  });

  it('renders Arrange Long buttons when that toolbar is visible', () => {
    showArrangeLongToolbar();

    render(<MainToolbar />);
    openArrange();
    expect(screen.getByTitle('Make Same Width')).toBeDefined();
    expect(screen.getByTitle('Make Same Height')).toBeDefined();
    expect(screen.getByTitle('Move H Together')).toBeDefined();
    expect(screen.getByTitle('Move V Together')).toBeDefined();
    expect(screen.queryByTitle('Resize Slots')).toBeNull();
  });

  it('Mirror Across Line is disabled when normalized arrangement selection has fewer than two objects', () => {
    useProjectStore.setState({
      selectedObjectIds: ['guide', 'axis'],
      project: makeProject({
        layers: [
          { id: 'tool', name: 'Tool', entries: [], enabled: true, order_index: 0, color_tag: '#f60', visible: true, is_tool_layer: true },
          { id: 'normal', name: 'Normal', entries: [], enabled: true, order_index: 1, color_tag: '#000', visible: true, is_tool_layer: false },
        ],
        objects: [
          makeProjectObject({
            id: 'guide',
            layer_id: 'tool',
            data: { type: 'vector_path', path_data: 'M 0 0 L 0 10', closed: false, ruler_guide_axis: 'vertical' },
          }),
          makeProjectObject({
            id: 'axis',
            layer_id: 'normal',
            data: { type: 'vector_path', path_data: 'M 0 0 L 10 0', closed: false, ruler_guide_axis: null },
          }),
        ],
      }),
    });

    render(<MainToolbar />);
    openArrange();
    expect(screen.getByTitle('Mirror Across Line').closest('button')?.disabled).toBe(true);
  });

  it('Mirror Across Line is enabled when a tool-layer line is selected as the axis', () => {
    useProjectStore.setState({
      selectedObjectIds: ['shape', 'tool-axis'],
      project: makeProject({
        layers: [
          makeLayer({ id: 'tool', is_tool_layer: true }),
          makeLayer({ id: 'normal' }),
        ],
        objects: [
          makeProjectObject({ id: 'shape', layer_id: 'normal' }),
          makeProjectObject({
            id: 'tool-axis',
            layer_id: 'tool',
            data: { type: 'vector_path', path_data: 'M 0 0 L 10 0', closed: false, ruler_guide_axis: null },
          }),
        ],
      }),
    });

    render(<MainToolbar />);
    openArrange();
    expect(screen.getByTitle('Mirror Across Line').closest('button')?.disabled).toBe(false);
  });

  it('Group disabled when selection < 2', () => {
    useProjectStore.setState({ selectedObjectIds: ['a'] });
    render(<MainToolbar />);
    openArrange();
    const groupBtn = screen.getByTitle('Group');
    expect(groupBtn.closest('button')?.disabled).toBe(true);
  });

  it('align is blocked when position transform is locked', () => {
    useProjectStore.setState({
      selectedObjectIds: ['a', 'b'],
      // full Project via makeProject; transform_locks via makeTransformLocks.
      project: makeProject({
        transform_locks: makeTransformLocks({ move_enabled: false }),
        workspace: { bed_width_mm: 400, bed_height_mm: 400, origin: 'top_left' as const },
        objects: [],
        layers: [],
      }),
    });

    const pushSpy = vi.fn();
    useNotificationStore.setState({ push: pushSpy });

    render(<MainToolbar />);
    openArrange();
    const alignBtn = screen.getByTitle('Align Left');
    fireEvent.click(alignBtn);

    expect(pushSpy).toHaveBeenCalledWith('Position is locked for this project', 'warning');
  });

  it('flip is blocked when position transform is locked', () => {
    useProjectStore.setState({
      selectedObjectIds: ['a'],
      // full Project via makeProject; transform_locks via makeTransformLocks.
      project: makeProject({
        transform_locks: makeTransformLocks({ move_enabled: false }),
        workspace: { bed_width_mm: 400, bed_height_mm: 400, origin: 'top_left' as const },
        objects: [],
        layers: [],
      }),
    });

    const pushSpy = vi.fn();
    useNotificationStore.setState({ push: pushSpy });
    const flipSpy = vi.fn();
    useProjectStore.setState({ flipObjects: flipSpy });

    render(<MainToolbar />);
    openArrange();
    const flipBtn = screen.getByTitle('Flip Horizontal');
    fireEvent.click(flipBtn);

    expect(pushSpy).toHaveBeenCalledWith('Position is locked for this project', 'warning');
    expect(flipSpy).not.toHaveBeenCalled();
  });

  it('flip buttons disabled when selected object is locked', () => {
    useProjectStore.setState({
      selectedObjectIds: ['a'],
      project: makeProject({
        objects: [makeProjectObject({ id: 'a', locked: true })],
        layers: [],
      }),
    });
    render(<MainToolbar />);
    openArrange();
    expect(screen.getByTitle('Flip Horizontal').closest('button')?.disabled).toBe(true);
    expect(screen.getByTitle('Flip Vertical').closest('button')?.disabled).toBe(true);
  });

  it('flip button is disabled and flipObjects not called when object locked', () => {
    const flipSpy = vi.fn();
    useProjectStore.setState({
      selectedObjectIds: ['a'],
      flipObjects: flipSpy,
      project: makeProject({
        objects: [makeProjectObject({ id: 'a', locked: true })],
        layers: [],
      }),
    });
    render(<MainToolbar />);
    openArrange();
    const btn = screen.getByTitle('Flip Horizontal').closest('button')!;
    expect(btn.disabled).toBe(true);
    fireEvent.click(btn);
    expect(flipSpy).not.toHaveBeenCalled();
  });

  it('center on page disabled when selected object is locked', () => {
    showArrangeLongToolbar();
    useProjectStore.setState({
      selectedObjectIds: ['a'],
      project: makeProject({
        objects: [makeProjectObject({ id: 'a', locked: true })],
        layers: [],
      }),
    });
    render(<MainToolbar />);
    openArrange();
    expect(screen.getByTitle('Center on Page').closest('button')?.disabled).toBe(true);
  });

  it('align buttons disabled when selected objects include locked ones', () => {
    showArrangeLongToolbar();
    useProjectStore.setState({
      selectedObjectIds: ['a', 'b'],
      project: makeProject({
        objects: [makeProjectObject({ id: 'a', locked: true }), makeProjectObject({ id: 'b', locked: false })],
        layers: [],
      }),
    });
    render(<MainToolbar />);
    openArrange();
    expect(screen.getByTitle('Align Left').closest('button')?.disabled).toBe(true);
    expect(screen.getByTitle('Distribute H-Centered').closest('button')?.disabled).toBe(true);
  });

  it('loads toolbar macros on mount and gives each visible macro a numbered identity', async () => {
    const loadMacros = vi.fn().mockResolvedValue(undefined);
    const runMacro = vi.fn();

    useMacroStore.setState({
      macros: [
        { id: 'macro-1', name: 'Home All', description: 'Homes the machine', commands: ['G28'], show_in_toolbar: true },
        { id: 'macro-hidden', name: 'Hidden', description: '', commands: ['M5'], show_in_toolbar: false },
        { id: 'macro-2', name: 'Air Assist', description: '', commands: ['M8'], show_in_toolbar: true },
      ],
      loadMacros,
      runMacro,
    });

    render(<MainToolbar />);

    await waitFor(() => {
      expect(loadMacros).toHaveBeenCalledOnce();
      expect(screen.getByTitle('1. Home All').textContent).toBe('1');
      expect(screen.getByTitle('2. Air Assist').textContent).toBe('2');
    });

    expect(screen.queryByTitle(/Hidden/)).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '2. Air Assist' }));
    expect(runMacro).toHaveBeenCalledWith('macro-2');
  });

  it('align surfaces backend failures instead of rejecting from the toolbar', async () => {
    const pushSpy = vi.fn();
    useNotificationStore.setState({ push: pushSpy });
    useProjectStore.setState({
      selectedObjectIds: ['a', 'b'],
      project: makeProject({
        objects: [makeProjectObject({ id: 'a', locked: false }), makeProjectObject({ id: 'b', locked: false })],
        layers: [],
      }),
    });
    vi.spyOn(projectService, 'alignObjects').mockRejectedValue(new Error('align failed'));

    render(<MainToolbar />);
    openArrange();
    fireEvent.click(screen.getByTitle('Align Left'));

    await waitFor(() => {
      expect(pushSpy).toHaveBeenCalledWith(expect.stringContaining('align failed'), 'error');
    });
  });

  it('distribute surfaces backend failures instead of rejecting from the toolbar', async () => {
    showArrangeLongToolbar();
    const pushSpy = vi.fn();
    useNotificationStore.setState({ push: pushSpy });
    useProjectStore.setState({
      selectedObjectIds: ['a', 'b', 'c'],
      project: makeProject({
        objects: [makeProjectObject({ id: 'a', locked: false }), makeProjectObject({ id: 'b', locked: false }), makeProjectObject({ id: 'c', locked: false })],
        layers: [],
      }),
    });
    vi.spyOn(projectService, 'distributeObjects').mockRejectedValue(new Error('distribute failed'));

    render(<MainToolbar />);
    openArrange();
    fireEvent.click(screen.getByTitle('Distribute H-Centered'));

    await waitFor(() => {
      expect(pushSpy).toHaveBeenCalledWith(expect.stringContaining('distribute failed'), 'error');
    });
  });
});

describe('NodeSubToolbar', () => {
  it('hides deferred node trim and extend buttons while keeping adjacent node actions available', () => {
    useUiStore.setState({ activeTool: 'node' });

    render(<NodeSubToolbar />);

    expect(screen.getByTitle('Insert Midpoint (M)')).toBeDefined();
    expect(screen.getByTitle('Align to Angle (A)')).toBeDefined();
    expect(screen.queryByTitle('Trim Segment to Intersection (T)')).toBeNull();
    expect(screen.queryByTitle('Trim to Intersection (T)')).toBeNull();
    expect(screen.queryByTitle('Extend to Intersection (E)')).toBeNull();
  });
});

describe('CreationToolbar', () => {
  it('renders standalone tool buttons and shapes submenu', () => {
    render(<CreationToolbar />);
    // Standalone buttons always visible
    const standaloneLabels = ['Select', 'Draw', 'Node Edit', 'Trim', 'Tabs', 'Text', 'Laser Position', 'Measure'];
    for (const label of standaloneLabels) {
      expect(screen.getByTitle(label)).toBeDefined();
    }
    // Shapes submenu shows the last-used shape (default: Rectangle)
    expect(screen.getByTitle('Rectangle')).toBeDefined();
  });

  it('highlights active tool', () => {
    useUiStore.setState({ activeTool: 'rect' });
    render(<CreationToolbar />);
    // The shapes submenu button should be highlighted when a shape tool is active
    const rectBtn = screen.getByTitle('Rectangle');
    expect(rectBtn.className).toContain('bg-bb-accent/15');
    const selectBtn = screen.getByTitle('Select');
    expect(selectBtn.className).not.toContain('bg-bb-accent/15');
  });
});
