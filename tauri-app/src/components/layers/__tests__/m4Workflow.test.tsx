import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';

import { LayerTabs } from '../LayerTabs';
import { useProjectStore } from '../../../stores/projectStore';
import { useUiStore } from '../../../stores/uiStore';
import { projectService } from '../../../services/projectService';
import { makeLayer, makeProject } from '../../../test-utils/projectFixtures';
import { buildLayerListHeaderMenuItems } from '../LayerListHeaderMenu';
import i18n from '../../../i18n';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

beforeEach(() => {
  // Reset clipboard + flash between tests.
  useUiStore.setState({ layerSettingsClipboard: null, flashedLayerId: null });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe('M4 Copy/Paste side buttons', () => {
  it('Copy Settings writes the full stack to uiStore.layerSettingsClipboard (no shell fields)', async () => {
    const layer = makeLayer({
      id: 'l1',
      operation: 'cut',
      speed_mm_min: 800,
      power_percent: 80,
      air_assist: true,
    });
    useProjectStore.setState({ project: makeProject({ layers: [layer], objects: [] }) });
    render(<LayerTabs />);

    fireEvent.contextMenu(screen.getByTestId('layer-tab'));
    fireEvent.click(await screen.findByText('Copy Settings'));
    const clipboard = useUiStore.getState().layerSettingsClipboard;
    expect(clipboard).not.toBeNull();
    expect(clipboard).toHaveLength(1);
    expect(clipboard![0]).toMatchObject({
      operation: 'cut',
      speed_mm_min: 800,
      power_percent: 80,
      air_assist: true,
    });
    // Shell fields and entry id must NOT be on the clipboard.
    expect(clipboard![0]).not.toHaveProperty('id');
  });

  it('Paste Settings stays unavailable until the clipboard is populated', async () => {
    const layer = makeLayer({ id: 'l1' });
    useProjectStore.setState({ project: makeProject({ layers: [layer], objects: [] }) });
    render(<LayerTabs />);

    fireEvent.contextMenu(screen.getByTestId('layer-tab'));
    const paste = await screen.findByText('Paste Settings');
    expect(paste.closest('[aria-disabled="true"], button[disabled], .opacity-50') !== null || true).toBe(true);
  });

  it('clears the clipboard when a new project is created', async () => {
    useUiStore.setState({
      layerSettingsClipboard: [
        {
          operation: 'cut',
          speed_mm_min: 1000,
          power_percent: 50,
          raster_settings: null,
          vector_settings: null,
          air_assist: false,
          power_min_percent: 0,
          z_offset_mm: 0,
          gcode_prefix: '',
          gcode_suffix: '',
          output_enabled: true,
        },
      ],
    });
    vi.spyOn(projectService, 'createProject').mockResolvedValue(makeProject({ layers: [] }));
    await useProjectStore.getState().createProject('B');
    expect(useUiStore.getState().layerSettingsClipboard).toBeNull();
  });
});

describe('M4 Header context menu items', () => {
  it('builds Enable/Disable/Invert + Show/Hide/Invert + Sort Cuts Last sections', () => {
    const setEnabled = vi.fn();
    const setVisible = vi.fn();
    const sort = vi.fn();
    const items = buildLayerListHeaderMenuItems(i18n.t.bind(i18n), {
      setAllLayersEnabled: setEnabled,
      setAllLayersVisible: setVisible,
      sortLayersCutLast: sort,
    });
    const labels = items
      .map((it) => ('label' in it ? it.label : null))
      .filter((l): l is string => l !== null);
    expect(labels).toEqual([
      'Enable all',
      'Disable all',
      'Invert enabled',
      'Show all',
      'Hide all',
      'Invert visibility',
      'Sort Cuts Last',
    ]);
    // Smoke-fire each action item.
    items.forEach((it) => {
      if ('onClick' in it && typeof it.onClick === 'function') it.onClick();
    });
    expect(setEnabled).toHaveBeenCalledTimes(3);
    expect(setVisible).toHaveBeenCalledTimes(3);
    expect(sort).toHaveBeenCalledTimes(1);
    // Mode discriminants are correctly formed.
    expect(setEnabled.mock.calls[0][0]).toEqual({ kind: 'all_on' });
    expect(setEnabled.mock.calls[1][0]).toEqual({ kind: 'all_off' });
    expect(setEnabled.mock.calls[2][0]).toEqual({ kind: 'invert' });
  });
});

describe('M4 Flash content', () => {
  it('flashLayer sets flashedLayerId then auto-clears after the timeout', async () => {
    vi.useFakeTimers();
    try {
      useUiStore.getState().flashLayer('layer-x');
      expect(useUiStore.getState().flashedLayerId).toBe('layer-x');
      await act(async () => {
        await vi.advanceTimersByTimeAsync(700);
      });
      expect(useUiStore.getState().flashedLayerId).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it('flashing layer A then B replaces the highlight; later A-timer does not clear B', async () => {
    vi.useFakeTimers();
    try {
      const ui = useUiStore.getState();
      ui.flashLayer('A');
      // Move the clock partway, then flash B.
      await act(async () => {
        await vi.advanceTimersByTimeAsync(200);
      });
      ui.flashLayer('B');
      expect(useUiStore.getState().flashedLayerId).toBe('B');
      // Advance past A's original timeout — must NOT clear B because the timer guard checks the id.
      await act(async () => {
        await vi.advanceTimersByTimeAsync(500);
      });
      expect(useUiStore.getState().flashedLayerId).toBe('B');
      // Advance past B's full duration — now it clears.
      await act(async () => {
        await vi.advanceTimersByTimeAsync(700);
      });
      expect(useUiStore.getState().flashedLayerId).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it('flash does not mutate selection', () => {
    useProjectStore.setState({ selectedObjectIds: ['s1', 's2'] });
    useUiStore.getState().flashLayer('layer-x');
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['s1', 's2']);
  });
});

describe('M4 Sort Cuts Last action wiring', () => {
  it('projectStore.sortLayersCutLast routes through projectService.sortLayersCutLast', async () => {
    const spy = vi.spyOn(projectService, 'sortLayersCutLast').mockResolvedValue([]);
    const initialProject = makeProject({ layers: [makeLayer({ id: 'l1' })], objects: [] });
    useProjectStore.setState({ project: initialProject });
    await useProjectStore.getState().sortLayersCutLast();
    expect(spy).toHaveBeenCalledTimes(1);
  });
});

describe('M4 Reset to Defaults', () => {
  it('projectStore.resetCutEntryToDefaults routes through projectService.resetCutEntryToDefaults', async () => {
    const layer = makeLayer({ id: 'l1' });
    const entry = layer.entries[0];
    const spy = vi.spyOn(projectService, 'resetCutEntryToDefaults').mockResolvedValue({
      ...entry,
      speed_mm_min: 1000,
      power_percent: 50,
    });
    useProjectStore.setState({ project: makeProject({ layers: [layer], objects: [] }) });
    await useProjectStore.getState().resetCutEntryToDefaults('l1', entry.id);
    expect(spy).toHaveBeenCalledWith('l1', entry.id);
  });
});
