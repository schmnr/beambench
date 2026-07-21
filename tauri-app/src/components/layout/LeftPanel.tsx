import { useRef, useEffect, useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { useUiStore } from '../../stores/uiStore';
import { getPanelById, PANEL_COMPONENTS } from '../../panels';
import type { PhysicalDockZone } from '../../panels';
import { TabBar } from '../shared/TabBar';
import { appService } from '../../services/appService';
import { usePanelDnd } from '../../panels/DndContext';
import { ContextMenu } from '../shared/ContextMenu';
import { usePanelTabContextMenu } from '../panels/usePanelTabContextMenu';
import { buildPanelTabMenuItems } from '../panels/panelTabMenuItems';
import type { ContextMenuEntry } from '../shared/ContextMenu';

interface CompactMenuState {
  visible: boolean;
  x: number;
  y: number;
  items: ContextMenuEntry[];
}

const COMPACT_CLOSED: CompactMenuState = { visible: false, x: 0, y: 0, items: [] };

export interface LeftPanelProps {
  compact?: boolean;
}

export function LeftPanel({ compact }: LeftPanelProps) {
  const { t } = useTranslation();
  const zone: PhysicalDockZone = 'left';
  const panelLayout = useUiStore((s) => s.panelLayout);
  const setZoneActiveTab = useUiStore((s) => s.setZoneActiveTab);
  const floatPanel = useUiStore((s) => s.floatPanel);
  const { dragState, startDrag, registerDropZone } = usePanelDnd();
  const { menuState, handleTabContextMenu, closeMenu } = usePanelTabContextMenu(zone);

  const [compactMenu, setCompactMenu] = useState<CompactMenuState>(COMPACT_CLOSED);
  const closeCompactMenu = useCallback(() => setCompactMenu(COMPACT_CLOSED), []);

  const zoneState = panelLayout.zones[zone];
  const hiddenIds = panelLayout.hiddenPanelIds;

  const visiblePanelIds = zoneState.panelIds.filter((id) => !hiddenIds.includes(id));
  const tabs = visiblePanelIds.map((id) => {
    const def = getPanelById(id);
    return { id, label: def ? t(def.titleKey) : id };
  });

  const activeTab = visiblePanelIds.includes(zoneState.activeTab)
    ? zoneState.activeTab
    : visiblePanelIds[0] ?? '';

  const PanelContent = activeTab ? (PANEL_COMPONENTS[activeTab] ?? null) : null;

  const handleTabChange = (tabId: string) => {
    setZoneActiveTab(zone, tabId);
    appService.persistLayout(useUiStore.getState().panelLayout);
  };

  const handleFloatPanel = (panelId: string) => {
    const def = getPanelById(panelId);
    const size = def?.defaultFloatSize ?? { w: 384, h: 300 };
    floatPanel(panelId, 100, 100, size.w, size.h);
  };

  const handleCompactContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();

    const panelId = activeTab;
    if (!panelId) return;

    const state = useUiStore.getState();

    const items = buildPanelTabMenuItems(t, {
      panelId,
      mode: 'docked',
      hiddenPanelIds: state.panelLayout.hiddenPanelIds,
      sidePanelsVisible: state.sidePanelsVisible,
      onFloat: (id) => {
        const panelDef = getPanelById(id);
        const size = panelDef?.defaultFloatSize ?? { w: 384, h: 300 };
        useUiStore.getState().floatPanel(id, 100, 100, size.w, size.h);
      },
      onClose: (id) => {
        useUiStore.getState().togglePanelVisibility(id);
      },
      onTogglePanel: (id) => {
        if (id === 'camera') {
          useUiStore.getState().toggleCameraWindow();
        } else {
          useUiStore.getState().togglePanelVisibility(id);
        }
      },
      onToggleSidePanels: () => {
        useUiStore.getState().toggleSidePanels();
      },
      onDock: () => {},
    });

    setCompactMenu({ visible: true, x: e.clientX, y: e.clientY, items });
  }, [activeTab, t]);

  const zoneRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    registerDropZone(zone, zoneRef.current);
    return () => registerDropZone(zone, null);
  }, [zone, registerDropZone]);

  let dropInsertIndex: number | null = null;
  if (dragState?.isDragging && dragState.activeDropTarget?.type === 'zone' && dragState.activeDropTarget.zone === zone) {
    dropInsertIndex = dragState.activeDropTarget.insertIndex;
  }

  if (tabs.length === 0) return <div ref={zoneRef} className="h-full" onContextMenu={(e) => e.preventDefault()} />;

  if (compact) {
    return (
      <div ref={zoneRef} className="h-full bg-bb-panel" onContextMenu={handleCompactContextMenu}>
        {PanelContent && <PanelContent />}
        {compactMenu.visible && (
          <ContextMenu x={compactMenu.x} y={compactMenu.y} items={compactMenu.items} onClose={closeCompactMenu} />
        )}
      </div>
    );
  }

  return (
    <div ref={zoneRef} className="h-full flex flex-col bg-bb-panel" onContextMenu={(e) => e.preventDefault()}>
      <TabBar
        tabs={tabs}
        activeTab={activeTab}
        onTabChange={handleTabChange}
        zone={zone}
        onTabDragStart={(panelId, e) => startDrag(panelId, zone, e)}
        onFloatPanel={handleFloatPanel}
        onTabContextMenu={handleTabContextMenu}
        dropInsertIndex={dropInsertIndex}
      />
      <div className="flex-1 min-h-0 overflow-y-auto">
        {PanelContent && <PanelContent />}
      </div>
      {menuState.visible && (
        <ContextMenu x={menuState.x} y={menuState.y} items={menuState.items} onClose={closeMenu} />
      )}
    </div>
  );
}
