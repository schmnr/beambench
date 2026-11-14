import { describe, expect, it } from 'vitest';
import { artilumePresetCases } from '../test-utils/artilumePresets';

type Locale = {
  panels: { machine: { preset: { catalog: Record<string, { description: string; advisory: string }> } } };
};

const locales = import.meta.glob<Locale>('../locales/*.json', { eager: true, import: 'default' });

describe('Artilume preset guidance', () => {
  it.each(Object.entries(locales))('preserves model-specific setup facts in %s', (_path, locale) => {
    const catalog = locale.panels.machine.preset.catalog;
    for (const preset of artilumePresetCases) {
      const entry = catalog[preset.presetId];
      expect(entry, preset.presetId).toBeDefined();
      expect(entry.description.trim(), preset.presetId).not.toBe('');
      expect(entry.advisory, preset.presetId).toContain(`X ${preset.width} x Y ${preset.height}`);
      for (const setting of ['115200', 'S1000', '$30', '$32=1']) {
        expect(entry.advisory, preset.presetId).toContain(setting);
      }
      if (preset.homing) {
        expect(entry.advisory, preset.presetId).toContain('$22=1');
      }
    }
  });
});
