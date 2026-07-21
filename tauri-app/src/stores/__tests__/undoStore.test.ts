import { describe, it, expect, vi, afterEach } from 'vitest';
import { useUndoStore } from '../undoStore';
import { useNotificationStore } from '../notificationStore';
import { useProjectStore } from '../projectStore';
import { projectService } from '../../services/projectService';
import { makeLayer, makeProject } from '../../test-utils/projectFixtures';

const initialUndoState = useUndoStore.getState();
const initialNotificationState = useNotificationStore.getState();
const initialProjectState = useProjectStore.getState();

afterEach(() => {
  vi.restoreAllMocks();
  useUndoStore.setState(initialUndoState, true);
  useNotificationStore.setState(initialNotificationState, true);
  useProjectStore.setState(initialProjectState, true);
});

describe('useUndoStore', () => {
  it('suppresses benign nothing-to-undo failures but refreshes undo flags', async () => {
    const pushSpy = vi.fn();
    useNotificationStore.setState({ push: pushSpy } as never);
    vi.spyOn(projectService, 'undoProject').mockRejectedValue(new Error('Nothing to undo'));
    vi.spyOn(projectService, 'getUndoState').mockResolvedValue({ can_undo: false, can_redo: true });

    await useUndoStore.getState().undo();

    expect(pushSpy).not.toHaveBeenCalled();
    expect(useUndoStore.getState().canUndo).toBe(false);
    expect(useUndoStore.getState().canRedo).toBe(true);
  });

  it('decorates the backend project payload on undo', async () => {
    // Backend payloads can carry layers without entries; decorateProject
    // must synthesize the primary entry just like every other load path.
    const project = makeProject({ layers: [makeLayer({ entries: [] })] });
    vi.spyOn(projectService, 'undoProject').mockResolvedValue(project);
    vi.spyOn(projectService, 'getUndoState').mockResolvedValue({ can_undo: false, can_redo: true });

    await useUndoStore.getState().undo();

    const stored = useProjectStore.getState().project;
    expect(stored?.layers[0]?.entries).toHaveLength(1);
  });

  it('decorates the backend project payload on redo', async () => {
    const project = makeProject({ layers: [makeLayer({ entries: [] })] });
    vi.spyOn(projectService, 'redoProject').mockResolvedValue(project);
    vi.spyOn(projectService, 'getUndoState').mockResolvedValue({ can_undo: true, can_redo: false });

    await useUndoStore.getState().redo();

    const stored = useProjectStore.getState().project;
    expect(stored?.layers[0]?.entries).toHaveLength(1);
  });

  it('notifies on unexpected redo failures and refreshes undo flags', async () => {
    const pushSpy = vi.fn();
    useNotificationStore.setState({ push: pushSpy } as never);
    vi.spyOn(projectService, 'redoProject').mockRejectedValue(new Error('redo drifted'));
    vi.spyOn(projectService, 'getUndoState').mockResolvedValue({ can_undo: true, can_redo: false });

    await useUndoStore.getState().redo();

    expect(pushSpy).toHaveBeenCalledWith(expect.stringContaining('redo drifted'), 'error');
    expect(useUndoStore.getState().canUndo).toBe(true);
    expect(useUndoStore.getState().canRedo).toBe(false);
  });
});
