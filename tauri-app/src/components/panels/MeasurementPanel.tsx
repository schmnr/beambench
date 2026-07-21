import { useTranslation } from 'react-i18next';
import { useAppStore } from '../../stores/appStore';
import { useMeasurementStore } from '../../stores/measurementStore';

const EMPTY_VALUE = '--';
type MetricRow = readonly [label: string, value: string];

function asUnit(unit: string | undefined): 'mm' | 'inches' {
  return unit === 'inches' ? 'inches' : 'mm';
}

function formatLength(valueMm: number | null | undefined, unit: 'mm' | 'inches'): string {
  if (valueMm == null || !Number.isFinite(valueMm)) return EMPTY_VALUE;
  if (unit === 'inches') return `${(valueMm / 25.4).toFixed(3)} in`;
  return `${valueMm.toFixed(2)} mm`;
}

function formatArea(valueMm2: number | null | undefined, unit: 'mm' | 'inches'): string {
  if (valueMm2 == null || !Number.isFinite(valueMm2)) return EMPTY_VALUE;
  if (unit === 'inches') return `${(valueMm2 / (25.4 * 25.4)).toFixed(3)} in^2`;
  return `${valueMm2.toFixed(2)} mm^2`;
}

function formatNumber(value: number | null | undefined): string {
  return value == null ? EMPTY_VALUE : String(value);
}

function formatPoint(
  point: { x: number; y: number } | null | undefined,
  unit: 'mm' | 'inches',
): string {
  if (!point) return EMPTY_VALUE;
  const x = unit === 'inches' ? point.x / 25.4 : point.x;
  const y = unit === 'inches' ? point.y / 25.4 : point.y;
  const suffix = unit === 'inches' ? 'in' : 'mm';
  const digits = unit === 'inches' ? 3 : 2;
  return `${x.toFixed(digits)}, ${y.toFixed(digits)} ${suffix}`;
}

function formatDiff(
  dxMm: number | null | undefined,
  dyMm: number | null | undefined,
  unit: 'mm' | 'inches',
): string {
  if (dxMm == null || dyMm == null) return EMPTY_VALUE;
  const x = unit === 'inches' ? dxMm / 25.4 : dxMm;
  const y = unit === 'inches' ? dyMm / 25.4 : dyMm;
  const suffix = unit === 'inches' ? 'in' : 'mm';
  const digits = unit === 'inches' ? 3 : 2;
  return `${x.toFixed(digits)}, ${y.toFixed(digits)} ${suffix}`;
}

function formatAngle(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return EMPTY_VALUE;
  return `${value.toFixed(1)} deg`;
}

function midpoint(
  start: { x: number; y: number } | null | undefined,
  end: { x: number; y: number } | null | undefined,
) {
  if (!start || !end) return null;
  return {
    x: (start.x + end.x) / 2,
    y: (start.y + end.y) / 2,
  };
}

