import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { useMachineStore } from '../../stores/machineStore';
import type {
  ControllerMismatchDecision,
  ControllerSelection,
  ExplicitControllerSelection,
  TransportKind,
} from '../../types/machine';
import { Select } from '../shared/Select';

type ControllerSelectionValue =
  | 'auto_detect'
  | 'grbl'
  | 'fluid_nc'
  | 'grbl_hal'
  | 'laser_pecker'
  | 'marlin'
  | 'snapmaker'
  | 'smoothieware'
  | 'ruida'
  | 'lihuiyu'
  | 'generic_grbl_compatible';

export function controllerSelectionValue(selection: ControllerSelection): ControllerSelectionValue {
  if (selection.mode === 'auto_detect') return 'auto_detect';
  if (selection.mode === 'generic_grbl_compatible') return 'generic_grbl_compatible';
  if (selection.mode === 'known_driver') {
    if (selection.driver === 'fluid_nc') return 'fluid_nc';
    if (selection.driver === 'grbl_hal') return 'grbl_hal';
    if (selection.driver === 'laser_pecker') return 'laser_pecker';
    if (selection.driver === 'marlin') return 'marlin';
    if (selection.driver === 'snapmaker') return 'snapmaker';
    if (selection.driver === 'smoothieware') return 'smoothieware';
    if (selection.driver === 'ruida') return 'ruida';
    if (selection.driver === 'lihuiyu') return 'lihuiyu';
    return 'grbl';
  }
  return 'auto_detect';
}

export function controllerSelectionFromValue(value: string): ControllerSelection {
  switch (value) {
    case 'grbl':
    case 'fluid_nc':
    case 'grbl_hal':
    case 'laser_pecker':
    case 'marlin':
    case 'snapmaker':
    case 'smoothieware':
    case 'ruida':
    case 'lihuiyu':
      return { mode: 'known_driver', driver: value };
    case 'generic_grbl_compatible':
      return { mode: 'generic_grbl_compatible' };
    default:
      return { mode: 'auto_detect' };
  }
}

function sameSelection(
  current: ControllerSelection,
  prompted: ExplicitControllerSelection,
): boolean {
  if (current.mode !== prompted.mode) return false;
  if (current.mode === 'known_driver' && prompted.mode === 'known_driver') {
    return current.driver === prompted.driver;
  }
  return true;
}

function decisionLabel(
  decision: ControllerMismatchDecision,
  detectedLabel: string,
  t: (key: string, options?: Record<string, unknown>) => string,
): string {
  switch (decision) {
    case 'use_detected':
      return t('controller_choice.use_detected', { controller: detectedLabel });
    case 'continue_selected_experimentally':
      return t('controller_choice.continue_experimental');
    case 'cancel':
      return t('common.cancel');
  }
}

