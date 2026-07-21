import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent, waitFor } from '@testing-library/react';
import {
  ContextMenu,
  type ContextMenuEntry,
  type ContextMenuSubmenu,
  type ContextMenuCheckItem,
} from '../ContextMenu';

afterEach(cleanup);

function makeItems(): ContextMenuEntry[] {
  return [
    { id: 'cut', label: 'Cut', shortcut: 'Ctrl+X', onClick: vi.fn() },
    { id: 'copy', label: 'Copy', shortcut: 'Ctrl+C', onClick: vi.fn() },
    { type: 'separator' },
    { id: 'paste', label: 'Paste', shortcut: 'Ctrl+V', disabled: true, onClick: vi.fn() },
    { id: 'delete', label: 'Delete', onClick: vi.fn() },
  ];
}

function makeSubmenuItems(): ContextMenuEntry[] {
  const submenu: ContextMenuSubmenu = {
    type: 'submenu',
    id: 'windows',
    label: 'Windows',
    children: [
      { type: 'check', id: 'panel-a', label: 'Panel A', checked: true, onClick: vi.fn() },
      { type: 'check', id: 'panel-b', label: 'Panel B', checked: false, onClick: vi.fn() },
    ],
  };
  return [
    submenu,
    { type: 'separator' },
    { id: 'action', label: 'Action', onClick: vi.fn() },
  ];
}

function makeCheckItems(): ContextMenuEntry[] {
  return [
    { type: 'check', id: 'check-on', label: 'Checked', checked: true, onClick: vi.fn() } as ContextMenuCheckItem,
    { type: 'check', id: 'check-off', label: 'Unchecked', checked: false, onClick: vi.fn() } as ContextMenuCheckItem,
  ];
}

describe('ContextMenu', () => {
  it('renders all items', () => {
    const items = makeItems();
    const onClose = vi.fn();
    render(<ContextMenu x={100} y={100} items={items} onClose={onClose} />);

    expect(screen.getByTestId('context-menu')).toBeDefined();
    expect(screen.getByText('Cut')).toBeDefined();
    expect(screen.getByText('Copy')).toBeDefined();
    expect(screen.getByText('Paste')).toBeDefined();
    expect(screen.getByText('Delete')).toBeDefined();
  });

  it('renders separator elements', () => {
    const items = makeItems();
    const onClose = vi.fn();
    render(<ContextMenu x={100} y={100} items={items} onClose={onClose} />);

    const menu = screen.getByTestId('context-menu');
    const separators = menu.querySelectorAll('.border-t');
    expect(separators.length).toBe(1);
  });

  it('renders shortcut labels', () => {
    const items = makeItems();
    const onClose = vi.fn();
    render(<ContextMenu x={100} y={100} items={items} onClose={onClose} />);

    expect(screen.getByText('Ctrl+X')).toBeDefined();
    expect(screen.getByText('Ctrl+C')).toBeDefined();
  });

  it('disabled items are not clickable', () => {
    const items = makeItems();
    const onClose = vi.fn();
    render(<ContextMenu x={100} y={100} items={items} onClose={onClose} />);

    const pasteBtn = screen.getByTestId('context-menu-item-paste');
    fireEvent.click(pasteBtn);

    const pasteItem = items[3] as unknown as { onClick: ReturnType<typeof vi.fn> };
    expect(pasteItem.onClick).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });

  it('clicking an enabled item fires onClick and onClose', () => {
    const items = makeItems();
    const onClose = vi.fn();
    render(<ContextMenu x={100} y={100} items={items} onClose={onClose} />);

    const cutBtn = screen.getByTestId('context-menu-item-cut');
    fireEvent.click(cutBtn);

    const cutItem = items[0] as unknown as { onClick: ReturnType<typeof vi.fn> };
    expect(cutItem.onClick).toHaveBeenCalledOnce();
    expect(onClose).toHaveBeenCalledOnce();
  });

  it('click-outside closes the menu', () => {
    const items = makeItems();
    const onClose = vi.fn();
    render(<ContextMenu x={100} y={100} items={items} onClose={onClose} />);

    fireEvent.mouseDown(document.body);
    expect(onClose).toHaveBeenCalledOnce();
  });

  it('Escape key closes the menu', () => {
    const items = makeItems();
    const onClose = vi.fn();
    render(<ContextMenu x={100} y={100} items={items} onClose={onClose} />);

    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledOnce();
  });
});

describe('ContextMenu submenu', () => {
  it('renders submenu item with arrow indicator', () => {
    const items = makeSubmenuItems();
    const onClose = vi.fn();
    render(<ContextMenu x={100} y={100} items={items} onClose={onClose} />);

    const windowsBtn = screen.getByTestId('context-menu-item-windows');
    expect(windowsBtn).toBeDefined();
    // Arrow indicator (▸)
    expect(windowsBtn.textContent).toContain('\u25B8');
  });

  it('submenu opens on hover and shows children', async () => {
    const items = makeSubmenuItems();
    const onClose = vi.fn();
    render(<ContextMenu x={100} y={100} items={items} onClose={onClose} />);

    const windowsBtn = screen.getByTestId('context-menu-item-windows');
    fireEvent.mouseEnter(windowsBtn.parentElement!);

    await waitFor(() => {
      expect(screen.getByTestId('context-submenu')).toBeDefined();
    });

    expect(screen.getByText('Panel A')).toBeDefined();
    expect(screen.getByText('Panel B')).toBeDefined();
  });
});

