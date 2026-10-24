import { beforeEach, describe, expect, it, vi } from 'vitest';
const mock = vi.hoisted(() => ({
  status: vi.fn(),
  connect: vi.fn(),
  cancel: vi.fn(),
  disconnect: vi.fn(),
}));
vi.mock('../../services/connectedLibraryService', () => ({ connectedLibraryService: mock }));
import { useConnectedLibraryStore as store } from '../connectedLibraryStore';
const account = {
  account: { account: { id: 'one', displayName: 'Maker' }, access: 'active' as const },
  remembered: true,
  walletUnavailable: false,
};
beforeEach(() => {
  vi.resetAllMocks();
  store.setState({ status: null, busy: false, error: null });
});
describe('connected library account boundaries', () => {
  it('discards login completion after cancellation', async () => {
    let finish!: (value: typeof account) => void;
    mock.connect.mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );
    const pending = store.getState().connect(true);
    await store.getState().cancel();
    finish(account);
    await pending;
    expect(store.getState().status).toBeNull();
    expect(store.getState().busy).toBe(false);
  });
  it('drops account state immediately even if remote revocation fails', async () => {
    store.setState({ status: account });
    mock.disconnect.mockRejectedValue('revocation_pending');
    const pending = store.getState().disconnect();
    expect(store.getState().status).toBeNull();
    await pending;
    expect(store.getState().error).toBe('revocation_pending');
  });
});
