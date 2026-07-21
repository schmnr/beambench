import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  UI_THEME_CACHE_KEY,
  createUiThemeController,
  parseUiThemeCache,
  readUiThemeCache,
  type UiThemeRuntime,
} from './theme';

function makeRuntime(options: { prefersLight?: boolean } = {}) {
  const values = new Map<string, string>();
  let listener: ((event: MediaQueryListEvent) => void) | null = null;
  const mediaQuery = {
    matches: options.prefersLight ?? false,
    addEventListener: vi.fn(
      (_type: 'change', nextListener: (event: MediaQueryListEvent) => void) => {
        listener = nextListener;
      },
    ),
    removeEventListener: vi.fn(
      (_type: 'change', nextListener: (event: MediaQueryListEvent) => void) => {
        if (listener === nextListener) listener = null;
      },
    ),
  };
  const runtime: UiThemeRuntime = {
    root: document.createElement('html'),
    storage: {
      getItem: (key) => values.get(key) ?? null,
      setItem: (key, value) => {
        values.set(key, value);
      },
    },
    matchMedia: vi.fn(() => mediaQuery),
  };

  return {
    runtime,
    mediaQuery,
    emitChange(matches: boolean) {
      listener?.({ matches } as MediaQueryListEvent);
    },
  };
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe('UI theme cache', () => {
  it('accepts only the current version and valid selected and resolved values', () => {
    expect(
      parseUiThemeCache(
        JSON.stringify({
          version: 1,
          selected: 'system',
          resolved: 'light',
        }),
      ),
    ).toEqual({ version: 1, selected: 'system', resolved: 'light' });
    expect(
      parseUiThemeCache(JSON.stringify({ version: 0, selected: 'light', resolved: 'light' })),
    ).toBeNull();
    expect(
      parseUiThemeCache(JSON.stringify({ version: 1, selected: 'sepia', resolved: 'light' })),
    ).toBeNull();
    expect(
      parseUiThemeCache(JSON.stringify({ version: 1, selected: 'dark', resolved: 'system' })),
    ).toBeNull();
    expect(parseUiThemeCache('not json')).toBeNull();
  });

  it('treats unavailable storage as a non-fatal cache miss', () => {
    const throwingStorage = {
      getItem: vi.fn(() => {
        throw new Error('blocked');
      }),
      setItem: vi.fn(() => {
        throw new Error('blocked');
      }),
    };
    expect(readUiThemeCache(throwingStorage)).toBeNull();

    const runtime = makeRuntime().runtime;
    runtime.storage = throwingStorage;
    expect(() => createUiThemeController(runtime).sync('light')).not.toThrow();
    expect(runtime.root.dataset.theme).toBe('light');
  });
});

describe('UI theme controller', () => {
  it('applies explicit themes to the root and mirrors them locally', () => {
    const { runtime } = makeRuntime({ prefersLight: false });
    const controller = createUiThemeController(runtime);
    runtime.storage?.setItem(
      UI_THEME_CACHE_KEY,
      JSON.stringify({ version: 1, selected: 'system', resolved: 'dark' }),
    );

    expect(controller.sync('light')).toBe('light');
    expect(runtime.root.dataset.theme).toBe('light');
    expect(runtime.root.style.colorScheme).toBe('light');
    expect(parseUiThemeCache(runtime.storage?.getItem(UI_THEME_CACHE_KEY) ?? null)).toEqual({
      version: 1,
      selected: 'light',
      resolved: 'light',
    });
  });

  it('uses the last valid System resolution if media queries are unavailable', () => {
    const { runtime } = makeRuntime();
    runtime.matchMedia = null;
    runtime.storage?.setItem(
      UI_THEME_CACHE_KEY,
      JSON.stringify({ version: 1, selected: 'system', resolved: 'light' }),
    );

    expect(createUiThemeController(runtime).sync('system')).toBe('light');
    expect(runtime.root.dataset.theme).toBe('light');
  });

  it('resolves System synchronously and follows operating system changes live', () => {
    const { runtime, mediaQuery, emitChange } = makeRuntime({ prefersLight: true });
    const controller = createUiThemeController(runtime);

    expect(controller.sync('system')).toBe('light');
    expect(runtime.root.dataset.theme).toBe('light');
    expect(mediaQuery.addEventListener).toHaveBeenCalledOnce();

    emitChange(false);
    expect(runtime.root.dataset.theme).toBe('dark');
    expect(parseUiThemeCache(runtime.storage?.getItem(UI_THEME_CACHE_KEY) ?? null)).toMatchObject({
      selected: 'system',
      resolved: 'dark',
    });

    controller.sync('dark');
    expect(mediaQuery.removeEventListener).toHaveBeenCalledOnce();
    emitChange(true);
    expect(runtime.root.dataset.theme).toBe('dark');
  });
});
