import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup } from '@testing-library/react';
import { StatusBar } from '../StatusBar';
import { useProjectStore } from '../../../stores/projectStore';
import { useAppStore } from '../../../stores/appStore';
import { useUiStore } from '../../../stores/uiStore';
import { useMeasurementStore } from '../../../stores/measurementStore';
import { makeProject, makeProjectObject } from '../../../test-utils/projectFixtures';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const initialProjectState = useProjectStore.getState();
const initialAppState = useAppStore.getState();
const initialUiState = useUiStore.getState();
const initialMeasurementState = useMeasurementStore.getState();

afterEach(() => {
  cleanup();
  useProjectStore.setState(initialProjectState, true);
  useAppStore.setState(initialAppState, true);
  useUiStore.setState(initialUiState, true);
  useMeasurementStore.setState(initialMeasurementState, true);
});

describe('StatusBar', () => {
  it('shows selection bounds when objects selected', () => {
    // use shared typed fixtures instead of partial `as never` payloads.
    useProjectStore.setState({
      project: makeProject({
        layers: [],
        objects: [makeProjectObject({
          id: 'o1',
          name: 'Obj1',
          bounds: { min: { x: 10, y: 20 }, max: { x: 50, y: 60 } },
        })],
      }),
      selectedObjectIds: ['o1'],
    });

    render(<StatusBar />);

    const boundsEl = screen.getByTestId('selection-bounds');
    expect(boundsEl).toBeDefined();
    expect(boundsEl.textContent).toContain('10.0');
    expect(boundsEl.textContent).toContain('1 obj');
  });

  it('shows cursor coordinates relative to a bottom-left machine origin', () => {
    useProjectStore.setState({
      project: makeProject({
        workspace: { bed_width_mm: 400, bed_height_mm: 300, origin: 'bottom_left' },
        layers: [],
        objects: [],
      }),
      selectedObjectIds: [],
    });
    useUiStore.setState({ cursorWorldPos: { x: 25, y: 300 } });

    render(<StatusBar />);

    expect(screen.getByText(/X:\s*25\.0 mm\s*Y:\s*0\.0 mm/)).toBeDefined();
  });

  it('shows selection bounds relative to a bottom-left machine origin', () => {
    useProjectStore.setState({
      project: makeProject({
        workspace: { bed_width_mm: 400, bed_height_mm: 300, origin: 'bottom_left' },
        layers: [],
        objects: [makeProjectObject({
          id: 'o1',
          bounds: { min: { x: 10, y: 20 }, max: { x: 50, y: 60 } },
        })],
      }),
      selectedObjectIds: ['o1'],
    });

    render(<StatusBar />);

    expect(screen.getByTestId('selection-bounds').textContent)
      .toContain('(10.0, 240.0) to (50.0, 280.0)');
  });

  it('shows no selection bounds when nothing selected', () => {
    useProjectStore.setState({
      project: makeProject({ layers: [], objects: [] }),
      selectedObjectIds: [],
    });

    render(<StatusBar />);

    expect(screen.queryByTestId('selection-bounds')).toBeNull();
  });

  it('distinguishes node segment trim from the standalone trim tool', () => {
    useUiStore.setState({ activeTool: 'node', nodeSubMode: 'trim' });

    render(<StatusBar />);

    expect(screen.getByText('Click a node-edit segment to trim that segment to intersections')).toBeDefined();
  });

  it('shows measurement status instead of selection bounds while measuring', () => {
    useProjectStore.setState({
      project: makeProject({
        layers: [],
        objects: [makeProjectObject({
          id: 'o1',
          bounds: { min: { x: 10, y: 20 }, max: { x: 50, y: 60 } },
        })],
      }),
      selectedObjectIds: ['o1'],
    });
    useUiStore.setState({ activeTool: 'measure' });
    useMeasurementStore.getState().setDrag({
      start: { x: 0, y: 0 },
      end: { x: 30, y: 40 },
      dxMm: 30,
      dyMm: 40,
      lengthMm: 50,
      angleDeg: 53.13,
    });

    render(<StatusBar />);

    expect(screen.getByTestId('measurement-status').textContent).toContain('len: 50.0 mm');
    expect(screen.queryByTestId('selection-bounds')).toBeNull();
  });

  it('shows object area in measurement status while hovering geometry', () => {
    useUiStore.setState({ activeTool: 'measure' });
    useMeasurementStore.getState().setHover({
      objectId: 'rect-1',
      objectMetrics: {
        objectId: 'rect-1',
        objectName: 'Measured Rect',
        nodes: 4,
        lines: 4,
        curves: 0,
        perimeterMm: 60,
        widthMm: 20,
        heightMm: 10,
        center: { x: 10, y: 5 },
        closed: true,
        areaMm2: 200,
      },
      segment: {
        start: { x: 0, y: 0 },
        end: { x: 20, y: 0 },
        dxMm: 20,
        dyMm: 0,
        lengthMm: 20,
        angleDeg: 0,
        segmentIndex: 0,
        t: 0.5,
      },
    });

    render(<StatusBar />);

    const status = screen.getByTestId('measurement-status').textContent;
    expect(status).toContain('area: 200.0 mm^2');
    expect(status).toContain('seg: 20.0 mm');
  });
});
