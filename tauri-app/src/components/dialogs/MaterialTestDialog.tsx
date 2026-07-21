import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { useMachineStore } from '../../stores/machineStore';
import { useAppStore } from '../../stores/appStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { NumberInput } from '../shared/NumberInput';
import { NumberStepper } from '../shared/NumberStepper';
import { Toggle } from '../shared/Toggle';
import {
  MaterialPresetPicker,
  QualityTestShell,
  useQualityTestSettingsPersistence,
} from './QualityTestShell';
import { qualityTestService } from '../../services/qualityTestService';
import {
  DEFAULT_MATERIAL_TEST_SETTINGS,
  DEFAULT_QUALITY_TEST_SETTINGS,
  type MaterialTestAxis,
  type MaterialTestAxisParam,
  type MaterialTestRecipe,
  type MaterialTestSettings,
  type QualityTestRequest,
} from '../../types/machine';
import type { CutEntry, OperationType } from '../../types/project';
import { defaultRasterSettings, defaultVectorSettings } from '../../types/cutEntryDefaults';
import {
  displaySpeedToMmMin,
  speedInputValue,
  speedMmMinToDisplay,
  speedStepForUnit,
  speedUnitLabel,
} from '../../utils/speedUnits';
import {
  mmToDisplay,
  displayToMm,
  roundDisplayLength,
  lengthStep,
  lengthUnitLabel,
  labelWithUnit,
} from '../../utils/lengthUnits';

interface MaterialTestDialogProps {
  onClose: () => void;
}

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function usesRaster(operation: OperationType): boolean {
  return operation === 'fill' || operation === 'image' || operation === 'offset_fill';
}

function usesVector(operation: OperationType): boolean {
  return (
    operation === 'line' ||
    operation === 'cut' ||
    operation === 'score' ||
    operation === 'offset_fill'
  );
}

function normalizeEntryForOperation(entry: CutEntry, operation: OperationType): CutEntry {
  return {
    ...entry,
    operation,
    raster_settings: usesRaster(operation)
      ? (entry.raster_settings ?? defaultRasterSettings())
      : null,
    vector_settings: usesVector(operation)
      ? (entry.vector_settings ?? defaultVectorSettings())
      : null,
  };
}

function normalizeMaterialSettings(
  raw: Partial<MaterialTestSettings> | undefined,
): MaterialTestSettings {
  const defaults = DEFAULT_MATERIAL_TEST_SETTINGS;
  const next = {
    ...clone(defaults),
    ...(raw ?? {}),
    x_axis: { ...defaults.x_axis, ...(raw?.x_axis ?? {}) },
    y_axis: { ...defaults.y_axis, ...(raw?.y_axis ?? {}) },
    sample_entry: { ...clone(defaults.sample_entry), ...(raw?.sample_entry ?? {}) },
    text_entry: { ...clone(defaults.text_entry), ...(raw?.text_entry ?? {}) },
    border_entry: { ...clone(defaults.border_entry), ...(raw?.border_entry ?? {}) },
  };
  next.sample_entry = normalizeEntryForOperation(next.sample_entry, next.sample_entry.operation);
  next.text_entry = normalizeEntryForOperation(next.text_entry, 'line');
  next.border_entry = normalizeEntryForOperation(next.border_entry, 'line');
  return next;
}

function axisDisplayValue(
  axis: MaterialTestAxis,
  field: 'min' | 'max',
  displayUnit: 'mm' | 'inches',
  speedTimeUnit: 'minutes' | 'seconds',
): number {
  if (axis.param === 'speed') {
    return speedInputValue(axis[field], displayUnit, speedTimeUnit);
  }
  if (axis.param === 'interval') {
    return roundDisplayLength(mmToDisplay(axis[field], displayUnit), displayUnit);
  }
  return axis[field];
}

