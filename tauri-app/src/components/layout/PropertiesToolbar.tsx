import { useState, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { bumpSettingsMutationSeq, useAppStore } from '../../stores/appStore';
import { appService } from '../../services/appService';
import { Lock, Unlock } from 'lucide-react';
import { NumberStepper } from '../shared/NumberStepper';
import type { AnchorPoint, ObjectData, ProjectObject, TextAlignment, TextAlignmentV, TextLayoutMode } from '../../types/project';
import { useNotificationStore } from '../../stores/notificationStore';
import { isTransformLocked, notifyTransformLocked, isObjectLocked, notifyObjectLocked } from '../../utils/transformLocks';
import { applyTextLayoutMode, clearTextGuidePath } from '../properties/textLayoutMode';
import { canvasToMachinePoint, machineToCanvasPoint } from '../../utils/workspaceCoordinates';
import { mmToDisplay, displayToMm, roundDisplayLength } from '../../utils/lengthUnits';

const DISPLAY_UNIT_MM = 'mm' as const;
const DISPLAY_UNIT_INCHES = 'inches' as const;
type DisplayUnit = typeof DISPLAY_UNIT_MM | typeof DISPLAY_UNIT_INCHES;
const UNIT_LABEL_INCHES = 'in';
const TOOL_TEXT = 'text' as const;
const TEXT_LAYOUT_STRAIGHT = 'straight' as const;
const TEXT_LAYOUT_BEND = 'bend' as const;
const TEXT_LAYOUT_PATH = 'path' as const;
const TEXT_ALIGNMENT_LEFT = 'left' as const;
const TEXT_ALIGNMENT_CENTER = 'center' as const;
const TEXT_ALIGNMENT_RIGHT = 'right' as const;
const TEXT_ALIGNMENT_TOP = 'top' as const;
const TEXT_ALIGNMENT_MIDDLE = 'middle' as const;
const TEXT_ALIGNMENT_BOTTOM = 'bottom' as const;
const TOAST_INFO = 'info' as const;
const TOAST_ERROR = 'error' as const;


export const anchorPoints: AnchorPoint[] = [
  'top_left', 'top_center', 'top_right',
  'center_left', 'center', 'center_right',
  'bottom_left', 'bottom_center', 'bottom_right',
];

export function getAnchorOffset(anchor: AnchorPoint, w: number, h: number): { ax: number; ay: number } {
  const col = anchorPoints.indexOf(anchor) % 3;
  const row = Math.floor(anchorPoints.indexOf(anchor) / 3);
  return { ax: (col / 2) * w, ay: (row / 2) * h };
}

/** For a single text object, compute the alignment anchor point. Non-text returns undefined. */
export function textAnchorPoint(obj: ProjectObject): { x: number; y: number } | undefined {
  if (obj.data.type !== TOOL_TEXT) return undefined;
  const { alignment, alignment_v } = obj.data;
  const b = obj.bounds;
  const w = b.max.x - b.min.x;
  const h = b.max.y - b.min.y;
  const x = alignment === TEXT_ALIGNMENT_LEFT ? b.min.x
          : alignment === TEXT_ALIGNMENT_RIGHT ? b.max.x
          : b.min.x + w / 2;
  const y = (alignment_v ?? TEXT_ALIGNMENT_TOP) === TEXT_ALIGNMENT_TOP ? b.min.y
          : alignment_v === TEXT_ALIGNMENT_BOTTOM ? b.max.y
          : b.min.y + h / 2;
  return { x, y };
}

const inputClass = 'w-24 px-1.5 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text text-right focus:outline-none focus:border-bb-accent h-7';
const narrowInputClass = 'w-[4.75rem] px-1.5 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text text-right focus:outline-none focus:border-bb-accent h-7';
const unitLabelClass = 'text-bb-text-dim text-xs w-4 text-center inline-block';

const defaultFontOptions = [
  { value: 'Arial', labelKey: 'toolbars.properties.font_arial' },
  { value: 'sans-serif', labelKey: 'toolbars.properties.font_sans_serif' },
  { value: 'serif', labelKey: 'toolbars.properties.font_serif' },
  { value: 'monospace', labelKey: 'toolbars.properties.font_monospace' },
];

const alignOptions: { value: TextAlignment; labelKey: string }[] = [
  { value: TEXT_ALIGNMENT_LEFT, labelKey: 'toolbars.properties.align_left' },
  { value: TEXT_ALIGNMENT_CENTER, labelKey: 'toolbars.properties.align_center' },
  { value: TEXT_ALIGNMENT_RIGHT, labelKey: 'toolbars.properties.align_right' },
];

const verticalAlignOptions: { value: TextAlignmentV; labelKey: string }[] = [
  { value: TEXT_ALIGNMENT_TOP, labelKey: 'toolbars.properties.align_top' },
  { value: TEXT_ALIGNMENT_MIDDLE, labelKey: 'toolbars.properties.align_middle' },
  { value: TEXT_ALIGNMENT_BOTTOM, labelKey: 'toolbars.properties.align_bottom' },
];

export const anchorLabelKeys: Record<AnchorPoint, string> = {
  top_left: 'toolbars.properties.anchor.top_left',
  top_center: 'toolbars.properties.anchor.top_center',
  top_right: 'toolbars.properties.anchor.top_right',
  center_left: 'toolbars.properties.anchor.center_left',
  center: 'toolbars.properties.anchor.center',
  center_right: 'toolbars.properties.anchor.center_right',
  bottom_left: 'toolbars.properties.anchor.bottom_left',
  bottom_center: 'toolbars.properties.anchor.bottom_center',
  bottom_right: 'toolbars.properties.anchor.bottom_right',
};

function CheckToggle({
  label,
  active,
  onClick,
  disabled: btnDisabled,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      disabled={btnDisabled}
      className={`flex items-center gap-1 text-xs ${
        btnDisabled
          ? 'text-bb-text-dim/40 cursor-not-allowed'
          : 'text-bb-text-muted hover:text-bb-text'
      }`}
    >
      <span className={`w-3 h-3 rounded-full border flex items-center justify-center shrink-0 ${
        btnDisabled
          ? 'border-bb-border'
          : active
            ? 'border-bb-accent bg-bb-accent'
            : 'border-bb-text-dim hover:border-bb-text-muted'
      }`}>
        {active && <span className="w-1.5 h-1.5 rounded-full bg-white" />}
      </span>
      <span>{label}</span>
    </button>
  );
}

function Divider() {
  return <div className="w-px self-stretch bg-bb-border mx-2.5" />;
}

interface BufferedNumericFieldProps {
  value: string | number;
  onChange: (e: React.ChangeEvent<HTMLInputElement>) => void;
  onBlur: (e: React.FocusEvent<HTMLInputElement>) => void;
  onKeyDown: (e: React.KeyboardEvent<HTMLInputElement>) => void;
}

/**
 * NumberStepper's arrow buttons synthesize a plain `Event('input')`, while real
 * typing (and paste) always arrives as an `InputEvent`. Stepper-driven changes
 * commit immediately per click; typed changes buffer until blur/Enter.
 */
function isStepperCommitEvent(e: React.ChangeEvent<HTMLInputElement>): boolean {
  const native: Event = e.nativeEvent;
  return native.type === 'input'
    && (typeof InputEvent === 'undefined' || !(native instanceof InputEvent));
}

/**
 * Buffers typed input locally so each keystroke does not dispatch a store commit
 * (IPC round-trip + undo entry). Commits on blur or Enter; Escape reverts the
 * buffer to the committed value. Stepper-arrow clicks still commit per click.
 * While an uncommitted edit is pending, external/store updates never clobber the
 * buffer; once committed, the buffer is released when the store-derived value
 * catches up (avoids flashing the stale value during the async IPC round-trip).
 */
export function useBufferedNumericField(
  committedValue: number | string,
  onCommit: (value: number) => void,
  resetKey: string,
): BufferedNumericFieldProps {
  const [buffer, setBuffer] = useState<string | null>(null);
  const dirtyRef = useRef(false);

  // Selection or display-unit changes invalidate any pending edit.
  useEffect(() => {
    dirtyRef.current = false;
    setBuffer(null);
  }, [resetKey]);

  // Release a committed (non-dirty) buffer once the store-derived value changes.
  useEffect(() => {
    if (!dirtyRef.current) setBuffer(null);
  }, [committedValue]);

  const commit = (raw: string) => {
    dirtyRef.current = false;
    const value = Number(raw);
    if (raw.trim() === '' || !Number.isFinite(value)) {
      setBuffer(null); // invalid input reverts to the committed value
      return;
    }
    onCommit(value);
  };

  return {
    value: buffer ?? committedValue,
    onChange: (e) => {
      if (isStepperCommitEvent(e)) {
        setBuffer(null);
        commit(e.target.value);
      } else {
        dirtyRef.current = true;
        setBuffer(e.target.value);
      }
    },
    onBlur: (e) => {
      if (buffer === null) return;
      if (dirtyRef.current) commit(e.target.value);
      setBuffer(null);
    },
    onKeyDown: (e) => {
      if (e.key === 'Enter') {
        if (dirtyRef.current) commit(e.currentTarget.value);
      } else if (e.key === 'Escape') {
        dirtyRef.current = false;
        setBuffer(null);
      }
    },
  };
}

interface PropertiesToolbarProps {
  showNumericEdits?: boolean;
  showTextOptions?: boolean;
}

export function PropertiesToolbar({
  showNumericEdits = true,
  showTextOptions: showTextOptionsToolbar = true,
}: PropertiesToolbarProps = {}) {
  const { t } = useTranslation();
  const project = useProjectStore((s) => s.project);
  const selectedObjectIds = useProjectStore((s) => s.selectedObjectIds);
  const updateObject = useProjectStore((s) => s.updateObject);
  const rotateObjects = useProjectStore((s) => s.rotateObjects);
  const updateObjectData = useProjectStore((s) => s.updateObjectData);
  const updateObjectBoundsBatch = useProjectStore((s) => s.updateObjectBoundsBatch);

  const activeTool = useUiStore((s) => s.activeTool);
  const lockAspect = useUiStore((s) => s.lockAspect);
  const toggleLockAspect = useUiStore((s) => s.toggleLockAspect);
  const textDefaults = useUiStore((s) => s.textDefaults);
  const updateTextDefaults = useUiStore((s) => s.updateTextDefaults);
  const settings = useAppStore((s) => s.settings);
  const displayUnit = (settings?.display_unit === DISPLAY_UNIT_INCHES ? DISPLAY_UNIT_INCHES : DISPLAY_UNIT_MM) as DisplayUnit;
  const unitLabel = displayUnit === DISPLAY_UNIT_MM ? DISPLAY_UNIT_MM : UNIT_LABEL_INCHES;
  const posStep = displayUnit === DISPLAY_UNIT_INCHES ? 0.005 : 0.1;
  const sizeMin = displayUnit === DISPLAY_UNIT_INCHES ? 0.001 : 0.01;
  const [anchor, setAnchor] = useState<AnchorPoint>('top_left');
  const [scaleXPercent, setScaleXPercent] = useState(100);
  const [scaleYPercent, setScaleYPercent] = useState(100);
  const [systemFonts, setSystemFonts] = useState<string[]>([]);
  const fontOptions = systemFonts.length > 0
    ? systemFonts.map((f) => ({ value: f, label: f }))
    : defaultFontOptions.map((f) => ({ value: f.value, label: t(f.labelKey) }));
  useEffect(() => {
    appService.getSystemFonts().then((systemFonts) => {
      if (systemFonts.length > 0) {
        setSystemFonts(systemFonts);
      }
    }).catch(() => {
      // keep defaults
    });
  }, []);

  const nudgeObjects = useProjectStore((s) => s.nudgeObjects);

  const hasSelection = selectedObjectIds.length > 0;
  const multiSel = selectedObjectIds.length > 1;
  const selectedObjects = hasSelection
    ? project?.objects.filter((o) => selectedObjectIds.includes(o.id)) ?? []
    : [];
  const obj = selectedObjects.length > 0 ? selectedObjects[0] : undefined;
  const isTextSelected = !multiSel && obj?.data?.type === TOOL_TEXT;
  const isTextToolActive = activeTool === TOOL_TEXT;
  const textOptionsAvailable = isTextSelected || isTextToolActive;
  const textData_ = isTextSelected && obj?.data?.type === TOOL_TEXT ? obj.data : undefined;
  // Match the backend's effective-layout rule: on_path + straight → path
  const effectiveMode_ = (textData_?.on_path && textData_?.layout_mode === TEXT_LAYOUT_STRAIGHT)
    ? TEXT_LAYOUT_PATH
    : textData_?.layout_mode;
  // When text tool active but no object selected, derive mode from defaults
  const effectiveMode = isTextSelected ? effectiveMode_ : textDefaults.layout_mode;
  const isCurvedMode = effectiveMode === TEXT_LAYOUT_PATH || effectiveMode === TEXT_LAYOUT_BEND;

  // Compute selection bounding box (union of all selected objects)
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

  /** Scale all selected objects proportionally around the selection anchor. */
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

  // Text-specific helpers — when a text object is selected, edit it directly;
  // when only the text tool is active, edit the defaults for the next creation.
  const textData = isTextSelected ? (obj!.data as Extract<ObjectData, { type: typeof TOOL_TEXT }>) : null;
  const textMissingGlyphs = textData?.missing_glyphs ?? [];

  const updateText = (partial: Partial<Extract<ObjectData, { type: typeof TOOL_TEXT }>>) => {
    if (textData && obj) {
      if (isObjectLocked(obj)) { notifyObjectLocked(); return; }
      updateTextDefaults(partial as Partial<typeof textDefaults>);
      void updateObjectData(obj.id, { ...textData, ...partial });
      return;
    }
    updateTextDefaults(partial as Partial<typeof textDefaults>);
  };

  const disabled = !hasSelection || !obj;

  // Buffered numeric fields: typing commits on blur/Enter only; stepper arrows
  // commit per click. Unit conversions stay at the commit boundary below.
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
  const textHeightField = useBufferedNumericField(
    textOptionsAvailable ? roundDisplayLength(mmToDisplay(textData?.font_size_mm ?? textDefaults.font_size_mm, displayUnit), displayUnit) : '',
    (v) => updateText({ font_size_mm: displayToMm(v, displayUnit) }),
    fieldResetKey,
  );
  const hSpaceField = useBufferedNumericField(
    roundDisplayLength(mmToDisplay(textData?.h_spacing ?? textDefaults.h_spacing, displayUnit), displayUnit),
    (v) => updateText({ h_spacing: displayToMm(v, displayUnit) }),
    fieldResetKey,
  );
  const vSpaceField = useBufferedNumericField(
    roundDisplayLength(mmToDisplay(textData?.v_spacing ?? textDefaults.v_spacing, displayUnit), displayUnit),
    (v) => updateText({ v_spacing: displayToMm(v, displayUnit) }),
    fieldResetKey,
  );
  const pathOffsetField = useBufferedNumericField(
    roundDisplayLength(mmToDisplay(textData?.path_offset ?? textDefaults.path_offset, displayUnit), displayUnit),
    (v) => updateText({ path_offset: displayToMm(v, displayUnit) }),
    fieldResetKey,
  );
  const bendRadiusField = useBufferedNumericField(
    roundDisplayLength(mmToDisplay((textData?.bend_radius ?? textDefaults.bend_radius) || 50, displayUnit), displayUnit),
    (v) => updateText({ bend_radius: displayToMm(v, displayUnit) }),
    fieldResetKey,
  );

  const beginGuidePathSelection = () => {
    if (!obj) return;
    useUiStore.getState().setPendingGuidePathText(obj.id);
    useNotificationStore.getState().push(t('toolbars.properties.select_guide_path_hint'), TOAST_INFO);
  };

  const toggleUnit = () => {
    const newUnit: DisplayUnit = displayUnit === DISPLAY_UNIT_MM ? DISPLAY_UNIT_INCHES : DISPLAY_UNIT_MM;
    const cur = useAppStore.getState().settings;
    if (!cur) return;
    const oldUnit = cur.display_unit;
    // Update store directly — this is the authoritative state
    bumpSettingsMutationSeq();
    useAppStore.setState({ settings: { ...cur, display_unit: newUnit } });
    // Persist to backend — rollback on failure
    void appService.updateSettings({ display_unit: newUnit }).catch(() => {
      bumpSettingsMutationSeq();
      useAppStore.setState({ settings: { ...useAppStore.getState().settings!, display_unit: oldUnit } });
      useNotificationStore.getState().push(t('toolbars.properties.error_save_display_unit'), TOAST_ERROR);
    });
  };

  if (!showNumericEdits && !showTextOptionsToolbar) {
    return null;
  }
  return (
    <div className="no-select flex items-stretch h-20 bg-bb-panel px-4 gap-0 text-xs border-b border-bb-border">
      {showNumericEdits && (
        <>
      {/* ── Position/Size section ── */}
      <div className="flex items-center gap-3">
        {/* X/Y position */}
        <div className="flex flex-col justify-center gap-1.5">
          <label className="flex items-center gap-1">
            <span className="text-bb-text-dim text-xs w-9 whitespace-nowrap">{t('toolbars.properties.position_x')}</span>
            <NumberStepper
              {...xField}
              step={posStep}
              disabled={disabled}
              className={inputClass}
            />
            <span className={unitLabelClass}>{unitLabel}</span>
          </label>
          <label className="flex items-center gap-1">
            <span className="text-bb-text-dim text-xs w-9 whitespace-nowrap">{t('toolbars.properties.position_y')}</span>
            <NumberStepper
              {...yField}
              step={posStep}
              disabled={disabled}
              className={inputClass}
            />
            <span className={unitLabelClass}>{unitLabel}</span>
          </label>
        </div>

        {/* Lock aspect ratio toggle */}
        <button
          onClick={toggleLockAspect}
          disabled={disabled}
          className={`flex flex-col items-center justify-center gap-1 px-1 py-1 rounded ${
            disabled
              ? 'text-bb-text-dim/40 cursor-not-allowed'
              : lockAspect
                ? 'text-bb-accent hover:text-bb-accent/80'
                : 'text-bb-text-dim hover:text-bb-text-muted'
          }`}
          title={t('toolbars.properties.lock_aspect_ratio')}
        >
          {lockAspect ? <Lock size={22} /> : <Unlock size={22} />}
        </button>

        {/* W/H size + Scale X/Y % */}
        <div className="flex flex-col justify-center gap-1.5">
          <div className="flex items-center gap-5">
            <label className="flex items-center gap-1">
              <span className="text-bb-text-dim text-xs w-10">{t('toolbars.properties.size_width')}</span>
              <NumberStepper
                {...wField}
                step={posStep}
                min={sizeMin}
                disabled={disabled}
                className={inputClass}
              />
              <span className={unitLabelClass}>{unitLabel}</span>
            </label>
            <label className="flex items-center gap-1">
              <NumberStepper
                {...scaleXField}
                step={1}
                min={1}
                disabled={disabled}
                className={narrowInputClass}
              />
              <span className="text-bb-text-dim text-xs">%</span>
            </label>
          </div>
          <div className="flex items-center gap-5">
            <label className="flex items-center gap-1">
              <span className="text-bb-text-dim text-xs w-10">{t('toolbars.properties.size_height')}</span>
              <NumberStepper
                {...hField}
                step={posStep}
                min={sizeMin}
                disabled={disabled}
                className={inputClass}
              />
              <span className={unitLabelClass}>{unitLabel}</span>
            </label>
            <label className="flex items-center gap-1">
              <NumberStepper
                {...scaleYField}
                step={1}
                min={1}
                disabled={disabled}
                className={narrowInputClass}
              />
              <span className="text-bb-text-dim text-xs">%</span>
            </label>
          </div>
        </div>

        {/* Anchor grid */}
        <div className="grid grid-cols-3 gap-1.5 shrink-0 self-center">
          {anchorPoints.map((ap) => (
            <button
              key={ap}
              onClick={() => setAnchor(ap)}
              disabled={disabled}
              className={`shrink-0 rounded-full ${
                anchor === ap
                  ? 'bg-bb-text w-2.5 h-2.5'
                  : 'border border-bb-text-dim w-2 h-2 hover:border-bb-text-muted'
              }`}
              title={t(anchorLabelKeys[ap])}
            />
          ))}
        </div>

        {/* Rotate */}
        <label className="flex items-center gap-1 ml-3 self-center">
          <span className="text-bb-text-dim text-xs">{t('toolbars.properties.rotation')}</span>
          <NumberStepper
            {...rotationField}
            step={1}
            disabled={disabled}
            className={narrowInputClass}
          />
        </label>

        {/* mm/in toggle */}
        <button
          onClick={toggleUnit}
          className="flex items-center justify-center px-1.5 py-0.5 rounded text-xs font-medium border border-bb-border bg-bb-bg text-bb-text hover:bg-bb-hover h-7 min-w-[2rem] self-center"
          title={t('toolbars.properties.switch_to_unit', {
            unit: displayUnit === DISPLAY_UNIT_MM
              ? t('toolbars.properties.unit_inches')
              : t('toolbars.properties.unit_millimeters'),
          })}
        >
          {unitLabel}
        </button>
      </div>
        </>
      )}

      {/* ── Text Options Toolbar ── */}
      {showNumericEdits && showTextOptionsToolbar && <Divider />}
      {showTextOptionsToolbar && (
      <div className={`flex items-center gap-3 ${!textOptionsAvailable ? 'opacity-40 pointer-events-none' : ''}`}>
        <div className="flex flex-col justify-center gap-1.5">
          {/* Row 1: Font | Height | HSpace | Align X | Mode */}
          <div className="flex items-center gap-3">
            <div className="flex items-center gap-3 w-[19rem] shrink-0">
              <label className="flex items-center gap-1">
                <span className="text-bb-text-dim text-xs">{t('toolbars.properties.font')}</span>
                <select
                  value={textData?.font_family ?? textDefaults.font_family}
                  onChange={(e) => updateText({ font_family: e.target.value })}
                  disabled={!textOptionsAvailable}
                  className={`w-28 px-1.5 py-0.5 bg-bb-bg border rounded text-xs text-bb-text focus:outline-none focus:border-bb-accent h-7 ${(textData?.missing_font || textMissingGlyphs.length > 0) ? 'border-yellow-500' : 'border-bb-border'}`}
                >
                  {fontOptions.map((f) => (
                    <option key={f.value} value={f.value}>{f.label}</option>
                  ))}
                </select>
                {textData?.missing_font && (
                  <span className="text-yellow-500 text-[10px]" title={t('toolbars.properties.font_missing', { font: textData.font_family })}>!</span>
                )}
                {!textData?.missing_font && textMissingGlyphs.length > 0 && (
                  <span className="text-yellow-500 text-[10px]" title={t('toolbars.properties.missing_glyphs', { glyphs: textMissingGlyphs.join(' ') })}>!</span>
                )}
              </label>
              <label className="flex items-center gap-1">
                <span className="text-bb-text-dim text-xs">{t('toolbars.properties.text_height')}</span>
                <NumberStepper {...textHeightField} min={displayUnit === DISPLAY_UNIT_INCHES ? 0.02 : 0.5} max={displayUnit === DISPLAY_UNIT_INCHES ? 20 : 500} step={posStep} disabled={!textOptionsAvailable} className={narrowInputClass} />
              </label>
            </div>
            <label className="flex items-center gap-1">
              <span className="text-bb-text-dim text-xs">{t('toolbars.properties.h_space')}</span>
              <NumberStepper {...hSpaceField} step={posStep} disabled={!textOptionsAvailable} className={narrowInputClass} />
            </label>
            <label className={`flex items-center gap-1${isCurvedMode ? ' opacity-40 pointer-events-none' : ''}`}>
              <span className="text-bb-text-dim text-xs">{t('toolbars.properties.align_x')}</span>
              <select
                value={textData?.alignment ?? textDefaults.alignment}
                onChange={(e) => updateText({ alignment: e.target.value as TextAlignment })}
                disabled={!textOptionsAvailable || isCurvedMode}
                className="w-[4.5rem] px-1.5 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text focus:outline-none focus:border-bb-accent h-7"
              >
                {alignOptions.map((ao) => (
                  <option key={ao.value} value={ao.value}>{t(ao.labelKey)}</option>
                ))}
              </select>
            </label>
            <label className="flex items-center gap-1">
              <select
                value={effectiveMode ?? TEXT_LAYOUT_STRAIGHT}
                onChange={(e) => {
                  const mode = e.target.value as TextLayoutMode;
                  if (isTextSelected && obj && textData) {
                    void applyTextLayoutMode(obj.id, textData, mode, {
                      bendRadiusFallback: textDefaults.bend_radius === 0 ? 50 : textDefaults.bend_radius,
                    });
                  } else if (mode === TEXT_LAYOUT_BEND) {
                    const curRadius = textDefaults.bend_radius;
                    updateText({ layout_mode: mode, bend_radius: curRadius === 0 ? 50 : curRadius });
                  } else if (mode === TEXT_LAYOUT_STRAIGHT) {
                    updateText({ layout_mode: mode, on_path: false });
                  } else {
                    updateText({ layout_mode: mode, on_path: mode === TEXT_LAYOUT_PATH });
                  }
                }}
                disabled={!textOptionsAvailable}
                className="w-20 px-1.5 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text focus:outline-none focus:border-bb-accent h-7"
              >
                <option value={TEXT_LAYOUT_STRAIGHT}>{t('toolbars.properties.mode_normal')}</option>
                <option value={TEXT_LAYOUT_BEND}>{t('toolbars.properties.mode_bend')}</option>
                <option value={TEXT_LAYOUT_PATH}>{t('toolbars.properties.mode_path')}</option>
              </select>
            </label>
          </div>
          {/* Row 2: Bold | Italic | Upper Case | Welded | Distort | VSpace | Align Y | Offset */}
          <div className="flex items-center gap-3">
            <div className="flex items-center gap-2.5 w-[19rem] shrink-0">
              <CheckToggle label={t('toolbars.properties.bold')} active={textData?.bold ?? textDefaults.bold} onClick={() => updateText({ bold: !(textData?.bold ?? textDefaults.bold) })} disabled={!textOptionsAvailable} />
              <CheckToggle label={t('toolbars.properties.italic')} active={textData?.italic ?? textDefaults.italic} onClick={() => updateText({ italic: !(textData?.italic ?? textDefaults.italic) })} disabled={!textOptionsAvailable} />
              <CheckToggle label={t('toolbars.properties.upper_case')} active={textData?.upper_case ?? textDefaults.upper_case} onClick={() => updateText({ upper_case: !(textData?.upper_case ?? textDefaults.upper_case) })} disabled={!textOptionsAvailable} />
              <CheckToggle label={t('toolbars.properties.welded')} active={textData?.welded ?? textDefaults.welded} onClick={() => updateText({ welded: !(textData?.welded ?? textDefaults.welded) })} disabled={!textOptionsAvailable} />
              <CheckToggle label={t('toolbars.properties.distort')} active={textData?.distort ?? textDefaults.distort} onClick={() => updateText({ distort: !(textData?.distort ?? textDefaults.distort) })} disabled={!textOptionsAvailable || !isCurvedMode} />
            </div>
            <label className={`flex items-center gap-1${isCurvedMode ? ' opacity-40 pointer-events-none' : ''}`}>
              <span className="text-bb-text-dim text-xs">{t('toolbars.properties.v_space')}</span>
              <NumberStepper {...vSpaceField} step={posStep} disabled={!textOptionsAvailable || isCurvedMode} className={narrowInputClass} />
            </label>
            <label className={`flex items-center gap-1${isCurvedMode ? ' opacity-40 pointer-events-none' : ''}`}>
              <span className="text-bb-text-dim text-xs">{t('toolbars.properties.align_y')}</span>
              <select
                value={textData?.alignment_v ?? textDefaults.alignment_v}
                onChange={(e) => updateText({ alignment_v: e.target.value as TextAlignmentV })}
                disabled={!textOptionsAvailable || isCurvedMode}
                className="w-[4.5rem] px-1.5 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text focus:outline-none focus:border-bb-accent h-7"
              >
                {verticalAlignOptions.map((option) => (
                  <option key={option.value} value={option.value}>{t(option.labelKey)}</option>
                ))}
              </select>
            </label>
            <label className={`flex items-center gap-1${!isCurvedMode ? ' opacity-40 pointer-events-none' : ''}`}>
              <span className="text-bb-text-dim text-xs">{t('toolbars.properties.offset')}</span>
              <NumberStepper {...pathOffsetField} step={posStep} disabled={!textOptionsAvailable || !isCurvedMode} className={narrowInputClass} />
            </label>
            {effectiveMode === TEXT_LAYOUT_BEND && (
              <label className="flex items-center gap-1">
                <span className="text-bb-text-dim text-xs">{t('toolbars.properties.radius')}</span>
                <NumberStepper {...bendRadiusField} step={displayUnit === DISPLAY_UNIT_INCHES ? 0.2 : 5} disabled={!textOptionsAvailable} className={narrowInputClass} />
              </label>
            )}
            {effectiveMode === TEXT_LAYOUT_PATH && isTextSelected && (
              <div className="flex items-center gap-1">
                {textData?.guide_path_id ? (
                  <>
                    <span className="text-bb-text-dim text-[10px]">{t('toolbars.properties.linked')}</span>
                    <button className="px-1.5 py-0.5 text-[10px] bg-bb-bg border border-bb-border rounded text-bb-text hover:bg-bb-hover h-6" onClick={beginGuidePathSelection}>{t('toolbars.properties.pick')}</button>
                    <button className="px-1.5 py-0.5 text-[10px] bg-bb-bg border border-bb-border rounded text-bb-text hover:bg-bb-hover h-6" onClick={() => { if (!obj) return; void clearTextGuidePath(obj.id); }}>{t('toolbars.properties.clear')}</button>
                  </>
                ) : (
                  <>
                    <span className="text-yellow-500 text-[10px]">{t('toolbars.properties.no_path')}</span>
                    <button className="px-1.5 py-0.5 text-[10px] bg-bb-bg border border-bb-border rounded text-bb-text hover:bg-bb-hover h-6" onClick={beginGuidePathSelection}>{t('toolbars.properties.select_path')}</button>
                  </>
                )}
              </div>
            )}
          </div>
        </div>
      </div>
      )}
    </div>
  );
}
