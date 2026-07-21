import {
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
  useEffect,
  useRef,
  useState,
} from 'react';
import { useFocusTrap } from '../../hooks/useFocusTrap';

interface DialogFrame {
  left: number;
  top: number;
  width: number;
  height: number;
}

interface MovableResizableDialogFrameProps {
  title: string;
  titleId: string;
  testId?: string;
  initialWidth: number;
  initialHeight: number;
  minWidth: number;
  minHeight: number;
  onRequestClose?: () => void;
  closeOnBackdropClick?: boolean;
  zIndexClassName?: string;
  backdropClassName?: string;
  headerActions?: ReactNode;
  children: ReactNode;
  footer: ReactNode;
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function initialFrame(
  width: number,
  height: number,
  minWidth: number,
  minHeight: number,
): DialogFrame {
  const viewportWidth = typeof window === 'undefined' ? width + 64 : window.innerWidth;
  const viewportHeight = typeof window === 'undefined' ? height + 64 : window.innerHeight;
  const safeWidth = clamp(width, minWidth, Math.max(minWidth, viewportWidth - 32));
  const safeHeight = clamp(height, minHeight, Math.max(minHeight, viewportHeight - 32));
  return {
    left: Math.max(16, Math.round((viewportWidth - safeWidth) / 2)),
    top: Math.max(16, Math.round((viewportHeight - safeHeight) / 2)),
    width: safeWidth,
    height: safeHeight,
  };
}

export function MovableResizableDialogFrame({
  title,
  titleId,
  testId,
  initialWidth,
  initialHeight,
  minWidth,
  minHeight,
  onRequestClose,
  closeOnBackdropClick = false,
  zIndexClassName = 'z-[9700]',
  backdropClassName = 'bg-black/20',
  headerActions,
  children,
  footer,
}: MovableResizableDialogFrameProps) {
  const [frame, setFrame] = useState(() =>
    initialFrame(initialWidth, initialHeight, minWidth, minHeight),
  );
  const backdropRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ startX: number; startY: number; left: number; top: number } | null>(
    null,
  );
  const resizeRef = useRef<{
    startX: number;
    startY: number;
    width: number;
    height: number;
  } | null>(null);

  useEffect(() => {
    backdropRef.current?.focus();
  }, []);

  // Runs after the backdrop-focus effect above, so the backdrop keeps initial
  // focus (Escape works immediately) while Tab cycling stays inside the dialog.
  useFocusTrap(backdropRef, true);

  useEffect(() => {
    const handleMouseMove = (event: MouseEvent) => {
      const dragState = dragRef.current;
      if (dragState) {
        const dx = event.clientX - dragState.startX;
        const dy = event.clientY - dragState.startY;
        setFrame((current) => ({
          ...current,
          left: clamp(dragState.left + dx, 0, Math.max(0, window.innerWidth - 80)),
          top: clamp(dragState.top + dy, 0, Math.max(0, window.innerHeight - 40)),
        }));
      }
      const resizeState = resizeRef.current;
      if (resizeState) {
        const dx = event.clientX - resizeState.startX;
        const dy = event.clientY - resizeState.startY;
        setFrame((current) => {
          const width = clamp(
            resizeState.width + dx,
            minWidth,
            Math.max(minWidth, window.innerWidth - current.left),
          );
          const height = clamp(
            resizeState.height + dy,
            minHeight,
            Math.max(minHeight, window.innerHeight - current.top),
          );
          return { ...current, width, height };
        });
      }
    };
    const handleMouseUp = () => {
      dragRef.current = null;
      resizeRef.current = null;
    };
    const handleWindowResize = () => {
      setFrame((current) => ({
        left: clamp(current.left, 0, Math.max(0, window.innerWidth - 80)),
        top: clamp(current.top, 0, Math.max(0, window.innerHeight - 40)),
        width: clamp(current.width, minWidth, Math.max(minWidth, window.innerWidth - current.left)),
        height: clamp(
          current.height,
          minHeight,
          Math.max(minHeight, window.innerHeight - current.top),
        ),
      }));
    };
    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
    window.addEventListener('resize', handleWindowResize);
    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
      window.removeEventListener('resize', handleWindowResize);
    };
  }, [minHeight, minWidth]);

  const startDrag = (event: ReactMouseEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    dragRef.current = {
      startX: event.clientX,
      startY: event.clientY,
      left: frame.left,
      top: frame.top,
    };
  };

  const startResize = (event: ReactMouseEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    resizeRef.current = {
      startX: event.clientX,
      startY: event.clientY,
      width: frame.width,
      height: frame.height,
    };
  };

  return (
    <div
      ref={backdropRef}
      className={`fixed inset-0 ${zIndexClassName} ${backdropClassName}`}
      data-testid={testId ? `${testId}-backdrop` : undefined}
      tabIndex={-1}
      onClick={(event) => {
        if (closeOnBackdropClick && event.target === event.currentTarget) {
          onRequestClose?.();
        }
      }}
      onKeyDown={(event) => {
        if (event.key === 'Escape') {
          onRequestClose?.();
        }
      }}
    >
      <section
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        data-testid={testId}
        className="absolute flex min-h-0 flex-col overflow-hidden rounded-lg border border-bb-border bg-bb-panel shadow-2xl"
        style={{
          left: frame.left,
          top: frame.top,
          width: frame.width,
          height: frame.height,
        }}
      >
        <div
          className="flex cursor-move select-none items-center justify-between gap-3 border-b border-bb-border px-5 py-3"
          data-testid={testId ? `${testId}-drag-handle` : undefined}
          onMouseDown={startDrag}
        >
          <h2 id={titleId} className="text-sm font-semibold text-bb-text">
            {title}
          </h2>
          {headerActions && (
            <div
              className="flex items-center gap-2"
              onMouseDown={(event) => event.stopPropagation()}
            >
              {headerActions}
            </div>
          )}
        </div>
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden">{children}</div>
        <div className="border-t border-bb-border">{footer}</div>
        <div
          className="absolute bottom-0 right-0 h-5 w-5 cursor-nwse-resize"
          data-testid={testId ? `${testId}-resize-handle` : undefined}
          onMouseDown={startResize}
        >
          <div className="absolute bottom-1 right-1 h-2.5 w-2.5 border-b border-r border-bb-text-dim" />
        </div>
      </section>
    </div>
  );
}
