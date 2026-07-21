import { describe, it, expect } from 'vitest';
import {
  resolveDestinationLayer,
  AUTO_LAYER_ID,
  type ResolveInput,
} from '../layerFamilyResolver';
import type { Project, Layer } from '../../types/project';
import { makeLayer, makeProject, type LayerFixtureOverrides } from '../../test-utils/projectFixtures';

/** Minimal Layer factory with sensible defaults for resolver tests. */
function layer(overrides: LayerFixtureOverrides & { id: string }): Layer {
  return makeLayer({
    name: overrides.name ?? 'Layer',
    operation: overrides.operation ?? 'line',
    enabled: overrides.enabled ?? true,
    order_index: overrides.order_index ?? 0,
    color_tag: overrides.color_tag ?? '#000000',
    speed_mm_min: overrides.speed_mm_min ?? 1000,
    power_percent: overrides.power_percent ?? 50,
    raster_settings: overrides.raster_settings ?? null,
    vector_settings: overrides.vector_settings ?? null,
    ...overrides,
  });
}

function project(layers: Layer[]): Project {
  return makeProject({
    metadata: {
      format_version: '1.0',
      app_version: '0.1.0',
      project_id: 'p',
      project_name: 'P',
      created_at: '',
      modified_at: '',
    },
    workspace: { bed_width_mm: 400, bed_height_mm: 400, origin: 'top_left' },
    layers,
    objects: [],
    assets: [],
  });
}

function baseInput(p: Project, overrides: Partial<ResolveInput> = {}): ResolveInput {
  return {
    project: p,
    requestedLayerId: null,
    pendingColor: null,
    selectedLayerId: null,
    contentKind: 'non_raster',
    ...overrides,
  };
}

