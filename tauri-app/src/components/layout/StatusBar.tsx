import { useTranslation } from 'react-i18next';
import { useAppStore } from '../../stores/appStore';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore, type NodeSubMode, type ToolType } from '../../stores/uiStore';
import { useMachineStore } from '../../stores/machineStore';
import { useMeasurementStore } from '../../stores/measurementStore';
import { zoomToFitBounds } from '../../canvas/ViewportTransform';
import { getCanvasViewportSize } from '../../canvas/canvasViewportRegistry';
import type { TransformLocks } from '../../types/project';
import { canvasToMachinePoint } from '../../utils/workspaceCoordinates';

const CONNECTION_COLORS: Record<string, string> = {
  disconnected: 'bg-gray-500',
  connecting: 'bg-yellow-500',
  ready: 'bg-green-500',
  alarm: 'bg-red-500',
};

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
  warp_selection: 'status.tool_hint.warp_selection',
  deform_selection: 'status.tool_hint.deform_selection',
};

const NODE_SUBMODE_HINT_KEYS: Partial<Record<NodeSubMode, string>> = {
  trim: 'status.tool_hint.node_trim',
};

const transformToggleKeys: { key: keyof TransformLocks; labelKey: string }[] = [
  { key: 'move_enabled', labelKey: 'toolbars.transform_toggles.move' },
  { key: 'size_enabled', labelKey: 'toolbars.transform_toggles.size' },
  { key: 'rotate_enabled', labelKey: 'toolbars.transform_toggles.rotate' },
  { key: 'shear_enabled', labelKey: 'toolbars.transform_toggles.shear' },
];

