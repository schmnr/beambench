import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent, waitFor } from '@testing-library/react';
import { GridArrayDialog } from '../GridArrayDialog';
import { CircularArrayDialog } from '../CircularArrayDialog';
import { CopyAlongPathDialog } from '../CopyAlongPathDialog';
import { useProjectStore } from '../../../stores/projectStore';

const mockInvoke = vi.fn().mockResolvedValue(null);
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => mockInvoke(...args) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const initialProjectState = useProjectStore.getState();

afterEach(() => {
  cleanup();
  mockInvoke.mockClear();
  useProjectStore.setState(initialProjectState, true);
});

describe('GridArrayDialog', () => {
  it('renders all form fields', () => {
    render(<GridArrayDialog objectIds={['obj-1']} onClose={vi.fn()} />);
    expect(screen.getByText('Rows')).toBeDefined();
    expect(screen.getByText('Columns')).toBeDefined();
    expect(screen.getByText('H Spacing (mm)')).toBeDefined();
    expect(screen.getByText('V Spacing (mm)')).toBeDefined();
  });

  it('renders the array options', () => {
    render(<GridArrayDialog objectIds={['obj-1']} onClose={vi.fn()} />);
    expect(screen.getByText('Spacing Mode')).toBeDefined();
    expect(screen.getByText('X Axis Mode')).toBeDefined();
    expect(screen.getByText('Y Axis Mode')).toBeDefined();
    expect(screen.getByText('Total Width (mm)')).toBeDefined();
    expect(screen.getByText('Total Height (mm)')).toBeDefined();
    expect(screen.getByText('Mirror Alternate Cols')).toBeDefined();
    expect(screen.getByText('Mirror Alternate Rows')).toBeDefined();
    expect(screen.getByText('Half Shift (Brickwork)')).toBeDefined();
    expect(screen.getByText('Group Results')).toBeDefined();
    expect(screen.getByText('Create Virtual Array')).toBeDefined();
  });

  it('submit calls gridArray with correct params', async () => {
    const spy = vi.spyOn(useProjectStore.getState(), 'gridArray').mockResolvedValue(undefined);
    const onClose = vi.fn();
    render(<GridArrayDialog objectIds={['obj-1']} onClose={onClose} />);

    fireEvent.click(screen.getByTestId('grid-array-submit'));

    await waitFor(() => {
      expect(spy).toHaveBeenCalledWith(expect.objectContaining({
        objectIds: ['obj-1'],
        rows: 2,
        cols: 2,
        hSpacingMm: 5,
        vSpacingMm: 5,
      }));
    });
    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
    spy.mockRestore();
  });

  it('keeps dialog open when gridArray fails', async () => {
    const spy = vi.spyOn(useProjectStore.getState(), 'gridArray').mockRejectedValue(new Error('fail'));
    const onClose = vi.fn();
    render(<GridArrayDialog objectIds={['obj-1']} onClose={onClose} />);

    fireEvent.click(screen.getByTestId('grid-array-submit'));

    await waitFor(() => {
      expect(spy).toHaveBeenCalled();
    });
    // Dialog should NOT close on failure
    expect(onClose).not.toHaveBeenCalled();
    spy.mockRestore();
  });

  it('refuses to submit after the active project changes', async () => {
    useProjectStore.setState({
      project: {
        metadata: { project_id: 'p1', project_name: 'Project A', created_at: '', modified_at: '' },
        workspace: { bed_width_mm: 400, bed_height_mm: 400 },
        layers: [],
        objects: [{
          id: 'obj-1',
          name: 'Shape A',
          visible: true,
          locked: false,
          transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
          bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
          layer_id: 'l1',
          z_index: 0,
          data: { type: 'shape', shape_type: 'rect' },
        }],
      } as never,
    });

    const spy = vi.spyOn(useProjectStore.getState(), 'gridArray').mockResolvedValue(undefined);
    const onClose = vi.fn();
    render(<GridArrayDialog objectIds={['obj-1']} onClose={onClose} />);

    useProjectStore.setState({
      project: {
        metadata: { project_id: 'p2', project_name: 'Project B', created_at: '', modified_at: '' },
        workspace: { bed_width_mm: 400, bed_height_mm: 400 },
        layers: [],
        objects: [],
      } as never,
    });

    fireEvent.click(screen.getByTestId('grid-array-submit'));

    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
    expect(spy).not.toHaveBeenCalled();
    spy.mockRestore();
  });
});

