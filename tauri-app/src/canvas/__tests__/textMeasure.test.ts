import { describe, expect, it, vi, beforeAll } from 'vitest';
import { isIdentityTransform, getCaretIndexFromClick, applyInverseTransformToPoint } from '../textMeasure';
import type { ProjectObject, Transform2D } from '../../types/project';
import type { ViewportParams } from '../ViewportTransform';
import { makeProjectObject, makeTextObjectData } from '../../test-utils/projectFixtures';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const identity: Transform2D = { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 };

// Mock measureText: each character is 10px wide
const mockMeasureText = vi.fn().mockImplementation((text: string) => ({
  width: text.length * 10,
}));

beforeAll(() => {
  // Mock canvas 2D context
  HTMLCanvasElement.prototype.getContext = vi.fn().mockReturnValue({
    measureText: mockMeasureText,
    font: '',
  }) as any;
});

const defaultVp: ViewportParams = {
  offset: { x: 0, y: 0 },
  zoom: 100,
  canvasWidth: 800,
  canvasHeight: 600,
};

function makeTextObj(overrides: Record<string, any> = {}): ProjectObject {
  return makeProjectObject({
    id: 'text-1',
    name: 'Text',
    transform: { ...identity },
    bounds: { min: { x: 0, y: 0 }, max: { x: 100, y: 20 } },
    layer_id: 'layer1',
    data: makeTextObjectData({
      content: 'Hello',
      font_family: 'Arial',
      font_size_mm: 10,
      ...overrides,
    }),
  });
}

describe('isIdentityTransform', () => {
  it('returns true for identity', () => {
    expect(isIdentityTransform(identity)).toBe(true);
  });

  it('returns false for rotated', () => {
    const rotated: Transform2D = { a: 0, b: 1, c: -1, d: 0, tx: 0, ty: 0 };
    expect(isIdentityTransform(rotated)).toBe(false);
  });

  it('returns false for scaled', () => {
    const scaled: Transform2D = { a: 2, b: 0, c: 0, d: 2, tx: 0, ty: 0 };
    expect(isIdentityTransform(scaled)).toBe(false);
  });

  it('returns false for translated', () => {
    const translated: Transform2D = { a: 1, b: 0, c: 0, d: 1, tx: 5, ty: 0 };
    expect(isIdentityTransform(translated)).toBe(false);
  });
});

describe('getCaretIndexFromClick', () => {
  it('returns null for non-text objects', () => {
    const vecObj: ProjectObject = makeProjectObject({
      id: 'vec-1',
      name: 'Vec',
      transform: { ...identity },
      bounds: { min: { x: 0, y: 0 }, max: { x: 100, y: 20 } },
      layer_id: 'layer1',
      data: { type: 'vector_path', path_data: 'M 0 0 L 10 10', closed: false },
    });
    expect(getCaretIndexFromClick({ x: 50, y: 10 }, vecObj, defaultVp)).toBeNull();
  });

  it('returns null for path-layout text (on_path=true)', () => {
    const obj = makeTextObj({ on_path: true });
    expect(getCaretIndexFromClick({ x: 50, y: 10 }, obj, defaultVp)).toBeNull();
  });

  it('returns null for bend-layout text', () => {
    const obj = makeTextObj({ layout_mode: 'bend' });
    expect(getCaretIndexFromClick({ x: 50, y: 10 }, obj, defaultVp)).toBeNull();
  });

  it('returns a valid caret index for rotated text (inverse-transformed)', () => {
    // 180-degree rotation: a=-1, d=-1, b=0, c=0
    const obj = makeTextObj({ content: 'Hello' });
    obj.bounds = { min: { x: 0, y: 0 }, max: { x: 100, y: 20 } };
    obj.transform = { a: -1, b: 0, c: 0, d: -1, tx: 0, ty: 0 };
    // The inverse transform should map the click back into untransformed space
    // For 180° rotation around center (50,10): world (50,10) maps back to (50,10)
    // Click at the center of the text → should get a valid index (not null)
    const result = getCaretIndexFromClick({ x: 50, y: 10 }, obj, defaultVp);
    expect(result).not.toBeNull();
    expect(typeof result).toBe('number');
  });

  it('returns null for singular-matrix transform (det≈0)', () => {
    const obj = makeTextObj();
    // Singular matrix: a=0, b=0, c=0, d=0 → det=0
    obj.transform = { a: 0, b: 0, c: 0, d: 0, tx: 0, ty: 0 };
    expect(getCaretIndexFromClick({ x: 50, y: 10 }, obj, defaultVp)).toBeNull();
  });

  it('returns null for text with max_width > 0', () => {
    const obj = makeTextObj({ max_width: 50 });
    expect(getCaretIndexFromClick({ x: 50, y: 10 }, obj, defaultVp)).toBeNull();
  });

  it('returns 0 when clicking before first character', () => {
    const obj = makeTextObj({ content: 'Hello' });
    // Click at the very left edge of bounds (before any character)
    // With zoom=100 and BASE_PX_PER_MM=2, worldToScreen: screenX = x*2+400
    // Bounds: min.x=0 → screenX=400, each char = 10px (mock)
    // Click world x=-5 → screen = -5*2+400=390 → before sx=400
    const result = getCaretIndexFromClick({ x: -5, y: 5 }, obj, defaultVp);
    expect(result).toBe(0);
  });

  it('returns content.length when clicking past last character', () => {
    const obj = makeTextObj({ content: 'Hello' });
    // 5 chars × 10px = 50px total width. sx=400 (left aligned at x=0).
    // Past end: click world x=200 → screen=800 → well past sx+50=450
    const result = getCaretIndexFromClick({ x: 200, y: 5 }, obj, defaultVp);
    expect(result).toBe(5);
  });

  it('returns correct mid-string index for left-aligned text', () => {
    const obj = makeTextObj({ content: 'Hello' });
    // Each char is 10px wide (mock). sx=400 (bounds.min.x=0, left aligned).
    // Char midpoints: H=[400,410] mid=405, e=[410,420] mid=415, l=[420,430] mid=425...
    // Click world x=6 → screen=6*2+400=412 → past midpoint of H (405), before midpoint of e (415) → index 1
    const result = getCaretIndexFromClick({ x: 6, y: 5 }, obj, defaultVp);
    expect(result).toBe(1);
  });
});

