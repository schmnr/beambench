import { useState, useEffect, type RefObject } from 'react';

interface CanvasSize {
  width: number;
  height: number;
}

export function useCanvasSize(containerRef: RefObject<HTMLDivElement | null>): CanvasSize {
  const [size, setSize] = useState<CanvasSize>({ width: 0, height: 0 });

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;

    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const { width, height } = entry.contentRect;
        setSize((prev) => {
          if (prev.width === Math.floor(width) && prev.height === Math.floor(height)) return prev;
          return { width: Math.floor(width), height: Math.floor(height) };
        });
      }
    });

    observer.observe(el);
    return () => observer.disconnect();
  }, [containerRef]);

  return size;
}
