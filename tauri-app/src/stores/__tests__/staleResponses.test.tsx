import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { cleanup } from '@testing-library/react';
import { useProjectStore } from '../projectStore';
import { useMachineStore } from '../machineStore';
import { useNotificationStore } from '../notificationStore';
import { useUndoStore } from '../undoStore';
import { projectService } from '../../services/projectService';
import { persistenceService } from '../../services/persistenceService';
import { machineService } from '../../services/machineService';
import { makeProject, makeProjectObject } from '../../test-utils/projectFixtures';
import { isUserCancel } from '../../utils/userCancel';
import i18n from '../../i18n';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const initialProject = useProjectStore.getState();
const initialMachine = useMachineStore.getState();
const initialNotifications = useNotificationStore.getState();
const initialUndo = useUndoStore.getState();

beforeEach(() => {
  vi.spyOn(useUndoStore.getState(), 'refresh').mockResolvedValue(undefined);
  useNotificationStore.setState({ notifications: [] });
});

afterEach(async () => {
  cleanup();
  vi.restoreAllMocks();
  useProjectStore.setState(initialProject, true);
  useMachineStore.setState(initialMachine, true);
  useNotificationStore.setState(initialNotifications, true);
  useUndoStore.setState(initialUndo, true);
  await i18n.changeLanguage('en');
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => { resolve = r; });
  return { promise, resolve };
}

it('a refresh that arrives after opening another project does not bring the old one back', async () => {
  const old = makeProject({ metadata: { ...makeProject().metadata, project_id: 'old-doc' } });
  const next = makeProject({ metadata: { ...makeProject().metadata, project_id: 'new-doc' } });
  useProjectStore.setState({ project: old, projectPath: '/old.lzrproj' });
  const pending = deferred<typeof old>();
  vi.spyOn(projectService, 'getProject').mockReturnValueOnce(pending.promise);
  vi.spyOn(persistenceService, 'openProjectFromPath').mockResolvedValue(next);
  const refresh = useProjectStore.getState().loadProject();
  await useProjectStore.getState().openProjectFromPath('/new.lzrproj');
  pending.resolve(old);
  await refresh;
  expect(useProjectStore.getState().project?.metadata.project_id).toBe('new-doc');
  expect(useProjectStore.getState().projectPath).toBe('/new.lzrproj');
});

it('refreshing keeps a rotated vector rotated', async () => {
  const transform = { a: 0, b: 1, c: -1, d: 0, tx: 100, ty: 50 };
  const object = makeProjectObject({
    data: { type: 'vector_path', path_data: 'M0 0 L10 0 L10 10 Z', closed: true },
    transform,
    bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
  });
  vi.spyOn(projectService, 'getProject').mockResolvedValue(makeProject({ objects: [object] }));
  await useProjectStore.getState().loadProject();
  expect(useProjectStore.getState().project?.objects[0].transform).toEqual(transform);
});

it('a status poll answered before disconnecting cannot make the machine Ready again', async () => {
  useMachineStore.setState({ sessionState: 'ready', connectionPreview: false });
  const pending = deferred<'ready'>();
  vi.spyOn(machineService, 'getSessionState').mockReturnValueOnce(pending.promise);
  vi.spyOn(machineService, 'disconnect').mockResolvedValue(undefined);
  const poll = useMachineStore.getState().refreshSessionState();
  await useMachineStore.getState().disconnect();
  pending.resolve('ready');
  await poll;
  expect(useMachineStore.getState().sessionState).toBe('disconnected');
});

it('a refresh during a nudge does not move the object twice', async () => {
  const original = makeProjectObject({ id: 'obj', bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } } });
  useProjectStore.setState({ project: makeProject({ objects: [original] }) });
  const pending = deferred<void>();
  vi.spyOn(projectService, 'nudgeObjects').mockReturnValueOnce(pending.promise);
  const nudge = useProjectStore.getState().nudgeObjects(['obj'], 5, 0);
  vi.spyOn(projectService, 'getProject').mockResolvedValue(
    makeProject({ objects: [{ ...original, bounds: { min: { x: 5, y: 0 }, max: { x: 15, y: 10 } } }] }),
  );
  await useProjectStore.getState().loadProject();
  pending.resolve();
  await nudge;
  expect(useProjectStore.getState().project?.objects[0].bounds.min.x).toBe(5);
});

it('a save failure that mentions "cancelled" is still reported', async () => {
  useProjectStore.setState({ project: makeProject({ dirty: true }), error: null });
  vi.spyOn(persistenceService, 'saveProject').mockRejectedValue(
    new Error('Write failed: device cancelled the I/O request'),
  );
  await useProjectStore.getState().saveProject();
  expect(useProjectStore.getState().error).toEqual(expect.stringContaining('Write failed'));
  expect(isUserCancel('Error: Save cancelled')).toBe(true);
  expect(isUserCancel('Plan generation cancelled')).toBe(true);
  expect(isUserCancel('Machine discovery cancelled by the device')).toBe(false);
});

it('the Emergency Stop confirmation is translated', async () => {
  await i18n.changeLanguage('es-ES');
  vi.spyOn(machineService, 'emergencyStop').mockResolvedValue(undefined);
  vi.spyOn(machineService, 'getSessionState').mockResolvedValue('ready');
  await useMachineStore.getState().emergencyStop();
  const notifications = useNotificationStore.getState().notifications;
  const message = notifications[notifications.length - 1]?.message;
  expect(message).toBe(i18n.t('notifications.machine.estop_sent'));
  expect(message).not.toBe('Emergency stop sent');
});
