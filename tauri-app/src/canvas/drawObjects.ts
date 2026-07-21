import type { ProjectObject, Layer, Transform2D } from '../types/project';
import type { EditablePath } from '../types/vector';
import type { ViewportParams } from './ViewportTransform';
import type { CanvasTheme } from './constants';
import { worldToScreen, worldToScreenDist, pxPerMm } from './ViewportTransform';
import { RASTER_PLACEHOLDER_STROKE } from './constants';

export interface RasterMaskRenderContext {
  inside: Path2D[];
  outside: Path2D[];
}

const MAX_MASKED_RASTER_OFFSCREEN_DIM = 8192;

/** Apply an affine transform matrix to the canvas context, pivoting around the object's bounds center */
export function applyTransform(
  ctx: CanvasRenderingContext2D,
  transform: Transform2D,
  vp: ViewportParams,
  bounds: { min: { x: number; y: number }; max: { x: number; y: number } },
): void {
  const scale = worldToScreenDist(1, vp.zoom);
  const cx = (bounds.min.x + bounds.max.x) / 2;
  const cy = (bounds.min.y + bounds.max.y) / 2;
  const pivot = worldToScreen({ x: cx, y: cy }, vp);

  ctx.translate(pivot.x, pivot.y);
  ctx.transform(
    transform.a,
    transform.b,
    transform.c,
    transform.d,
    transform.tx * scale,
    transform.ty * scale,
  );
  ctx.translate(-pivot.x, -pivot.y);
}

export function drawShape(
  ctx: CanvasRenderingContext2D,
  obj: ProjectObject,
  color: string,
  vp: ViewportParams,
  filled?: boolean,
): void {
  if (obj.data.type !== 'shape') return;

  const { kind, corner_radius } = obj.data;
  // Derive dimensions from bounds — authoritative source during resize
  const topLeft = worldToScreen(obj.bounds.min, vp);
  const w = worldToScreenDist(obj.bounds.max.x - obj.bounds.min.x, vp.zoom);
  const h = worldToScreenDist(obj.bounds.max.y - obj.bounds.min.y, vp.zoom);

  ctx.save();
  applyTransform(ctx, obj.transform, vp, obj.bounds);

  ctx.strokeStyle = color;
  ctx.lineWidth = 1.5;

  if (kind === 'rectangle') {
    if (corner_radius > 0) {
      const r = Math.min(worldToScreenDist(corner_radius, vp.zoom), w / 2, h / 2);
      ctx.beginPath();
      ctx.roundRect(topLeft.x, topLeft.y, w, h, r);
      if (filled) {
        ctx.fillStyle = color + 'B0';
        ctx.fill();
      }
      ctx.stroke();
    } else if (corner_radius < 0) {
      const r = Math.min(worldToScreenDist(Math.abs(corner_radius), vp.zoom), w / 2, h / 2);
      const k = r * 0.5522847498;
      ctx.beginPath();
      ctx.moveTo(topLeft.x + r, topLeft.y);
      ctx.lineTo(topLeft.x + w - r, topLeft.y);
      ctx.bezierCurveTo(
        topLeft.x + w - r,
        topLeft.y + k,
        topLeft.x + w - k,
        topLeft.y + r,
        topLeft.x + w,
        topLeft.y + r,
      );
      ctx.lineTo(topLeft.x + w, topLeft.y + h - r);
      ctx.bezierCurveTo(
        topLeft.x + w - k,
        topLeft.y + h - r,
        topLeft.x + w - r,
        topLeft.y + h - k,
        topLeft.x + w - r,
        topLeft.y + h,
      );
      ctx.lineTo(topLeft.x + r, topLeft.y + h);
      ctx.bezierCurveTo(
        topLeft.x + r,
        topLeft.y + h - k,
        topLeft.x + k,
        topLeft.y + h - r,
        topLeft.x,
        topLeft.y + h - r,
      );
      ctx.lineTo(topLeft.x, topLeft.y + r);
      ctx.bezierCurveTo(
        topLeft.x + k,
        topLeft.y + r,
        topLeft.x + r,
        topLeft.y + k,
        topLeft.x + r,
        topLeft.y,
      );
      ctx.closePath();
      if (filled) {
        ctx.fillStyle = color + 'B0';
        ctx.fill();
      }
      ctx.stroke();
    } else {
      ctx.beginPath();
      ctx.rect(topLeft.x, topLeft.y, w, h);
      if (filled) {
        ctx.fillStyle = color + 'B0';
        ctx.fill();
      }
      ctx.stroke();
    }
  } else if (kind === 'ellipse') {
    ctx.beginPath();
    ctx.ellipse(topLeft.x + w / 2, topLeft.y + h / 2, w / 2, h / 2, 0, 0, Math.PI * 2);
    if (filled) {
      ctx.fillStyle = color + 'B0';
      ctx.fill();
    }
    ctx.stroke();
  }

  ctx.restore();
}

export function drawText(
  ctx: CanvasRenderingContext2D,
  obj: ProjectObject,
  color: string,
  vp: ViewportParams,
  filled?: boolean,
): void {
  if (obj.data.type !== 'text') return;

  const { resolved_path_data } = obj.data;
  if (resolved_path_data) {
    const commands = parsePathData(resolved_path_data);
    if (commands.length === 0) return;

    const localToScreen = (px: number, py: number) =>
      worldToScreen({ x: obj.bounds.min.x + px, y: obj.bounds.min.y + py }, vp);

    ctx.save();
    applyTransform(ctx, obj.transform, vp, obj.bounds);
    ctx.strokeStyle = color;
    ctx.lineWidth = 1.5;
    ctx.beginPath();

    for (const cmd of commands) {
      switch (cmd.type) {
        case 'M': {
          const pt = localToScreen(cmd.x, cmd.y);
          ctx.moveTo(pt.x, pt.y);
          break;
        }
        case 'L': {
          const pt = localToScreen(cmd.x, cmd.y);
          ctx.lineTo(pt.x, pt.y);
          break;
        }
        case 'Q': {
          const cp = localToScreen(cmd.x1!, cmd.y1!);
          const pt = localToScreen(cmd.x, cmd.y);
          ctx.quadraticCurveTo(cp.x, cp.y, pt.x, pt.y);
          break;
        }
        case 'C': {
          const cp1 = localToScreen(cmd.x1!, cmd.y1!);
          const cp2 = localToScreen(cmd.x2!, cmd.y2!);
          const pt = localToScreen(cmd.x, cmd.y);
          ctx.bezierCurveTo(cp1.x, cp1.y, cp2.x, cp2.y, pt.x, pt.y);
          break;
        }
        case 'Z':
          ctx.closePath();
          break;
      }
    }

    if (filled) {
      ctx.fillStyle = color + 'B0';
      ctx.fill();
    }
    ctx.stroke();
    ctx.restore();
    return;
  }

  const {
    content,
    font_family,
    font_size_mm,
    alignment,
    alignment_v,
    bold,
    italic,
    upper_case,
    h_spacing,
    v_spacing,
    welded,
    distort,
    on_path,
    path_offset,
  } = obj.data;
  const displayText = upper_case ? content.toUpperCase() : content;
  const lines = displayText.split('\n');
  const topLeft = worldToScreen(obj.bounds.min, vp);
  const fontSize = worldToScreenDist(font_size_mm, vp.zoom);
  const boundsH = worldToScreenDist(obj.bounds.max.y - obj.bounds.min.y, vp.zoom);
  const boundsW = worldToScreenDist(obj.bounds.max.x - obj.bounds.min.x, vp.zoom);
  const letterSpacing = h_spacing ? worldToScreenDist(h_spacing, vp.zoom) : 0;
  const lineHeight = fontSize + (v_spacing ? worldToScreenDist(v_spacing, vp.zoom) : 0);
  const totalTextH = lines.length > 1 ? fontSize + (lines.length - 1) * lineHeight : fontSize;
  const pathOffsetPx = on_path && path_offset ? worldToScreenDist(path_offset, vp.zoom) : 0;

  ctx.save();
  applyTransform(ctx, obj.transform, vp, obj.bounds);

  const style = `${italic ? 'italic ' : ''}${bold ? 'bold ' : ''}`;
  ctx.font = `${style}${fontSize}px ${font_family}`;
  ctx.fillStyle = color;
  ctx.textBaseline = 'top';

  // Welded: add a matching stroke so adjacent glyphs visually merge
  if (welded) {
    ctx.strokeStyle = color;
    ctx.lineWidth = Math.max(1, fontSize * 0.08);
    ctx.lineJoin = 'round';
  }

  // Compute first line Y based on vertical alignment
  let baseY: number;
  if (alignment_v === 'middle') {
    baseY = topLeft.y + (boundsH - totalTextH) / 2;
  } else if (alignment_v === 'bottom') {
    baseY = topLeft.y + boundsH - totalTextH;
  } else {
    baseY = topLeft.y;
  }

  // Helper: emit one character with optional weld stroke and distort scale
  const emitChar = (ch: string, cx: number, ly: number, charCenterNorm: number) => {
    if (distort) {
      // Barrel distortion: characters near center are taller, edges are compressed
      const t = (charCenterNorm - 0.5) * 2; // -1..1
      const sy = 1 - 0.15 * t * t; // peak 1.0 at center, 0.85 at edges
      ctx.save();
      ctx.translate(cx, ly + fontSize / 2);
      ctx.scale(1, sy);
      ctx.translate(0, -fontSize / 2);
      ctx.fillText(ch, 0, 0);
      if (welded) ctx.strokeText(ch, 0, 0);
      ctx.restore();
    } else {
      ctx.fillText(ch, cx, ly);
      if (welded) ctx.strokeText(ch, cx, ly);
    }
  };

  const drawLine = (text: string, ly: number) => {
    const needsPerChar = letterSpacing || distort;

    if (needsPerChar) {
      // Per-character rendering (for letter-spacing, distort, or weld stroke)
      const charWidths: number[] = [];
      let naturalW = 0;
      for (const ch of text) {
        const cw = ctx.measureText(ch).width;
        charWidths.push(cw);
        naturalW += cw;
      }
      const totalW = naturalW + letterSpacing * Math.max(0, charWidths.length - 1);
      let sx = topLeft.x + pathOffsetPx;
      if (alignment === 'center') sx = topLeft.x + (boundsW - totalW) / 2 + pathOffsetPx;
      else if (alignment === 'right') sx = topLeft.x + boundsW - totalW + pathOffsetPx;
      ctx.textAlign = 'left';
      let cx = sx;
      let idx = 0;
      for (const ch of text) {
        const charCenter = totalW > 0 ? (cx - sx + charWidths[idx] / 2) / totalW : 0.5;
        emitChar(ch, cx, ly, charCenter);
        cx += charWidths[idx] + letterSpacing;
        idx++;
      }
    } else if (alignment === 'center') {
      ctx.textAlign = 'center';
      const x = topLeft.x + boundsW / 2 + pathOffsetPx;
      ctx.fillText(text, x, ly);
      if (welded) ctx.strokeText(text, x, ly);
    } else if (alignment === 'right') {
      ctx.textAlign = 'right';
      const x = topLeft.x + boundsW + pathOffsetPx;
      ctx.fillText(text, x, ly);
      if (welded) ctx.strokeText(text, x, ly);
    } else {
      ctx.textAlign = 'left';
      const x = topLeft.x + pathOffsetPx;
      ctx.fillText(text, x, ly);
      if (welded) ctx.strokeText(text, x, ly);
    }
  };

  for (let i = 0; i < lines.length; i++) {
    drawLine(lines[i], baseY + i * lineHeight);
  }

  ctx.restore();
}

