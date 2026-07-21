import { describe, expect, it } from 'vitest';
import { resolveCanvasPointerSnap } from './pointerSnap';
import { makeLayer, makeProject, makeProjectObject } from '../test-utils/projectFixtures';

describe('resolveCanvasPointerSnap', () => {
  const project = makeProject({
    workspace: { bed_width_mm: 400, bed_height_mm: 300, origin: 'top_left' },
    layers: [
      makeLayer({
        id: 'layer1',
        enabled: true,
        visible: true,
        operation: 'line',
      }),
    ],
    objects: [
      makeProjectObject({
        id: 'snap-target',
        layer_id: 'layer1',
        bounds: { min: { x: 20, y: 20 }, max: { x: 40, y: 40 } },
        data: {
          type: 'shape',
          kind: 'rectangle',
          width: 20,
          height: 20,
          corner_radius: 0,
        },
      }),
    ],
  });

  it('ctrl disables grid and geometry snapping at the Canvas event layer', () => {
    const result = resolveCanvasPointerSnap({
      world: { x: 19.9, y: 20.1 },
      ctrlKey: true,
      altKey: false,
      project,
      zoom: 100,
      snapEnabled: true,
      gridVisible: true,
      effectiveSnapSpacing: 10,
      snapToObjects: true,
      snapThresholdPx: 5,
      preferredTargetKey: 'point:center:snap-target:30:30',
    });

    expect(result.snapped).toEqual({ x: 19.9, y: 20.1 });
    expect(result.nextPreferredTargetKey).toBeNull();
  });

  it('passes Alt-held preferred target memory through the Canvas snap resolver', () => {
    const lineProject = makeProject({
      workspace: { bed_width_mm: 400, bed_height_mm: 300, origin: 'top_left' },
      layers: [
        makeLayer({
          id: 'layer1',
          enabled: true,
          visible: true,
          operation: 'line',
        }),
      ],
      objects: [
        makeProjectObject({
          id: 'line',
          layer_id: 'layer1',
          bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 0 } },
          data: {
            type: 'vector_path',
            path_data: 'M 0 0 L 10 0',
            closed: false,
          },
        }),
      ],
    });

    const result = resolveCanvasPointerSnap({
      world: { x: 2, y: 1.9 },
      ctrlKey: false,
      altKey: true,
      project: lineProject,
      zoom: 100,
      snapEnabled: false,
      gridVisible: false,
      effectiveSnapSpacing: 10,
      snapToObjects: false,
      snapThresholdPx: 5,
      preferredTargetKey: 'line:seg:0',
    });

    expect(result.nextPreferredTargetKey).toBe('line:seg:0');
    expect(result.snapped.x).toBeCloseTo(2);
    expect(result.snapped.y).toBeCloseTo(0);
  });

  it('ignores nearby objects on disabled layers after indexed candidate retrieval', () => {
    const layeredProject = makeProject({
      workspace: { bed_width_mm: 400, bed_height_mm: 300, origin: 'top_left' },
      layers: [
        makeLayer({ id: 'enabled', enabled: true, visible: true, operation: 'line' }),
        makeLayer({ id: 'disabled', enabled: false, visible: true, operation: 'line' }),
      ],
      objects: [
        makeProjectObject({
          id: 'enabled-target',
          layer_id: 'enabled',
          bounds: { min: { x: 20, y: 20 }, max: { x: 40, y: 40 } },
          data: {
            type: 'shape',
            kind: 'rectangle',
            width: 20,
            height: 20,
            corner_radius: 0,
          },
        }),
        makeProjectObject({
          id: 'disabled-target',
          layer_id: 'disabled',
          bounds: { min: { x: 20.5, y: 20.5 }, max: { x: 40.5, y: 40.5 } },
          data: {
            type: 'shape',
            kind: 'rectangle',
            width: 20,
            height: 20,
            corner_radius: 0,
          },
        }),
      ],
    });

    const result = resolveCanvasPointerSnap({
      world: { x: 20.3, y: 20.3 },
      ctrlKey: false,
      altKey: false,
      project: layeredProject,
      zoom: 100,
      snapEnabled: false,
      gridVisible: false,
      effectiveSnapSpacing: 10,
      snapToObjects: true,
      snapThresholdPx: 10,
    });

    expect(result.snapped).toEqual({ x: 20, y: 20 });
  });
});
