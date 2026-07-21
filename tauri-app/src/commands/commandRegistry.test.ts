import { describe, expect, it } from 'vitest';
import { APP_COMMANDS } from './appCommandIds';
import {
  getCommand,
  getCommandMetadata,
  getEffectiveHotkey,
  getMissingCommandTranslationMetadata,
  hotkeyConflictsWithCommand,
  nativeAcceleratorForCommand,
} from './commandRegistry';
import { normalizeHotkey } from '../utils/hotkeyMatch';
import { TOOLS_MENU_CONTRACT } from './toolsMenuContract';
import { WINDOW_MENU_COMMAND_ORDER } from './windowMenuDefinitions';

describe('command registry product ids', () => {
  it('uses Arrange ids for Group and Ungroup while preserving accelerators', () => {
    const ids = new Set<string>(getCommandMetadata().map((command) => command.id));

    expect(ids.has('edit.group')).toBe(false);
    expect(ids.has('edit.ungroup')).toBe(false);
    expect(getCommand(APP_COMMANDS.ARRANGE_GROUP)).toMatchObject({
      id: 'arrange.group',
      group: 'Arrange',
      defaultHotkey: 'Ctrl+g',
    });
    expect(getCommand(APP_COMMANDS.ARRANGE_UNGROUP)).toMatchObject({
      id: 'arrange.ungroup',
      group: 'Arrange',
      defaultHotkey: 'Ctrl+u',
    });
  });

  it('registers Arrange shortcuts on the canonical command ids', () => {
    const shortcuts: Array<[string, string]> = [
      [APP_COMMANDS.ARRANGE_GROUP, 'Ctrl+g'],
      [APP_COMMANDS.ARRANGE_UNGROUP, 'Ctrl+u'],
      [APP_COMMANDS.ARRANGE_FLIP_HORIZONTAL, 'Ctrl+Shift+h'],
      [APP_COMMANDS.ARRANGE_FLIP_VERTICAL, 'Ctrl+Shift+v'],
      [APP_COMMANDS.ARRANGE_MIRROR_ACROSS_LINE, 'Ctrl+Shift+m'],
      [APP_COMMANDS.ARRANGE_ROTATE_CW, '.'],
      [APP_COMMANDS.ARRANGE_ROTATE_CCW, ','],
      [APP_COMMANDS.ARRANGE_TWO_POINT_ROTATE_SCALE, 'Ctrl+2'],
      [APP_COMMANDS.ARRANGE_ALIGN_LEFT, 'Alt+Left'],
      [APP_COMMANDS.ARRANGE_ALIGN_RIGHT, 'Alt+Right'],
      [APP_COMMANDS.ARRANGE_ALIGN_TOP, 'Alt+Up'],
      [APP_COMMANDS.ARRANGE_ALIGN_BOTTOM, 'Alt+Down'],
      [APP_COMMANDS.ARRANGE_ALIGN_CENTER_VERTICAL, 'Alt+PageUp'],
      [APP_COMMANDS.ARRANGE_ALIGN_CENTER_HORIZONTAL, 'Alt+PageDown'],
      [APP_COMMANDS.ARRANGE_MOVE_H_TOGETHER, 'Alt+Shift+h'],
      [APP_COMMANDS.ARRANGE_MOVE_V_TOGETHER, 'Alt+Shift+v'],
      [APP_COMMANDS.ARRANGE_MOVE_TO_PAGE_CENTER, 'p'],
      [APP_COMMANDS.ARRANGE_FORWARD, 'PageUp'],
      [APP_COMMANDS.ARRANGE_BACKWARD, 'PageDown'],
      [APP_COMMANDS.ARRANGE_FRONT, 'Ctrl+PageUp'],
      [APP_COMMANDS.ARRANGE_BACK, 'Ctrl+PageDown'],
      [APP_COMMANDS.ARRANGE_JOG_LASER_LEFT, 'Alt+Ctrl+['],
      [APP_COMMANDS.ARRANGE_JOG_LASER_RIGHT, 'Alt+Ctrl+]'],
      [APP_COMMANDS.ARRANGE_JOG_LASER_UP, 'Ctrl+Shift+]'],
      [APP_COMMANDS.ARRANGE_JOG_LASER_DOWN, 'Ctrl+Shift+['],
    ];

    for (const [id, hotkey] of shortcuts) {
      expect(getEffectiveHotkey(id, {})).toBe(normalizeHotkey(hotkey));
    }
    expect(nativeAcceleratorForCommand(APP_COMMANDS.ARRANGE_ALIGN_LEFT, {})).toBe('Alt+ArrowLeft');
    expect(nativeAcceleratorForCommand(APP_COMMANDS.ARRANGE_TWO_POINT_ROTATE_SCALE, {})).toBe('CmdOrCtrl+2');
    expect(nativeAcceleratorForCommand(APP_COMMANDS.ARRANGE_JOG_LASER_LEFT, {})).toBe('CmdOrCtrl+Alt+[');
  });

  it('removes BB-only sizing commands from the Arrange registry', () => {
    expect(getCommand(APP_COMMANDS.ARRANGE_MAKE_SAME_WIDTH)).toBeUndefined();
    expect(getCommand(APP_COMMANDS.ARRANGE_MAKE_SAME_HEIGHT)).toBeUndefined();
    expect(getCommand('arrange.resize_slots')).toBeUndefined();
    expect(getCommand('tools.resize_slots')).toBeUndefined();
  });

  it('registers the relocated Tools Boolean section definitively', () => {
    const booleanIds = getCommandMetadata()
      .filter((command) => command.id.startsWith('tools.boolean.'))
      .map((command) => command.id);

    expect(booleanIds).toEqual([
      APP_COMMANDS.TOOLS_BOOLEAN_WELD,
      APP_COMMANDS.TOOLS_BOOLEAN_UNION,
      APP_COMMANDS.TOOLS_BOOLEAN_SUBTRACT,
      APP_COMMANDS.TOOLS_BOOLEAN_INTERSECTION,
      APP_COMMANDS.TOOLS_BOOLEAN_ASSISTANT,
    ]);
  });

  it('pins Tools menu contract shortcuts', () => {
    const shortcutFor = (label: string) =>
      TOOLS_MENU_CONTRACT.find((item) => item.label === label)?.shortcut;

    expect(shortcutFor('Draw Lines')).toBe('Ctrl/Cmd+L');
    expect(shortcutFor('Edit Nodes')).toBe('Ctrl/Cmd+`');
    expect(shortcutFor('Add Tabs')).toBe('Ctrl/Cmd+Tab');
    expect(shortcutFor('Edit Text')).toBe('Ctrl/Cmd+T');
    expect(shortcutFor('Position Laser')).toBe('Ctrl/Cmd+Shift+L');
    expect(shortcutFor('Measure')).toBe('Ctrl/Cmd+M');
    expect(shortcutFor('Offset Shapes')).toBe('Alt/Option+O');
    expect(shortcutFor('Weld Shapes')).toBe('Ctrl/Cmd+W');
    expect(shortcutFor('Boolean Assistant')).toBe('Ctrl/Cmd+B');
    expect(shortcutFor('Cut Shapes')).toBe('Alt/Option+Shift+C');
    expect(shortcutFor('Adjust Image')).toBe('Alt/Option+I');
    expect(shortcutFor('Trace Image')).toBe('Alt/Option+T');
  });

  it('moves Convert to Bitmap to Edit while keeping its shortcut', () => {
    const ids = getCommandMetadata().map((command) => command.id);

    expect(ids).toContain(APP_COMMANDS.EDIT_CONVERT_TO_BITMAP);
    expect(ids).not.toContain('tools.convert_to_bitmap');
    expect(getEffectiveHotkey(APP_COMMANDS.EDIT_CONVERT_TO_BITMAP, {})).toBe('Ctrl+Shift+b');
    expect(getEffectiveHotkey(APP_COMMANDS.EDIT_CONVERT_TO_PATH, {})).toBe('Ctrl+Shift+c');
  });

  it('registers new Edit ids and keeps Settings as the App Preferences alias action', () => {
    expect(getCommand(APP_COMMANDS.APP_PREFERENCES)).toMatchObject({
      label: 'Settings',
      defaultHotkey: 'Ctrl+,',
      editable: false,
    });
    expect(getCommand(APP_COMMANDS.EDIT_SETTINGS)).toMatchObject({
      label: 'Settings',
      group: 'Edit',
      editable: false,
    });
    expect(getCommand(APP_COMMANDS.EDIT_CLOSE_SELECTED_PATHS_WITH_TOLERANCE)).toBeDefined();
    expect(getCommand(APP_COMMANDS.EDIT_SELECT_OPEN_SHAPES)).toBeDefined();
    expect(getCommand(APP_COMMANDS.EDIT_SELECT_OPEN_SHAPES_SET_TO_FILL)).toBeDefined();
    expect(getCommand(APP_COMMANDS.EDIT_SELECT_ALL_SHAPES_IN_CURRENT_LAYER)).toBeDefined();
    expect(getCommand(APP_COMMANDS.EDIT_SELECT_CONTAINED_SHAPES)).toBeDefined();
    expect(getCommand(APP_COMMANDS.EDIT_SELECT_SHAPES_SMALLER_THAN_SELECTED)).toBeDefined();
    expect(getCommand(APP_COMMANDS.EDIT_IMAGE_REFRESH)).toBeDefined();
    expect(getCommand(APP_COMMANDS.EDIT_IMAGE_REPLACE)).toBeDefined();
    expect(getCommand(APP_COMMANDS.EDIT_IMAGE_REPLACE_TO_FIT)).toBeDefined();
    expect(getCommand(APP_COMMANDS.EDIT_CLOSE_AND_JOIN)).toBeDefined();
  });

  it('keeps the documented hotkeys conflict-free for import and invert selection', () => {
    expect(hotkeyConflictsWithCommand(APP_COMMANDS.FILE_IMPORT, 'Ctrl+i', {})).toBeNull();
    expect(hotkeyConflictsWithCommand(APP_COMMANDS.EDIT_INVERT_SELECTION, 'Ctrl+Shift+i', {})).toBeNull();
    expect(hotkeyConflictsWithCommand(APP_COMMANDS.EDIT_CONVERT_TO_BITMAP, 'Ctrl+Shift+b', {})).toBeNull();
    expect(hotkeyConflictsWithCommand(APP_COMMANDS.ARRANGE_MOVE_H_TOGETHER, 'Alt+Shift+h', {})).toBeNull();
    expect(hotkeyConflictsWithCommand(APP_COMMANDS.ARRANGE_MOVE_V_TOGETHER, 'Alt+Shift+v', {})).toBeNull();
  });

  it('registers Window commands in product order', () => {
    const windowIds = getCommandMetadata()
      .filter((command) => command.group === 'Window')
      .map((command) => command.id);

    expect(windowIds).toEqual([...WINDOW_MENU_COMMAND_ORDER]);
    expect(getEffectiveHotkey(APP_COMMANDS.WINDOW_PREVIEW, {})).toBe('Alt+p');
    expect(getEffectiveHotkey(APP_COMMANDS.WINDOW_ZOOM_TO_PAGE, {})).toBe('Ctrl+0');
    expect(getEffectiveHotkey(APP_COMMANDS.WINDOW_FRAME_SELECTION, {})).toBe('Ctrl+Shift+a');
    expect(getEffectiveHotkey(APP_COMMANDS.WINDOW_TOGGLE_WIREFRAME_FILLED, {})).toBe('Shift+Alt+w');
  });

  it('registers Laser Tools test commands for native menu dispatch', () => {
    expect(getCommand(APP_COMMANDS.LASER_MATERIAL_TEST)).toMatchObject({
      label: 'Material Test',
      group: 'Laser Tools',
    });
    expect(getCommand(APP_COMMANDS.LASER_FOCUS_TEST)).toMatchObject({
      label: 'Focus Test',
      group: 'Laser Tools',
    });
    expect(getCommand(APP_COMMANDS.LASER_INTERVAL_TEST)).toMatchObject({
      label: 'Interval Test',
      group: 'Laser Tools',
    });
  });

  it('maps every command to Hotkey Editor translation keys', () => {
    expect(getMissingCommandTranslationMetadata()).toEqual([]);
  });
});
