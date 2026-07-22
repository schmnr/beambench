import { useRef, useEffect, useState } from 'react';
import { useMachineStore } from '../../stores/machineStore';
import { DeviceSettingsDialog } from '../dialogs/DeviceSettingsDialog';
import { useTranslation } from 'react-i18next';
import { useUiStore } from '../../stores/uiStore';
import { getPanelById, PANEL_COMPONENTS } from '../../panels';
import type { PhysicalDockZone } from '../../panels';
import { TabBar } from '../shared/TabBar';
import { ZoneSplitter } from './ZoneSplitter';
import { appService } from '../../services/appService';
import { usePanelDnd } from '../../panels/DndContext';
import { ContextMenu } from '../shared/ContextMenu';
import { usePanelTabContextMenu } from '../panels/usePanelTabContextMenu';

function ZonePanel({ zone }: { zone: PhysicalDockZone }) {
  const { t } = useTranslation();
  const panelLayout = useUiStore((s) => s.panelLayout);
  const setZoneActiveTab = useUiStore((s) => s.setZoneActiveTab);
  const floatPanel = useUiStore((s) => s.floatPanel);
  const { dragState, startDrag, registerDropZone } = usePanelDnd();
  const { menuState, handleTabContextMenu, closeMenu } = usePanelTabContextMenu(zone);

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

  const zoneRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    registerDropZone(zone, zoneRef.current);
    return () => registerDropZone(zone, null);
  }, [zone, registerDropZone]);

  // Compute drop insert index for this zone from drag state
  let dropInsertIndex: number | null = null;
  if (dragState?.isDragging && dragState.activeDropTarget?.type === 'zone' && dragState.activeDropTarget.zone === zone) {
    dropInsertIndex = dragState.activeDropTarget.insertIndex;
  }

  if (tabs.length === 0) return <div ref={zoneRef} className="h-full" onContextMenu={(e) => e.preventDefault()} />;

  return (
    <div ref={zoneRef} className="h-full flex flex-col overflow-hidden" onContextMenu={(e) => e.preventDefault()}>
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

export function RightPanel() {
  const { t } = useTranslation();
  const upperSplitRatio = useUiStore((s) => s.panelLayout.upperSplitRatio);
  const activeProfile = useMachineStore(
    (s) => (s.profiles ?? []).find((p) => p.id === s.activeProfileId) ?? null,
  );
  const [showProfiles, setShowProfiles] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  return (
    <div
      ref={containerRef}
      className="no-select h-full bg-bb-bg flex flex-col px-2 pb-2 pt-1.5"
      onContextMenu={(e) => e.preventDefault()}
    >
      {/* Machine profile chip — NodeCraft artboard-chip style */}
      <button
        className="z-10 -mb-px self-start rounded-t-lg bg-bb-accent px-3 py-1 text-xxs font-bold text-bb-on-accent hover:bg-bb-accent-hover"
        onClick={() => setShowProfiles(true)}
        title={t('panels.machine.laser.manage_machine_profiles')}
        data-testid="machine-profile-chip"
      >
        ⌗ {activeProfile?.name ?? t('panels.machine.laser.no_machine')}
      </button>

      {/* Upper zone card */}
      <div
        className="flex flex-col min-h-0 overflow-hidden rounded-b-xl rounded-tr-xl border border-bb-border bg-bb-panel shadow-lg"
        style={{ flex: upperSplitRatio }}
      >
        <ZonePanel zone="upper-right" />
      </div>

      {/* Splitter */}
      <ZoneSplitter containerRef={containerRef} />

      {/* Lower zone card */}
      <div
        className="flex flex-col min-h-0 overflow-hidden rounded-xl border border-bb-border bg-bb-panel shadow-lg"
        style={{ flex: 1 - upperSplitRatio }}
      >
        <ZonePanel zone="lower-right" />
      </div>

      {showProfiles && <DeviceSettingsDialog onClose={() => setShowProfiles(false)} />}
    </div>
  );
}
