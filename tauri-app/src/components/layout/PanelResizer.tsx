import { useCallback, useEffect, useRef } from 'react';

interface PanelResizerProps {
  onResize: (delta: number) => void;
  direction: 'left' | 'right' | 'bottom';
}

export function PanelResizer({ onResize, direction }: PanelResizerProps) {
  const dragging = useRef(false);
  const lastPos = useRef(0);

  const isHorizontal = direction === 'bottom';

  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      dragging.current = true;
      lastPos.current = isHorizontal ? e.clientY : e.clientX;
    },
    [isHorizontal],
  );

  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (!dragging.current) return;
      if (isHorizontal) {
        const delta = e.clientY - lastPos.current;
        lastPos.current = e.clientY;
        onResize(-delta);
      } else {
        const delta = e.clientX - lastPos.current;
        lastPos.current = e.clientX;
        onResize(direction === 'left' ? delta : -delta);
      }
    };

    const handleMouseUp = () => {
      dragging.current = false;
    };

    window.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', handleMouseUp);
    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
    };
  }, [onResize, direction, isHorizontal]);

  if (isHorizontal) {
    return (
      <div
        onMouseDown={handleMouseDown}
        className="h-1 cursor-row-resize bg-bb-border hover:bg-bb-accent/40 transition-colors flex-shrink-0"
      />
    );
  }

  return (
    <div
      onMouseDown={handleMouseDown}
      className="w-1 cursor-col-resize bg-bb-border hover:bg-bb-accent/40 transition-colors flex-shrink-0"
    />
  );
}
