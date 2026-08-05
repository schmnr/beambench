import { describe, expect, it } from 'vitest';
import { makeProject, makeProjectObject } from '../test-utils/projectFixtures';
import { selectSimilarObjectIds } from './selectionSimilar';

const bounds = (x: number, y: number, width: number, height: number) => ({
  min: { x, y }, max: { x: x + width, y: y + height },
});

describe('selectSimilarObjectIds', () => {
  const objects = [
    makeProjectObject({ id: 'a', layer_id: 'layer-1', bounds: bounds(0, 0, 10, 10), data: { type: 'shape', kind: 'ellipse', width: 10, height: 10, corner_radius: 0 } }),
    makeProjectObject({ id: 'b', layer_id: 'layer-1', bounds: bounds(20, 0, 10, 10), data: { type: 'shape', kind: 'ellipse', width: 10, height: 10, corner_radius: 0 } }),
    makeProjectObject({ id: 'c', layer_id: 'layer-2', bounds: bounds(40, 0, 20, 10), data: { type: 'shape', kind: 'rectangle', width: 20, height: 10, corner_radius: 0 } }),
  ];
  const project = makeProject({
    objects,
    layers: [
      { id: 'layer-1', name: 'Line', entries: [], enabled: true, order_index: 0, color_tag: '#000', visible: true, is_tool_layer: false },
      { id: 'layer-2', name: 'Fill', entries: [], enabled: true, order_index: 1, color_tag: '#f00', visible: true, is_tool_layer: false },
    ],
  });

  it('matches layer, type, size, and circle diameter from the anchor', () => {
    expect(selectSimilarObjectIds(project, 'a', 'layer')).toEqual(['a', 'b']);
    expect(selectSimilarObjectIds(project, 'a', 'type')).toEqual(['a', 'b']);
    expect(selectSimilarObjectIds(project, 'a', 'size')).toEqual(['a', 'b']);
    expect(selectSimilarObjectIds(project, 'a', 'circle_diameter')).toEqual(['a', 'b']);
  });
});
