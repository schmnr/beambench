import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { remoteHostService } from '../../services/remoteHostService';
import { remoteJobActive, useRemoteHostStore } from '../../stores/remoteHostStore';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { sessionJobOptions } from '../../types/jobOptions';
import type { PortInfo } from '../../types/machine';
import { PreviewWindow } from '../dialogs/PreviewWindow';

const field =
  'w-full min-w-0 rounded border border-bb-border bg-bb-bg px-2 py-1.5 text-xs text-bb-text';
const control =
  'rounded border px-2 py-1.5 text-xs disabled:opacity-50 disabled:cursor-not-allowed';
const button = `${control} border-bb-border bg-bb-bg text-bb-text hover:bg-bb-hover`;
export function RemoteHostPanel() {
  const { t } = useTranslation();
  const state = useRemoteHostStore();
  const { snapshot, review, busy, online } = state;
  const project = useProjectStore((s) => s.project);
  const [showSettings, setShowSettings] = useState(false);
  const [address, setAddress] = useState(state.url);
  const [token, setToken] = useState('');
  const [profileId, setProfileId] = useState('');
  const [ports, setPorts] = useState<PortInfo[]>([]);
  const [port, setPort] = useState('');
  const [baud, setBaud] = useState(115200);
  const [motion, setMotion] = useState(false);
  const [laser, setLaser] = useState(false);
  const [advisories, setAdvisories] = useState(false);
  const [showPreview, setShowPreview] = useState(false);
  const active = remoteJobActive(snapshot);
  const owned = online && snapshot?.owned;
  const ready = owned && !busy && !active;
  const currentReview = review?.transfer_id === snapshot?.transfer_id ? review : null;
  const profileMatches = !profileId || profileId === snapshot?.active_profile_id;
  const canStart =
    ready &&
    profileMatches &&
    currentReview &&
    !snapshot?.consumed &&
    currentReview.preflight.outcome !== 'fail' &&
    motion &&
    laser &&
    ((currentReview.preflight.advisories?.length ?? 0) === 0 || advisories);

  useEffect(() => {
    if (state.url) setAddress(state.url);
  }, [state.url]);
  useEffect(() => {
    setMotion(false);
    setLaser(false);
    setAdvisories(false);
  }, [review?.transfer_id]);
  const refresh = async () => {
    const snapshot = await remoteHostService.status();
    useRemoteHostStore.setState({ snapshot, online: true, lastSeen: Date.now() });
  };
  const run = async (operation: { (): Promise<unknown> }) => {
    if (useRemoteHostStore.getState().busy) return;
    useRemoteHostStore.setState({ busy: true, error: null });
    try {
      await operation();
      await refresh();
    } catch (e) {
      useRemoteHostStore.setState({ error: String(e) });
    } finally {
      useRemoteHostStore.setState({ busy: false });
    }
  };
  const claim = async () => {
    await remoteHostService.request('remote/claim', {});
    await refresh();
    if (useRemoteHostStore.getState().snapshot?.transfer_id) {
      useRemoteHostStore.setState({ review: await remoteHostService.review() });
    }
  };
  const listPorts = async () => {
    const result = await remoteHostService.request<{ ports: PortInfo[] }>('machine/ports');
    setPorts(result.ports);
  };
  const action = (path: string, body: unknown = {}) =>
    run(() => remoteHostService.request(path, body));
  const receiptBody = {
    transfer_id: currentReview?.transfer_id,
    confirm_motion: motion,
    confirm_laser_on: laser,
    confirm_advisories: advisories,
  };
  const progress = snapshot?.progress;
  return (
    <div className="flex min-w-0 flex-col gap-3 text-xs" data-testid="remote-host-panel">
      <p className="text-bb-text-muted">{t('remote_host.independent')}</p>
      {state.configured && (
        <div className="flex items-center justify-between gap-2">
          <span className="min-w-0 break-all font-medium">{state.url}</span>
          <button className={button} onClick={() => setShowSettings((s) => !s)}>
            {t('remote_host.settings')}
          </button>
        </div>
      )}
      {(!state.configured || showSettings) && (
        <>
          <label className="flex flex-col gap-1">
            {t('remote_host.address')}
            <input
              className={field}
              value={address}
              onChange={(e) => setAddress(e.target.value)}
              placeholder={t('remote_host.address')}
              disabled={active || busy}
              autoComplete="off"
            />
          </label>
          <label className="flex flex-col gap-1">
            {t('remote_host.token')}
            <input
              type="password"
              className={field}
              value={token}
              onChange={(e) => setToken(e.target.value)}
              disabled={active || busy}
              autoComplete="off"
            />
          </label>
          <div className="flex flex-wrap gap-1">
            <button
              className={button}
              disabled={active || busy || !address.trim() || token.length < 32}
              onClick={() =>
                void run(async () => {
                  const settings = await remoteHostService.configure(address, token);
                  setToken('');
                  setShowSettings(false);
                  useRemoteHostStore.setState({ ...settings, review: null, snapshot: null });
                  await claim();
                  await listPorts();
                })
              }
            >
              {t('remote_host.save')}
            </button>
            {state.configured && (
              <button
                className={button}
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    await claim();
                    await listPorts();
                  })
                }
              >
                {t('remote_host.connect')}
              </button>
            )}
          </div>
        </>
      )}
      {state.configured && !showSettings && (
        <button className={button} disabled={busy} onClick={() => void run(claim)}>
          {t('remote_host.connect')}
        </button>
      )}
      {!state.configured && <p className="text-bb-text-muted">{t('remote_host.setup')}</p>}
      {state.configured && (
        <p role="status" className={online ? 'text-bb-text' : 'text-bb-warning-fg'}>
          {t(online ? 'remote_host.online' : 'remote_host.offline')}
        </p>
      )}
      {state.lastSeen && (
        <p className="text-bb-text-muted">
          {t('remote_host.last_seen', { time: new Date(state.lastSeen).toLocaleTimeString() })}
        </p>
      )}
      {state.connectionError && (
        <p role="alert" className="break-words text-bb-error-fg">
          {state.connectionError}
        </p>
      )}
      {state.error && (
        <p role="alert" className="break-words text-bb-error-fg">
          {state.error}
        </p>
      )}
      {online && !snapshot?.owned && (
        <p className="text-bb-warning-fg">{t('remote_host.owned_elsewhere')}</p>
      )}
      <button
        className="rounded bg-bb-error px-2 py-2 font-medium text-bb-on-error disabled:opacity-50"
        disabled={!state.configured}
        onClick={() => {
          // Stop must remain available while another command is pending.
          void remoteHostService
            .request('machine/emergency-stop', {})
            .then(refresh)
            .catch((e) => useRemoteHostStore.setState({ error: String(e) }));
        }}
      >
        {t('remote_host.stop')}
      </button>
      {progress && (
        <div className="space-y-1">
          <p>
            {t('remote_host.progress', {
              state: t(`panels.machine.job_progress.state.${progress.state}`),
              acknowledged: progress.acknowledged_lines,
              total: progress.total_lines,
            })}
          </p>
          <div
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={progress.total_lines}
            aria-valuenow={progress.acknowledged_lines}
            className="h-1.5 overflow-hidden rounded bg-bb-surface-3"
          >
            <div
              className="h-full bg-bb-accent"
              style={{
                width: `${Math.min(100, (100 * progress.acknowledged_lines) / Math.max(1, progress.total_lines))}%`,
              }}
            />
          </div>
          {progress.error_message && <p className="text-bb-error-fg">{progress.error_message}</p>}
        </div>
      )}
      {snapshot && (
        <>
          <label className="flex items-start gap-2">
            <input
              type="checkbox"
              checked={motion}
              disabled={busy || !online}
              onChange={(e) => setMotion(e.target.checked)}
            />
            {t('remote_host.motion_confirm')}
          </label>
          <details open={snapshot.machine.session_state === 'disconnected'}>
            <summary className="cursor-pointer py-1 text-bb-text-muted">
              {t('remote_host.laser_setup')}
            </summary>
            <div className="mt-2 flex flex-col gap-2">
              <label className="flex flex-col gap-1">
                {t('remote_host.host_profile')}
                <select
                  className={field}
                  value={profileId || snapshot.active_profile_id || ''}
                  disabled={!ready}
                  onChange={(e) => {
                    setProfileId(e.target.value);
                    useRemoteHostStore.setState({ review: null });
                  }}
                >
                  <option value="">{t('remote_host.choose_profile')}</option>
                  {snapshot.profiles.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
                </select>
              </label>
              {snapshot.profiles.length === 0 && (
                <p className="text-bb-text-muted">{t('remote_host.no_profiles')}</p>
              )}
              <div className="grid grid-cols-2 gap-2">
                <label className="flex min-w-0 flex-col gap-1">
                  {t('remote_host.port')}
                  <select
                    className={field}
                    value={port}
                    disabled={!ready}
                    onChange={(e) => setPort(e.target.value)}
                  >
                    <option value="">{t('remote_host.port')}</option>
                    {ports.map((p) => (
                      <option key={p.port_name} value={p.port_name}>
                        {p.port_name}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="flex min-w-0 flex-col gap-1">
                  {t('remote_host.baud')}
                  <input
                    className={field}
                    type="number"
                    min="1"
                    max="4000000"
                    value={baud}
                    disabled={!ready}
                    onChange={(e) => setBaud(Number(e.target.value))}
                  />
                </label>
              </div>
              <div className="flex flex-wrap gap-1">
                <button className={button} disabled={!ready} onClick={() => void run(listPorts)}>
                  {t('remote_host.refresh_ports')}
                </button>
                <button
                  className={button}
                  disabled={!ready || !motion || !port || !Number.isInteger(baud) || baud <= 0}
                  onClick={() =>
                    void run(async () => {
                      await remoteHostService.request('remote/connect', {
                        port,
                        baud_rate: baud,
                        profile_id: profileId || snapshot.active_profile_id,
                        confirm_motion: motion,
                      });
                      useRemoteHostStore.setState({ review: null });
                    })
                  }
                >
                  {t('remote_host.connect_laser')}
                </button>
                <button
                  className={button}
                  disabled={!ready || !motion}
                  onClick={() => void action('machine/home', { confirm_motion: motion })}
                >
                  {t('remote_host.home')}
                </button>
                <button
                  className={button}
                  disabled={!ready}
                  onClick={() => void action('machine/unlock')}
                >
                  {t('remote_host.unlock')}
                </button>
              </div>
            </div>
          </details>
          <button
            className={`${control} border-bb-accent bg-bb-bg text-bb-accent hover:bg-bb-hover`}
            disabled={!ready || !project || !(profileId || snapshot.active_profile_id)}
            onClick={() =>
              void run(async () => {
                useRemoteHostStore.setState({ review: null });
                const local = useProjectStore.getState();
                const result = await remoteHostService.prepare(
                  profileId || snapshot.active_profile_id!,
                  sessionJobOptions(useUiStore.getState().jobOptions, local.selectedObjectIds),
                );
                useRemoteHostStore.setState({ review: result });
              })
            }
          >
            {t('remote_host.prepare')}
          </button>
          <p className="text-bb-text-muted">{t('remote_host.new_edits')}</p>
          {snapshot.project_name && (
            <p className="font-medium">
              {t('remote_host.uploaded', { name: snapshot.project_name })}
            </p>
          )}
          {snapshot.transfer_id && (
            <button
              className={button}
              disabled={!ready}
              onClick={() =>
                void run(async () =>
                  useRemoteHostStore.setState({ review: await remoteHostService.review() }),
                )
              }
            >
              {t('remote_host.review')}
            </button>
          )}
          {currentReview && (
            <>
              <button className={button} onClick={() => setShowPreview(true)}>
                {t('remote_host.preview')}
              </button>
              <p className="text-bb-text-muted">{t('remote_host.prepared')}</p>
              <ul className="space-y-1" aria-label={t('remote_host.prepared')}>
                {currentReview.preflight.checks.map((c, i) => (
                  <li className={c.passed ? 'text-bb-text-muted' : 'text-bb-error-fg'} key={i}>
                    {c.description}: {c.message}
                  </li>
                ))}
                {(currentReview.preflight.advisories ?? []).map((a, i) => (
                  <li className="text-bb-warning-fg" key={`advisory-${i}`}>
                    {a.message}
                  </li>
                ))}
              </ul>
            </>
          )}
          <label className="flex items-start gap-2">
            <input
              type="checkbox"
              checked={laser}
              disabled={busy || !online}
              onChange={(e) => setLaser(e.target.checked)}
            />
            {t('remote_host.laser_confirm')}
          </label>
          {!!currentReview?.preflight.advisories?.length && (
            <label className="flex items-start gap-2">
              <input
                type="checkbox"
                checked={advisories}
                onChange={(e) => setAdvisories(e.target.checked)}
              />
              {t('remote_host.advisory_confirm')}
            </label>
          )}
          <div className="grid grid-cols-2 gap-1">
            <button
              className={button}
              disabled={!ready || !profileMatches || !currentReview || snapshot.consumed || !motion}
              onClick={() => void action('remote/frame', receiptBody)}
            >
              {t('remote_host.frame')}
            </button>
            <button
              className={`${control} border-bb-accent bg-bb-accent text-bb-on-accent hover:bg-bb-accent-hover`}
              disabled={!canStart}
              onClick={() => {
                // Treat even a lost Start response as consumed until the host says otherwise.
                useRemoteHostStore.setState({ snapshot: { ...snapshot, consumed: true } });
                void action('remote/start', receiptBody);
              }}
            >
              {t('remote_host.start')}
            </button>
            <button
              className={button}
              disabled={!owned || busy || progress?.state !== 'running'}
              onClick={() => void action('jobs/pause')}
            >
              {t('remote_host.pause')}
            </button>
            <button
              className={button}
              disabled={!owned || busy || progress?.state !== 'paused'}
              onClick={() => void action('jobs/resume')}
            >
              {t('remote_host.resume')}
            </button>
          </div>
          {owned && (
            <button
              className={button}
              disabled={!ready}
              onClick={() => void action('remote/release')}
            >
              {t('remote_host.release')}
            </button>
          )}
        </>
      )}
      {busy && <p role="status">{t('remote_host.working')}</p>}
      {showPreview && currentReview && (
        <PreviewWindow
          title={t('remote_host.preview_title', { host: state.url })}
          data={currentReview.preview}
          layers={currentReview.layers}
          workspace={currentReview.workspace}
          previewState="current"
          onClose={() => setShowPreview(false)}
        />
      )}
    </div>
  );
}
