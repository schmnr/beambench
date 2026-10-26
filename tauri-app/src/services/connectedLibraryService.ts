import { getDocumentGeneration, requireCurrentDocument } from '../stores/documentGeneration';
import { invoke } from './invoke';
import i18n from '../i18n';
export type LibraryProvider = 'craftgineer' | 'printcutcarve';
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
  collection: { id: string; name: string } | null;
}
export interface LibraryAccount {
  account: { id: string; displayName: string | null };
  access: 'active' | 'subscription_required';
}
export interface ConnectionStatus {
  account: LibraryAccount | null;
  remembered: boolean;
  walletUnavailable: boolean;
  nativeGeneration?: number;
  paidThrough?: string | null;
}
export interface LibraryCollection {
  id: string;
  name: string;
  slug: string;
  fileCount: number;
  preview: LibraryFile | null;
}
export interface LibraryCollections {
  accountId: string;
  collections: LibraryCollection[];
  nextCursor: string | null;
}
export interface LibraryListing {
  accountId: string;
  files: LibraryFile[];
  unavailableFiles?: { id: string; name: string; reason: string }[];
  nextCursor: string | null;
}
function createLibraryService(provider: LibraryProvider) {
  return {
    status: (restore = false) =>
      invoke<ConnectionStatus>('connected_library_status', { provider, restore }),
    connect: (remember: boolean) =>
      invoke<ConnectionStatus>('connected_library_connect', {
        provider,
        remember,
        browserMessage: i18n.t('connected_library.browser_done'),
      }),
    cancel: () => invoke<void>('connected_library_cancel', { provider }),
    disconnect: () => invoke<void>('connected_library_disconnect', { provider }),
    collections: (q: string, cursor: string | null = null) =>
      invoke<LibraryCollections>('connected_library_collections', { provider, q, cursor }),
    files: (q: string, cursor: string | null = null, collection: string | null = null) =>
      invoke<LibraryListing>('connected_library_files', { provider, q, cursor, collection }),
    thumbnail: (file: LibraryFile) =>
      invoke<string>('connected_library_thumbnail', { provider, file }),
    openWebsite: (slug: string | null = null) =>
      invoke<void>('connected_library_open_website', { provider, slug }),
    insert: async (
      file: LibraryFile,
      projectId: string,
      layerId: string,
      createLayer?: ImportLayer,
      requestId = crypto.randomUUID(),
    ) => {
      const generation = getDocumentGeneration();
      const expectedEpoch = requireCurrentDocument(
        generation,
        await invoke<number>('connected_library_prepare_insert', { projectId }),
      );
      return invoke<ProjectObject[]>('connected_library_insert', {
        provider,
        file,
        projectId,
        layerId,
        expectedEpoch,
        requestId,
        createLayer: createLayer ?? null,
      });
    },
  };
}
export const connectedLibraryService = createLibraryService('craftgineer');
export const printcutcarveLibraryService = createLibraryService('printcutcarve');
export function libraryService(provider: LibraryProvider) {
  return provider === 'craftgineer' ? connectedLibraryService : printcutcarveLibraryService;
}
