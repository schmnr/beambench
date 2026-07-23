import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { RotateCcw } from 'lucide-react';
import { useProjectStore } from '../../stores/projectStore';
import { useMachineStore } from '../../stores/machineStore';
import { useAppStore } from '../../stores/appStore';
import type {
  CutEntry,
  OffsetFillGroupingMode,
  OperationType,
  ProjectObject,
  RasterMode,
} from '../../types/project';
import type { MachineProfile } from '../../types/machine';
import { NumberInput } from '../shared/NumberInput';
import { Toggle } from '../shared/Toggle';
import { ToggleSwitch } from '../shared/ToggleSwitch';
import { effectiveLineIntervalMm } from '../../types/rasterSettings';
// M4: defaults moved to a dedicated module with a SYNC-WITH-RUST contract; the user-facing
// Reset to Defaults action routes through the backend (projectStore.resetCutEntryToDefaults).
import { defaultRasterSettings, defaultVectorSettings } from '../../types/cutEntryDefaults';
import {
  displaySpeedToMmMin,
  formatSpeedForDisplay,
  speedInputValue,
  speedMmMinToDisplay,
  speedStepForUnit,
  speedUnitLabel,
} from '../../utils/speedUnits';
import { mmToDisplay, displayToMm, roundDisplayLength, lengthStep, lengthUnitLabel, labelWithUnit } from '../../utils/lengthUnits';

const OPERATION_TOOL = 'tool' as const;
const OPERATION_IMAGE = 'image' as const;
const OPERATION_FILL = 'fill' as const;
const OPERATION_OFFSET_FILL = 'offset_fill' as const;
const OPERATION_CUT = 'cut' as const;
const OPERATION_SCORE = 'score' as const;
const OPERATION_LINE = 'line' as const;
const DISPLAY_UNIT_INCHES = 'inches' as const;
const DISPLAY_UNIT_MM = 'mm' as const;
const SPEED_TIME_SECONDS = 'seconds' as const;
const SPEED_TIME_MINUTES = 'minutes' as const;
const GROUP_ALL_SHAPES = 'all_shapes_at_once' as const;
const GROUPS_TOGETHER = 'groups_together' as const;
const SHAPES_INDIVIDUALLY = 'shapes_individually' as const;
const RASTER_MODE_GRAYSCALE = 'grayscale' as const;
const RASTER_MODE_THRESHOLD = 'threshold' as const;
const RASTER_MODE_FLOYD_STEINBERG = 'floyd_steinberg' as const;
const RASTER_MODE_ORDERED_DITHER = 'ordered_dither' as const;
const RASTER_MODE_STUCKI = 'stucki' as const;
const RASTER_MODE_JARVIS = 'jarvis' as const;
const RASTER_MODE_SIERRA = 'sierra' as const;
const RASTER_MODE_ATKINSON = 'atkinson' as const;
const RASTER_MODE_HALFTONE = 'halftone' as const;
const RASTER_MODE_NEWSPRINT = 'newsprint' as const;
const RASTER_MODE_SKETCH = 'sketch' as const;
const EXPANDED_ICON = '▾';
const COLLAPSED_ICON = '▸';
const MOVE_UP_ICON = '▲';
const MOVE_DOWN_ICON = '▼';
const GROUPING_OPTIONS = [
  { value: GROUP_ALL_SHAPES, labelKey: 'panels.sub_layer_stack.group_all_shapes' },
  { value: GROUPS_TOGETHER, labelKey: 'panels.sub_layer_stack.group_groups_together' },
  { value: SHAPES_INDIVIDUALLY, labelKey: 'panels.sub_layer_stack.group_shapes_individually' },
] as const;

function modeLabel(operation: OperationType, t: (key: string) => string): string {
  switch (operation) {
    case OPERATION_TOOL:
      return t('panels.sub_layer_stack.operation_tool');
    case OPERATION_IMAGE:
      return t('panels.sub_layer_stack.operation_image');
    case OPERATION_FILL:
      return t('panels.sub_layer_stack.operation_fill');
    case OPERATION_OFFSET_FILL:
      return t('panels.sub_layer_stack.operation_offset_fill');
    case OPERATION_CUT:
    case OPERATION_SCORE:
    case OPERATION_LINE:
    default:
      return t('panels.sub_layer_stack.operation_line');
  }
}

