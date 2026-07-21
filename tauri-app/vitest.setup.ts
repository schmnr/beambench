// Test setup: initialize i18next so components using useTranslation()
// resolve keys to English values instead of returning the key itself.
import { i18nReady } from './src/i18n';

await i18nReady;
