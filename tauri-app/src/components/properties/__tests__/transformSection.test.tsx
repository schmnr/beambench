import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent, act } from '@testing-library/react';
import { TransformSection } from '../TransformSection';
import { useProjectStore } from '../../../stores/projectStore';
import { useUiStore } from '../../../stores/uiStore';
import { useAppStore } from '../../../stores/appStore';
import { useNotificationStore } from '../../../stores/notificationStore';
import { makeAppSettings, makeLayer, makeProject as makeProjectFixture, makeProjectObject, makeTransformLocks } from '../../../test-utils/projectFixtures';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((cmd: string) => {
    if (cmd === 'get_system_fonts') return Promise.resolve(['Arial', 'Helvetica', 'Times New Roman']);
    return Promise.resolve(null);
  }),
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const makeProject = () => ({
  ...makeProjectFixture({
    metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p1', project_name: 'Test', created_at: '', modified_at: '' },
    layers: [makeLayer({ id: 'l1', name: 'L1', operation: 'line', color_tag: '#ff0000' })],
    assets: [],
  }),
  objects: [makeProjectObject({
    id: 'obj1', name: 'Rect1',
    bounds: { min: { x: 10, y: 20 }, max: { x: 60, y: 70 } },
    layer_id: 'l1',
    data: { type: 'shape' as const, kind: 'rectangle' as const, width: 50, height: 50, corner_radius: 0 },
  })],
});

const makeBottomLeftProject = () => {
  const project = makeProject();
  return {
    ...project,
    workspace: { ...project.workspace, bed_width_mm: 400, bed_height_mm: 300, origin: 'bottom_left' as const },
  };
};

// DOM order of spinbuttons:
// X/Y col: X(0), Y(1)  then W/H+Scale col: W(2), SX(3), H(4), SY(5)  then Rot(6)
const IDX_X = 0;
const IDX_Y = 1;
const IDX_W = 2;
const IDX_H = 3;
const IDX_SCALE_X = 4;
const IDX_SCALE_Y = 5;
const IDX_ROT = 6;

// Typed input buffers locally and commits on blur (or Enter).
const typeAndCommit = (input: Element, value: string) => {
  fireEvent.change(input, { target: { value } });
  fireEvent.blur(input);
};

const initialState = useProjectStore.getState();
const initialUiState = useUiStore.getState();
const initialAppState = useAppStore.getState();

afterEach(() => {
  cleanup();
  useProjectStore.setState(initialState, true);
  useUiStore.setState(initialUiState, true);
  useAppStore.setState(initialAppState, true);
});

