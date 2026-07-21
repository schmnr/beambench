// Visual constants for canvas rendering

/** At 100% zoom, 1mm = this many screen pixels */
export const BASE_PX_PER_MM = 2.0;

// --- Colors ---
export const CANVAS_BG = '#484848';
export const BED_FILL = '#2a2a2a';
export const BED_STROKE = '#606060';
export const GRID_LINE = '#555555';
export const GRID_LINE_MAJOR = '#666666';
export const ORIGIN_COLOR = '#ff4444';
export const SELECTION_STROKE = '#22c0ee';
export const SELECTION_FILL = 'rgba(34, 192, 238, 0.08)';
export const RUBBER_BAND_STROKE = '#22c0ee';
export const RUBBER_BAND_FILL = 'rgba(34, 192, 238, 0.1)';
export const HANDLE_FILL = '#ffffff';
export const HANDLE_STROKE = '#22c0ee';
export const SHAPE_PREVIEW_STROKE = '#22c0ee';
export const SHAPE_PREVIEW_FILL = 'rgba(34, 192, 238, 0.15)';
export const RASTER_PLACEHOLDER_FILL = '#3a3a3a';
export const RASTER_PLACEHOLDER_STROKE = '#707070';

// --- Sizes (screen pixels, constant regardless of zoom) ---
export const HANDLE_SIZE = 8;
export const HANDLE_HIT_SIZE = 12;
export const ROTATION_CORNER_OFFSET = 10;
export const SHEAR_HANDLE_OFFSET = 14;
export const ROTATION_ARC_RADIUS = 12;
export const ROTATION_SNAP_SHIFT_DEG = 15;
export const ROTATION_SNAP_CTRL_DEG = 5;
export const SELECTION_DASH = [6, 4];
export const ORIGIN_SIZE = 12;
export const ORIGIN_LINE_WIDTH = 1.5;
export const BED_LINE_WIDTH = 1.5;

// --- Grid ---
export const DEFAULT_GRID_SPACING_MM = 10;
export const MAJOR_GRID_INTERVAL = 5; // every 5th line is major
export const MIN_GRID_SCREEN_PX = 8; // don't draw grid lines closer than this
export const MIN_INCH_GRID_SCREEN_PX = 20; // inch-mode needs a higher threshold so lines are visible

// --- Rulers ---
export const RULER_SIZE = 20;

// --- Zoom ---
export const MIN_ZOOM = 10;
export const MAX_ZOOM = 800;
export const ZOOM_FACTOR = 1.1;
export const ZOOM_STEP = 25;

// --- Drag ---
export const DRAG_THRESHOLD_PX = 3;

// --- Snap ---
/** Screen-pixel threshold for object-to-object snapping */
export const SNAP_THRESHOLD_PX = 5;
export const OBJECT_SNAP_PX = SNAP_THRESHOLD_PX;
export const SNAP_GUIDE_COLOR = '#ff4444';
export const SNAP_GUIDE_ALPHA = 0.6;

// --- Node editing ---
export const NODE_SIZE = 7;
export const NODE_HIT_SIZE = 10;
export const NODE_FILL = '#ffffff';
export const NODE_STROKE = '#22c0ee';
export const NODE_SELECTED_FILL = '#22c0ee';
export const NODE_SELECTED_STROKE = '#ffffff';
export const HANDLE_POINT_SIZE = 5;
export const HANDLE_POINT_FILL = '#ffffff';
export const HANDLE_POINT_STROKE = '#888888';
export const HANDLE_LINE_STROKE = '#888888';
export const HANDLE_LINE_WIDTH = 0.75;

// --- Line preview ---
export const LINE_PREVIEW_STROKE = '#22c0ee';

// --- Measure ---
export const MEASURE_LINE_STROKE = '#ffd43b';
export const MEASURE_TEXT_COLOR = '#ffd43b';

// --- Crossing selection ---
export const CROSSING_RUBBER_BAND_FILL = 'rgba(107, 255, 107, 0.1)';
export const CROSSING_RUBBER_BAND_STROKE = '#69db7c';

// --- Theme system ---
export interface CanvasTheme {
  canvasBg: string;
  bedFill: string;
  bedStroke: string;
  gridLine: string;
  gridLineMajor: string;
  selectionStroke: string;
  selectionFill: string;
  rubberBandStroke: string;
  rubberBandFill: string;
  handleFill: string;
  handleStroke: string;
  rasterPlaceholderFill: string;
  rasterPlaceholderStroke: string;
  rulerBg: string;
  rulerText: string;
  rulerTick: string;
}

export const DARK_THEME: CanvasTheme = {
  canvasBg: CANVAS_BG,
  bedFill: BED_FILL,
  bedStroke: BED_STROKE,
  gridLine: GRID_LINE,
  gridLineMajor: GRID_LINE_MAJOR,
  selectionStroke: SELECTION_STROKE,
  selectionFill: SELECTION_FILL,
  rubberBandStroke: RUBBER_BAND_STROKE,
  rubberBandFill: RUBBER_BAND_FILL,
  handleFill: HANDLE_FILL,
  handleStroke: HANDLE_STROKE,
  rasterPlaceholderFill: RASTER_PLACEHOLDER_FILL,
  rasterPlaceholderStroke: RASTER_PLACEHOLDER_STROKE,
  rulerBg: '#3a3a3a',
  rulerText: '#aaaaaa',
  rulerTick: '#666666',
};

export const LIGHT_THEME: CanvasTheme = {
  canvasBg: '#f8f8f8',
  bedFill: '#ffffff',
  bedStroke: '#cccccc',
  gridLine: '#e0e0e0',
  gridLineMajor: '#cccccc',
  selectionStroke: '#0e8cb5',
  selectionFill: 'rgba(14,140,181,0.08)',
  rubberBandStroke: '#0e8cb5',
  rubberBandFill: 'rgba(14,140,181,0.1)',
  handleFill: '#ffffff',
  handleStroke: '#0e8cb5',
  rasterPlaceholderFill: '#e8e8e8',
  rasterPlaceholderStroke: '#aaaaaa',
  rulerBg: '#f0f0f0',
  rulerText: '#888888',
  rulerTick: '#999999',
};
