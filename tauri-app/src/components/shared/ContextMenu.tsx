import { useEffect, useRef, useState, useCallback } from 'react';
import { createPortal } from 'react-dom';

const SUBMENU_ARROW = '▸';
const CHECK_GLYPH = '✓';

export interface ContextMenuItem {
  id: string;
  label: string;
  shortcut?: string;
  disabled?: boolean;
  onClick: () => void;
}

export interface ContextMenuSeparator {
  type: 'separator';
}

export interface ContextMenuSubmenu {
  type: 'submenu';
  id: string;
  label: string;
  children: ContextMenuEntry[];
}

export interface ContextMenuCheckItem {
  type: 'check';
  id: string;
  label: string;
  checked: boolean;
  onClick: () => void;
}

export type ContextMenuEntry =
  | ContextMenuItem
  | ContextMenuSeparator
  | ContextMenuSubmenu
  | ContextMenuCheckItem;

export function isSeparator(entry: ContextMenuEntry): entry is ContextMenuSeparator {
  return 'type' in entry && entry.type === 'separator';
}

export function isSubmenu(entry: ContextMenuEntry): entry is ContextMenuSubmenu {
  return 'type' in entry && entry.type === 'submenu';
}

export function isCheckItem(entry: ContextMenuEntry): entry is ContextMenuCheckItem {
  return 'type' in entry && entry.type === 'check';
}

export interface ContextMenuProps {
  x: number;
  y: number;
  items: ContextMenuEntry[];
  onClose: () => void;
}

/** Renders a single submenu panel, positioned relative to the parent item. */
function SubmenuPanel({
  items,
  parentRect,
  onClose,
  onCloseSubmenu,
  autoFocus,
}: {
  items: ContextMenuEntry[];
  parentRect: DOMRect;
  onClose: () => void;
  onCloseSubmenu: () => void;
  autoFocus: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number }>({
    left: parentRect.right,
    top: parentRect.top,
  });

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    let left = parentRect.right;
    let top = parentRect.top;
    // Flip to left if right edge overflows
    if (left + rect.width > window.innerWidth) {
      left = parentRect.left - rect.width;
    }
    if (top + rect.height > window.innerHeight) {
      top = window.innerHeight - rect.height - 4;
    }
    if (left < 0) left = 4;
    if (top < 0) top = 4;
    setPos({ left, top });
  }, [parentRect]);

  return (
    <div
      ref={ref}
      className="fixed bg-bb-panel border border-bb-border rounded shadow-lg py-1 min-w-[180px] z-[101]"
      style={{ left: pos.left, top: pos.top }}
      data-testid="context-submenu"
    >
      <MenuItems
        items={items}
        onClose={onClose}
        onCloseSubmenu={onCloseSubmenu}
        autoFocus={autoFocus}
      />
    </div>
  );
}

