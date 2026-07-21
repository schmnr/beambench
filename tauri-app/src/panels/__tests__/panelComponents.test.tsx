import { describe, it, expect, vi } from 'vitest';
import { PANEL_REGISTRY } from '../panelRegistry';
import { PANEL_COMPONENTS } from '../panelComponents';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

describe('panelComponents', () => {
  it('every registry panel has a component', () => {
    for (const panel of PANEL_REGISTRY) {
      expect(PANEL_COMPONENTS[panel.id]).toBeDefined();
      expect(typeof PANEL_COMPONENTS[panel.id]).toBe('function');
    }
  });

  it('no orphaned components without registry entries', () => {
    const registryIds = new Set(PANEL_REGISTRY.map((p) => p.id));
    for (const id of Object.keys(PANEL_COMPONENTS)) {
      expect(registryIds.has(id)).toBe(true);
    }
  });
});
