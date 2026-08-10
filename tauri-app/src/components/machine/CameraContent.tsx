import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { useCameraStore } from '../../stores/cameraStore';
import { useMachineStore } from '../../stores/machineStore';
import { useProjectStore } from '../../stores/projectStore';
import { CameraAlignmentDialog } from '../dialogs/CameraAlignmentDialog';
import {
  CameraCaptureActions,
  CameraOverlayControls,
  CameraOverlaySetupControls,
  CameraOverlayStatus,
  CameraStillPreview,
} from './CameraOverlayControls';
import { usePanelHost } from '../../panels';

/**
 * Camera panel content — extracted from CameraWindow for use inside
 * the panel system. No portal, no modal overlay, no close button
 * (panel chrome handles those).
 */
export function CameraContent() {
  const { t } = useTranslation();
  const { orientation } = usePanelHost();
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
  const canAlignCamera = cameraControlsEnabled && Boolean(overlayState?.frame) && !loading;
  const cameraStatusText = selectedDevice?.status_text ?? overlayState?.frame?.captured_at
    ?? (devices.length > 0
      ? t('panels.machine.camera.status.cameras_available', { count: devices.length })
      : t('panels.machine.camera.status.no_cameras_found'));
  const emptyDeviceLabel = devices.length > 0
    ? t('panels.machine.camera.choose_camera')
    : t('panels.machine.camera.no_camera');

  return (
    <div className={orientation === 'wide'
      ? 'bb-bottom-camera text-xs'
      : 'space-y-3 overflow-y-auto p-3 text-xs'}>
      <section className="space-y-2" data-camera-region="status">
      {/* Header with status + actions */}
      <div className="flex items-center justify-between gap-2">
        <div className="text-bb-text-muted">
          {cameraStatusText}
        </div>
        <CameraCaptureActions
          controlsEnabled={cameraControlsEnabled}
          onRescan={() => void refreshDevices()}
          onCapture={() => void captureFrame(project?.workspace ?? null)}
        />
      </div>

      {/* Device selector */}
      <label className="flex items-center justify-between gap-2">
        <span className="text-bb-text-muted shrink-0">{t('panels.machine.camera.device')}</span>
        <select
          value={selectedCameraId ?? ''}
          onChange={(e) => void selectCamera(e.target.value || null)}
          disabled={!hasActiveProfile || loading}
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
      </section>

      <section data-camera-region="preview">
      <CameraStillPreview frame={overlayState?.frame} />
      </section>
      <section className="space-y-2" data-camera-region="controls">
      <CameraOverlayControls />
      <CameraOverlaySetupControls controlsEnabled={cameraControlsEnabled && !loading} />
      </section>

      {/* Action buttons */}
      <section className="space-y-2" data-camera-region="actions">
      <div className="flex gap-1 flex-wrap">
        {calibration && !alignment && (
          <button
            className="px-2 py-1 rounded bg-bb-bg border border-bb-border text-bb-text hover:bg-bb-hover disabled:opacity-60"
            disabled={!cameraControlsEnabled || loading}
            onClick={() => void resetCalibration()}
          >
            {t('panels.machine.camera.reset_calibration')}
          </button>
        )}
        <button
          className="px-2 py-1 rounded bg-bb-bg border border-bb-border text-bb-text hover:bg-bb-hover disabled:opacity-60"
          disabled={!cameraControlsEnabled || !alignment || loading}
          onClick={() => void resetAlignment()}
        >
          {t('panels.machine.camera.reset_alignment')}
        </button>
      </div>

      <details className="text-bb-text-muted">
        <summary className="cursor-pointer select-none hover:text-bb-text">
          {t('panels.machine.camera.advanced_actions')}
        </summary>
        <button
          type="button"
          className="mt-1 px-2 py-1 rounded bg-bb-bg border border-bb-border text-bb-text hover:bg-bb-hover disabled:opacity-60"
          disabled={!cameraControlsEnabled || loading}
          onClick={() => void refreshOverlayState()}
        >
          {t('panels.machine.camera.reload_camera_state')}
        </button>
      </details>

      <div className="flex gap-1">
        <button
          className="px-2 py-1 rounded bg-bb-accent text-bb-on-accent hover:bg-bb-accent-hover disabled:opacity-60"
          disabled={!canAlignCamera}
          onClick={() => setShowAlignmentDialog(true)}
        >
          {t('panels.machine.camera.align_camera')}
        </button>
      </div>
      {cameraControlsEnabled && !overlayState?.frame && (
        <div className="text-bb-text-muted">
          {t('panels.machine.camera.capture_before_alignment')}
        </div>
      )}
      </section>

      {showAlignmentDialog && selectedCameraId && (
        <CameraAlignmentDialog onClose={() => setShowAlignmentDialog(false)} />
      )}
    </div>
  );
}
