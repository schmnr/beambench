import type { TFunction } from 'i18next';
import { MENU_LABEL_KEYS } from '../i18n/menuLabelKeys';

/**
 * Build the `NativeMenuLabels` payload Rust expects. Iterates the static
 * menu label map, looks up each translation key via `t`, and emits the
 * by_title map. Falls back to the English title if the key is missing in
 * the active locale.
 */
export function buildNativeMenuLabels(t: TFunction): { by_title: Record<string, string> } {
  const by_title: Record<string, string> = {};
  for (const [english, key] of Object.entries(MENU_LABEL_KEYS)) {
    const translated = t(key);
    // i18next returns the key string when a translation is missing —
    // skip those so Rust falls back to English instead of showing
    // `menus.foo.label` in the native menu.
    if (translated && translated !== key) {
      by_title[english] = translated;
    }
  }
  return { by_title };
}
