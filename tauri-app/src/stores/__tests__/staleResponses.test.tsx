import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { cleanup } from '@testing-library/react';
import { useProjectStore } from '../projectStore';
import { useMachineStore } from '../machineStore';
import { useNotificationStore } from '../notificationStore';
import { useUndoStore } from '../undoStore';
import { projectService } from '../../services/projectService';
import { persistenceService } from '../../services/persistenceService';
import { machineService } from '../../services/machineService';
import { materialService } from '../../services/materialService';
import { vectorService } from '../../services/vectorService';
import { useMaterialStore } from '../materialStore';
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

it.each(['saveProject', 'saveProjectAs'] as const)(
  'late %s completion cannot change another document save path',
  async (action) => {
    useProjectStore.setState({
      project: makeProject(),
      projectPath: '/old.lzrproj',
    });
    const pending = deferred<string>();
    vi.spyOn(persistenceService, action).mockReturnValueOnce(pending.promise);
    vi.spyOn(persistenceService, 'openProjectFromPath').mockResolvedValue(
      makeProject({
        metadata: { ...makeProject().metadata, project_id: 'new-doc' },
      }),
    );
    const saving = useProjectStore.getState()[action]();
    await useProjectStore.getState().openProjectFromPath('/new.lzrproj');
    pending.resolve('/old-saved.lzrproj');
    await saving;
    expect(useProjectStore.getState().projectPath).toBe('/new.lzrproj');
  },
);
it('late runtime capabilities cannot repopulate a disconnected machine', async () => {
  useMachineStore.setState({ sessionState: 'ready', connectionPreview: false });
  const pending =
    deferred<
      Awaited<ReturnType<typeof machineService.getMachineRuntimeState>>
    >();
  vi.spyOn(machineService, 'getMachineRuntimeState').mockReturnValueOnce(
    pending.promise,
  );
  vi.spyOn(machineService, 'disconnect').mockResolvedValue(undefined);
  const loading = useMachineStore.getState().loadRuntimeCapabilities();
  await useMachineStore.getState().disconnect();
  pending.resolve({ capabilities: { can_jog: true } } as Awaited<
    ReturnType<typeof machineService.getMachineRuntimeState>
  >);
  await loading;
  expect(useMachineStore.getState().capabilities).toBeNull();
});
it('backend disconnect state invalidates a pending session poll', async () => {
  useMachineStore.setState({ sessionState: 'ready', connectionPreview: false });
  const pending = deferred<'ready'>();
  vi.spyOn(machineService, 'getSessionState').mockReturnValueOnce(
    pending.promise,
  );
  const polling = useMachineStore.getState().refreshSessionState();
  useMachineStore.setState({
    sessionState: 'disconnected',
    machineStatus: null,
    capabilities: null,
  });
  pending.resolve('ready');
  await polling;
  expect(useMachineStore.getState().sessionState).toBe('disconnected');
});

it('superseded boolean refresh does not dirty or select into another document', async () => {
  const { vectorService } = await import('../../services/vectorService');
  useProjectStore.setState({ project: makeProject(), booleanPending: false });
  vi.spyOn(vectorService, 'booleanUnion').mockResolvedValue(
    makeProjectObject({ id: 'old-result' }),
  );
  const pending = deferred<ReturnType<typeof makeProject>>();
  vi.spyOn(projectService, 'getProject').mockReturnValueOnce(pending.promise);
  const edit = useProjectStore.getState().booleanUnion('a', 'b');
  await vi.waitFor(() => expect(projectService.getProject).toHaveBeenCalled());
  const next = makeProject({
    dirty: false,
    metadata: { ...makeProject().metadata, project_id: 'new-doc' },
  });
  vi.spyOn(persistenceService, 'openProjectFromPath').mockResolvedValue(next);
  await useProjectStore.getState().openProjectFromPath('/new.lzrproj');
  pending.resolve(makeProject());
  await edit;
  expect(useProjectStore.getState().project?.dirty).toBe(false);
  expect(useProjectStore.getState().selectedObjectIds).toEqual([]);
});