export function drawVectorPath(
  ctx: CanvasRenderingContext2D,
  obj: ProjectObject,
  color: string,
  vp: ViewportParams,
  filled?: boolean,
): void {
  if (obj.data.type !== 'vector_path') return;

  const info = getVectorPathRenderInfoForObject(obj);
  if (!info) return;
  const { commands, bbox: pathBBox, path2d: cachedPath } = info;
  if (commands.length === 0) return;
  const subpaths = getPathSubpathRenderStates(commands);
  const hasClosedSubpath = subpaths.some((subpath) => subpath.closed);
  const canUseWholePathFill = !filled || !hasClosedSubpath || subpaths.every((subpath) => subpath.closed);
  const boundsMinX = obj.bounds.min.x;
  const boundsMinY = obj.bounds.min.y;
  const boundsW = obj.bounds.max.x - obj.bounds.min.x;
  const boundsH = obj.bounds.max.y - obj.bounds.min.y;

  const mapPt = (px: number, py: number) => {
    const mapped = mapPathCoordToBounds(px, py, pathBBox, boundsMinX, boundsMinY, boundsW, boundsH);
    return worldToScreen(mapped, vp);
  };

  if (
    supportsTransformedPath2DFastPath() &&
    cachedPath &&
    pathBBox.width > 0 &&
    pathBBox.height > 0 &&
    canUseWholePathFill
  ) {
    // Build a local-path -> screen matrix that matches mapPathCoordToBounds +
    // worldToScreen exactly. vp.zoom is a percentage; actual pixels-per-mm is
    // pxPerMm(vp.zoom). vp.offset is the world point at canvas center, not a
    // screen translation.
    const scale = pxPerMm(vp.zoom);
    const sx = boundsW / pathBBox.width;
    const sy = boundsH / pathBBox.height;
    const screenScaleX = sx * scale;
    const screenScaleY = sy * scale;
    const baseTx = (boundsMinX - pathBBox.minX * sx - vp.offset.x) * scale + vp.canvasWidth / 2;
    const baseTy = (boundsMinY - pathBBox.minY * sy - vp.offset.y) * scale + vp.canvasHeight / 2;
    const pivot = worldToScreen(
      {
        x: (obj.bounds.min.x + obj.bounds.max.x) / 2,
        y: (obj.bounds.min.y + obj.bounds.max.y) / 2,
      },
      vp,
    );
    const objectMatrix = new DOMMatrix([
      obj.transform.a,
      obj.transform.b,
      obj.transform.c,
      obj.transform.d,
      obj.transform.tx * scale,
      obj.transform.ty * scale,
    ]);
    const finalMatrix = new DOMMatrix()
      .translateSelf(pivot.x, pivot.y)
      .multiplySelf(objectMatrix)
      .translateSelf(-pivot.x, -pivot.y)
      .multiplySelf(
        new DOMMatrix([
          screenScaleX,
          0,
          0,
          screenScaleY,
          baseTx,
          baseTy,
        ]),
      );
    const screenPath = new Path2D();
    screenPath.addPath(cachedPath, finalMatrix);

    ctx.save();
    ctx.strokeStyle = color;
    ctx.lineWidth = 1.5;
    if (filled && hasClosedSubpath) {
      ctx.fillStyle = color + 'B0';
      ctx.fill(screenPath);
    }
    ctx.stroke(screenPath);
    ctx.restore();
    return;
  }

  ctx.save();
  applyTransform(ctx, obj.transform, vp, obj.bounds);

  ctx.strokeStyle = color;
  ctx.lineWidth = 1.5;

  if (filled && hasClosedSubpath) {
    ctx.beginPath();
    for (const subpath of subpaths) {
      if (subpath.closed) {
        drawPathCommandsToContext(ctx, subpath.commands, mapPt, true);
      }
    }
    ctx.fillStyle = color + 'B0';
    ctx.fill('evenodd');
  }

  ctx.beginPath();
  for (const subpath of subpaths) {
    drawPathCommandsToContext(ctx, subpath.commands, mapPt, subpath.closed);
  }

  ctx.stroke();
  ctx.restore();
}

export function drawEditableVectorPath(
  ctx: CanvasRenderingContext2D,
  paths: EditablePath[],
  color: string,
  vp: ViewportParams,
  nodeToWorld?: (pos: { x: number; y: number }) => { x: number; y: number },
  filled?: boolean,
): void {
  const mapPos = nodeToWorld ?? ((p: { x: number; y: number }) => p);

  ctx.save();
  ctx.strokeStyle = color;
  ctx.lineWidth = 1.5;

  const drawPath = (path: EditablePath): void => {
    const { nodes, closed } = path;
    if (nodes.length === 0) return;

    const first = worldToScreen(mapPos(nodes[0].position), vp);
    ctx.moveTo(first.x, first.y);

    for (let i = 1; i < nodes.length; i++) {
      const prev = nodes[i - 1];
      const curr = nodes[i];
      const prevOut = prev.handle_out ? worldToScreen(mapPos(prev.handle_out), vp) : null;
      const currIn = curr.handle_in ? worldToScreen(mapPos(curr.handle_in), vp) : null;
      const currPos = worldToScreen(mapPos(curr.position), vp);

      if (prevOut && currIn) {
        ctx.bezierCurveTo(prevOut.x, prevOut.y, currIn.x, currIn.y, currPos.x, currPos.y);
      } else if (prevOut) {
        ctx.quadraticCurveTo(prevOut.x, prevOut.y, currPos.x, currPos.y);
      } else if (currIn) {
        ctx.quadraticCurveTo(currIn.x, currIn.y, currPos.x, currPos.y);
      } else {
        ctx.lineTo(currPos.x, currPos.y);
      }
    }

    if (closed && nodes.length > 1) {
      const last = nodes[nodes.length - 1];
      const lastOut = last.handle_out ? worldToScreen(mapPos(last.handle_out), vp) : null;
      const firstIn = nodes[0].handle_in ? worldToScreen(mapPos(nodes[0].handle_in), vp) : null;

      if (lastOut && firstIn) {
        ctx.bezierCurveTo(lastOut.x, lastOut.y, firstIn.x, firstIn.y, first.x, first.y);
      } else if (lastOut) {
        ctx.quadraticCurveTo(lastOut.x, lastOut.y, first.x, first.y);
      } else if (firstIn) {
        ctx.quadraticCurveTo(firstIn.x, firstIn.y, first.x, first.y);
      }
      ctx.closePath();
    }
  };

  if (filled && paths.some((path) => path.closed && path.nodes.length > 0)) {
    ctx.beginPath();
    for (const path of paths) {
      if (path.closed) drawPath(path);
    }
    ctx.fillStyle = color + 'B0';
    ctx.fill('evenodd');
  }

  ctx.beginPath();
  for (const path of paths) {
    drawPath(path);
  }
  ctx.stroke();

  ctx.restore();
}

