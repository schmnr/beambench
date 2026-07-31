import { useRef, useCallback } from 'react';

interface Tab {
  id: string;
  label: string;
}

interface TabBarProps {
  tabs: Tab[];
  activeTab: string;
  onTabChange: (tabId: string) => void;
  onTabDragStart?: (panelId: string, e: React.MouseEvent) => void;
  onTabContextMenu?: (panelId: string, e: React.MouseEvent) => void;
  dropInsertIndex?: number | null;
}

export function TabBar({
  tabs,
  activeTab,
  onTabChange,
  onTabDragStart,
  onTabContextMenu,
  dropInsertIndex,
}: TabBarProps) {
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
      role="tablist"
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onContextMenu={(event) => {
        if (!activeTab || !onTabContextMenu) return;
        event.preventDefault();
        event.stopPropagation();
        onTabContextMenu(activeTab, event);
      }}
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
            role="tab"
            aria-selected={activeTab === tab.id}
            className={`px-2.5 h-full border-b-2 text-xs whitespace-nowrap transition-colors ${
              activeTab === tab.id
                ? 'border-bb-accent font-semibold text-bb-accent'
                : 'border-transparent text-bb-text-muted hover:text-bb-text'
            }`}
          >
            {tab.label}
          </button>
        </div>
      ))}
      {/* Drop indicator at end */}
      {dropInsertIndex === tabs.length && (
        <div className="w-0.5 h-5 bg-bb-accent self-center" data-testid="drop-indicator" />
      )}
    </div>
  );
}
