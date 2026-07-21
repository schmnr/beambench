import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useMacroStore } from '../macroStore';
import { useNotificationStore } from '../notificationStore';

vi.mock('../../services/macroService', () => ({
  macroService: {
    getMacros: vi.fn(),
    saveMacro: vi.fn(),
    deleteMacro: vi.fn(),
    runMacro: vi.fn(),
  },
}));

import { macroService } from '../../services/macroService';
import type { MacroDefinition } from '../../types/macro';

const mockedMacro = macroService as {
  getMacros: ReturnType<typeof vi.fn>;
  saveMacro: ReturnType<typeof vi.fn>;
  deleteMacro: ReturnType<typeof vi.fn>;
  runMacro: ReturnType<typeof vi.fn>;
};

const sampleMacro: MacroDefinition = {
  id: 'm1',
  name: 'Home XY',
  description: 'Homes the X and Y axes',
  commands: ['G28 X Y'],
};

describe('macroStore', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useMacroStore.setState({ macros: [], loading: false, error: null });
    useNotificationStore.setState({ notifications: [] });
  });

  it('loadMacros fetches and sets macros', async () => {
    mockedMacro.getMacros.mockResolvedValue([sampleMacro]);

    await useMacroStore.getState().loadMacros();

    const state = useMacroStore.getState();
    expect(state.macros).toEqual([sampleMacro]);
    expect(state.loading).toBe(false);
    expect(state.error).toBeNull();
  });

  it('saveMacro saves and reloads', async () => {
    mockedMacro.saveMacro.mockResolvedValue(sampleMacro);
    mockedMacro.getMacros.mockResolvedValue([sampleMacro]);

    await expect(useMacroStore.getState().saveMacro(sampleMacro)).resolves.toBe(true);

    expect(mockedMacro.saveMacro).toHaveBeenCalledWith(sampleMacro);
    expect(mockedMacro.getMacros).toHaveBeenCalled();
    expect(useMacroStore.getState().macros).toEqual([sampleMacro]);
  });

  it('deleteMacro deletes and reloads', async () => {
    mockedMacro.deleteMacro.mockResolvedValue(undefined);
    mockedMacro.getMacros.mockResolvedValue([]);

    await useMacroStore.getState().deleteMacro('m1');

    expect(mockedMacro.deleteMacro).toHaveBeenCalledWith('m1');
    expect(useMacroStore.getState().macros).toEqual([]);
  });

  it('runMacro calls service and notifies success', async () => {
    mockedMacro.runMacro.mockResolvedValue(undefined);

    await useMacroStore.getState().runMacro('m1');

    expect(mockedMacro.runMacro).toHaveBeenCalledWith('m1');
    const notifications = useNotificationStore.getState().notifications;
    expect(notifications).toHaveLength(1);
    expect(notifications[0].type).toBe('success');
  });

  it('handles errors with notification', async () => {
    mockedMacro.getMacros.mockRejectedValue('Load failed');

    await useMacroStore.getState().loadMacros();

    const state = useMacroStore.getState();
    expect(state.error).toBe('Load failed');
    expect(state.loading).toBe(false);
    const notifications = useNotificationStore.getState().notifications;
    expect(notifications).toHaveLength(1);
    expect(notifications[0].type).toBe('error');
  });

  it('saveMacro returns false when the save fails', async () => {
    mockedMacro.saveMacro.mockRejectedValue('Save failed');

    await expect(useMacroStore.getState().saveMacro(sampleMacro)).resolves.toBe(false);

    expect(useMacroStore.getState().error).toBe('Save failed');
  });
});