export function drawRasterImage(
  ctx: CanvasRenderingContext2D,
  obj: ProjectObject,
  color: string,
  vp: ViewportParams,
  imageCache?: Map<string, HTMLImageElement | HTMLCanvasElement>,
  imageErrorCache?: Map<string, string>,
  maskContext?: RasterMaskRenderContext | null,
): void {
  if (obj.data.type !== 'raster_image') return;

  const hasUsableMasks = Boolean(maskContext && (maskContext.inside.length > 0 || maskContext.outside.length > 0));
  if (hasUsableMasks && drawMaskedRasterImage(ctx, obj, color, vp, imageCache, imageErrorCache, maskContext!)) {
    return;
  }

  drawRasterImageContent(ctx, obj, color, vp, imageCache, imageErrorCache);
}

function drawRasterImageContent(
  ctx: CanvasRenderingContext2D,
  obj: ProjectObject,
  color: string,
  vp: ViewportParams,
  imageCache?: Map<string, HTMLImageElement | HTMLCanvasElement>,
  imageErrorCache?: Map<string, string>,
): void {
  if (obj.data.type !== 'raster_image') return;

  const topLeft = worldToScreen(obj.bounds.min, vp);
  const w = worldToScreenDist(obj.bounds.max.x - obj.bounds.min.x, vp.zoom);
  const h = worldToScreenDist(obj.bounds.max.y - obj.bounds.min.y, vp.zoom);

  ctx.save();
  applyTransform(ctx, obj.transform, vp, obj.bounds);

  // Main canvas shows a grayscale positioning proxy. The cache entry is a
  // pre-baked grayscale `HTMLCanvasElement` once loading is complete; before
  // then it's still the raw `HTMLImageElement` and we treat that as "not
  // ready yet" so we don't briefly flash the color source.
  const cached = imageCache?.get(obj.data.asset_key);
  const loadError = imageErrorCache?.get(obj.data.asset_key);
  const loadedImageFallback =
    cached instanceof HTMLImageElement && cached.complete && cached.naturalWidth > 0 && cached.naturalHeight > 0;
  if (cached instanceof HTMLCanvasElement || loadedImageFallback) {
    ctx.drawImage(cached, topLeft.x, topLeft.y, w, h);
  } else if (loadError) {
    ctx.strokeStyle = '#ef4444';
    ctx.lineWidth = 1;
    ctx.setLineDash([4, 4]);
    ctx.strokeRect(topLeft.x, topLeft.y, w, h);
    ctx.setLineDash([]);

    const fontSize = Math.max(10, Math.min(w / 9, 14));
    ctx.font = `${fontSize}px sans-serif`;
    ctx.fillStyle = '#ef4444';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText('Missing image', topLeft.x + w / 2, topLeft.y + h / 2);
  } else {
    // Loading placeholder: outline only
    ctx.strokeStyle = RASTER_PLACEHOLDER_STROKE;
    ctx.lineWidth = 1;
    ctx.setLineDash([4, 4]);
    ctx.strokeRect(topLeft.x, topLeft.y, w, h);
    ctx.setLineDash([]);

    const fontSize = Math.max(10, Math.min(w / 8, 14));
    ctx.font = `${fontSize}px sans-serif`;
    ctx.fillStyle = color;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText('Loading...', topLeft.x + w / 2, topLeft.y + h / 2);
  }

  ctx.restore();
}

function drawMaskedRasterImage(
  ctx: CanvasRenderingContext2D,
  obj: ProjectObject,
  color: string,
  vp: ViewportParams,
  imageCache: Map<string, HTMLImageElement | HTMLCanvasElement> | undefined,
  imageErrorCache: Map<string, string> | undefined,
  maskContext: RasterMaskRenderContext,
): boolean {
  const aabb = getTransformedObjectScreenAabb(obj, vp);
  const minX = Math.floor(aabb.minX);
  const minY = Math.floor(aabb.minY);
  const width = Math.max(1, Math.ceil(aabb.maxX) - minX);
  const height = Math.max(1, Math.ceil(aabb.maxY) - minY);
  if (
    !Number.isFinite(width) ||
    !Number.isFinite(height) ||
    width > MAX_MASKED_RASTER_OFFSCREEN_DIM ||
    height > MAX_MASKED_RASTER_OFFSCREEN_DIM
  ) {
    return false;
  }

  const rasterCanvas = document.createElement('canvas');
  rasterCanvas.width = width;
  rasterCanvas.height = height;
  const rasterCtx = rasterCanvas.getContext('2d');
  if (!rasterCtx) {
    return false;
  }

  rasterCtx.save();
  rasterCtx.translate(-minX, -minY);
  drawRasterImageContent(rasterCtx, obj, color, vp, imageCache, imageErrorCache);
  rasterCtx.restore();

  if (maskContext.inside.length > 0) {
    const alphaCanvas = document.createElement('canvas');
    alphaCanvas.width = width;
    alphaCanvas.height = height;
    const alphaCtx = alphaCanvas.getContext('2d');
    if (!alphaCtx) {
      return false;
    }
    alphaCtx.save();
    alphaCtx.translate(-minX, -minY);
    alphaCtx.fillStyle = '#000000';
    for (const path of maskContext.inside) {
      alphaCtx.fill(path);
    }
    alphaCtx.restore();

    rasterCtx.save();
    rasterCtx.globalCompositeOperation = 'destination-in';
    rasterCtx.drawImage(alphaCanvas, 0, 0);
    rasterCtx.restore();
  }

  if (maskContext.outside.length > 0) {
    rasterCtx.save();
    rasterCtx.translate(-minX, -minY);
    rasterCtx.globalCompositeOperation = 'destination-out';
    rasterCtx.fillStyle = '#000000';
    for (const path of maskContext.outside) {
      rasterCtx.fill(path);
    }
    rasterCtx.restore();
  }

  ctx.drawImage(rasterCanvas, minX, minY);
  return true;
}

