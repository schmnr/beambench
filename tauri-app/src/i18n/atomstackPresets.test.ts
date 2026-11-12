import { describe, expect, it } from 'vitest';
import { atomstackPresetCases } from '../test-utils/atomstackPresets';

type Locale = {
  panels: { machine: { preset: { catalog: Record<string, { description: string; advisory: string }> } } };
};

const locales = import.meta.glob<Locale>('../locales/*.json', { eager: true, import: 'default' });

describe('AtomStack preset guidance', () => {
  it.each(Object.entries(locales))('localizes the exact settings in %s', (_path, locale) => {
    const catalog = locale.panels.machine.preset.catalog;
    for (const preset of atomstackPresetCases) {
      const entry = catalog[preset.presetId];
      expect(entry, preset.presetId).toBeDefined();
      expect(entry.description.trim(), preset.presetId).not.toBe('');
      expect(entry.advisory, preset.presetId).toMatch(
        new RegExp(`${preset.width}\\s*[x×*]\\s*${preset.height}`),
      );
      expect(entry.advisory, preset.presetId).toContain('$30');
      expect(entry.advisory, preset.presetId).toContain('$32=1');
      if (preset.baud === 460800) expect(entry.advisory, preset.presetId).toContain('460800');
      if (preset.constantPower) expect(entry.advisory, preset.presetId).toContain('M3');
      if (preset.airOn) {
        expect(entry.advisory, preset.presetId).toContain(preset.airOn);
        expect(entry.advisory, preset.presetId).toContain('M9');
      }
    }
  });
});
