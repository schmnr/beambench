import { createContext, useContext, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import type { PhysicalDockZone } from './panelRegistry';
import { getPanelTypeId } from './panelRegistry';

export type PanelHostPlacement =
  | { kind: 'docked'; zone: PhysicalDockZone }
  | { kind: 'floating' };

export interface PanelHostContextValue {
  panelInstanceId: string;
  panelTypeId: string;
  placement: PanelHostPlacement;
  orientation: 'vertical' | 'wide';
  width: number;
  height: number;
}

const DEFAULT_PANEL_HOST: PanelHostContextValue = {
  panelInstanceId: '',
  panelTypeId: '',
  placement: { kind: 'docked', zone: 'top-right' },
  orientation: 'vertical',
  width: 0,
  height: 0,
};

const PanelHostContext = createContext<PanelHostContextValue>(DEFAULT_PANEL_HOST);

export function PanelHost({
  panelInstanceId,
  placement,
  children,
}: {
  panelInstanceId: string;
  placement: PanelHostPlacement;
  children: ReactNode;
}) {
  const hostRef = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const panelTypeId = getPanelTypeId(panelInstanceId);

  useLayoutEffect(() => {
    const element = hostRef.current;
    if (!element) return undefined;
    const updateSize = () => {
      const rect = element.getBoundingClientRect();
      setSize((current) => (
        current.width === rect.width && current.height === rect.height
          ? current
          : { width: rect.width, height: rect.height }
      ));
    };
    updateSize();
    if (typeof ResizeObserver === 'undefined') return undefined;
    const observer = new ResizeObserver(updateSize);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const orientation = placement.kind === 'docked' && placement.zone === 'bottom'
    ? 'wide'
    : size.width >= 720 && size.width > size.height * 1.5
      ? 'wide'
      : 'vertical';
  const value = useMemo<PanelHostContextValue>(() => ({
    panelInstanceId,
    panelTypeId,
    placement,
    orientation,
    width: size.width,
    height: size.height,
  }), [orientation, panelInstanceId, panelTypeId, placement, size.height, size.width]);

  return (
    <PanelHostContext.Provider value={value}>
      <div
        ref={hostRef}
        className="h-full min-h-0 w-full min-w-0 overflow-y-auto"
        data-panel-instance={panelInstanceId}
        data-panel-type={panelTypeId}
        data-panel-orientation={orientation}
        data-panel-zone={placement.kind === 'docked' ? placement.zone : 'floating'}
      >
        {children}
      </div>
    </PanelHostContext.Provider>
  );
}

export function usePanelHost(): PanelHostContextValue {
  return useContext(PanelHostContext);
}