function usesLineSurface(operation: OperationType): boolean {
  return operation === OPERATION_LINE || operation === OPERATION_CUT || operation === OPERATION_SCORE;
}

function displayModeValue(operation: OperationType): OperationType {
  return usesLineSurface(operation) ? OPERATION_LINE : operation;
}

function layerHasRasterObjects(layerId: string, objects: ProjectObject[]): boolean {
  return objects.some((object) => object.layer_id === layerId && object.data.type === 'raster_image');
}

function profileUsesDspMinPower(profile: MachineProfile | null): boolean {
  const firmware = profile?.firmware_type.trim().toLowerCase() ?? '';
  return ['dsp', 'ruida', 'trocen', 'topwisdom'].some((token) => firmware.includes(token));
}

function buildOperationPatch(entry: CutEntry, operation: OperationType) {
  const raster_settings =
    operation === OPERATION_IMAGE || operation === OPERATION_FILL || operation === OPERATION_OFFSET_FILL
      ? (entry.raster_settings ?? defaultRasterSettings())
      : null;
  const vector_settings =
    operation === OPERATION_LINE || operation === OPERATION_CUT || operation === OPERATION_SCORE || operation === OPERATION_OFFSET_FILL
      ? (entry.vector_settings ?? defaultVectorSettings())
      : null;
  return {
    operation,
    raster_settings,
    vector_settings,
  };
}

function getPasses(entry: CutEntry): number {
  return entry.vector_settings?.passes ?? entry.raster_settings?.passes ?? 1;
}

function lineIntervalToLinesPerInch(lineIntervalMm: number): number {
  return lineIntervalMm > 0 ? 25.4 / lineIntervalMm : 0;
}

function linesPerInchToLineInterval(linesPerInch: number): number {
  return linesPerInch > 0 ? 25.4 / linesPerInch : defaultRasterSettings().line_interval_mm;
}

function buildLineIntervalPatch(entry: CutEntry, lineIntervalMm: number) {
  const normalized = lineIntervalMm > 0 ? lineIntervalMm : defaultRasterSettings().line_interval_mm;
  return {
    raster_settings: {
      ...(entry.raster_settings ?? defaultRasterSettings()),
      line_interval_mm: normalized,
      dpi: normalized > 0 ? Math.round(25.4 / normalized) : 254,
    },
  };
}

function OffsetFillModeGraphic() {
  return (
    <svg
      width="64"
      height="64"
      viewBox="0 0 64 64"
      className="shrink-0 text-bb-text"
      aria-hidden="true"
      data-testid="offset-fill-mode-graphic"
    >
      <circle cx="28" cy="28" r="20" fill="none" stroke="currentColor" strokeWidth="2" opacity="0.9" />
      <circle cx="28" cy="28" r="13" fill="none" stroke="currentColor" strokeWidth="2" opacity="0.75" />
      <circle cx="28" cy="28" r="6" fill="none" stroke="currentColor" strokeWidth="2" opacity="0.6" />
      <path
        d="M49 43c-1 4-3 7-7 10"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
      />
      <path
        d="M44 49l-2 4 5-1"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <circle cx="28" cy="28" r="2.5" fill="currentColor" />
    </svg>
  );
}

interface SubLayerStackProps {
  layerId: string;
}

