import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { useCameraStore } from '../../stores/cameraStore';
import { useMachineStore } from '../../stores/machineStore';
import { useProjectStore } from '../../stores/projectStore';
import { CameraCalibrationDialog } from '../dialogs/CameraCalibrationDialog';
import { CameraAlignmentDialog } from '../dialogs/CameraAlignmentDialog';
import {
  CameraOverlayControls,
  CameraOverlaySetupControls,
  CameraOverlayStatus,
  CameraStillPreview,
} from './CameraOverlayControls';

/**
 * Camera panel content — extracted from CameraWindow for use inside
 * the panel system. No portal, no modal overlay, no close button
 * (panel chrome handles those).
 */
export function CameraContent() {
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

  return (
    <div className="p-3 text-xs space-y-3 overflow-y-auto">
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

      {showCalibrationDialog && selectedCameraId && (
        <CameraCalibrationDialog
          cameraId={selectedCameraId}
          onClose={() => setShowCalibrationDialog(false)}
        />
      )}

      {showAlignmentDialog && selectedCameraId && (
        <CameraAlignmentDialog onClose={() => setShowAlignmentDialog(false)} />
      )}
    </div>
  );
}
