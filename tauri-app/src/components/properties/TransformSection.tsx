import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { bumpSettingsMutationSeq, useAppStore } from '../../stores/appStore';
import { appService } from '../../services/appService';
import { Lock, Unlock } from 'lucide-react';
import { NumberStepper } from '../shared/NumberStepper';
import type { AnchorPoint } from '../../types/project';
import { useNotificationStore } from '../../stores/notificationStore';
import {
  isTransformLocked,
  notifyTransformLocked,
  notifyObjectLocked,
} from '../../utils/transformLocks';
import { canvasToMachinePoint, machineToCanvasPoint } from '../../utils/workspaceCoordinates';
import { mmToDisplay, displayToMm, roundDisplayLength } from '../../utils/lengthUnits';
import {
  anchorPoints,
  getAnchorOffset,
  textAnchorPoint,
  anchorLabelKeys,
  useBufferedNumericField,
} from '../shared/transformFields';

const DISPLAY_UNIT_MM = 'mm' as const;
const DISPLAY_UNIT_INCHES = 'inches' as const;
type DisplayUnit = typeof DISPLAY_UNIT_MM | typeof DISPLAY_UNIT_INCHES;
const UNIT_LABEL_INCHES = 'in';
const TOAST_ERROR = 'error' as const;

const fieldClass =
  'w-full min-w-0 bg-transparent px-0 text-right text-xs text-bb-text focus:outline-none';

/** Boxed field with the label inside (mockup style: [X  6.34  mm]). */
function FieldBox({
  label,
  suffix,
  children,
}: {
  label: string;
  suffix?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="flex h-7 min-w-0 flex-1 items-center gap-1 rounded-lg border border-bb-border bg-bb-bg px-1.5 focus-within:border-bb-accent">
      <span className="min-w-[0.875rem] shrink-0 text-[9px] font-semibold uppercase text-bb-text-dim">{label}</span>
      {children}
      {suffix && <span className="shrink-0 pl-0.5 text-[9px] text-bb-text-dim">{suffix}</span>}
    </label>
  );
}

/**
 * Sectioned Transform block for the Properties panel: X/Y position, W/H size
 * with aspect lock, scale %, rotation, anchor grid, and unit toggle.
 *
 * The math and guard behavior mirrors PropertiesToolbar's numeric section;
 * that toolbar is scheduled for retirement, at which point this becomes the
 * only copy.
 */
