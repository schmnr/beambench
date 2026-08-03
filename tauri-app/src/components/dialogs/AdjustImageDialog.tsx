import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
  type WheelEvent as ReactWheelEvent,
} from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import {
  Check,
  EyeOff,
  ImageIcon,
  LoaderCircle,
  Minus,
  Plus,
  RefreshCw,
  RotateCcw,
  Save,
  SlidersHorizontal,
  Sparkles,
  Trash2,
  X,
} from 'lucide-react';
import { importService } from '../../services/importService';
import { projectService } from '../../services/projectService';
import { useNotificationStore } from '../../stores/notificationStore';
import { wrapBackendError } from '../../i18n/errors';
import { useProjectStore } from '../../stores/projectStore';
import { useAppStore } from '../../stores/appStore';
import { effectiveDpi, effectiveLineIntervalMm } from '../../types/rasterSettings';
import {
  displayToMm,
  labelWithUnit,
  lengthStep,
  lengthUnitLabel,
  mmToDisplay,
  roundDisplayLength,
} from '../../utils/lengthUnits';
import type { RasterAdjustments, RasterMode, RasterSettings } from '../../types/project';
import { MovableResizableDialogFrame } from '../shared/MovableResizableDialogFrame';
import { NumberInput } from '../shared/NumberInput';
import { RangeInput } from '../shared/RangeInput';
import { INSPECTOR_HELP_TEXT_CLASS } from '../shared/panelAppearance';

interface AdjustImageDialogProps {
  objectId: string;
  onClose: () => void;
}

interface PreviewData {
  png_base64: string;
  width: number;
  height: number;
}

const DEFAULT_ADJUSTMENTS: RasterAdjustments = {
  brightness: 0,
  contrast: 0,
  gamma: 1,
  invert: false,
  threshold: 128,
  saturation: 1,
  sharpen: 0,
  edge_enhance: false,
  enhance_radius: 0,
  enhance_amount: 0,
  enhance_denoise: 0,
};

// The backend compares request IDs across dialog lifetimes. Keep this sequence
// outside the component so reopening Adjust Image cannot restart at request 1.
let adjustPreviewRequestSequence = Date.now() * 1000;

function nextAdjustPreviewRequestId(): number {
  adjustPreviewRequestSequence += 1;
  return adjustPreviewRequestSequence;
}

function errorMessage(error: unknown): string {
  return wrapBackendError(error instanceof Error ? error.message : String(error));
}

function IconButton({
  icon,
  label,
  onClick,
  disabled = false,
  testId,
}: {
  icon: ReactNode;
  label: string;
  onClick: () => void;
  disabled?: boolean;
  testId?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      aria-label={label}
      title={label}
      data-testid={testId}
      className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-bb-border bg-bb-bg text-bb-text-muted transition-colors hover:border-bb-accent/40 hover:bg-bb-hover hover:text-bb-text focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent disabled:cursor-not-allowed disabled:opacity-45"
    >
      {icon}
    </button>
  );
}

function OptionChip({
  label,
  checked,
  onChange,
  icon,
  disabled = false,
  fullWidth = false,
}: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  icon?: ReactNode;
  disabled?: boolean;
  fullWidth?: boolean;
}) {
  return (
    <label
      className={`flex h-8 cursor-pointer items-center gap-2 rounded-lg border px-2.5 text-xs transition-colors ${fullWidth ? 'w-full' : ''} ${
        checked
          ? 'border-bb-accent/50 bg-bb-accent/10 text-bb-text'
          : 'border-bb-border bg-bb-bg text-bb-text-muted hover:border-bb-accent/30 hover:bg-bb-hover'
      } ${disabled ? 'cursor-not-allowed opacity-50' : ''}`}
    >
      {icon}
      <span className="min-w-0 flex-1 truncate">{label}</span>
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
        disabled={disabled}
        className="h-3.5 w-3.5 accent-bb-accent"
      />
    </label>
  );
}

