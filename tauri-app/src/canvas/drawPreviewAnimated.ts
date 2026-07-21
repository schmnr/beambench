import type { Point2D } from '../types/project';
import type { RasterRunExtent } from '../types/preview';
import type { ViewportParams } from './ViewportTransform';
import { worldToScreen, worldToScreenDist, worldBoundsToScreenRect } from './ViewportTransform';
import {
  DEFAULT_RAPID_SPEED_MM_MIN,
} from './previewTimeline';
import {
  applyRasterBitmapTransform,
  drawPerRunOverscanMarkers,
  drawRasterRunStripes,
} from './drawPreview';
import { plannerStripPointToWorld } from './rasterTransform';
import type {
  AnimationTimeline,
  VectorSegment,
  RasterSegment,
  TravelSegment,
  FrameSegment,
} from './previewTimeline';
import type { PreviewBitmapCache } from './previewBitmapCache';

// Style constants
const TRAVEL_DASH = [4, 4];
const FRAME_DASH = [6, 3];
const HEAD_DOT_RADIUS = 4;
const HEAD_DOT_COLOR = '#ff4444';

export interface InterpolateResult {
  partialPoints: Point2D[];
  headPos: Point2D;
}

/**
 * Walk along a polyline by cumulative length to find the cutoff at `fraction` (0..1).
 * Returns partial points up to the cutoff and the exact head position.
 */
export function interpolatePolyline(
  points: Point2D[],
  fraction: number,
): InterpolateResult {
  if (points.length === 0) {
    return { partialPoints: [], headPos: { x: 0, y: 0 } };
  }
  if (points.length === 1 || fraction <= 0) {
    return { partialPoints: [points[0]], headPos: points[0] };
  }
  if (fraction >= 1) {
    return { partialPoints: [...points], headPos: points[points.length - 1] };
  }

  // Compute total length
  let totalLen = 0;
  for (let i = 1; i < points.length; i++) {
    const dx = points[i].x - points[i - 1].x;
    const dy = points[i].y - points[i - 1].y;
    totalLen += Math.sqrt(dx * dx + dy * dy);
  }

  const targetLen = fraction * totalLen;
  let accumulated = 0;
  const result: Point2D[] = [points[0]];

  for (let i = 1; i < points.length; i++) {
    const dx = points[i].x - points[i - 1].x;
    const dy = points[i].y - points[i - 1].y;
    const segLen = Math.sqrt(dx * dx + dy * dy);

    if (accumulated + segLen >= targetLen) {
      // Cutoff is within this segment
      const remaining = targetLen - accumulated;
      const t = segLen > 0 ? remaining / segLen : 0;
      const headPos = {
        x: points[i - 1].x + dx * t,
        y: points[i - 1].y + dy * t,
      };
      result.push(headPos);
      return { partialPoints: result, headPos };
    }

    accumulated += segLen;
    result.push(points[i]);
  }

  // Should not reach here, but safety fallback
  return { partialPoints: result, headPos: points[points.length - 1] };
}

export interface RasterRunProgress {
  /** Runs fully or partially completed at this progress fraction. The
   *  last entry may have `partialFrac` in (0, 1) for the active run;
   *  all earlier entries are complete. */
  visibleRuns: RasterRunExtent[];
  /** Fraction (0..1) of the last visible run that has been burned. */
  lastRunPartialFrac: number;
  /** Head position derived from the run cutoff point (world coordinates). */
  headPos: Point2D;
  /** Index of the last fully-completed run in the original run_extents
   *  array (not counting the partial active run). -1 if none. */
  lastCompleteIndex: number;
}

/**
 * Compute raster head position from segment geometry and progress,
 * independent of tone-strip data.  Models the actual emitter motion:
 *
 * - Y (advance axis) **steps** per scanline rather than interpolating smoothly.
 * - Bidirectional: alternating LTR / RTL scan across the full scan extent.
 * - Unidirectional: LTR scan then rapid return to next line start.
 *   The last line has no return phase.
 * - Scan extent includes overscan (bounds already expanded by distiller).
 *
 * @param speedMmMin  Burn speed — used together with DEFAULT_RAPID_SPEED_MM_MIN
 *                    to compute the scan/return time split for unidirectional.
 *                    Pass 0 to skip return modelling (bidirectional fallback).
 */
