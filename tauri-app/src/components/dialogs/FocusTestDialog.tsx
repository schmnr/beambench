import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { useMachineStore } from '../../stores/machineStore';
import { useProjectStore } from '../../stores/projectStore';
import { useAppStore } from '../../stores/appStore';
import { NumberInput } from '../shared/NumberInput';
import { Select } from '../shared/Select';
import { Toggle } from '../shared/Toggle';
import {
  mmToDisplay,
  displayToMm,
  roundDisplayLength,
  lengthStep,
  lengthUnitLabel,
  labelWithUnit,
} from '../../utils/lengthUnits';
import {
  speedInputValue,
  displaySpeedToMmMin,
  speedStepForUnit,
  speedUnitLabel,
} from '../../utils/speedUnits';
import { QualityTestShell, useQualityTestSettingsPersistence } from './QualityTestShell';
import {
  DEFAULT_FOCUS_TEST_SETTINGS,
  type FocusTestSettings,
  type FocusTestZMode,
  type QualityTestRequest,
} from '../../types/machine';

interface FocusTestDialogProps {
  onClose: () => void;
}

const FOCUS_CONTROL_WIDTH = 'w-24';

function normalizeFocusSettings(raw: Partial<FocusTestSettings> | undefined): FocusTestSettings {
  const legacy = raw as (Partial<FocusTestSettings> & {
    steps?: number;
    high_power_labels?: boolean;
  }) | undefined;
  return {
    ...DEFAULT_FOCUS_TEST_SETTINGS,
    ...(raw ?? {}),
    intervals: Math.max(
      1,
      Math.round(legacy?.intervals ?? legacy?.steps ?? DEFAULT_FOCUS_TEST_SETTINGS.intervals),
    ),
    perforated_labels:
      legacy?.perforated_labels ??
      legacy?.high_power_labels ??
      DEFAULT_FOCUS_TEST_SETTINGS.perforated_labels,
  };
}

