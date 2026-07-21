import { useMemo, useState } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { useCameraStore } from '../../stores/cameraStore';
import type { CalibrationPoint, CameraCalibration } from '../../types/camera';
import { NumberInput } from '../shared/NumberInput';

interface CameraCalibrationDialogProps {
  cameraId: string;
  onClose: () => void;
}

const DEFAULT_POINTS: CalibrationPoint[] = [
  { image_x: 100, image_y: 100, reference_x: 10, reference_y: 10 },
  { image_x: 500, image_y: 100, reference_x: 50, reference_y: 10 },
  { image_x: 100, image_y: 500, reference_x: 10, reference_y: 50 },
];

export function CameraCalibrationDialog({ cameraId, onClose }: CameraCalibrationDialogProps) {
  const { t } = useTranslation();
  const overlayState = useCameraStore((s) => s.overlayState);
  const calibration = useCameraStore((s) => s.calibration);
  const solveCalibration = useCameraStore((s) => s.solveCalibration);
  const saveCalibration = useCameraStore((s) => s.saveCalibration);
  const resetCalibration = useCameraStore((s) => s.resetCalibration);

  const [points, setPoints] = useState<CalibrationPoint[]>(DEFAULT_POINTS);
  const [solvedCalibration, setSolvedCalibration] = useState<CameraCalibration | null>(calibration);

  const imageWidth = overlayState?.frame?.width_px ?? calibration?.image_width_px ?? 1920;
  const imageHeight = overlayState?.frame?.height_px ?? calibration?.image_height_px ?? 1080;

  const canSolve = useMemo(
    () => points.filter((point) => (
      Number.isFinite(point.image_x)
      && Number.isFinite(point.image_y)
      && Number.isFinite(point.reference_x)
      && Number.isFinite(point.reference_y)
    )).length >= 3,
    [points],
  );

  const updatePoint = (index: number, patch: Partial<CalibrationPoint>) => {
    setSolvedCalibration(null);
    setPoints((current) =>
      current.map((point, pointIndex) =>
        pointIndex === index ? { ...point, ...patch } : point,
      ),
    );
  };

  const addPoint = () => {
    setSolvedCalibration(null);
    setPoints((current) => [
      ...current,
      { image_x: 0, image_y: 0, reference_x: 0, reference_y: 0 },
    ]);
  };

  const removePoint = (index: number) => {
    setSolvedCalibration(null);
    setPoints((current) => current.filter((_, pointIndex) => pointIndex !== index));
  };

  const handleSolve = async () => {
    const next = await solveCalibration(cameraId, {
      image_width_px: imageWidth,
      image_height_px: imageHeight,
      points,
    });
    setSolvedCalibration(next);
  };

  const handleSave = async () => {
    if (!solvedCalibration) return;
    try {
      await saveCalibration(cameraId, solvedCalibration);
      onClose();
    } catch {
      // Store already surfaced the error; keep the dialog open for retry/inspection.
    }
  };

  const handleReset = async () => {
    const reset = await resetCalibration();
    if (reset) {
      setSolvedCalibration(null);
    }
  };

  return createPortal(
    <div role="dialog" aria-modal="true" aria-labelledby="dialog-title" className="fixed inset-0 bg-black/50 flex items-center justify-center z-50" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <div className="bg-bb-panel border border-bb-border rounded-lg shadow-xl p-4 min-w-[640px] max-h-[80vh] overflow-y-auto">
        <div className="flex items-center justify-between mb-3">
          <h2 id="dialog-title" className="text-sm font-semibold text-bb-text">{t('dialog.camera_calibration.title')}</h2>
          <button className="text-xs px-2 py-1 rounded bg-bb-hover text-bb-text hover:bg-bb-border" onClick={onClose}>
            {t('common.close')}
          </button>
        </div>

        <div className="text-xs text-bb-text-muted mb-3">
          {t('dialog.camera_calibration.help')}
        </div>

        <div className="text-xs text-bb-text-muted mb-3">
          {t('dialog.camera_calibration.frame_size', { width: imageWidth, height: imageHeight })}
        </div>

        <div className="space-y-3">
          {points.map((point, index) => (
            <div key={index} className="border border-bb-border rounded p-2">
              <div className="flex items-center justify-between mb-2">
                <div className="text-xs font-medium text-bb-text">{t('dialog.camera_calibration.point', { index: index + 1 })}</div>
                {points.length > 3 && (
                  <button
                    className="text-xs px-2 py-1 rounded bg-bb-hover text-bb-text hover:bg-bb-border"
                    onClick={() => removePoint(index)}
                  >
                    {t('dialog.camera_calibration.remove')}
                  </button>
                )}
              </div>
              <div className="grid grid-cols-2 gap-2">
                <NumberInput label={t('dialog.camera_calibration.image_x')} value={point.image_x} onChange={(value) => updatePoint(index, { image_x: value })} step={1} />
                <NumberInput label={t('dialog.camera_calibration.image_y')} value={point.image_y} onChange={(value) => updatePoint(index, { image_y: value })} step={1} />
                <NumberInput label={t('dialog.camera_calibration.ref_x')} value={point.reference_x} onChange={(value) => updatePoint(index, { reference_x: value })} step={0.1} />
                <NumberInput label={t('dialog.camera_calibration.ref_y')} value={point.reference_y} onChange={(value) => updatePoint(index, { reference_y: value })} step={0.1} />
              </div>
            </div>
          ))}
        </div>

        <div className="flex gap-2 mt-3">
          <button className="px-2 py-1 rounded bg-bb-hover text-bb-text hover:bg-bb-border" onClick={addPoint}>
            {t('dialog.camera_calibration.add_point')}
          </button>
          <button className="px-2 py-1 rounded bg-bb-accent text-bb-on-accent hover:bg-bb-accent-hover disabled:opacity-60" disabled={!canSolve} onClick={() => void handleSolve()}>
            {t('dialog.camera_calibration.solve')}
          </button>
          <button className="px-2 py-1 rounded bg-bb-hover text-bb-text hover:bg-bb-border disabled:opacity-60" disabled={!solvedCalibration} onClick={() => void handleSave()}>
            {t('dialog.camera_calibration.save_calibration')}
          </button>
          <button className="px-2 py-1 rounded bg-bb-hover text-bb-text hover:bg-bb-border disabled:opacity-60" disabled={!calibration} onClick={() => void handleReset()}>
            {t('dialog.camera_calibration.reset_saved')}
          </button>
        </div>

        {solvedCalibration && (
          <div className="mt-4 text-xs bg-bb-bg border border-bb-border rounded p-3 text-bb-text-muted">
            <div>{t('dialog.camera_calibration.quality', { value: (solvedCalibration.quality_score * 100).toFixed(1) })}</div>
            <div>{t('dialog.camera_calibration.rmse', { value: solvedCalibration.rmse_px.toFixed(3) })}</div>
            <div>{t('dialog.camera_calibration.scale', { value: solvedCalibration.transform.scale.toFixed(5) })}</div>
            <div>{t('dialog.camera_calibration.rotation', { value: solvedCalibration.transform.rotation_deg.toFixed(3) })}</div>
            <div>
              {t('dialog.camera_calibration.translation', { x: solvedCalibration.transform.translation_x.toFixed(3), y: solvedCalibration.transform.translation_y.toFixed(3) })}
            </div>
          </div>
        )}
      </div>
    </div>,
    document.body,
  );
}
