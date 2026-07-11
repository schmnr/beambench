import rawManifest from './locale-manifest.json';

export type TextDirection = 'ltr' | 'rtl';

export interface LocaleDefinition {
  code: string;
  nativeName: string;
  englishName: string;
  direction: TextDirection;
  htmlLang: string;
  hreflang: string;
  ogLocale: string;
}

export interface LocaleManifest {
  schemaVersion: number;
  defaultLocale: string;
  locales: LocaleDefinition[];
}

export const LOCALE_MANIFEST = rawManifest as LocaleManifest;
export const LOCALE_DEFINITIONS = LOCALE_MANIFEST.locales;

export function formatLocaleDisplayName(locale: LocaleDefinition): string {
  return locale.nativeName === locale.englishName
    ? locale.nativeName
    : `${locale.nativeName} (${locale.englishName})`;
}

export function findLocaleDefinition(code: string): LocaleDefinition | undefined {
  return LOCALE_DEFINITIONS.find((locale) => locale.code === code);
}
