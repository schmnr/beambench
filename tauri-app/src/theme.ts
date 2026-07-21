import type { UiTheme } from './types/commands';

export type ResolvedUiTheme = Exclude<UiTheme, 'system'>;

export const UI_THEME_CACHE_KEY = 'beam-bench.ui-theme';
export const UI_THEME_CACHE_VERSION = 1;
export const UI_THEME_MEDIA_QUERY = '(prefers-color-scheme: light)';

export interface UiThemeCache {
  version: typeof UI_THEME_CACHE_VERSION;
  selected: UiTheme;
  resolved: ResolvedUiTheme;
}

interface ThemeStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

interface ThemeMediaQuery {
  matches: boolean;
  addEventListener?: (type: 'change', listener: (event: MediaQueryListEvent) => void) => void;
  removeEventListener?: (type: 'change', listener: (event: MediaQueryListEvent) => void) => void;
  addListener?: (listener: (event: MediaQueryListEvent) => void) => void;
  removeListener?: (listener: (event: MediaQueryListEvent) => void) => void;
}

export interface UiThemeRuntime {
  root: HTMLElement;
  storage: ThemeStorage | null;
  matchMedia: ((query: string) => ThemeMediaQuery) | null;
}

export interface UiThemeController {
  sync(selected: UiTheme): ResolvedUiTheme;
  dispose(): void;
}

export function isUiTheme(value: unknown): value is UiTheme {
  return value === 'system' || value === 'light' || value === 'dark';
}

export function isResolvedUiTheme(value: unknown): value is ResolvedUiTheme {
  return value === 'light' || value === 'dark';
}

export function parseUiThemeCache(raw: string | null): UiThemeCache | null {
  if (raw === null) return null;

  try {
    const value = JSON.parse(raw) as Partial<UiThemeCache> | null;
    if (
      value?.version !== UI_THEME_CACHE_VERSION ||
      !isUiTheme(value.selected) ||
      !isResolvedUiTheme(value.resolved)
    ) {
      return null;
    }
    return {
      version: UI_THEME_CACHE_VERSION,
      selected: value.selected,
      resolved: value.resolved,
    };
  } catch {
    return null;
  }
}

export function readUiThemeCache(storage: ThemeStorage | null): UiThemeCache | null {
  if (!storage) return null;
  try {
    return parseUiThemeCache(storage.getItem(UI_THEME_CACHE_KEY));
  } catch {
    return null;
  }
}

export function writeUiThemeCache(
  storage: ThemeStorage | null,
  selected: UiTheme,
  resolved: ResolvedUiTheme,
): void {
  if (!storage) return;
  try {
    storage.setItem(
      UI_THEME_CACHE_KEY,
      JSON.stringify({
        version: UI_THEME_CACHE_VERSION,
        selected,
        resolved,
      } satisfies UiThemeCache),
    );
  } catch {
    // Theme persistence is a startup optimization. Backend settings remain authoritative.
  }
}

export function applyResolvedUiTheme(root: HTMLElement, resolved: ResolvedUiTheme): void {
  root.dataset.theme = resolved;
  root.style.colorScheme = resolved;
}

function subscribeToThemeChanges(
  mediaQuery: ThemeMediaQuery,
  listener: (event: MediaQueryListEvent) => void,
): () => void {
  if (mediaQuery.addEventListener && mediaQuery.removeEventListener) {
    mediaQuery.addEventListener('change', listener);
    return () => mediaQuery.removeEventListener?.('change', listener);
  }
  if (mediaQuery.addListener && mediaQuery.removeListener) {
    mediaQuery.addListener(listener);
    return () => mediaQuery.removeListener?.(listener);
  }
  return () => {};
}

export function createUiThemeController(runtime: UiThemeRuntime): UiThemeController {
  let unsubscribeSystemTheme: (() => void) | null = null;

  const dispose = () => {
    const unsubscribe = unsubscribeSystemTheme;
    unsubscribeSystemTheme = null;
    try {
      unsubscribe?.();
    } catch {
      // A platform theme-listener failure must not block settings hydration or saving.
    }
  };

  return {
    sync(selected) {
      dispose();

      let mediaQuery: ThemeMediaQuery | null = null;
      if (selected === 'system' && runtime.matchMedia) {
        try {
          mediaQuery = runtime.matchMedia(UI_THEME_MEDIA_QUERY);
        } catch {
          // Keep CSS authoritative and use the last valid cache if media queries are unavailable.
        }
      }
      const cached =
        selected === 'system' && !mediaQuery ? readUiThemeCache(runtime.storage) : null;
      const resolved: ResolvedUiTheme =
        selected === 'system'
          ? mediaQuery
            ? mediaQuery.matches
              ? 'light'
              : 'dark'
            : cached?.selected === 'system'
              ? cached.resolved
              : 'dark'
          : selected;

      applyResolvedUiTheme(runtime.root, resolved);
      writeUiThemeCache(runtime.storage, selected, resolved);

      if (mediaQuery) {
        const onChange = (event: MediaQueryListEvent) => {
          const nextResolved: ResolvedUiTheme = event.matches ? 'light' : 'dark';
          applyResolvedUiTheme(runtime.root, nextResolved);
          writeUiThemeCache(runtime.storage, 'system', nextResolved);
        };
        try {
          unsubscribeSystemTheme = subscribeToThemeChanges(mediaQuery, onChange);
        } catch {
          unsubscribeSystemTheme = null;
        }
      }

      return resolved;
    },
    dispose,
  };
}

function browserRuntime(): UiThemeRuntime {
  const storage = (() => {
    try {
      return window.localStorage;
    } catch {
      return null;
    }
  })();

  return {
    root: document.documentElement,
    storage,
    matchMedia: typeof window.matchMedia === 'function' ? window.matchMedia.bind(window) : null,
  };
}

export const uiThemeController = createUiThemeController(browserRuntime());
