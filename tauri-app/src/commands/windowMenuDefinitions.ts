import { APP_COMMANDS, type AppCommandId } from './appCommandIds';
import type { ToolbarId } from '../panels';
import type { ArtworkDisplayMode } from '../stores/uiStore';

export const WINDOW_ARTWORK_DISPLAY_ITEMS = [
  { label: 'By Layer Operation', commandId: APP_COMMANDS.WINDOW_ARTWORK_DISPLAY_BY_LAYER, mode: 'by_layer' },
  { label: 'Wireframe Override', commandId: APP_COMMANDS.WINDOW_ARTWORK_DISPLAY_WIREFRAME, mode: 'wireframe' },
  { label: 'Filled Override', commandId: APP_COMMANDS.WINDOW_ARTWORK_DISPLAY_FILLED, mode: 'filled' },
] as const satisfies ReadonlyArray<{ label: string; commandId: AppCommandId; mode: ArtworkDisplayMode }>;

export const WINDOW_PANEL_MENU_ITEMS = [
  { label: 'Art Library', commandId: APP_COMMANDS.WINDOW_PANEL_ART_LIBRARY, panelId: 'art_library' },
  { label: 'Camera Control', commandId: APP_COMMANDS.WINDOW_PANEL_CAMERA_CONTROL, panelId: 'camera' },
  { label: 'Console', commandId: APP_COMMANDS.WINDOW_PANEL_CONSOLE, panelId: 'console' },
  { label: 'Macros', commandId: APP_COMMANDS.WINDOW_PANEL_MACROS, panelId: 'macros' },
  { label: 'Layers', commandId: APP_COMMANDS.WINDOW_PANEL_CUTS_LAYERS, panelId: 'cuts_layers' },
  { label: 'Laser Control', commandId: APP_COMMANDS.WINDOW_PANEL_LASER, panelId: 'laser' },
  { label: 'Material Library', commandId: APP_COMMANDS.WINDOW_PANEL_MATERIAL_LIBRARY, panelId: 'material' },
  { label: 'Move', commandId: APP_COMMANDS.WINDOW_PANEL_MOVE, panelId: 'move' },
  { label: 'Project Notes', commandId: APP_COMMANDS.WINDOW_PANEL_NOTES, panelId: 'notes' },
  { label: 'Properties', commandId: APP_COMMANDS.WINDOW_PANEL_SHAPE_PROPERTIES, panelId: 'properties' },
] as const satisfies ReadonlyArray<{ label: string; commandId: AppCommandId; panelId: string }>;

export const WINDOW_TOOLBAR_MENU_ITEMS = [
  { label: 'Arrange', commandId: APP_COMMANDS.WINDOW_TOOLBAR_ARRANGE, toolbarId: 'arrange' },
  { label: 'Arrange (Long)', commandId: APP_COMMANDS.WINDOW_TOOLBAR_ARRANGE_LONG, toolbarId: 'arrangeLong' },
  { label: 'Modifiers', commandId: APP_COMMANDS.WINDOW_TOOLBAR_MODIFIERS, toolbarId: 'modifiers' },
  { label: 'Docking', commandId: APP_COMMANDS.WINDOW_TOOLBAR_DOCKING, toolbarId: 'docking' },
  { label: 'Main', commandId: APP_COMMANDS.WINDOW_TOOLBAR_MAIN, toolbarId: 'main' },
  { label: 'Tools', commandId: APP_COMMANDS.WINDOW_TOOLBAR_TOOLS, toolbarId: 'tools' },
] as const satisfies ReadonlyArray<{ label: string; commandId: AppCommandId; toolbarId: ToolbarId }>;

export const WINDOW_MENU_COMMAND_ORDER = [
  APP_COMMANDS.WINDOW_RESET_LAYOUT,
  APP_COMMANDS.WINDOW_PREVIEW,
  APP_COMMANDS.WINDOW_REFRESH_PREVIEW,
  APP_COMMANDS.WINDOW_ZOOM_TO_PAGE,
  APP_COMMANDS.WINDOW_ZOOM_IN,
  APP_COMMANDS.WINDOW_ZOOM_OUT,
  APP_COMMANDS.WINDOW_FRAME_SELECTION,
  ...WINDOW_ARTWORK_DISPLAY_ITEMS.map((item) => item.commandId),
  APP_COMMANDS.WINDOW_SMOOTH_EDGES,
  APP_COMMANDS.WINDOW_TOGGLE_OPERATION_WIREFRAME,
  APP_COMMANDS.WINDOW_SIDE_PANELS,
  ...WINDOW_PANEL_MENU_ITEMS.map((item) => item.commandId),
  ...WINDOW_TOOLBAR_MENU_ITEMS.map((item) => item.commandId),
] as const satisfies ReadonlyArray<AppCommandId>;

export const WINDOW_ARTWORK_DISPLAY_BY_COMMAND = Object.fromEntries(
  WINDOW_ARTWORK_DISPLAY_ITEMS.map((item) => [item.commandId, item.mode]),
) as Partial<Record<AppCommandId, ArtworkDisplayMode>>;

export const WINDOW_PANEL_BY_COMMAND = Object.fromEntries(
  WINDOW_PANEL_MENU_ITEMS.map((item) => [item.commandId, item.panelId]),
) as Partial<Record<AppCommandId, string>>;

export const WINDOW_TOOLBAR_BY_COMMAND = Object.fromEntries(
  WINDOW_TOOLBAR_MENU_ITEMS.map((item) => [item.commandId, item.toolbarId]),
) as Partial<Record<AppCommandId, ToolbarId>>;
