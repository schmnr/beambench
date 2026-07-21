import { createContext, useContext, useRef, useState, useCallback } from 'react';
import type { PhysicalDockZone } from './panelRegistry';
import { getPanelById } from './panelRegistry';
import { startPanelDrag, updatePanelDrag, endPanelDrag } from './dndEngine';
import type { DragState, ZoneRect, TabRect } from './dndEngine';
import { useUiStore } from '../stores/uiStore';
import { appService } from '../services/appService';
import i18n from '../i18n';

interface PanelDndContextValue {
  dragState: DragState | null;
  startDrag: (panelId: string, sourceZone: PhysicalDockZone | 'floating', e: React.MouseEvent) => void;
  registerDropZone: (zone: PhysicalDockZone, ref: HTMLElement | null) => void;
  registerTabRect: (zone: PhysicalDockZone, panelId: string, ref: HTMLElement | null) => void;
}

const PanelDndContext = createContext<PanelDndContextValue>({
  dragState: null,
  startDrag: () => {},
  registerDropZone: () => {},
  registerTabRect: () => {},
});

export function usePanelDnd() {
  return useContext(PanelDndContext);
}

export function PanelDndProvider({ children }: { children: React.ReactNode }) {
  const [dragState, setDragState] = useState<DragState | null>(null);
  const zoneRefs = useRef<Map<PhysicalDockZone, HTMLElement>>(new Map());
  const tabRefsMap = useRef<Map<string, { zone: PhysicalDockZone; el: HTMLElement }>>(new Map());

  const registerDropZone = useCallback((zone: PhysicalDockZone, ref: HTMLElement | null) => {
    if (ref) {
      zoneRefs.current.set(zone, ref);
    } else {
      zoneRefs.current.delete(zone);
    }
  }, []);

  const registerTabRect = useCallback((zone: PhysicalDockZone, panelId: string, ref: HTMLElement | null) => {
    if (ref) {
      tabRefsMap.current.set(panelId, { zone, el: ref });
    } else {
      tabRefsMap.current.delete(panelId);
    }
  }, []);

  const collectRects = useCallback((): { zoneRects: ZoneRect[]; tabRects: TabRect[] } => {
    const zoneRects: ZoneRect[] = [];
    for (const [zone, el] of zoneRefs.current) {
      zoneRects.push({ zone, rect: el.getBoundingClientRect() });
    }
    const tabRects: TabRect[] = [];
    for (const [panelId, { zone, el }] of tabRefsMap.current) {
      tabRects.push({ zone, panelId, rect: el.getBoundingClientRect() });
    }
    return { zoneRects, tabRects };
  }, []);

  const startDrag = useCallback(
    (panelId: string, sourceZone: PhysicalDockZone | 'floating', e: React.MouseEvent) => {
      const state = startPanelDrag(panelId, sourceZone, e.clientX, e.clientY);
      setDragState(state);

      const def = getPanelById(panelId);
      const title = def ? i18n.t(def.titleKey) : panelId;
      const defaultW = def?.defaultFloatSize?.w ?? 384;
      const defaultH = def?.defaultFloatSize?.h ?? 300;

      let current = state;

      const handleMouseMove = (me: MouseEvent) => {
        const { zoneRects, tabRects } = collectRects();
        current = updatePanelDrag(current, me.clientX, me.clientY, title, zoneRects, tabRects, defaultW, defaultH);
        setDragState({ ...current });
      };

      const handleMouseUp = () => {
        document.removeEventListener('mousemove', handleMouseMove);
        document.removeEventListener('mouseup', handleMouseUp);

        const target = endPanelDrag(current);
        setDragState(null);

        if (!target) return;

        const store = useUiStore.getState();
        if (target.type === 'float') {
          store.floatPanel(panelId, target.x, target.y, defaultW, defaultH);
        } else if (target.type === 'zone') {
          if (sourceZone === 'floating') {
            store.dockPanel(panelId, target.zone, target.insertIndex);
          } else if (sourceZone === target.zone) {
            store.reorderPanelInZone(panelId, target.zone, target.insertIndex);
          } else {
            store.movePanelBetweenZones(panelId, sourceZone, target.zone, target.insertIndex);
          }
        }
        appService.persistLayout(useUiStore.getState().panelLayout);
      };

      document.addEventListener('mousemove', handleMouseMove);
      document.addEventListener('mouseup', handleMouseUp);
    },
    [collectRects],
  );

  return (
    <PanelDndContext.Provider value={{ dragState, startDrag, registerDropZone, registerTabRect }}>
      {children}
    </PanelDndContext.Provider>
  );
}
