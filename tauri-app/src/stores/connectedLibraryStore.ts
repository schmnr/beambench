import { create } from 'zustand';
import { useEffect, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  connectedLibraryService,
  printcutcarveLibraryService,
  type LibraryProvider,
  type ConnectionStatus,
} from '../services/connectedLibraryService';
function rememberAccount(rememberKey: string, value?: boolean): boolean {
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
  connecting: boolean;
  rememberChoice: boolean;
  setRememberChoice: (value: boolean) => void;
  error: string | null;
  generation: number;
  nativeGeneration: number;
  reject: (error: string) => void;
  receive: (status: ConnectionStatus, error?: string | null) => void;
  restore: () => Promise<void>;
  connect: (remember: boolean) => Promise<void>;
  disconnect: () => Promise<void>;
  cancel: () => Promise<void>;
}
function connectionStore(provider: LibraryProvider, service: typeof connectedLibraryService) {
  let generation = 0;
  const rememberKey = `beam-bench.${provider}.remember`;
  const remember = (value?: boolean) => rememberAccount(rememberKey, value);
  return create<ConnectionState>((set, get) => {
    const run = async (
      operation: () => Promise<ConnectionStatus>,
      accepted?: (status: ConnectionStatus) => void,
      connecting = false,
    ) => {
      const own = ++generation;
      set({ busy: true, connecting, error: null, generation: own });
      try {
        const status = await operation();
        if (own === generation && (status.nativeGeneration ?? Infinity) >= get().nativeGeneration) {
          const nativeGeneration = Math.max(get().nativeGeneration, status.nativeGeneration ?? -1);
          accepted?.(status);
          if (status.account || !status.walletUnavailable) remember(status.remembered);
          set({ status, nativeGeneration });
        }
      } catch (error) {
        if (own === generation) {
          if (
            String(error).includes('invalid_token') ||
            String(error).includes('library_account_mismatch')
          )
            remember(false);
          set({
            error: String(error),
            ...(String(error).includes('invalid_token') ||
            String(error).includes('library_account_mismatch')
              ? { status: null }
              : {}),
          });
        }
      } finally {
        if (own === generation) set({ busy: false, connecting: false });
      }
    };
    return {
      status: null,
      busy: false,
      connecting: false,
      rememberChoice: true,
      setRememberChoice: (rememberChoice) => set({ rememberChoice }),
      error: null,
      generation: 0,
      nativeGeneration: -1,
      receive: (status, error = null) => {
        if (
          status.nativeGeneration === undefined ||
          status.nativeGeneration <= get().nativeGeneration
        )
          return;
        const current = get().status?.account;
        const changed =
          current?.account.id !== status.account?.account.id ||
          current?.access !== status.account?.access;
        const own = changed ? ++generation : generation;
        if (status.account || !status.walletUnavailable) remember(status.remembered);
        set({
          status,
          busy: changed ? false : get().busy,
          connecting: changed ? false : get().connecting,
          error,
          generation: own,
          nativeGeneration: status.nativeGeneration,
        });
      },
      reject: (error) => {
        if (!error.includes('invalid_token') && !error.includes('library_account_mismatch')) return;
        const own = ++generation;
        remember(false);
        set({ status: null, busy: false, connecting: false, error, generation: own });
      },
      restore: () => run(() => service.status(remember())),
      connect: (rememberChoice) =>
        run(
          () => service.connect(rememberChoice),
          (status) => {
            remember(status.remembered);
          },
          true,
        ),
      cancel: async () => {
        const own = ++generation;
        remember(false);
        set({ status: null, busy: false, connecting: false, generation: own });
        try {
          await service.cancel();
        } catch (error) {
          if (own === generation) set({ error: String(error) });
        }
      },
      disconnect: async () => {
        const own = ++generation;
        remember(false);
        // Drop the account and all previews before waiting for network or wallet I/O.
        set({ status: null, error: null, busy: true, connecting: false, generation: own });
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
}
export const useConnectedLibraryStore = connectionStore('craftgineer', connectedLibraryService);
export const usePrintcutcarveLibraryStore = connectionStore(
  'printcutcarve',
  printcutcarveLibraryService,
);
export function libraryStore(provider: LibraryProvider) {
  return provider === 'craftgineer' ? useConnectedLibraryStore : usePrintcutcarveLibraryStore;
}
export function useConnectedLibraryEvents() {
  const [ready, setReady] = useState(false);
  useEffect(() => {
    let cancelled = false;
    let unlisten: UnlistenFn | undefined;
    const restore = () => Promise.allSettled([
      useConnectedLibraryStore.getState().restore(),
      usePrintcutcarveLibraryStore.getState().restore(),
    ]).then(() => { if (!cancelled) setReady(true); });
    void listen<{ provider: LibraryProvider; status: ConnectionStatus; error?: string | null }>(
      'connected-library-changed',
      ({ payload }) => {
        if (payload.provider === 'craftgineer' || payload.provider === 'printcutcarve') {
          libraryStore(payload.provider).getState().receive(payload.status, payload.error);
        }
      },
    )
      .then((stop) => {
        if (cancelled) stop();
        else {
          unlisten = stop;
        }
      })
      .catch(() => {})
      .finally(() => { if (!cancelled) void restore(); });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);
  return ready;
}
