import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, cleanup, fireEvent, waitFor, act } from '@testing-library/react';
import { ImportDropZone } from '../ImportDropZone';
import { useProjectStore } from '../../../stores/projectStore';
import { useArtLibraryStore } from '../../../stores/artLibraryStore';
import { makeLayer, makeProject } from '../../../test-utils/projectFixtures';

const tauriDragDrop = vi.hoisted(() => ({
  handler: null as null | ((event: { payload: { type: string; paths?: string[] } }) => void | Promise<void>),
}));

vi.mock('@tauri-apps/api/webviewWindow', () => ({
  getCurrentWebviewWindow: () => ({
    onDragDropEvent: vi.fn(async (handler: typeof tauriDragDrop.handler) => {
      tauriDragDrop.handler = handler;
      return () => {
        tauriDragDrop.handler = null;
      };
    }),
  }),
}));

// Mock importService used by the store's import actions
const mockImportFilePaths = vi.fn().mockResolvedValue([]);
const mockImportFileData = vi.fn().mockResolvedValue([]);
const mockPickFiles = vi.fn().mockResolvedValue([]);
vi.mock('../../../services/importService', () => ({
  importService: {
    pickFiles: (...args: unknown[]) => mockPickFiles(...args),
    importFilePaths: (...args: unknown[]) => mockImportFilePaths(...args),
    importFileData: (...args: unknown[]) => mockImportFileData(...args),
    importGcodeFile: vi.fn(),
  },
}));

/** Matcher for the content-based payload HTML5 drops produce. */
const dataFiles = (names: string[]) =>
  names.map((filename) => expect.objectContaining({ filename, dataBase64: expect.any(String) }));

vi.mock('../../../services/projectService', () => ({
  projectService: {
    addLayer: vi.fn(),
    updateLayer: vi.fn(),
    updateCutEntry: vi.fn(),
    getProject: vi.fn().mockResolvedValue(null),
  },
}));

import { projectService } from '../../../services/projectService';
const mockedProjectService = projectService as unknown as Record<string, ReturnType<typeof vi.fn>>;

const initialState = useProjectStore.getState();
const initialArtLibraryState = useArtLibraryStore.getState();

function makeResolvedLayer(
  id: string,
  name: string,
  operation: 'line' | 'image',
  colorTag: string,
) {
  return makeLayer({
    id,
    name,
    operation,
    color_tag: colorTag,
    order_index: 1,
    power_percent: 100,
    speed_mm_min: 1000,
  });
}

function mockCreatedLayer(layer: ReturnType<typeof makeResolvedLayer>) {
  mockedProjectService.addLayer.mockResolvedValue(layer);
  mockedProjectService.updateLayer.mockResolvedValue(layer);
  mockedProjectService.updateCutEntry.mockResolvedValue(layer.entries[0]);
}

afterEach(() => {
  cleanup();
  useProjectStore.setState(initialState, true);
  useArtLibraryStore.setState(initialArtLibraryState, true);
  tauriDragDrop.handler = null;
  mockImportFilePaths.mockClear();
  mockImportFileData.mockClear();
  mockPickFiles.mockReset();
  mockPickFiles.mockResolvedValue([]);
  vi.unstubAllGlobals();
  mockedProjectService.addLayer.mockClear();
  mockedProjectService.updateLayer.mockClear();
  mockedProjectService.updateCutEntry.mockClear();
});

function setProject() {
  useProjectStore.setState({
    project: makeProject({
      metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p1', project_name: 'Test', created_at: '', modified_at: '' },
      layers: [makeLayer({
        id: 'l1',
        name: 'Layer 1',
        operation: 'cut',
        color_tag: '#ff0000',
        power_percent: 100,
      })],
      objects: [],
      assets: [],
    }),
    selectedLayerId: 'l1',
  });
}

