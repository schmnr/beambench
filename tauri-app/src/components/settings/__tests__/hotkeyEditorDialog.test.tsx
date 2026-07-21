import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { HotkeyEditorDialog } from '../HotkeyEditorDialog';
import { useAppStore } from '../../../stores/appStore';
import { useMacroStore } from '../../../stores/macroStore';
import { useNotificationStore } from '../../../stores/notificationStore';
import { makeAppSettings } from '../../../test-utils/projectFixtures';
import i18n from '../../../i18n';

const initialAppState = useAppStore.getState();
const initialMacroState = useMacroStore.getState();
const initialNotificationState = useNotificationStore.getState();

afterEach(async () => {
  cleanup();
  vi.restoreAllMocks();
  await i18n.changeLanguage('en');
  useAppStore.setState(initialAppState, true);
  useMacroStore.setState(initialMacroState, true);
  useNotificationStore.setState(initialNotificationState, true);
});

describe('HotkeyEditorDialog', () => {
  it('assigns and saves a canonical custom hotkey', async () => {
    const updateSettings = vi.fn().mockResolvedValue(undefined);
    const push = vi.fn();
    const onClose = vi.fn();
    useAppStore.setState({
      settings: makeAppSettings(),
      updateSettings,
    });
    useNotificationStore.setState({ push });

    render(<HotkeyEditorDialog onClose={onClose} />);
    expect(screen.getByTestId('hotkey-editor-dialog-drag-handle')).toBeDefined();
    expect(screen.getByTestId('hotkey-editor-dialog-resize-handle')).toBeDefined();
    fireEvent.change(screen.getByPlaceholderText('Search commands'), { target: { value: 'Rectangle' } });
    fireEvent.click(screen.getByRole('button', { name: 'Assign' }));
    fireEvent.keyDown(window, { key: 'R', ctrlKey: true, shiftKey: true });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => {
      expect(updateSettings).toHaveBeenCalledWith({
        custom_hotkeys: { 'tools.rectangle': 'Ctrl+Shift+r' },
      });
      expect(push).toHaveBeenCalledWith('Hotkeys updated.', 'success');
      expect(onClose).toHaveBeenCalledOnce();
    });
  });

  it('rejects conventionally reserved platform-equivalent hotkeys', () => {
    useAppStore.setState({ settings: makeAppSettings() });

    render(<HotkeyEditorDialog onClose={vi.fn()} />);
    fireEvent.change(screen.getByPlaceholderText('Search commands'), { target: { value: 'Rectangle' } });
    fireEvent.click(screen.getByRole('button', { name: 'Assign' }));
    fireEvent.keyDown(window, { key: 'q', metaKey: true });

    expect(screen.getByText('Ctrl+q is reserved.')).toBeDefined();
  });

  it('renders and searches command labels in the active language', async () => {
    await i18n.changeLanguage('de');
    useAppStore.setState({ settings: makeAppSettings() });

    render(<HotkeyEditorDialog onClose={vi.fn()} />);
    fireEvent.change(screen.getByPlaceholderText('Suchbefehle'), { target: { value: 'Rechteck' } });

    expect(screen.getByText('Rechteck')).toBeDefined();
    expect(screen.getAllByText('Werkzeuge').length).toBeGreaterThan(0);
    expect(screen.queryByText('Rectangle')).toBeNull();
  });

  it('moves and resizes from the window chrome', () => {
    useAppStore.setState({ settings: makeAppSettings() });

    render(<HotkeyEditorDialog onClose={vi.fn()} />);
    const dialog = screen.getByTestId('hotkey-editor-dialog');
    const startLeft = parseFloat(dialog.style.left);
    const startWidth = parseFloat(dialog.style.width);

    fireEvent.mouseDown(screen.getByTestId('hotkey-editor-dialog-drag-handle'), { clientX: 100, clientY: 100 });
    fireEvent.mouseMove(document, { clientX: 140, clientY: 130 });
    fireEvent.mouseUp(document);
    expect(parseFloat(dialog.style.left)).toBe(startLeft + 40);

    fireEvent.mouseDown(screen.getByTestId('hotkey-editor-dialog-resize-handle'), { clientX: 300, clientY: 300 });
    fireEvent.mouseMove(document, { clientX: 360, clientY: 340 });
    fireEvent.mouseUp(document);
    expect(parseFloat(dialog.style.width)).toBe(startWidth + 60);
  });
});