export function computeRasterHeadPos(
  bounds: { min: Point2D; max: Point2D },
  lineCount: number,
  directionMode: 'unidirectional' | 'bidirectional',
  progress: number,
  scanAxis: 'horizontal' | 'vertical' | undefined,
  speedMmMin: number = 0,
): Point2D {
  const lc = lineCount || 1;
  const p = Math.max(0, Math.min(1, progress));
  const isVertical = scanAxis === 'vertical';

  // Scan axis: the direction the head sweeps per line.
  // Advance axis: the direction lines step along.
  const scanMin = isVertical ? bounds.min.y : bounds.min.x;
  const scanMax = isVertical ? bounds.max.y : bounds.max.x;
  const scanExtent = scanMax - scanMin;
  const advMin = isVertical ? bounds.min.x : bounds.min.y;
  const advMax = isVertical ? bounds.max.x : bounds.max.y;
  const advExtent = advMax - advMin;

  // Unidirectional: each line cycle = scan + rapid-return.
  // Compute the fraction of cycle time spent scanning.
  let scanFrac = 1; // bidirectional: full cycle is scan
  if (directionMode === 'unidirectional' && speedMmMin > 0 && scanExtent > 0) {
    const scanTime = scanExtent / speedMmMin;
    const returnTime = scanExtent / DEFAULT_RAPID_SPEED_MM_MIN;
    const cycleTime = scanTime + returnTime;
    scanFrac = cycleTime > 0 ? scanTime / cycleTime : 1;
  }

  // Map overall progress to line + position within line cycle.
  const lineProgress = p * lc;
  const currentLine = Math.min(Math.floor(lineProgress), lc - 1);
  const withinLine = lineProgress - currentLine;

  // Advance-axis position: fixed per line (stepped, not smooth).
  const lineAdv = lc > 1
    ? advMin + (currentLine / (lc - 1)) * advExtent
    : advMin + advExtent * 0.5;
  const nextLineAdv = lc > 1 && currentLine < lc - 1
    ? advMin + ((currentLine + 1) / (lc - 1)) * advExtent
    : lineAdv;

  let scanPos: number;
  let advPos: number;

  const isLastLine = currentLine >= lc - 1;

  if (directionMode === 'bidirectional') {
    // Even lines: scanMin → scanMax, odd lines: scanMax → scanMin.
    const isForward = currentLine % 2 === 0;
    scanPos = isForward
      ? scanMin + withinLine * scanExtent
      : scanMax - withinLine * scanExtent;
    advPos = lineAdv;
  } else {
    // Unidirectional: scan LTR then rapid return (except last line).
    const effectiveScanFrac = isLastLine ? 1 : scanFrac;

    if (withinLine <= effectiveScanFrac) {
      // Scan phase: head sweeps from scanMin to scanMax.
      const phaseFrac = effectiveScanFrac > 0
        ? withinLine / effectiveScanFrac : 1;
      scanPos = scanMin + phaseFrac * scanExtent;
      advPos = lineAdv;
    } else {
      // Return phase: head rapids from scanMax back to scanMin,
      // advancing to the next line simultaneously.
      const returnFrac = 1 - effectiveScanFrac;
      const phaseFrac = returnFrac > 0
        ? (withinLine - effectiveScanFrac) / returnFrac : 1;
      scanPos = scanMax - phaseFrac * scanExtent;
      advPos = lineAdv + phaseFrac * (nextLineAdv - lineAdv);
    }
  }

  // Map back to world coordinates.
  if (isVertical) {
    return { x: advPos, y: scanPos };
  }
  return { x: scanPos, y: advPos };
}

/**
 * Map a progress fraction (0..1) to a set of visible tone strips,
 * weighted by cumulative physical distance (|end_x_mm - start_x_mm|) so that
 * longer strips receive proportionally more playback time.
 *
 * Returns `headPos` derived from the strip cutoff point so the head follows
 * the actual run structure (including gaps between multi-run scanlines)
 * rather than sweeping through blank space.
 *
 * Accepts either the legacy boolean vertical flag or full raster transform
 * metadata so rotated rasters can resolve head position in world space.
 */
export function interpolateRasterRuns(
  runs: RasterRunExtent[],
  progress: number,
  transform:
    | boolean
    | {
      scanAxis?: 'horizontal' | 'vertical';
      scanAngleDeg?: number;
      scanOrigin?: Point2D;
    } = false,
): RasterRunProgress {
  const scanAxis = typeof transform === 'boolean'
    ? (transform ? 'vertical' : 'horizontal')
    : transform.scanAxis;
  const scanAngleDeg = typeof transform === 'boolean' ? 0 : transform.scanAngleDeg;
  const scanOrigin = typeof transform === 'boolean' ? undefined : transform.scanOrigin;
  const originHead: Point2D = { x: 0, y: 0 };
  if (runs.length === 0 || progress <= 0) {
    return { visibleRuns: [], lastRunPartialFrac: 0, headPos: originHead, lastCompleteIndex: -1 };
  }

  let totalDist = 0;
  for (const r of runs) {
    totalDist += Math.abs(r.end_x_mm - r.start_x_mm);
  }

  if (totalDist <= 0 || progress >= 1) {
    const last = runs[runs.length - 1];
    const headPos = plannerStripPointToWorld(last.y_mm, last.end_x_mm, {
      scanAxis,
      scanAngleDeg,
      scanOrigin,
    });
    return {
      visibleRuns: [...runs],
      lastRunPartialFrac: 1,
      headPos,
      lastCompleteIndex: runs.length - 1,
    };
  }

  const targetDist = progress * totalDist;
  let accumulated = 0;
  const visible: RasterRunExtent[] = [];
  let lastComplete = -1;

  for (let i = 0; i < runs.length; i++) {
    const r = runs[i];
    const len = Math.abs(r.end_x_mm - r.start_x_mm);

    if (accumulated + len >= targetDist) {
      const remaining = targetDist - accumulated;
      const frac = len > 0 ? remaining / len : 1;
      const cutoff = r.start_x_mm + (r.end_x_mm - r.start_x_mm) * frac;
      visible.push({ ...r, end_x_mm: cutoff });
      const headPos = plannerStripPointToWorld(r.y_mm, cutoff, {
        scanAxis,
        scanAngleDeg,
        scanOrigin,
      });
      return { visibleRuns: visible, lastRunPartialFrac: frac, headPos, lastCompleteIndex: lastComplete };
    }

    accumulated += len;
    visible.push(r);
    lastComplete = i;
  }

  const last = runs[runs.length - 1];
  const headPos = plannerStripPointToWorld(last.y_mm, last.end_x_mm, {
    scanAxis,
    scanAngleDeg,
    scanOrigin,
  });
  return {
    visibleRuns: visible,
    lastRunPartialFrac: 1,
    headPos,
    lastCompleteIndex: runs.length - 1,
  };
}

