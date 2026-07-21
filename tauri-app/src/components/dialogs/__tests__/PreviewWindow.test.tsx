import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { PreviewWindow } from '../PreviewWindow';
import { makeLayer, makeWorkspace } from '../../../test-utils/projectFixtures';
import type { PreviewData } from '../../../types/preview';

vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

class MockResizeObserver {
  observe = vi.fn();
  unobserve = vi.fn();
  disconnect = vi.fn();
}

globalThis.ResizeObserver = MockResizeObserver as unknown as typeof ResizeObserver;

HTMLCanvasElement.prototype.getContext = vi.fn().mockReturnValue({
  clearRect: vi.fn(),
  fillRect: vi.fn(),
  strokeRect: vi.fn(),
  drawImage: vi.fn(),
  beginPath: vi.fn(),
  moveTo: vi.fn(),
  lineTo: vi.fn(),
  stroke: vi.fn(),
  fill: vi.fn(),
  save: vi.fn(),
  restore: vi.fn(),
  translate: vi.fn(),
  scale: vi.fn(),
  rotate: vi.fn(),
  setLineDash: vi.fn(),
  setTransform: vi.fn(),
  clip: vi.fn(),
  rect: vi.fn(),
  fillStyle: '',
  strokeStyle: '',
  lineWidth: 1,
}) as never;

// return the full typed PreviewData so schema drift surfaces at compile time.
function buildPreviewData(
  planId: string,
  durationSecs: number,
  overrides: Partial<PreviewData> = {},
): PreviewData {
  return {
    plan_id: planId,
    revision_hash: `${planId}-rev`,
    bounds: { min: { x: 0, y: 0 }, max: { x: durationSecs, y: 10 } },
    layers: [{
      layer_id: 'layer-1',
      vector_paths: [{
        points: [{ x: 0, y: 0 }, { x: durationSecs, y: 0 }],
        closed: false,
        power_percent: 50,
        speed_mm_min: 60,
        sequence: 1,
      }],
      raster_regions: [],
    }],
    travel_moves: [],
    frame: null,
    stats: {
      total_distance_mm: durationSecs,
      travel_distance_mm: 0,
      burn_distance_mm: durationSecs,
      estimated_duration_secs: durationSecs,
      segment_count: 1,
      raster_line_count: 0,
    },
    warnings: [],
    failed_entries: [],
    ...overrides,
  };
}

describe('PreviewWindow', () => {
  beforeEach(() => {
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation(() => 1);
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => undefined);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it('opens new preview data at the completed state', () => {
    const onClose = vi.fn();
    // use typed builders. `layers` needs the full Layer shape;
    // `workspace` needs the Workspace shape. PreviewWindow only reads a handful
    // of layer fields but typechecks against the full `Layer[]` interface.
    const layers = [makeLayer({ id: 'layer-1', operation: 'cut', color_tag: '#ff0000' })];
    const workspace = makeWorkspace();

    const { rerender } = render(
      <PreviewWindow
        data={buildPreviewData('plan-1', 10)}
        previewState="current"
        layers={layers}
        workspace={workspace}
        onClose={onClose}
      />,
    );

    expect(screen.getByTestId('preview-window-drag-handle')).toBeDefined();
    expect(screen.getByTestId('preview-window-resize-handle')).toBeDefined();
    expect(screen.getByText('Playback: 0:10 / 0:10')).toBeDefined();

    fireEvent.click(screen.getByTitle('Play'));
    expect(screen.getByTitle('Pause')).toBeDefined();

    rerender(
      <PreviewWindow
        data={buildPreviewData('plan-2', 20)}
        previewState="current"
        layers={layers}
        workspace={workspace}
        onClose={onClose}
      />,
    );

    expect(screen.getByText('Playback: 0:20 / 0:20')).toBeDefined();
    expect(screen.getByTitle('Play')).toBeDefined();
  });

  it('does not re-jump to the end when preview options change duration', () => {
    const onClose = vi.fn();
    const layers = [makeLayer({ id: 'layer-1', operation: 'cut', color_tag: '#ff0000' })];
    const workspace = makeWorkspace();
    const data = buildPreviewData('plan-with-travel', 10, {
      travel_moves: [{
        from: { x: 0, y: 0 },
        to: { x: 10000, y: 0 },
        sequence: 0,
      }],
      layers: [{
        layer_id: 'layer-1',
        vector_paths: [{
          points: [{ x: 0, y: 0 }, { x: 10, y: 0 }],
          closed: false,
          power_percent: 50,
          speed_mm_min: 60,
          sequence: 1,
        }],
        raster_regions: [],
      }],
    });

    render(
      <PreviewWindow
        data={data}
        previewState="current"
        layers={layers}
        workspace={workspace}
        onClose={onClose}
      />,
    );

    expect(screen.getByText('Playback: 1:10 / 1:10')).toBeDefined();
    fireEvent.click(screen.getByTitle('Go to start'));
    expect(screen.getByText('Playback: 0:00 / 1:10')).toBeDefined();

    fireEvent.click(screen.getByLabelText('Show Travel'));

    expect(screen.getByText('Playback: 0:00 / 0:10')).toBeDefined();
  });

  it('suppresses the local generating overlay when the global offset-fill dialog is visible', () => {
    const onClose = vi.fn();
    const layers = [makeLayer({ id: 'layer-1', operation: 'cut', color_tag: '#ff0000' })];
    const workspace = makeWorkspace();

    const { rerender } = render(
      <PreviewWindow
        data={null}
        previewState="generating"
        layers={layers}
        workspace={workspace}
        onClose={onClose}
      />,
    );

    expect(screen.getByText('Generating preview...')).toBeDefined();

    rerender(
      <PreviewWindow
        data={null}
        previewState="generating"
        previewGenerationDialogVisible
        layers={layers}
        workspace={workspace}
        onClose={onClose}
      />,
    );

    expect(screen.queryByText('Generating preview...')).toBeNull();
  });
});
