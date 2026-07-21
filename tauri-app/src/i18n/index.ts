import i18next from 'i18next';
import { initReactI18next } from 'react-i18next';
import ICUImport from 'i18next-icu/cjs';
import { buildPseudoBundle } from './pseudo';

import en from '../locales/en.json';
import de from '../locales/de.json';
import esES from '../locales/es-ES.json';
import es419 from '../locales/es-419.json';
import fr from '../locales/fr.json';
import it from '../locales/it.json';
import ptBR from '../locales/pt-BR.json';
import nl from '../locales/nl.json';
import pl from '../locales/pl.json';
import cs from '../locales/cs.json';
import sv from '../locales/sv.json';
import nb from '../locales/nb.json';
import da from '../locales/da.json';
import fi from '../locales/fi.json';
import hu from '../locales/hu.json';
import tr from '../locales/tr.json';
import el from '../locales/el.json';
import ru from '../locales/ru.json';
import sl from '../locales/sl.json';
import ja from '../locales/ja.json';
import ko from '../locales/ko.json';
import zhCN from '../locales/zh-CN.json';
import zhTW from '../locales/zh-TW.json';

const ICU = (
  (ICUImport as unknown as { default?: typeof ICUImport }).default ?? ICUImport
);

export const SUPPORTED_LOCALES = [
  'en', 'de', 'es-ES', 'es-419', 'fr', 'it', 'pt-BR', 'nl', 'pl', 'cs',
  'sv', 'nb', 'da', 'fi', 'hu', 'tr', 'el', 'ru', 'sl',
  'ja', 'ko', 'zh-CN', 'zh-TW',
] as const;
export type SupportedLocale = (typeof SUPPORTED_LOCALES)[number];

const resources: Record<string, { translation: Record<string, unknown> }> = {
  en: { translation: en },
  de: { translation: de },
  'es-ES': { translation: esES },
  'es-419': { translation: es419 },
  fr: { translation: fr },
  it: { translation: it },
  'pt-BR': { translation: ptBR },
  nl: { translation: nl },
  pl: { translation: pl },
  cs: { translation: cs },
  sv: { translation: sv },
  nb: { translation: nb },
  da: { translation: da },
  fi: { translation: fi },
  hu: { translation: hu },
  tr: { translation: tr },
  el: { translation: el },
  ru: { translation: ru },
  sl: { translation: sl },
  ja: { translation: ja },
  ko: { translation: ko },
  'zh-CN': { translation: zhCN },
  'zh-TW': { translation: zhTW },
};

// Pseudo-locale: registered only in dev builds, never persisted.
// Switching to 'en-XA' must go through i18n.changeLanguage() directly,
// never through updateSettings({ display_language: 'en-XA' }) — the Rust
// validator (validate_locale_code) will reject it because it is absent
// from SUPPORTED_LOCALES. This intentional split keeps the persisted
// locale shape clean while still giving us a dev-time layout-stress tool.
if ((import.meta as ImportMeta & { env?: { DEV?: boolean } }).env?.DEV) {
  resources['en-XA'] = { translation: buildPseudoBundle(en) };
}

// Dev/test override: `?lang=<code>` in the URL sets the initial locale.
// Useful for manual browser-tab debugging during `npm run dev` and for
// any agent (Computer Use) doing visual checks against a specific locale
// without going through settings persistence. Production builds ignore
// this — they always boot in 'en' and switch when settings hydrate.
const dev = (import.meta as ImportMeta & { env?: { DEV?: boolean } }).env?.DEV;
let initialLanguage: string = 'en';
if (dev && typeof window !== 'undefined') {
  const param = new URLSearchParams(window.location.search).get('lang');
  if (param && (SUPPORTED_LOCALES as readonly string[]).concat(['en-XA']).includes(param)) {
    initialLanguage = param;
  }
}

export const i18nReady = i18next
  .use(ICU)
  .use(initReactI18next)
  .init({
    resources,
    lng: initialLanguage,
    fallbackLng: 'en',
    supportedLngs: dev ? [...SUPPORTED_LOCALES, 'en-XA'] : SUPPORTED_LOCALES,
    interpolation: { escapeValue: false },
    returnNull: false,
  });

export default i18next;
