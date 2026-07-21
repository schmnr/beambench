import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { CloseSelectedPathsWithToleranceDialog } from '../CloseSelectedPathsWithToleranceDialog';
import { projectService } from '../../../services/projectService';
import { useProjectStore } from '../../../stores/projectStore';
import { usePreviewStore } from '../../../stores/previewStore';
import { useUndoStore } from '../../../stores/undoStore';
import { useNotificationStore } from '../../../stores/notificationStore';
import { useAppStore } from '../../../stores/appStore';

vi.mock('../../../services/projectService', () => ({
  projectService: {
    countOpenPathsWithTolerance: vi.fn().mockResolvedValue({
      openShapesFound: 2,
      shapesClosed: 0,
      remainingOpen: 2,
    }),
    closeSelectedPathsWithTolerance: vi.fn().mockResolvedValue({
      openShapesFound: 2,
      shapesClosed: 2,
      remainingOpen: 0,
    }),
  },
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const initialProjectState = useProjectStore.getState();
const initialPreviewState = usePreviewStore.getState();
const initialUndoState = useUndoStore.getState();
const initialNotificationState = useNotificationStore.getState();
const initialAppState = useAppStore.getState();

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  useProjectStore.setState(initialProjectState, true);
  usePreviewStore.setState(initialPreviewState, true);
  useUndoStore.setState(initialUndoState, true);
  useNotificationStore.setState(initialNotificationState, true);
  useAppStore.setState(initialAppState, true);
});

describe('CloseSelectedPathsWithToleranceDialog', () => {
  it('shows the threshold in millimeters by default', async () => {
    render(<CloseSelectedPathsWithToleranceDialog objectIds={['obj-1']} onClose={vi.fn()} />);

    expect(screen.getByText('0.50 mm')).toBeDefined();
    // Slider min/max legend stays in mm.
    expect(screen.getByText('0.01')).toBeDefined();
    expect(screen.getByText('5')).toBeDefined();

    // Let the debounced count settle inside the test to avoid act warnings.
    await waitFor(() => {
      expect(projectService.countOpenPathsWithTolerance).toHaveBeenCalled();
    });
  });

  it('shows the threshold and slider legend in inches when display unit is inches', async () => {
    useAppStore.setState({
      settings: { display_unit: 'inches', speed_time_unit: 'minutes' } as never,
    });

    render(<CloseSelectedPathsWithToleranceDialog objectIds={['obj-1']} onClose={vi.fn()} />);

    // 0.5 mm -> 0.0197 in
    expect(screen.getByText('0.0197 in')).toBeDefined();
    // Legend: 0.01 mm -> 0.0004 in, 5 mm -> 0.1969 in
    expect(screen.getByText('0.0004')).toBeDefined();
    expect(screen.getByText('0.1969')).toBeDefined();

    await waitFor(() => {
      expect(projectService.countOpenPathsWithTolerance).toHaveBeenCalled();
    });
  });

  it('keeps the committed threshold in millimeters regardless of display unit', async () => {
    useAppStore.setState({
      settings: { display_unit: 'inches', speed_time_unit: 'minutes' } as never,
    });
    useProjectStore.setState({ loadProject: vi.fn().mockResolvedValue(undefined) });
    usePreviewStore.setState({ invalidate: vi.fn() });
    useUndoStore.setState({ refresh: vi.fn().mockResolvedValue(undefined) });

    render(<CloseSelectedPathsWithToleranceDialog objectIds={['obj-1']} onClose={vi.fn()} />);

    fireEvent.change(screen.getByLabelText('Distance Threshold'), { target: { value: '2' } });
    fireEvent.click(screen.getByText('Apply'));

    await waitFor(() => {
      expect(projectService.closeSelectedPathsWithTolerance).toHaveBeenCalledWith(
        ['obj-1'],
        2,
        'move_ends_together',
      );
    });
  });
});
