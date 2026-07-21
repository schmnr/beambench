import type { PhysicalDockZone } from './panelRegistry';

export type DropTarget =
  | { type: 'zone'; zone: PhysicalDockZone; insertIndex: number }
  | { type: 'float'; x: number; y: number }
  | null;

export interface DragState {
  panelId: string;
  sourceZone: PhysicalDockZone | 'floating';
  ghostEl: HTMLElement | null;
  startX: number;
  startY: number;
  currentX: number;
  currentY: number;
  isDragging: boolean;
  activeDropTarget: DropTarget;
}

const DRAG_THRESHOLD = 5;
const GHOST_WIDTH = 120;
const GHOST_HEIGHT = 28;

export function startPanelDrag(
  panelId: string,
  sourceZone: PhysicalDockZone | 'floating',
  startX: number,
  startY: number,
): DragState {
  return {
    panelId,
    sourceZone,
    ghostEl: null,
    startX,
    startY,
    currentX: startX,
    currentY: startY,
    isDragging: false,
    activeDropTarget: null,
  };
}

function createGhost(title: string): HTMLElement {
  const el = document.createElement('div');
  el.style.position = 'fixed';
  el.style.pointerEvents = 'none';
  el.style.zIndex = '45';
  el.style.width = `${GHOST_WIDTH}px`;
  el.style.height = `${GHOST_HEIGHT}px`;
  el.style.display = 'flex';
  el.style.alignItems = 'center';
  el.style.padding = '0 8px';
  el.style.fontSize = '11px';
  el.style.borderRadius = '4px';
  el.style.opacity = '0.9';
  el.style.backgroundColor = 'rgb(var(--bb-accent, 34 192 238))';
  el.style.color = 'rgb(var(--bb-on-accent, 17 24 39))';
  el.style.boxShadow = '0 2px 8px rgba(0,0,0,0.3)';
  el.textContent = title;
  el.setAttribute('data-dnd-ghost', 'true');
  document.body.appendChild(el);
  return el;
}

function positionGhost(ghost: HTMLElement, x: number, y: number): void {
  ghost.style.left = `${x - GHOST_WIDTH / 2}px`;
  ghost.style.top = `${y - GHOST_HEIGHT / 2}px`;
}

export interface ZoneRect {
  zone: PhysicalDockZone;
  rect: DOMRect;
}

export interface TabRect {
  zone: PhysicalDockZone;
  panelId: string;
  rect: DOMRect;
}

export function resolveDropTarget(
  clientX: number,
  clientY: number,
  zoneRects: ZoneRect[],
  tabRects: TabRect[],
  defaultFloatW: number,
  defaultFloatH: number,
): DropTarget {
  for (const { zone, rect } of zoneRects) {
    if (clientX >= rect.left && clientX <= rect.right && clientY >= rect.top && clientY <= rect.bottom) {
      // Inside a zone — compute insert index from tab positions
      const zoneTabs = tabRects
        .filter((t) => t.zone === zone)
        .sort((a, b) => a.rect.left - b.rect.left);

      let insertIndex = zoneTabs.length;
      for (let i = 0; i < zoneTabs.length; i++) {
        const tabMid = zoneTabs[i].rect.left + zoneTabs[i].rect.width / 2;
        if (clientX < tabMid) {
          insertIndex = i;
          break;
        }
      }

      return { type: 'zone', zone, insertIndex };
    }
  }

  // Outside all zones → float, adjusted so panel centers on cursor
  return {
    type: 'float',
    x: clientX - defaultFloatW / 2,
    y: clientY - defaultFloatH / 2,
  };
}

export function updatePanelDrag(
  state: DragState,
  clientX: number,
  clientY: number,
  title: string,
  zoneRects: ZoneRect[],
  tabRects: TabRect[],
  defaultFloatW: number,
  defaultFloatH: number,
): DragState {
  const dx = clientX - state.startX;
  const dy = clientY - state.startY;
  const distance = Math.sqrt(dx * dx + dy * dy);

  const newState = { ...state, currentX: clientX, currentY: clientY };

  if (!state.isDragging && distance >= DRAG_THRESHOLD) {
    newState.isDragging = true;
    newState.ghostEl = createGhost(title);
    positionGhost(newState.ghostEl, clientX, clientY);
  }

  if (newState.isDragging && newState.ghostEl) {
    positionGhost(newState.ghostEl, clientX, clientY);
    newState.activeDropTarget = resolveDropTarget(clientX, clientY, zoneRects, tabRects, defaultFloatW, defaultFloatH);
  }

  return newState;
}

export function endPanelDrag(state: DragState): DropTarget {
  if (state.ghostEl) {
    state.ghostEl.remove();
  }

  if (!state.isDragging) {
    return null;
  }

  return state.activeDropTarget;
}
