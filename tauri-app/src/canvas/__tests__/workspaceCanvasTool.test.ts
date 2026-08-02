import { describe, expect, it } from 'vitest';

import {
  resolveWorkspaceCanvasTool,
  tracksObjectDragInteraction,
} from '../workspaceCanvasTool';

describe('resolveWorkspaceCanvasTool', () => {
  it('keeps Design tools available but rejects Laser Position', () => {
    expect(resolveWorkspaceCanvasTool('design', 'rect')).toBe('rect');
    expect(resolveWorkspaceCanvasTool('design', 'laser_position')).toBe('select');
  });

  it('keeps Run read-only except for Laser Position', () => {
    expect(resolveWorkspaceCanvasTool('run', 'rect')).toBe('select');
    expect(resolveWorkspaceCanvasTool('run', 'select')).toBe('select');
    expect(resolveWorkspaceCanvasTool('run', 'laser_position')).toBe('laser_position');
  });

  it('only tracks normal object drags for Select in Design', () => {
    expect(tracksObjectDragInteraction('design', 'select')).toBe(true);
    expect(tracksObjectDragInteraction('design', 'warp')).toBe(false);
    expect(tracksObjectDragInteraction('design', 'node')).toBe(false);
    expect(tracksObjectDragInteraction('run', 'select')).toBe(false);
  });
});
