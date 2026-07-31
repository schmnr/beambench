import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { useAppStore } from '../../stores/appStore';
import {
  effectiveTransformLocks,
  isTransformLocked,
  notifyTransformLocked,
  notifyObjectLocked,
} from '../../utils/transformLocks';
import { IconButton } from '../shared/IconButton';
import { LayoutGrid } from 'lucide-react';

const RadiusIcon = ({ size = 24 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none">
    {/* Two straight edges forming a corner */}
    <path d="M4 3 L4 14 Q4 20, 10 20 L21 20" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" fill="none" />
    {/* Radius arc indicator */}
    <path d="M4 12 Q4 12, 4 14 Q4 18, 8 20" stroke="rgb(var(--bb-accent))" strokeWidth="1.8" strokeLinecap="round" fill="none" strokeDasharray="2 2" />
  </svg>
);

const StartPointIcon = ({ size = 24 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none">
    {/* Flag pole */}
    <line x1="4" y1="3" x2="4" y2="22" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    {/* Flag */}
    <path d="M4 3 L20 7 L4 12 Z" fill="rgb(var(--bb-accent))" />
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
import { resolveEffectiveData, isEffectiveVector } from '../../commands/selectionContext';

const OffsetIcon = ({ size = 24 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
    <rect x="6" y="6" width="12" height="12" rx="2.5" />
    <rect x="2.5" y="2.5" width="19" height="19" rx="4" strokeDasharray="3 2.5" />
  </svg>
);

const SMALL_BUTTON_SIZE = 'sm' as const;
const TOOL_SELECT = 'select' as const;
const TOOL_RADIUS = 'radius' as const;
const MODIFIER_OFFSET = 'offset' as const;
const MODIFIER_GRID_ARRAY = 'grid_array' as const;
const MODIFIER_CIRCULAR_ARRAY = 'circular_array' as const;

export function ModifiersToolbar() {
  const { t } = useTranslation();
  const project = useProjectStore((s) => s.project);
  const selectedObjectIds = useProjectStore((s) => s.selectedObjectIds);

  const activeTool = useUiStore((s) => s.activeTool);
  const modifierPropertiesSession = useUiStore((s) => s.modifierPropertiesSession);

  const selCount = selectedObjectIds.length;
  const selectedObjects = project?.objects.filter((o) => selectedObjectIds.includes(o.id)) ?? [];
  const anyLocked = selectedObjects.some((o) => o.locked);
  const hasSel = selCount > 0 && !anyLocked;
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
    const locks = effectiveTransformLocks(selectedObjects);
    if (isTransformLocked(locks, 'position')) { notifyTransformLocked('position'); return true; }
    return false;
  };

  const toggleModifierProperties = (kind: 'offset' | 'grid_array' | 'circular_array') => {
    const ui = useUiStore.getState();
    if (modifierPropertiesSession?.kind === kind) {
      ui.closeModifierProperties();
      return;
    }
    if (!guardLock()) ui.openModifierProperties(kind, selectedObjectIds);
  };

  const GroupSeparator = () => <div className="w-10 h-px bg-bb-border my-0.5" />;

  return (
    <div className="no-select w-16 bg-bb-panel py-1.5 gap-0.5 text-xs flex flex-col items-center border-t border-t-bb-border">
      <IconButton
        icon={<OffsetIcon size={24} />}
        label={t('toolbars.modifiers.offset')}
        onClick={() => toggleModifierProperties(MODIFIER_OFFSET)}
        disabled={!hasSel}
        active={modifierPropertiesSession?.kind === 'offset'}
        size={SMALL_BUTTON_SIZE}
      />
      <GroupSeparator />
      <IconButton
        icon={<LayoutGrid size={24} />}
        label={t('toolbars.modifiers.grid_array')}
        onClick={() => toggleModifierProperties(MODIFIER_GRID_ARRAY)}
        disabled={!hasSel}
        active={modifierPropertiesSession?.kind === 'grid_array'}
        size={SMALL_BUTTON_SIZE}
      />
      <IconButton
        icon={<CircularArrayIcon size={24} />}
        label={t('toolbars.modifiers.circular_array')}
        onClick={() => toggleModifierProperties(MODIFIER_CIRCULAR_ARRAY)}
        disabled={!hasSel}
        active={modifierPropertiesSession?.kind === 'circular_array'}
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
    </div>
  );
}