describe('CircularArrayDialog', () => {
  it('renders all form fields', () => {
    render(<CircularArrayDialog objectIds={['obj-1']} onClose={vi.fn()} />);
    expect(screen.getByText('Count')).toBeDefined();
    expect(screen.getByText('Radius (mm)')).toBeDefined();
    expect(screen.getByText('Rotate Copies')).toBeDefined();
  });

  it('renders new center and angle options', () => {
    render(<CircularArrayDialog objectIds={['obj-1']} onClose={vi.fn()} />);
    expect(screen.getByText('Center Mode')).toBeDefined();
    expect(screen.getByText('Start Angle (deg)')).toBeDefined();
    expect(screen.getByText('End Angle (deg)')).toBeDefined();
    expect(screen.getByText('Group Results')).toBeDefined();
  });

  it('submit calls circularArray with correct params', async () => {
    const spy = vi.spyOn(useProjectStore.getState(), 'circularArray').mockResolvedValue(undefined);
    const onClose = vi.fn();
    render(<CircularArrayDialog objectIds={['obj-1']} onClose={onClose} />);

    fireEvent.click(screen.getByTestId('circular-array-submit'));

    await waitFor(() => {
      expect(spy).toHaveBeenCalledWith(expect.objectContaining({
        objectIds: ['obj-1'],
        count: 6,
        radiusMm: 50,
        rotateCopies: true,
      }));
    });
    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
    spy.mockRestore();
  });

  it('keeps dialog open when circularArray fails', async () => {
    const spy = vi.spyOn(useProjectStore.getState(), 'circularArray').mockRejectedValue(new Error('fail'));
    const onClose = vi.fn();
    render(<CircularArrayDialog objectIds={['obj-1']} onClose={onClose} />);

    fireEvent.click(screen.getByTestId('circular-array-submit'));

    await waitFor(() => {
      expect(spy).toHaveBeenCalled();
    });
    expect(onClose).not.toHaveBeenCalled();
    spy.mockRestore();
  });

  it('disables "Object as center" with single selection', () => {
    render(<CircularArrayDialog objectIds={['obj-1']} onClose={vi.fn()} />);
    const select = screen.getByDisplayValue('Auto from selection');
    const options = select.querySelectorAll('option');
    const chooseOption = Array.from(options).find((o) => o.value === 'chooseObject');
    expect(chooseOption).toBeDefined();
    expect(chooseOption!.disabled).toBe(true);
  });

  it('enables "Object as center" with multi selection', () => {
    render(<CircularArrayDialog objectIds={['obj-1', 'obj-2']} onClose={vi.fn()} />);
    const select = screen.getByDisplayValue('Auto from selection');
    const options = select.querySelectorAll('option');
    const chooseOption = Array.from(options).find((o) => o.value === 'chooseObject');
    expect(chooseOption).toBeDefined();
    expect(chooseOption!.disabled).toBe(false);
  });

  it('chooseObject mode submits centerObjectId', async () => {
    const spy = vi.spyOn(useProjectStore.getState(), 'circularArray').mockResolvedValue(undefined);
    const onClose = vi.fn();
    render(<CircularArrayDialog objectIds={['obj-1', 'obj-2']} onClose={onClose} />);

    // Switch to "Object as center" mode
    const modeSelect = screen.getByDisplayValue('Auto from selection');
    fireEvent.change(modeSelect, { target: { value: 'chooseObject' } });

    // Pick obj-1 as center (obj-2 is default since it's last in array)
    const centerSelect = screen.getByTestId('center-object-select');
    fireEvent.change(centerSelect, { target: { value: 'obj-1' } });

    fireEvent.click(screen.getByTestId('circular-array-submit'));

    await waitFor(() => {
      expect(spy).toHaveBeenCalledWith(expect.objectContaining({
        centerObjectId: 'obj-1',
      }));
    });
    spy.mockRestore();
  });

  it('refuses to submit after the active project changes', async () => {
    useProjectStore.setState({
      project: {
        metadata: { project_id: 'p1', project_name: 'Project A', created_at: '', modified_at: '' },
        workspace: { bed_width_mm: 400, bed_height_mm: 400 },
        layers: [],
        objects: [{
          id: 'obj-1',
          name: 'Shape A',
          visible: true,
          locked: false,
          transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
          bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
          layer_id: 'l1',
          z_index: 0,
          data: { type: 'shape', shape_type: 'rect' },
        }],
      } as never,
    });

    const spy = vi.spyOn(useProjectStore.getState(), 'circularArray').mockResolvedValue(undefined);
    const onClose = vi.fn();
    render(<CircularArrayDialog objectIds={['obj-1']} onClose={onClose} />);

    useProjectStore.setState({
      project: {
        metadata: { project_id: 'p2', project_name: 'Project B', created_at: '', modified_at: '' },
        workspace: { bed_width_mm: 400, bed_height_mm: 400 },
        layers: [],
        objects: [],
      } as never,
    });

    fireEvent.click(screen.getByTestId('circular-array-submit'));

    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
    expect(spy).not.toHaveBeenCalled();
    spy.mockRestore();
  });
});

describe('CopyAlongPathDialog', () => {
  it('renders all form fields', () => {
    render(<CopyAlongPathDialog objectIds={['obj-1']} pathObjectId="path-1" onClose={vi.fn()} />);
    expect(screen.getByText('Number of Copies')).toBeDefined();
    expect(screen.getByText('Rotate Copies')).toBeDefined();
    expect(screen.getByText('Scale Copies')).toBeDefined();
    expect(screen.getByText('Final Scale %')).toBeDefined();
  });

  it('submit calls copyAlongPath with dialog options', async () => {
    const spy = vi.spyOn(useProjectStore.getState(), 'copyAlongPath').mockResolvedValue(true);
    const onClose = vi.fn();
    render(<CopyAlongPathDialog objectIds={['obj-1']} pathObjectId="path-1" onClose={onClose} />);

    fireEvent.click(screen.getByTestId('copy-along-path-submit'));

    await waitFor(() => {
      expect(spy).toHaveBeenCalledWith(['obj-1'], 'path-1', {
        count: 6,
        rotateCopies: true,
        scaleCopies: false,
        finalScalePercent: 100,
      });
    });
    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
    spy.mockRestore();
  });

  it('auto-closes when the active project changes', async () => {
    useProjectStore.setState({
      project: {
        metadata: { project_id: 'p1', project_name: 'Project A', created_at: '', modified_at: '' },
        workspace: { bed_width_mm: 400, bed_height_mm: 400 },
        layers: [],
        objects: [],
      } as never,
    });
    const onClose = vi.fn();
    render(<CopyAlongPathDialog objectIds={['obj-1']} pathObjectId="path-1" onClose={onClose} />);

    useProjectStore.setState({
      project: {
        metadata: { project_id: 'p2', project_name: 'Project B', created_at: '', modified_at: '' },
        workspace: { bed_width_mm: 400, bed_height_mm: 400 },
        layers: [],
        objects: [],
      } as never,
    });

    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
  });
});
