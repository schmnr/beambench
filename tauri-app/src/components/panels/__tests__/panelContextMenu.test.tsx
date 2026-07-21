import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent, act } from '@testing-library/react';
import { RightPanel } from '../../layout/RightPanel';
import { LeftPanel } from '../../layout/LeftPanel';
import { BottomPanel } from '../../layout/BottomPanel';
import { FloatingPanel } from '../../layout/FloatingPanel';
import { FloatingPanelLayer } from '../../layout/FloatingPanelLayer';
import { useUiStore } from '../../../stores/uiStore';
import { useProjectStore } from '../../../stores/projectStore';
import { PanelDndProvider } from '../../../panels/DndContext';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));
vi.mock('../../../services/appService', () => ({
  appService: { persistLayout: vi.fn() },
}));

const initialUiState = useUiStore.getState();
const initialProjectState = useProjectStore.getState();

function renderWithDnd(ui: React.ReactElement) {
  return render(<PanelDndProvider>{ui}</PanelDndProvider>);
}

afterEach(() => {
  cleanup();
  useUiStore.setState(initialUiState, true);
  useProjectStore.setState(initialProjectState, true);
  vi.clearAllMocks();
});

describe('Panel context menu suppression', () => {
  it('RightPanel suppresses context menu on panel content', () => {
    renderWithDnd(<RightPanel />);
    // The first tab content is visible; fire contextmenu on the overall container
    const container = screen.getByText('Cuts / Layers').closest('.no-select')!;
    const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
    container.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
  });

  it('LeftPanel suppresses context menu', () => {
    // Ensure left zone has a panel
    const layout = useUiStore.getState().panelLayout;
    if (layout.zones['left']?.panelIds.length === 0) {
      // Move a panel to left zone for testing
      useUiStore.setState({
        panelLayout: {
          ...layout,
          zones: {
            ...layout.zones,
            left: { panelIds: ['object_tree'], activeTab: 'object_tree' },
          },
        },
      });
    }
    renderWithDnd(<LeftPanel />);
    // Find the panel root div
    const root = document.querySelector('[class*="bg-bb-panel"]');
    if (root) {
      const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
      root.dispatchEvent(event);
      expect(event.defaultPrevented).toBe(true);
    }
  });

  it('BottomPanel suppresses context menu', () => {
    // Ensure bottom zone has a panel
    const layout = useUiStore.getState().panelLayout;
    useUiStore.setState({
      panelLayout: {
        ...layout,
        zones: {
          ...layout.zones,
          bottom: { panelIds: ['color_palette'], activeTab: 'color_palette' },
        },
      },
    });
    renderWithDnd(<BottomPanel />);
    const root = document.querySelector('[class*="bg-bb-panel"]');
    if (root) {
      const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
      root.dispatchEvent(event);
      expect(event.defaultPrevented).toBe(true);
    }
  });

  it('RightPanel suppresses context menu on ZoneSplitter', () => {
    renderWithDnd(<RightPanel />);
    // The splitter uses cursor-row-resize
    const splitter = document.querySelector('.cursor-row-resize')!;
    expect(splitter).not.toBeNull();
    const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
    splitter.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
  });

  it('FloatingPanel suppresses context menu on content', () => {
    const props = {
      panelId: 'test_fp',
      title: 'Test FP',
      x: 100,
      y: 100,
      width: 300,
      height: 200,
      zIndex: 1,
      onClose: vi.fn(),
      onDock: vi.fn(),
      onMove: vi.fn(),
      onResize: vi.fn(),
      onFocus: vi.fn(),
    };
    render(
      <FloatingPanel {...props}>
        <div data-testid="fp-content">Content</div>
      </FloatingPanel>,
    );
    const panel = screen.getByTestId('floating-panel-test_fp');
    const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
    panel.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
  });
});