export function drawBarcode(
  ctx: CanvasRenderingContext2D,
  obj: ProjectObject,
  color: string,
  vp: ViewportParams,
  barcodePathCache?: Map<string, string>,
): void {
  if (obj.data.type !== 'barcode') return;

  const { barcode_type, data, width, height, options } = obj.data;
  const normalizedOptions = {
    show_text: options?.show_text ?? false,
    qr_error_correction: options?.qr_error_correction ?? 'medium',
    data_matrix_force_square: options?.data_matrix_force_square ?? false,
  };
  const cacheKey = `${barcode_type}:${data}:${width}:${height}:${JSON.stringify(normalizedOptions)}`;
  const pathD = barcodePathCache?.get(cacheKey);
  const showText = normalizedOptions.show_text
    && ['code128', 'code39', 'code93', 'codabar', 'standard_2_of_5', 'ean13', 'ean8', 'upc_a'].includes(barcode_type);

  if (pathD) {
    const commands = parsePathData(pathD);
    if (commands.length === 0) return;

    const pathBBox = computePathBBox(commands);
    const boundsMinX = obj.bounds.min.x;
    const boundsMinY = obj.bounds.min.y;
    const boundsW = obj.bounds.max.x - obj.bounds.min.x;
    const fullBoundsH = obj.bounds.max.y - obj.bounds.min.y;
    const textWorldHeight = showText ? Math.min(fullBoundsH * 0.22, 6) : 0;
    const boundsH = Math.max(0.1, fullBoundsH - textWorldHeight);

    const mapPt = (px: number, py: number) => {
      const mapped = mapPathCoordToBounds(
        px,
        py,
        pathBBox,
        boundsMinX,
        boundsMinY,
        boundsW,
        boundsH,
      );
      return worldToScreen(mapped, vp);
    };

    ctx.save();
    applyTransform(ctx, obj.transform, vp, obj.bounds);

    ctx.strokeStyle = color;
    ctx.lineWidth = 1.5;
    ctx.beginPath();

    for (const cmd of commands) {
      switch (cmd.type) {
        case 'M': {
          const screenPt = mapPt(cmd.x, cmd.y);
          ctx.moveTo(screenPt.x, screenPt.y);
          break;
        }
        case 'L': {
          const screenPt = mapPt(cmd.x, cmd.y);
          ctx.lineTo(screenPt.x, screenPt.y);
          break;
        }
        case 'Q': {
          const qcp = mapPt(cmd.x1!, cmd.y1!);
          const screenPt = mapPt(cmd.x, cmd.y);
          ctx.quadraticCurveTo(qcp.x, qcp.y, screenPt.x, screenPt.y);
          break;
        }
        case 'C': {
          const cp1 = mapPt(cmd.x1!, cmd.y1!);
          const cp2 = mapPt(cmd.x2!, cmd.y2!);
          const screenPt = mapPt(cmd.x, cmd.y);
          ctx.bezierCurveTo(cp1.x, cp1.y, cp2.x, cp2.y, screenPt.x, screenPt.y);
          break;
        }
        case 'Z':
          ctx.closePath();
          break;
      }
    }

    ctx.fillStyle = color + 'B0';
    ctx.fill();
    ctx.stroke();
    if (showText) {
      const topLeft = worldToScreen({ x: obj.bounds.min.x, y: obj.bounds.max.y - textWorldHeight }, vp);
      const boundsWidthPx = worldToScreenDist(obj.bounds.max.x - obj.bounds.min.x, vp.zoom);
      const textHeightPx = worldToScreenDist(textWorldHeight, vp.zoom);
      const fontSize = Math.max(8, Math.min(textHeightPx * 0.65, boundsWidthPx / Math.max(data.length * 0.62, 1)));
      ctx.fillStyle = color;
      ctx.font = `${fontSize}px sans-serif`;
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText(data, topLeft.x + boundsWidthPx / 2, topLeft.y + textHeightPx / 2, boundsWidthPx);
    }
    ctx.restore();
  } else {
    // Loading placeholder
    const topLeft = worldToScreen(obj.bounds.min, vp);
    const w = worldToScreenDist(obj.bounds.max.x - obj.bounds.min.x, vp.zoom);
    const h = worldToScreenDist(obj.bounds.max.y - obj.bounds.min.y, vp.zoom);

    ctx.save();
    applyTransform(ctx, obj.transform, vp, obj.bounds);

    ctx.strokeStyle = RASTER_PLACEHOLDER_STROKE;
    ctx.lineWidth = 1;
    ctx.setLineDash([4, 4]);
    ctx.strokeRect(topLeft.x, topLeft.y, w, h);
    ctx.setLineDash([]);

    const fontSize = Math.max(10, Math.min(w / 8, 14));
    ctx.font = `${fontSize}px sans-serif`;
    ctx.fillStyle = color;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText('Barcode', topLeft.x + w / 2, topLeft.y + h / 2);

    ctx.restore();
  }
}

export function drawPolygon(
  ctx: CanvasRenderingContext2D,
  obj: ProjectObject,
  color: string,
  vp: ViewportParams,
  filled?: boolean,
): void {
  if (obj.data.type !== 'polygon') return;

  const { sides, radius } = obj.data;
  if (sides < 3) return;
  const localPts = buildPolygonPoints(sides, radius);
  drawPointShape(ctx, obj, color, vp, localPts, filled);
}

export function drawStar(
  ctx: CanvasRenderingContext2D,
  obj: ProjectObject,
  color: string,
  vp: ViewportParams,
  filled?: boolean,
): void {
  if (obj.data.type !== 'star') return;

  const cmds = buildStarPath(
    obj.data.points,
    obj.data.bulge,
    obj.data.ratio,
    obj.data.dual_radius,
    obj.data.ratio2 ?? undefined,
    obj.data.corner_radius ?? 0,
    obj.data.corner_radii,
  );
  drawPathShape(ctx, obj, color, vp, cmds, filled);
}

function drawPointShape(
  ctx: CanvasRenderingContext2D,
  obj: ProjectObject,
  color: string,
  vp: ViewportParams,
  localPts: { x: number; y: number }[],
  filled?: boolean,
): void {
  if (localPts.length < 3) return;
  // Delegate to drawPathShape with line-only commands
  const cmds: ShapeCmd[] = localPts.map((pt, i) =>
    i === 0
      ? { type: 'move' as const, x: pt.x, y: pt.y }
      : { type: 'line' as const, x: pt.x, y: pt.y },
  );
  drawPathShape(ctx, obj, color, vp, cmds, filled);
}

export function quadraticPoint(
  p0: { x: number; y: number },
  q: { x: number; y: number },
  p1: { x: number; y: number },
  t: number,
): { x: number; y: number } {
  const mt = 1 - t;
  return {
    x: mt * mt * p0.x + 2 * mt * t * q.x + t * t * p1.x,
    y: mt * mt * p0.y + 2 * mt * t * q.y + t * t * p1.y,
  };
}

export function quadraticExtremumT(a: number, q: number, b: number): number | null {
  const denom = a - 2 * q + b;
  if (Math.abs(denom) < 1e-12) return null;
  const t = (a - q) / denom;
  return t > 0 && t < 1 ? t : null;
}

export function getShapeCommandBounds(
  cmds: ShapeCmd[],
): { minX: number; minY: number; maxX: number; maxY: number } | null {
  let minX = Infinity,
    minY = Infinity,
    maxX = -Infinity,
    maxY = -Infinity;
  let current: { x: number; y: number } | null = null;

  for (const cmd of cmds) {
    const end = { x: cmd.x, y: cmd.y };
    minX = Math.min(minX, end.x);
    minY = Math.min(minY, end.y);
    maxX = Math.max(maxX, end.x);
    maxY = Math.max(maxY, end.y);
    if (cmd.type === 'quad' && current) {
      const ctrl = { x: cmd.qx, y: cmd.qy };
      const etx = quadraticExtremumT(current.x, ctrl.x, end.x);
      if (etx !== null) {
        const p = quadraticPoint(current, ctrl, end, etx);
        minX = Math.min(minX, p.x);
        maxX = Math.max(maxX, p.x);
      }
      const ety = quadraticExtremumT(current.y, ctrl.y, end.y);
      if (ety !== null) {
        const p = quadraticPoint(current, ctrl, end, ety);
        minY = Math.min(minY, p.y);
        maxY = Math.max(maxY, p.y);
      }
    }
    current = end;
  }

  if (!Number.isFinite(minX) || !Number.isFinite(minY) || !Number.isFinite(maxX) || !Number.isFinite(maxY)) {
    return null;
  }
  return { minX, minY, maxX, maxY };
}

function drawPathShape(
  ctx: CanvasRenderingContext2D,
  obj: ProjectObject,
  color: string,
  vp: ViewportParams,
  cmds: ShapeCmd[],
  filled?: boolean,
): void {
  if (cmds.length < 3) return;

  // Step 1: Compute AABB of LOCAL vertices (no transform applied)
  const bbox = getShapeCommandBounds(cmds);
  if (!bbox) return;
  const { minX, minY, maxX, maxY } = bbox;

  // Step 2: Map local → bounds (using untransformed AABB)
  const pathW = maxX - minX,
    pathH = maxY - minY;
  const bMinX = obj.bounds.min.x,
    bMinY = obj.bounds.min.y;
  const bW = obj.bounds.max.x - bMinX,
    bH = obj.bounds.max.y - bMinY;
  const sx = pathW > 0 ? bW / pathW : 1;
  const sy = pathH > 0 ? bH / pathH : 1;
  const mapTx = bMinX - minX * sx,
    mapTy = bMinY - minY * sy;
  const toScreen = (lx: number, ly: number) =>
    worldToScreen({ x: lx * sx + mapTx, y: ly * sy + mapTy }, vp);

  // Step 3: Canvas transform (same model as rect/text/vector_path)
  ctx.save();
  applyTransform(ctx, obj.transform, vp, obj.bounds);
  ctx.strokeStyle = color;
  ctx.lineWidth = 1.5;
  ctx.beginPath();

  // Step 4: Draw using bounds-space coordinates
  for (const cmd of cmds) {
    if (cmd.type === 'move') {
      const s = toScreen(cmd.x, cmd.y);
      ctx.moveTo(s.x, s.y);
    } else if (cmd.type === 'line') {
      const s = toScreen(cmd.x, cmd.y);
      ctx.lineTo(s.x, s.y);
    } else {
      const sq = toScreen(cmd.qx, cmd.qy);
      const se = toScreen(cmd.x, cmd.y);
      ctx.quadraticCurveTo(sq.x, sq.y, se.x, se.y);
    }
  }

  ctx.closePath();

  if (filled) {
    ctx.fillStyle = color + 'B0';
    ctx.fill();
  }

  ctx.stroke();
  ctx.restore();
}

