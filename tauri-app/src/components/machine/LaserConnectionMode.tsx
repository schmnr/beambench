import { useTranslation } from 'react-i18next';
import { useRemoteHostStore } from '../../stores/remoteHostStore';
import { useMachineStore } from '../../stores/machineStore';

export function LaserConnectionMode() {
  const { t } = useTranslation();
  const enabled = useRemoteHostStore((s) => s.enabled);
  const busy = useRemoteHostStore((s) => s.busy);
  const localSession = useMachineStore((s) => s.sessionState);
  return (
    <label className="flex flex-col gap-1 text-xs text-bb-text-muted">
      {t('remote_host.mode')}
      <select
        className="rounded border border-bb-border bg-bb-bg px-2 py-1.5 text-bb-text"
        value={enabled ? 'remote' : 'local'}
        disabled={busy || (!enabled && localSession !== 'disconnected')}
        onChange={(event) => useRemoteHostStore.setState({ enabled: event.target.value === 'remote' })}
      >
        <option value="local">{t('remote_host.local')}</option>
        <option value="remote">{t('remote_host.remote')}</option>
      </select>
    </label>
  );
}
