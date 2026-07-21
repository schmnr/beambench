import { describe, expect, it } from 'vitest';

// Machine state indicators are the primary "is my laser running" surfaces.
// Hardcoded hex colors ignore the light/dark theme and rendered as ~2:1
// contrast text in light mode; state colors must come from theme tokens
// (see components/machine/stateColors.ts).
type RawGlob = (
  p: string,
  o: { eager: true; query: '?raw'; import: 'default' },
) => Record<string, string>;

const sources: Record<string, string> = {
  ...(import.meta as ImportMeta & { glob: RawGlob }).glob(
    '../{StatusDisplay,JobProgressBar}.tsx',
    { eager: true, query: '?raw', import: 'default' },
  ),
  ...(import.meta as ImportMeta & { glob: RawGlob }).glob(
    '../../dialogs/DeviceSettingsDialog.tsx',
    { eager: true, query: '?raw', import: 'default' },
  ),
};

describe('machine state indicator theming', () => {
  it('covers the three state indicator components', () => {
    expect(Object.keys(sources)).toHaveLength(3);
  });

  it.each(Object.entries(sources))(
    '%s uses theme tokens instead of hardcoded hex colors',
    (_file, source) => {
      const hexes = source.match(/#[0-9a-fA-F]{3,8}\b/g) ?? [];
      expect(hexes).toEqual([]);
    },
  );
});
