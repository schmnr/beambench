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
    expect(screen.getByRole('dialog', { name: 'From the makers of Beam Bench' })).toBeDefined();
    expect(screen.getByText('Craftgineer')).toBeDefined();
    expect(screen.getByText('PrintCutCarve')).toBeDefined();
  });

  it('positions Craftgineer around professional results, not free tools', () => {
    render(<WelcomeDialog />);
    expect(screen.getByText('Professional tools. Effortless results.')).toBeDefined();
    expect(screen.getByText('Explore professional tools')).toBeDefined();
    expect(screen.queryByText(/free tools/i)).toBeNull();
  });

  it('shows three animated Craftgineer previews without video playback', () => {
    render(<WelcomeDialog />);
    const previews = [...document.body.querySelectorAll('.welcome-cg-media img')];

    expect(previews).toHaveLength(3);
    previews.forEach((preview) => expect(preview.getAttribute('src')).toContain('-preview.webp'));
    document.body.querySelectorAll('.welcome-cg-media source').forEach((source) => {
      expect(source.getAttribute('media')).toBe('(prefers-reduced-motion: reduce)');
      expect(source.getAttribute('srcset')).toContain('-poster.png');
    });
    expect(document.body.querySelector('video')).toBeNull();
  });

  it('falls back to a static poster when an animated preview cannot decode', () => {
    render(<WelcomeDialog />);
    const preview = document.body.querySelector<HTMLImageElement>('.welcome-cg-media img');
    expect(preview).not.toBeNull();
    fireEvent.error(preview!);
    expect(preview!.getAttribute('src')).toContain('-poster.png');
  });

  it('shows PrintCutCarve examples and the Platinum Club offer', () => {
    render(<WelcomeDialog />);
    expect(document.body.querySelectorAll('.welcome-pcc-gallery img')).toHaveLength(3);
    expect(screen.getByText('Get every design for $5 a month, while you can')).toBeDefined();
    expect(screen.getByText('Platinum Club, billed yearly at $59.99')).toBeDefined();
  });

  it('has no permanent opt-out control', () => {
    render(<WelcomeDialog />);
    expect(screen.queryByRole('checkbox')).toBeNull();
  });

  it('opens each product site without closing the panel', () => {
    render(<WelcomeDialog />);

    fireEvent.click(screen.getByText('Explore professional tools'));
    expect(mockedInvoke).toHaveBeenCalledWith('open_external_url', {
      url: 'https://craftgineer.com',
    });
    expect(useWelcomeStore.getState().dialogOpen).toBe(true);

    fireEvent.click(screen.getByText('Join the Platinum Club'));
    expect(mockedInvoke).toHaveBeenCalledWith('open_external_url', {
      url: 'https://printcutcarve.com',
    });
    expect(useWelcomeStore.getState().dialogOpen).toBe(true);
  });

  it('closes for the session on Escape', () => {
    render(<WelcomeDialog />);
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(useWelcomeStore.getState().dialogOpen).toBe(false);
  });

  it('closes for the session on backdrop click', () => {
    render(<WelcomeDialog />);
    fireEvent.click(screen.getByRole('dialog'));
    expect(useWelcomeStore.getState().dialogOpen).toBe(false);
  });

  it('closes for the session from the close button', () => {
    render(<WelcomeDialog />);
    fireEvent.click(screen.getByRole('button', { name: 'Close welcome' }));
    expect(useWelcomeStore.getState().dialogOpen).toBe(false);
  });

  it('never persists an opt-out when dismissed', () => {
    render(<WelcomeDialog />);
    fireEvent.keyDown(window, { key: 'Escape' });
    const settingsCall = mockedInvoke.mock.calls.find(([cmd]) => cmd === 'update_app_settings');
    expect(settingsCall).toBeUndefined();
  });
});