/** Renders a list of context menu entries. Shared between root and submenus. */
function MenuItems({
  items,
  onClose,
  onCloseSubmenu,
  autoFocus,
}: {
  items: ContextMenuEntry[];
  onClose: () => void;
  /** Called when ArrowLeft is pressed — closes this submenu and returns focus to parent. */
  onCloseSubmenu?: () => void;
  /** When true, auto-focus the first focusable item on mount. */
  autoFocus?: boolean;
}) {
  const [focusedIndex, setFocusedIndex] = useState(-1);
  const [openSubmenuIndex, setOpenSubmenuIndex] = useState<number | null>(null);
  // Whether the submenu was opened via keyboard (so it should auto-focus)
  const [submenuKeyboardOpened, setSubmenuKeyboardOpened] = useState(false);
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const submenuTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Find focusable indices (skip separators)
  const isFocusable = useCallback(
    (i: number) => !isSeparator(items[i]),
    [items],
  );

  const findFirstFocusable = useCallback(() => {
    for (let i = 0; i < items.length; i++) {
      if (isFocusable(i)) return i;
    }
    return -1;
  }, [items.length, isFocusable]);

  const moveFocus = useCallback(
    (dir: 1 | -1) => {
      setFocusedIndex((prev) => {
        let next = prev;
        for (let step = 0; step < items.length; step++) {
          next = (next + dir + items.length) % items.length;
          if (isFocusable(next)) return next;
        }
        return prev;
      });
    },
    [items.length, isFocusable],
  );

  // Auto-focus first item on mount when requested (keyboard-opened submenu)
  useEffect(() => {
    if (autoFocus) {
      const first = findFirstFocusable();
      if (first >= 0) setFocusedIndex(first);
    }
  }, [autoFocus, findFirstFocusable]);

  // Keep DOM focus in sync
  useEffect(() => {
    if (focusedIndex >= 0 && itemRefs.current[focusedIndex]) {
      itemRefs.current[focusedIndex]?.focus();
    }
  }, [focusedIndex]);

  const openSubmenuViaKeyboard = useCallback((index: number) => {
    setSubmenuKeyboardOpened(true);
    setOpenSubmenuIndex(index);
  }, []);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      switch (e.key) {
        case 'ArrowDown':
          e.preventDefault();
          e.stopPropagation();
          moveFocus(1);
          break;
        case 'ArrowUp':
          e.preventDefault();
          e.stopPropagation();
          moveFocus(-1);
          break;
        case 'ArrowRight':
          if (focusedIndex >= 0 && isSubmenu(items[focusedIndex])) {
            e.preventDefault();
            e.stopPropagation();
            openSubmenuViaKeyboard(focusedIndex);
          }
          break;
        case 'ArrowLeft':
          // Close this submenu and return focus to parent
          if (onCloseSubmenu) {
            e.preventDefault();
            e.stopPropagation();
            onCloseSubmenu();
          }
          break;
        case 'Enter':
          e.preventDefault();
          e.stopPropagation();
          if (focusedIndex >= 0) {
            const entry = items[focusedIndex];
            if (isSubmenu(entry)) {
              openSubmenuViaKeyboard(focusedIndex);
            } else if (isCheckItem(entry)) {
              entry.onClick();
              onClose();
            } else if (!isSeparator(entry) && !entry.disabled) {
              entry.onClick();
              onClose();
            }
          }
          break;
        case 'Escape':
          e.preventDefault();
          e.stopPropagation();
          onClose();
          break;
      }
    },
    [focusedIndex, items, moveFocus, onClose, onCloseSubmenu, openSubmenuViaKeyboard],
  );

  const scheduleSubmenuOpen = useCallback((index: number) => {
    if (submenuTimerRef.current) clearTimeout(submenuTimerRef.current);
    submenuTimerRef.current = setTimeout(() => {
      setSubmenuKeyboardOpened(false);
      setOpenSubmenuIndex(index);
    }, 100);
  }, []);

  const scheduleSubmenuClose = useCallback(() => {
    if (submenuTimerRef.current) clearTimeout(submenuTimerRef.current);
    submenuTimerRef.current = setTimeout(() => setOpenSubmenuIndex(null), 200);
  }, []);

  const cancelSubmenuTimer = useCallback(() => {
    if (submenuTimerRef.current) {
      clearTimeout(submenuTimerRef.current);
      submenuTimerRef.current = null;
    }
  }, []);

  /** Called when child submenu's ArrowLeft fires — close submenu and re-focus parent item. */
  const handleChildSubmenuClose = useCallback(() => {
    setOpenSubmenuIndex(null);
    setSubmenuKeyboardOpened(false);
    // Re-focus the parent submenu trigger button
    if (focusedIndex >= 0 && itemRefs.current[focusedIndex]) {
      itemRefs.current[focusedIndex]?.focus();
    }
  }, [focusedIndex]);

  return (
    <div onKeyDown={handleKeyDown}>
      {items.map((entry, i) => {
        if (isSeparator(entry)) {
          return <div key={`sep-${i}`} className="border-t border-bb-border my-0.5" />;
        }

        if (isSubmenu(entry)) {
          const isFocused = focusedIndex === i;
          const isOpen = openSubmenuIndex === i;
          return (
            <div
              key={entry.id}
              className="relative"
              onMouseEnter={() => {
                setFocusedIndex(i);
                scheduleSubmenuOpen(i);
              }}
              onMouseLeave={scheduleSubmenuClose}
            >
              <button
                ref={(el) => { itemRefs.current[i] = el; }}
                className={`w-full text-left px-2.5 py-0.5 text-sm flex items-center justify-between ${
                  isFocused ? 'bg-bb-hover text-bb-text' : 'text-bb-text hover:bg-bb-hover'
                }`}
                data-testid={`context-menu-item-${entry.id}`}
                tabIndex={-1}
                onFocus={() => setFocusedIndex(i)}
              >
                <span className="flex items-center gap-1">
                  <span className="inline-block w-4" />
                  {entry.label}
                </span>
                <span className="text-bb-text-dim text-xs ml-4">{SUBMENU_ARROW}</span>
              </button>
              {isOpen && itemRefs.current[i] && (
                <SubmenuPanel
                  items={entry.children}
                  parentRect={itemRefs.current[i]!.getBoundingClientRect()}
                  onClose={onClose}
                  onCloseSubmenu={handleChildSubmenuClose}
                  autoFocus={submenuKeyboardOpened}
                />
              )}
            </div>
          );
        }

        if (isCheckItem(entry)) {
          const isFocused = focusedIndex === i;
          return (
            <button
              key={entry.id}
              ref={(el) => { itemRefs.current[i] = el; }}
              className={`w-full text-left px-2.5 py-0.5 text-sm flex items-center ${
                isFocused ? 'bg-bb-hover text-bb-text' : 'text-bb-text hover:bg-bb-hover'
              }`}
              onClick={() => {
                entry.onClick();
                onClose();
              }}
              onMouseEnter={() => {
                setFocusedIndex(i);
                cancelSubmenuTimer();
              }}
              data-testid={`context-menu-item-${entry.id}`}
              tabIndex={-1}
              onFocus={() => setFocusedIndex(i)}
            >
              <span className="inline-block w-4 text-center">
                {entry.checked ? CHECK_GLYPH : ''}
              </span>
              <span className="ml-1">{entry.label}</span>
            </button>
          );
        }

        // Normal item
        const isFocused = focusedIndex === i;
        return (
          <button
            key={entry.id}
            ref={(el) => { itemRefs.current[i] = el; }}
            className={`w-full text-left px-2.5 py-0.5 text-sm flex items-center justify-between ${
              entry.disabled
                ? 'text-bb-text-dim cursor-default'
                : isFocused
                  ? 'bg-bb-hover text-bb-text'
                  : 'text-bb-text hover:bg-bb-hover'
            }`}
            onClick={
              entry.disabled
                ? undefined
                : () => {
                    entry.onClick();
                    onClose();
                  }
            }
            disabled={entry.disabled}
            onMouseEnter={() => {
              setFocusedIndex(i);
              cancelSubmenuTimer();
            }}
            data-testid={`context-menu-item-${entry.id}`}
            tabIndex={-1}
            onFocus={() => setFocusedIndex(i)}
          >
            <span className="flex items-center gap-1">
              <span className="inline-block w-4" />
              {entry.label}
            </span>
            {entry.shortcut && (
              <span className="text-bb-text-dim text-xs ml-4">{entry.shortcut}</span>
            )}
          </button>
        );
      })}
    </div>
  );
}

