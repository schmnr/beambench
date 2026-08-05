import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useMachineStore } from '../../stores/machineStore';
import { useUiStore } from '../../stores/uiStore';
import {
  COLUMN_ZONES,
  getPanelComponent,
  getPanelById,
  getWorkspacePanelLayout,
  PANEL_REGISTRY,
  PanelHost,
} from '../../panels';
import type { ColumnDockZone, PanelColumnSide } from '../../panels';
import { TabBar } from '../shared/TabBar';
import { ZoneSplitter } from './ZoneSplitter';
import { appService } from '../../services/appService';
import { usePanelDnd } from '../../panels/DndContext';
import { ContextMenu } from '../shared/ContextMenu';
import { usePanelTabContextMenu } from '../panels/usePanelTabContextMenu';
import { DeviceSettingsDialog } from '../dialogs/DeviceSettingsDialog';

const MACHINE_PROFILE_SYMBOL = '⌗';
const TOP_PANEL_EDGE = 'top' as const;
const BOTTOM_PANEL_EDGE = 'bottom' as const;

function DockZonePanel({ zone }: { zone: ColumnDockZone }) {
  const { t } = useTranslation();
  const panelLayout = useUiStore((state) => state.panelLayout);
  const workspaceMode = useUiStore((state) => state.workspaceMode);
  const setZoneActiveTab = useUiStore((state) => state.setZoneActiveTab);
  const addPanelInstance = useUiStore((state) => state.addPanelInstance);
  const { dragState, startDrag, registerDropZone } = usePanelDnd();
  const { menuState, handleTabContextMenu, closeMenu } = usePanelTabContextMenu(zone);
  const workspaceLayout = getWorkspacePanelLayout(panelLayout, workspaceMode);
  const zoneState = workspaceLayout.zones[zone];
  const visiblePanelIds = zoneState.panelIds.filter(
    (panelId) => !workspaceLayout.hiddenPanelIds.includes(panelId),
  );
  const tabs = visiblePanelIds.map((panelId) => {
    const definition = getPanelById(panelId);
    const instanceSuffix = panelId.includes('::') ? ` ${panelId.split('::').slice(-1)[0]}` : '';
    return { id: panelId, label: definition ? `${t(definition.titleKey)}${instanceSuffix}` : panelId };
  });
  const activeTab = visiblePanelIds.includes(zoneState.activeTab)
    ? zoneState.activeTab
    : visiblePanelIds[0] ?? '';
  const PanelContent = activeTab ? getPanelComponent(activeTab) : null;
  const panelPlacement = { kind: 'docked', zone } as const;
  const zoneRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    registerDropZone(zone, zoneRef.current);
    return () => registerDropZone(zone, null);
  }, [registerDropZone, zone]);

  const dropInsertIndex = dragState?.isDragging
    && dragState.activeDropTarget?.type === 'zone'
    && dragState.activeDropTarget.zone === zone
    ? dragState.activeDropTarget.insertIndex
    : null;

  if (tabs.length === 0) {
    return (
      <div
        ref={zoneRef}
        className="flex h-full items-start justify-center px-3 pt-3"
        onContextMenu={(event) => event.preventDefault()}
        data-testid={`empty-panel-zone-${zone}`}
      >
        <select
          value=""
          onChange={(event) => {
            if (event.target.value) addPanelInstance(event.target.value, zone);
          }}
          className="w-full rounded-lg border border-dashed border-bb-border bg-bb-surface px-2 py-2 text-xs text-bb-text-muted outline-none hover:border-bb-accent/60 hover:text-bb-text focus:border-bb-accent"
          aria-label={t('context_menu.panels')}
          data-testid={`empty-zone-panel-picker-${zone}`}
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
    <div
      ref={zoneRef}
      className="flex h-full flex-col overflow-hidden"
      onContextMenu={(event) => event.preventDefault()}
      data-testid={`panel-zone-${zone}`}
    >
      <TabBar
        tabs={tabs}
        activeTab={activeTab}
        onTabChange={(panelId) => {
          setZoneActiveTab(zone, panelId);
          appService.persistLayout(useUiStore.getState().panelLayout);
        }}
        onTabDragStart={(panelId, event) => startDrag(panelId, zone, event)}
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

export function PanelColumn({ side, showMachineProfile = false }: {
  side: PanelColumnSide;
  showMachineProfile?: boolean;
}) {
  const { t } = useTranslation();
  const panelLayout = useUiStore((state) => state.panelLayout);
  const workspaceMode = useUiStore((state) => state.workspaceMode);
  const setColumnBoundary = useUiStore((state) => state.setColumnBoundary);
  const revealColumnEdge = useUiStore((state) => state.revealColumnEdge);
  const workspaceLayout = getWorkspacePanelLayout(panelLayout, workspaceMode);
  const ratios = workspaceLayout.columnRatios[side];
  const zones = COLUMN_ZONES[side];
  const activeIndices = ratios
    .map((ratio, index) => ratio > 0 ? index : -1)
    .filter((index) => index >= 0);
  const activeProfile = useMachineStore(
    (state) => (state.profiles ?? []).find((profile) => profile.id === state.activeProfileId) ?? null,
  );
  const [showProfiles, setShowProfiles] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const canSplit = activeIndices.length < 3;
  const bottomBoundary = activeIndices.length <= 1 ? 0 : 1;

  return (
    <div
      ref={containerRef}
      className="no-select flex h-full w-full flex-col bg-bb-bg px-2 pb-2 pt-1.5"
      onContextMenu={(event) => event.preventDefault()}
      data-testid={`panel-column-${side}`}
    >
      {showMachineProfile && (
        <button
          className="z-10 -mb-px self-start rounded-t-lg bg-bb-accent px-3 py-1 text-xxs font-bold text-bb-on-accent hover:bg-bb-accent-hover"
          onClick={() => setShowProfiles(true)}
          title={t('panels.machine.laser.manage_machine_profiles')}
          data-testid="machine-profile-chip"
        >
          {MACHINE_PROFILE_SYMBOL} {activeProfile?.name ?? t('panels.machine.laser.no_machine')}
        </button>
      )}

      {canSplit && (
        <ZoneSplitter
          containerRef={containerRef}
          edge={TOP_PANEL_EDGE}
          onBeforeDrag={() => revealColumnEdge(side, TOP_PANEL_EDGE)}
          onDragRatio={(ratio) => setColumnBoundary(side, 0, ratio)}
          testId={`panel-column-${side}-top-reveal-handle`}
        />
      )}

      {activeIndices.map((zoneIndex, visibleIndex) => (
        <div key={zones[zoneIndex]} className="contents">
          <div
            className={`flex min-h-0 flex-col overflow-hidden border border-bb-border bg-bb-panel shadow-lg ${
              showMachineProfile && visibleIndex === 0
                ? 'rounded-b-xl rounded-tr-xl'
                : 'rounded-xl'
            }`}
            style={{ flex: ratios[zoneIndex] }}
          >
            <DockZonePanel zone={zones[zoneIndex]} />
          </div>
          {visibleIndex < activeIndices.length - 1 && (
            <ZoneSplitter
              containerRef={containerRef}
              onDragRatio={(ratio) => setColumnBoundary(
                side,
                zoneIndex as 0 | 1,
                ratio,
                activeIndices[visibleIndex + 1] as 1 | 2,
              )}
              testId={`panel-column-${side}-splitter-${zoneIndex}`}
            />
          )}
        </div>
      ))}

      {canSplit && (
        <ZoneSplitter
          containerRef={containerRef}
          edge={BOTTOM_PANEL_EDGE}
          onBeforeDrag={() => revealColumnEdge(side, BOTTOM_PANEL_EDGE)}
          onDragRatio={(ratio) => setColumnBoundary(side, bottomBoundary as 0 | 1, ratio)}
          testId={`panel-column-${side}-bottom-reveal-handle`}
        />
      )}

      {showProfiles && <DeviceSettingsDialog onClose={() => setShowProfiles(false)} />}
    </div>
  );
}
