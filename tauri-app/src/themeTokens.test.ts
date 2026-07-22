import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const themeCss = readFileSync('src/index.css', 'utf8');

type Rgb = readonly [number, number, number];

function themeVariables(theme: 'dark' | 'light'): Record<string, string> {
  const selector = theme === 'dark'
    ? /:root,\s*html\[data-theme='dark'\]\s*\{([^}]*)\}/
    : /html\[data-theme='light'\]\s*\{([^}]*)\}/;
  const block = themeCss.match(selector)?.[1];
  if (!block) throw new Error(`Missing ${theme} theme block`);

  return Object.fromEntries(
    [...block.matchAll(/--([\w-]+):\s*([^;]+);/g)].map((match) => [match[1], match[2].trim()]),
  );
}

function rgb(variables: Record<string, string>, token: string): Rgb {
  const value = variables[token];
  if (!value) throw new Error(`Missing --${token}`);
  const channels = value.split(/\s+/).map(Number);
  if (channels.length !== 3 || channels.some((channel) => !Number.isFinite(channel))) {
    throw new Error(`--${token} is not an RGB channel triplet: ${value}`);
  }
  return channels as unknown as Rgb;
}

function relativeLuminance(color: Rgb): number {
  const [red, green, blue] = color.map((channel) => {
    const normalized = channel / 255;
    return normalized <= 0.04045
      ? normalized / 12.92
      : ((normalized + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
}

function contrastRatio(foreground: Rgb, background: Rgb): number {
  const foregroundLuminance = relativeLuminance(foreground);
  const backgroundLuminance = relativeLuminance(background);
  const lighter = Math.max(foregroundLuminance, backgroundLuminance);
  const darker = Math.min(foregroundLuminance, backgroundLuminance);
  return (lighter + 0.05) / (darker + 0.05);
}

describe('theme tokens', () => {
  const dark = themeVariables('dark');
  const light = themeVariables('light');

  it('locks the approved dark palette', () => {
    expect(Object.fromEntries([
      'bb-bg',
      'bb-bg-alt',
      'bb-input',
      'bb-panel',
      'bb-panel-header',
      'bb-surface',
      'bb-surface-elevated',
      'bb-border',
      'bb-hover',
      'bb-accent',
      'bb-accent-hover',
      'bb-error',
      'bb-error-hover',
      'bb-warning',
      'bb-text',
      'bb-text-muted',
      'bb-text-dim',
      'bb-text-disabled',
    ].map((token) => [token, dark[token]]))).toEqual({
      'bb-bg': '19 20 23',
      'bb-bg-alt': '21 22 26',
      'bb-input': '26 28 32',
      'bb-panel': '14 15 17',
      'bb-panel-header': '26 28 32',
      'bb-surface': '21 22 26',
      'bb-surface-elevated': '17 18 22',
      'bb-border': '34 36 42',
      'bb-hover': '31 34 40',
      'bb-accent': '45 212 222',
      'bb-accent-hover': '34 190 200',
      'bb-error': '220 38 38',
      'bb-error-hover': '239 68 68',
      'bb-warning': '245 158 11',
      'bb-text': '232 234 237',
      'bb-text-muted': '156 163 175',
      'bb-text-dim': '107 114 128',
      'bb-text-disabled': '75 85 99',
    });
  });

  it('locks the approved light palette', () => {
    expect(light).toMatchObject({
      'bb-bg': '243 245 247',
      'bb-input': '255 255 255',
      'bb-panel': '248 250 252',
      'bb-surface': '255 255 255',
      'bb-surface-2': '238 242 246',
      'bb-surface-3': '226 232 240',
      'bb-border': '200 208 218',
      'bb-control-border': '131 143 157',
      'bb-hover': '227 232 238',
      'bb-accent': '0 115 148',
      'bb-accent-hover': '0 103 133',
      'bb-on-accent': '255 255 255',
      'bb-text': '24 33 43',
      'bb-text-muted': '79 95 112',
      'bb-text-dim': '87 104 122',
      'bb-error-fg': '180 35 24',
      'bb-error-bg': '254 205 202',
      'bb-warning-fg': '181 71 8',
      'bb-warning-bg': '254 240 199',
      'bb-success-fg': '6 118 71',
      'bb-success-bg': '209 250 223',
      'bb-info-fg': '23 92 211',
      'bb-info-bg': '209 233 255',
    });
  });

  it('keeps approved text and status pairs at 4.5:1 or better', () => {
    const lightTextPairs: ReadonlyArray<readonly [string, string]> = [
      ...['bb-bg', 'bb-panel', 'bb-surface', 'bb-surface-2', 'bb-surface-3', 'bb-hover'].flatMap(
        (background) => [
          ['bb-text', background] as const,
          ['bb-text-muted', background] as const,
          ['bb-text-dim', background] as const,
        ],
      ),
      ['bb-accent', 'bb-bg'],
      ['bb-accent', 'bb-panel'],
      ['bb-accent', 'bb-surface'],
      ['bb-on-accent', 'bb-accent'],
      ['bb-on-error', 'bb-error'],
      ['bb-on-warning', 'bb-warning'],
      ['bb-on-success', 'bb-success'],
      ['bb-error-fg', 'bb-error-bg'],
      ['bb-warning-fg', 'bb-warning-bg'],
      ['bb-success-fg', 'bb-success-bg'],
      ['bb-info-fg', 'bb-info-bg'],
    ];
    const darkSemanticPairs: ReadonlyArray<readonly [string, string]> = [
      ['bb-on-accent', 'bb-accent'],
      ['bb-on-error', 'bb-error'],
      ['bb-on-warning', 'bb-warning'],
      ['bb-on-success', 'bb-success'],
      ['bb-error-fg', 'bb-error-bg'],
      ['bb-warning-fg', 'bb-warning-bg'],
      ['bb-success-fg', 'bb-success-bg'],
      ['bb-info-fg', 'bb-info-bg'],
    ];

    for (const [foreground, background] of lightTextPairs) {
      expect(
        contrastRatio(rgb(light, foreground), rgb(light, background)),
        `light --${foreground} on --${background}`,
      ).toBeGreaterThanOrEqual(4.5);
    }
    for (const [foreground, background] of darkSemanticPairs) {
      expect(
        contrastRatio(rgb(dark, foreground), rgb(dark, background)),
        `dark --${foreground} on --${background}`,
      ).toBeGreaterThanOrEqual(4.5);
    }
  });

  it('keeps control boundaries and focus indicators at 3:1 or better', () => {
    const boundaryPairs: ReadonlyArray<readonly [string, string]> = [
      ['bb-control-border', 'bb-bg'],
      ['bb-control-border', 'bb-input'],
      ['bb-control-border', 'bb-panel'],
      ['bb-control-border', 'bb-surface'],
      ['bb-accent', 'bb-bg'],
      ['bb-accent', 'bb-input'],
      ['bb-accent', 'bb-panel'],
      ['bb-accent', 'bb-surface'],
    ];

    // Structural --bb-border is intentionally not a control/focus boundary.
    expect(boundaryPairs.some(([foreground]) => foreground === 'bb-border')).toBe(false);
    for (const variables of [dark, light]) {
      for (const [foreground, background] of boundaryPairs) {
        expect(contrastRatio(rgb(variables, foreground), rgb(variables, background))).toBeGreaterThanOrEqual(3);
      }
    }
  });
});

const INTENTIONAL_RAW_COLORS: Readonly<Record<string, readonly string[]>> = {
  './App.tsx': ['bg-black/20'],
  './components/dialogs/AdjustImageDialog.tsx': ['bg-black/20', 'bg-black/50'],
  './components/dialogs/BarcodeDialog.tsx': ['bg-black/50'],
  './components/dialogs/BooleanAssistantDialog.tsx': ['bg-black/50'],
  './components/dialogs/CameraAlignmentDialog.tsx': ['bg-black/50', 'bg-black/60', 'border-white'],
  './components/dialogs/CameraCalibrationDialog.tsx': ['bg-black/50'],
  './components/dialogs/CircularArrayDialog.tsx': ['bg-black/50'],
  './components/dialogs/CloseSelectedPathsWithToleranceDialog.tsx': ['bg-black/50'],
  './components/dialogs/CopyAlongPathDialog.tsx': ['bg-black/50'],
  './components/dialogs/DockDialog.tsx': ['bg-black/50'],
  './components/dialogs/FeedbackReportDialog.tsx': ['bg-black/35'],
  './components/dialogs/GridArrayDialog.tsx': ['bg-black/50'],
  './components/dialogs/NestDialog.tsx': ['bg-black/50'],
  './components/dialogs/NotesDialog.tsx': ['bg-black/50'],
  './components/dialogs/OffsetDialog.tsx': ['bg-black/50'],
  './components/dialogs/PreviewWindow.tsx': ['bg-black/40', 'bg-black/50'],
  './components/dialogs/ResizeSlotsDialog.tsx': ['bg-black/50'],
  './components/dialogs/TraceImageDialog.tsx': ['bg-black/30', 'bg-black/50'],
  './components/layers/CutSettingsEditor.tsx': ['bg-black/50'],
  './components/layers/DitherSamplePreview.tsx': ['bg-white'],
  './components/layers/LayerList.tsx': ['bg-green-500', 'bg-white', 'bg-yellow-500'],
  './components/layout/ColorPalette.tsx': ['border-white'],
  './components/layout/MenuBar.tsx': ['bg-black/50'],
  './components/layout/PropertiesToolbar.tsx': ['bg-white', 'border-yellow-500', 'text-yellow-500'],
  './components/layout/StatusBar.tsx': ['bg-gray-500', 'bg-green-500', 'bg-red-500', 'bg-yellow-500'],
  './components/machine/CameraWindow.tsx': ['bg-black/50'],
  './components/machine/LaserPanel.tsx': [
    'from-gray-500', 'from-green-500', 'from-red-500', 'from-yellow-500',
    'to-gray-700', 'to-green-700', 'to-red-700', 'to-yellow-700',
  ],
  './components/machine/MovePanel.tsx': ['bg-amber-500', 'bg-green-500', 'bg-red-500/5', 'border-red-500/50'],
  './components/machine/PreflightDialog.tsx': ['bg-black/50'],
  './components/panels/ArtLibraryPanel.tsx': ['bg-black/50', 'bg-white'],
  './components/settings/AboutDialog.tsx': ['bg-black/50'],
  './components/settings/RecoveryDialog.tsx': ['bg-black/50'],
  './components/settings/SettingsDialog.tsx': ['bg-white'],
  './components/settings/UpdateDialog.tsx': ['bg-black/50'],
  './components/shared/MovableResizableDialogFrame.tsx': ['bg-black/20'],
  './components/welcome/WelcomeDialog.tsx': ['bg-black/50'],
};

describe('raw UI colors', () => {
  it('requires every production raw Tailwind color to be explicitly classified', () => {
    const sourceFiles = import.meta.glob('./**/*.{ts,tsx}', {
      eager: true,
      import: 'default',
      query: '?raw',
    }) as Record<string, string>;
    const rawColor = /\b(?:(?:text|bg|border|ring|outline|divide|placeholder|from|via|to)-(?:slate|gray|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose)-(?:[0-9]{2,3})(?:\/[0-9]+)?|(?:text|bg|border)-(?:white|black)(?:\/[0-9]+)?|(?:bg|text|border)-\[#[0-9a-fA-F]{3,8}\])\b/g;
    const violations: string[] = [];

    for (const [path, source] of Object.entries(sourceFiles)) {
      if (path.includes('/__tests__/') || /\.test\.[jt]sx?$/.test(path)) continue;
      for (const match of source.matchAll(rawColor)) {
        const token = match[0];
        if (!INTENTIONAL_RAW_COLORS[path]?.includes(token)) {
          const line = source.slice(0, match.index).split('\n').length;
          violations.push(`${path}:${line} ${token}`);
        }
      }
    }

    expect(violations).toEqual([]);
  });
});
