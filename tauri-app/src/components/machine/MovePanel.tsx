import { useCallback, useEffect, useRef, useState, type PointerEvent, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import {
  ArrowDown,
  ArrowDownLeft,
  ArrowDownRight,
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  ArrowUpLeft,
  ArrowUpRight,
  Crosshair,
  Flame,
  Home,
  LocateFixed,
  MapPin,
  Navigation,
  RotateCcw,
  Save,
  Square,
  Trash2,
  X,
} from 'lucide-react';
import type { SavedPosition } from '../../types/commands';
import { machineService } from '../../services/machineService';
import { useMachineStore } from '../../stores/machineStore';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { NumberInput } from '../shared/NumberInput';
import { StatusDisplay } from './StatusDisplay';
import { OverrideControls } from './OverrideControls';
import { moveLaserToSelection } from '../../commands/arrangeActions';
import { wrapBackendError } from '../../i18n/errors';
import { mmToDisplay, displayToMm, roundDisplayLength, lengthStep, lengthUnitLabel, labelWithUnit } from '../../utils/lengthUnits';
import { speedInputValue, displaySpeedToMmMin, speedStepForUnit, speedUnitLabel, formatSpeedForDisplay } from '../../utils/speedUnits';
import { useAppStore } from '../../stores/appStore';

const STEP_SIZES = [0.1, 1, 10, 100] as const;
const STEP_SIZES_IN = [0.005, 0.05, 0.5, 2] as const;
const CONTINUOUS_JOG_DISTANCE_MM = 100000;
const HOLD_DELAY_MS = 250;
const SHOW_CONNECTION_PREVIEW =
  (import.meta as unknown as { env?: { DEV?: boolean } }).env?.DEV === true;

const BTN =
  'inline-flex items-center justify-center gap-1 rounded border border-bb-border bg-bb-surface px-2 py-1 text-xs text-bb-text hover:bg-bb-hover disabled:cursor-default disabled:opacity-50';
const ICON_BTN =
  'inline-flex h-10 w-10 items-center justify-center rounded border border-bb-border bg-bb-surface text-bb-text hover:bg-bb-hover disabled:cursor-default disabled:opacity-50';
const PRIMARY_BTN =
  'inline-flex items-center justify-center gap-1 rounded bg-bb-accent px-2 py-1 text-xs font-medium text-bb-on-accent hover:bg-bb-accent-hover disabled:cursor-default disabled:opacity-50';
const SECTION = 'space-y-2.5 border-t border-bb-border pt-3';
const SECTION_HEADER = 'text-xs font-medium text-bb-accent uppercase tracking-wider';
const FIRE_SECTION = 'space-y-2.5 border-t border-red-500/50 bg-red-500/5 px-2 pb-2 pt-3 -mx-1';

type JogVector = { x: number; y: number };

function positionText(
  pos: { x: number; y: number; z?: number | null } | null | undefined,
  showZ: boolean,
  displayUnit: 'mm' | 'inches',
) {
  if (!pos) return '--';
  const unit = lengthUnitLabel(displayUnit);
  const x = roundDisplayLength(mmToDisplay(pos.x, displayUnit), displayUnit);
  const y = roundDisplayLength(mmToDisplay(pos.y, displayUnit), displayUnit);
  const zPart = showZ
    ? `, Z ${roundDisplayLength(mmToDisplay(Number(pos.z ?? 0), displayUnit), displayUnit)}`
    : '';
  return `X ${x}, Y ${y}${zPart} ${unit}`;
}

function isJobActive(state: string | undefined) {
  return state === 'preparing' || state === 'ready_to_run' || state === 'running' || state === 'paused';
}

function connectionDotClass(connected: boolean, readyIdle: boolean) {
  if (readyIdle) return 'bg-green-500';
  if (connected) return 'bg-amber-500';
  return 'bg-bb-text-dim';
}

function axisReadout(value: number | undefined, displayUnit: 'mm' | 'inches') {
  if (value === undefined) return '--';
  const converted = mmToDisplay(value, displayUnit);
  return displayUnit === 'inches' ? converted.toFixed(4) : converted.toFixed(2);
}

export function MovePanel(): React.ReactElement {
  const { t } = useTranslation();
  const project = useProjectStore((s) => s.project);
  const selectedObjectIds = useProjectStore((s) => s.selectedObjectIds);
  const setOptimization = useProjectStore((s) => s.setOptimization);
  const sessionState = useMachineStore((s) => s.sessionState);
  const machineStatus = useMachineStore((s) => s.machineStatus);
  const machineCoordinatesValid = useMachineStore((s) => s.machineCoordinatesValid);
  const connectionPreview = useMachineStore((s) => s.connectionPreview);
  const capabilities = useMachineStore((s) => s.capabilities);
  const jobProgress = useMachineStore((s) => s.jobProgress);
  const profiles = useMachineStore((s) => s.profiles);
  const activeProfileId = useMachineStore((s) => s.activeProfileId);
  const refreshStatus = useMachineStore((s) => s.refreshStatus);
  const refreshSessionState = useMachineStore((s) => s.refreshSessionState);
  const loadProfiles = useMachineStore((s) => s.loadProfiles);
  const home = useMachineStore((s) => s.home);
  const unlock = useMachineStore((s) => s.unlock);
  const setWorkOrigin = useMachineStore((s) => s.setWorkOrigin);
  const resetWorkOrigin = useMachineStore((s) => s.resetWorkOrigin);
  const setConnectionPreview = useMachineStore((s) => s.setConnectionPreview);
  const jogDistance = useUiStore((s) => s.moveWindowJogDistanceMm);
  const setJogDistance = useUiStore((s) => s.setMoveWindowJogDistanceMm);
  const moveFeedRate = useUiStore((s) => s.moveWindowJogFeedRateMmMin);
  const setMoveFeedRate = useUiStore((s) => s.setMoveWindowJogFeedRateMmMin);
  const displayUnit = useAppStore((s) => s.settings?.display_unit) ?? 'mm';
  const speedTimeUnit = useAppStore((s) => s.settings?.speed_time_unit) ?? 'minutes';
  const stepSizes = displayUnit === 'inches' ? STEP_SIZES_IN : STEP_SIZES;

  const profileList = Array.isArray(profiles) ? profiles : [];
  const activeProfile = profileList.find((profile) => profile.id === activeProfileId) ?? profileList[0];
  const isRuidaProfile = activeProfile?.firmware_type.trim().toLowerCase() === 'ruida';
  const supportsZ = !isRuidaProfile && activeProfile?.supports_z_moves === true;
  const ruidaTableAxis = activeProfile?.ruida_table_axis ?? 'disabled';
  const supportsRuidaTableJog = isRuidaProfile && ruidaTableAxis !== 'disabled';
  const fireEnabled = activeProfile?.enable_laser_fire_button === true;
  const connected = sessionState !== 'disconnected';
  const alarmLocked = sessionState === 'alarm' || machineStatus?.run_state === 'alarm';
  const readyIdle = sessionState === 'ready' && machineStatus?.run_state === 'idle';
  // Gate manual controls on the connected controller's reported capabilities
  // (null while unknown - gates fail closed). Driver-string checks are not a
  // substitute: multiple controllers share the same constraints.
  const supportsAbsolutePositioning = capabilities?.reports_absolute_position === true;
  const homeSupported = capabilities?.can_home === true;
  const jogSupported = capabilities?.can_jog === true;
  const continuousJogSupported = capabilities?.can_jog_continuous === true;
  const unlockSupported = capabilities?.can_unlock === true;
  const manualFireSupported = capabilities?.can_manual_fire === true;
  const overridesSupported = capabilities?.can_adjust_overrides === true;
  const machineZeroReady = supportsAbsolutePositioning && readyIdle && machineCoordinatesValid;
  const machineZeroNeedsHome = supportsAbsolutePositioning && readyIdle && !machineCoordinatesValid;
  const jogging = machineStatus?.run_state === 'jog';
  const hasSelection = !!project && selectedObjectIds.length > 0;
  const hasProject = !!project;
  const activeJob = isJobActive(jobProgress?.state);
  // Controllers without absolute position reporting (Ruida, Lihuiyu) share a
  // zero-valued status placeholder. Do not present it as a measured position
  // or let it seed software absolute-position actions.
  const workPosition = supportsAbsolutePositioning ? machineStatus?.work_position : undefined;

  const [goX, setGoX] = useState(0);
  const [goY, setGoY] = useState(0);
  const [goZ, setGoZ] = useState(0);
  const [savedPositions, setSavedPositions] = useState<SavedPosition[]>([]);
  const [saveModalOpen, setSaveModalOpen] = useState(false);
  const [manageOpen, setManageOpen] = useState(false);
  const [positionName, setPositionName] = useState('');
  const savedPositionList = Array.isArray(savedPositions) ? savedPositions : [];
  const hiddenSavedPositionCount = Math.max(0, savedPositionList.length - 4);
  const [fireHeld, setFireHeld] = useState(false);

  const holdTimerRef = useRef<number | null>(null);
  const pendingJogRef = useRef<JogVector | null>(null);
  const continuousJogActiveRef = useRef(false);
  const finiteJogOnlyRef = useRef(false);
  const fireTokenRef = useRef<string | null>(null);
  const fireStartPendingRef = useRef(false);
  const fireReleasePendingRef = useRef(false);
  const fireKeepaliveRef = useRef<number | null>(null);

  const notifyError = useCallback((error: unknown) => {
    useNotificationStore.getState().push(wrapBackendError(String(error)), 'error');
  }, []);

  const reloadSavedPositions = useCallback(async () => {
    try {
      const positions = await machineService.getSavedPositions();
      setSavedPositions(Array.isArray(positions) ? positions : []);
    } catch {
      setSavedPositions([]);
    }
  }, []);

  useEffect(() => {
    void reloadSavedPositions();
    void loadProfiles();
  }, [loadProfiles, reloadSavedPositions]);

  useEffect(() => {
    if (!workPosition) return;
    setGoX(workPosition.x);
    setGoY(workPosition.y);
    setGoZ(workPosition.z ?? 0);
  }, [workPosition]);

  const stopFire = useCallback(async () => {
    const token = fireTokenRef.current;
    setFireHeld(false);
    if (fireKeepaliveRef.current !== null) {
      window.clearInterval(fireKeepaliveRef.current);
      fireKeepaliveRef.current = null;
    }
    fireTokenRef.current = null;
    if (!token) {
      if (fireStartPendingRef.current) {
        fireReleasePendingRef.current = true;
      }
      return;
    }
    fireReleasePendingRef.current = false;
    try {
      await machineService.laserFireStop(token);
    } catch (error) {
      notifyError(error);
    }
  }, [notifyError]);

  const releaseJog = useCallback(async () => {
    const pending = pendingJogRef.current;
    pendingJogRef.current = null;
    if (finiteJogOnlyRef.current) {
      finiteJogOnlyRef.current = false;
      if (pending && readyIdle) {
        try {
          await machineService.jog(pending.x * jogDistance, pending.y * jogDistance, moveFeedRate);
          await refreshStatus();
          await refreshSessionState();
        } catch (error) {
          notifyError(error);
        }
      }
      return;
    }
    if (holdTimerRef.current !== null) {
      window.clearTimeout(holdTimerRef.current);
      holdTimerRef.current = null;
      if (pending && readyIdle) {
        try {
          await machineService.jog(pending.x * jogDistance, pending.y * jogDistance, moveFeedRate);
          await refreshStatus();
          await refreshSessionState();
        } catch (error) {
          notifyError(error);
        }
      }
      return;
    }
    if (continuousJogActiveRef.current) {
      continuousJogActiveRef.current = false;
      try {
        await machineService.jogCancel();
        await refreshStatus();
        await refreshSessionState();
      } catch (error) {
        notifyError(error);
      }
    }
  }, [jogDistance, moveFeedRate, notifyError, readyIdle, refreshSessionState, refreshStatus]);

  useEffect(() => {
    const handleWindowBlur = () => {
      void releaseJog();
      void stopFire();
    };
    const handlePointerRelease = () => {
      void releaseJog();
      void stopFire();
    };
    const handleVisibility = () => {
      if (document.hidden) {
        void releaseJog();
        void stopFire();
      }
    };
    window.addEventListener('blur', handleWindowBlur);
    window.addEventListener('pointerup', handlePointerRelease);
    window.addEventListener('pointercancel', handlePointerRelease);
    document.addEventListener('visibilitychange', handleVisibility);
    return () => {
      window.removeEventListener('blur', handleWindowBlur);
      window.removeEventListener('pointerup', handlePointerRelease);
      window.removeEventListener('pointercancel', handlePointerRelease);
      document.removeEventListener('visibilitychange', handleVisibility);
      void releaseJog();
      void stopFire();
    };
  }, [releaseJog, stopFire]);

  useEffect(() => {
    if (!readyIdle || !connected) {
      void releaseJog();
      void stopFire();
    }
  }, [connected, readyIdle, releaseJog, stopFire]);

  const handleStopJog = async () => {
    // Clear any pending/active jog bookkeeping so releaseJog doesn't double-cancel.
    pendingJogRef.current = null;
    continuousJogActiveRef.current = false;
    finiteJogOnlyRef.current = false;
    if (holdTimerRef.current !== null) {
      window.clearTimeout(holdTimerRef.current);
      holdTimerRef.current = null;
    }
    try {
      await machineService.jogCancel();
      await refreshStatus();
      await refreshSessionState();
    } catch (error) {
      notifyError(error);
    }
  };

  const startJogPointer = (event: PointerEvent<HTMLButtonElement>, vector: JogVector) => {
    if (!jogSupported || !readyIdle) return;
    event.currentTarget.setPointerCapture?.(event.pointerId);
    pendingJogRef.current = vector;
    if (!continuousJogSupported) {
      finiteJogOnlyRef.current = true;
      return;
    }
    holdTimerRef.current = window.setTimeout(() => {
      holdTimerRef.current = null;
      continuousJogActiveRef.current = true;
      void machineService
        .jog(
          vector.x * CONTINUOUS_JOG_DISTANCE_MM,
          vector.y * CONTINUOUS_JOG_DISTANCE_MM,
          moveFeedRate,
          null,
          true,
        )
        .catch((error) => {
          continuousJogActiveRef.current = false;
          notifyError(error);
        });
    }, HOLD_DELAY_MS);
  };

  const handleGo = async () => {
    try {
      await machineService.moveLaserTo(goX, goY, moveFeedRate, supportsZ ? goZ : null);
      await refreshStatus();
      useNotificationStore.getState().push(
        t('panels.move.notifications.moving_to_coordinates', {
          x: roundDisplayLength(mmToDisplay(goX, displayUnit), displayUnit),
          y: roundDisplayLength(mmToDisplay(goY, displayUnit), displayUnit),
          unit: lengthUnitLabel(displayUnit),
        }),
        'info',
      );
    } catch (error) {
      notifyError(error);
    }
  };

  const handleRuidaTableJog = async (direction: -1 | 1) => {
    if (!supportsRuidaTableJog || !jogSupported || !readyIdle) return;
    try {
      await machineService.jog(
        0,
        0,
        activeProfile?.z_move_feed_mm_min ?? 300,
        direction * jogDistance,
      );
      await refreshStatus();
      await refreshSessionState();
    } catch (error) {
      notifyError(error);
    }
  };

  const handleGoMachineZero = async () => {
    if (!machineCoordinatesValid) {
      useNotificationStore.getState().push(t('errors.machine_zero_requires_home'), 'warning');
      return;
    }
    try {
      await machineService.moveLaserToMachine(0, 0, moveFeedRate, supportsZ ? 0 : null);
      await refreshStatus();
      useNotificationStore.getState().push(t('panels.move.notifications.moving_to_machine_zero'), 'info');
    } catch (error) {
      notifyError(error);
    }
  };

  const handleMoveLaserToSelection = async () => {
    try {
      await moveLaserToSelection('center');
      await refreshStatus();
    } catch (error) {
      notifyError(error);
    }
  };

  const handleSaveCurrent = async () => {
    const pos = workPosition;
    if (!pos) return;
    const name = positionName.trim() || t('panels.move.default_position_name', { index: savedPositionList.length + 1 });
    try {
      const updated = await machineService.savePosition(name, pos.x, pos.y, supportsZ ? pos.z : null);
      setSavedPositions(Array.isArray(updated) ? updated : []);
      setSaveModalOpen(false);
      setPositionName('');
    } catch (error) {
      notifyError(error);
    }
  };

  const handleDeleteSaved = async (id: string) => {
    try {
      const updated = await machineService.deleteSavedPosition(id);
      setSavedPositions(Array.isArray(updated) ? updated : []);
    } catch (error) {
      notifyError(error);
    }
  };

  const handleGoSaved = async (position: SavedPosition) => {
    try {
      await machineService.moveLaserTo(position.x, position.y, moveFeedRate, supportsZ ? position.z ?? null : null);
      await refreshStatus();
      useNotificationStore.getState().push(t('panels.move.notifications.moving_to_position', { name: position.name }), 'info');
    } catch (error) {
      notifyError(error);
    }
  };

  const handleGoOrigin = async () => {
    const origin = project?.user_origin;
    if (!origin) return;
    try {
      await machineService.moveLaserTo(origin[0], origin[1], moveFeedRate);
      await refreshStatus();
      useNotificationStore.getState().push(t('panels.move.notifications.moving_to_user_origin'), 'info');
    } catch (error) {
      notifyError(error);
    }
  };

  const handleSetFinish = async () => {
    const pos = workPosition;
    if (!pos || !project) return;
    try {
      await setOptimization({ finish_position: 'custom_xy', finish_x: pos.x, finish_y: pos.y });
      useNotificationStore.getState().push(t('panels.move.notifications.finish_position_set'), 'success');
    } catch (error) {
      notifyError(error);
    }
  };

  const handleFireStart = async (event: PointerEvent<HTMLButtonElement>) => {
    if (!fireEnabled || !readyIdle || fireTokenRef.current) return;
    event.currentTarget.setPointerCapture?.(event.pointerId);
    fireStartPendingRef.current = true;
    fireReleasePendingRef.current = false;
    setFireHeld(true);
    try {
      const result = await machineService.laserFireStart(activeProfile?.default_fire_power_percent ?? 1);
      fireStartPendingRef.current = false;
      if (fireReleasePendingRef.current) {
        fireReleasePendingRef.current = false;
        setFireHeld(false);
        await machineService.laserFireStop(result.token);
        return;
      }
      fireTokenRef.current = result.token;
      fireKeepaliveRef.current = window.setInterval(() => {
        const token = fireTokenRef.current;
        if (!token) return;
        void machineService.laserFireKeepalive(token).catch((error) => {
          void stopFire();
          notifyError(error);
        });
      }, Math.max(100, result.keepalive_interval_ms));
    } catch (error) {
      fireStartPendingRef.current = false;
      setFireHeld(false);
      notifyError(error);
    }
  };

  const jogButtons: Array<{ key: string; title: string; vector: JogVector; icon: ReactNode }> = [
    { key: 'nw', title: t('panels.machine.jog.northwest'), vector: { x: -1, y: 1 }, icon: <ArrowUpLeft size={16} /> },
    { key: 'n', title: t('panels.machine.jog.up'), vector: { x: 0, y: 1 }, icon: <ArrowUp size={16} /> },
    { key: 'ne', title: t('panels.machine.jog.northeast'), vector: { x: 1, y: 1 }, icon: <ArrowUpRight size={16} /> },
    { key: 'w', title: t('panels.machine.jog.left'), vector: { x: -1, y: 0 }, icon: <ArrowLeft size={16} /> },
    { key: 'center', title: '', vector: { x: 0, y: 0 }, icon: <Crosshair size={15} /> },
    { key: 'e', title: t('panels.machine.jog.right'), vector: { x: 1, y: 0 }, icon: <ArrowRight size={16} /> },
    { key: 'sw', title: t('panels.machine.jog.southwest'), vector: { x: -1, y: -1 }, icon: <ArrowDownLeft size={16} /> },
    { key: 's', title: t('panels.machine.jog.down'), vector: { x: 0, y: -1 }, icon: <ArrowDown size={16} /> },
    { key: 'se', title: t('panels.machine.jog.southeast'), vector: { x: 1, y: -1 }, icon: <ArrowDownRight size={16} /> },
  ];

  return (
    <div className="space-y-3.5 px-3 pb-3 text-xs text-bb-text">
      <div className="space-y-2">
        <div className="flex items-center justify-between gap-2">
          <div>
            <div className={SECTION_HEADER}>{t('panels.move.machine_positioning')}</div>
            <div className="mt-1 flex items-center gap-1.5 text-bb-text-muted">
              <span className={`h-2 w-2 rounded-full ${connectionDotClass(connected, readyIdle)}`} />
              <span>{connected ? t('panels.move.status_connected') : t('panels.move.status_disconnected')}</span>
            </div>
          </div>
          <div className="flex flex-wrap justify-end gap-1">
            <button className={BTN} onClick={() => void refreshStatus()} disabled={!connected}>
              <LocateFixed size={14} />
              {t('panels.move.get_position')}
            </button>
            <button
              className={BTN}
              onClick={() => void home()}
              disabled={!homeSupported || !readyIdle}
            >
              <Home size={14} />
              {t('panels.machine.jog.home')}
            </button>
            <button className={BTN} onClick={() => void unlock()} disabled={!unlockSupported || !alarmLocked}>
              <RotateCcw size={14} />
              {t('panels.machine.jog.unlock')}
            </button>
          </div>
        </div>
        <div
          className={`grid gap-1 rounded border border-bb-border bg-bb-bg px-2 py-1.5 font-mono text-[11px] tabular-nums ${supportsZ ? 'grid-cols-3' : 'grid-cols-2'}`}
          aria-label={t('panels.move.current_position', { position: positionText(workPosition, supportsZ, displayUnit) })}
        >
          <div className="min-w-0">
            <span className="mr-1 text-bb-text-dim">X</span>
            <span>{axisReadout(workPosition?.x, displayUnit)}</span>
          </div>
          <div className="min-w-0">
            <span className="mr-1 text-bb-text-dim">Y</span>
            <span>{axisReadout(workPosition?.y, displayUnit)}</span>
          </div>
          {supportsZ && (
            <div className="min-w-0">
              <span className="mr-1 text-bb-text-dim">Z</span>
              <span>{axisReadout(workPosition?.z, displayUnit)}</span>
            </div>
          )}
        </div>
        {connected && <StatusDisplay />}
        {SHOW_CONNECTION_PREVIEW && (
          <label className="flex items-center justify-between gap-2 rounded border border-dashed border-bb-border bg-bb-surface px-2 py-1 text-bb-text-muted">
            <span>
              <span className="block text-bb-text">{t('panels.move.preview_connected_ui')}</span>
              <span className="block">{t('panels.move.preview_connected_help')}</span>
            </span>
            <input
              type="checkbox"
              checked={connectionPreview}
              onChange={(event) => setConnectionPreview(event.target.checked)}
              className="h-3.5 w-3.5 accent-bb-accent"
            />
          </label>
        )}
      </div>

      <div className={SECTION}>
        <div className="flex items-center justify-between">
          <span className={SECTION_HEADER}>{t('panels.move.feed_rate')}</span>
          <span className="text-bb-text-muted">{formatSpeedForDisplay(moveFeedRate, displayUnit, speedTimeUnit)} {speedUnitLabel(displayUnit, speedTimeUnit)}</span>
        </div>
        <NumberInput
          label={labelWithUnit(t('panels.move.feed_mm_min'), speedUnitLabel(displayUnit, speedTimeUnit))}
          value={speedInputValue(moveFeedRate, displayUnit, speedTimeUnit)}
          onChange={(v) => setMoveFeedRate(displaySpeedToMmMin(v, displayUnit, speedTimeUnit))}
          min={speedInputValue(1, displayUnit, speedTimeUnit)}
          max={speedInputValue(10000, displayUnit, speedTimeUnit)}
          step={speedStepForUnit(displayUnit, speedTimeUnit)}
        />
      </div>

      <div className={SECTION}>
        <div className={SECTION_HEADER}>{t('panels.move.go_to_position')}</div>
        <div className="grid grid-cols-2 gap-2">
          <NumberInput
            label={labelWithUnit(t('panels.move.axis_x'), lengthUnitLabel(displayUnit))}
            value={roundDisplayLength(mmToDisplay(goX, displayUnit), displayUnit)}
            onChange={(v) => setGoX(displayToMm(v, displayUnit))}
            step={lengthStep(displayUnit, 1, 0.05)}
          />
          <NumberInput
            label={labelWithUnit(t('panels.move.axis_y'), lengthUnitLabel(displayUnit))}
            value={roundDisplayLength(mmToDisplay(goY, displayUnit), displayUnit)}
            onChange={(v) => setGoY(displayToMm(v, displayUnit))}
            step={lengthStep(displayUnit, 1, 0.05)}
          />
          {supportsZ && (
            <NumberInput
              label={labelWithUnit(t('panels.move.axis_z'), lengthUnitLabel(displayUnit))}
              value={roundDisplayLength(mmToDisplay(goZ, displayUnit), displayUnit)}
              onChange={(v) => setGoZ(displayToMm(v, displayUnit))}
              step={lengthStep(displayUnit, 0.1, 0.005)}
            />
          )}
        </div>
        <div className="grid grid-cols-2 gap-1">
          <button className={`${PRIMARY_BTN} col-span-2`} disabled={!supportsAbsolutePositioning || !readyIdle} onClick={() => void handleGo()} data-testid="goto-button">
            <Navigation size={14} />
            {t('common.go')}
          </button>
          <button className={BTN} disabled={!machineZeroReady} onClick={() => void handleGoMachineZero()} data-testid="goto-machine-zero-button">
            <MapPin size={14} />
            {t('panels.move.go_machine_zero')}
          </button>
          <button className={BTN} disabled={!workPosition} onClick={() => {
            const pos = workPosition;
            if (!pos) return;
            setGoX(pos.x);
            setGoY(pos.y);
            setGoZ(pos.z ?? 0);
          }}>
            {t('panels.move.use_current')}
          </button>
          {machineZeroNeedsHome && (
            <div className="col-span-2 text-xs text-bb-text-muted">
              {t('errors.machine_zero_requires_home')}
            </div>
          )}
        </div>
      </div>

      <div className={SECTION}>
        <div className={SECTION_HEADER}>{t('panels.move.jog')}</div>
        <div className="flex gap-3">
          <div className="grid grid-cols-3 gap-0.5 rounded bg-bb-bg p-1.5">
            {jogButtons.map((button) => (
              button.key === 'center' ? (
                <div key={button.key} className="flex h-10 w-10 items-center justify-center rounded border border-bb-border bg-bb-bg-alt text-bb-text-muted">
                  {button.icon}
                </div>
              ) : (
                <button
                  key={button.key}
                  className={ICON_BTN}
                  title={button.title}
                  disabled={!jogSupported || !readyIdle}
                  onPointerDown={(event) => startJogPointer(event, button.vector)}
                  onPointerUp={() => void releaseJog()}
                  onPointerCancel={() => void releaseJog()}
                  onLostPointerCapture={() => void releaseJog()}
                >
                  {button.icon}
                </button>
              )
            ))}
          </div>
          <div className="min-w-0 flex-1 space-y-2">
            <div>
              <div className="mb-1 text-bb-text-muted">{labelWithUnit(t('panels.machine.jog.step_size_mm'), lengthUnitLabel(displayUnit))}</div>
              <div className="grid grid-cols-4 gap-1">
                {stepSizes.map((size) => (
                  <button
                    key={size}
                    className={roundDisplayLength(mmToDisplay(jogDistance, displayUnit), displayUnit) === size ? PRIMARY_BTN : BTN}
                    onClick={() => setJogDistance(displayToMm(size, displayUnit))}
                  >
                    {size}
                  </button>
                ))}
              </div>
            </div>
            {continuousJogSupported && <div className="text-bb-text-muted">{t('panels.move.hold_to_jog')}</div>}
            {supportsRuidaTableJog && (
              <div>
                <div className="mb-1 text-bb-text-muted">
                  {t('panels.move.lift_table_axis', { axis: ruidaTableAxis.toUpperCase() })}
                </div>
                <div className="grid grid-cols-2 gap-1">
                  <button
                    className={BTN}
                    disabled={!jogSupported || !readyIdle}
                    onClick={() => void handleRuidaTableJog(-1)}
                    data-testid="ruida-table-jog-negative"
                  >
                    <ArrowDown size={14} />
                    {ruidaTableAxis.toUpperCase()}−
                  </button>
                  <button
                    className={BTN}
                    disabled={!jogSupported || !readyIdle}
                    onClick={() => void handleRuidaTableJog(1)}
                    data-testid="ruida-table-jog-positive"
                  >
                    <ArrowUp size={14} />
                    {ruidaTableAxis.toUpperCase()}+
                  </button>
                </div>
              </div>
            )}
          </div>
        </div>
        {jogging && continuousJogSupported && (
          <button
            data-testid="stop-jog-button"
            className="inline-flex w-full items-center justify-center gap-1 rounded bg-bb-error px-2 py-1.5 text-xs font-semibold text-bb-on-error hover:bg-bb-error-hover"
            onClick={() => void handleStopJog()}
          >
            <Square size={14} />
            {t('panels.move.stop_jog')}
          </button>
        )}
      </div>

      <div className={SECTION}>
        <div className="flex items-center justify-between">
          <span className={SECTION_HEADER}>{t('panels.move.saved_positions')}</span>
          <div className="flex gap-1">
            <button className={BTN} disabled={!supportsAbsolutePositioning || !workPosition} onClick={() => setSaveModalOpen(true)}>
              <Save size={14} />
              {t('panels.move.save_current')}
            </button>
            <button className={BTN} onClick={() => setManageOpen(true)} disabled={savedPositionList.length === 0}>
              {t('panels.move.manage')}
            </button>
          </div>
        </div>
        {savedPositionList.length === 0 ? (
          <div className="text-bb-text-muted">{t('panels.move.no_saved_positions')}</div>
        ) : (
          <div className="space-y-1">
            {savedPositionList.slice(0, 4).map((position) => (
              <div key={position.id} className="flex items-center gap-1">
                <MapPin size={13} className="shrink-0 text-bb-text-dim" />
                <span className="min-w-0 flex-1 truncate text-bb-text-muted">
                  {t('panels.move.saved_position_summary', {
                    name: position.name,
                    x: roundDisplayLength(mmToDisplay(position.x, displayUnit), displayUnit),
                    y: roundDisplayLength(mmToDisplay(position.y, displayUnit), displayUnit),
                    z: position.z == null ? '' : roundDisplayLength(mmToDisplay(position.z, displayUnit), displayUnit),
                  })}
                </span>
                <button className={BTN} disabled={!supportsAbsolutePositioning || !readyIdle} onClick={() => void handleGoSaved(position)}>
                  {t('common.go')}
                </button>
              </div>
            ))}
            {hiddenSavedPositionCount > 0 && (
              <div className="pl-5 text-bb-text-dim">
                {t('panels.move.saved_positions_more', { count: hiddenSavedPositionCount })}
              </div>
            )}
          </div>
        )}
      </div>

      <div className={SECTION}>
        <div className={SECTION_HEADER}>{t('panels.move.origin_finish')}</div>
        <div className="grid grid-cols-2 gap-1">
          <button className={BTN} disabled={!supportsAbsolutePositioning || !readyIdle || !hasProject} onClick={() => void setWorkOrigin()}>
            <Crosshair size={14} />
            {t('panels.move.set_user_origin')}
          </button>
          <button className={BTN} disabled={!supportsAbsolutePositioning || !hasProject || !project?.user_origin} onClick={() => void resetWorkOrigin()}>
            <RotateCcw size={14} />
            {t('panels.move.clear_user_origin')}
          </button>
          <button className={BTN} disabled={!supportsAbsolutePositioning || !readyIdle || !project?.user_origin} onClick={() => void handleGoOrigin()}>
            <Navigation size={14} />
            {t('panels.move.go_to_user_origin')}
          </button>
          <button className={BTN} disabled={!supportsAbsolutePositioning || !workPosition || !hasProject} onClick={() => void handleSetFinish()}>
            <MapPin size={14} />
            {t('panels.move.set_finish_position')}
          </button>
        </div>
      </div>

      <div className={SECTION}>
        <button className={BTN + ' w-full'} disabled={!supportsAbsolutePositioning || !readyIdle || !hasSelection} onClick={() => void handleMoveLaserToSelection()}>
          <LocateFixed size={14} />
          {t('panels.move.laser_to_selection')}
        </button>
        {!hasProject && <div className="text-bb-text-muted">{t('panels.move.project_actions_disabled')}</div>}
      </div>

      {fireEnabled && manualFireSupported && (
        <div className={FIRE_SECTION}>
          <button
            className={`inline-flex w-full items-center justify-center gap-1 rounded bg-bb-error px-2 py-1.5 text-xs font-semibold text-bb-on-error hover:bg-bb-error-hover disabled:cursor-default disabled:opacity-50 ${fireHeld ? 'animate-pulse shadow-[0_0_0_1px_rgba(239,68,68,0.45),0_0_18px_rgba(239,68,68,0.25)]' : ''}`}
            disabled={!readyIdle}
            onPointerDown={(event) => void handleFireStart(event)}
            onPointerUp={() => void stopFire()}
            onPointerCancel={() => void stopFire()}
            onLostPointerCapture={() => void stopFire()}
          >
            <Flame size={14} />
            {t('panels.move.fire_hold')}
          </button>
          <div className="text-bb-text-muted">
            {t('panels.move.fire_power', { power: activeProfile?.default_fire_power_percent ?? 1 })}
          </div>
        </div>
      )}

      {activeJob && overridesSupported && (
        <div className={SECTION}>
          <OverrideControls />
        </div>
      )}

      {saveModalOpen && (
        <div className="rounded border border-bb-border bg-bb-surface p-2 shadow">
          <div className="mb-2 flex items-center justify-between">
            <span className="font-medium">{t('panels.move.save_position')}</span>
            <button className="text-bb-text-muted hover:text-bb-text" onClick={() => setSaveModalOpen(false)} title={t('common.close')}>
              <X size={14} />
            </button>
          </div>
          <input
            className="mb-2 w-full rounded border border-bb-border bg-bb-surface-elevated px-2 py-1 text-xs text-bb-text"
            value={positionName}
            onChange={(event) => setPositionName(event.target.value)}
            placeholder={t('panels.move.position_name')}
          />
          <button className={PRIMARY_BTN} onClick={() => void handleSaveCurrent()}>
            {t('common.save')}
          </button>
        </div>
      )}

      {manageOpen && (
        <div className="rounded border border-bb-border bg-bb-surface p-2 shadow">
          <div className="mb-2 flex items-center justify-between">
            <span className="font-medium">{t('panels.move.manage_positions')}</span>
            <button className="text-bb-text-muted hover:text-bb-text" onClick={() => setManageOpen(false)} title={t('common.close')}>
              <X size={14} />
            </button>
          </div>
          <div className="space-y-1">
            {savedPositionList.map((position) => (
              <div key={position.id} className="flex items-center gap-1">
                <span className="min-w-0 flex-1 truncate text-bb-text-muted">{position.name}</span>
                <button className={ICON_BTN + ' h-7 w-7'} onClick={() => void handleDeleteSaved(position.id)} title={t('panels.move.delete_position_title')}>
                  <Trash2 size={13} />
                </button>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
