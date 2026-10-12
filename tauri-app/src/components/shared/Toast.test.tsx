import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import i18n from '../../i18n';
import { MACHINE_PROFILES_OPEN_EVENT } from '../../feedbackEvents';
import { Toast } from './Toast';

afterEach(cleanup);

describe('missing profile recovery', () => {
  const notification = () => ({
    id: 'missing-profile',
    message: i18n.t('errors.no_active_machine_profile'),
    type: 'error' as const,
    createdAt: Date.now(),
  });

  it('opens machine profiles and dismisses the setup error', () => {
    const onOpen = vi.fn();
    const onDismiss = vi.fn();
    window.addEventListener(MACHINE_PROFILES_OPEN_EVENT, onOpen);
    try {
      render(<Toast notification={notification()} onDismiss={onDismiss} />);
      fireEvent.click(screen.getByRole('button', { name: i18n.t('dialog.machine_profile.title') }));
      expect(onOpen).toHaveBeenCalledOnce();
      expect(onDismiss).toHaveBeenCalledWith('missing-profile');
    } finally {
      window.removeEventListener(MACHINE_PROFILES_OPEN_EVENT, onOpen);
    }
  });

  it('keeps an explicit recovery action supplied by the originating control', () => {
    const onAction = vi.fn();
    const onDismiss = vi.fn();
    render(<Toast notification={{ ...notification(), actionLabel: 'Configure', onAction }} onDismiss={onDismiss} />);
    expect(screen.queryByRole('button', { name: i18n.t('dialog.machine_profile.title') })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Configure' }));
    expect(onAction).toHaveBeenCalledOnce();
    expect(onDismiss).toHaveBeenCalledWith('missing-profile');
  });
});
