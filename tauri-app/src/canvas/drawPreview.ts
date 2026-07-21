import type {
  VectorPreview,
  RasterPreview,
  TravelMove,
  PreviewFrame,
  FillOutline,
  RasterRunExtent,
} from '../types/preview';
import type { Point2D } from '../types/project';
import type { ViewportParams } from './ViewportTransform';
import {
  worldToScreen,
  worldToScreenDist,
  pxPerMm,
  worldBoundsToScreenRect,
} from './ViewportTransform';
import { isCardinalAngle, plannerStripPointToWorld } from './rasterTransform';
import type { PreviewBitmapCache } from './previewBitmapCache';

// --- Preview colors ---
const TRAVEL_COLOR = 'rgba(128, 128, 128, 0.4)';
const TRAVEL_DASH = [4, 4];
const FRAME_COLOR = 'rgba(0, 200, 200, 0.6)';
const FRAME_DASH = [6, 3];
const VECTOR_ALPHA = 0.7;
const ARROW_SIZE = 6;

export function fillNormalizedRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
): void {
  ctx.fillRect(
    Math.min(x, x + w),
    Math.min(y, y + h),
    Math.abs(w),
    Math.abs(h),
  );
}

export function applyRasterBitmapTransform(
  ctx: CanvasRenderingContext2D,
  origin: Point2D,
  angleDeg: number,
  isVerticalScan: boolean,
  vp: ViewportParams,
): number {
  const screenOrigin = worldToScreen(origin, vp);
  const scale = pxPerMm(vp.zoom);
  ctx.translate(screenOrigin.x, screenOrigin.y);
  if (vp.yAxis === 'up') {
    ctx.scale(scale, -scale);
    if (angleDeg !== 0) {
      ctx.rotate((angleDeg * Math.PI) / 180);
    }
  } else {
    if (angleDeg !== 0) {
      ctx.rotate((angleDeg * Math.PI) / 180);
    }
    ctx.scale(scale, scale);
  }
  if (isVerticalScan) {
    // World mapping: worldX = localY, worldY = localX.
    ctx.transform(0, 1, 1, 0, 0, 0);
  }
  return scale;
}

export function drawVectorPreview(
  ctx: CanvasRenderingContext2D,
  vectors: VectorPreview[],
  _layerColor: string,
  vp: ViewportParams,
): void {
  ctx.save();
  ctx.strokeStyle = '#000000';
  ctx.globalAlpha = VECTOR_ALPHA;
  ctx.lineWidth = 1.5;

  for (const vec of vectors) {
    if (vec.points.length < 2) continue;

    ctx.beginPath();
    const first = worldToScreen(vec.points[0], vp);
    ctx.moveTo(first.x, first.y);

    for (let i = 1; i < vec.points.length; i++) {
      const p = worldToScreen(vec.points[i], vp);
      ctx.lineTo(p.x, p.y);
    }

    if (vec.closed) {
      ctx.closePath();
    }
    ctx.stroke();

    // Draw direction arrow at midpoint of first segment
    if (vec.points.length >= 2) {
      const from = worldToScreen(vec.points[0], vp);
      const to = worldToScreen(vec.points[1], vp);
      drawArrow(ctx, from.x, from.y, to.x, to.y);
    }
  }

  ctx.restore();
}