it('an edit finishing after replacement cannot start a refresh for the new document', async () => {
  const { vectorService } = await import('../../services/vectorService');
  useProjectStore.setState({ project: makeProject(), booleanPending: false });
  const pending = deferred<ReturnType<typeof makeProjectObject>>();
  vi.spyOn(vectorService, 'booleanUnion').mockReturnValueOnce(pending.promise);
  const fetching = vi
    .spyOn(projectService, 'getProject')
    .mockResolvedValue(makeProject());
  const edit = useProjectStore.getState().booleanUnion('a', 'b');
  vi.spyOn(persistenceService, 'openProjectFromPath').mockResolvedValue(
    makeProject({
      dirty: false,
      metadata: { ...makeProject().metadata, project_id: 'new-doc' },
    }),
  );
  await useProjectStore.getState().openProjectFromPath('/new.lzrproj');
  pending.resolve(makeProjectObject({ id: 'old-result' }));
  await edit;
  expect(fetching).not.toHaveBeenCalled();
  expect(useProjectStore.getState().project?.metadata.project_id).toBe(
    'new-doc',
  );
  expect(useProjectStore.getState().project?.dirty).toBe(false);
});

it.each(['undo', 'redo'] as const)(
  'late %s cannot replace a newly opened document',
  async (action) => {
    useProjectStore.setState({ project: makeProject() });
    const pending = deferred<ReturnType<typeof makeProject>>();
    vi.spyOn(
      projectService,
      action === 'undo' ? 'undoProject' : 'redoProject',
    ).mockReturnValueOnce(pending.promise);
    const history = useUndoStore.getState()[action]();
    vi.spyOn(persistenceService, 'openProjectFromPath').mockResolvedValue(
      makeProject({
        metadata: { ...makeProject().metadata, project_id: 'new-doc' },
      }),
    );
    await useProjectStore.getState().openProjectFromPath('/new.lzrproj');
    pending.resolve(
      makeProject({
        metadata: { ...makeProject().metadata, project_id: 'old-doc' },
      }),
    );
    await history;
    expect(useProjectStore.getState().project?.metadata.project_id).toBe(
      'new-doc',
    );
  },
);

async function openOtherDocument() {
  const next = makeProject({
    dirty: false,
    objects: [makeProjectObject({ id: 'shared', name: 'New object' })],
    metadata: { ...makeProject().metadata, project_id: 'new-doc' },
  });
  vi.spyOn(persistenceService, 'openProjectFromPath').mockResolvedValue(next);
  await useProjectStore.getState().openProjectFromPath('/new.lzrproj');
}

it('a late object update never replaces a same-ID object in the next document', async () => {
  useProjectStore.setState({ project: makeProject({ objects: [makeProjectObject({ id: 'shared' })] }) });
  const pending = deferred<ReturnType<typeof makeProjectObject>>();
  vi.spyOn(projectService, 'updateObject').mockReturnValue(pending.promise);
  const action = useProjectStore.getState().updateObject('shared', { name: 'Old edit' });
  await openOtherDocument();
  pending.resolve(makeProjectObject({ id: 'shared', name: 'Old edit' }));
  await action;
  const state = useProjectStore.getState();
  expect(state.project?.objects[0].name).toBe('New object');
  expect(state.project?.dirty).toBe(false);
  expect(state.error).toBeNull();
  expect(useNotificationStore.getState().notifications).toEqual([]);
});

it('a late vector edit failure is not reported against the next document', async () => {
  useProjectStore.setState({ project: makeProject({ objects: [makeProjectObject({ id: 'shared' })] }) });
  let reject!: (error: unknown) => void;
  vi.spyOn(vectorService, 'closePath').mockReturnValue(new Promise((_, r) => { reject = r; }));
  const action = useProjectStore.getState().closePath('shared');
  await openOtherDocument();
  reject(new Error('Object not found'));
  await action;
  expect(useProjectStore.getState().error).toBeNull();
  expect(useNotificationStore.getState().notifications).toEqual([]);
});

