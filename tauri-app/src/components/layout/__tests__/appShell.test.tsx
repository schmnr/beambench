import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import type { ReactNode } from 'react';

let nativeMenuActive = false;

vi.mock('../../../utils/platform', () => ({
  isNativeMenuActive: () => nativeMenuActive,
}));
vi.mock('../MenuBar', () => ({ MenuBar: () => <div>File</div> }));
vi.mock('../MainToolbar', () => ({ MainToolbar: () => <div>MainToolbar</div> }));
vi.mock('../CreationToolbar', () => ({ CreationToolbar: () => <div>CreationToolbar</div> }));
vi.mock('../NodeSubToolbar', () => ({ NodeSubToolbar: () => <div>NodeSubToolbar</div> }));
vi.mock('../ModifiersToolbar', () => ({ ModifiersToolbar: () => <div>ModifiersToolbar</div> }));
vi.mock('../StatusBar', () => ({ StatusBar: () => <div>StatusBar</div> }));
vi.mock('../RightPanel', () => ({ RightPanel: () => <div>RightPanel</div> }));
vi.mock('../RunPanel', () => ({ RunPanel: () => <div>RunPanel</div> }));
vi.mock('../RunLeftPanel', () => ({ RunLeftPanel: () => <div>RunLeftPanel</div> }));
vi.mock('../LeftPanel', () => ({ LeftPanel: () => <div>LeftPanel</div> }));
vi.mock('../BottomPanel', () => ({ BottomPanel: () => <div>BottomPanel</div> }));
vi.mock('../PanelResizer', () => ({ PanelResizer: () => <div>PanelResizer</div> }));
vi.mock('../FloatingPanelLayer', () => ({ FloatingPanelLayer: () => <div>FloatingPanelLayer</div> }));
vi.mock('../LibraryDrawer', () => ({ LibraryDrawer: () => <div>LibraryDrawer</div> }));
vi.mock('../../canvas/Canvas', () => ({ Canvas: () => <div>Canvas</div> }));
vi.mock('../../layers/LayerTabs', () => ({ LayerTabs: () => <div>LayerTabs</div> }));
vi.mock('../../import/ImportDropZone', () => ({ ImportDropZone: ({ children }: { children: ReactNode }) => <div>{children}</div> }));
vi.mock('../../../panels/DndContext', () => ({ PanelDndProvider: ({ children }: { children: ReactNode }) => <div>{children}</div> }));

import { AppShell } from '../AppShell';
import { useUiStore } from '../../../stores/uiStore';
import { usePreviewStore } from '../../../stores/previewStore';

const initialUiState = useUiStore.getState();
const initialPreviewState = usePreviewStore.getState();

afterEach(() => {
  cleanup();
  nativeMenuActive = false;
  useUiStore.setState(initialUiState, true);
  usePreviewStore.setState(initialPreviewState, true);
});

describe('AppShell workspace modes', () => {
  it('uses only the dedicated machine panels in Run mode', () => {
    const setCanvasPreviewActive = vi.fn();
    usePreviewStore.setState({ setCanvasPreviewActive });
    useUiStore.setState((state) => ({
      workspaceMode: 'run',
      sidePanelsVisible: true,
      panelLayout: {
        ...state.panelLayout,
        zones: {
          ...state.panelLayout.zones,
          left: { panelIds: ['art_library'], activeTab: 'art_library' },
          bottom: { panelIds: ['console'], activeTab: 'console' },
          'upper-right': { panelIds: [], activeTab: '' },
          'lower-right': { panelIds: [], activeTab: '' },
        },
      },
    }));

    render(<AppShell />);

    expect(screen.getByText('RunLeftPanel')).toBeDefined();
    expect(screen.getByText('RunPanel')).toBeDefined();
    expect(screen.queryByText('LeftPanel')).toBeNull();
    expect(screen.queryByText('BottomPanel')).toBeNull();
    expect(screen.queryByText('FloatingPanelLayer')).toBeNull();
    expect(setCanvasPreviewActive).toHaveBeenCalledWith(true);
  });

  it('keeps Run controls visible when Design side panels are hidden', () => {
    usePreviewStore.setState({ setCanvasPreviewActive: vi.fn() });
    useUiStore.setState({ workspaceMode: 'run', sidePanelsVisible: false });

    render(<AppShell />);

    expect(screen.getByText('RunLeftPanel')).toBeDefined();
    expect(screen.getByText('RunPanel')).toBeDefined();
  });
});

describe('AppShell native menu behavior', () => {
  it('renders the React menu bar off macOS', () => {
    nativeMenuActive = false;
    render(<AppShell />);
    expect(screen.getByText('File')).toBeDefined();
  });

  it('hides the React menu bar when the native menu is active', () => {
    nativeMenuActive = true;
    render(<AppShell />);
    expect(screen.queryByText('File')).toBeNull();
  });
});
