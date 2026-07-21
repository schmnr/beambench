import { useCallback, useRef } from 'react';
import { useUiStore } from '../../stores/uiStore';
import { appService } from '../../services/appService';

export function ZoneSplitter({ containerRef }: { containerRef: React.RefObject<HTMLDivElement | null> }) {
  const setUpperSplitRatio = useUiStore((s) => s.setUpperSplitRatio);
  const dragging = useRef(false);

  const onMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      dragging.current = true;

      const onMouseMove = (ev: MouseEvent) => {
        if (!dragging.current || !containerRef.current) return;
        const rect = containerRef.current.getBoundingClientRect();
        const ratio = (ev.clientY - rect.top) / rect.height;
        setUpperSplitRatio(ratio);
      };

      const onMouseUp = () => {
        dragging.current = false;
        document.removeEventListener('mousemove', onMouseMove);
        document.removeEventListener('mouseup', onMouseUp);
        // Persist layout after drag ends
        appService.persistLayout(useUiStore.getState().panelLayout);
      };

      document.addEventListener('mousemove', onMouseMove);
      document.addEventListener('mouseup', onMouseUp);
    },
    [containerRef, setUpperSplitRatio]
  );

  return (
    <div
      className="h-1 bg-bb-border shrink-0 cursor-row-resize hover:bg-bb-accent/40 transition-colors"
      onMouseDown={onMouseDown}
    />
  );
}
