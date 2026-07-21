import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { DockDialog } from '../DockDialog';
import { useProjectStore } from '../../../stores/projectStore';
import { useUiStore } from '../../../stores/uiStore';
import { makeProject } from '../../../test-utils/projectFixtures';

const initialProjectState = useProjectStore.getState();
const initialUiState = useUiStore.getState();

afterEach(() => {
  cleanup();
  useProjectStore.setState(initialProjectState, true);
  useUiStore.setState(initialUiState, true);
  vi.restoreAllMocks();
});

describe('DockDialog', () => {
  it('persists dock settings while editing, even before apply', async () => {
    useProjectStore.setState({
      project: makeProject(),
      dockObjects: vi.fn().mockResolvedValue(true),
    });
    const onClose = vi.fn();
    const { unmount } = render(<DockDialog objectIds={['obj-1']} onClose={onClose} />);

    const padding = screen.getByLabelText('Padding (mm)') as HTMLInputElement;
    fireEvent.change(padding, { target: { value: '4.5' } });
    fireEvent.click(screen.getByLabelText('Move as group'));

    await waitFor(() => {
      expect(useUiStore.getState().dockSettings.paddingMm).toBe(4.5);
      expect(useUiStore.getState().dockSettings.moveAsGroup).toBe(true);
    });

    unmount();
    render(<DockDialog objectIds={['obj-1']} onClose={onClose} />);
    expect((screen.getByLabelText('Padding (mm)') as HTMLInputElement).value).toBe('4.5');
    expect((screen.getByLabelText('Move as group') as HTMLInputElement).checked).toBe(true);
  });

  it('stays open when the dock action reports failure', async () => {
    const dockObjects = vi.fn().mockResolvedValue(false);
    useProjectStore.setState({
      project: makeProject(),
      dockObjects,
    });
    const onClose = vi.fn();
    render(<DockDialog objectIds={['obj-1']} onClose={onClose} />);

    fireEvent.click(screen.getByText('Dock Left'));

    await waitFor(() => expect(dockObjects).toHaveBeenCalled());
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByText('Dock')).toBeDefined();
  });

  it('closes when the active project changes', async () => {
    useProjectStore.setState({ project: makeProject() });
    const onClose = vi.fn();
    render(<DockDialog objectIds={['obj-1']} onClose={onClose} />);

    await act(async () => {
      useProjectStore.setState({
        project: makeProject({
          metadata: {
            format_version: '1',
            app_version: '0.1.0',
            project_id: 'other',
            project_name: 'Other Project',
            created_at: '',
            modified_at: '',
          },
        }),
      });
    });

    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });
});
