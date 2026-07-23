import { describe, it, expect } from 'vitest';
import { IntlMessageFormat } from 'intl-messageformat';
import en from '../en.json';
import { LOCALE_DEFINITIONS } from '../../i18n/localeManifest';

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

function collectMessages(obj: unknown, prefix = ''): Record<string, string> {
  if (typeof obj !== 'object' || obj === null) return {};
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(obj as Record<string, unknown>)) {
    const path = prefix ? `${prefix}.${k}` : k;
    if (typeof v === 'string') out[path] = v;
    else Object.assign(out, collectMessages(v, path));
  }
  return out;
}

interface AstNode {
  type: number;
  value?: string;
  options?: Record<string, { value: AstNode[] }>;
  children?: AstNode[];
}

function collectMessageTokens(message: string, locale: string): { arguments: string[]; tags: string[] } {
  const ast = new IntlMessageFormat(message, locale).getAst() as AstNode[];
  const argumentsFound = new Set<string>();
  const tagsFound = new Set<string>();

  const visit = (nodes: AstNode[]) => {
    for (const node of nodes) {
      if (node.value && node.type >= 1 && node.type <= 6) argumentsFound.add(node.value);
      if (node.value && node.type === 8) tagsFound.add(node.value);
      if (node.children) visit(node.children);
      if (node.options) {
        for (const option of Object.values(node.options)) visit(option.value);
      }
    }
  };

  visit(ast);
  return {
    arguments: [...argumentsFound].sort(),
    tags: [...tagsFound].sort(),
  };
}

describe('locale-parity', () => {
  const enMessages = collectMessages(en);
  const enKeys = Object.keys(enMessages).sort();
  const enTokens = Object.fromEntries(
    Object.entries(enMessages).map(([key, message]) => [key, collectMessageTokens(message, 'en')]),
  );

  it('contains exactly the expected locale set', () => {
    const expected = LOCALE_DEFINITIONS.map((locale) => locale.code).sort();
    expect(Object.keys(allLocales).sort()).toEqual(expected);
  });

  for (const [code, bundle] of Object.entries(allLocales)) {
    if (code === 'en') continue;
    it(`${code} has identical key set to en`, () => {
      const localeKeys = Object.keys(collectMessages(bundle)).sort();
      const missing = enKeys.filter((k) => !localeKeys.includes(k));
      const extra = localeKeys.filter((k) => !enKeys.includes(k));
      expect(missing, `Missing in ${code}`).toEqual([]);
      expect(extra, `Extra in ${code}`).toEqual([]);
    });

    it(`${code} has valid ICU messages with matching arguments and tags`, () => {
      const localeMessages = collectMessages(bundle);
      for (const key of enKeys) {
        const tokens = collectMessageTokens(localeMessages[key], code);
        expect(tokens.arguments, `Argument mismatch in ${code}:${key}`).toEqual(enTokens[key].arguments);
        expect(tokens.tags, `Tag mismatch in ${code}:${key}`).toEqual(enTokens[key].tags);
      }
    });
  }
});
