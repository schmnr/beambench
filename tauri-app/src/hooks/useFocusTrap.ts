import { useEffect, type RefObject } from 'react';

const FOCUSABLE_SELECTOR = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(', ');

function isVisible(element: HTMLElement): boolean {
  if (element.hidden) return false;
  const style = window.getComputedStyle(element);
  return style.display !== 'none' && style.visibility !== 'hidden';
}

function getFocusable(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(isVisible);
}

/**
 * Traps Tab / Shift+Tab focus cycling inside `containerRef` while `active` is true.
 *
 * On activation, focuses the first focusable element unless focus is already inside
 * the container (so `autoFocus` inputs and pre-focused backdrops are preserved —
 * note `contains` includes the container itself). Tab from the last element wraps
 * to the first; Shift+Tab from the first wraps to the last. Only Tab keydowns are
 * intercepted; Escape handlers and mouse interactions are unaffected.
 */
export function useFocusTrap(containerRef: RefObject<HTMLElement | null>, active: boolean): void {
  useEffect(() => {
    if (!active) return undefined;
    const container = containerRef.current;
    if (!container) return undefined;

    const activeElement = document.activeElement;
    if (!(activeElement instanceof HTMLElement) || !container.contains(activeElement)) {
      getFocusable(container)[0]?.focus();
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Tab') return;
      const focusable = getFocusable(container);
      if (focusable.length === 0) {
        event.preventDefault();
        return;
      }
      const current = document.activeElement;
      const index = current instanceof HTMLElement ? focusable.indexOf(current) : -1;
      if (event.shiftKey) {
        // From the first element (or from outside the cycle) wrap to the last.
        if (index <= 0) {
          event.preventDefault();
          focusable[focusable.length - 1].focus();
        }
      } else if (index === -1 || index === focusable.length - 1) {
        // From the last element (or from outside the cycle) wrap to the first.
        event.preventDefault();
        focusable[0].focus();
      }
    };

    container.addEventListener('keydown', handleKeyDown);
    return () => container.removeEventListener('keydown', handleKeyDown);
  }, [containerRef, active]);
}