export function AdjustImageDialog({ objectId, onClose }: AdjustImageDialogProps) {
  const { t } = useTranslation();
  const displayUnit = useAppStore((state) => state.settings?.display_unit) ?? 'mm';
  const object = useProjectStore(
    (state) => state.project?.objects.find((candidate) => candidate.id === objectId) ?? null,
  );
  const layer = useProjectStore((state) => {
    const currentObject = state.project?.objects.find((candidate) => candidate.id === objectId);
    if (!currentObject) return null;
    return state.project?.layers.find((candidate) => candidate.id === currentObject.layer_id) ?? null;
  });

  const assetKey =
    object?.data.type === 'raster_image'
      ? (object.data as { asset_key: string }).asset_key
      : null;
  const adjustments =
    object?.data.type === 'raster_image'
      ? (object.data as { adjustments?: RasterAdjustments }).adjustments ?? DEFAULT_ADJUSTMENTS
      : DEFAULT_ADJUSTMENTS;
  const rasterSettings = layer?.entries[0]?.raster_settings ?? null;

  const [brightness, setBrightness] = useState(adjustments.brightness);
  const [contrast, setContrast] = useState(adjustments.contrast);
  const [gamma, setGamma] = useState(adjustments.gamma);
  const [invertAdjust, setInvertAdjust] = useState(adjustments.invert);
  const [threshold, setThreshold] = useState(adjustments.threshold);
  const [saturation, setSaturation] = useState(adjustments.saturation);
  const [sharpen, setSharpen] = useState(adjustments.sharpen ?? 0);
  const [edgeEnhance, setEdgeEnhance] = useState(adjustments.edge_enhance ?? false);
  const [enhanceRadius, setEnhanceRadius] = useState(adjustments.enhance_radius ?? 0);
  const [enhanceAmount, setEnhanceAmount] = useState(adjustments.enhance_amount ?? 0);
  const [enhanceDenoise, setEnhanceDenoise] = useState(adjustments.enhance_denoise ?? 0);

  const [mode, setMode] = useState<RasterMode>(rasterSettings?.mode ?? 'grayscale');
  const [negative, setNegative] = useState(rasterSettings?.invert ?? false);
  const [lineInterval, setLineInterval] = useState(effectiveLineIntervalMm(rasterSettings));
  const [halftoneCpi, setHalftoneCpi] = useState(
    rasterSettings?.halftone_cells_per_inch ?? 10,
  );
  const [halftoneAngle, setHalftoneAngle] = useState(
    rasterSettings?.halftone_angle_deg ?? 0,
  );
  const [newsprintAngle, setNewsprintAngle] = useState(
    rasterSettings?.newsprint_angle_deg ?? 45,
  );
  const [newsprintFreq, setNewsprintFreq] = useState(
    rasterSettings?.newsprint_frequency ?? 10,
  );

  const [presets, setPresets] = useState<
    Array<{ name: string; adjustments: RasterAdjustments }>
  >([]);
  const [selectedPreset, setSelectedPreset] = useState('');
  const [showSaveInput, setShowSaveInput] = useState(false);
  const [savePresetName, setSavePresetName] = useState('');
  const [presetSaving, setPresetSaving] = useState(false);
  const [presetDeleting, setPresetDeleting] = useState(false);
  const [autoAdjusting, setAutoAdjusting] = useState(false);
  const [applying, setApplying] = useState(false);

  const [sourceBlobUrl, setSourceBlobUrl] = useState<string | null>(null);
  const [sourceLoading, setSourceLoading] = useState(true);
  const [sourceError, setSourceError] = useState<string | null>(null);
  const [sourceRefreshToken, setSourceRefreshToken] = useState(0);
  const [previewData, setPreviewData] = useState<PreviewData | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [previewRefreshToken, setPreviewRefreshToken] = useState(0);
  const requestIdRef = useRef(0);

  const [invertDisplay, setInvertDisplay] = useState(false);
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [isPanning, setIsPanning] = useState(false);
  const panDragRef = useRef<{
    pointerId: number;
    startX: number;
    startY: number;
    originX: number;
    originY: number;
  } | null>(null);

  useEffect(() => {
    if (!object || assetKey === null) onClose();
  }, [assetKey, object, onClose]);

  const loadPresets = useCallback(async () => {
    try {
      const next = await importService.getImagePresets();
      setPresets(Array.isArray(next) ? next : []);
    } catch (error) {
      setPresets([]);
      useNotificationStore.getState().push(errorMessage(error), 'error');
    }
  }, []);

  useEffect(() => {
    void loadPresets();
  }, [loadPresets]);

  useEffect(() => {
    if (!assetKey) return;
    let active = true;
    setSourceLoading(true);
    setSourceError(null);
    useProjectStore
      .getState()
      .loadAssetData(assetKey)
      .then((url) => {
        if (!active) return;
        if (!url) {
          setSourceBlobUrl(null);
          setSourceError(t('dialog.preview.generation_failed'));
          return;
        }
        setSourceBlobUrl(url);
      })
      .catch((error) => {
        if (!active) return;
        setSourceBlobUrl(null);
        setSourceError(errorMessage(error));
      })
      .finally(() => {
        if (active) setSourceLoading(false);
      });
    return () => {
      active = false;
    };
  }, [assetKey, sourceRefreshToken, t]);

  const currentAdjustments = useCallback(
    (): RasterAdjustments => ({
      ...adjustments,
      brightness,
      contrast,
      gamma,
      invert: invertAdjust,
      threshold,
      saturation,
      sharpen,
      edge_enhance: edgeEnhance,
      enhance_radius: enhanceRadius,
      enhance_amount: enhanceAmount,
      enhance_denoise: enhanceDenoise,
    }),
    [
      adjustments,
      brightness,
      contrast,
      edgeEnhance,
      enhanceAmount,
      enhanceDenoise,
      enhanceRadius,
      gamma,
      invertAdjust,
      saturation,
      sharpen,
      threshold,
    ],
  );

  const dpi = lineInterval > 0 ? Math.round(25.4 / lineInterval) : effectiveDpi(rasterSettings);

  useEffect(() => {
    const requestId = nextAdjustPreviewRequestId();
    requestIdRef.current = requestId;
    setPreviewError(null);
    void importService.cancelAdjustImagePreview(requestId).catch(() => undefined);

    const timer = window.setTimeout(() => {
      setPreviewLoading(true);
      importService
        .adjustImagePreview({
          objectId,
          brightness,
          contrast,
          gamma,
          invert: invertAdjust,
          threshold,
          saturation,
          sharpen,
          edgeEnhance,
          enhanceRadius,
          enhanceAmount,
          enhanceDenoise,
          mode,
          dpi,
          negative,
          passThrough: rasterSettings?.pass_through ?? false,
          halftoneCellsPerInch: halftoneCpi,
          halftoneAngleDeg: halftoneAngle,
          newsprintAngleDeg: newsprintAngle,
          newsprintFrequency: newsprintFreq,
          requestId,
        })
        .then((data) => {
          if (requestId !== requestIdRef.current) return;
          setPreviewData(data);
        })
        .catch((error) => {
          if (requestId !== requestIdRef.current) return;
          setPreviewError(errorMessage(error));
        })
        .finally(() => {
          if (requestId === requestIdRef.current) setPreviewLoading(false);
        });
    }, 300);

    return () => window.clearTimeout(timer);
  }, [
    brightness,
    contrast,
    dpi,
    edgeEnhance,
    enhanceAmount,
    enhanceDenoise,
    enhanceRadius,
    gamma,
    halftoneAngle,
    halftoneCpi,
    invertAdjust,
    mode,
    negative,
    newsprintAngle,
    newsprintFreq,
    objectId,
    previewRefreshToken,
    rasterSettings?.pass_through,
    saturation,
    sharpen,
    threshold,
  ]);

  useEffect(
    () => () => {
      const cancelId = nextAdjustPreviewRequestId();
      requestIdRef.current = cancelId;
      void importService.cancelAdjustImagePreview(cancelId).catch(() => undefined);
    },
    [],
  );

  const resetView = useCallback(() => {
    setZoom(1);
    setPan({ x: 0, y: 0 });
  }, []);

  const adjustZoom = useCallback((nextZoom: number) => {
    const clamped = Math.max(0.5, Math.min(10, nextZoom));
    setZoom(clamped);
    if (Math.abs(clamped - 1) < 1e-6) setPan({ x: 0, y: 0 });
  }, []);

  const handleWheel = useCallback(
    (event: ReactWheelEvent) => {
      event.preventDefault();
      event.stopPropagation();
      adjustZoom(zoom * (event.deltaY < 0 ? 1.15 : 1 / 1.15));
    },
    [adjustZoom, zoom],
  );

  const handlePointerDown = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      if (event.button !== 0) return;
      event.preventDefault();
      event.currentTarget.setPointerCapture(event.pointerId);
      panDragRef.current = {
        pointerId: event.pointerId,
        startX: event.clientX,
        startY: event.clientY,
        originX: pan.x,
        originY: pan.y,
      };
      setIsPanning(true);
    },
    [pan.x, pan.y],
  );

  const handlePointerMove = useCallback((event: ReactPointerEvent<HTMLDivElement>) => {
    const drag = panDragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    setPan({
      x: drag.originX + event.clientX - drag.startX,
      y: drag.originY + event.clientY - drag.startY,
    });
  }, []);

  const handlePointerEnd = useCallback((event: ReactPointerEvent<HTMLDivElement>) => {
    const drag = panDragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    panDragRef.current = null;
    setIsPanning(false);
  }, []);

  const handleResetAll = () => {
    setBrightness(0);
    setContrast(0);
    setGamma(1);
    setInvertAdjust(false);
    setThreshold(128);
    setSaturation(1);
    setSharpen(0);
    setEdgeEnhance(false);
    setEnhanceRadius(0);
    setEnhanceAmount(0);
    setEnhanceDenoise(0);
    setMode('grayscale');
    setNegative(false);
    setLineInterval(0.1);
    setHalftoneCpi(10);
    setHalftoneAngle(0);
    setNewsprintAngle(45);
    setNewsprintFreq(10);
    setSelectedPreset('');
  };

  const handleAutoAdjust = async () => {
    if (autoAdjusting || applying) return;
    setAutoAdjusting(true);
    try {
      const auto = await importService.autoAdjustImage(objectId);
      setBrightness(Math.round(auto.brightness * 100) / 100);
      setContrast(Math.round(auto.contrast * 100) / 100);
      setGamma(Math.round(auto.gamma * 100) / 100);
      setSharpen(Math.round(auto.sharpen * 100) / 100);
      setSelectedPreset('');
    } catch (error) {
      useNotificationStore.getState().push(errorMessage(error), 'error');
    } finally {
      setAutoAdjusting(false);
    }
  };

  const handlePresetSave = async () => {
    const presetName = savePresetName.trim();
    if (!presetName || presetSaving || applying) return;
    setPresetSaving(true);
    try {
      await importService.saveImagePreset(presetName, currentAdjustments());
      await loadPresets();
      setSelectedPreset(presetName);
      setShowSaveInput(false);
      setSavePresetName('');
    } catch (error) {
      useNotificationStore.getState().push(errorMessage(error), 'error');
    } finally {
      setPresetSaving(false);
    }
  };

  const handlePresetDelete = async () => {
    if (!selectedPreset || presetDeleting || applying) return;
    setPresetDeleting(true);
    try {
      await importService.deleteImagePreset(selectedPreset);
      await loadPresets();
      setSelectedPreset('');
    } catch (error) {
      useNotificationStore.getState().push(errorMessage(error), 'error');
    } finally {
      setPresetDeleting(false);
    }
  };

  const applyPreset = (name: string) => {
    const preset = presets.find((candidate) => candidate.name === name);
    setSelectedPreset(name);
    if (!preset) return;
    setBrightness(preset.adjustments.brightness);
    setContrast(preset.adjustments.contrast);
    setGamma(preset.adjustments.gamma);
    setInvertAdjust(preset.adjustments.invert);
    setThreshold(preset.adjustments.threshold);
    setSaturation(preset.adjustments.saturation);
    setSharpen(preset.adjustments.sharpen ?? 0);
    setEdgeEnhance(preset.adjustments.edge_enhance ?? false);
    setEnhanceRadius(preset.adjustments.enhance_radius ?? 0);
    setEnhanceAmount(preset.adjustments.enhance_amount ?? 0);
    setEnhanceDenoise(preset.adjustments.enhance_denoise ?? 0);
  };

  const handleOk = async () => {
    if (applying) return;
    const currentObject = useProjectStore
      .getState()
      .project?.objects.find((candidate) => candidate.id === objectId);
    if (!currentObject || currentObject.data.type !== 'raster_image' || !layer) {
      onClose();
      return;
    }

    const canonicalLineInterval = lineInterval > 0 ? lineInterval : 0.1;
    const nextRasterSettings: RasterSettings = {
      dpi: Math.max(1, Math.round(25.4 / canonicalLineInterval)),
      mode,
      scan_angle: rasterSettings?.scan_angle ?? 0,
      bidirectional: rasterSettings?.bidirectional ?? true,
      overscan_mm: rasterSettings?.overscan_mm ?? 0,
      passes: rasterSettings?.passes ?? 1,
      line_interval_mm: canonicalLineInterval,
      // These are configured elsewhere. Opening Adjust Image must not silently
      // clear layer options that this dialog does not expose.
      crosshatch: rasterSettings?.crosshatch ?? false,
      flood_fill: rasterSettings?.flood_fill ?? false,
      angle_passes: rasterSettings?.angle_passes ?? 1,
      angle_increment_deg: rasterSettings?.angle_increment_deg ?? 90,
      pass_through: rasterSettings?.pass_through ?? false,
      halftone_cells_per_inch: halftoneCpi,
      halftone_angle_deg: halftoneAngle,
      newsprint_angle_deg: newsprintAngle,
      newsprint_frequency: newsprintFreq,
      invert: negative,
      dot_width_correction_mm: rasterSettings?.dot_width_correction_mm ?? 0,
      ramp_length_mm: rasterSettings?.ramp_length_mm ?? 0,
    };

    setApplying(true);
    try {
      await projectService.applyAdjustImageDialog(
        objectId,
        currentAdjustments(),
        layer.id,
        nextRasterSettings,
      );
      await useProjectStore.getState().loadProject({ invalidatePreview: true });
      onClose();
    } catch (error) {
      useNotificationStore.getState().push(errorMessage(error), 'error');
      setApplying(false);
    }
  };

  const sourceWidth =
    object?.data.type === 'raster_image'
      ? (object.data as { original_width_px: number }).original_width_px
      : 0;
  const sourceHeight =
    object?.data.type === 'raster_image'
      ? (object.data as { original_height_px: number }).original_height_px
      : 0;
  const previewTransform = {
    transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
    transformOrigin: 'center center',
    filter: invertDisplay ? 'invert(1)' : undefined,
  };
  const originalTransform = {
    transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
    transformOrigin: 'center center',
  };
  const busy = applying || autoAdjusting || presetSaving || presetDeleting;
  const numberInputWidthClassName = 'w-24';
  const rasterModeOptions: Array<{ value: RasterMode; label: string }> = [
    { value: 'grayscale', label: t('dialog.adjust_image.mode_grayscale') },
    { value: 'threshold', label: t('dialog.adjust_image.mode_threshold') },
    { value: 'floyd_steinberg', label: t('dialog.adjust_image.mode_floyd_steinberg') },
    { value: 'ordered_dither', label: t('dialog.adjust_image.mode_ordered_dither') },
    { value: 'stucki', label: t('dialog.adjust_image.mode_stucki') },
    { value: 'jarvis', label: t('dialog.adjust_image.mode_jarvis') },
    { value: 'sierra', label: t('dialog.adjust_image.mode_sierra') },
    { value: 'atkinson', label: t('dialog.adjust_image.mode_atkinson') },
    { value: 'halftone', label: t('dialog.adjust_image.mode_halftone') },
    { value: 'newsprint', label: t('dialog.adjust_image.mode_newsprint') },
    { value: 'sketch', label: t('dialog.adjust_image.mode_sketch') },
  ];
  const previewPaneClass = `relative min-h-0 flex-1 overflow-hidden rounded-xl border border-bb-accent/35 bg-bb-bg shadow-inner ${isPanning ? 'cursor-grabbing' : 'cursor-grab'}`;

  return createPortal(
    <MovableResizableDialogFrame
      title={t('dialog.adjust_image.title')}
      titleId="adjust-image-title"
      testId="adjust-image-dialog"
      initialWidth={1080}
      initialHeight={700}
      minWidth={780}
      minHeight={520}
      onRequestClose={busy ? undefined : onClose}
      backdropClassName="bg-black/50"
      headerActions={
        <button
          type="button"
          onClick={onClose}
          disabled={busy}
          aria-label={t('common.close')}
          className="flex h-7 w-7 items-center justify-center rounded-lg text-bb-text-muted transition-colors hover:bg-bb-hover hover:text-bb-text focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent disabled:cursor-not-allowed disabled:opacity-45"
        >
          <X size={15} />
        </button>
      }
      footer={
        <div className="flex items-center justify-end gap-2 px-4 py-3">
          <button
            type="button"
            onClick={onClose}
            disabled={busy}
            className="h-9 rounded-lg border border-bb-border bg-bb-bg px-3 text-xs font-medium text-bb-text transition-colors hover:border-bb-accent/40 hover:bg-bb-hover focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent disabled:cursor-not-allowed disabled:opacity-50"
          >
            {t('common.cancel')}
          </button>
          <button
            type="button"
            onClick={() => void handleOk()}
            disabled={busy}
            data-testid="adjust-submit"
            className="flex h-9 min-w-28 items-center justify-center gap-2 rounded-lg bg-bb-accent px-4 text-xs font-semibold text-bb-on-accent transition-colors hover:bg-bb-accent-hover focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent disabled:cursor-not-allowed disabled:opacity-60"
          >
            {applying && (
              <LoaderCircle size={14} className="animate-spin motion-reduce:animate-none" />
            )}
            {t('common.ok')}
          </button>
        </div>
      }
    >
      <div className="flex min-h-0 flex-1 overflow-hidden">
        <section className="flex min-w-0 flex-1 flex-col p-3 pr-2">
          <div className="mb-2 flex min-h-9 items-center gap-2">
            <div className="flex min-w-0 items-center gap-2 text-xs text-bb-text-dim" aria-live="polite">
              <ImageIcon size={14} className="shrink-0 text-bb-accent" />
              <span className="truncate">
                {previewLoading
                  ? t('dialog.adjust_image.updating')
                  : `${sourceWidth} × ${sourceHeight}`}
              </span>
            </div>
            <div className="flex-1" />
            <span className="w-12 text-right text-xs tabular-nums text-bb-text-dim">
              {Math.round(zoom * 100)}%
            </span>
            <IconButton
              icon={<Minus size={14} />}
              label={t('menus.view.zoom_out')}
              onClick={() => adjustZoom(zoom / 1.2)}
            />
            <button
              type="button"
              onClick={resetView}
              aria-pressed={
                Math.abs(zoom - 1) < 1e-6 && Math.abs(pan.x) < 1e-6 && Math.abs(pan.y) < 1e-6
              }
              className="h-8 rounded-lg border border-bb-border bg-bb-bg px-2.5 text-xs text-bb-text transition-colors hover:border-bb-accent/40 hover:bg-bb-hover focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent aria-pressed:border-bb-accent/50 aria-pressed:bg-bb-accent/10"
            >
              {t('dialog.trace_image.fit')}
            </button>
            <IconButton
              icon={<Plus size={14} />}
              label={t('menus.view.zoom_in')}
              onClick={() => adjustZoom(zoom * 1.2)}
            />
          </div>

          <div className="flex min-h-0 flex-1 gap-2">
            <div className="flex min-w-0 flex-1 flex-col gap-1.5">
              <div className="flex items-center justify-between px-1 text-xs">
                <span className="font-medium text-bb-text">{t('dialog.adjust_image.original')}</span>
                <span className="tabular-nums text-bb-text-dim">
                  {sourceWidth > 0 ? `${sourceWidth} × ${sourceHeight}` : ''}
                </span>
              </div>
              <div
                className={previewPaneClass}
                onWheel={handleWheel}
                onPointerDown={handlePointerDown}
                onPointerMove={handlePointerMove}
                onPointerUp={handlePointerEnd}
                onPointerCancel={handlePointerEnd}
                data-testid="adjust-original-preview"
              >
                {sourceBlobUrl && (
                  <img
                    src={sourceBlobUrl}
                    alt={t('dialog.adjust_image.original')}
                    className="h-full w-full select-none object-contain pointer-events-none"
                    style={originalTransform}
                    onError={() => {
                      setSourceBlobUrl(null);
                      setSourceError(t('dialog.preview.generation_failed'));
                    }}
                  />
                )}
                {sourceLoading && (
                  <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 bg-black/30">
                    <LoaderCircle size={20} className="animate-spin text-bb-accent motion-reduce:animate-none" />
                    <span className="text-xs text-bb-text">{t('dialog.adjust_image.loading')}</span>
                  </div>
                )}
                {sourceError && !sourceLoading && (
                  <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 bg-black/50 px-6 text-center" role="alert">
                    <span className="text-xs text-bb-error-fg">{sourceError}</span>
                    <button
                      type="button"
                      onClick={() => setSourceRefreshToken((token) => token + 1)}
                      className="flex h-8 items-center gap-1.5 rounded-lg border border-bb-border bg-bb-panel px-2.5 text-xs text-bb-text hover:border-bb-accent/40 hover:bg-bb-hover focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent"
                    >
                      <RefreshCw size={13} />
                      {t('dialog.preview.refresh')}
                    </button>
                  </div>
                )}
              </div>
            </div>

            <div className="flex min-w-0 flex-1 flex-col gap-1.5">
              <div className="flex items-center justify-between px-1 text-xs">
                <span className="font-medium text-bb-text">{t('dialog.adjust_image.processed')}</span>
                <span className="tabular-nums text-bb-text-dim">
                  {previewData ? `${previewData.width} × ${previewData.height}` : ''}
                </span>
              </div>
              <div
                className={previewPaneClass}
                onWheel={handleWheel}
                onPointerDown={handlePointerDown}
                onPointerMove={handlePointerMove}
                onPointerUp={handlePointerEnd}
                onPointerCancel={handlePointerEnd}
                data-testid="adjust-processed-preview"
              >
                {previewData && (
                  <img
                    src={`data:image/png;base64,${previewData.png_base64}`}
                    alt={t('dialog.adjust_image.processed')}
                    className="h-full w-full select-none object-contain pointer-events-none"
                    style={previewTransform}
                  />
                )}
                {previewLoading && (
                  <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 bg-black/30">
                    <LoaderCircle size={20} className="animate-spin text-bb-accent motion-reduce:animate-none" />
                    <span className="text-xs text-bb-text">{t('dialog.adjust_image.updating')}</span>
                  </div>
                )}
                {previewError && !previewLoading && (
                  <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 bg-black/50 px-6 text-center" role="alert">
                    <span className="text-xs text-bb-error-fg">{previewError}</span>
                    <button
                      type="button"
                      onClick={() => setPreviewRefreshToken((token) => token + 1)}
                      className="flex h-8 items-center gap-1.5 rounded-lg border border-bb-border bg-bb-panel px-2.5 text-xs text-bb-text hover:border-bb-accent/40 hover:bg-bb-hover focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent"
                    >
                      <RefreshCw size={13} />
                      {t('dialog.preview.refresh')}
                    </button>
                  </div>
                )}
                {!previewData && !previewLoading && !previewError && (
                  <div className="absolute inset-0 flex items-center justify-center text-xs text-bb-text-dim">
                    {t('dialog.adjust_image.no_preview')}
                  </div>
                )}
              </div>
            </div>
          </div>

          <div className="mt-2 flex items-center gap-2">
            <OptionChip
              label={t('dialog.adjust_image.invert_display')}
              checked={invertDisplay}
              onChange={setInvertDisplay}
              icon={<EyeOff size={12} />}
            />
          </div>
          <div className={`mt-1 ${INSPECTOR_HELP_TEXT_CLASS}`} data-testid="invert-display-help">
            {t('dialog.adjust_image.invert_display_help', {
              defaultValue: 'Preview only. The saved image and laser output are unchanged.',
            })}
          </div>
        </section>

        <aside className="w-[330px] shrink-0 overflow-y-auto border-l border-bb-border bg-bb-surface">
          <div className="flex h-10 items-center justify-between border-b border-bb-border bg-gradient-to-r from-bb-accent/10 to-bb-surface/30 px-3">
            <div className="flex items-center gap-2">
              <SlidersHorizontal size={14} className="text-bb-accent" />
              <span className="text-xs font-medium text-bb-text">{t('menus.edit.settings')}</span>
            </div>
            <div className="flex items-center gap-1.5">
              <button
                type="button"
                data-testid="adjust-auto"
                onClick={() => void handleAutoAdjust()}
                disabled={busy}
                className="flex h-7 items-center gap-1.5 rounded-lg px-2 text-xs text-bb-text-muted transition-colors hover:bg-bb-hover hover:text-bb-text focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent disabled:cursor-not-allowed disabled:opacity-50"
              >
                {autoAdjusting ? (
                  <LoaderCircle size={13} className="animate-spin motion-reduce:animate-none" />
                ) : (
                  <Sparkles size={13} />
                )}
                {t('dialog.adjust_image.auto')}
              </button>
              <button
                type="button"
                onClick={handleResetAll}
                disabled={busy}
                aria-label={t('dialog.adjust_image.reset_all')}
                title={t('dialog.adjust_image.reset_all')}
                className="flex h-7 w-7 items-center justify-center rounded-lg text-bb-text-muted transition-colors hover:bg-bb-hover hover:text-bb-text focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent disabled:cursor-not-allowed disabled:opacity-50"
              >
                <RotateCcw size={13} />
              </button>
            </div>
          </div>

          <div className="flex flex-col gap-4 p-3">
            <section className="flex flex-col gap-2">
              <div className="flex items-center justify-between gap-2">
                <span className="text-xs font-medium text-bb-text">
                  {t('dialog.adjust_image.presets')}
                </span>
                <div className="flex items-center gap-1">
                  <IconButton
                    icon={presetSaving ? <LoaderCircle size={13} className="animate-spin motion-reduce:animate-none" /> : <Save size={13} />}
                    label={t('common.save')}
                    onClick={() => setShowSaveInput(true)}
                    disabled={busy || showSaveInput}
                  />
                  <IconButton
                    icon={presetDeleting ? <LoaderCircle size={13} className="animate-spin motion-reduce:animate-none" /> : <Trash2 size={13} />}
                    label={t('dialog.adjust_image.delete')}
                    onClick={() => void handlePresetDelete()}
                    disabled={busy || !selectedPreset}
                  />
                </div>
              </div>
              <select
                data-testid="adjust-preset-select"
                value={selectedPreset}
                onChange={(event) => applyPreset(event.target.value)}
                disabled={busy}
                aria-label={t('dialog.adjust_image.presets')}
                className="h-8 w-full rounded-lg border border-bb-control-border bg-bb-input px-2 text-xs text-bb-text focus:border-bb-accent focus:outline-none disabled:opacity-50"
              >
                <option value="">{t('dialog.adjust_image.select_preset')}</option>
                {presets.map((preset) => (
                  <option key={preset.name} value={preset.name}>
                    {preset.name}
                  </option>
                ))}
              </select>
              {showSaveInput && (
                <div className="flex items-center gap-1.5">
                  <input
                    type="text"
                    value={savePresetName}
                    onChange={(event) => setSavePresetName(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter' && savePresetName.trim()) {
                        event.preventDefault();
                        event.stopPropagation();
                        void handlePresetSave();
                      } else if (event.key === 'Escape') {
                        event.preventDefault();
                        event.stopPropagation();
                        setShowSaveInput(false);
                        setSavePresetName('');
                      }
                    }}
                    placeholder={t('dialog.adjust_image.preset_name')}
                    aria-label={t('dialog.adjust_image.preset_name')}
                    autoFocus
                    disabled={presetSaving}
                    className="h-8 min-w-0 flex-1 rounded-lg border border-bb-control-border bg-bb-input px-2 text-xs text-bb-text focus:border-bb-accent focus:outline-none"
                  />
                  <IconButton
                    icon={<Check size={13} />}
                    label={t('common.ok')}
                    onClick={() => void handlePresetSave()}
                    disabled={!savePresetName.trim() || presetSaving}
                    testId="adjust-save-preset-confirm"
                  />
                  <IconButton
                    icon={<X size={13} />}
                    label={t('common.cancel')}
                    onClick={() => {
                      setShowSaveInput(false);
                      setSavePresetName('');
                    }}
                    disabled={presetSaving}
                  />
                </div>
              )}
            </section>

            <div className="border-t border-bb-border pt-3">
              <div className="mb-3 text-xs font-medium text-bb-text">
                {t('dialog.adjust_image.image_settings')}
              </div>
              <div className="flex flex-col gap-3">
                <RangeInput label={t('dialog.adjust_image.contrast')} value={contrast} onChange={setContrast} min={-1} max={1} step={0.01} disabled={busy} testId="adjust-contrast" />
                <RangeInput label={t('dialog.adjust_image.brightness')} value={brightness} onChange={setBrightness} min={-1} max={1} step={0.01} disabled={busy} testId="adjust-brightness" />
                <RangeInput label={t('dialog.adjust_image.gamma')} value={gamma} onChange={setGamma} min={0.1} max={3} step={0.01} disabled={busy} testId="adjust-gamma" />
                {(mode === 'threshold' || rasterSettings?.pass_through) && (
                  <RangeInput label={t('dialog.adjust_image.threshold')} value={threshold} onChange={(value) => setThreshold(Math.round(value))} min={0} max={255} step={1} disabled={busy} testId="adjust-threshold" />
                )}
                {!rasterSettings?.pass_through && (
                  <>
                    <RangeInput label={t('dialog.adjust_image.saturation')} value={saturation} onChange={setSaturation} min={0} max={2} step={0.01} disabled={busy} testId="adjust-saturation" />
                    <RangeInput label={t('dialog.adjust_image.sharpen')} value={sharpen} onChange={setSharpen} min={0} max={2} step={0.01} disabled={busy} testId="adjust-sharpen" />
                    <div className="grid grid-cols-2 gap-1.5">
                      <OptionChip label={t('dialog.adjust_image.invert')} checked={invertAdjust} onChange={setInvertAdjust} disabled={busy} fullWidth />
                      <OptionChip label={t('dialog.adjust_image.edge_enhance')} checked={edgeEnhance} onChange={setEdgeEnhance} disabled={busy} fullWidth />
                    </div>
                    <div className={INSPECTOR_HELP_TEXT_CLASS}>
                      {t('dialog.adjust_image.invert_adjustment_help', {
                        defaultValue: 'Invert is saved with this image and runs before the layer’s raster processing.',
                      })}
                    </div>
                  </>
                )}
                <RangeInput label={t('dialog.adjust_image.enhance_radius')} value={enhanceRadius} onChange={setEnhanceRadius} min={0} max={10} step={0.1} disabled={busy} testId="adjust-enhance-radius" />
                <RangeInput label={t('dialog.adjust_image.enhance_amount')} value={enhanceAmount} onChange={setEnhanceAmount} min={0} max={3} step={0.1} disabled={busy} testId="adjust-enhance-amount" />
                <RangeInput label={t('dialog.adjust_image.enhance_denoise')} value={enhanceDenoise} onChange={setEnhanceDenoise} min={0} max={3} step={0.1} disabled={busy} testId="adjust-enhance-denoise" />
              </div>
            </div>

            <div className="border-t border-bb-border pt-3">
              <div className="mb-3 text-xs font-medium text-bb-text">
                {t('dialog.adjust_image.layer_settings')}
              </div>
              <div className="flex flex-col gap-2.5">
                <label className="flex items-center justify-between gap-2 text-xs">
                  <span className="text-bb-text-muted">{t('dialog.adjust_image.image_mode')}</span>
                  <select
                    value={mode}
                    onChange={(event) => setMode(event.target.value as RasterMode)}
                    disabled={busy}
                    className="h-8 w-40 rounded-lg border border-bb-control-border bg-bb-input px-2 text-xs text-bb-text focus:border-bb-accent focus:outline-none disabled:opacity-50"
                  >
                    {rasterModeOptions.map((option) => (
                      <option key={option.value} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                </label>
                <OptionChip label={t('dialog.adjust_image.negative_image')} checked={negative} onChange={setNegative} disabled={busy} fullWidth />
                <div className={INSPECTOR_HELP_TEXT_CLASS}>
                  {t('dialog.adjust_image.negative_image_help', {
                    defaultValue: 'Negative Image inverts the layer output. If Invert above is also on, the two inversions can cancel.',
                  })}
                </div>
                {mode === 'halftone' && (
                  <>
                    <NumberInput label={t('dialog.adjust_image.cells_per_inch')} value={halftoneCpi} onChange={(value) => setHalftoneCpi(Math.round(value))} min={1} max={100} step={1} disabled={busy} inputWidthClassName={numberInputWidthClassName} />
                    <NumberInput label={t('dialog.adjust_image.halftone_angle')} value={halftoneAngle} onChange={setHalftoneAngle} min={0} max={360} step={0.5} disabled={busy} inputWidthClassName={numberInputWidthClassName} />
                  </>
                )}
                {mode === 'newsprint' && (
                  <>
                    <NumberInput label={t('dialog.adjust_image.newsprint_angle')} value={newsprintAngle} onChange={setNewsprintAngle} min={0} max={360} step={1} disabled={busy} inputWidthClassName={numberInputWidthClassName} />
                    <NumberInput label={t('dialog.adjust_image.frequency')} value={newsprintFreq} onChange={setNewsprintFreq} min={1} max={100} step={1} disabled={busy} inputWidthClassName={numberInputWidthClassName} />
                  </>
                )}
                <NumberInput
                  label={labelWithUnit(t('dialog.adjust_image.line_interval'), lengthUnitLabel(displayUnit))}
                  value={roundDisplayLength(mmToDisplay(lineInterval, displayUnit), displayUnit)}
                  onChange={(value) => setLineInterval(displayToMm(value, displayUnit))}
                  min={mmToDisplay(0.01, displayUnit)}
                  max={mmToDisplay(10, displayUnit)}
                  step={lengthStep(displayUnit, 0.001, 0.001)}
                  disabled={busy}
                  inputWidthClassName={numberInputWidthClassName}
                />
                <NumberInput
                  label={t('dialog.adjust_image.dpi')}
                  value={dpi}
                  onChange={(value) => {
                    const nextDpi = Math.max(1, value);
                    setLineInterval(parseFloat((25.4 / nextDpi).toFixed(4)));
                  }}
                  min={1}
                  max={2540}
                  step={1}
                  disabled={busy}
                  inputWidthClassName={numberInputWidthClassName}
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
