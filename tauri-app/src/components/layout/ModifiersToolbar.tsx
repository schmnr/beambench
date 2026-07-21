import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { useAppStore } from '../../stores/appStore';
import { isTransformLocked, notifyTransformLocked, notifyObjectLocked } from '../../utils/transformLocks';
import { mmToDisplay, displayToMm, roundDisplayLength } from '../../utils/lengthUnits';
import { IconButton } from '../shared/IconButton';
import { NumberStepper } from '../shared/NumberStepper';
import { ToolbarSubmenuButton, type SubmenuItem } from '../shared/ToolbarSubmenuButton';
import { OffsetDialog } from '../dialogs/OffsetDialog';
import { GridArrayDialog } from '../dialogs/GridArrayDialog';
import { CircularArrayDialog } from '../dialogs/CircularArrayDialog';
import { LayoutGrid } from 'lucide-react';

const RadiusIcon = ({ size = 24 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none">
    {/* Two straight edges forming a corner */}
    <path d="M4 3 L4 14 Q4 20, 10 20 L21 20" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" fill="none" />
    {/* Radius arc indicator */}
    <path d="M4 12 Q4 12, 4 14 Q4 18, 8 20" stroke="rgb(34,192,238)" strokeWidth="1.8" strokeLinecap="round" fill="none" strokeDasharray="2 2" />
  </svg>
);

const StartPointIcon = ({ size = 24 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none">
    {/* Flag pole */}
    <line x1="4" y1="3" x2="4" y2="22" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    {/* Flag */}
    <path d="M4 3 L20 7 L4 12 Z" fill="rgb(34,192,238)" />
    {/* Ground line with arrow */}
    <line x1="2" y1="22" x2="12" y2="22" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    <path d="M10 19.5 L14 22 L10 24.5" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" fill="none" />
  </svg>
);

const CircularArrayIcon = ({ size = 24 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none">
    {/* Guide circle (dashed) */}
    <circle cx="12" cy="12" r="8" stroke="currentColor" strokeWidth="1" strokeDasharray="2 2" opacity="0.4" />
    {/* Circles arranged in a ring */}
    <circle cx="12" cy="4" r="2.2" stroke="currentColor" strokeWidth="1.5" fill="none" />
    <circle cx="18.9" cy="8" r="2.2" stroke="currentColor" strokeWidth="1.5" fill="none" />
    <circle cx="18.9" cy="16" r="2.2" stroke="currentColor" strokeWidth="1.5" fill="none" />
    <circle cx="12" cy="20" r="2.2" stroke="currentColor" strokeWidth="1.5" fill="none" />
    <circle cx="5.1" cy="16" r="2.2" stroke="currentColor" strokeWidth="1.5" fill="none" />
    <circle cx="5.1" cy="8" r="2.2" stroke="currentColor" strokeWidth="1.5" fill="none" />
  </svg>
);
import { resolveEffectiveData, isEffectiveVector, isBooleanCompatible } from '../../commands/selectionContext';

const OffsetIcon = ({ size = 24 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
    <rect x="6" y="6" width="12" height="12" rx="2.5" />
    <rect x="2.5" y="2.5" width="19" height="19" rx="4" strokeDasharray="3 2.5" />
  </svg>
);

/* ── Boolean / Weld icons ──
   Two overlapping rectangles with large overlap for readability at small sizes.
   Rect A = (1,1 → 15,15), Rect B = (9,9 → 23,23).
   Overlap region = (9,9 → 15,15) — 6×6 px, ~43% of each rect. */

const UnionIcon = ({ size = 24 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" stroke="none">
    <path d="M1,1 H15 V9 H23 V23 H9 V15 H1 Z" />
  </svg>
);

const SubtractIcon = ({ size = 24 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor">
    {/* A minus overlap — the L-shaped remainder */}
    <path d="M1,1 H15 V9 H9 V15 H1 Z" fill="currentColor" stroke="none" />
    {/* B shown as dashed outline */}
    <rect x="9" y="9" width="14" height="14" strokeWidth="1.5" strokeDasharray="2.5 2" />
  </svg>
);

const IntersectIcon = ({ size = 24 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor">
    {/* Both rects as dashed outlines */}
    <rect x="1" y="1" width="14" height="14" strokeWidth="1.5" strokeDasharray="2.5 2" />
    <rect x="9" y="9" width="14" height="14" strokeWidth="1.5" strokeDasharray="2.5 2" />
    {/* Overlap region filled — large and obvious */}
    <rect x="9" y="9" width="6" height="6" fill="currentColor" stroke="none" />
  </svg>
);

const ExcludeIcon = ({ size = 24 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor">
    <rect x="1" y="1" width="14" height="14" strokeWidth="1.5" strokeDasharray="2.5 2" />
    <rect x="9" y="9" width="14" height="14" strokeWidth="1.5" strokeDasharray="2.5 2" />
    <path d="M1,1 H15 V9 H9 V15 H1 Z" fill="currentColor" stroke="none" />
    <path d="M15,9 H23 V23 H9 V15 H15 Z" fill="currentColor" stroke="none" />
  </svg>
);

const WeldIcon = ({ size = 24 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor">
    {/* Asymmetric 3-rect weld:
        R1 left-middle (1,5→14,18), R2 upper-right (10,1→23,14), R3 lower-middle (5,12→19,23) */}
    <path d="M10,1 H23 V14 H19 V23 H5 V18 H1 V5 H10 Z" strokeWidth="1.5" />
    {/* Dashed internal edges showing original rect boundaries */}
    <line x1="10" y1="5" x2="14" y2="5" strokeWidth="1" strokeDasharray="1.5 1.5" />
    <line x1="10" y1="5" x2="10" y2="14" strokeWidth="1" strokeDasharray="1.5 1.5" />
    <line x1="14" y1="5" x2="14" y2="18" strokeWidth="1" strokeDasharray="1.5 1.5" />
    <line x1="5" y1="12" x2="19" y2="12" strokeWidth="1" strokeDasharray="1.5 1.5" />
    <line x1="5" y1="18" x2="14" y2="18" strokeWidth="1" strokeDasharray="1.5 1.5" />
    <line x1="10" y1="14" x2="19" y2="14" strokeWidth="1" strokeDasharray="1.5 1.5" />
    <line x1="5" y1="12" x2="5" y2="18" strokeWidth="1" strokeDasharray="1.5 1.5" />
    <line x1="19" y1="12" x2="19" y2="14" strokeWidth="1" strokeDasharray="1.5 1.5" />
  </svg>
);

const BOOLEAN_ITEM_DEFS: Array<Omit<SubmenuItem, 'label'> & { labelKey: string }> = [
  { id: 'union', icon: <UnionIcon size={20} />, labelKey: 'toolbars.modifiers.union' },
  { id: 'subtract', icon: <SubtractIcon size={20} />, labelKey: 'toolbars.modifiers.subtract' },
  { id: 'intersect', icon: <IntersectIcon size={20} />, labelKey: 'toolbars.modifiers.intersect' },
  { id: 'exclude', icon: <ExcludeIcon size={20} />, labelKey: 'toolbars.modifiers.exclude' },
];

const SMALL_BUTTON_SIZE = 'sm' as const;
const TOOL_SELECT = 'select' as const;
const TOOL_RADIUS = 'radius' as const;

export function ModifiersToolbar() {
  const { t } = useTranslation();
  const project = useProjectStore((s) => s.project);
  const selectedObjectIds = useProjectStore((s) => s.selectedObjectIds);
  const booleanUnion = useProjectStore((s) => s.booleanUnion);
  const booleanSubtract = useProjectStore((s) => s.booleanSubtract);
  const booleanIntersection = useProjectStore((s) => s.booleanIntersection);
  const booleanExclude = useProjectStore((s) => s.booleanExclude);
  const booleanWeld = useProjectStore((s) => s.booleanWeld);

  const activeTool = useUiStore((s) => s.activeTool);
  const radiusToolValue = useUiStore((s) => s.radiusToolValue);
  const settings = useAppStore((s) => s.settings);

  const [showOffsetDialog, setShowOffsetDialog] = useState(false);
  const [showGridArrayDialog, setShowGridArrayDialog] = useState(false);
  const [showCircularArrayDialog, setShowCircularArrayDialog] = useState(false);
  const [localRadiusStr, setLocalRadiusStr] = useState<string | null>(null);

  const displayUnit = (settings?.display_unit === 'inches' ? 'inches' : 'mm') as 'mm' | 'inches';
  const unitLabel = displayUnit === 'mm' ? 'mm' : 'in';
  const radiusStep = displayUnit === 'inches' ? 0.01 : 0.5;
  const effectiveRadius = radiusToolValue ?? settings?.last_radius_mm ?? 5;

  // Clear stale local input when leaving radius tool or switching units
  useEffect(() => {
    if (activeTool !== 'radius') setLocalRadiusStr(null);
  }, [activeTool]);
  useEffect(() => {
    setLocalRadiusStr(null);
  }, [displayUnit]);

  const selCount = selectedObjectIds.length;
  const selectedObjects = project?.objects.filter((o) => selectedObjectIds.includes(o.id)) ?? [];
  const anyLocked = selectedObjects.some((o) => o.locked);
  const hasSel = selCount > 0 && !anyLocked;
  const booleanPending = useProjectStore((s) => s.booleanPending);
  const allSelectedBooleanOk = selectedObjects.length >= 2 && selectedObjects.every((o) =>
    isBooleanCompatible(o, project?.objects ?? []),
  );
  const hasBool = selCount === 2 && !anyLocked && !booleanPending && allSelectedBooleanOk;
  const hasWeld = selCount >= 2 && !anyLocked && !booleanPending && allSelectedBooleanOk;
  const hasOne = selCount === 1 && !anyLocked;
  const hasVector = hasOne && selectedObjects[0] &&
    isEffectiveVector(selectedObjects[0], project?.objects ?? []);
  const resolvedData = hasOne && selectedObjects[0]
    ? resolveEffectiveData(selectedObjects[0], project?.objects ?? [])
    : null;
  const isBulgedStar = resolvedData?.type === 'star' &&
    (resolvedData as Extract<typeof resolvedData, { type: 'star' }>).bulge > 0;
  const hasRadius = hasVector && !isBulgedStar;

  const guardLock = (): boolean => {
    if (anyLocked) { notifyObjectLocked(); return true; }
    const locks = project?.transform_locks;
    if (isTransformLocked(locks, 'position')) { notifyTransformLocked('position'); return true; }
    return false;
  };

  const lastBooleanOp = useUiStore((s) => s.lastBooleanOp);
  const setLastBooleanOp = useUiStore((s) => s.setLastBooleanOp);

  const booleanOps: Record<string, typeof booleanUnion> = {
    union: booleanUnion,
    subtract: booleanSubtract,
    intersect: booleanIntersection,
    exclude: booleanExclude,
  };
  const booleanItems = BOOLEAN_ITEM_DEFS.map((item) => ({
    id: item.id,
    icon: item.icon,
    label: t(item.labelKey),
  }));

  const executeBooleanOp = (opId: string) => {
    if (guardLock()) return;
    setLastBooleanOp(opId);
    const op = booleanOps[opId];
    if (op) void op(selectedObjectIds[0], selectedObjectIds[1]);
  };

  const GroupSeparator = () => <div className="w-10 h-px bg-bb-border my-0.5" />;

  return (
    <div className="no-select w-16 bg-bb-panel py-1 gap-0.5 text-xs border-r border-bb-border flex flex-col items-center border-t border-t-bb-border">
      <IconButton
        icon={<OffsetIcon size={24} />}
        label={t('toolbars.modifiers.offset')}
        onClick={() => { if (!guardLock()) setShowOffsetDialog(true); }}
        disabled={!hasSel}
        size={SMALL_BUTTON_SIZE}
      />
      <GroupSeparator />
      <IconButton
        icon={<WeldIcon size={24} />}
        label={t('toolbars.modifiers.weld')}
        onClick={() => { if (!guardLock()) void booleanWeld(selectedObjectIds); }}
        disabled={!hasWeld}
        size={SMALL_BUTTON_SIZE}
      />
      <ToolbarSubmenuButton
        items={booleanItems}
        activeItemId={lastBooleanOp}
        onSelect={executeBooleanOp}
        isToolActive={false}
        disabled={!hasBool}
        size={SMALL_BUTTON_SIZE}
      />
      <GroupSeparator />
      <IconButton
        icon={<LayoutGrid size={24} />}
        label={t('toolbars.modifiers.grid_array')}
        onClick={() => { if (!guardLock()) setShowGridArrayDialog(true); }}
        disabled={!hasSel}
        size={SMALL_BUTTON_SIZE}
      />
      <IconButton
        icon={<CircularArrayIcon size={24} />}
        label={t('toolbars.modifiers.circular_array')}
        onClick={() => { if (!guardLock()) setShowCircularArrayDialog(true); }}
        disabled={!hasSel}
        size={SMALL_BUTTON_SIZE}
      />
      <GroupSeparator />
      <IconButton
        icon={<StartPointIcon size={24} />}
        label={t('toolbars.modifiers.set_start_point')}
        onClick={() => {
          if (!hasVector || guardLock()) return;
          useUiStore.getState().setPendingStartPoint(selectedObjectIds[0]);
        }}
        disabled={!hasVector}
        size={SMALL_BUTTON_SIZE}
      />
      <IconButton
        icon={<RadiusIcon size={24} />}
        label={t('toolbars.modifiers.radius_tool')}
        onClick={() => {
          if (!hasRadius || guardLock()) return;
          const { activeTool: at, setActiveTool } = useUiStore.getState();
          if (at === TOOL_RADIUS) {
            // Persist before deactivating
            const rv = useUiStore.getState().radiusToolValue;
            if (rv !== null) {
              void useAppStore.getState().updateSettings({ last_radius_mm: rv });
            }
            setActiveTool(TOOL_SELECT);
          } else {
            setActiveTool(TOOL_RADIUS);
          }
        }}
        disabled={!hasRadius}
        active={activeTool === TOOL_RADIUS}
        size={SMALL_BUTTON_SIZE}
      />

      <div className={`flex flex-col items-center gap-1 mt-1 px-0.5 ${activeTool !== TOOL_RADIUS ? 'opacity-40 pointer-events-none' : ''}`}>
        <span className="text-sm font-medium leading-none text-bb-text-dim">{t('toolbars.modifiers.radius_with_unit', { unit: unitLabel })}</span>
        <NumberStepper
          className="w-[3.75rem] text-center text-sm bg-bb-bg border border-bb-border rounded px-1 py-0.5 text-bb-text"
          value={localRadiusStr ?? roundDisplayLength(mmToDisplay(effectiveRadius, displayUnit), displayUnit)}
          step={radiusStep}
          onChange={(e) => {
            const raw = e.target.value;
            setLocalRadiusStr(raw);
            const v = parseFloat(raw);
            if (!isNaN(v)) {
              useUiStore.getState().setRadiusToolValue(displayToMm(v, displayUnit));
            }
          }}
          onBlur={() => setLocalRadiusStr(null)}
          onKeyDown={(e) => { if (e.key === 'Enter') setLocalRadiusStr(null); }}
        />
      </div>

      {showOffsetDialog && (
        <OffsetDialog objectIds={selectedObjectIds} onClose={() => setShowOffsetDialog(false)} />
      )}
      {showGridArrayDialog && (
        <GridArrayDialog objectIds={selectedObjectIds} onClose={() => setShowGridArrayDialog(false)} />
      )}
      {showCircularArrayDialog && (
        <CircularArrayDialog objectIds={selectedObjectIds} onClose={() => setShowCircularArrayDialog(false)} />
      )}
    </div>
  );
}
