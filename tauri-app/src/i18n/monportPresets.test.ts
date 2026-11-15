import { describe, expect, it } from 'vitest';
import { monportPresetCases } from '../test-utils/monportPresets';

type Locale = {
  panels: { machine: { preset: { catalog: Record<string, { description: string; advisory: string }> } } };
};
const locales = import.meta.glob<Locale>('../locales/*.json', { eager: true, import: 'default' });

describe('Monport preset guidance', () => {
  it.each(Object.entries(locales))('preserves K40 setup facts in %s', (_path, locale) => {
    for (const { presetId } of monportPresetCases) {
      const entry = locale.panels.machine.preset.catalog[presetId];
      expect(entry, presetId).toBeDefined();
      expect(entry.description.trim(), presetId).not.toBe('');
      for (const setting of ['X 300 x Y 200 mm', '115200', 'S1000/M4', '$30', '$32=1', '$22=1', 'Nano', 'Ruida']) {
        expect(entry.advisory, presetId).toContain(setting);
      }
    }
  });
});
