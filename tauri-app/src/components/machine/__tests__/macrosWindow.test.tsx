import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent } from '@testing-library/react';

import { MacrosWindow } from '../MacrosWindow.js';
import { useMacroStore } from '../../../stores/macroStore.js';
import { useNotificationStore } from '../../../stores/notificationStore.js';
import type { MacroDefinition } from '../../../types/macro.js';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const initialState = useMacroStore.getState();
const initialNotificationState = useNotificationStore.getState();

afterEach(() => {
  cleanup();
  useMacroStore.setState(initialState, true);
  useNotificationStore.setState(initialNotificationState, true);
});

const sampleMacros: MacroDefinition[] = [
  { id: 'm1', name: 'Home All', description: 'Homes all axes', commands: ['G28'] },
  { id: 'm2', name: 'Zero Work', description: 'Zeros the work coordinates', commands: ['G92 X0 Y0'] },
];

describe('MacrosWindow', () => {
  it('renders macro list with names', () => {
    useMacroStore.setState({ macros: sampleMacros });
    render(<MacrosWindow />);
    expect(screen.getByText('Home All')).toBeDefined();
    expect(screen.getByText('Zero Work')).toBeDefined();
  });

  it('play button calls runMacro with correct id', () => {
    const runMacro = vi.fn();
    useMacroStore.setState({ macros: sampleMacros, runMacro });
    render(<MacrosWindow />);
    fireEvent.click(screen.getAllByTitle('Run')[0]);
    expect(runMacro).toHaveBeenCalledWith('m1');
  });

  it('add button creates new macro', () => {
    const saveMacro = vi.fn();
    useMacroStore.setState({ saveMacro });
    render(<MacrosWindow />);
    fireEvent.click(screen.getByText('+ Add Macro'));
    expect(saveMacro).toHaveBeenCalledOnce();
  });

  it('delete button calls deleteMacro', () => {
    const deleteMacro = vi.fn();
    useMacroStore.setState({ macros: sampleMacros, deleteMacro });
    render(<MacrosWindow />);
    fireEvent.click(screen.getAllByTitle('Delete')[0]);
    expect(deleteMacro).toHaveBeenCalledWith('m1');
  });

  it('blocks saving a conflicting hotkey and keeps the editor open', () => {
    const saveMacro = vi.fn();
    const push = vi.fn();
    useNotificationStore.setState({ push });
    useMacroStore.setState({ macros: sampleMacros, saveMacro });

    render(<MacrosWindow />);

    fireEvent.click(screen.getAllByTitle('Edit')[0]);
    fireEvent.change(screen.getByTestId('hotkey-input'), { target: { value: 'Ctrl+S' } });
    fireEvent.click(screen.getByText('Save'));

    expect(saveMacro).not.toHaveBeenCalled();
    expect(push).toHaveBeenCalledWith('Conflicts with built-in shortcut "Ctrl+S"', 'error');
    expect(screen.getByTestId('hotkey-input')).toBeDefined();
  });

  it.each(['Cmd+Z', 'Command+Z', 'Meta+Z'])(
    'blocks saving a macOS-equivalent alias of a built-in shortcut (%s)',
    (hotkey) => {
    const saveMacro = vi.fn();
    const push = vi.fn();
    useNotificationStore.setState({ push });
    useMacroStore.setState({ macros: sampleMacros, saveMacro });

    render(<MacrosWindow />);

    fireEvent.click(screen.getAllByTitle('Edit')[0]);
    fireEvent.change(screen.getByTestId('hotkey-input'), { target: { value: hotkey } });
    fireEvent.click(screen.getByText('Save'));

    expect(saveMacro).not.toHaveBeenCalled();
    expect(push).toHaveBeenCalledWith('Conflicts with built-in shortcut "Ctrl+Z"', 'error');
    expect(screen.getByTestId('hotkey-input')).toBeDefined();
    },
  );

  it('blocks saving a semantic duplicate of another macro hotkey', () => {
    const saveMacro = vi.fn();
    const push = vi.fn();
    useNotificationStore.setState({ push });
    useMacroStore.setState({
      macros: [
        { ...sampleMacros[0], hotkey: 'Ctrl+1' },
        sampleMacros[1],
      ],
      saveMacro,
    });

    render(<MacrosWindow />);

    fireEvent.click(screen.getAllByTitle('Edit')[1]);
    fireEvent.change(screen.getByTestId('hotkey-input'), { target: { value: 'Cmd+1' } });
    fireEvent.click(screen.getByText('Save'));

    expect(saveMacro).not.toHaveBeenCalled();
    expect(push).toHaveBeenCalledWith('Conflicts with macro "Home All"', 'error');
    expect(screen.getByTestId('hotkey-input')).toBeDefined();
  });

  it('keeps the editor open when saving fails', async () => {
    const saveMacro = vi.fn().mockResolvedValue(false);
    const loadMacros = vi.fn().mockResolvedValue(undefined);
    useMacroStore.setState({ macros: sampleMacros, saveMacro, loadMacros });

    render(<MacrosWindow />);

    fireEvent.click(screen.getAllByTitle('Edit')[0]);
    fireEvent.change(screen.getByDisplayValue('Home All'), { target: { value: 'Home All Updated' } });
    fireEvent.click(screen.getByText('Save'));

    expect(saveMacro).toHaveBeenCalledOnce();
    expect(await screen.findByDisplayValue('Home All Updated')).toBeDefined();
    expect(screen.getByText('Cancel')).toBeDefined();
  });

  it('guards switching macros when the current edit is dirty', () => {
    useMacroStore.setState({ macros: sampleMacros });

    render(<MacrosWindow />);

    fireEvent.click(screen.getAllByTitle('Edit')[0]);
    fireEvent.change(screen.getByDisplayValue('Home All'), { target: { value: 'Dirty Macro' } });
    fireEvent.click(screen.getAllByTitle('Edit')[0]);

    expect(screen.getByText('Discard unsaved macro changes?')).toBeDefined();
    expect(screen.getByDisplayValue('Dirty Macro')).toBeDefined();

    fireEvent.click(screen.getByRole('button', { name: 'Keep Editing' }));
    expect(screen.queryByText('Discard unsaved macro changes?')).toBeNull();
    expect(screen.getByDisplayValue('Dirty Macro')).toBeDefined();
  });
});
