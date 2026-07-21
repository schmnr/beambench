import { create } from 'zustand';
import { useProjectStore } from './projectStore';

/** A deferred action waiting on the unsaved-changes decision. */
export interface PendingUnsavedAction {
  /** Runs once the user has saved or chosen to discard changes. */
  execute: () => void | Promise<void>;
}

interface UnsavedGuardState {
  pendingAction: PendingUnsavedAction | null;
  request: (action: PendingUnsavedAction) => void;
  clear: () => void;
}

export const useUnsavedGuardStore = create<UnsavedGuardState>((set) => ({
  pendingAction: null,
  request: (action) => set({ pendingAction: action }),
  clear: () => set({ pendingAction: null }),
}));

/**
 * Run `execute` immediately when the open project has no unsaved changes,
 * otherwise park it behind the unsaved-changes dialog.
 */
export function guardUnsavedChanges(execute: () => void | Promise<void>): void {
  const dirty = useProjectStore.getState().project?.dirty ?? false;
  if (!dirty) {
    void execute();
    return;
  }
  useUnsavedGuardStore.getState().request({ execute });
}