function applyObjectTransformToWorld(
  point: { x: number; y: number },
  obj: ProjectObject,
): { x: number; y: number } {
  const cx = (obj.bounds.min.x + obj.bounds.max.x) / 2;
  const cy = (obj.bounds.min.y + obj.bounds.max.y) / 2;
  const dx = point.x - cx;
  const dy = point.y - cy;
  return {
    x: cx + obj.transform.a * dx + obj.transform.c * dy + obj.transform.tx,
    y: cy + obj.transform.b * dx + obj.transform.d * dy + obj.transform.ty,
  };
}

function objectWorldToScreen(
  point: { x: number; y: number },
  obj: ProjectObject,
  vp: ViewportParams,
): { x: number; y: number } {
  return worldToScreen(applyObjectTransformToWorld(point, obj), vp);
}

function getTransformedObjectScreenAabb(obj: ProjectObject, vp: ViewportParams): {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
} {
  const corners = [
    obj.bounds.min,
    { x: obj.bounds.max.x, y: obj.bounds.min.y },
    obj.bounds.max,
    { x: obj.bounds.min.x, y: obj.bounds.max.y },
  ].map((point) => objectWorldToScreen(point, obj, vp));
  return {
    minX: Math.min(...corners.map((point) => point.x)),
    minY: Math.min(...corners.map((point) => point.y)),
    maxX: Math.max(...corners.map((point) => point.x)),
    maxY: Math.max(...corners.map((point) => point.y)),
  };
}

function addScreenPointsToPath(path: Path2D, points: { x: number; y: number }[]): boolean {
  if (points.length < 3) return false;
  path.moveTo(points[0].x, points[0].y);
  for (let i = 1; i < points.length; i++) {
    path.lineTo(points[i].x, points[i].y);
  }
  path.closePath();
  return true;
}

function addBoundsRectToScreenPath(path: Path2D, obj: ProjectObject, vp: ViewportParams): boolean {
  return addScreenPointsToPath(path, [
    objectWorldToScreen(obj.bounds.min, obj, vp),
    objectWorldToScreen({ x: obj.bounds.max.x, y: obj.bounds.min.y }, obj, vp),
    objectWorldToScreen(obj.bounds.max, obj, vp),
    objectWorldToScreen({ x: obj.bounds.min.x, y: obj.bounds.max.y }, obj, vp),
  ]);
}

function addRoundedRectToScreenPath(path: Path2D, obj: ProjectObject, vp: ViewportParams, radiusMm: number): boolean {
  const minX = obj.bounds.min.x;
  const minY = obj.bounds.min.y;
  const maxX = obj.bounds.max.x;
  const maxY = obj.bounds.max.y;
  const width = maxX - minX;
  const height = maxY - minY;
  const radius = Math.min(Math.abs(radiusMm), Math.abs(width) / 2, Math.abs(height) / 2);
  if (radius <= 1e-9) return addBoundsRectToScreenPath(path, obj, vp);

  const points: { x: number; y: number }[] = [];
  const segmentsPerCorner = 12;
  const corners = [
    { cx: maxX - radius, cy: minY + radius, start: -Math.PI / 2, end: 0 },
    { cx: maxX - radius, cy: maxY - radius, start: 0, end: Math.PI / 2 },
    { cx: minX + radius, cy: maxY - radius, start: Math.PI / 2, end: Math.PI },
    { cx: minX + radius, cy: minY + radius, start: Math.PI, end: Math.PI * 1.5 },
  ];
  for (const corner of corners) {
    for (let i = 0; i <= segmentsPerCorner; i++) {
      const t = i / segmentsPerCorner;
      const angle = corner.start + (corner.end - corner.start) * t;
      points.push(objectWorldToScreen(
        { x: corner.cx + Math.cos(angle) * radius, y: corner.cy + Math.sin(angle) * radius },
        obj,
        vp,
      ));
    }
  }
  return addScreenPointsToPath(path, points);
}

function addEllipseToScreenPath(path: Path2D, obj: ProjectObject, vp: ViewportParams): boolean {
  const points: { x: number; y: number }[] = [];
  const cx = (obj.bounds.min.x + obj.bounds.max.x) / 2;
  const cy = (obj.bounds.min.y + obj.bounds.max.y) / 2;
  const rx = (obj.bounds.max.x - obj.bounds.min.x) / 2;
  const ry = (obj.bounds.max.y - obj.bounds.min.y) / 2;
  if (rx <= 0 || ry <= 0) return false;
  for (let i = 0; i < 64; i++) {
    const theta = (i / 64) * Math.PI * 2;
    points.push(objectWorldToScreen({ x: cx + Math.cos(theta) * rx, y: cy + Math.sin(theta) * ry }, obj, vp));
  }
  return addScreenPointsToPath(path, points);
}

function addVectorCommandsToScreenPath(
  path: Path2D,
  commands: PathCommand[],
  bbox: PathBBox,
  obj: ProjectObject,
  vp: ViewportParams,
  closePath: boolean,
): boolean {
  if (commands.length === 0) return false;
  const boundsMinX = obj.bounds.min.x;
  const boundsMinY = obj.bounds.min.y;
  const boundsW = obj.bounds.max.x - obj.bounds.min.x;
  const boundsH = obj.bounds.max.y - obj.bounds.min.y;
  const mapPt = (px: number, py: number) =>
    objectWorldToScreen(mapPathCoordToBounds(px, py, bbox, boundsMinX, boundsMinY, boundsW, boundsH), obj, vp);

  let drew = false;
  for (const cmd of commands) {
    switch (cmd.type) {
      case 'M': {
        const pt = mapPt(cmd.x, cmd.y);
        path.moveTo(pt.x, pt.y);
        drew = true;
        break;
      }
      case 'L': {
        const pt = mapPt(cmd.x, cmd.y);
        path.lineTo(pt.x, pt.y);
        break;
      }
      case 'Q': {
        const cp = mapPt(cmd.x1!, cmd.y1!);
        const pt = mapPt(cmd.x, cmd.y);
        path.quadraticCurveTo(cp.x, cp.y, pt.x, pt.y);
        break;
      }
      case 'C': {
        const cp1 = mapPt(cmd.x1!, cmd.y1!);
        const cp2 = mapPt(cmd.x2!, cmd.y2!);
        const pt = mapPt(cmd.x, cmd.y);
        path.bezierCurveTo(cp1.x, cp1.y, cp2.x, cp2.y, pt.x, pt.y);
        break;
      }
      case 'Z':
        path.closePath();
        break;
    }
  }
  if (closePath && commands[commands.length - 1]?.type !== 'Z') {
    path.closePath();
  }
  return drew;
}

function addShapeCommandsToScreenPath(
  path: Path2D,
  obj: ProjectObject,
  vp: ViewportParams,
  cmds: ShapeCmd[],
): boolean {
  if (cmds.length < 3) return false;

  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const cmd of cmds) {
    minX = Math.min(minX, cmd.x);
    minY = Math.min(minY, cmd.y);
    maxX = Math.max(maxX, cmd.x);
    maxY = Math.max(maxY, cmd.y);
    if (cmd.type === 'quad') {
      minX = Math.min(minX, cmd.qx);
      minY = Math.min(minY, cmd.qy);
      maxX = Math.max(maxX, cmd.qx);
      maxY = Math.max(maxY, cmd.qy);
    }
  }
  const pathW = maxX - minX;
  const pathH = maxY - minY;
  const boundsMinX = obj.bounds.min.x;
  const boundsMinY = obj.bounds.min.y;
  const boundsW = obj.bounds.max.x - boundsMinX;
  const boundsH = obj.bounds.max.y - boundsMinY;
  const sx = pathW > 0 ? boundsW / pathW : 1;
  const sy = pathH > 0 ? boundsH / pathH : 1;
  const mapTx = boundsMinX - minX * sx;
  const mapTy = boundsMinY - minY * sy;
  const toScreen = (x: number, y: number) =>
    objectWorldToScreen({ x: x * sx + mapTx, y: y * sy + mapTy }, obj, vp);

  for (const cmd of cmds) {
    if (cmd.type === 'move') {
      const pt = toScreen(cmd.x, cmd.y);
      path.moveTo(pt.x, pt.y);
    } else if (cmd.type === 'line') {
      const pt = toScreen(cmd.x, cmd.y);
      path.lineTo(pt.x, pt.y);
    } else {
      const cp = toScreen(cmd.qx, cmd.qy);
      const pt = toScreen(cmd.x, cmd.y);
      path.quadraticCurveTo(cp.x, cp.y, pt.x, pt.y);
    }
  }
  path.closePath();
  return true;
}

