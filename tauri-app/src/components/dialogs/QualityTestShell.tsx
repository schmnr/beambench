import { useEffect, useMemo, useRef, useState } from 'react';
import { wrapBackendError } from '../../i18n/errors';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';

import { useMachineStore } from '../../stores/machineStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { useProjectStore } from '../../stores/projectStore';
import { materialService, type MaterialApplyWarning } from '../../services/materialService';
import {
  formatQualityTestError,
  formatQualityTestWarning,
  makeSeedCutEntry,
  qualityTestService,
  type QualityTestCanvasResponse,
  type QualityTestPreviewResponse,
} from '../../services/qualityTestService';
import type { CutEntry, OperationType } from '../../types/project';
import type { MaterialPreset } from '../../types/material';
import {
  DEFAULT_QUALITY_TEST_SETTINGS,
  type MachineProfile,
  type QualityTestRequest,
  type QualityTestSettings,
  type QualityTestWarning,
} from '../../types/machine';
import type { Layer, Workspace } from '../../types/project';
import { MovableResizableDialogFrame } from '../shared/MovableResizableDialogFrame';
import { PreviewWindow } from './PreviewWindow';

const QT_LAYER_PALETTE = ['#ff6b6b', '#4dabf7', '#69db7c', '#ffd43b', '#9775fa', '#f783ac'];

/**
 * Synthesize stub `Layer` objects from a `PreviewData` so `PreviewWindow` can render the
 * generated geometry without needing the real synthetic project on the frontend. We don't have
 * the seeded `CutEntry` here, so each layer's primary entry is a placeholder Line.
 */
function buildStubLayersForPreview(preview: QualityTestPreviewResponse['preview']): Layer[] {
  const layers = preview.layers ?? [];
  return layers.map((pl, i) => {
    const vectorPaths = pl.vector_paths ?? [];
    const rasterRegions = pl.raster_regions ?? [];
    return {
      id: pl.layer_id,
      name: `qt/${i}`,
      entries: [
        {
          id: `qt-entry-${pl.layer_id}`,
          operation: rasterRegions.length > 0 ? 'fill' : 'line',
          speed_mm_min: vectorPaths[0]?.speed_mm_min ?? rasterRegions[0]?.speed_mm_min ?? 1000,
          power_percent: vectorPaths[0]?.power_percent ?? 50,
          raster_settings: null,
          vector_settings: null,
          air_assist: false,
          power_min_percent: 0,
          z_offset_mm: 0,
          gcode_prefix: '',
          gcode_suffix: '',
          output_enabled: true,
        } as unknown as Layer['entries'][number],
      ],
      enabled: true,
      order_index: i,
      color_tag: QT_LAYER_PALETTE[i % QT_LAYER_PALETTE.length],
      visible: true,
      is_tool_layer: false,
    };
  });
}

export type QualityToolKind = 'material' | 'focus' | 'interval';

type QualityTestKey = keyof QualityTestSettings;
type QualityTestSettingsValue = QualityTestSettings[QualityTestKey];

/**
 * Persists per-dialog settings into the active machine profile, debounced. Each dialog calls this
 * with its own local state so changes flow back to disk without re-entering the project undo path.
 */
export function useQualityTestSettingsPersistence<K extends QualityTestKey>(
  field: K,
  settings: QualityTestSettings[K],
) {
  const { t } = useTranslation();
  const profiles = useMachineStore((s) => s.profiles);
  const activeProfileId = useMachineStore((s) => s.activeProfileId);
  const saveProfile = useMachineStore((s) => s.saveProfile);
  const push = useNotificationStore((s) => s.push);
  const lastSerialized = useRef<string | null>(null);

  useEffect(() => {
    const profile = profiles.find((p) => p.id === activeProfileId);
    if (!profile) return;
    const current = profile.quality_test_settings ?? DEFAULT_QUALITY_TEST_SETTINGS;
    const incomingJson = JSON.stringify(settings);
    if (lastSerialized.current === incomingJson) return;
    if (JSON.stringify(current[field]) === incomingJson) {
      lastSerialized.current = incomingJson;
      return;
    }
    const timer = window.setTimeout(() => {
      const next: MachineProfile = {
        ...profile,
        quality_test_settings: { ...current, [field]: settings },
      };
      lastSerialized.current = incomingJson;
      void saveProfile(next).catch((e) => {
        push(t('dialog.quality_test.persist_failed', { detail: String(e) }), 'warning');
      });
    }, 250);
    return () => window.clearTimeout(timer);
  }, [field, settings, profiles, activeProfileId, saveProfile, push, t]);
}