export function drawRasterPreview(
  ctx: CanvasRenderingContext2D,
  regions: RasterPreview[],
  layerColor: string,
  vp: ViewportParams,
  bitmapCache: PreviewBitmapCache,
  _preferRunPreview = false,
): void {
  ctx.save();

  const burnColor = layerColor || '#000000';

  for (const region of regions) {
    const angleDeg = region.scan_angle_deg ?? 0;
    const hasOutlines = region.outlines && region.outlines.length > 0;

    const rect = worldBoundsToScreenRect(region.bounds, vp);
    const { x, y, w, h } = rect;

    // --- Primary path: draw the processed preview bitmap via drawImage.
    //
    // The bitmap lives in the planner's local run-space (pre-rotation,
    // pre-transpose). The frontend transforms it to world space via
    // translate(scan_origin) -> rotate(scan_angle_deg) -> scale(mm->px)
    // -> drawImage(local_origin_mm, local_width_mm, local_height_mm).
    // For orthogonal scans scan_angle_deg is 0 so the rotate is a no-op.
    const bitmap = region.preview_bitmap;
    const localOrigin = region.local_origin_mm;
    const localW = region.local_width_mm ?? 0;
    const localH = region.local_height_mm ?? 0;
    const sequence = region.sequence ?? -1;
    const useOutlinedRunPreview = hasOutlines
      && !!region.run_extents
      && region.run_extents.length > 0;
    // Overscan is a scanline-level behavior: the head overscans before the
    // first energized pixel of the row and after the last one. For image
    // rows with internal laser-off gaps, using sub-run extents paints fake
    // "overscan bands" between islands. Prefer planner row extents here.
    const overscanRuns = region.scanline_extents && region.scanline_extents.length > 0
      ? region.scanline_extents
      : region.run_extents && region.run_extents.length > 0
        ? region.run_extents
        : region.overscan_run_extents;
    const overscanMm = region.overscan_mm ?? 0;
    const usePerRunOverscan = overscanMm > 0
      && !!overscanRuns
      && overscanRuns.length > 0;

    // Draw overscan behind the burn preview, not on top of it.
    // For bitmap rasters this avoids the old "pink stripes invert the image"
    // artifact while still showing the true run-by-run overscan shape.
    if (usePerRunOverscan) {
      drawPerRunOverscanMarkers(ctx, region, overscanRuns!, vp, true);
    } else if (overscanMm > 0) {
      const osPx = worldToScreenDist(overscanMm, vp.zoom);
      const isVertical = (region.scan_axis ?? 'horizontal') === 'vertical';
      ctx.globalAlpha = 0.25;
      ctx.fillStyle = 'rgba(255, 60, 60, 0.6)';
      if (isVertical) {
        ctx.fillRect(x, y, w, osPx);
        ctx.fillRect(x, y + h - osPx, w, osPx);
      } else {
        ctx.fillRect(x, y, osPx, h);
        ctx.fillRect(x + w - osPx, y, osPx, h);
      }
    }

    let bitmapDrawn = false;
    if (useOutlinedRunPreview) {
      drawRasterRunStripes(ctx, region, region.run_extents!, burnColor, vp);
      bitmapDrawn = true;
    } else if (
      bitmap
      && localOrigin
      && localW > 0
      && localH > 0
      && sequence >= 0
    ) {
      const img = bitmapCache.ensurePreviewBitmap(sequence, bitmap.png_bytes);
      if (img && img.complete && img.naturalWidth > 0) {
        // The bitmap is the planner's exact burn footprint (transparent
        // background, burned pixels only) — never clip it to the outlines,
        // which are simplified for IPC and can collapse thin slivers.
        ctx.save();
        const origin = region.scan_origin ?? { x: 0, y: 0 };
        const isVerticalScan = region.scan_axis === 'vertical';
        const scale = applyRasterBitmapTransform(ctx, origin, angleDeg, isVerticalScan, vp);
        ctx.imageSmoothingEnabled = shouldSmoothPreviewBitmap(
          bitmap.width_px,
          bitmap.height_px,
          localW,
          localH,
          scale,
        );
        ctx.drawImage(img, localOrigin.x, localOrigin.y, localW, localH);
        ctx.restore();
        bitmapDrawn = true;
      }
    }

    // --- Secondary: outline + overscan markers always draw regardless
    // of whether the bitmap was available. If the bitmap wasn't ready
    // yet (still loading), we also fall through to the legacy hatched
    // placeholder so the region isn't invisible during decode.
    if (!bitmapDrawn) {
      const powerFactor = region.avg_power_normalized ?? 1.0;
      const alpha = (0.3 + region.fill_density * 0.55) * (0.3 + powerFactor * 0.7);
      if (hasOutlines) {
        ctx.save();
        buildOutlineClipPath(ctx, region.outlines!, vp);
        ctx.clip('evenodd');
      }
      ctx.globalAlpha = alpha * 0.3;
      ctx.fillStyle = burnColor;
      ctx.fillRect(x, y, w, h);
      ctx.globalAlpha = alpha;
      ctx.strokeStyle = burnColor;
      ctx.lineWidth = 0.75;
      drawHatching(ctx, x, y, w, h, angleDeg);
      if (hasOutlines) {
        ctx.restore();
      }
    }

    // --- Outline on top of bitmap.
    if (hasOutlines) {
      ctx.globalAlpha = 0.6;
      ctx.strokeStyle = burnColor;
      ctx.lineWidth = 1;
      for (const outline of region.outlines!) {
        if (outline.points.length < 2) continue;
        ctx.beginPath();
        const first = worldToScreen(outline.points[0], vp);
        ctx.moveTo(first.x, first.y);
        for (let i = 1; i < outline.points.length; i++) {
          const p = worldToScreen(outline.points[i], vp);
          ctx.lineTo(p.x, p.y);
        }
        if (outline.closed) ctx.closePath();
        ctx.stroke();
      }
    } else {
      ctx.globalAlpha = 0.5;
      ctx.strokeStyle = burnColor;
      ctx.lineWidth = 1;
      ctx.strokeRect(x, y, w, h);
    }

    // --- Per-run overscan markers (fallback/debug path only).
    //
    // If the bitmap wasn't ready and we also didn't have enough geometry for
    // the primary overscan pass above, fall back to on-top markers so the
    // region still communicates overscan during decode/loading.
    if (
      !bitmapDrawn
      && !usePerRunOverscan
      && (region.overscan_mm ?? 0) > 0
      && overscanRuns
      && overscanRuns.length > 0
    ) {
      drawPerRunOverscanMarkers(ctx, region, overscanRuns, vp, true);
    }
  }

  ctx.restore();
}