function MetricSection({ title, rows }: { title: string; rows: readonly MetricRow[] }) {
  return (
    <section className="rounded border border-bb-border">
      <div className="border-b border-bb-border bg-bb-surface px-2 py-1 font-medium text-bb-text-dim">
        {title}
      </div>
      <table className="w-full border-collapse">
        <tbody>
          {rows.map(([label, value]) => (
            <tr key={label} className="border-b border-bb-border last:border-b-0">
              <td className="w-2/5 px-2 py-1 text-bb-text-dim">{label}</td>
              <td className="px-2 py-1 font-mono text-bb-text">{value}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}

export function MeasurementPanel() {
  const { t } = useTranslation();
  const measurement = useMeasurementStore((s) => s.state);
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const unit = asUnit(settings?.display_unit);

  const objectMetrics = measurement.type === 'hover' ? measurement.objectMetrics : null;
  const segment = measurement.type === 'hover'
    ? measurement.segment
    : measurement.type === 'drag'
      ? {
          start: measurement.start,
          end: measurement.end,
          dxMm: measurement.dxMm,
          dyMm: measurement.dyMm,
          lengthMm: measurement.lengthMm,
          angleDeg: measurement.angleDeg,
        }
      : null;

  const objectRows: MetricRow[] = objectMetrics
    ? [
        [t('panels.measurement.width'), formatLength(objectMetrics.widthMm, unit)],
        [t('panels.measurement.height'), formatLength(objectMetrics.heightMm, unit)],
        [t('panels.measurement.center'), formatPoint(objectMetrics.center, unit)],
        [t('panels.measurement.area'), objectMetrics.areaMm2 == null ? t('panels.measurement.not_applicable') : formatArea(objectMetrics.areaMm2, unit)],
        [t('panels.measurement.perimeter'), formatLength(objectMetrics.perimeterMm, unit)],
        [t('panels.measurement.closed_open'), objectMetrics.closed == null ? t('panels.measurement.not_applicable') : objectMetrics.closed ? t('panels.measurement.closed') : t('panels.measurement.open')],
        [t('panels.measurement.nodes'), formatNumber(objectMetrics.nodes)],
        [t('panels.measurement.lines'), formatNumber(objectMetrics.lines)],
        [t('panels.measurement.curves'), formatNumber(objectMetrics.curves)],
      ]
    : [];

  const segmentRows: MetricRow[] = segment
    ? [
        [t('panels.measurement.length'), formatLength(segment.lengthMm, unit)],
        [t('panels.measurement.start'), formatPoint(segment.start, unit)],
        [t('panels.measurement.end'), formatPoint(segment.end, unit)],
        [t('panels.measurement.midpoint'), formatPoint(midpoint(segment.start, segment.end), unit)],
        [t('panels.measurement.difference'), formatDiff(segment.dxMm, segment.dyMm, unit)],
        [t('panels.measurement.angle'), formatAngle(segment.angleDeg)],
      ]
    : [];

  const setUnit = (nextUnit: 'mm' | 'inches') => {
    if (nextUnit === unit) return;
    void updateSettings({ display_unit: nextUnit });
  };

  return (
    <div className="flex h-full flex-col gap-2 px-2 py-2 text-xs text-bb-text">
      <div className="flex items-center justify-between gap-2">
        <div className="min-w-0">
          <div className="truncate font-medium text-bb-text">
            {measurement.type === 'hover'
              ? measurement.objectMetrics.objectName
              : measurement.type === 'drag'
                ? t('panels.measurement.temporary_line')
                : t('panels.measurement.no_measurement')}
          </div>
          <div className="text-bb-text-dim">
            {measurement.type === 'hover'
              ? t('panels.measurement.hover')
              : measurement.type === 'drag'
                ? t('panels.measurement.drag')
                : t('panels.measurement.hint')}
          </div>
        </div>
        <div className="flex rounded border border-bb-border overflow-hidden">
          <button
            type="button"
            className={`px-2 py-1 ${unit === 'mm' ? 'bg-bb-accent text-bb-on-accent' : 'bg-bb-surface hover:bg-bb-surface-2'}`}
            onClick={() => setUnit('mm')}
          >
            {t('panels.measurement.unit_mm')}
          </button>
          <button
            type="button"
            className={`px-2 py-1 border-l border-bb-border ${unit === 'inches' ? 'bg-bb-accent text-bb-on-accent' : 'bg-bb-surface hover:bg-bb-surface-2'}`}
            onClick={() => setUnit('inches')}
          >
            {t('panels.measurement.unit_in')}
          </button>
        </div>
      </div>

      <div className="min-h-0 space-y-2 overflow-auto">
        {objectRows.length > 0 ? (
          <MetricSection title={t('panels.measurement.object')} rows={objectRows} />
        ) : null}
        {segmentRows.length > 0 ? (
          <MetricSection title={measurement.type === 'drag' ? t('panels.measurement.line') : t('panels.measurement.segment')} rows={segmentRows} />
        ) : null}
        {objectRows.length === 0 && segmentRows.length === 0 ? (
          <div className="rounded border border-bb-border px-2 py-3 text-bb-text-dim">
            {t('panels.measurement.no_measurement')}
          </div>
        ) : null}
      </div>
    </div>
  );
}
