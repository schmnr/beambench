import { describe, expect, it } from 'vitest';
import type { ExecutionPlan, PlanSegment } from '../plan';

describe('types/plan contract', () => {
  it('ExecutionPlan accepts live planner segment variants and fields', () => {
    const segments: PlanSegment[] = [
      {
        type: 'vector',
        cut_entry_id: 'entry-1',
        polyline: [{ x: 0, y: 0 }, { x: 10, y: 10 }],
        closed: false,
        power_percent: 80,
        speed_mm_min: 1200,
        layer_id: 'layer-1',
        perforation_enabled: true,
        perforation_on_ms: 5,
        perforation_off_ms: 15,
        source_object_id: 'obj-1',
        source_subpath_index: 2,
      },
      {
        type: 'raster',
        cut_entry_id: 'entry-2',
        scanlines: [],
        line_interval_mm: 0.1,
        direction_mode: 'bidirectional',
        power_mode: 'grayscale',
        speed_mm_min: 1500,
        layer_id: 'layer-2',
        scan_angle_deg: 45,
        scan_origin: { x: 50, y: 25 },
        overscan_mm: 2,
        outlines: [{ points: [{ x: 0, y: 0 }, { x: 1, y: 1 }], closed: false }],
        scan_axis: 'horizontal',
        power_max_percent: 90,
        power_min_percent: 10,
        dot_width_correction_mm: 0.08,
        ramp_length_mm: 0.5,
        x_pixel_mm: 0.1,
      },
      {
        type: 'offset_fill',
        layer_id: 'layer-3',
        object_id: 'obj-3',
        offset_mm: 0.25,
        angle_deg: 30,
      },
    ];

    const plan: ExecutionPlan = {
      id: 'plan-1',
      project_id: 'project-1',
      revision_hash: 'rev-1',
      created_at: '2026-04-16T12:00:00Z',
      bounds: { min: { x: 0, y: 0 }, max: { x: 100, y: 50 } },
      total_distance_mm: 250,
      estimated_duration_secs: 35,
      segments,
      layer_order: ['layer-1', 'layer-2', 'layer-3'],
      warnings: [],
      failed_entries: [],
    };

    expect(plan.segments).toHaveLength(3);
    expect(plan.segments[2]?.type).toBe('offset_fill');
  });
});
