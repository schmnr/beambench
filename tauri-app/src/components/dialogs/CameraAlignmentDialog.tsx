import { useEffect, useMemo, useState, type KeyboardEvent, type MouseEvent } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { useCameraStore } from '../../stores/cameraStore';
import { useProjectStore } from '../../stores/projectStore';
import { useAppStore } from '../../stores/appStore';
import type { AlignmentPoint, CameraAlignment } from '../../types/camera';
import type { Workspace } from '../../types/project';
import { cameraFrameAssetUrl } from '../../services/cameraFrameAsset';
import { canvasToMachinePoint, machineToCanvasPoint } from '../../utils/workspaceCoordinates';
import { NumberInput } from '../shared/NumberInput';
import { mmToDisplay, displayToMm, roundDisplayLength, lengthStep, lengthUnitLabel, labelWithUnit } from '../../utils/lengthUnits';

interface CameraAlignmentDialogProps {
  onClose: () => void;
}

interface CameraAlignmentDraft {
  points: AlignmentPoint[];
  activePointIndex: number;
  pickedPointIndexes: number[];
}

const alignmentDrafts = new Map<string, CameraAlignmentDraft>();

export function resetCameraAlignmentDraftsForTests() {
  alignmentDrafts.clear();
}

function defaultPoints(
  frameWidth: number,
  frameHeight: number,
  workspace: Workspace | null,
  pointCount: 4 | 9,
): AlignmentPoint[] {
  const bedWidth = workspace?.bed_width_mm ?? 100;
  const bedHeight = workspace?.bed_height_mm ?? 100;
  const ratios = pointCount === 9 ? [0.1, 0.5, 0.9] : [0.08, 0.92];
  const pairs = ratios.flatMap((yRatio) => ratios.map((xRatio) => ({
    camera: { x: frameWidth * xRatio, y: frameHeight * yRatio },
    canvas: { x: bedWidth * xRatio, y: bedHeight * yRatio },
  })));
  return pairs.map(({ camera, canvas }) => {
    const machine = workspace ? canvasToMachinePoint(canvas, workspace) : canvas;
    return {
      camera_x: camera.x,
      camera_y: camera.y,
      workspace_x_mm: machine.x,
      workspace_y_mm: machine.y,
    };
  });
}