export function ControllerChoiceControls({
  disabled = false,
  transportKind = 'serial',
}: {
  disabled?: boolean;
  transportKind?: Extract<TransportKind, 'serial' | 'tcp' | 'usb_packet'>;
}) {
  const { t } = useTranslation();
  const selection = useMachineStore((state) => state.controllerSelection);
  const challenge = useMachineStore((state) => state.controllerConnectionChallenge);
  const loading = useMachineStore((state) => state.loading);
  const setSelection = useMachineStore((state) => state.setControllerSelection);
  const continueConnection = useMachineStore((state) => state.continueControllerConnection);
  const resolution = challenge?.resolution;
  const detectedIdentity = challenge?.detected_identity;
  const detectedLabel =
    detectedIdentity?.firmware_identity ??
    detectedIdentity?.model ??
    t('controller_choice.unknown_controller');
  const detectedVersion = detectedIdentity?.firmware_version;

  const serialOptions = [
    { value: 'grbl', label: t('controller_choice.option_grbl') },
    { value: 'auto_detect', label: t('controller_choice.option_auto') },
    { value: 'fluid_nc', label: t('controller_choice.option_fluidnc') },
    { value: 'grbl_hal', label: t('controller_choice.option_grblhal') },
    { value: 'laser_pecker', label: t('controller_choice.option_laserpecker') },
    { value: 'marlin', label: t('controller_choice.option_marlin') },
    { value: 'snapmaker', label: t('controller_choice.option_snapmaker') },
    { value: 'smoothieware', label: t('controller_choice.option_smoothieware') },
    { value: 'generic_grbl_compatible', label: t('controller_choice.option_generic') },
  ];
  const networkOptions = [
    { value: 'auto_detect', label: t('controller_choice.option_auto') },
    { value: 'fluid_nc', label: t('controller_choice.option_fluidnc') },
    { value: 'grbl_hal', label: t('controller_choice.option_grblhal') },
    { value: 'laser_pecker', label: t('controller_choice.option_laserpecker') },
    { value: 'ruida', label: t('controller_choice.option_ruida') },
  ];
  const usbOptions = [{ value: 'lihuiyu', label: t('controller_choice.option_lihuiyu') }];
  const options =
    transportKind === 'tcp'
      ? networkOptions
      : transportKind === 'usb_packet'
        ? usbOptions
        : serialOptions;

  useEffect(() => {
    const value = controllerSelectionValue(selection);
    const allowed =
      transportKind === 'tcp'
        ? ['auto_detect', 'fluid_nc', 'grbl_hal', 'laser_pecker', 'ruida']
        : transportKind === 'usb_packet'
          ? ['lihuiyu']
          : [
              'auto_detect',
              'grbl',
              'fluid_nc',
              'grbl_hal',
              'laser_pecker',
              'marlin',
              'snapmaker',
              'smoothieware',
              'generic_grbl_compatible',
            ];
    if (!allowed.includes(value)) {
      setSelection(
        transportKind === 'usb_packet'
          ? { mode: 'known_driver', driver: 'lihuiyu' }
          : { mode: 'auto_detect' },
      );
    }
  }, [selection, setSelection, transportKind]);

  const promptedSelection =
    resolution && resolution.outcome === 'mismatch_decision_required' ? resolution.selected : null;
  const selectionChanged =
    promptedSelection !== null && !sameSelection(selection, promptedSelection);
  const decisions =
    resolution && resolution.outcome === 'mismatch_decision_required' && !selectionChanged
      ? resolution.allowed_decisions
      : [];

  return (
    <div className="space-y-2" data-testid="controller-choice-controls">
      <Select
        label={t('controller_choice.controller')}
        value={controllerSelectionValue(selection)}
        options={options}
        onChange={(value) => setSelection(controllerSelectionFromValue(value))}
        disabled={disabled || loading}
      />
      {challenge && resolution && (
        <div
          className="space-y-2 rounded border border-bb-warning-border bg-bb-warning-bg p-2"
          data-testid="controller-choice-challenge"
        >
          {detectedIdentity && (
            <div className="text-xs font-medium text-bb-text">
              {t('controller_choice.detected', {
                controller: detectedLabel,
                version: detectedVersion ? ` ${detectedVersion}` : '',
              })}
            </div>
          )}
          <div className="text-xs text-bb-warning-fg">
            {resolution.outcome === 'selection_required'
              ? t('controller_choice.selection_required')
              : resolution.outcome === 'mismatch_decision_required'
                ? t('controller_choice.mismatch')
                : resolution.outcome === 'blocked'
                  ? resolution.message
                  : t('controller_choice.choose_next_step')}
          </div>
          <div className="flex flex-wrap gap-1">
            {(resolution.outcome === 'selection_required' ||
              resolution.outcome === 'blocked' ||
              selectionChanged) && (
              <button
                type="button"
                className="rounded bg-bb-accent px-2 py-1 text-xs text-bb-on-accent hover:bg-bb-accent-hover disabled:opacity-60"
                disabled={loading}
                onClick={() => void continueConnection()}
              >
                {t('controller_choice.try_selection')}
              </button>
            )}
            {decisions.map((decision) => (
              <button
                key={decision}
                type="button"
                className="rounded border border-bb-border bg-bb-bg px-2 py-1 text-xs text-bb-text hover:bg-bb-hover disabled:opacity-60"
                disabled={loading}
                onClick={() => void continueConnection(decision)}
              >
                {decisionLabel(decision, detectedLabel, t)}
              </button>
            ))}
            {decisions.length === 0 && resolution.outcome !== 'cancelled' && (
              <button
                type="button"
                className="rounded border border-bb-border bg-bb-bg px-2 py-1 text-xs text-bb-text hover:bg-bb-hover disabled:opacity-60"
                disabled={loading}
                onClick={() => void continueConnection('cancel')}
              >
                {t('common.cancel')}
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
