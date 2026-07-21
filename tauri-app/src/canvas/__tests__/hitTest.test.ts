import { describe, expect, it, vi } from 'vitest';
import type { ProjectObject, Transform2D } from '../../types/project';
import type { ViewportParams } from '../ViewportTransform';
import { makeProjectObject, makeTextObjectData } from '../../test-utils/projectFixtures';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

// Mock Path2D + isPointInPath/isPointInStroke (jsdom doesn't have them)
let mockIsPointInPath = false;
let mockIsPointInStroke = false;
const isPointInPathSpy = vi.fn(() => mockIsPointInPath);
const isPointInStrokeSpy = vi.fn(() => mockIsPointInStroke);

class MockPath2D {
  moveTo() {}
  lineTo() {}
  quadraticCurveTo() {}
  bezierCurveTo() {}
  closePath() {}
}

globalThis.Path2D = MockPath2D as any;
HTMLCanvasElement.prototype.getContext = vi.fn().mockReturnValue({
  measureText: vi.fn().mockReturnValue({ width: 10 }),
  font: '',
  lineWidth: 1,
  isPointInPath: isPointInPathSpy,
  isPointInStroke: isPointInStrokeSpy,
}) as any;

import { hitTestPoint, isPointInTextGlyphs } from '../hitTest';

const identity: Transform2D = { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 };

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
      font_family: 'Arial',
      ...overrides,
    }),
  });
}

describe('isPointInTextGlyphs', () => {
  it('returns true (bbox fallback) when no resolved_path_data', () => {
    const obj = makeTextObj();
    const result = isPointInTextGlyphs({ x: 400, y: 300 }, obj, defaultVp);
    expect(result).toBe(true);
  });

  it('returns false when click is outside glyphs (inside bbox)', () => {
    mockIsPointInPath = false;
    mockIsPointInStroke = false;
    const obj = makeTextObj({ resolved_path_data: 'M 0 0 L 10 0 L 10 10 L 0 10 Z' });
    // Click inside bbox but outside glyphs
    const result = isPointInTextGlyphs({ x: 500, y: 310 }, obj, defaultVp);
    expect(result).toBe(false);
  });

  it('returns true when click hits glyph fill', () => {
    mockIsPointInPath = true;
    mockIsPointInStroke = false;
    const obj = makeTextObj({ resolved_path_data: 'M 0 0 L 10 0 L 10 10 L 0 10 Z' });
    const result = isPointInTextGlyphs({ x: 401, y: 301 }, obj, defaultVp);
    expect(result).toBe(true);
  });

  it('returns true when click hits glyph stroke', () => {
    mockIsPointInPath = false;
    mockIsPointInStroke = true;
    const obj = makeTextObj({ resolved_path_data: 'M 0 0 L 10 0 L 10 10 L 0 10 Z' });
    const result = isPointInTextGlyphs({ x: 401, y: 301 }, obj, defaultVp);
    expect(result).toBe(true);
  });
});

describe('hitTestPoint glyph refinement', () => {
  it('misses text with resolved_path_data when click is in bbox but not on glyphs', () => {
    mockIsPointInPath = false;
    mockIsPointInStroke = false;
    const obj = makeTextObj({ resolved_path_data: 'M 0 0 L 10 0 L 10 10 L 0 10 Z' });
    // worldToScreen: x=50 → screen=50*2+400=500, y=10 → screen=10*2+300=320
    // This is inside the bbox (min.x=0→screen=400, max.x=100→screen=600)
    const result = hitTestPoint({ x: 450, y: 350 }, [obj], defaultVp);
    expect(result).toBeNull();
  });

  it('hits text with resolved_path_data when click is on glyph', () => {
    mockIsPointInPath = true;
    mockIsPointInStroke = false;
    const obj = makeTextObj({ resolved_path_data: 'M 0 0 L 10 0 L 10 10 L 0 10 Z' });
    const result = hitTestPoint({ x: 500, y: 320 }, [obj], defaultVp);
    expect(result).toBe(obj);
  });

  it('hits text without resolved_path_data using bbox only', () => {
    // No resolved_path_data → bbox-only hit test
    const obj = makeTextObj();
    // Click inside bbox
    const result = hitTestPoint({ x: 500, y: 320 }, [obj], defaultVp);
    expect(result).toBe(obj);
  });

  it('hits vector-path geometry when the cached path contains the point', () => {
    mockIsPointInPath = false;
    mockIsPointInStroke = true;
    isPointInPathSpy.mockClear();
    isPointInStrokeSpy.mockClear();
    const obj = makeProjectObject({
      id: 'vector-1',
      name: 'Vector',
      transform: { ...identity },
      bounds: { min: { x: 0, y: 0 }, max: { x: 100, y: 100 } },
      layer_id: 'layer1',
      data: {
        type: 'vector_path',
        path_data: 'M 0 0 L 10 0 L 10 10',
        closed: false,
      },
    });

    const result = hitTestPoint({ x: 500, y: 320 }, [obj], defaultVp);
    expect(result).toBe(obj);
  });

  it('coarsely rejects rotated heavy vectors outside the transformed bounds before exact hit-testing', () => {
    mockIsPointInPath = true;
    mockIsPointInStroke = true;
    isPointInPathSpy.mockClear();
    isPointInStrokeSpy.mockClear();
    const heavyPath = `M 0 0 ${Array.from({ length: 6000 }, (_, index) => `L ${index + 1} ${index % 11}`).join(' ')}`;
    const obj = makeProjectObject({
      id: 'vector-rotated',
      name: 'Rotated Vector',
      transform: {
        a: Math.cos(Math.PI / 4),
        b: Math.sin(Math.PI / 4),
        c: -Math.sin(Math.PI / 4),
        d: Math.cos(Math.PI / 4),
        tx: 0,
        ty: 0,
      },
      bounds: { min: { x: 0, y: 0 }, max: { x: 100, y: 100 } },
      layer_id: 'layer1',
      data: {
        type: 'vector_path',
        path_data: heavyPath,
        closed: false,
      },
    });

    const result = hitTestPoint({ x: 780, y: 20 }, [obj], defaultVp);
    expect(result).toBeNull();
    expect(isPointInPathSpy).not.toHaveBeenCalled();
    expect(isPointInStrokeSpy).not.toHaveBeenCalled();
  });

  it('still hits inside a rotated heavy vector after the coarse reject passes', () => {
    mockIsPointInPath = false;
    mockIsPointInStroke = true;
    const heavyPath = `M 0 0 ${Array.from({ length: 6000 }, (_, index) => `L ${index + 1} ${index % 7}`).join(' ')}`;
    const obj = makeProjectObject({
      id: 'vector-rotated-hit',
      name: 'Rotated Vector Hit',
      transform: {
        a: Math.cos(Math.PI / 4),
        b: Math.sin(Math.PI / 4),
        c: -Math.sin(Math.PI / 4),
        d: Math.cos(Math.PI / 4),
        tx: 0,
        ty: 0,
      },
      bounds: { min: { x: 0, y: 0 }, max: { x: 100, y: 100 } },
      layer_id: 'layer1',
      data: {
        type: 'vector_path',
        path_data: heavyPath,
        closed: false,
      },
    });

    const result = hitTestPoint({ x: 500, y: 300 }, [obj], defaultVp);
    expect(result).toBe(obj);
  });
});
