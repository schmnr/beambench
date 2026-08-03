import type { ContextMenuEntry } from '../shared/ContextMenu';
import type { Layer, ProjectObject } from '../../types/project';
import type { TFunction } from 'i18next';

interface LayerMenuCallbacks {
  toggleEnabled: (layerId: string, enabled: boolean) => void;
  toggleVisible: (layerId: string, visible: boolean) => void;
  selectObjects: (objectIds: string[]) => void;
  copySettings?: (layer: Layer) => void;
  pasteSettings?: (layerId: string) => void;
  startRename?: (layerId: string) => void;
  deleteLayer?: (layerId: string) => void;
  toggleLockObjects?: (layerId: string) => void;
  hasClipboard?: boolean;

  // M4 additions
  /** "Disable all layers but this one" — keeps the row's layer enabled, disables every other. */
  disableAllButThis?: (layerId: string) => void;
  /** "Hide all layers but this one" — keeps the row's layer visible, hides every other. */
  hideAllButThis?: (layerId: string) => void;
  /** "Flash content on this layer" — transient canvas highlight; does not change selection. */
  flashLayer?: (layerId: string) => void;
}

export function buildLayerContextMenuItems(
  t: TFunction,
  layer: Layer,
  objects: ProjectObject[],
  callbacks: LayerMenuCallbacks,
): ContextMenuEntry[] {
  const layerObjects = objects.filter((o) => o.layer_id === layer.id);

  const items: ContextMenuEntry[] = [
    {
      id: 'toggle-enabled',
      label: layer.enabled ? t('common.disable') : t('common.enable'),
      disabled: layer.is_tool_layer,
      onClick: () => callbacks.toggleEnabled(layer.id, !layer.enabled),
    },
  ];

  if (callbacks.disableAllButThis && !layer.is_tool_layer) {
    items.push({
      id: 'disable-all-but-this',
      label: t('panels.layers.context_menu.disable_all_but_this'),
      onClick: () => callbacks.disableAllButThis!(layer.id),
    });
  }

  items.push({ type: 'separator' });

  items.push({
    id: 'toggle-visible',
    label: layer.visible === false ? t('common.show') : t('common.hide'),
    onClick: () => callbacks.toggleVisible(layer.id, layer.visible === false),
  });

  if (callbacks.hideAllButThis) {
    items.push({
      id: 'hide-all-but-this',
      label: t('panels.layers.context_menu.hide_all_but_this'),
      onClick: () => callbacks.hideAllButThis!(layer.id),
    });
  }

  items.push({ type: 'separator' });

  items.push({
    id: 'select-layer-objects',
    label: t('panels.layers.context_menu.select_all_on_layer'),
    disabled: layerObjects.length === 0,
    onClick: () => callbacks.selectObjects(layerObjects.map((o) => o.id)),
  });

  if (callbacks.flashLayer) {
    items.push({
      id: 'flash-layer',
      label: t('panels.layers.context_menu.flash_content'),
      disabled: layerObjects.length === 0,
      onClick: () => callbacks.flashLayer!(layer.id),
    });
  }

  // Copy/Paste/Rename section
  if (callbacks.copySettings || callbacks.pasteSettings || callbacks.startRename) {
    items.push({ type: 'separator' });

    if (callbacks.copySettings) {
      items.push({
        id: 'copy-settings',
        label: t('panels.layers.context_menu.copy_settings'),
        disabled: layer.is_tool_layer,
        onClick: () => callbacks.copySettings!(layer),
      });
    }

    if (callbacks.pasteSettings) {
      items.push({
        id: 'paste-settings',
        label: t('panels.layers.context_menu.paste_settings'),
        disabled: layer.is_tool_layer || !callbacks.hasClipboard,
        onClick: () => callbacks.pasteSettings!(layer.id),
      });
    }

    if (callbacks.startRename) {
      items.push({
        id: 'rename',
        label: t('common.rename'),
        onClick: () => callbacks.startRename!(layer.id),
      });
    }

    if (callbacks.toggleLockObjects) {
      items.push({
        id: 'toggle-lock-objects',
        label: t('panels.layers.toggle_lock_title'),
        disabled: layerObjects.length === 0,
        onClick: () => callbacks.toggleLockObjects!(layer.id),
      });
    }

    if (callbacks.deleteLayer) {
      items.push({
        id: 'delete-layer',
        label: t('panels.layers.delete_layer'),
        onClick: () => callbacks.deleteLayer!(layer.id),
      });
    }
  }

  return items;
}