export function StatusBar() {
  const { t } = useTranslation();
  const status = useAppStore((s) => s.status);
  const settings = useAppStore((s) => s.settings);
  const zoom = useUiStore((s) => s.zoom);
  const zoomIn = useUiStore((s) => s.zoomIn);
  const zoomOut = useUiStore((s) => s.zoomOut);
  const zoomToFit = useUiStore((s) => s.zoomToFit);
  const cursorWorldPos = useUiStore((s) => s.cursorWorldPos);
  const gridVisible = useUiStore((s) => s.gridVisible);
  const snapToGrid = useUiStore((s) => s.snapToGrid);
  const toggleGrid = useUiStore((s) => s.toggleGrid);
  const toggleSnap = useUiStore((s) => s.toggleSnap);
  const activeTool = useUiStore((s) => s.activeTool);
  const nodeSubMode = useUiStore((s) => s.nodeSubMode);
  const nodeEditNodeCount = useUiStore((s) => s.nodeEditNodeCount);
  const project = useProjectStore((s) => s.project);
  const setTransformLocks = useProjectStore((s) => s.setTransformLocks);

  const selectedObjectIds = useProjectStore((s) => s.selectedObjectIds);
  const measurement = useMeasurementStore((s) => s.state);

  const sessionState = useMachineStore((s) => s.sessionState);
  const jobProgress = useMachineStore((s) => s.jobProgress);

  const unit = settings?.display_unit ?? 'mm';
  const unitLabel = unit === 'inches' ? 'in' : 'mm';
  // TransformLocks is non-optional on every field. Default matches
  // the backend `Default` impl (all enabled).
  const locks: TransformLocks = project?.transform_locks ?? {
    move_enabled: true,
    size_enabled: true,
    rotate_enabled: true,
    shear_enabled: true,
  };

  const handleToggle = (key: keyof TransformLocks) => {
    void setTransformLocks({ ...locks, [key]: !locks[key] });
  };

  const isLocked = (key: keyof TransformLocks) => locks[key] === false;

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
    if (measurement.type === 'drag') {
      return `dx: ${formatPos(measurement.dxMm)} ${unitLabel}  dy: ${formatPos(measurement.dyMm)} ${unitLabel}  len: ${formatPos(measurement.lengthMm)} ${unitLabel}  angle: ${measurement.angleDeg.toFixed(1)}°`;
    }
    if (measurement.type === 'hover') {
      const objectSummary = `w: ${formatPos(measurement.objectMetrics.widthMm)} ${unitLabel}  h: ${formatPos(measurement.objectMetrics.heightMm)} ${unitLabel}  area: ${formatArea(measurement.objectMetrics.areaMm2)}`;
      if (measurement.segment) {
        return `${objectSummary}  seg: ${formatPos(measurement.segment.lengthMm)} ${unitLabel} @ ${measurement.segment.angleDeg.toFixed(1)}°`;
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

  const normalizedSessionState = sessionState ?? 'disconnected';
  const connectionColor = CONNECTION_COLORS[normalizedSessionState] ?? 'bg-gray-500';
  const connectionLabel = t(`status.connection.${normalizedSessionState}`, {
    defaultValue:
      normalizedSessionState.charAt(0).toUpperCase() + normalizedSessionState.slice(1),
  });

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
    : TOOL_HINT_KEYS[activeTool] ?? '';
  const toolHint = toolHintKey ? t(toolHintKey) : '';

  return (
    <div className="no-select flex items-center justify-between h-6 bg-bb-panel px-3 text-xs text-bb-text-muted border-t border-bb-border">
      {/* Left: transform toggles + modes + machine state + tool hint */}
      <span className="flex items-center gap-2">
        {/* Transform toggles (absorbed from TransformToggles) */}
        {transformToggleKeys.map(({ key, labelKey }) => (
          <button
            key={key}
            onClick={() => handleToggle(key)}
            className={`px-1 py-0 rounded text-xs ${
              isLocked(key)
                ? 'bg-bb-accent/15 border border-bb-accent/30 text-bb-text'
                : 'text-bb-text-muted hover:text-bb-text hover:bg-bb-surface'
            }`}
          >
            {t(labelKey)}
          </button>
        ))}
        <span className="w-px h-3 bg-bb-border mx-1" />
        {/* Modes, disabled until implemented */}
        <button
          disabled
          title={t('status.rotary_tooltip')}
          className="px-1 py-0 rounded text-xs text-bb-text-disabled cursor-not-allowed"
        >
          {t('status.rotary')}
        </button>
        <button
          disabled
          title={t('status.print_cut_tooltip')}
          className="px-1 py-0 rounded text-xs text-bb-text-disabled cursor-not-allowed"
        >
          {t('status.print_cut')}
        </button>
        <span className="w-px h-3 bg-bb-border mx-1" />
        <span className="flex items-center gap-1.5">
          <span className={`w-2 h-2 rounded-full ${connectionColor}`} />
          <span>{connectionLabel}</span>
        </span>
        {jobLabel && (
          <>
            <span className="w-px h-3 bg-bb-border mx-1" />
            <span className="text-bb-accent">{jobLabel}</span>
          </>
        )}
        <span className="w-px h-3 bg-bb-border mx-1" />
        <span>
          {project?.metadata.project_name ?? status?.state ?? t('status.initializing')}
          {project?.dirty ? <span className="text-bb-accent ml-1" title={t('status.unsaved_changes')}>*</span> : null}
        </span>
        {toolHint && (
          <>
            <span className="w-px h-3 bg-bb-border mx-1" />
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

      {/* Right: grid/snap indicators + zoom controls */}
      <span className="flex items-center gap-2">
        <button
          onClick={toggleGrid}
          className={`px-1 rounded ${gridVisible ? 'text-bb-text' : 'text-bb-text-dim'} hover:text-bb-text`}
          title={t('status.grid_tooltip')}
        >
          {t('status.grid')}
        </button>
        <button
          onClick={toggleSnap}
          className={`px-1 rounded ${snapToGrid ? 'text-bb-text' : 'text-bb-text-dim'} hover:text-bb-text`}
          title={t('status.snap_tooltip')}
        >
          {t('status.snap')}
        </button>
        <span className="w-px h-3 bg-bb-border mx-1" />
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