it('a late ruler guide never brings back the previous document', async () => {
  useProjectStore.setState({ project: makeProject({ metadata: { ...makeProject().metadata, project_id: 'old-doc' } }) });
  const pending = deferred<Awaited<ReturnType<typeof projectService.addObjectAtomic>>>();
  vi.spyOn(projectService, 'addObjectAtomic').mockReturnValue(pending.promise);
  const action = useProjectStore.getState().addRulerGuide('vertical', 10);
  await openOtherDocument();
  pending.resolve({ object: makeProjectObject({ id: 'guide' }), createdLayer: null });
  await action;
  expect(useProjectStore.getState().project?.metadata.project_id).toBe('new-doc');
  expect(useProjectStore.getState().project?.objects.map((o) => o.id)).toEqual(['shared']);
});

it('a late material preset never dirties the next document', async () => {
  useProjectStore.setState({ project: makeProject() });
  const pending = deferred<Awaited<ReturnType<typeof materialService.applyPreset>>>();
  vi.spyOn(materialService, 'applyPreset').mockReturnValue(pending.promise);
  const action = useMaterialStore.getState().applyPreset('preset', 'layer-1');
  await openOtherDocument();
  const next = useProjectStore.getState().project!;
  vi.spyOn(projectService, 'getProject').mockResolvedValue(next);
  pending.resolve({ applied_layer_id: 'layer-1', targeted_entry_id: 'entry-1', warnings: [] });
  await action;
  expect(useProjectStore.getState().project?.dirty).toBe(false);
  expect(useNotificationStore.getState().notifications).toEqual([]);
});

it('simultaneous edit and open replies must not merge old object into new document',async()=>{
  useProjectStore.setState({project:makeProject({objects:[makeProjectObject({id:'shared'})]})});
  vi.spyOn(useUndoStore.getState(),'refresh').mockResolvedValue(undefined);
  const edit=deferred<ReturnType<typeof makeProjectObject>>();
  const opening=deferred<ReturnType<typeof makeProject>>();
  vi.spyOn(projectService,'updateObject').mockReturnValue(edit.promise);
  vi.spyOn(persistenceService,'openProjectFromPath').mockReturnValue(opening.promise);
  const action=useProjectStore.getState().updateObject('shared',{name:'Old edit'});
  const open=useProjectStore.getState().openProjectFromPath('/new.lzrproj');
  edit.resolve(makeProjectObject({id:'shared',name:'Old edit'}));
  opening.resolve(makeProject({dirty:false,objects:[makeProjectObject({id:'shared',name:'New object'})],metadata:{...makeProject().metadata,project_id:'new-doc'}}));
  await Promise.all([action,open]);
  expect(useProjectStore.getState().project?.objects[0].name).toBe('New object');
});

it('simultaneous guide and open replies cannot resurrect the previous document', async () => {
  useProjectStore.setState({project:makeProject()});
  const guide = deferred<Awaited<ReturnType<typeof projectService.addObjectAtomic>>>();
  const opening = deferred<ReturnType<typeof makeProject>>();
  vi.spyOn(projectService,'addObjectAtomic').mockReturnValueOnce(guide.promise);
  vi.spyOn(persistenceService,'openProjectFromPath').mockReturnValueOnce(opening.promise);
  const action = useProjectStore.getState().addRulerGuide('vertical',10);
  const open = useProjectStore.getState().openProjectFromPath('/new.lzrproj');
  guide.resolve({object:makeProjectObject({id:'old-guide'}),createdLayer:null});
  opening.resolve(makeProject({dirty:false,metadata:{...makeProject().metadata,project_id:'new-doc'}}));
  await Promise.all([action,open]);
  expect(useProjectStore.getState().project?.metadata.project_id).toBe('new-doc');
  expect(useProjectStore.getState().project?.dirty).toBe(false);
  expect(useProjectStore.getState().selectedObjectIds).toEqual([]);
});
