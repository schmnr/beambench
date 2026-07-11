import { describe, expect, it } from 'vitest';
import {
  LOCALE_DEFINITIONS,
  LOCALE_MANIFEST,
  findLocaleDefinition,
  formatLocaleDisplayName,
} from '../localeManifest';

describe('locale manifest', () => {
  it('has the supported schema and a valid default locale', () => {
    expect(LOCALE_MANIFEST.schemaVersion).toBe(1);
    expect(LOCALE_MANIFEST.defaultLocale).toBe('en');
    expect(LOCALE_DEFINITIONS.some((locale) => locale.code === LOCALE_MANIFEST.defaultLocale)).toBe(true);
  });

  it('has unique, complete locale metadata', () => {
    const codes = LOCALE_DEFINITIONS.map((locale) => locale.code);
    expect(new Set(codes).size).toBe(codes.length);
    expect(codes).toHaveLength(23);

    for (const locale of LOCALE_DEFINITIONS) {
      expect(locale.nativeName).not.toBe('');
      expect(locale.englishName).not.toBe('');
      expect(['ltr', 'rtl']).toContain(locale.direction);
      expect(locale.htmlLang).not.toBe('');
      expect(locale.hreflang).not.toBe('');
      expect(locale.ogLocale).toMatch(/^[a-z]{2,3}_(?:[A-Z]{2}|\d{3})$/);
    }
  });

  it('keeps the deliberate search-engine locale mappings', () => {
    expect(findLocaleDefinition('es-419')?.hreflang).toBe('es');
    expect(findLocaleDefinition('zh-CN')?.hreflang).toBe('zh-Hans');
    expect(findLocaleDefinition('zh-TW')?.hreflang).toBe('zh-Hant');
  });

  it('formats endonym and English labels for the language menu', () => {
    expect(formatLocaleDisplayName(findLocaleDefinition('en')!)).toBe('English');
    expect(formatLocaleDisplayName(findLocaleDefinition('de')!)).toBe('Deutsch (German)');
    expect(formatLocaleDisplayName(findLocaleDefinition('ja')!)).toBe('日本語 (Japanese)');
  });
});