function axisValueFromDisplay(
  axis: MaterialTestAxis,
  value: number,
  displayUnit: 'mm' | 'inches',
  speedTimeUnit: 'minutes' | 'seconds',
): number {
  if (axis.param === 'speed') {
    return displaySpeedToMmMin(value, displayUnit, speedTimeUnit);
  }
  if (axis.param === 'passes') {
    return Math.max(1, Math.round(value));
  }
  if (axis.param === 'interval') {
    return Math.max(0.01, displayToMm(value, displayUnit));
  }
  return Math.max(0, Math.min(100, value));
}

function axisValueLabel(
  label: string,
  axis: MaterialTestAxis,
  displayUnit: 'mm' | 'inches',
  speedLabel: string,
): string {
  if (axis.param === 'speed') {
    return labelWithUnit(label, speedLabel);
  }
  if (axis.param === 'interval') {
    return labelWithUnit(label, lengthUnitLabel(displayUnit));
  }
  return label;
}

function AxisNumberField({
  label,
  value,
  onChange,
  min,
  max,
  step,
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
}) {
  return (
    <label className="min-w-0 space-y-1 text-xs">
      <span className="block truncate text-bb-text-muted" title={label}>
        {label}
      </span>
      <NumberStepper
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        min={min}
        max={max}
        step={step}
        containerClassName="w-full"
        className="w-full min-w-0 rounded border border-bb-border bg-bb-input px-1.5 py-0.5 text-right text-xs text-bb-text focus:outline-none focus:border-bb-accent"
      />
    </label>
  );
}

