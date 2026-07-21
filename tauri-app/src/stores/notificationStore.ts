import { create } from 'zustand';

export type NotificationType = 'success' | 'error' | 'warning' | 'info';

export interface Notification {
  id: string;
  message: string;
  type: NotificationType;
  createdAt: number;
  actionLabel?: string;
  onAction?: () => void;
}

const MAX_VISIBLE = 5;
const AUTO_DISMISS_MS: Record<NotificationType, number> = {
  success: 5000,
  info: 5000,
  warning: 5000,
  error: 8000,
};

let nextId = 0;

// Auto-dismiss timers keyed by notification id so a deduped push can refresh
// the existing toast's timer instead of stacking a duplicate.
const dismissTimers = new Map<string, ReturnType<typeof setTimeout>>();

function clearDismissTimer(id: string) {
  const timer = dismissTimers.get(id);
  if (timer !== undefined) {
    clearTimeout(timer);
    dismissTimers.delete(id);
  }
}

interface NotificationStoreState {
  notifications: Notification[];
  push: (
    message: string,
    type: NotificationType,
    options?: {
      actionLabel?: string;
      onAction?: () => void;
      autoDismissMs?: number | null;
    },
  ) => string;
  dismiss: (id: string) => void;
}

export const useNotificationStore = create<NotificationStoreState>((set, get) => ({
  notifications: [],

  push: (message, type, options) => {
    const delay = options?.autoDismissMs === undefined ? AUTO_DISMISS_MS[type] : options.autoDismissMs;

    // Dedupe: if an identical toast is already visible, refresh its
    // auto-dismiss timer instead of appending a duplicate.
    const existing = get().notifications.find((n) => n.message === message && n.type === type);
    if (existing) {
      clearDismissTimer(existing.id);
      if (delay !== null && delay > 0) {
        dismissTimers.set(existing.id, setTimeout(() => {
          get().dismiss(existing.id);
        }, delay));
      }
      return existing.id;
    }

    const id = `notif-${++nextId}`;
    const notification: Notification = {
      id,
      message,
      type,
      createdAt: Date.now(),
      actionLabel: options?.actionLabel,
      onAction: options?.onAction,
    };

    set((s) => {
      const updated = [...s.notifications, notification];
      // Keep only the last MAX_VISIBLE; drop timers for evicted toasts
      const visible = updated.slice(-MAX_VISIBLE);
      for (const dropped of updated.slice(0, updated.length - visible.length)) {
        clearDismissTimer(dropped.id);
      }
      return { notifications: visible };
    });

    // Auto-dismiss
    if (delay !== null && delay > 0) {
      dismissTimers.set(id, setTimeout(() => {
        get().dismiss(id);
      }, delay));
    }
    return id;
  },

  dismiss: (id) => {
    clearDismissTimer(id);
    set((s) => ({
      notifications: s.notifications.filter((n) => n.id !== id),
    }));
  },
}));
