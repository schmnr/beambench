import { create } from 'zustand';
import type { AppSettings } from '../types/commands';

/**
 * Pure predicate: should the welcome/promo screen auto-show on this launch?
 *
 * An active paid period verified through either connected service suppresses
 * the startup promotion. Free accounts and pack purchases keep library access.
 */
export function hasActivePaidConnection(...paidThrough: (string | null | undefined)[]): boolean {
  return paidThrough.some((date) => !!date && Date.parse(date) > Date.now());
}
export function shouldShowWelcome(settings: AppSettings | null | undefined, ...paidThrough: (string | null | undefined)[]): boolean {
  return !!settings && !hasActivePaidConnection(...paidThrough);
}

interface WelcomeStoreState {
  dialogOpen: boolean;
  openDialog: () => void;
  closeDialog: () => void;
}

export const useWelcomeStore = create<WelcomeStoreState>((set) => ({
  dialogOpen: false,
  openDialog: () => set({ dialogOpen: true }),
  closeDialog: () => set({ dialogOpen: false }),
}));
