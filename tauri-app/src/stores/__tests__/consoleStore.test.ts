import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useConsoleStore } from '../consoleStore';
import { useNotificationStore } from '../notificationStore';

vi.mock('../../services/machineService', () => ({
  machineService: {
    sendGcodeLine: vi.fn(),
    getConsoleLog: vi.fn(),
    clearConsoleLog: vi.fn(),
  },
}));

import { machineService } from '../../services/machineService';

const mockedMachine = machineService as unknown as {
  sendGcodeLine: ReturnType<typeof vi.fn>;
  getConsoleLog: ReturnType<typeof vi.fn>;
  clearConsoleLog: ReturnType<typeof vi.fn>;
};

describe('consoleStore', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useConsoleStore.setState({
      entries: [],
      inputHistory: [],
      historyIndex: -1,
    });
    useNotificationStore.setState({ notifications: [] });
  });

  it('has correct initial state', () => {
    const state = useConsoleStore.getState();
    expect(state.entries).toEqual([]);
    expect(state.inputHistory).toEqual([]);
    expect(state.historyIndex).toBe(-1);
  });

  it('sendCommand adds to history and refreshes log', async () => {
    mockedMachine.sendGcodeLine.mockResolvedValue(undefined);
    mockedMachine.getConsoleLog.mockResolvedValue([
      { timestamp: '2026-01-01T00:00:00Z', direction: 'sent', content: 'G0 X10' },
    ]);

    await expect(useConsoleStore.getState().sendCommand('G0 X10')).resolves.toBe(true);

    expect(mockedMachine.sendGcodeLine).toHaveBeenCalledWith('G0 X10');
    const state = useConsoleStore.getState();
    expect(state.inputHistory).toEqual(['G0 X10']);
    expect(state.historyIndex).toBe(-1);
    expect(state.entries).toHaveLength(1);
  });

  it('sendCommand returns false and preserves history on failure', async () => {
    mockedMachine.sendGcodeLine.mockRejectedValue(new Error('send failed'));

    await expect(useConsoleStore.getState().sendCommand('G0 X10')).resolves.toBe(false);

    expect(useConsoleStore.getState().inputHistory).toEqual([]);
    const notifications = useNotificationStore.getState().notifications;
    expect(notifications[notifications.length - 1]?.type).toBe('error');
  });

  it('refreshLog populates entries', async () => {
    const entries = [
      { timestamp: '2026-01-01T00:00:00Z', direction: 'sent' as const, content: 'G28' },
      { timestamp: '2026-01-01T00:00:01Z', direction: 'received' as const, content: 'ok' },
    ];
    mockedMachine.getConsoleLog.mockResolvedValue(entries);

    await useConsoleStore.getState().refreshLog();

    expect(useConsoleStore.getState().entries).toEqual(entries);
  });

  it('clearLog clears the backend log before emptying entries', async () => {
    mockedMachine.clearConsoleLog.mockResolvedValue(undefined);
    useConsoleStore.setState({
      entries: [{ timestamp: 't', direction: 'sent', content: 'hi' }],
    });

    await useConsoleStore.getState().clearLog();
    expect(mockedMachine.clearConsoleLog).toHaveBeenCalledOnce();
    expect(useConsoleStore.getState().entries).toEqual([]);
  });

  it('clearLog preserves entries when the backend clear fails', async () => {
    mockedMachine.clearConsoleLog.mockRejectedValue(new Error('clear failed'));
    const entries = [{ timestamp: 't', direction: 'sent' as const, content: 'hi' }];
    useConsoleStore.setState({ entries });

    await useConsoleStore.getState().clearLog();

    expect(useConsoleStore.getState().entries).toEqual(entries);
    const notifications = useNotificationStore.getState().notifications;
    expect(notifications[notifications.length - 1]?.type).toBe('error');
  });

  it('clearLog ignores an older refresh that resolves after clearing', async () => {
    let resolveRefresh: ((entries: Array<{ timestamp: string; direction: 'sent'; content: string }>) => void) | undefined;
    mockedMachine.getConsoleLog.mockImplementation(() => new Promise((resolve) => {
      resolveRefresh = resolve;
    }));
    mockedMachine.clearConsoleLog.mockResolvedValue(undefined);

    const refresh = useConsoleStore.getState().refreshLog();
    await useConsoleStore.getState().clearLog();
    resolveRefresh?.([{ timestamp: 't', direction: 'sent', content: 'stale' }]);
    await refresh;

    expect(useConsoleStore.getState().entries).toEqual([]);
  });

  it('historyUp/Down cycles through input history', () => {
    useConsoleStore.setState({
      inputHistory: ['G0 X0', 'G0 X10', 'G0 X20'],
      historyIndex: -1,
    });

    const s = useConsoleStore.getState;

    // First up goes to last entry
    expect(s().historyUp()).toBe('G0 X20');
    expect(s().historyIndex).toBe(2);

    // Second up goes back
    expect(s().historyUp()).toBe('G0 X10');
    expect(s().historyIndex).toBe(1);

    // Down goes forward
    expect(s().historyDown()).toBe('G0 X20');
    expect(s().historyIndex).toBe(2);

    // Down past end resets
    expect(s().historyDown()).toBe('');
    expect(s().historyIndex).toBe(-1);
  });

  it('historyUp clamps at 0', () => {
    useConsoleStore.setState({
      inputHistory: ['only'],
      historyIndex: -1,
    });

    const s = useConsoleStore.getState;
    s().historyUp(); // index = 0
    expect(s().historyUp()).toBe('only');
    expect(s().historyIndex).toBe(0);
  });
});
