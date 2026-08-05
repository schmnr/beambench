import { useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { useUiStore } from '../../stores/uiStore';
import { getPanelById, getPanelComponent, getWorkspacePanelLayout, PanelHost, type PhysicalDockZone } from '../../panels';
import { FloatingPanel } from './FloatingPanel';
import { ContextMenu, type ContextMenuEntry } from '../shared/ContextMenu';
import { buildPanelTabMenuItems } from '../panels/panelTabMenuItems';

interface MenuState {
  visible: boolean;
  x: number;
  y: number;
  items: ContextMenuEntry[];
}

const CLOSED: MenuState = { visible: false, x: 0, y: 0, items: [] };
const FLOATING_PANEL_PLACEMENT = { kind: 'floating' } as const;

/** Dock zone to fall back to when re-docking a panel whose default zone is floating. */
function resolveDockFallback(defaultZone: string): string {
  return defaultZone === 'floating' ? 'top-right' : defaultZone;
}

export function FloatingPanelLayer() {
  const { t } = useTranslation();
  const panelLayout = useUiStore((s) => s.panelLayout);
  const workspaceMode = useUiStore((s) => s.workspaceMode);
  const workspaceLayout = getWorkspacePanelLayout(panelLayout, workspaceMode);
  const floatingPanels = workspaceLayout.floatingPanels;
  const hiddenPanelIds = workspaceLayout.hiddenPanelIds;
  const moveFloatingPanel = useUiStore((s) => s.moveFloatingPanel);
  const resizeFloatingPanel = useUiStore((s) => s.resizeFloatingPanel);
  const bringToFront = useUiStore((s) => s.bringToFront);
  const removePanelInstance = useUiStore((s) => s.removePanelInstance);
  const dockPanel = useUiStore((s) => s.dockPanel);

  const [menuState, setMenuState] = useState<MenuState>(CLOSED);
  const closeMenu = useCallback(() => setMenuState(CLOSED), []);

  const handleTitleContextMenu = useCallback((panelId: string, e: React.MouseEvent) => {
    const state = useUiStore.getState();
    const activeLayout = getWorkspacePanelLayout(state.panelLayout, state.workspaceMode);
    const fp = activeLayout.floatingPanels.find((f) => f.panelId === panelId);

    const items = buildPanelTabMenuItems(t, {
      panelId,
      mode: 'floating',
      sidePanelsVisible: state.sidePanelsVisible,
      onFloat: () => {}, // already floating
      onDock: (id) => {
        const panelDef = getPanelById(id);
        const fallback = panelDef?.defaultZone === 'floating' ? 'top-right' : panelDef?.defaultZone ?? 'top-right';
        useUiStore.getState().dockPanel(id, (fp?.originZone ?? fallback) as PhysicalDockZone, fp?.originIndex);
      },
      onClose: (id) => {
        useUiStore.getState().removePanelInstance(id);
      },
      onAddPanel: (id) => {
        const panelDef = getPanelById(id);
        const target = resolveDockFallback(panelDef?.defaultZone ?? 'top-right') as PhysicalDockZone;
        useUiStore.getState().addPanelInstance(id, target);
      },
      onToggleSidePanels: () => {
        useUiStore.getState().toggleSidePanels();
      },
    });

    setMenuState({ visible: true, x: e.clientX, y: e.clientY, items });
  }, [t]);

  const visibleFloating = floatingPanels.filter((fp) => !hiddenPanelIds.includes(fp.panelId));
  if (visibleFloating.length === 0 && !menuState.visible) return null;

  return (
    <>
      {visibleFloating.map((fp) => {
        const def = getPanelById(fp.panelId);
        if (!def) return null;
        const PanelContent = getPanelComponent(fp.panelId);
        if (!PanelContent) return null;
        const dockFallbackZone = resolveDockFallback(def.defaultZone);

        return (
          <FloatingPanel
            key={fp.panelId}
            panelId={fp.panelId}
            title={`${def.titleKey ? t(def.titleKey) : def.title}${
              fp.panelId.includes('::') ? ` ${fp.panelId.split('::').slice(-1)[0]}` : ''
            }`}
            x={fp.x}
            y={fp.y}
            width={fp.width}
            height={fp.height}
            zIndex={fp.zIndex}
            minWidth={def.minFloatSize?.w}
            minHeight={def.minFloatSize?.h}
            onClose={() => removePanelInstance(fp.panelId)}
            onDock={() => {
              dockPanel(fp.panelId, (fp.originZone ?? dockFallbackZone) as PhysicalDockZone, fp.originIndex);
            }}
            onMove={(x, y) => moveFloatingPanel(fp.panelId, x, y)}
            onResize={(w, h) => resizeFloatingPanel(fp.panelId, w, h)}
            onFocus={() => bringToFront(fp.panelId)}
            onTitleContextMenu={(e) => handleTitleContextMenu(fp.panelId, e)}
          >
            <PanelHost panelInstanceId={fp.panelId} placement={FLOATING_PANEL_PLACEMENT}>
              <PanelContent />
            </PanelHost>
          </FloatingPanel>
        );
      })}
      {menuState.visible && (
        <ContextMenu x={menuState.x} y={menuState.y} items={menuState.items} onClose={closeMenu} />
      )}
    </>
  );
}
