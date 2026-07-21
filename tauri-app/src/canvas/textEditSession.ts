import { useProjectStore } from '../stores/projectStore';
import type { ProjectObject } from '../types/project';

interface PendingEdit {
  objectId: string;
  content: string;
  initialContent: string;
}

let pendingEdit: PendingEdit | null = null;
let commitPromise: Promise<boolean> | null = null;

/** Called by TextEditOverlay on mount. */
export function setPendingEdit(objectId: string, initialContent: string): void {
  if (pendingEdit?.objectId === objectId) return;
  pendingEdit = { objectId, content: initialContent, initialContent };
}

/** Called by TextEditOverlay on every keystroke. */
export function updatePendingContent(content: string): void {
  if (pendingEdit) pendingEdit.content = content;
}

/** Clear pending edit tracking (on unmount). */
export function clearPendingEdit(): void {
  pendingEdit = null;
}

export function hasPendingTextEdit(): boolean {
  return pendingEdit !== null;
}

/** Release mount ownership without discarding unsaved content. */
export function releasePendingEdit(objectId?: string): void {
  if (!pendingEdit) return;
  if (objectId && pendingEdit.objectId !== objectId) return;
  if (pendingEdit.content === pendingEdit.initialContent) {
    pendingEdit = null;
  }
}

/**
 * Commit the current pending edit to the backend.
 * Called explicitly before any session transition — NOT reliant on blur.
 * Returns false when persistence fails so callers can preserve or recover
 * the edit session instead of discarding unsaved content.
 *
 * Reads LATEST textData from the project store at commit time, only
 * overriding `content`. This ensures toolbar changes (font, alignment,
 * etc.) made during the edit session are preserved.
 */
export async function commitPendingTextEdit(): Promise<boolean> {
  if (!pendingEdit) return true;
  if (commitPromise) return commitPromise;

  const currentEdit = { ...pendingEdit };
  const { objectId, content, initialContent } = currentEdit;
  if (content === initialContent) {
    pendingEdit = null;
    return true;
  }

  const project = useProjectStore.getState().project;
  const obj = project?.objects?.find((o: ProjectObject) => o.id === objectId);
  if (!obj || obj.data.type !== 'text') {
    pendingEdit = null;
    return true;
  }
  const textData = obj.data;

  commitPromise = (async () => {
    const saved = await useProjectStore.getState().updateObjectData(
      objectId,
      { ...textData, content, variable_text: undefined },
    );
    if (!saved) {
      return false;
    }

    if (!pendingEdit || pendingEdit.objectId !== objectId) {
      return true;
    }

    if (pendingEdit.content === content) {
      pendingEdit = null;
      return true;
    }

    pendingEdit = {
      ...pendingEdit,
      initialContent: content,
    };
    return false;
  })();

  try {
    return await commitPromise;
  } finally {
    commitPromise = null;
  }
}

/** Return the current pending content (or null if no pending edit). */
export function getPendingContent(): string | null {
  return pendingEdit?.content ?? null;
}

export function getPendingContentForObject(objectId: string): string | null {
  return pendingEdit?.objectId === objectId ? pendingEdit.content : null;
}

/** Discard pending edit without saving (for Escape / cancel). */
export function discardPendingTextEdit(): void {
  pendingEdit = null;
}

/**
 * Synchronous check for "brand-new text with no content". Callers invoke
 * this BEFORE committing/clearing state so pendingEdit still reflects what
 * the user typed — otherwise a race with `commitPendingTextEdit` (which
 * clears pendingEdit) could mis-read stale/empty project state.
 */
export function isNewEmptyText(objectId: string | null, mode: string | null): boolean {
  if (!objectId || mode !== 'new') return false;
  const pendingContent = pendingEdit?.objectId === objectId ? pendingEdit.content : null;
  if (pendingContent !== null) return pendingContent.trim() === '';
  const project = useProjectStore.getState().project;
  const obj = project?.objects?.find((o) => o.id === objectId);
  if (obj && obj.data.type === 'text') {
    return obj.data.content.trim() === '';
  }
  return false;
}

/**
 * Remove the edited text object if it was just-created (mode 'new') and
 * has no content at the moment this is called. Safe to invoke at any
 * point, but callers who commit first should use `isNewEmptyText` BEFORE
 * committing and then `removeObject` after, to avoid racing the commit.
 */
export async function maybeDeleteNewEmptyText(
  objectId: string | null,
  mode: string | null,
): Promise<boolean> {
  if (!isNewEmptyText(objectId, mode)) return false;
  await useProjectStore.getState().removeObject(objectId!);
  return true;
}
