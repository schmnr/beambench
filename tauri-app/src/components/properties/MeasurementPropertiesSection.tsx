import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { ArrowLeftRight, CircleDot, Ruler, TriangleRight, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { MeasurementMode } from '../../canvas/measurement';
import { useAppStore } from '../../stores/appStore';
import { useMeasurementStore } from '../../stores/measurementStore';
import type { Point2D } from '../../types/project';
import { ContextualToolSection } from './ContextualToolSection';

type DisplayUnit = 'mm' | 'inches';
type MetricRow = { key: string; label: string; value: string };

const MODE_DEFINITIONS: Array<{ mode: MeasurementMode; labelKey: string; fallback: string; icon: ReactNode }> = [
  { mode: 'linear', labelKey: 'panels.measurement.mode_linear', fallback: 'Linear', icon: <Ruler size={15} /> },
  { mode: 'angle', labelKey: 'panels.measurement.mode_angle', fallback: 'Angle', icon: <TriangleRight size={15} /> },
  { mode: 'radius', labelKey: 'panels.measurement.mode_radius', fallback: 'Radius', icon: <CircleDot size={15} /> },
  { mode: 'gap', labelKey: 'panels.measurement.mode_gap', fallback: 'Gap', icon: <ArrowLeftRight size={15} /> },
];

function displayUnit(value: string | undefined): DisplayUnit {
  return value === 'inches' ? 'inches' : 'mm';
}

function formatLength(valueMm: number, unit: DisplayUnit): string {
  return unit === 'inches' ? `${(valueMm / 25.4).toFixed(3)} in` : `${valueMm.toFixed(2)} mm`;
}

function formatArea(valueMm2: number | null, unit: DisplayUnit, notApplicable: string): string {
  if (valueMm2 == null) return notApplicable;
  return unit === 'inches'
    ? `${(valueMm2 / (25.4 * 25.4)).toFixed(3)} in²`
    : `${valueMm2.toFixed(2)} mm²`;
}

function formatPoint(point: Point2D, unit: DisplayUnit): string {
  const x = unit === 'inches' ? point.x / 25.4 : point.x;
  const y = unit === 'inches' ? point.y / 25.4 : point.y;
  const suffix = unit === 'inches' ? 'in' : 'mm';
  const digits = unit === 'inches' ? 3 : 2;
  return `${x.toFixed(digits)}, ${y.toFixed(digits)} ${suffix}`;
}

function midpoint(start: Point2D, end: Point2D): Point2D {
  return { x: (start.x + end.x) / 2, y: (start.y + end.y) / 2 };
}

export function MeasurementPropertiesSection() {
  const { t } = useTranslation();
  const measurement = useMeasurementStore((state) => state);
  const unit = displayUnit(useAppStore((state) => state.settings?.display_unit));
  const [copiedKey, setCopiedKey] = useState<string | null>(null);

  useEffect(() => {
    if (!copiedKey) return;
    const timeout = window.setTimeout(() => setCopiedKey(null), 900);
    return () => window.clearTimeout(timeout);
  }, [copiedKey]);

  const rows = useMemo<MetricRow[]>(() => {
    const notApplicable = t('panels.measurement.not_applicable');
    const result = measurement.result;
    const linear = measurement.draft
      ?? (result?.kind === 'linear' || result?.kind === 'gap' ? result : null);
    if (linear) {
      const baseRows: MetricRow[] = [
        { key: 'length', label: result?.kind === 'gap' ? t('panels.measurement.minimum_gap', { defaultValue: 'Minimum gap' }) : t('panels.measurement.length'), value: formatLength(linear.lengthMm, unit) },
        { key: 'horizontal', label: t('panels.measurement.horizontal', { defaultValue: 'Horizontal' }), value: formatLength(Math.abs(linear.dxMm), unit) },
        { key: 'vertical', label: t('panels.measurement.vertical', { defaultValue: 'Vertical' }), value: formatLength(Math.abs(linear.dyMm), unit) },
        { key: 'angle', label: t('panels.measurement.angle'), value: `${linear.angleDeg.toFixed(1)}°` },
        { key: 'start', label: t('panels.measurement.start'), value: formatPoint(linear.start, unit) },
        { key: 'end', label: t('panels.measurement.end'), value: formatPoint(linear.end, unit) },
        { key: 'midpoint', label: t('panels.measurement.midpoint'), value: formatPoint(midpoint(linear.start, linear.end), unit) },
      ];
      if (result?.kind === 'gap') {
        baseRows.splice(3, 0,
          { key: 'horizontal-gap', label: t('panels.measurement.horizontal_gap', { defaultValue: 'Horizontal gap' }), value: formatLength(result.horizontalGapMm, unit) },
          { key: 'vertical-gap', label: t('panels.measurement.vertical_gap', { defaultValue: 'Vertical gap' }), value: formatLength(result.verticalGapMm, unit) },
        );
      }
      return baseRows;
    }
    if (result?.kind === 'angle') {
      return [
        { key: 'angle', label: t('panels.measurement.angle'), value: `${result.angleDeg.toFixed(1)}°` },
        { key: 'first-segment', label: t('panels.measurement.first_segment', { defaultValue: 'First segment' }), value: formatLength(result.first.lengthMm, unit) },
        { key: 'second-segment', label: t('panels.measurement.second_segment', { defaultValue: 'Second segment' }), value: formatLength(result.second.lengthMm, unit) },
      ];
    }
    if (result?.kind === 'radius') {
      if (result.circular) {
        return [
          { key: 'radius', label: t('panels.measurement.radius', { defaultValue: 'Radius' }), value: formatLength(result.radiusXmm, unit) },
          { key: 'diameter', label: t('panels.measurement.diameter', { defaultValue: 'Diameter' }), value: formatLength(result.diameterXmm, unit) },
          { key: 'center', label: t('panels.measurement.center'), value: formatPoint(result.center, unit) },
        ];
      }
      return [
        { key: 'radius-x', label: t('panels.measurement.radius_x', { defaultValue: 'Radius X' }), value: formatLength(result.radiusXmm, unit) },
        { key: 'radius-y', label: t('panels.measurement.radius_y', { defaultValue: 'Radius Y' }), value: formatLength(result.radiusYmm, unit) },
        { key: 'diameter-x', label: t('panels.measurement.diameter_x', { defaultValue: 'Diameter X' }), value: formatLength(result.diameterXmm, unit) },
        { key: 'diameter-y', label: t('panels.measurement.diameter_y', { defaultValue: 'Diameter Y' }), value: formatLength(result.diameterYmm, unit) },
        { key: 'center', label: t('panels.measurement.center'), value: formatPoint(result.center, unit) },
      ];
    }
    if (measurement.hover) {
      const metrics = measurement.hover.objectMetrics;
      const hoverRows: MetricRow[] = [
        { key: 'width', label: t('panels.measurement.width'), value: formatLength(metrics.widthMm, unit) },
        { key: 'height', label: t('panels.measurement.height'), value: formatLength(metrics.heightMm, unit) },
        { key: 'perimeter', label: t('panels.measurement.perimeter'), value: metrics.perimeterMm == null ? notApplicable : formatLength(metrics.perimeterMm, unit) },
        { key: 'area', label: t('panels.measurement.area'), value: formatArea(metrics.areaMm2, unit, notApplicable) },
      ];
      if (measurement.hover.segment) {
        hoverRows.push({ key: 'segment', label: t('panels.measurement.segment'), value: formatLength(measurement.hover.segment.lengthMm, unit) });
      }
      return hoverRows;
    }
    return [];
  }, [measurement.draft, measurement.hover, measurement.result, t, unit]);

  const hint = (() => {
    if (measurement.pending?.kind === 'linear') return t('panels.measurement.linear_second_hint', { defaultValue: 'Click the second point' });
    if (measurement.pending?.kind === 'angle') return t('panels.measurement.angle_second_hint', { defaultValue: 'Click a second segment' });
    if (measurement.pending?.kind === 'gap') return t('panels.measurement.gap_second_hint', { defaultValue: 'Click the second object' });
    if (measurement.mode === 'angle') return t('panels.measurement.angle_hint', { defaultValue: 'Click two segments to measure their angle' });
    if (measurement.mode === 'radius') return t('panels.measurement.radius_hint', { defaultValue: 'Click a circle or ellipse' });
    if (measurement.mode === 'gap') return t('panels.measurement.gap_hint', { defaultValue: 'Click two objects to measure their closest gap' });
    return t('panels.measurement.linear_hint', { defaultValue: 'Click-drag, or click two points' });
  })();

  const copyValue = async (row: MetricRow) => {
    if (!navigator.clipboard?.writeText) return;
    await navigator.clipboard.writeText(row.value);
    setCopiedKey(row.key);
  };

  return (
    <ContextualToolSection
      title={t('panels.measurement.title', { defaultValue: 'Measure' })}
      icon={<Ruler size={15} />}
      testId="measurement-properties-section"
    >
      <div className="grid grid-cols-4 overflow-hidden rounded-lg border border-bb-border bg-bb-surface/70" role="toolbar" aria-label={t('panels.measurement.modes', { defaultValue: 'Measurement modes' })}>
        {MODE_DEFINITIONS.map(({ mode, labelKey, fallback, icon }) => {
          const active = measurement.mode === mode;
          const label = t(labelKey, { defaultValue: fallback });
          return (
            <button
              key={mode}
              type="button"
              aria-label={label}
              aria-pressed={active}
              title={label}
              onClick={() => useMeasurementStore.getState().setMode(mode)}
              className={`flex h-10 min-w-0 flex-col items-center justify-center gap-0.5 border-r border-bb-border text-[9px] outline-none transition-colors last:border-r-0 focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-bb-accent ${
                active ? 'bg-bb-accent/15 text-bb-accent' : 'text-bb-text-dim hover:bg-bb-hover hover:text-bb-text'
              }`}
            >
              {icon}
              <span className="truncate px-1">{label}</span>
            </button>
          );
        })}
      </div>

      <div className="rounded-lg border border-bb-border bg-bb-surface/45 px-2.5 py-2 text-[11px] text-bb-text-dim">
        {hint}
      </div>

      {rows.length > 0 && (
        <div className="overflow-hidden rounded-lg border border-bb-border" data-testid="measurement-results">
          {rows.map((row) => (
            <div key={row.key} className="grid grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)] items-center border-b border-bb-border last:border-b-0">
              <span className="truncate px-2 py-1.5 text-[11px] text-bb-text-dim">{row.label}</span>
              <button
                type="button"
                title={t('panels.measurement.copy_value', { defaultValue: 'Copy value' })}
                onClick={() => void copyValue(row)}
                className={`min-w-0 truncate px-2 py-1.5 text-right font-mono text-[11px] outline-none transition-colors hover:bg-bb-hover focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-bb-accent ${
                  copiedKey === row.key ? 'text-bb-accent' : 'text-bb-text'
                }`}
              >
                {copiedKey === row.key ? t('panels.measurement.copied', { defaultValue: 'Copied' }) : row.value}
              </button>
            </div>
          ))}
        </div>
      )}

      {(measurement.result || measurement.pending || measurement.draft) && (
        <button
          type="button"
          onClick={() => useMeasurementStore.getState().clearMeasurement()}
          className="flex h-8 w-full items-center justify-center gap-1.5 rounded-lg border border-bb-border bg-bb-surface px-2 text-[11px] font-medium text-bb-text outline-none transition-colors hover:bg-bb-hover focus-visible:ring-1 focus-visible:ring-bb-accent"
        >
          <X size={13} />
          {t('panels.measurement.clear_measurement', { defaultValue: 'Clear measurement' })}
        </button>
      )}

      <div className="text-[10px] text-bb-text-dim">{t('panels.measurement.keyboard_hint', { defaultValue: 'Shift constrains angles. Escape clears, then returns to Select.' })}</div>
    </ContextualToolSection>
  );
}
