import type { NestError, NestOptions, Project } from '../types/project';
import { machineService } from '../services/machineService';
import { projectService } from '../services/projectService';
import { useMachineStore } from '../stores/machineStore';
import { useNotificationStore } from '../stores/notificationStore';
import i18n from '../i18n';
import { useProjectStore } from '../stores/projectStore';
import { useUiStore } from '../stores/uiStore';
import { isTransformLocked, notifyTransformLocked } from '../utils/transformLocks';
import { canvasToMachinePoint, machineToCanvasPoint } from '../utils/workspaceCoordinates';

export type SelectionAnchor =
  | 'center'
  | 'upper_left'
  | 'upper_right'
  | 'lower_left'
  | 'lower_right'
  | 'left'
  | 'right'
  | 'top'
  | 'bottom';

export type JogDirection = 'left' | 'right' | 'up' | 'down';

export interface SelectionBounds {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
  w: number;
  h: number;
}

export function getSelectionBounds(
  project: Pick<Project, 'objects'> | null,
  selectedObjectIds: string[],
): SelectionBounds | null {
  if (!project || selectedObjectIds.length === 0) return null;
  const selected = project.objects.filter((object) => selectedObjectIds.includes(object.id));
  if (selected.length === 0) return null;
  const minX = Math.min(...selected.map((object) => object.bounds.min.x));
  const minY = Math.min(...selected.map((object) => object.bounds.min.y));
  const maxX = Math.max(...selected.map((object) => object.bounds.max.x));
  const maxY = Math.max(...selected.map((object) => object.bounds.max.y));
  return { minX, minY, maxX, maxY, w: maxX - minX, h: maxY - minY };
}

export function anchorPoint(bounds: SelectionBounds, anchor: SelectionAnchor): { x: number; y: number } {
  const cx = (bounds.minX + bounds.maxX) / 2;
  const cy = (bounds.minY + bounds.maxY) / 2;
  switch (anchor) {
    case 'upper_left': return { x: bounds.minX, y: bounds.minY };
    case 'upper_right': return { x: bounds.maxX, y: bounds.minY };
    case 'lower_left': return { x: bounds.minX, y: bounds.maxY };
    case 'lower_right': return { x: bounds.maxX, y: bounds.maxY };
    case 'left': return { x: bounds.minX, y: cy };
    case 'right': return { x: bounds.maxX, y: cy };
    case 'top': return { x: cx, y: bounds.minY };
    case 'bottom': return { x: cx, y: bounds.maxY };
    case 'center': return { x: cx, y: cy };
  }
}

export function startFromOffset(
  project: Pick<Project, 'start_from' | 'user_origin'> | null,
  workPosition: { x: number; y: number } | null | undefined,
): { x: number; y: number } {
  const startFrom = project?.start_from ?? 'absolute_coords';
  if (startFrom === 'user_origin' && project?.user_origin) {
    return { x: project.user_origin[0], y: project.user_origin[1] };
  }
  if (startFrom === 'current_position' && workPosition) {
    return { x: workPosition.x, y: workPosition.y };
  }
  return { x: 0, y: 0 };
}

function moveTopLeftForAnchor(
  bounds: SelectionBounds,
  anchor: SelectionAnchor,
  target: { x: number; y: number },
): { x: number; y: number } {
  const current = anchorPoint(bounds, anchor);
  return {
    x: bounds.minX + (target.x - current.x),
    y: bounds.minY + (target.y - current.y),
  };
}

export async function moveSelectedToPageAnchor(anchor: SelectionAnchor): Promise<void> {
  const ps = useProjectStore.getState();
  const project = ps.project;
  const bounds = getSelectionBounds(project, ps.selectedObjectIds);
  if (!project || !bounds) return;
  if (isTransformLocked(project.transform_locks, 'position')) {
    notifyTransformLocked('position');
    return;
  }

  const bedW = project.workspace.bed_width_mm;
  const bedH = project.workspace.bed_height_mm;
  const target = anchorPoint({ minX: 0, minY: 0, maxX: bedW, maxY: bedH, w: bedW, h: bedH }, anchor);
  const next = moveTopLeftForAnchor(bounds, anchor, target);
  await ps.moveObjectsTo(ps.selectedObjectIds, next.x, next.y);
}

