import { useRef, useCallback, useEffect } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';

interface FloatingPanelProps {
  panelId: string;
  title: string;
  x: number;
  y: number;
  width: number;
  height: number;
  zIndex: number;
  minWidth?: number;
  minHeight?: number;
  children: React.ReactNode;
  onClose: () => void;
  onDock: () => void;
  onMove: (x: number, y: number) => void;
  onResize: (w: number, h: number) => void;
  onFocus: () => void;
  onTitleContextMenu?: (e: React.MouseEvent) => void;
}

export function FloatingPanel({
  panelId,
  title,
  x,
  y,
  width,
  height,
  zIndex,
  minWidth = 200,
  minHeight = 150,
  children,
  onClose,
  onDock,
  onMove,
  onResize,
  onFocus,
  onTitleContextMenu,
}: FloatingPanelProps) {
  const { t } = useTranslation();
  const dragRef = useRef<{ startX: number; startY: number; panelX: number; panelY: number } | null>(null);
  const resizeRef = useRef<{ startX: number; startY: number; startW: number; startH: number } | null>(null);
  const dragCleanupRef = useRef<(() => void) | null>(null);
  const resizeCleanupRef = useRef<(() => void) | null>(null);

  // Cleanup document listeners on unmount to prevent leaks
  useEffect(() => {
    return () => {
      dragCleanupRef.current?.();
      resizeCleanupRef.current?.();
    };
  }, []);

  // Title bar drag
  const handleTitleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (e.button !== 0) return; // left-click only — prevent right-click drag
      // Ignore clicks on buttons
      if ((e.target as HTMLElement).closest('button')) return;
      e.preventDefault();
      onFocus();
      dragRef.current = { startX: e.clientX, startY: e.clientY, panelX: x, panelY: y };

      const handleMouseMove = (me: MouseEvent) => {
        if (!dragRef.current) return;
        const dx = me.clientX - dragRef.current.startX;
        const dy = me.clientY - dragRef.current.startY;
        const newX = Math.max(0, Math.min(window.innerWidth - 100, dragRef.current.panelX + dx));
        const newY = Math.max(0, Math.min(window.innerHeight - 40, dragRef.current.panelY + dy));
        onMove(newX, newY);
      };

      const handleMouseUp = () => {
        dragRef.current = null;
        dragCleanupRef.current = null;
        document.removeEventListener('mousemove', handleMouseMove);
        document.removeEventListener('mouseup', handleMouseUp);
      };

      document.addEventListener('mousemove', handleMouseMove);
      document.addEventListener('mouseup', handleMouseUp);
      dragCleanupRef.current = () => {
        document.removeEventListener('mousemove', handleMouseMove);
        document.removeEventListener('mouseup', handleMouseUp);
      };
    },
    [x, y, onMove, onFocus],
  );

  // Resize handle
  const handleResizeMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      onFocus();
      resizeRef.current = { startX: e.clientX, startY: e.clientY, startW: width, startH: height };

      const handleMouseMove = (me: MouseEvent) => {
        if (!resizeRef.current) return;
        const dx = me.clientX - resizeRef.current.startX;
        const dy = me.clientY - resizeRef.current.startY;
        onResize(
          Math.max(minWidth, resizeRef.current.startW + dx),
          Math.max(minHeight, resizeRef.current.startH + dy),
        );
      };

      const handleMouseUp = () => {
        resizeRef.current = null;
        resizeCleanupRef.current = null;
        document.removeEventListener('mousemove', handleMouseMove);
        document.removeEventListener('mouseup', handleMouseUp);
      };

      document.addEventListener('mousemove', handleMouseMove);
      document.addEventListener('mouseup', handleMouseUp);
      resizeCleanupRef.current = () => {
        document.removeEventListener('mousemove', handleMouseMove);
        document.removeEventListener('mouseup', handleMouseUp);
      };
    },
    [width, height, minWidth, minHeight, onResize, onFocus],
  );

  return createPortal(
    <div
      data-testid={`floating-panel-${panelId}`}
      className="fixed bg-bb-panel border border-bb-border rounded-lg shadow-xl flex flex-col overflow-hidden"
      style={{ left: x, top: y, width, height, zIndex: 30 + zIndex }}
      onMouseDown={onFocus}
      onContextMenu={(e) => e.preventDefault()}
    >
      {/* Title bar */}
      <div
        className="h-7 flex items-center px-2 bg-bb-panel-header border-b border-bb-border cursor-move select-none shrink-0"
        onMouseDown={handleTitleMouseDown}
        onContextMenu={(e) => {
          e.preventDefault();
          e.stopPropagation();
          onTitleContextMenu?.(e);
        }}
      >
        <span className="text-xs text-bb-text font-medium flex-1 truncate">{title}</span>
        <button
          className="text-bb-text-muted hover:text-bb-text text-[10px] px-1"
          onClick={onDock}
          title={t('panels.floating.dock_tooltip')}
        >
          {t('context_menu.dock')}
        </button>
        <button
          className="text-bb-text-muted hover:text-bb-text text-sm px-1 leading-none"
          onClick={onClose}
          title={t('panels.floating.close_tooltip')}
        >
          ×
        </button>
      </div>

      {/* Content */}
      <div className="flex-1 min-h-0 overflow-y-auto">{children}</div>

      {/* Resize handle */}
      <div
        className="absolute bottom-0 right-0 w-2 h-2 cursor-nwse-resize"
        onMouseDown={handleResizeMouseDown}
      />
    </div>,
    document.body,
  );
}
