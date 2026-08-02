import { useState, useEffect, useRef, useCallback, useLayoutEffect } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import {
  Check,
  Crop,
  Eye,
  EyeOff,
  LoaderCircle,
  Minus,
  Plus,
  RefreshCw,
  RotateCcw,
  ScanLine,
  X,
} from 'lucide-react';
import { importService, type TraceBoundaryPx } from '../../services/importService';
import { useProjectStore } from '../../stores/projectStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { measureCanvasPerf } from '../../canvas/canvasPerf';
import { parsePathData, type PathCommand } from '../../canvas/drawObjects';
import { RangeInput } from '../shared/RangeInput';
import { MovableResizableDialogFrame } from '../shared/MovableResizableDialogFrame';

interface TraceImageDialogProps {
  objectId: string;
  onClose: () => void;
}

interface ParsedTracePath {
  commands: PathCommand[];
  displayNodes: Array<{ x: number; y: number }>;
}

type ImagePoint = { x: number; y: number };

// The backend compares preview request IDs across dialog lifetimes. A counter
// scoped to the component would restart at 1 after reopening and every new
// preview would then be rejected as stale.
let tracePreviewRequestSequence = Date.now() * 1000;

function nextTracePreviewRequestId(): number {
  tracePreviewRequestSequence += 1;
  return tracePreviewRequestSequence;
}

