import { describe, it, expect } from 'vitest';
import { transformToPseudo, buildPseudoBundle } from '../pseudo';

describe('transformToPseudo', () => {
  it('accents plain ASCII letters', () => {
    expect(transformToPseudo('Open')).toMatch(/⟦.*Öṕéñ.*⟧/);
  });

  it('preserves single placeholders verbatim', () => {
    const out = transformToPseudo('Hello {name}');
    expect(out).toContain('{name}');
    expect(out).toMatch(/⟦.*Ĥéĺĺö.*\{name\}.*⟧/);
  });

  it('preserves nested ICU plural placeholders', () => {
    const out = transformToPseudo('{count, plural, one {# layer} other {# layers}}');
    expect(out).toContain('{count, plural,');
    expect(out).toMatch(/\{# ĺäýéŕ\}/);
    expect(out).toMatch(/\{# ĺäýéŕš\}/);
  });

  it('preserves Trans tags', () => {
    const out = transformToPseudo('Click <0>here</0> to continue');
    expect(out).toContain('<0>');
    expect(out).toContain('</0>');
    expect(out).toMatch(/ĥéŕé/);
  });

  it('pads strings to ~130% of original length', () => {
    const input = 'Cancel';
    const out = transformToPseudo(input);
    expect(out.length).toBeGreaterThanOrEqual(Math.floor(input.length * 1.3));
  });

  it('returns wrapped marker for empty string', () => {
    expect(transformToPseudo('')).toBe('⟦⟧');
  });

  it('leaves digits and punctuation untouched', () => {
    const out = transformToPseudo('100% (done)');
    expect(out).toContain('100%');
    expect(out).toContain('(');
    expect(out).toContain(')');
  });
});

describe('buildPseudoBundle', () => {
  it('recursively transforms nested objects', () => {
    const en = {
      menus: {
        file: {
          label: 'File',
          open: 'Open',
        },
      },
    };
    const xa = buildPseudoBundle(en) as { menus: { file: { label: string; open: string } } };
    expect(xa.menus.file.label).toMatch(/⟦.*Ƒíĺé.*⟧/);
    expect(xa.menus.file.open).toMatch(/⟦.*Öṕéñ.*⟧/);
  });
});
