import { APP_COMMANDS, type AppCommandId } from './appCommandIds';
import { MENU_LABEL_KEYS, type MenuLabelEnglish } from '../i18n/menuLabelKeys';
import {
  hotkeyFromKeyboardEvent,
  matchesParsedHotkey,
  normalizeHotkey,
  parseHotkey,
  type ParsedHotkey,
} from '../utils/hotkeyMatch';

export interface CommandMetadata {
  id: AppCommandId;
  label: string;
  labelKey: string;
  group: string;
  groupKey: string;
  defaultHotkey?: string;
  editable: boolean;
}

export type CustomHotkeys = Record<string, string>;

type CommandDefinition = Omit<CommandMetadata, 'labelKey' | 'groupKey'>;

const COMMAND_DEFINITIONS: CommandDefinition[] = [
  { id: APP_COMMANDS.APP_PREFERENCES, label: 'Settings', group: 'Edit', defaultHotkey: 'Ctrl+,', editable: false },
  { id: APP_COMMANDS.FILE_NEW, label: 'New', group: 'File', defaultHotkey: 'Ctrl+n', editable: true },
  { id: APP_COMMANDS.FILE_OPEN, label: 'Open', group: 'File', defaultHotkey: 'Ctrl+o', editable: false },
  { id: APP_COMMANDS.FILE_IMPORT, label: 'Import', group: 'File', defaultHotkey: 'Ctrl+i', editable: true },
  { id: APP_COMMANDS.FILE_SAVE, label: 'Save', group: 'File', defaultHotkey: 'Ctrl+s', editable: false },
  { id: APP_COMMANDS.FILE_SAVE_AS, label: 'Save As', group: 'File', defaultHotkey: 'Ctrl+Shift+s', editable: false },
  { id: APP_COMMANDS.FILE_EXPORT, label: 'Export', group: 'File', defaultHotkey: 'Alt+x', editable: true },
  { id: APP_COMMANDS.FILE_PRINT_BLACK, label: 'Print Black', group: 'File', defaultHotkey: 'Ctrl+p', editable: true },
  { id: APP_COMMANDS.FILE_PRINT_COLORS, label: 'Print Colors', group: 'File', defaultHotkey: 'Ctrl+Shift+p', editable: true },
  { id: APP_COMMANDS.FILE_PREFS_IMPORT, label: 'Import Prefs', group: 'Preferences', editable: false },
  { id: APP_COMMANDS.FILE_PREFS_EXPORT, label: 'Export Prefs', group: 'Preferences', editable: false },
  { id: APP_COMMANDS.FILE_PREFS_OPEN_FOLDER, label: 'Open Prefs Folder', group: 'Preferences', editable: false },
  { id: APP_COMMANDS.FILE_PREFS_RESET_DEFAULTS, label: 'Reset Prefs to Defaults', group: 'Preferences', editable: false },
  { id: APP_COMMANDS.FILE_PREFS_EDIT_HOTKEYS, label: 'Edit Hotkeys', group: 'Preferences', editable: false },
  { id: APP_COMMANDS.APP_QUIT, label: 'Quit', group: 'App', defaultHotkey: 'Ctrl+q', editable: false },
  { id: APP_COMMANDS.EDIT_UNDO, label: 'Undo', group: 'Edit', defaultHotkey: 'Ctrl+z', editable: true },
  { id: APP_COMMANDS.EDIT_REDO, label: 'Redo', group: 'Edit', defaultHotkey: 'Ctrl+Shift+z', editable: true },
  { id: APP_COMMANDS.EDIT_SELECT_ALL, label: 'Select All', group: 'Edit', defaultHotkey: 'Ctrl+a', editable: true },
  { id: APP_COMMANDS.EDIT_INVERT_SELECTION, label: 'Invert Selection', group: 'Edit', defaultHotkey: 'Ctrl+Shift+i', editable: true },
  { id: APP_COMMANDS.EDIT_CUT, label: 'Cut', group: 'Edit', defaultHotkey: 'Ctrl+x', editable: false },
  { id: APP_COMMANDS.EDIT_COPY, label: 'Copy', group: 'Edit', defaultHotkey: 'Ctrl+c', editable: false },
  { id: APP_COMMANDS.EDIT_PASTE, label: 'Paste', group: 'Edit', defaultHotkey: 'Ctrl+v', editable: false },
  { id: APP_COMMANDS.EDIT_PASTE_IN_PLACE, label: 'Paste in Place', group: 'Edit', defaultHotkey: 'Alt+v', editable: true },
  { id: APP_COMMANDS.EDIT_DUPLICATE, label: 'Duplicate', group: 'Edit', defaultHotkey: 'Ctrl+d', editable: true },
  { id: APP_COMMANDS.EDIT_DELETE, label: 'Delete', group: 'Edit', defaultHotkey: 'Backspace', editable: true },
  { id: APP_COMMANDS.EDIT_SETTINGS, label: 'Settings', group: 'Edit', editable: false },
  { id: APP_COMMANDS.EDIT_CONVERT_TO_PATH, label: 'Convert to Path', group: 'Edit', defaultHotkey: 'Ctrl+Shift+c', editable: true },
  { id: APP_COMMANDS.EDIT_CONVERT_TO_BITMAP, label: 'Convert to Bitmap', group: 'Edit', defaultHotkey: 'Ctrl+Shift+b', editable: true },
  { id: APP_COMMANDS.EDIT_CLOSE_PATH, label: 'Close Path', group: 'Edit', editable: true },
  { id: APP_COMMANDS.EDIT_AUTO_JOIN_SELECTED_SHAPES, label: 'Auto-Join Selected Shapes', group: 'Edit', defaultHotkey: 'Alt+j', editable: true },
  { id: APP_COMMANDS.EDIT_CLOSE_AND_JOIN, label: 'Close & Join', group: 'Edit', editable: true },
  { id: APP_COMMANDS.EDIT_OPTIMIZE_SELECTED_SHAPES, label: 'Optimize Selected Shapes', group: 'Edit', defaultHotkey: 'Alt+Shift+o', editable: true },
  { id: APP_COMMANDS.EDIT_DELETE_DUPLICATES, label: 'Delete Duplicates', group: 'Edit', defaultHotkey: 'Alt+d', editable: true },
  { id: APP_COMMANDS.EDIT_CLOSE_SELECTED_PATHS_WITH_TOLERANCE, label: 'Close Selected Paths With Tolerance', group: 'Edit', editable: true },
  { id: APP_COMMANDS.EDIT_SELECT_OPEN_SHAPES, label: 'Select Open Shapes', group: 'Edit', editable: true },
  { id: APP_COMMANDS.EDIT_SELECT_OPEN_SHAPES_SET_TO_FILL, label: 'Select Open Shapes Set to Fill', group: 'Edit', editable: true },
  { id: APP_COMMANDS.EDIT_SELECT_ALL_SHAPES_IN_CURRENT_LAYER, label: 'Select All Shapes in Current Layer', group: 'Edit', editable: true },
  { id: APP_COMMANDS.EDIT_SELECT_CONTAINED_SHAPES, label: 'Select Contained Shapes', group: 'Edit', editable: true },
  { id: APP_COMMANDS.EDIT_SELECT_SHAPES_SMALLER_THAN_SELECTED, label: 'Select Shapes Smaller Than Selected', group: 'Edit', editable: true },
  { id: APP_COMMANDS.EDIT_IMAGE_REFRESH, label: 'Refresh Image', group: 'Edit', editable: true },
  { id: APP_COMMANDS.EDIT_IMAGE_REPLACE, label: 'Replace Image', group: 'Edit', editable: true },
  { id: APP_COMMANDS.EDIT_IMAGE_REPLACE_TO_FIT, label: 'Replace Image to Fit', group: 'Edit', editable: true },
  { id: APP_COMMANDS.TOOLS_SELECT, label: 'Select', group: 'Tools', defaultHotkey: 'Esc', editable: true },
  { id: APP_COMMANDS.TOOLS_NODE, label: 'Edit Nodes', group: 'Tools', defaultHotkey: 'Ctrl+`', editable: true },
  { id: APP_COMMANDS.TOOLS_LINE, label: 'Draw Lines', group: 'Tools', defaultHotkey: 'Ctrl+l', editable: true },
  { id: APP_COMMANDS.TOOLS_RECTANGLE, label: 'Rectangle', group: 'Tools', defaultHotkey: 'Ctrl+r', editable: true },
  { id: APP_COMMANDS.TOOLS_ELLIPSE, label: 'Ellipse', group: 'Tools', defaultHotkey: 'Ctrl+e', editable: true },
  { id: APP_COMMANDS.TOOLS_TRIANGLE, label: 'Triangle', group: 'Tools', editable: true },
  { id: APP_COMMANDS.TOOLS_PENTAGON, label: 'Pentagon', group: 'Tools', editable: true },
  { id: APP_COMMANDS.TOOLS_POLYGON, label: 'Polygon', group: 'Tools', editable: true },
  { id: APP_COMMANDS.TOOLS_OCTAGON, label: 'Octagon', group: 'Tools', editable: true },
  { id: APP_COMMANDS.TOOLS_STAR, label: 'Star', group: 'Tools', editable: true },
  { id: APP_COMMANDS.TOOLS_DUAL_STAR, label: 'Dual Star', group: 'Tools', editable: true },
  { id: APP_COMMANDS.TOOLS_TABS, label: 'Add Tabs', group: 'Tools', defaultHotkey: 'Ctrl+Tab', editable: true },
  { id: APP_COMMANDS.TOOLS_TRIM, label: 'Trim Shapes', group: 'Tools', defaultHotkey: 'Ctrl+k', editable: true },
  { id: APP_COMMANDS.TOOLS_TEXT, label: 'Edit Text', group: 'Tools', defaultHotkey: 'Ctrl+t', editable: true },
  { id: APP_COMMANDS.TOOLS_POSITION_LASER, label: 'Position Laser', group: 'Tools', defaultHotkey: 'Ctrl+Shift+l', editable: true },
  { id: APP_COMMANDS.TOOLS_MEASURE, label: 'Measure', group: 'Tools', defaultHotkey: 'Ctrl+m', editable: true },
  { id: APP_COMMANDS.TOOLS_BARCODE, label: 'Create Bar Code', group: 'Tools', editable: true },
  { id: APP_COMMANDS.TOOLS_OFFSET, label: 'Offset Shapes', group: 'Tools', defaultHotkey: 'Alt+o', editable: true },
  { id: APP_COMMANDS.TOOLS_BOOLEAN_WELD, label: 'Weld Shapes', group: 'Tools', defaultHotkey: 'Ctrl+w', editable: true },
  { id: APP_COMMANDS.TOOLS_BOOLEAN_UNION, label: 'Boolean Union', group: 'Tools', defaultHotkey: 'Alt++', editable: true },
  { id: APP_COMMANDS.TOOLS_BOOLEAN_SUBTRACT, label: 'Boolean Subtract', group: 'Tools', defaultHotkey: 'Alt+-', editable: true },
  { id: APP_COMMANDS.TOOLS_BOOLEAN_INTERSECTION, label: 'Boolean Intersection', group: 'Tools', defaultHotkey: 'Alt+*', editable: true },
  { id: APP_COMMANDS.TOOLS_BOOLEAN_ASSISTANT, label: 'Boolean Assistant', group: 'Tools', defaultHotkey: 'Ctrl+b', editable: true },
  { id: APP_COMMANDS.TOOLS_CUT_SHAPES, label: 'Cut Shapes', group: 'Tools', defaultHotkey: 'Alt+Shift+c', editable: true },
  { id: APP_COMMANDS.TOOLS_ADJUST_IMAGE, label: 'Adjust Image', group: 'Tools', defaultHotkey: 'Alt+i', editable: true },
  { id: APP_COMMANDS.TOOLS_TRACE_IMAGE, label: 'Trace Image', group: 'Tools', defaultHotkey: 'Alt+t', editable: true },
  { id: APP_COMMANDS.TOOLS_APPLY_PATH_TO_TEXT, label: 'Apply Path to Text', group: 'Tools', editable: true },
  { id: APP_COMMANDS.TOOLS_APPLY_MASK_TO_IMAGE, label: 'Apply Mask to Image', group: 'Tools', editable: true },
  { id: APP_COMMANDS.TOOLS_CROP_IMAGE, label: 'Crop Image', group: 'Tools', editable: true },
  { id: APP_COMMANDS.TOOLS_WARP_SELECTION, label: 'Warp Selection (4 Points)', group: 'Tools', editable: true },
  { id: APP_COMMANDS.TOOLS_DEFORM_SELECTION, label: 'Deform Selection (16 Points)', group: 'Tools', editable: true },
  { id: APP_COMMANDS.ARRANGE_GROUP, label: 'Group', group: 'Arrange', defaultHotkey: 'Ctrl+g', editable: true },
  { id: APP_COMMANDS.ARRANGE_UNGROUP, label: 'Ungroup', group: 'Arrange', defaultHotkey: 'Ctrl+u', editable: true },
  { id: APP_COMMANDS.ARRANGE_AUTO_GROUP, label: 'Auto-Group', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_ALIGN_CENTERS, label: 'Align Centers', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_ALIGN_LEFT, label: 'Align Left', group: 'Arrange', defaultHotkey: 'Alt+Left', editable: true },
  { id: APP_COMMANDS.ARRANGE_ALIGN_RIGHT, label: 'Align Right', group: 'Arrange', defaultHotkey: 'Alt+Right', editable: true },
  { id: APP_COMMANDS.ARRANGE_ALIGN_TOP, label: 'Align Top', group: 'Arrange', defaultHotkey: 'Alt+Up', editable: true },
  { id: APP_COMMANDS.ARRANGE_ALIGN_BOTTOM, label: 'Align Bottom', group: 'Arrange', defaultHotkey: 'Alt+Down', editable: true },
  { id: APP_COMMANDS.ARRANGE_ALIGN_CENTER_HORIZONTAL, label: 'Align Horizontal Centers', group: 'Arrange', defaultHotkey: 'Alt+PageDown', editable: true },
  { id: APP_COMMANDS.ARRANGE_ALIGN_CENTER_VERTICAL, label: 'Align Vertical Centers', group: 'Arrange', defaultHotkey: 'Alt+PageUp', editable: true },
  { id: APP_COMMANDS.ARRANGE_DISTRIBUTE_V_SPACED, label: 'Distribute V-Spaced', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_DISTRIBUTE_V_CENTERED, label: 'Distribute V-Centered', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_DISTRIBUTE_H_SPACED, label: 'Distribute H-Spaced', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_DISTRIBUTE_H_CENTERED, label: 'Distribute H-Centered', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_FRONT, label: 'Bring to Front', group: 'Arrange', defaultHotkey: 'Ctrl+PageUp', editable: true },
  { id: APP_COMMANDS.ARRANGE_FORWARD, label: 'Bring Forward', group: 'Arrange', defaultHotkey: 'PageUp', editable: true },
  { id: APP_COMMANDS.ARRANGE_BACKWARD, label: 'Send Backward', group: 'Arrange', defaultHotkey: 'PageDown', editable: true },
  { id: APP_COMMANDS.ARRANGE_BACK, label: 'Send to Back', group: 'Arrange', defaultHotkey: 'Ctrl+PageDown', editable: true },
  { id: APP_COMMANDS.ARRANGE_FLIP_HORIZONTAL, label: 'Flip Horizontal', group: 'Arrange', defaultHotkey: 'Ctrl+Shift+h', editable: true },
  { id: APP_COMMANDS.ARRANGE_FLIP_VERTICAL, label: 'Flip Vertical', group: 'Arrange', defaultHotkey: 'Ctrl+Shift+v', editable: true },
  { id: APP_COMMANDS.ARRANGE_MIRROR_ACROSS_LINE, label: 'Mirror Across Line', group: 'Arrange', defaultHotkey: 'Ctrl+Shift+m', editable: true },
  { id: APP_COMMANDS.ARRANGE_ROTATE_CW, label: 'Rotate 90° Clockwise', group: 'Arrange', defaultHotkey: '.', editable: true },
  { id: APP_COMMANDS.ARRANGE_ROTATE_CCW, label: 'Rotate 90° Counter-Clockwise', group: 'Arrange', defaultHotkey: ',', editable: true },
  { id: APP_COMMANDS.ARRANGE_TWO_POINT_ROTATE_SCALE, label: 'Two-Point Rotate / Scale', group: 'Arrange', defaultHotkey: 'Ctrl+2', editable: true },
  { id: APP_COMMANDS.ARRANGE_NEST_SELECTED, label: 'Nest Selected', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_GRID_ARRAY, label: 'Grid Array', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_CIRCULAR_ARRAY, label: 'Circular Array', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_MOVE_H_TOGETHER, label: 'Move H Together', group: 'Arrange', defaultHotkey: 'Alt+Shift+h', editable: true },
  { id: APP_COMMANDS.ARRANGE_MOVE_V_TOGETHER, label: 'Move V Together', group: 'Arrange', defaultHotkey: 'Alt+Shift+v', editable: true },
  { id: APP_COMMANDS.ARRANGE_DOCK_LEFT, label: 'Dock Left', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_DOCK_RIGHT, label: 'Dock Right', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_DOCK_UP, label: 'Dock Up', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_DOCK_DOWN, label: 'Dock Down', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_MOVE_TO_LASER_POSITION, label: 'Move to Laser Position', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_MOVE_TO_PAGE_CENTER, label: 'Move to Page Center', group: 'Arrange', defaultHotkey: 'p', editable: true },
  { id: APP_COMMANDS.ARRANGE_MOVE_TO_UPPER_LEFT, label: 'Move to Upper Left', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_MOVE_TO_UPPER_RIGHT, label: 'Move to Upper Right', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_MOVE_TO_LOWER_LEFT, label: 'Move to Lower Left', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_MOVE_TO_LOWER_RIGHT, label: 'Move to Lower Right', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_MOVE_TO_LEFT, label: 'Move to Left', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_MOVE_TO_RIGHT, label: 'Move to Right', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_MOVE_TO_TOP, label: 'Move to Top', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_MOVE_TO_BOTTOM, label: 'Move to Bottom', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_CENTER, label: 'Move Laser to Selection Center', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_UPPER_LEFT, label: 'Move Laser to Upper Left of Selection', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_UPPER_RIGHT, label: 'Move Laser to Upper Right of Selection', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_LOWER_LEFT, label: 'Move Laser to Lower Left of Selection', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_LOWER_RIGHT, label: 'Move Laser to Lower Right of Selection', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_LEFT, label: 'Move Laser to Left of Selection', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_RIGHT, label: 'Move Laser to Right of Selection', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_TOP, label: 'Move Laser to Top of Selection', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_BOTTOM, label: 'Move Laser to Bottom of Selection', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_JOG_LASER_LEFT, label: 'Jog Laser Left', group: 'Arrange', defaultHotkey: 'Alt+Ctrl+[', editable: true },
  { id: APP_COMMANDS.ARRANGE_JOG_LASER_RIGHT, label: 'Jog Laser Right', group: 'Arrange', defaultHotkey: 'Alt+Ctrl+]', editable: true },
  { id: APP_COMMANDS.ARRANGE_JOG_LASER_UP, label: 'Jog Laser Up', group: 'Arrange', defaultHotkey: 'Ctrl+Shift+]', editable: true },
  { id: APP_COMMANDS.ARRANGE_JOG_LASER_DOWN, label: 'Jog Laser Down', group: 'Arrange', defaultHotkey: 'Ctrl+Shift+[', editable: true },
  { id: APP_COMMANDS.ARRANGE_BREAK_APART, label: 'Break Apart', group: 'Arrange', defaultHotkey: 'Alt+b', editable: true },
  { id: APP_COMMANDS.ARRANGE_COPY_ALONG_PATH, label: 'Copy Along Path', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_LOCK, label: 'Lock Selected Shapes', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.ARRANGE_UNLOCK, label: 'Unlock Selected Shapes', group: 'Arrange', editable: true },
  { id: APP_COMMANDS.FILE_SAVE_MACHINE_FILES, label: 'Save Machine Files', group: 'Laser Tools', defaultHotkey: 'Alt+Shift+l', editable: true },
  { id: APP_COMMANDS.LASER_MATERIAL_TEST, label: 'Material Test', group: 'Laser Tools', editable: false },
  { id: APP_COMMANDS.LASER_FOCUS_TEST, label: 'Focus Test', group: 'Laser Tools', editable: false },
  { id: APP_COMMANDS.LASER_INTERVAL_TEST, label: 'Interval Test', group: 'Laser Tools', editable: false },
  { id: APP_COMMANDS.WINDOW_RESET_LAYOUT, label: 'Reset to Default Layout', group: 'Window', editable: false },
  { id: APP_COMMANDS.WINDOW_PREVIEW, label: 'Preview', group: 'Window', defaultHotkey: 'Alt+p', editable: true },
  { id: APP_COMMANDS.WINDOW_ZOOM_TO_PAGE, label: 'Zoom to Page', group: 'Window', defaultHotkey: 'Ctrl+0', editable: true },
  { id: APP_COMMANDS.WINDOW_ZOOM_IN, label: 'Zoom In', group: 'Window', defaultHotkey: 'Ctrl+=', editable: true },
  { id: APP_COMMANDS.WINDOW_ZOOM_OUT, label: 'Zoom Out', group: 'Window', defaultHotkey: 'Ctrl+-', editable: true },
  { id: APP_COMMANDS.WINDOW_FRAME_SELECTION, label: 'Frame Selection', group: 'Window', defaultHotkey: 'Ctrl+Shift+a', editable: true },
  { id: APP_COMMANDS.WINDOW_VIEW_STYLE_WIREFRAME_COARSE, label: 'Wireframe / Coarse', group: 'Window', editable: false },
  { id: APP_COMMANDS.WINDOW_VIEW_STYLE_WIREFRAME_SMOOTH, label: 'Wireframe / Smooth', group: 'Window', editable: false },
  { id: APP_COMMANDS.WINDOW_VIEW_STYLE_FILLED_COARSE, label: 'Filled / Coarse', group: 'Window', editable: false },
  { id: APP_COMMANDS.WINDOW_VIEW_STYLE_FILLED_SMOOTH, label: 'Filled / Smooth', group: 'Window', editable: false },
  { id: APP_COMMANDS.WINDOW_TOGGLE_WIREFRAME_FILLED, label: 'Toggle Wireframe / Filled', group: 'Window', defaultHotkey: 'Alt+Shift+w', editable: true },
  { id: APP_COMMANDS.WINDOW_SIDE_PANELS, label: 'Side Panels', group: 'Window', defaultHotkey: 'F12', editable: true },
  { id: APP_COMMANDS.WINDOW_PANEL_ART_LIBRARY, label: 'Art Library', group: 'Window', editable: false },
  { id: APP_COMMANDS.WINDOW_TOOLBAR_ARRANGE, label: 'Arrange', group: 'Window', editable: false },
  { id: APP_COMMANDS.WINDOW_TOOLBAR_ARRANGE_LONG, label: 'Arrange (Long)', group: 'Window', editable: false },
  { id: APP_COMMANDS.WINDOW_TOOLBAR_MODIFIERS, label: 'Modifiers', group: 'Window', editable: false },
  { id: APP_COMMANDS.WINDOW_PANEL_CAMERA_CONTROL, label: 'Camera Control', group: 'Window', editable: false },
  { id: APP_COMMANDS.WINDOW_PANEL_CONSOLE, label: 'Console', group: 'Window', editable: false },
  { id: APP_COMMANDS.WINDOW_PANEL_MACROS, label: 'Macros', group: 'Window', editable: false },
  { id: APP_COMMANDS.WINDOW_PANEL_CUTS_LAYERS, label: 'Cuts / Layers', group: 'Window', editable: false },
  { id: APP_COMMANDS.WINDOW_TOOLBAR_DOCKING, label: 'Docking', group: 'Window', editable: false },
  { id: APP_COMMANDS.WINDOW_PANEL_LASER, label: 'Laser Control', group: 'Window', editable: false },
  { id: APP_COMMANDS.WINDOW_PANEL_MATERIAL_LIBRARY, label: 'Material Library', group: 'Window', editable: false },
  { id: APP_COMMANDS.WINDOW_TOOLBAR_MAIN, label: 'Main', group: 'Window', editable: false },
  { id: APP_COMMANDS.WINDOW_PANEL_MOVE, label: 'Move', group: 'Window', editable: false },
  { id: APP_COMMANDS.WINDOW_PANEL_SHAPE_PROPERTIES, label: 'Shape Properties', group: 'Window', editable: false },
  { id: APP_COMMANDS.WINDOW_TOOLBAR_TOOLS, label: 'Tools', group: 'Window', editable: false },
  { id: APP_COMMANDS.HELP_QUICK_HELP, label: 'Quick Help', group: 'Help', defaultHotkey: 'F1', editable: true },
];