export async function moveSelectedToLaserPosition(): Promise<void> {
  const ps = useProjectStore.getState();
  const project = ps.project;
  const machineStatus = useMachineStore.getState().machineStatus;
  if (!project || !machineStatus?.work_position || ps.selectedObjectIds.length === 0) return;
  if (isTransformLocked(project.transform_locks, 'position')) {
    notifyTransformLocked('position');
    return;
  }
  const { x, y } = machineToCanvasPoint(machineStatus.work_position, project.workspace);
  await ps.moveObjectsTo(ps.selectedObjectIds, x, y);
}

export async function moveLaserToSelection(anchor: SelectionAnchor): Promise<void> {
  const ps = useProjectStore.getState();
  const project = ps.project;
  const bounds = getSelectionBounds(project, ps.selectedObjectIds);
  if (!project || !bounds) return;
  const machineStatus = useMachineStore.getState().machineStatus;
  const pt = canvasToMachinePoint(anchorPoint(bounds, anchor), project.workspace);
  const offset = startFromOffset(project, machineStatus?.work_position);
  const feedRate = useUiStore.getState().moveWindowJogFeedRateMmMin;
  await machineService.moveLaserTo(pt.x + offset.x, pt.y + offset.y, feedRate);
  useNotificationStore.getState().push(i18n.t('notifications.moving_laser_to_selection'), 'info');
}

export async function jogLaser(direction: JogDirection): Promise<void> {
  const machine = useMachineStore.getState();
  if (machine.machineStatus?.run_state !== 'idle') return;
  const ui = useUiStore.getState();
  const d = ui.moveWindowJogDistanceMm;
  const vectors: Record<JogDirection, { x: number; y: number }> = {
    left: { x: -d, y: 0 },
    right: { x: d, y: 0 },
    up: { x: 0, y: d },
    down: { x: 0, y: -d },
  };
  const vector = vectors[direction];
  await machine.jog(vector.x, vector.y, ui.moveWindowJogFeedRateMmMin);
}

function formatNestError(error: unknown): string {
  if (typeof error === 'object' && error !== null && 'message' in error) {
    const nestError = error as Partial<NestError>;
    const message = typeof nestError.message === 'string' ? nestError.message : String(error);
    const ids = Array.isArray(nestError.unplacedObjectIds) ? nestError.unplacedObjectIds : [];
    if (ids.length === 0) return message;

    const project = useProjectStore.getState().project;
    const names = ids.map((id) => project?.objects.find((object) => object.id === id)?.name ?? id);
    return `${message}\nAffected: ${names.join(', ')}`;
  }
  return String(error);
}

function waitForNextPaint(): Promise<void> {
  if (typeof requestAnimationFrame !== 'function') return Promise.resolve();
  return new Promise((resolve) => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => resolve());
    });
  });
}

export async function nestSelected(options?: NestOptions, objectIds?: string[]): Promise<void> {
  const ui = useUiStore.getState();
  if (ui.nestingInProgress) return;

  const projectStore = useProjectStore.getState();
  const selectedIds = objectIds ? [...objectIds] : [...projectStore.selectedObjectIds];
  if (selectedIds.length === 0) {
    useNotificationStore.getState().push(i18n.t('notifications.select_before_nesting'), 'info');
    return;
  }
  ui.setNestingInProgress(true);
  try {
    await waitForNextPaint();
    const result = await projectService.nestSelected(selectedIds, options ?? ui.nestSettings);
    if (!Array.isArray(result.placedObjectIds) || !Array.isArray(result.unplacedObjectIds)) {
      throw new Error('Nest Selected returned an invalid result.');
    }
    if (result.unplacedObjectIds.length > 0) {
      throw new Error(
        `Nesting could not fit ${result.unplacedObjectIds.length} object(s) inside the selected container`,
      );
    }
    await useProjectStore.getState().loadProject({ invalidatePreview: true });
    useProjectStore.setState({ selectedObjectIds: result.placedObjectIds });
    useNotificationStore.getState().push(i18n.t('notifications.nested_objects', { count: result.placedObjectIds.length }), 'success');
  } catch (error) {
    useNotificationStore.getState().push(formatNestError(error), 'error');
  } finally {
    useUiStore.getState().setNestingInProgress(false);
  }
}
