import { useEffect } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { AppEvent } from '../types/events';

/**
 * Subscribe to Tauri events from the Rust backend.
 * Automatically unsubscribes on component unmount.
 */
export function useEventListener<T = unknown>(
  eventName: string,
  handler: (event: AppEvent<T>) => void,
  onError?: (error: unknown) => void,
) {
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let cancelled = false;

    listen<AppEvent<T>>(eventName, (tauriEvent) => {
      handler(tauriEvent.payload);
    }).then((fn) => {
      if (cancelled) {
        fn();
      } else {
        unlisten = fn;
      }
    }).catch((error) => {
      if (!cancelled) {
        onError?.(error);
      }
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [eventName, handler, onError]);
}