const COMMAND_LABEL_KEY_OVERRIDES: Partial<Record<AppCommandId, string>> = {
  [APP_COMMANDS.FILE_PRINT_BLACK]: 'menus.file.print_black',
  [APP_COMMANDS.FILE_PRINT_COLORS]: 'menus.file.print_colors',
  [APP_COMMANDS.FILE_PREFS_EDIT_HOTKEYS]: 'dialog.hotkey_editor.title',
  [APP_COMMANDS.APP_QUIT]: 'menus.app.quit',
  [APP_COMMANDS.LASER_MATERIAL_TEST]: 'dialog.material_test.title',
  [APP_COMMANDS.LASER_FOCUS_TEST]: 'dialog.focus_test.title',
  [APP_COMMANDS.LASER_INTERVAL_TEST]: 'dialog.interval_test.title',
};

const COMMAND_GROUP_KEYS: Record<string, string> = {
  App: 'menus.app.label',
  Arrange: 'menus.arrange.label',
  Edit: 'menus.edit.label',
  File: 'menus.file.label',
  Help: 'menus.help.label',
  'Laser Tools': 'menus.laser_tools.label',
  Preferences: 'menus.file.preferences',
  Tools: 'menus.tools.label',
  Window: 'menus.window.label',
};

function resolveCommandLabelKey(command: CommandDefinition): string {
  return COMMAND_LABEL_KEY_OVERRIDES[command.id] ?? MENU_LABEL_KEYS[command.label as MenuLabelEnglish] ?? '';
}