describe('TransformSection — position/size', () => {
  it('renders X/Y/W/H fields when object selected', () => {
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'] });
    render(<TransformSection />);
    expect(screen.getByText('X')).toBeDefined();
    expect(screen.getByText('Y')).toBeDefined();
    expect(screen.getByText('W')).toBeDefined();
    expect(screen.getByText('H')).toBeDefined();
    expect(screen.getAllByRole('spinbutton').length).toBe(7);
  });

  it('displays correct values from selected object bounds', () => {
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'] });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    // X=10, Y=20, W=50, H=50
    expect(inputs[IDX_X]).toHaveProperty('value', '10');
    expect(inputs[IDX_Y]).toHaveProperty('value', '20');
    expect(inputs[IDX_W]).toHaveProperty('value', '50');
    expect(inputs[IDX_H]).toHaveProperty('value', '50');
  });

  it('displays Y position relative to a bottom-left machine origin', () => {
    useProjectStore.setState({ project: makeBottomLeftProject(), selectedObjectIds: ['obj1'] });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');

    expect(inputs[IDX_X]).toHaveProperty('value', '10');
    expect(inputs[IDX_Y]).toHaveProperty('value', '280');
  });

  it('field edit commits updateObject with new bounds on blur', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'], updateObject });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    // Change X from 10 to 15
    typeAndCommit(inputs[IDX_X], '15');
    expect(updateObject).toHaveBeenCalledWith('obj1', {
      bounds: { min: { x: 15, y: 20 }, max: { x: 65, y: 70 } },
    });
  });

  it('Y edit converts from bottom-left machine coordinates back to canvas coordinates', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeBottomLeftProject(), selectedObjectIds: ['obj1'], updateObject });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');

    typeAndCommit(inputs[IDX_Y], '270');

    expect(updateObject).toHaveBeenCalledWith('obj1', {
      bounds: { min: { x: 10, y: 30 }, max: { x: 60, y: 80 } },
    });
  });

  it('Lock aspect constrains dimensions proportionally', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'], updateObject });
    render(<TransformSection />);
    // Enable lock via the padlock toggle button
    const lockButton = screen.getByTitle('Lock aspect ratio');
    fireEvent.click(lockButton);
    // Change W from 50 to 100 — H should scale to 100 too (1:1 aspect)
    const inputs = screen.getAllByRole('spinbutton');
    typeAndCommit(inputs[IDX_W], '100');
    expect(updateObject).toHaveBeenCalledWith('obj1', {
      bounds: { min: { x: 10, y: 20 }, max: { x: 110, y: 120 } },
    });
  });

  it('Anchor grid renders 9 buttons', () => {
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'] });
    render(<TransformSection />);
    // 9 anchor buttons + the tool/field buttons
    const allButtons = screen.getAllByRole('button');
    // Anchor grid has exactly 9 circular buttons
    const anchorButtons = allButtons.filter((b) => b.classList.contains('rounded-full'));
    expect(anchorButtons.length).toBe(9);
  });

  it('renders Rotate and Scale % fields', () => {
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'] });
    render(<TransformSection />);
    expect(screen.getByText('Rotation')).toBeDefined();
    // Scale % fields are unlabeled — verify they exist as spinbuttons
    const inputs = screen.getAllByRole('spinbutton');
    expect(inputs[IDX_SCALE_X]).toHaveProperty('value', '100');
    expect(inputs[IDX_SCALE_Y]).toHaveProperty('value', '100');
  });

  it('X change is blocked when position is locked', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    const proj = makeProject();
    proj.transform_locks = makeTransformLocks({ move_enabled: false });
    useProjectStore.setState({ project: proj, selectedObjectIds: ['obj1'], updateObject });
    useNotificationStore.setState({ notifications: [] });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    typeAndCommit(inputs[IDX_X], '15');
    expect(updateObject).not.toHaveBeenCalled();
    const notifications = useNotificationStore.getState().notifications;
    expect(notifications[notifications.length - 1]?.message).toContain('Position is locked');
  });

  it('W change is blocked when scale is locked', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    const proj = makeProject();
    proj.transform_locks = makeTransformLocks({ size_enabled: false });
    useProjectStore.setState({ project: proj, selectedObjectIds: ['obj1'], updateObject });
    useNotificationStore.setState({ notifications: [] });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    typeAndCommit(inputs[IDX_W], '100');
    expect(updateObject).not.toHaveBeenCalled();
    const notifications = useNotificationStore.getState().notifications;
    expect(notifications[notifications.length - 1]?.message).toContain('Scale is locked');
  });

  it('rotation field calls rotateObjects', () => {
    const rotateObjects = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'], rotateObjects });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    typeAndCommit(inputs[IDX_ROT], '45');
    expect(rotateObjects).toHaveBeenCalledWith(['obj1'], 45);
  });

  it('rotation is blocked when rotation is locked', () => {
    const rotateObjects = vi.fn().mockResolvedValue(undefined);
    const proj = makeProject();
    (proj as Record<string, unknown>).transform_locks = { rotate_enabled: false };
    useProjectStore.setState({ project: proj, selectedObjectIds: ['obj1'], rotateObjects });
    useNotificationStore.setState({ notifications: [] });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    typeAndCommit(inputs[IDX_ROT], '45');
    expect(rotateObjects).not.toHaveBeenCalled();
    const notifications = useNotificationStore.getState().notifications;
    expect(notifications[notifications.length - 1]?.message).toContain('Rotation is locked');
  });

  it('W change with center anchor adjusts both min and max', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'], updateObject });
    render(<TransformSection />);
    // Select center anchor (5th button in grid = index 4 of anchor buttons)
    const allButtons = screen.getAllByRole('button');
    const anchorButtons = allButtons.filter((b) => b.classList.contains('rounded-full'));
    fireEvent.click(anchorButtons[4]); // center anchor
    // Change W from 50 to 100 — center should stay fixed at 35 (10 + 50/2)
    const inputs = screen.getAllByRole('spinbutton');
    typeAndCommit(inputs[IDX_W], '100');
    // anchor X = 10 + (1/2)*50 = 35, newMinX = 35 - (1/2)*100 = -15
    expect(updateObject).toHaveBeenCalledWith('obj1', {
      bounds: { min: { x: -15, y: 20 }, max: { x: 85, y: 70 } },
    });
  });

  it('renders nothing when nothing is selected', () => {
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: [] });
    const { container } = render(<TransformSection />);
    expect(container.firstChild).toBeNull();
  });

  it('X change is blocked when object is locked', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    const proj = makeProject();
    proj.objects[0].locked = true;
    useProjectStore.setState({ project: proj, selectedObjectIds: ['obj1'], updateObject });
    useNotificationStore.setState({ notifications: [] });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    typeAndCommit(inputs[IDX_X], '15');
    expect(updateObject).not.toHaveBeenCalled();
    const notifications = useNotificationStore.getState().notifications;
    expect(notifications[notifications.length - 1]?.message).toContain('Object is locked');
  });

  it('rotation is blocked when object is locked', () => {
    const rotateObjects = vi.fn().mockResolvedValue(undefined);
    const proj = makeProject();
    proj.objects[0].locked = true;
    useProjectStore.setState({ project: proj, selectedObjectIds: ['obj1'], rotateObjects });
    useNotificationStore.setState({ notifications: [] });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    typeAndCommit(inputs[IDX_ROT], '45');
    expect(rotateObjects).not.toHaveBeenCalled();
    const notifications = useNotificationStore.getState().notifications;
    expect(notifications[notifications.length - 1]?.message).toContain('Object is locked');
  });
});