export function drawRasterRunStripes(
  ctx: CanvasRenderingContext2D,
  region: {
    line_interval_mm: number;
    scan_axis?: 'horizontal' | 'vertical';
    scan_angle_deg?: number;
    scan_origin?: Point2D;
    outlines?: FillOutline[];
  },
  visibleStrips: RasterRunExtent[],
  layerColor: string,
  vp: ViewportParams,
  options: {
    showBaseTint?: boolean;
    emphasizeVisibleRuns?: boolean;
  } = {},
): void {
  if (visibleStrips.length === 0) return;
  const showBaseTint = options.showBaseTint ?? true;
  const emphasizeVisibleRuns = options.emphasizeVisibleRuns ?? false;

  const intervalPx = worldToScreenDist(region.line_interval_mm, vp.zoom);
  // Preview-only density control:
  // when scanlines are closer than about one screen pixel, drawing every
  // physical line causes axis-aligned fills (especially plain rectangles)
  // to saturate into a solid black block. That is a rendering artifact, not
  // actual planner duplication. Decimate in screen space so preview remains
  // readable while preserving the scan direction/look.
  const targetSpacingPx = emphasizeVisibleRuns ? 0.55 : 1.25;
  const stride = intervalPx > 0 ? Math.max(1, Math.ceil(targetSpacingPx / intervalPx)) : 1;
  const effectiveIntervalPx = intervalPx * stride;
  const thickness = emphasizeVisibleRuns
    ? Math.min(1.8, Math.max(0.9, effectiveIntervalPx * 0.8))
    : Math.min(1.25, Math.max(0.5, effectiveIntervalPx * 0.42));

  ctx.save();

  // Solid base tint under the stripes so the fill reads as one continuous
  // engraved region at any zoom. Alpha is set so stripes-over-base looks
  // like a uniform shade when stripes can't be individually resolved, and
  // still lets individual stripes stand out when zoomed in.
  //
  // Only this cosmetic tint is clipped to the outlines (the even-odd clip
  // is what carves holes out of the per-outline fills). The stripes below
  // are the planner's exact burn extents and must NOT be clipped: preview
  // outlines are simplified under a point budget and can collapse thin
  // slivers the laser still burns.
  const color = layerColor || '#000000';
  if (showBaseTint && region.outlines && region.outlines.length > 0) {
    ctx.save();
    buildOutlineClipPath(ctx, region.outlines, vp);
    ctx.clip('evenodd');
    ctx.fillStyle = color;
    ctx.globalAlpha = 0.35;
    for (const outline of region.outlines) {
      if (outline.points.length < 3) continue;
      ctx.beginPath();
      const first = worldToScreen(outline.points[0], vp);
      ctx.moveTo(first.x, first.y);
      for (let i = 1; i < outline.points.length; i++) {
        const p = worldToScreen(outline.points[i], vp);
        ctx.lineTo(p.x, p.y);
      }
      ctx.closePath();
      ctx.fill('evenodd');
    }
    ctx.restore();
  }

  ctx.strokeStyle = color;
  ctx.globalAlpha = emphasizeVisibleRuns ? 0.82 : 0.6;
  ctx.lineWidth = thickness;
  ctx.lineCap = 'butt';

  if (emphasizeVisibleRuns) {
    ctx.save();
    ctx.strokeStyle = color;
    ctx.globalAlpha = 0.28;
    ctx.lineWidth = Math.max(thickness * 2.1, effectiveIntervalPx * 1.05, 1.5);
    ctx.lineCap = 'butt';

    let lastUnderlayCoord = Number.NEGATIVE_INFINITY;
    for (let i = 0; i < visibleStrips.length; i += stride) {
      const strip = visibleStrips[i];
      const start = plannerStripPointToWorld(strip.y_mm, strip.start_x_mm, {
        scanAxis: region.scan_axis === 'vertical' ? 'vertical' : 'horizontal',
        scanAngleDeg: region.scan_angle_deg,
        scanOrigin: region.scan_origin,
      });
      const end = plannerStripPointToWorld(strip.y_mm, strip.end_x_mm, {
        scanAxis: region.scan_axis === 'vertical' ? 'vertical' : 'horizontal',
        scanAngleDeg: region.scan_angle_deg,
        scanOrigin: region.scan_origin,
      });
      const s = worldToScreen(start, vp);
      const e = worldToScreen(end, vp);
      const coord = region.scan_axis === 'vertical' ? s.x : s.y;
      if (Math.abs(coord - lastUnderlayCoord) < 0.45) {
        continue;
      }
      lastUnderlayCoord = coord;
      ctx.beginPath();
      ctx.moveTo(s.x, s.y);
      ctx.lineTo(e.x, e.y);
      ctx.stroke();
    }
    ctx.restore();

    ctx.strokeStyle = color;
    ctx.globalAlpha = 0.9;
    ctx.lineWidth = thickness;
    ctx.lineCap = 'butt';
  }

  let lastScreenCoord = Number.NEGATIVE_INFINITY;
  for (let i = 0; i < visibleStrips.length; i += stride) {
    const strip = visibleStrips[i];
    const start = plannerStripPointToWorld(strip.y_mm, strip.start_x_mm, {
      scanAxis: region.scan_axis === 'vertical' ? 'vertical' : 'horizontal',
      scanAngleDeg: region.scan_angle_deg,
      scanOrigin: region.scan_origin,
    });
    const end = plannerStripPointToWorld(strip.y_mm, strip.end_x_mm, {
      scanAxis: region.scan_axis === 'vertical' ? 'vertical' : 'horizontal',
      scanAngleDeg: region.scan_angle_deg,
      scanOrigin: region.scan_origin,
    });
    const s = worldToScreen(start, vp);
    const e = worldToScreen(end, vp);
    const coord = region.scan_axis === 'vertical' ? s.x : s.y;
    const minScreenGap = emphasizeVisibleRuns ? 0.45 : 0.85;
    if (Math.abs(coord - lastScreenCoord) < minScreenGap) {
      continue;
    }
    lastScreenCoord = coord;
    ctx.beginPath();
    ctx.moveTo(s.x, s.y);
    ctx.lineTo(e.x, e.y);
    ctx.stroke();
  }

  ctx.restore();
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
  // When previewing below native pixel density, nearest-neighbor
  // drops rows/columns and makes dark dithered regions look
  // under-burned. Smooth only for downsampling; keep crisp pixels
  // when zoomed in at or above 1 screen px per source px.
  return screenWidth < widthPx || screenHeight < heightPx;
}

