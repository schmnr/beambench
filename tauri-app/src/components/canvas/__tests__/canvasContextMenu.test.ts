import { describe, it, expect, vi } from 'vitest';
import i18n from '../../../i18n';
import { buildCanvasContextMenuItems } from '../canvasMenuItems';
import { createSelectionContext } from '../../../commands/selectionContext';

const t = i18n.getFixedT('en');
import type { ProjectObject } from '../../../types/project';
import type { SelectionContext } from '../../../commands/selectionContext';
import type {
  ContextMenuItem,
  ContextMenuSubmenu,
  ContextMenuCheckItem,
  ContextMenuEntry,
} from '../../shared/ContextMenu';
import { isSubmenu, isCheckItem, isSeparator } from '../../shared/ContextMenu';
import { makeProjectObject } from '../../../test-utils/projectFixtures';
import { useProjectStore } from '../../../stores/projectStore';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

function makeObject(overrides: Partial<ProjectObject> = {}): ProjectObject {
  return makeProjectObject({
    id: 'obj-1',
    name: 'Test Object',
    transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
    bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
    layer_id: 'layer-1',
    data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 },
    ...overrides,
  });
}

function ctx(
  selectedObjectIds: string[] = [],
  objects: ProjectObject[] = [],
  hasClipboard = false,
  hiddenPanelIds: string[] = [],
): SelectionContext {
  return createSelectionContext(selectedObjectIds, objects, hasClipboard, hiddenPanelIds);
}

function findItem(items: ContextMenuEntry[], id: string): ContextMenuItem | undefined {
  for (const entry of items) {
    if (!isSeparator(entry) && !isSubmenu(entry) && !isCheckItem(entry) && entry.id === id) {
      return entry;
    }
  }
  return undefined;
}

function findSubmenu(items: ContextMenuEntry[], id: string): ContextMenuSubmenu | undefined {
  for (const entry of items) {
    if (isSubmenu(entry) && entry.id === id) return entry;
  }
  return undefined;
}