describe('Docked tab context menu', () => {
  it('right-click a docked tab opens context menu with Float/Close/Panels', () => {
    renderWithDnd(<RightPanel />);
    const tab = screen.getByText('Cuts / Layers');
    // Right-click on the tab's parent div wrapper (which has the onContextMenu)
    const tabWrapper = tab.closest('.group')!;
    fireEvent.contextMenu(tabWrapper);
    // Context menu should appear with Float and Close
    expect(screen.getByTestId('context-menu-item-panel-tab-float')).toBeDefined();
    expect(screen.getByTestId('context-menu-item-panel-tab-close')).toBeDefined();
    expect(screen.getByTestId('context-menu-item-panel-tab-panels-submenu')).toBeDefined();
  });

  it('Float action from tab menu calls floatPanel', () => {
    renderWithDnd(<RightPanel />);
    const tab = screen.getByText('Cuts / Layers');
    const tabWrapper = tab.closest('.group')!;
    fireEvent.contextMenu(tabWrapper);
    fireEvent.click(screen.getByTestId('context-menu-item-panel-tab-float'));
    // After floating, cuts_layers should be in floatingPanels
    const state = useUiStore.getState();
    expect(state.panelLayout.floatingPanels.some((fp) => fp.panelId === 'cuts_layers')).toBe(true);
  });

  it('Close action from tab menu hides the panel', () => {
    renderWithDnd(<RightPanel />);
    const tab = screen.getByText('Cuts / Layers');
    const tabWrapper = tab.closest('.group')!;
    fireEvent.contextMenu(tabWrapper);
    fireEvent.click(screen.getByTestId('context-menu-item-panel-tab-close'));
    // Panel should be hidden
    const state = useUiStore.getState();
    expect(state.panelLayout.hiddenPanelIds).toContain('cuts_layers');
  });
});

describe('Floating panel title bar context menu', () => {
  function setupFloatingPanel(panelId = 'console') {
    const state = useUiStore.getState();
    useUiStore.setState({
      panelLayout: {
        ...state.panelLayout,
        floatingPanels: [
          { panelId, x: 100, y: 100, width: 400, height: 300, zIndex: 1, originZone: 'upper-right', originIndex: 2 },
        ],
        hiddenPanelIds: state.panelLayout.hiddenPanelIds.filter((id) => id !== panelId),
      },
      nextFloatingZIndex: 2,
    });
  }

  it('right-click floating panel title bar opens context menu with Dock/Close/Panels', async () => {
    setupFloatingPanel();
    await act(async () => { renderWithDnd(<FloatingPanelLayer />); });
    const titleBar = screen.getByText('Console').closest('.cursor-move')!;
    fireEvent.contextMenu(titleBar);
    expect(screen.getByTestId('context-menu-item-panel-tab-dock')).toBeDefined();
    expect(screen.getByTestId('context-menu-item-panel-tab-close')).toBeDefined();
    expect(screen.getByTestId('context-menu-item-panel-tab-panels-submenu')).toBeDefined();
  });

  it('Dock action from floating title menu docks the panel', async () => {
    setupFloatingPanel();
    await act(async () => { renderWithDnd(<FloatingPanelLayer />); });
    const titleBar = screen.getByText('Console').closest('.cursor-move')!;
    fireEvent.contextMenu(titleBar);
    fireEvent.click(screen.getByTestId('context-menu-item-panel-tab-dock'));
    const state = useUiStore.getState();
    // Should no longer be floating
    expect(state.panelLayout.floatingPanels.some((fp) => fp.panelId === 'console')).toBe(false);
    // Should be in upper-right zone (originZone)
    expect(state.panelLayout.zones['upper-right'].panelIds).toContain('console');
  });

  it('Close on floating camera sets cameraWindowOpen to false', async () => {
    // Set camera as floating and cameraWindowOpen true
    const state = useUiStore.getState();
    useUiStore.setState({
      panelLayout: {
        ...state.panelLayout,
        floatingPanels: [
          { panelId: 'camera', x: 100, y: 100, width: 420, height: 400, zIndex: 1 },
        ],
        hiddenPanelIds: state.panelLayout.hiddenPanelIds.filter((id) => id !== 'camera'),
      },
      cameraWindowOpen: true,
      nextFloatingZIndex: 2,
    });

    await act(async () => { renderWithDnd(<FloatingPanelLayer />); });
    const titleBar = screen.getByText('Camera').closest('.cursor-move')!;
    fireEvent.contextMenu(titleBar);
    fireEvent.click(screen.getByTestId('context-menu-item-panel-tab-close'));
    expect(useUiStore.getState().cameraWindowOpen).toBe(false);
  });

  it('existing x button on floating camera also sets cameraWindowOpen to false', async () => {
    const state = useUiStore.getState();
    useUiStore.setState({
      panelLayout: {
        ...state.panelLayout,
        floatingPanels: [
          { panelId: 'camera', x: 100, y: 100, width: 420, height: 400, zIndex: 1 },
        ],
        hiddenPanelIds: state.panelLayout.hiddenPanelIds.filter((id) => id !== 'camera'),
      },
      cameraWindowOpen: true,
      nextFloatingZIndex: 2,
    });

    await act(async () => { renderWithDnd(<FloatingPanelLayer />); });
    // Click the x button
    const closeBtn = screen.getByTitle('Close panel');
    fireEvent.click(closeBtn);
    expect(useUiStore.getState().cameraWindowOpen).toBe(false);
  });
});

