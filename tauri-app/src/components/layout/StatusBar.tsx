import { useTranslation } from 'react-i18next';
import { useAppStore } from '../../stores/appStore';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore, type NodeSubMode, type ToolType } from '../../stores/uiStore';
import { useMachineStore } from '../../stores/machineStore';
import { useMeasurementStore } from '../../stores/measurementStore';
import { zoomToFitBounds } from '../../canvas/ViewportTransform';
import { getCanvasViewportSize } from '../../canvas/canvasViewportRegistry';
import { canvasToMachinePoint } from '../../utils/workspaceCoordinates';

const TOOL_HINT_KEYS: Record<ToolType, string> = {
  select: 'status.tool_hint.select',
  rect: 'status.tool_hint.rect',
  ellipse: 'status.tool_hint.ellipse',
  star: 'status.tool_hint.star',
  text: 'status.tool_hint.text',
  node: 'status.tool_hint.node',
  line: 'status.tool_hint.line',
  polygon: 'status.tool_hint.polygon',
  trim: 'status.tool_hint.trim',
  tabs: 'status.tool_hint.tabs',
  radius: 'status.tool_hint.radius',
  measure: 'status.tool_hint.measure',
  laser_position: 'status.tool_hint.laser_position',
  two_point_rotate_scale: 'status.tool_hint.two_point_rotate_scale',
  warp: 'status.tool_hint.warp_selection',
};

const NODE_SUBMODE_HINT_KEYS: Partial<Record<NodeSubMode, string>> = {
  trim: 'status.tool_hint.node_trim',
};