function errorDetail(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function extractTraceNodes(commands: ReadonlyArray<PathCommand>): Array<{ x: number; y: number }> {
  const nodes: Array<{ x: number; y: number }> = [];
  for (const cmd of commands) {
    switch (cmd.type) {
      case 'M':
      case 'L':
      case 'Q':
      case 'C':
        nodes.push({ x: cmd.x, y: cmd.y });
        break;
      case 'Z':
        break;
    }
  }

  return nodes;
}

function distance(a: { x: number; y: number }, b: { x: number; y: number }): number {
  return Math.hypot(a.x - b.x, a.y - b.y);
}

function bendAngle(
  prev: { x: number; y: number },
  curr: { x: number; y: number },
  next: { x: number; y: number },
): number {
  const ax = curr.x - prev.x;
  const ay = curr.y - prev.y;
  const bx = next.x - curr.x;
  const by = next.y - curr.y;
  const aLen = Math.hypot(ax, ay);
  const bLen = Math.hypot(bx, by);
  if (aLen < 1e-6 || bLen < 1e-6) return Math.PI;
  const dot = (ax * bx + ay * by) / (aLen * bLen);
  return Math.acos(Math.max(-1, Math.min(1, dot)));
}

function filterDisplayNodes(commands: ReadonlyArray<PathCommand>): Array<{ x: number; y: number }> {
  const nodes = extractTraceNodes(commands);
  if (nodes.length <= 4) return nodes;

  const closed = commands[commands.length - 1]?.type === 'Z';
  const minSpacing = 6;
  const minBend = 0.22;
  const result: Array<{ x: number; y: number }> = [];

  for (let i = 0; i < nodes.length; i++) {
    const curr = nodes[i];
    if (!closed && (i === 0 || i === nodes.length - 1)) {
      result.push(curr);
      continue;
    }

    const prev = nodes[(i - 1 + nodes.length) % nodes.length];
    const next = nodes[(i + 1) % nodes.length];
    const lastKept = result[result.length - 1];
    const farEnough = !lastKept || distance(lastKept, curr) >= minSpacing;
    const significantTurn = bendAngle(prev, curr, next) >= minBend;

    if (farEnough || significantTurn) {
      result.push(curr);
    }
  }

  if (!closed) {
    const last = nodes[nodes.length - 1];
    if (distance(result[result.length - 1], last) > 1e-6) {
      result.push(last);
    }
  }

  return result;
}

function parseTracePaths(paths: string[]): ParsedTracePath[] {
  return paths.map((pathData) => {
    const commands = parsePathData(pathData);
    return {
      commands,
      displayNodes: filterDisplayNodes(commands),
    };
  });
}

function buildPath2DFromCommands(commands: ReadonlyArray<PathCommand>): Path2D {
  const path = new Path2D();
  for (const cmd of commands) {
    switch (cmd.type) {
      case 'M':
        path.moveTo(cmd.x, cmd.y);
        break;
      case 'L':
        path.lineTo(cmd.x, cmd.y);
        break;
      case 'Q':
        path.quadraticCurveTo(cmd.x1!, cmd.y1!, cmd.x, cmd.y);
        break;
      case 'C':
        path.bezierCurveTo(cmd.x1!, cmd.y1!, cmd.x2!, cmd.y2!, cmd.x, cmd.y);
        break;
      case 'Z':
        path.closePath();
        break;
    }
  }
  return path;
}

function normalizeBoundaryPx(
  start: ImagePoint,
  end: ImagePoint,
  sourceWidth: number,
  sourceHeight: number,
): TraceBoundaryPx | null {
  if (sourceWidth <= 0 || sourceHeight <= 0) return null;
  const x0 = Math.max(0, Math.min(sourceWidth, Math.floor(Math.min(start.x, end.x))));
  const y0 = Math.max(0, Math.min(sourceHeight, Math.floor(Math.min(start.y, end.y))));
  const x1 = Math.max(0, Math.min(sourceWidth, Math.ceil(Math.max(start.x, end.x))));
  const y1 = Math.max(0, Math.min(sourceHeight, Math.ceil(Math.max(start.y, end.y))));
  const width = x1 - x0;
  const height = y1 - y0;
  if (width <= 0 || height <= 0) return null;
  if (x0 === 0 && y0 === 0 && width === sourceWidth && height === sourceHeight) return null;
  return { x: x0, y: y0, width, height };
}

export function TraceImageDialog({ objectId, onClose }: TraceImageDialogProps) {
  const { t } = useTranslation();
  // Potrace parameters
  const [threshold, setThreshold] = useState(128);
  const [cutoff, setCutoff] = useState(0);
  const [turdsize, setTurdsize] = useState(2);
  const [alphamax, setAlphamax] = useState(1.0);
  const [opttolerance, setOpttolerance] = useState(0.2);
  const [traceAlpha, setTraceAlpha] = useState(false);
  const [sketchTrace, setSketchTrace] = useState(false);
  const [deleteSource, setDeleteSource] = useState(true);

  // UI toggles
  const [fadeImage, setFadeImage] = useState(true);
  const [showPoints, setShowPoints] = useState(false);

  // Preview state
  const [previewData, setPreviewData] = useState<{ paths: ParsedTracePath[]; sourceWidth: number; sourceHeight: number } | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [sourceError, setSourceError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [sourceBlobUrl, setSourceBlobUrl] = useState<string | null>(null);
  const [sourceRefreshToken, setSourceRefreshToken] = useState(0);
  const [previewViewport, setPreviewViewport] = useState({ width: 460, height: 320 });
  const [sourceImageLoaded, setSourceImageLoaded] = useState(false);
  const [boundary, setBoundary] = useState<TraceBoundaryPx | null>(null);
  const [draftBoundary, setDraftBoundary] = useState<TraceBoundaryPx | null>(null);
  const [previewRefreshToken, setPreviewRefreshToken] = useState(0);
  const committedBoundaryRef = useRef<TraceBoundaryPx | null>(null);
  const requestIdRef = useRef(0);
  const path2DCacheRef = useRef(new WeakMap<PathCommand[], Path2D>());
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const previewFrameRef = useRef<HTMLDivElement>(null);
  const sourceImageRef = useRef<HTMLImageElement | null>(null);

  // Preview zoom/pan state
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const previewPanDragRef = useRef<{ startX: number; startY: number; origPanX: number; origPanY: number } | null>(null);
  const boundaryDragRef = useRef<{
    start: ImagePoint;
    current: ImagePoint;
  } | null>(null);
  const spacePressedRef = useRef(false);
  const [spacePanning, setSpacePanning] = useState(false);

  // Load source image blob URL for preview background
  const object = useProjectStore((s) =>
    s.project?.objects.find((o) => o.id === objectId) ?? null
  );
  const assetKey = object?.data.type === 'raster_image' ? (object.data as { asset_key: string }).asset_key : null;

  useEffect(() => {
    if (!object || assetKey === null) {
      onClose();
    }
  }, [assetKey, object, onClose]);

  useEffect(() => {
    if (!assetKey) return;
    let active = true;
    setSourceError(null);
    useProjectStore.getState().loadAssetData(assetKey)
      .then((url) => {
        if (active) setSourceBlobUrl(url);
      })
      .catch((error) => {
        if (!active) return;
        setSourceBlobUrl(null);
        setSourceError(t('dialog.trace_image.trace_failed', { detail: errorDetail(error) }));
      });
    return () => {
      active = false;
    };
  }, [assetKey, sourceRefreshToken, t]);

  useEffect(() => {
    if (!sourceBlobUrl) {
      sourceImageRef.current = null;
      setSourceImageLoaded(false);
      return;
    }

    let active = true;
    const img = new Image();
    img.onload = () => {
      if (!active) return;
      sourceImageRef.current = img;
      setSourceImageLoaded(true);
    };
    img.onerror = () => {
      if (!active) return;
      sourceImageRef.current = null;
      setSourceImageLoaded(false);
      setSourceError(t('dialog.trace_image.trace_failed', { detail: t('dialog.preview.generation_failed') }));
    };
    img.src = sourceBlobUrl;

    return () => {
      active = false;
      if (sourceImageRef.current === img) {
        sourceImageRef.current = null;
      }
    };
  }, [sourceBlobUrl, t]);

  const sourceWidth = previewData?.sourceWidth
    ?? (object?.data.type === 'raster_image' ? (object.data as { original_width_px: number }).original_width_px : 0);
  const sourceHeight = previewData?.sourceHeight
    ?? (object?.data.type === 'raster_image' ? (object.data as { original_height_px: number }).original_height_px : 0);
  const fitScale = sourceWidth > 0 && sourceHeight > 0
    ? Math.min(previewViewport.width / sourceWidth, previewViewport.height / sourceHeight)
    : 1;
  const actualScale = fitScale * zoom;

  const clientToImagePoint = useCallback((clientX: number, clientY: number): ImagePoint | null => {
    const frame = previewFrameRef.current;
    if (!frame || sourceWidth <= 0 || sourceHeight <= 0 || actualScale <= 0) return null;
    const rect = frame.getBoundingClientRect();
    const centerX = previewViewport.width / 2 + pan.x;
    const centerY = previewViewport.height / 2 + pan.y;
    return {
      x: (clientX - rect.left - centerX) / actualScale + sourceWidth / 2,
      y: (clientY - rect.top - centerY) / actualScale + sourceHeight / 2,
    };
  }, [actualScale, pan.x, pan.y, previewViewport.height, previewViewport.width, sourceHeight, sourceWidth]);

  // Space+drag panning state. Escape is owned by the shared dialog frame so
  // it cannot bubble through and clear the canvas selection after closing.
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.code === 'Space' && !e.repeat) {
        const target = e.target as HTMLElement | null;
        // Number inputs don't accept spaces, so Space can still mean "pan" while one is focused
        const numberInput = target?.tagName === 'INPUT' && (target as HTMLInputElement).type === 'number';
        const editable = !numberInput && (target?.tagName === 'INPUT' || target?.tagName === 'TEXTAREA' || target?.tagName === 'SELECT' || target?.isContentEditable);
        if (!editable) {
          spacePressedRef.current = true;
          setSpacePanning(true);
          e.preventDefault();
        }
      }
    };
    const handleKeyUp = (e: KeyboardEvent) => {
      if (e.code === 'Space') {
        spacePressedRef.current = false;
        setSpacePanning(false);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('keyup', handleKeyUp);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('keyup', handleKeyUp);
    };
  }, []);

  useLayoutEffect(() => {
    const frame = previewFrameRef.current;
    if (!frame) return;

    const measure = () => {
      const nextWidth = Math.max(1, Math.round(frame.clientWidth));
      const nextHeight = Math.max(1, Math.round(frame.clientHeight));
      setPreviewViewport((prev) => (
        prev.width === nextWidth && prev.height === nextHeight
          ? prev
          : { width: nextWidth, height: nextHeight }
      ));
    };

    measure();

    if (typeof ResizeObserver === 'undefined') {
      window.addEventListener('resize', measure);
      return () => window.removeEventListener('resize', measure);
    }

    const observer = new ResizeObserver(measure);
    observer.observe(frame);
    return () => observer.disconnect();
  }, []);

  // Debounced preview
  useEffect(() => {
    const myId = nextTracePreviewRequestId();
    requestIdRef.current = myId;
    setPreviewError(null);
    // Supersede any expensive in-flight preview immediately instead of
    // allowing it to run for the full debounce interval.
    void importService.cancelTraceImagePreview(myId).catch(() => undefined);
    const timer = setTimeout(() => {
      setPreviewLoading(true);
      importService.traceImagePreview(objectId, threshold, cutoff, turdsize, alphamax, opttolerance, traceAlpha, sketchTrace, myId, boundary)
        .then((data) => {
          if (myId !== requestIdRef.current) return;
          setPreviewData({
            paths: parseTracePaths(data.paths),
            sourceWidth: data.source_width,
            sourceHeight: data.source_height,
          });
          setPreviewError(null);
          setPreviewLoading(false);
        })
        .catch((error) => {
          if (myId !== requestIdRef.current) return;
          setPreviewLoading(false);
          setPreviewData(null);
          setPreviewError(t('dialog.trace_image.trace_failed', { detail: errorDetail(error) }));
        });
    }, 500);
    return () => clearTimeout(timer);
  }, [objectId, threshold, cutoff, turdsize, alphamax, opttolerance, traceAlpha, sketchTrace, boundary, previewRefreshToken, t]);

  useEffect(() => () => {
    const cancelId = nextTracePreviewRequestId();
    requestIdRef.current = cancelId;
    void importService.cancelTraceImagePreview(cancelId).catch(() => undefined);
  }, []);

  // Render preview canvas
  const renderPreview = useCallback(() => {
    measureCanvasPerf('trace-preview-render', () => {
      const canvas = canvasRef.current;
      if (!canvas) return;
      const ctx = canvas.getContext('2d');
      if (!ctx) return;
      const devicePixelRatio = window.devicePixelRatio || 1;
      const canvasW = previewViewport.width;
      const canvasH = previewViewport.height;
      ctx.setTransform(devicePixelRatio, 0, 0, devicePixelRatio, 0, 0);
      ctx.clearRect(0, 0, canvasW, canvasH);

      if (sourceWidth === 0 || sourceHeight === 0) return;
      const paths = previewData?.paths ?? [];

      const fitScale = Math.min(canvasW / sourceWidth, canvasH / sourceHeight);
      const contentScale = fitScale * zoom;
      const centerX = canvasW / 2 + pan.x;
      const centerY = canvasH / 2 + pan.y;

      ctx.save();
      ctx.translate(centerX, centerY);
      ctx.scale(contentScale, contentScale);
      ctx.translate(-sourceWidth / 2, -sourceHeight / 2);

      if (sourceImageLoaded && sourceImageRef.current) {
        ctx.save();
        ctx.globalAlpha = fadeImage ? 0.4 : 1;
        ctx.drawImage(sourceImageRef.current, 0, 0, sourceWidth, sourceHeight);
        ctx.restore();
      }

      const accentChannels = getComputedStyle(canvas).getPropertyValue('--bb-accent').trim();
      ctx.strokeStyle = accentChannels ? `rgb(${accentChannels})` : '#22d3ee';
      ctx.lineWidth = 1.5 / contentScale;
      for (const path of paths) {
        let cached = path2DCacheRef.current.get(path.commands);
        if (!cached) {
          cached = buildPath2DFromCommands(path.commands);
          path2DCacheRef.current.set(path.commands, cached);
        }
        ctx.stroke(cached);
      }

      if (showPoints && paths.length > 0) {
        // Dark outline keeps markers visible on light images, white fill on dark ones
        const size = 5 / contentScale;
        ctx.fillStyle = '#ffffff';
        ctx.strokeStyle = 'rgba(0, 0, 0, 0.75)';
        ctx.lineWidth = 1 / contentScale;
        for (const path of paths) {
          for (const { x, y } of path.displayNodes) {
            ctx.fillRect(x - size / 2, y - size / 2, size, size);
            ctx.strokeRect(x - size / 2, y - size / 2, size, size);
          }
        }
      }

      const activeBoundary = draftBoundary ?? boundary;
      if (activeBoundary) {
        ctx.fillStyle = 'rgba(0, 0, 0, 0.45)';
        ctx.fillRect(0, 0, sourceWidth, activeBoundary.y);
        ctx.fillRect(
          0,
          activeBoundary.y + activeBoundary.height,
          sourceWidth,
          sourceHeight - activeBoundary.y - activeBoundary.height,
        );
        ctx.fillRect(0, activeBoundary.y, activeBoundary.x, activeBoundary.height);
        ctx.fillRect(
          activeBoundary.x + activeBoundary.width,
          activeBoundary.y,
          sourceWidth - activeBoundary.x - activeBoundary.width,
          activeBoundary.height,
        );
        ctx.strokeStyle = '#00e5ff';
        ctx.lineWidth = 1.5 / contentScale;
        ctx.strokeRect(activeBoundary.x, activeBoundary.y, activeBoundary.width, activeBoundary.height);
      }

      ctx.restore();
    });
  }, [boundary, draftBoundary, fadeImage, pan.x, pan.y, previewData, previewViewport.height, previewViewport.width, showPoints, sourceHeight, sourceImageLoaded, sourceWidth, zoom]);

  useEffect(() => { renderPreview(); }, [renderPreview]);

  const devicePixelRatio = typeof window === 'undefined' ? 1 : (window.devicePixelRatio || 1);
  const canvasWidth = Math.max(1, Math.round(previewViewport.width * devicePixelRatio));
  const canvasHeight = Math.max(1, Math.round(previewViewport.height * devicePixelRatio));
  const actualZoomPercent = Math.round(actualScale * 100);
  const oneToOneZoom = fitScale > 0 ? 1 / fitScale : 1;

  const handleSubmit = async () => {
    if (submitting) return;
    setSubmitting(true);
    try {
      const committedBoundary = boundaryDragRef.current
        ? normalizeBoundaryPx(
            boundaryDragRef.current.start,
            boundaryDragRef.current.current,
            sourceWidth,
            sourceHeight,
          )
        : (draftBoundary ?? committedBoundaryRef.current);
      committedBoundaryRef.current = committedBoundary;
      setBoundary(committedBoundary);
      setDraftBoundary(null);
      boundaryDragRef.current = null;
      const traced = await importService.traceImage(
        objectId,
        threshold,
        cutoff,
        turdsize,
        alphamax,
        opttolerance,
        traceAlpha,
        sketchTrace,
        deleteSource,
        committedBoundary,
      );
      const tracedIds = traced.map((entry) => entry.id);
      const store = useProjectStore.getState();
      await store.loadProject({ invalidatePreview: true });
      if (tracedIds.length > 0) {
        store.selectObjects(tracedIds);
      }
      onClose();
    } catch (error) {
      useNotificationStore.getState().push(t('dialog.trace_image.trace_failed', { detail: String(error) }), 'error');
      setSubmitting(false);
    }
  };

  const adjustZoom = useCallback((nextZoom: number, anchor?: { x: number; y: number }) => {
    const clampedZoom = Math.max(0.25, Math.min(16, nextZoom));
    setZoom((currentZoom) => {
      if (!anchor || !previewFrameRef.current) return clampedZoom;
      const rect = previewFrameRef.current.getBoundingClientRect();
      const relativeX = anchor.x - rect.left - rect.width / 2;
      const relativeY = anchor.y - rect.top - rect.height / 2;
      const ratio = clampedZoom / currentZoom;
      setPan((currentPan) => ({
        x: relativeX - (relativeX - currentPan.x) * ratio,
        y: relativeY - (relativeY - currentPan.y) * ratio,
      }));
      return clampedZoom;
    });
  }, []);

  const resetView = useCallback(() => {
    setZoom(1);
    setPan({ x: 0, y: 0 });
  }, []);

  const clearBoundary = useCallback(() => {
    requestIdRef.current += 1;
    setPreviewLoading(false);
    setPreviewData(null);
    setDraftBoundary(null);
    boundaryDragRef.current = null;
    committedBoundaryRef.current = null;
    setBoundary(null);
    setPreviewRefreshToken((token) => token + 1);
  }, []);

  const handlePreviewWheel = useCallback((e: React.WheelEvent<HTMLDivElement>) => {
    e.preventDefault();
    const factor = e.deltaY < 0 ? 1.15 : 1 / 1.15;
    adjustZoom(zoom * factor, { x: e.clientX, y: e.clientY });
  }, [adjustZoom, zoom]);

  const handlePreviewMouseDown = useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    // preventDefault below keeps focus where it was, so release a focused field
    // explicitly or Space stays captured as typing after the user clicks the preview
    const active = document.activeElement as HTMLElement | null;
    if (active && active !== document.body && (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA' || active.tagName === 'SELECT')) {
      active.blur();
    }
    if (e.button === 1 || (e.button === 0 && spacePressedRef.current)) {
      e.preventDefault();
      e.stopPropagation();
      previewPanDragRef.current = {
        startX: e.clientX,
        startY: e.clientY,
        origPanX: pan.x,
        origPanY: pan.y,
      };
      return;
    }
    if (e.button !== 0) return;
    const start = clientToImagePoint(e.clientX, e.clientY);
    if (!start) return;
    e.preventDefault();
    e.stopPropagation();
    boundaryDragRef.current = {
      start,
      current: start,
    };
    setDraftBoundary(null);
  }, [clientToImagePoint, pan.x, pan.y]);

  useEffect(() => {
    const handleWindowMouseMove = (e: MouseEvent) => {
      if (previewPanDragRef.current) {
        setPan({
          x: previewPanDragRef.current.origPanX + e.clientX - previewPanDragRef.current.startX,
          y: previewPanDragRef.current.origPanY + e.clientY - previewPanDragRef.current.startY,
        });
      }
      if (boundaryDragRef.current) {
        const current = clientToImagePoint(e.clientX, e.clientY);
        if (current) {
          boundaryDragRef.current.current = current;
          setDraftBoundary(normalizeBoundaryPx(boundaryDragRef.current.start, current, sourceWidth, sourceHeight));
        }
      }
    };

    const handleWindowMouseUp = () => {
      if (boundaryDragRef.current) {
        // A click should not erase an existing crop. Commit only after the
        // pointer has selected at least one source-image pixel.
        if (distance(boundaryDragRef.current.start, boundaryDragRef.current.current) >= 1) {
          const nextBoundary = normalizeBoundaryPx(
            boundaryDragRef.current.start,
            boundaryDragRef.current.current,
            sourceWidth,
            sourceHeight,
          );
          committedBoundaryRef.current = nextBoundary;
          setBoundary(nextBoundary);
        }
        setDraftBoundary(null);
        boundaryDragRef.current = null;
      }
      previewPanDragRef.current = null;
    };

    window.addEventListener('mousemove', handleWindowMouseMove);
    window.addEventListener('mouseup', handleWindowMouseUp);
    return () => {
      window.removeEventListener('mousemove', handleWindowMouseMove);
      window.removeEventListener('mouseup', handleWindowMouseUp);
    };
  }, [clientToImagePoint, sourceHeight, sourceWidth]);

  const closeDialog = () => {
    if (!submitting) onClose();
  };

  const resetTraceSettings = () => {
    setThreshold(128);
    setCutoff(0);
    setTurdsize(2);
    setAlphamax(1);
    setOpttolerance(0.2);
    setTraceAlpha(false);
    setSketchTrace(false);
  };

  const retryPreview = () => {
    setSourceBlobUrl(null);
    setSourceRefreshToken((token) => token + 1);
    setPreviewRefreshToken((token) => token + 1);
  };

  const hasBoundary = Boolean(boundary || draftBoundary);
  const visibleError = previewError ?? sourceError;

  return createPortal(
    <MovableResizableDialogFrame
      title={t('dialog.trace_image.title')}
      titleId="trace-dialog-title"
      testId="trace-dialog"
      initialWidth={940}
      initialHeight={680}
      minWidth={720}
      minHeight={500}
      zIndexClassName="z-[9750]"
      backdropClassName="bg-black/50"
      closeOnBackdropClick={!submitting}
      onRequestClose={closeDialog}
      headerActions={(
        <button
          type="button"
          onClick={closeDialog}
          disabled={submitting}
          className="flex h-8 w-8 items-center justify-center rounded-lg border border-transparent text-bb-muted transition-colors hover:border-bb-border hover:bg-bb-hover hover:text-bb-text focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent disabled:cursor-not-allowed disabled:opacity-50"
          aria-label={t('common.close')}
        >
          <X size={16} />
        </button>
      )}
      footer={(
        <div className="flex items-center justify-between gap-3 bg-bb-surface px-4 py-3">
          <TraceOption
            checked={deleteSource}
            onChange={setDeleteSource}
            label={t('dialog.trace_image.delete_source')}
            disabled={submitting}
          />
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={closeDialog}
              disabled={submitting}
              className="h-9 rounded-lg border border-bb-border bg-bb-bg px-3 text-xs font-medium text-bb-text transition-colors hover:border-bb-accent/40 hover:bg-bb-hover focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent disabled:cursor-not-allowed disabled:opacity-50"
            >
              {t('common.cancel')}
            </button>
            <button
              type="button"
              data-testid="trace-submit"
              onClick={() => void handleSubmit()}
              disabled={submitting}
              className="flex h-9 min-w-28 items-center justify-center gap-2 rounded-lg bg-bb-accent px-4 text-xs font-semibold text-bb-on-accent transition-colors hover:bg-bb-accent-hover focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent disabled:cursor-not-allowed disabled:opacity-60"
            >
              {submitting && <LoaderCircle size={14} className="animate-spin motion-reduce:animate-none" />}
              {submitting ? t('dialog.trace_image.tracing') : t('common.ok')}
            </button>
          </div>
        </div>
      )}
    >
      <div className="flex min-h-0 flex-1 overflow-hidden">
        <section className="flex min-w-0 flex-1 flex-col p-3 pr-2">
          <div className="mb-2 flex min-h-9 items-center gap-2">
            <div className="flex min-w-0 items-center gap-2 text-xs text-bb-text-dim" aria-live="polite">
              <ScanLine size={14} className="shrink-0 text-bb-accent" />
              {previewLoading ? (
                <span>{t('dialog.trace_image.tracing')}</span>
              ) : previewData ? (
                <span className="truncate">{t('dialog.trace_image.paths_found', { count: previewData.paths.length })}</span>
              ) : (
                <span>{t('menus.view.preview')}</span>
              )}
              {hasBoundary && <Crop size={13} className="shrink-0 text-bb-accent" />}
            </div>
            <div className="flex-1" />
            <span className="w-12 text-right text-xs tabular-nums text-bb-text-dim" data-testid="trace-zoom-label">
              {actualZoomPercent}%
            </span>
            <TraceIconButton
              icon={<Minus size={14} />}
              label={t('menus.view.zoom_out')}
              onClick={() => adjustZoom(zoom / 1.2)}
              testId="trace-zoom-out"
            />
            <button
              type="button"
              onClick={resetView}
              aria-pressed={Math.abs(zoom - 1) < 1e-6 && Math.abs(pan.x) < 1e-6 && Math.abs(pan.y) < 1e-6}
              className="h-8 rounded-lg border border-bb-border bg-bb-bg px-2.5 text-xs text-bb-text transition-colors hover:border-bb-accent/40 hover:bg-bb-hover focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent aria-pressed:border-bb-accent/50 aria-pressed:bg-bb-accent/10"
              data-testid="trace-zoom-reset"
            >
              {t('dialog.trace_image.fit')}
            </button>
            <button
              type="button"
              onClick={() => adjustZoom(oneToOneZoom)}
              aria-pressed={Math.abs(actualScale - 1) < 1e-3}
              className="h-8 rounded-lg border border-bb-border bg-bb-bg px-2.5 text-xs tabular-nums text-bb-text transition-colors hover:border-bb-accent/40 hover:bg-bb-hover focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent aria-pressed:border-bb-accent/50 aria-pressed:bg-bb-accent/10"
              data-testid="trace-zoom-100"
            >
              100%
            </button>
            <TraceIconButton
              icon={<Plus size={14} />}
              label={t('menus.view.zoom_in')}
              onClick={() => adjustZoom(zoom * 1.2)}
              testId="trace-zoom-in"
            />
          </div>

          <div
            ref={previewFrameRef}
            className={`relative min-h-0 flex-1 overflow-hidden rounded-xl border border-bb-accent/35 shadow-inner ${previewPanDragRef.current ? 'cursor-grabbing' : spacePanning ? 'cursor-grab' : 'cursor-crosshair'}`}
            style={{
              backgroundImage: 'linear-gradient(45deg, #808080 25%, transparent 25%), linear-gradient(-45deg, #808080 25%, transparent 25%), linear-gradient(45deg, transparent 75%, #808080 75%), linear-gradient(-45deg, transparent 75%, #808080 75%)',
              backgroundSize: '16px 16px',
              backgroundPosition: '0 0, 0 8px, 8px -8px, -8px 0px',
              backgroundColor: '#a0a0a0',
            }}
            onWheel={handlePreviewWheel}
            onMouseDown={handlePreviewMouseDown}
            data-testid="trace-preview-frame"
          >
            <canvas
              ref={canvasRef}
              width={canvasWidth}
              height={canvasHeight}
              className="relative h-full w-full"
              data-testid="trace-preview-canvas"
            />
            {previewLoading && (
              <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 bg-black/30 backdrop-blur-[1px]">
                <LoaderCircle size={20} className="animate-spin text-bb-accent motion-reduce:animate-none" />
                <span className="text-xs text-bb-text">{t('dialog.trace_image.tracing')}</span>
              </div>
            )}
            {visibleError && !previewLoading && (
              <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 bg-black/50 px-8 text-center" role="alert">
                <span className="max-w-md text-xs text-bb-error-fg">{visibleError}</span>
                <button
                  type="button"
                  onClick={retryPreview}
                  className="flex h-8 items-center gap-1.5 rounded-lg border border-bb-border bg-bb-panel px-2.5 text-xs text-bb-text transition-colors hover:border-bb-accent/40 hover:bg-bb-hover focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent"
                >
                  <RefreshCw size={13} />
                  {t('dialog.preview.refresh')}
                </button>
              </div>
            )}
          </div>

          <div className="mt-2 flex flex-wrap items-center gap-1.5 text-xs">
            <TraceOption
              checked={fadeImage}
              onChange={setFadeImage}
              label={t('dialog.trace_image.fade_image')}
              icon={fadeImage ? <EyeOff size={12} /> : <Eye size={12} />}
            />
            <TraceOption
              checked={showPoints}
              onChange={setShowPoints}
              label={t('dialog.trace_image.show_points')}
            />
            <button
              type="button"
              onClick={clearBoundary}
              disabled={!hasBoundary}
              className="flex h-8 items-center gap-1.5 rounded-lg border border-bb-border bg-bb-bg px-2.5 text-bb-text-muted transition-colors hover:border-bb-accent/30 hover:bg-bb-hover hover:text-bb-text focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent disabled:cursor-not-allowed disabled:opacity-45"
              data-testid="trace-clear-boundary"
            >
              <Crop size={12} />
              {t('dialog.trace_image.clear_boundary')}
            </button>
            <span className="ml-auto text-[11px] text-bb-text-dim">{t('dialog.trace_image.pan_hint')}</span>
          </div>
        </section>

        <aside className="w-[292px] shrink-0 overflow-y-auto border-l border-bb-border bg-bb-surface">
          <div className="flex h-9 items-center justify-between border-b border-bb-border bg-gradient-to-r from-bb-accent/10 to-bb-surface/30 px-3">
            <div className="flex items-center gap-2">
              <ScanLine size={14} className="text-bb-accent" />
              <span className="text-xs font-medium text-bb-text">{t('menus.edit.settings')}</span>
            </div>
            <button
              type="button"
              onClick={resetTraceSettings}
              disabled={submitting}
              className="flex h-7 w-7 items-center justify-center rounded-lg text-bb-text-muted transition-colors hover:bg-bb-hover hover:text-bb-text focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent disabled:cursor-not-allowed disabled:opacity-50"
              aria-label={t('panels.variable_text.reset')}
              title={t('panels.variable_text.reset')}
            >
              <RotateCcw size={13} />
            </button>
          </div>
          <div className="flex flex-col gap-4 p-3">
            <RangeInput
              label={t('dialog.trace_image.threshold')}
              value={threshold}
              onChange={(value) => setThreshold(Math.max(cutoff, value))}
              min={cutoff}
              max={255}
              step={1}
              disabled={submitting}
              testId="trace-threshold"
            />
            <RangeInput
              label={t('dialog.trace_image.cutoff')}
              value={cutoff}
              onChange={(value) => setCutoff(Math.min(threshold, value))}
              min={0}
              max={threshold}
              step={1}
              disabled={submitting}
            />
            <RangeInput
              label={t('dialog.trace_image.ignore_less_than')}
              value={turdsize}
              onChange={(value) => setTurdsize(Math.max(0, Math.round(value)))}
              min={0}
              max={100}
              step={1}
              disabled={submitting}
            />
            <RangeInput
              label={t('dialog.trace_image.smoothness')}
              value={alphamax}
              onChange={setAlphamax}
              min={0}
              max={1.334}
              step={0.01}
              disabled={submitting}
            />
            <RangeInput
              label={t('dialog.trace_image.optimize')}
              value={opttolerance}
              onChange={setOpttolerance}
              min={0}
              max={2}
              step={0.01}
              disabled={submitting}
            />

            <div className="border-t border-bb-border pt-3">
              <div className="flex flex-col gap-1.5">
                <TraceOption
                  checked={traceAlpha}
                  onChange={setTraceAlpha}
                  label={t('dialog.trace_image.trace_transparency')}
                  fullWidth
                  disabled={submitting}
                />
                <TraceOption
                  checked={sketchTrace}
                  onChange={setSketchTrace}
                  label={t('dialog.trace_image.sketch_trace')}
                  fullWidth
                  disabled={submitting}
                />
              </div>
            </div>
          </div>
        </aside>
      </div>
    </MovableResizableDialogFrame>,
    document.body,
  );
}