const makeMultiProject = () => ({
  ...makeProjectFixture({
    metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p1', project_name: 'Test', created_at: '', modified_at: '' },
    layers: [makeLayer({ id: 'l1', name: 'L1', operation: 'line', color_tag: '#ff0000' })],
    assets: [],
  }),
  objects: [
    makeProjectObject({
      id: 'obj1', name: 'Rect1',
      bounds: { min: { x: 10, y: 20 }, max: { x: 60, y: 70 } },
      layer_id: 'l1',
      data: { type: 'shape' as const, kind: 'rectangle' as const, width: 50, height: 50, corner_radius: 0 },
    }),
    makeProjectObject({
      id: 'obj2', name: 'Rect2',
      bounds: { min: { x: 100, y: 20 }, max: { x: 150, y: 70 } },
      layer_id: 'l1', z_index: 1,
      data: { type: 'shape' as const, kind: 'rectangle' as const, width: 50, height: 50, corner_radius: 0 },
      created_at: '2026-01-01T00:00:01Z',
    }),
  ],
});

describe('TransformSection — multi-selection', () => {
  it('displays selection bounding box when multiple objects selected', () => {
    useProjectStore.setState({ project: makeMultiProject(), selectedObjectIds: ['obj1', 'obj2'] });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    // Selection bbox: min(10,100)=10, max(60,150)=150 → X=10, W=140
    // Y: min(20,20)=20, max(70,70)=70 → Y=20, H=50
    expect(inputs[IDX_X]).toHaveProperty('value', '10');
    expect(inputs[IDX_W]).toHaveProperty('value', '140');
    expect(inputs[IDX_Y]).toHaveProperty('value', '20');
    expect(inputs[IDX_H]).toHaveProperty('value', '50');
  });

  it('X change calls nudgeObjects for multi-selection', () => {
    const nudgeObjects = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeMultiProject(), selectedObjectIds: ['obj1', 'obj2'], nudgeObjects });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    // Move X from 10 to 20 → dx=10
    typeAndCommit(inputs[IDX_X], '20');
    expect(nudgeObjects).toHaveBeenCalledWith(['obj1', 'obj2'], 10, 0);
  });

  it('Y change calls nudgeObjects for multi-selection', () => {
    const nudgeObjects = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeMultiProject(), selectedObjectIds: ['obj1', 'obj2'], nudgeObjects });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    // Move Y from 20 to 30 → dy=10
    typeAndCommit(inputs[IDX_Y], '30');
    expect(nudgeObjects).toHaveBeenCalledWith(['obj1', 'obj2'], 0, 10);
  });

  it('W change calls updateObject for each selected object (proportional scale)', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    const updateObjectBoundsBatch = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeMultiProject(), selectedObjectIds: ['obj1', 'obj2'], updateObject, updateObjectBoundsBatch });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    // Selection W=140. Change to 280 → sx=2. Anchor at top_left (col=0) → anchorX=10
    // obj1: min.x = 10 + (10-10)*2 = 10, max.x = 10 + (60-10)*2 = 110
    // obj2: min.x = 10 + (100-10)*2 = 190, max.x = 10 + (150-10)*2 = 290
    typeAndCommit(inputs[IDX_W], '280');
    expect(updateObject).not.toHaveBeenCalled();
    expect(updateObjectBoundsBatch).toHaveBeenCalledTimes(1);
    expect(updateObjectBoundsBatch).toHaveBeenCalledWith([
      { id: 'obj1', bounds: { min: { x: 10, y: 20 }, max: { x: 110, y: 70 } } },
      { id: 'obj2', bounds: { min: { x: 190, y: 20 }, max: { x: 290, y: 70 } } },
    ]);
  });

  it('H change uses one batch bounds update for multi-selection', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    const updateObjectBoundsBatch = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeMultiProject(), selectedObjectIds: ['obj1', 'obj2'], updateObject, updateObjectBoundsBatch });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    typeAndCommit(inputs[IDX_H], '100');
    expect(updateObject).not.toHaveBeenCalled();
    expect(updateObjectBoundsBatch).toHaveBeenCalledTimes(1);
    expect(updateObjectBoundsBatch).toHaveBeenCalledWith([
      { id: 'obj1', bounds: { min: { x: 10, y: 20 }, max: { x: 60, y: 120 } } },
      { id: 'obj2', bounds: { min: { x: 100, y: 20 }, max: { x: 150, y: 120 } } },
    ]);
  });

  it('rotation calls rotateObjects with all selected IDs', () => {
    const rotateObjects = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeMultiProject(), selectedObjectIds: ['obj1', 'obj2'], rotateObjects });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    typeAndCommit(inputs[IDX_ROT], '45');
    expect(rotateObjects).toHaveBeenCalledWith(['obj1', 'obj2'], 45);
  });
});