function resolveCommandGroupKey(command: CommandDefinition): string {
  return COMMAND_GROUP_KEYS[command.group] ?? '';
}

const COMMANDS: CommandMetadata[] = COMMAND_DEFINITIONS.map((command) => ({
  ...command,
  labelKey: resolveCommandLabelKey(command),
  groupKey: resolveCommandGroupKey(command),
}));

const COMMAND_MAP = new Map(COMMANDS.map((command) => [command.id, command]));
const CONVENTIONALLY_RESERVED_HOTKEYS = new Set(
  ['Ctrl+Q', 'Ctrl+Tab', 'Ctrl+Space', 'Alt+Tab']
    .map((spec) => normalizeHotkey(spec))
    .filter((spec): spec is string => spec !== null),
);

let cachedSignature = '';
let cachedParsedHotkeys = new Map<AppCommandId, ParsedHotkey>();

function signature(customHotkeys: CustomHotkeys): string {
  return Object.keys(customHotkeys)
    .sort()
    .map((key) => `${key}:${customHotkeys[key]}`)
    .join('|');
}

function rebuildCache(customHotkeys: CustomHotkeys): Map<AppCommandId, ParsedHotkey> {
  const next = new Map<AppCommandId, ParsedHotkey>();
  for (const command of COMMANDS) {
    const parsed = parseHotkey(getEffectiveHotkey(command.id, customHotkeys));
    if (parsed) next.set(command.id, parsed);
  }
  return next;
}