export function FocusTestDialog({ onClose }: FocusTestDialogProps) {
  const { t } = useTranslation();
  const profiles = useMachineStore((s) => s.profiles);
  const displayUnit = useAppStore((s) => s.settings?.display_unit) ?? 'mm';
  const speedTimeUnit = useAppStore((s) => s.settings?.speed_time_unit) ?? 'minutes';

  const zModeOptions: Array<{ value: FocusTestZMode; label: string }> = [
    { value: 'AbsoluteWorkCoord', label: t('dialog.focus_test.z_mode_material') },
    { value: 'RelativeTemporary', label: t('dialog.focus_test.z_mode_current') },
  ];
  const activeProfileId = useMachineStore((s) => s.activeProfileId);
  const project = useProjectStore((s) => s.project);
  const setMaterialHeightAction = useProjectStore((s) => s.setMaterialHeight);

  const activeProfile = useMemo(
    () => profiles.find((p) => p.id === activeProfileId) ?? null,
    [profiles, activeProfileId],
  );

  const persisted = useMemo(
    () => normalizeFocusSettings(activeProfile?.quality_test_settings?.focus),
    [activeProfile],
  );

  const [settings, setSettings] = useState<FocusTestSettings>(persisted);
  // Re-seed when active profile changes — see MaterialTestDialog for rationale.
  const lastProfileIdRef = useRef(activeProfileId);
  useEffect(() => {
    if (lastProfileIdRef.current !== activeProfileId) {
      lastProfileIdRef.current = activeProfileId;
      setSettings(persisted);
    }
  }, [activeProfileId, persisted]);
  useQualityTestSettingsPersistence('focus', settings);

  const materialHeight = project?.material_height_mm ?? null;

  const buildRequest = (): QualityTestRequest =>
    ({ kind: 'focus', ...settings } as QualityTestRequest);

  const update = <K extends keyof FocusTestSettings>(key: K, value: FocusTestSettings[K]) =>
    setSettings((s) => ({ ...s, [key]: value }));

  const setMaterialHeight = (v: number | null) => {
    if (!project) return;
    void setMaterialHeightAction(v);
  };

  const liveActionsGateReason = (): string | null => {
    const outputGate = outputActionsGateReason();
    if (outputGate !== null) return outputGate;
    if (!activeProfile?.supports_z_moves) {
      return t('dialog.focus_test.gate_no_z_support');
    }
    return null;
  };

  const outputActionsGateReason = (): string | null => {
    if (settings.mode === 'AbsoluteWorkCoord' && (materialHeight === null || Number.isNaN(materialHeight))) {
      return project
        ? t('dialog.focus_test.gate_needs_height')
        : t('dialog.focus_test.gate_needs_open_project');
    }
    return null;
  };

  return (
    <QualityTestShell
      title={t('dialog.focus_test.title')}
      toolKind="focus"
      buildRequest={buildRequest}
      outputActionsGateReason={outputActionsGateReason}
      liveActionsGateReason={liveActionsGateReason}
      onClose={onClose}
    >
      <div className="grid grid-cols-2 gap-2">
        <NumberInput label={labelWithUnit(t('dialog.focus_test.z_min'), lengthUnitLabel(displayUnit))} value={roundDisplayLength(mmToDisplay(settings.z_min_mm, displayUnit), displayUnit)} onChange={(v) => update('z_min_mm', displayToMm(v, displayUnit))} step={lengthStep(displayUnit, 0.1, 0.005)} inputWidthClassName={FOCUS_CONTROL_WIDTH} />
        <NumberInput label={labelWithUnit(t('dialog.focus_test.z_max'), lengthUnitLabel(displayUnit))} value={roundDisplayLength(mmToDisplay(settings.z_max_mm, displayUnit), displayUnit)} onChange={(v) => update('z_max_mm', displayToMm(v, displayUnit))} step={lengthStep(displayUnit, 0.1, 0.005)} inputWidthClassName={FOCUS_CONTROL_WIDTH} />
        <NumberInput label={labelWithUnit(t('dialog.focus_test.speed'), speedUnitLabel(displayUnit, speedTimeUnit))} value={speedInputValue(settings.speed_mm_min, displayUnit, speedTimeUnit)} onChange={(v) => update('speed_mm_min', Math.max(1, displaySpeedToMmMin(v, displayUnit, speedTimeUnit)))} min={speedInputValue(1, displayUnit, speedTimeUnit)} step={speedStepForUnit(displayUnit, speedTimeUnit)} inputWidthClassName={FOCUS_CONTROL_WIDTH} />
        <NumberInput label={t('dialog.focus_test.power')} value={settings.power_percent} onChange={(v) => update('power_percent', Math.max(0, Math.min(100, v)))} min={0} max={100} step={1} inputWidthClassName={FOCUS_CONTROL_WIDTH} />
        <NumberInput label={t('dialog.focus_test.samples')} value={settings.intervals} onChange={(v) => update('intervals', Math.max(1, Math.round(v)))} min={1} max={50} step={1} inputWidthClassName={FOCUS_CONTROL_WIDTH} />
        <Select
          label={t('dialog.focus_test.z_reference')}
          value={settings.mode}
          options={zModeOptions}
          onChange={(v) => update('mode', v as FocusTestZMode)}
          selectClassName={FOCUS_CONTROL_WIDTH}
        />
        <NumberInput label={labelWithUnit(t('dialog.focus_test.line_length'), lengthUnitLabel(displayUnit))} value={roundDisplayLength(mmToDisplay(settings.line_length_mm, displayUnit), displayUnit)} onChange={(v) => update('line_length_mm', displayToMm(v, displayUnit))} min={mmToDisplay(1, displayUnit)} step={lengthStep(displayUnit, 1, 0.05)} inputWidthClassName={FOCUS_CONTROL_WIDTH} />
        <NumberInput label={labelWithUnit(t('dialog.focus_test.step_spacing'), lengthUnitLabel(displayUnit))} value={roundDisplayLength(mmToDisplay(settings.step_spacing_mm, displayUnit), displayUnit)} onChange={(v) => update('step_spacing_mm', displayToMm(v, displayUnit))} min={mmToDisplay(1, displayUnit)} step={lengthStep(displayUnit, 0.5, 0.05)} inputWidthClassName={FOCUS_CONTROL_WIDTH} />
      </div>
      <Toggle label={t('dialog.focus_test.perforated_labels')} checked={settings.perforated_labels} onChange={(v) => update('perforated_labels', v)} />
      {settings.mode === 'AbsoluteWorkCoord' && (
        <div className="border-t border-bb-border pt-2 mt-1">
          <NumberInput
            label={labelWithUnit(t('dialog.focus_test.material_height'), lengthUnitLabel(displayUnit))}
            value={roundDisplayLength(mmToDisplay(materialHeight ?? 0, displayUnit), displayUnit)}
            onChange={(v) => setMaterialHeight(displayToMm(v, displayUnit))}
            step={lengthStep(displayUnit, 0.1, 0.005)}
            inputWidthClassName={FOCUS_CONTROL_WIDTH}
          />
          <div className="text-[11px] text-bb-text-muted mt-1">
            {t('dialog.focus_test.material_height_help')}
          </div>
        </div>
      )}
    </QualityTestShell>
  );
}
