import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  moveLaserToProjectPoint: vi.fn(),
  moveObjectsTo: vi.fn(),
  push: vi.fn(),
  projectState: {} as Record<string, unknown>,
  machineState: {} as Record<string, unknown>,
}));

vi.mock('../../services/machineService', () => ({
  machineService: {
    moveLaserToProjectPoint: mocks.moveLaserToProjectPoint,
  },
}));
vi.mock('../../stores/projectStore', () => ({
  useProjectStore: { getState: () => mocks.projectState },
}));
vi.mock('../../stores/machineStore', () => ({
  useMachineStore: { getState: () => mocks.machineState },
}));
vi.mock('../../stores/uiStore', () => ({
  useUiStore: { getState: () => ({ moveWindowJogFeedRateMmMin: 1500 }) },
}));
vi.mock('../../stores/notificationStore', () => ({
  useNotificationStore: { getState: () => ({ push: mocks.push }) },
}));
vi.mock('../../i18n', () => ({ default: { t: (key: string) => key } }));

import { moveLaserToSelection, moveSelectedToLaserPosition } from '../arrangeActions';

function project() {
  return {
    workspace: { bed_width_mm: 400, bed_height_mm: 300, origin: 'bottom_left' },
    objects: [{
      id: 'object-1',
      layer_id: 'layer-1',
      bounds: { min: { x: 10, y: 20 }, max: { x: 20, y: 30 } },
      transform_locks: {
        move_enabled: true,
        size_enabled: true,
        rotate_enabled: true,
        shear_enabled: true,
      },
    }],
  };
}

describe('arrange laser positioning', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.projectState = {
      project: project(),
      selectedObjectIds: ['object-1'],
      moveObjectsTo: mocks.moveObjectsTo,
    };
    mocks.machineState = {
      machineStatus: {
        machine_position: { x: 80, y: 90, z: 0 },
        work_position: { x: 10, y: 20, z: 0 },
      },
    };
  });

  it('maps selection motion through the backend project placement', async () => {
    await moveLaserToSelection('center');

    expect(mocks.moveLaserToProjectPoint).toHaveBeenCalledWith(15, 25, 1500);
  });

  it('moves artwork to the physical laser position instead of its work coordinate', async () => {
    await moveSelectedToLaserPosition();

    expect(mocks.moveObjectsTo).toHaveBeenCalledWith(['object-1'], 80, 210);
  });
});
