import { beforeEach, expect, it, vi } from 'vitest';
const mock = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('./invoke', () => mock);
import { connectedLibraryService as service, type LibraryFile } from './connectedLibraryService';
import { beginNewDocument } from '../stores/documentGeneration';
const file: LibraryFile = {
  id: 'saved-file',
  name: 'Artwork',
  mimeType: 'image/svg+xml',
  sizeBytes: 10,
  revision: `sha256:${'a'.repeat(64)}`,
  sourceTool: null,
  createdAt: '2026-10-24T12:00:00Z',
  collection: null,
};
beforeEach(() => {
  vi.resetAllMocks();
});
it('does not begin a download after the frontend document changed while obtaining its lease', async () => {
  let finish!: (epoch: number) => void;
  mock.invoke.mockReturnValue(
    new Promise((resolve) => {
      finish = resolve;
    }),
  );
  const pending = service.insert(file, 'project', 'layer');
  beginNewDocument();
  finish(7);
  await expect(pending).rejects.toThrow('stale_document');
  expect(mock.invoke).toHaveBeenCalledTimes(1);
});
it('carries the native document lease through insertion', async () => {
  mock.invoke.mockResolvedValueOnce(7).mockResolvedValueOnce([]);
  await service.insert(file, 'project', 'layer');
  expect(mock.invoke).toHaveBeenLastCalledWith('craftgineer_insert', {
    file,
    projectId: 'project',
    layerId: 'layer',
    expectedEpoch: 7,
    requestId: expect.any(String),
    createLayer: null,
  });
});
