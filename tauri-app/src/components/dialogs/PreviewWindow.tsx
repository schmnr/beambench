import { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import {
  Check,
  Clock3,
  Flame,
  ListTree,
  Pause,
  Play,
  RefreshCw,
  Route,
  ScanLine,
  SkipBack,
  SkipForward,
  StepBack,
  StepForward,
  X,
} from 'lucide-react';
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
  '#ff6b6b',
  '#ffa94d',
  '#ffd43b',
  '#69db7c',
  '#4dabf7',
  '#9775fa',
  '#f783ac',
  '#20c997',
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
  const totalSeconds = Math.max(0, Math.round(secs));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) {
    return `${hours}:${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
  }
  return `${minutes}:${seconds.toString().padStart(2, '0')}`;
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
  const playingRef = useRef(false);
  const currentTimeRef = useRef(0);
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

  const setPlaybackTime = useCallback((time: number) => {
    currentTimeRef.current = time;
    setCurrentTime(time);
  }, []);

  const pausePlayback = useCallback(() => {
    playingRef.current = false;
    setPlaying(false);
    if (rafRef.current) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = 0;
    }
  }, []);

  // --- User-controlled viewport (zoom/pan). `null` means use the
  // default bed-fit viewport. Any wheel-zoom or drag-pan replaces
  // it with an explicit override.
  const [vpOverride, setVpOverride] = useState<{
    offset: { x: number; y: number };
    zoom: number;
  } | null>(null);
  const dragRef = useRef<{
    startX: number;
    startY: number;
    origOffset: { x: number; y: number };
  } | null>(null);
  const [playbackSpeed, setPlaybackSpeed] = useState(1);
  const [showTravel, setShowTravel] = useState(true);
  const [showBurnProgress, setShowBurnProgress] = useState(true);
  const [showOverscan, setShowOverscan] = useState(true);
  const [shadeByPower, setShadeByPower] = useState(false);
  const [invertView, setInvertView] = useState(false);
  const [canvasSize, setCanvasSize] = useState({ width: 800, height: 400 });

  // Build layer color lookup
  const layerColors = useMemo(() => {
    return layers.map(
      (l, i) => l.color_tag || DEFAULT_LAYER_COLORS[i % DEFAULT_LAYER_COLORS.length],
    );
  }, [layers]);

  const layerOperations = useMemo(() => {
    return Object.fromEntries(
      layers.map((layer) => {
        const hasFillEntry = layer.entries.some(
          (entry) => entry.operation === 'fill' || entry.operation === 'offset_fill',
        );
        return [layer.id, hasFillEntry ? 'fill' : (layer.entries[0]?.operation ?? 'line')];
      }),
    );
  }, [layers]);

  // Build timeline from data.
  // When showTravel is off, travel segments get zero duration so playback
  // doesn't stall on invisible time and stepping skips them naturally.
  const timeline = useMemo<AnimationTimeline | null>(() => {
    if (!data) return null;
    return buildTimeline(
      data,
      layerColors,
      DEFAULT_RAPID_SPEED_MM_MIN,
      !showTravel,
      layerOperations,
    );
  }, [data, layerColors, showTravel, layerOperations]);

  const playbackDuration = timeline?.playbackDuration ?? 0;

  // Clamp currentTime when playback duration shrinks (e.g., toggling travel off)
  useEffect(() => {
    if (currentTimeRef.current > playbackDuration) {
      pausePlayback();
      setPlaybackTime(playbackDuration);
    }
  }, [pausePlayback, playbackDuration, setPlaybackTime]);

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
      const bedRect = worldBoundsToScreenRect(
        {
          min: { x: 0, y: 0 },
          max: { x: workspace.bed_width_mm, y: workspace.bed_height_mm },
        },
        vp,
      );

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
  }, [
    timeline,
    currentTime,
    canvasSize,
    showTravel,
    showBurnProgress,
    showOverscan,
    shadeByPower,
    invertView,
    vpOverride,
    workspace,
  ]);

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
    pausePlayback();
    setVpOverride(null);
  }, [data?.plan_id, pausePlayback]);

  // A new preview opens at the completed state.
  // Duration can change when options like "Show Travel" change, so the plan-id
  // ref is load-bearing: option toggles must not re-jump playback to the end.
  useEffect(() => {
    const planId = data?.plan_id ?? null;
    if (!planId) {
      lastInitedPlanIdRef.current = null;
      setPlaybackTime(0);
      return;
    }
    if (planId !== lastInitedPlanIdRef.current) {
      lastInitedPlanIdRef.current = planId;
      setPlaybackTime(playbackDuration);
    }
  }, [data?.plan_id, playbackDuration, setPlaybackTime]);

  // Dispose the cache on unmount.
  useEffect(() => {
    return () => {
      playingRef.current = false;
      if (rafRef.current) cancelAnimationFrame(rafRef.current);
      bitmapCacheRef.current?.clear();
    };
  }, []);

  // Animation loop
  useEffect(() => {
    if (!playing || !timeline) return;

    playingRef.current = true;
    lastFrameRef.current = performance.now();

    const animate = (now: number) => {
      if (!playingRef.current) return;

      const dt = (now - lastFrameRef.current) / 1000;
      lastFrameRef.current = now;
      const next = Math.min(playbackDuration, currentTimeRef.current + dt * playbackSpeed);
      setPlaybackTime(next);

      if (next >= playbackDuration) {
        playingRef.current = false;
        setPlaying(false);
        rafRef.current = 0;
        return;
      }

      if (playingRef.current) {
        rafRef.current = requestAnimationFrame(animate);
      }
    };

    rafRef.current = requestAnimationFrame(animate);

    return () => {
      if (rafRef.current) cancelAnimationFrame(rafRef.current);
      rafRef.current = 0;
    };
  }, [playing, playbackSpeed, playbackDuration, setPlaybackTime, timeline]);

  // --- Playback controls ---
  const goToStart = useCallback(() => {
    pausePlayback();
    setPlaybackTime(0);
  }, [pausePlayback, setPlaybackTime]);

  const goToEnd = useCallback(() => {
    pausePlayback();
    setPlaybackTime(playbackDuration);
  }, [pausePlayback, playbackDuration, setPlaybackTime]);

  const stepBack = useCallback(() => {
    if (!timeline) return;
    pausePlayback();
    // Find previous segment boundary
    let prev = 0;
    for (const seg of timeline.segments) {
      if (seg.startTime >= currentTimeRef.current - 0.001) break;
      prev = seg.startTime;
    }
    setPlaybackTime(prev);
  }, [pausePlayback, setPlaybackTime, timeline]);

  const stepForward = useCallback(() => {
    if (!timeline) return;
    pausePlayback();
    // Find next segment boundary
    for (const seg of timeline.segments) {
      if (seg.startTime > currentTimeRef.current + 0.001) {
        setPlaybackTime(seg.startTime);
        return;
      }
    }
    setPlaybackTime(playbackDuration);
  }, [pausePlayback, playbackDuration, setPlaybackTime, timeline]);

  const togglePlay = useCallback(() => {
    if (playingRef.current) {
      pausePlayback();
      return;
    }
    if (!timeline || playbackDuration <= 0) return;
    if (currentTimeRef.current >= playbackDuration) {
      setPlaybackTime(0);
    }
    playingRef.current = true;
    setPlaying(true);
  }, [pausePlayback, playbackDuration, setPlaybackTime, timeline]);

  // Space is the conventional transport shortcut. Keep form controls and
  // buttons isolated so adjusting speed or a toggle never starts playback.
  useEffect(() => {
    const handleKey = (event: KeyboardEvent) => {
      if (event.code !== 'Space') return;
      const target = event.target;
      if (
        target instanceof HTMLElement &&
        (target.isContentEditable ||
          ['INPUT', 'SELECT', 'TEXTAREA', 'BUTTON'].includes(target.tagName))
      ) {
        return;
      }
      event.preventDefault();
      togglePlay();
    };
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [togglePlay]);

  // Stats from backend (authoritative)
  const stats = data?.stats;

  return createPortal(
    <MovableResizableDialogFrame
      title={t('dialog.preview.title')}
      titleId="preview-window-title"
      testId="preview-window"
      initialWidth={900}
      initialHeight={680}
      minWidth={640}
      minHeight={460}
      zIndexClassName="z-[9750]"
      backdropClassName="bg-black/50"
      closeOnBackdropClick
      onRequestClose={onClose}
      headerActions={
        <>
          {previewState === 'stale' && (
            <span className="text-xs text-bb-warning-fg">
              {manualRefreshRequired
                ? t('dialog.preview.stale_refresh_required')
                : t('dialog.preview.stale')}
            </span>
          )}
          {previewState === 'stale' && onRefresh && (
            <button
              type="button"
              onClick={onRefresh}
              className="flex h-8 items-center gap-1.5 rounded-lg border border-bb-border bg-bb-bg px-2.5 text-xs text-bb-text transition-colors hover:border-bb-accent/40 hover:bg-bb-hover"
            >
              <RefreshCw size={13} />
              {t('dialog.preview.refresh')}
            </button>
          )}
          <button
            type="button"
            onClick={onClose}
            className="flex h-8 w-8 items-center justify-center rounded-lg border border-transparent text-bb-muted transition-colors hover:border-bb-border hover:bg-bb-hover hover:text-bb-text"
            aria-label={t('common.close')}
          >
            <X size={16} />
          </button>
        </>
      }
      footer={
        <div className="flex flex-wrap gap-1.5 bg-bb-surface px-3 py-2 text-xs text-bb-muted">
          <PreviewOption
            checked={showTravel}
            onChange={setShowTravel}
            label={t('dialog.preview.show_travel')}
          />
          <PreviewOption
            checked={showBurnProgress}
            onChange={setShowBurnProgress}
            label={t('dialog.preview.show_progress')}
          />
          <PreviewOption
            checked={showOverscan}
            onChange={setShowOverscan}
            label={t('dialog.preview.show_overscan')}
          />
          <PreviewOption
            checked={shadeByPower}
            onChange={setShadeByPower}
            label={t('dialog.preview.shade_by_power')}
          />
          <PreviewOption
            checked={invertView}
            onChange={setInvertView}
            label={t('dialog.preview.invert')}
          />
        </div>
      }
    >
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
        {/* Canvas area */}
        <div
          ref={containerRef}
          className="relative m-3 mb-2 min-h-0 flex-1 overflow-hidden rounded-xl border border-bb-accent/35 bg-bb-bg shadow-inner"
        >
          {previewState === 'generating' && !previewGenerationDialogVisible && (
            <div className="absolute inset-0 z-10 flex flex-col items-center justify-center gap-3 bg-black/40 backdrop-blur-[1px]">
              <span className="text-sm text-bb-text-muted">{t('dialog.preview.generating')}</span>
              {onCancelGeneration && (
                <button
                  type="button"
                  onClick={onCancelGeneration}
                  className="rounded-lg border border-bb-border bg-bb-panel px-3 py-1.5 text-xs text-bb-text hover:border-bb-accent/40 hover:bg-bb-hover"
                >
                  {t('common.cancel')}
                </button>
              )}
            </div>
          )}
          {previewState === 'error' && (
            <div className="absolute inset-0 flex items-center justify-center bg-black/40 z-10">
              <span className="text-bb-error-fg text-sm">
                {t('dialog.preview.generation_failed')}
              </span>
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
                ? {
                    min: { x: 0, y: 0 },
                    max: { x: workspace.bed_width_mm, y: workspace.bed_height_mm },
                  }
                : (timeline?.jobBounds ?? { min: { x: 0, y: 0 }, max: { x: 400, y: 400 } });
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
                ? {
                    min: { x: 0, y: 0 },
                    max: { x: workspace.bed_width_mm, y: workspace.bed_height_mm },
                  }
                : (timeline?.jobBounds ?? { min: { x: 0, y: 0 }, max: { x: 400, y: 400 } });
              const defaultVp = zoomToFitBounds(bedBounds, canvasSize.width, canvasSize.height, 30);
              const cur = vpOverride ?? defaultVp;
              dragRef.current = {
                startX: e.clientX,
                startY: e.clientY,
                origOffset: { ...cur.offset },
              };
            }}
            onMouseMove={(e) => {
              if (!dragRef.current) return;
              const bedBounds = workspace
                ? {
                    min: { x: 0, y: 0 },
                    max: { x: workspace.bed_width_mm, y: workspace.bed_height_mm },
                  }
                : (timeline?.jobBounds ?? { min: { x: 0, y: 0 }, max: { x: 400, y: 400 } });
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
            onMouseUp={() => {
              dragRef.current = null;
            }}
            onMouseLeave={() => {
              dragRef.current = null;
            }}
            onDoubleClick={() => setVpOverride(null)}
          />
        </div>

        {/* Warnings row */}
        {data && data.warnings.length > 0 && (
          <div className="mx-3 mb-2 flex flex-col gap-0.5 rounded-lg border border-bb-warning-border bg-bb-warning-bg px-3 py-2 text-xs text-bb-warning-fg">
            {data.warnings.map((w, i) => (
              <span key={i}>{w}</span>
            ))}
          </div>
        )}

        {data && data.failed_entries.length > 0 && (
          <div className="mx-3 mb-2 flex flex-col gap-0.5 rounded-lg border border-bb-error-border bg-bb-error-bg px-3 py-2 text-xs text-bb-error-fg">
            {data.failed_entries.map((failure, i) => (
              <span key={i}>{failure}</span>
            ))}
          </div>
        )}

        {/* Stats row */}
        <div className="mx-3 mb-2 flex flex-wrap gap-1.5 text-xs text-bb-text-muted">
          <PreviewStat
            icon={<Clock3 size={13} />}
            label={t('dialog.preview.duration', {
              value: stats ? formatTime(stats.estimated_duration_secs) : '--:--',
            })}
          />
          <PreviewStat
            icon={<ListTree size={13} />}
            label={t('dialog.preview.segments', { value: stats?.segment_count ?? '-' })}
          />
          <PreviewStat
            icon={<Flame size={13} />}
            label={t('dialog.preview.burn', {
              value: stats ? formatDistance(stats.burn_distance_mm, displayUnit) : '-',
            })}
          />
          <PreviewStat
            icon={<Route size={13} />}
            label={t('dialog.preview.travel', {
              value: stats ? formatDistance(stats.travel_distance_mm, displayUnit) : '-',
            })}
          />
          {stats && stats.raster_line_count > 0 && (
            <PreviewStat
              icon={<ScanLine size={13} />}
              label={t('dialog.preview.raster_lines', { value: stats.raster_line_count })}
            />
          )}
        </div>

        {/* Playback controls */}
        <div className="mx-3 mb-3 overflow-hidden rounded-xl border border-bb-accent/40 bg-bb-surface shadow-sm">
          <div className="flex h-9 items-center gap-2 border-b border-bb-border bg-gradient-to-r from-bb-accent/10 to-bb-surface/30 px-3">
            <Clock3 size={14} className="text-bb-accent" />
            <span className="text-xs font-medium tabular-nums text-bb-text">
              {t('dialog.preview.playback', {
                current: formatTime(currentTime),
                total: formatTime(playbackDuration),
              })}
            </span>
          </div>
          <div className="flex items-center gap-3 p-3">
            {/* Transport buttons */}
            <div className="flex shrink-0 items-center gap-1.5">
              <TransportBtn
                icon={<SkipBack size={16} />}
                title={t('dialog.preview.go_to_start')}
                onClick={goToStart}
                disabled={!timeline}
              />
              <TransportBtn
                icon={<StepBack size={16} />}
                title={t('dialog.preview.previous_segment')}
                onClick={stepBack}
                disabled={!timeline}
              />
              <TransportBtn
                icon={
                  playing ? (
                    <Pause size={18} strokeWidth={2.5} />
                  ) : (
                    <Play size={18} strokeWidth={2.5} />
                  )
                }
                title={playing ? t('dialog.preview.pause') : t('dialog.preview.play')}
                onClick={togglePlay}
                disabled={!timeline}
                primary
                active={playing}
                testId="preview-play-pause"
              />
              <TransportBtn
                icon={<StepForward size={16} />}
                title={t('dialog.preview.next_segment')}
                onClick={stepForward}
                disabled={!timeline}
              />
              <TransportBtn
                icon={<SkipForward size={16} />}
                title={t('dialog.preview.go_to_end')}
                onClick={goToEnd}
                disabled={!timeline}
              />
            </div>

            {/* Scrubber */}
            <div className="flex min-w-0 flex-1 items-center">
              <input
                type="range"
                min={0}
                max={playbackDuration || 1}
                step={0.01}
                value={currentTime}
                onPointerDown={pausePlayback}
                onChange={(e) => {
                  pausePlayback();
                  setPlaybackTime(Number(e.target.value));
                }}
                className="h-2 w-full cursor-pointer accent-bb-accent disabled:cursor-not-allowed"
                disabled={!timeline}
              />
            </div>

            {/* Speed selector */}
            <select
              value={playbackSpeed}
              onChange={(e) => setPlaybackSpeed(Number(e.target.value))}
              className="h-9 rounded-lg border border-bb-border bg-bb-bg px-2 text-xs font-medium text-bb-text outline-none transition-colors hover:border-bb-accent/40 focus:border-bb-accent"
            >
              {SPEED_OPTIONS.map((s) => (
                <option key={s} value={s}>
                  {t('dialog.preview.speed_multiplier', { value: s })}
                </option>
              ))}
            </select>
          </div>
        </div>
      </div>
    </MovableResizableDialogFrame>,
    document.body,
  );
}

function TransportBtn({
  icon,
  title,
  onClick,
  disabled,
  primary,
  active,
  testId,
}: {
  icon: React.ReactNode;
  title: string;
  onClick: () => void;
  disabled?: boolean;
  primary?: boolean;
  active?: boolean;
  testId?: string;
}) {
  return (
    <button
      type="button"
      title={title}
      aria-label={title}
      aria-pressed={primary ? active : undefined}
      data-testid={testId}
      onClick={onClick}
      disabled={disabled}
      className={`flex items-center justify-center rounded-lg border outline-none transition-colors focus-visible:ring-1 focus-visible:ring-bb-accent ${
        primary
          ? 'h-10 w-12 border-bb-accent bg-bb-accent text-bb-on-accent shadow-[0_0_12px_rgba(45,212,222,0.2)] hover:bg-bb-accent-hover'
          : 'h-9 w-9 border-bb-border bg-bb-bg text-bb-text-muted hover:border-bb-accent/40 hover:bg-bb-hover hover:text-bb-text'
      } disabled:opacity-60 disabled:cursor-not-allowed`}
    >
      {icon}
    </button>
  );
}

function PreviewOption({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
}) {
  return (
    <label
      className={`flex h-8 cursor-pointer items-center gap-1.5 rounded-lg border px-2.5 transition-colors ${
        checked
          ? 'border-bb-accent/45 bg-bb-accent/10 text-bb-text'
          : 'border-bb-border bg-bb-bg text-bb-text-muted hover:border-bb-accent/30 hover:bg-bb-hover'
      }`}
    >
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
        className="sr-only"
      />
      <span
        className={`flex h-3.5 w-3.5 items-center justify-center rounded border ${
          checked
            ? 'border-bb-accent bg-bb-accent text-bb-on-accent'
            : 'border-bb-control-border bg-bb-input'
        }`}
      >
        {checked ? <Check size={10} strokeWidth={3} /> : null}
      </span>
      {label}
    </label>
  );
}

function PreviewStat({ icon, label }: { icon: React.ReactNode; label: string }) {
  return (
    <span className="flex h-7 items-center gap-1.5 rounded-lg border border-bb-border bg-bb-surface px-2.5 tabular-nums">
      <span className="text-bb-accent">{icon}</span>
      {label}
    </span>
  );
}
