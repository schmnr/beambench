import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent, act } from '@testing-library/react';
import { TransformSection } from '../TransformSection';
import { useProjectStore } from '../../../stores/projectStore';
import { useAppStore } from '../../../stores/appStore';
import { useUiStore } from '../../../stores/uiStore';
import { useNotificationStore } from '../../../stores/notificationStore';
import { makeAppSettings, makeLayer, makeProject as makeProjectFixture, makeProjectObject, makeTransformLocks } from '../../../test-utils/projectFixtures';
import { clearCanvasViewportSize, setCanvasViewportSize } from '../../../canvas/canvasViewportRegistry';

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
const initialAppState = useAppStore.getState();
const initialUiState = useUiStore.getState();

afterEach(() => {
  cleanup();
  useProjectStore.setState(initialState, true);
  useAppStore.setState(initialAppState, true);
  useUiStore.setState(initialUiState, true);
  clearCanvasViewportSize();
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

  it('uses the shared inspector title treatment', () => {
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'] });
    render(<TransformSection />);

    const titleBar = screen.getByTestId('transform-title-bar');
    expect(titleBar.className).toContain('bg-gradient-to-r');
    expect(titleBar.className).toContain('from-bb-accent/10');
    expect(titleBar.className).toContain('to-bb-surface/30');
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
    const project = makeProject();
    project.objects[0].lock_aspect_ratio = true;
    useProjectStore.setState({ project, selectedObjectIds: ['obj1'], updateObject });
    render(<TransformSection />);
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

  it('places Fit Selection beside Center on Page before the anchor grid', () => {
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'] });
    render(<TransformSection />);

    const centerButton = screen.getByRole('button', { name: 'Center on Page' });
    const fitSelectionButton = screen.getByRole('button', { name: 'Fit Selection' });
    const firstAnchor = screen.getByRole('button', { name: 'Top left' });
    expect(centerButton.className).toContain('h-7');
    expect(centerButton.className).toContain('w-7');
    expect(centerButton.querySelector('.lucide-focus')).not.toBeNull();
    expect(fitSelectionButton.className).toContain('h-7');
    expect(fitSelectionButton.className).toContain('w-7');
    expect(fitSelectionButton.querySelector('.lucide-search')).not.toBeNull();
    expect(fitSelectionButton.querySelector('.lucide-focus')).toBeNull();
    expect(centerButton.compareDocumentPosition(fitSelectionButton) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(fitSelectionButton.compareDocumentPosition(firstAnchor) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('fits the visual selection bounds from the Transform header', () => {
    const zoomToFit = vi.fn();
    const project = makeProject();
    project.objects[0].transform = { a: 1, b: 0.4, c: 0.5, d: 1, tx: 0, ty: 0 };
    useProjectStore.setState({ project, selectedObjectIds: ['obj1'] });
    useUiStore.setState({ zoomToFit });
    setCanvasViewportSize({ width: 1000, height: 600 });

    render(<TransformSection />);
    fireEvent.click(screen.getByRole('button', { name: 'Fit Selection' }));

    expect(zoomToFit).toHaveBeenCalledWith(
      expect.objectContaining({ x: expect.any(Number), y: expect.any(Number) }),
      expect.any(Number),
    );
  });

  it('toggles every transform lock from the header and changes its icon', () => {
    const updateObjectTransformState = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({
      project: makeProject(),
      selectedObjectIds: ['obj1'],
      updateObjectTransformState,
    });

    const { rerender } = render(<TransformSection />);
    const lockAllButton = screen.getByRole('button', { name: 'Lock all transforms' });
    expect(lockAllButton.querySelector('.lucide-lock-open')).not.toBeNull();
    fireEvent.click(lockAllButton);
    expect(updateObjectTransformState).toHaveBeenCalledWith(['obj1'], {
      transformLocks: makeTransformLocks({
        move_enabled: false,
        size_enabled: false,
        rotate_enabled: false,
        shear_enabled: false,
      }),
      lockAspectRatio: true,
    });

    const lockedProject = makeProject();
    lockedProject.objects[0].transform_locks = makeTransformLocks({
      move_enabled: false,
      size_enabled: false,
      rotate_enabled: false,
      shear_enabled: false,
    });
    lockedProject.objects[0].lock_aspect_ratio = true;
    useProjectStore.setState({ project: lockedProject });
    rerender(<TransformSection />);

    const unlockAllButton = screen.getByRole('button', { name: 'Unlock all transforms' });
    expect(unlockAllButton.querySelector('.lucide-lock')).not.toBeNull();
    fireEvent.click(unlockAllButton);
    expect(updateObjectTransformState).toHaveBeenLastCalledWith(['obj1'], {
      transformLocks: makeTransformLocks(),
      lockAspectRatio: false,
    });
  });

  it('centers the selected object on the page from the Transform header', () => {
    const moveObjectsTo = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project: makeProject(),
      selectedObjectIds: ['obj1'],
      moveObjectsTo,
    });

    render(<TransformSection />);
    fireEvent.click(screen.getByRole('button', { name: 'Center on Page' }));

    expect(moveObjectsTo).toHaveBeenCalledWith(['obj1'], 175, 175);
  });

  it('renders Rotate and Scale % fields', () => {
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'] });
    render(<TransformSection />);
    expect(screen.getByText('⟳')).toBeDefined();
    // Scale % fields are unlabeled — verify they exist as spinbuttons
    const inputs = screen.getAllByRole('spinbutton');
    expect(inputs[IDX_SCALE_X]).toHaveProperty('value', '100');
    expect(inputs[IDX_SCALE_Y]).toHaveProperty('value', '100');
  });

  it('reports transform-aware width and height after shear', () => {
    const project = makeProject();
    project.objects[0].transform = { a: 1, b: 0.4, c: 0.5, d: 1, tx: 0, ty: 0 };
    useProjectStore.setState({ project, selectedObjectIds: ['obj1'] });

    render(<TransformSection />);

    const inputs = screen.getAllByRole('spinbutton');
    expect(inputs[IDX_W]).toHaveProperty('value', '75');
    expect(inputs[IDX_H]).toHaveProperty('value', '70');
  });

  it('translates a visible sheared width edit back to the raw object bounds', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    const project = makeProject();
    project.objects[0].transform = { a: 1, b: 0, c: 0.5, d: 1, tx: 0, ty: 0 };
    useProjectStore.setState({ project, selectedObjectIds: ['obj1'], updateObject });

    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    typeAndCommit(inputs[IDX_W], '100');

    expect(updateObject).toHaveBeenCalledTimes(1);
    const updates = updateObject.mock.calls[0][1];
    expect(updates.bounds.min.x).toBeCloseTo(10);
    expect(updates.bounds.max.x).toBeCloseTo(85);
    expect(updates.bounds.min.y).toBeCloseTo(20);
    expect(updates.bounds.max.y).toBeCloseTo(70);
  });

  it('puts each object transform lock beside the fields it governs', () => {
    const updateObjectTransformState = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({
      project: makeProject(),
      selectedObjectIds: ['obj1'],
      updateObjectTransformState,
    });

    render(<TransformSection />);

    expect(screen.queryByTestId('transform-lock-controls')).toBeNull();
    const moveLock = screen.getByRole('button', { name: 'Move' });
    expect(screen.getByRole('button', { name: 'Size' })).toBeDefined();
    expect(screen.getByRole('button', { name: 'Rotate' })).toBeDefined();
    expect(screen.getByRole('button', { name: 'Shear' })).toBeDefined();
    expect(moveLock.getAttribute('aria-pressed')).toBe('false');
    fireEvent.click(moveLock);
    expect(updateObjectTransformState).toHaveBeenCalledWith(['obj1'], {
      transformLockKey: 'move_enabled',
      transformEnabled: false,
    });
  });

  it('uses the same padlock vocabulary for proportional W/H sizing', () => {
    const updateObjectTransformState = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({
      project: makeProject(),
      selectedObjectIds: ['obj1'],
      updateObjectTransformState,
    });

    const { rerender } = render(<TransformSection />);

    const aspectButton = screen.getByTitle('Lock aspect ratio');
    expect(aspectButton.querySelector('.lucide-lock-open')).not.toBeNull();
    fireEvent.click(aspectButton);
    expect(updateObjectTransformState).toHaveBeenCalledWith(['obj1'], { lockAspectRatio: true });
    const lockedProject = makeProject();
    lockedProject.objects[0].lock_aspect_ratio = true;
    act(() => {
      useProjectStore.setState({ project: lockedProject });
    });
    rerender(<TransformSection />);
    expect(screen.getByTitle('Lock aspect ratio').querySelector('.lucide-lock')).not.toBeNull();
  });

  it('exposes a distinct aspect link directly between the scale fields', () => {
    const updateObjectTransformState = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({
      project: makeProject(),
      selectedObjectIds: ['obj1'],
      updateObjectTransformState,
    });

    const { rerender } = render(<TransformSection />);

    const aspectLink = screen.getByRole('button', { name: 'Lock aspect ratio (SX / SY)' });
    expect(aspectLink.getAttribute('aria-pressed')).toBe('false');
    expect(aspectLink.querySelector('.lucide-unlink-2')).not.toBeNull();
    fireEvent.click(aspectLink);
    expect(updateObjectTransformState).toHaveBeenCalledWith(['obj1'], { lockAspectRatio: true });

    const lockedProject = makeProject();
    lockedProject.objects[0].lock_aspect_ratio = true;
    act(() => {
      useProjectStore.setState({ project: lockedProject });
    });
    rerender(<TransformSection />);

    const linkedAspect = screen.getByRole('button', { name: 'Lock aspect ratio (SX / SY)' });
    expect(linkedAspect.getAttribute('aria-pressed')).toBe('true');
    expect(linkedAspect.querySelector('.lucide-link-2')).not.toBeNull();
    expect(linkedAspect.className).toContain('text-bb-accent');
  });

  it('uses the cyan active color for every locked transform control', () => {
    const project = makeProject();
    project.objects[0].transform_locks = makeTransformLocks({
      move_enabled: false,
      size_enabled: false,
      rotate_enabled: false,
      shear_enabled: false,
    });
    useProjectStore.setState({ project, selectedObjectIds: ['obj1'] });
    project.objects[0].lock_aspect_ratio = true;

    render(<TransformSection />);

    const lockButtons = [
      screen.getByRole('button', { name: 'Move' }),
      screen.getByTitle('Lock aspect ratio'),
      screen.getByRole('button', { name: 'Lock aspect ratio (SX / SY)' }),
      screen.getByRole('button', { name: 'Size' }),
      screen.getByRole('button', { name: 'Rotate' }),
      screen.getByRole('button', { name: 'Shear' }),
    ];
    lockButtons.forEach((button) => {
      expect(button.className).toContain('text-bb-accent');
      expect(button.className).not.toContain('text-bb-text-dim');
    });
  });

  it('X change is blocked when position is locked', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    const proj = makeProject();
    proj.objects[0].transform_locks = makeTransformLocks({ move_enabled: false });
    useProjectStore.setState({ project: proj, selectedObjectIds: ['obj1'], updateObject });
    useNotificationStore.setState({ notifications: [] });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    typeAndCommit(inputs[IDX_X], '15');
    expect(updateObject).not.toHaveBeenCalled();
    const notifications = useNotificationStore.getState().notifications;
    expect(notifications[notifications.length - 1]?.message).toContain('Position is locked');
  });

  it('keeps transform locks isolated between objects on the same layer', () => {
    const project = makeProject();
    project.objects[0].transform_locks = makeTransformLocks({ move_enabled: false });
    project.objects.push(makeProjectObject({
      id: 'obj2',
      name: 'Rect2',
      layer_id: 'l1',
      bounds: { min: { x: 80, y: 20 }, max: { x: 130, y: 70 } },
      data: {
        type: 'shape',
        kind: 'rectangle',
        width: 50,
        height: 50,
        corner_radius: 0,
      },
    }));
    useProjectStore.setState({ project, selectedObjectIds: ['obj1'] });
    const { rerender } = render(<TransformSection />);
    expect(screen.getByRole('button', { name: 'Move' }).getAttribute('aria-pressed')).toBe('true');

    useProjectStore.setState({ selectedObjectIds: ['obj2'] });
    rerender(<TransformSection />);
    expect(screen.getByRole('button', { name: 'Move' }).getAttribute('aria-pressed')).toBe('false');
  });

  it('W change is blocked when scale is locked', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    const proj = makeProject();
    proj.objects[0].transform_locks = makeTransformLocks({ size_enabled: false });
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
    proj.objects[0].transform_locks = makeTransformLocks({ rotate_enabled: false });
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

  it('centers the combined selection bounds on the page', () => {
    const moveObjectsTo = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project: makeMultiProject(),
      selectedObjectIds: ['obj1', 'obj2'],
      moveObjectsTo,
    });

    render(<TransformSection />);
    fireEvent.click(screen.getByRole('button', { name: 'Center on Page' }));

    expect(moveObjectsTo).toHaveBeenCalledWith(['obj1', 'obj2'], 130, 175);
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

  it('keeps the committed scale percentage after the bounds update and scales from that baseline', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    const project = makeProject();
    useProjectStore.setState({ project, selectedObjectIds: ['obj1'], updateObject });
    render(<TransformSection />);

    let inputs = screen.getAllByRole('spinbutton');
    typeAndCommit(inputs[IDX_SCALE_X], '150');
    expect(updateObject).toHaveBeenLastCalledWith('obj1', {
      bounds: { min: { x: 10, y: 20 }, max: { x: 85, y: 70 } },
    });

    act(() => {
      useProjectStore.setState({
        project: {
          ...project,
          objects: [{
            ...project.objects[0],
            bounds: { min: { x: 10, y: 20 }, max: { x: 85, y: 70 } },
          }],
        },
      });
    });

    inputs = screen.getAllByRole('spinbutton');
    expect(inputs[IDX_SCALE_X]).toHaveProperty('value', '150');
    typeAndCommit(inputs[IDX_SCALE_X], '200');
    expect(updateObject).toHaveBeenLastCalledWith('obj1', {
      bounds: { min: { x: 10, y: 20 }, max: { x: 110, y: 70 } },
    });

    act(() => {
      useProjectStore.setState({ project });
    });
    inputs = screen.getAllByRole('spinbutton');
    expect(inputs[IDX_SCALE_X]).toHaveProperty('value', '100');
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
    const project = makeProject();
    project.objects[0].lock_aspect_ratio = true;
    useProjectStore.setState({ project, selectedObjectIds: ['obj1'], updateObject });
    render(<TransformSection />);
    const inputs = screen.getAllByRole('spinbutton');
    typeAndCommit(inputs[IDX_SCALE_X], '200');
    expect(updateObject).toHaveBeenCalledWith('obj1', {
      bounds: { min: { x: 10, y: 20 }, max: { x: 110, y: 120 } },
    });
  });

  it('locked Scale Y changes width and height proportionally', () => {
    const updateObject = vi.fn().mockResolvedValue(undefined);
    const project = makeProject();
    project.objects[0].lock_aspect_ratio = true;
    useProjectStore.setState({ project, selectedObjectIds: ['obj1'], updateObject });
    render(<TransformSection />);
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
    const project = makeMultiProject();
    project.objects.forEach((object) => { object.lock_aspect_ratio = true; });
    useProjectStore.setState({
      project,
      selectedObjectIds: ['obj1', 'obj2'],
      updateObjectBoundsBatch,
    });
    render(<TransformSection />);
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