// Suppress unused-import warning for re-exported settings types in test files.
export type { QualityTestSettings, QualityTestSettingsValue };

interface QualityTestShellProps {
  title: string;
  toolKind: QualityToolKind;
  buildRequest: () => QualityTestRequest;
  createOnCanvas?: () => QualityTestCanvasPromise;
  /** Optional pre-flight gate for every generated-output action. */
  outputActionsGateReason?: () => string | null;
  /** Optional pre-flight gate. Return a string to disable Frame/Start with this reason. */
  liveActionsGateReason?: () => string | null;
  children: React.ReactNode;
  onClose: () => void;
}
type QualityTestCanvasPromise = Promise<QualityTestCanvasResponse>;

/**
 * Shared shell for the three M3 quality-test dialogs (Material/Focus/Interval).
 *
 * Handles: preview/frame/start/save buttons, warnings banner, live-actions gating,
 * per-machine-profile persistence, and Escape-key dismissal. Tool-specific form fields
 * live inside `children`.
 */
export function QualityTestShell({
  title,
  toolKind,
  buildRequest,
  createOnCanvas,
  outputActionsGateReason,
  liveActionsGateReason,
  children,
  onClose,
}: QualityTestShellProps) {
  const { t } = useTranslation();
  const profiles = useMachineStore((s) => s.profiles);
  const activeProfileId = useMachineStore((s) => s.activeProfileId);
  const sessionState = useMachineStore((s) => s.sessionState);
  const machineStatus = useMachineStore((s) => s.machineStatus);
  const jobProgress = useMachineStore((s) => s.jobProgress);
  const pauseJob = useMachineStore((s) => s.pauseJob);
  const resumeJob = useMachineStore((s) => s.resumeJob);
  const cancelJob = useMachineStore((s) => s.cancelJob);
  const applyBackendProjectUpdate = useProjectStore((s) => s.applyBackendProjectUpdate);
  const push = useNotificationStore((s) => s.push);

  const activeProfile = useMemo(
    () => profiles.find((p) => p.id === activeProfileId) ?? null,
    [profiles, activeProfileId],
  );

  const project = useProjectStore((s) => s.project);
  const projectWorkspace = project?.workspace ?? null;

  const [previewResp, setPreviewResp] = useState<QualityTestPreviewResponse | null>(null);
  const [showPreview, setShowPreview] = useState(false);
  const [busy, setBusy] = useState<
    null | 'preview' | 'frame' | 'start' | 'save' | 'canvas' | 'pause' | 'resume' | 'stop'
  >(null);
  const [errorText, setErrorText] = useState<string | null>(null);

  const previewLayers = useMemo(
    () => (previewResp ? buildStubLayersForPreview(previewResp.preview) : []),
    [previewResp],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  const outputGate = outputActionsGateReason?.() ?? null;
  // Frame/Start drive the physical machine, so they need a ready, idle
  // machine and an active profile regardless of the tool-specific gate.
  // Output actions (Preview, Save G-code) work offline and are not gated
  // here. This is shared by every quality-test dialog, so it lives in the
  // shell.
  const hasActiveProfile = (profiles ?? []).some((profile) => profile.id === activeProfileId);
  const machineGate: string | null =
    sessionState !== 'ready'
      ? t('dialog.quality_test.gate_not_ready')
      : !hasActiveProfile
        ? t('dialog.quality_test.gate_no_profile')
        : machineStatus?.run_state !== 'idle'
          ? t('dialog.quality_test.gate_not_idle')
          : null;
  const liveGate = machineGate ?? liveActionsGateReason?.() ?? outputGate;
  const boundsBlock = previewResp?.warnings.find((w) => w.kind === 'bounds_exceeded');
  const isRunning = jobProgress?.state === 'running';
  const isPaused = jobProgress?.state === 'paused';
  // A staged Ruida/Lihuiyu transfer reports Preparing: the job is live (Stop
  // must be reachable, especially on Lihuiyu where acknowledged packets may
  // already be executing) but not yet pausable.
  const isPreparing = jobProgress?.state === 'preparing';
  const isJobActive = isRunning || isPaused || isPreparing;
  const outputDisabled = outputGate !== null || busy !== null;
  const liveDisabled =
    liveGate !== null || boundsBlock !== undefined || busy !== null || isJobActive;
  const frameSize =
    toolKind === 'material'
      ? { initialWidth: 780, initialHeight: 640, minWidth: 620, minHeight: 460 }
      : { initialWidth: 560, initialHeight: 520, minWidth: 480, minHeight: 360 };

  const runPreview = async () => {
    setErrorText(null);
    setBusy('preview');
    try {
      const resp = await qualityTestService.preview(buildRequest());
      setPreviewResp(resp);
      setShowPreview(true);
    } catch (e) {
      setErrorText(formatQualityTestError(e as string));
    } finally {
      setBusy(null);
    }
  };

  const runFrame = async () => {
    setErrorText(null);
    setBusy('frame');
    try {
      const progress = await qualityTestService.frame(buildRequest());
      useMachineStore.setState({ jobProgress: progress });
    } catch (e) {
      setErrorText(formatQualityTestError(e as string));
    } finally {
      setBusy(null);
    }
  };

  const runStart = async () => {
    setErrorText(null);
    setBusy('start');
    try {
      const progress = await qualityTestService.start(buildRequest());
      useMachineStore.setState({ jobProgress: progress });
    } catch (e) {
      setErrorText(formatQualityTestError(e as string));
    } finally {
      setBusy(null);
    }
  };

  const runPause = async () => {
    setErrorText(null);
    setBusy('pause');
    try {
      await pauseJob();
      useMachineStore.setState((s) => ({
        jobProgress: s.jobProgress ? { ...s.jobProgress, state: 'paused' } : null,
      }));
    } catch (e) {
      setErrorText(wrapBackendError(String(e)));
    } finally {
      setBusy(null);
    }
  };

  const runResume = async () => {
    setErrorText(null);
    setBusy('resume');
    try {
      await resumeJob();
      useMachineStore.setState((s) => ({
        jobProgress: s.jobProgress ? { ...s.jobProgress, state: 'running' } : null,
      }));
    } catch (e) {
      setErrorText(wrapBackendError(String(e)));
    } finally {
      setBusy(null);
    }
  };

  const runStop = async () => {
    setErrorText(null);
    setBusy('stop');
    try {
      await cancelJob();
      useMachineStore.setState({ jobProgress: null });
    } catch (e) {
      setErrorText(wrapBackendError(String(e)));
    } finally {
      setBusy(null);
    }
  };

  const runSave = async () => {
    setErrorText(null);
    setBusy('save');
    try {
      const resp = await qualityTestService.exportGcode(buildRequest());
      if (resp) {
        push(t('dialog.quality_test.saved_gcode', { path: resp.path }), 'info');
      }
    } catch (e) {
      setErrorText(formatQualityTestError(e as string));
    } finally {
      setBusy(null);
    }
  };

  const runCreateOnCanvas = async () => {
    if (!createOnCanvas) return;
    setErrorText(null);
    setBusy('canvas');
    try {
      const resp = await createOnCanvas();
      await applyBackendProjectUpdate(resp.project, {
        selectedObjectIds: resp.createdObjectIds,
        selectedLayerId: resp.createdLayerIds[0] ?? null,
      });
      for (const warning of resp.warnings) {
        push(formatQualityTestWarning(warning), 'warning');
      }
      push(t('dialog.quality_test.created_on_canvas', { count: resp.createdObjectIds.length }), 'success');
    } catch (e) {
      setErrorText(formatQualityTestError(e as string));
    } finally {
      setBusy(null);
    }
  };

  return createPortal(
    <>
      <MovableResizableDialogFrame
        title={title}
        titleId={`quality-test-${toolKind}-title`}
        testId={`qt-${toolKind}-dialog`}
        initialWidth={frameSize.initialWidth}
        initialHeight={frameSize.initialHeight}
        minWidth={frameSize.minWidth}
        minHeight={frameSize.minHeight}
        onRequestClose={onClose}
        footer={
          <div className="flex flex-wrap justify-end gap-2 px-4 py-3">
            <button
              onClick={onClose}
              className="px-3 py-1 text-xs font-medium rounded bg-bb-bg hover:bg-bb-hover text-bb-text"
            >
              {t('common.close')}
            </button>
            <button
              data-testid={`qt-${toolKind}-preview`}
              onClick={() => void runPreview()}
              disabled={outputDisabled}
              className="px-3 py-1 text-xs font-medium rounded bg-bb-bg hover:bg-bb-hover text-bb-text disabled:opacity-50"
            >
              {busy === 'preview' ? t('dialog.quality_test.generating') : t('dialog.quality_test.preview')}
            </button>
            {createOnCanvas && (
              <button
                data-testid={`qt-${toolKind}-create-canvas`}
                onClick={() => void runCreateOnCanvas()}
                disabled={busy !== null}
                className="px-3 py-1 text-xs font-medium rounded bg-bb-bg hover:bg-bb-hover text-bb-text disabled:opacity-50"
              >
                {busy === 'canvas' ? t('dialog.quality_test.creating') : t('dialog.quality_test.create_on_canvas')}
              </button>
            )}
            <button
              data-testid={`qt-${toolKind}-frame`}
              onClick={() => void runFrame()}
              disabled={liveDisabled}
              className="px-3 py-1 text-xs font-medium rounded bg-bb-bg hover:bg-bb-hover text-bb-text disabled:opacity-50"
            >
              {t('dialog.quality_test.frame')}
            </button>
            {isRunning ? (
              <>
                <button
                  data-testid={`qt-${toolKind}-pause`}
                  onClick={() => void runPause()}
                  disabled={busy !== null}
                  className="px-3 py-1 text-xs font-medium rounded bg-bb-bg hover:bg-bb-hover text-bb-text disabled:opacity-50"
                >
                  {busy === 'pause' ? t('dialog.quality_test.pausing') : t('dialog.quality_test.pause')}
                </button>
                <button
                  data-testid={`qt-${toolKind}-stop`}
                  onClick={() => void runStop()}
                  disabled={busy !== null}
                  className="px-3 py-1 text-xs font-medium rounded bg-bb-error/80 hover:bg-bb-error text-bb-text disabled:opacity-50"
                >
                  {busy === 'stop' ? t('dialog.quality_test.stopping') : t('dialog.quality_test.stop')}
                </button>
              </>
            ) : isPaused ? (
              <>
                <button
                  data-testid={`qt-${toolKind}-resume`}
                  onClick={() => void runResume()}
                  disabled={busy !== null}
                  className="px-3 py-1 text-xs font-medium rounded bg-bb-accent hover:bg-bb-accent-hover text-bb-on-accent disabled:opacity-50"
                >
                  {busy === 'resume' ? t('dialog.quality_test.resuming') : t('dialog.quality_test.resume')}
                </button>
                <button
                  data-testid={`qt-${toolKind}-stop`}
                  onClick={() => void runStop()}
                  disabled={busy !== null}
                  className="px-3 py-1 text-xs font-medium rounded bg-bb-error/80 hover:bg-bb-error text-bb-text disabled:opacity-50"
                >
                  {busy === 'stop' ? t('dialog.quality_test.stopping') : t('dialog.quality_test.stop')}
                </button>
              </>
            ) : isPreparing ? (
              <button
                data-testid={`qt-${toolKind}-stop`}
                onClick={() => void runStop()}
                disabled={busy !== null}
                className="px-3 py-1 text-xs font-medium rounded bg-bb-error/80 hover:bg-bb-error text-bb-text disabled:opacity-50"
              >
                {busy === 'stop' ? t('dialog.quality_test.stopping') : t('dialog.quality_test.stop')}
              </button>
            ) : (
              <button
                data-testid={`qt-${toolKind}-start`}
                onClick={() => void runStart()}
                disabled={liveDisabled}
                className="px-3 py-1 text-xs font-medium rounded bg-bb-accent hover:bg-bb-accent-hover text-bb-on-accent disabled:opacity-50"
              >
                {busy === 'start' ? t('dialog.quality_test.starting') : t('dialog.quality_test.start')}
              </button>
            )}
            <button
              data-testid={`qt-${toolKind}-save`}
              onClick={() => void runSave()}
              disabled={outputDisabled}
              className="px-3 py-1 text-xs font-medium rounded bg-bb-bg hover:bg-bb-hover text-bb-text disabled:opacity-50"
            >
              {t('dialog.quality_test.save_gcode')}
            </button>
          </div>
        }
      >
        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
        {!activeProfile && (
          <div
            className="text-xs text-bb-warning-fg bg-bb-warning-bg border border-bb-warning-border rounded px-2 py-1 mb-2"
            role="status"
          >
            {t('dialog.quality_test.no_active_profile')}
          </div>
        )}
        <div className="space-y-2 mb-3">{children}</div>

        {previewResp && previewResp.warnings.length > 0 && (
          <div className="text-xs text-bb-warning-fg bg-bb-warning-bg border border-bb-warning-border rounded px-2 py-1 mb-2 space-y-1">
            {previewResp.warnings.map((w, i) => (
              <div key={`${i}-${(w as QualityTestWarning).kind}`}>
                {formatQualityTestWarning(w)}
              </div>
            ))}
          </div>
        )}
        {(outputGate || liveGate) && (
          <div
            className="text-xs text-bb-text-muted bg-bb-bg/40 border border-bb-border rounded px-2 py-1 mb-2"
            role="status"
            data-testid={`qt-${toolKind}-live-gate`}
          >
            {t('dialog.quality_test.actions_disabled', {
              action: outputGate ? t('dialog.quality_test.quality_test_actions') : t('dialog.quality_test.frame_and_start'),
              reason: outputGate ?? liveGate,
            })}
          </div>
        )}
        {errorText && (
          <div
            className="text-xs text-bb-error bg-bb-error/10 border border-bb-error/40 rounded px-2 py-1 mb-2"
            role="alert"
          >
            {errorText}
          </div>
        )}
        {previewResp && (
          <div
            className="text-xs text-bb-text-muted mb-2 flex items-center justify-between"
            data-testid={`qt-${toolKind}-preview-stats`}
          >
            <span>
              {t('dialog.quality_test.preview_stats', {
                segments: previewResp.preview.stats.segment_count,
                distance: previewResp.preview.stats.total_distance_mm.toFixed(1),
                seconds: previewResp.preview.stats.estimated_duration_secs.toFixed(1),
              })}
            </span>
            <button
              data-testid={`qt-${toolKind}-show-preview`}
              onClick={() => setShowPreview(true)}
              className="text-bb-accent hover:underline"
            >
              {t('dialog.quality_test.open_preview_window')}
            </button>
          </div>
        )}
        </div>
      </MovableResizableDialogFrame>
      {showPreview && previewResp && (
        <PreviewWindow
          data={previewResp.preview}
          previewState="current"
          layers={previewLayers}
          workspace={projectWorkspace as Workspace | null}
          onClose={() => setShowPreview(false)}
        />
      )}
    </>,
    document.body,
  );
}

type PresetSeedOperation = Exclude<OperationType, 'tool'>;

interface MaterialPresetPickerProps {
  /** Operation kind for the placeholder seed entry sent to the backend. */
  seedOperation: PresetSeedOperation;
  /** Callback receiving the updated entry from the transient apply path. Dialogs use it to derive
   *  their own field defaults (speed range, power range, line interval, etc.). */
  onApplied: (entry: CutEntry) => void;
}

/**
 * Material-preset chip rendered above each quality-test dialog's form. Routes through the
 * transient `apply_material_preset_to_seed` command so picks update dialog defaults without
 * touching the active project's layer or undo history.
 */
export function MaterialPresetPicker({ seedOperation, onApplied }: MaterialPresetPickerProps) {
  const { t } = useTranslation();
  const push = useNotificationStore((s) => s.push);
  const [presets, setPresets] = useState<MaterialPreset[]>([]);
  const [selected, setSelected] = useState<string>('');
  const [warnings, setWarnings] = useState<MaterialApplyWarning[]>([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    materialService
      .getPresets()
      .then((list) => setPresets(Array.isArray(list) ? list : []))
      .catch((e) => push(t('dialog.quality_test.load_presets_failed', { detail: String(e) }), 'warning'));
  }, [push, t]);

  const handlePick = async (presetId: string) => {
    setSelected(presetId);
    setWarnings([]);
    if (!presetId) return;
    setLoading(true);
    try {
      const seed = makeSeedCutEntry(seedOperation);
      const { entry, warnings: ws } = await qualityTestService.applyMaterialPresetToSeed(
        presetId,
        seed,
      );
      setWarnings(ws);
      onApplied(entry);
    } catch (e) {
      push(t('dialog.quality_test.apply_preset_failed', { detail: String(e) }), 'warning');
    } finally {
      setLoading(false);
    }
  };

  if (presets.length === 0) return null;

  return (
    <div className="border-b border-bb-border pb-2 mb-2 space-y-1">
      <label className="flex items-center gap-2 text-xs">
        <span className="text-bb-text-muted shrink-0">{t('dialog.quality_test.seed_material_setting')}</span>
        <select
          data-testid="qt-preset-picker"
          value={selected}
          disabled={loading}
          onChange={(e) => void handlePick(e.target.value)}
          className="flex-1 bg-bb-bg border border-bb-border rounded px-1 py-0.5 text-bb-text"
        >
          <option value="">{t('dialog.quality_test.custom_no_preset')}</option>
          {presets.map((p) => (
            <option key={p.id} value={p.id}>
              {p.material} · {p.name}
            </option>
          ))}
        </select>
      </label>
      {warnings.length > 0 && (
        <div className="text-[11px] text-bb-warning-fg bg-bb-warning-bg border border-bb-warning-border rounded px-2 py-1">
          {warnings.map((w, i) => (
            <div key={i}>{w.message}</div>
          ))}
        </div>
      )}
    </div>
  );
}