export function StatusBar() {
  const { t } = useTranslation();
  const status = useAppStore((s) => s.status);
  const settings = useAppStore((s) => s.settings);
  const zoom = useUiStore((s) => s.zoom);
  const zoomIn = useUiStore((s) => s.zoomIn);
  const zoomOut = useUiStore((s) => s.zoomOut);
  const zoomToFit = useUiStore((s) => s.zoomToFit);
  const cursorWorldPos = useUiStore((s) => s.cursorWorldPos);
  const activeTool = useUiStore((s) => s.activeTool);
  const meshDeformMode = useUiStore((s) => s.meshDeformMode);
  const textBoxModeActive = useUiStore((s) => (s.textDefaults.max_width ?? 0) > 0);
  const nodeSubMode = useUiStore((s) => s.nodeSubMode);
  const nodeEditNodeCount = useUiStore((s) => s.nodeEditNodeCount);
  const project = useProjectStore((s) => s.project);

  const selectedObjectIds = useProjectStore((s) => s.selectedObjectIds);
  const measurement = useMeasurementStore((s) => s);

  const jobProgress = useMachineStore((s) => s.jobProgress);

  const unit = settings?.display_unit ?? 'mm';
  const unitLabel = unit === 'inches' ? 'in' : 'mm';

  const displayPoint = (point: { x: number; y: number }) => (
    project ? canvasToMachinePoint(point, project.workspace) : point
  );

  const cursorDisplayPos = cursorWorldPos ? displayPoint(cursorWorldPos) : null;

  // Compute selection bounds in the same coordinate system shown by the rulers.
  const selectionBounds = (() => {
    if (!project || selectedObjectIds.length === 0) return null;
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const id of selectedObjectIds) {
      const obj = project.objects.find((o) => o.id === id);
      if (obj) {
        const corners = [
          obj.bounds.min,
          { x: obj.bounds.max.x, y: obj.bounds.min.y },
          obj.bounds.max,
          { x: obj.bounds.min.x, y: obj.bounds.max.y },
        ].map(displayPoint);
        for (const corner of corners) {
          minX = Math.min(minX, corner.x);
          minY = Math.min(minY, corner.y);
          maxX = Math.max(maxX, corner.x);
          maxY = Math.max(maxY, corner.y);
        }
      }
    }
    return isFinite(minX) ? { minX, minY, maxX, maxY } : null;
  })();

  const formatPos = (val: number) => {
    if (unit === 'inches') return (val / 25.4).toFixed(3);
    return val.toFixed(1);
  };

  const formatArea = (val: number | null | undefined) => {
    if (val == null || !Number.isFinite(val)) return 'N/A';
    if (unit === 'inches') return `${(val / (25.4 * 25.4)).toFixed(3)} in^2`;
    return `${val.toFixed(1)} mm^2`;
  };

  const measurementStatus = (() => {
    if (activeTool !== 'measure') return null;
    const linear = measurement.draft
      ?? (measurement.result?.kind === 'linear' || measurement.result?.kind === 'gap'
        ? measurement.result
        : null);
    if (linear) {
      return `dx: ${formatPos(linear.dxMm)} ${unitLabel}  dy: ${formatPos(linear.dyMm)} ${unitLabel}  len: ${formatPos(linear.lengthMm)} ${unitLabel}  angle: ${linear.angleDeg.toFixed(1)}°`;
    }
    if (measurement.result?.kind === 'angle') {
      return `angle: ${measurement.result.angleDeg.toFixed(1)}°`;
    }
    if (measurement.result?.kind === 'radius') {
      return measurement.result.circular
        ? `radius: ${formatPos(measurement.result.radiusXmm)} ${unitLabel}  diameter: ${formatPos(measurement.result.diameterXmm)} ${unitLabel}`
        : `rx: ${formatPos(measurement.result.radiusXmm)} ${unitLabel}  ry: ${formatPos(measurement.result.radiusYmm)} ${unitLabel}`;
    }
    if (measurement.hover) {
      const objectSummary = `w: ${formatPos(measurement.hover.objectMetrics.widthMm)} ${unitLabel}  h: ${formatPos(measurement.hover.objectMetrics.heightMm)} ${unitLabel}  area: ${formatArea(measurement.hover.objectMetrics.areaMm2)}`;
      if (measurement.hover.segment) {
        return `${objectSummary}  seg: ${formatPos(measurement.hover.segment.lengthMm)} ${unitLabel} @ ${measurement.hover.segment.angleDeg.toFixed(1)}°`;
      }
      return objectSummary;
    }
    return null;
  })();

  const handleZoomToFit = () => {
    if (!project) return;
    const { bed_width_mm, bed_height_mm } = project.workspace;
    const size = getCanvasViewportSize();
    if (!size) return;
    const result = zoomToFitBounds(
      { min: { x: 0, y: 0 }, max: { x: bed_width_mm, y: bed_height_mm } },
      size.width,
      size.height,
    );
    zoomToFit(result.offset, result.zoom);
  };

  const jobPercent =
    jobProgress && jobProgress.total_lines > 0
      ? Math.round((jobProgress.acknowledged_lines / jobProgress.total_lines) * 100)
      : 0;
  const jobLabel =
    jobProgress?.state === 'preparing'
      ? t('status.job.preparing', { percent: jobPercent })
      : jobProgress?.state === 'running'
        ? t('status.job.running', { percent: jobPercent })
        : jobProgress?.state === 'paused'
          ? t('status.job.paused')
          : null;

  const toolHintKey = activeTool === 'node'
    ? NODE_SUBMODE_HINT_KEYS[nodeSubMode] ?? TOOL_HINT_KEYS.node
    : activeTool === 'warp' && meshDeformMode === 'mesh'
      ? 'status.tool_hint.deform_selection'
    : TOOL_HINT_KEYS[activeTool] ?? '';
  const toolHint = activeTool === 'text' && textBoxModeActive
    ? t('panels.text_properties.creation_hint')
    : toolHintKey ? t(toolHintKey) : '';

  return (
    <div className="no-select flex items-center justify-between h-6 bg-bb-panel px-3 text-xs text-bb-text-muted border-t border-bb-border">
      {/* Left: transient job state + contextual tool hint. Persistent machine
          and project identity live in the main toolbar. */}
      <span className="flex items-center gap-2">
        {jobLabel && (
          <span className="text-bb-accent">{jobLabel}</span>
        )}
        {toolHint && (
          <>
            {jobLabel ? <span className="w-px h-3 bg-bb-border mx-1" /> : null}
            <span className="text-bb-text-dim italic">{toolHint}</span>
          </>
        )}
      </span>

      {/* Center: cursor position + node info + selection bounds */}
      <span className="font-mono flex items-center gap-3">
        {activeTool === 'node' && nodeEditNodeCount > 0 ? (
          <span className="text-bb-accent">
            {t('status.nodes', { count: nodeEditNodeCount })}
          </span>
        ) : null}
        {cursorDisplayPos ? (
          <span>
            {t('status.cursor_position', {
              x: formatPos(cursorDisplayPos.x),
              y: formatPos(cursorDisplayPos.y),
              unit: unitLabel,
            })}
          </span>
        ) : (
          <span>{'\u00A0'}</span>
        )}
        {measurementStatus ? (
          <span data-testid="measurement-status" className="text-bb-accent">
            {measurementStatus}
          </span>
        ) : selectionBounds ? (
          <span data-testid="selection-bounds" className="text-bb-text-dim">
            {t('status.selection_bounds', {
              minX: formatPos(selectionBounds.minX),
              minY: formatPos(selectionBounds.minY),
              maxX: formatPos(selectionBounds.maxX),
              maxY: formatPos(selectionBounds.maxY),
              count: selectedObjectIds.length,
            })}
          </span>
        ) : null}
      </span>

      {/* Right: zoom + build identity. Grid and Snap live in the main toolbar. */}
      <span className="flex items-center gap-2">
        <button onClick={zoomOut} className="hover:text-bb-text px-0.5" title={t('status.zoom_out')}>
          -
        </button>
        <button
          onClick={handleZoomToFit}
          className="hover:text-bb-text min-w-[3rem] text-center"
          title={t('status.zoom_to_fit')}
        >
          {zoom}%
        </button>
        <button onClick={zoomIn} className="hover:text-bb-text px-0.5" title={t('status.zoom_in')}>
          +
        </button>
        {status?.version ? (
          <>
            <span className="w-px h-3 bg-bb-border mx-1" />
            <span>{t('status.version', { version: status.version })}</span>
          </>
        ) : null}
      </span>
    </div>
  );
}