export function buildObjectScreenMaskPath(obj: ProjectObject, vp: ViewportParams): Path2D | null {
  if (typeof Path2D === 'undefined') return null;
  const path = new Path2D();
  switch (obj.data.type) {
    case 'shape':
      if (obj.data.kind === 'ellipse') {
        return addEllipseToScreenPath(path, obj, vp) ? path : null;
      }
      if (obj.data.corner_radius > 0) {
        return addRoundedRectToScreenPath(path, obj, vp, obj.data.corner_radius) ? path : null;
      }
      return addBoundsRectToScreenPath(path, obj, vp) ? path : null;
    case 'vector_path': {
      if (!obj.data.closed) return null;
      const info = getVectorPathRenderInfoForObject(obj);
      if (!info) return null;
      return addVectorCommandsToScreenPath(path, info.commands, info.bbox, obj, vp, true) ? path : null;
    }
    case 'polygon': {
      const points = buildPolygonPoints(obj.data.sides, obj.data.radius);
      const cmds = points.map((point, index) =>
        index === 0
          ? { type: 'move' as const, x: point.x, y: point.y }
          : { type: 'line' as const, x: point.x, y: point.y },
      );
      return addShapeCommandsToScreenPath(path, obj, vp, cmds) ? path : null;
    }
    case 'star': {
      const cmds = buildStarPath(
        obj.data.points,
        obj.data.bulge,
        obj.data.ratio,
        obj.data.dual_radius,
        obj.data.ratio2 ?? undefined,
        obj.data.corner_radius ?? 0,
        obj.data.corner_radii,
      );
      return addShapeCommandsToScreenPath(path, obj, vp, cmds) ? path : null;
    }
    default:
      return null;
  }
}

export function buildPolygonPoints(sides: number, radius: number): { x: number; y: number }[] {
  const pts: { x: number; y: number }[] = [];
  for (let i = 0; i < sides; i++) {
    const theta = (i / sides) * Math.PI * 2 - Math.PI / 2;
    pts.push({ x: radius + radius * Math.cos(theta), y: radius + radius * Math.sin(theta) });
  }
  return pts;
}

/** Path command for shapes that may include curves */
export type ShapeCmd =
  | { type: 'move'; x: number; y: number }
  | { type: 'line'; x: number; y: number }
  | { type: 'quad'; qx: number; qy: number; x: number; y: number };

type StarAnchor = { x: number; y: number; radius: number };

function starAnchor(cx: number, cy: number, radius: number, angle: number): StarAnchor {
  return { x: cx + radius * Math.cos(angle), y: cy + radius * Math.sin(angle), radius };
}

function effectiveStarCornerRadii(
  anchorCount: number,
  cornerRadius = 0,
  cornerRadii?: number[],
): number[] {
  if (cornerRadii && cornerRadii.length === anchorCount) {
    return cornerRadii.map((r) => Math.max(0, r));
  }
  return Array.from({ length: anchorCount }, () => Math.max(0, cornerRadius));
}

function buildRoundedStarCommands(
  verts: StarAnchor[],
  cornerRadius = 0,
  cornerRadii?: number[],
): ShapeCmd[] {
  if (verts.length < 3) return [];
  const radii = effectiveStarCornerRadii(verts.length, cornerRadius, cornerRadii);
  const corners = verts.map((curr, i) => {
    const prev = verts[(i + verts.length - 1) % verts.length];
    const next = verts[(i + 1) % verts.length];
    const radius = radii[i] ?? 0;
    const inDx = curr.x - prev.x;
    const inDy = curr.y - prev.y;
    const outDx = next.x - curr.x;
    const outDy = next.y - curr.y;
    const lenIn = Math.hypot(inDx, inDy);
    const lenOut = Math.hypot(outDx, outDy);
    if (radius < 1e-9 || lenIn < 1e-9 || lenOut < 1e-9) {
      return { rounded: false as const, entry: curr, exit: curr, control: curr };
    }
    const trim = Math.min(radius, lenIn * 0.45, lenOut * 0.45);
    if (trim < 1e-9) {
      return { rounded: false as const, entry: curr, exit: curr, control: curr };
    }
    const inUx = inDx / lenIn;
    const inUy = inDy / lenIn;
    const outUx = outDx / lenOut;
    const outUy = outDy / lenOut;
    const entry = { x: curr.x - inUx * trim, y: curr.y - inUy * trim };
    const exit = { x: curr.x + outUx * trim, y: curr.y + outUy * trim };
    const handle = trim * 0.5522847498;
    return {
      rounded: true as const,
      entry,
      exit,
      control: {
        x: (entry.x + exit.x) * 0.5 + (inUx - outUx) * handle * 0.25,
        y: (entry.y + exit.y) * 0.5 + (inUy - outUy) * handle * 0.25,
      },
    };
  });

  const cmds: ShapeCmd[] = [];
  const first = corners[0];
  cmds.push({
    type: 'move',
    x: first.rounded ? first.exit.x : verts[0].x,
    y: first.rounded ? first.exit.y : verts[0].y,
  });
  for (let i = 1; i < verts.length; i++) {
    const corner = corners[i];
    if (!corner.rounded) {
      cmds.push({ type: 'line', x: verts[i].x, y: verts[i].y });
      continue;
    }
    cmds.push({ type: 'line', x: corner.entry.x, y: corner.entry.y });
    cmds.push({
      type: 'quad',
      qx: corner.control.x,
      qy: corner.control.y,
      x: corner.exit.x,
      y: corner.exit.y,
    });
  }
  if (first.rounded) {
    cmds.push({ type: 'line', x: first.entry.x, y: first.entry.y });
    cmds.push({
      type: 'quad',
      qx: first.control.x,
      qy: first.control.y,
      x: first.exit.x,
      y: first.exit.y,
    });
  }
  return cmds;
}

/** Quadratic control point for a bulged star edge, keeping anchor angles fixed. */
function bulgeControlPoint(
  cx: number,
  cy: number,
  a: StarAnchor,
  b: StarAnchor,
  bulgeFactor: number,
): { x: number; y: number } {
  const mx = (a.x + b.x) * 0.5;
  const my = (a.y + b.y) * 0.5;
  const dx = mx - cx;
  const dy = my - cy;
  const midRadius = Math.sqrt(dx * dx + dy * dy);
  if (midRadius < 1e-12) return { x: mx, y: my };
  const targetRadius = midRadius + bulgeFactor * (Math.max(a.radius, b.radius) - midRadius);
  return {
    x: cx + (dx / midRadius) * targetRadius,
    y: cy + (dy / midRadius) * targetRadius,
  };
}

export function buildStarPath(
  points: number,
  bulge: number,
  ratio: number,
  dualRadius: boolean,
  ratio2?: number,
  cornerRadius = 0,
  cornerRadii?: number[],
): ShapeCmd[] {
  const outerRadius = 50;
  const ratioA = Math.min(0.95, Math.max(0.05, ratio));
  const ratioB = Math.min(1, Math.max(0.05, ratio2 ?? 0.7));
  const bulgeFactor = Math.min(1, Math.max(0, bulge));
  const n = Math.max(3, points);
  const cx = outerRadius;
  const cy = outerRadius;

  const verts: StarAnchor[] = [];
  if (dualRadius) {
    const step = (Math.PI * 2) / n;
    const valleyR = outerRadius * ratioA;
    const secondaryR = outerRadius * ratioB;
    for (let i = 0; i < n; i++) {
      const baseAngle = i * step - Math.PI / 2;
      verts.push(starAnchor(cx, cy, outerRadius, baseAngle));
      verts.push(starAnchor(cx, cy, valleyR, baseAngle + step * 0.25));
      verts.push(starAnchor(cx, cy, secondaryR, baseAngle + step * 0.5));
      verts.push(starAnchor(cx, cy, valleyR, baseAngle + step * 0.75));
    }
  } else {
    const outerStep = (Math.PI * 2) / n;
    const innerRadius = outerRadius * ratioA;
    for (let i = 0; i < n; i++) {
      const outerAngle = i * outerStep - Math.PI / 2;
      const innerAngle = outerAngle + outerStep * 0.5;
      verts.push(starAnchor(cx, cy, outerRadius, outerAngle));
      verts.push(starAnchor(cx, cy, innerRadius, innerAngle));
    }
  }

  if (
    Math.abs(bulgeFactor) < 1e-9 &&
    (cornerRadius > 1e-9 || (cornerRadii?.some((r) => r > 1e-9) ?? false))
  ) {
    return buildRoundedStarCommands(verts, cornerRadius, cornerRadii);
  }

  const cmds: ShapeCmd[] = [];
  for (let i = 0; i < verts.length; i++) {
    if (i === 0) {
      cmds.push({ type: 'move', x: verts[0].x, y: verts[0].y });
    } else if (Math.abs(bulgeFactor) < 1e-9) {
      cmds.push({ type: 'line', x: verts[i].x, y: verts[i].y });
    } else {
      const q = bulgeControlPoint(cx, cy, verts[i - 1], verts[i], bulgeFactor);
      cmds.push({ type: 'quad', qx: q.x, qy: q.y, x: verts[i].x, y: verts[i].y });
    }
  }
  if (Math.abs(bulgeFactor) >= 1e-9 && verts.length > 1) {
    const q = bulgeControlPoint(cx, cy, verts[verts.length - 1], verts[0], bulgeFactor);
    cmds.push({ type: 'quad', qx: q.x, qy: q.y, x: verts[0].x, y: verts[0].y });
  }
  return cmds;
}