export function MaterialTestDialog({ onClose }: MaterialTestDialogProps) {
  const { t } = useTranslation();
  const profiles = useMachineStore((s) => s.profiles);
  const activeProfileId = useMachineStore((s) => s.activeProfileId);
  const saveProfile = useMachineStore((s) => s.saveProfile);
  const push = useNotificationStore((s) => s.push);
  const appSettings = useAppStore((s) => s.settings);
  const displayUnit = appSettings?.display_unit === 'inches' ? 'inches' : 'mm';
  const speedTimeUnit = appSettings?.speed_time_unit === 'seconds' ? 'seconds' : 'minutes';
  const speedLabel = speedUnitLabel(displayUnit, speedTimeUnit);
  const speedStep = speedStepForUnit(displayUnit, speedTimeUnit);
  const minDisplaySpeed = speedMmMinToDisplay(1, displayUnit, speedTimeUnit);
  const axisOptions: Array<{ value: MaterialTestAxisParam; label: string }> = [
    { value: 'speed', label: t('dialog.material_test.axis_speed') },
    { value: 'power', label: t('dialog.material_test.axis_power') },
    { value: 'interval', label: t('dialog.material_test.axis_interval') },
    { value: 'passes', label: t('dialog.material_test.axis_passes') },
  ];
  const operationOptions: Array<{ value: OperationType; label: string }> = [
    { value: 'fill', label: t('dialog.material_test.operation_fill') },
    { value: 'line', label: t('dialog.material_test.operation_line') },
    { value: 'cut', label: t('dialog.material_test.operation_cut') },
    { value: 'score', label: t('dialog.material_test.operation_score') },
    { value: 'offset_fill', label: t('dialog.material_test.operation_offset_fill') },
  ];

  const activeProfile = useMemo(
    () => profiles.find((p) => p.id === activeProfileId) ?? null,
    [profiles, activeProfileId],
  );
  const persisted = useMemo(
    () => normalizeMaterialSettings(activeProfile?.quality_test_settings?.material),
    [activeProfile],
  );
  const recipes = activeProfile?.quality_test_settings?.material_recipes ?? [];

  const [settings, setSettings] = useState<MaterialTestSettings>(persisted);
  const [selectedRecipeId, setSelectedRecipeId] = useState(
    activeProfile?.quality_test_settings?.active_material_recipe_id ?? '',
  );
  const [recipeName, setRecipeName] = useState('');
  const [axisMessage, setAxisMessage] = useState<string | null>(null);

  const lastProfileIdRef = useRef(activeProfileId);
  useEffect(() => {
    if (lastProfileIdRef.current !== activeProfileId) {
      lastProfileIdRef.current = activeProfileId;
      setSettings(persisted);
      setSelectedRecipeId(activeProfile?.quality_test_settings?.active_material_recipe_id ?? '');
      setRecipeName('');
      setAxisMessage(null);
    }
  }, [activeProfileId, activeProfile, persisted]);
  useQualityTestSettingsPersistence('material', settings);

  const buildRequest = (): QualityTestRequest =>
    ({ kind: 'material', ...settings }) as QualityTestRequest;

  const update = <K extends keyof MaterialTestSettings>(key: K, value: MaterialTestSettings[K]) =>
    setSettings((s) => ({ ...s, [key]: value }));

  const updateAxis = (axisKey: 'x_axis' | 'y_axis', patch: Partial<MaterialTestAxis>) => {
    setSettings((s) => ({
      ...s,
      [axisKey]: {
        ...s[axisKey],
        ...patch,
        count:
          patch.count === undefined
            ? s[axisKey].count
            : Math.max(1, Math.min(10, Math.round(patch.count))),
      },
    }));
  };

  const updateSampleEntry = (patch: Partial<CutEntry>) => {
    setSettings((s) => ({ ...s, sample_entry: { ...s.sample_entry, ...patch } }));
  };

  const updateTextEntry = (patch: Partial<CutEntry>) => {
    setSettings((s) => ({ ...s, text_entry: { ...s.text_entry, ...patch } }));
  };

  const updateBorderEntry = (patch: Partial<CutEntry>) => {
    setSettings((s) => ({ ...s, border_entry: { ...s.border_entry, ...patch } }));
  };

  const setOperation = (operation: OperationType) => {
    setSettings((s) => {
      const normalizedEntry = normalizeEntryForOperation(s.sample_entry, operation);
      const intervalInvalid = !usesRaster(operation);
      const xInvalid = intervalInvalid && s.x_axis.param === 'interval';
      const yInvalid = intervalInvalid && s.y_axis.param === 'interval';
      if (xInvalid || yInvalid) {
        setAxisMessage(t('dialog.material_test.interval_axis_disabled'));
      }
      return {
        ...s,
        sample_entry: normalizedEntry,
        x_axis: xInvalid ? { ...s.x_axis, param: 'speed' } : s.x_axis,
        y_axis: yInvalid ? { ...s.y_axis, param: 'speed' } : s.y_axis,
      };
    });
  };

  const persistQualityTestSettings = async (
    material_recipes: MaterialTestRecipe[],
    active_material_recipe_id: string | null,
  ) => {
    if (!activeProfile) return;
    const current = activeProfile.quality_test_settings ?? DEFAULT_QUALITY_TEST_SETTINGS;
    await saveProfile({
      ...activeProfile,
      quality_test_settings: {
        ...current,
        material: settings,
        material_recipes,
        active_material_recipe_id,
      },
    });
  };

  const saveRecipe = async () => {
    const id = selectedRecipeId || crypto.randomUUID();
    const name =
      recipeName.trim() || recipes.find((r) => r.id === id)?.name || t('dialog.material_test.recipe_default', { index: recipes.length + 1 });
    const recipe: MaterialTestRecipe = { id, name, settings: clone(settings) };
    const next = [...recipes.filter((r) => r.id !== id), recipe];
    await persistQualityTestSettings(next, id);
    setSelectedRecipeId(id);
    setRecipeName(name);
    push(t('dialog.material_test.recipe_saved'), 'success');
  };

  const loadRecipe = () => {
    const recipe = recipes.find((r) => r.id === selectedRecipeId);
    if (!recipe) return;
    setSettings(normalizeMaterialSettings(recipe.settings));
    setRecipeName(recipe.name);
    push(t('dialog.material_test.recipe_loaded', { name: recipe.name }), 'info');
  };

  const deleteRecipe = async () => {
    if (!selectedRecipeId) return;
    const next = recipes.filter((r) => r.id !== selectedRecipeId);
    await persistQualityTestSettings(next, null);
    setSelectedRecipeId('');
    setRecipeName('');
    push(t('dialog.material_test.recipe_deleted'), 'success');
  };

  const exportRecipes = async () => {
    const path = await qualityTestService.exportRecipes(recipes);
    if (path) push(t('dialog.material_test.recipes_exported', { path }), 'success');
  };

  const importRecipes = async () => {
    const imported = await qualityTestService.importRecipes();
    if (!imported || imported.length === 0) return;
    const byId = new Map(recipes.map((r) => [r.id, r]));
    for (const recipe of imported) byId.set(recipe.id, recipe);
    const next = [...byId.values()];
    await persistQualityTestSettings(next, imported[0].id);
    setSelectedRecipeId(imported[0].id);
    setSettings(normalizeMaterialSettings(imported[0].settings));
    setRecipeName(imported[0].name);
    push(t('dialog.material_test.recipes_imported'), 'success');
  };

  const onPresetApplied = (entry: CutEntry) => {
    setSettings((s) => ({
      ...s,
      sample_entry: normalizeEntryForOperation(
        {
          ...s.sample_entry,
          speed_mm_min: entry.speed_mm_min,
          power_percent: entry.power_percent,
          raster_settings: entry.raster_settings ?? s.sample_entry.raster_settings,
        },
        s.sample_entry.operation,
      ),
      y_axis:
        s.y_axis.param === 'speed'
          ? {
              ...s.y_axis,
              min: Math.max(1, Math.round(entry.speed_mm_min * 0.5)),
              max: Math.max(1, Math.round(entry.speed_mm_min * 1.5)),
            }
          : s.y_axis,
      x_axis:
        s.x_axis.param === 'power'
          ? {
              ...s.x_axis,
              min: Math.max(0, Math.round(entry.power_percent - 30)),
              max: Math.min(100, Math.round(entry.power_percent + 20)),
            }
          : s.x_axis,
    }));
  };

  const renderAxisEditor = (axisKey: 'x_axis' | 'y_axis', label: string) => {
    const axis = settings[axisKey];
    const intervalDisabled = !usesRaster(settings.sample_entry.operation);
    const min =
      axis.param === 'speed'
        ? minDisplaySpeed
        : axis.param === 'interval'
          ? mmToDisplay(0.01, displayUnit)
          : axis.param === 'passes'
            ? 1
            : 0;
    const max = axis.param === 'power' ? 100 : undefined;
    const step =
      axis.param === 'speed'
        ? speedStep
        : axis.param === 'interval'
          ? lengthStep(displayUnit, 0.01, 0.001)
          : axis.param === 'passes'
            ? 1
            : 5;
    return (
      <div className="rounded border border-bb-border p-2 space-y-2">
        <div className="text-xs font-medium text-bb-text">{label}</div>
        <label className="flex items-center gap-2 text-xs">
          <span className="w-16 text-bb-text-muted">{t('dialog.material_test.param')}</span>
          <select
            data-testid={`qt-material-${axisKey}-param`}
            value={axis.param}
            onChange={(e) => {
              setAxisMessage(null);
              updateAxis(axisKey, { param: e.target.value as MaterialTestAxisParam });
            }}
            className="flex-1 rounded border border-bb-border bg-bb-bg px-1 py-0.5 text-bb-text"
          >
            {axisOptions.map((option) => (
              <option
                key={option.value}
                value={option.value}
                disabled={option.value === 'interval' && intervalDisabled}
              >
                {option.label}
              </option>
            ))}
          </select>
        </label>
        <div className="grid grid-cols-3 gap-2">
          <AxisNumberField
            label={t('dialog.material_test.count')}
            value={axis.count}
            onChange={(v) => updateAxis(axisKey, { count: v })}
            min={1}
            max={10}
            step={1}
          />
          <AxisNumberField
            label={axisValueLabel(t('dialog.material_test.min'), axis, displayUnit, speedLabel)}
            value={axisDisplayValue(axis, 'min', displayUnit, speedTimeUnit)}
            onChange={(v) =>
              updateAxis(axisKey, {
                min: axisValueFromDisplay(axis, v, displayUnit, speedTimeUnit),
              })
            }
            min={min}
            max={max}
            step={step}
          />
          <AxisNumberField
            label={axisValueLabel(t('dialog.material_test.max'), axis, displayUnit, speedLabel)}
            value={axisDisplayValue(axis, 'max', displayUnit, speedTimeUnit)}
            onChange={(v) =>
              updateAxis(axisKey, {
                max: axisValueFromDisplay(axis, v, displayUnit, speedTimeUnit),
              })
            }
            min={min}
            max={max}
            step={step}
          />
        </div>
      </div>
    );
  };

  const raster = settings.sample_entry.raster_settings;
  const samplePasses =
    settings.sample_entry.raster_settings?.passes ??
    settings.sample_entry.vector_settings?.passes ??
    1;

  return (
    <QualityTestShell
      title={t('dialog.material_test.title')}
      toolKind="material"
      buildRequest={buildRequest}
      onClose={onClose}
    >
      <MaterialPresetPicker seedOperation="fill" onApplied={onPresetApplied} />

      <div className="rounded border border-bb-border p-2 space-y-2">
        <div className="flex items-center gap-2">
          <select
            data-testid="qt-material-recipe-select"
            value={selectedRecipeId}
            onChange={(e) => {
              setSelectedRecipeId(e.target.value);
              setRecipeName(recipes.find((r) => r.id === e.target.value)?.name ?? '');
            }}
            className="min-w-0 flex-1 rounded border border-bb-border bg-bb-bg px-1 py-0.5 text-xs text-bb-text"
          >
            <option value="">{t('dialog.material_test.custom_recipe')}</option>
            {recipes.map((recipe) => (
              <option key={recipe.id} value={recipe.id}>
                {recipe.name}
              </option>
            ))}
          </select>
          <button
            data-testid="qt-material-recipe-load"
            className="rounded bg-bb-bg px-2 py-1 text-xs text-bb-text disabled:opacity-50"
            disabled={!selectedRecipeId}
            onClick={loadRecipe}
          >
            {t('dialog.material_test.load')}
          </button>
          <button
            data-testid="qt-material-recipe-delete"
            className="rounded bg-bb-bg px-2 py-1 text-xs text-bb-text disabled:opacity-50"
            disabled={!selectedRecipeId}
            onClick={() => void deleteRecipe()}
          >
            {t('dialog.material_test.delete')}
          </button>
        </div>
        <div className="flex items-center gap-2">
          <input
            data-testid="qt-material-recipe-name"
            value={recipeName}
            onChange={(e) => setRecipeName(e.target.value)}
            placeholder={t('dialog.material_test.recipe_name')}
            className="min-w-0 flex-1 rounded border border-bb-border bg-bb-input px-2 py-1 text-xs text-bb-text"
          />
          <button
            data-testid="qt-material-recipe-save"
            className="rounded bg-bb-bg px-2 py-1 text-xs text-bb-text"
            onClick={() => void saveRecipe()}
          >
            {t('dialog.material_test.save_recipe')}
          </button>
          <button
            data-testid="qt-material-recipe-import"
            className="rounded bg-bb-bg px-2 py-1 text-xs text-bb-text"
            onClick={() => void importRecipes()}
          >
            {t('dialog.material_test.import')}
          </button>
          <button
            data-testid="qt-material-recipe-export"
            className="rounded bg-bb-bg px-2 py-1 text-xs text-bb-text"
            onClick={() => void exportRecipes()}
          >
            {t('dialog.material_test.export')}
          </button>
        </div>
      </div>

      {axisMessage && (
        <div
          data-testid="qt-material-axis-message"
          className="text-[11px] text-bb-warning-fg bg-bb-warning-bg border border-bb-warning-border rounded px-2 py-1"
        >
          {axisMessage}
        </div>
      )}

      <div className="grid grid-cols-2 gap-2">
        {renderAxisEditor('x_axis', t('dialog.material_test.x_axis'))}
        {renderAxisEditor('y_axis', t('dialog.material_test.y_axis'))}
      </div>

      <div className="grid grid-cols-2 gap-2">
        <label className="flex items-center gap-2 text-xs">
          <span className="text-bb-text-muted shrink-0">{t('dialog.material_test.operation')}</span>
          <select
            data-testid="qt-material-operation"
            value={settings.sample_entry.operation}
            onChange={(e) => setOperation(e.target.value as OperationType)}
            className="flex-1 rounded border border-bb-border bg-bb-bg px-1 py-0.5 text-bb-text"
          >
            {operationOptions.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </label>
        <NumberInput
          label={t('dialog.material_test.base_speed', { unit: speedLabel })}
          value={speedInputValue(settings.sample_entry.speed_mm_min, displayUnit, speedTimeUnit)}
          onChange={(v) =>
            updateSampleEntry({ speed_mm_min: displaySpeedToMmMin(v, displayUnit, speedTimeUnit) })
          }
          min={minDisplaySpeed}
          step={speedStep}
        />
        <NumberInput
          label={t('dialog.material_test.base_power')}
          value={settings.sample_entry.power_percent}
          onChange={(v) => updateSampleEntry({ power_percent: v })}
          min={0}
          max={100}
          step={5}
        />
        <NumberInput
          label={t('dialog.material_test.base_passes')}
          value={samplePasses}
          onChange={(v) => {
            const passes = Math.max(1, Math.round(v));
            updateSampleEntry({
              raster_settings: settings.sample_entry.raster_settings
                ? { ...settings.sample_entry.raster_settings, passes }
                : null,
              vector_settings: settings.sample_entry.vector_settings
                ? { ...settings.sample_entry.vector_settings, passes }
                : null,
            });
          }}
          min={1}
          step={1}
        />
        {raster && (
          <NumberInput
            label={labelWithUnit(t('dialog.material_test.base_interval'), lengthUnitLabel(displayUnit))}
            value={roundDisplayLength(mmToDisplay(raster.line_interval_mm, displayUnit), displayUnit)}
            onChange={(v) => {
              const mm = Math.max(0.01, displayToMm(v, displayUnit));
              updateSampleEntry({
                raster_settings: {
                  ...raster,
                  line_interval_mm: mm,
                  dpi: Math.max(1, Math.round(25.4 / mm)),
                },
              });
            }}
            min={mmToDisplay(0.01, displayUnit)}
            step={lengthStep(displayUnit, 0.01, 0.001)}
          />
        )}
        <NumberInput
          label={labelWithUnit(t('dialog.material_test.cell_width'), lengthUnitLabel(displayUnit))}
          value={roundDisplayLength(mmToDisplay(settings.cell_w_mm, displayUnit), displayUnit)}
          onChange={(v) => update('cell_w_mm', displayToMm(v, displayUnit))}
          min={mmToDisplay(1, displayUnit)}
          step={lengthStep(displayUnit, 1, 0.05)}
        />
        <NumberInput
          label={labelWithUnit(t('dialog.material_test.cell_height'), lengthUnitLabel(displayUnit))}
          value={roundDisplayLength(mmToDisplay(settings.cell_h_mm, displayUnit), displayUnit)}
          onChange={(v) => update('cell_h_mm', displayToMm(v, displayUnit))}
          min={mmToDisplay(1, displayUnit)}
          step={lengthStep(displayUnit, 1, 0.05)}
        />
        <NumberInput
          label={labelWithUnit(t('dialog.material_test.cell_spacing'), lengthUnitLabel(displayUnit))}
          value={roundDisplayLength(mmToDisplay(settings.cell_spacing_mm, displayUnit), displayUnit)}
          onChange={(v) => update('cell_spacing_mm', displayToMm(v, displayUnit))}
          min={mmToDisplay(0, displayUnit)}
          step={lengthStep(displayUnit, 1, 0.05)}
        />
      </div>

      <div className="grid grid-cols-2 gap-2 border-t border-bb-border pt-2">
        <Toggle
          label={t('dialog.material_test.text_labels')}
          checked={settings.enable_text}
          onChange={(v) => update('enable_text', v)}
        />
        <Toggle
          label={t('dialog.material_test.border')}
          checked={settings.enable_border}
          onChange={(v) => update('enable_border', v)}
        />
        {settings.enable_text && (
          <>
            <NumberInput
              label={labelWithUnit(t('dialog.material_test.text_speed'), speedUnitLabel(displayUnit, speedTimeUnit))}
              value={speedInputValue(settings.text_entry.speed_mm_min, displayUnit, speedTimeUnit)}
              onChange={(v) => updateTextEntry({ speed_mm_min: displaySpeedToMmMin(v, displayUnit, speedTimeUnit) })}
              min={minDisplaySpeed}
              step={speedStep}
            />
            <NumberInput
              label={t('dialog.material_test.text_power')}
              value={settings.text_entry.power_percent}
              onChange={(v) => updateTextEntry({ power_percent: v })}
              min={0}
              max={100}
              step={5}
            />
          </>
        )}
        {settings.enable_border && (
          <>
            <NumberInput
              label={labelWithUnit(t('dialog.material_test.border_speed'), speedUnitLabel(displayUnit, speedTimeUnit))}
              value={speedInputValue(settings.border_entry.speed_mm_min, displayUnit, speedTimeUnit)}
              onChange={(v) => updateBorderEntry({ speed_mm_min: displaySpeedToMmMin(v, displayUnit, speedTimeUnit) })}
              min={minDisplaySpeed}
              step={speedStep}
            />
            <NumberInput
              label={t('dialog.material_test.border_power')}
              value={settings.border_entry.power_percent}
              onChange={(v) => updateBorderEntry({ power_percent: v })}
              min={0}
              max={100}
              step={5}
            />
          </>
        )}
      </div>

      <div className="border-t border-bb-border pt-2 space-y-2">
        <Toggle
          label={t('dialog.material_test.use_absolute_center')}
          checked={settings.absolute_center_enabled}
          onChange={(v) => update('absolute_center_enabled', v)}
        />
        {settings.absolute_center_enabled && (
          <div className="grid grid-cols-2 gap-2">
            <NumberInput
              label={labelWithUnit(t('dialog.material_test.center_x'), lengthUnitLabel(displayUnit))}
              value={roundDisplayLength(mmToDisplay(settings.x_center_mm, displayUnit), displayUnit)}
              onChange={(v) => update('x_center_mm', displayToMm(v, displayUnit))}
              step={lengthStep(displayUnit, 1, 0.05)}
            />
            <NumberInput
              label={labelWithUnit(t('dialog.material_test.center_y'), lengthUnitLabel(displayUnit))}
              value={roundDisplayLength(mmToDisplay(settings.y_center_mm, displayUnit), displayUnit)}
              onChange={(v) => update('y_center_mm', displayToMm(v, displayUnit))}
              step={lengthStep(displayUnit, 1, 0.05)}
            />
          </div>
        )}
      </div>
    </QualityTestShell>
  );
}
