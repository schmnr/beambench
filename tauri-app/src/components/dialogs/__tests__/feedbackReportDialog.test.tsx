import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { FeedbackReportDialog } from '../FeedbackReportDialog';
import { feedbackService } from '../../../services/feedbackService';
import type { DiagnosticBundleV1 } from '../../../types/feedback';

vi.mock('../../../services/feedbackService', () => ({
  feedbackService: {
    previewReport: vi.fn(),
    saveReport: vi.fn(),
    submitReport: vi.fn(),
    revealReport: vi.fn(),
    getConnectionDiagnostics: vi.fn(),
  },
}));

const sampleBundle: DiagnosticBundleV1 = {
  schema_version: 1,
  kind: 'bug',
  created_at: '2026-05-15T12:00:00Z',
  client: {
    app_version: '0.1.0',
    tauri_version: '2.x',
    rust_version: '1.x',
    build_target: 'aarch64-apple-darwin',
    git_sha: 'abc123',
  },
  system: {
    os: 'macOS',
    os_version: '14.6.0',
    arch: 'aarch64',
    locale: 'en-US',
  },
  machine: {
    connected: false,
    model: null,
    firmware_version: null,
    baud_rate: null,
    port_name: null,
    port_vendor_id: null,
    port_product_id: null,
    session_state: 'disconnected',
    handshake_message: null,
  },
  ports_detected: [],
  connection_events: [],
  recent_serial: {
    tx_hex: '',
    tx_ascii: '',
    rx_hex: '',
    rx_ascii: '',
  },
  recent_logs: [],
  recent_panics: [],
  project_metadata: {
    object_count: 1,
    size_bytes: 1024,
    has_raster: false,
    has_vector: true,
    has_text: false,
    project_path: '<userhome>/Documents/test.lzrproj',
  },
  known_issues: [],
  project_file_attached: false,
  source_context: null,
};

beforeEach(() => {
  vi.mocked(feedbackService.previewReport).mockResolvedValue(sampleBundle);
  vi.mocked(feedbackService.saveReport).mockResolvedValue({
    path: '/tmp/beambench-report-bug.json',
    size_bytes: 2048,
  });
  vi.mocked(feedbackService.submitReport).mockResolvedValue({ report_id: 'r-abc12345' });
  Object.defineProperty(navigator, 'clipboard', {
    configurable: true,
    value: { writeText: vi.fn().mockResolvedValue(undefined) },
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('FeedbackReportDialog', () => {
  it('shows a validation message instead of silently ignoring submit without a description', async () => {
    render(<FeedbackReportDialog kind="bug" onClose={vi.fn()} />);

    const submit = screen.getByRole('button', { name: 'Submit' });
    expect((submit as HTMLButtonElement).disabled).toBe(false);

    fireEvent.click(submit);

    expect(await screen.findByText('Description is required before submitting a bug report.')).toBeDefined();
    expect(feedbackService.submitReport).not.toHaveBeenCalled();

    fireEvent.change(screen.getByLabelText('Description'), {
      target: { value: 'The laser will not connect.' },
    });

    fireEvent.click(submit);

    await waitFor(() => expect(feedbackService.submitReport).toHaveBeenCalled());
  });

  it('submits through the feedback service and shows the returned report ID', async () => {
    render(<FeedbackReportDialog kind="bug" title="Connection failure" description="GRBL never responds." onClose={vi.fn()} />);

    fireEvent.change(screen.getByLabelText('Reply-to email'), {
      target: { value: 'founder@example.com' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Submit' }));

    await waitFor(() => {
      expect(feedbackService.submitReport).toHaveBeenCalledWith(expect.objectContaining({
        kind: 'bug',
        title: 'Connection failure',
        description: 'GRBL never responds.',
        reply_to_email: 'founder@example.com',
        include_project_file: false,
      }));
    });
    expect(await screen.findByText('Report Submitted')).toBeDefined();
    expect(screen.getByText('r-abc12345')).toBeDefined();

    fireEvent.click(screen.getByRole('button', { name: 'Copy' }));
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith('r-abc12345');
  });

  it('shows visible progress immediately after submit is clicked', async () => {
    let resolveSubmit: ((value: { report_id: string }) => void) | undefined;
    vi.mocked(feedbackService.submitReport).mockReturnValueOnce(new Promise((resolve) => {
      resolveSubmit = resolve;
    }));
    render(<FeedbackReportDialog kind="bug" title="Slow submit" description="Network is slow." onClose={vi.fn()} />);

    fireEvent.click(screen.getByRole('button', { name: 'Submit' }));

    expect((await screen.findByRole('status')).textContent).toContain('Submitting report to Beam Bench...');
    expect((screen.getByRole('button', { name: 'Submitting...' }) as HTMLButtonElement).disabled).toBe(true);

    resolveSubmit?.({ report_id: 'r-slow123' });
    expect(await screen.findByText('r-slow123')).toBeDefined();
  });

  it('preserves the form and save fallback when submit fails', async () => {
    vi.mocked(feedbackService.submitReport).mockRejectedValueOnce(new Error('Too many reports recently. Save the report to a file and try again later.'));
    render(<FeedbackReportDialog kind="bug" title="Port blocked" description="It fails after connect." onClose={vi.fn()} />);

    fireEvent.click(screen.getByRole('button', { name: 'Submit' }));

    expect(await screen.findByText('Operation failed: Error: Too many reports recently. Save the report to a file and try again later.')).toBeDefined();
    expect(screen.getByDisplayValue('Port blocked')).toBeDefined();
    expect(screen.getByDisplayValue('It fails after connect.')).toBeDefined();
    expect((screen.getByRole('button', { name: 'Save Report to File' }) as HTMLButtonElement).disabled).toBe(false);
  });

  it('keeps the save-to-file success path', async () => {
    render(<FeedbackReportDialog kind="bug" title="Save local" description="I want a file." onClose={vi.fn()} />);

    fireEvent.click(screen.getByRole('button', { name: 'Save Report to File' }));

    expect(await screen.findByText('Report Saved')).toBeDefined();
    expect(screen.getByText('/tmp/beambench-report-bug.json')).toBeDefined();
  });

  it('shows the schema summary and scrubbed actual-content preview', async () => {
    render(<FeedbackReportDialog kind="connectivity" title="Connection problem" onClose={vi.fn()} />);

    expect(screen.getByText('project file blob')).toBeDefined();
    expect(screen.getByText('bundle.recent logs')).toBeDefined();

    fireEvent.click(screen.getByRole('button', { name: 'What gets sent' }));

    expect(await screen.findByText(/"<userhome>\/Documents\/test.lzrproj"/)).toBeDefined();
  });

  it('clears the actual-content preview when report fields change', async () => {
    render(<FeedbackReportDialog kind="connectivity" title="Connection problem" onClose={vi.fn()} />);

    fireEvent.click(screen.getByRole('button', { name: 'What gets sent' }));
    expect(await screen.findByText(/"<userhome>\/Documents\/test.lzrproj"/)).toBeDefined();

    fireEvent.change(screen.getByLabelText('Title'), {
      target: { value: 'Updated connection problem' },
    });

    expect(screen.queryByText(/"<userhome>\/Documents\/test.lzrproj"/)).toBeNull();

    const callsBeforeReopen = vi.mocked(feedbackService.previewReport).mock.calls.length;
    fireEvent.click(screen.getByRole('button', { name: 'What gets sent' }));

    await waitFor(() => {
      expect(vi.mocked(feedbackService.previewReport).mock.calls.length).toBeGreaterThan(callsBeforeReopen);
    });
  });
});