function stubUnreadableFileReader() {
  class UnreadableFileReader {
    error = new DOMException('The requested file could not be read.', 'NotReadableError');
    result: string | ArrayBuffer | null = null;
    onerror: ((event: Event) => void) | null = null;
    onload: ((event: Event) => void) | null = null;

    readAsDataURL() {
      queueMicrotask(() => this.onerror?.(new Event('error')));
    }
  }

  vi.stubGlobal('FileReader', UnreadableFileReader);
}

async function emitNativeDragDropEvent(payload: { type: string; paths?: string[] }) {
  if (!tauriDragDrop.handler) {
    throw new Error('Native drag-drop handler was not registered');
  }
  await act(async () => {
    await tauriDragDrop.handler?.({ payload });
  });
}

describe('ImportDropZone', () => {
  it('imports supported file paths from Tauri native drag-drop events', async () => {
    setProject();
    mockImportFilePaths.mockResolvedValue([{ id: 'obj1' }]);
    mockedProjectService.getProject.mockResolvedValue(useProjectStore.getState().project);

    render(
      <ImportDropZone>
        <div>content</div>
      </ImportDropZone>,
    );

    await emitNativeDragDropEvent({
      type: 'drop',
      paths: ['/tmp/drawing.dxf'],
    });

    await waitFor(() => {
      expect(mockImportFilePaths).toHaveBeenCalledWith(
        ['/tmp/drawing.dxf'],
        'l1',
      );
    });
  });

  it('imports dropped file contents for supported formats including DXF', async () => {
    setProject();
    mockImportFileData.mockResolvedValue([{ id: 'obj1' }]);
    mockedProjectService.getProject.mockResolvedValue(useProjectStore.getState().project);

    const { container } = render(
      <ImportDropZone>
        <div>content</div>
      </ImportDropZone>,
    );

    const zone = container.firstChild as HTMLElement;

    const dxfFile = new File(['0\nSECTION'], 'drawing.dxf');

    fireEvent.drop(zone, {
      dataTransfer: {
        files: [dxfFile],
        items: [],
        types: ['Files'],
      },
    });

    await waitFor(() => {
      expect(mockImportFileData).toHaveBeenCalledWith(dataFiles(['drawing.dxf']), 'l1');
    });
  });

  it('recovers an unreadable Windows drop through the native file picker', async () => {
    setProject();
    stubUnreadableFileReader();
    mockPickFiles.mockResolvedValueOnce(['C:\\Users\\maker\\drawing.dxf']);
    mockImportFilePaths.mockResolvedValueOnce([{ id: 'obj1', layer_id: 'l1' }]);
    mockedProjectService.getProject.mockResolvedValue(useProjectStore.getState().project);

    const { container } = render(
      <ImportDropZone>
        <div>content</div>
      </ImportDropZone>,
    );

    const zone = container.firstChild as HTMLElement;
    fireEvent.drop(zone, {
      dataTransfer: {
        files: [new File(['0\nSECTION'], 'drawing.dxf')],
        items: [],
        types: ['Files'],
      },
    });

    await waitFor(() => {
      expect(mockPickFiles).toHaveBeenCalledOnce();
      expect(mockImportFilePaths).toHaveBeenCalledWith(['C:\\Users\\maker\\drawing.dxf'], 'l1');
    });
    expect(mockImportFileData).not.toHaveBeenCalled();
  });

  it('allows the native recovery picker to be cancelled', async () => {
    setProject();
    stubUnreadableFileReader();
    mockPickFiles.mockResolvedValueOnce([]);

    const { container } = render(
      <ImportDropZone>
        <div>content</div>
      </ImportDropZone>,
    );

    const zone = container.firstChild as HTMLElement;
    fireEvent.drop(zone, {
      dataTransfer: {
        files: [new File(['x'], 'photo.png')],
        items: [],
        types: ['Files'],
      },
    });

    await waitFor(() => {
      expect(mockPickFiles).toHaveBeenCalledOnce();
    });
    expect(mockImportFilePaths).not.toHaveBeenCalled();
    expect(mockImportFileData).not.toHaveBeenCalled();
  });

  it('imports Lbrn projects dropped from the desktop', async () => {
    setProject();
    mockImportFileData.mockResolvedValue([{ id: 'obj1', layer_id: 'l1' }]);
    mockedProjectService.getProject.mockResolvedValue(useProjectStore.getState().project);

    const { container } = render(
      <ImportDropZone>
        <div>content</div>
      </ImportDropZone>,
    );

    const zone = container.firstChild as HTMLElement;
    const lbrnFile = new File(['<LbrnProject FormatVersion="1"/>'], 'project.lbrn2');

    fireEvent.drop(zone, {
      dataTransfer: { files: [lbrnFile], items: [], types: ['Files'] },
    });

    await waitFor(() => {
      expect(mockImportFileData).toHaveBeenCalledWith(dataFiles(['project.lbrn2']), 'l1');
      expect(useProjectStore.getState().selectedObjectIds).toEqual(['obj1']);
    });
  });

  it('does not intercept in-app art library drags as file imports', async () => {
    setProject();
    useArtLibraryStore.setState((state) => ({
      ...state,
      dragState: {
        sourceLibraryId: 'library-1',
        itemId: 'item-1',
        dropEffect: 'copy',
        dropAllowed: true,
        targetLibraryId: null,
      },
    }));

    const { container, queryByText } = render(
      <ImportDropZone>
        <div>content</div>
      </ImportDropZone>,
    );
    const zone = container.firstChild as HTMLElement;
    const svgFile = new File([''], 'shape.svg');
    Object.defineProperty(svgFile, 'path', { value: '/tmp/shape.svg' });

    fireEvent.dragEnter(zone, {
      dataTransfer: { files: [svgFile], items: [], types: ['Files'] },
    });
    fireEvent.drop(zone, {
      dataTransfer: { files: [svgFile], items: [], types: ['Files'] },
    });

    await waitFor(() => {
      expect(mockImportFilePaths).not.toHaveBeenCalled();
      expect(mockImportFileData).not.toHaveBeenCalled();
    });
    expect(queryByText('Drop files to import')).toBeNull();
  });

  it('does not intercept art library drags armed after the drop zone rendered', async () => {
    setProject();

    const { container, queryByText } = render(
      <ImportDropZone>
        <div>content</div>
      </ImportDropZone>,
    );
    const zone = container.firstChild as HTMLElement;
    const svgFile = new File([''], 'shape.svg');
    Object.defineProperty(svgFile, 'path', { value: '/tmp/shape.svg' });

    useArtLibraryStore.setState((state) => ({
      ...state,
      dragState: {
        sourceLibraryId: 'library-1',
        itemId: 'item-1',
        dropEffect: 'copy',
        dropAllowed: true,
        targetLibraryId: null,
      },
    }));

    fireEvent.dragEnter(zone, {
      dataTransfer: { files: [svgFile], items: [], types: ['Files'] },
    });
    fireEvent.drop(zone, {
      dataTransfer: { files: [svgFile], items: [], types: ['Files'] },
    });

    await waitFor(() => {
      expect(mockImportFilePaths).not.toHaveBeenCalled();
      expect(mockImportFileData).not.toHaveBeenCalled();
    });
    expect(queryByText('Drop files to import')).toBeNull();
  });

  it('ignores G-code extensions in drag-drop import', async () => {
    setProject();
    mockedProjectService.getProject.mockResolvedValue(useProjectStore.getState().project);

    const { container } = render(
      <ImportDropZone>
        <div>content</div>
      </ImportDropZone>,
    );

    const zone = container.firstChild as HTMLElement;
    const gcodeFile = new File([''], 'toolpath.nc');
    Object.defineProperty(gcodeFile, 'path', { value: '/tmp/toolpath.nc' });

    fireEvent.drop(zone, {
      dataTransfer: {
        files: [gcodeFile],
        items: [],
        types: ['Files'],
      },
    });

    await waitFor(() => {
      expect(mockImportFilePaths).not.toHaveBeenCalled();
      expect(mockImportFileData).not.toHaveBeenCalled();
    });
  });

  it('batches multiple supported files into one importFilePaths call', async () => {
    setProject(); // active layer l1 is cut, color #ff0000
    // Mixed batch contains rasters → layer-family resolver creates
    // an image sibling in the l1 color family first, then hands
    // the whole batch to the backend with the image sibling id.
    // (The backend still per-object auto-routes vectors to a
    // non-image sibling, but the frontend seeds with the raster
    // destination so post-import selection makes sense.)
    mockCreatedLayer(makeResolvedLayer('l-img', 'Layer 1 (Image)', 'image', '#ff0000'));
    mockedProjectService.getProject.mockResolvedValue({
      ...useProjectStore.getState().project!,
      layers: [
        ...useProjectStore.getState().project!.layers,
        makeResolvedLayer('l-img', 'Layer 1 (Image)', 'image', '#ff0000'),
      ],
    });
    mockImportFileData.mockResolvedValue([{ id: 'obj1', layer_id: 'l-img' }]);

    const { container } = render(
      <ImportDropZone>
        <div>content</div>
      </ImportDropZone>,
    );

    const zone = container.firstChild as HTMLElement;

    const files = ['a.svg', 'b.png', 'c.bmp', 'd.pdf'].map((name) => new File(['x'], name));

    fireEvent.drop(zone, {
      dataTransfer: { files, items: [], types: ['Files'] },
    });

    await waitFor(() => {
      // Resolver sees raster in batch, no image sibling for the
      // 'l1' color family, so it creates one (inheriting the name
      // via the "Layer 1 (Image)" suggestion).
      expect(mockedProjectService.addLayer).toHaveBeenCalledWith('Layer 1 (Image)', 'image');
      // The batch is handed off using the new image sibling id —
      // the backend splits vectors to their own sibling via
      // per-object routing.
      expect(mockImportFileData).toHaveBeenCalledWith(
        dataFiles(['a.svg', 'b.png', 'c.bmp', 'd.pdf']),
        'l-img',
      );
    });
  });

  it('routes a single raster drop through the family resolver to an image sibling', async () => {
    setProject(); // active layer l1 is cut, color #ff0000
    mockCreatedLayer(makeResolvedLayer('l-img', 'Layer 1 (Image)', 'image', '#ff0000'));
    mockedProjectService.getProject.mockResolvedValue({
      ...useProjectStore.getState().project!,
      layers: [
        ...useProjectStore.getState().project!.layers,
        makeResolvedLayer('l-img', 'Layer 1 (Image)', 'image', '#ff0000'),
      ],
    });
    mockImportFileData.mockResolvedValue([{ id: 'obj1', layer_id: 'l-img' }]);

    const { container } = render(
      <ImportDropZone>
        <div>content</div>
      </ImportDropZone>,
    );

    const zone = container.firstChild as HTMLElement;
    const pngFile = new File(['x'], 'photo.png');

    fireEvent.drop(zone, {
      dataTransfer: { files: [pngFile], items: [], types: ['Files'] },
    });

    await waitFor(() => {
      // Resolver creates the image sibling, then hands the import
      // to it instead of the caller's l1 (cut) layer.
      expect(mockedProjectService.addLayer).toHaveBeenCalledWith('Layer 1 (Image)', 'image');
      expect(mockImportFileData).toHaveBeenCalledWith(dataFiles(['photo.png']), 'l-img');
    });

    // Final selection matches the layer id returned by the backend
    // for the imported object.
    await waitFor(() => {
      expect(useProjectStore.getState().selectedLayerId).toBe('l-img');
    });
  });

  it('ignores unsupported file extensions', async () => {
    setProject();
    const { container } = render(
      <ImportDropZone>
        <div>content</div>
      </ImportDropZone>,
    );

    const zone = container.firstChild as HTMLElement;

    const file = new File([''], 'readme.txt');
    Object.defineProperty(file, 'path', { value: '/tmp/readme.txt' });

    fireEvent.drop(zone, {
      dataTransfer: { files: [file], items: [], types: ['Files'] },
    });

    // Give time for async handler
    await new Promise((r) => setTimeout(r, 50));

    expect(mockImportFilePaths).not.toHaveBeenCalled();
    expect(mockImportFileData).not.toHaveBeenCalled();
  });

  it('routes raster import to new image layer when pending palette matches non-image layer', async () => {
    setProject();
    // Pending palette matches existing cut layer's color
    useProjectStore.setState({ pendingPaletteColor: '#ff0000' });

    mockCreatedLayer(makeResolvedLayer('l-img', 'Layer 1 (Image)', 'image', '#ff0000'));
    mockedProjectService.getProject.mockResolvedValue({
      ...useProjectStore.getState().project!,
      layers: [
        ...useProjectStore.getState().project!.layers,
        makeResolvedLayer('l-img', 'Layer 1 (Image)', 'image', '#ff0000'),
      ],
    });
    mockImportFileData.mockResolvedValue([{ id: 'obj1', layer_id: 'l-img' }]);

    const { container } = render(
      <ImportDropZone>
        <div>content</div>
      </ImportDropZone>,
    );

    const zone = container.firstChild as HTMLElement;
    const pngFile = new File(['x'], 'photo.png');

    fireEvent.drop(zone, {
      dataTransfer: { files: [pngFile], items: [], types: ['Files'] },
    });

    await waitFor(() => {
      // Raster + pending color matching an existing non-image layer →
      // family resolver creates an image sibling with the layer name
      // inherited from the existing family member ("Layer 1 (Image)").
      expect(mockedProjectService.addLayer).toHaveBeenCalledWith('Layer 1 (Image)', 'image');
      // updateLayer receives the canonicalised lowercase color_tag
      // (the resolver normalizes before returning).
      expect(mockedProjectService.updateLayer).toHaveBeenCalledWith(
        'l-img',
        expect.objectContaining({ color_tag: '#ff0000' }),
      );
      // Raster should route to the newly created image layer, not the
      // existing cut layer
      expect(mockImportFileData).toHaveBeenCalledWith(dataFiles(['photo.png']), 'l-img');
    });

    expect(useProjectStore.getState().pendingPaletteColor).toBeNull();
  });

  it('resolves pending palette color on drag-drop import', async () => {
    setProject();
    useProjectStore.setState({ pendingPaletteColor: '#0000FF' });

    mockCreatedLayer(makeResolvedLayer('l-new', 'C03 (Line)', 'line', '#0000ff'));
    mockedProjectService.getProject.mockResolvedValue({
      ...useProjectStore.getState().project!,
      layers: [
        ...useProjectStore.getState().project!.layers,
        makeResolvedLayer('l-new', 'C03 (Line)', 'line', '#0000ff'),
      ],
    });
    mockImportFileData.mockResolvedValue([{ id: 'obj1', layer_id: 'l-new' }]);

    const { container } = render(
      <ImportDropZone>
        <div>content</div>
      </ImportDropZone>,
    );

    const zone = container.firstChild as HTMLElement;
    const svgFile = new File(['<svg/>'], 'shape.svg');

    fireEvent.drop(zone, {
      dataTransfer: { files: [svgFile], items: [], types: ['Files'] },
    });

    await waitFor(() => {
      // Vector import + pending color with no existing family →
      // resolver requests a same-color sibling named from the
      // palette family label rather than the generic operation.
      expect(mockedProjectService.addLayer).toHaveBeenCalledWith('C03 (Line)', 'line');
      // color_tag normalized to lowercase
      expect(mockedProjectService.updateLayer).toHaveBeenCalledWith(
        'l-new',
        expect.objectContaining({ color_tag: '#0000ff' }),
      );
      expect(mockImportFileData).toHaveBeenCalledWith(dataFiles(['shape.svg']), 'l-new');
    });

    // Pending color should be cleared
    expect(useProjectStore.getState().pendingPaletteColor).toBeNull();
  });
});