/** Draw a single object based on its type */
export function drawObject(
  ctx: CanvasRenderingContext2D,
  obj: ProjectObject,
  layer: Layer,
  vp: ViewportParams,
  imageCache?: Map<string, HTMLImageElement | HTMLCanvasElement>,
  _theme?: CanvasTheme,
  filled?: boolean,
  isToolLayer?: boolean,
  barcodePathCache?: Map<string, string>,
  imageErrorCache?: Map<string, string>,
): void {
  const color = layer.color_tag;

  if (isToolLayer) {
    ctx.save();
    ctx.globalAlpha = 0.6;
    ctx.setLineDash([8, 4]);
  }

  switch (obj.data.type) {
    case 'shape':
      drawShape(ctx, obj, color, vp, filled);
      break;
    case 'polygon':
      drawPolygon(ctx, obj, color, vp, filled);
      break;
    case 'star':
      drawStar(ctx, obj, color, vp, filled);
      break;
    case 'text':
      drawText(ctx, obj, color, vp, filled);
      break;
    case 'vector_path':
      drawVectorPath(ctx, obj, color, vp, filled);
      break;
    case 'raster_image':
      drawRasterImage(ctx, obj, color, vp, imageCache, imageErrorCache);
      break;
    case 'barcode':
      drawBarcode(ctx, obj, color, vp, barcodePathCache);
      break;
    case 'group':
      // Groups are flattened in rendering — children are drawn individually
      break;
    case 'virtual_clone':
      // VirtualClones are resolved at the renderer level before drawObject is called.
      // If we reach here, the clone was not resolved — draw a placeholder.
      break;
  }

  if (isToolLayer) {
    ctx.restore();
  }
}

// --- Path parsing (M/L/Q/C/Z subset) ---

export interface PathCommand {
  type: 'M' | 'L' | 'Q' | 'C' | 'Z';
  x: number;
  y: number;
  x1?: number;
  y1?: number;
  x2?: number;
  y2?: number;
}

export interface PathSubpathRenderState {
  commands: PathCommand[];
  closed: boolean;
}

export interface PathBBox {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
  width: number;
  height: number;
}

export interface VectorPathRenderInfo {
  commands: PathCommand[];
  bbox: PathBBox;
  path2d: Path2D | null;
  commandCount: number;
}

type VectorPathObjectData = Extract<ProjectObject['data'], { type: 'vector_path' }>;

export function getPathSubpathRenderStates(commands: PathCommand[]): PathSubpathRenderState[] {
  const subpaths: Array<{ commands: PathCommand[]; explicitClosed: boolean }> = [];
  let current: PathCommand[] = [];
  let explicitClosed = false;

  for (const cmd of commands) {
    if (cmd.type === 'M') {
      if (current.length > 0) {
        subpaths.push({ commands: current, explicitClosed });
      }
      current = [cmd];
      explicitClosed = false;
      continue;
    }

    if (current.length === 0) {
      current = [cmd];
    } else {
      current.push(cmd);
    }
    if (cmd.type === 'Z') {
      explicitClosed = true;
    }
  }

  if (current.length > 0) {
    subpaths.push({ commands: current, explicitClosed });
  }

  return subpaths.map((subpath) => ({
    commands: subpath.commands,
    closed: subpath.explicitClosed,
  }));
}

function drawPathCommandsToContext(
  ctx: CanvasRenderingContext2D,
  commands: PathCommand[],
  mapPt: (x: number, y: number) => { x: number; y: number },
  forceClose: boolean,
): void {
  for (const cmd of commands) {
    switch (cmd.type) {
      case 'M': {
        const screenPt = mapPt(cmd.x, cmd.y);
        ctx.moveTo(screenPt.x, screenPt.y);
        break;
      }
      case 'L': {
        const screenPt = mapPt(cmd.x, cmd.y);
        ctx.lineTo(screenPt.x, screenPt.y);
        break;
      }
      case 'Q': {
        const qcp = mapPt(cmd.x1!, cmd.y1!);
        const screenPt = mapPt(cmd.x, cmd.y);
        ctx.quadraticCurveTo(qcp.x, qcp.y, screenPt.x, screenPt.y);
        break;
      }
      case 'C': {
        const cp1 = mapPt(cmd.x1!, cmd.y1!);
        const cp2 = mapPt(cmd.x2!, cmd.y2!);
        const screenPt = mapPt(cmd.x, cmd.y);
        ctx.bezierCurveTo(cp1.x, cp1.y, cp2.x, cp2.y, screenPt.x, screenPt.y);
        break;
      }
      case 'Z':
        ctx.closePath();
        break;
    }
  }

  if (forceClose && commands[commands.length - 1]?.type !== 'Z') {
    ctx.closePath();
  }
}

const PATH_PARSE_CACHE_LIMIT = 256;
const pathCommandCache = new Map<string, PathCommand[]>();
let pathBBoxCache = new WeakMap<PathCommand[], PathBBox>();
const path2DCache = new Map<string, Path2D | null>();
let vectorPathRenderInfoCache = new WeakMap<VectorPathObjectData, VectorPathRenderInfo>();
let transformedPath2DSupport: boolean | undefined;

function rememberParsedPath(d: string, commands: PathCommand[]): PathCommand[] {
  if (pathCommandCache.has(d)) {
    pathCommandCache.delete(d);
  }
  pathCommandCache.set(d, commands);
  if (path2DCache.has(d)) {
    const cachedPath = path2DCache.get(d) ?? null;
    path2DCache.delete(d);
    path2DCache.set(d, cachedPath);
  }
  if (pathCommandCache.size > PATH_PARSE_CACHE_LIMIT) {
    const oldestKey = pathCommandCache.keys().next().value;
    if (oldestKey !== undefined) {
      pathCommandCache.delete(oldestKey);
      path2DCache.delete(oldestKey);
    }
  }
  return commands;
}

function getCachedParsedPathCommands(d: string): PathCommand[] | undefined {
  const cached = pathCommandCache.get(d);
  if (!cached) {
    return undefined;
  }
  pathCommandCache.delete(d);
  pathCommandCache.set(d, cached);
  return cached;
}

function getOrCreateParsedPathCommands(d: string): PathCommand[] {
  return getCachedParsedPathCommands(d) ?? parsePathData(d);
}

function getCachedPath2D(d: string): Path2D | null {
  const cached = path2DCache.get(d);
  if (cached !== undefined || path2DCache.has(d)) {
    path2DCache.delete(d);
    path2DCache.set(d, cached ?? null);
    return cached ?? null;
  }
  if (typeof Path2D === 'undefined') {
    path2DCache.set(d, null);
    return null;
  }
  try {
    const path = new Path2D(d);
    path2DCache.set(d, path);
    if (path2DCache.size > PATH_PARSE_CACHE_LIMIT) {
      const oldestKey = path2DCache.keys().next().value;
      if (oldestKey !== undefined) {
        path2DCache.delete(oldestKey);
      }
    }
    return path;
  } catch {
    path2DCache.set(d, null);
    return null;
  }
}

function probeTransformedPath2D(): boolean {
  if (
    typeof DOMMatrix === 'undefined' ||
    typeof Path2D === 'undefined' ||
    typeof document === 'undefined'
  ) {
    return false;
  }

  try {
    const canvas = document.createElement('canvas');
    canvas.width = 48;
    canvas.height = 48;
    const ctx = canvas.getContext('2d');
    if (!ctx || typeof ctx.getImageData !== 'function') {
      return false;
    }

    const sourcePath = new Path2D('M 1 1 L 7 1 L 7 7 L 1 7 Z');
    const destPath = new Path2D();
    const matrix = new DOMMatrix()
      .translateSelf(24, 18)
      .rotateSelf(20)
      .scaleSelf(1.75, 1.25);
    destPath.addPath(sourcePath, matrix);

    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.strokeStyle = '#000000';
    ctx.lineWidth = 2;
    ctx.stroke(destPath);

    const transformedHasInk = hasAlpha(ctx.getImageData(18, 12, 18, 18).data);
    const sourceHasInk = hasAlpha(ctx.getImageData(0, 0, 12, 12).data);
    return transformedHasInk && !sourceHasInk;
  } catch {
    return false;
  }
}

function hasAlpha(data: Uint8ClampedArray): boolean {
  for (let i = 3; i < data.length; i += 4) {
    if (data[i] > 0) {
      return true;
    }
  }
  return false;
}

export function supportsTransformedPath2DFastPath(): boolean {
  if (transformedPath2DSupport === undefined) {
    transformedPath2DSupport = probeTransformedPath2D();
    if (!transformedPath2DSupport) {
      console.warn('Beam Bench: transformed Path2D fast path unavailable; using vector fallback renderer');
    }
  }
  return transformedPath2DSupport;
}