export function ContextMenu({ x, y, items, onClose }: ContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);

  // Viewport-edge clamping
  useEffect(() => {
    const el = menuRef.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    let left = x;
    let top = y;
    if (left + rect.width > window.innerWidth) {
      left = window.innerWidth - rect.width - 4;
    }
    if (top + rect.height > window.innerHeight) {
      top = window.innerHeight - rect.height - 4;
    }
    if (left < 0) left = 4;
    if (top < 0) top = 4;
    el.style.left = `${left}px`;
    el.style.top = `${top}px`;
  }, [x, y]);

  // Click-outside dismissal
  useEffect(() => {
    const handleMouseDown = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose();
      }
    };
    document.addEventListener('mousedown', handleMouseDown);
    return () => document.removeEventListener('mousedown', handleMouseDown);
  }, [onClose]);

  // Escape key dismissal
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [onClose]);

  // Scroll dismissal (capture phase)
  useEffect(() => {
    const handleScroll = () => onClose();
    document.addEventListener('scroll', handleScroll, true);
    return () => document.removeEventListener('scroll', handleScroll, true);
  }, [onClose]);

  return createPortal(
    <div
      ref={menuRef}
      className="fixed bg-bb-panel border border-bb-border rounded shadow-lg py-1 min-w-[180px] z-[100]"
      style={{ left: x, top: y }}
      onContextMenu={(e) => e.preventDefault()}
      data-testid="context-menu"
    >
      <MenuItems items={items} onClose={onClose} />
    </div>,
    document.body,
  );
}