describe('buildCanvasContextMenuItems', () => {
  it('returns expected base items', () => {
    const obj = makeObject();
    const items = buildCanvasContextMenuItems(t, ctx(['obj-1'], [obj]));

    expect(findItem(items, 'cut')).toBeDefined();
    expect(findItem(items, 'copy')).toBeDefined();
    expect(findItem(items, 'paste')).toBeDefined();
    expect(findItem(items, 'duplicate')).toBeDefined();
    expect(findItem(items, 'select-all')).toBeDefined();
    expect(findItem(items, 'delete')).toBeDefined();
    expect(findItem(items, 'group')).toBeDefined();
    expect(findItem(items, 'ungroup')).toBeDefined();
    expect(findItem(items, 'convert-path')).toBeDefined();
    expect(findItem(items, 'convert-bitmap')).toBeDefined();
    expect(findItem(items, 'preview')).toBeDefined();
    expect(findItem(items, 'show-properties')).toBeDefined();
  });

  it('has windows submenu', () => {
    const items = buildCanvasContextMenuItems(t, ctx());
    const sub = findSubmenu(items, 'windows');
    expect(sub).toBeDefined();
    expect(sub!.label).toBe('Windows');
    // Should contain check items for panels
    const checks = sub!.children.filter(isCheckItem);
    expect(checks.length).toBeGreaterThan(0);
  });

  it('windows submenu has side-panels toggle', () => {
    const items = buildCanvasContextMenuItems(t, ctx());
    const sub = findSubmenu(items, 'windows')!;
    const sidePanels = sub.children.find(
      (c) => isCheckItem(c) && c.id === 'window-side-panels',
    ) as ContextMenuCheckItem;
    expect(sidePanels).toBeDefined();
    expect(sidePanels.label).toBe('Side Panels');
  });

  it('disables cut/copy/duplicate/delete when no selection', () => {
    const obj = makeObject();
    const items = buildCanvasContextMenuItems(t, ctx([], [obj]));

    expect(findItem(items, 'cut')?.disabled).toBe(true);
    expect(findItem(items, 'copy')?.disabled).toBe(true);
    expect(findItem(items, 'duplicate')?.disabled).toBe(true);
    expect(findItem(items, 'delete')?.disabled).toBe(true);
  });

  it('enables cut/copy/duplicate/delete when selection exists', () => {
    const obj = makeObject();
    const items = buildCanvasContextMenuItems(t, ctx(['obj-1'], [obj]));

    expect(findItem(items, 'cut')?.disabled).toBe(false);
    expect(findItem(items, 'copy')?.disabled).toBe(false);
    expect(findItem(items, 'duplicate')?.disabled).toBe(false);
    expect(findItem(items, 'delete')?.disabled).toBe(false);
  });

  it('paste stays enabled with an empty object clipboard (system clipboard fallback)', () => {
    const items = buildCanvasContextMenuItems(t, ctx([], [], false));
    expect(findItem(items, 'paste')?.disabled).toBe(false);
  });

  it('paste enabled when clipboard has data', () => {
    const items = buildCanvasContextMenuItems(t, ctx([], [], true));
    expect(findItem(items, 'paste')?.disabled).toBe(false);
  });

  it('group disabled with fewer than 2 selected', () => {
    const obj = makeObject();
    const items = buildCanvasContextMenuItems(t, ctx(['obj-1'], [obj]));
    expect(findItem(items, 'group')?.disabled).toBe(true);
  });

  it('group enabled with 2+ selected', () => {
    const obj1 = makeObject({ id: 'obj-1' });
    const obj2 = makeObject({ id: 'obj-2' });
    const items = buildCanvasContextMenuItems(t, ctx(['obj-1', 'obj-2'], [obj1, obj2]));
    expect(findItem(items, 'group')?.disabled).toBe(false);
  });

  it('ungroup disabled when selection is not exactly 1', () => {
    const items = buildCanvasContextMenuItems(t, ctx([], []));
    expect(findItem(items, 'ungroup')?.disabled).toBe(true);

    const obj1 = makeObject({ id: 'obj-1' });
    const obj2 = makeObject({ id: 'obj-2' });
    const items2 = buildCanvasContextMenuItems(t, ctx(['obj-1', 'obj-2'], [obj1, obj2]));
    expect(findItem(items2, 'ungroup')?.disabled).toBe(true);
  });

  it('ungroup disabled when single selection is not a group', () => {
    const obj = makeObject({ id: 'obj-1' });
    const items = buildCanvasContextMenuItems(t, ctx(['obj-1'], [obj]));
    expect(findItem(items, 'ungroup')?.disabled).toBe(true);
  });

  it('ungroup enabled when single selection is a group', () => {
    const group = makeObject({
      id: 'grp-1',
      data: { type: 'group', children: ['c1', 'c2'] },
    });
    const items = buildCanvasContextMenuItems(t, ctx(['grp-1'], [group]));
    expect(findItem(items, 'ungroup')?.disabled).toBe(false);
  });

  it('lock item appears when unlocked objects selected', () => {
    const obj = makeObject({ locked: false });
    const items = buildCanvasContextMenuItems(t, ctx(['obj-1'], [obj]));
    expect(findItem(items, 'lock')).toBeDefined();
    expect(findItem(items, 'unlock')).toBeUndefined();
  });

  it('unlock item appears when locked objects selected', () => {
    const obj = makeObject({ locked: true });
    const items = buildCanvasContextMenuItems(t, ctx(['obj-1'], [obj]));
    expect(findItem(items, 'unlock')).toBeDefined();
    expect(findItem(items, 'lock')).toBeUndefined();
  });

  it('both lock and unlock appear when mixed lock state', () => {
    const obj1 = makeObject({ id: 'obj-1', locked: false });
    const obj2 = makeObject({ id: 'obj-2', locked: true });
    const items = buildCanvasContextMenuItems(t, ctx(['obj-1', 'obj-2'], [obj1, obj2]));
    expect(findItem(items, 'lock')).toBeDefined();
    expect(findItem(items, 'unlock')).toBeDefined();
  });

  it('no lock/unlock items when nothing selected', () => {
    const items = buildCanvasContextMenuItems(t, ctx([], []));
    expect(findItem(items, 'lock')).toBeUndefined();
    expect(findItem(items, 'unlock')).toBeUndefined();
  });

  it('mutating actions disabled when selected object is locked', () => {
    const obj = makeObject({ locked: true });
    const items = buildCanvasContextMenuItems(t, ctx(['obj-1'], [obj]));

    expect(findItem(items, 'cut')?.disabled).toBe(true);
    expect(findItem(items, 'duplicate')?.disabled).toBe(true);
    expect(findItem(items, 'delete')?.disabled).toBe(true);
    expect(findItem(items, 'convert-path')?.disabled).toBe(true);
    expect(findItem(items, 'convert-bitmap')?.disabled).toBe(true);

    // Copy should still work on locked objects
    expect(findItem(items, 'copy')?.disabled).toBe(false);

    // Unlock should be available
    expect(findItem(items, 'unlock')).toBeDefined();
  });

  it('group disabled when any selected object is locked', () => {
    const obj1 = makeObject({ id: 'obj-1', locked: false });
    const obj2 = makeObject({ id: 'obj-2', locked: true });
    const items = buildCanvasContextMenuItems(t, ctx(['obj-1', 'obj-2'], [obj1, obj2]));
    expect(findItem(items, 'group')?.disabled).toBe(true);
  });

  it('convert-bitmap disabled for single raster_image', () => {
    const raster = makeObject({
      id: 'img-1',
      data: { type: 'raster_image', asset_key: 'k1', original_width_px: 100, original_height_px: 100 },
    });
    const items = buildCanvasContextMenuItems(t, ctx(['img-1'], [raster]));
    expect(findItem(items, 'convert-bitmap')?.disabled).toBe(true);
  });

  it('convert-path disabled for single raster_image', () => {
    const raster = makeObject({
      id: 'img-1',
      data: { type: 'raster_image', asset_key: 'k1', original_width_px: 100, original_height_px: 100 },
    });
    const items = buildCanvasContextMenuItems(t, ctx(['img-1'], [raster]));
    expect(findItem(items, 'convert-path')?.disabled).toBe(true);
  });

  it('menu order matches the product specification', () => {
    const obj = makeObject();
    const items = buildCanvasContextMenuItems(t, ctx(['obj-1'], [obj]));

    // Extract ids/types in order, skipping separators
    const ids: string[] = [];
    for (const entry of items) {
      if (isSeparator(entry)) continue;
      if (isSubmenu(entry)) ids.push(entry.id);
      else if (isCheckItem(entry)) ids.push(entry.id);
      else ids.push(entry.id);
    }

    const expectedOrder = [
      'windows',
      'cut', 'copy', 'paste', 'duplicate',
      'delete', 'select-all',
      'group', 'ungroup',
      'lock', // single unlocked object
      'convert-path', 'convert-bitmap',
      // trace-image/adjust-image only appear for raster_image
      'preview', 'show-properties',
    ];

    expect(ids).toEqual(expectedOrder);
  });

  it('trace-image present for single raster_image', () => {
    const raster = makeObject({
      id: 'img-1',
      data: { type: 'raster_image', asset_key: 'k1', original_width_px: 100, original_height_px: 100 },
    });
    const items = buildCanvasContextMenuItems(t, ctx(['img-1'], [raster]));
    expect(findItem(items, 'trace-image')).toBeDefined();
  });

  it('trace-image hidden for vector object', () => {
    const obj = makeObject();
    const items = buildCanvasContextMenuItems(t, ctx(['obj-1'], [obj]));
    expect(findItem(items, 'trace-image')).toBeUndefined();
  });

  it('trace-image hidden when no selection', () => {
    const items = buildCanvasContextMenuItems(t, ctx());
    expect(findItem(items, 'trace-image')).toBeUndefined();
  });

  it('trace-image callback fires on click', () => {
    const raster = makeObject({
      id: 'img-1',
      data: { type: 'raster_image', asset_key: 'k1', original_width_px: 100, original_height_px: 100 },
    });
    const onTraceImage = vi.fn();
    const items = buildCanvasContextMenuItems(t, ctx(['img-1'], [raster]), { onTraceImage });
    findItem(items, 'trace-image')?.onClick?.();
    expect(onTraceImage).toHaveBeenCalledOnce();
  });

  it('does not contain removed items (close-and-join, edit-nodes)', () => {
    const vec = makeObject({
      id: 'vec-1',
      data: { type: 'vector_path', path_data: 'M 0 0 L 10 10', closed: false },
    });
    const items = buildCanvasContextMenuItems(t, ctx(['vec-1'], [vec]));
    expect(findItem(items, 'close-and-join')).toBeUndefined();
    expect(findItem(items, 'edit-nodes')).toBeUndefined();
  });

  it('adjust-image present for single raster_image', () => {
    const raster = makeObject({
      id: 'img-1',
      data: { type: 'raster_image', asset_key: 'k1', original_width_px: 100, original_height_px: 100 },
    });
    const items = buildCanvasContextMenuItems(t, ctx(['img-1'], [raster]));
    expect(findItem(items, 'adjust-image')).toBeDefined();
  });

  it('adjust-image hidden for vector object', () => {
    const obj = makeObject();
    const items = buildCanvasContextMenuItems(t, ctx(['obj-1'], [obj]));
    expect(findItem(items, 'adjust-image')).toBeUndefined();
  });

  it('adjust-image callback fires on click', () => {
    const raster = makeObject({
      id: 'img-1',
      data: { type: 'raster_image', asset_key: 'k1', original_width_px: 100, original_height_px: 100 },
    });
    const onAdjustImage = vi.fn();
    const items = buildCanvasContextMenuItems(t, ctx(['img-1'], [raster]), { onAdjustImage });
    findItem(items, 'adjust-image')?.onClick?.();
    expect(onAdjustImage).toHaveBeenCalledOnce();
  });

  it('save-processed-bitmap appears for single raster_image and calls callback', () => {
    const raster = makeObject({
      id: 'img-1',
      data: { type: 'raster_image', asset_key: 'k1', original_width_px: 100, original_height_px: 100 },
    });
    const onSaveProcessedBitmap = vi.fn();
    const items = buildCanvasContextMenuItems(t, ctx(['img-1'], [raster]), { onSaveProcessedBitmap });
    const item = findItem(items, 'save-processed-bitmap');

    expect(item).toBeDefined();
    item?.onClick?.();
    expect(onSaveProcessedBitmap).toHaveBeenCalledOnce();
  });

  it('remove-image-mask appears only for masked raster images', () => {
    const unmaskedRaster = makeObject({
      id: 'img-1',
      data: { type: 'raster_image', asset_key: 'k1', original_width_px: 100, original_height_px: 100, masks: [] },
    });
    const maskedRaster = makeObject({
      id: 'img-2',
      data: {
        type: 'raster_image',
        asset_key: 'k2',
        original_width_px: 100,
        original_height_px: 100,
        masks: [{ object_id: 'mask-1', polarity: 'keep_inside' }],
      },
    });
    const vector = makeObject({
      id: 'vec-1',
      data: { type: 'vector_path', path_data: 'M 0 0 L 10 0 L 10 10 Z', closed: true },
    });

    expect(findItem(buildCanvasContextMenuItems(t, ctx(['img-1'], [unmaskedRaster])), 'remove-image-mask')).toBeUndefined();
    expect(findItem(buildCanvasContextMenuItems(t, ctx(['vec-1'], [vector])), 'remove-image-mask')).toBeUndefined();
    expect(findItem(buildCanvasContextMenuItems(t, ctx(['img-2'], [maskedRaster])), 'remove-image-mask')).toBeDefined();
  });

  it('use-as-image-mask appears for raster plus vector mask selection', () => {
    const raster = makeObject({
      id: 'img-1',
      data: { type: 'raster_image', asset_key: 'k1', original_width_px: 100, original_height_px: 100 },
    });
    const mask = makeObject({
      id: 'mask-1',
      data: { type: 'vector_path', path_data: 'M 0 0 L 10 0 L 10 10 Z', closed: true },
    });
    const items = buildCanvasContextMenuItems(t, ctx(['img-1', 'mask-1'], [raster, mask]));

    expect(findItem(items, 'use-as-image-mask')).toBeDefined();
  });

  it('use-as-image-mask calls assignImageMask for the selected raster and masks', () => {
    const raster = makeObject({
      id: 'img-1',
      data: { type: 'raster_image', asset_key: 'k1', original_width_px: 100, original_height_px: 100 },
    });
    const mask = makeObject({
      id: 'mask-1',
      data: { type: 'vector_path', path_data: 'M 0 0 L 10 0 L 10 10 Z', closed: true },
    });
    const assignImageMask = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ assignImageMask });

    const items = buildCanvasContextMenuItems(t, ctx(['img-1', 'mask-1'], [raster, mask]));
    findItem(items, 'use-as-image-mask')?.onClick?.();

    expect(assignImageMask).toHaveBeenCalledWith('img-1', ['mask-1'], 'keep_inside');
  });

  it('remove-image-mask calls removeImageMask for the selected image', () => {
    const maskedRaster = makeObject({
      id: 'img-1',
      data: {
        type: 'raster_image',
        asset_key: 'k1',
        original_width_px: 100,
        original_height_px: 100,
        masks: [{ object_id: 'mask-1', polarity: 'keep_inside' }],
      },
    });
    const removeImageMask = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ removeImageMask });

    const items = buildCanvasContextMenuItems(t, ctx(['img-1'], [maskedRaster]));
    findItem(items, 'remove-image-mask')?.onClick?.();

    expect(removeImageMask).toHaveBeenCalledWith('img-1');
  });
});
