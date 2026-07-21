import { useEffect, useMemo, useState } from 'react';
import { wrapBackendError } from '../../i18n/errors';
import { useTranslation } from 'react-i18next';
import { machineService } from '../../services/machineService';
import { useNotificationStore } from '../../stores/notificationStore';
import type { MachineProfile, MachineProfilePreset, ProfileFieldDiff } from '../../types/machine';

interface MachinePresetPanelProps {
  profile: MachineProfile;
  profileExists: boolean;
  dirty: boolean;
  onApplied: (profile: MachineProfile) => void;
}

function formatValue(value: unknown, t: (key: string) => string): string {
  if (value === null || value === undefined) return t('panels.machine.preset.none');
  if (typeof value === 'string') return value === '' ? t('panels.machine.preset.empty') : value;
  return JSON.stringify(value);
}

export function MachinePresetPanel({
  profile,
  profileExists,
  dirty,
  onApplied,
}: MachinePresetPanelProps) {
  const { t } = useTranslation();
  const pushNotification = useNotificationStore((s) => s.push);
  const [presets, setPresets] = useState<MachineProfilePreset[]>([]);
  const [selectedPresetId, setSelectedPresetId] = useState<string>('');
  const [diff, setDiff] = useState<ProfileFieldDiff[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    machineService
      .getMachineProfilePresets()
      .then((items) => {
        if (cancelled) return;
        const nextPresets = Array.isArray(items) ? items : [];
        setPresets(nextPresets);
        setSelectedPresetId((current) => current || profile.preset_id || nextPresets[0]?.id || '');
      })
      .catch((e) => {
        if (!cancelled) setError(wrapBackendError(String(e)));
      });
    return () => {
      cancelled = true;
    };
  }, [profile.preset_id]);

  useEffect(() => {
    setDiff(null);
    setError(null);
  }, [profile.id, selectedPresetId]);

  const selectedPreset = useMemo(
    () => presets.find((preset) => preset.id === selectedPresetId) ?? null,
    [presets, selectedPresetId],
  );

  const disabledReason =
    !profileExists ? t('panels.machine.preset.save_profile_before_applying')
    : dirty ? t('panels.machine.preset.save_or_discard_before_applying')
    : null;

  const loadDiff = async () => {
    if (!selectedPresetId || disabledReason) return null;
    setLoading(true);
    setError(null);
    try {
      const nextDiff = await machineService.getMachineProfilePresetDiff(profile.id, selectedPresetId);
      setDiff(nextDiff);
      return nextDiff;
    } catch (e) {
      const msg = String(e);
      setError(msg);
      pushNotification(t('panels.machine.preset.failed_to_preview', { detail: msg }), 'error');
      return null;
    } finally {
      setLoading(false);
    }
  };

  const applyPreset = async () => {
    if (!selectedPresetId || disabledReason || diff === null) return;
    setLoading(true);
    setError(null);
    try {
      const result = await machineService.applyMachineProfilePreset(profile.id, selectedPresetId, true);
      setDiff(result.diff);
      onApplied(result.profile);
      pushNotification(t('panels.machine.preset.applied'), 'success');
    } catch (e) {
      const msg = String(e);
      setError(msg);
      pushNotification(t('panels.machine.preset.failed_to_apply', { detail: msg }), 'error');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="border-t border-bb-border pt-2 mt-3" data-testid="machine-preset-panel">
      <div className="mb-2 flex items-center justify-between gap-2">
        <div className="text-xs font-semibold text-bb-text">{t('panels.machine.preset.title')}</div>
        {profile.preset_id && (
          <div className="text-[11px] text-bb-text-muted">
            {profile.preset_id}
            {profile.preset_version ? ` v${profile.preset_version}` : ''}
          </div>
        )}
      </div>
      <div className="space-y-2">
        <div className="flex items-center gap-2">
          <select
            className="min-w-0 flex-1 rounded border border-bb-border bg-bb-bg px-2 py-1 text-xs text-bb-text focus:border-bb-accent focus:outline-none"
            value={selectedPresetId}
            disabled={presets.length === 0 || loading}
            onChange={(e) => setSelectedPresetId(e.target.value)}
            data-testid="machine-preset-select"
          >
            {presets.map((preset) => (
              <option key={preset.id} value={preset.id}>
                {preset.name}
              </option>
            ))}
          </select>
          <button
            className="rounded border border-bb-border px-2 py-1 text-xs text-bb-text hover:bg-bb-hover disabled:cursor-not-allowed disabled:opacity-50"
            disabled={!selectedPresetId || loading || Boolean(disabledReason)}
            onClick={() => void loadDiff()}
            data-testid="machine-preset-preview"
          >
            {t('panels.machine.preset.preview')}
          </button>
          <button
            className="rounded bg-bb-accent px-2 py-1 text-xs font-medium text-bb-on-accent hover:bg-bb-accent-hover disabled:cursor-not-allowed disabled:opacity-50"
            disabled={!selectedPresetId || loading || Boolean(disabledReason) || diff === null}
            onClick={() => void applyPreset()}
            data-testid="machine-preset-apply"
          >
            {t('common.apply')}
          </button>
        </div>
        {selectedPreset && (
          <div className="text-[11px] text-bb-text-muted">
            {selectedPreset.description}
          </div>
        )}
        {selectedPreset?.advisory_text && (
          <div className="rounded border border-bb-warning-border bg-bb-warning-bg px-2 py-1 text-[11px] text-bb-warning-fg">
            {selectedPreset.advisory_text}
          </div>
        )}
        {disabledReason && <div className="text-[11px] text-bb-warning-fg">{disabledReason}</div>}
        {error && <div className="text-[11px] text-bb-error-fg">{error}</div>}
        {diff && (
          <div className="max-h-32 overflow-y-auto rounded border border-bb-border">
            {diff.length === 0 ? (
              <div className="px-2 py-1 text-[11px] text-bb-text-muted">{t('panels.machine.preset.no_changes')}</div>
            ) : (
              <table className="w-full text-left text-[11px] text-bb-text-muted">
                <tbody>
                  {diff.map((entry) => (
                    <tr key={entry.field} className="border-t border-bb-border first:border-t-0">
                      <td className="w-36 px-2 py-1 font-medium text-bb-text">{entry.field}</td>
                      <td className="px-2 py-1">{formatValue(entry.old, t)}</td>
                      <td className="px-2 py-1 text-bb-text">{formatValue(entry.new, t)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
