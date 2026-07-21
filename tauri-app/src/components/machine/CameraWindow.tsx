import { useState, useEffect } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { X } from 'lucide-react';
import { useCameraStore } from '../../stores/cameraStore';
import { useMachineStore } from '../../stores/machineStore';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { CameraCalibrationDialog } from '../dialogs/CameraCalibrationDialog';
import { CameraAlignmentDialog } from '../dialogs/CameraAlignmentDialog';
import {
  CameraOverlayControls,
  CameraOverlaySetupControls,
  CameraOverlayStatus,
  CameraStillPreview,
} from './CameraOverlayControls';

export function CameraWindow() {
  const { t } = useTranslation();
  const activeProfileId = useMachineStore((s) => s.activeProfileId);
  const project = useProjectStore((s) => s.project);
  const devices = useCameraStore((s) => s.devices) ?? [];
  const selectedCameraId = useCameraStore((s) => s.selectedCameraId);
  const overlayState = useCameraStore((s) => s.overlayState);
  const calibration = useCameraStore((s) => s.calibration);
  const alignment = useCameraStore((s) => s.alignment);
  const loading = useCameraStore((s) => s.loading);
  const refreshDevices = useCameraStore((s) => s.refreshDevices);
  const selectCamera = useCameraStore((s) => s.selectCamera);
  const refreshOverlayState = useCameraStore((s) => s.refreshOverlayState);
  const captureFrame = useCameraStore((s) => s.captureFrame);
  const refreshCalibration = useCameraStore((s) => s.refreshCalibration);
  const refreshAlignment = useCameraStore((s) => s.refreshAlignment);
  const resetCalibration = useCameraStore((s) => s.resetCalibration);
  const resetAlignment = useCameraStore((s) => s.resetAlignment);
  const toggleCameraWindow = useUiStore((s) => s.toggleCameraWindow);

  const [showCalibrationDialog, setShowCalibrationDialog] = useState(false);
  const [showAlignmentDialog, setShowAlignmentDialog] = useState(false);

  // Refresh devices, overlay state, calibration and alignment on mount
  useEffect(() => {
    void (async () => {
      await refreshDevices();
      await refreshOverlayState();
      await refreshCalibration();
      await refreshAlignment();
    })();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const selectedDevice = devices.find((device) => device.camera_id === selectedCameraId);
  const hasActiveProfile = activeProfileId !== null;
  const cameraControlsEnabled = hasActiveProfile && !!selectedCameraId;
  const cameraStatusText = selectedDevice?.status_text ?? overlayState?.frame?.captured_at
    ?? (devices.length > 0
      ? t('panels.machine.camera.status.cameras_available', { count: devices.length })
      : t('panels.machine.camera.status.no_cameras_found'));
  const emptyDeviceLabel = devices.length > 0
    ? t('panels.machine.camera.choose_camera')
    : t('panels.machine.camera.no_camera');

  const handleOverlayClick = (e: React.MouseEvent<HTMLDivElement>) => {
    if (e.target === e.currentTarget) {
      toggleCameraWindow();
    }
  };

  return createPortal(
    <div
      className="fixed inset-0 bg-black/50 flex items-center justify-center z-50"
      onClick={handleOverlayClick}
    >
      <div className="bg-bb-panel border border-bb-border rounded-lg shadow-xl p-4 min-w-[420px] max-h-[70vh] flex flex-col">
        <div className="flex items-center justify-between mb-3">
          <h2 className="text-sm font-semibold text-bb-text">{t('panels.machine.camera.title')}</h2>
          <button
            onClick={toggleCameraWindow}
            aria-label={t('common.close')}
            className="text-bb-text-muted hover:text-bb-text text-sm px-1"
          >
            <X size={14} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto text-xs space-y-3">
          {/* Header with status + actions */}
          <div className="flex items-center justify-between gap-2">
            <div className="text-bb-text-muted">
              {cameraStatusText}
            </div>
            <div className="flex gap-1">
              <button
                className="px-2 py-1 rounded bg-bb-bg border border-bb-border text-bb-text hover:bg-bb-hover"
                onClick={() => void refreshDevices()}
              >
                {t('panels.machine.camera.refresh')}
              </button>
              <button
                className="px-2 py-1 rounded bg-bb-accent text-bb-on-accent hover:bg-bb-accent-hover disabled:opacity-60"
                disabled={!cameraControlsEnabled || loading}
                onClick={() => void captureFrame(project?.workspace ?? null)}
              >
                {t('panels.machine.camera.update_overlay')}
              </button>
            </div>
          </div>

          {/* Device selector */}
          <label className="flex items-center justify-between gap-2">
            <span className="text-bb-text-muted shrink-0">{t('panels.machine.camera.device')}</span>
            <select
              value={selectedCameraId ?? ''}
              onChange={(e) => void selectCamera(e.target.value || null)}
              disabled={!hasActiveProfile}
              className="w-40 px-1 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text focus:outline-none focus:border-bb-accent"
            >
              <option value="">{emptyDeviceLabel}</option>
              {devices.map((device) => (
                <option key={device.camera_id} value={device.camera_id}>
                  {device.display_name}
                </option>
              ))}
            </select>
          </label>

          {!hasActiveProfile && (
            <div className="text-bb-text-muted">
              {t('panels.machine.camera.select_profile_for_camera')}
            </div>
          )}

          {/* Status grid */}
          <CameraOverlayStatus />

          <CameraStillPreview frame={overlayState?.frame} />
          <CameraOverlayControls />
          <CameraOverlaySetupControls controlsEnabled={cameraControlsEnabled} />

          {/* Action buttons */}
          <div className="flex gap-1 flex-wrap">
            <button
              className="px-2 py-1 rounded bg-bb-bg border border-bb-border text-bb-text hover:bg-bb-hover disabled:opacity-60"
              disabled={!cameraControlsEnabled}
              onClick={() => void refreshOverlayState()}
            >
              {t('panels.machine.camera.refresh_overlay')}
            </button>
            <button
              className="px-2 py-1 rounded bg-bb-bg border border-bb-border text-bb-text hover:bg-bb-hover disabled:opacity-60"
              disabled={!cameraControlsEnabled || !calibration}
              onClick={() => void resetCalibration()}
            >
              {t('panels.machine.camera.reset_calibration')}
            </button>
            <button
              className="px-2 py-1 rounded bg-bb-bg border border-bb-border text-bb-text hover:bg-bb-hover disabled:opacity-60"
              disabled={!cameraControlsEnabled || !alignment}
              onClick={() => void resetAlignment()}
            >
              {t('panels.machine.camera.reset_alignment')}
            </button>
          </div>

          <div className="flex gap-1">
            <button
              className="px-2 py-1 rounded bg-bb-bg border border-bb-border text-bb-text hover:bg-bb-hover disabled:opacity-60"
              disabled={!cameraControlsEnabled}
              onClick={() => setShowCalibrationDialog(true)}
            >
              {t('panels.machine.camera.calibrate_lens')}
            </button>
            <button
              className="px-2 py-1 rounded bg-bb-bg border border-bb-border text-bb-text hover:bg-bb-hover disabled:opacity-60"
              disabled={!cameraControlsEnabled}
              onClick={() => setShowAlignmentDialog(true)}
            >
              {t('panels.machine.camera.align_camera')}
            </button>
          </div>
        </div>

        {/* Close button */}
        <div className="flex justify-end mt-4">
          <button
            onClick={toggleCameraWindow}
            className="px-3 py-1 text-xs font-medium rounded bg-bb-bg hover:bg-bb-hover text-bb-text transition-colors"
          >
            {t('common.close')}
          </button>
        </div>
      </div>

      {showCalibrationDialog && selectedCameraId && (
        <CameraCalibrationDialog
          cameraId={selectedCameraId}
          onClose={() => setShowCalibrationDialog(false)}
        />
      )}

      {showAlignmentDialog && selectedCameraId && (
        <CameraAlignmentDialog onClose={() => setShowAlignmentDialog(false)} />
      )}
    </div>,
    document.body
  );
}
