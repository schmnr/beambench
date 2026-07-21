import type { Workspace, ProjectObject, Layer, Point2D } from '../types/project';
import { worldToScreen, pxPerMm, type ViewportParams } from './ViewportTransform';
import type { EditablePath, NodeId, NodeSelectionTarget } from '../types/vector';
import type { PreviewData } from '../types/preview';
import type { CanvasTheme } from './constants';
import { DARK_THEME } from './constants';
import type { PreviewState } from '../stores/previewStore';
import { drawBed, drawGrid, drawOrigin, drawRulers } from './drawWorkspace';
import {
  applyTransform,
  buildObjectScreenMaskPath,
  drawObject,
  drawRasterImage,
  drawEditableVectorPath,
  getPathSubpathRenderStates,
  getVectorPathCommandCountForObject,
  getVectorPathRenderInfoForObject,
  supportsTransformedPath2DFastPath,
  type PathCommand,
  type RasterMaskRenderContext,
} from './drawObjects';
import {
  drawSelectionHighlight,
  drawRubberBand,
  drawShapePreview,
  drawNodeHandles,
  drawHoveredSegment,
  drawMeasureLine,
  drawPolygonPreview,
  drawStarPreview,
  drawPenPreview,
  drawTrimPreview,
  drawTabMarkers,
  drawRadiusCornerMarkers,
  drawStartPointOverlay,
  drawMeshDeformOverlay,
} from './drawSelection';
import {
  drawVectorPreview,
  drawRasterPreview,
  drawTravelPreview,
  drawFramePreview,
} from './drawPreview';
import { previewViewportForWorkspace } from './previewViewport';
import { PreviewBitmapCache } from './previewBitmapCache';
import { useNotificationStore } from '../stores/notificationStore';
import i18n from '../i18n';
import { getCachedTransformedBoundsWorld } from './sceneIndex';
import { measureCanvasPerf } from './canvasPerf';
import {
  CAMERA_OVERLAY_HANDLE_SIZE_PX,
  cameraOverlayScreenHandleGeometry,
  cameraOverlayCanvasTransform,
  type CameraOverlayRenderParams,
} from './cameraOverlay';
import type { MeasurementDragMetrics, MeasurementSegmentMetrics } from './measurement';

export type ToolOverlay =
  | { type: 'none' }
  | { type: 'rubber-band'; startScreen: Point2D; endScreen: Point2D; crossing?: boolean }
  | {
      type: 'shape-preview';
      // World coordinates, converted at draw time with the CURRENT viewport
      // so zooming/panning mid-draw keeps the preview at true size.
      startWorld: Point2D;
      endWorld: Point2D;
      kind: 'rectangle' | 'ellipse' | 'polygon' | 'star';
      sides?: number;
      bulge?: number;
      ratio?: number;
      ratio2?: number;
      dualRadius?: boolean;
      cornerRadius?: number;
    }
  | {
      type: 'node-edit';
      paths: EditablePath[];
      selectedTargets: NodeSelectionTarget[];
      primaryTarget: NodeSelectionTarget | null;
      nodeToWorld?: (pos: { x: number; y: number }) => { x: number; y: number };
      objectId?: string;
      suspendPreview?: boolean;
      hoveredSegment?: { nodeId: NodeId; t: number } | null;
      hoveredEndpoint?: NodeId | null;
      joinTargetNodeId?: NodeId | null;
      selectionRect?: { startScreen: Point2D; endScreen: Point2D; crossing?: boolean } | null;
    }
  | { type: 'snap-guides'; guides: { axis: 'x' | 'y'; value: number }[] }
  | { type: 'ruler-guide-preview'; axis: 'horizontal' | 'vertical'; value: number }
  | { type: 'offset-preview'; paths: { points: Point2D[]; closed: boolean }[] }
  | {
      type: 'measure-line';
      startScreen: Point2D;
      endScreen: Point2D;
      distanceMm: number;
      angleDeg: number;
    }
  | {
      type: 'measure-inspection';
      hoverObjectId?: string;
      hoverSegment?: MeasurementSegmentMetrics | null;
      drag?: MeasurementDragMetrics;
    }
  | { type: 'trim-preview'; segmentScreenPoints: Point2D[] }
  | {
      type: 'pen-preview';
      points: { anchor: Point2D; handleIn?: Point2D; handleOut?: Point2D }[];
      screenPoints: { anchor: Point2D; handleIn?: Point2D; handleOut?: Point2D }[];
      currentScreen: Point2D;
      dragging: boolean;
      closed: boolean;
    }
  | {
      type: 'tab-markers';
      objectId: string;
      markers: { worldX: number; worldY: number; hovered: boolean }[];
    }
  | {
      type: 'radius-corners';
      objectId: string;
      markers: { worldX: number; worldY: number; hovered: boolean; alreadyFilleted: boolean }[];
    }
  | {
      type: 'start-point-pick';
      vertices: {
        worldX: number;
        worldY: number;
        isStart: boolean;
        subpathIndex: number;
        vertexIndex: number;
        subpathClosed: boolean;
      }[];
      subpathArrows: Array<{
        subpathIndex: number;
        fromX: number;
        fromY: number;
        toX: number;
        toY: number;
        isCustom: boolean;
      }>;
      hoveredIndex: number | null;
    }
  | {
      type: 'mesh-deform';
      gridSize: number;
      handles: {
        worldX: number;
        worldY: number;
        active?: boolean;
        hovered?: boolean;
      }[];
    }
  | {
      type: 'camera-overlay-adjust';
      widthPx: number;
      heightPx: number;
      transform: CameraOverlayRenderParams['transform'];
    };

export interface RenderParams {
  workspace: Workspace;
  objects: ProjectObject[];
  layers: Layer[];
  selectedObjectIds: string[];
  vp: ViewportParams;
  gridVisible: boolean;
  gridSpacingMm: number;
  toolOverlay: ToolOverlay;
  previewData?: PreviewData | null;
  showPreview?: boolean;
  previewState?: PreviewState;
  cameraOverlay?: CameraOverlayRenderParams | null;
  previewManualRefreshRequired?: boolean;
  theme?: CanvasTheme;
  antialiasing?: boolean;
  filledRendering?: boolean;
  selectionDashOffset?: number;
  showLastPosition?: boolean;
  laserPosition?: Point2D | null;
  /** Object ID to skip rendering (e.g. text being edited inline) */
  skipObjectId?: string | null;
  /** Always-visible tab markers for objects with tabs (filtered to exclude active tool target) */
  persistentTabMarkers?: { worldX: number; worldY: number }[];
  /** Display unit for rulers and measurement overlays */
  displayUnit?: 'mm' | 'inches';
  /** Transform locks for hiding handles */
  transformLocks?: import('../types/project').TransformLocks;
  /** M4: layer id to briefly highlight ("Flash content"). Tints all objects on that layer
   *  with a gold outline. Selection state is untouched. */
  flashedLayerId?: string | null;
  /** Transient interaction state used for gesture-time proxy rendering. */
  interactionState?: CanvasInteractionState;
}

export interface CanvasInteractionState {
  active: boolean;
  kind: 'none' | 'object-drag' | 'pan' | 'zoom';
  objectIds?: string[];
}

// Default layer colors for preview rendering
const DEFAULT_LAYER_COLORS = [
  '#ff6b6b',
  '#ffa94d',
  '#ffd43b',
  '#69db7c',
  '#4dabf7',
  '#9775fa',
  '#f783ac',
  '#20c997',
];

/**
 * Cached raster image. During load this is the raw `HTMLImageElement`.
 * After load it is replaced with a pre-grayscaled `HTMLCanvasElement` so
 * the main canvas renders a grayscale positioning proxy without having to
 * rely on runtime `ctx.filter` (which has been flaky on some WebKit builds).
 */
export type CachedRasterImage = HTMLImageElement | HTMLCanvasElement;

type VectorProxyBitmap = HTMLCanvasElement;

type VectorProxyEntry = {
  bitmap: VectorProxyBitmap;
  tier: number;
};

type ScreenRect = {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
};

type BaseSceneObjectSnapshot = {
  rect: ScreenRect;
  dataRef: ProjectObject['data'];
  layerId: string;
  zIndex: number;
};

const HEAVY_VECTOR_COMMAND_THRESHOLD = 5000;
const VECTOR_PROXY_CACHE_LIMIT = 8;
const VECTOR_PROXY_MIN_TIER = 512;
const VECTOR_PROXY_MAX_TIER = 2048;
const VIEWPORT_CULL_MARGIN_PX = 48;
const BASE_DIRTY_RECT_PADDING_PX = 24;
const MEASURE_OBJECT_STROKE = '#22c55e';
const MEASURE_SEGMENT_STROKE = '#a855f7';

export class CanvasRenderer {
  private ctx: CanvasRenderingContext2D;
  private imageCache: Map<string, CachedRasterImage> = new Map();
  private imageErrorCache: Map<string, string> = new Map();
  barcodePathCache: Map<string, string> = new Map();
  barcodeErrorCache: Map<string, string> = new Map();
  private barcodeLoading: Set<string> = new Set();
  private vectorProxyCache: Map<string, VectorProxyEntry> = new Map();
  private cameraOverlayImageCache: Map<string, HTMLImageElement> = new Map();
  private cameraOverlayImageErrorCache: Map<string, string> = new Map();
  private vectorPathIds: WeakMap<PathCommand[], number> = new WeakMap();
  private nextVectorPathId = 1;
  private renderCallback: (() => void) | null = null;
  private lastBaseSceneSignature: string | null = null;
  private lastBaseObjectSnapshots: Map<string, BaseSceneObjectSnapshot> = new Map();
  /** Cache of decoded processed-raster preview bitmaps + burned-mask
   *  offscreens for animation. Cleared on plan rebuild. */
  readonly previewBitmapCache: PreviewBitmapCache = new PreviewBitmapCache();