describe('ContextMenu check items', () => {
  it('renders checkmark when checked', () => {
    const items = makeCheckItems();
    const onClose = vi.fn();
    render(<ContextMenu x={100} y={100} items={items} onClose={onClose} />);

    const checkedBtn = screen.getByTestId('context-menu-item-check-on');
    expect(checkedBtn.textContent).toContain('\u2713');
  });

  it('renders empty when unchecked', () => {
    const items = makeCheckItems();
    const onClose = vi.fn();
    render(<ContextMenu x={100} y={100} items={items} onClose={onClose} />);

    const uncheckedBtn = screen.getByTestId('context-menu-item-check-off');
    expect(uncheckedBtn.textContent).not.toContain('\u2713');
  });

  it('check item click fires onClick and closes menu', () => {
    const items = makeCheckItems();
    const onClose = vi.fn();
    render(<ContextMenu x={100} y={100} items={items} onClose={onClose} />);

    const checkedBtn = screen.getByTestId('context-menu-item-check-on');
    fireEvent.click(checkedBtn);

    const checkItem = items[0] as ContextMenuCheckItem;
    expect(checkItem.onClick).toHaveBeenCalledOnce();
    expect(onClose).toHaveBeenCalledOnce();
  });
});

describe('ContextMenu keyboard navigation', () => {
  // Helper: get the inner container that has onKeyDown
  function getKeyTarget() {
    const menu = screen.getByTestId('context-menu');
    // The onKeyDown handler is on the first child div inside the menu
    return menu.firstElementChild as HTMLElement;
  }

  it('ArrowDown moves focus to first item', () => {
    const items = makeItems();
    const onClose = vi.fn();
    render(<ContextMenu x={100} y={100} items={items} onClose={onClose} />);

    fireEvent.keyDown(getKeyTarget(), { key: 'ArrowDown' });

    const cutBtn = screen.getByTestId('context-menu-item-cut');
    expect(document.activeElement).toBe(cutBtn);
  });

  it('ArrowDown skips separators', () => {
    const items = makeItems();
    const onClose = vi.fn();
    render(<ContextMenu x={100} y={100} items={items} onClose={onClose} />);

    const target = getKeyTarget();
    // Move to cut (index 0)
    fireEvent.keyDown(target, { key: 'ArrowDown' });
    // Move to copy (index 1)
    fireEvent.keyDown(target, { key: 'ArrowDown' });
    // Move past separator to paste (index 3)
    fireEvent.keyDown(target, { key: 'ArrowDown' });

    const pasteBtn = screen.getByTestId('context-menu-item-paste');
    expect(document.activeElement).toBe(pasteBtn);
  });

  it('Enter activates focused item', () => {
    const items = makeItems();
    const onClose = vi.fn();
    render(<ContextMenu x={100} y={100} items={items} onClose={onClose} />);

    const target = getKeyTarget();
    // Move to cut
    fireEvent.keyDown(target, { key: 'ArrowDown' });
    // Activate
    fireEvent.keyDown(target, { key: 'Enter' });

    const cutItem = items[0] as unknown as { onClick: ReturnType<typeof vi.fn> };
    expect(cutItem.onClick).toHaveBeenCalledOnce();
    expect(onClose).toHaveBeenCalledOnce();
  });

  it('ArrowRight opens submenu on focused submenu item', async () => {
    const items = makeSubmenuItems();
    const onClose = vi.fn();
    render(<ContextMenu x={100} y={100} items={items} onClose={onClose} />);

    const target = getKeyTarget();
    // Move to submenu item (first item)
    fireEvent.keyDown(target, { key: 'ArrowDown' });
    // Open submenu
    fireEvent.keyDown(target, { key: 'ArrowRight' });

    await waitFor(() => {
      expect(screen.getByTestId('context-submenu')).toBeDefined();
    });
  });

  it('ArrowRight into submenu auto-focuses first child', async () => {
    const items = makeSubmenuItems();
    const onClose = vi.fn();
    render(<ContextMenu x={100} y={100} items={items} onClose={onClose} />);

    const target = getKeyTarget();
    // Move to submenu item and open via keyboard
    fireEvent.keyDown(target, { key: 'ArrowDown' });
    fireEvent.keyDown(target, { key: 'ArrowRight' });

    await waitFor(() => {
      expect(screen.getByTestId('context-submenu')).toBeDefined();
    });

    // First child (Panel A) should be focused
    const panelABtn = screen.getByTestId('context-menu-item-panel-a');
    expect(document.activeElement).toBe(panelABtn);
  });

  it('ArrowLeft in submenu closes it and returns focus to parent', async () => {
    const items = makeSubmenuItems();
    const onClose = vi.fn();
    render(<ContextMenu x={100} y={100} items={items} onClose={onClose} />);

    const target = getKeyTarget();
    // Open submenu via keyboard
    fireEvent.keyDown(target, { key: 'ArrowDown' });
    fireEvent.keyDown(target, { key: 'ArrowRight' });

    await waitFor(() => {
      expect(screen.getByTestId('context-submenu')).toBeDefined();
    });

    // ArrowLeft inside the submenu
    const submenu = screen.getByTestId('context-submenu');
    const submenuKeyTarget = submenu.firstElementChild as HTMLElement;
    fireEvent.keyDown(submenuKeyTarget, { key: 'ArrowLeft' });

    // Submenu should be closed
    await waitFor(() => {
      expect(screen.queryByTestId('context-submenu')).toBeNull();
    });

    // Focus returns to the parent submenu trigger
    const windowsBtn = screen.getByTestId('context-menu-item-windows');
    expect(document.activeElement).toBe(windowsBtn);
  });
});