export function CameraAlignmentDialog({ onClose }: CameraAlignmentDialogProps) {
  const { t } = useTranslation();
  const displayUnit = useAppStore((s) => s.settings?.display_unit) ?? 'mm';
  const alignment = useCameraStore((s) => s.alignment);
  const overlayState = useCameraStore((s) => s.overlayState);
  const solveAlignment = useCameraStore((s) => s.solveAlignment);
  const saveAlignment = useCameraStore((s) => s.saveAlignment);
  const resetAlignment = useCameraStore((s) => s.resetAlignment);
  const workspace = useProjectStore((s) => s.project?.workspace ?? null);
  const frame = overlayState?.frame ?? null;
  const frameWidth = frame?.width_px ?? 100;
  const frameHeight = frame?.height_px ?? 100;
  const frameUrl = frame ? cameraFrameAssetUrl(frame.file_path, frame.handle_id) : null;
  const draftKey = frame && overlayState?.selected_camera_id
    ? `${overlayState.selected_camera_id}:${frame.handle_id}`
    : null;
  const restoredDraft = draftKey ? alignmentDrafts.get(draftKey) : null;

  const [points, setPoints] = useState<AlignmentPoint[]>(() =>
    restoredDraft?.points ?? defaultPoints(frameWidth, frameHeight, workspace, 9));
  const [activePointIndex, setActivePointIndex] = useState(restoredDraft?.activePointIndex ?? 0);
  const [pickedPointIndexes, setPickedPointIndexes] = useState<Set<number>>(
    () => new Set(restoredDraft?.pickedPointIndexes ?? []),
  );
  const [draftWasRestored] = useState(Boolean(restoredDraft));
  const [draftDirty, setDraftDirty] = useState(Boolean(restoredDraft));
  const [solvedAlignment, setSolvedAlignment] = useState<CameraAlignment | null>(alignment);

  useEffect(() => {
    if (!draftKey || !draftDirty) return;
    alignmentDrafts.set(draftKey, {
      points,
      activePointIndex,
      pickedPointIndexes: [...pickedPointIndexes],
    });
  }, [activePointIndex, draftDirty, draftKey, pickedPointIndexes, points]);

  const applyPointLayout = (pointCount: 4 | 9) => {
    setPoints(defaultPoints(frameWidth, frameHeight, workspace, pointCount));
    setActivePointIndex(0);
    setPickedPointIndexes(new Set());
    setDraftDirty(true);
    setSolvedAlignment(null);
  };

  const updatePoint = (index: number, patch: Partial<AlignmentPoint>) => {
    setSolvedAlignment(null);
    setDraftDirty(true);
    if ('camera_x' in patch || 'camera_y' in patch) {
      setPickedPointIndexes((current) => new Set(current).add(index));
    }
    setPoints((current) =>
      current.map((point, pointIndex) =>
        pointIndex === index ? { ...point, ...patch } : point,
      ),
    );
  };

  const addPoint = () => {
    setSolvedAlignment(null);
    setDraftDirty(true);
    setPoints((current) => [
      ...current,
      { camera_x: 0, camera_y: 0, workspace_x_mm: 0, workspace_y_mm: 0 },
    ]);
  };

  const removePoint = (index: number) => {
    setSolvedAlignment(null);
    setDraftDirty(true);
    setPoints((current) => current.filter((_, pointIndex) => pointIndex !== index));
    setPickedPointIndexes(new Set());
    setActivePointIndex((current) => Math.min(current, points.length - 2));
  };

  const pickImagePoint = (event: MouseEvent<HTMLButtonElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return;
    const cameraX = ((event.clientX - rect.left) / rect.width) * frameWidth;
    const cameraY = ((event.clientY - rect.top) / rect.height) * frameHeight;
    updatePoint(activePointIndex, { camera_x: cameraX, camera_y: cameraY });
    setActivePointIndex((current) => Math.min(current + 1, points.length - 1));
  };

  const validationIssues = useMemo(() => {
    const issues: string[] = [];
    if (frame && pickedPointIndexes.size < points.length) {
      issues.push(t('dialog.camera_alignment.validation.pick_all', {
        picked: pickedPointIndexes.size,
        total: points.length,
      }));
    }

    const cameraKeys = points.map((point) => `${point.camera_x.toFixed(2)}:${point.camera_y.toFixed(2)}`);
    if (new Set(cameraKeys).size !== cameraKeys.length) {
      issues.push(t('dialog.camera_alignment.validation.duplicate_camera'));
    }
    const workspaceKeys = points.map((point) =>
      `${point.workspace_x_mm.toFixed(3)}:${point.workspace_y_mm.toFixed(3)}`);
    if (new Set(workspaceKeys).size !== workspaceKeys.length) {
      issues.push(t('dialog.camera_alignment.validation.duplicate_workspace'));
    }

    const cameraXs = points.map((point) => point.camera_x);
    const cameraYs = points.map((point) => point.camera_y);
    const cameraSpreadX = Math.max(...cameraXs) - Math.min(...cameraXs);
    const cameraSpreadY = Math.max(...cameraYs) - Math.min(...cameraYs);
    if (frame && (
      cameraSpreadX < frameWidth * 0.15
      || cameraSpreadY < frameHeight * 0.15
    )) {
      issues.push(t('dialog.camera_alignment.validation.weak_camera_spread'));
    }

    const workspaceXs = points.map((point) => point.workspace_x_mm);
    const workspaceYs = points.map((point) => point.workspace_y_mm);
    const workspaceSpreadX = Math.max(...workspaceXs) - Math.min(...workspaceXs);
    const workspaceSpreadY = Math.max(...workspaceYs) - Math.min(...workspaceYs);
    if (workspace && (
      workspaceSpreadX < workspace.bed_width_mm * 0.15
      || workspaceSpreadY < workspace.bed_height_mm * 0.15
    )) {
      issues.push(t('dialog.camera_alignment.validation.weak_workspace_spread'));
    }

    if (workspace && points.some((point) => (
      point.workspace_x_mm < 0
      || point.workspace_y_mm < 0
      || point.workspace_x_mm > workspace.bed_width_mm
      || point.workspace_y_mm > workspace.bed_height_mm
    ))) {
      issues.push(t('dialog.camera_alignment.validation.outside_workspace'));
    }
    return issues;
  }, [frame, frameHeight, frameWidth, pickedPointIndexes, points, t, workspace]);

  const handleInputKeyDown = (event: KeyboardEvent<HTMLInputElement>, pointIndex: number) => {
    if (event.key !== 'Enter') return;
    event.preventDefault();
    event.stopPropagation();
    const dialog = event.currentTarget.closest('[role="dialog"]');
    const inputs = dialog
      ? [...dialog.querySelectorAll<HTMLInputElement>('input[type="number"]')]
      : [];
    const currentIndex = inputs.indexOf(event.currentTarget);
    const nextInput = currentIndex >= 0 ? inputs[currentIndex + 1] : null;
    if (nextInput) {
      nextInput.focus();
      nextInput.select();
      setActivePointIndex(Math.min(Math.floor((currentIndex + 1) / 4), points.length - 1));
    } else {
      event.currentTarget.blur();
      setActivePointIndex(pointIndex);
    }
  };

  const handleSolve = async () => {
    const solvePoints = workspace
      ? points.map((point) => {
          const canvasPoint = machineToCanvasPoint(
            { x: point.workspace_x_mm, y: point.workspace_y_mm },
            workspace,
          );
          return {
            ...point,
            workspace_x_mm: canvasPoint.x,
            workspace_y_mm: canvasPoint.y,
          };
        })
      : points;
    const next = await solveAlignment({ points: solvePoints });
    setSolvedAlignment(next);
  };

  const handleSave = async () => {
    if (!solvedAlignment) return;
    try {
      await saveAlignment(solvedAlignment);
      if (draftKey) alignmentDrafts.delete(draftKey);
      onClose();
    } catch {
      // Store already surfaced the error; keep the dialog open for retry/inspection.
    }
  };

  const handleReset = async () => {
    const reset = await resetAlignment();
    if (reset) {
      if (draftKey) alignmentDrafts.delete(draftKey);
      setPoints(defaultPoints(frameWidth, frameHeight, workspace, points.length === 4 ? 4 : 9));
      setPickedPointIndexes(new Set());
      setActivePointIndex(0);
      setDraftDirty(false);
      setSolvedAlignment(null);
    }
  };

  const activePoint = points[activePointIndex] ?? points[0];
  const bedWidth = workspace?.bed_width_mm ?? 100;
  const bedHeight = workspace?.bed_height_mm ?? 100;
  const canSolve = points.length >= 4 && validationIssues.length === 0;

  return createPortal(
    <div role="dialog" aria-modal="true" aria-labelledby="dialog-title" className="fixed inset-0 bg-black/50 flex items-center justify-center z-50" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <div className="bg-bb-panel border border-bb-border rounded-lg shadow-xl p-4 w-[min(920px,calc(100vw-2rem))] max-h-[88vh] overflow-y-auto">
        <div className="flex items-center justify-between mb-3">
          <h2 id="dialog-title" className="text-sm font-semibold text-bb-text">{t('dialog.camera_alignment.title')}</h2>
          <button className="text-xs px-2 py-1 rounded bg-bb-hover text-bb-text hover:bg-bb-border" onClick={onClose}>
            {t('common.close')}
          </button>
        </div>

        <div className="text-xs text-bb-text-muted mb-3">
          {t('dialog.camera_alignment.help')}
        </div>

        {draftWasRestored && (
          <div className="mb-3 rounded border border-bb-info-border bg-bb-info-bg px-3 py-2 text-xs text-bb-info-fg">
            {t('dialog.camera_alignment.draft_restored')}
          </div>
        )}

        <div className="mb-3 flex items-center justify-between gap-3 text-xs">
          <div className="font-medium text-bb-text">
            {t('dialog.camera_alignment.progress', {
              current: activePointIndex + 1,
              total: points.length,
            })}
          </div>
          <div className="text-bb-text-muted">
            {t('dialog.camera_alignment.picked_progress', {
              picked: pickedPointIndexes.size,
              total: points.length,
            })}
          </div>
        </div>

        <div className="flex gap-2 mb-3" role="group" aria-label={t('dialog.camera_alignment.layout')}>
          <button
            type="button"
            className="px-2 py-1 rounded bg-bb-hover text-bb-text hover:bg-bb-border"
            onClick={() => applyPointLayout(4)}
          >
            {t('dialog.camera_alignment.quick_points')}
          </button>
          <button
            type="button"
            className="px-2 py-1 rounded bg-bb-accent text-bb-on-accent hover:bg-bb-accent-hover"
            onClick={() => applyPointLayout(9)}
          >
            {t('dialog.camera_alignment.wide_angle_points')}
          </button>
        </div>

        {frameUrl && activePoint && (
          <div className="grid grid-cols-1 gap-3 mb-3 md:grid-cols-[minmax(0,1fr)_15rem]">
            <div>
              <div className="mb-1 text-xs text-bb-text-muted">
                {t('dialog.camera_alignment.click_instruction', { index: activePointIndex + 1 })}
              </div>
              <div className="flex justify-center">
                <button
                  type="button"
                  className="relative block max-w-full overflow-hidden rounded border border-bb-control-border bg-bb-bg cursor-crosshair"
                  aria-label={t('dialog.camera_alignment.point', { index: activePointIndex + 1 })}
                  onClick={pickImagePoint}
                >
                  <img
                    src={frameUrl}
                    alt=""
                    draggable={false}
                    className="block max-h-72 max-w-full w-auto h-auto"
                  />
                  {points.map((point, index) => pickedPointIndexes.has(index) && (
                    <span
                      key={index}
                      className={`absolute flex h-5 w-5 -translate-x-1/2 -translate-y-1/2 items-center justify-center rounded-full border-2 text-[10px] font-semibold pointer-events-none ${index === activePointIndex ? 'border-bb-accent bg-bb-bg text-bb-accent' : 'border-bb-success bg-bb-success text-bb-on-success'}`}
                      style={{
                        left: `${Math.max(0, Math.min(100, (point.camera_x / frameWidth) * 100))}%`,
                        top: `${Math.max(0, Math.min(100, (point.camera_y / frameHeight) * 100))}%`,
                      }}
                    >
                      {index + 1}
                    </span>
                  ))}
                </button>
              </div>
            </div>

            <div>
              <div className="mb-1 text-xs text-bb-text-muted">
                {t('dialog.camera_alignment.bed_reference')}
              </div>
              <div className="relative aspect-[4/3] rounded border border-bb-control-border bg-bb-bg">
                {points.map((point, index) => {
                  const left = Math.max(0, Math.min(100, (point.workspace_x_mm / bedWidth) * 100));
                  const rawTop = Math.max(0, Math.min(100, (point.workspace_y_mm / bedHeight) * 100));
                  const top = workspace?.origin === 'bottom_left' ? 100 - rawTop : rawTop;
                  return (
                    <button
                      key={index}
                      type="button"
                      className={`absolute flex h-5 w-5 -translate-x-1/2 -translate-y-1/2 items-center justify-center rounded-full border-2 text-[10px] font-semibold ${index === activePointIndex ? 'border-bb-accent bg-bb-bg text-bb-accent' : pickedPointIndexes.has(index) ? 'border-bb-success bg-bb-success text-bb-on-success' : 'border-bb-control-border bg-bb-surface text-bb-text'}`}
                      style={{ left: `${left}%`, top: `${top}%` }}
                      onClick={() => setActivePointIndex(index)}
                      aria-label={t('dialog.camera_alignment.select_bed_point', { index: index + 1 })}
                    >
                      {index + 1}
                    </button>
                  );
                })}
              </div>
              <div className="mt-2 text-xs text-bb-text-muted">
                {t('dialog.camera_alignment.active_coordinate', {
                  x: roundDisplayLength(mmToDisplay(activePoint.workspace_x_mm, displayUnit), displayUnit),
                  y: roundDisplayLength(mmToDisplay(activePoint.workspace_y_mm, displayUnit), displayUnit),
                  unit: lengthUnitLabel(displayUnit),
                })}
              </div>
            </div>
          </div>
        )}

        {validationIssues.length > 0 && (
          <div className="mb-3 rounded border border-bb-warning-border bg-bb-warning-bg px-3 py-2 text-xs text-bb-warning-fg">
            <div className="font-medium">{t('dialog.camera_alignment.check_points')}</div>
            <ul className="mt-1 list-disc space-y-0.5 pl-4">
              {validationIssues.map((issue) => <li key={issue}>{issue}</li>)}
            </ul>
          </div>
        )}

        <div className="space-y-3">
          {points.map((point, index) => (
            <div
              key={index}
              data-alignment-point-index={index}
              className={`border rounded p-2 ${index === activePointIndex ? 'border-bb-accent bg-bb-accent/5' : 'border-bb-border'}`}
            >
              <div className="flex items-center justify-between mb-2">
                <label className="flex items-center gap-2 text-xs font-medium text-bb-text">
                  <input
                    type="radio"
                    name="camera-alignment-point"
                    checked={index === activePointIndex}
                    onChange={() => setActivePointIndex(index)}
                  />
                  {t('dialog.camera_alignment.point', { index: index + 1 })}
                </label>
                {points.length > 3 && (
                  <button
                    className="text-xs px-2 py-1 rounded bg-bb-hover text-bb-text hover:bg-bb-border"
                    onClick={() => removePoint(index)}
                  >
                    {t('dialog.camera_alignment.remove')}
                  </button>
                )}
              </div>
              <div className="grid grid-cols-2 gap-2">
                <NumberInput label={t('dialog.camera_alignment.camera_x')} value={point.camera_x} onChange={(value) => updatePoint(index, { camera_x: value })} onKeyDown={(event) => handleInputKeyDown(event, index)} step={0.1} />
                <NumberInput label={t('dialog.camera_alignment.camera_y')} value={point.camera_y} onChange={(value) => updatePoint(index, { camera_y: value })} onKeyDown={(event) => handleInputKeyDown(event, index)} step={0.1} />
                <NumberInput label={labelWithUnit(t('dialog.camera_alignment.workspace_x'), lengthUnitLabel(displayUnit))} value={roundDisplayLength(mmToDisplay(point.workspace_x_mm, displayUnit), displayUnit)} onChange={(value) => updatePoint(index, { workspace_x_mm: displayToMm(value, displayUnit) })} onKeyDown={(event) => handleInputKeyDown(event, index)} step={lengthStep(displayUnit, 0.1, 0.005)} />
                <NumberInput label={labelWithUnit(t('dialog.camera_alignment.workspace_y'), lengthUnitLabel(displayUnit))} value={roundDisplayLength(mmToDisplay(point.workspace_y_mm, displayUnit), displayUnit)} onChange={(value) => updatePoint(index, { workspace_y_mm: displayToMm(value, displayUnit) })} onKeyDown={(event) => handleInputKeyDown(event, index)} step={lengthStep(displayUnit, 0.1, 0.005)} />
              </div>
            </div>
          ))}
        </div>

        <div className="flex gap-2 mt-3">
          <button className="px-2 py-1 rounded bg-bb-hover text-bb-text hover:bg-bb-border" onClick={addPoint}>
            {t('dialog.camera_alignment.add_point')}
          </button>
          <button className="px-2 py-1 rounded bg-bb-accent text-bb-on-accent hover:bg-bb-accent-hover disabled:opacity-60" disabled={!canSolve} onClick={() => void handleSolve()}>
            {t('dialog.camera_alignment.solve')}
          </button>
          <button className="px-2 py-1 rounded bg-bb-hover text-bb-text hover:bg-bb-border disabled:opacity-60" disabled={!solvedAlignment} onClick={() => void handleSave()}>
            {t('dialog.camera_alignment.save_alignment')}
          </button>
          <button className="px-2 py-1 rounded bg-bb-hover text-bb-text hover:bg-bb-border disabled:opacity-60" disabled={!alignment} onClick={() => void handleReset()}>
            {t('dialog.camera_alignment.reset_saved')}
          </button>
        </div>

        {solvedAlignment && (
          <div className={`mt-4 text-xs border rounded p-3 ${solvedAlignment.quality_score >= 0.9 ? 'border-bb-success-border bg-bb-success-bg text-bb-success-fg' : solvedAlignment.quality_score >= 0.7 ? 'border-bb-warning-border bg-bb-warning-bg text-bb-warning-fg' : 'border-bb-error-border bg-bb-error-bg text-bb-error-fg'}`}>
            <div className="mb-1 font-medium">
              {t(solvedAlignment.quality_score >= 0.9
                ? 'dialog.camera_alignment.quality_good'
                : solvedAlignment.quality_score >= 0.7
                  ? 'dialog.camera_alignment.quality_review'
                  : 'dialog.camera_alignment.quality_retry')}
            </div>
            <div>{t('dialog.camera_alignment.quality', { value: (solvedAlignment.quality_score * 100).toFixed(1) })}</div>
            <div>{t('dialog.camera_alignment.rmse', { value: solvedAlignment.rmse_mm.toFixed(3) })}</div>
            <div>{t('dialog.camera_alignment.scale', { value: solvedAlignment.transform.scale.toFixed(5) })}</div>
            <div>{t('dialog.camera_alignment.rotation', { value: solvedAlignment.transform.rotation_deg.toFixed(3) })}</div>
            <div>
              {t('dialog.camera_alignment.translation', {
                x: solvedAlignment.transform.translation_x.toFixed(3),
                y: solvedAlignment.transform.translation_y.toFixed(3),
              })}
            </div>
          </div>
        )}
      </div>
    </div>,
    document.body,
  );
}
