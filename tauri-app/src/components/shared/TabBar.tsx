import { useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import type { PhysicalDockZone } from '../../panels';

const FLOAT_ICON = '⊡';

interface Tab {
  id: string;
  label: string;
}

interface TabBarProps {
  tabs: Tab[];
  activeTab: string;
  onTabChange: (tabId: string) => void;
  zone?: PhysicalDockZone;
  onTabDragStart?: (panelId: string, e: React.MouseEvent) => void;
  onFloatPanel?: (panelId: string) => void;
  onTabContextMenu?: (panelId: string, e: React.MouseEvent) => void;
  dropInsertIndex?: number | null;
}

export function TabBar({ tabs, activeTab, onTabChange, onTabDragStart, onFloatPanel, onTabContextMenu, dropInsertIndex }: TabBarProps) {
  const { t } = useTranslation();
  const scrollRef = useRef<HTMLDivElement>(null);
  const dragScrollRef = useRef<{ startX: number; scrollLeft: number } | null>(null);

  const handlePointerDown = useCallback((e: React.PointerEvent) => {
    if (e.button !== 0) return; // left-click only — prevent right-click grab-scroll
    const el = scrollRef.current;
    if (!el) return;
    // Only grab-scroll on middle-click or when clicking empty space (not a tab button)
    const target = e.target as HTMLElement;
    if (target.closest('button')) return;
    dragScrollRef.current = { startX: e.clientX, scrollLeft: el.scrollLeft };
    el.setPointerCapture(e.pointerId);
  }, []);

  const handlePointerMove = useCallback((e: React.PointerEvent) => {
    if (!dragScrollRef.current || !scrollRef.current) return;
    const dx = e.clientX - dragScrollRef.current.startX;
    scrollRef.current.scrollLeft = dragScrollRef.current.scrollLeft - dx;
  }, []);

  const handlePointerUp = useCallback(() => {
    dragScrollRef.current = null;
  }, []);

  return (
    <div
      ref={scrollRef}
      className="flex items-center h-8 bg-bb-panel border-b border-bb-border overflow-x-auto scrollbar-none px-1"
      data-testid="tab-bar"
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
    >
      {tabs.map((tab, i) => (
        <div
          key={tab.id}
          className="relative flex items-center h-full group"
          onContextMenu={(e) => {
            if (onTabContextMenu) {
              e.preventDefault();
              e.stopPropagation();
              onTabContextMenu(tab.id, e);
            }
          }}
        >
          {/* Drop indicator */}
          {dropInsertIndex === i && (
            <div className="absolute left-0 top-1 bottom-1 w-0.5 bg-bb-accent z-10" data-testid="drop-indicator" />
          )}
          <button
            onMouseDown={(e) => {
              if (e.button !== 0) return; // left-click only — prevent right-click DnD
              if (onTabDragStart) {
                onTabDragStart(tab.id, e);
              }
            }}
            onClick={() => onTabChange(tab.id)}
            className={`px-2.5 h-full border-b-2 text-xs whitespace-nowrap transition-colors ${
              activeTab === tab.id
                ? 'border-bb-accent font-semibold text-bb-accent'
                : 'border-transparent text-bb-text-muted hover:text-bb-text'
            }`}
          >
            {tab.label}
          </button>
          {/* Float button — visible on hover */}
          {onFloatPanel && (
            <button
              className="hidden group-hover:flex items-center justify-center w-3 h-3 text-bb-text-muted hover:text-bb-text text-[9px] mr-0.5"
              onClick={(e) => {
                e.stopPropagation();
                onFloatPanel(tab.id);
              }}
              title={t('panels.floating.float_tooltip')}
              data-testid={`float-btn-${tab.id}`}
            >
              {FLOAT_ICON}
            </button>
          )}
        </div>
      ))}
      {/* Drop indicator at end */}
      {dropInsertIndex === tabs.length && (
        <div className="w-0.5 h-5 bg-bb-accent self-center" data-testid="drop-indicator" />
      )}
    </div>
  );
}
