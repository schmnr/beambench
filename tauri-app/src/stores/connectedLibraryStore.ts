import { create } from 'zustand';
import {
  connectedLibraryService as service,
  type ConnectionStatus,
} from '../services/connectedLibraryService';
let generation = 0;
const rememberKey = 'beam-bench.craftgineer.remember';
function rememberAccount(value?: boolean): boolean {
  try {
    if (value !== undefined) window.localStorage.setItem(rememberKey, value ? '1' : '0');
    return window.localStorage.getItem(rememberKey) === '1';
  } catch {
    return false;
  }
}
interface ConnectionState {
  status: ConnectionStatus | null;
  busy: boolean;
  error: string | null;
  generation: number;
  reject: (error: string) => void;
  restore: () => Promise<void>;
  connect: (remember: boolean) => Promise<void>;
  disconnect: () => Promise<void>;
  cancel: () => Promise<void>;
}
export const useConnectedLibraryStore = create<ConnectionState>((set) => {
  const run = async (
    operation: () => Promise<ConnectionStatus>,
    accepted?: (status: ConnectionStatus) => void,
  ) => {
    const own = ++generation;
    set({ busy: true, error: null, generation: own });
    try {
      const status = await operation();
      if (own === generation) {
        accepted?.(status);
        set({ status });
      }
    } catch (error) {
      if (own === generation) {
        if (String(error).includes('invalid_token')) rememberAccount(false);
        set({
          error: String(error),
          ...(String(error).includes('invalid_token') ? { status: null } : {}),
        });
      }
    } finally {
      if (own === generation) set({ busy: false });
    }
  };
  return {
    status: null,
    busy: false,
    error: null,
    generation: 0,
    reject: (error) => {
      if (!error.includes('invalid_token') && !error.includes('library_account_mismatch')) return;
      const own = ++generation;
      rememberAccount(false);
      set({ status: null, busy: false, error, generation: own });
    },
    restore: () => run(() => service.status(rememberAccount())),
    connect: (remember) =>
      run(
        () => service.connect(remember),
        (status) => {
          rememberAccount(status.remembered);
        },
      ),
    cancel: async () => {
      const own = ++generation;
      rememberAccount(false);
      set({ status: null, busy: false, generation: own });
      try {
        await service.cancel();
      } catch (error) {
        if (own === generation) set({ error: String(error) });
      }
    },
    disconnect: async () => {
      const own = ++generation;
      rememberAccount(false);
      // Drop the account and all previews before waiting for network or wallet I/O.
      set({ status: null, error: null, busy: true, generation: own });
      try {
        await service.disconnect();
      } catch (error) {
        if (own === generation) set({ error: String(error) });
      } finally {
        if (own === generation) set({ busy: false });
      }
    },
  };
});
