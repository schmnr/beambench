import { create } from 'zustand';
import type { AppSettings } from '../types/commands';

/**
 * Pure predicate: should the welcome/promo screen auto-show on this launch?
 *
 * It shows on every startup once settings have loaded. There is no permanent
 * opt-out; the screen can only be closed for the current session and returns
 * on the next launch.
 */
export function shouldShowWelcome(settings: AppSettings | null | undefined): boolean {
  return !!settings;
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
