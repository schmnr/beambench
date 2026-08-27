// Test setup: initialize i18next so components using useTranslation()
// resolve keys to English values instead of returning the key itself.
import { i18nReady } from './src/i18n';

await i18nReady;

const originalConsoleError = console.error.bind(console);
console.error = (...args: unknown[]) => {
  const message = typeof args[0] === 'string' ? args[0] : '';
  if (message.includes('not wrapped in act')) {
    throw new Error(`React test update escaped act(): ${message}`);
  }
  originalConsoleError(...args);
};
