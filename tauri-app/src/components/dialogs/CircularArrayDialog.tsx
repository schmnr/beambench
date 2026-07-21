import { useState, useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { notifyObjectLocked } from '../../utils/transformLocks';
import { useAppStore } from '../../stores/appStore';
import { NumberInput } from '../shared/NumberInput';
import { Toggle } from '../shared/Toggle';
import { mmToDisplay, displayToMm, roundDisplayLength, lengthStep, lengthUnitLabel, labelWithUnit } from '../../utils/lengthUnits';
import { useFocusTrap } from '../../hooks/useFocusTrap';

type CenterMode = 'auto' | 'chooseObject' | 'explicit';

interface CircularArrayDialogProps {
  objectIds: string[];
  onClose: () => void;
}

export function CircularArrayDialog({ objectIds, onClose }: CircularArrayDialogProps) {
  const { t } = useTranslation();
  const project = useProjectStore((s) => s.project);
  const projectId = project?.metadata.project_id ?? null;
  const initialProjectIdRef = useRef(projectId);
  const displayUnit = useAppStore((s) => s.settings?.display_unit) ?? 'mm';
  const dialogRef = useRef<HTMLDivElement>(null);
  useFocusTrap(dialogRef, true);

  const [count, setCount] = useState(6);
  const [radius, setRadius] = useState(50);
  const [rotateCopies, setRotateCopies] = useState(true);
  const [centerMode, setCenterMode] = useState<CenterMode>('auto');
  const [centerObjectId, setCenterObjectId] = useState(objectIds.length >= 2 ? objectIds[objectIds.length - 1] : '');
  const [explicitCX, setExplicitCX] = useState(0);
  const [explicitCY, setExplicitCY] = useState(0);
  const [startAngle, setStartAngle] = useState(0);
  const [endAngle, setEndAngle] = useState(360);
  const [groupResults, setGroupResults] = useState(false);
  const [createVirtual, setCreateVirtual] = useState(false);

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

  const isPartialArc = Math.abs(endAngle - startAngle) < 359.9;
  const effectiveCount = count > 1 ? count : 2;
  const stepAngle = isPartialArc
    ? (endAngle - startAngle) / (effectiveCount - 1)
    : (endAngle - startAngle) / effectiveCount;

  const handleSubmit = async () => {
    const objects = useProjectStore.getState().project?.objects ?? [];
    const currentProject = useProjectStore.getState().project;
    const currentProjectId = currentProject?.metadata.project_id ?? null;
    if (currentProjectId !== initialProjectIdRef.current) {
      useNotificationStore.getState().push(t('dialog.circular_array.error_project_changed'), 'warning');
      onClose();
      return;
    }
    if (currentProject && objectIds.some((id) => !currentProject.objects.some((object) => object.id === id))) {
      useNotificationStore.getState().push(t('dialog.circular_array.error_objects_unavailable'), 'warning');
      onClose();
      return;
    }
    if (objectIds.some((id) => objects.find((o) => o.id === id)?.locked)) {
      notifyObjectLocked();
      return;
    }
    try {
      await useProjectStore.getState().circularArray({
        objectIds,
        count,
        radiusMm: radius,
        rotateCopies,
        centerObjectId: centerMode === 'chooseObject' && centerObjectId ? centerObjectId : undefined,
        centerX: centerMode === 'explicit' ? explicitCX : undefined,
        centerY: centerMode === 'explicit' ? explicitCY : undefined,
        startAngleDeg: startAngle,
        endAngleDeg: endAngle,
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
      <div className="bg-bb-panel border border-bb-border rounded-lg shadow-xl p-4 min-w-[340px] max-h-[80vh] overflow-y-auto">
        <h2 id="dialog-title" className="text-sm font-semibold text-bb-text mb-3">{t('dialog.circular_array.title')}</h2>

        {/* Array */}
        <div className="text-xs text-bb-text-muted font-medium mb-1">{t('dialog.circular_array.section_array')}</div>
        <div className="space-y-2 mb-3">
          <NumberInput label={t('dialog.circular_array.count')} value={count} onChange={setCount} min={2} max={100} />
          <NumberInput label={labelWithUnit(t('dialog.circular_array.radius'), lengthUnitLabel(displayUnit))} value={roundDisplayLength(mmToDisplay(radius, displayUnit), displayUnit)} onChange={(v) => setRadius(displayToMm(v, displayUnit))} min={mmToDisplay(0.1, displayUnit)} step={lengthStep(displayUnit, 1, 0.05)} />
        </div>

        {/* Center */}
        <div className="text-xs text-bb-text-muted font-medium mb-1">{t('dialog.circular_array.section_center')}</div>
        <div className="space-y-2 mb-3">
          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="text-bb-text-muted">{t('dialog.circular_array.center_mode')}</span>
            <select
              value={centerMode}
              onChange={(e) => setCenterMode(e.target.value as CenterMode)}
              className="bg-bb-bg border border-bb-border rounded px-2 py-0.5 text-xs text-bb-text"
            >
              <option value="auto">{t('dialog.circular_array.center_mode_auto')}</option>
              <option value="chooseObject" disabled={objectIds.length < 2}>{t('dialog.circular_array.center_mode_object')}</option>
              <option value="explicit">{t('dialog.circular_array.center_mode_explicit')}</option>
            </select>
          </div>
          {centerMode === 'chooseObject' && objectIds.length >= 2 && (
            <div className="flex items-center justify-between gap-2 text-xs">
              <span className="text-bb-text-muted">{t('dialog.circular_array.center_object')}</span>
              <select
                value={centerObjectId}
                onChange={(e) => setCenterObjectId(e.target.value)}
                className="bg-bb-bg border border-bb-border rounded px-2 py-0.5 text-xs text-bb-text"
                data-testid="center-object-select"
              >
                {objectIds.map((id) => {
                  const obj = (project?.objects ?? []).find((o) => o.id === id);
                  return <option key={id} value={id}>{obj?.name ?? id}</option>;
                })}
              </select>
            </div>
          )}
          {centerMode === 'explicit' && (
            <>
              <NumberInput label={labelWithUnit(t('dialog.circular_array.center_x'), lengthUnitLabel(displayUnit))} value={roundDisplayLength(mmToDisplay(explicitCX, displayUnit), displayUnit)} onChange={(v) => setExplicitCX(displayToMm(v, displayUnit))} step={lengthStep(displayUnit, 1, 0.05)} />
              <NumberInput label={labelWithUnit(t('dialog.circular_array.center_y'), lengthUnitLabel(displayUnit))} value={roundDisplayLength(mmToDisplay(explicitCY, displayUnit), displayUnit)} onChange={(v) => setExplicitCY(displayToMm(v, displayUnit))} step={lengthStep(displayUnit, 1, 0.05)} />
            </>
          )}
        </div>

        {/* Angles */}
        <div className="text-xs text-bb-text-muted font-medium mb-1">{t('dialog.circular_array.section_angles')}</div>
        <div className="space-y-2 mb-3">
          <NumberInput label={t('dialog.circular_array.start_angle')} value={startAngle} onChange={setStartAngle} min={-360} max={360} step={15} />
          <NumberInput label={t('dialog.circular_array.end_angle')} value={endAngle} onChange={setEndAngle} min={-360} max={720} step={15} />
          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="text-bb-text-muted">{t('dialog.circular_array.step_angle')}</span>
            <span className="text-bb-text">{stepAngle.toFixed(1)}&deg;</span>
          </div>
          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="text-bb-text-muted">{t('dialog.circular_array.rotate_copies')}</span>
            <Toggle checked={rotateCopies} onChange={setRotateCopies} />
          </div>
        </div>

        {/* Options */}
        <div className="text-xs text-bb-text-muted font-medium mb-1">{t('dialog.circular_array.section_options')}</div>
        <div className="space-y-2 mb-3">
          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="text-bb-text-muted">{t('dialog.circular_array.group_results')}</span>
            <Toggle checked={groupResults} onChange={setGroupResults} />
          </div>
          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="text-bb-text-muted">{t('dialog.circular_array.create_virtual')}</span>
            <Toggle checked={createVirtual} onChange={setCreateVirtual} />
          </div>
        </div>

        <div className="flex justify-end gap-2 mt-4">
          <button onClick={onClose} className="px-3 py-1 text-xs font-medium rounded bg-bb-bg hover:bg-bb-hover text-bb-text">{t('common.cancel')}</button>
          <button data-testid="circular-array-submit" onClick={() => void handleSubmit()} className="px-3 py-1 text-xs font-medium rounded bg-bb-accent hover:bg-bb-accent-hover text-bb-on-accent">{t('common.apply')}</button>
        </div>
      </div>
    </div>,
    document.body
  );
}
