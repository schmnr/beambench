import { describe, it, expect, afterEach } from 'vitest';
import { startPanelDrag, updatePanelDrag, endPanelDrag, resolveDropTarget } from '../dndEngine';
import type { ZoneRect, TabRect } from '../dndEngine';

// Clean up any ghost elements after each test
afterEach(() => {
  document.querySelectorAll('[data-dnd-ghost]').forEach((el) => el.remove());
});

const sampleZoneRects: ZoneRect[] = [
  { zone: 'upper-right', rect: new DOMRect(600, 0, 300, 200) },
  { zone: 'lower-right', rect: new DOMRect(600, 210, 300, 200) },
];

const sampleTabRects: TabRect[] = [
  { zone: 'upper-right', panelId: 'cuts_layers', rect: new DOMRect(600, 0, 80, 28) },
  { zone: 'upper-right', panelId: 'move', rect: new DOMRect(680, 0, 60, 28) },
  { zone: 'upper-right', panelId: 'console', rect: new DOMRect(740, 0, 70, 28) },
  { zone: 'lower-right', panelId: 'laser', rect: new DOMRect(600, 210, 60, 28) },
  { zone: 'lower-right', panelId: 'material', rect: new DOMRect(660, 210, 80, 28) },
];

describe('dndEngine', () => {
  describe('startPanelDrag', () => {
    it('creates initial state with isDragging=false', () => {
      const state = startPanelDrag('console', 'upper-right', 100, 200);
      expect(state.isDragging).toBe(false);
      expect(state.panelId).toBe('console');
      expect(state.sourceZone).toBe('upper-right');
      expect(state.startX).toBe(100);
      expect(state.startY).toBe(200);
      expect(state.ghostEl).toBeNull();
    });
  });

  describe('updatePanelDrag', () => {
    it('movement below threshold keeps isDragging=false', () => {
      const state = startPanelDrag('console', 'upper-right', 100, 200);
      const updated = updatePanelDrag(state, 102, 201, 'Console', sampleZoneRects, sampleTabRects, 420, 300);
      expect(updated.isDragging).toBe(false);
      expect(updated.ghostEl).toBeNull();
    });

    it('movement at threshold sets isDragging=true and creates ghost', () => {
      const state = startPanelDrag('console', 'upper-right', 100, 200);
      const updated = updatePanelDrag(state, 105, 200, 'Console', sampleZoneRects, sampleTabRects, 420, 300);
      expect(updated.isDragging).toBe(true);
      expect(updated.ghostEl).not.toBeNull();
      expect(updated.ghostEl!.textContent).toBe('Console');
    });
  });

  describe('endPanelDrag', () => {
    it('returns null when not dragging', () => {
      const state = startPanelDrag('console', 'upper-right', 100, 200);
      const result = endPanelDrag(state);
      expect(result).toBeNull();
    });

    it('returns drop target when dragging', () => {
      let state = startPanelDrag('console', 'upper-right', 100, 200);
      state = updatePanelDrag(state, 650, 100, 'Console', sampleZoneRects, sampleTabRects, 420, 300);
      const result = endPanelDrag(state);
      expect(result).not.toBeNull();
      expect(result!.type).toBe('zone');
    });

    it('cleans up ghost element', () => {
      let state = startPanelDrag('console', 'upper-right', 100, 200);
      state = updatePanelDrag(state, 200, 200, 'Console', sampleZoneRects, sampleTabRects, 420, 300);
      expect(state.ghostEl).not.toBeNull();
      endPanelDrag(state);
      expect(document.querySelectorAll('[data-dnd-ghost]')).toHaveLength(0);
    });
  });

  describe('resolveDropTarget', () => {
    it('returns zone target when cursor inside zone rect', () => {
      const target = resolveDropTarget(650, 100, sampleZoneRects, sampleTabRects, 420, 300);
      expect(target).not.toBeNull();
      expect(target!.type).toBe('zone');
      if (target!.type === 'zone') {
        expect(target!.zone).toBe('upper-right');
      }
    });

    it('returns float target when cursor outside all zones', () => {
      const target = resolveDropTarget(200, 300, sampleZoneRects, sampleTabRects, 420, 300);
      expect(target).not.toBeNull();
      expect(target!.type).toBe('float');
      if (target!.type === 'float') {
        expect(target!.x).toBe(200 - 420 / 2);
        expect(target!.y).toBe(300 - 300 / 2);
      }
    });

    it('computes correct insertIndex from tab positions', () => {
      // Cursor between cuts_layers and move tabs
      const target1 = resolveDropTarget(670, 14, sampleZoneRects, sampleTabRects, 420, 300);
      expect(target1!.type).toBe('zone');
      if (target1!.type === 'zone') {
        expect(target1!.insertIndex).toBe(1); // after cuts_layers, before move
      }

      // Cursor before first tab
      const target2 = resolveDropTarget(610, 14, sampleZoneRects, sampleTabRects, 420, 300);
      if (target2!.type === 'zone') {
        expect(target2!.insertIndex).toBe(0);
      }

      // Cursor after last tab
      const target3 = resolveDropTarget(850, 14, sampleZoneRects, sampleTabRects, 420, 300);
      if (target3!.type === 'zone') {
        expect(target3!.insertIndex).toBe(3); // after all 3 upper tabs
      }
    });

    it('returns lower-right zone for cursor in lower zone', () => {
      const target = resolveDropTarget(650, 230, sampleZoneRects, sampleTabRects, 420, 300);
      expect(target!.type).toBe('zone');
      if (target!.type === 'zone') {
        expect(target!.zone).toBe('lower-right');
      }
    });
  });
});
