import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { useCameraStore } from '../../stores/cameraStore';
import { useProjectStore } from '../../stores/projectStore';
import type { CameraFrameHandle } from '../../types/camera';
import { cameraFrameAssetUrl } from '../../services/cameraFrameAsset';

export function CameraStillPreview({ frame }: { frame: CameraFrameHandle | null | undefined }) {
  const { t } = useTranslation();
  const src = useMemo(() => {
    if (!frame) return null;
    return cameraFrameAssetUrl(frame.file_path, frame.handle_id);
  }, [frame]);

  if (!frame || !src) return null;

  return (
    <div className="space-y-1">
      <div className="text-bb-text-muted">
        {t('panels.machine.camera.latest_capture', { width: frame.width_px, height: frame.height_px })}
      </div>
      <div className="h-32 rounded border border-bb-border bg-bb-bg overflow-hidden">
        <img
          src={src}
          alt={t('panels.machine.camera.latest_camera_capture_alt')}
          className="w-full h-full object-contain"
        />
      </div>
    </div>
  );
}

export function CameraOverlayControls() {
  const { t } = useTranslation();
  const overlayVisible = useCameraStore((s) => s.overlayVisible);
  const overlayOpacity = useCameraStore((s) => s.overlayOpacity);
  const setOverlayVisible = useCameraStore((s) => s.setOverlayVisible);
  const setOverlayOpacity = useCameraStore((s) => s.setOverlayOpacity);

  return (
    <div className="space-y-2">
      <label className="flex items-center justify-between gap-2 text-bb-text-muted">
        <span>{t('panels.machine.camera.overlay')}</span>
        <input
          type="checkbox"
          checked={overlayVisible}
          onChange={(event) => setOverlayVisible(event.currentTarget.checked)}
          className="accent-bb-accent"
        />
      </label>
      <label className="flex items-center gap-2 text-bb-text-muted">
        <span className="shrink-0">{t('panels.machine.camera.opacity')}</span>
        <input
          type="range"
          min={0}
          max={1}
          step={0.05}
          value={overlayOpacity}
          onChange={(event) => setOverlayOpacity(Number(event.currentTarget.value))}
          className="min-w-0 flex-1 accent-bb-accent"
        />
        <span className="w-8 text-right">{Math.round(overlayOpacity * 100)}%</span>
      </label>
    </div>
  );
}

export function CameraOverlayStatus() {
  const { t } = useTranslation();
  const overlayState = useCameraStore((s) => s.overlayState);
  const calibration = useCameraStore((s) => s.calibration);
  const alignment = useCameraStore((s) => s.alignment);
  const draftOverlayTransform = useCameraStore((s) => s.draftOverlayTransform);
  const overlayAdjustMode = useCameraStore((s) => s.overlayAdjustMode);
  const overlayDraftDirty = useCameraStore((s) => s.overlayDraftDirty);

  const savedAlignment = alignment ?? overlayState?.alignment ?? null;
  const savedMapping = calibration ?? overlayState?.calibration ?? null;
  const frame = overlayState?.frame ?? null;
  const overlayStatus = !frame
    ? t('panels.machine.camera.status.no_frame')
    : overlayDraftDirty
      ? t('panels.machine.camera.status.unsaved_changes')
      : overlayAdjustMode
        ? t('panels.machine.camera.status.adjusting_overlay')
        : !savedAlignment && draftOverlayTransform
          ? t('panels.machine.camera.status.preview_fitted_to_bed')
          : savedAlignment
            ? t('panels.machine.camera.status.saved_alignment')
            : savedMapping
              ? t('dialog.camera_calibration.title')
              : t('panels.machine.camera.status.no_alignment');
  const alignmentText = savedAlignment
    ? savedAlignment.source === 'manual_adjust'
      ? t('panels.machine.camera.alignment.manual')
      : `${Math.round(savedAlignment.quality_score * 100)}%`
    : draftOverlayTransform
      ? t('panels.machine.camera.alignment.preview')
      : t('panels.machine.camera.alignment.none');

  return (
    <div className="grid grid-cols-2 gap-2 text-bb-text-muted">
      <div>{t('panels.machine.camera.overlay_status', { status: overlayStatus })}</div>
      <div>
        {t('panels.machine.camera.frame_status', {
          frame: frame ? `${frame.width_px}x${frame.height_px}` : t('panels.machine.camera.alignment.none'),
        })}
      </div>
      <div>
        {t('dialog.camera_calibration.title')}: {savedMapping
          ? `${Math.round(savedMapping.quality_score * 100)}%`
          : t('panels.machine.camera.alignment.none')}
      </div>
      <div>{t('panels.machine.camera.alignment_status', { alignment: alignmentText })}</div>
    </div>
  );
}