function TraceIconButton({
  icon,
  label,
  onClick,
  testId,
}: {
  icon: React.ReactNode;
  label: string;
  onClick: () => void;
  testId?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={label}
      aria-label={label}
      data-testid={testId}
      className="flex h-8 w-8 items-center justify-center rounded-lg border border-bb-border bg-bb-bg text-bb-text-muted transition-colors hover:border-bb-accent/40 hover:bg-bb-hover hover:text-bb-text focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent"
    >
      {icon}
    </button>
  );
}

function TraceOption({
  checked,
  onChange,
  label,
  icon,
  disabled = false,
  fullWidth = false,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  icon?: React.ReactNode;
  disabled?: boolean;
  fullWidth?: boolean;
}) {
  return (
    <label
      className={`flex h-8 items-center gap-1.5 rounded-lg border px-2.5 text-xs transition-colors ${
        fullWidth ? 'w-full justify-between' : ''
      } ${
        disabled
          ? 'cursor-not-allowed border-bb-border bg-bb-bg text-bb-text-disabled opacity-60'
          : checked
            ? 'cursor-pointer border-bb-accent/45 bg-bb-accent/10 text-bb-text'
            : 'cursor-pointer border-bb-border bg-bb-bg text-bb-text-muted hover:border-bb-accent/30 hover:bg-bb-hover'
      }`}
    >
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
        disabled={disabled}
        className="sr-only"
      />
      <span className="flex min-w-0 items-center gap-1.5">
        {icon}
        <span className="truncate">{label}</span>
      </span>
      <span
        className={`flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded border ${
          checked
            ? 'border-bb-accent bg-bb-accent text-bb-on-accent'
            : 'border-bb-control-border bg-bb-input'
        }`}
      >
        {checked ? <Check size={10} strokeWidth={3} /> : null}
      </span>
    </label>
  );
}
