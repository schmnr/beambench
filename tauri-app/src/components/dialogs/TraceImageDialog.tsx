import { useState, useEffect, useRef, useCallback, useLayoutEffect } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { importService, type TraceBoundaryPx } from '../../services/importService';
import { useProjectStore } from '../../stores/projectStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { measureCanvasPerf } from '../../canvas/canvasPerf';
import { parsePathData, type PathCommand } from '../../canvas/drawObjects';
import { NumberStepper } from '../shared/NumberStepper';
import { Toggle } from '../shared/Toggle';
import { useFocusTrap } from '../../hooks/useFocusTrap';

interface TraceImageDialogProps {
  objectId: string;
  onClose: () => void;
}

interface ParsedTracePath {
  commands: PathCommand[];
  displayNodes: Array<{ x: number; y: number }>;
}

type ImagePoint = { x: number; y: number };

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
  const [sourceBlobUrl, setSourceBlobUrl] = useState<string | null>(null);
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
  const dialogRef = useRef<HTMLDivElement>(null);
  useFocusTrap(dialogRef, true);

  // Dialog window state
  const [dialogPos, setDialogPos] = useState<{ x: number; y: number } | null>(null);
  const [dialogSize, setDialogSize] = useState<{ w: number; h: number } | null>(null);
  const headerDragRef = useRef<{ startX: number; startY: number; origX: number; origY: number } | null>(null);
  const resizeDragRef = useRef<{ startX: number; startY: number; origW: number; origH: number } | null>(null);

  // Preview zoom/pan state
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const previewPanDragRef = useRef<{ startX: number; startY: number; origPanX: number; origPanY: number } | null>(null);
  const boundaryDragRef = useRef<{ start: ImagePoint; current: ImagePoint } | null>(null);
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
    useProjectStore.getState().loadAssetData(assetKey).then(setSourceBlobUrl);
  }, [assetKey]);

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
    };
    img.src = sourceBlobUrl;

    return () => {
      active = false;
      if (sourceImageRef.current === img) {
        sourceImageRef.current = null;
      }
    };
  }, [sourceBlobUrl]);

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

  // Escape key and Space+drag panning state
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
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
  }, [onClose]);

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

  const clampDialogSize = useCallback((width: number, height: number) => ({
    w: Math.max(520, Math.min(window.innerWidth - 24, width)),
    h: Math.max(620, Math.min(window.innerHeight - 24, height)),
  }), []);

  const clampDialogPos = useCallback((x: number, y: number, size: { w: number; h: number } | null) => {
    const width = size?.w ?? 560;
    const height = size?.h ?? 760;
    return {
      x: Math.max(12, Math.min(window.innerWidth - width - 12, x)),
      y: Math.max(12, Math.min(window.innerHeight - height - 12, y)),
    };
  }, []);

  // Debounced preview
  useEffect(() => {
    const myId = ++requestIdRef.current;
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
          setPreviewLoading(false);
        })
        .catch(() => {
          setPreviewLoading(false);
          if (myId !== requestIdRef.current) return;
          setPreviewData(null);
        });
    }, 500);
    return () => clearTimeout(timer);
  }, [objectId, threshold, cutoff, turdsize, alphamax, opttolerance, traceAlpha, sketchTrace, boundary, previewRefreshToken]);

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

      ctx.strokeStyle = '#e040fb';
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
    boundaryDragRef.current = { start, current: start };
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
        const nextBoundary = normalizeBoundaryPx(
          boundaryDragRef.current.start,
          boundaryDragRef.current.current,
          sourceWidth,
          sourceHeight,
        );
        committedBoundaryRef.current = nextBoundary;
        setBoundary(nextBoundary);
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

  const inputCls = "w-[70px] px-1.5 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text text-right focus:outline-none focus:border-bb-accent";
  const toggleBtnCls = (active: boolean) =>
    `px-2 py-1 rounded border text-xs ${active ? 'border-bb-accent text-bb-accent bg-bb-accent/10' : 'border-bb-border text-bb-text-muted hover:bg-bb-hover'}`;

  return createPortal(
    <div ref={dialogRef} role="dialog" aria-modal="true" aria-labelledby="trace-dialog-title"
         className="fixed inset-0 bg-black/50 z-50"
         style={{ display: 'flex', alignItems: dialogPos ? 'flex-start' : 'center', justifyContent: dialogPos ? 'flex-start' : 'center' }}
         onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
         onMouseMove={(e) => {
           if (headerDragRef.current) {
             const next = clampDialogPos(
               headerDragRef.current.origX + e.clientX - headerDragRef.current.startX,
               headerDragRef.current.origY + e.clientY - headerDragRef.current.startY,
               dialogSize,
             );
             setDialogPos(next);
           }
           if (resizeDragRef.current) {
             const nextSize = clampDialogSize(
               resizeDragRef.current.origW + e.clientX - resizeDragRef.current.startX,
               resizeDragRef.current.origH + e.clientY - resizeDragRef.current.startY,
             );
             setDialogSize(nextSize);
             setDialogPos((currentPos) => {
               if (!currentPos) return currentPos;
               return clampDialogPos(currentPos.x, currentPos.y, nextSize);
             });
           }
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
         }}
         onMouseUp={() => {
           if (boundaryDragRef.current) {
             const nextBoundary = normalizeBoundaryPx(
               boundaryDragRef.current.start,
               boundaryDragRef.current.current,
               sourceWidth,
               sourceHeight,
             );
             committedBoundaryRef.current = nextBoundary;
             setBoundary(nextBoundary);
             setDraftBoundary(null);
             boundaryDragRef.current = null;
           }
           headerDragRef.current = null;
           resizeDragRef.current = null;
           previewPanDragRef.current = null;
         }}>
      <div
        className="bg-bb-panel border border-bb-border rounded-lg shadow-xl flex flex-col relative"
        style={{
          ...(dialogPos ? { position: 'absolute', left: dialogPos.x, top: dialogPos.y } : {}),
          ...(dialogSize ? dialogSize : { width: 560, height: 760 }),
          maxWidth: 'calc(100vw - 24px)',
          maxHeight: 'calc(100vh - 24px)',
        }}
        onClick={(e) => e.stopPropagation()}
      >
        <div
          className="flex items-center justify-center h-6 cursor-grab active:cursor-grabbing select-none shrink-0 rounded-t-lg hover:bg-bb-bg/50"
          data-testid="trace-dialog-drag-handle"
          onMouseDown={(e) => {
            const rect = e.currentTarget.closest('[class*="bg-bb-panel"]')?.getBoundingClientRect();
            if (!rect) return;
            const size = dialogSize ?? { w: rect.width, h: rect.height };
            headerDragRef.current = {
              startX: e.clientX,
              startY: e.clientY,
              origX: dialogPos?.x ?? rect.left,
              origY: dialogPos?.y ?? rect.top,
            };
            if (!dialogPos) setDialogPos(clampDialogPos(rect.left, rect.top, size));
            if (!dialogSize) setDialogSize(size);
          }}
          title={t('dialog.trace_image.drag_to_move')}
        >
          <div className="w-10 h-1 rounded-full bg-bb-text-muted/30" />
        </div>

        <div className="flex flex-col gap-3 p-5 pt-0 flex-1 min-h-0 overflow-hidden">
          <h2 id="trace-dialog-title" className="text-sm font-semibold text-bb-text">{t('dialog.trace_image.title')}</h2>

          <div className="flex flex-col gap-2 flex-1 min-h-0">
            <div className="flex items-center gap-2 text-xs">
              {previewData && (
                <p className="text-bb-text-dim">
                  {t('dialog.trace_image.paths_found', { count: previewData.paths.length })}
                </p>
              )}
              <div className="flex-1" />
              <span className="text-bb-text-dim" data-testid="trace-zoom-label">{actualZoomPercent}%</span>
              <button type="button" onClick={() => adjustZoom(zoom / 1.2)} className={toggleBtnCls(false)} data-testid="trace-zoom-out">-</button>
              <button type="button" onClick={resetView} className={toggleBtnCls(Math.abs(zoom - 1) < 1e-6 && Math.abs(pan.x) < 1e-6 && Math.abs(pan.y) < 1e-6)} data-testid="trace-zoom-reset">
                {t('dialog.trace_image.fit')}
              </button>
              <button type="button" onClick={() => adjustZoom(oneToOneZoom)} className={toggleBtnCls(Math.abs(actualScale - 1) < 1e-3)} data-testid="trace-zoom-100">
                100%
              </button>
              <button type="button" onClick={() => adjustZoom(zoom * 1.2)} className={toggleBtnCls(false)} data-testid="trace-zoom-in">+</button>
            </div>

            {/* Preview area with checkerboard background */}
            <div
              ref={previewFrameRef}
              className={`relative border border-bb-border rounded overflow-hidden flex-1 min-h-[300px] ${previewPanDragRef.current ? 'cursor-grabbing' : spacePanning ? 'cursor-grab' : 'cursor-crosshair'}`}
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
                className="relative w-full h-full"
                data-testid="trace-preview-canvas"
              />
              {previewLoading && (
                <div className="absolute inset-0 flex items-center justify-center bg-black/30">
                  <span className="text-xs text-bb-text-muted animate-pulse">{t('dialog.trace_image.tracing')}</span>
                </div>
              )}
            </div>
            <p className="text-[11px] text-bb-text-dim">{t('dialog.trace_image.pan_hint')}</p>
          </div>

          {/* Controls — two-column grid */}
          <div className="grid grid-cols-[1fr_auto] gap-x-3 gap-y-1.5 items-center text-xs shrink-0">
            <span className="text-bb-text-muted">{t('dialog.trace_image.cutoff')}</span>
            <NumberStepper value={cutoff} onChange={(e) => setCutoff(Number(e.target.value))} min={0} max={255} step={1} className={inputCls} />

            <span className="text-bb-text-muted">{t('dialog.trace_image.threshold')}</span>
            <NumberStepper value={threshold} onChange={(e) => setThreshold(Number(e.target.value))} min={0} max={255} step={1} className={inputCls} data-testid="trace-threshold" />

            <span className="text-bb-text-muted">{t('dialog.trace_image.ignore_less_than')}</span>
            <NumberStepper value={turdsize} onChange={(e) => setTurdsize(Math.max(0, Number(e.target.value)))} min={0} max={100} step={1} className={inputCls} />

            <span className="text-bb-text-muted">{t('dialog.trace_image.smoothness')}</span>
            <NumberStepper value={alphamax} onChange={(e) => setAlphamax(Number(e.target.value))} min={0} max={1.334} step={0.001} className={inputCls} />

            <span className="text-bb-text-muted">{t('dialog.trace_image.optimize')}</span>
            <NumberStepper value={opttolerance} onChange={(e) => setOpttolerance(Number(e.target.value))} min={0} max={2} step={0.01} className={inputCls} />
          </div>

          <div className="flex items-center gap-4 text-xs shrink-0">
            <Toggle label={t('dialog.trace_image.trace_transparency')} checked={traceAlpha} onChange={setTraceAlpha} />
            <Toggle label={t('dialog.trace_image.sketch_trace')} checked={sketchTrace} onChange={setSketchTrace} />
          </div>

          <div className="flex items-center gap-2 text-xs flex-wrap shrink-0">
            <button onClick={() => setFadeImage(!fadeImage)} className={toggleBtnCls(fadeImage)}>
              {t('dialog.trace_image.fade_image')}
            </button>
            <button onClick={() => setShowPoints(!showPoints)} className={toggleBtnCls(showPoints)}>
              {t('dialog.trace_image.show_points')}
            </button>
            <button onClick={clearBoundary} className={toggleBtnCls(Boolean(boundary || draftBoundary))} data-testid="trace-clear-boundary">
              {t('dialog.trace_image.clear_boundary')}
            </button>
            <div className="flex-1" />
            <Toggle label={t('dialog.trace_image.delete_source')} checked={deleteSource} onChange={setDeleteSource} />
          </div>

          <div className="flex justify-end gap-2 pt-1 border-t border-bb-border shrink-0">
            <button onClick={onClose} className="px-3 py-1 text-xs font-medium rounded bg-bb-bg hover:bg-bb-hover text-bb-text">{t('common.cancel')}</button>
            <button data-testid="trace-submit" onClick={() => void handleSubmit()} className="px-4 py-1.5 text-xs font-medium rounded bg-bb-accent hover:bg-bb-accent-hover text-bb-on-accent">{t('common.ok')}</button>
          </div>
        </div>

        <div className="absolute bottom-0 right-0 w-5 h-5 cursor-nwse-resize group"
          data-testid="trace-dialog-resize-handle"
          onMouseDown={(e) => {
            e.stopPropagation();
            const rect = e.currentTarget.closest('[class*="bg-bb-panel"]')?.getBoundingClientRect();
            if (!rect) return;
            resizeDragRef.current = {
              startX: e.clientX,
              startY: e.clientY,
              origW: dialogSize?.w ?? rect.width,
              origH: dialogSize?.h ?? rect.height,
            };
            if (!dialogPos) setDialogPos(clampDialogPos(rect.left, rect.top, { w: rect.width, h: rect.height }));
            if (!dialogSize) setDialogSize({ w: rect.width, h: rect.height });
          }}
        >
          <svg viewBox="0 0 16 16" className="w-full h-full text-bb-text-muted/60 group-hover:text-bb-text-muted">
            <path d="M14 6L6 14M14 10L10 14M14 14L14 14" stroke="currentColor" strokeWidth="1.5" fill="none" strokeLinecap="round" />
          </svg>
        </div>
      </div>
    </div>,
    document.body
  );
}
