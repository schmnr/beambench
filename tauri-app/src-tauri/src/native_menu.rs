use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use beambench_service::ServiceContext;
use serde::{Deserialize, Deserializer, Serialize};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, State};

pub const NATIVE_MENU_EVENT: &str = "native-menu-command";

pub mod command {
    pub const APP_ABOUT: &str = "app.about";
    pub const APP_PREFERENCES: &str = "app.preferences";
    pub const APP_QUIT: &str = "app.quit";

    pub const FILE_NEW: &str = "file.new";
    pub const FILE_NEW_WINDOW: &str = "file.new_window";
    pub const FILE_OPEN_RECENT: &str = "file.open_recent";
    pub const FILE_RECENT_EMPTY: &str = "file.recent.empty";
    pub const FILE_OPEN: &str = "file.open";
    pub const FILE_IMPORT: &str = "file.import";
    pub const FILE_NOTES: &str = "file.notes";
    pub const FILE_SAVE: &str = "file.save";
    pub const FILE_SAVE_AS: &str = "file.save_as";
    pub const FILE_EXPORT: &str = "file.export";
    pub const FILE_PREFS_IMPORT: &str = "file.preferences.import";
    pub const FILE_PREFS_EXPORT: &str = "file.preferences.export";
    pub const FILE_PREFS_OPEN_FOLDER: &str = "file.preferences.open_folder";
    pub const FILE_PREFS_RESET_DEFAULTS: &str = "file.preferences.reset_defaults";
    pub const FILE_PRINT_BLACK: &str = "file.print_black";
    pub const FILE_PRINT_COLORS: &str = "file.print_colors";
    pub const FILE_SAVE_PROCESSED_BITMAP: &str = "file.save_processed_bitmap";
    pub const FILE_SAVE_BACKGROUND_CAPTURE: &str = "file.save_background_capture";

    pub const EDIT_UNDO: &str = "edit.undo";
    pub const EDIT_REDO: &str = "edit.redo";
    pub const EDIT_SELECT_ALL: &str = "edit.select_all";
    pub const EDIT_INVERT_SELECTION: &str = "edit.invert_selection";
    pub const EDIT_CUT: &str = "edit.cut";
    pub const EDIT_COPY: &str = "edit.copy";
    pub const EDIT_PASTE: &str = "edit.paste";
    pub const EDIT_PASTE_IN_PLACE: &str = "edit.paste_in_place";
    pub const EDIT_DUPLICATE: &str = "edit.duplicate";
    pub const EDIT_DELETE: &str = "edit.delete";
    pub const EDIT_SETTINGS: &str = "edit.settings";
    pub const EDIT_CONVERT_TO_PATH: &str = "edit.convert_to_path";
    pub const EDIT_CONVERT_TO_BITMAP: &str = "edit.convert_to_bitmap";
    pub const EDIT_CLOSE_PATH: &str = "edit.close_path";
    pub const EDIT_AUTO_JOIN_SELECTED_SHAPES: &str = "edit.auto_join_selected_shapes";
    pub const EDIT_CLOSE_AND_JOIN: &str = "edit.close_and_join";
    pub const EDIT_OPTIMIZE_SELECTED_SHAPES: &str = "edit.optimize_selected_shapes";
    pub const EDIT_DELETE_DUPLICATES: &str = "edit.delete_duplicates";
    pub const EDIT_CLOSE_SELECTED_PATHS_WITH_TOLERANCE: &str =
        "edit.close_selected_paths_with_tolerance";
    pub const EDIT_SELECT_OPEN_SHAPES: &str = "edit.select_open_shapes";
    pub const EDIT_SELECT_OPEN_SHAPES_SET_TO_FILL: &str = "edit.select_open_shapes_set_to_fill";
    pub const EDIT_SELECT_ALL_SHAPES_IN_CURRENT_LAYER: &str =
        "edit.select_all_shapes_in_current_layer";
    pub const EDIT_SELECT_CONTAINED_SHAPES: &str = "edit.select_contained_shapes";
    pub const EDIT_SELECT_SHAPES_SMALLER_THAN_SELECTED: &str =
        "edit.select_shapes_smaller_than_selected";
    pub const EDIT_IMAGE_REFRESH: &str = "edit.image.refresh";
    pub const EDIT_IMAGE_REPLACE: &str = "edit.image.replace";
    pub const EDIT_IMAGE_REPLACE_TO_FIT: &str = "edit.image.replace_to_fit";

    pub const TOOLS_SELECT: &str = "tools.select";
    pub const TOOLS_NODE: &str = "tools.node";
    pub const TOOLS_LINE: &str = "tools.line";
    pub const TOOLS_TRIANGLE: &str = "tools.triangle";
    pub const TOOLS_RECTANGLE: &str = "tools.rectangle";
    pub const TOOLS_ELLIPSE: &str = "tools.ellipse";
    pub const TOOLS_PENTAGON: &str = "tools.pentagon";
    pub const TOOLS_TEXT: &str = "tools.text";
    pub const TOOLS_POLYGON: &str = "tools.polygon";
    pub const TOOLS_OCTAGON: &str = "tools.octagon";
    pub const TOOLS_STAR: &str = "tools.star";
    pub const TOOLS_DUAL_STAR: &str = "tools.dual_star";
    pub const TOOLS_POSITION_LASER: &str = "tools.position_laser";
    pub const TOOLS_MEASURE: &str = "tools.measure";
    pub const TOOLS_OFFSET: &str = "tools.offset";
    pub const TOOLS_TRACE_IMAGE: &str = "tools.trace_image";
    pub const TOOLS_ADJUST_IMAGE: &str = "tools.adjust_image";
    pub const TOOLS_TABS: &str = "tools.tabs";
    pub const TOOLS_TRIM: &str = "tools.trim";
    pub const TOOLS_BARCODE: &str = "tools.barcode";
    pub const TOOLS_APPLY_PATH_TO_TEXT: &str = "tools.apply_path_to_text";
    pub const TOOLS_APPLY_MASK_TO_IMAGE: &str = "tools.apply_mask_to_image";
    pub const TOOLS_CROP_IMAGE: &str = "tools.crop_image";
    pub const TOOLS_WARP_SELECTION: &str = "tools.warp_selection";
    pub const TOOLS_DEFORM_SELECTION: &str = "tools.deform_selection";
    pub const TOOLS_CUT_SHAPES: &str = "tools.cut_shapes";
    pub const TOOLS_BOOLEAN_UNION: &str = "tools.boolean.union";
    pub const TOOLS_BOOLEAN_SUBTRACT: &str = "tools.boolean.subtract";
    pub const TOOLS_BOOLEAN_INTERSECTION: &str = "tools.boolean.intersection";
    pub const TOOLS_BOOLEAN_WELD: &str = "tools.boolean.weld";
    pub const TOOLS_BOOLEAN_ASSISTANT: &str = "tools.boolean.assistant";

    pub const ARRANGE_GROUP: &str = "arrange.group";
    pub const ARRANGE_UNGROUP: &str = "arrange.ungroup";
    pub const ARRANGE_AUTO_GROUP: &str = "arrange.auto_group";
    pub const ARRANGE_TWO_POINT_ROTATE_SCALE: &str = "arrange.two_point_rotate_scale";
    pub const ARRANGE_ALIGN_CENTERS: &str = "arrange.align.centers";
    pub const ARRANGE_ALIGN_LEFT: &str = "arrange.align.left";
    pub const ARRANGE_ALIGN_RIGHT: &str = "arrange.align.right";
    pub const ARRANGE_ALIGN_TOP: &str = "arrange.align.top";
    pub const ARRANGE_ALIGN_BOTTOM: &str = "arrange.align.bottom";
    pub const ARRANGE_ALIGN_CENTER_HORIZONTAL: &str = "arrange.align.center_horizontal";
    pub const ARRANGE_ALIGN_CENTER_VERTICAL: &str = "arrange.align.center_vertical";
    pub const ARRANGE_DISTRIBUTE_V_SPACED: &str = "arrange.distribute.v_spaced";
    pub const ARRANGE_DISTRIBUTE_V_CENTERED: &str = "arrange.distribute.v_centered";
    pub const ARRANGE_DISTRIBUTE_H_SPACED: &str = "arrange.distribute.h_spaced";
    pub const ARRANGE_DISTRIBUTE_H_CENTERED: &str = "arrange.distribute.h_centered";
    pub const ARRANGE_FRONT: &str = "arrange.front";
    pub const ARRANGE_FORWARD: &str = "arrange.forward";
    pub const ARRANGE_BACKWARD: &str = "arrange.backward";
    pub const ARRANGE_BACK: &str = "arrange.back";
    pub const ARRANGE_FLIP_HORIZONTAL: &str = "arrange.flip_horizontal";
    pub const ARRANGE_FLIP_VERTICAL: &str = "arrange.flip_vertical";
    pub const ARRANGE_MIRROR_ACROSS_LINE: &str = "arrange.mirror_across_line";
    pub const ARRANGE_ROTATE_CW: &str = "arrange.rotate_cw";
    pub const ARRANGE_ROTATE_CCW: &str = "arrange.rotate_ccw";
    pub const ARRANGE_GRID_ARRAY: &str = "arrange.grid_array";
    pub const ARRANGE_CIRCULAR_ARRAY: &str = "arrange.circular_array";
    pub const ARRANGE_MOVE_H_TOGETHER: &str = "arrange.move_h_together";
    pub const ARRANGE_MOVE_V_TOGETHER: &str = "arrange.move_v_together";
    pub const ARRANGE_DOCK_LEFT: &str = "arrange.dock.left";
    pub const ARRANGE_DOCK_RIGHT: &str = "arrange.dock.right";
    pub const ARRANGE_DOCK_UP: &str = "arrange.dock.up";
    pub const ARRANGE_DOCK_DOWN: &str = "arrange.dock.down";
    pub const ARRANGE_NEST_SELECTED: &str = "arrange.nest_selected";
    pub const ARRANGE_MOVE_TO_LASER_POSITION: &str = "arrange.move_selected.to_laser_position";
    pub const ARRANGE_MOVE_TO_PAGE_CENTER: &str = "arrange.move_selected.to_page_center";
    pub const ARRANGE_MOVE_TO_UPPER_LEFT: &str = "arrange.move_selected.to_upper_left";
    pub const ARRANGE_MOVE_TO_UPPER_RIGHT: &str = "arrange.move_selected.to_upper_right";
    pub const ARRANGE_MOVE_TO_LOWER_LEFT: &str = "arrange.move_selected.to_lower_left";
    pub const ARRANGE_MOVE_TO_LOWER_RIGHT: &str = "arrange.move_selected.to_lower_right";
    pub const ARRANGE_MOVE_TO_LEFT: &str = "arrange.move_selected.to_left";
    pub const ARRANGE_MOVE_TO_RIGHT: &str = "arrange.move_selected.to_right";
    pub const ARRANGE_MOVE_TO_TOP: &str = "arrange.move_selected.to_top";
    pub const ARRANGE_MOVE_TO_BOTTOM: &str = "arrange.move_selected.to_bottom";
    pub const ARRANGE_MOVE_LASER_TO_SELECTION_CENTER: &str =
        "arrange.move_laser_to_selection.center";
    pub const ARRANGE_MOVE_LASER_TO_SELECTION_UPPER_LEFT: &str =
        "arrange.move_laser_to_selection.upper_left";
    pub const ARRANGE_MOVE_LASER_TO_SELECTION_UPPER_RIGHT: &str =
        "arrange.move_laser_to_selection.upper_right";
    pub const ARRANGE_MOVE_LASER_TO_SELECTION_LOWER_LEFT: &str =
        "arrange.move_laser_to_selection.lower_left";
    pub const ARRANGE_MOVE_LASER_TO_SELECTION_LOWER_RIGHT: &str =
        "arrange.move_laser_to_selection.lower_right";
    pub const ARRANGE_MOVE_LASER_TO_SELECTION_LEFT: &str = "arrange.move_laser_to_selection.left";
    pub const ARRANGE_MOVE_LASER_TO_SELECTION_RIGHT: &str = "arrange.move_laser_to_selection.right";
    pub const ARRANGE_MOVE_LASER_TO_SELECTION_TOP: &str = "arrange.move_laser_to_selection.top";
    pub const ARRANGE_MOVE_LASER_TO_SELECTION_BOTTOM: &str =
        "arrange.move_laser_to_selection.bottom";
    pub const ARRANGE_JOG_LASER_LEFT: &str = "arrange.jog_laser.left";
    pub const ARRANGE_JOG_LASER_RIGHT: &str = "arrange.jog_laser.right";
    pub const ARRANGE_JOG_LASER_UP: &str = "arrange.jog_laser.up";
    pub const ARRANGE_JOG_LASER_DOWN: &str = "arrange.jog_laser.down";
    pub const ARRANGE_BREAK_APART: &str = "arrange.break_apart";
    pub const ARRANGE_COPY_ALONG_PATH: &str = "arrange.copy_along_path";
    pub const ARRANGE_LOCK: &str = "arrange.lock";
    pub const ARRANGE_UNLOCK: &str = "arrange.unlock";