describe('TransformSection — Scale X/Y', () => {
  it('Scale X only changes width', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'], updateObject });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    // Scale X to 200% — width doubles, height unchanged
    typeAndCommit(inputs[IDX_SCALE_X], '200');
    expect(updateObject).toHaveBeenCalledWith('obj1', {
      bounds: { min: { x: 10, y: 20 }, max: { x: 110, y: 70 } },
    });
  });

  it('Scale Y only changes height', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'], updateObject });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    // Scale Y to 200% — height doubles, width unchanged
    typeAndCommit(inputs[IDX_SCALE_Y], '200');
    expect(updateObject).toHaveBeenCalledWith('obj1', {
      bounds: { min: { x: 10, y: 20 }, max: { x: 60, y: 120 } },
    });
  });

  it('locked Scale X changes width and height proportionally', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'], updateObject });
    render(<TransformSection />);
    fireEvent.click(screen.getByTitle('Lock aspect ratio'));
    const inputs = screen.getAllByRole('spinbutton');
    typeAndCommit(inputs[IDX_SCALE_X], '200');
    expect(updateObject).toHaveBeenCalledWith('obj1', {
      bounds: { min: { x: 10, y: 20 }, max: { x: 110, y: 120 } },
    });
  });

  it('locked Scale Y changes width and height proportionally', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'], updateObject });
    render(<TransformSection />);
    fireEvent.click(screen.getByTitle('Lock aspect ratio'));
    const inputs = screen.getAllByRole('spinbutton');
    typeAndCommit(inputs[IDX_SCALE_Y], '200');
    expect(updateObject).toHaveBeenCalledWith('obj1', {
      bounds: { min: { x: 10, y: 20 }, max: { x: 110, y: 120 } },
    });
  });

  it('multi-selection Scale X uses one batch bounds update', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    const updateObjectBoundsBatch = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeMultiProject(), selectedObjectIds: ['obj1', 'obj2'], updateObject, updateObjectBoundsBatch });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    typeAndCommit(inputs[IDX_SCALE_X], '200');
    expect(updateObject).not.toHaveBeenCalled();
    expect(updateObjectBoundsBatch).toHaveBeenCalledTimes(1);
    expect(updateObjectBoundsBatch).toHaveBeenCalledWith([
      { id: 'obj1', bounds: { min: { x: 10, y: 20 }, max: { x: 110, y: 70 } } },
      { id: 'obj2', bounds: { min: { x: 190, y: 20 }, max: { x: 290, y: 70 } } },
    ]);
  });

  it('multi-selection Scale Y uses one batch bounds update', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    const updateObjectBoundsBatch = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeMultiProject(), selectedObjectIds: ['obj1', 'obj2'], updateObject, updateObjectBoundsBatch });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    typeAndCommit(inputs[IDX_SCALE_Y], '200');
    expect(updateObject).not.toHaveBeenCalled();
    expect(updateObjectBoundsBatch).toHaveBeenCalledTimes(1);
    expect(updateObjectBoundsBatch).toHaveBeenCalledWith([
      { id: 'obj1', bounds: { min: { x: 10, y: 20 }, max: { x: 60, y: 120 } } },
      { id: 'obj2', bounds: { min: { x: 100, y: 20 }, max: { x: 150, y: 120 } } },
    ]);
  });

  it('locked multi-selection percentage scaling preserves the selection aspect ratio', () => {
    const updateObjectBoundsBatch = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project: makeMultiProject(),
      selectedObjectIds: ['obj1', 'obj2'],
      updateObjectBoundsBatch,
    });
    render(<TransformSection />);
    fireEvent.click(screen.getByTitle('Lock aspect ratio'));
    const inputs = screen.getAllByRole('spinbutton');
    typeAndCommit(inputs[IDX_SCALE_X], '200');
    expect(updateObjectBoundsBatch).toHaveBeenCalledWith([
      { id: 'obj1', bounds: { min: { x: 10, y: 20 }, max: { x: 110, y: 120 } } },
      { id: 'obj2', bounds: { min: { x: 190, y: 20 }, max: { x: 290, y: 120 } } },
    ]);
  });
});

