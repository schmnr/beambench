import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { useMachineStore } from '../../stores/machineStore';
import { useAppStore } from '../../stores/appStore';
import { NumberInput } from '../shared/NumberInput';
import { NumberStepper } from '../shared/NumberStepper';
import {
  MaterialPresetPicker,
  QualityTestShell,
  useQualityTestSettingsPersistence,
} from './QualityTestShell';
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
import type { DisplayUnit } from '../../utils/lengthUnits';
import {
  DEFAULT_INTERVAL_TEST_SETTINGS,
  type IntervalTestSettings,
  type QualityTestRequest,
} from '../../types/machine';

interface IntervalTestDialogProps {
  onClose: () => void;
}

function intervalToDpi(intervalMm: number): number | null {
  return intervalMm > 0 ? Math.round(25.4 / intervalMm) : null;
}

function RasterIntervalInput({
  label,
  value,
  dpiUnit,
  displayUnit,
  onChange,
}: {
  label: string;
  value: number;
  dpiUnit: string;
  displayUnit: DisplayUnit;
  onChange: (value: number) => void;
}) {
  const dpi = intervalToDpi(value);
  return (
    <label className="grid grid-cols-[1fr_auto] items-start gap-x-2 text-xs">
      <span className="pt-1 text-bb-text-muted">{labelWithUnit(label, lengthUnitLabel(displayUnit))}</span>
      <span className="flex flex-col items-end gap-1">
        <NumberStepper
          value={roundDisplayLength(mmToDisplay(value, displayUnit), displayUnit)}
          onChange={(e) => onChange(displayToMm(Number(e.target.value), displayUnit))}
          min={mmToDisplay(0.01, displayUnit)}
          step={lengthStep(displayUnit, 0.01, 0.001)}
          className="w-24 px-1.5 py-0.5 bg-bb-input border border-bb-border rounded text-xs text-bb-text text-right focus:outline-none focus:border-bb-accent"
        />
        <span className="text-[10px] leading-none text-bb-text-dim">
          {dpi === null ? '--' : dpi} {dpiUnit}
        </span>
      </span>
    </label>
  );
}

function normalizeIntervalSettings(raw: Partial<IntervalTestSettings> | undefined): IntervalTestSettings {
  return {
    ...DEFAULT_INTERVAL_TEST_SETTINGS,
    ...(raw ?? {}),
  };
}

export function IntervalTestDialog({ onClose }: IntervalTestDialogProps) {
  const { t } = useTranslation();
  const profiles = useMachineStore((s) => s.profiles);
  const activeProfileId = useMachineStore((s) => s.activeProfileId);
  const displayUnit = useAppStore((s) => s.settings?.display_unit) ?? 'mm';
  const speedTimeUnit = useAppStore((s) => s.settings?.speed_time_unit) ?? 'minutes';
  const persisted = useMemo(() => {
    const p = profiles.find((p) => p.id === activeProfileId);
    return normalizeIntervalSettings(p?.quality_test_settings?.interval);
  }, [profiles, activeProfileId]);

  const [settings, setSettings] = useState<IntervalTestSettings>(persisted);
  // Re-seed when active profile changes — see MaterialTestDialog for rationale.
  const lastProfileIdRef = useRef(activeProfileId);
  useEffect(() => {
    if (lastProfileIdRef.current !== activeProfileId) {
      lastProfileIdRef.current = activeProfileId;
      setSettings(persisted);
    }
  }, [activeProfileId, persisted]);
  useQualityTestSettingsPersistence('interval', settings);

  const buildRequest = (): QualityTestRequest =>
    ({ kind: 'interval', ...settings } as QualityTestRequest);

  const update = <K extends keyof IntervalTestSettings>(key: K, value: IntervalTestSettings[K]) =>
    setSettings((s) => ({ ...s, [key]: value }));

  /** Center the interval sweep around the preset's line interval. */
  const onPresetApplied = (entry: {
    speed_mm_min: number;
    power_percent: number;
    raster_settings: { line_interval_mm: number } | null;
  }) => {
    const interval = entry.raster_settings?.line_interval_mm;
    setSettings((s) => ({
      ...s,
      speed_mm_min: Math.max(1, entry.speed_mm_min),
      power_percent: Math.max(0, Math.min(100, entry.power_percent)),
      ...(interval == null || !(interval > 0)
        ? {}
        : {
            interval_min_mm: Math.max(0.01, +(interval * 0.5).toFixed(3)),
            interval_max_mm: +(interval * 1.5).toFixed(3),
          }),
    }));
  };

  const dpiUnit = t('dialog.interval_test.dpi_unit');

  return (
    <QualityTestShell
      title={t('dialog.interval_test.title')}
      toolKind="interval"
      buildRequest={buildRequest}
      onClose={onClose}
    >
      <MaterialPresetPicker seedOperation="fill" onApplied={onPresetApplied} />
      <div className="grid grid-cols-2 gap-2">
        <RasterIntervalInput label={t('dialog.interval_test.min_interval')} dpiUnit={dpiUnit} displayUnit={displayUnit} value={settings.interval_min_mm} onChange={(v) => update('interval_min_mm', v)} />
        <RasterIntervalInput label={t('dialog.interval_test.max_interval')} dpiUnit={dpiUnit} displayUnit={displayUnit} value={settings.interval_max_mm} onChange={(v) => update('interval_max_mm', v)} />
        <NumberInput label={labelWithUnit(t('dialog.interval_test.speed'), speedUnitLabel(displayUnit, speedTimeUnit))} value={speedInputValue(settings.speed_mm_min, displayUnit, speedTimeUnit)} onChange={(v) => update('speed_mm_min', Math.max(1, displaySpeedToMmMin(v, displayUnit, speedTimeUnit)))} min={speedInputValue(1, displayUnit, speedTimeUnit)} step={speedStepForUnit(displayUnit, speedTimeUnit)} />
        <NumberInput label={t('dialog.interval_test.power')} value={settings.power_percent} onChange={(v) => update('power_percent', Math.max(0, Math.min(100, v)))} min={0} max={100} step={1} />
        <NumberInput label={t('dialog.interval_test.samples')} value={settings.steps} onChange={(v) => update('steps', Math.max(2, Math.round(v)))} min={2} max={20} step={1} />
        <NumberInput label={labelWithUnit(t('dialog.interval_test.cell_w'), lengthUnitLabel(displayUnit))} value={roundDisplayLength(mmToDisplay(settings.cell_w_mm, displayUnit), displayUnit)} onChange={(v) => update('cell_w_mm', displayToMm(v, displayUnit))} min={mmToDisplay(1, displayUnit)} step={lengthStep(displayUnit, 1, 0.05)} />
        <NumberInput label={labelWithUnit(t('dialog.interval_test.cell_h'), lengthUnitLabel(displayUnit))} value={roundDisplayLength(mmToDisplay(settings.cell_h_mm, displayUnit), displayUnit)} onChange={(v) => update('cell_h_mm', displayToMm(v, displayUnit))} min={mmToDisplay(1, displayUnit)} step={lengthStep(displayUnit, 1, 0.05)} />
        <NumberInput label={labelWithUnit(t('dialog.interval_test.cell_spacing'), lengthUnitLabel(displayUnit))} value={roundDisplayLength(mmToDisplay(settings.cell_spacing_mm, displayUnit), displayUnit)} onChange={(v) => update('cell_spacing_mm', displayToMm(v, displayUnit))} min={mmToDisplay(0, displayUnit)} step={lengthStep(displayUnit, 1, 0.05)} />
      </div>
    </QualityTestShell>
  );
}
