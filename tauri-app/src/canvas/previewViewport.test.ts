import { describe, expect, it } from 'vitest';
import { previewViewportForWorkspace } from './previewViewport';
import { worldToScreen } from './ViewportTransform';
import type { ViewportParams } from './ViewportTransform';
import type { Workspace } from '../types/project';

const vp: ViewportParams = {
  offset: { x: 25, y: 40 },
  zoom: 100,
  canvasWidth: 800,
  canvasHeight: 600,
};

describe('previewViewportForWorkspace', () => {
  it('leaves top-left workspaces in normal canvas coordinates', () => {
    const workspace: Workspace = {
      bed_width_mm: 400,
      bed_height_mm: 300,
      origin: 'top_left',
    };

    expect(previewViewportForWorkspace(vp, workspace)).toBe(vp);
  });

  it('converts bottom-left workspaces to a Y-up machine viewport', () => {
    const workspace: Workspace = {
      bed_width_mm: 400,
      bed_height_mm: 300,
      origin: 'bottom_left',
    };

    expect(previewViewportForWorkspace(vp, workspace)).toEqual({
      ...vp,
      offset: { x: 25, y: 260 },
      yAxis: 'up',
    });
  });

  it('maps machine-space preview points onto matching canvas-space locations', () => {
    const workspace: Workspace = {
      bed_width_mm: 400,
      bed_height_mm: 300,
      origin: 'bottom_left',
    };
    const previewVp = previewViewportForWorkspace(vp, workspace);

    expect(worldToScreen({ x: 25, y: 260 }, previewVp))
      .toEqual(worldToScreen({ x: 25, y: 40 }, vp));
  });
});
