// Short, user-facing descriptions for each raster-mode choice surfaced in
// the image-layer settings UI. These are discoverability help — a one-line
// summary of what the mode does and when it is the right pick. They are NOT
// a substitute for actual docs.
//
// Keep these honest about what is approximate. We currently collapse
// Halftone and Newsprint onto the OrderedDither planner path, so the help
// text for those two notes that they are ordered-dither presets.

import type { RasterMode } from '../../types/project';

export const RASTER_MODE_HELP_KEYS: Record<RasterMode, string> = {
  grayscale: 'panels.dither_sample.help.grayscale',
  threshold: 'panels.dither_sample.help.threshold',
  floyd_steinberg: 'panels.dither_sample.help.floyd_steinberg',
  ordered_dither: 'panels.dither_sample.help.ordered_dither',
  stucki: 'panels.dither_sample.help.stucki',
  jarvis: 'panels.dither_sample.help.jarvis',
  sierra: 'panels.dither_sample.help.sierra',
  atkinson: 'panels.dither_sample.help.atkinson',
  halftone: 'panels.dither_sample.help.halftone',
  newsprint: 'panels.dither_sample.help.newsprint',
  sketch: 'panels.dither_sample.help.sketch',
};
