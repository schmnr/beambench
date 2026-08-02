import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { useProjectStore } from '../../../stores/projectStore';
import { makeProject } from '../../../test-utils/projectFixtures';
import { ProjectNotesPanel } from '../ProjectNotesPanel';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));

const initialProjectState = useProjectStore.getState();
const project = (id: string, notes: string) => makeProject({
  metadata: {
    format_version: '1',
    app_version: '0.1.0',
    project_id: id,
    project_name: `Project ${id}`,
    created_at: '',
    modified_at: '',
  },
  notes,
});

afterEach(() => {
  cleanup();
  useProjectStore.setState(initialProjectState, true);
  vi.restoreAllMocks();
});

describe('ProjectNotesPanel', () => {
  it('renders the active project notes and starts with Save disabled', () => {
    useProjectStore.setState({ project: project('p1', 'Existing notes text') });

    render(<ProjectNotesPanel />);

    expect((screen.getByTestId('notes-textarea') as HTMLTextAreaElement).value).toBe('Existing notes text');
    expect((screen.getByTestId('notes-save') as HTMLButtonElement).disabled).toBe(true);
  });

  it('saves an edited note without closing the panel', async () => {
    useProjectStore.setState({ project: project('p1', '') });
    const updateProjectNotes = vi.spyOn(useProjectStore.getState(), 'updateProjectNotes').mockResolvedValue(true);
    render(<ProjectNotesPanel />);

    fireEvent.change(screen.getByTestId('notes-textarea'), { target: { value: 'New project notes' } });
    fireEvent.click(screen.getByTestId('notes-save'));

    await waitFor(() => expect(updateProjectNotes).toHaveBeenCalledWith('New project notes'));
    expect(screen.getByTestId('notes-textarea')).toBeDefined();
  });

  it('supports Ctrl+S while editing notes', async () => {
    useProjectStore.setState({ project: project('p1', '') });
    const updateProjectNotes = vi.spyOn(useProjectStore.getState(), 'updateProjectNotes').mockResolvedValue(true);
    render(<ProjectNotesPanel />);

    const textarea = screen.getByTestId('notes-textarea');
    fireEvent.change(textarea, { target: { value: 'Keyboard save' } });
    fireEvent.keyDown(textarea, { key: 's', ctrlKey: true });

    await waitFor(() => expect(updateProjectNotes).toHaveBeenCalledWith('Keyboard save'));
  });

  it('loads the next project notes instead of carrying over a draft', async () => {
    useProjectStore.setState({ project: project('p1', 'Notes A') });
    render(<ProjectNotesPanel />);
    fireEvent.change(screen.getByTestId('notes-textarea'), { target: { value: 'Draft for A' } });

    act(() => useProjectStore.setState({ project: project('p2', 'Notes B') }));

    await waitFor(() => {
      expect((screen.getByTestId('notes-textarea') as HTMLTextAreaElement).value).toBe('Notes B');
    });
  });

  it('shows the standard empty state without a project', () => {
    useProjectStore.setState({ project: null });
    render(<ProjectNotesPanel />);

    expect(screen.getByText('No project')).toBeDefined();
    expect(screen.queryByTestId('notes-textarea')).toBeNull();
  });
});
