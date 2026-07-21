import { describe, expect, it } from 'vitest';
import { expandSelectionMembers, normalizeArrangementSelection, normalizeSelectionMembers } from '../arrangementSelection';
import { makeLayer, makeProject, makeProjectObject } from '../../test-utils/projectFixtures';

describe('normalizeArrangementSelection', () => {
  it('excludes tool-layer and ruler-guide objects', () => {
    const project = makeProject({
      layers: [
        makeLayer({ id: 'cut', is_tool_layer: false }),
        makeLayer({ id: 'tool', is_tool_layer: true }),
      ],
      objects: [
        makeProjectObject({ id: 'shape', layer_id: 'cut' }),
        makeProjectObject({
          id: 'guide',
          layer_id: 'tool',
          data: { type: 'vector_path', path_data: 'M 0 0 L 0 10', closed: false, ruler_guide_axis: 'vertical' },
        }),
      ],
    });

    expect(normalizeArrangementSelection(project, ['shape', 'guide'])).toEqual(['shape']);
  });

  it('promotes selected group children to the top-level parent while preserving order', () => {
    const project = makeProject({
      objects: [
        makeProjectObject({ id: 'child-a' }),
        makeProjectObject({ id: 'child-b' }),
        makeProjectObject({ id: 'group-inner', data: { type: 'group', children: ['child-a'] } }),
        makeProjectObject({ id: 'group-outer', data: { type: 'group', children: ['group-inner', 'child-b'] } }),
      ],
    });

    expect(normalizeArrangementSelection(project, ['child-a', 'child-b'])).toEqual(['group-outer']);
  });
});

describe('normalizeSelectionMembers', () => {
  it('keeps tool-layer and ruler-guide objects (selection-side, unlike arrangement)', () => {
    const project = makeProject({
      layers: [
        makeLayer({ id: 'cut', is_tool_layer: false }),
        makeLayer({ id: 'tool', is_tool_layer: true }),
      ],
      objects: [
        makeProjectObject({ id: 'shape', layer_id: 'cut' }),
        makeProjectObject({ id: 'tool-shape', layer_id: 'tool' }),
        makeProjectObject({
          id: 'guide',
          layer_id: 'tool',
          data: { type: 'vector_path', path_data: 'M 0 0 L 0 10', closed: false, ruler_guide_axis: 'vertical' },
        }),
      ],
    });

    expect(normalizeSelectionMembers(project, ['shape', 'tool-shape', 'guide'])).toEqual([
      'shape',
      'tool-shape',
      'guide',
    ]);
  });

  it('still drops IDs that do not exist in the project', () => {
    const project = makeProject({
      objects: [makeProjectObject({ id: 'shape' })],
    });

    expect(normalizeSelectionMembers(project, ['shape', 'missing'])).toEqual(['shape']);
  });

  it('promotes group children to top-level parent and dedupes', () => {
    const project = makeProject({
      objects: [
        makeProjectObject({ id: 'child-a' }),
        makeProjectObject({ id: 'child-b' }),
        makeProjectObject({ id: 'group', data: { type: 'group', children: ['child-a', 'child-b'] } }),
      ],
    });

    expect(normalizeSelectionMembers(project, ['child-a', 'child-b'])).toEqual(['group']);
  });
});

describe('expandSelectionMembers', () => {
  it('keeps tool-layer and ruler-guide roots while expanding groups', () => {
    const project = makeProject({
      layers: [
        makeLayer({ id: 'cut', is_tool_layer: false }),
        makeLayer({ id: 'tool', is_tool_layer: true }),
      ],
      objects: [
        makeProjectObject({ id: 'child', layer_id: 'cut' }),
        makeProjectObject({ id: 'group', layer_id: 'cut', data: { type: 'group', children: ['child'] } }),
        makeProjectObject({
          id: 'guide',
          layer_id: 'tool',
          data: { type: 'vector_path', path_data: 'M 0 0 L 0 10', closed: false, ruler_guide_axis: 'vertical' },
        }),
      ],
    });

    expect(expandSelectionMembers(project, ['guide', 'group'])).toEqual(['guide', 'group', 'child']);
  });
});
