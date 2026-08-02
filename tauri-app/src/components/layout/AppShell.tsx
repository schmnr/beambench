import { useEffect, useRef, useState } from 'react';
import { ChevronLeft, ChevronRight } from 'lucide-react';
import { useUiStore } from '../../stores/uiStore';
import { useProjectStore } from '../../stores/projectStore';
import { appService } from '../../services/appService';
import { MenuBar } from './MenuBar';
import { MainToolbar } from './MainToolbar';
import { CreationToolbar } from './CreationToolbar';
import { ModifiersToolbar } from './ModifiersToolbar';
import { StatusBar } from './StatusBar';
import { RightPanel } from './RightPanel';
import { PanelColumn } from './PanelColumn';
import { BottomPanel } from './BottomPanel';
import { PanelResizer } from './PanelResizer';
import { Canvas } from '../canvas/Canvas';
import { LayerTabs } from '../layers/LayerTabs';
import { LibraryDrawer } from './LibraryDrawer';
import { ImportDropZone } from '../import/ImportDropZone';
import { FloatingPanelLayer } from './FloatingPanelLayer';
import { PanelDndProvider, usePanelDnd } from '../../panels/DndContext';
import { isNativeMenuActive } from '../../utils/platform';
import { COLUMN_ZONES, getWorkspacePanelLayout } from '../../panels';

const LEFT_COLUMN = 'left' as const;
const RIGHT_COLUMN = 'right' as const;
const PROPERTIES_PANEL_ID = 'properties';
const DESIGN_WORKSPACE = 'design';

function getDockRecoveryKey(workspaceMode: 'design' | 'run', side: 'left' | 'right') {
  return `${workspaceMode}-${side}`;
}

function EmptyDockRecovery({ side, onOpen }: {
  side: 'left' | 'right';
  onOpen: () => void;
}) {
  const { dragState, registerDropZone } = usePanelDnd();
  const recoveryRef = useRef<HTMLButtonElement>(null);
  const label = side === 'left' ? 'Open left dock' : 'Open right dock';
  const Icon = side === 'left' ? ChevronRight : ChevronLeft;
  const zone = side === 'left' ? 'top-left' : 'top-right';
  const isDropTarget = dragState?.activeDropTarget?.type === 'zone'
    && dragState.activeDropTarget.zone === zone;

  useEffect(() => {
    registerDropZone(zone, recoveryRef.current);
    return () => registerDropZone(zone, null);
  }, [registerDropZone, zone]);

  return (
    <button
      ref={recoveryRef}
      type="button"
      onClick={onOpen}
      className={`group flex w-3 shrink-0 items-center justify-center bg-bb-bg text-bb-text-dim outline-none hover:text-bb-accent focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-bb-accent ${
        isDropTarget ? 'bg-bb-accent/15 text-bb-accent' : ''
      }`}
      aria-label={label}
      title={label}
      data-testid={`open-${side}-dock`}
    >
      <span className="flex h-14 w-2 items-center justify-center rounded-full border border-bb-border bg-bb-panel transition-colors group-hover:border-bb-accent/60">
        <Icon size={9} strokeWidth={2.5} />
      </span>
    </button>
  );
}

function BottomDockRecovery({ onOpen }: { onOpen: () => void }) {
  const { dragState, registerDropZone } = usePanelDnd();
  const recoveryRef = useRef<HTMLButtonElement>(null);
  const isDropTarget = dragState?.activeDropTarget?.type === 'zone'
    && dragState.activeDropTarget.zone === 'bottom';
  const label = 'Open bottom dock';

  useEffect(() => {
    registerDropZone('bottom', recoveryRef.current);
    return () => registerDropZone('bottom', null);
  }, [registerDropZone]);

  return (
    <button
      ref={recoveryRef}
      type="button"
      onClick={onOpen}
      className={`group flex h-3 shrink-0 items-center justify-center bg-bb-bg outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-bb-accent ${
        isDropTarget ? 'bg-bb-accent/15' : ''
      }`}
      aria-label={label}
      title={label}
      data-testid="open-bottom-dock"
    >
      <span className={`h-1 w-14 rounded-full border border-bb-border bg-bb-panel transition-colors group-hover:border-bb-accent/60 ${
        isDropTarget ? 'border-bb-accent' : ''
      }`} />
    </button>
  );
}

