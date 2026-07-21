import { create } from 'zustand';
import { appService } from '../services/appService';
import {
  checkForUpdate,
  downloadAndInstallUpdate,
  getUpdateEnvironmentBlocker,
  getUpdateInstallBlocker,
  isCrossDeviceInstallError,
  UpdateInstallBlockedError,
  type UpdateInfo,
  type UpdateProgress,
} from '../services/updateService';
import { useAppStore } from './appStore';
import { useNotificationStore } from './notificationStore';
import i18n from '../i18n';
import type { AppSettings } from '../types/commands';

const UPDATE_SNOOZE_MS = 24 * 60 * 60 * 1000;

type UpdateStatus =
  | 'idle'
  | 'checking'
  | 'available'
  | 'up_to_date'
  | 'downloading'
  | 'installing'
  | 'relaunching'
  | 'blocked'
  | 'error';

type UpdateCheckMode = 'startup' | 'manual';

interface UpdateStoreState {
  availableUpdate: UpdateInfo | null;
  status: UpdateStatus;
  progress: UpdateProgress | null;
  error: string | null;
  dialogOpen: boolean;
  openDialog: () => void;
  closeDialog: () => void;
  checkForUpdates: (mode?: UpdateCheckMode) => Promise<UpdateInfo | null>;
  runStartupCheck: () => Promise<UpdateInfo | null>;
  installAvailableUpdate: () => Promise<void>;
  snoozeAvailableUpdate: () => Promise<void>;
  skipAvailableUpdate: () => Promise<void>;
}

function updateIsSnoozed(settings: AppSettings | null | undefined, now = Date.now()): boolean {
  const value = settings?.update_snoozed_until ?? '';
  if (!value) return false;
  const snoozedUntil = Date.parse(value);
  return Number.isFinite(snoozedUntil) && snoozedUntil > now;
}

function updateIsSkipped(settings: AppSettings | null | undefined, update: UpdateInfo): boolean {
  return (settings?.skipped_update_version ?? '') === update.version;
}

async function getSettingsForUpdateCheck(): Promise<AppSettings | null> {
  const storeSettings = useAppStore.getState().settings;
  if (storeSettings) return storeSettings;
  try {
    return await appService.getSettings();
  } catch {
    return null;
  }
}

async function persistUpdateSettings(updates: {
  update_snoozed_until?: string;
  skipped_update_version?: string;
}): Promise<void> {
  const updateSettings = useAppStore.getState().updateSettings;
  await updateSettings(updates);
}

export const useUpdateStore = create<UpdateStoreState>((set, get) => ({
  availableUpdate: null,
  status: 'idle',
  progress: null,
  error: null,
  dialogOpen: false,

  openDialog: () => set({ dialogOpen: true }),
  closeDialog: () => set({ dialogOpen: false }),

  runStartupCheck: async () => {
    const settings = await getSettingsForUpdateCheck();
    if (settings?.check_for_updates_on_startup === false) {
      return null;
    }
    return get().checkForUpdates('startup');
  },

  checkForUpdates: async (mode = 'manual') => {
    if (get().status === 'checking') {
      return null;
    }
    set({ status: 'checking', error: null, progress: null });
    try {
      const update = await checkForUpdate();
      if (!update) {
        set({ status: mode === 'manual' ? 'up_to_date' : 'idle', availableUpdate: null });
        if (mode === 'manual') {
          useNotificationStore.getState().push(i18n.t('notifications.update.up_to_date'), 'info');
        }
        return null;
      }

      const settings = await getSettingsForUpdateCheck();
      if (mode === 'startup' && (updateIsSnoozed(settings) || updateIsSkipped(settings, update))) {
        set({ status: 'idle', availableUpdate: update });
        return update;
      }

      set({
        status: 'available',
        availableUpdate: update,
        dialogOpen: mode === 'manual',
      });

      if (mode === 'startup') {
        useNotificationStore.getState().push(i18n.t('notifications.update.available', { version: update.version }), 'info', {
          actionLabel: i18n.t('notifications.update.view_action'),
          onAction: () => get().openDialog(),
          autoDismissMs: null,
        });
      }
      return update;
    } catch (error) {
      const message = String(error);
      set({ status: mode === 'manual' ? 'error' : 'idle', error: message });
      if (mode === 'manual') {
        useNotificationStore.getState().push(i18n.t('notifications.update.check_failed', { detail: message }), 'error');
      }
      return null;
    }
  },

  installAvailableUpdate: async () => {
    const blocker = getUpdateInstallBlocker();
    if (blocker) {
      set({ status: 'blocked', error: blocker });
      return;
    }

    // The macOS in-place swap cannot work when the app runs translocated,
    // from the mounted DMG, or from a different disk than the temp folder.
    // Fail before downloading, with a remedy instead of a raw os error.
    const environmentBlocker = await getUpdateEnvironmentBlocker();
    if (environmentBlocker) {
      set({
        status: 'blocked',
        error: i18n.t(`dialog.update.blocked_${environmentBlocker}`),
      });
      return;
    }

    set({ status: 'downloading', error: null, progress: null });
    try {
      await downloadAndInstallUpdate((progress) => {
        set({ progress, status: progress.phase === 'finished' ? 'installing' : 'downloading' });
      });
      set({ status: 'relaunching' });
    } catch (error) {
      if (isCrossDeviceInstallError(error)) {
        // The precheck missed a cross-volume layout and the swap failed
        // with EXDEV anyway; show the remedy, not "os error 18".
        set({ status: 'blocked', error: i18n.t('dialog.update.blocked_other_disk') });
        return;
      }
      const message = error instanceof Error ? error.message : String(error);
      set({
        status: error instanceof UpdateInstallBlockedError ? 'blocked' : 'error',
        error: message,
      });
    }
  },

  snoozeAvailableUpdate: async () => {
    const update = get().availableUpdate;
    if (!update) return;
    const snoozedUntil = new Date(Date.now() + UPDATE_SNOOZE_MS).toISOString();
    await persistUpdateSettings({
      update_snoozed_until: snoozedUntil,
      skipped_update_version: '',
    });
    set({ dialogOpen: false, status: 'idle', error: null });
    useNotificationStore.getState().push(i18n.t('notifications.update.postponed'), 'info');
  },

  skipAvailableUpdate: async () => {
    const update = get().availableUpdate;
    if (!update) return;
    await persistUpdateSettings({
      skipped_update_version: update.version,
      update_snoozed_until: '',
    });
    set({ dialogOpen: false, status: 'idle', error: null });
    useNotificationStore.getState().push(i18n.t('notifications.update.skipped', { version: update.version }), 'info');
  },
}));
