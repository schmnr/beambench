import { useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { useAppStore } from '../../stores/appStore';
import { notifyObjectLocked } from '../../utils/transformLocks';
import { NumberInput } from '../shared/NumberInput';
import { Toggle } from '../shared/Toggle';
import { mmToDisplay, displayToMm, roundDisplayLength, lengthStep, lengthUnitLabel, labelWithUnit } from '../../utils/lengthUnits';
import type { GridArraySizingMode, GridSpacingMode } from '../../types/vector';
import { computeGridArrayFootprint, fitGridArrayCounts } from '../../utils/gridArraySizing';
import { useFocusTrap } from '../../hooks/useFocusTrap';

interface GridArrayDialogProps {
  objectIds: string[];
  onClose: () => void;
}

export function GridArrayDialog({ objectIds, onClose }: GridArrayDialogProps) {
  const { t } = useTranslation();
  const project = useProjectStore((s) => s.project);
  const projectId = project?.metadata.project_id ?? null;
  const selectedObjects = useMemo(
    () => (project?.objects ?? []).filter((object) => objectIds.includes(object.id)),
    [objectIds, project?.objects],
  );
  const initialProjectIdRef = useRef(projectId);
  const displayUnit = useAppStore((s) => s.settings?.display_unit) ?? 'mm';
  const dialogRef = useRef<HTMLDivElement>(null);
  useFocusTrap(dialogRef, true);

  const [rows, setRows] = useState(2);
  const [cols, setCols] = useState(2);
  const [sizingModeX, setSizingModeX] = useState<GridArraySizingMode>('count');
  const [sizingModeY, setSizingModeY] = useState<GridArraySizingMode>('count');
  const [totalWidth, setTotalWidth] = useState(25);
  const [totalHeight, setTotalHeight] = useState(25);
  const [hSpacing, setHSpacing] = useState(5);
  const [vSpacing, setVSpacing] = useState(5);
  const [spacingMode, setSpacingMode] = useState<GridSpacingMode>('centerToCenter');
  const [mirrorAlternateCols, setMirrorAlternateCols] = useState(false);
  const [mirrorAlternateRows, setMirrorAlternateRows] = useState(false);
  const [xColShift, setXColShift] = useState(0);
  const [yRowShift, setYRowShift] = useState(0);
  const [halfShift, setHalfShift] = useState(false);
  const [reverseH, setReverseH] = useState(false);
  const [reverseV, setReverseV] = useState(false);
  const [randomOrientation, setRandomOrientation] = useState(false);
  const [randomSeed, setRandomSeed] = useState(42);
  const [groupResults, setGroupResults] = useState(false);
  const [createVirtual, setCreateVirtual] = useState(false);

  const params = useMemo(() => ({
    rows,
    cols,
    sizingModeX,
    sizingModeY,
    totalWidthMm: totalWidth,
    totalHeightMm: totalHeight,
    hSpacingMm: hSpacing,
    vSpacingMm: vSpacing,
    spacingMode,
    xColShiftMm: xColShift,
    yRowShiftMm: yRowShift,
    halfShift,
    reverseH,
    reverseV,
  }), [
    rows,
    cols,
    sizingModeX,
    sizingModeY,
    totalWidth,
    totalHeight,
    hSpacing,
    vSpacing,
    spacingMode,
    xColShift,
    yRowShift,
    halfShift,
    reverseH,
    reverseV,
  ]);

  const fittedCounts = useMemo(
    () => fitGridArrayCounts(selectedObjects, params),
    [params, selectedObjects],
  );
  const effectiveRows = sizingModeY === 'total' ? fittedCounts.rows : rows;
  const effectiveCols = sizingModeX === 'total' ? fittedCounts.cols : cols;
  const footprint = useMemo(
    () => computeGridArrayFootprint(selectedObjects, {
      ...params,
      rows: effectiveRows,
      cols: effectiveCols,
    }),
    [effectiveCols, effectiveRows, params, selectedObjects],
  );

  useEffect(() => {
    if (selectedObjects.length > 0) {
      setTotalWidth((current) => (current > 0 ? current : footprint.width || 1));
      setTotalHeight((current) => (current > 0 ? current : footprint.height || 1));
    }
  }, [footprint.height, footprint.width, selectedObjects.length]);

  useEffect(() => {
    if (sizingModeX === 'count') {
      setTotalWidth(Number(footprint.width.toFixed(2)));
    }
  }, [footprint.width, sizingModeX]);

  useEffect(() => {
    if (sizingModeY === 'count') {
      setTotalHeight(Number(footprint.height.toFixed(2)));
    }
  }, [footprint.height, sizingModeY]);

  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [onClose]);

  useEffect(() => {
    if (projectId !== initialProjectIdRef.current) {
      onClose();
    }
  }, [projectId, onClose]);

  const isValid = rows >= 1
    && cols >= 1
    && (sizingModeX === 'count' || totalWidth > 0)
    && (sizingModeY === 'count' || totalHeight > 0);

  const handleSubmit = async () => {
    const objects = useProjectStore.getState().project?.objects ?? [];
    const currentProject = useProjectStore.getState().project;
    const currentProjectId = currentProject?.metadata.project_id ?? null;
    if (currentProjectId !== initialProjectIdRef.current) {
      useNotificationStore.getState().push(t('dialog.grid_array.error_project_changed'), 'warning');
      onClose();
      return;
    }
    if (currentProject && objectIds.some((id) => !currentProject.objects.some((object) => object.id === id))) {
      useNotificationStore.getState().push(t('dialog.grid_array.error_objects_unavailable'), 'warning');
      onClose();
      return;
    }
    if (objectIds.some((id) => objects.find((o) => o.id === id)?.locked)) {
      notifyObjectLocked();
      return;
    }
    if (!isValid) {
      return;
    }
    try {
      await useProjectStore.getState().gridArray({
        objectIds,
        rows,
        cols,
        sizingModeX,
        sizingModeY,
        totalWidthMm: sizingModeX === 'total' ? totalWidth : undefined,
        totalHeightMm: sizingModeY === 'total' ? totalHeight : undefined,
        hSpacingMm: hSpacing,
        vSpacingMm: vSpacing,
        spacingMode,
        mirrorAlternateCols,
        mirrorAlternateRows,
        xColShiftMm: xColShift,
        yRowShiftMm: yRowShift,
        halfShift,
        reverseH,
        reverseV,
        randomOrientation,
        randomSeed,
        groupResults,
        createVirtual,
        autoIncrementText: false,
        textIncrement: 1,
      });
      onClose();
    } catch {
      // Store already showed notification — keep dialog open for correction
    }
  };

  return createPortal(
    <div ref={dialogRef} role="dialog" aria-modal="true" aria-labelledby="dialog-title" className="fixed inset-0 bg-black/50 flex items-center justify-center z-50" onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="bg-bb-panel border border-bb-border rounded-lg shadow-xl p-4 min-w-[360px] max-h-[80vh] overflow-y-auto">
        <h2 id="dialog-title" className="text-sm font-semibold text-bb-text mb-3">{t('dialog.grid_array.title')}</h2>

        <div className="text-xs text-bb-text-muted font-medium mb-1">{t('dialog.grid_array.section_grid_size')}</div>
        <div className="space-y-2 mb-3">
          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="text-bb-text-muted">{t('dialog.grid_array.x_axis_mode')}</span>
            <select
              value={sizingModeX}
              onChange={(event) => setSizingModeX(event.target.value as GridArraySizingMode)}
              className="bg-bb-bg border border-bb-border rounded px-2 py-0.5 text-xs text-bb-text"
            >
              <option value="count">{t('dialog.grid_array.mode_count')}</option>
              <option value="total">{t('dialog.grid_array.mode_total_width')}</option>
            </select>
          </div>
          <NumberInput label={t('dialog.grid_array.columns')} value={effectiveCols} onChange={setCols} min={1} max={100} disabled={sizingModeX === 'total'} />
          <NumberInput label={labelWithUnit(t('dialog.grid_array.total_width'), lengthUnitLabel(displayUnit))} value={roundDisplayLength(mmToDisplay(totalWidth, displayUnit), displayUnit)} onChange={(v) => setTotalWidth(displayToMm(v, displayUnit))} min={mmToDisplay(0.01, displayUnit)} step={lengthStep(displayUnit, 0.5, 0.02)} disabled={sizingModeX === 'count'} />

          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="text-bb-text-muted">{t('dialog.grid_array.y_axis_mode')}</span>
            <select
              value={sizingModeY}
              onChange={(event) => setSizingModeY(event.target.value as GridArraySizingMode)}
              className="bg-bb-bg border border-bb-border rounded px-2 py-0.5 text-xs text-bb-text"
            >
              <option value="count">{t('dialog.grid_array.mode_count')}</option>
              <option value="total">{t('dialog.grid_array.mode_total_height')}</option>
            </select>
          </div>
          <NumberInput label={t('dialog.grid_array.rows')} value={effectiveRows} onChange={setRows} min={1} max={100} disabled={sizingModeY === 'total'} />
          <NumberInput label={labelWithUnit(t('dialog.grid_array.total_height'), lengthUnitLabel(displayUnit))} value={roundDisplayLength(mmToDisplay(totalHeight, displayUnit), displayUnit)} onChange={(v) => setTotalHeight(displayToMm(v, displayUnit))} min={mmToDisplay(0.01, displayUnit)} step={lengthStep(displayUnit, 0.5, 0.02)} disabled={sizingModeY === 'count'} />
        </div>

        <div className="text-xs text-bb-text-muted font-medium mb-1">{t('dialog.grid_array.section_spacing')}</div>
        <div className="space-y-2 mb-3">
          <NumberInput label={labelWithUnit(t('dialog.grid_array.h_spacing'), lengthUnitLabel(displayUnit))} value={roundDisplayLength(mmToDisplay(hSpacing, displayUnit), displayUnit)} onChange={(v) => setHSpacing(displayToMm(v, displayUnit))} min={0} step={lengthStep(displayUnit, 0.5, 0.02)} />
          <NumberInput label={labelWithUnit(t('dialog.grid_array.v_spacing'), lengthUnitLabel(displayUnit))} value={roundDisplayLength(mmToDisplay(vSpacing, displayUnit), displayUnit)} onChange={(v) => setVSpacing(displayToMm(v, displayUnit))} min={0} step={lengthStep(displayUnit, 0.5, 0.02)} />
          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="text-bb-text-muted">{t('dialog.grid_array.spacing_mode')}</span>
            <select
              value={spacingMode}
              onChange={(e) => setSpacingMode(e.target.value as GridSpacingMode)}
              className="bg-bb-bg border border-bb-border rounded px-2 py-0.5 text-xs text-bb-text"
            >
              <option value="centerToCenter">{t('dialog.grid_array.spacing_center')}</option>
              <option value="edgeToEdge">{t('dialog.grid_array.spacing_edge')}</option>
            </select>
          </div>
        </div>

        <div className="mb-3 rounded border border-bb-border bg-bb-bg px-2 py-1 text-xs text-bb-text-muted">
          {t('dialog.grid_array.footprint', { width: roundDisplayLength(mmToDisplay(footprint.width, displayUnit), displayUnit), height: roundDisplayLength(mmToDisplay(footprint.height, displayUnit), displayUnit) })} {lengthUnitLabel(displayUnit)}
        </div>

        <div className="text-xs text-bb-text-muted font-medium mb-1">{t('dialog.grid_array.section_layout')}</div>
        <div className="space-y-2 mb-3">
          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="text-bb-text-muted">{t('dialog.grid_array.mirror_cols')}</span>
            <Toggle checked={mirrorAlternateCols} onChange={setMirrorAlternateCols} />
          </div>
          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="text-bb-text-muted">{t('dialog.grid_array.mirror_rows')}</span>
            <Toggle checked={mirrorAlternateRows} onChange={setMirrorAlternateRows} />
          </div>
          <NumberInput label={labelWithUnit(t('dialog.grid_array.x_col_shift'), lengthUnitLabel(displayUnit))} value={roundDisplayLength(mmToDisplay(xColShift, displayUnit), displayUnit)} onChange={(v) => setXColShift(displayToMm(v, displayUnit))} step={lengthStep(displayUnit, 0.5, 0.02)} />
          <NumberInput label={labelWithUnit(t('dialog.grid_array.y_row_shift'), lengthUnitLabel(displayUnit))} value={roundDisplayLength(mmToDisplay(yRowShift, displayUnit), displayUnit)} onChange={(v) => setYRowShift(displayToMm(v, displayUnit))} step={lengthStep(displayUnit, 0.5, 0.02)} />
          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="text-bb-text-muted">{t('dialog.grid_array.half_shift')}</span>
            <Toggle checked={halfShift} onChange={setHalfShift} />
          </div>
          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="text-bb-text-muted">{t('dialog.grid_array.reverse_h')}</span>
            <Toggle checked={reverseH} onChange={setReverseH} />
          </div>
          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="text-bb-text-muted">{t('dialog.grid_array.reverse_v')}</span>
            <Toggle checked={reverseV} onChange={setReverseV} />
          </div>
        </div>

        <div className="text-xs text-bb-text-muted font-medium mb-1">{t('dialog.grid_array.section_options')}</div>
        <div className="space-y-2 mb-3">
          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="text-bb-text-muted">{t('dialog.grid_array.random_orientation')}</span>
            <Toggle checked={randomOrientation} onChange={setRandomOrientation} />
          </div>
          {randomOrientation && (
            <NumberInput label={t('dialog.grid_array.random_seed')} value={randomSeed} onChange={setRandomSeed} min={0} />
          )}
          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="text-bb-text-muted">{t('dialog.grid_array.group_results')}</span>
            <Toggle checked={groupResults} onChange={setGroupResults} />
          </div>
          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="text-bb-text-muted">{t('dialog.grid_array.create_virtual')}</span>
            <Toggle checked={createVirtual} onChange={setCreateVirtual} />
          </div>
        </div>

        {!isValid && (
          <div className="mb-3 rounded border border-bb-warning-border bg-bb-warning-bg px-2 py-1 text-xs text-bb-warning-fg">
            {t('dialog.grid_array.validation_error')}
          </div>
        )}

        <div className="flex justify-end gap-2 mt-4">
          <button onClick={onClose} className="px-3 py-1 text-xs font-medium rounded bg-bb-bg hover:bg-bb-hover text-bb-text">{t('common.cancel')}</button>
          <button data-testid="grid-array-submit" onClick={() => void handleSubmit()} disabled={!isValid} className="px-3 py-1 text-xs font-medium rounded bg-bb-accent hover:bg-bb-accent-hover text-bb-on-accent disabled:opacity-50">{t('common.apply')}</button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