export function SubLayerStack({ layerId }: SubLayerStackProps) {
  const { t } = useTranslation();
  const layer = useProjectStore((s) => s.project?.layers.find((candidate) => candidate.id === layerId) ?? null);
  const projectObjects = useProjectStore((s) => s.project?.objects ?? []);
  const activeProfile = useMachineStore((s) =>
    s.profiles.find((profile) => profile.id === s.activeProfileId) ?? null,
  );
  const addCutEntry = useProjectStore((s) => s.addCutEntry);
  const removeCutEntry = useProjectStore((s) => s.removeCutEntry);
  const reorderCutEntry = useProjectStore((s) => s.reorderCutEntry);
  const updateCutEntry = useProjectStore((s) => s.updateCutEntry);
  const resetCutEntryToDefaults = useProjectStore((s) => s.resetCutEntryToDefaults);
  const appSettings = useAppStore((s) => s.settings);
  const displayUnit = appSettings?.display_unit === DISPLAY_UNIT_INCHES ? DISPLAY_UNIT_INCHES : DISPLAY_UNIT_MM;
  const speedTimeUnit = appSettings?.speed_time_unit === SPEED_TIME_SECONDS ? SPEED_TIME_SECONDS : SPEED_TIME_MINUTES;
  const speedLabel = speedUnitLabel(displayUnit, speedTimeUnit);
  const speedStep = speedStepForUnit(displayUnit, speedTimeUnit);
  const maxDisplaySpeed = speedMmMinToDisplay(50000, displayUnit, speedTimeUnit);
  const minDisplaySpeed = speedMmMinToDisplay(1, displayUnit, speedTimeUnit);
  const [expandedEntryId, setExpandedEntryId] = useState<string | null>(null);
  const offsetFillUnsupportedHelp = t('panels.sub_layer_stack.offset_fill_unsupported_help');

  const expandedId = useMemo(() => {
    if (!layer) return null;
    const entries = layer.entries ?? [];
    if (expandedEntryId && entries.some((entry) => entry.id === expandedEntryId)) {
      return expandedEntryId;
    }
    return entries[0]?.id ?? null;
  }, [expandedEntryId, layer]);

  if (!layer) {
    return null;
  }

  if (layer.is_tool_layer) {
    return (
      <div
        className="rounded border border-dashed border-bb-border bg-bb-bg-alt/60 p-3 text-xs text-bb-text-muted"
        data-testid="tool-layer-settings-placeholder"
      >
        {t('panels.sub_layer_stack.tool_layer')}
      </div>
    );
  }

  const entries = layer.entries;
  const supportsImageMode = layerHasRasterObjects(layerId, projectObjects);

  return (
    <div className="flex flex-col gap-2">
      {entries.length > 1 && (
        <div className="pb-1 text-[10px] text-bb-text-dim">{t('panels.sub_layer_stack.order_hint')}</div>
      )}
      {entries.length > 1 && (
      <div className="flex flex-wrap gap-1" data-testid="sub-layer-tabs">
        {entries.map((entry, index) => (
          <button
            key={entry.id}
            type="button"
            className={`rounded-t border px-2 py-1 text-xs ${
              entry.id === expandedId
                ? 'border-bb-accent bg-bb-accent/20 text-bb-text'
                : 'border-bb-border bg-bb-bg text-bb-text-muted hover:bg-bb-hover'
            }`}
            onClick={() => setExpandedEntryId(entry.id)}
            data-testid={`sub-layer-tab-${index}`}
          >
            {index + 1}. {modeLabel(entry.operation, t)}
          </button>
        ))}
      </div>
      )}
      {entries.map((entry, index) => {
        const expanded = entry.id === expandedId;
        if (!expanded) return null;
        const passes = getPasses(entry);
        const lineInterval = effectiveLineIntervalMm(entry.raster_settings);
        const linesPerInch = lineIntervalToLinesPerInch(lineInterval);
        const showsRasterSettings = entry.operation === OPERATION_IMAGE || entry.operation === OPERATION_FILL || entry.operation === OPERATION_OFFSET_FILL;
        const usesRasterPasses = entry.operation === OPERATION_IMAGE || entry.operation === OPERATION_FILL;
        const isOffsetFill = entry.operation === OPERATION_OFFSET_FILL;
        const isLineSurface = usesLineSurface(entry.operation);
        const canVector = isLineSurface || entry.operation === OPERATION_OFFSET_FILL;
        const showMinPower =
          profileUsesDspMinPower(activeProfile) ||
          (entry.operation === OPERATION_IMAGE && entry.raster_settings?.mode === RASTER_MODE_GRAYSCALE);
        const showZOffset = activeProfile?.supports_z_moves === true;
        const groupingMode = entry.vector_settings?.offset_fill_grouping_mode ?? GROUP_ALL_SHAPES;
        const modeOptions: Array<{ value: OperationType; label: string }> = [
          { value: OPERATION_LINE, label: t('panels.sub_layer_stack.operation_line') },
          { value: OPERATION_FILL, label: t('panels.sub_layer_stack.operation_fill') },
          { value: OPERATION_OFFSET_FILL, label: t('panels.sub_layer_stack.operation_offset_fill') },
        ];
        if (supportsImageMode || entry.operation === OPERATION_IMAGE) {
          modeOptions.push({ value: OPERATION_IMAGE, label: t('panels.sub_layer_stack.operation_image') });
        }

        return (
          <div
            key={entry.id}
            className={
              entries.length > 1
                ? `rounded-xl border border-bb-border bg-bb-surface shadow-sm transition-opacity ${entry.output_enabled ? '' : 'opacity-55'}`
                : ''
            }
            data-testid={`sub-layer-card-${index}`}
          >
            {entries.length > 1 && (
            <div className="flex items-center gap-2.5 px-3 py-2.5">
              <button
                type="button"
                className="text-xs text-bb-text-dim hover:text-bb-text"
                onClick={() => setExpandedEntryId(expanded ? null : entry.id)}
                data-testid={`sub-layer-expand-${entry.id}`}
              >
                {expanded ? EXPANDED_ICON : COLLAPSED_ICON}
              </button>
              <div className="min-w-0 flex-1">
                <div className="truncate text-xs font-semibold text-bb-text">
                  {index + 1} · {modeLabel(entry.operation, t)}
                </div>
                <div className="truncate text-[10px] text-bb-text-dim">
                  {t('panels.sub_layer_stack.summary', {
                    speed: formatSpeedForDisplay(entry.speed_mm_min, displayUnit, speedTimeUnit),
                    power: Math.round(entry.power_percent),
                    passes,
                  })}
                </div>
              </div>
              <div className="flex items-center gap-1.5">
                <label className="flex items-center gap-1.5 text-[10px] text-bb-text-muted">
                  {t('panels.sub_layer_stack.output')}
                  <ToggleSwitch
                    active={entry.output_enabled}
                    onClick={() => void updateCutEntry(layer.id, entry.id, { output_enabled: !entry.output_enabled })}
                    aria-label={t('panels.sub_layer_stack.output')}
                  />
                </label>
                {entries.length > 1 && (
                <>
                <button
                  type="button"
                  className="rounded border border-bb-border px-1 text-xs text-bb-text disabled:opacity-40"
                  onClick={() => void reorderCutEntry(layer.id, entry.id, index - 1)}
                  disabled={index === 0}
                  data-testid={`sub-layer-up-${entry.id}`}
                >
                  {MOVE_UP_ICON}
                </button>
                <button
                  type="button"
                  className="rounded border border-bb-border px-1 text-xs text-bb-text disabled:opacity-40"
                  onClick={() => void reorderCutEntry(layer.id, entry.id, index + 1)}
                  disabled={index === entries.length - 1}
                  data-testid={`sub-layer-down-${entry.id}`}
                >
                  {MOVE_DOWN_ICON}
                </button>
                </>
                )}
                <button
                  type="button"
                  className="rounded border border-bb-border p-1 text-bb-text-muted hover:text-bb-text"
                  onClick={() => void resetCutEntryToDefaults(layer.id, entry.id)}
                  title={t('panels.sub_layer_stack.reset_to_defaults_title')}
                  data-testid={`sub-layer-reset-${entry.id}`}
                >
                  <RotateCcw size={12} />
                </button>
                {entries.length > 1 && (
                <button
                  type="button"
                  className="rounded border border-bb-border px-1 text-xs text-bb-text disabled:opacity-40"
                  onClick={() => void removeCutEntry(layer.id, entry.id)}
                  data-testid={`sub-layer-delete-${entry.id}`}
                >
                  {t('panels.sub_layer_stack.delete')}
                </button>
                )}
              </div>
            </div>
            )}


            {expanded && (
              <div className={entries.length > 1 ? "flex flex-col gap-3 border-t border-bb-border px-3 py-3" : "flex flex-col gap-3"}>
                <div className="flex items-center gap-2">
                  <label className="text-xs text-bb-text-dim">{t('panels.sub_layer_stack.mode')}</label>
                  <select
                    className="flex-1 rounded border border-bb-border bg-bb-input px-2 py-1 text-xs text-bb-text"
                    value={displayModeValue(entry.operation)}
                    onChange={(e) =>
                      void updateCutEntry(
                        layer.id,
                        entry.id,
                        buildOperationPatch(entry, e.target.value as OperationType),
                      )
                    }
                  >
                    {modeOptions.map((option) => (
                      <option key={option.value} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                </div>
                <NumberInput
                  label={t('panels.sub_layer_stack.speed_with_unit', { unit: speedLabel })}
                  value={speedInputValue(entry.speed_mm_min, displayUnit, speedTimeUnit)}
                  onChange={(speed) => void updateCutEntry(layer.id, entry.id, {
                    speed_mm_min: displaySpeedToMmMin(speed, displayUnit, speedTimeUnit),
                  })}
                  min={minDisplaySpeed}
                  max={maxDisplaySpeed}
                  step={speedStep}
                />
                <div className="flex flex-col gap-1.5">
                  <NumberInput
                    label={t('panels.sub_layer_stack.power_percent')}
                    value={entry.power_percent}
                    onChange={(power_percent) => void updateCutEntry(layer.id, entry.id, { power_percent })}
                    min={0}
                    max={100}
                    step={1}
                  />
                  <input
                    type="range"
                    min={0}
                    max={100}
                    step={1}
                    value={entry.power_percent}
                    onChange={(e) => void updateCutEntry(layer.id, entry.id, { power_percent: Number(e.target.value) })}
                    aria-label={t('panels.sub_layer_stack.power_percent')}
                    className="bb-range w-full"
                    style={{
                      background: `linear-gradient(to right, rgb(var(--bb-accent)) 0%, #f59e0b ${entry.power_percent}%, rgb(var(--bb-surface-3)) ${entry.power_percent}%, rgb(var(--bb-surface-3)) 100%)`,
                    }}
                    data-testid={`sub-layer-power-slider-${entry.id}`}
                  />
                </div>
                {showMinPower && (
                  <NumberInput
                    label={t('panels.sub_layer_stack.min_power_percent')}
                    value={entry.power_min_percent}
                    onChange={(power_min_percent) => void updateCutEntry(layer.id, entry.id, { power_min_percent })}
                    min={0}
                    max={100}
                    step={1}
                  />
                )}
                <NumberInput
                  label={t('panels.sub_layer_stack.passes')}
                  value={passes}
                  onChange={(nextPasses) =>
                    void updateCutEntry(layer.id, entry.id, {
                      raster_settings: usesRasterPasses
                        ? { ...(entry.raster_settings ?? defaultRasterSettings()), passes: nextPasses }
                        : entry.raster_settings,
                      vector_settings: canVector
                        ? { ...(entry.vector_settings ?? defaultVectorSettings()), passes: nextPasses }
                        : entry.vector_settings,
                    })
                  }
                  min={1}
                  max={20}
                  step={1}
                />
                <div className="flex min-h-6 items-center justify-between text-xs">
                  <span className="text-bb-text-muted">{t('panels.sub_layer_stack.air_assist')}</span>
                  <ToggleSwitch
                    active={entry.air_assist}
                    onClick={() => void updateCutEntry(layer.id, entry.id, { air_assist: !entry.air_assist })}
                    aria-label={t('panels.sub_layer_stack.air_assist')}
                  />
                </div>
                {showZOffset && (
                  <NumberInput
                    label={labelWithUnit(t('panels.sub_layer_stack.z_offset_mm'), lengthUnitLabel(displayUnit))}
                    value={roundDisplayLength(mmToDisplay(entry.z_offset_mm, displayUnit), displayUnit)}
                    onChange={(v) => void updateCutEntry(layer.id, entry.id, { z_offset_mm: displayToMm(v, displayUnit) })}
                    min={mmToDisplay(-100, displayUnit)}
                    max={mmToDisplay(100, displayUnit)}
                    step={lengthStep(displayUnit, 0.1, 0.005)}
                  />
                )}

                {showsRasterSettings && !isOffsetFill && (
                  <div className="flex flex-col gap-2 rounded border border-bb-border/70 p-2">
                    <div className="text-xs font-medium uppercase tracking-wide text-bb-accent">{t('panels.sub_layer_stack.raster')}</div>
                    {entry.operation === OPERATION_IMAGE && (
                      <div className="flex items-center gap-2">
                        <label className="text-xs text-bb-text-dim">{t('panels.sub_layer_stack.mode')}</label>
                        <select
                          className="flex-1 rounded border border-bb-border bg-bb-input px-2 py-1 text-xs text-bb-text"
                          value={(entry.raster_settings?.mode ?? RASTER_MODE_FLOYD_STEINBERG) as RasterMode}
                          onChange={(e) =>
                            void updateCutEntry(layer.id, entry.id, {
                              raster_settings: {
                                ...(entry.raster_settings ?? defaultRasterSettings()),
                                mode: e.target.value as RasterMode,
                              },
                            })
                          }
                        >
                          <option value={RASTER_MODE_GRAYSCALE}>{t('panels.sub_layer_stack.raster_mode_grayscale')}</option>
                          <option value={RASTER_MODE_THRESHOLD}>{t('panels.sub_layer_stack.raster_mode_threshold')}</option>
                          <option value={RASTER_MODE_FLOYD_STEINBERG}>{t('panels.sub_layer_stack.raster_mode_floyd_steinberg')}</option>
                          <option value={RASTER_MODE_ORDERED_DITHER}>{t('panels.sub_layer_stack.raster_mode_ordered_dither')}</option>
                          <option value={RASTER_MODE_STUCKI}>{t('panels.sub_layer_stack.raster_mode_stucki')}</option>
                          <option value={RASTER_MODE_JARVIS}>{t('panels.sub_layer_stack.raster_mode_jarvis')}</option>
                          <option value={RASTER_MODE_SIERRA}>{t('panels.sub_layer_stack.raster_mode_sierra')}</option>
                          <option value={RASTER_MODE_ATKINSON}>{t('panels.sub_layer_stack.raster_mode_atkinson')}</option>
                          <option value={RASTER_MODE_HALFTONE}>{t('panels.sub_layer_stack.raster_mode_halftone')}</option>
                          <option value={RASTER_MODE_NEWSPRINT}>{t('panels.sub_layer_stack.raster_mode_newsprint')}</option>
                          <option value={RASTER_MODE_SKETCH}>{t('panels.sub_layer_stack.raster_mode_sketch')}</option>
                        </select>
                      </div>
                    )}
                    <NumberInput
                      label={labelWithUnit(t('panels.sub_layer_stack.line_interval_mm'), lengthUnitLabel(displayUnit))}
                      value={roundDisplayLength(mmToDisplay(lineInterval, displayUnit), displayUnit)}
                      onChange={(v) => {
                        const line_interval_mm = displayToMm(v, displayUnit);
                        void updateCutEntry(layer.id, entry.id, {
                          raster_settings: {
                            ...(entry.raster_settings ?? defaultRasterSettings()),
                            line_interval_mm,
                            dpi: line_interval_mm > 0 ? Math.round(25.4 / line_interval_mm) : 254,
                          },
                        });
                      }}
                      min={mmToDisplay(0.01, displayUnit)}
                      max={mmToDisplay(10, displayUnit)}
                      step={lengthStep(displayUnit, 0.001, 0.001)}
                    />
                    <NumberInput
                      label={t('panels.sub_layer_stack.scan_angle_degrees_symbol')}
                      value={entry.raster_settings?.scan_angle ?? 0}
                      onChange={(scan_angle) =>
                        void updateCutEntry(layer.id, entry.id, {
                          raster_settings: {
                            ...(entry.raster_settings ?? defaultRasterSettings()),
                            scan_angle,
                          },
                        })
                      }
                      min={0}
                      max={360}
                      step={1}
                    />
                    <NumberInput
                      label={labelWithUnit(t('panels.sub_layer_stack.overscan_mm'), lengthUnitLabel(displayUnit))}
                      value={roundDisplayLength(mmToDisplay(entry.raster_settings?.overscan_mm ?? 0, displayUnit), displayUnit)}
                      onChange={(v) =>
                        void updateCutEntry(layer.id, entry.id, {
                          raster_settings: {
                            ...(entry.raster_settings ?? defaultRasterSettings()),
                            overscan_mm: displayToMm(v, displayUnit),
                          },
                        })
                      }
                      min={0}
                      max={mmToDisplay(20, displayUnit)}
                      step={lengthStep(displayUnit, 0.1, 0.005)}
                    />
                    <Toggle
                      label={t('panels.sub_layer_stack.bidirectional')}
                      checked={entry.raster_settings?.bidirectional ?? true}
                      onChange={(bidirectional) =>
                        void updateCutEntry(layer.id, entry.id, {
                          raster_settings: {
                            ...(entry.raster_settings ?? defaultRasterSettings()),
                            bidirectional,
                          },
                        })
                      }
                    />
                  </div>
                )}

                {isOffsetFill && (
                  <div
                    className="flex flex-col gap-3 rounded border border-bb-border/70 p-2"
                    data-testid={`offset-fill-settings-${entry.id}`}
                  >
                    <div className="text-xs font-medium uppercase tracking-wide text-bb-accent">
                      {t('panels.sub_layer_stack.operation_offset_fill')}
                    </div>
                    <div className="flex items-start gap-3">
                      <OffsetFillModeGraphic />
                      <div className="flex min-w-0 flex-1 flex-col gap-2">
                        <NumberInput
                          label={labelWithUnit(t('panels.sub_layer_stack.line_interval_mm'), lengthUnitLabel(displayUnit))}
                          value={roundDisplayLength(mmToDisplay(lineInterval, displayUnit), displayUnit)}
                          onChange={(v) =>
                            void updateCutEntry(
                              layer.id,
                              entry.id,
                              buildLineIntervalPatch(entry, displayToMm(v, displayUnit)),
                            )
                          }
                          min={mmToDisplay(0.01, displayUnit)}
                          max={mmToDisplay(10, displayUnit)}
                          step={lengthStep(displayUnit, 0.001, 0.001)}
                        />
                        <NumberInput
                          label={t('panels.sub_layer_stack.lines_per_inch')}
                          value={linesPerInch}
                          onChange={(nextLinesPerInch) =>
                            void updateCutEntry(
                              layer.id,
                              entry.id,
                              buildLineIntervalPatch(
                                entry,
                                linesPerInchToLineInterval(nextLinesPerInch),
                              ),
                            )
                          }
                          min={1}
                          max={2540}
                          step={1}
                        />
                      </div>
                    </div>
                    <div className="flex flex-col gap-1.5">
                      <div className="text-xs text-bb-text-muted">{t('panels.sub_layer_stack.grouping')}</div>
                      {GROUPING_OPTIONS.map((option) => (
                        <label key={option.value} className="flex items-center gap-2 text-xs text-bb-text">
                          <input
                            type="radio"
                            name={`offset-fill-grouping-${entry.id}`}
                            value={option.value}
                            checked={groupingMode === option.value}
                            onChange={(event) =>
                              void updateCutEntry(layer.id, entry.id, {
                                vector_settings: {
                                  ...(entry.vector_settings ?? defaultVectorSettings()),
                                  offset_fill_grouping_mode:
                                    event.target.value as OffsetFillGroupingMode,
                                },
                              })
                            }
                            className="accent-bb-accent"
                          />
                          <span>{t(option.labelKey)}</span>
                        </label>
                      ))}
                    </div>
                    <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
                      <div title={offsetFillUnsupportedHelp}>
                        <Toggle
                          label={t('panels.sub_layer_stack.bidirectional_fill')}
                          checked={false}
                          onChange={() => {}}
                          disabled
                        />
                      </div>
                      <div title={offsetFillUnsupportedHelp}>
                        <Toggle
                          label={t('panels.sub_layer_stack.cross_hatch')}
                          checked={false}
                          onChange={() => {}}
                          disabled
                        />
                      </div>
                      <div title={offsetFillUnsupportedHelp} className="sm:col-span-2">
                        <NumberInput
                          label={t('panels.sub_layer_stack.scan_angle_deg')}
                          value={0}
                          onChange={() => {}}
                          min={0}
                          max={360}
                          step={1}
                          disabled
                        />
                      </div>
                    </div>
                  </div>
                )}

                {canVector && (
                  <div className="flex flex-col gap-2 rounded border border-bb-border/70 p-2">
                    {isLineSurface && (
                      <>
                        <Toggle
                          label={t('panels.sub_layer_stack.perforation')}
                          checked={entry.vector_settings?.perforation_enabled ?? false}
                          onChange={(perforation_enabled) =>
                            void updateCutEntry(layer.id, entry.id, {
                              vector_settings: {
                                ...(entry.vector_settings ?? defaultVectorSettings()),
                                perforation_enabled,
                              },
                            })
                          }
                        />
                        {(entry.vector_settings?.perforation_enabled ?? false) && (
                          <div className="grid grid-cols-2 gap-2">
                            <NumberInput
                              label={t('panels.layers.quick_edit.on_ms')}
                              value={entry.vector_settings?.perforation_on_ms ?? 10}
                              onChange={(perforation_on_ms) =>
                                void updateCutEntry(layer.id, entry.id, {
                                  vector_settings: {
                                    ...(entry.vector_settings ?? defaultVectorSettings()),
                                    perforation_on_ms,
                                  },
                                })
                              }
                              min={1}
                              max={1000}
                              step={1}
                            />
                            <NumberInput
                              label={t('panels.layers.quick_edit.off_ms')}
                              value={entry.vector_settings?.perforation_off_ms ?? 10}
                              onChange={(perforation_off_ms) =>
                                void updateCutEntry(layer.id, entry.id, {
                                  vector_settings: {
                                    ...(entry.vector_settings ?? defaultVectorSettings()),
                                    perforation_off_ms,
                                  },
                                })
                              }
                              min={1}
                              max={1000}
                              step={1}
                            />
                          </div>
                        )}
                      </>
                    )}
                    <NumberInput
                      label={labelWithUnit(t('panels.sub_layer_stack.kerf_offset_mm'), lengthUnitLabel(displayUnit))}
                      value={roundDisplayLength(mmToDisplay(entry.vector_settings?.kerf_offset_mm ?? 0, displayUnit), displayUnit)}
                      onChange={(v) =>
                        void updateCutEntry(layer.id, entry.id, {
                          vector_settings: {
                            ...(entry.vector_settings ?? defaultVectorSettings()),
                            kerf_offset_mm: displayToMm(v, displayUnit),
                          },
                        })
                      }
                      min={mmToDisplay(-5, displayUnit)}
                      max={mmToDisplay(5, displayUnit)}
                      step={lengthStep(displayUnit, 0.01, 0.001)}
                    />
                    {entry.operation === OPERATION_OFFSET_FILL && (
                      <>
                        <NumberInput
                          label={labelWithUnit(t('panels.sub_layer_stack.offset_overlap_mm'), lengthUnitLabel(displayUnit))}
                          value={roundDisplayLength(mmToDisplay(entry.vector_settings?.offset_overlap_mm ?? 0, displayUnit), displayUnit)}
                          onChange={(v) =>
                            void updateCutEntry(layer.id, entry.id, {
                              vector_settings: {
                                ...(entry.vector_settings ?? defaultVectorSettings()),
                                offset_overlap_mm: displayToMm(v, displayUnit),
                              },
                            })
                          }
                          min={0}
                          max={mmToDisplay(10, displayUnit)}
                          step={lengthStep(displayUnit, 0.01, 0.001)}
                        />
                        <Toggle
                          label={t('panels.sub_layer_stack.offset_outward')}
                          checked={entry.vector_settings?.offset_outward ?? false}
                          onChange={(offset_outward) =>
                            void updateCutEntry(layer.id, entry.id, {
                              vector_settings: {
                                ...(entry.vector_settings ?? defaultVectorSettings()),
                                offset_outward,
                              },
                            })
                          }
                        />
                      </>
                    )}
                  </div>
                )}
              </div>
            )}
          </div>
        );
      })}

      <button
        type="button"
        className="rounded border border-dashed border-bb-border px-2 py-2 text-sm text-bb-text disabled:opacity-40"
        onClick={() => void addCutEntry(layer.id, entries.length > 0 ? entries[entries.length - 1]?.id ?? null : null)}
        disabled={entries.length >= 11}
        data-testid="add-sub-layer"
        title={entries.length >= 11 ? t('panels.sub_layer_stack.max_sub_layers_title') : t('panels.sub_layer_stack.add_sub_layer_title')}
      >
        {t('panels.sub_layer_stack.add_sub_layer')}
      </button>
    </div>
  );
}
