import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent } from '@testing-library/react';
import { RightPanel } from '../RightPanel';
import { useUiStore } from '../../../stores/uiStore';
import { useProjectStore } from '../../../stores/projectStore';
import { appService } from '../../../services/appService';
import { createDefaultLayout } from '../../../panels';
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

describe('RightPanel', () => {
  it('renders upper zone with 5 tabs in correct order', () => {
    renderWithDnd(<RightPanel />);
    const expectedOrder = ['Cuts / Layers', 'Move', 'Console', 'Macros', 'Shape Properties'];
    for (const label of expectedOrder) {
      expect(screen.getByText(label)).toBeDefined();
    }
    // Verify order by checking all tab bar buttons
    const tabBars = screen.getAllByTestId('tab-bar');
    const upperTabBar = tabBars[0];
    const buttons = upperTabBar.querySelectorAll('button');
    const labels = Array.from(buttons)
      .map((b) => b.textContent?.trim())
      .filter((t) => t && !t.includes('⊡'));
    expect(labels.indexOf('Macros')).toBeLessThan(labels.indexOf('Shape Properties'));
  });

  it('renders lower zone with 2 tabs', () => {
    renderWithDnd(<RightPanel />);
    expect(screen.getByText('Laser Control')).toBeDefined();
    expect(screen.getByText('Material Library')).toBeDefined();
    // Color Palette moved to bottom zone
  });

  it('defaults to cuts_layers upper tab and laser lower tab', () => {
    const layout = useUiStore.getState().panelLayout;
    expect(layout.zones['upper-right'].activeTab).toBe('cuts_layers');
    expect(layout.zones['lower-right'].activeTab).toBe('laser');
  });

  it('uses visible scroll containers for tall docked content', () => {
    const { container } = renderWithDnd(<RightPanel />);
    const scrollPanes = Array.from(container.querySelectorAll('.overflow-y-auto'));

    expect(scrollPanes).toHaveLength(2);
    for (const pane of scrollPanes) {
      expect(pane.className).not.toContain('scrollbar-none');
    }
  });

  it('switches upper tab on click', () => {
    renderWithDnd(<RightPanel />);
    fireEvent.click(screen.getByText('Console'));
    expect(useUiStore.getState().panelLayout.zones['upper-right'].activeTab).toBe('console');
  });

  it('switches lower tab on click', () => {
    renderWithDnd(<RightPanel />);
    fireEvent.click(screen.getByText('Material Library'));
    expect(useUiStore.getState().panelLayout.zones['lower-right'].activeTab).toBe('material');
  });

  it('highlights active upper tab with accent border', () => {
    renderWithDnd(<RightPanel />);
    const cutsTab = screen.getByText('Cuts / Layers');
    expect(cutsTab.className).toContain('border-bb-accent');
  });

  it('highlights active lower tab with accent border', () => {
    renderWithDnd(<RightPanel />);
    const laserTab = screen.getByText('Laser Control');
    expect(laserTab.className).toContain('border-bb-accent');
  });

  it('hides a panel when toggled hidden', () => {
    useUiStore.getState().togglePanelVisibility('macros');
    renderWithDnd(<RightPanel />);
    expect(screen.queryByText('Macros')).toBeNull();
  });

  it('shows a panel when toggled visible again', () => {
    useUiStore.getState().togglePanelVisibility('macros');
    useUiStore.getState().togglePanelVisibility('macros');
    renderWithDnd(<RightPanel />);
    expect(screen.getByText('Macros')).toBeDefined();
  });

  it('persists layout when switching tabs', () => {
    renderWithDnd(<RightPanel />);
    fireEvent.click(screen.getByText('Console'));
    expect(appService.persistLayout).toHaveBeenCalledWith(
      expect.objectContaining({
        zones: expect.objectContaining({
          'upper-right': expect.objectContaining({ activeTab: 'console' }),
        }),
      })
    );
  });

  it('resetLayout restores default state', () => {
    useUiStore.getState().togglePanelVisibility('macros');
    useUiStore.getState().setUpperSplitRatio(0.3);
    useUiStore.getState().resetLayout();
    const layout = useUiStore.getState().panelLayout;
    const def = createDefaultLayout();
    expect(layout.hiddenPanelIds).toEqual(def.hiddenPanelIds);
    expect(layout.upperSplitRatio).toBe(def.upperSplitRatio);
    expect(layout.zones['upper-right'].panelIds).toEqual(def.zones['upper-right'].panelIds);
  });

  it('togglePanelVisibility switches active tab when hiding current tab', () => {
    useUiStore.getState().setZoneActiveTab('upper-right', 'macros');
    useUiStore.getState().togglePanelVisibility('macros');
    const layout = useUiStore.getState().panelLayout;
    expect(layout.zones['upper-right'].activeTab).not.toBe('macros');
    expect(layout.hiddenPanelIds).toContain('macros');
  });
});
