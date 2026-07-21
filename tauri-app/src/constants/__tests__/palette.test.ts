import { describe, it, expect } from 'vitest';
import { PALETTE_COLORS } from '../palette';

type Lab = [number, number, number];

const EXPECTED_PALETTE_COLORS = [
  { hex: '#000000', name: 'Black', is_tool_layer: false },
  { hex: '#FF0000', name: 'Red', is_tool_layer: false },
  { hex: '#00FF00', name: 'Green', is_tool_layer: false },
  { hex: '#0000FF', name: 'Blue', is_tool_layer: false },
  { hex: '#00FFFF', name: 'Cyan', is_tool_layer: false },
  { hex: '#FF00FF', name: 'Magenta', is_tool_layer: false },
  { hex: '#FFFF00', name: 'Yellow', is_tool_layer: false },
  { hex: '#FF8000', name: 'Orange', is_tool_layer: false },
  { hex: '#FBB6F0', name: 'Lilac', is_tool_layer: false },
  { hex: '#2EB88A', name: 'Sea Green', is_tool_layer: false },
  { hex: '#FF0080', name: 'Pink', is_tool_layer: false },
  { hex: '#93B946', name: 'Moss', is_tool_layer: false },
  { hex: '#0080FF', name: 'Sky Blue', is_tool_layer: false },
  { hex: '#804000', name: 'Brown', is_tool_layer: false },
  { hex: '#800000', name: 'Maroon', is_tool_layer: false },
  { hex: '#008000', name: 'Dark Green', is_tool_layer: false },
  { hex: '#000080', name: 'Navy', is_tool_layer: false },
  { hex: '#808000', name: 'Olive', is_tool_layer: false },
  { hex: '#008080', name: 'Dark Cyan', is_tool_layer: false },
  { hex: '#800080', name: 'Dark Magenta', is_tool_layer: false },
  { hex: '#FF8080', name: 'Coral', is_tool_layer: false },
  { hex: '#D1F0C2', name: 'Pale Green', is_tool_layer: false },
  { hex: '#987ECE', name: 'Violet', is_tool_layer: false },
  { hex: '#EFCF8F', name: 'Sand', is_tool_layer: false },
  { hex: '#314C81', name: 'Steel Blue', is_tool_layer: false },
  { hex: '#5C2336', name: 'Plum', is_tool_layer: false },
  { hex: '#808080', name: 'Gray', is_tool_layer: false },
  { hex: '#C0C0C0', name: 'Light Gray', is_tool_layer: false },
  { hex: '#404040', name: 'Dark Gray', is_tool_layer: false },
  { hex: '#B8860B', name: 'Gold', is_tool_layer: false },
  { hex: '#DA0B3F', name: 'Tool 1', is_tool_layer: true },
  { hex: '#00D4FF', name: 'Tool 2', is_tool_layer: true },
] as const;

function parseHex(hex: string): [number, number, number] {
  return [
    parseInt(hex.slice(1, 3), 16),
    parseInt(hex.slice(3, 5), 16),
    parseInt(hex.slice(5, 7), 16),
  ];
}

function srgbToLinear(value: number): number {
  const normalized = value / 255;
  return normalized <= 0.04045
    ? normalized / 12.92
    : ((normalized + 0.055) / 1.055) ** 2.4;
}

function hexToLab(hex: string): Lab {
  const [r, g, b] = parseHex(hex).map(srgbToLinear);
  const x = r * 0.4124564 + g * 0.3575761 + b * 0.1804375;
  const y = r * 0.2126729 + g * 0.7151522 + b * 0.0721750;
  const z = r * 0.0193339 + g * 0.1191920 + b * 0.9503041;

  const f = (t: number): number => (t > 0.008856 ? Math.cbrt(t) : 7.787 * t + 16 / 116);
  const fx = f(x / 0.95047);
  const fy = f(y);
  const fz = f(z / 1.08883);
  return [116 * fy - 16, 500 * (fx - fy), 200 * (fy - fz)];
}

