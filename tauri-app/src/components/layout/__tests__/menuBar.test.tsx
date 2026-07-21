import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent, act, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';
import { MenuBar } from '../MenuBar';
import { useMachineStore } from '../../../stores/machineStore';
import { usePreviewStore } from '../../../stores/previewStore';
import { useProjectStore } from '../../../stores/projectStore';
import { useUiStore } from '../../../stores/uiStore';
import { useNotificationStore } from '../../../stores/notificationStore';
import { previewService } from '../../../services/previewService';
import { persistenceService } from '../../../services/persistenceService';
import { printService } from '../../../services/printService';
import { appService } from '../../../services/appService';
import { projectService } from '../../../services/projectService';
import { makeAppSettings, makeLayer, makeProject as makeProjectFixture, makeProjectObject, makeTextObjectData } from '../../../test-utils/projectFixtures';
import { TOOLS_MENU_CONTRACT } from '../../../commands/toolsMenuContract';
import { APP_COMMANDS } from '../../../commands/appCommandIds';
import { getCommand } from '../../../commands/commandRegistry';
import { WINDOW_MENU_COMMAND_ORDER } from '../../../commands/windowMenuDefinitions';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn().mockResolvedValue(null),
  save: vi.fn().mockResolvedValue(null),
}));

HTMLCanvasElement.prototype.getContext = vi.fn().mockReturnValue({
  clearRect: vi.fn(),
  drawImage: vi.fn(),
  getImageData: vi.fn(() => ({ data: new Uint8ClampedArray(4) })),
  putImageData: vi.fn(),
  fillRect: vi.fn(),
  strokeRect: vi.fn(),
  beginPath: vi.fn(),
  moveTo: vi.fn(),
  lineTo: vi.fn(),
  stroke: vi.fn(),
  fill: vi.fn(),
  arc: vi.fn(),
}) as never;

const initialProjectState = useProjectStore.getState();
const initialMachineState = useMachineStore.getState();
const initialPreviewState = usePreviewStore.getState();
const initialUiState = useUiStore.getState();
const initialNotificationState = useNotificationStore.getState();

if (!('createObjectURL' in URL)) {
  Object.defineProperty(URL, 'createObjectURL', {
    writable: true,
    value: vi.fn(() => 'blob:test'),
  });
}

function mockDefaultInvoke() {
  vi.mocked(invoke).mockImplementation(async (command: string) => {
    if (command === 'get_app_settings') return makeAppSettings();
    return null;
  });
}

mockDefaultInvoke();

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.mocked(invoke).mockReset();
  mockDefaultInvoke();
  vi.mocked(open).mockReset();
  vi.mocked(open).mockResolvedValue(null);
  vi.mocked(save).mockReset();
  vi.mocked(save).mockResolvedValue(null);
  useMachineStore.setState(initialMachineState, true);
  usePreviewStore.setState(initialPreviewState, true);
  useProjectStore.setState(initialProjectState, true);
  useUiStore.setState(initialUiState, true);
  useNotificationStore.setState(initialNotificationState, true);
});

function setProjectWithSelection(selection: string[] = ['txt1', 'path1']) {
  // use shared typed builders so the fixture satisfies the full
  // Project/Layer/ProjectObject schemas without partial `as never` casts.
  useProjectStore.setState({
    project: makeProjectFixture({
      layers: [
        makeLayer({ id: 'l1', name: 'Layer 1', operation: 'cut', order_index: 0, color_tag: '#ff0000', speed_mm_min: 1000, power_percent: 100 }),
        makeLayer({ id: 'l2', name: 'Layer 2', operation: 'cut', order_index: 1, color_tag: '#00ff00', speed_mm_min: 800, power_percent: 80 }),
      ],
      objects: [
        makeProjectObject({
          id: 'txt1',
          name: 'Text',
          layer_id: 'l1',
          bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
          data: makeTextObjectData({ content: 'Hello', font_family: 'Arial', font_size_mm: 5 }),
        }),
        makeProjectObject({
          id: 'path1',
          name: 'Path',
          layer_id: 'l1',
          bounds: { min: { x: 20, y: 20 }, max: { x: 30, y: 30 } },
          z_index: 1,
          data: { type: 'vector_path', path_data: 'M0,0 L10,0', closed: false },
        }),
        makeProjectObject({
          id: 'path2',
          name: 'Path 2',
          layer_id: 'l1',
          bounds: { min: { x: 31, y: 31 }, max: { x: 39, y: 39 } },
          z_index: 2,
          data: { type: 'vector_path', path_data: 'M5,5 L15,5', closed: false },
        }),
        makeProjectObject({
          id: 'img1',
          name: 'Image',
          layer_id: 'l1',
          bounds: { min: { x: 40, y: 40 }, max: { x: 60, y: 60 } },
          z_index: 3,
          data: { type: 'raster_image', asset_key: 'asset1', original_width_px: 100, original_height_px: 100 },
        }),
        makeProjectObject({
          id: 'clone1',
          name: 'Clone Path',
          layer_id: 'l1',
          bounds: { min: { x: 50, y: 50 }, max: { x: 60, y: 60 } },
          z_index: 4,
          data: { type: 'virtual_clone', source_id: 'path1' },
        }),
      ],
    }),
    selectedObjectIds: selection,
    selectedLayerId: 'l1',
  });
}

