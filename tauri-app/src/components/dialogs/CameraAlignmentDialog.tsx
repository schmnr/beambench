import { useState, type MouseEvent } from 'react';
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

function defaultPoints(frameWidth: number, frameHeight: number, workspace: Workspace | null): AlignmentPoint[] {
  const bedWidth = workspace?.bed_width_mm ?? 100;
  const bedHeight = workspace?.bed_height_mm ?? 100;
  const pairs = [
    { camera: { x: 0, y: 0 }, canvas: { x: 0, y: 0 } },
    { camera: { x: frameWidth, y: 0 }, canvas: { x: bedWidth, y: 0 } },
    { camera: { x: frameWidth, y: frameHeight }, canvas: { x: bedWidth, y: bedHeight } },
    { camera: { x: 0, y: frameHeight }, canvas: { x: 0, y: bedHeight } },
  ];
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

  const [points, setPoints] = useState<AlignmentPoint[]>(() =>
    defaultPoints(frameWidth, frameHeight, workspace));
  const [activePointIndex, setActivePointIndex] = useState(0);
  const [solvedAlignment, setSolvedAlignment] = useState<CameraAlignment | null>(alignment);

  const updatePoint = (index: number, patch: Partial<AlignmentPoint>) => {
    setSolvedAlignment(null);
    setPoints((current) =>
      current.map((point, pointIndex) =>
        pointIndex === index ? { ...point, ...patch } : point,
      ),
    );
  };

  const addPoint = () => {
    setSolvedAlignment(null);
    setPoints((current) => [
      ...current,
      { camera_x: 0, camera_y: 0, workspace_x_mm: 0, workspace_y_mm: 0 },
    ]);
  };

  const removePoint = (index: number) => {
    setSolvedAlignment(null);
    setPoints((current) => current.filter((_, pointIndex) => pointIndex !== index));
    setActivePointIndex((current) => Math.min(current, points.length - 2));
  };

  const pickImagePoint = (event: MouseEvent<HTMLButtonElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return;
    const cameraX = ((event.clientX - rect.left) / rect.width) * frameWidth;
    const cameraY = ((event.clientY - rect.top) / rect.height) * frameHeight;
    updatePoint(activePointIndex, { camera_x: cameraX, camera_y: cameraY });
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
      onClose();
    } catch {
      // Store already surfaced the error; keep the dialog open for retry/inspection.
    }
  };

  const handleReset = async () => {
    const reset = await resetAlignment();
    if (reset) {
      setSolvedAlignment(null);
    }
  };

  return createPortal(
    <div role="dialog" aria-modal="true" aria-labelledby="dialog-title" className="fixed inset-0 bg-black/50 flex items-center justify-center z-50" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <div className="bg-bb-panel border border-bb-border rounded-lg shadow-xl p-4 min-w-[640px] max-h-[80vh] overflow-y-auto">
        <div className="flex items-center justify-between mb-3">
          <h2 id="dialog-title" className="text-sm font-semibold text-bb-text">{t('dialog.camera_alignment.title')}</h2>
          <button className="text-xs px-2 py-1 rounded bg-bb-hover text-bb-text hover:bg-bb-border" onClick={onClose}>
            {t('common.close')}
          </button>
        </div>

        <div className="text-xs text-bb-text-muted mb-3">
          {t('dialog.camera_alignment.help')}
        </div>

        {frameUrl && (
          <div className="flex justify-center mb-3">
            <button
              type="button"
              className="relative block max-w-full overflow-hidden border border-bb-border bg-bb-bg cursor-crosshair"
              aria-label={t('dialog.camera_alignment.point', { index: activePointIndex + 1 })}
              onClick={pickImagePoint}
            >
              <img
                src={frameUrl}
                alt=""
                draggable={false}
                className="block max-h-64 max-w-full w-auto h-auto"
              />
              {points.map((point, index) => (
                <span
                  key={index}
                  className={`absolute w-3 h-3 -translate-x-1/2 -translate-y-1/2 border-2 rounded-full pointer-events-none ${index === activePointIndex ? 'border-bb-accent bg-bb-bg' : 'border-white bg-black/60'}`}
                  style={{
                    left: `${Math.max(0, Math.min(100, (point.camera_x / frameWidth) * 100))}%`,
                    top: `${Math.max(0, Math.min(100, (point.camera_y / frameHeight) * 100))}%`,
                  }}
                />
              ))}
            </button>
          </div>
        )}

        <div className="space-y-3">
          {points.map((point, index) => (
            <div key={index} className="border border-bb-border rounded p-2">
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
                <NumberInput label={t('dialog.camera_alignment.camera_x')} value={point.camera_x} onChange={(value) => updatePoint(index, { camera_x: value })} step={0.1} />
                <NumberInput label={t('dialog.camera_alignment.camera_y')} value={point.camera_y} onChange={(value) => updatePoint(index, { camera_y: value })} step={0.1} />
                <NumberInput label={labelWithUnit(t('dialog.camera_alignment.workspace_x'), lengthUnitLabel(displayUnit))} value={roundDisplayLength(mmToDisplay(point.workspace_x_mm, displayUnit), displayUnit)} onChange={(value) => updatePoint(index, { workspace_x_mm: displayToMm(value, displayUnit) })} step={lengthStep(displayUnit, 0.1, 0.005)} />
                <NumberInput label={labelWithUnit(t('dialog.camera_alignment.workspace_y'), lengthUnitLabel(displayUnit))} value={roundDisplayLength(mmToDisplay(point.workspace_y_mm, displayUnit), displayUnit)} onChange={(value) => updatePoint(index, { workspace_y_mm: displayToMm(value, displayUnit) })} step={lengthStep(displayUnit, 0.1, 0.005)} />
              </div>
            </div>
          ))}
        </div>

        <div className="flex gap-2 mt-3">
          <button className="px-2 py-1 rounded bg-bb-hover text-bb-text hover:bg-bb-border" onClick={addPoint}>
            {t('dialog.camera_alignment.add_point')}
          </button>
          <button className="px-2 py-1 rounded bg-bb-accent text-bb-on-accent hover:bg-bb-accent-hover disabled:opacity-60" disabled={points.length < 3} onClick={() => void handleSolve()}>
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
          <div className="mt-4 text-xs bg-bb-bg border border-bb-border rounded p-3 text-bb-text-muted">
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
