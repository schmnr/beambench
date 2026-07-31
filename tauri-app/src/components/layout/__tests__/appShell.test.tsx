import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import type { ReactNode } from 'react';

let nativeMenuActive = false;
const dndMocks = vi.hoisted(() => ({ registerDropZone: vi.fn() }));

vi.mock('../../../utils/platform', () => ({
  isNativeMenuActive: () => nativeMenuActive,
}));
vi.mock('../MenuBar', () => ({ MenuBar: () => <div>File</div> }));
vi.mock('../MainToolbar', () => ({ MainToolbar: () => <div>MainToolbar</div> }));
vi.mock('../CreationToolbar', () => ({ CreationToolbar: () => <div>CreationToolbar</div> }));
vi.mock('../ModifiersToolbar', () => ({ ModifiersToolbar: () => <div>ModifiersToolbar</div> }));
vi.mock('../StatusBar', () => ({ StatusBar: () => <div>StatusBar</div> }));
vi.mock('../RightPanel', () => ({ RightPanel: () => <div>RightPanel</div> }));
vi.mock('../PanelColumn', () => ({ PanelColumn: ({ side }: { side: string }) => <div>PanelColumn-{side}</div> }));
vi.mock('../BottomPanel', () => ({ BottomPanel: () => <div>BottomPanel</div> }));
vi.mock('../PanelResizer', () => ({ PanelResizer: () => <div>PanelResizer</div> }));
vi.mock('../FloatingPanelLayer', () => ({ FloatingPanelLayer: () => <div>FloatingPanelLayer</div> }));
vi.mock('../LibraryDrawer', () => ({ LibraryDrawer: () => <div>LibraryDrawer</div> }));
vi.mock('../../canvas/Canvas', () => ({ Canvas: () => <div>Canvas</div> }));
vi.mock('../../layers/LayerTabs', () => ({ LayerTabs: () => <div>LayerTabs</div> }));
vi.mock('../../import/ImportDropZone', () => ({ ImportDropZone: ({ children }: { children: ReactNode }) => <div>{children}</div> }));
vi.mock('../../../panels/DndContext', () => ({
  PanelDndProvider: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  usePanelDnd: () => ({
    dragState: null,
    registerDropZone: dndMocks.registerDropZone,
  }),
}));

import { AppShell } from '../AppShell';
import { useUiStore } from '../../../stores/uiStore';
import { usePreviewStore } from '../../../stores/previewStore';
import { useProjectStore } from '../../../stores/projectStore';

const initialUiState = useUiStore.getState();
const initialPreviewState = usePreviewStore.getState();
const initialProjectState = useProjectStore.getState();

afterEach(() => {
  cleanup();
  nativeMenuActive = false;
  useUiStore.setState(initialUiState, true);
  usePreviewStore.setState(initialPreviewState, true);
  useProjectStore.setState(initialProjectState, true);
  dndMocks.registerDropZone.mockClear();
});

