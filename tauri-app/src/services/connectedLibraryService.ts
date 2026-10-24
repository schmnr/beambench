import { getDocumentGeneration, requireCurrentDocument } from '../stores/documentGeneration';
import { invoke } from './invoke';
import type { ImportLayer } from './importService';
import type { ProjectObject } from '../types/project';
export interface LibraryFile {
  id: string;
  name: string;
  mimeType: 'image/svg+xml' | 'image/png' | 'image/jpeg' | 'image/webp';
  sizeBytes: number;
  revision: string;
  sourceTool: string | null;
  createdAt: string;
  collection: null;
}
export interface LibraryAccount {
  account: { id: string; displayName: string | null };
  access: 'active' | 'subscription_required';
}
export interface ConnectionStatus {
  account: LibraryAccount | null;
  remembered: boolean;
  walletUnavailable: boolean;
}
export interface LibraryListing {
  accountId: string;
  files: LibraryFile[];
  nextCursor: string | null;
}
export const connectedLibraryService = {
  status: (restore = false) => invoke<ConnectionStatus>('craftgineer_status', { restore }),
  connect: (remember: boolean) => invoke<ConnectionStatus>('craftgineer_connect', { remember }),
  cancel: () => invoke<void>('craftgineer_cancel'),
  disconnect: () => invoke<void>('craftgineer_disconnect'),
  files: (q: string, cursor: string | null = null) =>
    invoke<LibraryListing>('craftgineer_files', { q, cursor }),
  thumbnail: (file: LibraryFile) => invoke<string>('craftgineer_thumbnail', { file }),
  insert: async (
    file: LibraryFile,
    projectId: string,
    layerId: string,
    createLayer?: ImportLayer,
  ) => {
    const generation = getDocumentGeneration();
    const expectedEpoch = requireCurrentDocument(
      generation,
      await invoke<number>('craftgineer_prepare_insert', { projectId }),
    );
    return invoke<ProjectObject[]>('craftgineer_insert', {
      file,
      projectId,
      layerId,
      expectedEpoch,
      requestId: crypto.randomUUID(),
      createLayer: createLayer ?? null,
    });
  },
};
