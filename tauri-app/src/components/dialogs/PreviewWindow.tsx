import { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import type { PreviewData } from '../../types/preview';
import type { Layer, Workspace } from '../../types/project';
import { pxPerMm } from '../../canvas/ViewportTransform';
import { buildTimeline, DEFAULT_RAPID_SPEED_MM_MIN } from '../../canvas/previewTimeline';
import type { AnimationTimeline } from '../../canvas/previewTimeline';
import { drawAnimatedPreview } from '../../canvas/drawPreviewAnimated';
import { zoomToFitBounds, worldBoundsToScreenRect } from '../../canvas/ViewportTransform';
import { previewViewportForWorkspace } from '../../canvas/previewViewport';
import { PreviewBitmapCache } from '../../canvas/previewBitmapCache';
import type { PreviewState } from '../../stores/previewStore';
import { useAppStore } from '../../stores/appStore';
import { MovableResizableDialogFrame } from '../shared/MovableResizableDialogFrame';

// Default layer colors (same as CanvasRenderer)
const DEFAULT_LAYER_COLORS = [
  '#ff6b6b', '#ffa94d', '#ffd43b', '#69db7c',
  '#4dabf7', '#9775fa', '#f783ac', '#20c997',
];

const SPEED_OPTIONS = [0.1, 0.25, 0.5, 1, 2, 4, 8, 16, 24, 40];

interface PreviewWindowProps {
  data: PreviewData | null;
  previewState: PreviewState;
  manualRefreshRequired?: boolean;
  previewGenerationDialogVisible?: boolean;
  layers: Layer[];
  workspace: Workspace | null;
  onRefresh?: () => void;
  onCancelGeneration?: () => void;
  onClose: () => void;
}

function formatTime(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  return `${m}:${s.toString().padStart(2, '0')}`;
}

function formatDistance(mm: number, unit: 'mm' | 'inches' = 'mm'): string {
  if (unit === 'inches') {
    const inches = mm / 25.4;
    return `${inches.toFixed(1)}in`;
  }
  if (mm >= 1000) return `${(mm / 1000).toFixed(1)}m`;
  return `${mm.toFixed(0)}mm`;
}

export function PreviewWindow({
  data,
  previewState,
  manualRefreshRequired = false,
  previewGenerationDialogVisible = false,
  layers,
  workspace,
  onRefresh,
  onCancelGeneration,
  onClose,
}: PreviewWindowProps) {
  const { t } = useTranslation();
  const appSettings = useAppStore((s) => s.settings);
  const displayUnit = (appSettings?.display_unit === 'inches' ? 'inches' : 'mm') as 'mm' | 'inches';
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const rafRef = useRef<number>(0);
  const lastFrameRef = useRef<number>(0);
  // PreviewWindow owns its own PreviewBitmapCache, independent from
  // the main canvas renderer's cache, so decoded bitmaps and
  // burned-mask offscreens are scoped to this dialog's lifecycle.
  const bitmapCacheRef = useRef<PreviewBitmapCache | null>(null);
  if (bitmapCacheRef.current === null) {
    bitmapCacheRef.current = new PreviewBitmapCache();
  }

  const [playing, setPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const lastInitedPlanIdRef = useRef<string | null>(null);

  // --- User-controlled viewport (zoom/pan). `null` means use the
  // default bed-fit viewport. Any wheel-zoom or drag-pan replaces
  // it with an explicit override.
  const [vpOverride, setVpOverride] = useState<{
    offset: { x: number; y: number };
    zoom: number;
  } | null>(null);
  const dragRef = useRef<{ startX: number; startY: number; origOffset: { x: number; y: number } } | null>(null);
  const [playbackSpeed, setPlaybackSpeed] = useState(1);
  const [showTravel, setShowTravel] = useState(true);
  const [showBurnProgress, setShowBurnProgress] = useState(true);
  const [showOverscan, setShowOverscan] = useState(true);
  const [shadeByPower, setShadeByPower] = useState(false);
  const [invertView, setInvertView] = useState(false);
  const [canvasSize, setCanvasSize] = useState({ width: 800, height: 400 });

  // Build layer color lookup
  const layerColors = useMemo(() => {
    return layers.map((l, i) => l.color_tag || DEFAULT_LAYER_COLORS[i % DEFAULT_LAYER_COLORS.length]);
  }, [layers]);

  const layerOperations = useMemo(() => {
    return Object.fromEntries(layers.map((layer) => {
      const hasFillEntry = layer.entries.some(
        (entry) => entry.operation === 'fill' || entry.operation === 'offset_fill',
      );
      return [layer.id, hasFillEntry ? 'fill' : (layer.entries[0]?.operation ?? 'line')];
    }));
  }, [layers]);

  // Build timeline from data.
  // When showTravel is off, travel segments get zero duration so playback
  // doesn't stall on invisible time and stepping skips them naturally.
  const timeline = useMemo<AnimationTimeline | null>(() => {
    if (!data) return null;
    return buildTimeline(data, layerColors, DEFAULT_RAPID_SPEED_MM_MIN, !showTravel, layerOperations);
  }, [data, layerColors, showTravel, layerOperations]);

  const playbackDuration = timeline?.playbackDuration ?? 0;

  // Clamp currentTime when playback duration shrinks (e.g., toggling travel off)
  useEffect(() => {
    if (currentTime > playbackDuration) {
      setCurrentTime(playbackDuration);
    }
  }, [playbackDuration]); // eslint-disable-line react-hooks/exhaustive-deps

  // Escape key handler
  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [onClose]);

  // Canvas resize observer
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const ro = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const { width, height } = entry.contentRect;
        if (width > 0 && height > 0) {
          setCanvasSize({ width: Math.floor(width), height: Math.floor(height) });
        }
      }
    });
    ro.observe(container);
    return () => ro.disconnect();
  }, []);

  // Set canvas pixel size
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = canvasSize.width * dpr;
    canvas.height = canvasSize.height * dpr;
    canvas.style.width = `${canvasSize.width}px`;
    canvas.style.height = `${canvasSize.height}px`;
    const ctx = canvas.getContext('2d');
    if (ctx) ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }, [canvasSize]);

  // Render function
  const renderFrame = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas || !timeline) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    // Clear
    ctx.clearRect(0, 0, canvasSize.width, canvasSize.height);

    // Fill background — light canvas by default, dark when inverted
    ctx.fillStyle = invertView ? '#1a1a1a' : '#f5f5f0';
    ctx.fillRect(0, 0, canvasSize.width, canvasSize.height);

    // Compute viewport: user override → bed bounds → job bounds fallback.
    const bedBounds = workspace
      ? { min: { x: 0, y: 0 }, max: { x: workspace.bed_width_mm, y: workspace.bed_height_mm } }
      : timeline.jobBounds;
    const defaultVp = zoomToFitBounds(bedBounds, canvasSize.width, canvasSize.height, 30);
    const displayVp = {
      offset: vpOverride?.offset ?? defaultVp.offset,
      zoom: vpOverride?.zoom ?? defaultVp.zoom,
      canvasWidth: canvasSize.width,
      canvasHeight: canvasSize.height,
    };
    const vp = previewViewportForWorkspace(displayVp, workspace);

    // Draw bed boundary so the user can see workspace extents.
    if (workspace) {
      const bedRect = worldBoundsToScreenRect({
        min: { x: 0, y: 0 },
        max: { x: workspace.bed_width_mm, y: workspace.bed_height_mm },
      }, vp);

      // Fill the bed area slightly brighter than the outer background
      // so the workspace is visually distinct.
      ctx.fillStyle = invertView ? '#222222' : '#ffffff';
      ctx.fillRect(bedRect.x, bedRect.y, bedRect.w, bedRect.h);

      // Thin border around the bed
      ctx.strokeStyle = invertView ? '#444444' : '#cccccc';
      ctx.lineWidth = 1;
      ctx.strokeRect(bedRect.x, bedRect.y, bedRect.w, bedRect.h);
    }

    drawAnimatedPreview(
      ctx,
      timeline,
      currentTime,
      vp,
      {
        showTravel,
        showBurnProgress,
        showOverscan,
        shadeByPower,
        invertView,
      },
      bitmapCacheRef.current!,
    );
  }, [timeline, currentTime, canvasSize, showTravel, showBurnProgress, showOverscan, shadeByPower, invertView, vpOverride, workspace]);

  // Re-render on state changes
  useEffect(() => {
    renderFrame();
  }, [renderFrame]);

  // Wire the bitmap cache's decode-complete callback to re-render.
  useEffect(() => {
    bitmapCacheRef.current?.setOnLoad(() => {
      renderFrame();
    });
    return () => {
      bitmapCacheRef.current?.setOnLoad(null);
    };
  }, [renderFrame]);

  // Clear plan-scoped render state whenever the incoming plan changes, so
  // stale blob URLs from a previous plan are released promptly.
  useEffect(() => {
    bitmapCacheRef.current?.clear();
    setPlaying(false);
    setVpOverride(null);
  }, [data?.plan_id]);

  // A new preview opens at the completed state.
  // Duration can change when options like "Show Travel" change, so the plan-id
  // ref is load-bearing: option toggles must not re-jump playback to the end.
  useEffect(() => {
    const planId = data?.plan_id ?? null;
    if (!planId) {
      lastInitedPlanIdRef.current = null;
      setCurrentTime(0);
      return;
    }
    if (planId !== lastInitedPlanIdRef.current) {
      lastInitedPlanIdRef.current = planId;
      setCurrentTime(playbackDuration);
    }
  }, [data?.plan_id, playbackDuration]);

  // Dispose the cache on unmount.
  useEffect(() => {
    return () => {
      bitmapCacheRef.current?.clear();
    };
  }, []);

  // Animation loop
  useEffect(() => {
    if (!playing || !timeline) return;

    lastFrameRef.current = performance.now();

    const animate = (now: number) => {
      const dt = (now - lastFrameRef.current) / 1000;
      lastFrameRef.current = now;

      setCurrentTime((prev) => {
        const next = prev + dt * playbackSpeed;
        if (next >= playbackDuration) {
          setPlaying(false);
          return playbackDuration;
        }
        return next;
      });

      rafRef.current = requestAnimationFrame(animate);
    };

    rafRef.current = requestAnimationFrame(animate);

    return () => {
      if (rafRef.current) cancelAnimationFrame(rafRef.current);
    };
  }, [playing, playbackSpeed, playbackDuration, timeline]);

  // --- Playback controls ---
  const goToStart = () => { setPlaying(false); setCurrentTime(0); };
  const goToEnd = () => { setPlaying(false); setCurrentTime(playbackDuration); };

  const stepBack = () => {
    if (!timeline) return;
    setPlaying(false);
    // Find previous segment boundary
    let prev = 0;
    for (const seg of timeline.segments) {
      if (seg.startTime >= currentTime - 0.001) break;
      prev = seg.startTime;
    }
    setCurrentTime(prev);
  };

  const stepForward = () => {
    if (!timeline) return;
    setPlaying(false);
    // Find next segment boundary
    for (const seg of timeline.segments) {
      if (seg.startTime > currentTime + 0.001) {
        setCurrentTime(seg.startTime);
        return;
      }
    }
    setCurrentTime(playbackDuration);
  };

  const togglePlay = () => {
    if (currentTime >= playbackDuration) {
      setCurrentTime(0);
    }
    setPlaying((p) => !p);
  };

  // Stats from backend (authoritative)
  const stats = data?.stats;

  return createPortal(
    <MovableResizableDialogFrame
      title={t('dialog.preview.title')}
      titleId="preview-window-title"
      testId="preview-window"
      initialWidth={800}
      initialHeight={600}
      minWidth={520}
      minHeight={380}
      zIndexClassName="z-[9750]"
      backdropClassName="bg-black/50"
      closeOnBackdropClick
      onRequestClose={onClose}
      headerActions={
        <>
          {previewState === 'stale' && (
            <span className="text-xs text-bb-warning-fg">
              {manualRefreshRequired ? t('dialog.preview.stale_refresh_required') : t('dialog.preview.stale')}
            </span>
          )}
          {previewState === 'stale' && onRefresh && (
            <button
              type="button"
              onClick={onRefresh}
              className="px-2 py-1 text-xs rounded border border-bb-border text-bb-text hover:bg-bb-hover"
            >
              {t('dialog.preview.refresh')}
            </button>
          )}
          <button
            onClick={onClose}
            className="text-bb-muted hover:text-bb-text text-lg leading-none px-1"
            aria-label={t('common.close')}
          >
            &times;
          </button>
        </>
      }
      footer={
        <div className="flex flex-wrap gap-4 px-4 py-1.5 text-xs text-bb-muted">
          <label className="flex cursor-pointer items-center gap-1">
            <input
              type="checkbox"
              checked={showTravel}
              onChange={(e) => setShowTravel(e.target.checked)}
              className="accent-bb-accent"
            />
            {t('dialog.preview.show_travel')}
          </label>
          <label className="flex cursor-pointer items-center gap-1">
            <input
              type="checkbox"
              checked={showBurnProgress}
              onChange={(e) => setShowBurnProgress(e.target.checked)}
              className="accent-bb-accent"
            />
            {t('dialog.preview.show_progress')}
          </label>
          <label className="flex cursor-pointer items-center gap-1">
            <input
              type="checkbox"
              checked={showOverscan}
              onChange={(e) => setShowOverscan(e.target.checked)}
              className="accent-bb-accent"
            />
            {t('dialog.preview.show_overscan')}
          </label>
          <label className="flex cursor-pointer items-center gap-1">
            <input
              type="checkbox"
              checked={shadeByPower}
              onChange={(e) => setShadeByPower(e.target.checked)}
              className="accent-bb-accent"
            />
            {t('dialog.preview.shade_by_power')}
          </label>
          <label className="flex cursor-pointer items-center gap-1">
            <input
              type="checkbox"
              checked={invertView}
              onChange={(e) => setInvertView(e.target.checked)}
              className="accent-bb-accent"
            />
            {t('dialog.preview.invert')}
          </label>
        </div>
      }
    >
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
        {/* Canvas area */}
        <div ref={containerRef} className="flex-1 min-h-0 relative">
          {previewState === 'generating' && !previewGenerationDialogVisible && (
            <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 bg-black/40 z-10">
	              <span className="text-bb-muted text-sm">{t('dialog.preview.generating')}</span>
              {onCancelGeneration && (
                <button
                  type="button"
                  onClick={onCancelGeneration}
                  className="px-3 py-1 text-xs rounded border border-bb-border text-bb-text bg-bb-panel hover:bg-bb-hover"
                >
	                  {t('common.cancel')}
                </button>
              )}
            </div>
          )}
          {previewState === 'error' && (
            <div className="absolute inset-0 flex items-center justify-center bg-black/40 z-10">
	              <span className="text-bb-error-fg text-sm">{t('dialog.preview.generation_failed')}</span>
            </div>
          )}
          {previewState === 'idle' && !data && (
            <div className="absolute inset-0 flex items-center justify-center bg-black/40 z-10">
	              <span className="text-bb-muted text-sm">{t('dialog.preview.no_data')}</span>
            </div>
          )}
          <canvas
            ref={canvasRef}
            className="w-full h-full"
            style={{ display: 'block', cursor: dragRef.current ? 'grabbing' : 'grab' }}
            onWheel={(e) => {
              e.preventDefault();
              // Compute the current viewport so we zoom around the mouse
              const bedBounds = workspace
                ? { min: { x: 0, y: 0 }, max: { x: workspace.bed_width_mm, y: workspace.bed_height_mm } }
                : timeline?.jobBounds ?? { min: { x: 0, y: 0 }, max: { x: 400, y: 400 } };
              const defaultVp = zoomToFitBounds(bedBounds, canvasSize.width, canvasSize.height, 30);
              const cur = vpOverride ?? defaultVp;
              const factor = e.deltaY < 0 ? 1.15 : 1 / 1.15;
              const newZoom = Math.max(5, Math.min(2000, cur.zoom * factor));

              // Zoom toward mouse position: adjust offset so the world
              // point under the cursor stays fixed on screen.
              const rect = canvasRef.current?.getBoundingClientRect();
              const mouseX = e.clientX - (rect?.left ?? 0);
              const mouseY = e.clientY - (rect?.top ?? 0);
              const scale = pxPerMm(cur.zoom);
              const newScale = pxPerMm(newZoom);
              // World point under mouse: (mouseX - canvasWidth/2) / scale + offset
              const worldX = (mouseX - canvasSize.width / 2) / scale + cur.offset.x;
              const worldY = (mouseY - canvasSize.height / 2) / scale + cur.offset.y;
              // New offset so the same world point maps to the same screen pos
              const newOffsetX = worldX - (mouseX - canvasSize.width / 2) / newScale;
              const newOffsetY = worldY - (mouseY - canvasSize.height / 2) / newScale;

              setVpOverride({ offset: { x: newOffsetX, y: newOffsetY }, zoom: newZoom });
            }}
            onMouseDown={(e) => {
              if (e.button !== 0) return;
              const bedBounds = workspace
                ? { min: { x: 0, y: 0 }, max: { x: workspace.bed_width_mm, y: workspace.bed_height_mm } }
                : timeline?.jobBounds ?? { min: { x: 0, y: 0 }, max: { x: 400, y: 400 } };
              const defaultVp = zoomToFitBounds(bedBounds, canvasSize.width, canvasSize.height, 30);
              const cur = vpOverride ?? defaultVp;
              dragRef.current = { startX: e.clientX, startY: e.clientY, origOffset: { ...cur.offset } };
            }}
            onMouseMove={(e) => {
              if (!dragRef.current) return;
              const bedBounds = workspace
                ? { min: { x: 0, y: 0 }, max: { x: workspace.bed_width_mm, y: workspace.bed_height_mm } }
                : timeline?.jobBounds ?? { min: { x: 0, y: 0 }, max: { x: 400, y: 400 } };
              const defaultVp = zoomToFitBounds(bedBounds, canvasSize.width, canvasSize.height, 30);
              const cur = vpOverride ?? defaultVp;
              const scale = pxPerMm(cur.zoom);
              const dx = (e.clientX - dragRef.current.startX) / scale;
              const dy = (e.clientY - dragRef.current.startY) / scale;
              setVpOverride({
                offset: {
                  x: dragRef.current.origOffset.x - dx,
                  y: dragRef.current.origOffset.y - dy,
                },
                zoom: cur.zoom,
              });
            }}
            onMouseUp={() => { dragRef.current = null; }}
            onMouseLeave={() => { dragRef.current = null; }}
            onDoubleClick={() => setVpOverride(null)}
          />
        </div>

        {/* Warnings row */}
        {data && data.warnings.length > 0 && (
          <div className="px-4 py-1.5 border-t border-bb-border text-xs text-bb-warning-fg flex flex-col gap-0.5">
            {data.warnings.map((w, i) => (
              <span key={i}>{w}</span>
            ))}
          </div>
        )}

        {data && data.failed_entries.length > 0 && (
          <div className="px-4 py-1.5 border-t border-bb-border text-xs text-bb-error-fg flex flex-col gap-0.5">
            {data.failed_entries.map((failure, i) => (
              <span key={i}>{failure}</span>
            ))}
          </div>
        )}

        {/* Stats row */}
        <div className="px-4 py-1.5 border-t border-bb-border text-xs text-bb-muted flex gap-4 flex-wrap">
	          <span>{t('dialog.preview.duration', { value: stats ? formatTime(stats.estimated_duration_secs) : '--:--' })}</span>
	          <span>{t('dialog.preview.segments', { value: stats?.segment_count ?? '-' })}</span>
	          <span>{t('dialog.preview.burn', { value: stats ? formatDistance(stats.burn_distance_mm, displayUnit) : '-' })}</span>
	          <span>{t('dialog.preview.travel', { value: stats ? formatDistance(stats.travel_distance_mm, displayUnit) : '-' })}</span>
	          {stats && stats.raster_line_count > 0 && (
	            <span>{t('dialog.preview.raster_lines', { value: stats.raster_line_count })}</span>
	          )}
        </div>

        {/* Playback controls */}
        <div className="px-4 py-2 border-t border-bb-border">
          <div className="flex items-center gap-2">
            {/* Transport buttons */}
            <div className="flex items-center gap-1">
	              <TransportBtn label="|&lt;" title={t('dialog.preview.go_to_start')} onClick={goToStart} disabled={!timeline} />
	              <TransportBtn label="&lt;&lt;" title={t('dialog.preview.previous_segment')} onClick={stepBack} disabled={!timeline} />
	              <TransportBtn
	                label={playing ? '||' : '\u25B6'}
	                title={playing ? t('dialog.preview.pause') : t('dialog.preview.play')}
                onClick={togglePlay}
                disabled={!timeline}
                primary
              />
	              <TransportBtn label="&gt;&gt;" title={t('dialog.preview.next_segment')} onClick={stepForward} disabled={!timeline} />
	              <TransportBtn label="|&gt;" title={t('dialog.preview.go_to_end')} onClick={goToEnd} disabled={!timeline} />
            </div>

            {/* Scrubber */}
            <input
              type="range"
              min={0}
              max={playbackDuration || 1}
              step={0.01}
              value={currentTime}
              onChange={(e) => {
                setCurrentTime(Number(e.target.value));
                if (playing) setPlaying(false);
              }}
              className="flex-1 h-1.5 accent-bb-accent cursor-pointer"
              disabled={!timeline}
            />

            {/* Speed selector */}
            <select
              value={playbackSpeed}
              onChange={(e) => setPlaybackSpeed(Number(e.target.value))}
              className="bg-bb-bg border border-bb-border rounded text-xs text-bb-text px-1 py-0.5"
            >
              {SPEED_OPTIONS.map((s) => (
	                <option key={s} value={s}>{t('dialog.preview.speed_multiplier', { value: s })}</option>
              ))}
            </select>
          </div>

          {/* Elapsed time */}
          <div className="text-center text-xs text-bb-muted mt-1">
	            {t('dialog.preview.playback', { current: formatTime(currentTime), total: formatTime(playbackDuration) })}
          </div>
        </div>

      </div>
    </MovableResizableDialogFrame>,
    document.body,
  );
}

function TransportBtn({ label, title, onClick, disabled, primary }: {
  label: string;
  title: string;
  onClick: () => void;
  disabled?: boolean;
  primary?: boolean;
}) {
  return (
    <button
      title={title}
      onClick={onClick}
      disabled={disabled}
      className={`px-2 py-1 text-xs font-mono rounded ${
        primary
          ? 'bg-bb-accent hover:bg-bb-accent-hover text-bb-on-accent'
          : 'bg-bb-bg hover:bg-bb-hover text-bb-text'
      } disabled:opacity-60 disabled:cursor-not-allowed`}
      dangerouslySetInnerHTML={{ __html: label }}
    />
  );
}