export function resetTransformedPath2DProbeForTests(): void {
  transformedPath2DSupport = undefined;
}

export function getVectorPathRenderInfo(pathData: string): VectorPathRenderInfo {
  const commands = getOrCreateParsedPathCommands(pathData);
  return {
    commands,
    bbox: computePathBBox(commands),
    path2d: getCachedPath2D(pathData),
    commandCount: commands.length,
  };
}

export function getVectorPathRenderInfoForData(data: VectorPathObjectData): VectorPathRenderInfo {
  const cached = vectorPathRenderInfoCache.get(data);
  if (cached) {
    return cached;
  }

  const info = getVectorPathRenderInfo(data.path_data);
  vectorPathRenderInfoCache.set(data, info);
  return info;
}

export function getVectorPathRenderInfoForObject(obj: ProjectObject): VectorPathRenderInfo | null {
  if (obj.data.type !== 'vector_path') {
    return null;
  }
  return getVectorPathRenderInfoForData(obj.data);
}

export function getVectorPathCommandCount(pathData: string): number {
  return getVectorPathRenderInfo(pathData).commandCount;
}

export function getVectorPathCommandCountForObject(obj: ProjectObject): number {
  const info = getVectorPathRenderInfoForObject(obj);
  return info?.commandCount ?? 0;
}

export function resetVectorPathCachesForTests(): void {
  pathCommandCache.clear();
  path2DCache.clear();
  pathBBoxCache = new WeakMap<PathCommand[], PathBBox>();
  vectorPathRenderInfoCache = new WeakMap<VectorPathObjectData, VectorPathRenderInfo>();
}

export function cubicAt(p0: number, p1: number, p2: number, p3: number, t: number): number {
  const mt = 1 - t;
  return mt * mt * mt * p0 + 3 * mt * mt * t * p1 + 3 * mt * t * t * p2 + t * t * t * p3;
}

export function quadAt(p0: number, p1: number, p2: number, t: number): number {
  const mt = 1 - t;
  return mt * mt * p0 + 2 * mt * t * p1 + t * t * p2;
}

/**
 * Compute the visible geometry bbox of path commands.
 * Unlike a naive control-point bbox, this samples curve geometry so
 * the canvas matches planner/preview behavior for paths whose handles
 * extend beyond the actual contour.
 */
export function computePathBBox(commands: PathCommand[]): PathBBox {
  const cached = pathBBoxCache.get(commands);
  if (cached) {
    return cached;
  }

  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  let currX = 0;
  let currY = 0;
  let startX = 0;
  let startY = 0;

  const include = (x: number, y: number) => {
    minX = Math.min(minX, x);
    minY = Math.min(minY, y);
    maxX = Math.max(maxX, x);
    maxY = Math.max(maxY, y);
  };

  for (const cmd of commands) {
    switch (cmd.type) {
      case 'M':
        currX = cmd.x;
        currY = cmd.y;
        startX = cmd.x;
        startY = cmd.y;
        include(currX, currY);
        break;
      case 'L':
        include(currX, currY);
        include(cmd.x, cmd.y);
        currX = cmd.x;
        currY = cmd.y;
        break;
      case 'Q': {
        include(currX, currY);
        const steps = 32;
        for (let i = 1; i <= steps; i++) {
          const t = i / steps;
          include(quadAt(currX, cmd.x1!, cmd.x, t), quadAt(currY, cmd.y1!, cmd.y, t));
        }
        currX = cmd.x;
        currY = cmd.y;
        break;
      }
      case 'C': {
        include(currX, currY);
        const steps = 48;
        for (let i = 1; i <= steps; i++) {
          const t = i / steps;
          include(
            cubicAt(currX, cmd.x1!, cmd.x2!, cmd.x, t),
            cubicAt(currY, cmd.y1!, cmd.y2!, cmd.y, t),
          );
        }
        currX = cmd.x;
        currY = cmd.y;
        break;
      }
      case 'Z':
        include(currX, currY);
        include(startX, startY);
        currX = startX;
        currY = startY;
        break;
    }
  }

  if (!isFinite(minX)) {
    const emptyBBox = { minX: 0, minY: 0, maxX: 0, maxY: 0, width: 0, height: 0 };
    pathBBoxCache.set(commands, emptyBBox);
    return emptyBBox;
  }

  const bbox = {
    minX,
    minY,
    maxX,
    maxY,
    width: maxX - minX,
    height: maxY - minY,
  };
  pathBBoxCache.set(commands, bbox);
  return bbox;
}

/** Map a path-data-space coordinate to bounds-space */
export function mapPathCoordToBounds(
  x: number,
  y: number,
  pathBBox: PathBBox,
  boundsMinX: number,
  boundsMinY: number,
  boundsW: number,
  boundsH: number,
): { x: number; y: number } {
  const nx =
    pathBBox.width > 0
      ? ((x - pathBBox.minX) / pathBBox.width) * boundsW + boundsMinX
      : boundsMinX + boundsW / 2;
  const ny =
    pathBBox.height > 0
      ? ((y - pathBBox.minY) / pathBBox.height) * boundsH + boundsMinY
      : boundsMinY + boundsH / 2;
  return { x: nx, y: ny };
}

/** Convert EditablePath[] back to an SVG path `d` string (inverse of get_editable_path) */
export function editablePathsToSvgD(paths: EditablePath[]): string {
  const parts: string[] = [];
  for (const path of paths) {
    const { nodes, closed } = path;
    if (nodes.length === 0) continue;

    // First node: MoveTo
    parts.push(`M ${nodes[0].position.x} ${nodes[0].position.y}`);

    // Subsequent nodes: determine curve type from handles
    for (let i = 1; i < nodes.length; i++) {
      const prev = nodes[i - 1];
      const curr = nodes[i];
      const hasOut = prev.handle_out != null;
      const hasIn = curr.handle_in != null;

      if (hasOut && hasIn) {
        // Cubic bezier
        parts.push(
          `C ${prev.handle_out!.x} ${prev.handle_out!.y} ${curr.handle_in!.x} ${curr.handle_in!.y} ${curr.position.x} ${curr.position.y}`,
        );
      } else if (hasOut) {
        // Quadratic with control = handle_out
        parts.push(
          `Q ${prev.handle_out!.x} ${prev.handle_out!.y} ${curr.position.x} ${curr.position.y}`,
        );
      } else if (hasIn) {
        // Quadratic with control = handle_in
        parts.push(
          `Q ${curr.handle_in!.x} ${curr.handle_in!.y} ${curr.position.x} ${curr.position.y}`,
        );
      } else {
        // Line
        parts.push(`L ${curr.position.x} ${curr.position.y}`);
      }
    }

    if (closed && nodes.length > 1) {
      // Emit the closing segment (last node → first node) as a curve if handles exist
      const last = nodes[nodes.length - 1];
      const first = nodes[0];
      const hasOut = last.handle_out != null;
      const hasIn = first.handle_in != null;

      if (hasOut && hasIn) {
        parts.push(
          `C ${last.handle_out!.x} ${last.handle_out!.y} ${first.handle_in!.x} ${first.handle_in!.y} ${first.position.x} ${first.position.y}`,
        );
      } else if (hasOut) {
        parts.push(
          `Q ${last.handle_out!.x} ${last.handle_out!.y} ${first.position.x} ${first.position.y}`,
        );
      } else if (hasIn) {
        parts.push(
          `Q ${first.handle_in!.x} ${first.handle_in!.y} ${first.position.x} ${first.position.y}`,
        );
      }
      // If neither handle exists, bare Z handles the straight-line close
      parts.push('Z');
    } else if (closed) {
      parts.push('Z');
    }
  }
  return parts.join(' ');
}

export function parsePathData(d: string): PathCommand[] {
  const cached = getCachedParsedPathCommands(d);
  if (cached) {
    return cached;
  }

  const commands: PathCommand[] = [];
  const tokens = d.match(/[MLQCZ]|[-+]?[0-9]*\.?[0-9]+/gi);
  if (!tokens) return rememberParsedPath(d, commands);

  let i = 0;
  const num = () => parseFloat(tokens[i++]);

  while (i < tokens.length) {
    const cmd = tokens[i++].toUpperCase();
    switch (cmd) {
      case 'M':
        commands.push({ type: 'M', x: num(), y: num() });
        break;
      case 'L':
        commands.push({ type: 'L', x: num(), y: num() });
        break;
      case 'Q':
        commands.push({ type: 'Q', x1: num(), y1: num(), x: num(), y: num() });
        break;
      case 'C':
        commands.push({
          type: 'C',
          x1: num(),
          y1: num(),
          x2: num(),
          y2: num(),
          x: num(),
          y: num(),
        });
        break;
      case 'Z':
        commands.push({ type: 'Z', x: 0, y: 0 });
        break;
    }
  }

  return rememberParsedPath(d, commands);
}