export interface AnimatedPreviewOptions {
  showTravel: boolean;
  showBurnProgress: boolean;
  showOverscan: boolean;
  shadeByPower: boolean;
  invertView: boolean;
}

export function drawAnimatedPreview(
  ctx: CanvasRenderingContext2D,
  timeline: AnimationTimeline,
  currentTime: number,
  vp: ViewportParams,
  options: AnimatedPreviewOptions,
  bitmapCache: PreviewBitmapCache,
): void {
  ctx.save();

  // --- Draw segments based on current time ---
  let headPos: Point2D | null = null;

  for (const seg of timeline.segments) {
    if (seg.startTime > currentTime) {
      // Future segment — skip
      continue;
    }

    switch (seg.type) {
      case 'vector':
        drawVectorSegment(ctx, seg, currentTime, vp, options, (pos) => {
          headPos = pos;
        });
        break;
      case 'raster':
        drawRasterSegment(ctx, seg, currentTime, vp, options, bitmapCache, (pos) => {
          headPos = pos;
        });
        break;
      case 'travel':
        if (options.showTravel) {
          drawTravelSegment(ctx, seg, currentTime, vp, options, (pos) => {
            headPos = pos;
          });
        }
        break;
      case 'frame':
        drawFrameSegment(ctx, seg, currentTime, vp, options, (pos) => {
          headPos = pos;
        });
        break;
    }
  }

  // --- Draw head dot ---
  if (options.showBurnProgress && headPos && currentTime > 0) {
    const screenHead = worldToScreen(headPos, vp);
    ctx.fillStyle = HEAD_DOT_COLOR;
    ctx.beginPath();
    ctx.arc(screenHead.x, screenHead.y, HEAD_DOT_RADIUS, 0, Math.PI * 2);
    ctx.fill();
  }

  ctx.restore();
}

function drawTravelSegment(
  ctx: CanvasRenderingContext2D,
  seg: TravelSegment,
  currentTime: number,
  vp: ViewportParams,
  options: AnimatedPreviewOptions,
  setHead: (pos: Point2D) => void,
): void {
  const isComplete = seg.endTime <= currentTime;
  const isInProgress = !isComplete && seg.startTime <= currentTime;

  const from = worldToScreen(seg.from, vp);
  const to = worldToScreen(seg.to, vp);

  const travelColor = options.invertView
    ? 'rgba(200, 200, 200, 0.35)'
    : 'rgba(128, 128, 128, 0.35)';
  ctx.strokeStyle = travelColor;
  ctx.lineWidth = 0.75;
  ctx.setLineDash(TRAVEL_DASH);

  if (isComplete) {
    ctx.beginPath();
    ctx.moveTo(from.x, from.y);
    ctx.lineTo(to.x, to.y);
    ctx.stroke();
    setHead(seg.to);
  } else if (isInProgress) {
    if (options.showBurnProgress) {
      // Partial travel line with head tracking
      const duration = seg.endTime - seg.startTime;
      const fraction = duration > 0 ? (currentTime - seg.startTime) / duration : 1;
      const mid = {
        x: seg.from.x + (seg.to.x - seg.from.x) * fraction,
        y: seg.from.y + (seg.to.y - seg.from.y) * fraction,
      };
      const midScreen = worldToScreen(mid, vp);
      ctx.beginPath();
      ctx.moveTo(from.x, from.y);
      ctx.lineTo(midScreen.x, midScreen.y);
      ctx.stroke();
      setHead(mid);
    } else {
      // Draw full travel line without partial cutoff
      ctx.beginPath();
      ctx.moveTo(from.x, from.y);
      ctx.lineTo(to.x, to.y);
      ctx.stroke();
    }
  }

  ctx.setLineDash([]);
}

