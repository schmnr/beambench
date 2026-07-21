import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { useNotificationStore } from '../notificationStore';

describe('notificationStore', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    useNotificationStore.setState({ notifications: [] });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('pushes a notification', () => {
    useNotificationStore.getState().push('Hello', 'info');
    const { notifications } = useNotificationStore.getState();
    expect(notifications).toHaveLength(1);
    expect(notifications[0].message).toBe('Hello');
    expect(notifications[0].type).toBe('info');
  });

  it('dismisses a notification by id', () => {
    useNotificationStore.getState().push('A', 'info');
    const { notifications } = useNotificationStore.getState();
    const id = notifications[0].id;

    useNotificationStore.getState().dismiss(id);
    expect(useNotificationStore.getState().notifications).toHaveLength(0);
  });

  it('auto-dismisses info after 5s', () => {
    useNotificationStore.getState().push('Auto', 'info');
    expect(useNotificationStore.getState().notifications).toHaveLength(1);

    vi.advanceTimersByTime(5000);
    expect(useNotificationStore.getState().notifications).toHaveLength(0);
  });

  it('auto-dismisses error after 8s', () => {
    useNotificationStore.getState().push('Err', 'error');
    expect(useNotificationStore.getState().notifications).toHaveLength(1);

    vi.advanceTimersByTime(5000);
    expect(useNotificationStore.getState().notifications).toHaveLength(1);

    vi.advanceTimersByTime(3000);
    expect(useNotificationStore.getState().notifications).toHaveLength(0);
  });

  it('enforces max 5 visible notifications', () => {
    for (let i = 0; i < 7; i++) {
      useNotificationStore.getState().push(`Msg ${i}`, 'info');
    }
    const { notifications } = useNotificationStore.getState();
    expect(notifications).toHaveLength(5);
    // Should keep the last 5
    expect(notifications[0].message).toBe('Msg 2');
    expect(notifications[4].message).toBe('Msg 6');
  });

  it('assigns unique ids', () => {
    useNotificationStore.getState().push('A', 'info');
    useNotificationStore.getState().push('B', 'info');
    const { notifications } = useNotificationStore.getState();
    expect(notifications[0].id).not.toBe(notifications[1].id);
  });

  describe('dedupe', () => {
    it('does not stack an identical (message, type) toast and returns the existing id', () => {
      const firstId = useNotificationStore.getState().push('Same', 'info');
      const secondId = useNotificationStore.getState().push('Same', 'info');

      expect(secondId).toBe(firstId);
      expect(useNotificationStore.getState().notifications).toHaveLength(1);
    });

    it('refreshes the existing toast auto-dismiss timer on duplicate push', () => {
      useNotificationStore.getState().push('Same', 'info');
      vi.advanceTimersByTime(4000);

      // Duplicate push 1s before expiry restarts the 5s window.
      useNotificationStore.getState().push('Same', 'info');
      vi.advanceTimersByTime(4000);
      expect(useNotificationStore.getState().notifications).toHaveLength(1);

      vi.advanceTimersByTime(1000);
      expect(useNotificationStore.getState().notifications).toHaveLength(0);
    });

    it('same message with a different type is not deduped', () => {
      useNotificationStore.getState().push('Same', 'info');
      useNotificationStore.getState().push('Same', 'error');
      expect(useNotificationStore.getState().notifications).toHaveLength(2);
    });

    it('a dismissed toast can be pushed again', () => {
      const id = useNotificationStore.getState().push('Same', 'info');
      useNotificationStore.getState().dismiss(id);

      const newId = useNotificationStore.getState().push('Same', 'info');
      expect(newId).not.toBe(id);
      expect(useNotificationStore.getState().notifications).toHaveLength(1);
    });
  });
});
