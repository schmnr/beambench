import { useRef, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { useUiStore } from '../../stores/uiStore';
import { getPanelById, getPanelComponent, getWorkspacePanelLayout, PANEL_REGISTRY, PanelHost } from '../../panels';
import type { PhysicalDockZone } from '../../panels';
import { TabBar } from '../shared/TabBar';
import { appService } from '../../services/appService';
import { usePanelDnd } from '../../panels/DndContext';
import { ContextMenu } from '../shared/ContextMenu';
import { usePanelTabContextMenu } from '../panels/usePanelTabContextMenu';

export function BottomPanel() {
  const { t } = useTranslation();
  const zone: PhysicalDockZone = 'bottom';
  const panelLayout = useUiStore((s) => s.panelLayout);
  const workspaceMode = useUiStore((s) => s.workspaceMode);
  const setZoneActiveTab = useUiStore((s) => s.setZoneActiveTab);
  const addPanelInstance = useUiStore((s) => s.addPanelInstance);
  const { dragState, startDrag, registerDropZone } = usePanelDnd();
  const { menuState, handleTabContextMenu, closeMenu } = usePanelTabContextMenu(zone);

  const workspaceLayout = getWorkspacePanelLayout(panelLayout, workspaceMode);
  const zoneState = workspaceLayout.zones[zone];
  const hiddenIds = workspaceLayout.hiddenPanelIds;

  const visiblePanelIds = zoneState.panelIds.filter((id) => !hiddenIds.includes(id));
  const tabs = visiblePanelIds.map((id) => {
    const def = getPanelById(id);
    const suffix = id.includes('::') ? ` ${id.split('::').slice(-1)[0]}` : '';
    return { id, label: def ? `${t(def.titleKey)}${suffix}` : id };
  });

  const activeTab = visiblePanelIds.includes(zoneState.activeTab)
    ? zoneState.activeTab
    : visiblePanelIds[0] ?? '';

  const PanelContent = activeTab ? getPanelComponent(activeTab) : null;
  const panelPlacement = { kind: 'docked', zone } as const;

  const handleTabChange = (tabId: string) => {
    setZoneActiveTab(zone, tabId);
    appService.persistLayout(useUiStore.getState().panelLayout);
  };

  const zoneRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    registerDropZone(zone, zoneRef.current);
    return () => registerDropZone(zone, null);
  }, [zone, registerDropZone]);

  let dropInsertIndex: number | null = null;
  if (dragState?.isDragging && dragState.activeDropTarget?.type === 'zone' && dragState.activeDropTarget.zone === zone) {
    dropInsertIndex = dragState.activeDropTarget.insertIndex;
  }

  if (tabs.length === 0) {
    return (
      <div
        ref={zoneRef}
        className="flex h-full w-full items-start justify-center bg-bb-bg px-3 pt-3"
        onContextMenu={(e) => e.preventDefault()}
        data-testid="empty-bottom-panel-zone"
      >
        <select
          value=""
          onChange={(event) => {
            if (event.target.value) addPanelInstance(event.target.value, zone);
          }}
          className="w-full max-w-md rounded-lg border border-dashed border-bb-border bg-bb-surface px-2 py-2 text-xs text-bb-text-muted outline-none hover:border-bb-accent/60 hover:text-bb-text focus:border-bb-accent"
          aria-label={t('context_menu.panels')}
          data-testid="empty-bottom-panel-picker"
        >
          <option value="">+ {t('context_menu.panels')}</option>
          {PANEL_REGISTRY.map((panel) => (
            <option key={panel.id} value={panel.id}>{t(panel.titleKey)}</option>
          ))}
        </select>
      </div>
    );
  }

  return (
    <div ref={zoneRef} className="flex h-full w-full flex-col bg-bb-panel" onContextMenu={(e) => e.preventDefault()}>
      <TabBar
        tabs={tabs}
        activeTab={activeTab}
        onTabChange={handleTabChange}
        onTabDragStart={(panelId, e) => startDrag(panelId, zone, e)}
        onTabContextMenu={handleTabContextMenu}
        dropInsertIndex={dropInsertIndex}
      />
      <div className="min-h-0 flex-1 overflow-hidden">
        {PanelContent && (
          <PanelHost panelInstanceId={activeTab} placement={panelPlacement}>
            <PanelContent />
          </PanelHost>
        )}
      </div>
      {menuState.visible && (
        <ContextMenu x={menuState.x} y={menuState.y} items={menuState.items} onClose={closeMenu} />
      )}
    </div>
  );
}
