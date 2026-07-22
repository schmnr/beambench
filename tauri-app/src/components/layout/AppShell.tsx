import { useUiStore } from '../../stores/uiStore';
import { appService } from '../../services/appService';
import { MenuBar } from './MenuBar';
import { MainToolbar } from './MainToolbar';
import { PropertiesToolbar } from './PropertiesToolbar';
import { CreationToolbar } from './CreationToolbar';
import { NodeSubToolbar } from './NodeSubToolbar';
import { ModifiersToolbar } from './ModifiersToolbar';
import { StatusBar } from './StatusBar';
import { RightPanel } from './RightPanel';
import { LeftPanel } from './LeftPanel';
import { BottomPanel } from './BottomPanel';
import { PanelResizer } from './PanelResizer';
import { Canvas } from '../canvas/Canvas';
import { ImportDropZone } from '../import/ImportDropZone';
import { FloatingPanelLayer } from './FloatingPanelLayer';
import { PanelDndProvider } from '../../panels/DndContext';
import { isNativeMenuActive } from '../../utils/platform';

export function AppShell() {
  const rightPanelWidth = useUiStore((s) => s.panelLayout.rightPanelWidth);
  const leftPanelWidth = useUiStore((s) => s.panelLayout.leftPanelWidth);
  const bottomPanelHeight = useUiStore((s) => s.panelLayout.bottomPanelHeight);
  const setRightPanelWidth = useUiStore((s) => s.setRightPanelWidth);
  const setLeftPanelWidth = useUiStore((s) => s.setLeftPanelWidth);
  const setBottomPanelHeight = useUiStore((s) => s.setBottomPanelHeight);
  const sidePanelsVisible = useUiStore((s) => s.sidePanelsVisible);
  const panelLayout = useUiStore((s) => s.panelLayout);
  const toolbarVisibility = panelLayout.toolbarVisibility;

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

  const leftVisibleIds = sidePanelsVisible
    ? panelLayout.zones.left?.panelIds.filter((id) => !panelLayout.hiddenPanelIds.includes(id)) ?? []
    : [];
  const leftHasContent = leftVisibleIds.length > 0;

  const bottomVisibleIds = sidePanelsVisible
    ? panelLayout.zones.bottom?.panelIds.filter((id) => !panelLayout.hiddenPanelIds.includes(id)) ?? []
    : [];
  const bottomHasContent = bottomVisibleIds.length > 0;

  const rightHasContent = sidePanelsVisible && (
    panelLayout.zones['upper-right']?.panelIds.some((id) => !panelLayout.hiddenPanelIds.includes(id)) ||
    panelLayout.zones['lower-right']?.panelIds.some((id) => !panelLayout.hiddenPanelIds.includes(id))
  );

  // Auto-size: when only color_palette is in bottom zone, use fixed 48px (one row)
  const bottomOnlyColorPalette = bottomVisibleIds.length === 1 && bottomVisibleIds[0] === 'color_palette';
  const effectiveBottomHeight = bottomOnlyColorPalette ? 48 : bottomPanelHeight;

  // Auto-size: when only color_palette is in left zone, use fixed 44px (one column)
  const leftOnlyColorPalette = leftVisibleIds.length === 1 && leftVisibleIds[0] === 'color_palette';
  const effectiveLeftWidth = leftOnlyColorPalette ? 44 : leftPanelWidth;

  return (
    <PanelDndProvider>
      <div className="h-full flex flex-col">
        {!isNativeMenuActive() && <MenuBar />}
        <MainToolbar />
        <PropertiesToolbar
          showNumericEdits={toolbarVisibility.numericEdits}
          showTextOptions={toolbarVisibility.textOptions}
        />
        {/* Content wrapper: content row + full-width bottom panel */}
        <div className="flex-1 flex flex-col min-h-0">
          {/* Content row */}
          <div className="flex-1 flex min-h-0">
            {/* Left icon toolbars */}
            <div className="flex flex-shrink-0 min-h-0 overflow-y-auto scrollbar-none bg-bb-bg">
              {(toolbarVisibility.tools || toolbarVisibility.modifiers) && (
                <div className="my-2 ml-2 flex flex-col flex-shrink-0 self-start overflow-hidden rounded-xl border border-bb-border bg-bb-panel shadow-lg">
                  {toolbarVisibility.tools && <CreationToolbar />}
                  {toolbarVisibility.modifiers && <ModifiersToolbar />}
                </div>
              )}
              {toolbarVisibility.tools && <NodeSubToolbar />}
            </div>
            {/* Left panel zone (between toolbars and canvas) */}
            {leftHasContent && (
              <>
                <div className="flex-shrink-0" style={{ width: effectiveLeftWidth }}>
                  <LeftPanel compact={leftOnlyColorPalette} />
                </div>
                {!leftOnlyColorPalette && (
                  <PanelResizer
                    direction="left"
                    onResize={(delta) => handleLeftResize(delta)}
                  />
                )}
              </>
            )}
            {/* Canvas */}
            <div className="flex-1 min-w-0 min-h-0">
              <ImportDropZone>
                <Canvas />
              </ImportDropZone>
            </div>
            {/* Right panel zone */}
            {rightHasContent && (
              <>
                <PanelResizer
                  direction="right"
                  onResize={(delta) => handleRightResize(delta)}
                />
                <div className="flex-shrink-0" style={{ width: rightPanelWidth }}>
                  <RightPanel />
                </div>
              </>
            )}
          </div>
          {/* Bottom panel zone — full width below canvas + right panel */}
          {bottomHasContent && (
            <>
              {!bottomOnlyColorPalette && (
                <PanelResizer
                  direction="bottom"
                  onResize={(delta) => handleBottomResize(delta)}
                />
              )}
              <div className="flex-shrink-0" style={{ height: effectiveBottomHeight }}>
                <BottomPanel />
              </div>
            </>
          )}
        </div>
        <StatusBar />
      </div>
      <FloatingPanelLayer />
    </PanelDndProvider>
  );
}