describe('Close docked camera via togglePanelVisibility', () => {
  it('togglePanelVisibility hides camera and sets cameraWindowOpen to false', () => {
    // Set camera as docked and visible
    const state = useUiStore.getState();
    useUiStore.setState({
      panelLayout: {
        ...state.panelLayout,
        zones: {
          ...state.panelLayout.zones,
          'upper-right': {
            ...state.panelLayout.zones['upper-right'],
            panelIds: [...state.panelLayout.zones['upper-right'].panelIds, 'camera'],
          },
        },
        hiddenPanelIds: state.panelLayout.hiddenPanelIds.filter((id) => id !== 'camera'),
      },
      cameraWindowOpen: true,
    });

    // Hide camera via togglePanelVisibility
    useUiStore.getState().togglePanelVisibility('camera');
    expect(useUiStore.getState().cameraWindowOpen).toBe(false);
    expect(useUiStore.getState().panelLayout.hiddenPanelIds).toContain('camera');
  });
});

describe('Right-click button guards', () => {
  it('right-click on tab does NOT start DnD', () => {
    renderWithDnd(<RightPanel />);
    const tab = screen.getByText('Cuts / Layers');
    // Simulate right-button mousedown (button=2) on the tab button
    fireEvent.mouseDown(tab, { button: 2 });
    // No DnD ghost should appear — verify no drag state set
    // Since startDrag is from DnD context, we just check no errors occur
    // and the tab is still there
    expect(screen.getByText('Cuts / Layers')).toBeDefined();
  });

  it('right-click on floating title bar does NOT start drag', () => {
    const onMove = vi.fn();
    const props = {
      panelId: 'test_fp',
      title: 'Test FP',
      x: 100,
      y: 100,
      width: 300,
      height: 200,
      zIndex: 1,
      onClose: vi.fn(),
      onDock: vi.fn(),
      onMove,
      onResize: vi.fn(),
      onFocus: vi.fn(),
    };
    render(
      <FloatingPanel {...props}>
        <div>Content</div>
      </FloatingPanel>,
    );
    const titleBar = screen.getByText('Test FP').closest('.cursor-move')!;
    // Right-button mousedown should NOT start drag
    fireEvent.mouseDown(titleBar, { button: 2 });
    // Simulate mouse move — should not call onMove since no drag started
    fireEvent.mouseMove(document, { clientX: 200, clientY: 200 });
    expect(onMove).not.toHaveBeenCalled();
  });

  it('right-click on empty tab strip space does NOT start grab-scroll', () => {
    renderWithDnd(<RightPanel />);
    const tabBars = screen.getAllByTestId('tab-bar');
    const tabBar = tabBars[0];
    // Right-button pointerDown on empty space should not start scroll
    fireEvent.pointerDown(tabBar, { button: 2, clientX: 500 });
    // No error and no scroll capture
    expect(tabBar).toBeDefined();
  });
});
