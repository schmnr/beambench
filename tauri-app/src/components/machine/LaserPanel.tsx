import { useState, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { useMachineStore } from '../../stores/machineStore';
import { useUiStore } from '../../stores/uiStore';
import { usePreviewStore } from '../../stores/previewStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { wrapBackendError } from '../../i18n/errors';
import { useAppStore } from '../../stores/appStore';
import { mmToDisplay, displayToMm, roundDisplayLength, lengthStep, lengthUnitLabel, labelWithUnit } from '../../utils/lengthUnits';
import { NumberStepper } from '../shared/NumberStepper';
import { MovableResizableDialogFrame } from '../shared/MovableResizableDialogFrame';
import { JobProgressBar } from './JobProgressBar';
import { OverrideControls } from './OverrideControls';
import { PreflightDialog } from './PreflightDialog';
import { DeviceSettingsDialog } from '../dialogs/DeviceSettingsDialog';
import type {
  StartFromMode,
  AnchorPoint,
  FinishPosition,
  DirectionOrder,
  OptimizationOrderKey,
  ProjectOptimization,
} from '../../types/project';
import { DEFAULT_PROJECT_OPTIMIZATION } from '../../types/project';

const PAUSE_ICON = '\u23f8';
const STOP_ICON = '\u25a0';
const PLAY_ICON = '\u25b6';

const START_FROM_OPTIONS: { value: StartFromMode; labelKey: string }[] = [
  { value: 'absolute_coords', labelKey: 'panels.machine.laser.start_from.absolute_coords' },
  { value: 'current_position', labelKey: 'panels.machine.laser.start_from.current_position' },
  { value: 'user_origin', labelKey: 'panels.machine.laser.start_from.user_origin' },
];

const ANCHOR_GRID: { value: AnchorPoint; label: string }[] = [
  { value: 'top_left', label: 'TL' },
  { value: 'top_center', label: 'TC' },
  { value: 'top_right', label: 'TR' },
  { value: 'center_left', label: 'CL' },
  { value: 'center', label: 'C' },
  { value: 'center_right', label: 'CR' },
  { value: 'bottom_left', label: 'BL' },
  { value: 'bottom_center', label: 'BC' },
  { value: 'bottom_right', label: 'BR' },
];

const START_FROM_STATUS_KEYS: Record<StartFromMode, string> = {
  absolute_coords: 'panels.machine.laser.start_from_status.absolute_coords',
  current_position: 'panels.machine.laser.start_from_status.current_position',
  user_origin: 'panels.machine.laser.start_from_status.user_origin',
};

const CONNECTION_LABEL_KEYS: Record<string, string> = {
  disconnected: 'panels.machine.laser.connection.disconnected',
  connecting: 'panels.machine.laser.connection.connecting',
  transport_open: 'panels.machine.laser.connection.transport_open',
  waiting_for_banner: 'panels.machine.laser.connection.waiting_for_banner',
  validating: 'panels.machine.laser.connection.validating',
  ready: 'panels.machine.laser.connection.connected',
  running: 'panels.machine.laser.connection.running',
  paused: 'panels.machine.laser.connection.paused',
  alarm: 'panels.machine.laser.connection.alarm',
  error: 'panels.machine.laser.connection.error',
};

const ANCHOR_TITLE_KEYS: Record<AnchorPoint, string> = {
  top_left: 'panels.machine.laser.anchor.top_left',
  top_center: 'panels.machine.laser.anchor.top_center',
  top_right: 'panels.machine.laser.anchor.top_right',
  center_left: 'panels.machine.laser.anchor.center_left',
  center: 'panels.machine.laser.anchor.center',
  center_right: 'panels.machine.laser.anchor.center_right',
  bottom_left: 'panels.machine.laser.anchor.bottom_left',
  bottom_center: 'panels.machine.laser.anchor.bottom_center',
  bottom_right: 'panels.machine.laser.anchor.bottom_right',
};

const ORDER_KEY_LAYER: OptimizationOrderKey = 'layer';
const ORDER_KEY_GROUP: OptimizationOrderKey = 'group';
const ORDER_KEY_PRIORITY: OptimizationOrderKey = 'priority';
const ORDER_KEYS: OptimizationOrderKey[] = [ORDER_KEY_LAYER, ORDER_KEY_GROUP, ORDER_KEY_PRIORITY];

function cloneOptimization(optimization: ProjectOptimization): ProjectOptimization {
  return JSON.parse(JSON.stringify(optimization)) as ProjectOptimization;
}

function isExportCancelledError(error: unknown): boolean {
  return error instanceof Error && error.message === 'Export cancelled';
}

/** Thin label+checkbox row used throughout the optimization popover. */
function CheckboxRow(props: {
  testId: string;
  label: string;
  checked: boolean;
  onChange: (next: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <label
      className={`flex items-center gap-1.5 text-xs ${
        props.disabled ? 'text-bb-text-muted opacity-60 cursor-not-allowed' : 'text-bb-text cursor-pointer'
      }`}
    >
      <input
        type="checkbox"
        data-testid={props.testId}
        checked={props.checked}
        onChange={(e) => props.onChange(e.target.checked)}
        disabled={props.disabled}
        className="accent-bb-accent"
      />
      {props.label}
    </label>
  );
}

export function LaserPanel() {
  const { t } = useTranslation();
  const displayUnit = useAppStore((s) => s.settings?.display_unit) ?? 'mm';
  const startInFlightRef = useRef(false);
  const project = useProjectStore((s) => s.project);
  const setStartFrom = useProjectStore((s) => s.setStartFrom);
  const setJobOrigin = useProjectStore((s) => s.setJobOrigin);
  const sessionState = useMachineStore((s) => s.sessionState);
  const profiles = useMachineStore((s) => s.profiles) ?? [];
  const activeProfileId = useMachineStore((s) => s.activeProfileId);
  const jobProgress = useMachineStore((s) => s.jobProgress);
  const machineStatus = useMachineStore((s) => s.machineStatus);
  const preflightReport = useMachineStore((s) => s.preflightReport);
  const emergencyStop = useMachineStore((s) => s.emergencyStop);
  const setActiveProfile = useMachineStore((s) => s.setActiveProfile);
  const frameJob = useMachineStore((s) => s.frameJob);
  const loading = useMachineStore((s) => s.loading);
  const runPreflight = useMachineStore((s) => s.runPreflight);
  const startJob = useMachineStore((s) => s.startJob);
  const pauseJob = useMachineStore((s) => s.pauseJob);
  const resumeJob = useMachineStore((s) => s.resumeJob);

  const jobOptions = useUiStore((s) => s.jobOptions);
  const updateJobOptions = useUiStore((s) => s.updateJobOptions);

  // Post-M1 optimization lives on the project, not the machine store.
  // `DEFAULT_PROJECT_OPTIMIZATION` covers pre-Phase-1 project fixtures
  // that may not have an `optimization` block populated yet.
  const projectOptimization: ProjectOptimization = useProjectStore(
    (s) => s.project?.optimization ?? DEFAULT_PROJECT_OPTIMIZATION,
  );
  const setOptimization = useProjectStore((s) => s.setOptimization);
  const exportGcode = useProjectStore((s) => s.exportGcode);

  const showLastPosition = useUiStore((s) => s.showLastPosition);
  const toggleShowLastPosition = useUiStore((s) => s.toggleShowLastPosition);

  const previewState = usePreviewStore((s) => s.state);
  const generatePreview = usePreviewStore((s) => s.generatePreview);

  const showPreflightDialog = useMachineStore((s) => s.showPreflightDialog);
  const openPreflightDialog = useMachineStore((s) => s.openPreflightDialog);
  const closePreflightDialog = useMachineStore((s) => s.closePreflightDialog);

  const [showDevicesDialog, setShowDevicesDialog] = useState(false);
  const [frameConfirm, setFrameConfirm] = useState(false);
  const [frameLaserOnConfirm, setFrameLaserOnConfirm] = useState(false);
  const [frameShiftArmed, setFrameShiftArmed] = useState(false);
  const [showOptDialog, setShowOptDialog] = useState(false);
  const [optimizationDraft, setOptimizationDraft] = useState<ProjectOptimization>(projectOptimization);
  const optimizationSnapshotRef = useRef<ProjectOptimization>(cloneOptimization(projectOptimization));
  const [frameMode, setFrameMode] = useState<'rectangular' | 'rubber_band'>('rectangular');
  const [startInFlight, setStartInFlight] = useState(false);

  // Reset frameConfirm after 3 seconds
  useEffect(() => {
    if (frameConfirm) {
      const timeout = setTimeout(() => {
        setFrameConfirm(false);
        setFrameLaserOnConfirm(false);
      }, 3000);
      return () => clearTimeout(timeout);
    }
  }, [frameConfirm]);

  useEffect(() => {
    const isEditableTarget = (target: EventTarget | null) => {
      return target instanceof HTMLElement && (
        target.tagName === 'INPUT' ||
        target.tagName === 'TEXTAREA' ||
        target.isContentEditable
      );
    };
    const handleKey = (event: KeyboardEvent) => {
      if (!isEditableTarget(event.target)) {
        setFrameShiftArmed(event.shiftKey);
      }
    };
    const clear = () => setFrameShiftArmed(false);
    window.addEventListener('keydown', handleKey);
    window.addEventListener('keyup', handleKey);
    window.addEventListener('blur', clear);
    return () => {
      window.removeEventListener('keydown', handleKey);
      window.removeEventListener('keyup', handleKey);
      window.removeEventListener('blur', clear);
    };
  }, []);

  const startFrom = project?.start_from ?? 'absolute_coords';
  const jobOrigin = project?.job_origin ?? 'top_left';
  const isAbsoluteStart = startFrom === 'absolute_coords';
  const isAlarmState = sessionState === 'alarm' || machineStatus?.run_state === 'alarm';
  const effectiveSessionState = isAlarmState ? 'alarm' : sessionState;
  const hiddenSelectionJobOptionsActive = jobOptions.cut_selected_graphics || jobOptions.use_selection_origin;

  const connectionGradient =
    effectiveSessionState === 'ready' || effectiveSessionState === 'running' || effectiveSessionState === 'paused' ? 'from-green-500 to-green-700'
    : effectiveSessionState === 'connecting' || effectiveSessionState === 'transport_open' || effectiveSessionState === 'waiting_for_banner' || effectiveSessionState === 'validating' ? 'from-yellow-500 to-yellow-700'
    : effectiveSessionState === 'alarm' || effectiveSessionState === 'error' ? 'from-red-500 to-red-700'
    : 'from-gray-500 to-gray-700';

  const activeProfile = activeProfileId ? profiles.find((p) => p.id === activeProfileId) : null;
  const profileLaserOnFraming = activeProfile?.laser_on_when_framing ?? false;
  const laserOnFramingArmed = frameConfirm
    ? frameLaserOnConfirm
    : profileLaserOnFraming || frameShiftArmed;

  const isConnected = sessionState === 'ready' || sessionState === 'running' || sessionState === 'paused' || sessionState === 'alarm';
  const isIdleState =
    machineStatus?.run_state === 'idle' ||
    (sessionState === 'ready' && (!machineStatus || machineStatus.run_state === 'unknown'));
  const canUseMotionControls = isIdleState && !isAlarmState;
  const hasJob = jobProgress !== null;
  const isJobActive =
    jobProgress?.state === 'preparing' || jobProgress?.state === 'running' || jobProgress?.state === 'paused';
  const canStart =
    sessionState === 'ready' &&
    machineStatus?.run_state === 'idle' &&
    !loading &&
    previewState !== 'generating' &&
    !startInFlight;
  const canPause = jobProgress?.state === 'running';
  const canResume = jobProgress?.state === 'paused';
  const canStop =
    jobProgress?.state === 'preparing' || jobProgress?.state === 'running' || jobProgress?.state === 'paused';

  useEffect(() => {
    if (hiddenSelectionJobOptionsActive) {
      updateJobOptions({ cut_selected_graphics: false, use_selection_origin: false });
    }
  }, [hiddenSelectionJobOptionsActive, updateJobOptions]);

  const handleFrame = async (laserOnOverride: boolean) => {
    try {
      await frameJob(frameMode, undefined, laserOnOverride);
      setFrameConfirm(false);
      setFrameLaserOnConfirm(false);
    } catch (e) {
      useNotificationStore.getState().push(wrapBackendError(String(e)), 'error');
      setFrameConfirm(false);
      setFrameLaserOnConfirm(false);
    }
  };

  const handleFrameClick = (shiftKey: boolean) => {
    const armed = profileLaserOnFraming || shiftKey || frameShiftArmed;
    if (frameConfirm) {
      void handleFrame(frameLaserOnConfirm || armed);
    } else {
      setFrameLaserOnConfirm(armed);
      setFrameConfirm(true);
    }
  };

  const handleSaveGcode = async () => {
    try {
      await exportGcode();
      useNotificationStore.getState().push(t('panels.machine.laser.notifications.gcode_exported'), 'info');
    } catch (e) {
      if (isExportCancelledError(e)) return;
      useNotificationStore.getState().push(wrapBackendError(String(e)), 'error');
    }
  };

  const handleStart = async () => {
    if (startInFlightRef.current) return;

    startInFlightRef.current = true;
    setStartInFlight(true);

    try {
      let previewReady = previewState === 'current';
      if (!previewReady) {
        previewReady = await generatePreview();
      }
      if (!previewReady) {
        return;
      }

      const report = await runPreflight();
      if (!report) return;
      if (report.outcome === 'pass') {
        await startJob();
      } else {
        openPreflightDialog();
      }
    } finally {
      startInFlightRef.current = false;
      setStartInFlight(false);
    }
  };

  const updateOptimizationDraft = (patch: Partial<ProjectOptimization>) => {
    setOptimizationDraft((current) => ({ ...current, ...patch }));
  };

  const setDraftOrderKey = (key: OptimizationOrderKey, enabled: boolean) => {
    setOptimizationDraft((current) => {
      const nextKeys = enabled
        ? ORDER_KEYS.filter((candidate) => candidate === key || current.ordering.includes(candidate))
        : current.ordering.filter((candidate) => candidate !== key);
      return { ...current, ordering: nextKeys };
    });
  };

  const openOptimizationDialog = () => {
    const snapshot = cloneOptimization(projectOptimization);
    optimizationSnapshotRef.current = snapshot;
    setOptimizationDraft(cloneOptimization(snapshot));
    setShowOptDialog(true);
  };

  const handleOptimizationOk = async () => {
    await setOptimization(optimizationDraft);
    setShowOptDialog(false);
  };

  const handleOptimizationCancel = () => {
    setOptimizationDraft(cloneOptimization(optimizationSnapshotRef.current));
    setShowOptDialog(false);
  };

  return (
    <div className="flex flex-col gap-1.5 px-2 py-2 text-xs">
      {/* 1. Connection gradient bar */}
      <div className={`h-1.5 rounded-full bg-gradient-to-r ${connectionGradient}`} data-testid="connection-bar" />

      {/* 2. Status label */}
      <div className="text-center text-bb-text-muted text-xs">
        {CONNECTION_LABEL_KEYS[effectiveSessionState] ? t(CONNECTION_LABEL_KEYS[effectiveSessionState]) : effectiveSessionState}
      </div>

      {/* 3. Large Pause / Stop / Start buttons (when connected) */}
      {isConnected && (
        <div className="grid grid-cols-3 gap-1" data-testid="job-buttons">
          <button
            data-testid="pause-button"
            className="py-2.5 text-sm font-medium rounded bg-bb-warning text-bb-on-warning disabled:opacity-60 disabled:cursor-not-allowed hover:bg-bb-warning-hover"
            disabled={!canPause}
            onClick={pauseJob}
          >
            {PAUSE_ICON} {t('panels.machine.laser.pause')}
          </button>
          <button
            data-testid="stop-button"
            className="py-2.5 text-sm font-medium rounded bg-bb-error text-bb-on-error disabled:opacity-60 disabled:cursor-not-allowed hover:bg-bb-error-hover"
            disabled={!canStop}
            onClick={emergencyStop}
          >
            {STOP_ICON} {t('panels.machine.laser.stop')}
          </button>
          {canResume ? (
            <button
              data-testid="resume-button"
              className="py-2.5 text-sm font-medium rounded bg-bb-accent text-bb-on-accent disabled:opacity-60 disabled:cursor-not-allowed hover:bg-bb-accent-hover"
              onClick={resumeJob}
            >
              {PLAY_ICON} {t('panels.machine.laser.resume')}
            </button>
          ) : (
            <button
              data-testid="start-button"
              className="py-2.5 text-sm font-medium rounded bg-bb-success text-bb-on-success disabled:opacity-60 disabled:cursor-not-allowed hover:bg-bb-success-hover"
              disabled={!canStart}
              onClick={() => void handleStart()}
            >
              {PLAY_ICON} {t('panels.machine.laser.start')}
            </button>
          )}
        </div>
      )}

      {/* 4. Frame (when connected + idle, no active job) */}
      {isConnected && !hasJob && (
        <div className="flex flex-col gap-1">
          <button
            disabled={!canUseMotionControls}
            className={`px-2 py-1 rounded border disabled:opacity-50 disabled:cursor-not-allowed ${
              laserOnFramingArmed
                ? 'bg-bb-error-bg border-bb-error-border text-bb-error-fg hover:brightness-95'
                : 'bg-bb-bg border-bb-border text-bb-text hover:bg-bb-hover'
            }`}
            onMouseMove={(e) => setFrameShiftArmed(e.shiftKey)}
            onMouseLeave={() => setFrameShiftArmed(false)}
            onClick={(e) => handleFrameClick(e.shiftKey)}
          >
            {frameConfirm
              ? frameLaserOnConfirm ? t('panels.machine.laser.confirm_laser_frame') : t('panels.machine.laser.confirm_frame')
              : laserOnFramingArmed ? t('panels.machine.laser.frame_laser_on') : t('panels.machine.laser.frame')}
          </button>
          <button
            data-testid="frame-mode-toggle"
            disabled={!canUseMotionControls}
            className={`px-2 py-1 rounded border text-xs disabled:opacity-50 disabled:cursor-not-allowed ${
              frameMode === 'rubber_band'
                ? 'bg-bb-accent border-bb-accent text-bb-on-accent'
                : 'bg-bb-bg border-bb-border text-bb-text hover:bg-bb-hover'
            }`}
            onClick={() => setFrameMode(frameMode === 'rectangular' ? 'rubber_band' : 'rectangular')}
            title={frameMode === 'rectangular' ? t('panels.machine.laser.rectangular_frame') : t('panels.machine.laser.rubber_band_frame')}
          >
            {frameMode === 'rectangular' ? t('panels.machine.laser.frame_mode_rect') : t('panels.machine.laser.frame_mode_hull')}
          </button>
        </div>
      )}

      {frameConfirm && (
        <div className={`rounded px-2 py-1 ${frameLaserOnConfirm ? 'bg-bb-error-bg text-bb-error-fg' : 'bg-bb-warning-bg text-bb-warning-fg'}`}>
          {frameLaserOnConfirm
            ? t('panels.machine.laser.laser_on_frame_warning')
            : t('panels.machine.laser.frame_warning')}
        </div>
      )}

      {/* 6. Show Last Position + Optimization Settings row */}
      <div className="flex gap-1">
        <button
          data-testid="show-last-position-button"
          className={`flex-1 px-2 py-1 rounded border text-xs ${
            showLastPosition
              ? 'bg-bb-accent/20 border-bb-accent text-bb-text'
              : 'bg-bb-bg border-bb-border text-bb-text-muted hover:bg-bb-hover'
          }`}
          onClick={toggleShowLastPosition}
        >
          {t('panels.machine.laser.show_last_position')}
        </button>
        <button
          data-testid="optimization-settings-button"
          className="flex-1 px-2 py-1 rounded bg-bb-bg border border-bb-border text-bb-text-muted hover:bg-bb-hover text-xs disabled:cursor-not-allowed disabled:opacity-50"
          onClick={openOptimizationDialog}
          disabled={showOptDialog}
        >
          {t('panels.machine.laser.optimization')}
        </button>
      </div>

      {/* Optimization modal edits a local draft and persists only on OK. */}
      {showOptDialog && (
        <MovableResizableDialogFrame
          title={t('panels.machine.laser.optimization_settings')}
          titleId="optimization-dialog-title"
          testId="optimization-modal"
          initialWidth={560}
          initialHeight={720}
          minWidth={480}
          minHeight={520}
          onRequestClose={handleOptimizationCancel}
          closeOnBackdropClick
          footer={
            <div className="flex justify-end gap-2 px-3 py-3">
              <button
                className="rounded border border-bb-border bg-bb-bg px-3 py-1 text-xs text-bb-text hover:bg-bb-hover"
                onClick={handleOptimizationCancel}
              >
                {t('common.cancel')}
              </button>
              <button
                className="rounded bg-bb-accent px-3 py-1 text-xs font-medium text-bb-on-accent hover:bg-bb-accent-hover"
                onClick={() => { void handleOptimizationOk(); }}
              >
                {t('common.ok')}
              </button>
            </div>
          }
        >
          <div className="min-h-0 flex-1 overflow-y-auto p-3 space-y-3">
          <div className="flex items-center justify-end">
            <CheckboxRow
              testId="optimization-enabled"
              label={t('common.enable')}
              checked={optimizationDraft.enabled}
              onChange={(v) => updateOptimizationDraft({ enabled: v })}
            />
          </div>
          {/* Group: Order by */}
          <fieldset className="space-y-1">
            <legend className="text-bb-text-muted text-xs mb-1">{t('panels.machine.laser.optimization_order_by')}</legend>
            <CheckboxRow
              testId="order-by-layer"
              label={t('panels.machine.laser.optimization_layer')}
              checked={optimizationDraft.ordering.includes(ORDER_KEY_LAYER)}
              onChange={(v) => setDraftOrderKey(ORDER_KEY_LAYER, v)}
            />
            <CheckboxRow
              testId="order-by-group"
              label={t('panels.machine.laser.optimization_group')}
              checked={optimizationDraft.ordering.includes(ORDER_KEY_GROUP)}
              onChange={(v) => setDraftOrderKey(ORDER_KEY_GROUP, v)}
            />
            <CheckboxRow
              testId="order-by-priority"
              label={t('panels.machine.laser.optimization_priority')}
              checked={optimizationDraft.ordering.includes(ORDER_KEY_PRIORITY)}
              onChange={(v) => setDraftOrderKey(ORDER_KEY_PRIORITY, v)}
            />
          </fieldset>

          {/* Group: Within each */}
          <fieldset className="space-y-1">
            <legend className="text-bb-text-muted text-xs mb-1">{t('panels.machine.laser.optimization_within_each')}</legend>
            <CheckboxRow
              testId="inner-first"
              label={t('panels.machine.laser.cut_inner_shapes_first')}
              checked={optimizationDraft.inner_first}
              onChange={(v) => updateOptimizationDraft({ inner_first: v })}
            />
            <label className="flex items-center gap-1.5 text-xs text-bb-text">
              <span className="min-w-[5.5rem]">{t('panels.machine.laser.direction')}</span>
              <select
                data-testid="direction-order-select"
                value={optimizationDraft.direction_order}
                onChange={(e) =>
                  updateOptimizationDraft({ direction_order: e.target.value as DirectionOrder })
                }
                className="flex-1 bg-bb-surface-elevated text-bb-text border border-bb-border rounded px-2 py-0.5 text-xs"
              >
                <option value="none">{t('panels.machine.laser.direction_none')}</option>
                <option value="top_down">{t('panels.machine.laser.direction_top_down')}</option>
                <option value="bottom_up">{t('panels.machine.laser.direction_bottom_up')}</option>
                <option value="left_right">{t('panels.machine.laser.direction_left_right')}</option>
                <option value="right_left">{t('panels.machine.laser.direction_right_left')}</option>
              </select>
            </label>
            <CheckboxRow
              testId="choose-best-start"
              label={t('panels.machine.laser.choose_best_starting_point')}
              checked={optimizationDraft.choose_best_start}
              onChange={(v) => updateOptimizationDraft({ choose_best_start: v })}
            />
            <CheckboxRow
              testId="choose-corners"
              label={t('panels.machine.laser.choose_corners_if_possible')}
              checked={optimizationDraft.choose_corners}
              onChange={(v) => updateOptimizationDraft({ choose_corners: v })}
              disabled={!optimizationDraft.choose_best_start}
            />
            <CheckboxRow
              testId="choose-best-direction"
              label={t('panels.machine.laser.choose_best_direction')}
              checked={optimizationDraft.choose_best_direction}
              onChange={(v) => updateOptimizationDraft({ choose_best_direction: v })}
            />
          </fieldset>

          {/* Group: Travel */}
          <fieldset className="space-y-1">
            <legend className="text-bb-text-muted text-xs mb-1">{t('panels.machine.laser.travel')}</legend>
            <CheckboxRow
              testId="reduce-travel"
              label={t('panels.machine.laser.reduce_travel_moves')}
              checked={optimizationDraft.reduce_travel}
              onChange={(v) => updateOptimizationDraft({ reduce_travel: v })}
            />
            <CheckboxRow
              testId="hide-backlash"
              label={t('panels.machine.laser.hide_backlash')}
              checked={optimizationDraft.hide_backlash}
              onChange={(v) => updateOptimizationDraft({ hide_backlash: v })}
            />
            <CheckboxRow
              testId="reduce-direction-changes"
              label={t('panels.machine.laser.reduce_direction_changes')}
              checked={optimizationDraft.reduce_direction_changes}
              onChange={(v) => updateOptimizationDraft({ reduce_direction_changes: v })}
            />
          </fieldset>

          {/* Group: Cleanup */}
          <fieldset className="space-y-1">
            <legend className="text-bb-text-muted text-xs mb-1">{t('panels.machine.laser.cleanup')}</legend>
            <CheckboxRow
              testId="remove-overlapping"
              label={t('panels.machine.laser.remove_overlapping_lines')}
              checked={optimizationDraft.remove_overlapping}
              onChange={(v) => updateOptimizationDraft({ remove_overlapping: v })}
            />
            {optimizationDraft.remove_overlapping && (
              <label className="flex items-center gap-1.5 text-xs text-bb-text ml-5">
                <span>{labelWithUnit(t('panels.machine.laser.tolerance_mm'), lengthUnitLabel(displayUnit))}</span>
                <NumberStepper
                  data-testid="remove-overlap-tolerance"
                  value={roundDisplayLength(mmToDisplay(optimizationDraft.remove_overlap_tolerance_mm, displayUnit), displayUnit)}
                  onChange={(e) => {
                    // The previous `Number(value) || 0.05` coerced an
                    // explicit 0 back to 0.05 because 0 is falsy,
                    // making the control unable to express the
                    // exact-zero tolerance the backend supports
                    // (`dedupe::remove_near_duplicates` treats 0 as
                    // "flag on but no-op").
                    //
                    // Two distinct invalid-input cases:
                    //   1. Empty string — user has cleared the field
                    //      mid-edit. `Number('')` is 0 (!), so we
                    //      filter it by string first to avoid
                    //      committing a phantom 0.
                    //   2. NaN — non-numeric input; ignore.
                    // Everything else (including a negative value
                    // that the stepper's `min={0}` will visually
                    // clamp) flows through.
                    const raw = e.target.value;
                    if (raw === '') return;
                    const parsed = Number(raw);
                    if (!Number.isFinite(parsed)) return;
                    updateOptimizationDraft({ remove_overlap_tolerance_mm: displayToMm(parsed, displayUnit) });
                  }}
                  step={lengthStep(displayUnit, 0.01, 0.001)}
                  min={0}
                  className="w-20 px-1 py-0.5 bg-bb-surface-elevated text-bb-text border border-bb-border rounded text-xs"
                />
              </label>
            )}
          </fieldset>

          {/* Group: Output positioning (unchanged surface, rewired path) */}
          <fieldset className="space-y-1">
            <legend className="text-bb-text-muted text-xs mb-1">{t('panels.machine.laser.output')}</legend>
            <div>
              <label className="flex items-center gap-1.5 text-xs text-bb-text cursor-pointer">
                <input
                  type="checkbox"
                  data-testid="start-point-checkbox"
                  checked={
                    optimizationDraft.start_point_x != null &&
                    optimizationDraft.start_point_y != null
                  }
                  onChange={(e) => {
                    if (e.target.checked) {
                      updateOptimizationDraft({ start_point_x: 0, start_point_y: 0 });
                    } else {
                      updateOptimizationDraft({ start_point_x: null, start_point_y: null });
                    }
                  }}
                  className="accent-bb-accent"
                />
                {t('panels.machine.laser.custom_start_point')}
              </label>
              {optimizationDraft.start_point_x != null &&
                optimizationDraft.start_point_y != null && (
                  <div className="flex items-center gap-1 mt-1 ml-5">
                    <NumberStepper
                      data-testid="start-point-x"
                      value={roundDisplayLength(mmToDisplay(optimizationDraft.start_point_x, displayUnit), displayUnit)}
                      onChange={(e) =>
                        updateOptimizationDraft({ start_point_x: displayToMm(Number(e.target.value) || 0, displayUnit) })
                      }
                      step={lengthStep(displayUnit, 1, 0.05)}
                      className="flex-1 px-1 py-0.5 bg-bb-surface-elevated text-bb-text border border-bb-border rounded text-xs"
                      placeholder={labelWithUnit('X', lengthUnitLabel(displayUnit))}
                    />
                    <NumberStepper
                      data-testid="start-point-y"
                      value={roundDisplayLength(mmToDisplay(optimizationDraft.start_point_y, displayUnit), displayUnit)}
                      onChange={(e) =>
                        updateOptimizationDraft({ start_point_y: displayToMm(Number(e.target.value) || 0, displayUnit) })
                      }
                      step={lengthStep(displayUnit, 1, 0.05)}
                      className="flex-1 px-1 py-0.5 bg-bb-surface-elevated text-bb-text border border-bb-border rounded text-xs"
                      placeholder={labelWithUnit('Y', lengthUnitLabel(displayUnit))}
                    />
                    <span className="text-xs text-bb-text-muted shrink-0">{lengthUnitLabel(displayUnit)}</span>
                  </div>
                )}
            </div>
            <div>
              <label className="block text-bb-text-muted text-xs mb-1">{t('panels.machine.laser.finish_position')}</label>
              <select
                data-testid="finish-position-select"
                value={optimizationDraft.finish_position}
                onChange={(e) => {
                  const val = e.target.value as FinishPosition;
                  if (val === 'custom_xy') {
                    updateOptimizationDraft({ finish_position: val, finish_x: 0, finish_y: 0 });
                  } else {
                    updateOptimizationDraft({ finish_position: val, finish_x: null, finish_y: null });
                  }
                }}
                className="w-full bg-bb-surface-elevated text-bb-text border border-bb-border rounded px-2 py-1 text-xs"
              >
                <option value="origin">{t('panels.machine.laser.finish_return_to_origin')}</option>
                <option value="dont_move">{t('panels.machine.laser.finish_dont_move')}</option>
                <option value="custom_xy">{t('panels.machine.laser.finish_custom_xy')}</option>
              </select>
              {optimizationDraft.finish_position === 'custom_xy' && (
                <div className="flex items-center gap-1 mt-1">
                  <NumberStepper
                    data-testid="finish-x"
                    value={roundDisplayLength(mmToDisplay(optimizationDraft.finish_x ?? 0, displayUnit), displayUnit)}
                    onChange={(e) =>
                      updateOptimizationDraft({ finish_x: displayToMm(Number(e.target.value) || 0, displayUnit) })
                    }
                    step={lengthStep(displayUnit, 1, 0.05)}
                    className="flex-1 px-1 py-0.5 bg-bb-surface-elevated text-bb-text border border-bb-border rounded text-xs"
                    placeholder={labelWithUnit('X', lengthUnitLabel(displayUnit))}
                  />
                  <NumberStepper
                    data-testid="finish-y"
                    value={roundDisplayLength(mmToDisplay(optimizationDraft.finish_y ?? 0, displayUnit), displayUnit)}
                    onChange={(e) =>
                      updateOptimizationDraft({ finish_y: displayToMm(Number(e.target.value) || 0, displayUnit) })
                    }
                    step={lengthStep(displayUnit, 1, 0.05)}
                    className="flex-1 px-1 py-0.5 bg-bb-surface-elevated text-bb-text border border-bb-border rounded text-xs"
                    placeholder={labelWithUnit('Y', lengthUnitLabel(displayUnit))}
                  />
                  <span className="text-xs text-bb-text-muted shrink-0">{lengthUnitLabel(displayUnit)}</span>
                </div>
              )}
            </div>
          </fieldset>
          </div>
        </MovableResizableDialogFrame>
      )}

      {/* Job progress (when job active) */}
      {hasJob && <JobProgressBar />}

      {/* Override controls (when job running/paused) */}
      {isJobActive && <OverrideControls />}

      {/* Save GCode */}
      {isConnected && project && (
        <button
          data-testid="save-gcode-button"
          className="px-2 py-1 rounded bg-bb-bg border border-bb-border text-bb-text hover:bg-bb-hover"
          onClick={() => void handleSaveGcode()}
        >
          {t('panels.machine.laser.save_gcode')}
        </button>
      )}

      {/* 8. Divider */}
      <div className="border-t border-bb-border my-0.5" />

      {/* 9. Devices row at bottom */}
      <div className="flex items-center gap-2" data-testid="devices-row">
        <select
          data-testid="profile-select"
          aria-label={t('panels.machine.laser.profile_select')}
          value={activeProfileId ?? ''}
          disabled={profiles.length === 0 || isConnected || loading}
          onChange={(e) => { void setActiveProfile(e.target.value === '' ? null : e.target.value); }}
          className="min-w-0 flex-1 rounded border border-bb-border bg-bb-panel px-1.5 py-1 text-xs text-bb-text disabled:cursor-not-allowed disabled:opacity-60"
          title={isConnected ? t('panels.machine.laser.profile_select_connected_title') : t('panels.machine.laser.profile_select')}
        >
          <option value="">{t('panels.machine.laser.no_machine')}</option>
          {profiles.map((profile) => (
            <option key={profile.id} value={profile.id}>
              {profile.name}
            </option>
          ))}
        </select>
        <button
          data-testid="devices-button"
          className="shrink-0 px-2 py-1 rounded bg-bb-bg border border-bb-border text-bb-text hover:bg-bb-hover"
          onClick={() => setShowDevicesDialog(true)}
        >
          {t('panels.machine.laser.manage_machine_profiles')}
        </button>
      </div>

      {/* 10. Start From */}
      <div>
        <label className="block text-bb-text-muted mb-1">{t('panels.machine.laser.start_from_label')}</label>
        <select
          className="w-fit min-w-40 max-w-full rounded bg-bb-panel border border-bb-border text-bb-text px-1.5 py-1 focus:outline-none focus:border-bb-accent"
          value={startFrom}
          onChange={(e) => void setStartFrom(e.target.value as StartFromMode)}
        >
          {START_FROM_OPTIONS.map((opt) => (
            <option key={opt.value} value={opt.value}>{t(opt.labelKey)}</option>
          ))}
        </select>
      </div>

      <div
        data-testid="start-from-status"
        role="status"
        className="rounded border border-bb-border bg-bb-bg/50 px-2 py-1 text-bb-text-muted"
      >
        {t(START_FROM_STATUS_KEYS[startFrom])}
      </div>

      {/* 11. Job Origin grid */}
      <div>
        <label className="block text-bb-text-muted mb-1">{t('panels.machine.laser.job_origin')}</label>
        <div className="grid grid-cols-3 gap-1 w-fit">
          {ANCHOR_GRID.map((cell) => (
            <button
              key={cell.value}
              disabled={isAbsoluteStart}
              className={`w-7 h-7 rounded text-xs font-medium border disabled:cursor-not-allowed ${
                isAbsoluteStart
                  ? jobOrigin === cell.value
                    ? 'bg-bb-accent/40 text-bb-text-muted border-bb-accent/50'
                    : 'bg-bb-panel text-bb-text-muted border-bb-border opacity-50'
                  : jobOrigin === cell.value
                  ? 'bg-bb-accent text-bb-on-accent border-bb-accent'
                  : 'bg-bb-panel text-bb-text-muted border-bb-border hover:bg-bb-hover'
              }`}
              onClick={() => void setJobOrigin(cell.value)}
              title={
                isAbsoluteStart
                  ? t('panels.machine.laser.job_origin_ignored_title')
                  : t(ANCHOR_TITLE_KEYS[cell.value])
              }
            >
              {cell.label}
            </button>
          ))}
        </div>
      </div>

      {/* Preflight dialog */}
      {showPreflightDialog && preflightReport && (
        <PreflightDialog
          report={preflightReport}
          onClose={closePreflightDialog}
        />
      )}

      {/* Devices dialog */}
      {showDevicesDialog && (
        <DeviceSettingsDialog onClose={() => setShowDevicesDialog(false)} />
      )}
    </div>
  );
}
