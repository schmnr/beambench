import { describe, expect, it } from 'vitest';
import { canvasToMachinePoint, machineToCanvasPoint } from '../workspaceCoordinates';
import type { Workspace } from '../../types/project';

const topLeftWorkspace: Workspace = {
  bed_width_mm: 400,
  bed_height_mm: 300,
  origin: 'top_left',
};

const bottomLeftWorkspace: Workspace = {
  bed_width_mm: 400,
  bed_height_mm: 300,
  origin: 'bottom_left',
};

describe('workspace coordinate conversion', () => {
  it('keeps top-left workspace coordinates unchanged', () => {
    expect(canvasToMachinePoint({ x: 25, y: 40 }, topLeftWorkspace)).toEqual({ x: 25, y: 40 });
    expect(machineToCanvasPoint({ x: 25, y: 40 }, topLeftWorkspace)).toEqual({ x: 25, y: 40 });
  });

  it('flips Y across the bed height for bottom-left workspaces', () => {
    expect(canvasToMachinePoint({ x: 25, y: 40 }, bottomLeftWorkspace)).toEqual({ x: 25, y: 260 });
    expect(machineToCanvasPoint({ x: 25, y: 260 }, bottomLeftWorkspace)).toEqual({ x: 25, y: 40 });
  });
});
