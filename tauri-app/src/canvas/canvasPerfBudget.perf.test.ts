import { afterAll, afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { CanvasRenderer } from './CanvasRenderer';
import type { RenderParams } from './CanvasRenderer';
import { getCanvasPerfSamples, resetCanvasPerfSamples, type CanvasPerfMetric } from './canvasPerf';
import { hitTestPoint } from './hitTest';
import { resolveCanvasPointerSnap } from './pointerSnap';
import { worldToScreen, type ViewportParams } from './ViewportTransform';
import { DARK_THEME } from './constants';
import { makeLayer, makeProject, makeProjectObject } from '../test-utils/projectFixtures';
import type { Layer, Project, ProjectObject } from '../types/project';
import { resetSceneIndexCachesForTests } from './sceneIndex';
import { resetAlignmentCachesForTests } from './alignment';
import { resetTransformedPath2DProbeForTests } from './drawObjects';

// This harness measures JS-side canvas hot-path work under a synthetic heavy scene.
// It does not measure real browser raster paint cost because the canvas context is mocked.
// Treat it as an algorithmic regression gate, not a full rendering benchmark.

type PerfGlobal = typeof globalThis & {
  __BB_CANVAS_PERF?: boolean;
};

type PerfSummary = {
  min: number;
  median: number;
  p95: number;
  max: number;
};

const PERF_BUDGETS: Record<CanvasPerfMetric, { stat: keyof PerfSummary; maxMs: number }> = {
  'hit-test': { stat: 'p95', maxMs: 16 },
  'snap-resolution': { stat: 'p95', maxMs: 16 },
  // Base-scene render tail latency in jsdom is prone to occasional GC spikes.
  // The median is the more stable steady-state regression signal here.
  'base-render': { stat: 'median', maxMs: 33 },
  'overlay-render': { stat: 'p95', maxMs: 16 },
  'trace-preview-render': { stat: 'p95', maxMs: 16 },
};

// Forty samples keeps CI time low while still catching obvious hot-path regressions.
// If this gate starts flaking, raise the iteration count before loosening budgets.
const PERF_ITERATIONS = 40;
const PERF_WARMUP_ITERATIONS = 8;
const defaultVp: ViewportParams = {
  offset: { x: 200, y: 150 },
  zoom: 100,
  canvasWidth: 1200,
  canvasHeight: 900,
};

globalThis.ResizeObserver = vi.fn().mockImplementation(() => ({
  observe: vi.fn(),
  unobserve: vi.fn(),
  disconnect: vi.fn(),
}));

const mockGetContext = vi.fn();
HTMLCanvasElement.prototype.getContext = mockGetContext;

function createMockCtx(): CanvasRenderingContext2D {
  return {
    save: vi.fn(),
    restore: vi.fn(),
    beginPath: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    stroke: vi.fn(),
    fill: vi.fn(),
    closePath: vi.fn(),
    fillRect: vi.fn(),
    strokeRect: vi.fn(),
    clearRect: vi.fn(),
    rect: vi.fn(),
    clip: vi.fn(),
    arc: vi.fn(),
    arcTo: vi.fn(),
    ellipse: vi.fn(),
    bezierCurveTo: vi.fn(),
    quadraticCurveTo: vi.fn(),
    roundRect: vi.fn(),
    transform: vi.fn(),
    translate: vi.fn(),
    rotate: vi.fn(),
    scale: vi.fn(),
    setLineDash: vi.fn(),
    fillText: vi.fn(),
    strokeText: vi.fn(),
    measureText: vi.fn(() => ({ width: 0 })),
    setTransform: vi.fn(),
    drawImage: vi.fn(),
    getImageData: vi.fn(() => ({ data: new Uint8ClampedArray(4) })),
    strokeStyle: '',
    fillStyle: '',
    lineWidth: 1,
    globalAlpha: 1,
    imageSmoothingEnabled: true,
    lineDashOffset: 0,
    font: '',
    textAlign: '' as CanvasTextAlign,
    textBaseline: '' as CanvasTextBaseline,
    canvas: document.createElement('canvas'),
  } as unknown as CanvasRenderingContext2D;
}

function buildHeavyPath(commandCount = 6000): string {
  return `M 0 0 ${Array.from(
    { length: commandCount },
    (_, index) => `L ${index + 1} ${(index * 7) % 31}`,
  ).join(' ')}`;
}

function buildHeavyScene(count = 24): { layer: Layer; objects: ProjectObject[]; project: Project } {
  // Start with the repeated-copy heavy-vector case. Additional fixtures such as
  // mixed raster/vector or imported real-world SVG scenes can extend this over time.
  const layer = makeLayer({
    id: 'layer1',
    name: 'Layer 1',
    operation: 'line',
    enabled: true,
    visible: true,
  });
  const heavyPath = buildHeavyPath();
  const objects = Array.from({ length: count }, (_, index) => {
    const column = index % 6;
    const row = Math.floor(index / 6);
    const x = 20 + column * 45;
    const y = 20 + row * 45;
    return makeProjectObject({
      id: `heavy-${index}`,
      name: `Heavy ${index}`,
      layer_id: layer.id,
      z_index: index,
      bounds: { min: { x, y }, max: { x: x + 24, y: y + 24 } },
      data: { type: 'vector_path', path_data: heavyPath, closed: false },
    });
  });

  return {
    layer,
    objects,
    project: makeProject({
      workspace: { bed_width_mm: 400, bed_height_mm: 300, origin: 'top_left' },
      layers: [layer],
      objects,
    }),
  };
}

function getPerfGlobal(): PerfGlobal {
  return globalThis as PerfGlobal;
}

function disablePerfCollection(): void {
  const perfGlobal = getPerfGlobal();
  delete perfGlobal.__BB_CANVAS_PERF;
  resetCanvasPerfSamples();
}

function summarizeSamples(samples: number[]): PerfSummary {
  const sorted = [...samples].sort((a, b) => a - b);
  const pick = (fraction: number): number => {
    const index = Math.min(sorted.length - 1, Math.max(0, Math.ceil(sorted.length * fraction) - 1));
    return sorted[index]!;
  };
  return {
    min: sorted[0]!,
    median: pick(0.5),
    p95: pick(0.95),
    max: sorted[sorted.length - 1]!,
  };
}

function collectPerfSamples(
  metric: CanvasPerfMetric,
  op: () => void,
  iterations = PERF_ITERATIONS,
  warmupIterations = PERF_WARMUP_ITERATIONS,
): PerfSummary {
  disablePerfCollection();
  for (let index = 0; index < warmupIterations; index += 1) {
    op();
  }

  getPerfGlobal().__BB_CANVAS_PERF = true;
  resetCanvasPerfSamples();
  for (let index = 0; index < iterations; index += 1) {
    op();
  }

  const samples = getCanvasPerfSamples()[metric] ?? [];
  disablePerfCollection();

  expect(samples.length).toBe(iterations);
  return summarizeSamples(samples);
}

function expectWithinBudget(metric: CanvasPerfMetric, summary: PerfSummary): void {
  const budget = PERF_BUDGETS[metric];
  const measured = summary[budget.stat];
  expect(
    measured,
    `${metric} p95=${summary.p95.toFixed(2)}ms median=${summary.median.toFixed(2)}ms max=${summary.max.toFixed(2)}ms`,
  ).toBeLessThanOrEqual(budget.maxMs);
}

describe.sequential('canvas performance budgets', () => {
  const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});

  beforeEach(() => {
    mockGetContext.mockImplementation(() => createMockCtx());
    resetSceneIndexCachesForTests();
    resetAlignmentCachesForTests();
    resetTransformedPath2DProbeForTests();
    disablePerfCollection();
  });

  afterEach(() => {
    disablePerfCollection();
    warnSpy.mockClear();
  });

  afterAll(() => {
    warnSpy.mockRestore();
  });

  it('keeps the heavy-scene hot paths within the current budgets', () => {
    const { layer, objects, project } = buildHeavyScene();
    const target = objects[objects.length - 1]!;
    const targetCenter = {
      x: (target.bounds.min.x + target.bounds.max.x) / 2,
      y: (target.bounds.min.y + target.bounds.max.y) / 2,
    };
    const targetScreen = worldToScreen(targetCenter, defaultVp);
    const snapWorld = {
      x: target.bounds.min.x - 0.8,
      y: target.bounds.min.y - 0.8,
    };
    const renderCtx = createMockCtx();
    const renderer = new CanvasRenderer(renderCtx);
    const baseParams: RenderParams = {
      workspace: project.workspace,
      objects,
      layers: [layer],
      selectedObjectIds: [target.id],
      vp: defaultVp,
      gridVisible: false,
      gridSpacingMm: 10,
      toolOverlay: { type: 'none' },
      theme: DARK_THEME,
      interactionState: { active: false, kind: 'none' },
    };
    let overlayDashOffset = 0;
    let hitResultId: string | undefined;
    let snapTargetKey: string | null = null;

    const hitTestSummary = collectPerfSamples('hit-test', () => {
      hitResultId = hitTestPoint(targetScreen, objects, defaultVp)?.id;
    });
    const snapSummary = collectPerfSamples('snap-resolution', () => {
      const result = resolveCanvasPointerSnap({
        world: snapWorld,
        ctrlKey: false,
        altKey: false,
        project,
        zoom: defaultVp.zoom,
        snapEnabled: false,
        gridVisible: false,
        effectiveSnapSpacing: 10,
        snapToObjects: true,
        snapThresholdPx: 10,
      });
      snapTargetKey = result.nextPreferredTargetKey;
    });
    const baseRenderSummary = collectPerfSamples('base-render', () => {
      renderer.renderBaseScene(baseParams);
    });
    const overlayRenderSummary = collectPerfSamples('overlay-render', () => {
      overlayDashOffset = (overlayDashOffset + 1) % 16;
      renderer.renderToolOverlay({
        ...baseParams,
        selectionDashOffset: overlayDashOffset,
      });
    });

    expect(hitResultId).toBe(target.id);
    expect(snapTargetKey).not.toBeNull();
    expectWithinBudget('hit-test', hitTestSummary);
    expectWithinBudget('snap-resolution', snapSummary);
    expectWithinBudget('base-render', baseRenderSummary);
    expectWithinBudget('overlay-render', overlayRenderSummary);
  });
});
