import { describe, expect, it } from 'vitest';
import { applyRasterBitmapTransform } from '../drawPreview';
import { plannerStripPointToWorld } from '../rasterTransform';
import { worldToScreen } from '../ViewportTransform';
import type { Point2D } from '../../types/project';

describe('raster bitmap placement', () => {
  for (const yAxis of ['up', 'down'] as const) {
    for (const zoom of [10, 400]) {
      for (const angle of [0, 90, 180, 270, 360, 45, -30]) {
        for (const scanAxis of ['horizontal', 'vertical'] as const) {
          it(`matches planner run placement at ${angle}°, ${scanAxis}, Y ${yAxis}, zoom ${zoom}`, () => {
            const transforms: ((point: Point2D) => Point2D)[] = [];
            const ctx = {
              translate: (x: number, y: number) => transforms.push((p) => ({ x: p.x + x, y: p.y + y })),
              scale: (x: number, y: number) => transforms.push((p) => ({ x: p.x * x, y: p.y * y })),
              rotate: (r: number) => transforms.push((p) => ({
                x: p.x * Math.cos(r) - p.y * Math.sin(r),
                y: p.x * Math.sin(r) + p.y * Math.cos(r),
              })),
              transform: (a: number, b: number, c: number, d: number, e: number, f: number) =>
                transforms.push((p) => ({ x: a * p.x + c * p.y + e, y: b * p.x + d * p.y + f })),
            } as unknown as CanvasRenderingContext2D;
            const origin = { x: 63.77, y: -82.73 };
            const viewport = { offset: { x: 100, y: 100 }, zoom, canvasWidth: 800, canvasHeight: 600, yAxis };
            applyRasterBitmapTransform(ctx, origin, angle, scanAxis === 'vertical', viewport);
            for (const point of [{ x: 87.77, y: 86.77 }, { x: 95.77, y: 94.77 }]) {
              const actual = transforms.reduceRight((p, transform) => transform(p), point);
              const expected = worldToScreen(plannerStripPointToWorld(point.y, point.x, {
                scanAxis, scanAngleDeg: angle, scanOrigin: origin,
              }), viewport);
              expect(actual.x).toBeCloseTo(expected.x, 8);
              expect(actual.y).toBeCloseTo(expected.y, 8);
            }
          });
        }
      }
    }
  }
});
