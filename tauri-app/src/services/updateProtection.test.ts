import {afterEach, expect, it, vi} from 'vitest';
import {check} from '@tauri-apps/plugin-updater';
import {relaunch} from '@tauri-apps/plugin-process';
import {checkForUpdate, clearPendingUpdate, downloadAndInstallUpdate} from './updateService';
import {useProjectStore} from '../stores/projectStore';
import {useUnsavedGuardStore} from '../stores/unsavedGuardStore';
import {makeProject} from '../test-utils/projectFixtures';
vi.mock('@tauri-apps/plugin-updater', () => ({check:vi.fn()}));
vi.mock('@tauri-apps/plugin-process', () => ({relaunch:vi.fn()}));
function updateFixture() { return {currentVersion:'1',version:'2',rawJson:{},download:vi.fn(async()=>{}),install:vi.fn(async()=>{}),close:vi.fn(async()=>{})}; }
afterEach(()=>{clearPendingUpdate();useProjectStore.setState({project:null});useUnsavedGuardStore.getState().clear();vi.clearAllMocks();});
it('keeps dirty work when the update prompt is cancelled', async()=>{
 const update=updateFixture();vi.mocked(check).mockResolvedValue(update as never);
 useProjectStore.setState({project:makeProject({dirty:true})});await checkForUpdate();
 const result=downloadAndInstallUpdate();const rejected=expect(result).rejects.toThrow('cancelled');
 expect(update.download).not.toHaveBeenCalled();
 useUnsavedGuardStore.getState().pendingAction?.cancel?.();await rejected;
 expect(update.install).not.toHaveBeenCalled();expect(relaunch).not.toHaveBeenCalled();
});
it('requires another decision after edits during download', async()=>{
 const update=updateFixture();vi.mocked(check).mockResolvedValue(update as never);
 update.download.mockImplementation(async()=>{useProjectStore.setState({project:makeProject({dirty:true})});});
 useProjectStore.setState({project:makeProject({dirty:true})});await checkForUpdate();
 const result=downloadAndInstallUpdate();const first=useUnsavedGuardStore.getState().pendingAction;
 expect(first).not.toBeNull();await first?.execute();
 await vi.waitFor(()=>expect(useUnsavedGuardStore.getState().pendingAction).not.toBe(first));
 expect(update.install).not.toHaveBeenCalled();await useUnsavedGuardStore.getState().pendingAction?.execute();
 await result;expect(update.install).toHaveBeenCalledOnce();expect(relaunch).toHaveBeenCalledOnce();
});
