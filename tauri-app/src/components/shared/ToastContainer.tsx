import { createPortal } from 'react-dom';
import { useNotificationStore } from '../../stores/notificationStore';
import { Toast } from './Toast';

export function ToastContainer() {
  const notifications = useNotificationStore((s) => s.notifications);
  const dismiss = useNotificationStore((s) => s.dismiss);

  if (notifications.length === 0) return null;

  return createPortal(
    <div className="fixed bottom-4 right-4 z-[9999] flex flex-col gap-2">
      {notifications.map((n) => (
        <Toast key={n.id} notification={n} onDismiss={dismiss} />
      ))}
    </div>,
    document.body,
  );
}
