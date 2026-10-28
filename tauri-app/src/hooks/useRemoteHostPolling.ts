import { useEffect } from 'react';
import { useRemoteHostStore } from '../stores/remoteHostStore';
import { remoteHostService } from '../services/remoteHostService';

/** Polling observes host state. No reconnect path can call Start or Prepare. */
export function useRemoteHostPolling() {
  const enabled = useRemoteHostStore((s) => s.enabled);
  const configured = useRemoteHostStore((s) => s.configured);
  useEffect(() => {
    if (!enabled) return;
    let stopped = false;
    void remoteHostService
      .settings()
      .then((settings) => {
        if (!stopped) useRemoteHostStore.setState(settings);
      })
      .catch((e) => {
        if (!stopped) useRemoteHostStore.setState({ connectionError: String(e), online: false });
      });
    return () => {
      stopped = true;
    };
  }, [enabled]);
  useEffect(() => {
    if (!enabled) return;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const snapshot = await remoteHostService.status();
        if (stopped) return;
        const previous = useRemoteHostStore.getState();
        const sameHost = previous.snapshot?.boot_id === snapshot.boot_id;
        useRemoteHostStore.setState({
          snapshot,
          online: true,
          lastSeen: Date.now(),
          connectionError: null,
          ...(!sameHost || previous.review?.transfer_id !== snapshot.transfer_id
            ? { review: null }
            : {}),
        });
      } catch (e) {
        if (!stopped) useRemoteHostStore.setState({ online: false, connectionError: String(e) });
      }
      if (!stopped) timer = setTimeout(poll, 1500);
    };
    if (configured) void poll();
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  }, [enabled, configured]);
}
