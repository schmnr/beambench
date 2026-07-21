import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup } from '@testing-library/react';
import { JobProgressBar } from '../JobProgressBar';
import { useMachineStore } from '../../../stores/machineStore';
import { makeJobProgress } from '../../../test-utils/projectFixtures';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockReturnValue(new Promise(() => {})),
}));

afterEach(() => {
  cleanup();
  useMachineStore.setState({ jobProgress: null });
});

// New i18n key is not in the locale files yet; i18next returns the raw key.
const FINISHING_KEY = /Finishing, waiting for the machine/;

describe('JobProgressBar', () => {
  it('renders nothing without job progress', () => {
    const { container } = render(<JobProgressBar />);
    expect(container.firstChild).toBeNull();
  });

  it('shows the finishing label and pulses the bar when running at >= 99.9%', () => {
    useMachineStore.setState({
      jobProgress: makeJobProgress({
        state: 'running',
        total_lines: 1000,
        acknowledged_lines: 1000,
      }),
    });
    const { container } = render(<JobProgressBar />);

    expect(screen.getByText(FINISHING_KEY)).toBeTruthy();
    const bar = container.querySelector('.bg-bb-accent');
    expect(bar?.className).toContain('animate-pulse');
  });

  it('does not show the finishing label mid-job', () => {
    useMachineStore.setState({
      jobProgress: makeJobProgress({
        state: 'running',
        total_lines: 1000,
        acknowledged_lines: 500,
      }),
    });
    const { container } = render(<JobProgressBar />);

    expect(screen.queryByText(FINISHING_KEY)).toBeNull();
    const bar = container.querySelector('.bg-bb-accent');
    expect(bar?.className).not.toContain('animate-pulse');
  });

  it('does not show the finishing label when a non-running job sits at 100%', () => {
    useMachineStore.setState({
      jobProgress: makeJobProgress({
        state: 'completed',
        total_lines: 1000,
        acknowledged_lines: 1000,
      }),
    });
    render(<JobProgressBar />);

    expect(screen.queryByText(FINISHING_KEY)).toBeNull();
  });

  it('shows the failure reason for failed jobs', () => {
    useMachineStore.setState({
      jobProgress: makeJobProgress({
        state: 'failed',
        error_message: 'Controller reported Idle while bytes were still waiting for acknowledgement.',
      }),
    });
    render(<JobProgressBar />);

    expect(
      screen.getByText('Controller reported Idle while bytes were still waiting for acknowledgement.'),
    ).toBeTruthy();
  });
});
