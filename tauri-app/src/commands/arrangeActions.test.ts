import { afterEach, describe, expect, it, vi } from 'vitest';
import { nestSelected } from './arrangeActions';
import { projectService } from '../services/projectService';
import { useNotificationStore } from '../stores/notificationStore';
import { useProjectStore } from '../stores/projectStore';
import { useUiStore } from '../stores/uiStore';

vi.mock('../services/projectService', () => ({
  projectService: {
    nestSelected: vi.fn(),
  },
}));

const initialProjectState = useProjectStore.getState();
const initialNotificationState = useNotificationStore.getState();
const initialUiState = useUiStore.getState();

afterEach(() => {
  vi.mocked(projectService.nestSelected).mockReset();
  useProjectStore.setState(initialProjectState, true);
  useNotificationStore.setState(initialNotificationState, true);
  useUiStore.setState(initialUiState, true);
});

describe('arrangeActions Nest Selected', () => {
  it('runs in-app nesting, refetches the project, and selects placed roots', async () => {
    vi.mocked(projectService.nestSelected).mockResolvedValue({
      targetContainerId: 'container',
      placedObjectIds: ['a', 'b'],
      unplacedObjectIds: [],
      utilization: 0.4,
      elapsedMs: 12,
    });
    const loadProject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ selectedObjectIds: ['container', 'a', 'b'], loadProject });

    await nestSelected();

    expect(projectService.nestSelected).toHaveBeenCalledWith(
      ['container', 'a', 'b'],
      useUiStore.getState().nestSettings,
    );
    expect(loadProject).toHaveBeenCalledWith({ invalidatePreview: true });
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['a', 'b']);
    const notifications = useNotificationStore.getState().notifications;
    expect(notifications[notifications.length - 1]).toMatchObject({
      message: 'Nested 2 objects.',
      type: 'success',
    });
    expect(useUiStore.getState().nestingInProgress).toBe(false);
  });

  it('uses dialog-provided object ids and settings when supplied', async () => {
    vi.mocked(projectService.nestSelected).mockResolvedValue({
      targetContainerId: 'container',
      placedObjectIds: ['part'],
      unplacedObjectIds: [],
      utilization: 0.5,
      elapsedMs: 15,
    });
    const loadProject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ selectedObjectIds: ['other'], loadProject });
    const settings = {
      ...useUiStore.getState().nestSettings,
      paddingMm: 2.5,
      allowMirror: true,
    };

    await nestSelected(settings, ['container', 'part']);

    expect(projectService.nestSelected).toHaveBeenCalledWith(['container', 'part'], settings);
    expect(loadProject).toHaveBeenCalledWith({ invalidatePreview: true });
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['part']);
  });

  it('treats partial native results as contract failures without refetching', async () => {
    vi.mocked(projectService.nestSelected).mockResolvedValue({
      targetContainerId: 'container',
      placedObjectIds: ['a'],
      unplacedObjectIds: ['b'],
      utilization: 0.2,
      elapsedMs: 20,
    });
    const loadProject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ selectedObjectIds: ['container', 'a', 'b'], loadProject });

    await nestSelected();

    expect(loadProject).not.toHaveBeenCalled();
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['container', 'a', 'b']);
    const notifications = useNotificationStore.getState().notifications;
    expect(notifications[notifications.length - 1]).toMatchObject({
      message: 'Nesting could not fit 1 object(s) inside the selected container',
      type: 'error',
    });
  });

  it('shows backend validation errors without refetching or leaking external workflow details', async () => {
    vi.mocked(projectService.nestSelected).mockRejectedValue({
      code: 'no_container',
      message: 'Select a closed vector-compatible container for nesting',
    });
    const loadProject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ selectedObjectIds: ['a', 'b'], loadProject });

    await nestSelected();

    expect(loadProject).not.toHaveBeenCalled();
    const notifications = useNotificationStore.getState().notifications;
    expect(notifications[notifications.length - 1]).toMatchObject({
      message: 'Select a closed vector-compatible container for nesting',
      type: 'error',
    });
    expect(useUiStore.getState().nestingInProgress).toBe(false);
  });

  it('formats structured unplaced errors with object names', async () => {
    vi.mocked(projectService.nestSelected).mockRejectedValue({
      code: 'unplaced',
      message: 'Nesting could not fit 1 object(s) inside the selected container',
      unplacedObjectIds: ['b'],
    });
    const loadProject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      selectedObjectIds: ['container', 'a', 'b'],
      loadProject,
      project: {
        objects: [{ id: 'b', name: 'Large star' }],
      } as never,
    });

    await nestSelected();

    expect(loadProject).not.toHaveBeenCalled();
    const notifications = useNotificationStore.getState().notifications;
    expect(notifications[notifications.length - 1]).toMatchObject({
      message: 'Nesting could not fit 1 object(s) inside the selected container\nAffected: Large star',
      type: 'error',
    });
  });

  it('handles an invalid native return shape without crashing selection state', async () => {
    vi.mocked(projectService.nestSelected).mockResolvedValue('/tmp/old-native-result.svg' as never);
    const loadProject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ selectedObjectIds: ['a', 'b'], loadProject });

    await nestSelected();

    expect(loadProject).not.toHaveBeenCalled();
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['a', 'b']);
    const notifications = useNotificationStore.getState().notifications;
    expect(notifications[notifications.length - 1]).toMatchObject({
      message: 'Nest Selected returned an invalid result.',
      type: 'error',
    });
    expect(useUiStore.getState().nestingInProgress).toBe(false);
  });

  it('notifies instead of silently no-oping when nothing is selected', async () => {
    useProjectStore.setState({ selectedObjectIds: [] });

    await nestSelected();

    expect(projectService.nestSelected).not.toHaveBeenCalled();
    const notifications = useNotificationStore.getState().notifications;
    expect(notifications[notifications.length - 1]).toMatchObject({
      message: 'Select objects before nesting.',
      type: 'info',
    });
  });

  it('does not queue another nest while one is already pending', async () => {
    useProjectStore.setState({ selectedObjectIds: ['container', 'part'] });
    useUiStore.getState().setNestingInProgress(true);

    await nestSelected();

    expect(projectService.nestSelected).not.toHaveBeenCalled();
    expect(useUiStore.getState().nestingInProgress).toBe(true);
    expect(useNotificationStore.getState().notifications).toEqual([]);
  });
});