export function AppShell() {
  const rightPanelWidth = useUiStore((s) => s.panelLayout.rightPanelWidth);
  const leftPanelWidth = useUiStore((s) => s.panelLayout.leftPanelWidth);
  const bottomPanelHeight = useUiStore((s) => s.panelLayout.bottomPanelHeight);
  const setRightPanelWidth = useUiStore((s) => s.setRightPanelWidth);
  const setLeftPanelWidth = useUiStore((s) => s.setLeftPanelWidth);
  const setBottomPanelHeight = useUiStore((s) => s.setBottomPanelHeight);
  const sidePanelsVisible = useUiStore((s) => s.sidePanelsVisible);
  const workspaceMode = useUiStore((s) => s.workspaceMode);
  const activeTool = useUiStore((s) => s.activeTool);
  const modifierPropertiesSession = useUiStore((s) => s.modifierPropertiesSession);
  const selectionKey = useProjectStore((s) => s.selectedObjectIds.join('|'));
  const runMode = workspaceMode === 'run';

  useEffect(() => {
    if (workspaceMode !== DESIGN_WORKSPACE || selectionKey.length === 0) return;
    const ui = useUiStore.getState();
    const workspace = getWorkspacePanelLayout(ui.panelLayout, ui.workspaceMode);
    if (!workspace.hiddenPanelIds.includes(PROPERTIES_PANEL_ID)) {
      ui.showPanel(PROPERTIES_PANEL_ID);
    }
  }, [selectionKey, workspaceMode]);

  useEffect(() => {
    if (workspaceMode !== DESIGN_WORKSPACE || !['text', 'node', 'radius', 'warp', 'measure'].includes(activeTool)) return;
    const ui = useUiStore.getState();
    if (!ui.sidePanelsVisible) ui.toggleSidePanels();
    useUiStore.getState().showPanel(PROPERTIES_PANEL_ID);
  }, [activeTool, workspaceMode]);

  useEffect(() => {
    if (!modifierPropertiesSession) return;
    if (selectionKey === modifierPropertiesSession.objectIds.join('|')) return;
    useUiStore.getState().closeModifierProperties();
  }, [modifierPropertiesSession, selectionKey]);
  const panelLayout = useUiStore((s) => s.panelLayout);
  const toolbarVisibility = panelLayout.toolbarVisibility;
  const [openedEmptyDocks, setOpenedEmptyDocks] = useState<Record<string, boolean>>({});
  const [bottomDockRequested, setBottomDockRequested] = useState(false);

  const handleRightResize = (delta: number) => {
    setRightPanelWidth(rightPanelWidth + delta);
    appService.persistLayout(useUiStore.getState().panelLayout);
  };

  const handleLeftResize = (delta: number) => {
    setLeftPanelWidth(leftPanelWidth + delta);
    appService.persistLayout(useUiStore.getState().panelLayout);
  };

  const handleBottomResize = (delta: number) => {
    setBottomPanelHeight(bottomPanelHeight + delta);
    appService.persistLayout(useUiStore.getState().panelLayout);
  };

  const workspaceLayout = getWorkspacePanelLayout(panelLayout, workspaceMode);
  const sideDocksEnabled = runMode || sidePanelsVisible;
  const columnHasContent = (side: 'left' | 'right') => sideDocksEnabled && COLUMN_ZONES[side].some(
    (zone) => workspaceLayout.zones[zone].panelIds.some(
      (panelId) => !workspaceLayout.hiddenPanelIds.includes(panelId),
    ),
  );
  const leftHasContent = columnHasContent('left');
  const rightHasContent = columnHasContent('right');
  const bottomHasContent = workspaceLayout.zones.bottom.panelIds.some(
    (panelId) => !workspaceLayout.hiddenPanelIds.includes(panelId),
  );
  const leftDockOpen = leftPanelWidth > 0
    && (leftHasContent || (sideDocksEnabled && openedEmptyDocks[getDockRecoveryKey(workspaceMode, 'left')]));
  const rightDockOpen = rightPanelWidth > 0
    && (rightHasContent || (sideDocksEnabled && openedEmptyDocks[getDockRecoveryKey(workspaceMode, 'right')]));
  const bottomDockOpen = bottomPanelHeight > 0 && (bottomHasContent || bottomDockRequested);
  const openEmptyDock = (side: 'left' | 'right') => {
    if (side === 'left' && leftPanelWidth <= 0) setLeftPanelWidth(280);
    if (side === 'right' && rightPanelWidth <= 0) setRightPanelWidth(440);
    setOpenedEmptyDocks((current) => ({ ...current, [getDockRecoveryKey(workspaceMode, side)]: true }));
  };

  useEffect(() => {
    if (!leftHasContent && !rightHasContent) return;
    setOpenedEmptyDocks((current) => {
      const next = { ...current };
      if (leftHasContent) delete next[getDockRecoveryKey(workspaceMode, 'left')];
      if (rightHasContent) delete next[getDockRecoveryKey(workspaceMode, 'right')];
      return next;
    });
  }, [leftHasContent, rightHasContent, workspaceMode]);

  useEffect(() => {
    if (bottomHasContent) setBottomDockRequested(false);
  }, [bottomHasContent]);
  const effectiveLeftWidth = leftPanelWidth;

  return (
    <PanelDndProvider>
      <div className="h-full flex flex-col">
        {!isNativeMenuActive() && <MenuBar />}
        <MainToolbar />
        {/* Content wrapper: content row + full-width bottom panel */}
        <div className="flex-1 flex flex-col min-h-0">
          {/* Content row */}
          <div className="relative flex-1 flex min-h-0">
            {/* Left icon toolbars (design mode only) */}
            <div className="flex flex-shrink-0 min-h-0 overflow-y-auto scrollbar-none bg-bb-bg">
              {!runMode && (
              <>
              {(toolbarVisibility.tools || toolbarVisibility.modifiers) && (
                <div className="my-2 ml-2 flex flex-col flex-shrink-0 self-start overflow-hidden rounded-xl border border-bb-border bg-bb-panel shadow-lg">
                  {toolbarVisibility.tools && <CreationToolbar />}
                  {toolbarVisibility.modifiers && <ModifiersToolbar />}
                </div>
              )}
              </>
              )}
            </div>
            {/* Library drawer overlays the canvas next to the rail */}
            {!runMode && <LibraryDrawer />}
            {/* Configurable left dock column */}
            {leftDockOpen ? (
              <>
                <div className="flex-shrink-0" style={{ width: effectiveLeftWidth }}>
                  <PanelColumn side={LEFT_COLUMN} />
                </div>
                <PanelResizer
                  direction="left"
                  onResize={(delta) => handleLeftResize(delta)}
                />
              </>
            ) : sideDocksEnabled ? (
              <EmptyDockRecovery side={LEFT_COLUMN} onOpen={() => openEmptyDock(LEFT_COLUMN)} />
            ) : null}
            {/* Canvas with layer tabs (both modes — in Run they show the cut
                order and let you click through layers) */}
            <div className="flex-1 min-w-0 min-h-0 flex flex-col">
              <LayerTabs />
              <div className="flex-1 min-h-0">
                {runMode ? (
                  <Canvas />
                ) : (
                  <ImportDropZone>
                    <Canvas />
                  </ImportDropZone>
                )}
              </div>
            </div>
            {/* Right panel zone */}
            {rightDockOpen ? (
              <>
                <PanelResizer
                  direction="right"
                  onResize={(delta) => handleRightResize(delta)}
                />
                <div className="flex-shrink-0" style={{ width: rightPanelWidth }}>
                  <RightPanel />
                </div>
              </>
            ) : sideDocksEnabled ? (
              <EmptyDockRecovery side={RIGHT_COLUMN} onOpen={() => openEmptyDock(RIGHT_COLUMN)} />
            ) : null}
          </div>
          {bottomDockOpen ? (
            <>
              <PanelResizer direction="bottom" onResize={handleBottomResize} />
              <div className="min-h-0 shrink-0" style={{ height: bottomPanelHeight }} data-testid="bottom-dock">
                <BottomPanel />
              </div>
            </>
          ) : (
            <BottomDockRecovery
              onOpen={() => {
                if (bottomPanelHeight <= 0) setBottomPanelHeight(220);
                setBottomDockRequested(true);
              }}
            />
          )}
        </div>
        <StatusBar />
      </div>
      <FloatingPanelLayer />
    </PanelDndProvider>
  );
}