export function getCommandMetadata(): CommandMetadata[] {
  return COMMANDS;
}

export function getMissingCommandTranslationMetadata(): Array<{ id: AppCommandId; label?: string; group?: string }> {
  return COMMANDS
    .filter((command) => !command.labelKey || !command.groupKey)
    .map((command) => ({
      id: command.id,
      ...(command.labelKey ? {} : { label: command.label }),
      ...(command.groupKey ? {} : { group: command.group }),
    }));
}

export function getCommand(commandId: string): CommandMetadata | undefined {
  return COMMAND_MAP.get(commandId as AppCommandId);
}

export function getEffectiveHotkey(commandId: string, customHotkeys: CustomHotkeys): string | null {
  const command = getCommand(commandId);
  if (!command) return null;
  const custom = customHotkeys[commandId];
  return normalizeHotkey(custom) ?? normalizeHotkey(command.defaultHotkey);
}

export function getParsedHotkeyCache(customHotkeys: CustomHotkeys): Map<AppCommandId, ParsedHotkey> {
  const nextSignature = signature(customHotkeys);
  if (nextSignature !== cachedSignature) {
    cachedSignature = nextSignature;
    cachedParsedHotkeys = rebuildCache(customHotkeys);
  }
  return cachedParsedHotkeys;
}

