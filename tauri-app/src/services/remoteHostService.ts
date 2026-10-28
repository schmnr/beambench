import { invoke } from './invoke';
import type { RemoteReview, RemoteSnapshot } from '../stores/remoteHostStore';
import type { SessionJobOptions } from '../types/jobOptions';

export const remoteHostService = {
  settings: () => invoke<{ url: string; configured: boolean }>('remote_host_settings'),
  configure: (url: string, token: string) =>
    invoke<{ url: string; configured: boolean }>('remote_host_configure', { url, token }),
  request: <T = unknown>(path: string, body?: unknown) =>
    invoke<T>('remote_host_request', { path: `/api/v1/${path}`, body: body ?? null }),
  status: async () => {
    const snapshot = await remoteHostService.request<RemoteSnapshot>('remote/status');
    if (snapshot.protocol_version !== 1 || !snapshot.machine || !Array.isArray(snapshot.profiles)) {
      throw new Error(
        'This host does not support the remote desktop protocol. Update Beam Bench on the host.',
      );
    }
    return snapshot;
  },
  review: () => remoteHostService.request<RemoteReview>('remote/review'),
  prepare: (profileId: string, options: SessionJobOptions) =>
    invoke<RemoteReview>('remote_host_prepare', { profileId, options }),
};
