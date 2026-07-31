import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { bumpSettingsMutationSeq, useAppStore } from '../../stores/appStore';
import { appService } from '../../services/appService';
import { Focus, Lock, Unlock } from 'lucide-react';
import { NumberStepper } from '../shared/NumberStepper';
import type { AnchorPoint, TransformLocks } from '../../types/project';
import { useNotificationStore } from '../../stores/notificationStore';
import {
  isTransformLocked,
  effectiveTransformLocks,
  DEFAULT_TRANSFORM_LOCKS,
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
import { INSPECTOR_SECTION_HEADER_CLASS } from '../shared/panelAppearance';
import { computeVisualBoundsWorld, getCombinedBounds } from '../../canvas/alignment';

const DISPLAY_UNIT_MM = 'mm' as const;
const DISPLAY_UNIT_INCHES = 'inches' as const;
type DisplayUnit = typeof DISPLAY_UNIT_MM | typeof DISPLAY_UNIT_INCHES;
const UNIT_LABEL_INCHES = 'in';
const TOAST_ERROR = 'error' as const;
const ROTATION_FIELD_LABEL = '⟳';
const MOVE_LOCK_KEY: keyof TransformLocks = 'move_enabled';
const SIZE_LOCK_KEY: keyof TransformLocks = 'size_enabled';
const ROTATE_LOCK_KEY: keyof TransformLocks = 'rotate_enabled';
const SHEAR_LOCK_KEY: keyof TransformLocks = 'shear_enabled';
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

function TransformLockButton({
  label,
  locked,
  onClick,
  disabled = false,
}: {
  label: string;
  locked: boolean;
  onClick: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      aria-label={label}
      aria-pressed={locked}
      title={label}
      className={`flex h-7 w-5 shrink-0 items-center justify-center rounded transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent disabled:cursor-default ${
        disabled
          ? 'text-bb-text-disabled'
          : locked
            ? 'text-bb-accent hover:text-bb-accent'
            : 'text-bb-text-dim hover:text-bb-text'
      }`}
    >
      {locked ? <Lock size={13} /> : <Unlock size={13} />}
    </button>
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
  const moveObjectsTo = useProjectStore((s) => s.moveObjectsTo);
  const updateObjectTransformState = useProjectStore((s) => s.updateObjectTransformState);

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
  const visualBounds = selectedObjects.length > 0
    ? getCombinedBounds(selectedObjects.map((object) => computeVisualBoundsWorld(object, project?.objects)))
    : undefined;
  const visualW = visualBounds ? visualBounds.max.x - visualBounds.min.x : 0;
  const visualH = visualBounds ? visualBounds.max.y - visualBounds.min.y : 0;
  const { ax, ay } = getAnchorOffset(anchor, w, h);
  const txtAnchor = !multiSel && obj ? textAnchorPoint(obj) : undefined;
  const canvasAnchorPoint = txtAnchor ?? (b ? { x: b.min.x + ax, y: b.min.y + ay } : { x: 0, y: 0 });
  const displayPoint = project ? canvasToMachinePoint(canvasAnchorPoint, project.workspace) : canvasAnchorPoint;
  const displayX = displayPoint.x;
  const displayY = displayPoint.y;
  const rotationDeg = !multiSel && obj
    ? Math.round(Math.atan2(obj.transform.b, obj.transform.a) * (180 / Math.PI) * 10) / 10
    : 0;

  const locks = effectiveTransformLocks(selectedObjects);
  const lockAspect = selectedObjects.length > 0
    && selectedObjects.every((object) => object.lock_aspect_ratio);
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

  const toggleTransformLock = (key: keyof TransformLocks) => {
    void updateObjectTransformState(selectedObjectIds, {
      transformLockKey: key,
      transformEnabled: !locks[key],
    });
  };

  const allLocksLocked = selectedObjects.length > 0 && selectedObjects.every((object) => {
    const objectLocks = object.transform_locks ?? DEFAULT_TRANSFORM_LOCKS;
    return Object.values(objectLocks).every((enabled) => enabled === false)
      && object.lock_aspect_ratio;
  });
  const allTransformsLockLabel = allLocksLocked
    ? t('toolbars.transform_toggles.unlock_all')
    : t('toolbars.transform_toggles.lock_all');
  const toggleAllTransformLocks = () => {
    const shouldLock = !allLocksLocked;
    const enabled = !shouldLock;
    void updateObjectTransformState(selectedObjectIds, {
      transformLocks: {
        move_enabled: enabled,
        size_enabled: enabled,
        rotate_enabled: enabled,
        shear_enabled: enabled,
      },
      lockAspectRatio: shouldLock,
    });
  };

  const toggleLockAspect = () => {
    void updateObjectTransformState(selectedObjectIds, { lockAspectRatio: !lockAspect });
  };

  const visualSizeAfterRawScale = (scaleX: number, scaleY: number) => {
    if (!b || selectedObjects.length === 0) return { width: visualW, height: visualH };
    const anchorX = b.min.x + (col / 2) * w;
    const anchorY = b.min.y + (row / 2) * h;
    const scaledObjects = selectedObjects.map((object) => ({
      ...object,
      bounds: {
        min: {
          x: anchorX + (object.bounds.min.x - anchorX) * scaleX,
          y: anchorY + (object.bounds.min.y - anchorY) * scaleY,
        },
        max: {
          x: anchorX + (object.bounds.max.x - anchorX) * scaleX,
          y: anchorY + (object.bounds.max.y - anchorY) * scaleY,
        },
      },
    }));
    const scaledBounds = getCombinedBounds(
      scaledObjects.map((object) => computeVisualBoundsWorld(object, scaledObjects)),
    );
    return {
      width: scaledBounds.max.x - scaledBounds.min.x,
      height: scaledBounds.max.y - scaledBounds.min.y,
    };
  };

  const rawScaleForVisualSize = (axis: 'width' | 'height', target: number) => {
    const currentVisualSize = axis === 'width' ? visualW : visualH;
    const currentRawSize = axis === 'width' ? w : h;
    if (currentVisualSize <= 0 || currentRawSize <= 0) return 1;
    if (lockAspect) return Math.max(target / currentVisualSize, 0.0001);
    const hasIdentityLinearTransforms = selectedObjects.every((object) =>
      object.transform.a === 1
      && object.transform.b === 0
      && object.transform.c === 0
      && object.transform.d === 1,
    );
    if (hasIdentityLinearTransforms) return Math.max(target / currentRawSize, 0.0001);

    const probeScale = 1.01;
    const probeSize = axis === 'width'
      ? visualSizeAfterRawScale(probeScale, 1).width
      : visualSizeAfterRawScale(1, probeScale).height;
    const response = (probeSize - currentVisualSize) / (probeScale - 1);
    if (Math.abs(response) < 1e-9) return Math.max(target / currentVisualSize, 0.0001);
    return Math.max(1 + (target - currentVisualSize) / response, 0.0001);
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
    const targetVisualW = displayToMm(newDisplayW, displayUnit);
    if (!b || targetVisualW <= 0) return;
    if (guardLocked()) return;
    if (isTransformLocked(locks, 'scale')) { notifyTransformLocked('scale'); return; }
    const scale = rawScaleForVisualSize('width', targetVisualW);
    const newW = w * scale;
    const newH = lockAspect ? h * scale : h;
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
    const targetVisualH = displayToMm(newDisplayH, displayUnit);
    if (!b || targetVisualH <= 0) return;
    if (guardLocked()) return;
    if (isTransformLocked(locks, 'scale')) { notifyTransformLocked('scale'); return; }
    const scale = rawScaleForVisualSize('height', targetVisualH);
    const newH = h * scale;
    const newW = lockAspect ? w * scale : w;
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

  const handleCenterOnPage = () => {
    if (!project || !b) return;
    if (guardLocked()) return;
    if (isTransformLocked(locks, 'position')) { notifyTransformLocked('position'); return; }
    const targetX = (project.workspace.bed_width_mm - w) / 2;
    const targetY = (project.workspace.bed_height_mm - h) / 2;
    void moveObjectsTo(selectedObjectIds, targetX, targetY);
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
    disabled ? '' : roundDisplayLength(mmToDisplay(visualW, displayUnit), displayUnit),
    handleWChange,
    fieldResetKey,
  );
  const hField = useBufferedNumericField(
    disabled ? '' : roundDisplayLength(mmToDisplay(visualH, displayUnit), displayUnit),
    handleHChange,
    fieldResetKey,
  );
  const scaleXField = useBufferedNumericField(disabled ? '' : scaleXPercent, handleScaleXChange, fieldResetKey);
  const scaleYField = useBufferedNumericField(disabled ? '' : scaleYPercent, handleScaleYChange, fieldResetKey);
  const rotationField = useBufferedNumericField(disabled ? '' : rotationDeg, handleRotateChange, fieldResetKey);

  if (!hasSelection) return null;

  return (
    <div className="border-b border-bb-border pb-3 mb-1" data-testid="transform-section">
      {/* Header: label, anchor grid, unit toggle */}
      <div className="flex items-center justify-between py-2">
        <span className={INSPECTOR_SECTION_HEADER_CLASS}>
          {t('panels.properties.transform')}
        </span>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={handleCenterOnPage}
            disabled={disabled || selectedObjects.some((object) => object.locked)}
            aria-label={t('toolbars.main.center_on_page')}
            title={t('toolbars.main.center_on_page')}
            className="flex h-7 w-7 items-center justify-center rounded-lg border border-bb-border bg-bb-bg text-bb-text-muted transition-colors hover:border-bb-accent/40 hover:bg-bb-hover hover:text-bb-text disabled:cursor-default disabled:text-bb-text-disabled disabled:hover:border-bb-border disabled:hover:bg-bb-bg"
          >
            <Focus size={17} />
          </button>
          <button
            type="button"
            onClick={toggleAllTransformLocks}
            disabled={disabled}
            aria-label={allTransformsLockLabel}
            aria-pressed={allLocksLocked}
            title={allTransformsLockLabel}
            className={`flex h-7 w-7 items-center justify-center rounded-lg border bg-bb-bg transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent disabled:cursor-default disabled:text-bb-text-disabled ${
              allLocksLocked
                ? 'border-bb-accent/50 text-bb-accent hover:bg-bb-hover'
                : 'border-bb-border text-bb-text-muted hover:border-bb-accent/40 hover:bg-bb-hover hover:text-bb-text'
            }`}
          >
            {allLocksLocked ? <Lock size={17} /> : <Unlock size={17} />}
          </button>
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
      <div className="grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-1">
        <FieldBox label="X" suffix={unitLabel}>
          <NumberStepper {...xField} step={posStep} disabled={disabled} className={fieldClass} containerClassName="min-w-0 flex-1" />
        </FieldBox>
        <TransformLockButton
          label={t('toolbars.transform_toggles.move')}
          locked={locks.move_enabled === false}
          onClick={() => toggleTransformLock(MOVE_LOCK_KEY)}
        />
        <FieldBox label="Y" suffix={unitLabel}>
          <NumberStepper {...yField} step={posStep} disabled={disabled} className={fieldClass} containerClassName="min-w-0 flex-1" />
        </FieldBox>
      </div>

      {/* W / proportional lock / H */}
      <div className="mt-1.5 flex items-center gap-1">
        <FieldBox label="W" suffix={unitLabel}>
          <NumberStepper {...wField} step={posStep} min={sizeMin} disabled={disabled} className={fieldClass} containerClassName="min-w-0 flex-1" />
        </FieldBox>
        <TransformLockButton
          label={t('toolbars.properties.lock_aspect_ratio')}
          locked={lockAspect}
          onClick={toggleLockAspect}
          disabled={disabled}
        />
        <FieldBox label="H" suffix={unitLabel}>
          <NumberStepper {...hField} step={posStep} min={sizeMin} disabled={disabled} className={fieldClass} containerClassName="min-w-0 flex-1" />
        </FieldBox>
      </div>

      {/* Scale, rotation, and canvas-only shear lock */}
      <div className="mt-1.5 grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-1">
        <FieldBox label="SX" suffix="%">
          <NumberStepper {...scaleXField} step={1} min={1} disabled={disabled} className={fieldClass} containerClassName="min-w-0 flex-1" />
        </FieldBox>
        <TransformLockButton
          label={t('toolbars.transform_toggles.size')}
          locked={locks.size_enabled === false}
          onClick={() => toggleTransformLock(SIZE_LOCK_KEY)}
        />
        <FieldBox label="SY" suffix="%">
          <NumberStepper {...scaleYField} step={1} min={1} disabled={disabled} className={fieldClass} containerClassName="min-w-0 flex-1" />
        </FieldBox>
      </div>
      <div className="mt-1.5 grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-1">
        <FieldBox label={ROTATION_FIELD_LABEL} suffix="°">
          <NumberStepper {...rotationField} step={1} disabled={disabled} className={fieldClass} containerClassName="min-w-0 flex-1" />
        </FieldBox>
        <TransformLockButton
          label={t('toolbars.transform_toggles.rotate')}
          locked={locks.rotate_enabled === false}
          onClick={() => toggleTransformLock(ROTATE_LOCK_KEY)}
        />
        <button
          type="button"
          onClick={() => toggleTransformLock(SHEAR_LOCK_KEY)}
          aria-label={t('toolbars.transform_toggles.shear')}
          aria-pressed={locks.shear_enabled === false}
          title={t('toolbars.transform_toggles.shear')}
          className={`flex h-7 min-w-0 items-center justify-between rounded-lg border border-bb-border bg-bb-bg px-2 text-[9px] font-semibold uppercase transition-colors hover:border-bb-accent/40 hover:text-bb-text focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent ${
            locks.shear_enabled === false ? 'text-bb-accent' : 'text-bb-text-dim'
          }`}
        >
          <span>{t('toolbars.transform_toggles.shear')}</span>
          {locks.shear_enabled === false ? <Lock size={13} /> : <Unlock size={13} />}
        </button>
      </div>
    </div>
  );
}
