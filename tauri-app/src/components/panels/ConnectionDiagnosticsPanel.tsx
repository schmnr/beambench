import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Send, Save, RefreshCw } from 'lucide-react';
import { feedbackService } from '../../services/feedbackService';
import type { ConnectionDiagnosticsSnapshot } from '../../types/feedback';
import { openFeedbackReport } from '../../feedbackEvents';
import { useNotificationStore } from '../../stores/notificationStore';
import i18n from '../../i18n';
import { wrapBackendError } from '../../i18n/errors';

function formatState(state: string): string {
  return state.replace(/_/g, ' ');
}

function TrafficBlock({ title, hex, ascii }: { title: string; hex: string; ascii: string }) {
  const { t } = useTranslation();
  return (
    <div className="rounded border border-bb-border bg-bb-bg/50 p-2">
      <div className="text-[11px] font-semibold uppercase tracking-wide text-bb-text-dim">{title}</div>
      <div className="mt-1 font-mono text-[11px] text-bb-text break-all">{hex || t('panels.diagnostics.none_captured')}</div>
      <div className="mt-1 font-mono text-[11px] text-bb-text-dim break-all">{ascii || ''}</div>
    </div>
  );
}

export function ConnectionDiagnosticsPanel() {
  const { t } = useTranslation();
  const [snapshot, setSnapshot] = useState<ConnectionDiagnosticsSnapshot | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    let timer: number | null = null;

    const refresh = async () => {
      try {
        const next = await feedbackService.getConnectionDiagnostics();
        if (!cancelled) {
          setSnapshot(next);
          setLoading(false);
        }
      } catch (error) {
        if (!cancelled) {
          setLoading(false);
          useNotificationStore.getState().push(i18n.t('notifications.diagnostics_unavailable', { detail: String(error) }), 'warning');
        }
      }
    };

    void refresh();
    timer = window.setInterval(() => { void refresh(); }, 1000);

    return () => {
      cancelled = true;
      if (timer !== null) window.clearInterval(timer);
    };
  }, []);

  const saveDiagnostics = async () => {
    try {
      const saved = await feedbackService.saveReport({
        kind: 'connectivity',
        description: null,
        notes: null,
        title: i18n.t('feedback.connection_diagnostics_title'),
        reply_to_email: null,
        include_project_file: false,
        source_context: { source: 'diagnostics_panel', correlation_ts: new Date().toISOString() },
      });
      useNotificationStore.getState().push(i18n.t('notifications.diagnostics_saved', { path: saved.path }), 'success');
    } catch (error) {
      if (!String(error).toLowerCase().includes('cancelled')) {
        useNotificationStore.getState().push(wrapBackendError(String(error)), 'error');
      }
    }
  };

  const sendDiagnostics = () => {
    openFeedbackReport({
      kind: 'connectivity',
      title: i18n.t('feedback.connection_problem_title'),
      notes: '',
      sourceContext: { source: 'diagnostics_panel', correlation_ts: new Date().toISOString() },
    });
  };

  return (
    <div className="flex h-full min-h-0 flex-col gap-3 p-3 text-xs text-bb-text">
      <div className="flex items-center justify-between gap-2">
        <div>
          <div className="text-sm font-semibold">{t('panels.registry.connection_diagnostics')}</div>
          <div className="text-[11px] text-bb-text-dim">
            {snapshot ? new Date(snapshot.captured_at).toLocaleTimeString() : loading ? t('common.loading') : t('panels.diagnostics.no_snapshot')}
          </div>
        </div>
        <button
          type="button"
          className="rounded border border-bb-border px-2 py-1 text-bb-text hover:bg-bb-hover"
          onClick={() => { void feedbackService.getConnectionDiagnostics().then(setSnapshot); }}
          aria-label={t('panels.diagnostics.refresh_aria')}
          title={t('panels.diagnostics.refresh')}
        >
          <RefreshCw size={14} />
        </button>
      </div>

      <div className="rounded border border-bb-border bg-bb-bg/50 p-2">
        <div className="text-[11px] font-semibold uppercase tracking-wide text-bb-text-dim">{t('panels.diagnostics.state')}</div>
        <div className="mt-1 text-sm capitalize">{formatState(snapshot?.machine.session_state ?? t('common.unknown'))}</div>
        <div className="mt-1 text-[11px] text-bb-text-dim">
          {snapshot?.machine.handshake_message ?? t('panels.diagnostics.no_connection_attempt')}
        </div>
        {snapshot?.machine.firmware_version && (
          <div className="mt-1 font-mono text-[11px]">{snapshot.machine.firmware_version}</div>
        )}
      </div>

      <div className="min-h-0 overflow-auto rounded border border-bb-border">
        <table className="w-full text-left text-[11px]">
          <thead className="sticky top-0 bg-bb-panel text-bb-text-dim">
            <tr>
              <th className="px-2 py-1 font-medium">{t('panels.diagnostics.port')}</th>
              <th className="px-2 py-1 font-medium">{t('panels.diagnostics.vid')}</th>
              <th className="px-2 py-1 font-medium">{t('panels.diagnostics.pid')}</th>
              <th className="px-2 py-1 font-medium">{t('panels.diagnostics.state')}</th>
            </tr>
          </thead>
          <tbody>
            {(snapshot?.ports_detected ?? []).map((port) => (
              <tr key={port.name} className="border-t border-bb-border">
                <td className="px-2 py-1 font-mono">{port.name}</td>
                <td className="px-2 py-1 font-mono">{port.vendor_id ?? '-'}</td>
                <td className="px-2 py-1 font-mono">{port.product_id ?? '-'}</td>
                <td className="px-2 py-1">{port.in_use_by_beambench ? t('panels.diagnostics.in_use') : t('panels.diagnostics.available')}</td>
              </tr>
            ))}
            {snapshot && snapshot.ports_detected.length === 0 && (
              <tr>
                <td className="px-2 py-2 text-bb-text-dim" colSpan={4}>{t('panels.diagnostics.no_ports')}</td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      {snapshot?.known_issues.map((issue) => (
        <div key={issue.code} className="rounded border border-bb-warning-border bg-bb-warning-bg p-2 text-[11px] text-bb-warning-fg">
          {issue.message}
        </div>
      ))}

      {snapshot && snapshot.connection_events.length > 0 && (
        <div className="rounded border border-bb-border bg-bb-bg/50 p-2">
          <div className="text-[11px] font-semibold uppercase tracking-wide text-bb-text-dim">{t('panels.diagnostics.connection_events')}</div>
          <div className="mt-1 max-h-24 space-y-1 overflow-auto font-mono text-[11px] text-bb-text-dim">
            {snapshot.connection_events.slice(-6).map((event) => (
              <div key={`${event.ts}-${event.stage}-${event.message ?? event.error ?? ''}`}>
                {new Date(event.ts).toLocaleTimeString()} {event.stage}
                {event.port_name ? ` ${event.port_name}` : ''}
                {event.baud_rate ? ` @ ${event.baud_rate}` : ''}
                {event.error ? ` - ${event.error}` : event.message ? ` - ${event.message}` : ''}
              </div>
            ))}
          </div>
        </div>
      )}

      <div className="grid gap-2">
        <TrafficBlock title={t('panels.diagnostics.tx')} hex={snapshot?.recent_serial.tx_hex ?? ''} ascii={snapshot?.recent_serial.tx_ascii ?? ''} />
        <TrafficBlock title={t('panels.diagnostics.rx')} hex={snapshot?.recent_serial.rx_hex ?? ''} ascii={snapshot?.recent_serial.rx_ascii ?? ''} />
      </div>

      <div className="mt-auto flex gap-2 pt-1">
        <button
          type="button"
          className="inline-flex flex-1 items-center justify-center gap-1 rounded bg-bb-panel px-3 py-1.5 text-xs text-bb-text ring-1 ring-bb-border hover:bg-bb-hover"
          onClick={() => { void saveDiagnostics(); }}
        >
          <Save size={14} /> {t('panels.diagnostics.save')}
        </button>
        <button
          type="button"
          className="inline-flex flex-1 items-center justify-center gap-1 rounded bg-bb-accent px-3 py-1.5 text-xs font-medium text-bb-on-accent hover:bg-bb-accent-hover"
          onClick={sendDiagnostics}
        >
          <Send size={14} /> {t('panels.diagnostics.send')}
        </button>
      </div>
    </div>
  );
}
