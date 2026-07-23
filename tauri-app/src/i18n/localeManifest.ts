import rawManifest from './locale-manifest.json';

export type TextDirection = 'ltr' | 'rtl';

export interface LocaleDefinition {
  readonly code: string;
  readonly nativeName: string;
  readonly englishName: string;
  readonly direction: TextDirection;
  readonly htmlLang: string;
  readonly hreflang: string;
  readonly ogLocale: string;
}

export interface LocaleManifest {
  readonly schemaVersion: 1;
  readonly defaultLocale: string;
  readonly locales: readonly LocaleDefinition[];
}

const MANIFEST_KEYS = ['schemaVersion', 'defaultLocale', 'locales'] as const;
const LOCALE_KEYS = [
  'code',
  'nativeName',
  'englishName',
  'direction',
  'htmlLang',
  'hreflang',
  'ogLocale',
] as const;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function assertExactKeys(
  value: Record<string, unknown>,
  expectedKeys: readonly string[],
  path: string,
): void {
  const expected = new Set(expectedKeys);
  const missing = expectedKeys.filter((key) => !Object.prototype.hasOwnProperty.call(value, key));
  const unexpected = Object.keys(value).filter((key) => !expected.has(key));
  if (missing.length > 0 || unexpected.length > 0) {
    throw new Error(
      `${path} has invalid fields (missing: ${missing.join(', ') || 'none'}; unexpected: ${unexpected.join(', ') || 'none'})`,
    );
  }
}

function readNonEmptyString(value: unknown, path: string): string {
  if (typeof value !== 'string' || value.trim() === '') {
    throw new Error(`${path} must be a non-empty string`);
  }
  if (value !== value.trim()) {
    throw new Error(`${path} must not have leading or trailing whitespace`);
  }
  if (value !== value.normalize('NFC')) {
    throw new Error(`${path} must use NFC Unicode normalization`);
  }
  return value;
}

function readCanonicalLanguageTag(value: unknown, path: string): string {
  const tag = readNonEmptyString(value, path);
  let canonical: string[];
  try {
    canonical = Intl.getCanonicalLocales(tag);
  } catch {
    throw new Error(`${path} must be a valid BCP 47 language tag`);
  }
  if (canonical.length !== 1 || canonical[0] !== tag) {
    throw new Error(`${path} must use canonical BCP 47 casing and syntax`);
  }
  return tag;
}

function languageSubtag(tag: string): string {
  return tag.split('-')[0];
}

function regionSubtag(tag: string): string | undefined {
  return tag.split('-').slice(1).find((part) => /^(?:[A-Z]{2}|\d{3})$/.test(part));
}

function parseLocaleDefinition(value: unknown, index: number): LocaleDefinition {
  const path = `locale manifest locales[${index}]`;
  if (!isRecord(value)) throw new Error(`${path} must be an object`);
  assertExactKeys(value, LOCALE_KEYS, path);

  const code = readCanonicalLanguageTag(value.code, `${path}.code`);
  const nativeName = readNonEmptyString(value.nativeName, `${path}.nativeName`);
  const englishName = readNonEmptyString(value.englishName, `${path}.englishName`);
  if (value.direction !== 'ltr' && value.direction !== 'rtl') {
    throw new Error(`${path}.direction must be either ltr or rtl`);
  }
  const direction = value.direction;
  const htmlLang = readCanonicalLanguageTag(value.htmlLang, `${path}.htmlLang`);
  if (htmlLang !== code) {
    throw new Error(`${path}.htmlLang must match its locale code`);
  }
  const hreflang = readCanonicalLanguageTag(value.hreflang, `${path}.hreflang`);
  if (languageSubtag(hreflang) !== languageSubtag(code)) {
    throw new Error(`${path}.hreflang must use the locale's language`);
  }
  const ogLocale = readNonEmptyString(value.ogLocale, `${path}.ogLocale`);
  if (!/^[a-z]{2,3}_(?:[A-Z]{2}|\d{3})$/.test(ogLocale)) {
    throw new Error(`${path}.ogLocale must use language_TERRITORY format`);
  }
  const ogLanguageTag = readCanonicalLanguageTag(
    ogLocale.replace('_', '-'),
    `${path}.ogLocale`,
  );
  if (languageSubtag(ogLanguageTag) !== languageSubtag(code)) {
    throw new Error(`${path}.ogLocale must use the locale's language`);
  }
  const codeRegion = regionSubtag(code);
  if (codeRegion && regionSubtag(ogLanguageTag) !== codeRegion) {
    throw new Error(`${path}.ogLocale must use the locale's territory`);
  }

  return Object.freeze({
    code,
    nativeName,
    englishName,
    direction,
    htmlLang,
    hreflang,
    ogLocale,
  });
}

export function parseLocaleManifest(value: unknown): LocaleManifest {
  if (!isRecord(value)) throw new Error('Locale manifest must be an object');
  assertExactKeys(value, MANIFEST_KEYS, 'Locale manifest');
  if (value.schemaVersion !== 1) {
    throw new Error('Locale manifest schemaVersion must be 1');
  }
  const defaultLocale = readCanonicalLanguageTag(
    value.defaultLocale,
    'Locale manifest defaultLocale',
  );
  if (!Array.isArray(value.locales) || value.locales.length === 0) {
    throw new Error('Locale manifest locales must be a non-empty array');
  }

  const locales = value.locales.map(parseLocaleDefinition);
  const codes = locales.map((locale) => locale.code);
  if (new Set(codes).size !== codes.length) {
    throw new Error('Locale manifest locale codes must be unique');
  }
  if (!codes.includes(defaultLocale)) {
    throw new Error('Locale manifest defaultLocale must reference a locale entry');
  }

  return Object.freeze({
    schemaVersion: 1,
    defaultLocale,
    locales: Object.freeze(locales),
  });
}

export const LOCALE_MANIFEST = parseLocaleManifest(rawManifest);
export const LOCALE_DEFINITIONS = LOCALE_MANIFEST.locales;

export function formatLocaleDisplayName(locale: LocaleDefinition): string {
  return locale.nativeName === locale.englishName
    ? locale.nativeName
    : `${locale.nativeName} (${locale.englishName})`;
}

export function findLocaleDefinition(code: string): LocaleDefinition | undefined {
  return LOCALE_DEFINITIONS.find((locale) => locale.code === code);
}
