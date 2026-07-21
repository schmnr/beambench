import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { WelcomeDialog } from '../WelcomeDialog';
import { useWelcomeStore } from '../../../stores/welcomeStore';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));

const mockedInvoke = vi.mocked(invoke);
const initialWelcomeState = useWelcomeStore.getState();

beforeEach(() => {
  mockedInvoke.mockClear();
  mockedInvoke.mockResolvedValue(null);
  useWelcomeStore.setState({ dialogOpen: true });
});

afterEach(() => {
  cleanup();
  useWelcomeStore.setState(initialWelcomeState, true);
});

describe('WelcomeDialog', () => {
  it('renders both products with equal billing', () => {
    render(<WelcomeDialog />);
    expect(screen.getByRole('dialog', { name: 'Beam Bench is free' })).toBeDefined();
    expect(screen.getByText('Craftgineer')).toBeDefined();
    expect(screen.getByText('PrintCutCarve')).toBeDefined();
  });

  it('has no permanent opt-out control', () => {
    render(<WelcomeDialog />);
    expect(screen.queryByRole('checkbox')).toBeNull();
  });

  it('opens each product site without closing the panel', () => {
    render(<WelcomeDialog />);

    fireEvent.click(screen.getByText('Visit Craftgineer'));
    expect(mockedInvoke).toHaveBeenCalledWith('open_external_url', {
      url: 'https://craftgineer.com',
    });
    expect(useWelcomeStore.getState().dialogOpen).toBe(true);

    fireEvent.click(screen.getByText('Visit PrintCutCarve'));
    expect(mockedInvoke).toHaveBeenCalledWith('open_external_url', {
      url: 'https://printcutcarve.com',
    });
    expect(useWelcomeStore.getState().dialogOpen).toBe(true);
  });

  it('closes for the session on Escape', () => {
    render(<WelcomeDialog />);
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(useWelcomeStore.getState().dialogOpen).toBe(false);
  });

  it('closes for the session on backdrop click', () => {
    render(<WelcomeDialog />);
    fireEvent.click(screen.getByRole('dialog'));
    expect(useWelcomeStore.getState().dialogOpen).toBe(false);
  });

  it('never calls update_app_settings (no persisted opt-out)', () => {
    render(<WelcomeDialog />);
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    const settingsCall = mockedInvoke.mock.calls.find(([cmd]) => cmd === 'update_app_settings');
    expect(settingsCall).toBeUndefined();
  });
});
