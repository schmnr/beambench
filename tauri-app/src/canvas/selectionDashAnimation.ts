export interface SelectionDashAnimationState {
  selectedObjectCount: number;
  reduceMotion: boolean;
  hasHeavySelection: boolean;
  interactionActive: boolean;
  interactionKind: 'none' | 'pan' | 'zoom' | 'object-drag';
}

/**
 * Keep decorative selection animation out of the object-drag render loop.
 * The drag itself already requests scene frames, and an independent overlay
 * clear can produce visible compositor flicker on Windows.
 */
export function shouldAnimateSelectionDashes(state: SelectionDashAnimationState): boolean {
  return state.selectedObjectCount > 0
    && !state.reduceMotion
    && !state.hasHeavySelection
    && !(state.interactionActive && state.interactionKind === 'object-drag');
}