    pub const LASER_SAVE_MACHINE_FILES: &str = "file.save_machine_files";
    pub const LASER_MATERIAL_TEST: &str = "laser.material_test";
    pub const LASER_FOCUS_TEST: &str = "laser.focus_test";
    pub const LASER_INTERVAL_TEST: &str = "laser.interval_test";

    pub const WINDOW_SIDE_PANELS: &str = "window.side_panels";
    pub const WINDOW_PREVIEW: &str = "window.preview";
    pub const WINDOW_ZOOM_TO_PAGE: &str = "window.zoom_to_page";
    pub const WINDOW_ZOOM_IN: &str = "window.zoom_in";
    pub const WINDOW_ZOOM_OUT: &str = "window.zoom_out";
    pub const WINDOW_FRAME_SELECTION: &str = "window.frame_selection";
    pub const WINDOW_VIEW_STYLE_WIREFRAME_COARSE: &str = "window.view_style.wireframe_coarse";
    pub const WINDOW_VIEW_STYLE_WIREFRAME_SMOOTH: &str = "window.view_style.wireframe_smooth";
    pub const WINDOW_VIEW_STYLE_FILLED_COARSE: &str = "window.view_style.filled_coarse";
    pub const WINDOW_VIEW_STYLE_FILLED_SMOOTH: &str = "window.view_style.filled_smooth";
    pub const WINDOW_TOGGLE_WIREFRAME_FILLED: &str = "window.toggle_wireframe_filled";
    pub const WINDOW_PANEL_ART_LIBRARY: &str = "window.panel.art_library";
    pub const WINDOW_PANEL_CAMERA_CONTROL: &str = "window.panel.camera";
    pub const WINDOW_PANEL_CONSOLE: &str = "window.panel.console";
    pub const WINDOW_PANEL_MACROS: &str = "window.panel.macros";
    pub const WINDOW_PANEL_CUTS_LAYERS: &str = "window.panel.cuts_layers";
    pub const WINDOW_PANEL_COLOR_PALETTE: &str = "window.panel.color_palette";
    pub const WINDOW_PANEL_LASER: &str = "window.panel.laser";
    pub const WINDOW_PANEL_MATERIAL_LIBRARY: &str = "window.panel.material_library";
    pub const WINDOW_PANEL_MOVE: &str = "window.panel.move";
    pub const WINDOW_PANEL_SHAPE_PROPERTIES: &str = "window.panel.shape_properties";
    pub const WINDOW_TOOLBAR_ARRANGE: &str = "window.toolbar.arrange";
    pub const WINDOW_TOOLBAR_ARRANGE_LONG: &str = "window.toolbar.arrange_long";
    pub const WINDOW_TOOLBAR_MODIFIERS: &str = "window.toolbar.modifiers";
    pub const WINDOW_TOOLBAR_DOCKING: &str = "window.toolbar.docking";
    pub const WINDOW_TOOLBAR_MAIN: &str = "window.toolbar.main";
    pub const WINDOW_TOOLBAR_NUMERIC_EDITS: &str = "window.toolbar.numeric_edits";
    pub const WINDOW_TOOLBAR_TEXT_OPTIONS: &str = "window.toolbar.text_options";
    pub const WINDOW_TOOLBAR_TOOLS: &str = "window.toolbar.tools";
    pub const WINDOW_RESET_LAYOUT: &str = "window.reset_layout";

    pub const LANGUAGE_EN: &str = "language.en";
    pub const LANGUAGE_DE: &str = "language.de";
    pub const LANGUAGE_ES_ES: &str = "language.es-ES";
    pub const LANGUAGE_ES_419: &str = "language.es-419";
    pub const LANGUAGE_FR: &str = "language.fr";
    pub const LANGUAGE_IT: &str = "language.it";
    pub const LANGUAGE_PT_BR: &str = "language.pt-BR";
    pub const LANGUAGE_NL: &str = "language.nl";
    pub const LANGUAGE_PL: &str = "language.pl";
    pub const LANGUAGE_CS: &str = "language.cs";
    pub const LANGUAGE_SV: &str = "language.sv";
    pub const LANGUAGE_NB: &str = "language.nb";
    pub const LANGUAGE_DA: &str = "language.da";
    pub const LANGUAGE_FI: &str = "language.fi";
    pub const LANGUAGE_HU: &str = "language.hu";
    pub const LANGUAGE_TR: &str = "language.tr";
    pub const LANGUAGE_EL: &str = "language.el";
    pub const LANGUAGE_RU: &str = "language.ru";
    pub const LANGUAGE_SL: &str = "language.sl";
    pub const LANGUAGE_JA: &str = "language.ja";
    pub const LANGUAGE_KO: &str = "language.ko";
    pub const LANGUAGE_ZH_CN: &str = "language.zh-CN";
    pub const LANGUAGE_ZH_TW: &str = "language.zh-TW";

    pub const HELP_QUICK_HELP: &str = "help.quick_help";
    pub const HELP_REPORT_BUG: &str = "help.report_bug";
    pub const HELP_ABOUT: &str = "help.about";
}

const RECENT_MENU_ID_PREFIX: &str = "file.recent.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeMenuEntry {
    Command {
        id: &'static str,
        title: &'static str,
        accelerator: Option<&'static str>,
        enabled: bool,
    },
    Check {
        id: &'static str,
        title: &'static str,
        accelerator: Option<&'static str>,
        enabled: bool,
        checked: bool,
    },
    Submenu {
        title: &'static str,
        entries: &'static [NativeMenuEntry],
    },
    Label {
        title: &'static str,
    },
    RecentProjects,
    Separator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeMenuSection {
    pub title: &'static str,
    pub entries: &'static [NativeMenuEntry],
}

const APP_MENU: &[NativeMenuEntry] = &[
    NativeMenuEntry::Command {
        id: command::APP_ABOUT,
        title: "About Beam Bench",
        accelerator: None,
        enabled: true,
    },
    NativeMenuEntry::Command {
        id: command::APP_PREFERENCES,
        title: "Preferences...",
        accelerator: Some("CmdOrCtrl+,"),
        enabled: true,
    },
    NativeMenuEntry::Separator,
    NativeMenuEntry::Command {
        id: command::APP_QUIT,
        title: "Quit Beam Bench",
        accelerator: Some("CmdOrCtrl+Q"),
        enabled: true,
    },
];

const FILE_PREFERENCES_MENU: &[NativeMenuEntry] = &[
    NativeMenuEntry::Command {
        id: command::FILE_PREFS_IMPORT,
        title: "Import Prefs",
        accelerator: None,
        enabled: true,
    },
    NativeMenuEntry::Command {
        id: command::FILE_PREFS_EXPORT,
        title: "Export Prefs",
        accelerator: None,
        enabled: true,
    },
    NativeMenuEntry::Command {
        id: command::FILE_PREFS_OPEN_FOLDER,
        title: "Open Prefs Folder",
        accelerator: None,
        enabled: true,
    },
    NativeMenuEntry::Command {
        id: command::FILE_PREFS_RESET_DEFAULTS,
        title: "Reset Prefs to Defaults",
        accelerator: None,
        enabled: true,
    },
];

