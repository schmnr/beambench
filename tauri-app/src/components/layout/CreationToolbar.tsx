import { useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { useUiStore, type ToolType } from '../../stores/uiStore';
import { IconButton } from '../shared/IconButton';
import { ToolbarSubmenuButton, type SubmenuItem } from '../shared/ToolbarSubmenuButton';
import {
  MousePointer2, Square, Circle,
  Pentagon, Star, Type, ScissorsLineDashed,
  MapPin, Ruler, PenTool as PenToolIcon,
  Triangle, Hexagon, Octagon,
} from 'lucide-react';

const TabsIcon = ({ size = 24 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none">
    {/* Cut path (dashed circle) */}
    <circle cx="12" cy="12" r="9" stroke="currentColor" strokeWidth="1.8" strokeDasharray="5 3.5" fill="none" />
    {/* Tab bridges (solid segments crossing the cut) */}
    <rect x="11" y="1.5" width="2" height="3.5" rx="0.5" fill="rgb(var(--bb-accent))" />
    <rect x="11" y="19" width="2" height="3.5" rx="0.5" fill="rgb(var(--bb-accent))" />
    <rect x="1.5" y="11" width="3.5" height="2" rx="0.5" fill="rgb(var(--bb-accent))" />
    <rect x="19" y="11" width="3.5" height="2" rx="0.5" fill="rgb(var(--bb-accent))" />
  </svg>
);
import type { PolygonTool } from '../../canvas/tools/PolygonTool';
import type { StarTool } from '../../canvas/tools/StarTool';

const NodeEditIcon = ({ size = 24 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none">
    {/* V-shape path */}
    <path d="M3 3 L12 21" stroke="currentColor" strokeWidth="2" strokeLinecap="round" fill="none" />
    <path d="M12 21 L21 3" stroke="currentColor" strokeWidth="2" strokeLinecap="round" fill="none" />
    {/* End nodes (hollow circles) */}
    <circle cx="3" cy="3" r="2.8" stroke="currentColor" strokeWidth="1.8" fill="none" />
    <circle cx="21" cy="3" r="2.8" stroke="currentColor" strokeWidth="1.8" fill="none" />
    {/* Selected bottom node (hollow square, blue, bold) */}
    <rect x="9" y="18" width="6" height="6" rx="0.8" fill="rgb(var(--bb-accent))" />
  </svg>
);

// Dual Star: 5 long primary tips alternating with 5 shorter secondary tips and
// valleys between — generated from the tool's actual default geometry
// (outer radius, valleys at ratio 0.5, secondary tips at ratio 0.7) so the
// symbol matches what the tool draws, distinct from the plain 5-point Star.
const DualStarIcon = ({ size = 24 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none">
    <path
      d="M12 2 L13.5 7.2 L16.1 6.3 L16 9.1 L21.5 8.9 L17 12 L18.7 14.2 L16 14.9 L17.9 20.1 L13.5 16.8 L12 19 L10.5 16.8 L6.1 20.1 L8 14.9 L5.3 14.2 L7 12 L2.5 8.9 L8 9.1 L7.9 6.3 L10.5 7.2 Z"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinejoin="round"
      strokeLinecap="round"
      fill="none"
    />
  </svg>
);

const SHAPE_ITEM_DEFS: Array<Omit<SubmenuItem, 'label'> & { labelKey: string }> = [
  { id: 'rect', icon: <Square size={20} />, labelKey: 'toolbars.creation.rectangle' },
  { id: 'ellipse', icon: <Circle size={20} />, labelKey: 'toolbars.creation.ellipse' },
  { id: 'triangle', icon: <Triangle size={20} />, labelKey: 'toolbars.creation.triangle' },
  { id: 'pentagon', icon: <Pentagon size={20} />, labelKey: 'toolbars.creation.pentagon' },
  { id: 'polygon', icon: <Hexagon size={20} />, labelKey: 'toolbars.creation.polygon' },
  { id: 'octagon', icon: <Octagon size={20} />, labelKey: 'toolbars.creation.octagon' },
  { id: 'star', icon: <Star size={20} />, labelKey: 'toolbars.creation.star' },
  { id: 'dual_star', icon: <DualStarIcon size={20} />, labelKey: 'toolbars.creation.dual_star' },
];

const TOOL_SELECT = 'select' as const;
const TOOL_LINE = 'line' as const;
const TOOL_TEXT = 'text' as const;
const TOOL_NODE = 'node' as const;
const TOOL_TRIM = 'trim' as const;
const TOOL_TABS = 'tabs' as const;
const TOOL_LASER_POSITION = 'laser_position' as const;
const TOOL_MEASURE = 'measure' as const;
const SMALL_BUTTON_SIZE = 'sm' as const;

// Map sub-tool ID → ToolType that should be activated
const SHAPE_TOOL_MAP: Record<string, ToolType> = {
  rect: 'rect',
  ellipse: 'ellipse',
  triangle: 'polygon',
  pentagon: 'polygon',
  polygon: 'polygon',
  octagon: 'polygon',
  star: 'star',
  dual_star: 'star',
};

// Map polygon-preset sub-tool IDs to their side count
const POLYGON_SIDES: Record<string, number> = {
  triangle: 3,
  pentagon: 5,
  polygon: 6,
  octagon: 8,
};

// Access TOOL_INSTANCES from Canvas for configuring polygon sides / star dualRadius.
// We import lazily to avoid circular deps — the instances are singletons.
let toolInstancesRef: Record<string, unknown> | null = null;
export function getToolInstances(): Record<string, unknown> {
  if (!toolInstancesRef) {
    // Populated on first call from Canvas.tsx via the exported setter
    throw new Error('TOOL_INSTANCES not registered');
  }
  return toolInstancesRef;
}
export function registerToolInstances(instances: Record<string, unknown>): void {
  toolInstancesRef = instances;
}


function GroupSeparator() {
  return <div className="w-10 h-px bg-bb-border my-0.5" />;
}

const LibraryLauncherIcon = ({ size = 24 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
    <rect x="3" y="3" width="7" height="7" rx="1.5" />
    <rect x="14" y="3" width="7" height="7" rx="1.5" />
    <rect x="3" y="14" width="7" height="7" rx="1.5" />
    <rect x="14" y="14" width="7" height="7" rx="1.5" />
  </svg>
);

export function CreationToolbar() {
  const { t } = useTranslation();
  const activeTool = useUiStore((s) => s.activeTool);
  const libraryDrawerOpen = useUiStore((s) => s.libraryDrawerOpen);
  const toggleLibraryDrawer = useUiStore((s) => s.toggleLibraryDrawer);
  const setActiveTool = useUiStore((s) => s.setActiveTool);
  const lastShapeSubTool = useUiStore((s) => s.lastShapeSubTool);
  const setLastShapeSubTool = useUiStore((s) => s.setLastShapeSubTool);

  // Determine if any shape tool is currently active (for highlight)
  const shapeToolTypes: ToolType[] = ['rect', 'ellipse', 'polygon', 'star'];
  const isShapeActive = shapeToolTypes.includes(activeTool);
  const shapeItems = SHAPE_ITEM_DEFS.map((item) => ({
    id: item.id,
    icon: item.icon,
    label: t(item.labelKey),
  }));

  const handleShapeSelect = useCallback(
    (subToolId: string) => {
      setLastShapeSubTool(subToolId);

      const toolType = SHAPE_TOOL_MAP[subToolId] ?? 'rect';

      // Configure polygon sides for presets
      if (POLYGON_SIDES[subToolId] !== undefined) {
        try {
          const instances = getToolInstances();
          const polygonTool = instances.polygon as PolygonTool;
          polygonTool.sides = POLYGON_SIDES[subToolId];
        } catch {
          // Tool instances not yet registered — will use default sides
        }
      }

      // Configure star dual-radius
      if (subToolId === 'dual_star' || subToolId === 'star') {
        try {
          const instances = getToolInstances();
          const starTool = instances.star as StarTool;
          starTool.dualRadius = subToolId === 'dual_star';
        } catch {
          // Tool instances not yet registered
        }
      }

      setActiveTool(toolType);
    },
    [setActiveTool, setLastShapeSubTool],
  );

  return (
    <div className="no-select w-16 bg-bb-panel py-1.5 gap-0.5 text-xs flex flex-col items-center">
      {/* Library drawer launcher (Art / Materials) */}
      <IconButton
        icon={<LibraryLauncherIcon size={24} />}
        label={t('panels.library.title')}
        onClick={toggleLibraryDrawer}
        active={libraryDrawerOpen}
        size={SMALL_BUTTON_SIZE}
      />
      <GroupSeparator />

      {/* Select */}
      <IconButton
        icon={<MousePointer2 size={24} />}
        label={t('toolbars.creation.select')}
        onClick={() => setActiveTool(TOOL_SELECT)}
        active={activeTool === TOOL_SELECT}
        size={SMALL_BUTTON_SIZE}
      />
      <GroupSeparator />

      {/* Draw (Pen) */}
      <IconButton
        icon={<PenToolIcon size={24} />}
        label={t('toolbars.creation.draw')}
        onClick={() => setActiveTool(TOOL_LINE)}
        active={activeTool === TOOL_LINE}
        size={SMALL_BUTTON_SIZE}
      />

      {/* Shapes submenu */}
      <ToolbarSubmenuButton
        items={shapeItems}
        activeItemId={lastShapeSubTool}
        onSelect={handleShapeSelect}
        isToolActive={isShapeActive}
        size={SMALL_BUTTON_SIZE}
      />

      {/* Text */}
      <IconButton
        icon={<Type size={24} />}
        label={t('toolbars.creation.text')}
        onClick={() => setActiveTool(TOOL_TEXT)}
        active={activeTool === TOOL_TEXT}
        size={SMALL_BUTTON_SIZE}
      />

      <GroupSeparator />

      {/* Node Edit / Trim / Tabs */}
      <IconButton
        icon={<NodeEditIcon size={24} />}
        label={t('toolbars.creation.node_edit')}
        onClick={() => setActiveTool(TOOL_NODE)}
        active={activeTool === TOOL_NODE}
        size={SMALL_BUTTON_SIZE}
      />
      <IconButton
        icon={<ScissorsLineDashed size={24} />}
        label={t('toolbars.creation.trim')}
        onClick={() => setActiveTool(TOOL_TRIM)}
        active={activeTool === TOOL_TRIM}
        size={SMALL_BUTTON_SIZE}
      />
      <IconButton
        icon={<TabsIcon size={24} />}
        label={t('toolbars.creation.tabs')}
        onClick={() => setActiveTool(TOOL_TABS)}
        active={activeTool === TOOL_TABS}
        size={SMALL_BUTTON_SIZE}
      />

      <GroupSeparator />

      {/* Laser Position / Measure */}
      <IconButton
        icon={<MapPin size={24} />}
        label={t('toolbars.creation.laser_position')}
        onClick={() => setActiveTool(TOOL_LASER_POSITION)}
        active={activeTool === TOOL_LASER_POSITION}
        size={SMALL_BUTTON_SIZE}
      />
      <IconButton
        icon={<Ruler size={24} />}
        label={t('toolbars.creation.measure')}
        onClick={() => setActiveTool(TOOL_MEASURE)}
        active={activeTool === TOOL_MEASURE}
        size={SMALL_BUTTON_SIZE}
      />
    </div>
  );
}
