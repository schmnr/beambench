import { afterEach, describe, expect, it, vi } from 'vitest';
import { measureAsyncPerf } from './perfMarks';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe('measureAsyncPerf', () => {
  it('uses distinct marks for overlapping calls with the same logical name', async () => {
    const marks = new Set<string>();
    const measuredRanges: Array<{ name: string; startMark: string; endMark: string }> = [];
    vi.spyOn(performance, 'mark').mockImplementation((name) => {
      marks.add(String(name));
      return {} as PerformanceMark;
    });
    vi.spyOn(performance, 'measure').mockImplementation((name, startMark, endMark) => {
      if (!marks.has(String(startMark)) || !marks.has(String(endMark))) {
        throw new Error(`missing mark for ${String(name)}`);
      }
      measuredRanges.push({
        name: String(name),
        startMark: String(startMark),
        endMark: String(endMark),
      });
      return {} as PerformanceMeasure;
    });
    vi.spyOn(performance, 'clearMarks').mockImplementation((name) => {
      marks.delete(String(name));
    });
    vi.spyOn(performance, 'clearMeasures').mockImplementation(() => {});

    const first = deferred<string>();
    const second = deferred<string>();
    const firstResult = measureAsyncPerf('trace_image', () => first.promise);
    const secondResult = measureAsyncPerf('trace_image', () => second.promise);

    first.resolve('first');
    await expect(firstResult).resolves.toBe('first');
    second.resolve('second');
    await expect(secondResult).resolves.toBe('second');

    expect(measuredRanges).toHaveLength(2);
    expect(measuredRanges[0].name).toBe('trace_image');
    expect(measuredRanges[1].name).toBe('trace_image');
    expect(measuredRanges[0].startMark).not.toBe(measuredRanges[1].startMark);
    expect(measuredRanges[0].endMark).not.toBe(measuredRanges[1].endMark);
  });

  it('preserves a successful wrapped result when measurement throws', async () => {
    vi.spyOn(performance, 'mark').mockImplementation(() => ({}) as PerformanceMark);
    vi.spyOn(performance, 'measure').mockImplementation(() => {
      throw new Error('measure failed');
    });
    vi.spyOn(performance, 'clearMarks').mockImplementation(() => {});
    vi.spyOn(performance, 'clearMeasures').mockImplementation(() => {});

    await expect(measureAsyncPerf('trace_image', async () => 'ok')).resolves.toBe('ok');
  });

  it('preserves a wrapped error when measurement throws', async () => {
    const wrappedError = new Error('trace failed');
    vi.spyOn(performance, 'mark').mockImplementation(() => ({}) as PerformanceMark);
    vi.spyOn(performance, 'measure').mockImplementation(() => {
      throw new Error('measure failed');
    });
    vi.spyOn(performance, 'clearMarks').mockImplementation(() => {});
    vi.spyOn(performance, 'clearMeasures').mockImplementation(() => {});

    await expect(
      measureAsyncPerf('trace_image', async () => {
        throw wrappedError;
      }),
    ).rejects.toBe(wrappedError);
  });
});