/**
 * Draw per-run overscan markers for visible raster strips.
 *
 * Rotated rasters render overscan as line segments along the scan direction.
 * Orthogonal rasters keep the existing rectangular tick rendering.
 */
export function drawPerRunOverscanMarkers(
  ctx: CanvasRenderingContext2D,
  region: {
    scan_axis?: string;
    line_interval_mm: number;
    scan_angle_deg?: number;
    scan_origin?: { x: number; y: number };
    overscan_mm?: number;
  },
  visibleStrips: RasterRunExtent[],
  vp: ViewportParams,
  isRegionComplete: boolean,
): void {
  const overscanMm = region.overscan_mm ?? 0;
  if (overscanMm <= 0 || visibleStrips.length === 0) return;

  const isRotated = !isCardinalAngle(region.scan_angle_deg)
    && !!region.scan_origin;
  const isVertical = region.scan_axis === 'vertical';
  const lineInterval = worldToScreenDist(region.line_interval_mm, vp.zoom);
  const thickness = Math.max(1, lineInterval * 0.6);

  ctx.fillStyle = 'rgba(255, 60, 60, 0.4)';
  ctx.strokeStyle = 'rgba(255, 60, 60, 0.4)';
  ctx.globalAlpha = 0.85;
  ctx.lineWidth = thickness;
  ctx.lineCap = 'butt';

  for (let i = 0; i < visibleStrips.length; i++) {
    const strip = visibleStrips[i];
    const isLast = i === visibleStrips.length - 1;
    const showLeadOut = isRegionComplete || !isLast;

    if (isRotated && region.scan_origin) {
      const leadInStart = plannerStripPointToWorld(strip.y_mm, strip.start_x_mm - overscanMm, {
        scanAxis: region.scan_axis === 'vertical' ? 'vertical' : 'horizontal',
        scanAngleDeg: region.scan_angle_deg,
        scanOrigin: region.scan_origin,
      });
      const leadInEnd = plannerStripPointToWorld(strip.y_mm, strip.start_x_mm, {
        scanAxis: region.scan_axis === 'vertical' ? 'vertical' : 'horizontal',
        scanAngleDeg: region.scan_angle_deg,
        scanOrigin: region.scan_origin,
      });
      const liStart = worldToScreen(leadInStart, vp);
      const liEnd = worldToScreen(leadInEnd, vp);
      ctx.beginPath();
      ctx.moveTo(liStart.x, liStart.y);
      ctx.lineTo(liEnd.x, liEnd.y);
      ctx.stroke();

      if (showLeadOut) {
        const leadOutStart = plannerStripPointToWorld(strip.y_mm, strip.end_x_mm, {
          scanAxis: region.scan_axis === 'vertical' ? 'vertical' : 'horizontal',
          scanAngleDeg: region.scan_angle_deg,
          scanOrigin: region.scan_origin,
        });
        const leadOutEnd = plannerStripPointToWorld(strip.y_mm, strip.end_x_mm + overscanMm, {
          scanAxis: region.scan_axis === 'vertical' ? 'vertical' : 'horizontal',
          scanAngleDeg: region.scan_angle_deg,
          scanOrigin: region.scan_origin,
        });
        const loStart = worldToScreen(leadOutStart, vp);
        const loEnd = worldToScreen(leadOutEnd, vp);
        ctx.beginPath();
        ctx.moveTo(loStart.x, loStart.y);
        ctx.lineTo(loEnd.x, loEnd.y);
        ctx.stroke();
      }
    } else if (isVertical) {
      const cx = worldToScreen({ x: strip.y_mm, y: 0 }, vp).x;
      const liStart = worldToScreen({ x: 0, y: strip.start_x_mm - overscanMm }, vp).y;
      const liEnd = worldToScreen({ x: 0, y: strip.start_x_mm }, vp).y;
      fillNormalizedRect(ctx, cx - thickness / 2, liStart, thickness, liEnd - liStart);

      if (showLeadOut) {
        const loStart = worldToScreen({ x: 0, y: strip.end_x_mm }, vp).y;
        const loEnd = worldToScreen({ x: 0, y: strip.end_x_mm + overscanMm }, vp).y;
        fillNormalizedRect(ctx, cx - thickness / 2, loStart, thickness, loEnd - loStart);
      }
    } else {
      const cy = worldToScreen({ x: 0, y: strip.y_mm }, vp).y;
      const liStart = worldToScreen({ x: strip.start_x_mm - overscanMm, y: 0 }, vp).x;
      const liEnd = worldToScreen({ x: strip.start_x_mm, y: 0 }, vp).x;
      fillNormalizedRect(ctx, liStart, cy - thickness / 2, liEnd - liStart, thickness);

      if (showLeadOut) {
        const loStart = worldToScreen({ x: strip.end_x_mm, y: 0 }, vp).x;
        const loEnd = worldToScreen({ x: strip.end_x_mm + overscanMm, y: 0 }, vp).x;
        fillNormalizedRect(ctx, loStart, cy - thickness / 2, loEnd - loStart, thickness);
      }
    }
  }
}

