import { useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { useUiStore } from '../../stores/uiStore';
import { getPanelById } from '../../panels/panelRegistry';
import type { PhysicalDockZone } from '../../panels';
import type { ContextMenuEntry } from '../shared/ContextMenu';
import { buildPanelTabMenuItems } from './panelTabMenuItems';

interface MenuState {
  visible: boolean;
  x: number;
  y: number;
  items: ContextMenuEntry[];
}

const CLOSED: MenuState = { visible: false, x: 0, y: 0, items: [] };

export function usePanelTabContextMenu(zone: PhysicalDockZone) {
  const { t } = useTranslation();
  const [menuState, setMenuState] = useState<MenuState>(CLOSED);

  const closeMenu = useCallback(() => setMenuState(CLOSED), []);

  const handleTabContextMenu = useCallback(
    (panelId: string, e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();

      const state = useUiStore.getState();
      const items = buildPanelTabMenuItems(t, {
        panelId,
        mode: 'docked',
        sidePanelsVisible: state.sidePanelsVisible,
        onFloat: (id) => {
          const panelDef = getPanelById(id);
          const size = panelDef?.defaultFloatSize ?? { w: 384, h: 300 };
          useUiStore.getState().floatPanel(id, 100, 100, size.w, size.h);
        },
        onClose: (id) => {
          useUiStore.getState().removePanelInstance(id);
        },
        onAddPanel: (id) => {
          useUiStore.getState().addPanelInstance(id, zone);
        },
        onToggleSidePanels: () => {
          useUiStore.getState().toggleSidePanels();
        },
        onDock: () => {}, // not used in docked mode
      });

      setMenuState({ visible: true, x: e.clientX, y: e.clientY, items });
    },
    [t, zone],
  );

  return { menuState, handleTabContextMenu, closeMenu };
}