  constructor(ctx: CanvasRenderingContext2D) {
    this.ctx = ctx;
    // Re-trigger a render when a preview bitmap finishes decoding.
    this.previewBitmapCache.setOnLoad(() => this.renderCallback?.());
  }

  setRenderCallback(cb: () => void): void {
    this.renderCallback = cb;
  }

  /** Revoke blob URLs + drop cached bitmaps and burned masks. Called
   *  from Canvas.tsx whenever previewData changes (new plan build). */
  clearPreviewBitmapCache(): void {
    this.previewBitmapCache.clear();
  }

  /** Release all transient resources. Called from Canvas.tsx's
   *  renderer useEffect cleanup. */
  dispose(): void {
    this.clearImageCache();
    this.barcodePathCache.clear();
    this.barcodeErrorCache.clear();
    this.barcodeLoading.clear();
    this.vectorProxyCache.clear();
    this.clearCameraOverlayImageCache();
    this.lastBaseSceneSignature = null;
    this.lastBaseObjectSnapshots.clear();
    this.previewBitmapCache.clear();
  }

  clearImageCache(): void {
    this.imageCache.clear();
    this.imageErrorCache.clear();
  }

  clearCameraOverlayImageCache(): void {
    this.cameraOverlayImageCache.clear();
    this.cameraOverlayImageErrorCache.clear();
  }

  markImageLoadError(assetKey: string, error: string): void {
    this.imageCache.delete(assetKey);
    this.imageErrorCache.set(assetKey, error);
  }

  clearImageLoadError(assetKey: string): void {
    this.imageErrorCache.delete(assetKey);
  }

  /**
   * Ensure an image is loaded for the given asset key. The cached entry is
   * upgraded from an `HTMLImageElement` (during load) to a pre-grayscaled
   * `HTMLCanvasElement` once the source image finishes decoding.
   */
  ensureImage(assetKey: string, blobUrl: string): void {
    if (this.imageCache.has(assetKey)) return;

    const img = new Image();
    img.onload = () => {
      const gray = buildGrayscaleCanvas(img);
      if (gray) {
        this.imageCache.set(assetKey, gray);
      }
      this.imageErrorCache.delete(assetKey);
      this.renderCallback?.();
    };
    img.onerror = () => {
      this.imageCache.delete(assetKey);
      this.imageErrorCache.set(assetKey, 'Image decode failed');
      this.renderCallback?.();
    };
    img.src = blobUrl;
    this.imageCache.set(assetKey, img);
  }

  ensureCameraOverlayImage(frameHandleId: string, assetUrl: string): void {
    if (this.cameraOverlayImageCache.has(frameHandleId)) return;

    const img = new Image();
    img.crossOrigin = 'anonymous';
    img.onload = () => {
      for (const key of [...this.cameraOverlayImageCache.keys()]) {
        if (key !== frameHandleId) {
          this.cameraOverlayImageCache.delete(key);
        }
      }
      this.cameraOverlayImageErrorCache.delete(frameHandleId);
      this.renderCallback?.();
    };
    img.onerror = () => {
      this.cameraOverlayImageCache.delete(frameHandleId);
      this.cameraOverlayImageErrorCache.set(frameHandleId, 'Camera overlay image decode failed');
      this.renderCallback?.();
    };
    img.src = assetUrl;
    this.cameraOverlayImageCache.set(frameHandleId, img);
  }

  /** Ensure barcode SVG path data is generated for the given barcode object. Returns the d string if ready. */
  ensureBarcodePathData(obj: ProjectObject): string | null {
    if (obj.data.type !== 'barcode') return null;
    const { barcode_type, data, width, height, options } = obj.data;
    const normalizedOptions = {
      show_text: options?.show_text ?? false,
      qr_error_correction: options?.qr_error_correction ?? 'medium',
      data_matrix_force_square: options?.data_matrix_force_square ?? false,
    };
    const cacheKey = `${barcode_type}:${data}:${width}:${height}:${JSON.stringify(normalizedOptions)}`;

    const existing = this.barcodePathCache.get(cacheKey);
    if (existing !== undefined) return existing;
    if (this.barcodeErrorCache.has(cacheKey)) return null;

    if (this.barcodeLoading.has(cacheKey)) return null;

    this.barcodeLoading.add(cacheKey);
    import('@tauri-apps/api/core')
      .then(({ invoke }) =>
        invoke<string>('generate_barcode', {
          barcodeType: barcode_type,
          data,
          width,
          height,
          options: normalizedOptions,
        }),
      )
      .then((pathD) => {
        this.barcodePathCache.set(cacheKey, pathD);
        this.barcodeErrorCache.delete(cacheKey);
        this.barcodeLoading.delete(cacheKey);
        this.renderCallback?.();
      })
      .catch((error) => {
        const message = String(error);
        this.barcodeErrorCache.set(cacheKey, message);
        this.barcodeLoading.delete(cacheKey);
        useNotificationStore.getState().push(i18n.t('notifications.barcode_render_failed', { detail: message }), 'error');
        this.renderCallback?.();
      });

    return null;
  }

  private getVectorPathIdentity(commands: PathCommand[]): number {
    const existing = this.vectorPathIds.get(commands);
    if (existing !== undefined) {
      return existing;
    }
    const id = this.nextVectorPathId++;
    this.vectorPathIds.set(commands, id);
    return id;
  }

  private isHeavyVectorObject(obj: ProjectObject): boolean {
    if (obj.data.type !== 'vector_path') return false;
    return getVectorPathCommandCountForObject(obj) >= HEAVY_VECTOR_COMMAND_THRESHOLD;
  }

  private getProxyTierForObject(obj: ProjectObject, vp: ViewportParams): number {
    const scale = pxPerMm(vp.zoom);
    const projected = Math.max(
      Math.abs(obj.bounds.max.x - obj.bounds.min.x) * scale,
      Math.abs(obj.bounds.max.y - obj.bounds.min.y) * scale,
      1,
    );
    const desired = Math.max(
      VECTOR_PROXY_MIN_TIER,
      Math.min(VECTOR_PROXY_MAX_TIER, projected * 2),
    );
    if (desired <= 512) return 512;
    if (desired <= 1024) return 1024;
    return 2048;
  }

  private touchVectorProxyCache(key: string, entry: VectorProxyEntry): VectorProxyEntry {
    if (this.vectorProxyCache.has(key)) {
      this.vectorProxyCache.delete(key);
    }
    this.vectorProxyCache.set(key, entry);
    while (this.vectorProxyCache.size > VECTOR_PROXY_CACHE_LIMIT) {
      const oldest = this.vectorProxyCache.keys().next().value;
      if (oldest === undefined) break;
      this.vectorProxyCache.delete(oldest);
    }
    return entry;
  }

  private buildVectorProxyKey(
    obj: ProjectObject,
    color: string,
    filled: boolean,
    tier: number,
  ): string | null {
    if (obj.data.type !== 'vector_path') return null;
    const info = getVectorPathRenderInfoForObject(obj);
    if (!info) return null;
    return `${this.getVectorPathIdentity(info.commands)}:${filled ? 1 : 0}:${color}:${tier}`;
  }