describe('AppShell workspace modes', () => {
  it('switches to Properties when the object selection changes', () => {
    useUiStore.getState().setZoneActiveTab('top-right', 'cuts_layers');
    render(<AppShell />);

    act(() => useProjectStore.setState({ selectedObjectIds: ['obj-1'] }));
    expect(useUiStore.getState().panelLayout.zones['top-right'].activeTab).toBe('properties');

    act(() => useUiStore.getState().setZoneActiveTab('top-right', 'cuts_layers'));
    act(() => useProjectStore.setState({ selectedObjectIds: ['obj-1'] }));
    expect(useUiStore.getState().panelLayout.zones['top-right'].activeTab).toBe('cuts_layers');

    act(() => useProjectStore.setState({ selectedObjectIds: ['obj-2'] }));
    expect(useUiStore.getState().panelLayout.zones['top-right'].activeTab).toBe('properties');
  });

  it('finds Properties after the user moves it to another dock', () => {
    useUiStore.getState().dockPanel('console', 'middle-right');
    useUiStore.getState().movePanelBetweenZones('properties', 'top-right', 'middle-right');
    useUiStore.getState().setZoneActiveTab('middle-right', 'console');

    render(<AppShell />);
    act(() => useProjectStore.setState({ selectedObjectIds: ['obj-1'] }));

    const layout = useUiStore.getState().panelLayout;
    expect(layout.zones['middle-right'].activeTab).toBe('properties');
    expect(layout.zones['top-right'].activeTab).toBe('cuts_layers');
  });

  it('reveals the relocated Properties panel when the text tool is chosen', () => {
    useUiStore.getState().dockPanel('console', 'middle-right');
    useUiStore.getState().movePanelBetweenZones('properties', 'top-right', 'middle-right');
    useUiStore.getState().setZoneActiveTab('middle-right', 'console');
    useUiStore.setState({ sidePanelsVisible: false, activeTool: 'select', workspaceMode: 'design' });

    render(<AppShell />);
    act(() => useUiStore.setState({ activeTool: 'text' }));

    const state = useUiStore.getState();
    expect(state.sidePanelsVisible).toBe(true);
    expect(state.panelLayout.zones['middle-right'].activeTab).toBe('properties');
  });

  it('reveals the relocated Properties panel when the node tool is chosen', () => {
    useUiStore.getState().dockPanel('console', 'middle-right');
    useUiStore.getState().movePanelBetweenZones('properties', 'top-right', 'middle-right');
    useUiStore.getState().setZoneActiveTab('middle-right', 'console');
    useUiStore.setState({ sidePanelsVisible: false, activeTool: 'select', workspaceMode: 'design' });

    render(<AppShell />);
    act(() => useUiStore.setState({ activeTool: 'node' }));

    const state = useUiStore.getState();
    expect(state.sidePanelsVisible).toBe(true);
    expect(state.panelLayout.zones['middle-right'].activeTab).toBe('properties');
  });

  it('reveals the relocated Properties panel when the radius tool is chosen', () => {
    useUiStore.getState().dockPanel('console', 'middle-right');
    useUiStore.getState().movePanelBetweenZones('properties', 'top-right', 'middle-right');
    useUiStore.getState().setZoneActiveTab('middle-right', 'console');
    useUiStore.setState({ sidePanelsVisible: false, activeTool: 'select', workspaceMode: 'design' });

    render(<AppShell />);
    act(() => useUiStore.setState({ activeTool: 'radius' }));

    const state = useUiStore.getState();
    expect(state.sidePanelsVisible).toBe(true);
    expect(state.panelLayout.zones['middle-right'].activeTab).toBe('properties');
  });

  it('closes contextual modifier controls when the source selection changes', () => {
    useProjectStore.setState({ selectedObjectIds: ['obj-1'] });
    useUiStore.setState({
      modifierPropertiesSession: { kind: 'offset', objectIds: ['obj-1'] },
    });

    render(<AppShell />);
    act(() => useProjectStore.setState({ selectedObjectIds: ['obj-2'] }));

    expect(useUiStore.getState().modifierPropertiesSession).toBeNull();
  });

  it('reopens an emptied dock without resetting the layout', () => {
    useUiStore.setState((state) => ({
      workspaceMode: 'design',
      sidePanelsVisible: true,
      panelLayout: {
        ...state.panelLayout,
        zones: {
          ...state.panelLayout.zones,
          'top-right': { panelIds: [], activeTab: '' },
          'middle-right': { panelIds: [], activeTab: '' },
          'bottom-right': { panelIds: [], activeTab: '' },
        },
      },
    }));

    render(<AppShell />);

    expect(screen.queryByText('RightPanel')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Open right dock' }));
    expect(screen.getByText('RightPanel')).toBeDefined();
  });

  it('offers a slim recovery grip for an unused left dock', () => {
    render(<AppShell />);

    expect(screen.queryByText('PanelColumn-left')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Open left dock' }));
    expect(screen.getByText('PanelColumn-left')).toBeDefined();
  });

  it('accepts panel drops on collapsed left and bottom dock edges', () => {
    render(<AppShell />);

    expect(dndMocks.registerDropZone).toHaveBeenCalledWith('top-left', expect.any(HTMLElement));
    expect(dndMocks.registerDropZone).toHaveBeenCalledWith('bottom', expect.any(HTMLElement));
  });

  it('closes a recovered left dock after its last panel is pulled out', () => {
    render(<AppShell />);
    fireEvent.click(screen.getByRole('button', { name: 'Open left dock' }));

    act(() => useUiStore.getState().addPanelInstance('console', 'top-left'));
    expect(screen.getByText('PanelColumn-left')).toBeDefined();

    act(() => useUiStore.getState().floatPanel('console', 100, 100, 420, 300));
    expect(screen.queryByText('PanelColumn-left')).toBeNull();
    expect(screen.getByRole('button', { name: 'Open left dock' })).toBeDefined();
  });

  it('lets the entire left dock collapse against the tool edge', () => {
    useUiStore.setState({ workspaceMode: 'run' });
    render(<AppShell />);
    expect(screen.getByText('PanelColumn-left')).toBeDefined();

    act(() => useUiStore.getState().setLeftPanelWidth(149));

    expect(screen.queryByText('PanelColumn-left')).toBeNull();
    expect(screen.getByRole('button', { name: 'Open left dock' })).toBeDefined();
  });

  it('reveals an empty bottom dock from its edge grip', () => {
    render(<AppShell />);
    expect(screen.queryByText('BottomPanel')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Open bottom dock' }));

    expect(screen.getByText('BottomPanel')).toBeDefined();
    expect(screen.getByTestId('bottom-dock')).toBeDefined();
  });

  it('uses configurable panel columns in Run mode', () => {
    useUiStore.setState((state) => ({
      workspaceMode: 'run',
      sidePanelsVisible: true,
      panelLayout: {
        ...state.panelLayout,
        runZones: {
          ...state.panelLayout.runZones,
          'top-left': { panelIds: ['move'], activeTab: 'move' },
          'top-right': { panelIds: ['laser'], activeTab: 'laser' },
        },
      },
    }));

    render(<AppShell />);

    expect(screen.getByText('PanelColumn-left')).toBeDefined();
    expect(screen.getByText('RightPanel')).toBeDefined();
    expect(screen.getByText('FloatingPanelLayer')).toBeDefined();
  });

  it('keeps Run controls visible when Design side panels are hidden', () => {
    useUiStore.setState({ workspaceMode: 'run', sidePanelsVisible: false });

    render(<AppShell />);

    expect(screen.getByText('PanelColumn-left')).toBeDefined();
    expect(screen.getByText('RightPanel')).toBeDefined();
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