describe('resolveDestinationLayer', () => {
  describe('family lookup', () => {
    it('matches a family by color_tag case-insensitively', () => {
      const p = project([
        layer({ id: 'L1', color_tag: '#FF0000', operation: 'line' }),
      ]);
      const out = resolveDestinationLayer(baseInput(p, {
        pendingColor: '#ff0000',
        contentKind: 'non_raster',
      }));
      expect(out.kind).toBe('resolved');
      if (out.kind === 'resolved') expect(out.layerId).toBe('L1');
    });

    it('picks the image sibling for raster content within a family', () => {
      const p = project([
        layer({ id: 'V', color_tag: '#ff0000', operation: 'line' }),
        layer({ id: 'I', color_tag: '#ff0000', operation: 'image' }),
      ]);
      const out = resolveDestinationLayer(baseInput(p, {
        pendingColor: '#ff0000',
        contentKind: 'raster',
      }));
      expect(out.kind).toBe('resolved');
      if (out.kind === 'resolved') expect(out.layerId).toBe('I');
    });

    it('picks the non-image sibling for vector content within a family', () => {
      const p = project([
        layer({ id: 'V', color_tag: '#ff0000', operation: 'line' }),
        layer({ id: 'I', color_tag: '#ff0000', operation: 'image' }),
      ]);
      const out = resolveDestinationLayer(baseInput(p, {
        pendingColor: '#ff0000',
        contentKind: 'non_raster',
      }));
      expect(out.kind).toBe('resolved');
      if (out.kind === 'resolved') expect(out.layerId).toBe('V');
    });

    it('uses one canonical tool layer for raster and vector content', () => {
      const p = project([
        layer({
          id: 'T1',
          name: 'T1',
          color_tag: '#da0b3f',
          operation: 'tool',
          is_tool_layer: true,
        }),
      ]);
      const raster = resolveDestinationLayer(baseInput(p, {
        pendingColor: '#DA0B3F',
        contentKind: 'raster',
      }));
      const vector = resolveDestinationLayer(baseInput(p, {
        pendingColor: '#DA0B3F',
        contentKind: 'non_raster',
      }));

      expect(raster.kind).toBe('resolved');
      expect(vector.kind).toBe('resolved');
      if (raster.kind === 'resolved') expect(raster.layerId).toBe('T1');
      if (vector.kind === 'resolved') expect(vector.layerId).toBe('T1');
    });
  });

  describe('needs_create', () => {
    it('raster content + non-image-only family requests an image sibling', () => {
      const p = project([
        layer({ id: 'V', color_tag: '#ff0000', operation: 'line', speed_mm_min: 600, power_percent: 70 }),
      ]);
      const out = resolveDestinationLayer(baseInput(p, {
        pendingColor: '#ff0000',
        contentKind: 'raster',
      }));
      expect(out.kind).toBe('needs_create');
      if (out.kind === 'needs_create') {
        expect(out.operation).toBe('image');
        expect(out.colorTag).toBe('#ff0000');
        expect(out.copyFrom?.id).toBe('V');
        expect(out.copyFrom?.entries[0]?.speed_mm_min).toBe(600);
      }
    });

    it('non-raster content + image-only family requests a line sibling inheriting speed/power', () => {
      const p = project([
        layer({ id: 'I', color_tag: '#00ff00', operation: 'image', speed_mm_min: 400, power_percent: 30 }),
      ]);
      const out = resolveDestinationLayer(baseInput(p, {
        pendingColor: '#00ff00',
        contentKind: 'non_raster',
      }));
      expect(out.kind).toBe('needs_create');
      if (out.kind === 'needs_create') {
        expect(out.operation).toBe('line');
        expect(out.colorTag).toBe('#00ff00');
        expect(out.copyFrom?.id).toBe('I');
        expect(out.copyFrom?.entries[0]?.speed_mm_min).toBe(400);
        expect(out.copyFrom?.entries[0]?.power_percent).toBe(30);
        expect(out.suggestedName).toBe('C02 (Line)');
      }
    });

    it('non-raster content inherits the existing non-image operation (Cut family stays Cut)', () => {
      const p = project([
        layer({ id: 'C', color_tag: '#0000ff', operation: 'cut' }),
        layer({ id: 'I', color_tag: '#0000ff', operation: 'image' }),
      ]);
      // Non-raster content — existing 'cut' family member should be reused.
      const out = resolveDestinationLayer(baseInput(p, {
        pendingColor: '#0000ff',
        contentKind: 'non_raster',
      }));
      expect(out.kind).toBe('resolved');
      if (out.kind === 'resolved') expect(out.layerId).toBe('C');
    });

    it('new non-raster sibling inherits the family\'s non-image operation', () => {
      // Family has only a Score (non-image) layer + an Image layer.
      // A new non-raster request (different color to force create)...
      // Wait, the Score matches already. Instead: family has only an
      // Image layer. Request non-raster → needs_create, operation=line
      // (no non-image member to inherit from).
      const p = project([
        layer({ id: 'I', color_tag: '#0000ff', operation: 'image' }),
      ]);
      const out = resolveDestinationLayer(baseInput(p, {
        pendingColor: '#0000ff',
        contentKind: 'non_raster',
      }));
      expect(out.kind).toBe('needs_create');
      if (out.kind === 'needs_create') {
        expect(out.operation).toBe('line');
        expect(out.suggestedName).toBe('C03 (Line)');
      }
    });

    it('preserves a custom family base name when creating a sibling', () => {
      const p = project([
        layer({ id: 'I', name: 'Photo Pass', color_tag: '#ff0000', operation: 'image' }),
      ]);
      const out = resolveDestinationLayer(baseInput(p, {
        pendingColor: '#ff0000',
        contentKind: 'non_raster',
      }));
      expect(out.kind).toBe('needs_create');
      if (out.kind === 'needs_create') {
        expect(out.suggestedName).toBe('Photo Pass (Line)');
      }
    });

    it('creates a canonical tool layer without copying family settings', () => {
      const p = project([]);
      const out = resolveDestinationLayer(baseInput(p, {
        pendingColor: '#DA0B3F',
        contentKind: 'raster',
      }));

      expect(out.kind).toBe('needs_create');
      if (out.kind === 'needs_create') {
        expect(out.operation).toBe('tool');
        expect(out.colorTag).toBe('#da0b3f');
        expect(out.copyFrom).toBeNull();
        expect(out.suggestedName).toBe('T1');
      }
    });
  });

  describe('direct target short-circuit', () => {
    it('returns requestedLayerId unchanged when it already matches the content kind', () => {
      const p = project([
        layer({ id: 'L1', color_tag: '#ff0000', operation: 'line' }),
        layer({ id: 'L2', color_tag: '#00ff00', operation: 'cut' }),
      ]);
      // Explicitly target L2 (cut) with non-raster content — no reroute,
      // even though L1 also matches.
      const out = resolveDestinationLayer(baseInput(p, {
        requestedLayerId: 'L2',
        contentKind: 'non_raster',
      }));
      expect(out.kind).toBe('resolved');
      if (out.kind === 'resolved') expect(out.layerId).toBe('L2');
    });

    it('does NOT short-circuit when a pending palette color overrides the requested family', () => {
      const p = project([
        layer({ id: 'RED', color_tag: '#ff0000', operation: 'line' }),
      ]);
      const out = resolveDestinationLayer(baseInput(p, {
        requestedLayerId: 'RED',
        pendingColor: '#00ff00',
        contentKind: 'non_raster',
      }));
      expect(out.kind).toBe('needs_create');
      if (out.kind === 'needs_create') {
        expect(out.colorTag).toBe('#00ff00');
        expect(out.operation).toBe('line');
      }
    });

    it('does NOT short-circuit when the requested layer does not match the content kind', () => {
      const p = project([
        layer({ id: 'I', color_tag: '#ff0000', operation: 'image' }),
        layer({ id: 'L', color_tag: '#ff0000', operation: 'line' }),
      ]);
      // Request the image layer for vector content → resolver should
      // re-route to the line sibling in the same family.
      const out = resolveDestinationLayer(baseInput(p, {
        requestedLayerId: 'I',
        contentKind: 'non_raster',
      }));
      expect(out.kind).toBe('resolved');
      if (out.kind === 'resolved') expect(out.layerId).toBe('L');
    });
  });

  describe('color normalization', () => {
    it('matches family when one layer has 8-digit RGBA and pending is 6-digit RGB', () => {
      const p = project([
        layer({ id: 'L1', color_tag: '#ff0000ff', operation: 'line' }),
      ]);
      const out = resolveDestinationLayer(baseInput(p, {
        pendingColor: '#ff0000',
        contentKind: 'non_raster',
      }));
      expect(out.kind).toBe('resolved');
      if (out.kind === 'resolved') expect(out.layerId).toBe('L1');
    });

    it('matches family when pending is 8-digit RGBA and layer is 6-digit RGB', () => {
      const p = project([
        layer({ id: 'L1', color_tag: '#ff0000', operation: 'line' }),
      ]);
      const out = resolveDestinationLayer(baseInput(p, {
        pendingColor: '#FF0000FF',
        contentKind: 'non_raster',
      }));
      expect(out.kind).toBe('resolved');
      if (out.kind === 'resolved') expect(out.layerId).toBe('L1');
    });

    it('finds image sibling when tags differ by alpha suffix', () => {
      const p = project([
        layer({ id: 'V', color_tag: '#ff0000ff', operation: 'line' }),
        layer({ id: 'I', color_tag: '#ff0000', operation: 'image' }),
      ]);
      const out = resolveDestinationLayer(baseInput(p, {
        pendingColor: '#FF0000FF',
        contentKind: 'raster',
      }));
      expect(out.kind).toBe('resolved');
      if (out.kind === 'resolved') expect(out.layerId).toBe('I');
    });
  });

  describe('fallbacks', () => {
    it('empty project + no pending + no selection + non-raster → __auto__', () => {
      const p = project([]);
      const out = resolveDestinationLayer(baseInput(p, { contentKind: 'non_raster' }));
      expect(out.kind).toBe('resolved');
      if (out.kind === 'resolved') expect(out.layerId).toBe(AUTO_LAYER_ID);
    });

    it('empty project + no pending + no selection + raster → __auto__', () => {
      const p = project([]);
      const out = resolveDestinationLayer(baseInput(p, { contentKind: 'raster' }));
      expect(out.kind).toBe('resolved');
      if (out.kind === 'resolved') expect(out.layerId).toBe(AUTO_LAYER_ID);
    });

    it('uses selectedLayer.color_tag as target color when no pending is set', () => {
      const p = project([
        layer({ id: 'L1', color_tag: '#ff0000', operation: 'line' }),
      ]);
      const out = resolveDestinationLayer(baseInput(p, {
        selectedLayerId: 'L1',
        contentKind: 'non_raster',
      }));
      expect(out.kind).toBe('resolved');
      if (out.kind === 'resolved') expect(out.layerId).toBe('L1');
    });

    it('image-only selected + non-raster content → requests line sibling of same color', () => {
      // The "import raster, draw rectangle" flow.
      const p = project([
        layer({ id: 'I', color_tag: '#ff0000', operation: 'image', speed_mm_min: 3000, power_percent: 20 }),
      ]);
      const out = resolveDestinationLayer(baseInput(p, {
        selectedLayerId: 'I',
        contentKind: 'non_raster',
      }));
      expect(out.kind).toBe('needs_create');
      if (out.kind === 'needs_create') {
        expect(out.operation).toBe('line');
        expect(out.colorTag).toBe('#ff0000');
        expect(out.copyFrom?.entries[0]?.speed_mm_min).toBe(3000);
        expect(out.copyFrom?.entries[0]?.power_percent).toBe(20);
      }
    });
  });
});