function drawFrameSegment(
  ctx: CanvasRenderingContext2D,
  seg: FrameSegment,
  currentTime: number,
  vp: ViewportParams,
  options: AnimatedPreviewOptions,
  setHead: (pos: Point2D) => void,
): void {
  if (seg.path.length < 2) return;

  const isComplete = seg.endTime <= currentTime;
  const isInProgress = !isComplete && seg.startTime <= currentTime;

  const frameColor = options.invertView
    ? 'rgba(0, 200, 200, 0.6)'
    : 'rgba(0, 160, 160, 0.5)';
  ctx.strokeStyle = frameColor;
  ctx.lineWidth = 1.5;
  ctx.setLineDash(FRAME_DASH);

  if (isComplete) {
    ctx.beginPath();
    const first = worldToScreen(seg.path[0], vp);
    ctx.moveTo(first.x, first.y);
    for (let i = 1; i < seg.path.length; i++) {
      const p = worldToScreen(seg.path[i], vp);
      ctx.lineTo(p.x, p.y);
    }
    ctx.stroke();
  } else if (isInProgress) {
    if (options.showBurnProgress) {
      // Partial frame path with head tracking
      const duration = seg.endTime - seg.startTime;
      const fraction = duration > 0 ? (currentTime - seg.startTime) / duration : 1;
      const { partialPoints, headPos: head } = interpolatePolyline(seg.path, fraction);

      if (partialPoints.length >= 2) {
        ctx.beginPath();
        const first = worldToScreen(partialPoints[0], vp);
        ctx.moveTo(first.x, first.y);
        for (let i = 1; i < partialPoints.length; i++) {
          const p = worldToScreen(partialPoints[i], vp);
          ctx.lineTo(p.x, p.y);
        }
        ctx.stroke();
      }

      setHead(head);
    } else {
      // Draw full frame path without partial cutoff
      ctx.beginPath();
      const first = worldToScreen(seg.path[0], vp);
      ctx.moveTo(first.x, first.y);
      for (let i = 1; i < seg.path.length; i++) {
        const p = worldToScreen(seg.path[i], vp);
        ctx.lineTo(p.x, p.y);
      }
      ctx.stroke();
    }
  }

  ctx.setLineDash([]);
}

function drawVectorSegment(
  ctx: CanvasRenderingContext2D,
  seg: VectorSegment,
  currentTime: number,
  vp: ViewportParams,
  options: AnimatedPreviewOptions,
  setHead: (pos: Point2D) => void,
): void {
  if (seg.points.length < 2) return;

  const isComplete = seg.endTime <= currentTime;
  const isInProgress = !isComplete && seg.startTime <= currentTime;

  const baseColor = options.invertView ? '#ffffff' : '#000000';
  ctx.strokeStyle = baseColor;
  if (options.shadeByPower) {
    // power 0–100 maps to opacity 0.1–1.0
    ctx.globalAlpha = 0.1 + (seg.powerPercent / 100) * 0.9;
  } else {
    ctx.globalAlpha = 1.0;
  }
  ctx.lineWidth = 1.5;

  if (isComplete) {
    // Draw full path
    ctx.beginPath();
    const first = worldToScreen(seg.points[0], vp);
    ctx.moveTo(first.x, first.y);
    for (let i = 1; i < seg.points.length; i++) {
      const p = worldToScreen(seg.points[i], vp);
      ctx.lineTo(p.x, p.y);
    }
    if (seg.closed) ctx.closePath();
    ctx.stroke();
  } else if (isInProgress) {
    if (options.showBurnProgress) {
      // Draw partial path with head tracking
      const duration = seg.endTime - seg.startTime;
      const fraction = duration > 0 ? (currentTime - seg.startTime) / duration : 1;
      const { partialPoints, headPos: head } = interpolatePolyline(seg.points, fraction);

      if (partialPoints.length >= 2) {
        ctx.beginPath();
        const first = worldToScreen(partialPoints[0], vp);
        ctx.moveTo(first.x, first.y);
        for (let i = 1; i < partialPoints.length; i++) {
          const p = worldToScreen(partialPoints[i], vp);
          ctx.lineTo(p.x, p.y);
        }
        ctx.stroke();
      }

      setHead(head);
    } else {
      // Draw full path without partial cutoff
      ctx.beginPath();
      const first = worldToScreen(seg.points[0], vp);
      ctx.moveTo(first.x, first.y);
      for (let i = 1; i < seg.points.length; i++) {
        const p = worldToScreen(seg.points[i], vp);
        ctx.lineTo(p.x, p.y);
      }
      if (seg.closed) ctx.closePath();
      ctx.stroke();
    }
  }

  ctx.globalAlpha = 1;
}

