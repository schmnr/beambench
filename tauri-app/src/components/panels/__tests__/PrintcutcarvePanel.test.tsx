import '@testing-library/jest-dom/vitest';
import { beforeEach, afterEach, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { printcutcarveLibraryService as service } from '../../../services/connectedLibraryService';
import { usePrintcutcarveLibraryStore as store } from '../../../stores/connectedLibraryStore';
import { CraftgineerPanel } from '../CraftgineerPanel';
import type { LibraryFile } from '../../../services/connectedLibraryService';
const project = vi.hoisted(() => ({ importConnectedLibraryFile: vi.fn() }));
vi.mock('../../../services/connectedLibraryService', () => {
  const service = {
    status: vi.fn(),
    collections: vi.fn(),
    files: vi.fn(),
    thumbnail: vi.fn(),
    openWebsite: vi.fn(),
    disconnect: vi.fn(),
  };
  return {
    connectedLibraryService: { status: vi.fn() },
    printcutcarveLibraryService: service,
    libraryService: () => service,
  };
});
vi.mock('../../../stores/projectStore', () => ({
  useProjectStore: Object.assign(() => true, { getState: () => project }),
}));
const status = {
  account: { account: { id: 'buyer', displayName: 'Pack buyer' }, access: 'active' as const },
  remembered: true,
  walletUnavailable: false,
};
const file: LibraryFile = {
  id: 'file-one',
  name: 'Maple.svg',
  mimeType: 'image/svg+xml',
  sizeBytes: 100,
  revision: `sha256:${'a'.repeat(64)}`,
  sourceTool: null,
  createdAt: '2026-10-25T12:00:00Z',
  collection: { id: 'pack-one', name: 'Woodland pack' },
};
beforeEach(() => {
  vi.resetAllMocks();
  vi.stubGlobal(
    'IntersectionObserver',
    class {
      observe() {}
      disconnect() {}
    },
  );
  store.setState({ status, busy: false, error: null, nativeGeneration: -1 });
  vi.mocked(service.status).mockResolvedValue(status);
  vi.mocked(service.openWebsite).mockResolvedValue();
  vi.mocked(service.collections).mockResolvedValue({
    accountId: 'buyer',
    collections: [
      { id: 'pack-one', name: 'Woodland pack', slug: 'woodland-pack', fileCount: 2, preview: file },
    ],
    nextCursor: null,
  });
  vi.mocked(service.files).mockResolvedValue({
    accountId: 'buyer',
    files: [file],
    unavailableFiles: [{ id: 'file-two', name: 'Future.pdf', reason: 'unsupported_media_type' }],
    nextCursor: null,
  });
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
it('browses packs, imports one design through the project pipeline, and explains unavailable files', async () => {
  render(<CraftgineerPanel provider="printcutcarve" />);
  fireEvent.click(await screen.findByRole('button', { name: 'Open' }));
  expect(await screen.findByText('Maple.svg')).toBeTruthy();
  expect(service.files).toHaveBeenCalledWith('', null, 'pack-one');
  expect(screen.getByText('Future.pdf')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Insert' }));
  await waitFor(() =>
    expect(project.importConnectedLibraryFile).toHaveBeenCalledWith('printcutcarve', file),
  );
  fireEvent.click(screen.getAllByRole('button', { name: 'Open on PrintCutCarve' })[0]);
  expect(service.openWebsite).toHaveBeenCalledWith('woodland-pack');
  fireEvent.click(screen.getByRole('button', { name: 'Back to packs' }));
  expect(await screen.findByRole('button', { name: 'Open' })).toBeTruthy();
});
it('keeps the selected pack and search when refreshed', async () => {
  render(<CraftgineerPanel provider="printcutcarve" />);
  fireEvent.click(await screen.findByRole('button', { name: 'Open' }));
  await screen.findByText('Maple.svg');
  fireEvent.change(screen.getByRole('textbox'), { target: { value: 'maple' } });
  await waitFor(() => expect(service.files).toHaveBeenLastCalledWith('maple', null, 'pack-one'));
  await waitFor(() => expect(screen.getByRole('button', { name: 'Refresh' })).not.toBeDisabled());
  fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
  await waitFor(() => expect(service.files).toHaveBeenLastCalledWith('maple', null, 'pack-one'));
  expect(screen.getByRole('textbox')).toHaveValue('maple');
  expect(screen.getByRole('heading', { name: 'Woodland pack' })).toBeTruthy();
});
it('discards a slow listing and its pack selection after switching accounts', async () => {
  let finish!: (listing: Awaited<ReturnType<typeof service.collections>>) => void;
  vi.mocked(service.collections).mockReturnValueOnce(
    new Promise((resolve) => {
      finish = resolve;
    }),
  );
  render(<CraftgineerPanel provider="printcutcarve" />);
  await waitFor(() => expect(service.collections).toHaveBeenCalledOnce());
  act(() =>
    store
      .getState()
      .receive({
        ...status,
        account: { ...status.account, account: { id: 'other', displayName: 'Other buyer' } },
        nativeGeneration: 4,
      }),
  );
  await act(async () =>
    finish({
      accountId: 'buyer',
      collections: [
        { id: 'old', name: 'Private old pack', slug: 'old-pack', fileCount: 1, preview: null },
      ],
      nextCursor: null,
    }),
  );
  expect(screen.queryByText('Private old pack')).toBeNull();
});
it('allows retrying secure-storage cleanup after disconnect has removed the account', async () => {
  store.setState({ status: null, error: 'wallet_cleanup_required' });
  render(<CraftgineerPanel provider="printcutcarve" connections />);
  fireEvent.click(screen.getByRole('button', { name: 'Disconnect' }));
  await waitFor(() => expect(service.disconnect).toHaveBeenCalledOnce());
});
