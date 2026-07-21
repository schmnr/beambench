export type CanvasPerfMetric =
  | 'hit-test'
  | 'snap-resolution'
  | 'base-render'
  | 'overlay-render'
  | 'trace-preview-render';

type CanvasPerfGlobal = typeof globalThis & {
  __BB_CANVAS_PERF?: boolean;
  __BB_CANVAS_PERF_SAMPLES?: Partial<Record<CanvasPerfMetric, number[]>>;
};

const MAX_SAMPLES_PER_METRIC = 200;

function getPerfGlobal(): CanvasPerfGlobal {
  return globalThis as CanvasPerfGlobal;
}

function now(): number {
  return typeof performance !== 'undefined' ? performance.now() : Date.now();
}

export function isCanvasPerfEnabled(): boolean {
  return getPerfGlobal().__BB_CANVAS_PERF === true;
}

export function resetCanvasPerfSamples(): void {
  delete getPerfGlobal().__BB_CANVAS_PERF_SAMPLES;
}

export function getCanvasPerfSamples(): Partial<Record<CanvasPerfMetric, number[]>> {
  return getPerfGlobal().__BB_CANVAS_PERF_SAMPLES ?? {};
}

export function recordCanvasPerfSample(metric: CanvasPerfMetric, durationMs: number): void {
  if (!isCanvasPerfEnabled()) return;
  const perfGlobal = getPerfGlobal();
  const store = perfGlobal.__BB_CANVAS_PERF_SAMPLES ?? {};
  const samples = store[metric] ?? [];
  samples.push(durationMs);
  if (samples.length > MAX_SAMPLES_PER_METRIC) {
    samples.splice(0, samples.length - MAX_SAMPLES_PER_METRIC);
  }
  store[metric] = samples;
  perfGlobal.__BB_CANVAS_PERF_SAMPLES = store;
}

export function measureCanvasPerf<T>(metric: CanvasPerfMetric, fn: () => T): T {
  if (!isCanvasPerfEnabled()) {
    return fn();
  }
  const start = now();
  try {
    return fn();
  } finally {
    recordCanvasPerfSample(metric, now() - start);
  }
}
