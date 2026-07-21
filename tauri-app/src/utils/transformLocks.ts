import type { TransformLocks, ProjectObject } from '../types/project';
import { useNotificationStore } from '../stores/notificationStore';
import i18n from '../i18n';

export type TransformLockKind = 'position' | 'scale' | 'rotation' | 'shear';

const LOCK_MESSAGE_KEYS: Record<TransformLockKind, string> = {
  position: 'notifications.lock.position',
  scale: 'notifications.lock.scale',
  rotation: 'notifications.lock.rotation',
  shear: 'notifications.lock.shear',
};

export function isTransformLocked(
  locks: TransformLocks | undefined | null,
  kind: TransformLockKind,
): boolean {
  if (!locks) return false;
  switch (kind) {
    case 'position':
      return locks.move_enabled === false;
    case 'scale':
      return locks.size_enabled === false;
    case 'rotation':
      return locks.rotate_enabled === false;
    case 'shear':
      return locks.shear_enabled === false;
  }
}

export function notifyTransformLocked(kind: TransformLockKind): void {
  useNotificationStore.getState().push(i18n.t(LOCK_MESSAGE_KEYS[kind]), 'warning');
}

/** Returns true if the object is individually locked (obj.locked). */
export function isObjectLocked(obj: ProjectObject | undefined | null): boolean {
  return obj?.locked === true;
}

/** Returns true if any object in the list is locked. */
export function anyObjectLocked(objects: ProjectObject[]): boolean {
  return objects.some((o) => o.locked);
}

export function notifyObjectLocked(): void {
  useNotificationStore.getState().push(i18n.t('notifications.object_locked'), 'warning');
}
