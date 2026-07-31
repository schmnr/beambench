import { describe, expect, it } from 'vitest';

import { resolveWorkspaceCanvasTool } from '../workspaceCanvasTool';

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
});
