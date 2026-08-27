import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent, act } from '@testing-library/react';
import { RightPanel } from '../RightPanel';
import { PanelColumn } from '../PanelColumn';
import { useUiStore } from '../../../stores/uiStore';
import { useProjectStore } from '../../../stores/projectStore';
import { appService } from '../../../services/appService';
import { createDefaultLayout } from '../../../panels';
import { PanelDndProvider } from '../../../panels/DndContext';
import { makeProject, makeProjectObject } from '../../../test-utils/projectFixtures';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockReturnValue(new Promise(() => {})),
}));
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

describe('RightPanel', () => {
  it('uses only the two Design-default tabs in one card', () => {
    renderWithDnd(<RightPanel />);
    expect(screen.getByText('Layers')).toBeDefined();
    expect(screen.getByText('Properties')).toBeDefined();
    expect(screen.queryByText('Move')).toBeNull();
    expect(screen.queryByText('Console')).toBeNull();
    expect(screen.queryByText('Macros')).toBeNull();
    expect(screen.queryByText('Laser Control')).toBeNull();
    expect(screen.queryByText('Material Library')).toBeNull();
    expect(screen.getAllByTestId('tab-bar')).toHaveLength(1);
  });

  it('collapses the second Design dock behind top and bottom reveal handles', () => {
    renderWithDnd(<RightPanel />);
    expect(screen.getByTestId('panel-column-right-top-reveal-handle')).toBeDefined();
    expect(screen.getByTestId('panel-column-right-bottom-reveal-handle')).toBeDefined();
    expect(screen.queryByTestId('right-panel-splitter')).toBeNull();
  });

  it('pulling the bottom handle opens an empty secondary dock', () => {
    const rectSpy = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
      x: 0,
      y: 0,
      top: 0,
      right: 440,
      bottom: 1000,
      left: 0,
      width: 440,
      height: 1000,
      toJSON: () => ({}),
    });
    renderWithDnd(<RightPanel />);

    fireEvent.mouseDown(screen.getByTestId('panel-column-right-bottom-reveal-handle'));
    fireEvent.mouseMove(document, { clientY: 620 });
    fireEvent.mouseUp(document);

    expect(useUiStore.getState().panelLayout.upperSplitRatio).toBeCloseTo(0.62);
    expect(screen.getByTestId('empty-zone-panel-picker-middle-right')).toBeDefined();
    rectSpy.mockRestore();
  });

  it('supports a third bottom dock but no fourth dock', () => {
    useUiStore.getState().revealColumnEdge('right', 'bottom');
    useUiStore.getState().revealColumnEdge('right', 'bottom');
    renderWithDnd(<RightPanel />);

    expect(screen.getByTestId('empty-zone-panel-picker-middle-right')).toBeDefined();
    expect(screen.getByTestId('empty-zone-panel-picker-bottom-right')).toBeDefined();
    expect(screen.queryByTestId('panel-column-right-top-reveal-handle')).toBeNull();
    expect(screen.queryByTestId('panel-column-right-bottom-reveal-handle')).toBeNull();
  });

  it('collapses one section when a three-way divider is dragged to an edge', () => {
    const rectSpy = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
      x: 0,
      y: 0,
      top: 0,
      right: 440,
      bottom: 1000,
      left: 0,
      width: 440,
      height: 1000,
      toJSON: () => ({}),
    });
    useUiStore.getState().revealColumnEdge('right', 'bottom');
    useUiStore.getState().revealColumnEdge('right', 'bottom');
    renderWithDnd(<RightPanel />);

    fireEvent.mouseDown(screen.getByTestId('panel-column-right-splitter-0'));
    fireEvent.mouseMove(document, { clientY: 0 });
    fireEvent.mouseUp(document);

    expect(useUiStore.getState().panelLayout.columnRatios.right[0]).toBe(0);
    expect(useUiStore.getState().panelLayout.columnRatios.right.filter((ratio) => ratio > 0)).toHaveLength(2);
    rectSpy.mockRestore();
  });

  it('can split the Run Move column and add a panel below it', () => {
    useUiStore.setState({ workspaceMode: 'run' });
    useUiStore.getState().revealColumnEdge('left', 'bottom');
    renderWithDnd(<PanelColumn side="left" />);

    expect(screen.getByRole('tab', { name: 'Move' })).toBeDefined();
    expect(screen.getByTestId('empty-zone-panel-picker-middle-left')).toBeDefined();
    act(() => {
      useUiStore.getState().movePanelBetweenZones('camera', 'middle-right', 'middle-left');
    });
    expect(useUiStore.getState().panelLayout.runZones['middle-left'].panelIds).toContain('camera');
  });

  it('adds a selected panel to the revealed dock', () => {
    useUiStore.getState().setUpperSplitRatio(0.62);
    renderWithDnd(<RightPanel />);

    fireEvent.change(screen.getByTestId('empty-zone-panel-picker-middle-right'), {
      target: { value: 'console' },
    });

    const layout = useUiStore.getState().panelLayout;
    expect(layout.zones['middle-right'].panelIds).toContain('console');
    expect(layout.hiddenPanelIds).not.toContain('console');
  });

  it('uses the machine-oriented panel defaults in Run', () => {
    useUiStore.setState({ workspaceMode: 'run' });
    renderWithDnd(<RightPanel />);

    expect(screen.getByText('Laser Control')).toBeDefined();
    expect(screen.queryByText('Material Library')).toBeNull();
    expect(screen.getByText('Camera')).toBeDefined();
    expect(screen.getByText('Macros')).toBeDefined();
    expect(screen.getByText('Console')).toBeDefined();
    expect(screen.queryByText('Layers')).toBeNull();
    expect(screen.getAllByTestId('tab-bar')).toHaveLength(2);
    expect(screen.getAllByTestId('run-inspector-card')).toHaveLength(2);
    for (const card of screen.getAllByTestId('run-inspector-card')) {
      expect(card.className).toContain('border-bb-accent/40');
    }
  });

  it('keeps Design and Run active tabs independent', () => {
    const layout = useUiStore.getState().panelLayout;
    expect(layout.zones['top-right'].activeTab).toBe('cuts_layers');
    useUiStore.setState({ workspaceMode: 'run' });
    renderWithDnd(<RightPanel />);
    fireEvent.click(screen.getByText('Console'));
    const updated = useUiStore.getState().panelLayout;
    expect(updated.runZones['middle-right'].activeTab).toBe('console');
    expect(updated.zones['top-right'].activeTab).toBe('cuts_layers');
  });

  it('uses visible scroll containers for tall docked content', () => {
    useUiStore.setState({ workspaceMode: 'run' });
    const { container } = renderWithDnd(<RightPanel />);
    const scrollPanes = Array.from(container.querySelectorAll('.overflow-y-auto'));

    expect(scrollPanes.length).toBeGreaterThanOrEqual(2);
    for (const pane of scrollPanes) {
      expect(pane.className).not.toContain('scrollbar-none');
    }
  });

  it('switches upper tab on click', () => {
    renderWithDnd(<RightPanel />);
    fireEvent.click(screen.getByText('Properties'));
    expect(useUiStore.getState().panelLayout.zones['top-right'].activeTab).toBe('properties');
  });

  it('updates docked Properties content when an object becomes selected', () => {
    const project = makeProject({ objects: [makeProjectObject({ id: 'obj-1' })] });
    useProjectStore.setState({ project, selectedObjectIds: [] });
    useUiStore.getState().setZoneActiveTab('top-right', 'properties');
    renderWithDnd(<RightPanel />);
    expect(screen.getByText('Select an object to edit its properties')).toBeDefined();

    act(() => useProjectStore.getState().selectObjects(['obj-1']));

    expect(screen.getByTestId('transform-section')).toBeDefined();
    expect(screen.queryByText('Select an object to edit its properties')).toBeNull();
  });

  it('highlights the active upper tab with the original accent underline', () => {
    renderWithDnd(<RightPanel />);
    const cutsTab = screen.getByText('Layers');
    expect(cutsTab.getAttribute('aria-selected')).toBe('true');
    expect(cutsTab.className).toContain('border-bb-accent');
  });

  it('hides a panel when toggled hidden', () => {
    useUiStore.getState().togglePanelVisibility('properties');
    renderWithDnd(<RightPanel />);
    expect(screen.queryByText('Properties')).toBeNull();
  });

  it('shows a panel when toggled visible again', () => {
    useUiStore.getState().togglePanelVisibility('properties');
    useUiStore.getState().togglePanelVisibility('properties');
    renderWithDnd(<RightPanel />);
    expect(screen.getByText('Properties')).toBeDefined();
  });

  it('persists layout when switching tabs', () => {
    renderWithDnd(<RightPanel />);
    fireEvent.click(screen.getByText('Properties'));
    expect(appService.persistLayout).toHaveBeenCalledWith(
      expect.objectContaining({
        zones: expect.objectContaining({
          'top-right': expect.objectContaining({ activeTab: 'properties' }),
        }),
      })
    );
  });

  it('resetLayout restores default state', () => {
    useUiStore.getState().togglePanelVisibility('properties');
    useUiStore.getState().setUpperSplitRatio(0.3);
    useUiStore.getState().resetLayout();
    const layout = useUiStore.getState().panelLayout;
    const def = createDefaultLayout();
    expect(layout.hiddenPanelIds).toEqual(def.hiddenPanelIds);
    expect(layout.upperSplitRatio).toBe(def.upperSplitRatio);
    expect(layout.zones['top-right'].panelIds).toEqual(def.zones['top-right'].panelIds);
  });

  it('togglePanelVisibility switches active tab when hiding current tab', () => {
    useUiStore.getState().setZoneActiveTab('top-right', 'properties');
    useUiStore.getState().togglePanelVisibility('properties');
    const layout = useUiStore.getState().panelLayout;
    expect(layout.zones['top-right'].activeTab).not.toBe('properties');
    expect(layout.hiddenPanelIds).toContain('properties');
  });
});
