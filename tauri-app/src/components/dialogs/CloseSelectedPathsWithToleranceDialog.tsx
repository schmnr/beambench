import { useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { projectService } from '../../services/projectService';
import { useProjectStore } from '../../stores/projectStore';
import { usePreviewStore } from '../../stores/previewStore';
import { useUndoStore } from '../../stores/undoStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { useAppStore } from '../../stores/appStore';
import { wrapBackendError } from '../../i18n/errors';
import { mmToDisplay, roundDisplayLength, lengthUnitLabel } from '../../utils/lengthUnits';
import { useFocusTrap } from '../../hooks/useFocusTrap';

interface CloseSelectedPathsWithToleranceDialogProps {
  objectIds: string[];
  onClose: () => void;
}

type CloseMode = 'move_ends_together' | 'join_with_line';

// The slider operates in millimeters internally (the backend expects mm);
// only the labels are converted to the user's display unit.
const SLIDER_MIN_MM = 0.01;
const SLIDER_MAX_MM = 5;
const SLIDER_STEP_MM = 0.01;

export function CloseSelectedPathsWithToleranceDialog({
  objectIds,
  onClose,
}: CloseSelectedPathsWithToleranceDialogProps) {
  const { t } = useTranslation();
  const displayUnit = useAppStore((s) => s.settings?.display_unit) ?? 'mm';
  const [distanceThreshold, setDistanceThreshold] = useState(0.5);
  const [mode, setMode] = useState<CloseMode>('move_ends_together');
  const [status, setStatus] = useState({
    openShapesFound: 0,
    shapesClosed: 0,
    remainingOpen: 0,
  });
  const [busy, setBusy] = useState(false);
  const dialogRef = useRef<HTMLDivElement>(null);
  useFocusTrap(dialogRef, true);

  const thresholdLabel = useMemo(() => {
    const display = mmToDisplay(distanceThreshold, displayUnit);
    const text = display.toFixed(displayUnit === 'inches' ? 4 : 2);
    return `${text} ${lengthUnitLabel(displayUnit)}`;
  }, [distanceThreshold, displayUnit]);

  useEffect(() => {
    let cancelled = false;
    const timer = window.setTimeout(() => {
      void projectService.countOpenPathsWithTolerance(objectIds, distanceThreshold, mode)
        .then((result) => {
          if (!cancelled) setStatus(result);
        })
        .catch((error) => {
          if (!cancelled) useNotificationStore.getState().push(wrapBackendError(String(error)), 'error');
        });
    }, 150);

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [objectIds, distanceThreshold, mode]);

  const handleApply = async () => {
    setBusy(true);
    try {
      const result = await projectService.closeSelectedPathsWithTolerance(
        objectIds,
        distanceThreshold,
        mode,
      );
      setStatus(result);
      await useProjectStore.getState().loadProject({ invalidatePreview: true });
      usePreviewStore.getState().invalidate();
      await useUndoStore.getState().refresh();
    } catch (error) {
      useNotificationStore.getState().push(wrapBackendError(String(error)), 'error');
    } finally {
      setBusy(false);
    }
  };

  return createPortal(
    <div
      ref={dialogRef}
      role="dialog"
      aria-modal="true"
      aria-labelledby="close-selected-paths-title"
      className="fixed inset-0 bg-black/50 flex items-center justify-center z-50"
      onClick={(event) => { if (event.target === event.currentTarget) onClose(); }}
    >
      <div className="bg-bb-panel border border-bb-border rounded-lg shadow-xl p-4 min-w-[340px]">
        <h2 id="close-selected-paths-title" className="text-sm font-semibold text-bb-text mb-3">
          {t('dialog.close_paths.title')}
        </h2>
        <label className="block text-xs text-bb-text-muted mb-1" htmlFor="distance-threshold">
          {t('dialog.close_paths.distance_threshold')}
        </label>
        <div className="flex items-start gap-3">
          <div className="flex-1">
            <input
              id="distance-threshold"
              type="range"
              min={SLIDER_MIN_MM}
              max={SLIDER_MAX_MM}
              step={SLIDER_STEP_MM}
              value={distanceThreshold}
              onChange={(event) => setDistanceThreshold(Number(event.currentTarget.value))}
              className="w-full"
            />
            <div className="flex justify-between text-[10px] tabular-nums text-bb-text-dim">
              <span>{roundDisplayLength(mmToDisplay(SLIDER_MIN_MM, displayUnit), displayUnit)}</span>
              <span>{roundDisplayLength(mmToDisplay(SLIDER_MAX_MM, displayUnit), displayUnit)}</span>
            </div>
          </div>
          <span className="text-xs tabular-nums text-bb-text-muted w-20 text-right">{thresholdLabel}</span>
        </div>
        <div className="mt-3 space-y-1 text-xs text-bb-text">
          <label className="flex items-center gap-2">
            <input
              type="radio"
              checked={mode === 'move_ends_together'}
              onChange={() => setMode('move_ends_together')}
            />
            {t('dialog.close_paths.mode_move_ends')}
          </label>
          <label className="flex items-center gap-2">
            <input
              type="radio"
              checked={mode === 'join_with_line'}
              onChange={() => setMode('join_with_line')}
            />
            {t('dialog.close_paths.mode_join_line')}
          </label>
        </div>
        <div className="mt-3 rounded bg-bb-bg px-3 py-2 text-xs text-bb-text-muted space-y-1">
          <div>{t('dialog.close_paths.open_shapes_found', { count: status.openShapesFound })}</div>
          <div>{t('dialog.close_paths.shapes_closed', { count: status.shapesClosed })}</div>
          <div>{t('dialog.close_paths.remaining_open', { count: status.remainingOpen })}</div>
        </div>
        <div className="flex justify-end gap-2 mt-4">
          <button onClick={onClose} className="px-3 py-1 text-xs font-medium rounded bg-bb-bg hover:bg-bb-hover text-bb-text">
            {t('common.cancel')}
          </button>
          <button
            onClick={() => void handleApply()}
            disabled={busy}
            className="px-3 py-1 text-xs font-medium rounded bg-bb-accent hover:bg-bb-accent-hover text-bb-on-accent disabled:opacity-60"
          >
            {t('common.apply')}
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
