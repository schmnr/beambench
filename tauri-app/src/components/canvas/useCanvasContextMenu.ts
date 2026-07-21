import { useState, useCallback, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import type { ContextMenuEntry } from '../shared/ContextMenu';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { createSelectionContext } from '../../commands/selectionContext';
import { executeAppCommand } from '../../commands/appCommands';
import { APP_COMMANDS } from '../../commands/appCommandIds';
import { buildCanvasContextMenuItems } from './canvasMenuItems';

interface MenuState {
  visible: boolean;
  x: number;
  y: number;
  items: ContextMenuEntry[];
}

const HIDDEN: MenuState = { visible: false, x: 0, y: 0, items: [] };

export function useCanvasContextMenu() {
  const { t } = useTranslation();
  const [menuState, setMenuState] = useState<MenuState>(HIDDEN);
  const [traceImageObjectId, setTraceImageObjectId] = useState<string | null>(null);
  const [adjustImageObjectId, setAdjustImageObjectId] = useState<string | null>(null);
  const menuVisibleRef = useRef(false);

  const closeMenu = useCallback(() => {
    menuVisibleRef.current = false;
    setMenuState(HIDDEN);
  }, []);

  const handleContextMenu = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();

      // Right-click preserves the current selection.
      const ps = useProjectStore.getState();
      const uiState = useUiStore.getState();
      const ctx = createSelectionContext(
        ps.selectedObjectIds,
        ps.project?.objects ?? [],
        uiState.hasClipboard,
        uiState.panelLayout.hiddenPanelIds,
        ps.project?.assets ?? [],
      );
      const items = buildCanvasContextMenuItems(t, ctx, {
        onTraceImage: () => {
          if (ctx.canTraceImage && ctx.selectedObjectIds.length === 1) {
            setTraceImageObjectId(ctx.selectedObjectIds[0]);
          }
        },
        onAdjustImage: () => {
          if (ctx.canAdjustImage && ctx.selectedObjectIds.length === 1) {
            setAdjustImageObjectId(ctx.selectedObjectIds[0]);
          }
        },
        onSaveProcessedBitmap: () => {
          if (ctx.canSaveProcessedBitmap) {
            void executeAppCommand(APP_COMMANDS.FILE_SAVE_PROCESSED_BITMAP);
          }
        },
      });

      menuVisibleRef.current = true;
      setMenuState({
        visible: true,
        x: e.clientX,
        y: e.clientY,
        items,
      });
    },
    [t],
  );

  // Auto-close on selection change while menu is visible
  useEffect(() => {
    const unsubscribe = useProjectStore.subscribe((state, prev) => {
      if (menuVisibleRef.current && state.selectedObjectIds !== prev.selectedObjectIds) {
        closeMenu();
      }
    });
    return unsubscribe;
  }, [closeMenu]);

  const closeTraceDialog = useCallback(() => setTraceImageObjectId(null), []);
  const closeAdjustDialog = useCallback(() => setAdjustImageObjectId(null), []);

  return { menuState, handleContextMenu, closeMenu, traceImageObjectId, closeTraceDialog, adjustImageObjectId, closeAdjustDialog };
}