function drawRasterSegment(
  ctx: CanvasRenderingContext2D,
  seg: RasterSegment,
  currentTime: number,
  vp: ViewportParams,
  options: AnimatedPreviewOptions,
  bitmapCache: PreviewBitmapCache,
  setHead: (pos: Point2D) => void,
): void {
  const isComplete = seg.endTime <= currentTime;
  const isInProgress = !isComplete && seg.startTime <= currentTime;
  if (!isComplete && !isInProgress) {
    return;
  }

  const rect = worldBoundsToScreenRect(seg.bounds, vp);
  const { x, y, w, h } = rect;

  const burnColor = options.invertView ? '#ffffff' : (seg.layerColor || '#000000');
  const os = seg.overscanMm;
  const isVertical = seg.scanAxis === 'vertical';

  // Compute inner burn bounds (same as before) for overscan markers and
  // the fallback path.
  let burnRect: { x: number; y: number; w: number; h: number };
  let burnScreenW: number;
  let burnScreenH: number;
  let hasOverscan: boolean;
  let osScreenDist: number;
  if (isVertical && os > 0) {
    const burnMinY = seg.bounds.min.y + os;
    const burnMaxY = seg.bounds.max.y - os;
    const burnH = Math.max(0, burnMaxY - burnMinY);
    hasOverscan = burnH > 0;
    burnRect = worldBoundsToScreenRect({
      min: { x: seg.bounds.min.x, y: burnMinY },
      max: { x: seg.bounds.max.x, y: burnMaxY },
    }, vp);
    burnScreenW = burnRect.w;
    burnScreenH = burnRect.h;
    osScreenDist = worldToScreenDist(os, vp.zoom);
  } else if (!isVertical && os > 0) {
    const burnMinX = seg.bounds.min.x + os;
    const burnMaxX = seg.bounds.max.x - os;
    const burnW = Math.max(0, burnMaxX - burnMinX);
    hasOverscan = burnW > 0;
    burnRect = worldBoundsToScreenRect({
      min: { x: burnMinX, y: seg.bounds.min.y },
      max: { x: burnMaxX, y: seg.bounds.max.y },
    }, vp);
    burnScreenW = burnRect.w;
    burnScreenH = burnRect.h;
    osScreenDist = worldToScreenDist(os, vp.zoom);
  } else {
    hasOverscan = false;
    burnRect = rect;
    burnScreenW = w;
    burnScreenH = h;
    osScreenDist = 0;
  }

  const hasOutlines = !!(seg.outlines && seg.outlines.length > 0);

  // Attempt the bitmap path first. Requires: sequence id, bitmap,
  // local geometry, and run_extents for progress masking.
  const bitmap = seg.previewBitmap;
  const localOrigin = seg.localOriginMm;
  const localW = seg.localWidthMm ?? 0;
  const localH = seg.localHeightMm ?? 0;
  const runs = seg.runExtents ?? [];
  // Overscan is defined by the full scan row, not by each internal burn island.
  // Using grayscale sub-runs here produces false pink bands between nearby
  // features inside the same row.
  const overscanRuns = seg.scanlineExtents && seg.scanlineExtents.length > 0
    ? seg.scanlineExtents
    : runs.length > 0
      ? runs
      : (seg.overscanRunExtents ?? []);
  const sequence = seg.sequence ?? -1;
  const canUseBitmap = !!(
    bitmap
    && localOrigin
    && localW > 0
    && localH > 0
    && sequence >= 0
  );
  const preferTintedBitmapPreview = seg.preferRunPreview && canUseBitmap;
  const useOutlinedRunPreview = !preferTintedBitmapPreview && hasOutlines && runs.length > 0;

  if (useOutlinedRunPreview) {
    let interp: RasterRunProgress | null = null;
    let headPosWorld: Point2D | null = null;
    let visibleRuns = runs;

    if (isInProgress && options.showBurnProgress) {
      const duration = seg.endTime - seg.startTime;
      const progress = duration > 0 ? (currentTime - seg.startTime) / duration : 1;
      interp = interpolateRasterRuns(runs, progress, {
        scanAxis: seg.scanAxis,
        scanAngleDeg: seg.scanAngleDeg,
        scanOrigin: seg.scanOrigin,
      });
      visibleRuns = interp.visibleRuns;
      headPosWorld = interp.headPos;
    } else if (isComplete) {
      headPosWorld = seg.endPoint ?? { x: seg.bounds.max.x, y: seg.bounds.max.y };
    }

    drawRasterRunStripes(
      ctx,
      {
        line_interval_mm: seg.lineIntervalMm,
        scan_axis: seg.scanAxis,
        scan_angle_deg: seg.scanAngleDeg,
        scan_origin: seg.scanOrigin,
        outlines: seg.outlines,
      },
      visibleRuns,
      burnColor,
      vp,
      {
        showBaseTint: !seg.preferRunPreview,
        emphasizeVisibleRuns: seg.preferRunPreview,
      },
    );

    if (options.showOverscan && hasOverscan && osScreenDist > 0) {
      if (isInProgress && options.showBurnProgress) {
        if (interp && interp.visibleRuns.length > 0) {
          drawPerRunOverscanMarkers(
            ctx,
            {
              scan_axis: seg.scanAxis,
              line_interval_mm: seg.lineIntervalMm,
              scan_angle_deg: seg.scanAngleDeg,
              scan_origin: seg.scanOrigin,
              overscan_mm: seg.overscanMm,
            },
            interp.visibleRuns,
            vp,
            false,
          );
        }
      } else {
        drawPerRunOverscanMarkers(
          ctx,
          {
            scan_axis: seg.scanAxis,
            line_interval_mm: seg.lineIntervalMm,
            scan_angle_deg: seg.scanAngleDeg,
            scan_origin: seg.scanOrigin,
            overscan_mm: seg.overscanMm,
          },
          runs,
          vp,
          true,
        );
      }
    }

    if (!isInProgress) {
      ctx.globalAlpha = 0.5;
      ctx.strokeStyle = burnColor;
      ctx.lineWidth = 1;
      ctx.strokeRect(x, y, w, h);
    }

    if (headPosWorld) {
      setHead(headPosWorld);
    }
    ctx.globalAlpha = 1;
    return;
  }

  if (canUseBitmap) {
    const img = bitmapCache.ensurePreviewBitmap(sequence, bitmap!.png_bytes);
    if (img && img.complete && img.naturalWidth > 0) {
      const mask = bitmapCache.ensureBurnedMask(
        sequence,
        bitmap!.width_px,
        bitmap!.height_px,
      );
      const maskCtx = mask.getContext('2d');
      if (maskCtx) {
        let progress = 1;
        let headPosWorld: Point2D | null = null;
        let interp: RasterRunProgress | null = null;

        if (isInProgress) {
          const duration = seg.endTime - seg.startTime;
          progress = duration > 0 ? (currentTime - seg.startTime) / duration : 1;
        }

        // Incremental painting: each burned-mask canvas carries a
        // dataset attribute tracking the highest run index painted so
        // far. Only new runs since the last frame are drawn in, unless
        // progress moved backwards (scrubbing) in which case we reset.
        const PAINTED_KEY = 'bbPaintedCount';
        const prevCount = parseInt(
          (mask as HTMLCanvasElement).dataset[PAINTED_KEY] ?? '-1',
          10,
        );

        if (isComplete || progress >= 1) {
          // Full: paint every run exactly once (skip repainting if
          // already fully painted from a previous frame).
          if (prevCount < runs.length - 1) {
            maskCtx.clearRect(0, 0, mask.width, mask.height);
            maskCtx.drawImage(img, 0, 0);
            (mask as HTMLCanvasElement).dataset[PAINTED_KEY] = String(runs.length - 1);
          }
          headPosWorld = seg.endPoint ?? { x: seg.bounds.max.x, y: seg.bounds.max.y };
        } else {
          interp = interpolateRasterRuns(runs, progress, {
            scanAxis: seg.scanAxis,
            scanAngleDeg: seg.scanAngleDeg,
            scanOrigin: seg.scanOrigin,
          });
          const lastComplete = interp.lastCompleteIndex;

          if (lastComplete < prevCount) {
            // Backwards scrub — reset and repaint fresh.
            maskCtx.clearRect(0, 0, mask.width, mask.height);
            for (let i = 0; i <= lastComplete; i++) {
              paintRunRectToMask(
                maskCtx,
                img,
                runs[i],
                localOrigin!,
                localW,
                localH,
                bitmap!.width_px,
                bitmap!.height_px,
              );
            }
          } else {
            for (let i = prevCount + 1; i <= lastComplete; i++) {
              paintRunRectToMask(
                maskCtx,
                img,
                runs[i],
                localOrigin!,
                localW,
                localH,
                bitmap!.width_px,
                bitmap!.height_px,
              );
            }
          }
          (mask as HTMLCanvasElement).dataset[PAINTED_KEY] = String(lastComplete);

          // Active (partial) run: paint its slice up to cutoff.
          const partial = interp.visibleRuns[interp.visibleRuns.length - 1];
          if (partial && lastComplete + 1 < runs.length) {
            paintRunRectToMask(
              maskCtx,
              img,
              partial,
              localOrigin!,
              localW,
              localH,
              bitmap!.width_px,
              bitmap!.height_px,
            );
          }
          headPosWorld = interp.headPos;
        }

        // Draw overscan behind the burned bitmap so image-mode overscan follows
        // actual runs without painting pink markers over the burn itself.
        if (options.showOverscan && hasOverscan && osScreenDist > 0) {
          const usePerRunOverscan = overscanRuns.length > 0;
          if (isInProgress && options.showBurnProgress) {
            if (interp && interp.visibleRuns.length > 0) {
              const progressRuns = overscanRuns.length === runs.length
                ? interp.visibleRuns
                : overscanRuns.slice(0, interp.visibleRuns.length);
              drawPerRunOverscanMarkers(
                ctx,
                {
                  scan_axis: seg.scanAxis,
                  line_interval_mm: seg.lineIntervalMm,
                  scan_angle_deg: seg.scanAngleDeg,
                  scan_origin: seg.scanOrigin,
                  overscan_mm: seg.overscanMm,
                },
                progressRuns,
                vp,
                false,
              );
            }
          } else if (usePerRunOverscan) {
            drawPerRunOverscanMarkers(
              ctx,
              {
                scan_axis: seg.scanAxis,
                line_interval_mm: seg.lineIntervalMm,
                scan_angle_deg: seg.scanAngleDeg,
                scan_origin: seg.scanOrigin,
                overscan_mm: seg.overscanMm,
              },
              overscanRuns,
              vp,
              true,
            );
          } else {
            ctx.globalAlpha = 0.25;
            ctx.fillStyle = 'rgba(255, 60, 60, 0.6)';
            if (isVertical) {
              ctx.fillRect(x, y, w, osScreenDist);
              ctx.fillRect(x, y + h - osScreenDist, w, osScreenDist);
            } else {
              ctx.fillRect(x, y, osScreenDist, h);
              ctx.fillRect(x + w - osScreenDist, y, osScreenDist, h);
            }
          }
        }

        const displayMask = preferTintedBitmapPreview
          ? tintBurnedMask(bitmapCache, sequence, mask, bitmap!.width_px, bitmap!.height_px, burnColor)
          : mask;

        // Composite the burned mask onto the main canvas with the same
        // local->world transform as the static bitmap path. The mask is the
        // planner's exact burn footprint — never clip it to the outlines,
        // which are simplified for IPC and can collapse thin slivers the
        // laser still burns (leaving overscan markers stranded in white).
        ctx.save();
        const angleDeg = seg.scanAngleDeg ?? 0;
        const origin = seg.scanOrigin ?? { x: 0, y: 0 };
        const scale = applyRasterBitmapTransform(ctx, origin, angleDeg, isVertical, vp);
        ctx.imageSmoothingEnabled = shouldSmoothPreviewBitmap(
          bitmap!.width_px,
          bitmap!.height_px,
          localW,
          localH,
          scale,
        );
        ctx.drawImage(displayMask, localOrigin!.x, localOrigin!.y, localW, localH);
        ctx.restore();

        // Completed regions keep a light outline; in-progress raster playback
        // should not show a synthetic dashed border ahead of the head.
        if (!isInProgress) {
          ctx.globalAlpha = 0.5;
          ctx.strokeStyle = burnColor;
          ctx.lineWidth = 1;
          ctx.strokeRect(x, y, w, h);
        }

        if (headPosWorld) {
          setHead(headPosWorld);
        }
        ctx.globalAlpha = 1;
        return;
      }
    }
  }

  // --- Legacy fallback: no bitmap available (e.g. still loading).
  // Uses the old rectangular sweep so the region is visible during
  // decode. Head position comes from computeRasterHeadPos.
  if (isComplete) {
    if (hasOutlines) {
      ctx.save();
      buildClipPath(ctx, seg.outlines!, vp);
      ctx.clip('evenodd');
    }
    const powerFactor = seg.avgPowerNormalized ?? 1.0;
    ctx.fillStyle = burnColor;
    ctx.globalAlpha = (0.3 + 0.55) * (0.3 + powerFactor * 0.7);
    if (hasOverscan) {
      ctx.fillRect(burnRect.x, burnRect.y, burnScreenW, burnScreenH);
    } else {
      ctx.fillRect(x, y, w, h);
    }
    if (hasOutlines) {
      ctx.restore();
    }
    setHead(seg.endPoint ?? { x: seg.bounds.max.x, y: seg.bounds.max.y });
  } else {
    const duration = seg.endTime - seg.startTime;
    const progress = duration > 0 ? (currentTime - seg.startTime) / duration : 1;
    const geoHead = computeRasterHeadPos(
      seg.bounds,
      seg.lineCount,
      seg.directionMode,
      progress,
      seg.scanAxis,
      seg.speedMmMin,
    );

    ctx.save();
    if (hasOutlines) {
      buildClipPath(ctx, seg.outlines!, vp);
      ctx.clip('evenodd');
    }
    ctx.beginPath();
    if (isVertical) {
      const filledW = w * progress;
      ctx.rect(x, y, filledW, h);
    } else {
      const filledH = h * progress;
      ctx.rect(x, y, w, filledH);
    }
    ctx.clip();

    const powerFactor = seg.avgPowerNormalized ?? 1.0;
    ctx.fillStyle = burnColor;
    ctx.globalAlpha = (0.3 + 0.55) * (0.3 + powerFactor * 0.7);
    if (hasOverscan) {
      ctx.fillRect(burnRect.x, burnRect.y, burnScreenW, burnScreenH);
    } else {
      ctx.fillRect(x, y, w, h);
    }
    ctx.restore();

    if (options.showOverscan && hasOverscan) {
      drawOverscanProgress(
        ctx,
        isVertical,
        { x, y },
        w,
        h,
        osScreenDist,
        { x: burnRect.x, y: burnRect.y },
        burnScreenW,
        progress,
      );
    }

    setHead(geoHead);
  }

  ctx.globalAlpha = 1;
}

