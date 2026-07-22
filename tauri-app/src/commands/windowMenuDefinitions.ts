import { APP_COMMANDS, type AppCommandId } from './appCommandIds';
import type { ToolbarId } from '../panels';
import type { ViewStyle } from '../stores/uiStore';

export const WINDOW_VIEW_STYLE_ITEMS = [
  { label: 'Wireframe / Coarse', commandId: APP_COMMANDS.WINDOW_VIEW_STYLE_WIREFRAME_COARSE, viewStyle: 'wireframe_coarse' },
  { label: 'Wireframe / Smooth', commandId: APP_COMMANDS.WINDOW_VIEW_STYLE_WIREFRAME_SMOOTH, viewStyle: 'wireframe_smooth' },
  { label: 'Filled / Coarse', commandId: APP_COMMANDS.WINDOW_VIEW_STYLE_FILLED_COARSE, viewStyle: 'filled_coarse' },
  { label: 'Filled / Smooth', commandId: APP_COMMANDS.WINDOW_VIEW_STYLE_FILLED_SMOOTH, viewStyle: 'filled_smooth' },
] as const satisfies ReadonlyArray<{ label: string; commandId: AppCommandId; viewStyle: ViewStyle }>;

export const WINDOW_PANEL_MENU_ITEMS = [
  { label: 'Art Library', commandId: APP_COMMANDS.WINDOW_PANEL_ART_LIBRARY, panelId: 'art_library' },
  { label: 'Camera Control', commandId: APP_COMMANDS.WINDOW_PANEL_CAMERA_CONTROL, panelId: 'camera' },
  { label: 'Console', commandId: APP_COMMANDS.WINDOW_PANEL_CONSOLE, panelId: 'console' },
  { label: 'Macros', commandId: APP_COMMANDS.WINDOW_PANEL_MACROS, panelId: 'macros' },
  { label: 'Cuts / Layers', commandId: APP_COMMANDS.WINDOW_PANEL_CUTS_LAYERS, panelId: 'cuts_layers' },
  { label: 'Color Palette', commandId: APP_COMMANDS.WINDOW_PANEL_COLOR_PALETTE, panelId: 'color_palette' },
  { label: 'Laser Control', commandId: APP_COMMANDS.WINDOW_PANEL_LASER, panelId: 'laser' },
  { label: 'Material Library', commandId: APP_COMMANDS.WINDOW_PANEL_MATERIAL_LIBRARY, panelId: 'material' },
  { label: 'Move', commandId: APP_COMMANDS.WINDOW_PANEL_MOVE, panelId: 'move' },
  { label: 'Shape Properties', commandId: APP_COMMANDS.WINDOW_PANEL_SHAPE_PROPERTIES, panelId: 'properties' },
] as const satisfies ReadonlyArray<{ label: string; commandId: AppCommandId; panelId: string }>;

export const WINDOW_TOOLBAR_MENU_ITEMS = [
  { label: 'Arrange', commandId: APP_COMMANDS.WINDOW_TOOLBAR_ARRANGE, toolbarId: 'arrange' },
  { label: 'Arrange (Long)', commandId: APP_COMMANDS.WINDOW_TOOLBAR_ARRANGE_LONG, toolbarId: 'arrangeLong' },
  { label: 'Modifiers', commandId: APP_COMMANDS.WINDOW_TOOLBAR_MODIFIERS, toolbarId: 'modifiers' },
  { label: 'Docking', commandId: APP_COMMANDS.WINDOW_TOOLBAR_DOCKING, toolbarId: 'docking' },
  { label: 'Main', commandId: APP_COMMANDS.WINDOW_TOOLBAR_MAIN, toolbarId: 'main' },
  { label: 'Tools', commandId: APP_COMMANDS.WINDOW_TOOLBAR_TOOLS, toolbarId: 'tools' },
] as const satisfies ReadonlyArray<{ label: string; commandId: AppCommandId; toolbarId: ToolbarId }>;

export const WINDOW_PANEL_TOOLBAR_MENU_ITEMS = [
  WINDOW_PANEL_MENU_ITEMS[0],
  WINDOW_TOOLBAR_MENU_ITEMS[0],
  WINDOW_TOOLBAR_MENU_ITEMS[1],
  WINDOW_TOOLBAR_MENU_ITEMS[2],
  WINDOW_PANEL_MENU_ITEMS[1],
  WINDOW_PANEL_MENU_ITEMS[2],
  WINDOW_PANEL_MENU_ITEMS[3],
  WINDOW_PANEL_MENU_ITEMS[4],
  WINDOW_PANEL_MENU_ITEMS[5],
  WINDOW_TOOLBAR_MENU_ITEMS[3],
  WINDOW_PANEL_MENU_ITEMS[6],
  WINDOW_PANEL_MENU_ITEMS[7],
  WINDOW_TOOLBAR_MENU_ITEMS[4],
  WINDOW_PANEL_MENU_ITEMS[8],
  WINDOW_PANEL_MENU_ITEMS[9],
  WINDOW_TOOLBAR_MENU_ITEMS[5],
] as const;

export const WINDOW_MENU_COMMAND_ORDER = [
  APP_COMMANDS.WINDOW_RESET_LAYOUT,
  APP_COMMANDS.WINDOW_PREVIEW,
  APP_COMMANDS.WINDOW_ZOOM_TO_PAGE,
  APP_COMMANDS.WINDOW_ZOOM_IN,
  APP_COMMANDS.WINDOW_ZOOM_OUT,
  APP_COMMANDS.WINDOW_FRAME_SELECTION,
  ...WINDOW_VIEW_STYLE_ITEMS.map((item) => item.commandId),
  APP_COMMANDS.WINDOW_TOGGLE_WIREFRAME_FILLED,
  APP_COMMANDS.WINDOW_SIDE_PANELS,
  ...WINDOW_PANEL_TOOLBAR_MENU_ITEMS.map((item) => item.commandId),
] as const satisfies ReadonlyArray<AppCommandId>;

export const WINDOW_VIEW_STYLE_BY_COMMAND = Object.fromEntries(
  WINDOW_VIEW_STYLE_ITEMS.map((item) => [item.commandId, item.viewStyle]),
) as Partial<Record<AppCommandId, ViewStyle>>;

export const WINDOW_PANEL_BY_COMMAND = Object.fromEntries(
  WINDOW_PANEL_MENU_ITEMS.map((item) => [item.commandId, item.panelId]),
) as Partial<Record<AppCommandId, string>>;

export const WINDOW_TOOLBAR_BY_COMMAND = Object.fromEntries(
  WINDOW_TOOLBAR_MENU_ITEMS.map((item) => [item.commandId, item.toolbarId]),
) as Partial<Record<AppCommandId, ToolbarId>>;
