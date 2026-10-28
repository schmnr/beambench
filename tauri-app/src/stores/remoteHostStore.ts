import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type {
  JobProgress,
  MachineProfile,
  MachineRuntimeState,
  PreflightReport,
} from '../types/machine';
import type { Layer, Workspace } from '../types/project';
import type { PreviewData } from '../types/preview';

export interface RemoteSnapshot {
  protocol_version: number;
  boot_id: string;
  owned: boolean;
  transfer_id: string | null;
  consumed: boolean;
  busy: boolean;
  project_name: string | null;
  machine: MachineRuntimeState;
  progress: JobProgress | null;
  profiles: MachineProfile[];
  active_profile_id: string | null;
}
export interface RemoteReview {
  transfer_id: string;
  project_name: string;
  preview: PreviewData;
  preflight: PreflightReport;
  layers: Layer[];
  workspace: Workspace;
}
interface RemoteState {
  enabled: boolean;
  url: string;
  configured: boolean;
  online: boolean;
  lastSeen: number | null;
  snapshot: RemoteSnapshot | null;
  review: RemoteReview | null;
  busy: boolean;
  error: string | null;
  connectionError: string | null;
}
export const useRemoteHostStore = create<RemoteState>()(
  persist<RemoteState, [], [], { enabled: boolean }>(
    () => ({
      enabled: false,
      url: '',
      configured: false,
      online: false,
      lastSeen: null,
      snapshot: null,
      review: null,
      busy: false,
      error: null,
      connectionError: null,
    }),
    { name: 'beambench-remote-host-mode', partialize: (s) => ({ enabled: s.enabled }) },
  ),
);
export function remoteJobActive(snapshot: RemoteSnapshot | null): boolean {
  return (
    !!snapshot?.busy ||
    ['preparing', 'ready_to_run', 'running', 'paused'].includes(snapshot?.progress?.state ?? '')
  );
}
