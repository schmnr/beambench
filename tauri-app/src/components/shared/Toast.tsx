import { useTranslation } from 'react-i18next';
import type { Notification, NotificationType } from '../../stores/notificationStore';
import { openFeedbackReport } from '../../feedbackEvents';
import { useUiStore } from '../../stores/uiStore';

const STYLES: Record<NotificationType, string> = {
  success: 'bg-bb-success-bg border-bb-success-border text-bb-success-fg',
  error: 'bg-bb-error-bg border-bb-error-border text-bb-error-fg',
  warning: 'bg-bb-warning-bg border-bb-warning-border text-bb-warning-fg',
  info: 'bg-bb-info-bg border-bb-info-border text-bb-info-fg',
};

const ICONS: Record<NotificationType, string> = {
  success: '\u2713',
  error: '\u2717',
  warning: '\u26A0',
  info: '\u2139',
};

export function Toast({
  notification,
  onDismiss,
}: {
  notification: Notification;
  onDismiss: (id: string) => void;
}) {
  const { t } = useTranslation();
  const isConnectionError = notification.type === 'error'
    && /\b(connect|connection|serial|port|grbl|baud|usb)\b/i.test(notification.message);

  return (
    <div
      className={`flex items-start gap-2 px-3 py-2 rounded border shadow-lg text-sm max-w-sm ${STYLES[notification.type]}`}
      role="alert"
    >
      <span className="text-base leading-none mt-0.5">{ICONS[notification.type]}</span>
      <span className="flex-1 break-words">{notification.message}</span>
      {notification.actionLabel && notification.onAction ? (
        <button
          onClick={() => {
            notification.onAction?.();
            onDismiss(notification.id);
          }}
          className="rounded border border-current/30 px-2 py-0.5 text-xs font-medium opacity-80 hover:opacity-100"
        >
          {notification.actionLabel}
        </button>
      ) : null}
      {notification.type === 'error' ? (
        <button
          onClick={() => {
            openFeedbackReport({
              kind: 'bug',
              title: t('notifications.error_report_title'),
              description: notification.message,
              sourceContext: {
                source: 'error_toast',
                error_message: notification.message,
                feature: 'toast',
                correlation_ts: new Date(notification.createdAt).toISOString(),
              },
            });
            onDismiss(notification.id);
          }}
          className="rounded border border-current/30 px-2 py-0.5 text-xs font-medium opacity-80 hover:opacity-100"
        >
          {t('notifications.report')}
        </button>
      ) : null}
      {isConnectionError ? (
        <button
          onClick={() => {
            useUiStore.getState().showPanel('connection_diagnostics');
            onDismiss(notification.id);
          }}
          className="rounded border border-current/30 px-2 py-0.5 text-xs font-medium opacity-80 hover:opacity-100"
        >
          {t('notifications.open_diagnostics')}
        </button>
      ) : null}
      <button
        onClick={() => onDismiss(notification.id)}
        className="ml-1 opacity-60 hover:opacity-100 text-xs leading-none"
        aria-label={t('notifications.dismiss')}
      >
        &times;
      </button>
    </div>
  );
}
