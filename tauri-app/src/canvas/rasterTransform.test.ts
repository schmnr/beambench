import { describe, it, expect } from 'vitest';
import { plannerStripPointToWorld, stripPointToWorld, scanVector } from './rasterTransform';

describe('stripPointToWorld', () => {
  it('returns direct coordinates at 0 degrees', () => {
    const result = stripPointToWorld(10, 5, 0, { x: 0, y: 0 });
    expect(result.x).toBeCloseTo(5);
    expect(result.y).toBeCloseTo(10);
  });

  it('rotates around origin at 90 degrees', () => {
    const result = stripPointToWorld(0, 10, 90, { x: 0, y: 0 });
    expect(result.x).toBeCloseTo(0);
    expect(result.y).toBeCloseTo(10);
  });

  it('rotates around scan origin at 45 degrees', () => {
    const origin = { x: 10, y: 10 };
    const result = stripPointToWorld(0, 5, 45, origin);
    // cos(45) ~= 0.707, sin(45) ~= 0.707
    expect(result.x).toBeCloseTo(10 + 5 * Math.cos(Math.PI / 4));
    expect(result.y).toBeCloseTo(10 + 5 * Math.sin(Math.PI / 4));
  });

  it('fast path at 360 degrees returns direct coordinates', () => {
    const result = stripPointToWorld(10, 5, 360, { x: 0, y: 0 });
    expect(result.x).toBeCloseTo(5);
    expect(result.y).toBeCloseTo(10);
  });

  it('rotates with non-zero yMm at 90 degrees', () => {
    // At 90 degrees: x' = ox + xVal*cos(90) - yMm*sin(90) = ox - yMm
    //                y' = oy + xVal*sin(90) + yMm*cos(90) = oy + xVal
    const result = stripPointToWorld(5, 10, 90, { x: 0, y: 0 });
    expect(result.x).toBeCloseTo(-5);
    expect(result.y).toBeCloseTo(10);
  });

  it('handles negative scan angle', () => {
    const result = stripPointToWorld(0, 10, -45, { x: 0, y: 0 });
    // cos(-45) ~= 0.707, sin(-45) ~= -0.707
    expect(result.x).toBeCloseTo(10 * Math.cos(-Math.PI / 4));
    expect(result.y).toBeCloseTo(10 * Math.sin(-Math.PI / 4));
  });
});

describe('scanVector', () => {
  it('returns (1, 0) at 0 degrees', () => {
    const v = scanVector(0);
    expect(v.x).toBeCloseTo(1);
    expect(v.y).toBeCloseTo(0);
  });

  it('returns (0, 1) at 90 degrees', () => {
    const v = scanVector(90);
    expect(v.x).toBeCloseTo(0);
    expect(v.y).toBeCloseTo(1);
  });

  it('returns (sqrt2/2, sqrt2/2) at 45 degrees', () => {
    const v = scanVector(45);
    expect(v.x).toBeCloseTo(Math.SQRT2 / 2);
    expect(v.y).toBeCloseTo(Math.SQRT2 / 2);
  });

  it('returns (-1, 0) at 180 degrees', () => {
    const v = scanVector(180);
    expect(v.x).toBeCloseTo(-1);
    expect(v.y).toBeCloseTo(0);
  });
});

describe('plannerStripPointToWorld', () => {
  it('uses vertical transpose for orthogonal vertical strips', () => {
    const result = plannerStripPointToWorld(12, 5, { scanAxis: 'vertical', scanAngleDeg: 0 });
    expect(result).toEqual({ x: 12, y: 5 });
  });

  it('rotates local strip coordinates for non-cardinal rasters', () => {
    const result = plannerStripPointToWorld(0, 10, {
      scanAxis: 'horizontal',
      scanAngleDeg: 45,
      scanOrigin: { x: 10, y: 10 },
    });
    expect(result.x).toBeCloseTo(10 + 10 * Math.cos(Math.PI / 4));
    expect(result.y).toBeCloseTo(10 + 10 * Math.sin(Math.PI / 4));
  });
});