function tintBurnedMask(
  bitmapCache: PreviewBitmapCache,
  sequence: number,
  mask: HTMLCanvasElement,
  widthPx: number,
  heightPx: number,
  burnColor: string,
): HTMLCanvasElement {
  const tinted = bitmapCache.ensureTintedBurnedMask(sequence, widthPx, heightPx);
  const tintCtx = tinted.getContext('2d');
  if (!tintCtx) {
    return mask;
  }
  tintCtx.clearRect(0, 0, tinted.width, tinted.height);
  tintCtx.globalCompositeOperation = 'source-over';
  tintCtx.drawImage(mask, 0, 0);
  tintCtx.globalCompositeOperation = 'multiply';
  tintCtx.fillStyle = burnColor;
  tintCtx.fillRect(0, 0, tinted.width, tinted.height);
  tintCtx.globalCompositeOperation = 'destination-in';
  tintCtx.drawImage(mask, 0, 0);
  tintCtx.globalCompositeOperation = 'source-over';
  return tinted;
}

/** Copy a single planner run's pixel rectangle from the decoded
 *  preview bitmap into the burned-mask offscreen canvas. All
 *  coordinate math happens once per run, then a single drawImage call
 *  copies the slice verbatim. */
function paintRunRectToMask(
  maskCtx: CanvasRenderingContext2D,
  img: HTMLImageElement,
  run: RasterRunExtent,
  localOrigin: Point2D,
  localWidthMm: number,
  localHeightMm: number,
  widthPx: number,
  heightPx: number,
): void {
  if (localWidthMm <= 0 || localHeightMm <= 0) return;
  const xPitch = localWidthMm / widthPx;
  const yPitch = localHeightMm / heightPx;
  if (xPitch <= 0 || yPitch <= 0) return;

  const runMin = Math.min(run.start_x_mm, run.end_x_mm);
  const runMax = Math.max(run.start_x_mm, run.end_x_mm);

  let sx = Math.floor((runMin - localOrigin.x) / xPitch);
  let sxEnd = Math.ceil((runMax - localOrigin.x) / xPitch);
  let sy = Math.floor((run.y_mm - localOrigin.y) / yPitch);

  if (sx < 0) sx = 0;
  if (sxEnd > widthPx) sxEnd = widthPx;
  if (sy < 0) sy = 0;
  if (sy >= heightPx) return;

  const sw = sxEnd - sx;
  if (sw <= 0) return;

  maskCtx.drawImage(img, sx, sy, sw, 1, sx, sy, sw, 1);
}

