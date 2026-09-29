import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ConnectionDiagnosticsPanel } from '../ConnectionDiagnosticsPanel';
import { feedbackService } from '../../../services/feedbackService';
import type { ConnectionDiagnosticsSnapshot } from '../../../types/feedback';

vi.mock('../../../services/feedbackService', () => ({
  feedbackService: {
    getConnectionDiagnostics: vi.fn(),
    saveReport: vi.fn(),
    previewReport: vi.fn(),
    submitReport: vi.fn(),
    revealReport: vi.fn(),
  },
}));

const snapshot: ConnectionDiagnosticsSnapshot = {
  captured_at: '2026-05-15T12:00:00Z',
  ports_detected: [{
    name: '/dev/cu.usbserial-test',
    vendor_id: '0x10c4',
    product_id: '0xea60',
    in_use_by_beambench: false,
    available: true,
  }],
  machine: {
    connected: false,
    model: null,
    firmware_version: null,
    baud_rate: null,
    port_name: null,
    port_vendor_id: null,
    port_product_id: null,
    session_state: 'disconnected',
    handshake_message: 'No GRBL response.',
  },
  connection_events: [],
  recent_serial: {
    tx_hex: '',
    tx_ascii: '',
    rx_hex: '',
    rx_ascii: '',
  },
  known_issues: [],
};

beforeEach(() => {
  vi.mocked(feedbackService.getConnectionDiagnostics).mockResolvedValue(snapshot);
  vi.mocked(feedbackService.saveReport).mockResolvedValue({
    path: '/tmp/beambench-report-connectivity.json',
    size_bytes: 1024,
  });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.clearAllMocks();
});

describe('ConnectionDiagnosticsPanel', () => {
  it('shows BRLTTY guidance only for the diagnostic warning, with stop and restore kept separate', async () => {
    vi.mocked(feedbackService.getConnectionDiagnostics).mockResolvedValue({
      ...snapshot,
      system: { os: 'linux', os_version: 'Linux Mint 21.3', arch: 'x86_64', brltty_running: true, brltty_version: '6.4-4ubuntu3' },
      known_issues: [{ code: 'linux_brltty_possible_conflict', severity: 'warning', message: 'Backend fallback' }],
    });
    render(<ConnectionDiagnosticsPanel />);
    const summary = await screen.findByText('Troubleshoot BRLTTY');
    const details = summary.closest('details')!;
    expect(details.open).toBe(false);
    fireEvent.click(summary);
    expect(details.open).toBe(true);
    expect(screen.getByText(/It may be claiming the adapter/)).toBeDefined();
    expect(screen.getByText(/do not use braille/)).toBeDefined();
    expect(screen.getByText('sudo systemctl stop brltty.service')).toBeDefined();
    expect(screen.getByText('sudo systemctl start brltty.service')).toBeDefined();
    expect(screen.getByText('Linux Mint 21.3')).toBeDefined();
    expect(screen.getByText('BRLTTY 6.4-4ubuntu3')).toBeDefined();
    expect(feedbackService.saveReport).not.toHaveBeenCalled();
  });

  it('does not warn merely because BRLTTY is running', async () => {
    vi.mocked(feedbackService.getConnectionDiagnostics).mockResolvedValue({
      ...snapshot,
      system: { os: 'linux', arch: 'x86_64', brltty_running: true },
    });
    render(<ConnectionDiagnosticsPanel />);
    await screen.findByText('/dev/cu.usbserial-test');
    expect(screen.queryByText('Troubleshoot BRLTTY')).toBeNull();
  });

  it('opens a connectivity report dialog request from Send to Beam Bench', async () => {
    const openHandler = vi.fn();
    window.addEventListener('beam-bench-open-feedback-report', openHandler);

    render(<ConnectionDiagnosticsPanel />);
    expect(await screen.findByText('/dev/cu.usbserial-test')).toBeDefined();

    fireEvent.click(screen.getByRole('button', { name: 'Send to Beam Bench' }));

    expect(openHandler).toHaveBeenCalledWith(expect.objectContaining({
      detail: expect.objectContaining({
        kind: 'connectivity',
        title: 'Connection problem',
      }),
    }));
    window.removeEventListener('beam-bench-open-feedback-report', openHandler);
  });

  it('stops polling when unmounted', async () => {
    const clearIntervalSpy = vi.spyOn(window, 'clearInterval');
    const { unmount } = render(<ConnectionDiagnosticsPanel />);
    await screen.findByText('/dev/cu.usbserial-test');
    expect(feedbackService.getConnectionDiagnostics).toHaveBeenCalledTimes(1);

    unmount();
    await waitFor(() => expect(clearIntervalSpy).toHaveBeenCalled());
  });

  it('renders recent connection events when present', async () => {
    vi.mocked(feedbackService.getConnectionDiagnostics).mockResolvedValue({
      ...snapshot,
      connection_events: [{
        ts: '2026-05-15T12:00:00Z',
        stage: 'banner_timeout',
        port_name: '/dev/cu.usbserial-test',
        baud_rate: 115200,
        message: null,
        error: 'Timeout waiting for GRBL banner',
      }],
    });

    render(<ConnectionDiagnosticsPanel />);

    expect(await screen.findByText('Connection Events')).toBeDefined();
    expect(await screen.findByText(/banner_timeout/)).toBeDefined();
    expect(await screen.findByText(/Timeout waiting for GRBL banner/)).toBeDefined();
  });
});
