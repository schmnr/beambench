// @vitest-environment node
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';

/**
 * Regex-based complement to the AST-aware `i18next/no-literal-string` ESLint
 * rule. It catches plain JSX text and literal user-facing attributes. Template
 * expressions and strings assembled in TypeScript remain the responsibility of
 * ESLint. Known regex false positives are listed in
 * `.i18n-lint-exceptions.json`.
 */

const allowlistObj = JSON.parse(
  readFileSync(new URL('../../../.i18n-lint-exceptions.json', import.meta.url), 'utf-8'),
);
const allowlist = new Set(
  Object.keys(allowlistObj).filter((k) => !k.startsWith('_')),
);

// Scan every .tsx under src/ — the design says the guard must catch
// hardcoded English wherever it lives, not just src/components/.
const componentModules = import.meta.glob<string>('../../**/*.tsx', {
  eager: true,
  query: '?raw',
  import: 'default',
});

// Match JSX text literals between open/close brackets.
const JSX_TEXT_LITERAL = />\s*([A-Za-z][A-Za-z\s,.!?:]{4,})\s*</g;

// Match user-facing JSX attribute literals: title="...", placeholder="...",
// aria-label="...", alt="..." with a literal string value. Only flag values
// that contain a meaningful word (two+ consecutive letters); this skips
// `title=""`, `alt=" "`, single-character markers, etc.
const JSX_USER_ATTR_LITERAL =
  /\b(?:title|placeholder|aria-label|alt)\s*=\s*"([A-Za-z][^"]*[A-Za-z][A-Za-z\s,.!?:][^"]*)"/g;

describe('extraction-coverage', () => {
  // Each glob path is of the form '../../components/foo/Bar.tsx';
  // convert to allowlist-style 'src/components/foo/Bar.tsx'.
  const entries = Object.entries(componentModules)
    .filter(([path]) => !path.includes('/__tests__/'))
    .filter(([path]) => !path.endsWith('.test.tsx'))
    .map(([path, source]) => {
      const rel = path.replace(/^(\.\.\/)+/, 'src/');
      return { rel, source };
    });

  it('discovers component files via glob', () => {
    expect(entries.length).toBeGreaterThan(0);
    // Confirm that the wider glob finds files outside src/components.
    const paths = entries.map((e) => e.rel);
    expect(paths, 'must scan src/App.tsx').toContain('src/App.tsx');
    expect(paths, 'must scan src/panels/').toEqual(
      expect.arrayContaining([expect.stringMatching(/^src\/panels\//)]),
    );
  });

  const nonAllowlisted = entries.filter(({ rel }) => !allowlist.has(rel));

  for (const { rel, source } of nonAllowlisted) {
    it(`${rel} has no untranslated JSX text or user-facing attribute literals`, () => {
      const offenders: string[] = [];
      for (const m of source.matchAll(JSX_TEXT_LITERAL)) {
        const text = m[1].trim();
        if (text.length < 3) continue;
        offenders.push(`text: ${text}`);
      }
      for (const m of source.matchAll(JSX_USER_ATTR_LITERAL)) {
        const text = m[1].trim();
        if (text.length < 3) continue;
        offenders.push(`attr: ${text}`);
      }
      expect(offenders, `Untranslated literals in ${rel}`).toEqual([]);
    });
  }

  // Sentinel test so the suite never has zero registered cases.
  if (nonAllowlisted.length === 0) {
    it('allowlist covers every discovered component', () => {
      expect(nonAllowlisted).toEqual([]);
    });
  }
});
