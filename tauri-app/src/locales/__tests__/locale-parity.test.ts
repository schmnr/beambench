import { describe, it, expect } from 'vitest';
import en from '../en.json';

// Dynamically discover every locale file under src/locales/.
// Adding/removing a locale JSON automatically affects the test.
const localeModules = (
  import.meta as ImportMeta & {
    glob?: (pattern: string, opts: { eager: true; import: 'default' }) => Record<string, unknown>;
  }
).glob!('../*.json', { eager: true, import: 'default' });

function localeCodeFromPath(path: string): string {
  // path is like '../en.json' → 'en'
  const m = path.match(/\.\.\/(.+)\.json$/);
  if (!m) throw new Error(`Unexpected glob path: ${path}`);
  return m[1];
}

const allLocales: Record<string, unknown> = {};
for (const [path, bundle] of Object.entries(localeModules)) {
  allLocales[localeCodeFromPath(path)] = bundle;
}

function collectKeys(obj: unknown, prefix = ''): string[] {
  if (typeof obj !== 'object' || obj === null) return [];
  const out: string[] = [];
  for (const [k, v] of Object.entries(obj as Record<string, unknown>)) {
    const path = prefix ? `${prefix}.${k}` : k;
    if (typeof v === 'string') out.push(path);
    else out.push(...collectKeys(v, path));
  }
  return out.sort();
}

describe('locale-parity', () => {
  const enKeys = collectKeys(en);

  it('contains exactly the expected locale set', () => {
    const expected = [
      'en', 'de', 'es-ES', 'es-419', 'fr', 'it', 'pt-BR', 'nl', 'pl', 'cs',
      'sv', 'nb', 'da', 'fi', 'hu', 'tr', 'el', 'ru', 'sl',
      'ja', 'ko', 'zh-CN', 'zh-TW',
    ].sort();
    expect(Object.keys(allLocales).sort()).toEqual(expected);
  });

  for (const [code, bundle] of Object.entries(allLocales)) {
    if (code === 'en') continue;
    it(`${code} has identical key set to en`, () => {
      const localeKeys = collectKeys(bundle);
      const missing = enKeys.filter((k) => !localeKeys.includes(k));
      const extra = localeKeys.filter((k) => !enKeys.includes(k));
      expect(missing, `Missing in ${code}`).toEqual([]);
      expect(extra, `Extra in ${code}`).toEqual([]);
    });
  }
});