export function CameraOverlaySetupControls({ controlsEnabled }: { controlsEnabled: boolean }) {
  const { t } = useTranslation();
  const project = useProjectStore((s) => s.project);
  const overlayState = useCameraStore((s) => s.overlayState);
  const calibration = useCameraStore((s) => s.calibration);
  const alignment = useCameraStore((s) => s.alignment);
  const draftOverlayTransform = useCameraStore((s) => s.draftOverlayTransform);
  const overlayAdjustMode = useCameraStore((s) => s.overlayAdjustMode);
  const overlayDraftDirty = useCameraStore((s) => s.overlayDraftDirty);
  const beginOverlayAdjust = useCameraStore((s) => s.beginOverlayAdjust);
  const exitOverlayAdjust = useCameraStore((s) => s.exitOverlayAdjust);
  const fitDraftOverlayToWorkspace = useCameraStore((s) => s.fitDraftOverlayToWorkspace);
  const saveDraftAlignment = useCameraStore((s) => s.saveDraftAlignment);
  const discardDraftOverlay = useCameraStore((s) => s.discardDraftOverlay);

  const frame = overlayState?.frame ?? null;
  const savedMapping = calibration ?? overlayState?.calibration ?? null;
  const savedAlignment = alignment ?? overlayState?.alignment ?? null;
  const hasSavedTransform = savedAlignment !== null || savedMapping !== null;
  const workspace = project?.workspace ?? null;
  const hasDraft = draftOverlayTransform !== null;
  const canAdjust = controlsEnabled && !!frame && (hasDraft || hasSavedTransform || !!workspace);
  const canFit = controlsEnabled && !!frame && !!workspace;
  const canSave = controlsEnabled && !!draftOverlayTransform && (!hasSavedTransform || overlayDraftDirty);
  const showDiscard = !hasSavedTransform && hasDraft;

  return (
    <div className="flex gap-1 flex-wrap">
      <button
        className="px-2 py-1 rounded bg-bb-bg border border-bb-border text-bb-text hover:bg-bb-hover disabled:opacity-60"
        disabled={!canAdjust}
        onClick={() => {
          if (overlayAdjustMode) {
            exitOverlayAdjust();
          } else {
            beginOverlayAdjust(workspace);
          }
        }}
      >
        {overlayAdjustMode
          ? t('panels.machine.camera.done_adjusting')
          : t('panels.machine.camera.adjust_overlay')}
      </button>
      <button
        className="px-2 py-1 rounded bg-bb-bg border border-bb-border text-bb-text hover:bg-bb-hover disabled:opacity-60"
        disabled={!canFit}
        onClick={() => fitDraftOverlayToWorkspace(workspace)}
      >
        {t('panels.machine.camera.fit_to_bed')}
      </button>
      <button
        className="px-2 py-1 rounded bg-bb-bg border border-bb-border text-bb-text hover:bg-bb-hover disabled:opacity-60"
        disabled={!canSave}
        onClick={() => void saveDraftAlignment()}
      >
        {t('panels.machine.camera.save_alignment')}
      </button>
      {showDiscard && (
        <button
          className="px-2 py-1 rounded bg-bb-bg border border-bb-border text-bb-text hover:bg-bb-hover disabled:opacity-60"
          disabled={!controlsEnabled}
          onClick={() => discardDraftOverlay()}
        >
          {t('panels.machine.camera.discard_preview')}
        </button>
      )}
    </div>
  );
}