export function TransformSection() {
  const { t } = useTranslation();
  const project = useProjectStore((s) => s.project);
  const selectedObjectIds = useProjectStore((s) => s.selectedObjectIds);
  const updateObject = useProjectStore((s) => s.updateObject);
  const updateObjectBoundsBatch = useProjectStore((s) => s.updateObjectBoundsBatch);
  const rotateObjects = useProjectStore((s) => s.rotateObjects);
  const nudgeObjects = useProjectStore((s) => s.nudgeObjects);

  const lockAspect = useUiStore((s) => s.lockAspect);
  const toggleLockAspect = useUiStore((s) => s.toggleLockAspect);

  const settings = useAppStore((s) => s.settings);
  const displayUnit = (settings?.display_unit === DISPLAY_UNIT_INCHES
    ? DISPLAY_UNIT_INCHES
    : DISPLAY_UNIT_MM) as DisplayUnit;
  const unitLabel = displayUnit === DISPLAY_UNIT_MM ? DISPLAY_UNIT_MM : UNIT_LABEL_INCHES;
  const posStep = displayUnit === DISPLAY_UNIT_INCHES ? 0.005 : 0.1;
  const sizeMin = displayUnit === DISPLAY_UNIT_INCHES ? 0.001 : 0.01;

  const [anchor, setAnchor] = useState<AnchorPoint>('top_left');
  const [scaleXPercent, setScaleXPercent] = useState(100);
  const [scaleYPercent, setScaleYPercent] = useState(100);

  const hasSelection = selectedObjectIds.length > 0;
  const multiSel = selectedObjectIds.length > 1;
  const selectedObjects = hasSelection
    ? project?.objects.filter((o) => selectedObjectIds.includes(o.id)) ?? []
    : [];
  const obj = selectedObjects.length > 0 ? selectedObjects[0] : undefined;

  const selBounds = (() => {
    if (selectedObjects.length === 0) return undefined;
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const o of selectedObjects) {
      minX = Math.min(minX, o.bounds.min.x);
      minY = Math.min(minY, o.bounds.min.y);
      maxX = Math.max(maxX, o.bounds.max.x);
      maxY = Math.max(maxY, o.bounds.max.y);
    }
    return { min: { x: minX, y: minY }, max: { x: maxX, y: maxY } };
  })();

  const b = selBounds;
  const w = b ? b.max.x - b.min.x : 0;
  const h = b ? b.max.y - b.min.y : 0;
  const { ax, ay } = getAnchorOffset(anchor, w, h);
  const txtAnchor = !multiSel && obj ? textAnchorPoint(obj) : undefined;
  const canvasAnchorPoint = txtAnchor ?? (b ? { x: b.min.x + ax, y: b.min.y + ay } : { x: 0, y: 0 });
  const displayPoint = project ? canvasToMachinePoint(canvasAnchorPoint, project.workspace) : canvasAnchorPoint;
  const displayX = displayPoint.x;
  const displayY = displayPoint.y;
  const rotationDeg = !multiSel && obj
    ? Math.round(Math.atan2(obj.transform.b, obj.transform.a) * (180 / Math.PI) * 10) / 10
    : 0;

  const locks = project?.transform_locks;
  const selectionKey = selectedObjectIds.join(',');

  useEffect(() => {
    setScaleXPercent(100);
    setScaleYPercent(100);
  }, [selectionKey, selBounds?.min.x, selBounds?.min.y, selBounds?.max.x, selBounds?.max.y]);

  const col = anchorPoints.indexOf(anchor) % 3;
  const row = Math.floor(anchorPoints.indexOf(anchor) / 3);

  const guardLocked = (): boolean => {
    if (selectedObjects.some((o) => o.locked)) { notifyObjectLocked(); return true; }
    return false;
  };

  const handleXChange = (newDisplayX: number) => {
    if (!b) return;
    if (guardLocked()) return;
    if (isTransformLocked(locks, 'position')) { notifyTransformLocked('position'); return; }
    const newX = displayToMm(newDisplayX, displayUnit);
    const nextCanvasPoint = project
      ? machineToCanvasPoint({ x: newX, y: displayY }, project.workspace)
      : { x: newX, y: displayY };
    const dx = nextCanvasPoint.x - canvasAnchorPoint.x;
    if (multiSel) {
      void nudgeObjects(selectedObjectIds, dx, 0);
    } else if (obj) {
      void updateObject(obj.id, {
        bounds: { min: { x: b.min.x + dx, y: b.min.y }, max: { x: b.max.x + dx, y: b.max.y } },
      });
    }
  };

  const handleYChange = (newDisplayY: number) => {
    if (!b) return;
    if (guardLocked()) return;
    if (isTransformLocked(locks, 'position')) { notifyTransformLocked('position'); return; }
    const newY = displayToMm(newDisplayY, displayUnit);
    const nextCanvasPoint = project
      ? machineToCanvasPoint({ x: displayX, y: newY }, project.workspace)
      : { x: displayX, y: newY };
    const dy = nextCanvasPoint.y - canvasAnchorPoint.y;
    if (multiSel) {
      void nudgeObjects(selectedObjectIds, 0, dy);
    } else if (obj) {
      void updateObject(obj.id, {
        bounds: { min: { x: b.min.x, y: b.min.y + dy }, max: { x: b.max.x, y: b.max.y + dy } },
      });
    }
  };

  const scaleSelection = (sx: number, sy: number) => {
    if (!b || selectedObjects.length === 0) return;
    const anchorX = b.min.x + (col / 2) * w;
    const anchorY = b.min.y + (row / 2) * h;
    const entries = selectedObjects.map((o) => {
      const ob = o.bounds;
      const oMinX = anchorX + (ob.min.x - anchorX) * sx;
      const oMaxX = anchorX + (ob.max.x - anchorX) * sx;
      const oMinY = anchorY + (ob.min.y - anchorY) * sy;
      const oMaxY = anchorY + (ob.max.y - anchorY) * sy;
      return {
        id: o.id,
        bounds: { min: { x: oMinX, y: oMinY }, max: { x: oMaxX, y: oMaxY } },
      };
    });
    void updateObjectBoundsBatch(entries);
  };

  const handleWChange = (newDisplayW: number) => {
    const newW = displayToMm(newDisplayW, displayUnit);
    if (!b || newW <= 0) return;
    if (guardLocked()) return;
    if (isTransformLocked(locks, 'scale')) { notifyTransformLocked('scale'); return; }
    const newH = lockAspect ? (h * newW) / w : h;
    if (multiSel) {
      scaleSelection(newW / w, lockAspect ? newH / h : 1);
    } else if (obj) {
      const anchorX = b.min.x + (col / 2) * w;
      const anchorY = b.min.y + (lockAspect ? (row / 2) * h : ay);
      const newMinX = anchorX - (col / 2) * newW;
      const newMinY = lockAspect ? anchorY - (row / 2) * newH : b.min.y;
      void updateObject(obj.id, {
        bounds: { min: { x: newMinX, y: newMinY }, max: { x: newMinX + newW, y: newMinY + newH } },
      });
    }
  };

  const handleHChange = (newDisplayH: number) => {
    const newH = displayToMm(newDisplayH, displayUnit);
    if (!b || newH <= 0) return;
    if (guardLocked()) return;
    if (isTransformLocked(locks, 'scale')) { notifyTransformLocked('scale'); return; }
    const newW = lockAspect ? (w * newH) / h : w;
    if (multiSel) {
      scaleSelection(lockAspect ? newW / w : 1, newH / h);
    } else if (obj) {
      const anchorX = b.min.x + (lockAspect ? (col / 2) * w : ax);
      const anchorY = b.min.y + (row / 2) * h;
      const newMinX = lockAspect ? anchorX - (col / 2) * newW : b.min.x;
      const newMinY = anchorY - (row / 2) * newH;
      void updateObject(obj.id, {
        bounds: { min: { x: newMinX, y: newMinY }, max: { x: newMinX + newW, y: newMinY + newH } },
      });
    }
  };

  const handleScaleXChange = (pct: number) => {
    if (!b || pct <= 0) return;
    if (guardLocked()) return;
    if (isTransformLocked(locks, 'scale')) { notifyTransformLocked('scale'); return; }
    setScaleXPercent(pct);
    if (lockAspect) setScaleYPercent(pct);
    const factor = pct / 100;
    if (multiSel) {
      scaleSelection(factor, lockAspect ? factor : 1);
    } else if (obj) {
      const newW = w * factor;
      const newH = lockAspect ? h * factor : h;
      const anchorX = b.min.x + (col / 2) * w;
      const anchorY = b.min.y + (row / 2) * h;
      const newMinX = anchorX - (col / 2) * newW;
      const newMinY = lockAspect ? anchorY - (row / 2) * newH : b.min.y;
      void updateObject(obj.id, {
        bounds: { min: { x: newMinX, y: newMinY }, max: { x: newMinX + newW, y: newMinY + newH } },
      });
    }
  };

  const handleScaleYChange = (pct: number) => {
    if (!b || pct <= 0) return;
    if (guardLocked()) return;
    if (isTransformLocked(locks, 'scale')) { notifyTransformLocked('scale'); return; }
    setScaleYPercent(pct);
    if (lockAspect) setScaleXPercent(pct);
    const factor = pct / 100;
    if (multiSel) {
      scaleSelection(lockAspect ? factor : 1, factor);
    } else if (obj) {
      const newW = lockAspect ? w * factor : w;
      const newH = h * factor;
      const anchorX = b.min.x + (col / 2) * w;
      const anchorY = b.min.y + (row / 2) * h;
      const newMinX = lockAspect ? anchorX - (col / 2) * newW : b.min.x;
      const newMinY = anchorY - (row / 2) * newH;
      void updateObject(obj.id, {
        bounds: { min: { x: newMinX, y: newMinY }, max: { x: newMinX + newW, y: newMinY + newH } },
      });
    }
  };

  const handleRotateChange = (deg: number) => {
    if (guardLocked()) return;
    if (isTransformLocked(locks, 'rotation')) { notifyTransformLocked('rotation'); return; }
    void rotateObjects(selectedObjectIds, deg - rotationDeg);
  };

  const toggleUnit = () => {
    const newUnit: DisplayUnit = displayUnit === DISPLAY_UNIT_MM ? DISPLAY_UNIT_INCHES : DISPLAY_UNIT_MM;
    const cur = useAppStore.getState().settings;
    if (!cur) return;
    const oldUnit = cur.display_unit;
    bumpSettingsMutationSeq();
    useAppStore.setState({ settings: { ...cur, display_unit: newUnit } });
    void appService.updateSettings({ display_unit: newUnit }).catch(() => {
      bumpSettingsMutationSeq();
      useAppStore.setState({ settings: { ...useAppStore.getState().settings!, display_unit: oldUnit } });
      useNotificationStore.getState().push(t('toolbars.properties.error_save_display_unit'), TOAST_ERROR);
    });
  };

  const disabled = !hasSelection || !obj;

  const fieldResetKey = `${selectionKey}|${displayUnit}`;
  const xField = useBufferedNumericField(
    disabled ? '' : roundDisplayLength(mmToDisplay(displayX, displayUnit), displayUnit),
    handleXChange,
    fieldResetKey,
  );
  const yField = useBufferedNumericField(
    disabled ? '' : roundDisplayLength(mmToDisplay(displayY, displayUnit), displayUnit),
    handleYChange,
    fieldResetKey,
  );
  const wField = useBufferedNumericField(
    disabled ? '' : roundDisplayLength(mmToDisplay(w, displayUnit), displayUnit),
    handleWChange,
    fieldResetKey,
  );
  const hField = useBufferedNumericField(
    disabled ? '' : roundDisplayLength(mmToDisplay(h, displayUnit), displayUnit),
    handleHChange,
    fieldResetKey,
  );
  const scaleXField = useBufferedNumericField(disabled ? '' : scaleXPercent, handleScaleXChange, fieldResetKey);
  const scaleYField = useBufferedNumericField(disabled ? '' : scaleYPercent, handleScaleYChange, fieldResetKey);
  const rotationField = useBufferedNumericField(disabled ? '' : rotationDeg, handleRotateChange, fieldResetKey);

  if (!hasSelection) return null;

  return (
    <div className="border-b border-bb-border pb-3 mb-1">
      {/* Header: label, anchor grid, unit toggle */}
      <div className="flex items-center justify-between py-2">
        <span className="text-[10px] font-semibold tracking-wider text-bb-text-muted uppercase">
          {t('panels.properties.transform')}
        </span>
        <div className="flex items-center gap-3">
          <div className="grid grid-cols-3 gap-1">
            {anchorPoints.map((ap) => (
              <button
                key={ap}
                onClick={() => setAnchor(ap)}
                disabled={disabled}
                className={`shrink-0 rounded-full ${
                  anchor === ap
                    ? 'bg-bb-accent w-2 h-2'
                    : 'border border-bb-text-dim w-2 h-2 hover:border-bb-text-muted'
                }`}
                title={t(anchorLabelKeys[ap])}
              />
            ))}
          </div>
          <button
            onClick={toggleUnit}
            className="flex items-center justify-center px-1.5 rounded-lg text-xs font-medium border border-bb-border bg-bb-bg text-bb-text hover:bg-bb-hover h-6 min-w-[2rem]"
            title={t('toolbars.properties.switch_to_unit', {
              unit: displayUnit === DISPLAY_UNIT_MM
                ? t('toolbars.properties.unit_inches')
                : t('toolbars.properties.unit_millimeters'),
            })}
          >
            {unitLabel}
          </button>
        </div>
      </div>

      {/* X / Y */}
      <div className="grid grid-cols-2 gap-1.5">
        <FieldBox label="X" suffix={unitLabel}>
          <NumberStepper {...xField} step={posStep} disabled={disabled} className={fieldClass} containerClassName="min-w-0 flex-1" />
        </FieldBox>
        <FieldBox label="Y" suffix={unitLabel}>
          <NumberStepper {...yField} step={posStep} disabled={disabled} className={fieldClass} containerClassName="min-w-0 flex-1" />
        </FieldBox>
      </div>

      {/* W / lock / H */}
      <div className="mt-1.5 flex items-center gap-1">
        <FieldBox label="W" suffix={unitLabel}>
          <NumberStepper {...wField} step={posStep} min={sizeMin} disabled={disabled} className={fieldClass} containerClassName="min-w-0 flex-1" />
        </FieldBox>
        <button
          onClick={toggleLockAspect}
          disabled={disabled}
          className={`shrink-0 rounded p-0.5 ${
            disabled
              ? 'text-bb-text-dim/40 cursor-not-allowed'
              : lockAspect
                ? 'text-bb-accent hover:text-bb-accent/80'
                : 'text-bb-text-dim hover:text-bb-text-muted'
          }`}
          title={t('toolbars.properties.lock_aspect_ratio')}
        >
          {lockAspect ? <Lock size={14} /> : <Unlock size={14} />}
        </button>
        <FieldBox label="H" suffix={unitLabel}>
          <NumberStepper {...hField} step={posStep} min={sizeMin} disabled={disabled} className={fieldClass} containerClassName="min-w-0 flex-1" />
        </FieldBox>
      </div>

      {/* Scale + Rotation */}
      <div className="mt-1.5 grid grid-cols-2 gap-1.5">
        <FieldBox label="SX" suffix="%">
          <NumberStepper {...scaleXField} step={1} min={1} disabled={disabled} className={fieldClass} containerClassName="min-w-0 flex-1" />
        </FieldBox>
        <FieldBox label="SY" suffix="%">
          <NumberStepper {...scaleYField} step={1} min={1} disabled={disabled} className={fieldClass} containerClassName="min-w-0 flex-1" />
        </FieldBox>
      </div>
      <div className="mt-1.5 grid grid-cols-2 gap-1.5">
        <FieldBox label="⟳" suffix="°">
          <NumberStepper {...rotationField} step={1} disabled={disabled} className={fieldClass} containerClassName="min-w-0 flex-1" />
        </FieldBox>
      </div>
    </div>
  );
}