describe('TransformSection — buffered commit semantics', () => {
  it('typing partial values does not commit; blur commits once with the final value', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'], updateObject });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    // Typing "100" arrives as three keystrokes: 1, 10, 100
    fireEvent.change(inputs[IDX_X], { target: { value: '1' } });
    fireEvent.change(inputs[IDX_X], { target: { value: '10' } });
    fireEvent.change(inputs[IDX_X], { target: { value: '100' } });
    expect(updateObject).not.toHaveBeenCalled();
    expect(inputs[IDX_X]).toHaveProperty('value', '100');
    fireEvent.blur(inputs[IDX_X]);
    expect(updateObject).toHaveBeenCalledTimes(1);
    expect(updateObject).toHaveBeenCalledWith('obj1', {
      bounds: { min: { x: 100, y: 20 }, max: { x: 150, y: 70 } },
    });
  });

  it('Enter commits the typed value and blur does not double-commit', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'], updateObject });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    fireEvent.change(inputs[IDX_X], { target: { value: '15' } });
    expect(updateObject).not.toHaveBeenCalled();
    fireEvent.keyDown(inputs[IDX_X], { key: 'Enter' });
    expect(updateObject).toHaveBeenCalledTimes(1);
    expect(updateObject).toHaveBeenCalledWith('obj1', {
      bounds: { min: { x: 15, y: 20 }, max: { x: 65, y: 70 } },
    });
    fireEvent.blur(inputs[IDX_X]);
    expect(updateObject).toHaveBeenCalledTimes(1);
  });

  it('Escape reverts the buffer to the committed value without committing', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'], updateObject });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    fireEvent.change(inputs[IDX_X], { target: { value: '999' } });
    expect(inputs[IDX_X]).toHaveProperty('value', '999');
    fireEvent.keyDown(inputs[IDX_X], { key: 'Escape' });
    expect(inputs[IDX_X]).toHaveProperty('value', '10');
    fireEvent.blur(inputs[IDX_X]);
    expect(updateObject).not.toHaveBeenCalled();
  });

  it('clearing the field and blurring reverts without committing', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'], updateObject });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    fireEvent.change(inputs[IDX_W], { target: { value: '' } });
    fireEvent.blur(inputs[IDX_W]);
    expect(updateObject).not.toHaveBeenCalled();
    expect(inputs[IDX_W]).toHaveProperty('value', '50');
  });

  it('external bounds updates do not clobber a pending typed value', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    const project = makeProject();
    useProjectStore.setState({ project, selectedObjectIds: ['obj1'], updateObject });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    fireEvent.change(inputs[IDX_X], { target: { value: '42' } });
    act(() => {
      useProjectStore.setState({
        project: {
          ...project,
          objects: [{ ...project.objects[0], bounds: { min: { x: 30, y: 20 }, max: { x: 80, y: 70 } } }],
        },
      });
    });
    expect(inputs[IDX_X]).toHaveProperty('value', '42');
  });

  it('stepper arrow click commits immediately without blur', () => {
    const rotateObjects = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'], rotateObjects });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    // NumberStepper renders the up/down buttons next to the input
    const upButton = inputs[IDX_ROT].parentElement!.querySelectorAll('button')[0];
    fireEvent.pointerDown(upButton);
    fireEvent.pointerUp(upButton);
    expect(rotateObjects).toHaveBeenCalledTimes(1);
    expect(rotateObjects).toHaveBeenCalledWith(['obj1'], 1);
  });

  it('scale field accepts multi-digit entry and commits once on blur', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'], updateObject });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    fireEvent.change(inputs[IDX_SCALE_X], { target: { value: '5' } });
    fireEvent.change(inputs[IDX_SCALE_X], { target: { value: '50' } });
    expect(inputs[IDX_SCALE_X]).toHaveProperty('value', '50');
    expect(updateObject).not.toHaveBeenCalled();
    fireEvent.blur(inputs[IDX_SCALE_X]);
    expect(updateObject).toHaveBeenCalledTimes(1);
    // 50% of W=50 anchored top_left → bounds 10..35
    expect(updateObject).toHaveBeenCalledWith('obj1', {
      bounds: { min: { x: 10, y: 20 }, max: { x: 35, y: 70 } },
    });
  });

  it('selection change discards a pending typed value', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeMultiProject(), selectedObjectIds: ['obj1'], updateObject });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    fireEvent.change(inputs[IDX_X], { target: { value: '77' } });
    act(() => {
      useProjectStore.setState({ selectedObjectIds: ['obj2'] });
    });
    expect(inputs[IDX_X]).toHaveProperty('value', '100');
    fireEvent.blur(inputs[IDX_X]);
    expect(updateObject).not.toHaveBeenCalled();
  });
});