const FILE_MENU: &[NativeMenuEntry] = &[
    NativeMenuEntry::Command {
        id: command::FILE_NEW,
        title: "New",
        accelerator: Some("CmdOrCtrl+N"),
        enabled: true,
    },
    NativeMenuEntry::Command {
        id: command::FILE_NEW_WINDOW,
        title: "New Window",
        accelerator: None,
        enabled: true,
    },
    NativeMenuEntry::RecentProjects,
    NativeMenuEntry::Command {
        id: command::FILE_OPEN,
        title: "Open",
        accelerator: Some("CmdOrCtrl+O"),
        enabled: true,
    },
    NativeMenuEntry::Command {
        id: command::FILE_IMPORT,
        title: "Import",
        accelerator: Some("CmdOrCtrl+I"),
        enabled: true,
    },
    NativeMenuEntry::Command {
        id: command::FILE_NOTES,
        title: "Show Notes",
        accelerator: Some("CmdOrCtrl+Alt+N"),
        enabled: false,
    },
    NativeMenuEntry::Separator,
    NativeMenuEntry::Command {
        id: command::FILE_SAVE,
        title: "Save",
        accelerator: Some("CmdOrCtrl+S"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::FILE_SAVE_AS,
        title: "Save As",
        accelerator: Some("CmdOrCtrl+Shift+S"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::FILE_EXPORT,
        title: "Export",
        accelerator: Some("Alt+X"),
        enabled: false,
    },
    NativeMenuEntry::Separator,
    NativeMenuEntry::Submenu {
        title: "Preferences",
        entries: FILE_PREFERENCES_MENU,
    },
    NativeMenuEntry::Separator,
    NativeMenuEntry::Command {
        id: command::FILE_PRINT_BLACK,
        title: "Print (black only)",
        accelerator: Some("CmdOrCtrl+P"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::FILE_PRINT_COLORS,
        title: "Print (keep colors)",
        accelerator: Some("CmdOrCtrl+Shift+P"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::FILE_SAVE_PROCESSED_BITMAP,
        title: "Save Processed Bitmap",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::FILE_SAVE_BACKGROUND_CAPTURE,
        title: "Save Background Capture",
        accelerator: None,
        enabled: false,
    },
];

const EDIT_IMAGE_OPTIONS_MENU: &[NativeMenuEntry] = &[
    NativeMenuEntry::Command {
        id: command::EDIT_IMAGE_REFRESH,
        title: "Refresh Image",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::EDIT_IMAGE_REPLACE,
        title: "Replace Image",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::EDIT_IMAGE_REPLACE_TO_FIT,
        title: "Replace Image to Fit",
        accelerator: None,
        enabled: false,
    },
];

const EDIT_MENU: &[NativeMenuEntry] = &[
    NativeMenuEntry::Command {
        id: command::EDIT_UNDO,
        title: "Undo",
        accelerator: Some("CmdOrCtrl+Z"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::EDIT_REDO,
        title: "Redo",
        accelerator: Some("CmdOrCtrl+Shift+Z"),
        enabled: false,
    },
    NativeMenuEntry::Separator,
    NativeMenuEntry::Command {
        id: command::EDIT_SELECT_ALL,
        title: "Select All",
        accelerator: Some("CmdOrCtrl+A"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::EDIT_INVERT_SELECTION,
        title: "Invert Selection",
        accelerator: Some("CmdOrCtrl+Shift+I"),
        enabled: false,
    },
    NativeMenuEntry::Separator,
    NativeMenuEntry::Command {
        id: command::EDIT_CUT,
        title: "Cut",
        accelerator: Some("CmdOrCtrl+X"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::EDIT_COPY,
        title: "Copy",
        accelerator: Some("CmdOrCtrl+C"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::EDIT_DUPLICATE,
        title: "Duplicate",
        accelerator: Some("CmdOrCtrl+D"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::EDIT_PASTE,
        title: "Paste",
        accelerator: Some("CmdOrCtrl+V"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::EDIT_PASTE_IN_PLACE,
        title: "Paste in Place",
        accelerator: Some("Alt+V"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::EDIT_DELETE,
        title: "Delete",
        accelerator: Some("Backspace"),
        enabled: false,
    },
    NativeMenuEntry::Separator,
    NativeMenuEntry::Command {
        id: command::EDIT_CONVERT_TO_PATH,
        title: "Convert to Path",
        accelerator: Some("CmdOrCtrl+Shift+C"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::EDIT_CONVERT_TO_BITMAP,
        title: "Convert to Bitmap",
        accelerator: Some("CmdOrCtrl+Shift+B"),
        enabled: false,
    },
    NativeMenuEntry::Separator,
    NativeMenuEntry::Command {
        id: command::EDIT_CLOSE_PATH,
        title: "Close Path",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::EDIT_CLOSE_SELECTED_PATHS_WITH_TOLERANCE,
        title: "Close Selected Paths With Tolerance",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::EDIT_AUTO_JOIN_SELECTED_SHAPES,
        title: "Auto-Join Selected Shapes",
        accelerator: Some("Alt+J"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::EDIT_CLOSE_AND_JOIN,
        title: "Close & Join",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::EDIT_OPTIMIZE_SELECTED_SHAPES,
        title: "Optimize Selected Shapes",
        accelerator: Some("Alt+Shift+O"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::EDIT_DELETE_DUPLICATES,
        title: "Delete Duplicates",
        accelerator: Some("Alt+D"),
        enabled: false,
    },
    NativeMenuEntry::Separator,
    NativeMenuEntry::Command {
        id: command::EDIT_SELECT_OPEN_SHAPES,
        title: "Select Open Shapes",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::EDIT_SELECT_OPEN_SHAPES_SET_TO_FILL,
        title: "Select Open Shapes Set to Fill",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::EDIT_SELECT_ALL_SHAPES_IN_CURRENT_LAYER,
        title: "Select All Shapes in Current Layer",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::EDIT_SELECT_CONTAINED_SHAPES,
        title: "Select Contained Shapes",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::EDIT_SELECT_SHAPES_SMALLER_THAN_SELECTED,
        title: "Select Shapes Smaller Than Selected",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Separator,
    NativeMenuEntry::Submenu {
        title: "Image Options",
        entries: EDIT_IMAGE_OPTIONS_MENU,
    },
    NativeMenuEntry::Separator,
    NativeMenuEntry::Command {
        id: command::EDIT_SETTINGS,
        title: "Settings",
        accelerator: None,
        enabled: true,
    },
];

const TOOLS_DRAW_SHAPE_MENU: &[NativeMenuEntry] = &[
    NativeMenuEntry::Command {
        id: command::TOOLS_RECTANGLE,
        title: "Rectangle",
        accelerator: Some("CmdOrCtrl+R"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_ELLIPSE,
        title: "Ellipse",
        accelerator: Some("CmdOrCtrl+E"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_TRIANGLE,
        title: "Triangle",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_PENTAGON,
        title: "Pentagon",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_POLYGON,
        title: "Polygon",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_OCTAGON,
        title: "Octagon",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_STAR,
        title: "Star",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_DUAL_STAR,
        title: "Dual Star",
        accelerator: None,
        enabled: false,
    },
];

const TOOLS_MENU: &[NativeMenuEntry] = &[
    NativeMenuEntry::Command {
        id: command::TOOLS_SELECT,
        title: "Select",
        accelerator: Some("Esc"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_LINE,
        title: "Draw Lines",
        accelerator: Some("CmdOrCtrl+L"),
        enabled: false,
    },
    NativeMenuEntry::Submenu {
        title: "Draw Shape",
        entries: TOOLS_DRAW_SHAPE_MENU,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_NODE,
        title: "Edit Nodes",
        accelerator: Some("CmdOrCtrl+`"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_TRIM,
        title: "Trim Shapes",
        accelerator: Some("CmdOrCtrl+K"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_TABS,
        title: "Add Tabs",
        accelerator: Some("CmdOrCtrl+Tab"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_TEXT,
        title: "Edit Text",
        accelerator: Some("CmdOrCtrl+T"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_POSITION_LASER,
        title: "Position Laser",
        accelerator: Some("CmdOrCtrl+Shift+L"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_MEASURE,
        title: "Measure",
        accelerator: Some("CmdOrCtrl+M"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_BARCODE,
        title: "Create Bar Code",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_OFFSET,
        title: "Offset Shapes",
        accelerator: Some("Alt+O"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_BOOLEAN_WELD,
        title: "Weld Shapes",
        accelerator: Some("CmdOrCtrl+W"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_BOOLEAN_UNION,
        title: "Boolean Union",
        accelerator: Some("Alt++"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_BOOLEAN_SUBTRACT,
        title: "Boolean Subtract",
        accelerator: Some("Alt+-"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_BOOLEAN_INTERSECTION,
        title: "Boolean Intersection",
        accelerator: Some("Alt+*"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_BOOLEAN_ASSISTANT,
        title: "Boolean Assistant",
        accelerator: Some("CmdOrCtrl+B"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_CUT_SHAPES,
        title: "Cut Shapes",
        accelerator: Some("Alt+Shift+C"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_ADJUST_IMAGE,
        title: "Adjust Image",
        accelerator: Some("Alt+I"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_TRACE_IMAGE,
        title: "Trace Image",
        accelerator: Some("Alt+T"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_APPLY_PATH_TO_TEXT,
        title: "Apply Path to Text",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_APPLY_MASK_TO_IMAGE,
        title: "Apply Mask to Image",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_CROP_IMAGE,
        title: "Crop Image",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_WARP_SELECTION,
        title: "Warp Selection (4 Points)",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::TOOLS_DEFORM_SELECTION,
        title: "Deform Selection (16 Points)",
        accelerator: None,
        enabled: false,
    },
];

const ARRANGE_ALIGN_MENU: &[NativeMenuEntry] = &[
    NativeMenuEntry::Command {
        id: command::ARRANGE_ALIGN_CENTERS,
        title: "Align Centers",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_ALIGN_CENTER_VERTICAL,
        title: "Align Vertical Centers",
        accelerator: Some("Alt+PageUp"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_ALIGN_CENTER_HORIZONTAL,
        title: "Align Horizontal Centers",
        accelerator: Some("Alt+PageDown"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_ALIGN_LEFT,
        title: "Align Left",
        accelerator: Some("Alt+Left"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_ALIGN_RIGHT,
        title: "Align Right",
        accelerator: Some("Alt+Right"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_ALIGN_BOTTOM,
        title: "Align Bottom",
        accelerator: Some("Alt+Down"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_ALIGN_TOP,
        title: "Align Top",
        accelerator: Some("Alt+Up"),
        enabled: false,
    },
];

const ARRANGE_DISTRIBUTE_MENU: &[NativeMenuEntry] = &[
    NativeMenuEntry::Command {
        id: command::ARRANGE_DISTRIBUTE_V_SPACED,
        title: "Distribute V-Spaced",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_DISTRIBUTE_V_CENTERED,
        title: "Distribute V-Centered",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_DISTRIBUTE_H_SPACED,
        title: "Distribute H-Spaced",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_DISTRIBUTE_H_CENTERED,
        title: "Distribute H-Centered",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_MOVE_H_TOGETHER,
        title: "Move H Together",
        accelerator: Some("Alt+Shift+H"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_MOVE_V_TOGETHER,
        title: "Move V Together",
        accelerator: Some("Alt+Shift+V"),
        enabled: false,
    },
];

const ARRANGE_UNGROUP_MENU: &[NativeMenuEntry] = &[
    NativeMenuEntry::Command {
        id: command::ARRANGE_UNGROUP,
        title: "Ungroup",
        accelerator: Some("CmdOrCtrl+U"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_AUTO_GROUP,
        title: "Auto-Group",
        accelerator: None,
        enabled: false,
    },
];

const ARRANGE_FLIP_MENU: &[NativeMenuEntry] = &[
    NativeMenuEntry::Command {
        id: command::ARRANGE_FLIP_HORIZONTAL,
        title: "Flip Horizontal",
        accelerator: Some("CmdOrCtrl+Shift+H"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_FLIP_VERTICAL,
        title: "Flip Vertical",
        accelerator: Some("CmdOrCtrl+Shift+V"),
        enabled: false,
    },
];

const ARRANGE_ROTATE_MENU: &[NativeMenuEntry] = &[
    NativeMenuEntry::Command {
        id: command::ARRANGE_ROTATE_CW,
        title: "Rotate 90° Clockwise",
        accelerator: Some("Period"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_ROTATE_CCW,
        title: "Rotate 90° Counter-Clockwise",
        accelerator: Some("Comma"),
        enabled: false,
    },
];

const ARRANGE_DOCK_MENU: &[NativeMenuEntry] = &[
    NativeMenuEntry::Command {
        id: command::ARRANGE_DOCK_LEFT,
        title: "Dock Left",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_DOCK_RIGHT,
        title: "Dock Right",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_DOCK_UP,
        title: "Dock Up",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_DOCK_DOWN,
        title: "Dock Down",
        accelerator: None,
        enabled: false,
    },
];

const ARRANGE_MOVE_SELECTED_MENU: &[NativeMenuEntry] = &[
    NativeMenuEntry::Command {
        id: command::ARRANGE_MOVE_TO_LASER_POSITION,
        title: "Move to Laser Position",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_MOVE_TO_PAGE_CENTER,
        title: "Move to Page Center",
        accelerator: Some("P"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_MOVE_TO_UPPER_LEFT,
        title: "Move to Upper Left",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_MOVE_TO_UPPER_RIGHT,
        title: "Move to Upper Right",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_MOVE_TO_LOWER_LEFT,
        title: "Move to Lower Left",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_MOVE_TO_LOWER_RIGHT,
        title: "Move to Lower Right",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_MOVE_TO_LEFT,
        title: "Move to Left",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_MOVE_TO_RIGHT,
        title: "Move to Right",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_MOVE_TO_TOP,
        title: "Move to Top",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_MOVE_TO_BOTTOM,
        title: "Move to Bottom",
        accelerator: None,
        enabled: false,
    },
];

const ARRANGE_JOG_LASER_MENU: &[NativeMenuEntry] = &[
    NativeMenuEntry::Command {
        id: command::ARRANGE_JOG_LASER_LEFT,
        title: "Jog Laser Left",
        accelerator: Some("Alt+CmdOrCtrl+["),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_JOG_LASER_RIGHT,
        title: "Jog Laser Right",
        accelerator: Some("Alt+CmdOrCtrl+]"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_JOG_LASER_UP,
        title: "Jog Laser Up",
        accelerator: Some("CmdOrCtrl+Shift+]"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_JOG_LASER_DOWN,
        title: "Jog Laser Down",
        accelerator: Some("CmdOrCtrl+Shift+["),
        enabled: false,
    },
];

const ARRANGE_MOVE_LASER_MENU: &[NativeMenuEntry] = &[
    NativeMenuEntry::Command {
        id: command::ARRANGE_MOVE_LASER_TO_SELECTION_CENTER,
        title: "Move Laser to Selection Center",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_MOVE_LASER_TO_SELECTION_UPPER_LEFT,
        title: "Move Laser to Upper Left of Selection",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_MOVE_LASER_TO_SELECTION_UPPER_RIGHT,
        title: "Move Laser to Upper Right of Selection",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_MOVE_LASER_TO_SELECTION_LOWER_LEFT,
        title: "Move Laser to Lower Left of Selection",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_MOVE_LASER_TO_SELECTION_LOWER_RIGHT,
        title: "Move Laser to Lower Right of Selection",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_MOVE_LASER_TO_SELECTION_LEFT,
        title: "Move Laser to Left of Selection",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_MOVE_LASER_TO_SELECTION_RIGHT,
        title: "Move Laser to Right of Selection",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_MOVE_LASER_TO_SELECTION_TOP,
        title: "Move Laser to Top of Selection",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_MOVE_LASER_TO_SELECTION_BOTTOM,
        title: "Move Laser to Bottom of Selection",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Submenu {
        title: "Jog Laser",
        entries: ARRANGE_JOG_LASER_MENU,
    },
];

const ARRANGE_DRAW_ORDER_MENU: &[NativeMenuEntry] = &[
    NativeMenuEntry::Command {
        id: command::ARRANGE_FORWARD,
        title: "Bring Forward",
        accelerator: Some("PageUp"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_BACKWARD,
        title: "Send Backward",
        accelerator: Some("PageDown"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_FRONT,
        title: "Bring to Front",
        accelerator: Some("CmdOrCtrl+PageUp"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_BACK,
        title: "Send to Back",
        accelerator: Some("CmdOrCtrl+PageDown"),
        enabled: false,
    },
];

const ARRANGE_MENU: &[NativeMenuEntry] = &[
    NativeMenuEntry::Command {
        id: command::ARRANGE_GROUP,
        title: "Group",
        accelerator: Some("CmdOrCtrl+G"),
        enabled: false,
    },
    NativeMenuEntry::Submenu {
        title: "Ungroup",
        entries: ARRANGE_UNGROUP_MENU,
    },
    NativeMenuEntry::Separator,
    NativeMenuEntry::Submenu {
        title: "Flip Horizontal / Vertical",
        entries: ARRANGE_FLIP_MENU,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_MIRROR_ACROSS_LINE,
        title: "Mirror Across Line",
        accelerator: Some("CmdOrCtrl+Shift+M"),
        enabled: false,
    },
    NativeMenuEntry::Submenu {
        title: "Rotate 90° Clockwise / Counter-Clockwise",
        entries: ARRANGE_ROTATE_MENU,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_TWO_POINT_ROTATE_SCALE,
        title: "Two-Point Rotate / Scale",
        accelerator: Some("CmdOrCtrl+2"),
        enabled: false,
    },
    NativeMenuEntry::Separator,
    NativeMenuEntry::Submenu {
        title: "Align",
        entries: ARRANGE_ALIGN_MENU,
    },
    NativeMenuEntry::Submenu {
        title: "Distribute",
        entries: ARRANGE_DISTRIBUTE_MENU,
    },
    NativeMenuEntry::Separator,
    NativeMenuEntry::Command {
        id: command::ARRANGE_NEST_SELECTED,
        title: "Nest Selected",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Submenu {
        title: "Dock",
        entries: ARRANGE_DOCK_MENU,
    },
    NativeMenuEntry::Submenu {
        title: "Move Selected Objects",
        entries: ARRANGE_MOVE_SELECTED_MENU,
    },
    NativeMenuEntry::Submenu {
        title: "Move Laser to Selection",
        entries: ARRANGE_MOVE_LASER_MENU,
    },
    NativeMenuEntry::Separator,
    NativeMenuEntry::Command {
        id: command::ARRANGE_GRID_ARRAY,
        title: "Grid Array",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_CIRCULAR_ARRAY,
        title: "Circular Array",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_COPY_ALONG_PATH,
        title: "Copy Along Path",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_BREAK_APART,
        title: "Break Apart",
        accelerator: Some("Alt+B"),
        enabled: false,
    },
    NativeMenuEntry::Separator,
    NativeMenuEntry::Submenu {
        title: "Push in Draw Order",
        entries: ARRANGE_DRAW_ORDER_MENU,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_LOCK,
        title: "Lock Selected Shapes",
        accelerator: None,
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::ARRANGE_UNLOCK,
        title: "Unlock Selected Shapes",
        accelerator: None,
        enabled: false,
    },
];

const LASER_TOOLS_MENU: &[NativeMenuEntry] = &[
    NativeMenuEntry::Command {
        id: command::LASER_SAVE_MACHINE_FILES,
        title: "Save Machine Files",
        accelerator: Some("Alt+Shift+L"),
        enabled: false,
    },
    NativeMenuEntry::Separator,
    NativeMenuEntry::Command {
        id: command::LASER_MATERIAL_TEST,
        title: "Material Test...",
        accelerator: None,
        enabled: true,
    },
    NativeMenuEntry::Command {
        id: command::LASER_FOCUS_TEST,
        title: "Focus Test...",
        accelerator: None,
        enabled: true,
    },
    NativeMenuEntry::Command {
        id: command::LASER_INTERVAL_TEST,
        title: "Interval Test...",
        accelerator: None,
        enabled: true,
    },
];

const WINDOW_MENU: &[NativeMenuEntry] = &[
    NativeMenuEntry::Command {
        id: command::WINDOW_RESET_LAYOUT,
        title: "Reset to Default Layout",
        accelerator: None,
        enabled: true,
    },
    NativeMenuEntry::Separator,
    NativeMenuEntry::Check {
        id: command::WINDOW_PREVIEW,
        title: "Preview",
        accelerator: Some("Alt+P"),
        enabled: false,
        checked: false,
    },
    NativeMenuEntry::Command {
        id: command::WINDOW_ZOOM_TO_PAGE,
        title: "Zoom to Page",
        accelerator: Some("CmdOrCtrl+0"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::WINDOW_ZOOM_IN,
        title: "Zoom In",
        accelerator: Some("CmdOrCtrl+="),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::WINDOW_ZOOM_OUT,
        title: "Zoom Out",
        accelerator: Some("CmdOrCtrl+-"),
        enabled: false,
    },
    NativeMenuEntry::Command {
        id: command::WINDOW_FRAME_SELECTION,
        title: "Frame Selection",
        accelerator: Some("CmdOrCtrl+Shift+A"),
        enabled: false,
    },
    NativeMenuEntry::Separator,
    NativeMenuEntry::Label {
        title: "View Style:",
    },
    NativeMenuEntry::Check {
        id: command::WINDOW_VIEW_STYLE_WIREFRAME_COARSE,
        title: "Wireframe / Coarse",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::WINDOW_VIEW_STYLE_WIREFRAME_SMOOTH,
        title: "Wireframe / Smooth",
        accelerator: None,
        enabled: true,
        checked: true,
    },
    NativeMenuEntry::Check {
        id: command::WINDOW_VIEW_STYLE_FILLED_COARSE,
        title: "Filled / Coarse",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::WINDOW_VIEW_STYLE_FILLED_SMOOTH,
        title: "Filled / Smooth",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Command {
        id: command::WINDOW_TOGGLE_WIREFRAME_FILLED,
        title: "Toggle Wireframe / Filled",
        accelerator: Some("Alt+Shift+W"),
        enabled: true,
    },
    NativeMenuEntry::Separator,
    NativeMenuEntry::Check {
        id: command::WINDOW_SIDE_PANELS,
        title: "Toggle Side Panels",
        accelerator: Some("F12"),
        enabled: true,
        checked: true,
    },
    NativeMenuEntry::Separator,
    NativeMenuEntry::Check {
        id: command::WINDOW_PANEL_ART_LIBRARY,
        title: "Art Library",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::WINDOW_TOOLBAR_ARRANGE,
        title: "Arrange",
        accelerator: None,
        enabled: true,
        checked: true,
    },
    NativeMenuEntry::Check {
        id: command::WINDOW_TOOLBAR_ARRANGE_LONG,
        title: "Arrange (Long)",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::WINDOW_TOOLBAR_MODIFIERS,
        title: "Modifiers",
        accelerator: None,
        enabled: true,
        checked: true,
    },
    NativeMenuEntry::Check {
        id: command::WINDOW_PANEL_CAMERA_CONTROL,
        title: "Camera Control",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::WINDOW_PANEL_CONSOLE,
        title: "Console",
        accelerator: None,
        enabled: true,
        checked: true,
    },
    NativeMenuEntry::Check {
        id: command::WINDOW_PANEL_MACROS,
        title: "Macros",
        accelerator: None,
        enabled: true,
        checked: true,
    },
    NativeMenuEntry::Check {
        id: command::WINDOW_PANEL_CUTS_LAYERS,
        title: "Cuts / Layers",
        accelerator: None,
        enabled: true,
        checked: true,
    },
    NativeMenuEntry::Check {
        id: command::WINDOW_PANEL_COLOR_PALETTE,
        title: "Color Palette",
        accelerator: None,
        enabled: true,
        checked: true,
    },
    NativeMenuEntry::Check {
        id: command::WINDOW_TOOLBAR_DOCKING,
        title: "Docking",
        accelerator: None,
        enabled: true,
        checked: true,
    },
    NativeMenuEntry::Check {
        id: command::WINDOW_PANEL_LASER,
        title: "Laser",
        accelerator: None,
        enabled: true,
        checked: true,
    },
    NativeMenuEntry::Check {
        id: command::WINDOW_PANEL_MATERIAL_LIBRARY,
        title: "Material Library",
        accelerator: None,
        enabled: true,
        checked: true,
    },
    NativeMenuEntry::Check {
        id: command::WINDOW_TOOLBAR_MAIN,
        title: "Main",
        accelerator: None,
        enabled: true,
        checked: true,
    },
    NativeMenuEntry::Check {
        id: command::WINDOW_PANEL_MOVE,
        title: "Move",
        accelerator: None,
        enabled: true,
        checked: true,
    },
    NativeMenuEntry::Check {
        id: command::WINDOW_TOOLBAR_NUMERIC_EDITS,
        title: "Numeric Edits",
        accelerator: None,
        enabled: true,
        checked: true,
    },
    NativeMenuEntry::Check {
        id: command::WINDOW_PANEL_SHAPE_PROPERTIES,
        title: "Shape Properties",
        accelerator: None,
        enabled: true,
        checked: true,
    },
    NativeMenuEntry::Check {
        id: command::WINDOW_TOOLBAR_TEXT_OPTIONS,
        title: "Text Options",
        accelerator: None,
        enabled: true,
        checked: true,
    },
    NativeMenuEntry::Check {
        id: command::WINDOW_TOOLBAR_TOOLS,
        title: "Tools",
        accelerator: None,
        enabled: true,
        checked: true,
    },
];

// All 23 supported locales. Items are Check-style; the active locale
// shows a checkmark. Titles are endonym + English name to match
// Initial `checked: false` for non-English entries
// — the React layer flips check state via `update_native_menu_state`
// when display_language hydrates / changes.
const LANGUAGE_MENU: &[NativeMenuEntry] = &[
    NativeMenuEntry::Check {
        id: command::LANGUAGE_EN,
        title: "English",
        accelerator: None,
        enabled: true,
        checked: true,
    },
    NativeMenuEntry::Check {
        id: command::LANGUAGE_DE,
        title: "Deutsch (German)",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::LANGUAGE_ES_ES,
        title: "Español (Spanish)",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::LANGUAGE_ES_419,
        title: "Español, Latinoamérica (Spanish, Latin America)",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::LANGUAGE_FR,
        title: "Français (French)",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::LANGUAGE_IT,
        title: "Italiano (Italian)",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::LANGUAGE_PT_BR,
        title: "Português, Brasil (Portuguese, Brazil)",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::LANGUAGE_NL,
        title: "Nederlands (Dutch)",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::LANGUAGE_PL,
        title: "Polski (Polish)",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::LANGUAGE_CS,
        title: "Čeština (Czech)",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::LANGUAGE_SV,
        title: "Svenska (Swedish)",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::LANGUAGE_NB,
        title: "Norsk bokmål (Norwegian Bokmål)",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::LANGUAGE_DA,
        title: "Dansk (Danish)",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::LANGUAGE_FI,
        title: "Suomi (Finnish)",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::LANGUAGE_HU,
        title: "Magyar (Hungarian)",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::LANGUAGE_TR,
        title: "Türkçe (Turkish)",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::LANGUAGE_EL,
        title: "Ελληνικά (Greek)",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::LANGUAGE_RU,
        title: "Русский (Russian)",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::LANGUAGE_SL,
        title: "Slovenščina (Slovenian)",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::LANGUAGE_JA,
        title: "日本語 (Japanese)",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::LANGUAGE_KO,
        title: "한국어 (Korean)",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::LANGUAGE_ZH_CN,
        title: "简体中文 (Simplified Chinese)",
        accelerator: None,
        enabled: true,
        checked: false,
    },
    NativeMenuEntry::Check {
        id: command::LANGUAGE_ZH_TW,
        title: "繁體中文 (Traditional Chinese)",
        accelerator: None,
        enabled: true,
        checked: false,
    },
];

const HELP_MENU: &[NativeMenuEntry] = &[
    NativeMenuEntry::Command {
        id: command::HELP_QUICK_HELP,
        title: "Quick Help",
        accelerator: Some("F1"),
        enabled: true,
    },
    NativeMenuEntry::Command {
        id: command::HELP_REPORT_BUG,
        title: "Report a Bug...",
        accelerator: None,
        enabled: true,
    },
    NativeMenuEntry::Command {
        id: command::HELP_ABOUT,
        title: "About Beam Bench",
        accelerator: None,
        enabled: true,
    },
];

const MENU_SPEC: &[NativeMenuSection] = &[
    NativeMenuSection {
        title: "Beam Bench",
        entries: APP_MENU,
    },
    NativeMenuSection {
        title: "File",
        entries: FILE_MENU,
    },
    NativeMenuSection {
        title: "Edit",
        entries: EDIT_MENU,
    },
    NativeMenuSection {
        title: "Tools",
        entries: TOOLS_MENU,
    },
    NativeMenuSection {
        title: "Arrange",
        entries: ARRANGE_MENU,
    },
    NativeMenuSection {
        title: "Laser Tools",
        entries: LASER_TOOLS_MENU,
    },
    NativeMenuSection {
        title: "Window",
        entries: WINDOW_MENU,
    },
    NativeMenuSection {
        title: "Language",
        entries: LANGUAGE_MENU,
    },
    NativeMenuSection {
        title: "Help",
        entries: HELP_MENU,
    },
];

pub fn menu_spec() -> &'static [NativeMenuSection] {
    MENU_SPEC
}

/// Labels passed from the React layer at language-change time. Keys are
/// the English titles that appear in `menu_spec()` (e.g. `"File"`,
/// `"Edit"`, `"Recent Projects"`); values are the translated strings.
/// Any title not in the map falls back to its English original, so
/// `NativeMenuLabels::default()` reproduces the pre-i18n menu.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct NativeMenuLabels {
    #[serde(default, alias = "byTitle", alias = "by_title")]
    pub by_title: HashMap<String, String>,
}

impl NativeMenuLabels {
    fn resolve(&self, english: &str) -> String {
        self.by_title
            .get(english)
            .cloned()
            .unwrap_or_else(|| english.to_string())
    }
}

#[derive(Default)]
pub struct NativeMenuRegistry {
    normal_items: Mutex<HashMap<String, Vec<MenuItem<tauri::Wry>>>>,
    check_items: Mutex<HashMap<String, Vec<CheckMenuItem<tauri::Wry>>>>,
    default_accelerators: Mutex<HashMap<String, Option<String>>>,
    recent_submenu: Mutex<Option<Submenu<tauri::Wry>>>,
    recent_items: Mutex<Vec<MenuItem<tauri::Wry>>>,
    recent_paths: Mutex<HashMap<String, String>>,
}

#[derive(Default)]
struct NativeMenuHandles {
    normal_items: HashMap<String, Vec<MenuItem<tauri::Wry>>>,
    check_items: HashMap<String, Vec<CheckMenuItem<tauri::Wry>>>,
    default_accelerators: HashMap<String, Option<String>>,
    recent_submenu: Option<Submenu<tauri::Wry>>,
    recent_items: Vec<MenuItem<tauri::Wry>>,
    recent_paths: HashMap<String, String>,
}

impl NativeMenuRegistry {
    fn replace(&self, handles: NativeMenuHandles) -> Result<(), String> {
        *self
            .normal_items
            .lock()
            .map_err(|e| format!("Failed to lock native menu item registry: {e}"))? =
            handles.normal_items;
        *self
            .check_items
            .lock()
            .map_err(|e| format!("Failed to lock native check menu item registry: {e}"))? =
            handles.check_items;
        *self
            .default_accelerators
            .lock()
            .map_err(|e| format!("Failed to lock native accelerator registry: {e}"))? =
            handles.default_accelerators;
        *self
            .recent_submenu
            .lock()
            .map_err(|e| format!("Failed to lock recent projects submenu registry: {e}"))? =
            handles.recent_submenu;
        *self
            .recent_items
            .lock()
            .map_err(|e| format!("Failed to lock recent projects item registry: {e}"))? =
            handles.recent_items;
        *self
            .recent_paths
            .lock()
            .map_err(|e| format!("Failed to lock recent projects path registry: {e}"))? =
            handles.recent_paths;
        Ok(())
    }

    fn recent_path_for_id(&self, id: &str) -> Option<String> {
        self.recent_paths.lock().ok()?.get(id).cloned()
    }

    fn update_recent_files(
        &self,
        app: &AppHandle<tauri::Wry>,
        recent_files: &[NativeRecentFileState],
    ) -> Result<(), String> {
        let submenu = self
            .recent_submenu
            .lock()
            .map_err(|e| format!("Failed to lock recent projects submenu registry: {e}"))?
            .clone();
        let Some(submenu) = submenu else {
            return Ok(());
        };

        let mut recent_items = self
            .recent_items
            .lock()
            .map_err(|e| format!("Failed to lock recent projects item registry: {e}"))?;
        while !recent_items.is_empty() {
            submenu.remove_at(0).map_err(|e| e.to_string())?;
            recent_items.remove(0);
        }

        let mut recent_paths = self
            .recent_paths
            .lock()
            .map_err(|e| format!("Failed to lock recent projects path registry: {e}"))?;
        recent_paths.clear();

        if recent_files.is_empty() {
            let item = MenuItem::with_id(
                app,
                command::FILE_RECENT_EMPTY,
                "No Recent Projects",
                false,
                None::<&str>,
            )
            .map_err(|e| e.to_string())?;
            submenu.append(&item).map_err(|e| e.to_string())?;
            recent_items.push(item);
            return Ok(());
        }

        for (idx, file) in recent_files.iter().enumerate() {
            let id = format!("{RECENT_MENU_ID_PREFIX}{idx}");
            let label = if file.name.trim().is_empty() {
                file.path.as_str()
            } else {
                file.name.as_str()
            };
            let item = MenuItem::with_id(app, id.clone(), label, true, None::<&str>)
                .map_err(|e| e.to_string())?;
            submenu.append(&item).map_err(|e| e.to_string())?;
            recent_paths.insert(id, file.path.clone());
            recent_items.push(item);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct NativeMenuStateUpdate {
    pub items: Vec<NativeMenuItemState>,
    #[serde(default, rename = "recentFiles")]
    pub recent_files: Option<Vec<NativeRecentFileState>>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct NativeRecentFileState {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct NativeMenuItemState {
    pub id: String,
    pub enabled: Option<bool>,
    pub checked: Option<bool>,
    pub title: Option<String>,
    #[serde(default, deserialize_with = "deserialize_accelerator_update")]
    pub accelerator: Option<NativeMenuAcceleratorUpdate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeMenuAcceleratorUpdate {
    Set(String),
    Clear,
}

fn deserialize_accelerator_update<'de, D>(
    deserializer: D,
) -> Result<Option<NativeMenuAcceleratorUpdate>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    Ok(Some(match value {
        Some(accelerator) => NativeMenuAcceleratorUpdate::Set(accelerator),
        None => NativeMenuAcceleratorUpdate::Clear,
    }))
}

#[derive(Debug, Clone, Serialize)]
struct NativeMenuCommandPayload {
    #[serde(rename = "commandId")]
    command_id: String,
    #[serde(rename = "filePath", skip_serializing_if = "Option::is_none")]
    file_path: Option<String>,
}

pub fn install(app: &mut tauri::App) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let mut handles = NativeMenuHandles::default();
        let labels = NativeMenuLabels::default();
        let menu = build_menu(app.handle(), &mut handles, &labels)?;
        app.handle().set_menu(menu)?;
        app.handle().on_menu_event(handle_menu_event);
        app.state::<NativeMenuRegistry>()
            .replace(handles)
            .map_err(|e| tauri::Error::Anyhow(std::io::Error::other(e).into()))?;
    }
    Ok(())
}

/// Atomic locale rebuild. Builds a fresh menu using `labels`, swaps the
/// installed menu via `app.set_menu`, replaces the handle registry, and
/// re-applies the supplied state in a single command invocation so we
/// don't flash default state between rebuild and state-sync.
///
/// The React layer pushes both pieces (labels for relabeling, state for
/// recent files / check items / enable flags). Missing keys in `labels`
/// fall back to the English title via `NativeMenuLabels::resolve`.
#[tauri::command]
pub fn rebuild_native_menu(
    app: tauri::AppHandle<tauri::Wry>,
    registry: tauri::State<'_, NativeMenuRegistry>,
    labels: NativeMenuLabels,
    state: NativeMenuStateUpdate,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let mut handles = NativeMenuHandles::default();
        let menu = build_menu(&app, &mut handles, &labels).map_err(|e| e.to_string())?;
        app.set_menu(menu).map_err(|e| e.to_string())?;
        registry.replace(handles)?;
        apply_native_menu_state(&app, &registry, state)?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, registry, labels, state);
    }
    Ok(())
}

fn effective_accelerator(accelerator: Option<&'static str>, enabled: bool) -> Option<&'static str> {
    enabled.then_some(accelerator).flatten()
}

fn build_menu(
    app: &AppHandle<tauri::Wry>,
    handles: &mut NativeMenuHandles,
    labels: &NativeMenuLabels,
) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    for section in menu_spec() {
        let section_title = labels.resolve(section.title);
        let submenu = Submenu::new(app, &section_title, true)?;
        if section.title == "Beam Bench" {
            append_app_menu_entries(app, &submenu, handles, labels)?;
        } else {
            append_entries(app, &submenu, section.entries, handles, labels)?;
        }
        if section.title == "Beam Bench" {
            let separator = PredefinedMenuItem::separator(app)?;
            submenu.append(&separator)?;
            let hide = PredefinedMenuItem::hide(app, None)?;
            submenu.append(&hide)?;
            let hide_others = PredefinedMenuItem::hide_others(app, None)?;
            submenu.append(&hide_others)?;
            let show_all = PredefinedMenuItem::show_all(app, None)?;
            submenu.append(&show_all)?;
            let separator = PredefinedMenuItem::separator(app)?;
            submenu.append(&separator)?;
            let quit_title = labels.resolve("Quit Beam Bench");
            let quit = MenuItem::with_id(
                app,
                command::APP_QUIT,
                &quit_title,
                true,
                Some("CmdOrCtrl+Q"),
            )?;
            handles
                .normal_items
                .entry(command::APP_QUIT.to_string())
                .or_default()
                .push(quit.clone());
            handles.default_accelerators.insert(
                command::APP_QUIT.to_string(),
                Some("CmdOrCtrl+Q".to_string()),
            );
            submenu.append(&quit)?;
        }
        menu.append(&submenu)?;
    }
    Ok(menu)
}

fn append_app_menu_entries(
    app: &AppHandle<tauri::Wry>,
    submenu: &Submenu<tauri::Wry>,
    handles: &mut NativeMenuHandles,
    labels: &NativeMenuLabels,
) -> tauri::Result<()> {
    for entry in APP_MENU {
        if let NativeMenuEntry::Command {
            id: command::APP_QUIT,
            ..
        } = entry
        {
            continue;
        }
        append_entries(app, submenu, &[*entry], handles, labels)?;
    }
    Ok(())
}

fn append_entries(
    app: &AppHandle<tauri::Wry>,
    submenu: &Submenu<tauri::Wry>,
    entries: &[NativeMenuEntry],
    handles: &mut NativeMenuHandles,
    labels: &NativeMenuLabels,
) -> tauri::Result<()> {
    for entry in entries {
        match *entry {
            NativeMenuEntry::Command {
                id,
                title,
                accelerator,
                enabled,
            } => {
                let resolved = labels.resolve(title);
                let item = MenuItem::with_id(
                    app,
                    id,
                    &resolved,
                    enabled,
                    effective_accelerator(accelerator, enabled),
                )?;
                handles
                    .normal_items
                    .entry(id.to_string())
                    .or_default()
                    .push(item.clone());
                handles
                    .default_accelerators
                    .insert(id.to_string(), accelerator.map(str::to_string));
                submenu.append(&item)?;
            }
            NativeMenuEntry::Check {
                id,
                title,
                accelerator,
                enabled,
                checked,
            } => {
                let resolved = labels.resolve(title);
                let item = CheckMenuItem::with_id(
                    app,
                    id,
                    &resolved,
                    enabled,
                    checked,
                    effective_accelerator(accelerator, enabled),
                )?;
                handles
                    .check_items
                    .entry(id.to_string())
                    .or_default()
                    .push(item.clone());
                handles
                    .default_accelerators
                    .insert(id.to_string(), accelerator.map(str::to_string));
                submenu.append(&item)?;
            }
            NativeMenuEntry::Submenu { title, entries } => {
                let resolved = labels.resolve(title);
                let child = Submenu::new(app, &resolved, true)?;
                append_entries(app, &child, entries, handles, labels)?;
                submenu.append(&child)?;
            }
            NativeMenuEntry::Label { title } => {
                let resolved = labels.resolve(title);
                let item = MenuItem::with_id(
                    app,
                    format!("native-menu-label:{title}"),
                    &resolved,
                    false,
                    None::<&str>,
                )?;
                submenu.append(&item)?;
            }
            NativeMenuEntry::RecentProjects => {
                let recent_title = labels.resolve("Recent Projects");
                let child = Submenu::new(app, &recent_title, true)?;
                let empty_title = labels.resolve("No Recent Projects");
                let item = MenuItem::with_id(
                    app,
                    command::FILE_RECENT_EMPTY,
                    &empty_title,
                    false,
                    None::<&str>,
                )?;
                child.append(&item)?;
                handles.recent_submenu = Some(child.clone());
                handles.recent_items.push(item);
                submenu.append(&child)?;
            }
            NativeMenuEntry::Separator => {
                let item = PredefinedMenuItem::separator(app)?;
                submenu.append(&item)?;
            }
        }
    }
    Ok(())
}

fn handle_menu_event(app: &AppHandle<tauri::Wry>, event: tauri::menu::MenuEvent) {
    let event_id = event.id().as_ref().to_string();
    let registry = app.state::<NativeMenuRegistry>();
    if let Some(file_path) = registry.recent_path_for_id(&event_id) {
        let _ = app.emit(
            NATIVE_MENU_EVENT,
            NativeMenuCommandPayload {
                command_id: command::FILE_OPEN_RECENT.to_string(),
                file_path: Some(file_path),
            },
        );
        return;
    }

    if !command_ids().contains(event_id.as_str()) {
        return;
    }

    if event_id == command::APP_QUIT {
        handle_app_quit(app);
        return;
    }

    let _ = app.emit(
        NATIVE_MENU_EVENT,
        NativeMenuCommandPayload {
            command_id: event_id,
            file_path: None,
        },
    );
}

fn handle_app_quit(app: &AppHandle<tauri::Wry>) {
    if !frontend_is_ready(app) {
        cleanup_tracked_camera_frame_files(app);
        app.exit(0);
        return;
    }

    if project_has_unsaved_changes(app) {
        request_close_for_active_window(app);
        return;
    }

    cleanup_tracked_camera_frame_files(app);
    app.exit(0);
}

fn frontend_is_ready(app: &AppHandle<tauri::Wry>) -> bool {
    app.try_state::<crate::state::FrontendReady>()
        .map(|state| state.0.load(std::sync::atomic::Ordering::Acquire))
        .unwrap_or(false)
}

fn project_has_unsaved_changes(app: &AppHandle<tauri::Wry>) -> bool {
    app.try_state::<Arc<ServiceContext>>()
        .and_then(|ctx| {
            ctx.project
                .lock()
                .ok()
                .and_then(|project| project.as_ref().map(|project| project.dirty))
        })
        .unwrap_or(false)
}

fn cleanup_tracked_camera_frame_files(app: &AppHandle<tauri::Wry>) {
    if let Some(ctx) = app.try_state::<Arc<ServiceContext>>() {
        let deleted = ctx.cleanup_tracked_camera_frame_files();
        if deleted > 0 {
            tracing::info!(deleted, "Cleaned tracked camera frames");
        }
    }
}

fn request_close_for_active_window(app: &AppHandle<tauri::Wry>) {
    let mut windows = app.webview_windows();
    let target = windows
        .values()
        .find(|window| window.is_focused().unwrap_or(false))
        .cloned()
        .or_else(|| windows.remove("main"))
        .or_else(|| windows.into_values().next());

    if let Some(window) = target {
        if let Err(error) = window.close() {
            tracing::warn!(error = %error, "Failed to close window from native Quit menu");
        }
    } else {
        app.exit(0);
    }
}

fn command_ids() -> HashSet<&'static str> {
    let mut ids = HashSet::new();
    for section in menu_spec() {
        collect_command_ids(section.entries, &mut ids);
    }
    ids
}

fn collect_command_ids(entries: &[NativeMenuEntry], ids: &mut HashSet<&'static str>) {
    for entry in entries {
        match entry {
            NativeMenuEntry::Command { id, .. } | NativeMenuEntry::Check { id, .. } => {
                ids.insert(*id);
            }
            NativeMenuEntry::Submenu { entries, .. } => collect_command_ids(entries, ids),
            NativeMenuEntry::Label { .. }
            | NativeMenuEntry::RecentProjects
            | NativeMenuEntry::Separator => {}
        }
    }
}

#[tauri::command]
pub fn update_native_menu_state(
    app: AppHandle<tauri::Wry>,
    registry: State<'_, NativeMenuRegistry>,
    state: NativeMenuStateUpdate,
) -> Result<(), String> {
    apply_native_menu_state(&app, &registry, state)
}

/// Apply a state update against the currently-installed menu. Shared by
/// `update_native_menu_state` (the live patch path) and `rebuild_native_menu`
/// (the locale-change atomic rebuild path), so a rebuilt menu picks up the
/// same enable/check/recent-files state without a round-trip flash.
fn apply_native_menu_state(
    app: &AppHandle<tauri::Wry>,
    registry: &NativeMenuRegistry,
    state: NativeMenuStateUpdate,
) -> Result<(), String> {
    if let Some(recent_files) = state.recent_files.as_deref() {
        registry.update_recent_files(app, recent_files)?;
    }

    let normal_items = registry
        .normal_items
        .lock()
        .map_err(|e| format!("Failed to lock native menu item registry: {e}"))?;
    let check_items = registry
        .check_items
        .lock()
        .map_err(|e| format!("Failed to lock native check menu item registry: {e}"))?;
    let default_accelerators = registry
        .default_accelerators
        .lock()
        .map_err(|e| format!("Failed to lock native accelerator registry: {e}"))?;

    for item_state in state.items {
        let accelerator_update = next_accelerator(&item_state, &default_accelerators);
        if let Some(items) = normal_items.get(&item_state.id) {
            for item in items {
                if let Some(enabled) = item_state.enabled {
                    item.set_enabled(enabled).map_err(|e| e.to_string())?;
                }
                if let Some(title) = item_state.title.as_deref() {
                    item.set_text(title).map_err(|e| e.to_string())?;
                }
                if let Some(accelerator) = accelerator_update.as_ref() {
                    item.set_accelerator(accelerator.as_deref())
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        if let Some(items) = check_items.get(&item_state.id) {
            for item in items {
                if let Some(enabled) = item_state.enabled {
                    item.set_enabled(enabled).map_err(|e| e.to_string())?;
                }
                if let Some(checked) = item_state.checked {
                    item.set_checked(checked).map_err(|e| e.to_string())?;
                }
                if let Some(title) = item_state.title.as_deref() {
                    item.set_text(title).map_err(|e| e.to_string())?;
                }
                if let Some(accelerator) = accelerator_update.as_ref() {
                    item.set_accelerator(accelerator.as_deref())
                        .map_err(|e| e.to_string())?;
                }
            }
        }
    }

    Ok(())
}

fn next_accelerator(
    item_state: &NativeMenuItemState,
    default_accelerators: &HashMap<String, Option<String>>,
) -> Option<Option<String>> {
    if item_state.enabled == Some(false) {
        // Disabled items must not carry effective accelerators on macOS; otherwise
        // Cocoa consumes the shortcut and beeps. Enabled=false intentionally wins
        // even if a caller also supplies an accelerator update in the same patch.
        return Some(None);
    }
    if let Some(accelerator) = item_state.accelerator.clone() {
        return Some(match accelerator {
            NativeMenuAcceleratorUpdate::Set(accelerator) => Some(accelerator),
            NativeMenuAcceleratorUpdate::Clear => None,
        });
    }
    if item_state.enabled == Some(true) {
        return Some(default_accelerators.get(&item_state.id).cloned().flatten());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    struct MenuSnapshotRow {
        menu_path: String,
        command_id: &'static str,
        label: &'static str,
        effective_accelerator: Option<&'static str>,
        enabled: bool,
        checked: Option<bool>,
    }

    #[derive(Debug, Deserialize)]
    struct ToolsMenuContractItem {
        label: String,
        #[serde(rename = "commandId")]
        command_id: String,
        parent: Option<String>,
    }

    fn menu_snapshot() -> Vec<MenuSnapshotRow> {
        let mut rows = Vec::new();
        for section in menu_spec() {
            snapshot_entries(section.title.to_string(), section.entries, &mut rows);
        }
        rows
    }

    fn tools_menu_contract_rows() -> Vec<(String, String)> {
        let items: Vec<ToolsMenuContractItem> =
            serde_json::from_str(include_str!("../../src/commands/toolsMenuContract.json"))
                .expect("Tools menu contract JSON should parse");

        items
            .into_iter()
            .map(|item| {
                let menu_path = match item.parent {
                    Some(parent) => format!("Tools > {parent} > {}", item.label),
                    None => format!("Tools > {}", item.label),
                };
                (menu_path, item.command_id)
            })
            .collect()
    }

    fn snapshot_entries(
        parent_path: String,
        entries: &[NativeMenuEntry],
        rows: &mut Vec<MenuSnapshotRow>,
    ) {
        for entry in entries {
            match *entry {
                NativeMenuEntry::Command {
                    id,
                    title,
                    accelerator,
                    enabled,
                } => rows.push(MenuSnapshotRow {
                    menu_path: format!("{parent_path} > {title}"),
                    command_id: id,
                    label: title,
                    effective_accelerator: effective_accelerator(accelerator, enabled),
                    enabled,
                    checked: None,
                }),
                NativeMenuEntry::Check {
                    id,
                    title,
                    accelerator,
                    enabled,
                    checked,
                } => rows.push(MenuSnapshotRow {
                    menu_path: format!("{parent_path} > {title}"),
                    command_id: id,
                    label: title,
                    effective_accelerator: effective_accelerator(accelerator, enabled),
                    enabled,
                    checked: Some(checked),
                }),
                NativeMenuEntry::Submenu { title, entries } => {
                    snapshot_entries(format!("{parent_path} > {title}"), entries, rows);
                }
                NativeMenuEntry::Label { .. } => {}
                NativeMenuEntry::RecentProjects => rows.push(MenuSnapshotRow {
                    menu_path: format!("{parent_path} > Recent Projects > No Recent Projects"),
                    command_id: command::FILE_RECENT_EMPTY,
                    label: "No Recent Projects",
                    effective_accelerator: None,
                    enabled: false,
                    checked: None,
                }),
                NativeMenuEntry::Separator => {}
            }
        }
    }

    #[test]
    fn native_menu_file_shape_matches_product_order() {
        let file_rows: Vec<_> = menu_snapshot()
            .into_iter()
            .filter(|row| row.menu_path.starts_with("File > "))
            .map(|row| {
                (
                    row.menu_path,
                    row.command_id,
                    row.effective_accelerator,
                    row.enabled,
                )
            })
            .collect();

        assert_eq!(
            file_rows,
            vec![
                (
                    "File > New".to_string(),
                    command::FILE_NEW,
                    Some("CmdOrCtrl+N"),
                    true
                ),
                (
                    "File > New Window".to_string(),
                    command::FILE_NEW_WINDOW,
                    None,
                    true
                ),
                (
                    "File > Recent Projects > No Recent Projects".to_string(),
                    command::FILE_RECENT_EMPTY,
                    None,
                    false
                ),
                (
                    "File > Open".to_string(),
                    command::FILE_OPEN,
                    Some("CmdOrCtrl+O"),
                    true
                ),
                (
                    "File > Import".to_string(),
                    command::FILE_IMPORT,
                    Some("CmdOrCtrl+I"),
                    true
                ),
                (
                    "File > Show Notes".to_string(),
                    command::FILE_NOTES,
                    None,
                    false
                ),
                ("File > Save".to_string(), command::FILE_SAVE, None, false),
                (
                    "File > Save As".to_string(),
                    command::FILE_SAVE_AS,
                    None,
                    false
                ),
                (
                    "File > Export".to_string(),
                    command::FILE_EXPORT,
                    None,
                    false
                ),
                (
                    "File > Preferences > Import Prefs".to_string(),
                    command::FILE_PREFS_IMPORT,
                    None,
                    true
                ),
                (
                    "File > Preferences > Export Prefs".to_string(),
                    command::FILE_PREFS_EXPORT,
                    None,
                    true
                ),
                (
                    "File > Preferences > Open Prefs Folder".to_string(),
                    command::FILE_PREFS_OPEN_FOLDER,
                    None,
                    true
                ),
                (
                    "File > Preferences > Reset Prefs to Defaults".to_string(),
                    command::FILE_PREFS_RESET_DEFAULTS,
                    None,
                    true
                ),
                (
                    "File > Print (black only)".to_string(),
                    command::FILE_PRINT_BLACK,
                    None,
                    false
                ),
                (
                    "File > Print (keep colors)".to_string(),
                    command::FILE_PRINT_COLORS,
                    None,
                    false
                ),
                (
                    "File > Save Processed Bitmap".to_string(),
                    command::FILE_SAVE_PROCESSED_BITMAP,
                    None,
                    false
                ),
                (
                    "File > Save Background Capture".to_string(),
                    command::FILE_SAVE_BACKGROUND_CAPTURE,
                    None,
                    false
                ),
            ]
        );
    }

    #[test]
    fn native_menu_edit_shape_matches_product_order() {
        let edit_rows: Vec<_> = menu_snapshot()
            .into_iter()
            .filter(|row| row.menu_path.starts_with("Edit > "))
            .map(|row| (row.menu_path, row.command_id))
            .collect();

        assert_eq!(
            edit_rows,
            vec![
                ("Edit > Undo".to_string(), command::EDIT_UNDO),
                ("Edit > Redo".to_string(), command::EDIT_REDO),
                ("Edit > Select All".to_string(), command::EDIT_SELECT_ALL),
                (
                    "Edit > Invert Selection".to_string(),
                    command::EDIT_INVERT_SELECTION,
                ),
                ("Edit > Cut".to_string(), command::EDIT_CUT),
                ("Edit > Copy".to_string(), command::EDIT_COPY),
                ("Edit > Duplicate".to_string(), command::EDIT_DUPLICATE),
                ("Edit > Paste".to_string(), command::EDIT_PASTE),
                (
                    "Edit > Paste in Place".to_string(),
                    command::EDIT_PASTE_IN_PLACE,
                ),
                ("Edit > Delete".to_string(), command::EDIT_DELETE),
                (
                    "Edit > Convert to Path".to_string(),
                    command::EDIT_CONVERT_TO_PATH,
                ),
                (
                    "Edit > Convert to Bitmap".to_string(),
                    command::EDIT_CONVERT_TO_BITMAP,
                ),
                ("Edit > Close Path".to_string(), command::EDIT_CLOSE_PATH),
                (
                    "Edit > Close Selected Paths With Tolerance".to_string(),
                    command::EDIT_CLOSE_SELECTED_PATHS_WITH_TOLERANCE,
                ),
                (
                    "Edit > Auto-Join Selected Shapes".to_string(),
                    command::EDIT_AUTO_JOIN_SELECTED_SHAPES,
                ),
                (
                    "Edit > Close & Join".to_string(),
                    command::EDIT_CLOSE_AND_JOIN,
                ),
                (
                    "Edit > Optimize Selected Shapes".to_string(),
                    command::EDIT_OPTIMIZE_SELECTED_SHAPES,
                ),
                (
                    "Edit > Delete Duplicates".to_string(),
                    command::EDIT_DELETE_DUPLICATES,
                ),
                (
                    "Edit > Select Open Shapes".to_string(),
                    command::EDIT_SELECT_OPEN_SHAPES,
                ),
                (
                    "Edit > Select Open Shapes Set to Fill".to_string(),
                    command::EDIT_SELECT_OPEN_SHAPES_SET_TO_FILL,
                ),
                (
                    "Edit > Select All Shapes in Current Layer".to_string(),
                    command::EDIT_SELECT_ALL_SHAPES_IN_CURRENT_LAYER,
                ),
                (
                    "Edit > Select Contained Shapes".to_string(),
                    command::EDIT_SELECT_CONTAINED_SHAPES,
                ),
                (
                    "Edit > Select Shapes Smaller Than Selected".to_string(),
                    command::EDIT_SELECT_SHAPES_SMALLER_THAN_SELECTED,
                ),
                (
                    "Edit > Image Options > Refresh Image".to_string(),
                    command::EDIT_IMAGE_REFRESH,
                ),
                (
                    "Edit > Image Options > Replace Image".to_string(),
                    command::EDIT_IMAGE_REPLACE,
                ),
                (
                    "Edit > Image Options > Replace Image to Fit".to_string(),
                    command::EDIT_IMAGE_REPLACE_TO_FIT,
                ),
                ("Edit > Settings".to_string(), command::EDIT_SETTINGS),
            ]
        );
    }

    #[test]
    fn native_menu_arrange_shape_matches_product_order() {
        let arrange_rows: Vec<_> = menu_snapshot()
            .into_iter()
            .filter(|row| row.menu_path.starts_with("Arrange > "))
            .map(|row| (row.menu_path, row.command_id))
            .collect();

        assert_eq!(
            arrange_rows,
            vec![
                ("Arrange > Group".to_string(), command::ARRANGE_GROUP),
                ("Arrange > Ungroup > Ungroup".to_string(), command::ARRANGE_UNGROUP),
                ("Arrange > Ungroup > Auto-Group".to_string(), command::ARRANGE_AUTO_GROUP),
                (
                    "Arrange > Flip Horizontal / Vertical > Flip Horizontal".to_string(),
                    command::ARRANGE_FLIP_HORIZONTAL,
                ),
                (
                    "Arrange > Flip Horizontal / Vertical > Flip Vertical".to_string(),
                    command::ARRANGE_FLIP_VERTICAL,
                ),
                (
                    "Arrange > Mirror Across Line".to_string(),
                    command::ARRANGE_MIRROR_ACROSS_LINE,
                ),
                (
                    "Arrange > Rotate 90° Clockwise / Counter-Clockwise > Rotate 90° Clockwise"
                        .to_string(),
                    command::ARRANGE_ROTATE_CW,
                ),
                (
                    "Arrange > Rotate 90° Clockwise / Counter-Clockwise > Rotate 90° Counter-Clockwise"
                        .to_string(),
                    command::ARRANGE_ROTATE_CCW,
                ),
                (
                    "Arrange > Two-Point Rotate / Scale".to_string(),
                    command::ARRANGE_TWO_POINT_ROTATE_SCALE,
                ),
                (
                    "Arrange > Align > Align Centers".to_string(),
                    command::ARRANGE_ALIGN_CENTERS,
                ),
                (
                    "Arrange > Align > Align Vertical Centers".to_string(),
                    command::ARRANGE_ALIGN_CENTER_VERTICAL,
                ),
                (
                    "Arrange > Align > Align Horizontal Centers".to_string(),
                    command::ARRANGE_ALIGN_CENTER_HORIZONTAL,
                ),
                (
                    "Arrange > Align > Align Left".to_string(),
                    command::ARRANGE_ALIGN_LEFT,
                ),
                (
                    "Arrange > Align > Align Right".to_string(),
                    command::ARRANGE_ALIGN_RIGHT,
                ),
                (
                    "Arrange > Align > Align Bottom".to_string(),
                    command::ARRANGE_ALIGN_BOTTOM,
                ),
                (
                    "Arrange > Align > Align Top".to_string(),
                    command::ARRANGE_ALIGN_TOP,
                ),
                (
                    "Arrange > Distribute > Distribute V-Spaced".to_string(),
                    command::ARRANGE_DISTRIBUTE_V_SPACED,
                ),
                (
                    "Arrange > Distribute > Distribute V-Centered".to_string(),
                    command::ARRANGE_DISTRIBUTE_V_CENTERED,
                ),
                (
                    "Arrange > Distribute > Distribute H-Spaced".to_string(),
                    command::ARRANGE_DISTRIBUTE_H_SPACED,
                ),
                (
                    "Arrange > Distribute > Distribute H-Centered".to_string(),
                    command::ARRANGE_DISTRIBUTE_H_CENTERED,
                ),
                (
                    "Arrange > Distribute > Move H Together".to_string(),
                    command::ARRANGE_MOVE_H_TOGETHER,
                ),
                (
                    "Arrange > Distribute > Move V Together".to_string(),
                    command::ARRANGE_MOVE_V_TOGETHER,
                ),
                ("Arrange > Nest Selected".to_string(), command::ARRANGE_NEST_SELECTED),
                ("Arrange > Dock > Dock Left".to_string(), command::ARRANGE_DOCK_LEFT),
                ("Arrange > Dock > Dock Right".to_string(), command::ARRANGE_DOCK_RIGHT),
                ("Arrange > Dock > Dock Up".to_string(), command::ARRANGE_DOCK_UP),
                ("Arrange > Dock > Dock Down".to_string(), command::ARRANGE_DOCK_DOWN),
                (
                    "Arrange > Move Selected Objects > Move to Laser Position".to_string(),
                    command::ARRANGE_MOVE_TO_LASER_POSITION,
                ),
                (
                    "Arrange > Move Selected Objects > Move to Page Center".to_string(),
                    command::ARRANGE_MOVE_TO_PAGE_CENTER,
                ),
                (
                    "Arrange > Move Selected Objects > Move to Upper Left".to_string(),
                    command::ARRANGE_MOVE_TO_UPPER_LEFT,
                ),
                (
                    "Arrange > Move Selected Objects > Move to Upper Right".to_string(),
                    command::ARRANGE_MOVE_TO_UPPER_RIGHT,
                ),
                (
                    "Arrange > Move Selected Objects > Move to Lower Left".to_string(),
                    command::ARRANGE_MOVE_TO_LOWER_LEFT,
                ),
                (
                    "Arrange > Move Selected Objects > Move to Lower Right".to_string(),
                    command::ARRANGE_MOVE_TO_LOWER_RIGHT,
                ),
                (
                    "Arrange > Move Selected Objects > Move to Left".to_string(),
                    command::ARRANGE_MOVE_TO_LEFT,
                ),
                (
                    "Arrange > Move Selected Objects > Move to Right".to_string(),
                    command::ARRANGE_MOVE_TO_RIGHT,
                ),
                (
                    "Arrange > Move Selected Objects > Move to Top".to_string(),
                    command::ARRANGE_MOVE_TO_TOP,
                ),
                (
                    "Arrange > Move Selected Objects > Move to Bottom".to_string(),
                    command::ARRANGE_MOVE_TO_BOTTOM,
                ),
                (
                    "Arrange > Move Laser to Selection > Move Laser to Selection Center"
                        .to_string(),
                    command::ARRANGE_MOVE_LASER_TO_SELECTION_CENTER,
                ),
                (
                    "Arrange > Move Laser to Selection > Move Laser to Upper Left of Selection"
                        .to_string(),
                    command::ARRANGE_MOVE_LASER_TO_SELECTION_UPPER_LEFT,
                ),
                (
                    "Arrange > Move Laser to Selection > Move Laser to Upper Right of Selection"
                        .to_string(),
                    command::ARRANGE_MOVE_LASER_TO_SELECTION_UPPER_RIGHT,
                ),
                (
                    "Arrange > Move Laser to Selection > Move Laser to Lower Left of Selection"
                        .to_string(),
                    command::ARRANGE_MOVE_LASER_TO_SELECTION_LOWER_LEFT,
                ),
                (
                    "Arrange > Move Laser to Selection > Move Laser to Lower Right of Selection"
                        .to_string(),
                    command::ARRANGE_MOVE_LASER_TO_SELECTION_LOWER_RIGHT,
                ),
                (
                    "Arrange > Move Laser to Selection > Move Laser to Left of Selection".to_string(),
                    command::ARRANGE_MOVE_LASER_TO_SELECTION_LEFT,
                ),
                (
                    "Arrange > Move Laser to Selection > Move Laser to Right of Selection"
                        .to_string(),
                    command::ARRANGE_MOVE_LASER_TO_SELECTION_RIGHT,
                ),
                (
                    "Arrange > Move Laser to Selection > Move Laser to Top of Selection".to_string(),
                    command::ARRANGE_MOVE_LASER_TO_SELECTION_TOP,
                ),
                (
                    "Arrange > Move Laser to Selection > Move Laser to Bottom of Selection"
                        .to_string(),
                    command::ARRANGE_MOVE_LASER_TO_SELECTION_BOTTOM,
                ),
                (
                    "Arrange > Move Laser to Selection > Jog Laser > Jog Laser Left".to_string(),
                    command::ARRANGE_JOG_LASER_LEFT,
                ),
                (
                    "Arrange > Move Laser to Selection > Jog Laser > Jog Laser Right".to_string(),
                    command::ARRANGE_JOG_LASER_RIGHT,
                ),
                (
                    "Arrange > Move Laser to Selection > Jog Laser > Jog Laser Up".to_string(),
                    command::ARRANGE_JOG_LASER_UP,
                ),
                (
                    "Arrange > Move Laser to Selection > Jog Laser > Jog Laser Down".to_string(),
                    command::ARRANGE_JOG_LASER_DOWN,
                ),
                ("Arrange > Grid Array".to_string(), command::ARRANGE_GRID_ARRAY),
                (
                    "Arrange > Circular Array".to_string(),
                    command::ARRANGE_CIRCULAR_ARRAY,
                ),
                (
                    "Arrange > Copy Along Path".to_string(),
                    command::ARRANGE_COPY_ALONG_PATH,
                ),
                (
                    "Arrange > Break Apart".to_string(),
                    command::ARRANGE_BREAK_APART,
                ),
                (
                    "Arrange > Push in Draw Order > Bring Forward".to_string(),
                    command::ARRANGE_FORWARD,
                ),
                (
                    "Arrange > Push in Draw Order > Send Backward".to_string(),
                    command::ARRANGE_BACKWARD,
                ),
                (
                    "Arrange > Push in Draw Order > Bring to Front".to_string(),
                    command::ARRANGE_FRONT,
                ),
                (
                    "Arrange > Push in Draw Order > Send to Back".to_string(),
                    command::ARRANGE_BACK,
                ),
                (
                    "Arrange > Lock Selected Shapes".to_string(),
                    command::ARRANGE_LOCK,
                ),
                (
                    "Arrange > Unlock Selected Shapes".to_string(),
                    command::ARRANGE_UNLOCK,
                ),
            ]
        );
    }

    #[test]
    fn native_menu_tools_order_matches_contract() {
        let tools_rows: Vec<_> = menu_snapshot()
            .into_iter()
            .filter(|row| row.menu_path.starts_with("Tools > "))
            .map(|row| (row.menu_path, row.command_id.to_string()))
            .collect();

        assert_eq!(tools_rows, tools_menu_contract_rows());
    }

    #[test]
    fn native_menu_top_level_order_includes_language_before_help() {
        let titles: Vec<_> = menu_spec().iter().map(|section| section.title).collect();
        assert_eq!(
            titles,
            vec![
                "Beam Bench",
                "File",
                "Edit",
                "Tools",
                "Arrange",
                "Laser Tools",
                "Window",
                "Language",
                "Help",
            ]
        );
    }

    #[test]
    fn native_menu_window_order_matches_react_window_order() {
        let window_rows: Vec<_> = menu_snapshot()
            .into_iter()
            .filter(|row| row.menu_path.starts_with("Window > "))
            .map(|row| {
                (
                    row.menu_path,
                    row.command_id,
                    row.effective_accelerator,
                    row.checked,
                )
            })
            .collect();

        assert_eq!(
            window_rows,
            vec![
                (
                    "Window > Reset to Default Layout".to_string(),
                    command::WINDOW_RESET_LAYOUT,
                    None,
                    None
                ),
                (
                    "Window > Preview".to_string(),
                    command::WINDOW_PREVIEW,
                    None,
                    Some(false)
                ),
                (
                    "Window > Zoom to Page".to_string(),
                    command::WINDOW_ZOOM_TO_PAGE,
                    None,
                    None
                ),
                (
                    "Window > Zoom In".to_string(),
                    command::WINDOW_ZOOM_IN,
                    None,
                    None
                ),
                (
                    "Window > Zoom Out".to_string(),
                    command::WINDOW_ZOOM_OUT,
                    None,
                    None
                ),
                (
                    "Window > Frame Selection".to_string(),
                    command::WINDOW_FRAME_SELECTION,
                    None,
                    None
                ),
                (
                    "Window > Wireframe / Coarse".to_string(),
                    command::WINDOW_VIEW_STYLE_WIREFRAME_COARSE,
                    None,
                    Some(false)
                ),
                (
                    "Window > Wireframe / Smooth".to_string(),
                    command::WINDOW_VIEW_STYLE_WIREFRAME_SMOOTH,
                    None,
                    Some(true)
                ),
                (
                    "Window > Filled / Coarse".to_string(),
                    command::WINDOW_VIEW_STYLE_FILLED_COARSE,
                    None,
                    Some(false)
                ),
                (
                    "Window > Filled / Smooth".to_string(),
                    command::WINDOW_VIEW_STYLE_FILLED_SMOOTH,
                    None,
                    Some(false)
                ),
                (
                    "Window > Toggle Wireframe / Filled".to_string(),
                    command::WINDOW_TOGGLE_WIREFRAME_FILLED,
                    Some("Alt+Shift+W"),
                    None
                ),
                (
                    "Window > Toggle Side Panels".to_string(),
                    command::WINDOW_SIDE_PANELS,
                    Some("F12"),
                    Some(true)
                ),
                (
                    "Window > Art Library".to_string(),
                    command::WINDOW_PANEL_ART_LIBRARY,
                    None,
                    Some(false)
                ),
                (
                    "Window > Arrange".to_string(),
                    command::WINDOW_TOOLBAR_ARRANGE,
                    None,
                    Some(true)
                ),
                (
                    "Window > Arrange (Long)".to_string(),
                    command::WINDOW_TOOLBAR_ARRANGE_LONG,
                    None,
                    Some(false)
                ),
                (
                    "Window > Modifiers".to_string(),
                    command::WINDOW_TOOLBAR_MODIFIERS,
                    None,
                    Some(true)
                ),
                (
                    "Window > Camera Control".to_string(),
                    command::WINDOW_PANEL_CAMERA_CONTROL,
                    None,
                    Some(false)
                ),
                (
                    "Window > Console".to_string(),
                    command::WINDOW_PANEL_CONSOLE,
                    None,
                    Some(true)
                ),
                (
                    "Window > Macros".to_string(),
                    command::WINDOW_PANEL_MACROS,
                    None,
                    Some(true)
                ),
                (
                    "Window > Cuts / Layers".to_string(),
                    command::WINDOW_PANEL_CUTS_LAYERS,
                    None,
                    Some(true)
                ),
                (
                    "Window > Color Palette".to_string(),
                    command::WINDOW_PANEL_COLOR_PALETTE,
                    None,
                    Some(true)
                ),
                (
                    "Window > Docking".to_string(),
                    command::WINDOW_TOOLBAR_DOCKING,
                    None,
                    Some(true)
                ),
                (
                    "Window > Laser".to_string(),
                    command::WINDOW_PANEL_LASER,
                    None,
                    Some(true)
                ),
                (
                    "Window > Material Library".to_string(),
                    command::WINDOW_PANEL_MATERIAL_LIBRARY,
                    None,
                    Some(true)
                ),
                (
                    "Window > Main".to_string(),
                    command::WINDOW_TOOLBAR_MAIN,
                    None,
                    Some(true)
                ),
                (
                    "Window > Move".to_string(),
                    command::WINDOW_PANEL_MOVE,
                    None,
                    Some(true)
                ),
                (
                    "Window > Numeric Edits".to_string(),
                    command::WINDOW_TOOLBAR_NUMERIC_EDITS,
                    None,
                    Some(true)
                ),
                (
                    "Window > Shape Properties".to_string(),
                    command::WINDOW_PANEL_SHAPE_PROPERTIES,
                    None,
                    Some(true)
                ),
                (
                    "Window > Text Options".to_string(),
                    command::WINDOW_TOOLBAR_TEXT_OPTIONS,
                    None,
                    Some(true)
                ),
                (
                    "Window > Tools".to_string(),
                    command::WINDOW_TOOLBAR_TOOLS,
                    None,
                    Some(true)
                ),
            ]
        );
    }

    #[test]
    fn disabled_native_menu_items_have_no_effective_accelerator() {
        let offenders: Vec<_> = menu_snapshot()
            .into_iter()
            .filter(|row| !row.enabled && row.effective_accelerator.is_some())
            .map(|row| row.menu_path)
            .collect();

        assert!(
            offenders.is_empty(),
            "disabled items with accelerators: {offenders:?}"
        );
    }

    #[test]
    fn native_menu_command_ids_are_unique_and_stable() {
        let ids: Vec<_> = menu_snapshot()
            .into_iter()
            .map(|row| row.command_id)
            .filter(|id| *id != command::FILE_RECENT_EMPTY)
            .collect();
        let unique: HashSet<_> = ids.iter().copied().collect();
        let duplicate_ids: Vec<_> = unique
            .iter()
            .copied()
            .filter(|id| ids.iter().filter(|candidate| *candidate == id).count() > 1)
            .collect();

        assert!(duplicate_ids.is_empty(), "duplicate ids: {duplicate_ids:?}");
        assert!(unique.contains(command::APP_PREFERENCES));
        assert!(unique.contains(command::APP_QUIT));
        assert!(unique.contains(command::FILE_EXPORT));
        assert!(unique.contains(command::LASER_SAVE_MACHINE_FILES));
        assert!(unique.contains(command::LANGUAGE_EN));
        assert!(unique.contains(command::LANGUAGE_DE));
        assert!(unique.contains(command::LANGUAGE_ZH_TW));
        assert!(unique.contains(command::WINDOW_SIDE_PANELS));
    }

    #[test]
    fn native_menu_commands_are_mapped_in_agent_registry() {
        let unmapped: Vec<_> = command_ids()
            .into_iter()
            .filter(|id| !beambench_service::agent::menu_command_covered(id))
            .collect();

        assert!(
            unmapped.is_empty(),
            "native menu commands missing agent coverage: {unmapped:?}"
        );
    }

    #[test]
    fn agent_debug_refs_point_to_existing_native_entries() {
        let menu_ids = command_ids();
        let main_source = include_str!("main.rs");

        for cap in beambench_service::agent::capabilities() {
            if let Some(menu_command) = cap.debug_refs.menu_command {
                if let Some(prefix) = menu_command.strip_suffix('*') {
                    assert!(
                        menu_ids.iter().any(|id| id.starts_with(prefix)),
                        "capability {} points at missing menu command pattern {}",
                        cap.id,
                        menu_command
                    );
                } else {
                    assert!(
                        menu_ids.contains(menu_command),
                        "capability {} points at missing menu command {}",
                        cap.id,
                        menu_command
                    );
                }
            }

            if let Some(tauri_command) = cap.debug_refs.tauri_command {
                assert!(
                    main_source.contains(&format!("::{tauri_command}")),
                    "capability {} points at missing Tauri command {}",
                    cap.id,
                    tauri_command
                );
            }
        }
    }

    #[test]
    fn native_menu_state_update_payload_maps_frontend_shape() {
        let update: NativeMenuStateUpdate = serde_json::from_value(serde_json::json!({
            "items": [
                {
                    "id": command::FILE_EXPORT,
                    "enabled": true,
                    "checked": false,
                    "title": "Export Selection",
                    "accelerator": null
                }
            ],
            "recentFiles": [
                {
                    "name": "Recent Job",
                    "path": "/tmp/recent.lzrproj"
                }
            ]
        }))
        .expect("frontend state payload deserializes");

        assert_eq!(
            update.items,
            vec![NativeMenuItemState {
                id: command::FILE_EXPORT.to_string(),
                enabled: Some(true),
                checked: Some(false),
                title: Some("Export Selection".to_string()),
                accelerator: Some(NativeMenuAcceleratorUpdate::Clear),
            }]
        );
        assert_eq!(
            update.recent_files,
            Some(vec![NativeRecentFileState {
                name: "Recent Job".to_string(),
                path: "/tmp/recent.lzrproj".to_string(),
            }])
        );
    }

    #[test]
    fn enabled_state_restores_default_accelerator_when_not_overridden() {
        let mut defaults = HashMap::new();
        defaults.insert(command::FILE_EXPORT.to_string(), Some("Alt+X".to_string()));

        assert_eq!(
            next_accelerator(
                &NativeMenuItemState {
                    id: command::FILE_EXPORT.to_string(),
                    enabled: Some(true),
                    checked: None,
                    title: None,
                    accelerator: None,
                },
                &defaults,
            ),
            Some(Some("Alt+X".to_string()))
        );
        assert_eq!(
            next_accelerator(
                &NativeMenuItemState {
                    id: command::FILE_EXPORT.to_string(),
                    enabled: Some(false),
                    checked: None,
                    title: None,
                    accelerator: Some(NativeMenuAcceleratorUpdate::Set("Alt+X".to_string())),
                },
                &defaults,
            ),
            Some(None)
        );
    }
}
