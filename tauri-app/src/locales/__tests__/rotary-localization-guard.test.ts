import { describe, expect, it } from 'vitest';

const locales = (
  import.meta as ImportMeta & {
    glob?: (p: string, o: { eager: true; import: 'default' }) => Record<string, unknown>;
  }
).glob!('../*.json', { eager: true, import: 'default' });

const ROTARY_COPY_KEYS = [
  'dialog.device_settings.rotary_attachment',
  'dialog.device_settings.enable_rotary',
  'dialog.device_settings.rotary_safety',
  'dialog.device_settings.rotary_type',
  'dialog.device_settings.rotary_type_roller',
  'dialog.device_settings.rotary_type_chuck',
  'dialog.device_settings.rotary_axis',
  'dialog.device_settings.rotary_axis_x',
  'dialog.device_settings.rotary_axis_y',
  'dialog.device_settings.rotary_axis_z',
  'dialog.device_settings.rotary_mm_per_rotation',
  'dialog.device_settings.rotary_roller_diameter',
  'dialog.device_settings.rotary_object_diameter',
  'dialog.device_settings.rotary_circumference',
  'dialog.device_settings.rotary_reverse',
  'dialog.device_settings.rotary_z_note',
  'status.rotary_tooltip',
  'status.rotary_active_tooltip',
];

function getString(bundle: unknown, path: string): string | undefined {
  const value = path.split('.').reduce<unknown>(
    (current, key) => (
      current && typeof current === 'object'
        ? (current as Record<string, unknown>)[key]
        : undefined
    ),
    bundle,
  );
  return typeof value === 'string' ? value : undefined;
}

describe('rotary localization guard', () => {
  const english = locales['../en.json'];

  for (const [path, bundle] of Object.entries(locales)) {
    if (path === '../en.json') continue;
    const code = path.match(/\.\.\/(.+)\.json$/)?.[1] ?? path;

    it(`${code}: rotary copy is localized`, () => {
      const copiedEnglish = ROTARY_COPY_KEYS.filter(
        (key) => getString(bundle, key) === getString(english, key),
      );
      expect(copiedEnglish, `English rotary copy left in ${code}`).toEqual([]);
    });
  }
});
