import { useCallback, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { useUiStore } from '../../stores/uiStore';
import { appService } from '../../services/appService';

interface ZoneSplitterProps {
  containerRef: React.RefObject<HTMLDivElement | null>;
  edge?: 'top' | 'bottom';
  onBeforeDrag?: () => void;
  onDragRatio?: (ratio: number) => void;
  testId?: string;
}

export function ZoneSplitter({ containerRef, edge, onBeforeDrag, onDragRatio, testId }: ZoneSplitterProps) {
  const { t } = useTranslation();
  const setUpperSplitRatio = useUiStore((s) => s.setUpperSplitRatio);
  const dragging = useRef(false);

  const onMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      onBeforeDrag?.();
      dragging.current = true;

      const onMouseMove = (ev: MouseEvent) => {
        if (!dragging.current || !containerRef.current) return;
        const rect = containerRef.current.getBoundingClientRect();
        const ratio = (ev.clientY - rect.top) / rect.height;
        if (onDragRatio) onDragRatio(ratio);
        else setUpperSplitRatio(ratio);
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
    [containerRef, onBeforeDrag, onDragRatio, setUpperSplitRatio]
  );

  return (
    <div
      role="separator"
      aria-orientation="horizontal"
      aria-label={t('context_menu.panels')}
      className="group flex h-2 shrink-0 cursor-row-resize items-center justify-center"
      onMouseDown={onMouseDown}
      data-testid={testId ?? (edge ? `right-panel-${edge}-reveal-handle` : 'right-panel-splitter')}
    >
      <div className="h-0.5 w-12 rounded-full bg-bb-border transition-colors group-hover:bg-bb-accent/70" />
    </div>
  );
}
