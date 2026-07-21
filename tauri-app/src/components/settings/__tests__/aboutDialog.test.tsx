import { describe, it, expect, vi, afterEach } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';

import { AboutDialog } from '../AboutDialog';
import { useAppStore } from '../../../stores/appStore';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));

const initialAppState = useAppStore.getState();

afterEach(() => {
  cleanup();
  useAppStore.setState(initialAppState, true);
});

describe('AboutDialog', () => {
  it('closes on Escape', () => {
    const onClose = vi.fn();
    render(<AboutDialog onClose={onClose} />);

    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });

    expect(onClose).toHaveBeenCalled();
  });

  it('closes on backdrop click', () => {
    const onClose = vi.fn();
    render(<AboutDialog onClose={onClose} />);

    fireEvent.click(screen.getByRole('dialog'));

    expect(onClose).toHaveBeenCalled();
  });

  it('closes on Close button', () => {
    const onClose = vi.fn();
    render(<AboutDialog onClose={onClose} />);

    fireEvent.click(screen.getByText('Close'));

    expect(onClose).toHaveBeenCalled();
  });

  it('renders the app identity, tagline, and version', () => {
    useAppStore.setState({
      status: { version: '0.1.0', state: 'ready' },
    });

    render(<AboutDialog onClose={vi.fn()} />);

    expect(screen.getByRole('dialog', { name: 'Beam Bench' })).toBeDefined();
    expect(screen.getByText('Built by makers for makers.')).toBeDefined();
    expect(screen.getByText('Beta')).toBeDefined();
    expect(screen.getByText('0.1.0')).toBeDefined();
  });

  it('exposes the community and Craftgineer entry points', () => {
    render(<AboutDialog onClose={vi.fn()} />);

    expect(screen.getByText(/Join the community on Facebook/)).toBeDefined();
    expect(screen.getByText(/Craftgineer/)).toBeDefined();
  });

  it('shows the GPL source and Potrace notices', () => {
    render(<AboutDialog onClose={vi.fn()} />);

    expect(screen.getByText(/GNU General Public License/)).toBeDefined();
    expect(screen.getByText(/Copyright © 2001–2019 Peter Selinger/)).toBeDefined();
    expect(screen.getByRole('button', { name: /Source code/ })).toBeDefined();
    expect(screen.getByRole('button', { name: /GPL license/ })).toBeDefined();
    expect(screen.getByRole('button', { name: /Potrace/ })).toBeDefined();
  });
});