function shouldSmoothPreviewBitmap(
  widthPx: number,
  heightPx: number,
  localWidthMm: number,
  localHeightMm: number,
  zoom: number,
): boolean {
  if (widthPx <= 0 || heightPx <= 0 || localWidthMm <= 0 || localHeightMm <= 0 || zoom <= 0) {
    return false;
  }
  const screenWidth = localWidthMm * zoom;
  const screenHeight = localHeightMm * zoom;
  return screenWidth < widthPx || screenHeight < heightPx;
}

/** Draw growing overscan strips during in-progress raster. */
function drawOverscanProgress(
  ctx: CanvasRenderingContext2D,
  isVertical: boolean,
  topLeft: { x: number; y: number },
  w: number,
  h: number,
  osScreenDist: number,
  burnTopLeft: { x: number; y: number },
  burnScreenW: number,
  progress: number,
): void {
  ctx.fillStyle = 'rgba(255, 60, 60, 0.4)';
  ctx.globalAlpha = 0.85;
  if (isVertical) {
    const filledW = w * progress;
    ctx.fillRect(topLeft.x, topLeft.y, filledW, osScreenDist);
    ctx.fillRect(topLeft.x, topLeft.y + h - osScreenDist, filledW, osScreenDist);
  } else {
    const filledH = h * progress;
    ctx.fillRect(topLeft.x, topLeft.y, osScreenDist, filledH);
    ctx.fillRect(burnTopLeft.x + burnScreenW, topLeft.y, osScreenDist, filledH);
  }
}

/** Build a canvas clip path from fill outlines using even-odd rule. */
function buildClipPath(
  ctx: CanvasRenderingContext2D,
  outlines: { points: Point2D[]; closed: boolean }[],
  vp: ViewportParams,
): void {
  ctx.beginPath();
  for (const outline of outlines) {
    if (outline.points.length < 2) continue;
    const first = worldToScreen(outline.points[0], vp);
    ctx.moveTo(first.x, first.y);
    for (let i = 1; i < outline.points.length; i++) {
      const p = worldToScreen(outline.points[i], vp);
      ctx.lineTo(p.x, p.y);
    }
    if (outline.closed) ctx.closePath();
  }
}
