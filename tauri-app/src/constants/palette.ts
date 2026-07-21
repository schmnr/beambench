import type { PaletteColor } from '../types/palette';

// Must match crates/beambench-common/src/palette.rs PALETTE_COLORS exactly.
// The backend auto-assigns colors by index when creating layers, so
// frontend and backend must agree on hex values and ordering.
// `is_tool_layer` is non-optional; standard colors set it to false,
// tool colors set it to true.
export const PALETTE_COLORS: readonly PaletteColor[] = [
  { hex: '#000000', name: 'Black', is_tool_layer: false },
  { hex: '#FF0000', name: 'Red', is_tool_layer: false },
  { hex: '#00FF00', name: 'Green', is_tool_layer: false },
  { hex: '#0000FF', name: 'Blue', is_tool_layer: false },
  { hex: '#00FFFF', name: 'Cyan', is_tool_layer: false },
  { hex: '#FF00FF', name: 'Magenta', is_tool_layer: false },
  { hex: '#FFFF00', name: 'Yellow', is_tool_layer: false },
  { hex: '#FF8000', name: 'Orange', is_tool_layer: false },
  { hex: '#FBB6F0', name: 'Lilac', is_tool_layer: false },
  { hex: '#2EB88A', name: 'Sea Green', is_tool_layer: false },
  { hex: '#FF0080', name: 'Pink', is_tool_layer: false },
  { hex: '#93B946', name: 'Moss', is_tool_layer: false },
  { hex: '#0080FF', name: 'Sky Blue', is_tool_layer: false },
  { hex: '#804000', name: 'Brown', is_tool_layer: false },
  { hex: '#800000', name: 'Maroon', is_tool_layer: false },
  { hex: '#008000', name: 'Dark Green', is_tool_layer: false },
  { hex: '#000080', name: 'Navy', is_tool_layer: false },
  { hex: '#808000', name: 'Olive', is_tool_layer: false },
  { hex: '#008080', name: 'Dark Cyan', is_tool_layer: false },
  { hex: '#800080', name: 'Dark Magenta', is_tool_layer: false },
  { hex: '#FF8080', name: 'Coral', is_tool_layer: false },
  { hex: '#D1F0C2', name: 'Pale Green', is_tool_layer: false },
  { hex: '#987ECE', name: 'Violet', is_tool_layer: false },
  { hex: '#EFCF8F', name: 'Sand', is_tool_layer: false },
  { hex: '#314C81', name: 'Steel Blue', is_tool_layer: false },
  { hex: '#5C2336', name: 'Plum', is_tool_layer: false },
  { hex: '#808080', name: 'Gray', is_tool_layer: false },
  { hex: '#C0C0C0', name: 'Light Gray', is_tool_layer: false },
  { hex: '#404040', name: 'Dark Gray', is_tool_layer: false },
  { hex: '#B8860B', name: 'Gold', is_tool_layer: false },
  { hex: '#DA0B3F', name: 'Tool 1', is_tool_layer: true },
  { hex: '#00D4FF', name: 'Tool 2', is_tool_layer: true },
] as const;

/**
 * Lbrn's fixed layer palette. Its C## indices do not use the same color
 * order as Beam Bench's native palette, so imported layers need this table to
 * keep their source label and swatch number recognizable in the layer list.
 * Keep this in sync with LBRN_PALETTE_COLORS in the import service.
 */
export const LBRN_PALETTE_COLORS: readonly string[] = [
  '#000000', '#0000FF', '#FF0000', '#00E000', '#D0D000', '#FF8000', '#00E0E0', '#FF00FF',
  '#B4B4B4', '#0000A0', '#A00000', '#00A000', '#A0A000', '#C08000', '#00A0FF', '#A000A0',
  '#808080', '#7D87B9', '#BB7784', '#4A6FE3', '#D33F6A', '#8CD78C', '#F0B98D', '#F6C4E1',
  '#FA9ED4', '#500A78', '#B45A00', '#004754', '#86FA88', '#FFDB66', '#F36926', '#0C96D9',
] as const;
