import { describe, expect, it } from 'vitest';
import {
  LOCALE_DEFINITIONS,
  LOCALE_MANIFEST,
  findLocaleDefinition,
  formatLocaleDisplayName,
  parseLocaleManifest,
} from '../localeManifest';

function manifestWithLocalePatch(index: number, patch: Record<string, unknown>): unknown {
  return {
    ...LOCALE_MANIFEST,
    locales: LOCALE_DEFINITIONS.map((locale, localeIndex) => (
      localeIndex === index ? { ...locale, ...patch } : locale
    )),
  };
}

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
      expect(locale.nativeName.trim()).toBe(locale.nativeName);
      expect(locale.englishName.trim()).toBe(locale.englishName);
      expect(['ltr', 'rtl']).toContain(locale.direction);
      expect(locale.htmlLang).toBe(locale.code);
      expect(Intl.getCanonicalLocales(locale.code)).toEqual([locale.code]);
      expect(Intl.getCanonicalLocales(locale.hreflang)).toEqual([locale.hreflang]);
      expect(locale.ogLocale).toMatch(/^[a-z]{2,3}_(?:[A-Z]{2}|\d{3})$/);
    }
  });

  it('rejects malformed or incomplete metadata instead of trusting a type assertion', () => {
    expect(() => parseLocaleManifest(manifestWithLocalePatch(4, { nativeName: null })))
      .toThrow(/nativeName/);
    expect(() => parseLocaleManifest(manifestWithLocalePatch(4, { englishName: '   ' })))
      .toThrow(/englishName/);
    expect(() => parseLocaleManifest(manifestWithLocalePatch(1, { htmlLang: 'fr' })))
      .toThrow(/htmlLang/);
    expect(() => parseLocaleManifest(manifestWithLocalePatch(1, { hreflang: 'fr' })))
      .toThrow(/hreflang/);
    expect(() => parseLocaleManifest(manifestWithLocalePatch(6, {
      code: 'pt-br',
      htmlLang: 'pt-br',
    }))).toThrow(/canonical BCP 47/);
    expect(() => parseLocaleManifest(manifestWithLocalePatch(6, { ogLocale: 'pt_PT' })))
      .toThrow(/territory/);
    expect(() => parseLocaleManifest(manifestWithLocalePatch(2, { unexpected: true })))
      .toThrow(/unexpected/);
  });

  it('rejects duplicate codes and freezes validated data', () => {
    expect(() => parseLocaleManifest(manifestWithLocalePatch(1, {
      code: 'en',
      htmlLang: 'en',
      hreflang: 'en',
      ogLocale: 'en_US',
    }))).toThrow(/unique/);
    expect(Object.isFrozen(LOCALE_MANIFEST)).toBe(true);
    expect(Object.isFrozen(LOCALE_DEFINITIONS)).toBe(true);
    expect(LOCALE_DEFINITIONS.every(Object.isFrozen)).toBe(true);
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
