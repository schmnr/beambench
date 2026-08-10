import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { WelcomeDialog } from '../WelcomeDialog';
import { useWelcomeStore } from '../../../stores/welcomeStore';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));

const mockedInvoke = vi.mocked(invoke);
const initialWelcomeState = useWelcomeStore.getState();
let playSpy: ReturnType<typeof vi.spyOn>;
let pauseSpy: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  mockedInvoke.mockClear();
  mockedInvoke.mockResolvedValue(null);
  playSpy = vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue();
  pauseSpy = vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => undefined);
  useWelcomeStore.setState({ dialogOpen: true });
});

afterEach(() => {
  cleanup();
  playSpy.mockRestore();
  pauseSpy.mockRestore();
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

  it('shows three silent looping Craftgineer previews with posters', () => {
    render(<WelcomeDialog />);
    const videos = [...document.body.querySelectorAll('video')];

    expect(videos).toHaveLength(3);
    videos.forEach((video) => {
      expect(video.autoplay).toBe(true);
      expect(video.muted).toBe(true);
      expect(video.loop).toBe(true);
      expect(video.playsInline).toBe(true);
      expect(video.poster).toContain('-poster.png');
    });
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
