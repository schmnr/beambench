import type { TFunction } from 'i18next';
import type { ContextMenuEntry } from '../shared/ContextMenu';
import { PANEL_REGISTRY, getPanelById } from '../../panels/panelRegistry';

export type PanelTabMenuMode = 'docked' | 'floating';

export interface PanelTabMenuContext {
  panelId: string;
  mode: PanelTabMenuMode;
  hiddenPanelIds: string[];
  sidePanelsVisible: boolean;
  onFloat: (panelId: string) => void;
  onDock: (panelId: string) => void;
  onClose: (panelId: string) => void;
  onTogglePanel: (panelId: string) => void;
  onToggleSidePanels: () => void;
}

function buildPanelsSubmenu(t: TFunction, ctx: PanelTabMenuContext): ContextMenuEntry {
  const children: ContextMenuEntry[] = [
    {
      type: 'check',
      id: 'panel-tab-side-panels',
      label: t('context_menu.side_panels'),
      checked: ctx.sidePanelsVisible,
      onClick: () => ctx.onToggleSidePanels(),
    },
    { type: 'separator' },
    ...PANEL_REGISTRY.map((def) => ({
      type: 'check' as const,
      id: `panel-tab-${def.id}`,
      label: def.titleKey ? t(def.titleKey) : def.title,
      checked: !ctx.hiddenPanelIds.includes(def.id),
      onClick: () => ctx.onTogglePanel(def.id),
    })),
  ];

  return {
    type: 'submenu',
    id: 'panel-tab-panels-submenu',
    label: t('context_menu.panels'),
    children,
  };
}

export function buildPanelTabMenuItems(t: TFunction, ctx: PanelTabMenuContext): ContextMenuEntry[] {
  const def = getPanelById(ctx.panelId);
  const items: ContextMenuEntry[] = [];

  if (ctx.mode === 'docked') {
    if (def?.supportsFloat !== false) {
      items.push({
        id: 'panel-tab-float',
        label: t('context_menu.float'),
        onClick: () => ctx.onFloat(ctx.panelId),
      });
    }
    if (def?.supportsClose !== false) {
      items.push({
        id: 'panel-tab-close',
        label: t('common.close'),
        onClick: () => ctx.onClose(ctx.panelId),
      });
    }
  } else {
    // floating
    items.push({
      id: 'panel-tab-dock',
      label: t('context_menu.dock'),
      onClick: () => ctx.onDock(ctx.panelId),
    });
    if (def?.supportsClose !== false) {
      items.push({
        id: 'panel-tab-close',
        label: t('common.close'),
        onClick: () => ctx.onClose(ctx.panelId),
      });
    }
  }

  items.push({ type: 'separator' });
  items.push(buildPanelsSubmenu(t, ctx));

  return items;
}