/** Build a canvas clip path from fill outlines using even-odd rule. */
function buildOutlineClipPath(
  ctx: CanvasRenderingContext2D,
  outlines: FillOutline[],
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

export function drawTravelPreview(
  ctx: CanvasRenderingContext2D,
  moves: TravelMove[],
  vp: ViewportParams,
): void {
  ctx.save();
  ctx.strokeStyle = TRAVEL_COLOR;
  ctx.lineWidth = 0.75;
  ctx.setLineDash(TRAVEL_DASH);

  for (const move_ of moves) {
    const from = worldToScreen(move_.from, vp);
    const to = worldToScreen(move_.to, vp);
    ctx.beginPath();
    ctx.moveTo(from.x, from.y);
    ctx.lineTo(to.x, to.y);
    ctx.stroke();
  }

  ctx.setLineDash([]);
  ctx.restore();
}

export function drawFramePreview(
  ctx: CanvasRenderingContext2D,
  frame: PreviewFrame,
  vp: ViewportParams,
): void {
  if (frame.path.length < 2) return;

  ctx.save();
  ctx.strokeStyle = FRAME_COLOR;
  ctx.lineWidth = 1.5;
  ctx.setLineDash(FRAME_DASH);

  ctx.beginPath();
  const first = worldToScreen(frame.path[0], vp);
  ctx.moveTo(first.x, first.y);

  for (let i = 1; i < frame.path.length; i++) {
    const p = worldToScreen(frame.path[i], vp);
    ctx.lineTo(p.x, p.y);
  }
  ctx.stroke();

  ctx.setLineDash([]);
  ctx.restore();
}

function drawArrow(
  ctx: CanvasRenderingContext2D,
  fromX: number,
  fromY: number,
  toX: number,
  toY: number,
): void {
  const midX = (fromX + toX) / 2;
  const midY = (fromY + toY) / 2;
  const dx = toX - fromX;
  const dy = toY - fromY;
  const len = Math.sqrt(dx * dx + dy * dy);
  if (len < 1) return;

  const nx = dx / len;
  const ny = dy / len;

  ctx.beginPath();
  ctx.moveTo(midX + nx * ARROW_SIZE, midY + ny * ARROW_SIZE);
  ctx.lineTo(midX - nx * ARROW_SIZE * 0.5 + ny * ARROW_SIZE * 0.5, midY - ny * ARROW_SIZE * 0.5 - nx * ARROW_SIZE * 0.5);
  ctx.moveTo(midX + nx * ARROW_SIZE, midY + ny * ARROW_SIZE);
  ctx.lineTo(midX - nx * ARROW_SIZE * 0.5 - ny * ARROW_SIZE * 0.5, midY - ny * ARROW_SIZE * 0.5 + nx * ARROW_SIZE * 0.5);
  ctx.stroke();
}

function drawHatching(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  angleDeg?: number,
): void {
  const spacing = 6;
  ctx.save();
  ctx.beginPath();
  ctx.rect(x, y, w, h);
  ctx.clip();

  const angleRad = angleDeg != null ? (angleDeg * Math.PI) / 180 : Math.PI / 4; // default 45
  const cos = Math.cos(angleRad);
  const sin = Math.sin(angleRad);

  const maxDim = w + h;
  // Draw lines along scan direction, offset perpendicularly
  for (let i = -maxDim; i < maxDim; i += spacing) {
    ctx.beginPath();
    const ox = -sin * i; // perpendicular offset x
    const oy = cos * i;  // perpendicular offset y
    const cx = x + w / 2 + ox;
    const cy = y + h / 2 + oy;
    ctx.moveTo(cx - cos * maxDim, cy - sin * maxDim);
    ctx.lineTo(cx + cos * maxDim, cy + sin * maxDim);
    ctx.stroke();
  }

  ctx.restore();
}