describe('TransformSection — mm/in toggle', () => {
  it('displays values in inches when display_unit is inches', () => {
    useAppStore.setState({ settings: makeAppSettings({ display_unit: 'inches' }) });
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'] });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    // X=10mm → 10/25.4 ≈ 0.3937 in
    expect(Number(inputs[IDX_X].getAttribute('value') ?? inputs[IDX_X]?.nodeValue)).toBeCloseTo(0.3937, 3);
    // W=50mm → 50/25.4 ≈ 1.9685 in
    expect(Number(inputs[IDX_W].getAttribute('value') ?? inputs[IDX_W]?.nodeValue)).toBeCloseTo(1.9685, 3);
    // Unit label should show 'in'
    expect(screen.getAllByText('in').length).toBeGreaterThan(0);
  });

  it('input in inches converts back to mm for updateObject', () => {
    useAppStore.setState({ settings: makeAppSettings({ display_unit: 'inches' }) });
    const updateObject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'], updateObject });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    // Type 1 inch for X → should convert to 25.4mm
    // Current displayX in mm is 10 (top_left anchor). 1 inch = 25.4mm. dx = 25.4 - 10 = 15.4
    typeAndCommit(inputs[IDX_X], '1');
    expect(updateObject).toHaveBeenCalledWith('obj1', {
      bounds: { min: { x: 25.4, y: 20 }, max: { x: 75.4, y: 70 } },
    });
  });

  it('shows mm toggle button that switches units', () => {
    useAppStore.setState({ settings: makeAppSettings({ display_unit: 'mm' }) });
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'] });
    render(<TransformSection />);
    const toggleBtn = screen.getByTitle('Switch to inches');
    expect(toggleBtn.textContent).toBe('mm');
    fireEvent.click(toggleBtn);
    // Optimistic update sets display_unit directly in the store
    expect(useAppStore.getState().settings?.display_unit).toBe('inches');
  });
});