function ciede2000(lab1: Lab, lab2: Lab): number {
  const [l1, a1, b1] = lab1;
  const [l2, a2, b2] = lab2;
  const c1 = Math.hypot(a1, b1);
  const c2 = Math.hypot(a2, b2);
  const cBar = (c1 + c2) / 2;
  const g = 0.5 * (1 - Math.sqrt(cBar ** 7 / (cBar ** 7 + 25 ** 7)));
  const a1Prime = (1 + g) * a1;
  const a2Prime = (1 + g) * a2;
  const c1Prime = Math.hypot(a1Prime, b1);
  const c2Prime = Math.hypot(a2Prime, b2);
  const h1Prime = (Math.atan2(b1, a1Prime) * 180 / Math.PI + 360) % 360;
  const h2Prime = (Math.atan2(b2, a2Prime) * 180 / Math.PI + 360) % 360;
  const deltaLPrime = l2 - l1;
  const deltaCPrime = c2Prime - c1Prime;

  let deltaHPrime = h2Prime - h1Prime;
  if (c1Prime * c2Prime === 0) {
    deltaHPrime = 0;
  } else if (deltaHPrime > 180) {
    deltaHPrime -= 360;
  } else if (deltaHPrime < -180) {
    deltaHPrime += 360;
  }

  const deltaBigHPrime = 2 * Math.sqrt(c1Prime * c2Prime) * Math.sin(deltaHPrime * Math.PI / 360);
  const lBarPrime = (l1 + l2) / 2;
  const cBarPrime = (c1Prime + c2Prime) / 2;
  let hBarPrime: number;
  if (c1Prime * c2Prime === 0) {
    hBarPrime = h1Prime + h2Prime;
  } else if (Math.abs(h1Prime - h2Prime) > 180) {
    hBarPrime = (h1Prime + h2Prime + 360) / 2;
  } else {
    hBarPrime = (h1Prime + h2Prime) / 2;
  }
  if (hBarPrime >= 360) hBarPrime -= 360;

  const t = 1
    - 0.17 * Math.cos((hBarPrime - 30) * Math.PI / 180)
    + 0.24 * Math.cos(2 * hBarPrime * Math.PI / 180)
    + 0.32 * Math.cos((3 * hBarPrime + 6) * Math.PI / 180)
    - 0.20 * Math.cos((4 * hBarPrime - 63) * Math.PI / 180);
  const deltaTheta = 30 * Math.exp(-(((hBarPrime - 275) / 25) ** 2));
  const rC = 2 * Math.sqrt(cBarPrime ** 7 / (cBarPrime ** 7 + 25 ** 7));
  const sL = 1 + (0.015 * ((lBarPrime - 50) ** 2)) / Math.sqrt(20 + ((lBarPrime - 50) ** 2));
  const sC = 1 + 0.045 * cBarPrime;
  const sH = 1 + 0.015 * cBarPrime * t;
  const rT = -Math.sin(2 * deltaTheta * Math.PI / 180) * rC;
  const lTerm = deltaLPrime / sL;
  const cTerm = deltaCPrime / sC;
  const hTerm = deltaBigHPrime / sH;
  return Math.sqrt(lTerm ** 2 + cTerm ** 2 + hTerm ** 2 + rT * cTerm * hTerm);
}

describe('PALETTE_COLORS', () => {
  it('matches the shared palette metadata order exactly', () => {
    // is_tool_layer is non-optional on every entry.
    expect(PALETTE_COLORS).toEqual(EXPECTED_PALETTE_COLORS);
  });

  it('has exactly 32 entries', () => {
    expect(PALETTE_COLORS).toHaveLength(32);
  });

  it('has exactly 2 tool layers', () => {
    const toolLayers = PALETTE_COLORS.filter((c) => c.is_tool_layer);
    expect(toolLayers).toHaveLength(2);
  });

  it('all hex values match #RRGGBB format', () => {
    const hexRegex = /^#[0-9A-Fa-f]{6}$/;
    for (const color of PALETTE_COLORS) {
      expect(color.hex).toMatch(hexRegex);
    }
  });

  it('all hex values are unique', () => {
    expect(new Set(PALETTE_COLORS.map((color) => color.hex)).size).toBe(PALETTE_COLORS.length);
  });

  it('ciede2000 helper matches a Sharma reference pair', () => {
    expect(ciede2000([50, 2.6772, -79.7751], [50, 0, -82.7485])).toBeCloseTo(2.0425, 4);
  });

  it('palette colors are separated by at least 15 CIEDE2000', () => {
    const threshold = 15;
    for (let i = 0; i < PALETTE_COLORS.length; i++) {
      for (let j = i + 1; j < PALETTE_COLORS.length; j++) {
        const a = PALETTE_COLORS[i];
        const b = PALETTE_COLORS[j];
        const distance = ciede2000(hexToLab(a.hex), hexToLab(b.hex));
        expect(distance, `${a.name} ${a.hex} vs ${b.name} ${b.hex}`).toBeGreaterThanOrEqual(threshold);
      }
    }
  });

  it('tool colors are separated from standards and other tools', () => {
    const threshold = 15;
    const tools = PALETTE_COLORS.filter((color) => color.is_tool_layer);
    const standards = PALETTE_COLORS.filter((color) => !color.is_tool_layer);

    for (const tool of tools) {
      for (const standard of standards) {
        const distance = ciede2000(hexToLab(tool.hex), hexToLab(standard.hex));
        expect(distance, `${tool.name} ${tool.hex} vs ${standard.name} ${standard.hex}`).toBeGreaterThanOrEqual(threshold);
      }
    }

    for (let i = 0; i < tools.length; i++) {
      for (let j = i + 1; j < tools.length; j++) {
        const distance = ciede2000(hexToLab(tools[i].hex), hexToLab(tools[j].hex));
        expect(distance, `${tools[i].name} ${tools[i].hex} vs ${tools[j].name} ${tools[j].hex}`).toBeGreaterThanOrEqual(threshold);
      }
    }
  });
});
