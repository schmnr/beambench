import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent, waitFor, act } from '@testing-library/react';
import { ConsoleWindow } from '../ConsoleWindow';
import { useConsoleStore } from '../../../stores/consoleStore';

// jsdom doesn't have Element.scrollTo, so stub it.
Element.prototype.scrollTo = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockReturnValue(new Promise(() => {})),
}));

const initialState = useConsoleStore.getState();

beforeEach(() => {
  useConsoleStore.setState({ refreshLog: vi.fn().mockResolvedValue(undefined) });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  useConsoleStore.setState(initialState, true);
});

describe('ConsoleWindow', () => {
  it('renders log entries with direction arrows', () => {
    useConsoleStore.setState({
      entries: [
        { timestamp: '12:00', direction: 'sent', content: 'G0 X10' },
        { timestamp: '12:01', direction: 'received', content: 'ok' },
      ],
    });
    render(<ConsoleWindow />);
    expect(screen.getByText('→')).toBeTruthy();
    expect(screen.getByText('←')).toBeTruthy();
    expect(screen.getByText('G0 X10')).toBeTruthy();
    expect(screen.getByText('ok')).toBeTruthy();
  });

  it('send button calls sendCommand', async () => {
    const sendCommand = vi.fn().mockResolvedValue(true);
    useConsoleStore.setState({ sendCommand });
    render(<ConsoleWindow />);
    const input = screen.getByPlaceholderText('G-code...');
    fireEvent.change(input, { target: { value: 'G28' } });
    fireEvent.click(screen.getByText('Send'));
    expect(sendCommand).toHaveBeenCalledWith('G28');
    await waitFor(() => {
      expect((input as HTMLInputElement).value).toBe('');
    });
  });

  it('keeps the input value when sendCommand fails', async () => {
    const sendCommand = vi.fn().mockResolvedValue(false);
    useConsoleStore.setState({ sendCommand });
    render(<ConsoleWindow />);
    const input = screen.getByPlaceholderText('G-code...');
    fireEvent.change(input, { target: { value: 'G1 X5' } });
    fireEvent.click(screen.getByText('Send'));
    await waitFor(() => {
      expect(sendCommand).toHaveBeenCalledWith('G1 X5');
    });
    expect((input as HTMLInputElement).value).toBe('G1 X5');
  });

  it('up arrow navigates command history', () => {
    const historyUp = vi.fn().mockReturnValue('G0 X0');
    useConsoleStore.setState({ historyUp });
    render(<ConsoleWindow />);
    const input = screen.getByPlaceholderText('G-code...');
    fireEvent.keyDown(input, { key: 'ArrowUp' });
    expect(historyUp).toHaveBeenCalled();
    expect((input as HTMLInputElement).value).toBe('G0 X0');
  });

  it('clear button calls clearLog', () => {
    const clearLog = vi.fn().mockResolvedValue(undefined);
    useConsoleStore.setState({ clearLog });
    render(<ConsoleWindow />);
    fireEvent.click(screen.getByText('Clear'));
    expect(clearLog).toHaveBeenCalled();
  });

  it('refreshes the backing log while the console is mounted', () => {
    vi.useFakeTimers();
    const refreshLog = vi.fn().mockResolvedValue(undefined);
    useConsoleStore.setState({ refreshLog });
    const { unmount } = render(<ConsoleWindow />);
    expect(refreshLog).toHaveBeenCalledTimes(1);

    act(() => {
      vi.advanceTimersByTime(500);
    });
    expect(refreshLog).toHaveBeenCalledTimes(2);

    unmount();
    act(() => {
      vi.advanceTimersByTime(500);
    });
    expect(refreshLog).toHaveBeenCalledTimes(2);
  });

  describe('smart autoscroll', () => {
    function setupLog(scrollTop: number) {
      useConsoleStore.setState({
        entries: [{ timestamp: '12:00', direction: 'received', content: 'ok' }],
      });
      render(<ConsoleWindow />);
      const log = screen.getByTestId('console-log') as HTMLDivElement;
      // jsdom has no layout; fake a scrollable log.
      Object.defineProperty(log, 'scrollHeight', { value: 200, configurable: true });
      Object.defineProperty(log, 'clientHeight', { value: 100, configurable: true });
      log.scrollTop = scrollTop;
      fireEvent.scroll(log);
      return log;
    }

    it('follows new entries when the user is within 8px of the bottom', () => {
      // 200 - 92 - 100 = 8px from the bottom → still "stuck"
      const log = setupLog(92);

      act(() => {
        useConsoleStore.setState({
          entries: [
            { timestamp: '12:00', direction: 'received', content: 'ok' },
            { timestamp: '12:01', direction: 'received', content: 'ok' },
          ],
        });
      });

      expect(log.scrollTop).toBe(200);
    });

    it('preserves the scroll position when the user has scrolled up', () => {
      // 200 - 40 - 100 = 60px from the bottom → user is reading history
      const log = setupLog(40);

      act(() => {
        useConsoleStore.setState({
          entries: [
            { timestamp: '12:00', direction: 'received', content: 'ok' },
            { timestamp: '12:01', direction: 'received', content: 'ok' },
          ],
        });
      });

      expect(log.scrollTop).toBe(40);
    });
  });

  it('error entries have red text (derived from content prefix )', () => {
    useConsoleStore.setState({
      entries: [
        {
          timestamp: '12:00',
          direction: 'received',
          content: 'error:1',
        },
      ],
    });
    render(<ConsoleWindow />);
    const entryDiv = screen.getByText('error:1').closest('div');
    expect(entryDiv?.className.includes('text-bb-error-fg')).toBe(true);
  });
});