function activeElementAcceptsTextInput(): boolean {
  if (typeof document === 'undefined') return false;
  const active = document.activeElement as HTMLElement | null;
  if (!active) return false;
  return active.tagName === 'INPUT' || active.tagName === 'TEXTAREA' || active.isContentEditable;
}

export function findCommandForKeyboardEvent(e: KeyboardEvent, customHotkeys: CustomHotkeys): AppCommandId | null {
  if (activeElementAcceptsTextInput()) return null;
  for (const [commandId, parsed] of getParsedHotkeyCache(customHotkeys)) {
    if (matchesParsedHotkey(parsed, e)) return commandId;
  }
  return null;
}

export function defaultHotkeyIsOverriddenByEvent(e: KeyboardEvent, customHotkeys: CustomHotkeys): boolean {
  const eventHotkey = hotkeyFromKeyboardEvent(e);
  if (!eventHotkey) return false;
  return COMMANDS.some((command) => {
    if (!command.editable || !customHotkeys[command.id]) return false;
    return normalizeHotkey(command.defaultHotkey) === eventHotkey;
  });
}

export function isReservedHotkey(spec: string): boolean {
  const normalized = normalizeHotkey(spec);
  if (!normalized) return true;
  if (CONVENTIONALLY_RESERVED_HOTKEYS.has(normalized)) return true;
  return COMMANDS.some((command) => !command.editable && normalizeHotkey(command.defaultHotkey) === normalized);
}

export function hotkeyConflictsWithCommand(
  commandId: string,
  spec: string,
  customHotkeys: CustomHotkeys,
): CommandMetadata | null {
  const normalized = normalizeHotkey(spec);
  if (!normalized) return null;
  for (const command of COMMANDS) {
    if (command.id === commandId) continue;
    if (getEffectiveHotkey(command.id, customHotkeys) === normalized) return command;
  }
  return null;
}

export function toNativeAccelerator(spec: string | null | undefined): string | null {
  const normalized = normalizeHotkey(spec);
  if (!normalized) return null;
  return normalized
    .split('+')
    .map((token) => (token === 'Ctrl' ? 'CmdOrCtrl' : token === 'Escape' ? 'Esc' : token))
    .join('+');
}

export function nativeAcceleratorForCommand(commandId: string, customHotkeys: CustomHotkeys): string | null {
  return toNativeAccelerator(getEffectiveHotkey(commandId, customHotkeys));
}

export function nativeAcceleratorUpdates(customHotkeys: CustomHotkeys) {
  return COMMANDS.map((command) => ({
    id: command.id,
    accelerator: nativeAcceleratorForCommand(command.id, customHotkeys),
  }));
}
