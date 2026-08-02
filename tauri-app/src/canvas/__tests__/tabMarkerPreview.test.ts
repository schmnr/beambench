import { describe, expect, it } from 'vitest';
import type { Transform2D } from '../../types/project';
import { remapPersistentTabMarker, type PersistentTabMarkerSnapshot } from '../tabMarkerPreview';

const identity: Transform2D = { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 };

function marker(overrides: Partial<PersistentTabMarkerSnapshot> = {}): PersistentTabMarkerSnapshot {
  return {
    worldX: 10,
    worldY: 5,
    objectId: 'object-1',
    resolvedBounds: { min: { x: 0, y: 0 }, max: { x: 20, y: 10 } },
    resolvedTransform: identity,
    ...overrides,
  };
}

describe('remapPersistentTabMarker', () => {
  it('moves a marker with the live object bounds', () => {
    expect(remapPersistentTabMarker(
      marker(),
      { min: { x: 30, y: 40 }, max: { x: 50, y: 50 } },
      identity,
    )).toEqual({ worldX: 40, worldY: 45 });
  });

  it('scales the marker position with a live resize', () => {
    expect(remapPersistentTabMarker(
      marker({ worldX: 5, worldY: 2.5 }),
      { min: { x: 0, y: 0 }, max: { x: 40, y: 20 } },
      identity,
    )).toEqual({ worldX: 10, worldY: 5 });
  });

  it('follows a live rotation around the current bounds center', () => {
    const quarterTurn: Transform2D = { a: 0, b: 1, c: -1, d: 0, tx: 0, ty: 0 };
    const result = remapPersistentTabMarker(
      marker({ worldX: 20, worldY: 5 }),
      { min: { x: 0, y: 0 }, max: { x: 20, y: 10 } },
      quarterTurn,
    );
    expect(result.worldX).toBeCloseTo(10);
    expect(result.worldY).toBeCloseTo(15);
  });
});
