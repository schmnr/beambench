import { beforeEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { connectedLibraryService as service } from '../../../services/connectedLibraryService';
import { useConnectedLibraryStore as store } from '../../../stores/connectedLibraryStore';
import { CraftgineerPanel } from '../CraftgineerPanel';

vi.mock('../../../services/connectedLibraryService', () => {
  const service = { status: vi.fn() };
  return {
    connectedLibraryService: service,
    printcutcarveLibraryService: { status: vi.fn() },
    libraryService: () => service,
  };
});
vi.mock('../../../stores/projectStore', () => ({ useProjectStore: () => false }));

beforeEach(() => {
  vi.resetAllMocks();
  window.localStorage.clear();
  store.setState({ status: null, busy: false, error: null });
});

it('offers a refresh after offline restoration without asking for another browser login', async () => {
  window.localStorage.setItem('beam-bench.craftgineer.remember', '1');
  vi.mocked(service.status)
    .mockRejectedValueOnce('service_unavailable')
    .mockResolvedValueOnce({
      account: { account: { id: 'one', displayName: 'Maker' }, access: 'active' },
      remembered: true,
      walletUnavailable: false,
    });
  render(<CraftgineerPanel connections />);
  expect((await screen.findByRole('alert')).textContent).toBe(
    'Craftgineer could not be reached. Check your connection and try again.',
  );
  fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
  await waitFor(() => expect(store.getState().status?.remembered).toBe(true));
  expect(service.status).toHaveBeenNthCalledWith(1, true);
  expect(service.status).toHaveBeenNthCalledWith(2, true);
  expect(screen.queryByRole('alert')).toBeNull();
  expect(screen.getByRole('button', { name: 'Disconnect' })).toBeTruthy();
});

it('does not claim a session-only connection when wallet denial restored no account', async () => {
  vi.mocked(service.status).mockResolvedValue({ account: null, remembered: false, walletUnavailable: true });
  render(<CraftgineerPanel connections />);
  await waitFor(() => expect(store.getState().status?.walletUnavailable).toBe(true));
  expect(screen.queryByText(/Connected for this session only/)).toBeNull();
  expect(screen.getByRole('button', { name: 'Connect Craftgineer' })).toBeTruthy();
});
