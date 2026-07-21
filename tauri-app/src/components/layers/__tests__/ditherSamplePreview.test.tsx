import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';

import { DitherSamplePreview } from '../DitherSamplePreview';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe('DitherSamplePreview', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.mocked(invoke).mockReset();
    vi.spyOn(console, 'warn').mockImplementation(() => {});
    Object.defineProperty(URL, 'createObjectURL', {
      writable: true,
      configurable: true,
      value: vi.fn(() => `blob:${Math.random()}`),
    });
    Object.defineProperty(URL, 'revokeObjectURL', {
      writable: true,
      configurable: true,
      value: vi.fn(),
    });
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('ignores stale renders that resolve after a newer mode', async () => {
    const floyd = deferred<number[]>();
    const atkinson = deferred<number[]>();

    vi.mocked(invoke).mockImplementation((_command, payload) => {
      const mode = (payload as { mode: string }).mode;
      if (mode === 'floyd_steinberg') return floyd.promise;
      if (mode === 'atkinson') return atkinson.promise;
      return Promise.reject(new Error(`unexpected mode ${mode}`));
    });

    const { rerender } = render(<DitherSamplePreview mode="floyd_steinberg" />);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(150);
    });

    rerender(<DitherSamplePreview mode="atkinson" />);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(150);
    });

    await act(async () => {
      atkinson.resolve([1, 2, 3, 4]);
      await Promise.resolve();
      await Promise.resolve();
    });

    const latest = screen.getByAltText('Dither sample: atkinson');
    const latestSrc = latest.getAttribute('src');

    await act(async () => {
      floyd.resolve([5, 6, 7, 8]);
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(screen.queryByAltText('Dither sample: floyd_steinberg')).toBeNull();
    expect(screen.getByAltText('Dither sample: atkinson').getAttribute('src')).toBe(latestSrc);
  });

  it('shows an explicit failure state when render fails', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('backend failed'));

    render(<DitherSamplePreview mode="jarvis" />);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(150);
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(screen.getByText('Preview unavailable')).toBeDefined();
  });
});
