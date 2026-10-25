import { beforeEach, describe, expect, it, vi } from 'vitest';
const mock = vi.hoisted(() => ({
  status: vi.fn(),
  connect: vi.fn(),
  cancel: vi.fn(),
  disconnect: vi.fn(),
}));
const pcc = vi.hoisted(() => ({
  status: vi.fn(),
  connect: vi.fn(),
  cancel: vi.fn(),
  disconnect: vi.fn(),
}));
vi.mock('../../services/connectedLibraryService', () => ({
  connectedLibraryService: mock,
  printcutcarveLibraryService: pcc,
}));
import {
  useConnectedLibraryStore as store,
  usePrintcutcarveLibraryStore as pccStore,
} from '../connectedLibraryStore';
const account = {
  account: { account: { id: 'one', displayName: 'Maker' }, access: 'active' as const },
  remembered: true,
  walletUnavailable: false,
};
beforeEach(() => {
  vi.resetAllMocks();
  window.localStorage.clear();
  store.setState({ status: null, busy: false, error: null, nativeGeneration: -1 });
  pccStore.setState({ status: null, busy: false, error: null, nativeGeneration: -1 });
});
describe('connected library account boundaries', () => {
  it('keeps a local disconnect warning when the native event arrives before its reply', async () => {
    store.setState({ status: account });
    mock.disconnect.mockImplementation(async () => {
      store
        .getState()
        .receive({
          account: null,
          remembered: false,
          walletUnavailable: false,
          nativeGeneration: 12,
        });
      throw 'wallet_cleanup_required';
    });
    await store.getState().disconnect();
    expect(store.getState().status?.account).toBeNull();
    expect(store.getState().error).toBe('wallet_cleanup_required');
    expect(store.getState().busy).toBe(false);
  });
  it('applies a native disconnect to every window and rejects older account events', () => {
    store.getState().receive({ ...account, nativeGeneration: 10 });
    pccStore.getState().receive({ ...account, nativeGeneration: 1 });
    store.getState().receive({
      account: null,
      remembered: false,
      walletUnavailable: false,
      nativeGeneration: 11,
    });
    store.getState().receive({ ...account, nativeGeneration: 10 });
    expect(store.getState().status?.account).toBeNull();
    expect(pccStore.getState().status?.account).toEqual(account.account);
    expect(window.localStorage.getItem('beam-bench.craftgineer.remember')).toBe('0');
    expect(window.localStorage.getItem('beam-bench.printcutcarve.remember')).toBe('1');
  });
  it('disconnects PrintCutCarve without cancelling Craftgineer or its remembered preference', async () => {
    store.setState({ status: account });
    pccStore.setState({ status: account });
    window.localStorage.setItem('beam-bench.craftgineer.remember', '1');
    window.localStorage.setItem('beam-bench.printcutcarve.remember', '1');
    await pccStore.getState().disconnect();
    expect(store.getState().status).toEqual(account);
    expect(window.localStorage.getItem('beam-bench.craftgineer.remember')).toBe('1');
    expect(pccStore.getState().status).toBeNull();
    expect(window.localStorage.getItem('beam-bench.printcutcarve.remember')).toBe('0');
    expect(pcc.disconnect).toHaveBeenCalledOnce();
    expect(mock.disconnect).not.toHaveBeenCalled();
  });
  it.each(['service_unavailable', 'rate_limited'])(
    'keeps a remembered login after %s during relaunch and restores on retry',
    async (error) => {
      const key = 'beam-bench.craftgineer.remember';
      window.localStorage.setItem(key, '1');
      mock.status.mockRejectedValueOnce(error).mockResolvedValueOnce(account);
      await store.getState().restore();
      expect(mock.status).toHaveBeenLastCalledWith(true);
      expect(window.localStorage.getItem(key)).toBe('1');
      expect(store.getState().status).toBeNull();
      expect(store.getState().error).toBe(error);
      expect(store.getState().busy).toBe(false);

      // Relaunching while offline must not turn the next restore into status(false).
      store.setState({ status: null, busy: false, error: null });
      await store.getState().restore();
      expect(mock.status).toHaveBeenLastCalledWith(true);
      expect(store.getState().status).toEqual(account);
      expect(store.getState().error).toBeNull();
    },
  );
  it('forgets a rejected or ambiguously rotated credential', async () => {
    const key = 'beam-bench.craftgineer.remember';
    window.localStorage.setItem(key, '1');
    store.setState({ status: account });
    mock.status.mockRejectedValue('invalid_token');
    await store.getState().restore();
    expect(window.localStorage.getItem(key)).toBe('0');
    expect(store.getState().status).toBeNull();
  });
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
