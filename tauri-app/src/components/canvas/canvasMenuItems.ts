import type { TFunction } from 'i18next';
import type { ContextMenuEntry } from '../shared/ContextMenu';
import type { SelectionContext } from '../../commands/selectionContext';
import {
  clipboardCut,
  clipboardCopy,
  clipboardPaste,
  clipboardPasteInPlace,
  clipboardDuplicate,
  hasClipboardData,
} from '../../utils/clipboard';
import { pasteClipboardArtworkFromSystem } from '../../utils/systemClipboard';
import { useProjectStore } from '../../stores/projectStore';
import { usePreviewStore } from '../../stores/previewStore';
import { useUiStore } from '../../stores/uiStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { PANEL_REGISTRY, type PhysicalDockZone } from '../../panels/panelRegistry';
import { appService } from '../../services/appService';
import { useAppStore } from '../../stores/appStore';
import { APP_COMMANDS } from '../../commands/appCommandIds';
import { getEffectiveHotkey } from '../../commands/commandRegistry';

export interface CanvasMenuCallbacks {
  onTraceImage?: () => void;
  onAdjustImage?: () => void;
  onSaveProcessedBitmap?: () => void;
}

export function buildCanvasContextMenuItems(t: TFunction, ctx: SelectionContext, callbacks?: CanvasMenuCallbacks): ContextMenuEntry[] {
  const nodeMode = useUiStore.getState().activeTool === 'node';
  const customHotkeys = useAppStore.getState().settings?.custom_hotkeys ?? {};
  const shortcutFor = (commandId: string) => getEffectiveHotkey(commandId, customHotkeys) ?? undefined;
  const runNodeEditAction = (
    action: 'copy' | 'cut' | 'paste' | 'extract' | 'delete' | 'select_all',
  ) => {
    window.dispatchEvent(new CustomEvent('bb:node-edit-action', { detail: action }));
  };
  // --- Windows submenu ---
  const windowsSubmenu: ContextMenuEntry = {
    type: 'submenu',
    id: 'windows',
    label: t('context_menu.windows'),
    children: [
      {
        type: 'check',
        id: 'window-side-panels',
        label: t('context_menu.side_panels'),
        checked: useUiStore.getState().sidePanelsVisible,
        onClick: () => useUiStore.getState().toggleSidePanels(),
      },
      { type: 'separator' },
      ...PANEL_REGISTRY.map((panel) => ({
        type: 'check' as const,
        id: `window-${panel.id}`,
        label: panel.titleKey ? t(panel.titleKey) : panel.title,
        checked: !ctx.hiddenPanelIds.includes(panel.id),
        onClick: () => {
          if (panel.id === 'camera') {
            useUiStore.getState().toggleCameraWindow();
          } else {
            useUiStore.getState().togglePanelVisibility(panel.id);
          }
        },
      })),
    ],
  };

  // --- Show Properties action ---
  const showPropertiesAction = () => {
    const ui = useUiStore.getState();
    const layout = ui.panelLayout;
    const isHidden = layout.hiddenPanelIds.includes('properties');
    const isFloating = layout.floatingPanels.some((fp) => fp.panelId === 'properties');

    // Step 1: unhide if hidden
    if (isHidden) {
      ui.togglePanelVisibility('properties');
    }

    // Step 2: focus the panel
    if (isFloating || (isHidden && layout.floatingPanels.some((fp) => fp.panelId === 'properties'))) {
      useUiStore.getState().bringToFront('properties');
    } else {
      // Docked — ensure side panels visible
      if (!useUiStore.getState().sidePanelsVisible) {
        useUiStore.getState().toggleSidePanels();
      }
      // Switch to properties tab in its zone
      const updatedLayout = useUiStore.getState().panelLayout;
      for (const [zone, state] of Object.entries(updatedLayout.zones)) {
        if (state.panelIds.includes('properties')) {
          useUiStore.getState().setZoneActiveTab(zone as PhysicalDockZone, 'properties');
          break;
        }
      }
    }
    appService.persistLayout(useUiStore.getState().panelLayout);
  };

  return [
    // --- Windows submenu ---
    windowsSubmenu,
    { type: 'separator' },

    // --- Clipboard ---
    {
      id: 'cut',
      label: t('context_menu.cut'),
      shortcut: shortcutFor(APP_COMMANDS.EDIT_CUT),
      disabled: !ctx.canMutate,
      onClick: () => nodeMode
        ? runNodeEditAction('cut')
        : void clipboardCut([...ctx.selectedObjectIds]),
    },
    {
      id: 'copy',
      label: t('context_menu.copy'),
      shortcut: shortcutFor(APP_COMMANDS.EDIT_COPY),
      disabled: !ctx.hasSelection,
      onClick: () => nodeMode
        ? runNodeEditAction('copy')
        : clipboardCopy([...ctx.selectedObjectIds]),
    },
    {
      id: 'paste',
      label: t('context_menu.paste'),
      shortcut: shortcutFor(APP_COMMANDS.EDIT_PASTE),
      // Always enabled: the system clipboard may hold an image/SVG/file the
      // app cannot detect synchronously. Falls through to the system
      // clipboard when the in-app object clipboard is empty (same flow as
      // the Edit menu).
      disabled: false,
      onClick: () => {
        if (nodeMode) {
          runNodeEditAction('paste');
          return;
        }
        void (async () => {
          if (hasClipboardData()) {
            await clipboardPaste();
            return;
          }
          await pasteClipboardArtworkFromSystem();
        })();
      },
    },
    {
      id: 'paste-in-place',
      label: t('menus.edit.paste_in_place'),
      shortcut: shortcutFor(APP_COMMANDS.EDIT_PASTE_IN_PLACE),
      // Paste in Place requires our object snapshot clipboard; arbitrary
      // system artwork has no existing canvas position to preserve.
      disabled: !ctx.hasClipboard,
      onClick: () => void clipboardPasteInPlace(),
    },
    {
      id: 'duplicate',
      label: t('context_menu.duplicate'),
      shortcut: shortcutFor(APP_COMMANDS.EDIT_DUPLICATE),
      disabled: !ctx.canMutate,
      onClick: () => void clipboardDuplicate([...ctx.selectedObjectIds]),
    },
    { type: 'separator' },

    // --- Delete / Select All ---
    {
      id: 'delete',
      label: t('context_menu.delete'),
      shortcut: shortcutFor(APP_COMMANDS.EDIT_DELETE),
      disabled: !ctx.canMutate,
      onClick: () => nodeMode
        ? runNodeEditAction('delete')
        : void useProjectStore.getState().removeObjects([...ctx.selectedObjectIds]),
    },
    {
      id: 'select-all',
      label: t('context_menu.select_all'),
      shortcut: shortcutFor(APP_COMMANDS.EDIT_SELECT_ALL),
      onClick: () => nodeMode
        ? runNodeEditAction('select_all')
        : useProjectStore.getState().selectAllObjects(),
    },
    {
      type: 'submenu',
      id: 'select-similar',
      label: t('selection.select_similar'),
      disabled: !ctx.hasSelection,
      children: [
        ['layer', 'same_layer', APP_COMMANDS.EDIT_SELECT_SIMILAR_LAYER],
        ['type', 'same_type', APP_COMMANDS.EDIT_SELECT_SIMILAR_TYPE],
        ['size', 'same_size', APP_COMMANDS.EDIT_SELECT_SIMILAR_SIZE],
        ['operation', 'same_operation', APP_COMMANDS.EDIT_SELECT_SIMILAR_OPERATION],
        ['circle_diameter', 'same_circle_diameter', APP_COMMANDS.EDIT_SELECT_SIMILAR_CIRCLE_DIAMETER],
        ['open_closed', 'same_open_closed', APP_COMMANDS.EDIT_SELECT_SIMILAR_OPEN_CLOSED],
      ].map(([kind, key, commandId]) => ({
        id: `select-similar-${kind}`,
        label: t(`selection.${key}`),
        shortcut: shortcutFor(commandId),
        onClick: () => useProjectStore.getState().selectSimilar(kind as import('../../utils/selectionSimilar').SimilarSelectionKind),
      })),
    },
    ...(nodeMode
      ? [{
          id: 'extract-nodes-to-path',
          label: 'Extract Selected Nodes to New Path',
          disabled: !ctx.canMutate,
          onClick: () => runNodeEditAction('extract'),
        } as ContextMenuEntry]
      : []),
    { type: 'separator' },

    // --- Group / Ungroup ---
    {
      id: 'group',
      label: t('context_menu.group'),
      shortcut: shortcutFor(APP_COMMANDS.ARRANGE_GROUP),
      disabled: !ctx.canGroup,
      onClick: () => void useProjectStore.getState().groupObjects(ctx.selectedObjectIds),
    },
    {
      id: 'ungroup',
      label: t('context_menu.ungroup'),
      shortcut: shortcutFor(APP_COMMANDS.ARRANGE_UNGROUP),
      disabled: !ctx.canUngroup,
      onClick: () => void useProjectStore.getState().ungroupObjects(ctx.selectedObjectIds[0]),
    },
    { type: 'separator' },

    // --- Lock / Unlock — use batch commands for atomic undo ---
    ...(ctx.hasSelection && ctx.hasUnlocked
      ? [
          {
            id: 'lock',
            label: t('context_menu.lock_selected'),
            onClick: () => {
              const ids = ctx.selectedObjects.filter((o) => !o.locked).map((o) => o.id);
              if (ids.length > 0) void useProjectStore.getState().lockObjects(ids);
            },
          } as ContextMenuEntry,
        ]
      : []),
    ...(ctx.hasSelection && ctx.hasLocked
      ? [
          {
            id: 'unlock',
            label: t('context_menu.unlock_selected'),
            onClick: () => {
              const ids = ctx.selectedObjects.filter((o) => o.locked).map((o) => o.id);
              if (ids.length > 0) void useProjectStore.getState().unlockObjects(ids);
            },
          } as ContextMenuEntry,
        ]
      : []),
    // --- Unlink Virtual Clone ---
    ...(ctx.singleSelected?.data.type === 'virtual_clone'
      ? [{
          id: 'unlink-clone',
          label: t('context_menu.unlink_clone'),
          onClick: () => {
            void useProjectStore.getState().unlinkVirtualClone(ctx.singleSelected!.id);
          },
        } as ContextMenuEntry]
      : []),
    ...(ctx.hasSelection ? [{ type: 'separator' } as ContextMenuEntry] : []),

    // --- Convert to Path / Convert to Bitmap ---
    {
      id: 'convert-path',
      label: t('context_menu.convert_to_path'),
      shortcut: shortcutFor(APP_COMMANDS.EDIT_CONVERT_TO_PATH),
      disabled: !ctx.canConvertToPath,
      onClick: () => void useProjectStore.getState().convertToPath(ctx.selectedObjectIds[0]),
    },
    {
      id: 'convert-bitmap',
      label: t('context_menu.convert_to_bitmap'),
      shortcut: shortcutFor(APP_COMMANDS.EDIT_CONVERT_TO_BITMAP),
      disabled: !ctx.canConvertToBitmap,
      onClick: () => void useProjectStore.getState().convertToBitmap(ctx.selectedObjectIds[0], 300),
    },
    // --- Edit Text Shape (conditional) ---
    ...(ctx.singleSelected?.data.type === 'text'
      ? [{
          id: 'edit-text',
          label: t('context_menu.edit_text'),
          onClick: () => {
            void useUiStore.getState().beginTextEditSession(ctx.singleSelected!.id, 'double-click');
          },
        } as ContextMenuEntry]
      : []),
    // --- Image-only actions (conditional visibility) ---
    ...(ctx.canTraceImage
      ? [{ id: 'trace-image', label: t('context_menu.trace_image'), onClick: () => callbacks?.onTraceImage?.() } as ContextMenuEntry]
      : []),
    ...(ctx.canAdjustImage
      ? [{ id: 'adjust-image', label: t('context_menu.adjust_image'), onClick: () => callbacks?.onAdjustImage?.() } as ContextMenuEntry]
      : []),
    ...(ctx.canSaveProcessedBitmap
      ? [{ id: 'save-processed-bitmap', label: t('menus.file.export_processed_image'), onClick: () => callbacks?.onSaveProcessedBitmap?.() } as ContextMenuEntry]
      : []),
    ...(ctx.canUseAsImageMask
      ? [{
          id: 'use-as-image-mask',
          label: t('context_menu.use_as_image_mask'),
          onClick: () => {
            if (ctx.imageMaskSelectionHasInvalidMasks) {
              useNotificationStore.getState().push(t('context_menu.image_mask_requires_closed'), 'error');
              return;
            }
            if (ctx.imageMaskTargetId && ctx.imageMaskObjectIds.length > 0) {
              void useProjectStore.getState().assignImageMask(ctx.imageMaskTargetId, ctx.imageMaskObjectIds, 'keep_inside');
            }
          },
        } as ContextMenuEntry]
      : []),
    ...(ctx.canRemoveImageMask
      ? [{
          id: 'remove-image-mask',
          label: t('context_menu.remove_image_mask'),
          onClick: () => void useProjectStore.getState().removeImageMask(ctx.selectedObjectIds[0]),
        } as ContextMenuEntry]
      : []),
    { type: 'separator' },

    // --- Preview / Show Properties ---
    {
      id: 'preview',
      label: t('context_menu.preview'),
      shortcut: shortcutFor(APP_COMMANDS.WINDOW_PREVIEW),
      onClick: () => usePreviewStore.getState().togglePreview(),
    },
    {
      id: 'show-properties',
      label: t('context_menu.show_properties'),
      onClick: showPropertiesAction,
    },
  ];
}
