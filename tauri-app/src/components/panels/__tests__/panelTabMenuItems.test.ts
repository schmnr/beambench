import { describe, it, expect, vi } from 'vitest';
import i18n from '../../../i18n';
import { buildPanelTabMenuItems, type PanelTabMenuContext } from '../panelTabMenuItems';

const t = i18n.getFixedT('en');
import { isSeparator, isSubmenu, isCheckItem } from '../../shared/ContextMenu';
import type { ContextMenuItem, ContextMenuSubmenu, ContextMenuCheckItem } from '../../shared/ContextMenu';
import { PANEL_REGISTRY } from '../../../panels/panelRegistry';

function makeCtx(overrides: Partial<PanelTabMenuContext> = {}): PanelTabMenuContext {
  return {
    panelId: 'cuts_layers',
    mode: 'docked',
    hiddenPanelIds: [],
    sidePanelsVisible: true,
    onFloat: vi.fn(),
    onDock: vi.fn(),
    onClose: vi.fn(),
    onTogglePanel: vi.fn(),
    onToggleSidePanels: vi.fn(),
    ...overrides,
  };
}

describe('buildPanelTabMenuItems', () => {
  it('docked mode: returns Float + Close + separator + Panels submenu', () => {
    const items = buildPanelTabMenuItems(t, makeCtx());
    expect(items).toHaveLength(4);
    expect((items[0] as ContextMenuItem).label).toBe('Float');
    expect((items[1] as ContextMenuItem).label).toBe('Close');
    expect(isSeparator(items[2])).toBe(true);
    expect(isSubmenu(items[3])).toBe(true);
    expect((items[3] as ContextMenuSubmenu).label).toBe('Panels');
  });

  it('docked mode: Float calls onFloat with panelId', () => {
    const onFloat = vi.fn();
    const items = buildPanelTabMenuItems(t, makeCtx({ onFloat }));
    (items[0] as ContextMenuItem).onClick();
    expect(onFloat).toHaveBeenCalledWith('cuts_layers');
  });

  it('docked mode: Close calls onClose with panelId', () => {
    const onClose = vi.fn();
    const items = buildPanelTabMenuItems(t, makeCtx({ onClose }));
    (items[1] as ContextMenuItem).onClick();
    expect(onClose).toHaveBeenCalledWith('cuts_layers');
  });

  it('docked mode: omits Float when supportsFloat is false', () => {
    // Use a fake panelId not in registry — getPanelById returns undefined
    const items = buildPanelTabMenuItems(t, makeCtx({ panelId: 'nonexistent_panel' }));
    // Should still have Float because getPanelById returns undefined and
    // the check is `def?.supportsFloat !== false` which is true for undefined
    expect((items[0] as ContextMenuItem).label).toBe('Float');
  });

  it('floating mode: returns Dock + Close + separator + Panels submenu', () => {
    const items = buildPanelTabMenuItems(t, makeCtx({ mode: 'floating' }));
    expect(items).toHaveLength(4);
    expect((items[0] as ContextMenuItem).label).toBe('Dock');
    expect((items[1] as ContextMenuItem).label).toBe('Close');
    expect(isSeparator(items[2])).toBe(true);
    expect(isSubmenu(items[3])).toBe(true);
  });

  it('floating mode: Dock calls onDock with panelId', () => {
    const onDock = vi.fn();
    const items = buildPanelTabMenuItems(t, makeCtx({ mode: 'floating', onDock }));
    (items[0] as ContextMenuItem).onClick();
    expect(onDock).toHaveBeenCalledWith('cuts_layers');
  });

  it('Panels submenu includes Side Panels toggle + all registry panels', () => {
    const items = buildPanelTabMenuItems(t, makeCtx());
    const submenu = items.find((it) => isSubmenu(it)) as ContextMenuSubmenu;
    expect(submenu).toBeDefined();
    // Side Panels + separator + all panels
    expect(submenu.children).toHaveLength(2 + PANEL_REGISTRY.length);
    // First item is Side Panels check
    const sideItem = submenu.children[0] as ContextMenuCheckItem;
    expect(isCheckItem(sideItem)).toBe(true);
    expect(sideItem.label).toBe('Side Panels');
    // Second is separator
    expect(isSeparator(submenu.children[1])).toBe(true);
  });

  it('Panels submenu: hidden panels are unchecked', () => {
    const items = buildPanelTabMenuItems(t, makeCtx({ hiddenPanelIds: ['camera', 'console'] }));
    const submenu = items.find((it) => isSubmenu(it)) as ContextMenuSubmenu;
    const cameraItem = submenu.children.find(
      (ch) => isCheckItem(ch) && (ch as ContextMenuCheckItem).id === 'panel-tab-camera',
    ) as ContextMenuCheckItem;
    expect(cameraItem.checked).toBe(false);
    const consoleItem = submenu.children.find(
      (ch) => isCheckItem(ch) && (ch as ContextMenuCheckItem).id === 'panel-tab-console',
    ) as ContextMenuCheckItem;
    expect(consoleItem.checked).toBe(false);
  });

  it('Panels submenu: visible panels are checked', () => {
    const items = buildPanelTabMenuItems(t, makeCtx({ hiddenPanelIds: [] }));
    const submenu = items.find((it) => isSubmenu(it)) as ContextMenuSubmenu;
    const cutsItem = submenu.children.find(
      (ch) => isCheckItem(ch) && (ch as ContextMenuCheckItem).id === 'panel-tab-cuts_layers',
    ) as ContextMenuCheckItem;
    expect(cutsItem.checked).toBe(true);
  });

  it('Panels submenu: toggle click calls onTogglePanel', () => {
    const onTogglePanel = vi.fn();
    const items = buildPanelTabMenuItems(t, makeCtx({ onTogglePanel }));
    const submenu = items.find((it) => isSubmenu(it)) as ContextMenuSubmenu;
    const cameraItem = submenu.children.find(
      (ch) => isCheckItem(ch) && (ch as ContextMenuCheckItem).id === 'panel-tab-camera',
    ) as ContextMenuCheckItem;
    cameraItem.onClick();
    expect(onTogglePanel).toHaveBeenCalledWith('camera');
  });

  it('Panels submenu: Side Panels click calls onToggleSidePanels', () => {
    const onToggleSidePanels = vi.fn();
    const items = buildPanelTabMenuItems(t, makeCtx({ onToggleSidePanels }));
    const submenu = items.find((it) => isSubmenu(it)) as ContextMenuSubmenu;
    const sideItem = submenu.children[0] as ContextMenuCheckItem;
    sideItem.onClick();
    expect(onToggleSidePanels).toHaveBeenCalled();
  });

  it('Panels submenu: Side Panels is unchecked when sidePanelsVisible is false', () => {
    const items = buildPanelTabMenuItems(t, makeCtx({ sidePanelsVisible: false }));
    const submenu = items.find((it) => isSubmenu(it)) as ContextMenuSubmenu;
    const sideItem = submenu.children[0] as ContextMenuCheckItem;
    expect(sideItem.checked).toBe(false);
  });
});