  private createVectorProxyBitmap(
    obj: ProjectObject,
    color: string,
    filled: boolean,
    tier: number,
    vp: ViewportParams,
  ): VectorProxyBitmap | null {
    if (obj.data.type !== 'vector_path') return null;
    const info = getVectorPathRenderInfoForObject(obj);
    if (!info) return null;
    const bbox = info.bbox;
    const longestLocal = Math.max(bbox.width, bbox.height);
    if (longestLocal <= 0) return null;
    const subpaths = getPathSubpathRenderStates(info.commands);
    const hasClosedSubpath = subpaths.some((subpath) => subpath.closed);
    const canUseWholePathFill = !filled || !hasClosedSubpath || subpaths.every((subpath) => subpath.closed);

    const vpScale = pxPerMm(vp.zoom);
    const projectedMax = Math.max(
      Math.abs(obj.bounds.max.x - obj.bounds.min.x) * vpScale,
      Math.abs(obj.bounds.max.y - obj.bounds.min.y) * vpScale,
      1,
    );
    const oversample = tier / projectedMax;
    const scale = tier / longestLocal;
    const canvas = document.createElement('canvas');
    canvas.width = Math.max(1, Math.round(Math.max(bbox.width, 1e-6) * scale));
    canvas.height = Math.max(1, Math.round(Math.max(bbox.height, 1e-6) * scale));
    const ctx = canvas.getContext('2d');
    if (!ctx) return null;

    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.strokeStyle = color;
    ctx.lineWidth = 1.5 * oversample;
    if (supportsTransformedPath2DFastPath() && info.path2d && canUseWholePathFill) {
      const mapped = new Path2D();
      mapped.addPath(
        info.path2d,
        new DOMMatrix([
          scale,
          0,
          0,
          scale,
          -bbox.minX * scale,
          -bbox.minY * scale,
        ]),
      );
      if (filled && hasClosedSubpath) {
        ctx.fillStyle = color + 'B0';
        ctx.fill(mapped);
      }
      ctx.stroke(mapped);
      return canvas;
    }

    const mapLocal = (x: number, y: number) => ({
      x: (x - bbox.minX) * scale,
      y: (y - bbox.minY) * scale,
    });

    const drawCommands = (commands: PathCommand[], forceClose: boolean): void => {
      for (const cmd of commands) {
        switch (cmd.type) {
          case 'M': {
            const pt = mapLocal(cmd.x, cmd.y);
            ctx.moveTo(pt.x, pt.y);
            break;
          }
          case 'L': {
            const pt = mapLocal(cmd.x, cmd.y);
            ctx.lineTo(pt.x, pt.y);
            break;
          }
          case 'Q': {
            const cp = mapLocal(cmd.x1!, cmd.y1!);
            const pt = mapLocal(cmd.x, cmd.y);
            ctx.quadraticCurveTo(cp.x, cp.y, pt.x, pt.y);
            break;
          }
          case 'C': {
            const cp1 = mapLocal(cmd.x1!, cmd.y1!);
            const cp2 = mapLocal(cmd.x2!, cmd.y2!);
            const pt = mapLocal(cmd.x, cmd.y);
            ctx.bezierCurveTo(cp1.x, cp1.y, cp2.x, cp2.y, pt.x, pt.y);
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
    };

    if (filled && hasClosedSubpath) {
      ctx.beginPath();
      for (const subpath of subpaths) {
        if (subpath.closed) {
          drawCommands(subpath.commands, true);
        }
      }
      ctx.fillStyle = color + 'B0';
      ctx.fill('evenodd');
    }

    ctx.beginPath();
    for (const subpath of subpaths) {
      drawCommands(subpath.commands, subpath.closed);
    }
    ctx.stroke();
    return canvas;
  }

  private getOrCreateVectorProxy(
    obj: ProjectObject,
    color: string,
    filled: boolean,
    vp: ViewportParams,
  ): VectorProxyEntry | null {
    const tier = this.getProxyTierForObject(obj, vp);
    const key = this.buildVectorProxyKey(obj, color, filled, tier);
    if (!key) return null;

    const cached = this.vectorProxyCache.get(key);
    if (cached) {
      return this.touchVectorProxyCache(key, cached);
    }

    const bitmap = this.createVectorProxyBitmap(obj, color, filled, tier, vp);
    if (!bitmap) return null;
    return this.touchVectorProxyCache(key, { bitmap, tier });
  }

  private drawVectorProxy(
    ctx: CanvasRenderingContext2D,
    obj: ProjectObject,
    color: string,
    vp: ViewportParams,
    filled: boolean,
  ): boolean {
    const proxy = this.getOrCreateVectorProxy(obj, color, filled, vp);
    if (!proxy) return false;

    const topLeft = worldToScreen(obj.bounds.min, vp);
    const scale = pxPerMm(vp.zoom);
    const w = Math.abs(obj.bounds.max.x - obj.bounds.min.x) * scale;
    const h = Math.abs(obj.bounds.max.y - obj.bounds.min.y) * scale;
    if (w <= 0 || h <= 0) return false;

    ctx.save();
    applyTransform(ctx, obj.transform, vp, obj.bounds);
    ctx.drawImage(proxy.bitmap, topLeft.x, topLeft.y, w, h);
    ctx.restore();
    return true;
  }

  private shouldUseVectorProxy(obj: ProjectObject, _interactionState?: CanvasInteractionState): boolean {
    // Heavy imported SVG paths are expensive even while the scene is idle.
    // Keep the proxy always-on so redraws do not fall back to replaying
    // thousands of path commands on every base-scene paint.
    return this.isHeavyVectorObject(obj);
  }

  private isObjectCulled(obj: ProjectObject, vp: ViewportParams): boolean {
    const bounds = getCachedTransformedBoundsWorld(obj);
    const corners = [
      worldToScreen(bounds.min, vp),
      worldToScreen({ x: bounds.max.x, y: bounds.min.y }, vp),
      worldToScreen(bounds.max, vp),
      worldToScreen({ x: bounds.min.x, y: bounds.max.y }, vp),
    ];
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    for (const corner of corners) {
      minX = Math.min(minX, corner.x);
      minY = Math.min(minY, corner.y);
      maxX = Math.max(maxX, corner.x);
      maxY = Math.max(maxY, corner.y);
    }

    return (
      maxX < -VIEWPORT_CULL_MARGIN_PX ||
      maxY < -VIEWPORT_CULL_MARGIN_PX ||
      minX > vp.canvasWidth + VIEWPORT_CULL_MARGIN_PX ||
      minY > vp.canvasHeight + VIEWPORT_CULL_MARGIN_PX
    );
  }

  private resolveRenderableDataRef(
    obj: ProjectObject,
    allObjects: ProjectObject[],
  ): ProjectObject['data'] {
    return this.resolveConcreteObject(obj, allObjects)?.data ?? obj.data;
  }

  private resolveConcreteObject(
    obj: ProjectObject,
    allObjects: ProjectObject[],
  ): ProjectObject | null {
    if (obj.data.type !== 'virtual_clone') {
      return obj;
    }
    let sourceId = obj.data.source_id;
    let sourceObj = allObjects.find((candidate) => candidate.id === sourceId);
    let depth = 0;
    while (sourceObj && sourceObj.data.type === 'virtual_clone' && depth <= 10) {
      sourceId = sourceObj.data.source_id;
      sourceObj = allObjects.find((candidate) => candidate.id === sourceId);
      depth++;
    }
    if (!sourceObj || sourceObj.data.type === 'virtual_clone') {
      return null;
    }
    return {
      ...sourceObj,
      id: obj.id,
      bounds: obj.bounds,
      transform: obj.transform,
      layer_id: obj.layer_id,
    };
  }

  private buildRasterMaskRenderContext(
    obj: ProjectObject,
    allObjects: ProjectObject[],
    vp: ViewportParams,
  ): RasterMaskRenderContext | null {
    if (obj.data.type !== 'raster_image') return null;
    const masks = obj.data.masks ?? [];
    if (masks.length === 0) return null;

    const context: RasterMaskRenderContext = { inside: [], outside: [] };
    for (const mask of masks) {
      const maskObj = allObjects.find((candidate) => candidate.id === mask.object_id);
      if (!maskObj) continue;
      const resolvedMask = this.resolveConcreteObject(maskObj, allObjects);
      if (!resolvedMask) continue;
      const path = buildObjectScreenMaskPath(resolvedMask, vp);
      if (!path) continue;
      if (mask.polarity === 'keep_outside') {
        context.outside.push(path);
      } else {
        context.inside.push(path);
      }
    }

    return context.inside.length > 0 || context.outside.length > 0 ? context : null;
  }

  private drawObjectWithRasterMasks(
    ctx: CanvasRenderingContext2D,
    obj: ProjectObject,
    layer: Layer,
    vp: ViewportParams,
    allObjects: ProjectObject[],
    theme: CanvasTheme,
    filled: boolean | undefined,
    isToolLayer: boolean,
  ): void {
    if (obj.data.type === 'raster_image') {
      if (isToolLayer) {
        ctx.save();
        ctx.globalAlpha = 0.6;
        ctx.setLineDash([8, 4]);
      }
      drawRasterImage(
        ctx,
        obj,
        layer.color_tag,
        vp,
        this.imageCache,
        this.imageErrorCache,
        this.buildRasterMaskRenderContext(obj, allObjects, vp),
      );
      if (isToolLayer) {
        ctx.restore();
      }
      return;
    }

    drawObject(
      ctx,
      obj,
      layer,
      vp,
      this.imageCache,
      theme,
      filled,
      isToolLayer,
      this.barcodePathCache,
      this.imageErrorCache,
    );
  }

  private getObjectScreenRect(obj: ProjectObject, vp: ViewportParams): ScreenRect {
    const bounds = getCachedTransformedBoundsWorld(obj);
    const min = worldToScreen(bounds.min, vp);
    const max = worldToScreen(bounds.max, vp);
    return {
      minX: Math.min(min.x, max.x) - BASE_DIRTY_RECT_PADDING_PX,
      minY: Math.min(min.y, max.y) - BASE_DIRTY_RECT_PADDING_PX,
      maxX: Math.max(min.x, max.x) + BASE_DIRTY_RECT_PADDING_PX,
      maxY: Math.max(min.y, max.y) + BASE_DIRTY_RECT_PADDING_PX,
    };
  }

  private clampScreenRect(rect: ScreenRect, vp: ViewportParams): ScreenRect | null {
    const clamped: ScreenRect = {
      minX: Math.max(0, Math.floor(rect.minX)),
      minY: Math.max(0, Math.floor(rect.minY)),
      maxX: Math.min(vp.canvasWidth, Math.ceil(rect.maxX)),
      maxY: Math.min(vp.canvasHeight, Math.ceil(rect.maxY)),
    };
    if (clamped.maxX <= clamped.minX || clamped.maxY <= clamped.minY) {
      return null;
    }
    return clamped;
  }

  private rectEquals(a: ScreenRect, b: ScreenRect): boolean {
    return (
      a.minX === b.minX &&
      a.minY === b.minY &&
      a.maxX === b.maxX &&
      a.maxY === b.maxY
    );
  }

  private unionRects(a: ScreenRect | null, b: ScreenRect): ScreenRect {
    if (!a) return { ...b };
    return {
      minX: Math.min(a.minX, b.minX),
      minY: Math.min(a.minY, b.minY),
      maxX: Math.max(a.maxX, b.maxX),
      maxY: Math.max(a.maxY, b.maxY),
    };
  }

  private shouldForceFullBaseRedraw(): boolean {
    return (
      (
        globalThis as typeof globalThis & {
          __BB_FORCE_FULL_BASE_REDRAW?: boolean;
        }
      ).__BB_FORCE_FULL_BASE_REDRAW === true
    );
  }

  private getBaseSceneSignature(params: RenderParams, previewedLayerIds: Set<string>): string | null {
    if (params.toolOverlay.type === 'node-edit') {
      return null;
    }
    const previewRevision = params.previewData
      ? `${params.previewData.plan_id}:${params.previewData.revision_hash}`
      : 'none';
    const cameraOverlaySignature = params.cameraOverlay
      ? [
          params.cameraOverlay.frameHandleId,
          params.cameraOverlay.widthPx,
          params.cameraOverlay.heightPx,
          params.cameraOverlay.opacity,
          params.cameraOverlay.transform.scale,
          params.cameraOverlay.transform.rotation_deg,
          params.cameraOverlay.transform.translation_x,
          params.cameraOverlay.transform.translation_y,
        ].join(':')
      : 'none';
    const layerSignature = params.layers
      .map((layer) => [
        layer.id,
        layer.enabled !== false ? 1 : 0,
        layer.visible !== false ? 1 : 0,
        layer.color_tag ?? '',
        layer.is_tool_layer === true ? 1 : 0,
      ].join(':'))
      .join('|');
    return [
      params.workspace.bed_width_mm,
      params.workspace.bed_height_mm,
      params.workspace.origin,
      params.vp.offset.x,
      params.vp.offset.y,
      params.vp.zoom,
      params.vp.canvasWidth,
      params.vp.canvasHeight,
      params.gridVisible ? 1 : 0,
      params.gridSpacingMm,
      params.theme?.canvasBg ?? DARK_THEME.canvasBg,
      params.antialiasing !== false ? 1 : 0,
      params.filledRendering === true ? 1 : 0,
      params.displayUnit ?? 'mm',
      params.showPreview ? 1 : 0,
      previewRevision,
      cameraOverlaySignature,
      [...previewedLayerIds].sort().join(','),
      params.skipObjectId ?? '',
      params.flashedLayerId ?? '',
      params.toolOverlay.type === 'mesh-deform' ? 'mesh-deform' : '',
      params.showLastPosition ? 1 : 0,
      params.laserPosition?.x ?? '',
      params.laserPosition?.y ?? '',
      params.interactionState?.active ? 1 : 0,
      params.interactionState?.kind ?? 'none',
      layerSignature,
    ].join('~');
  }

  private buildBaseObjectSnapshots(
    visibleObjects: ProjectObject[],
    allObjects: ProjectObject[],
    vp: ViewportParams,
  ): Map<string, BaseSceneObjectSnapshot> {
    const snapshots = new Map<string, BaseSceneObjectSnapshot>();
    for (const obj of visibleObjects) {
      snapshots.set(obj.id, {
        rect: this.getObjectScreenRect(obj, vp),
        dataRef: this.resolveRenderableDataRef(obj, allObjects),
        layerId: obj.layer_id,
        zIndex: obj.z_index,
      });
    }
    return snapshots;
  }

  private computeDirtyBaseRect(
    current: Map<string, BaseSceneObjectSnapshot>,
    previous: Map<string, BaseSceneObjectSnapshot>,
    vp: ViewportParams,
  ): ScreenRect | null {
    let dirty: ScreenRect | null = null;

    for (const [id, snapshot] of current) {
      const prev = previous.get(id);
      if (!prev) {
        dirty = this.unionRects(dirty, snapshot.rect);
        continue;
      }
      if (
        !this.rectEquals(snapshot.rect, prev.rect) ||
        snapshot.dataRef !== prev.dataRef ||
        snapshot.layerId !== prev.layerId ||
        snapshot.zIndex !== prev.zIndex
      ) {
        dirty = this.unionRects(dirty, snapshot.rect);
        dirty = this.unionRects(dirty, prev.rect);
      }
    }

    for (const [id, snapshot] of previous) {
      if (!current.has(id)) {
        dirty = this.unionRects(dirty, snapshot.rect);
      }
    }

    return dirty ? this.clampScreenRect(dirty, vp) : null;
  }

  private drawCameraOverlay(
    ctx: CanvasRenderingContext2D,
    overlay: CameraOverlayRenderParams | null | undefined,
    vp: ViewportParams,
  ): void {
    if (!overlay || overlay.opacity <= 0) {
      return;
    }
    if (this.cameraOverlayImageErrorCache.has(overlay.frameHandleId)) {
      return;
    }

    const image = this.cameraOverlayImageCache.get(overlay.frameHandleId);
    if (!image || !image.complete || image.naturalWidth === 0) {
      return;
    }

    const transform = cameraOverlayCanvasTransform(overlay.transform, vp);
    ctx.save();
    ctx.globalAlpha *= Math.max(0, Math.min(1, overlay.opacity));
    ctx.translate(transform.translateX, transform.translateY);
    ctx.rotate(transform.rotationRad);
    ctx.scale(transform.scale, transform.scale);
    ctx.drawImage(image, 0, 0, overlay.widthPx, overlay.heightPx);
    ctx.restore();
  }

  private drawCameraOverlayAdjustHandles(
    ctx: CanvasRenderingContext2D,
    widthPx: number,
    heightPx: number,
    transform: CameraOverlayRenderParams['transform'],
    vp: ViewportParams,
  ): void {
    const handles = cameraOverlayScreenHandleGeometry(widthPx, heightPx, transform, vp);
    const half = CAMERA_OVERLAY_HANDLE_SIZE_PX / 2;
    ctx.save();
    ctx.strokeStyle = '#38bdf8';
    ctx.fillStyle = '#0f172a';
    ctx.lineWidth = 1.5;
    ctx.setLineDash([5, 3]);
    ctx.beginPath();
    handles.corners.forEach((handle, index) => {
      if (index === 0) ctx.moveTo(handle.screen.x, handle.screen.y);
      else ctx.lineTo(handle.screen.x, handle.screen.y);
    });
    ctx.closePath();
    ctx.stroke();
    ctx.setLineDash([]);
    for (const handle of handles.corners) {
      ctx.fillRect(
        handle.screen.x - half,
        handle.screen.y - half,
        CAMERA_OVERLAY_HANDLE_SIZE_PX,
        CAMERA_OVERLAY_HANDLE_SIZE_PX,
      );
      ctx.strokeRect(
        handle.screen.x - half,
        handle.screen.y - half,
        CAMERA_OVERLAY_HANDLE_SIZE_PX,
        CAMERA_OVERLAY_HANDLE_SIZE_PX,
      );
    }
    ctx.beginPath();
    ctx.moveTo(
      (handles.corners[0].screen.x + handles.corners[1].screen.x) / 2,
      (handles.corners[0].screen.y + handles.corners[1].screen.y) / 2,
    );
    ctx.lineTo(handles.rotation.x, handles.rotation.y);
    ctx.stroke();
    ctx.beginPath();
    ctx.arc(handles.rotation.x, handles.rotation.y, half, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();
    ctx.restore();
  }

  private drawMeasureInspection(
    ctx: CanvasRenderingContext2D,
    overlay: Extract<ToolOverlay, { type: 'measure-inspection' }>,
    params: RenderParams,
    displayUnit: 'mm' | 'inches',
    theme: CanvasTheme,
  ): void {
    const { objects, layers, vp } = params;
    if (overlay.hoverObjectId) {
      const object = objects.find((candidate) => candidate.id === overlay.hoverObjectId);
      if (object) {
        const sourceLayer = layers.find((layer) => layer.id === object.layer_id);
        const highlightLayer: Layer = sourceLayer
          ? { ...sourceLayer, color_tag: MEASURE_OBJECT_STROKE, is_tool_layer: false }
          : {
              id: 'measure-highlight',
              name: 'Measure Highlight',
              entries: [],
              enabled: true,
              order_index: 0,
              color_tag: MEASURE_OBJECT_STROKE,
              visible: true,
              is_tool_layer: false,
            };
        ctx.save();
        ctx.globalAlpha = 0.95;
        drawObject(
          ctx,
          object,
          highlightLayer,
          vp,
          this.imageCache,
          theme,
          false,
          false,
          this.barcodePathCache,
          this.imageErrorCache,
        );
        ctx.restore();
      }
    }

    if (overlay.hoverSegment) {
      const start = worldToScreen(overlay.hoverSegment.start, vp);
      const end = worldToScreen(overlay.hoverSegment.end, vp);
      ctx.save();
      ctx.strokeStyle = MEASURE_SEGMENT_STROKE;
      ctx.fillStyle = MEASURE_SEGMENT_STROKE;
      ctx.lineWidth = 2.25;
      ctx.beginPath();
      ctx.moveTo(start.x, start.y);
      ctx.lineTo(end.x, end.y);
      ctx.stroke();
      for (const point of [start, end]) {
        ctx.beginPath();
        ctx.arc(point.x, point.y, 3, 0, Math.PI * 2);
        ctx.fill();
      }
      ctx.restore();
    }

    if (overlay.drag) {
      drawMeasureLine(
        ctx,
        worldToScreen(overlay.drag.start, vp),
        worldToScreen(overlay.drag.end, vp),
        overlay.drag.lengthMm,
        overlay.drag.angleDeg,
        displayUnit,
      );
    }
  }

  private drawBaseSceneContents(
    params: RenderParams,
    visibleObjects: ProjectObject[],
    theme: CanvasTheme,
    displayUnit: 'mm' | 'inches',
  ): void {
    const { ctx } = this;
    const {
      workspace,
      objects,
      layers,
      vp,
      gridVisible,
      gridSpacingMm,
      toolOverlay,
      previewData,
      showPreview,
    } = params;

    ctx.fillStyle = theme.canvasBg;
    ctx.fillRect(0, 0, vp.canvasWidth, vp.canvasHeight);

    drawBed(ctx, workspace, vp, theme);
    if (gridVisible) {
      drawGrid(ctx, workspace, vp, gridSpacingMm, theme, displayUnit);
    }
    this.drawCameraOverlay(ctx, params.cameraOverlay, vp);
    drawOrigin(ctx, workspace, vp);

    const layerMap = new Map(layers.map((l) => [l.id, l]));

    for (const obj of visibleObjects) {
      const layer = layerMap.get(obj.layer_id);
      if (!layer) continue;
      const isToolLayer = layer.is_tool_layer === true;
      const layerDisabled = layer.enabled === false;

      if (obj.data.type === 'virtual_clone') {
        const cloneData = obj.data;
        let sourceObj = objects.find((o) => o.id === cloneData.source_id);
        let depth = 0;
        while (sourceObj && sourceObj.data.type === 'virtual_clone' && depth <= 10) {
          const sourceCloneData = sourceObj.data;
          sourceObj = objects.find((o) => o.id === sourceCloneData.source_id);
          depth++;
        }
        if (sourceObj && sourceObj.data.type !== 'virtual_clone') {
          const resolved = {
            ...sourceObj,
            id: obj.id,
            bounds: obj.bounds,
            transform: obj.transform,
            layer_id: obj.layer_id,
          };
          if (this.isObjectCulled(resolved, vp)) {
            continue;
          }
          ctx.save();
          if (layerDisabled) ctx.globalAlpha *= 0.3;
          ctx.globalAlpha *= 0.6;
          ctx.setLineDash([4, 4]);
          const usedProxy =
            resolved.data.type === 'vector_path' &&
            this.shouldUseVectorProxy(resolved, params.interactionState) &&
            this.drawVectorProxy(ctx, resolved, layer.color_tag, vp, params.filledRendering === true);
          if (!usedProxy) {
            this.drawObjectWithRasterMasks(
              ctx,
              resolved,
              layer,
              vp,
              objects,
              theme,
              params.filledRendering,
              isToolLayer,
            );
          }
          ctx.restore();
        }
        continue;
      }

      if (obj.data.type === 'barcode') {
        this.ensureBarcodePathData(obj);
      }

      if (
        toolOverlay.type === 'node-edit' &&
        toolOverlay.objectId === obj.id &&
        obj.data.type === 'vector_path'
      ) {
        if (isToolLayer || layerDisabled) {
          ctx.save();
          if (layerDisabled) ctx.globalAlpha *= 0.3;
          if (isToolLayer) {
            ctx.globalAlpha *= 0.6;
            ctx.setLineDash([8, 4]);
          }
        }
        drawEditableVectorPath(
          ctx,
          toolOverlay.paths,
          layer.color_tag,
          vp,
          toolOverlay.nodeToWorld,
          params.filledRendering,
        );
        if (isToolLayer || layerDisabled) {
          ctx.restore();
        }
      } else {
        if (this.isObjectCulled(obj, vp)) {
          continue;
        }
        if (layerDisabled) {
          ctx.save();
          ctx.globalAlpha *= 0.3;
        }
        const usedProxy =
          obj.data.type === 'vector_path' &&
          this.shouldUseVectorProxy(obj, params.interactionState) &&
          this.drawVectorProxy(ctx, obj, layer.color_tag, vp, params.filledRendering === true);
        if (!usedProxy) {
          this.drawObjectWithRasterMasks(
            ctx,
            obj,
            layer,
            vp,
            objects,
            theme,
            params.filledRendering,
            isToolLayer,
          );
        }
        if (layerDisabled) {
          ctx.restore();
        }
      }
    }

    if (
      showPreview &&
      previewData &&
      !(toolOverlay.type === 'node-edit' && toolOverlay.suspendPreview)
    ) {
      this.renderPreview(ctx, previewData, layers, previewViewportForWorkspace(vp, workspace));
    }

    if (params.flashedLayerId) {
      const flashedObjects = objects.filter((o) => o.layer_id === params.flashedLayerId);
      for (const obj of flashedObjects) {
        const vb = obj.bounds;
        const tlScreen = worldToScreen({ x: vb.min.x, y: vb.min.y }, vp);
        const brScreen = worldToScreen({ x: vb.max.x, y: vb.max.y }, vp);
        ctx.save();
        ctx.strokeStyle = '#facc15';
        ctx.lineWidth = 2;
        ctx.strokeRect(
          tlScreen.x,
          tlScreen.y,
          brScreen.x - tlScreen.x,
          brScreen.y - tlScreen.y,
        );
        ctx.restore();
      }
    }

    if (params.showLastPosition && params.laserPosition) {
      this.drawLaserPosition(ctx, params.laserPosition, vp);
    }

    drawRulers(ctx, workspace, vp, gridSpacingMm, theme, displayUnit);

    if (showPreview) {
      this.drawPreviewBadge(
        ctx,
        vp,
        previewData,
        params.previewState,
        params.previewManualRefreshRequired,
      );
    }
  }

  render(params: RenderParams): void {
    const { ctx } = this;
    const {
      workspace,
      objects,
      layers,
      selectedObjectIds,
      vp,
      gridVisible,
      gridSpacingMm,
      toolOverlay,
      previewData,
      showPreview,
    } = params;
    const theme = params.theme ?? DARK_THEME;
    const displayUnit = params.displayUnit ?? 'mm';
    const previewedLayerIds = new Set(
      showPreview && previewData
        ? previewData.layers
            .filter((layer) => layer.raster_regions.length > 0 || layer.vector_paths.length > 0)
            .map((layer) => layer.layer_id)
        : [],
    );

    // Apply antialiasing setting
    ctx.imageSmoothingEnabled = params.antialiasing !== false;

    // 1. Clear background
    ctx.fillStyle = theme.canvasBg;
    ctx.fillRect(0, 0, vp.canvasWidth, vp.canvasHeight);

    // 2. Bed boundary
    drawBed(ctx, workspace, vp, theme);

    // 3. Grid (on top of bed fill)
    if (gridVisible) {
      drawGrid(ctx, workspace, vp, gridSpacingMm, theme, displayUnit);
    }

    // 4. Camera overlay
    this.drawCameraOverlay(ctx, params.cameraOverlay, vp);

    // 5. Origin crosshair
    drawOrigin(ctx, workspace, vp);

    // 6. Objects (sorted by z_index, filtered by visibility + layer visible)
    const layerMap = new Map(layers.map((l) => [l.id, l]));
    const skipId = params.skipObjectId;
    const visibleObjects = objects
      .filter((obj) => {
        if (!obj.visible) return false;
        if (skipId && obj.id === skipId) return false;
        const layer = layerMap.get(obj.layer_id);
        if (layer && previewedLayerIds.has(layer.id)) return false;
        return layer?.visible !== false; // Show OFF hides completely
      })
      .sort((a, b) => a.z_index - b.z_index);

    for (const obj of visibleObjects) {
      const layer = layerMap.get(obj.layer_id);
      if (layer) {
        const isToolLayer = layer.is_tool_layer === true;
        const layerDisabled = layer.enabled === false;

        // Resolve VirtualClone: follow chain to find real source geometry
        if (obj.data.type === 'virtual_clone') {
          const cloneData = obj.data;
          let sourceObj = objects.find((o) => o.id === cloneData.source_id);
          // Follow clone chain — depth limit matches backend resolve_clone (depth > 10)
          let depth = 0;
          while (sourceObj && sourceObj.data.type === 'virtual_clone' && depth <= 10) {
            const cloneData = sourceObj.data;
            sourceObj = objects.find((o) => o.id === cloneData.source_id);
            depth++;
          }
          if (sourceObj && sourceObj.data.type !== 'virtual_clone') {
            const resolved = {
              ...sourceObj,
              id: obj.id,
              bounds: obj.bounds,
              transform: obj.transform,
              layer_id: obj.layer_id,
            };
            if (this.isObjectCulled(resolved, vp)) {
              continue;
            }
            ctx.save();
            if (layerDisabled) ctx.globalAlpha *= 0.3;
            ctx.globalAlpha *= 0.6;
            ctx.setLineDash([4, 4]);
            const usedProxy =
              resolved.data.type === 'vector_path' &&
              this.shouldUseVectorProxy(resolved, params.interactionState) &&
              this.drawVectorProxy(ctx, resolved, layer.color_tag, vp, params.filledRendering === true);
            if (!usedProxy) {
              this.drawObjectWithRasterMasks(
                ctx,
                resolved,
                layer,
                vp,
                objects,
                theme,
                params.filledRendering,
                isToolLayer,
              );
            }
            ctx.restore();
          }
          continue;
        }

        // During node editing, draw the actively edited object using in-flight
        // editable paths so the path line follows node/handle drags in real-time
        // Preload barcode path data if needed
        if (obj.data.type === 'barcode') {
          this.ensureBarcodePathData(obj);
        }

        if (
          toolOverlay.type === 'node-edit' &&
          toolOverlay.objectId === obj.id &&
          obj.data.type === 'vector_path'
        ) {
          if (isToolLayer || layerDisabled) {
            ctx.save();
            if (layerDisabled) ctx.globalAlpha *= 0.3;
            if (isToolLayer) {
              ctx.globalAlpha *= 0.6;
              ctx.setLineDash([8, 4]);
            }
          }
          drawEditableVectorPath(
            ctx,
            toolOverlay.paths,
            layer.color_tag,
            vp,
            toolOverlay.nodeToWorld,
            params.filledRendering,
          );
          if (isToolLayer || layerDisabled) {
            ctx.restore();
          }
        } else {
          if (this.isObjectCulled(obj, vp)) {
            continue;
          }
          if (layerDisabled) {
            ctx.save();
            ctx.globalAlpha *= 0.3;
          }
          const usedProxy =
            obj.data.type === 'vector_path' &&
            this.shouldUseVectorProxy(obj, params.interactionState) &&
            this.drawVectorProxy(ctx, obj, layer.color_tag, vp, params.filledRendering === true);
          if (!usedProxy) {
            this.drawObjectWithRasterMasks(
              ctx,
              obj,
              layer,
              vp,
              objects,
              theme,
              params.filledRendering,
              isToolLayer,
            );
          }
          if (layerDisabled) {
            ctx.restore();
          }
        }
      }
    }

    // 5b. Preview overlay (after objects, before selection)
    if (
      showPreview &&
      previewData &&
      !(toolOverlay.type === 'node-edit' && toolOverlay.suspendPreview)
    ) {
      this.renderPreview(ctx, previewData, layers, previewViewportForWorkspace(vp, workspace));
    }

    // 6. Selection highlights and handles (skip object being text-edited)
    const selectedObjects = objects.filter(
      (o) => selectedObjectIds.includes(o.id) && o.id !== skipId,
    );
    if (selectedObjects.length > 0) {
      const selectionLocks = toolOverlay.type === 'mesh-deform'
        ? { move_enabled: false, size_enabled: false, rotate_enabled: false, shear_enabled: false }
        : params.transformLocks;
      drawSelectionHighlight(
        ctx,
        selectedObjects,
        vp,
        theme,
        params.selectionDashOffset,
        selectionLocks,
        objects,
      );
    }

    // 6b. M4 flash overlay — transient gold outline on every object of the flashed layer.
    // Frontend-only; never mutates project state and never changes selection.
    if (params.flashedLayerId) {
      const flashedObjects = objects.filter((o) => o.layer_id === params.flashedLayerId);
      for (const obj of flashedObjects) {
        const vb = obj.bounds;
        const tlScreen = worldToScreen({ x: vb.min.x, y: vb.min.y }, vp);
        const brScreen = worldToScreen({ x: vb.max.x, y: vb.max.y }, vp);
        ctx.save();
        ctx.strokeStyle = '#facc15'; // amber-400 — visually distinct from selection
        ctx.lineWidth = 2;
        ctx.strokeRect(
          tlScreen.x,
          tlScreen.y,
          brScreen.x - tlScreen.x,
          brScreen.y - tlScreen.y,
        );
        ctx.restore();
      }
    }

    // 7. Tool overlay
    switch (toolOverlay.type) {
      case 'rubber-band':
        drawRubberBand(ctx, toolOverlay.startScreen, toolOverlay.endScreen, toolOverlay.crossing);
        break;
      case 'shape-preview': {
        // Convert with the CURRENT viewport so mid-draw zoom/pan keeps the
        // preview at true world size.
        const previewStart = worldToScreen(toolOverlay.startWorld, vp);
        const previewEnd = worldToScreen(toolOverlay.endWorld, vp);
        if (toolOverlay.kind === 'polygon' && toolOverlay.sides) {
          drawPolygonPreview(ctx, previewStart, previewEnd, toolOverlay.sides);
        } else if (toolOverlay.kind === 'star' && toolOverlay.sides) {
          drawStarPreview(
            ctx,
            previewStart,
            previewEnd,
            toolOverlay.sides,
            toolOverlay.bulge,
            toolOverlay.ratio,
            toolOverlay.dualRadius ?? false,
            toolOverlay.ratio2,
          );
        } else {
          drawShapePreview(
            ctx,
            previewStart,
            previewEnd,
            toolOverlay.kind as 'rectangle' | 'ellipse',
            toolOverlay.cornerRadius,
          );
        }
        break;
      }
      case 'node-edit':
        if (toolOverlay.hoveredSegment) {
          drawHoveredSegment(
            ctx,
            toolOverlay.paths,
            toolOverlay.hoveredSegment.nodeId,
            toolOverlay.hoveredSegment.t,
            vp,
            toolOverlay.nodeToWorld,
          );
        }
        if (toolOverlay.selectionRect) {
          drawRubberBand(
            ctx,
            toolOverlay.selectionRect.startScreen,
            toolOverlay.selectionRect.endScreen,
            toolOverlay.selectionRect.crossing,
          );
        }
        drawNodeHandles(
          ctx,
          toolOverlay.paths,
          toolOverlay.selectedTargets,
          vp,
          toolOverlay.nodeToWorld,
          toolOverlay.joinTargetNodeId,
        );
        break;
      case 'snap-guides':
        this.drawSnapGuides(ctx, toolOverlay.guides, vp);
        break;
      case 'ruler-guide-preview':
        this.drawSnapGuides(
          ctx,
          [{ axis: toolOverlay.axis === 'vertical' ? 'x' : 'y', value: toolOverlay.value }],
          vp,
        );
        break;
      case 'offset-preview':
        this.drawOffsetPreview(ctx, toolOverlay.paths, vp);
        break;
      case 'measure-line':
        drawMeasureLine(
          ctx,
          toolOverlay.startScreen,
          toolOverlay.endScreen,
          toolOverlay.distanceMm,
          toolOverlay.angleDeg,
          displayUnit,
        );
        break;
      case 'measure-inspection':
        this.drawMeasureInspection(ctx, toolOverlay, params, displayUnit, theme);
        break;
      case 'trim-preview':
        drawTrimPreview(ctx, toolOverlay.segmentScreenPoints);
        break;
      case 'pen-preview':
        drawPenPreview(
          ctx,
          toolOverlay.screenPoints,
          toolOverlay.currentScreen,
          toolOverlay.dragging,
          toolOverlay.closed,
        );
        break;
      case 'tab-markers':
        drawTabMarkers(
          ctx,
          toolOverlay.markers.map((m) => ({
            screen: worldToScreen({ x: m.worldX, y: m.worldY }, vp),
            hovered: m.hovered,
          })),
        );
        break;
      case 'radius-corners':
        drawRadiusCornerMarkers(
          ctx,
          toolOverlay.markers.map((m) => ({
            screen: worldToScreen({ x: m.worldX, y: m.worldY }, vp),
            hovered: m.hovered,
            alreadyFilleted: m.alreadyFilleted,
          })),
        );
        break;
      case 'start-point-pick':
        drawStartPointOverlay(
          ctx,
          toolOverlay.vertices.map((v) => ({
            screenX: worldToScreen({ x: v.worldX, y: v.worldY }, vp).x,
            screenY: worldToScreen({ x: v.worldX, y: v.worldY }, vp).y,
            isStart: v.isStart,
            subpathClosed: v.subpathClosed,
          })),
          toolOverlay.subpathArrows.map((a) => {
            const from = worldToScreen({ x: a.fromX, y: a.fromY }, vp);
            const to = worldToScreen({ x: a.toX, y: a.toY }, vp);
            return { fromX: from.x, fromY: from.y, toX: to.x, toY: to.y, isCustom: a.isCustom };
          }),
          toolOverlay.hoveredIndex,
        );
        break;
      case 'mesh-deform':
        drawMeshDeformOverlay(
          ctx,
          toolOverlay.handles.map((handle) => ({
            screen: worldToScreen({ x: handle.worldX, y: handle.worldY }, vp),
            active: handle.active,
            hovered: handle.hovered,
          })),
          toolOverlay.gridSize,
        );
        break;
      case 'camera-overlay-adjust':
        this.drawCameraOverlayAdjustHandles(
          ctx,
          toolOverlay.widthPx,
          toolOverlay.heightPx,
          toolOverlay.transform,
          vp,
        );
        break;
      case 'none':
        break;
    }

    // 7b. Persistent tab markers (always-visible on objects with tabs)
    if (params.persistentTabMarkers && params.persistentTabMarkers.length > 0) {
      drawTabMarkers(
        ctx,
        params.persistentTabMarkers.map((m) => ({
          screen: worldToScreen({ x: m.worldX, y: m.worldY }, vp),
          hovered: false,
        })),
      );
    }

    // 8. Laser last position marker
    if (params.showLastPosition && params.laserPosition) {
      this.drawLaserPosition(ctx, params.laserPosition, vp);
    }

    // 9. Rulers (on top of everything except badge)
    drawRulers(ctx, workspace, vp, gridSpacingMm, theme, displayUnit);

    // 10. Preview mode badge
    if (showPreview) {
      this.drawPreviewBadge(
        ctx,
        vp,
        previewData,
        params.previewState,
        params.previewManualRefreshRequired,
      );
    }
  }

  renderBaseScene(params: RenderParams): void {
    measureCanvasPerf('base-render', () => {
      const { ctx } = this;
      const {
        objects,
        layers,
        vp,
        previewData,
        showPreview,
      } = params;
      const theme = params.theme ?? DARK_THEME;
      const displayUnit = params.displayUnit ?? 'mm';
      const previewedLayerIds = new Set(
        showPreview && previewData
          ? previewData.layers
              .filter((layer) => layer.raster_regions.length > 0 || layer.vector_paths.length > 0)
              .map((layer) => layer.layer_id)
          : [],
      );

      ctx.imageSmoothingEnabled = params.antialiasing !== false;
      const layerMap = new Map(layers.map((l) => [l.id, l]));
      const skipId = params.skipObjectId;
      const visibleObjects = objects
        .filter((obj) => {
          if (!obj.visible) return false;
          if (skipId && obj.id === skipId) return false;
          const layer = layerMap.get(obj.layer_id);
          if (layer && previewedLayerIds.has(layer.id)) return false;
          return layer?.visible !== false;
        })
        .sort((a, b) => a.z_index - b.z_index);
      const currentSnapshots = this.buildBaseObjectSnapshots(visibleObjects, objects, vp);
      const currentSignature = this.getBaseSceneSignature(params, previewedLayerIds);
      const canUseDirtyRect =
        !this.shouldForceFullBaseRedraw() &&
        currentSignature !== null &&
        this.lastBaseSceneSignature === currentSignature;
      const dirtyRect = canUseDirtyRect
        ? this.computeDirtyBaseRect(currentSnapshots, this.lastBaseObjectSnapshots, vp)
        : null;

      if (canUseDirtyRect && dirtyRect) {
        ctx.save();
        ctx.beginPath();
        ctx.rect(
          dirtyRect.minX,
          dirtyRect.minY,
          dirtyRect.maxX - dirtyRect.minX,
          dirtyRect.maxY - dirtyRect.minY,
        );
        ctx.clip();
        ctx.clearRect(
          dirtyRect.minX,
          dirtyRect.minY,
          dirtyRect.maxX - dirtyRect.minX,
          dirtyRect.maxY - dirtyRect.minY,
        );
        this.drawBaseSceneContents(params, visibleObjects, theme, displayUnit);
        ctx.restore();
      } else {
        this.drawBaseSceneContents(params, visibleObjects, theme, displayUnit);
      }

      this.lastBaseSceneSignature = currentSignature;
      this.lastBaseObjectSnapshots = currentSnapshots;
    });
  }

  renderToolOverlay(params: RenderParams, clear = true): void {
    measureCanvasPerf('overlay-render', () => {
      const { ctx } = this;
      const { objects, selectedObjectIds, vp, toolOverlay } = params;
      const theme = params.theme ?? DARK_THEME;
      const displayUnit = params.displayUnit ?? 'mm';
      const skipId = params.skipObjectId;

      if (clear) {
        ctx.clearRect(0, 0, vp.canvasWidth, vp.canvasHeight);
      }

      const selectedObjects = objects.filter(
        (o) => selectedObjectIds.includes(o.id) && o.id !== skipId,
      );
      if (selectedObjects.length > 0) {
        const selectionLocks = toolOverlay.type === 'mesh-deform'
          ? { move_enabled: false, size_enabled: false, rotate_enabled: false, shear_enabled: false }
          : params.transformLocks;
        drawSelectionHighlight(
          ctx,
          selectedObjects,
          vp,
          theme,
          params.selectionDashOffset,
          selectionLocks,
          objects,
        );
      }

      switch (toolOverlay.type) {
      case 'rubber-band':
        drawRubberBand(ctx, toolOverlay.startScreen, toolOverlay.endScreen, toolOverlay.crossing);
        break;
      case 'shape-preview': {
        // Convert with the CURRENT viewport so mid-draw zoom/pan keeps the
        // preview at true world size.
        const previewStart = worldToScreen(toolOverlay.startWorld, vp);
        const previewEnd = worldToScreen(toolOverlay.endWorld, vp);
        if (toolOverlay.kind === 'polygon' && toolOverlay.sides) {
          drawPolygonPreview(ctx, previewStart, previewEnd, toolOverlay.sides);
        } else if (toolOverlay.kind === 'star' && toolOverlay.sides) {
          drawStarPreview(
            ctx,
            previewStart,
            previewEnd,
            toolOverlay.sides,
            toolOverlay.bulge,
            toolOverlay.ratio,
            toolOverlay.dualRadius ?? false,
            toolOverlay.ratio2,
          );
        } else {
          drawShapePreview(
            ctx,
            previewStart,
            previewEnd,
            toolOverlay.kind as 'rectangle' | 'ellipse',
            toolOverlay.cornerRadius,
          );
        }
        break;
      }
      case 'node-edit':
        if (toolOverlay.hoveredSegment) {
          drawHoveredSegment(
            ctx,
            toolOverlay.paths,
            toolOverlay.hoveredSegment.nodeId,
            toolOverlay.hoveredSegment.t,
            vp,
            toolOverlay.nodeToWorld,
          );
        }
        if (toolOverlay.selectionRect) {
          drawRubberBand(
            ctx,
            toolOverlay.selectionRect.startScreen,
            toolOverlay.selectionRect.endScreen,
            toolOverlay.selectionRect.crossing,
          );
        }
        drawNodeHandles(
          ctx,
          toolOverlay.paths,
          toolOverlay.selectedTargets,
          vp,
          toolOverlay.nodeToWorld,
          toolOverlay.joinTargetNodeId,
        );
        break;
      case 'snap-guides':
        this.drawSnapGuides(ctx, toolOverlay.guides, vp);
        break;
      case 'ruler-guide-preview':
        this.drawSnapGuides(
          ctx,
          [{ axis: toolOverlay.axis === 'vertical' ? 'x' : 'y', value: toolOverlay.value }],
          vp,
        );
        break;
      case 'offset-preview':
        this.drawOffsetPreview(ctx, toolOverlay.paths, vp);
        break;
      case 'measure-line':
        drawMeasureLine(
          ctx,
          toolOverlay.startScreen,
          toolOverlay.endScreen,
          toolOverlay.distanceMm,
          toolOverlay.angleDeg,
          displayUnit,
        );
        break;
      case 'measure-inspection':
        this.drawMeasureInspection(ctx, toolOverlay, params, displayUnit, theme);
        break;
      case 'trim-preview':
        drawTrimPreview(ctx, toolOverlay.segmentScreenPoints);
        break;
      case 'pen-preview':
        drawPenPreview(
          ctx,
          toolOverlay.screenPoints,
          toolOverlay.currentScreen,
          toolOverlay.dragging,
          toolOverlay.closed,
        );
        break;
      case 'tab-markers':
        drawTabMarkers(
          ctx,
          toolOverlay.markers.map((m) => ({
            screen: worldToScreen({ x: m.worldX, y: m.worldY }, vp),
            hovered: m.hovered,
          })),
        );
        break;
      case 'radius-corners':
        drawRadiusCornerMarkers(
          ctx,
          toolOverlay.markers.map((m) => ({
            screen: worldToScreen({ x: m.worldX, y: m.worldY }, vp),
            hovered: m.hovered,
            alreadyFilleted: m.alreadyFilleted,
          })),
        );
        break;
      case 'start-point-pick':
        drawStartPointOverlay(
          ctx,
          toolOverlay.vertices.map((v) => ({
            screenX: worldToScreen({ x: v.worldX, y: v.worldY }, vp).x,
            screenY: worldToScreen({ x: v.worldX, y: v.worldY }, vp).y,
            isStart: v.isStart,
            subpathClosed: v.subpathClosed,
          })),
          toolOverlay.subpathArrows.map((a) => {
            const from = worldToScreen({ x: a.fromX, y: a.fromY }, vp);
            const to = worldToScreen({ x: a.toX, y: a.toY }, vp);
            return { fromX: from.x, fromY: from.y, toX: to.x, toY: to.y, isCustom: a.isCustom };
          }),
          toolOverlay.hoveredIndex,
        );
        break;
      case 'mesh-deform':
        drawMeshDeformOverlay(
          ctx,
          toolOverlay.handles.map((handle) => ({
            screen: worldToScreen({ x: handle.worldX, y: handle.worldY }, vp),
            active: handle.active,
            hovered: handle.hovered,
          })),
          toolOverlay.gridSize,
        );
        break;
      case 'camera-overlay-adjust':
        this.drawCameraOverlayAdjustHandles(
          ctx,
          toolOverlay.widthPx,
          toolOverlay.heightPx,
          toolOverlay.transform,
          vp,
        );
        break;
      case 'none':
        break;
      }

      if (params.persistentTabMarkers && params.persistentTabMarkers.length > 0) {
        drawTabMarkers(
          ctx,
          params.persistentTabMarkers.map((m) => ({
            screen: worldToScreen({ x: m.worldX, y: m.worldY }, vp),
            hovered: false,
          })),
        );
      }
    });
  }

  private drawPreviewBadge(
    ctx: CanvasRenderingContext2D,
    vp: ViewportParams,
    previewData?: PreviewData | null,
    previewState?: PreviewState,
    previewManualRefreshRequired?: boolean,
  ): void {
    const hasData = previewData != null && previewData.layers.length > 0;
    const label = previewState === 'generating'
      ? 'PREVIEW (loading)'
      : previewState === 'error'
        ? 'PREVIEW (failed)'
        : previewManualRefreshRequired
          ? 'PREVIEW (refresh)'
          : previewState === 'stale'
            ? 'PREVIEW (stale)'
            : hasData
              ? 'PREVIEW'
              : 'PREVIEW (empty)';

    ctx.save();
    ctx.font = 'bold 11px system-ui, sans-serif';
    const metrics = ctx.measureText(label);
    const padH = 8;
    const padV = 4;
    const badgeW = metrics.width + padH * 2;
    const badgeH = 16 + padV * 2;
    const x = vp.canvasWidth - badgeW - 8;
    const y = 8;
    const r = 4;

    // Rounded rect background
    ctx.beginPath();
    ctx.moveTo(x + r, y);
    ctx.lineTo(x + badgeW - r, y);
    ctx.arcTo(x + badgeW, y, x + badgeW, y + r, r);
    ctx.lineTo(x + badgeW, y + badgeH - r);
    ctx.arcTo(x + badgeW, y + badgeH, x + badgeW - r, y + badgeH, r);
    ctx.lineTo(x + r, y + badgeH);
    ctx.arcTo(x, y + badgeH, x, y + badgeH - r, r);
    ctx.lineTo(x, y + r);
    ctx.arcTo(x, y, x + r, y, r);
    ctx.closePath();

    ctx.fillStyle = previewState === 'generating'
      ? 'rgba(14, 165, 233, 0.9)'
      : previewState === 'error'
        ? 'rgba(220, 38, 38, 0.9)'
        : previewManualRefreshRequired
          ? 'rgba(245, 158, 11, 0.9)'
          : previewState === 'stale'
            ? 'rgba(234, 179, 8, 0.88)'
            : hasData
              ? 'rgba(255, 107, 107, 0.85)'
              : 'rgba(160, 160, 160, 0.85)';
    ctx.fill();

    // Text
    ctx.fillStyle = '#ffffff';
    ctx.textBaseline = 'middle';
    ctx.fillText(label, x + padH, y + badgeH / 2);
    ctx.restore();
  }

  private drawLaserPosition(ctx: CanvasRenderingContext2D, pos: Point2D, vp: ViewportParams): void {
    const screen = worldToScreen(pos, vp);
    const r = 8;
    const crossLen = 14;

    ctx.save();

    // Dashed circle
    ctx.strokeStyle = '#ff3333';
    ctx.lineWidth = 1.5;
    ctx.setLineDash([3, 3]);
    ctx.beginPath();
    ctx.arc(screen.x, screen.y, r, 0, Math.PI * 2);
    ctx.stroke();

    // Solid crosshair
    ctx.setLineDash([]);
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.moveTo(screen.x - crossLen, screen.y);
    ctx.lineTo(screen.x - r - 2, screen.y);
    ctx.moveTo(screen.x + r + 2, screen.y);
    ctx.lineTo(screen.x + crossLen, screen.y);
    ctx.moveTo(screen.x, screen.y - crossLen);
    ctx.lineTo(screen.x, screen.y - r - 2);
    ctx.moveTo(screen.x, screen.y + r + 2);
    ctx.lineTo(screen.x, screen.y + crossLen);
    ctx.stroke();

    ctx.restore();
  }

  private drawSnapGuides(
    ctx: CanvasRenderingContext2D,
    guides: { axis: 'x' | 'y'; value: number }[],
    vp: ViewportParams,
  ): void {
    ctx.save();
    ctx.strokeStyle = '#ff6b6b';
    ctx.lineWidth = 1;
    ctx.setLineDash([4, 4]);

    for (const guide of guides) {
      if (guide.axis === 'x') {
        // Vertical guide line
        const screenPt = worldToScreen({ x: guide.value, y: 0 }, vp);
        ctx.beginPath();
        ctx.moveTo(screenPt.x, 0);
        ctx.lineTo(screenPt.x, vp.canvasHeight);
        ctx.stroke();
      } else {
        // Horizontal guide line
        const screenPt = worldToScreen({ x: 0, y: guide.value }, vp);
        ctx.beginPath();
        ctx.moveTo(0, screenPt.y);
        ctx.lineTo(vp.canvasWidth, screenPt.y);
        ctx.stroke();
      }
    }

    ctx.restore();
  }

  /**
   * Draw the Offset dialog's live preview as dashed ghost polylines (world
   * coords → screen at the current viewport). No direction arrows — this is a
   * result preview, not a path-order indicator.
   */
  private drawOffsetPreview(
    ctx: CanvasRenderingContext2D,
    paths: { points: Point2D[]; closed: boolean }[],
    vp: ViewportParams,
  ): void {
    ctx.save();
    ctx.strokeStyle = '#38bdf8';
    ctx.lineWidth = 1.5;
    ctx.globalAlpha = 0.9;
    ctx.setLineDash([5, 4]);

    for (const path of paths) {
      if (path.points.length < 2) continue;
      ctx.beginPath();
      const first = worldToScreen(path.points[0], vp);
      ctx.moveTo(first.x, first.y);
      for (let i = 1; i < path.points.length; i++) {
        const p = worldToScreen(path.points[i], vp);
        ctx.lineTo(p.x, p.y);
      }
      if (path.closed) ctx.closePath();
      ctx.stroke();
    }

    ctx.restore();
  }

  private renderPreview(
    ctx: CanvasRenderingContext2D,
    previewData: PreviewData,
    layers: Layer[],
    vp: ViewportParams,
  ): void {
    // Build color lookup from layer color_tag or fallback to defaults
    const layerColorMap = new Map<string, string>();
    layers.forEach((l, i) => {
      layerColorMap.set(l.id, l.color_tag || DEFAULT_LAYER_COLORS[i % DEFAULT_LAYER_COLORS.length]);
    });

    // Draw per-layer content
    for (const previewLayer of previewData.layers) {
      const color = layerColorMap.get(previewLayer.layer_id) ?? DEFAULT_LAYER_COLORS[0];
      const actualLayer = layers.find((layer) => layer.id === previewLayer.layer_id);
      const preferRunPreview = actualLayer?.entries.some(
        (entry) => entry.operation === 'fill' || entry.operation === 'offset_fill',
      ) ?? false;

      if (previewLayer.raster_regions.length > 0) {
        drawRasterPreview(
          ctx,
          previewLayer.raster_regions,
          color,
          vp,
          this.previewBitmapCache,
          preferRunPreview,
        );
      }
      if (previewLayer.vector_paths.length > 0) {
        drawVectorPreview(ctx, previewLayer.vector_paths, color, vp);
      }
    }

    // Draw travel moves
    if (previewData.travel_moves.length > 0) {
      drawTravelPreview(ctx, previewData.travel_moves, vp);
    }

    // Draw frame outline
    if (previewData.frame) {
      drawFramePreview(ctx, previewData.frame, vp);
    }
  }
}

/**
 * Bake a grayscale copy of the given source image onto an offscreen canvas
 * using direct ImageData luminance conversion. This avoids relying on
 * `ctx.filter = 'grayscale(...)'`, which some WebKit/WKWebView builds
 * implement inconsistently, and lets the main canvas render a stable
 * grayscale positioning proxy for color imports.
 */
function buildGrayscaleCanvas(img: HTMLImageElement): HTMLCanvasElement | null {
  const w = img.naturalWidth;
  const h = img.naturalHeight;
  if (w === 0 || h === 0) return null;

  const canvas = document.createElement('canvas');
  canvas.width = w;
  canvas.height = h;
  const gctx = canvas.getContext('2d');
  if (!gctx) return null;

  gctx.drawImage(img, 0, 0);

  try {
    const imageData = gctx.getImageData(0, 0, w, h);
    const data = imageData.data;
    for (let i = 0; i < data.length; i += 4) {
      // BT.601 luma weights — matches how most raster engraving pipelines
      // derive a single-channel intensity from RGB source pixels.
      const gray = Math.round(0.299 * data[i] + 0.587 * data[i + 1] + 0.114 * data[i + 2]);
      data[i] = gray;
      data[i + 1] = gray;
      data[i + 2] = gray;
    }
    gctx.putImageData(imageData, 0, 0);
  } catch {
    // If the canvas is tainted (e.g. a cross-origin image), fall back to
    // the untouched color draw rather than crashing. The source is always
    // a local blob URL in this app, so this should never fire in practice.
    return null;
  }

  return canvas;
}