function setMachineReady() {
  useMachineStore.setState({
    sessionState: 'ready',
    machineStatus: {
      run_state: 'idle',
      machine_position: { x: 0, y: 0, z: 0 },
      work_position: { x: 0, y: 0, z: 0 },
      feed_rate: 0,
      spindle_speed: 0,
      feed_override: 100,
      spindle_override: 100,
      rapid_override: 100,
      pin_states: '',
    },
    loading: false,
  });
}

function openedMenuLabels(): string[] {
  return Array.from(document.querySelectorAll('button.w-full'))
    .map((button) => button.querySelector('span')?.textContent?.trim() ?? '')
    .filter(Boolean);
}

function openedMenuButton(label: string): HTMLButtonElement {
  const button = Array.from(document.querySelectorAll<HTMLButtonElement>('button.w-full'))
    .find((candidate) => candidate.querySelector('span')?.textContent?.trim() === label);
  if (!button) throw new Error(`Menu item not found: ${label}`);
  return button;
}

describe('MenuBar', () => {
  it('File menu shows Notes item', () => {
    setProjectWithSelection();
    render(<MenuBar />);
    fireEvent.click(screen.getByText('File'));
    expect(screen.getByText('Show Notes')).toBeDefined();
  });

  it('File menu Import uses the same selected-layer behavior as the toolbar button', async () => {
    const project = makeProjectFixture();
    const importFiles = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project,
      selectedLayerId: project.layers[0].id,
      importFiles,
    });

    render(<MenuBar />);
    fireEvent.click(screen.getByText('File'));
    const importItem = screen.getByRole('button', { name: /^Import\b/ });
    expect(importItem.hasAttribute('disabled')).toBe(false);
    fireEvent.click(importItem);

    await waitFor(() => {
      expect(importFiles).toHaveBeenCalledWith(project.layers[0].id);
    });
  });

  it('File menu New Window opens a second app window', async () => {
    const openNewWindow = vi.spyOn(appService, 'openNewWindow').mockResolvedValue('main-test');

    render(<MenuBar />);
    fireEvent.click(screen.getByText('File'));
    const newWindowItem = screen.getByRole('button', { name: 'New Window' });
    expect(newWindowItem.hasAttribute('disabled')).toBe(false);
    fireEvent.click(newWindowItem);

    await waitFor(() => {
      expect(openNewWindow).toHaveBeenCalledOnce();
    });
  });

  it('Laser Tools menu routes Save Machine Files to G-code export', () => {
    setProjectWithSelection();
    usePreviewStore.setState({ state: 'current' });
    const exportGcode = vi.spyOn(previewService, 'exportGcode').mockResolvedValue('output.gcode');
    const exportArtwork = vi.spyOn(persistenceService, 'exportArtwork').mockResolvedValue('output.svg');

    render(<MenuBar />);
    fireEvent.click(screen.getByText('Laser Tools'));
    fireEvent.click(screen.getByText('Save Machine Files'));

    expect(exportGcode).toHaveBeenCalledOnce();
    expect(exportArtwork).not.toHaveBeenCalled();
  });

  it('Laser Tools menu opens Material Test', async () => {
    render(<MenuBar />);
    fireEvent.click(screen.getByText('Laser Tools'));
    fireEvent.click(screen.getByText('Material Test...'));

    expect(await screen.findByRole('dialog', { name: 'Material Test' })).toBeDefined();
  });

  it('Tools menu shows the required command items', () => {
    setProjectWithSelection(['txt1', 'path1']);
    render(<MenuBar />);
    fireEvent.click(screen.getByText('Tools'));
    expect(screen.getByText('Draw Shape')).toBeDefined();
    expect(screen.getByText('Apply Path to Text')).toBeDefined();
    expect(screen.getByText('Apply Mask to Image')).toBeDefined();
    expect(screen.getByText('Crop Image')).toBeDefined();
    expect(screen.getByText('Adjust Image')).toBeDefined();
    expect(screen.queryByText('Multi-File Trace Image')).toBeNull();
    expect(screen.queryByText('Resize Slots in Selection')).toBeNull();
    expect(screen.getByText('Boolean Intersection')).toBeDefined();
    expect(screen.getByText('Weld Shapes')).toBeDefined();
    expect(screen.queryByText('Convert to Bitmap')).toBeNull();
    expect(screen.queryByText('Boolean Exclude')).toBeNull();
    expect(screen.queryByText('Exclude')).toBeNull();
  });

  it('Tools menu command order matches the product contract', () => {
    setProjectWithSelection(['txt1', 'path1']);
    render(<MenuBar />);
    fireEvent.click(screen.getByText('Tools'));

    expect(openedMenuLabels().filter((label) => label !== 'Draw Shape')).toEqual(
      TOOLS_MENU_CONTRACT.map((item) => item.label),
    );
  });

  it('Apply Mask to Image assigns one image and closed vector masks', async () => {
    const assignImageMask = vi.fn().mockResolvedValue(undefined);
    setProjectWithSelection(['img1', 'path1']);
    useProjectStore.setState((state) => ({
      assignImageMask,
      project: state.project
        ? {
            ...state.project,
            objects: state.project.objects.map((object) =>
              object.id === 'path1' && object.data.type === 'vector_path'
                ? { ...object, data: { ...object.data, closed: true } }
                : object,
            ),
          }
        : state.project,
    }));

    render(<MenuBar />);
    fireEvent.click(screen.getByText('Tools'));
    fireEvent.click(screen.getByText('Apply Mask to Image'));

    await waitFor(() => {
      expect(assignImageMask).toHaveBeenCalledWith('img1', ['path1'], 'keep_inside');
    });
  });

  it('Apply Mask to Image is disabled for open mask selections', () => {
    const assignImageMask = vi.fn().mockResolvedValue(undefined);
    setProjectWithSelection(['img1', 'path1']);
    useProjectStore.setState({ assignImageMask });

    render(<MenuBar />);
    fireEvent.click(screen.getByText('Tools'));
    const item = screen.getByRole('button', { name: /Apply Mask to Image/ });

    expect(item.hasAttribute('disabled')).toBe(true);
    expect(assignImageMask).not.toHaveBeenCalled();
  });

  it('Tools menu opens Adjust Image dialog for a raster selection', () => {
    setProjectWithSelection(['img1']);
    render(<MenuBar />);
    fireEvent.click(screen.getByText('Tools'));
    fireEvent.click(screen.getByText('Adjust Image'));
    expect(screen.getByText('Adjust Image')).toBeDefined();
  });

  it('menu-launched Adjust Image stays bound to the launch selection', () => {
    setProjectWithSelection(['img1']);
    render(<MenuBar />);

    fireEvent.click(screen.getByText('Tools'));
    fireEvent.click(screen.getByText('Adjust Image'));
    expect(screen.getByText('Adjust Image')).toBeDefined();

    act(() => {
      useProjectStore.setState({ selectedObjectIds: [] });
    });
    expect(screen.getByText('Adjust Image')).toBeDefined();
  });

  it('menu-launched Trace Image stays bound to the launch selection', () => {
    setProjectWithSelection(['img1']);
    render(<MenuBar />);

    fireEvent.click(screen.getByText('Tools'));
    fireEvent.click(screen.getByText('Trace Image'));
    expect(screen.getByText('Trace Image')).toBeDefined();

    act(() => {
      useProjectStore.setState({ selectedObjectIds: [] });
    });
    expect(screen.getByText('Trace Image')).toBeDefined();
  });

  it('Edit menu exposes image-options submenu items', () => {
    setProjectWithSelection(['img1']);
    render(<MenuBar />);
    fireEvent.click(screen.getByText('Edit'));
    expect(screen.getByText('Image Options')).toBeDefined();
    expect(screen.getByText('Refresh Image')).toBeDefined();
    expect(screen.getByText('Replace Image')).toBeDefined();
    expect(screen.getByText('Replace Image to Fit')).toBeDefined();
  });

  it('disables Refresh Image when the selected raster asset has no source path', () => {
    setProjectWithSelection(['img1']);
    render(<MenuBar />);
    fireEvent.click(screen.getByText('Edit'));
    fireEvent.click(screen.getByText('Image Options'));

    expect(screen.getByRole('button', { name: 'Refresh Image' }).hasAttribute('disabled')).toBe(true);

    act(() => {
      useProjectStore.setState((state) => ({
        project: state.project
          ? {
              ...state.project,
              assets: [{
                id: 'asset1',
                original_filename: 'image.png',
                media_type: 'png',
                byte_size: 10,
                width_px: 100,
                height_px: 100,
                source_path: '/tmp/image.png',
              }],
            }
          : state.project,
      }));
    });

    expect(screen.getByRole('button', { name: 'Refresh Image' }).hasAttribute('disabled')).toBe(false);
  });

  it('Edit menu follows the product order', () => {
    setProjectWithSelection(['img1']);
    render(<MenuBar />);
    fireEvent.click(screen.getByText('Edit'));

    expect(openedMenuLabels()).toEqual([
      'Undo',
      'Redo',
      'Select All',
      'Invert Selection',
      'Cut',
      'Copy',
      'Duplicate',
      'Paste',
      'Paste in Place',
      'Delete',
      'Convert to Path',
      'Convert to Bitmap',
      'Close Path',
      'Close Selected Paths With Tolerance',
      'Auto-Join Selected Shapes',
      'Close & Join',
      'Optimize Selected Shapes',
      'Delete Duplicates',
      'Select Open Shapes',
      'Select Open Shapes Set to Fill',
      'Select All Shapes in Current Layer',
      'Select Contained Shapes',
      'Select Shapes Smaller Than Selected',
      'Image Options',
      'Refresh Image',
      'Replace Image',
      'Replace Image to Fit',
      'Settings',
    ]);
  });

  it('Edit menu disables Paste while the object clipboard is empty', () => {
    setProjectWithSelection();
    useUiStore.setState({ hasClipboard: false });

    const { rerender } = render(<MenuBar />);
    fireEvent.click(screen.getByText('Edit'));
    expect(openedMenuButton('Paste').disabled).toBe(true);

    useUiStore.setState({ hasClipboard: true });
    rerender(<MenuBar />);
    expect(openedMenuButton('Paste').disabled).toBe(false);
  });

  it('Arrange menu follows the product order', () => {
    setProjectWithSelection(['txt1', 'path1']);
    render(<MenuBar />);
    fireEvent.click(screen.getByText('Arrange'));

    expect(openedMenuLabels()).toEqual([
      'Group',
      'Ungroup',
      'Ungroup',
      'Auto-Group',
      'Flip Horizontal / Vertical',
      'Flip Horizontal',
      'Flip Vertical',
      'Mirror Across Line',
      'Rotate 90° Clockwise / Counter-Clockwise',
      'Rotate 90° Clockwise',
      'Rotate 90° Counter-Clockwise',
      'Two-Point Rotate / Scale',
      'Align',
      'Align Centers',
      'Align Vertical Centers',
      'Align Horizontal Centers',
      'Align Left',
      'Align Right',
      'Align Bottom',
      'Align Top',
      'Distribute',
      'Distribute V-Spaced',
      'Distribute V-Centered',
      'Distribute H-Spaced',
      'Distribute H-Centered',
      'Move H Together',
      'Move V Together',
      'Nest Selected',
      'Dock',
      'Dock Left',
      'Dock Right',
      'Dock Up',
      'Dock Down',
      'Move Selected Objects',
      'Move to Laser Position',
      'Move to Page Center',
      'Move to Upper Left',
      'Move to Upper Right',
      'Move to Lower Left',
      'Move to Lower Right',
      'Move to Left',
      'Move to Right',
      'Move to Top',
      'Move to Bottom',
      'Move Laser to Selection',
      'Move Laser to Selection Center',
      'Move Laser to Upper Left of Selection',
      'Move Laser to Upper Right of Selection',
      'Move Laser to Lower Left of Selection',
      'Move Laser to Lower Right of Selection',
      'Move Laser to Left of Selection',
      'Move Laser to Right of Selection',
      'Move Laser to Top of Selection',
      'Move Laser to Bottom of Selection',
      'Jog Laser',
      'Jog Laser Left',
      'Jog Laser Right',
      'Jog Laser Up',
      'Jog Laser Down',
      'Grid Array',
      'Circular Array',
      'Copy Along Path',
      'Break Apart',
      'Push in Draw Order',
      'Bring Forward',
      'Send Backward',
      'Bring to Front',
      'Send to Back',
      'Lock Selected Shapes',
      'Unlock Selected Shapes',
    ]);
  });

  it('Tools menu keeps the full Boolean section in order', () => {
    setProjectWithSelection(['txt1', 'path1']);
    render(<MenuBar />);
    fireEvent.click(screen.getByText('Tools'));

    const labels = openedMenuLabels();
    const start = labels.indexOf('Weld Shapes');
    expect(labels.slice(start, start + 6)).toEqual([
      'Weld Shapes',
      'Boolean Union',
      'Boolean Subtract',
      'Boolean Intersection',
      'Boolean Assistant',
      'Cut Shapes',
    ]);
  });

  it('Arrange menu shows copy-along-path and hides shelved rubber-band outline', () => {
    setProjectWithSelection(['txt1', 'path1']);
    render(<MenuBar />);
    fireEvent.click(screen.getByText('Arrange'));
    expect(screen.getByText('Copy Along Path')).toBeDefined();
    expect(screen.queryByText('Create Rubber-Band Outline from Selection')).toBeNull();
  });

  it('Arrange menu keeps supported actions and omits sizing entries', () => {
    setProjectWithSelection(['txt1', 'path1']);
    render(<MenuBar />);
    fireEvent.click(screen.getByText('Arrange'));
    expect(screen.getByText('Mirror Across Line')).toBeDefined();
    expect(screen.queryByText('Make Same Width')).toBeNull();
    expect(screen.queryByText('Make Same Height')).toBeNull();
    expect(screen.queryByText('Resize Slots')).toBeNull();
    expect(screen.getByText('Move H Together')).toBeDefined();
    expect(screen.getByText('Move V Together')).toBeDefined();
    expect(screen.getByText('Dock')).toBeDefined();
    expect(screen.getByText('Move Selected Objects')).toBeDefined();
    expect(screen.getByText('Move Laser to Selection')).toBeDefined();
  });

  it('enables Align and Move Together for one locked anchor plus one unlocked movable object', () => {
    setProjectWithSelection(['txt1', 'path1']);
    useProjectStore.setState((state) => ({
      project: state.project
        ? {
          ...state.project,
          objects: state.project.objects.map((object) => (
            object.id === 'path1' ? { ...object, locked: true } : object
          )),
        }
        : state.project,
    }));

    render(<MenuBar />);
    fireEvent.click(screen.getByText('Arrange'));

    expect(screen.getByText('Align Left').closest('button')?.hasAttribute('disabled')).toBe(false);
    expect(screen.getByText('Move H Together').closest('button')?.hasAttribute('disabled')).toBe(false);
  });

  it('enables Arrange lock and unlock according to the selected lock states', () => {
    setProjectWithSelection(['txt1', 'path1']);
    useProjectStore.setState((state) => ({
      project: state.project
        ? {
          ...state.project,
          objects: state.project.objects.map((object) => (
            object.id === 'path1' ? { ...object, locked: true } : object
          )),
        }
        : state.project,
    }));

    render(<MenuBar />);
    fireEvent.click(screen.getByText('Arrange'));

    expect(screen.getByText('Lock Selected Shapes').closest('button')?.hasAttribute('disabled')).toBe(false);
    expect(screen.getByText('Unlock Selected Shapes').closest('button')?.hasAttribute('disabled')).toBe(false);

    cleanup();
    setProjectWithSelection(['txt1', 'path1']);
    useProjectStore.setState((state) => ({
      project: state.project
        ? {
          ...state.project,
          objects: state.project.objects.map((object) => ({ ...object, locked: true })),
        }
        : state.project,
    }));

    render(<MenuBar />);
    fireEvent.click(screen.getByText('Arrange'));

    expect(screen.getByText('Lock Selected Shapes').closest('button')?.hasAttribute('disabled')).toBe(true);
    expect(screen.getByText('Unlock Selected Shapes').closest('button')?.hasAttribute('disabled')).toBe(false);
  });

  it('disables Close Path for unsupported single-object selections', () => {
    setProjectWithSelection(['img1']);
    render(<MenuBar />);
    fireEvent.click(screen.getByText('Edit'));
    expect(screen.getByRole('button', { name: /Close Path/ }).hasAttribute('disabled')).toBe(true);
  });

  it('disables Break Apart for unsupported single-object selections', () => {
    setProjectWithSelection(['img1']);
    render(<MenuBar />);
    fireEvent.click(screen.getByText('Arrange'));
    expect(screen.getByRole('button', { name: /Break Apart/ }).hasAttribute('disabled')).toBe(true);
  });

  it('Copy Along Path uses the last selected vector as the guide object', async () => {
    const copyAlongPath = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({ copyAlongPath } as never);
    setProjectWithSelection(['txt1', 'path1', 'path2']);

    render(<MenuBar />);
    fireEvent.click(screen.getByText('Arrange'));
    fireEvent.click(screen.getByText('Copy Along Path'));
    fireEvent.change(screen.getByDisplayValue('6'), { target: { value: '4' } });
    fireEvent.click(screen.getByTestId('copy-along-path-submit'));

    await waitFor(() => expect(copyAlongPath).toHaveBeenCalled());
    expect(copyAlongPath).toHaveBeenCalledWith(['txt1', 'path1'], 'path2', {
      count: 4,
      rotateCopies: true,
      scaleCopies: false,
      finalScalePercent: 100,
    });
  });

  it('Help menu shows Report a Bug', () => {
    render(<MenuBar />);
    fireEvent.click(screen.getByText('Help'));
    expect(screen.getByText('Report a Bug...')).toBeDefined();
  });

  it('opens docs from Quick Help', () => {
    const openExternalUrl = vi.spyOn(appService, 'openExternalUrl').mockResolvedValue(undefined);

    render(<MenuBar />);
    fireEvent.click(screen.getByText('Help'));
    fireEvent.click(screen.getByText('Quick Help'));

    expect(openExternalUrl).toHaveBeenCalledWith('https://beambench.com/docs');
  });

  it('captures the launch-time layer for Create Barcode', () => {
    const addObject = vi.fn().mockResolvedValue({ id: 'barcode-1' });
    useProjectStore.setState({ addObject } as never);
    setProjectWithSelection(['txt1']);

    render(<MenuBar />);
    fireEvent.click(screen.getByText('Tools'));
    fireEvent.click(screen.getByText('Create Bar Code'));

    act(() => {
      useProjectStore.setState({ selectedLayerId: 'l2' });
    });

    const dataInput = screen.getByText('Data').closest('label')?.querySelector('input');
    expect(dataInput).toBeTruthy();
    fireEvent.change(dataInput!, { target: { value: 'ABC123' } });
    fireEvent.click(screen.getByTestId('barcode-submit'));

    expect(addObject).toHaveBeenCalledWith(
      expect.stringContaining('Barcode'),
      'l1',
      expect.objectContaining({ type: 'barcode', data: 'ABC123' }),
      expect.any(Object),
    );
  });

  it('captures the launch-time selection for Grid Array', () => {
    const gridArray = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ gridArray } as never);
    setProjectWithSelection(['txt1', 'path1']);

    render(<MenuBar />);
    fireEvent.click(screen.getByText('Arrange'));
    fireEvent.click(screen.getByText('Grid Array'));

    act(() => {
      useProjectStore.setState({ selectedObjectIds: ['path2'] });
    });
    fireEvent.click(screen.getByTestId('grid-array-submit'));

    expect(gridArray).toHaveBeenCalledWith(expect.objectContaining({
      objectIds: ['txt1', 'path1'],
    }));
  });

  it('enables Close & Join for clone-backed vector selections', () => {
    setProjectWithSelection(['clone1', 'path2']);
    render(<MenuBar />);
    fireEvent.click(screen.getByText('Edit'));
    expect(screen.getByRole('button', { name: 'Close & Join' }).hasAttribute('disabled')).toBe(false);
  });

  it('treats cancelled menu exports as a quiet no-op', () => {
    const exportArtwork = vi.spyOn(persistenceService, 'exportArtwork').mockRejectedValue(new Error('Export cancelled'));
    setProjectWithSelection(['txt1']);

    render(<MenuBar />);
    fireEvent.click(screen.getByText('File'));
    fireEvent.click(screen.getByText('Export Selection'));

    expect(exportArtwork).toHaveBeenCalled();
  });

  it('wires File print menu items to print modes', () => {
    const printProject = vi.spyOn(printService, 'printProject').mockResolvedValue(undefined);
    setProjectWithSelection();

    render(<MenuBar />);
    fireEvent.click(screen.getByText('File'));
    fireEvent.click(screen.getByText('Print (black only)'));
    fireEvent.click(screen.getByText('File'));
    fireEvent.click(screen.getByText('Print (keep colors)'));

    expect(printProject).toHaveBeenNthCalledWith(1, 'black');
    expect(printProject).toHaveBeenNthCalledWith(2, 'color');
  });

  it('enables Save Processed Bitmap for a raster selection', () => {
    const saveProcessedBitmap = vi.spyOn(persistenceService, 'saveProcessedBitmap').mockResolvedValue('/tmp/processed.png');
    setProjectWithSelection(['img1']);

    render(<MenuBar />);
    fireEvent.click(screen.getByText('File'));
    const item = screen.getByRole('button', { name: 'Save Processed Bitmap' });
    expect(item.hasAttribute('disabled')).toBe(false);
    fireEvent.click(item);

    expect(saveProcessedBitmap).toHaveBeenCalledWith('img1');
  });

  it('disables Save Processed Bitmap for non-raster selections', () => {
    setProjectWithSelection(['txt1']);

    render(<MenuBar />);
    fireEvent.click(screen.getByText('File'));
    expect(screen.getByRole('button', { name: 'Save Processed Bitmap' }).hasAttribute('disabled')).toBe(true);
  });

  it('shows the expected File submenu structure', () => {
    setProjectWithSelection();
    render(<MenuBar />);
    fireEvent.click(screen.getByText('File'));

    expect(screen.getByText('New Window')).toBeDefined();
    expect(screen.getByRole('button', { name: 'New Window' }).hasAttribute('disabled')).toBe(false);
    expect(screen.getByText('Recent Projects')).toBeDefined();
    expect(screen.getByText('Preferences')).toBeDefined();
    expect(screen.getByText('Import Prefs')).toBeDefined();
    expect(screen.getByText('Export Prefs')).toBeDefined();
    expect(screen.getByText('Reset Prefs to Defaults')).toBeDefined();
    expect(screen.queryByText('Bundles')).toBeNull();
    expect(screen.queryByText('Import Bundles')).toBeNull();
    expect(screen.getByText('Print (black only)')).toBeDefined();
    expect(screen.getByText('Save Background Capture')).toBeDefined();
  });

  it('Edit Settings opens the live preferences dialog', () => {
    render(<MenuBar />);
    fireEvent.click(screen.getByText('Edit'));
    fireEvent.click(screen.getByText('Settings'));
    expect(screen.getByRole('dialog', { name: 'Preferences' })).toBeDefined();
  });

  it('File Preferences export writes a .bbprefs file', async () => {
    vi.mocked(save).mockResolvedValue('/tmp/beam-bench.bbprefs');
    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === 'get_app_settings') return makeAppSettings();
      if (command === 'export_preferences') return '/tmp/beam-bench.bbprefs';
      return null;
    });

    render(<MenuBar />);
    fireEvent.click(screen.getByText('File'));
    fireEvent.click(screen.getByText('Export Prefs'));

    await waitFor(() => {
      expect(save).toHaveBeenCalledWith(expect.objectContaining({
        title: 'Export Preferences',
        defaultPath: 'beam-bench.bbprefs',
      }));
      expect(invoke).toHaveBeenCalledWith('export_preferences', { path: '/tmp/beam-bench.bbprefs' });
    });
  });

  it('File Preferences reset restores backend defaults after confirmation', async () => {
    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === 'get_app_settings') return makeAppSettings();
      if (command === 'reset_preferences') return makeAppSettings({ autosave_enabled: false });
      return null;
    });

    render(<MenuBar />);
    fireEvent.click(screen.getByText('File'));
    fireEvent.click(screen.getByText('Reset Prefs to Defaults'));

    const dialog = screen.getByRole('dialog', { name: 'Reset Prefs to Defaults' });
    expect(dialog).toBeDefined();
    fireEvent.click(screen.getByRole('button', { name: 'OK' }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('reset_preferences');
    });
  });

  it('File Preferences reset cancel closes the dialog without resetting', async () => {
    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === 'get_app_settings') return makeAppSettings();
      return null;
    });

    render(<MenuBar />);
    fireEvent.click(screen.getByText('File'));
    fireEvent.click(screen.getByText('Reset Prefs to Defaults'));

    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));

    await waitFor(() => {
      expect(screen.queryByRole('dialog', { name: 'Reset Prefs to Defaults' })).toBeNull();
    });
    expect(invoke).not.toHaveBeenCalledWith('reset_preferences');
  });

  it('File Preferences hides the shelved hotkey editor', () => {
    render(<MenuBar />);
    fireEvent.click(screen.getByText('File'));
    expect(screen.queryByText('Edit Hotkeys...')).toBeNull();
    expect(screen.queryByRole('dialog', { name: 'Edit Hotkeys' })).toBeNull();
  });

  it('opens File submenus from the keyboard', () => {
    setProjectWithSelection();
    render(<MenuBar />);
    fireEvent.click(screen.getByText('File'));

    const preferences = screen.getByRole('button', { name: /Preferences/ });
    expect(preferences.getAttribute('aria-expanded')).toBe('false');

    fireEvent.keyDown(preferences, { key: 'Enter' });
    expect(preferences.getAttribute('aria-expanded')).toBe('true');

    fireEvent.keyDown(preferences, { key: 'Escape' });
    expect(preferences.getAttribute('aria-expanded')).toBe('false');
  });

  it('lists every supported locale in the Language menu', () => {
    render(<MenuBar />);
    fireEvent.click(screen.getByText('Language'));
    expect(screen.getByText('English')).toBeDefined();
    expect(screen.getByText('Deutsch (German)')).toBeDefined();
    expect(screen.getByText('日本語 (Japanese)')).toBeDefined();
    expect(screen.getByText('简体中文 (Simplified Chinese)')).toBeDefined();
  });

  it('routes Language menu clicks through useAppStore.updateSettings', async () => {
    const { useAppStore } = await import('../../../stores/appStore');
    const updateSettings = vi.fn().mockResolvedValue(undefined);
    useAppStore.setState({ updateSettings });

    render(<MenuBar />);
    fireEvent.click(screen.getByText('Language'));
    fireEvent.click(screen.getByText('Deutsch (German)'));

    expect(updateSettings).toHaveBeenCalledWith({ display_language: 'de' });
  });

  it('opens the report dialog from Help', () => {
    const openHandler = vi.fn();
    window.addEventListener('beam-bench-open-feedback-report', openHandler);

    render(<MenuBar />);
    fireEvent.click(screen.getByText('Help'));
    fireEvent.click(screen.getByText('Report a Bug...'));

    expect(openHandler).toHaveBeenCalled();
    window.removeEventListener('beam-bench-open-feedback-report', openHandler);
  });

  it('renders the Window menu in the shared parity order', () => {
    render(<MenuBar />);
    const windowButton = screen.getByText('Window');
    fireEvent.click(windowButton);
    const menu = windowButton.closest('div.relative')?.querySelector('div.absolute');
    expect(menu).toBeDefined();

    const labels = Array.from(menu?.querySelectorAll('button') ?? [])
      .map((button) => button.querySelector('span')?.textContent?.replace('\u2713', '').trim())
      .filter(Boolean);
    const expectedLabels = WINDOW_MENU_COMMAND_ORDER.map((commandId) => {
      if (commandId === APP_COMMANDS.WINDOW_SIDE_PANELS) return 'Toggle Side Panels';
      return getCommand(commandId)?.label;
    });

    expect(labels).toEqual(expectedLabels);
    expect(menu?.textContent).toContain('View Style:');
    expect(screen.getByText('Toggle Side Panels')).toBeDefined();
    expect(screen.getByText('Wireframe / Smooth')).toBeDefined();
    expect(screen.getByText('Camera Control')).toBeDefined();
    expect(screen.getByText('Shape Properties')).toBeDefined();
    expect(screen.queryByText('Variable Text')).toBeNull();
    expect(screen.queryByText('File List')).toBeNull();
    expect(screen.queryByText('Modes')).toBeNull();
  });

  it('keeps Alt+P canonical by removing the duplicate View Preview Window item', () => {
    render(<MenuBar />);
    fireEvent.click(screen.getByText('View'));

    expect(screen.getByText('Preview')).toBeDefined();
    expect(screen.getByText('Refresh Preview')).toBeDefined();
    expect(screen.queryByText('Preview Window')).toBeNull();
  });

  it('Arrange menu align surfaces backend failures', async () => {
    const pushSpy = vi.fn();
    useNotificationStore.setState({ push: pushSpy } as never);
    setProjectWithSelection(['txt1', 'path1']);
    vi.spyOn(projectService, 'alignObjects').mockRejectedValue(new Error('align failed'));

    render(<MenuBar />);
    fireEvent.click(screen.getByText('Arrange'));
    fireEvent.click(screen.getByText('Align Left'));

    await waitFor(() => {
      expect(pushSpy).toHaveBeenCalledWith(expect.stringContaining('align failed'), 'error');
    });
  });

  it('Arrange menu distribute surfaces backend failures', async () => {
    const pushSpy = vi.fn();
    useNotificationStore.setState({ push: pushSpy } as never);
    setProjectWithSelection(['txt1', 'path1', 'path2']);
    vi.spyOn(projectService, 'distributeObjects').mockRejectedValue(new Error('distribute failed'));

    render(<MenuBar />);
    fireEvent.click(screen.getByText('Arrange'));
    fireEvent.click(screen.getByText('Distribute H-Centered'));

    await waitFor(() => {
      expect(pushSpy).toHaveBeenCalledWith(expect.stringContaining('distribute failed'), 'error');
    });
  });

  it('does not start a job from the menu when preview bootstrap fails', async () => {
    const generatePreview = vi.fn().mockResolvedValue(false);
    const runPreflight = vi.fn().mockResolvedValue({ outcome: 'pass', checks: [] });
    const startJob = vi.fn().mockResolvedValue(undefined);

    setProjectWithSelection(['txt1']);
    setMachineReady();
    useMachineStore.setState({ runPreflight, startJob });
    usePreviewStore.setState({ state: 'idle', generatePreview } as never);

    render(<MenuBar />);
    fireEvent.click(screen.getByText('Machine'));
    fireEvent.click(screen.getByText('Start Job'));

    await waitFor(() => {
      expect(generatePreview).toHaveBeenCalled();
    });
    expect(runPreflight).not.toHaveBeenCalled();
    expect(startJob).not.toHaveBeenCalled();
  });

  it('disables Start Job while preview generation is already running', () => {
    setProjectWithSelection(['txt1']);
    setMachineReady();
    usePreviewStore.setState({ state: 'generating' } as never);

    render(<MenuBar />);
    fireEvent.click(screen.getByText('Machine'));

    expect(screen.getByRole('button', { name: 'Start Job' }).hasAttribute('disabled')).toBe(true);
  });

  it('disables Start Job while preview bootstrap is pending from the menu flow', async () => {
    let resolvePreview!: (value: boolean) => void;
    const generatePreview = vi.fn().mockImplementation(
      () => new Promise<boolean>((resolve) => {
        resolvePreview = resolve;
      }),
    );
    const runPreflight = vi.fn().mockResolvedValue({ outcome: 'pass', checks: [] });
    const startJob = vi.fn().mockResolvedValue(undefined);

    setProjectWithSelection(['txt1']);
    setMachineReady();
    useMachineStore.setState({ runPreflight, startJob });
    usePreviewStore.setState({ state: 'idle', generatePreview } as never);

    render(<MenuBar />);
    fireEvent.click(screen.getByText('Machine'));
    fireEvent.click(screen.getByText('Start Job'));

    await waitFor(() => {
      expect(generatePreview).toHaveBeenCalled();
    });

    fireEvent.click(screen.getByText('Machine'));
    expect(screen.getByRole('button', { name: 'Start Job' }).hasAttribute('disabled')).toBe(true);

    resolvePreview(true);
  });
});
