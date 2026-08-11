import { describe, expect, it } from 'vitest';
import { shouldAnimateSelectionDashes } from './selectionDashAnimation';

const idleSelection = {
  selectedObjectCount: 1,
  reduceMotion: false,
  hasHeavySelection: false,
  interactionActive: false,
  interactionKind: 'none' as const,
};

describe('shouldAnimateSelectionDashes', () => {
  it('animates an ordinary idle selection', () => {
    expect(shouldAnimateSelectionDashes(idleSelection)).toBe(true);
  });

  it('pauses the independent overlay animation during an object drag', () => {
    expect(shouldAnimateSelectionDashes({
      ...idleSelection,
      interactionActive: true,
      interactionKind: 'object-drag',
    })).toBe(false);
  });

  it('does not suppress animation merely because a non-drag interaction is active', () => {
    expect(shouldAnimateSelectionDashes({
      ...idleSelection,
      interactionActive: true,
      interactionKind: 'pan',
    })).toBe(true);
  });

  it('respects existing selection, reduced-motion, and heavy-selection guards', () => {
    expect(shouldAnimateSelectionDashes({ ...idleSelection, selectedObjectCount: 0 })).toBe(false);
    expect(shouldAnimateSelectionDashes({ ...idleSelection, reduceMotion: true })).toBe(false);
    expect(shouldAnimateSelectionDashes({ ...idleSelection, hasHeavySelection: true })).toBe(false);
  });
});
