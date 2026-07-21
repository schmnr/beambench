import type { ContextMenuEntry } from '../shared/ContextMenu';
import type { LayerBatchToggle } from '../../types/project';
import type { TFunction } from 'i18next';

/**
 * M4 header-strip context menu for the Cuts/Layers panel.
 *
 * Right-clicking the column header opens this menu carrying the global Output/Show batch toggles
 * plus Sort Cuts Last. All three sections route through atomic backend ops with one undo
 * snapshot each. Renders via the shared `ContextMenu` component (consumer wires up open/close
 * state and positioning).
 */
export interface LayerListHeaderMenuCallbacks {
  setAllLayersEnabled: (mode: LayerBatchToggle) => void;
  setAllLayersVisible: (mode: LayerBatchToggle) => void;
  sortLayersCutLast: () => void;
}

export function buildLayerListHeaderMenuItems(
  t: TFunction,
  callbacks: LayerListHeaderMenuCallbacks,
): ContextMenuEntry[] {
  return [
    {
      id: 'enable-all',
      label: t('panels.layers.header_menu.enable_all'),
      onClick: () => callbacks.setAllLayersEnabled({ kind: 'all_on' }),
    },
    {
      id: 'disable-all',
      label: t('panels.layers.header_menu.disable_all'),
      onClick: () => callbacks.setAllLayersEnabled({ kind: 'all_off' }),
    },
    {
      id: 'invert-enabled',
      label: t('panels.layers.header_menu.invert_enabled'),
      onClick: () => callbacks.setAllLayersEnabled({ kind: 'invert' }),
    },
    { type: 'separator' },
    {
      id: 'show-all',
      label: t('panels.layers.header_menu.show_all'),
      onClick: () => callbacks.setAllLayersVisible({ kind: 'all_on' }),
    },
    {
      id: 'hide-all',
      label: t('panels.layers.header_menu.hide_all'),
      onClick: () => callbacks.setAllLayersVisible({ kind: 'all_off' }),
    },
    {
      id: 'invert-visibility',
      label: t('panels.layers.header_menu.invert_visibility'),
      onClick: () => callbacks.setAllLayersVisible({ kind: 'invert' }),
    },
    { type: 'separator' },
    {
      id: 'sort-cuts-last',
      label: t('panels.layers.header_menu.sort_cuts_last'),
      onClick: () => callbacks.sortLayersCutLast(),
    },
  ];
}
