import { useState, useRef, useCallback, useEffect } from 'react';
import { createPortal } from 'react-dom';

export interface SubmenuItem {
  id: string;
  icon: React.ReactNode;
  label: string;
}

interface ToolbarSubmenuButtonProps {
  items: SubmenuItem[];
  activeItemId: string;
  onSelect: (id: string) => void;
  isToolActive: boolean;
  disabled?: boolean;
  size?: 'sm' | 'md';
}

const LONG_PRESS_MS = 300;

export function ToolbarSubmenuButton({
  items,
  activeItemId,
  onSelect,
  isToolActive,
  disabled,
  size = 'sm',
}: ToolbarSubmenuButtonProps) {
  const [open, setOpen] = useState(false);
  const btnRef = useRef<HTMLButtonElement>(null);
  const flyoutRef = useRef<HTMLDivElement>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const didLongPressRef = useRef(false);

  const activeItem = items.find((i) => i.id === activeItemId) ?? items[0];
  const dim = size === 'sm' ? 'w-9 h-9' : 'w-10 h-10';

  const [flyoutPos, setFlyoutPos] = useState({ top: 0, left: 0 });

  const openMenu = useCallback(() => {
    if (btnRef.current) {
      const rect = btnRef.current.getBoundingClientRect();
      setFlyoutPos({ top: rect.top, left: rect.right + 2 });
    }
    setOpen(true);
  }, []);

  const closeMenu = useCallback(() => {
    setOpen(false);
  }, []);

  const handleSelect = useCallback(
    (id: string) => {
      onSelect(id);
      closeMenu();
    },
    [onSelect, closeMenu],
  );

  // Long-press detection
  const handlePointerDown = useCallback(
    (e: React.PointerEvent) => {
      if (disabled) return;
      didLongPressRef.current = false;
      timerRef.current = setTimeout(() => {
        didLongPressRef.current = true;
        openMenu();
      }, LONG_PRESS_MS);
      // Prevent text selection during long-press
      e.preventDefault();
    },
    [disabled, openMenu],
  );

  const handlePointerUp = useCallback(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    // If it was a short click (not long-press), activate the current sub-tool
    if (!didLongPressRef.current && !disabled) {
      onSelect(activeItemId);
    }
  }, [disabled, onSelect, activeItemId]);

  const handlePointerLeave = useCallback(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  // Corner arrow click — opens submenu immediately
  const handleArrowClick = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      e.preventDefault();
      if (!disabled) openMenu();
    },
    [disabled, openMenu],
  );

  // Dismiss on Escape or click outside
  useEffect(() => {
    if (!open) return;
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') closeMenu();
    };
    const handleClick = (e: MouseEvent) => {
      const target = e.target as Node;
      if (btnRef.current?.contains(target)) return;
      if (flyoutRef.current?.contains(target)) return;
      closeMenu();
    };
    window.addEventListener('keydown', handleKey);
    window.addEventListener('pointerdown', handleClick);
    return () => {
      window.removeEventListener('keydown', handleKey);
      window.removeEventListener('pointerdown', handleClick);
    };
  }, [open, closeMenu]);

  // Clean up timer on unmount
  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  return (
    <>
      <button
        ref={btnRef}
        onPointerDown={handlePointerDown}
        onPointerUp={handlePointerUp}
        onPointerLeave={handlePointerLeave}
        disabled={disabled}
        aria-label={activeItem.label}
        title={activeItem.label}
        className={`${dim} relative flex items-center justify-center rounded select-none ${
          isToolActive
            ? 'bg-bb-accent/15 border border-bb-accent/30 text-bb-text'
            : disabled
              ? 'text-bb-text-disabled cursor-default'
              : 'text-bb-text-muted hover:text-bb-text hover:bg-bb-surface'
        }`}
      >
        {activeItem.icon}
        {/* Corner arrow indicator */}
        <span
          onPointerDown={(e) => { e.stopPropagation(); }}
          onPointerUp={(e) => { e.stopPropagation(); }}
          onClick={handleArrowClick}
          className="absolute bottom-0 right-0 w-2.5 h-2.5 flex items-center justify-center cursor-pointer"
        >
          <svg width="5" height="5" viewBox="0 0 5 5" className="fill-current opacity-50">
            <polygon points="0,5 5,0 5,5" />
          </svg>
        </span>
      </button>

      {/* Flyout submenu */}
      {open &&
        createPortal(
          <div
            ref={flyoutRef}
            className="fixed z-[9999] bg-bb-panel border border-bb-border rounded shadow-lg py-1 min-w-[140px]"
            style={{ top: flyoutPos.top, left: flyoutPos.left }}
          >
            {items.map((item) => (
              <button
                key={item.id}
                onClick={() => handleSelect(item.id)}
                className={`w-full flex items-center gap-2 px-2 py-1 text-xs hover:bg-bb-surface ${
                  item.id === activeItemId ? 'text-bb-accent' : 'text-bb-text'
                }`}
              >
                <span className="w-6 h-6 flex items-center justify-center shrink-0">{item.icon}</span>
                <span>{item.label}</span>
              </button>
            ))}
          </div>,
          document.body,
        )}
    </>
  );
}
