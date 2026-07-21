/** Palette color types for the layer color palette. */

// backend `PaletteColor` has `is_tool_layer: bool` (non-optional).
// All 32 palette constants set this field explicitly, so a frontend mirror
// that marks it optional allowed holes the backend never produces.
export interface PaletteColor {
  hex: string;
  name: string;
  is_tool_layer: boolean;
}