describe('applyInverseTransformToPoint', () => {
  const bounds = { min: { x: 0, y: 0 }, max: { x: 100, y: 20 } };

  it('round-trips identity transform', () => {
    const pt = { x: 30, y: 10 };
    const result = applyInverseTransformToPoint(pt, identity, bounds);
    expect(result).not.toBeNull();
    expect(result!.x).toBeCloseTo(30, 6);
    expect(result!.y).toBeCloseTo(10, 6);
  });

  it('round-trips 90-degree rotation', () => {
    // 90° CW: a=0, b=1, c=-1, d=0
    const transform: Transform2D = { a: 0, b: 1, c: -1, d: 0, tx: 0, ty: 0 };
    // Forward: x' = a*(x-cx)+c*(y-cy)+tx+cx, y' = b*(x-cx)+d*(y-cy)+ty+cy
    // cx=50, cy=10, original=(30,5)
    // x' = 0*(30-50)+(-1)*(5-10)+0+50 = 0+5+50 = 55
    // y' = 1*(30-50)+0*(5-10)+0+10 = -20+10 = -10
    const transformed = { x: 55, y: -10 };
    const result = applyInverseTransformToPoint(transformed, transform, bounds);
    expect(result).not.toBeNull();
    expect(result!.x).toBeCloseTo(30, 6);
    expect(result!.y).toBeCloseTo(5, 6);
  });

  it('returns null for singular matrix', () => {
    const transform: Transform2D = { a: 0, b: 0, c: 0, d: 0, tx: 0, ty: 0 };
    const result = applyInverseTransformToPoint({ x: 50, y: 10 }, transform, bounds);
    expect(result).toBeNull();
  });

  it('handles translation-only transform', () => {
    const transform: Transform2D = { a: 1, b: 0, c: 0, d: 1, tx: 5, ty: -3 };
    // Forward: x' = 1*(x-cx)+0*(y-cy)+5+cx = x+5, y' = 0*(x-cx)+1*(y-cy)+(-3)+cy = y-3
    // original=(20,15) → transformed=(25,12)
    const transformed = { x: 25, y: 12 };
    const result = applyInverseTransformToPoint(transformed, transform, bounds);
    expect(result).not.toBeNull();
    expect(result!.x).toBeCloseTo(20, 6);
    expect(result!.y).toBeCloseTo(15, 6);
  });
});
