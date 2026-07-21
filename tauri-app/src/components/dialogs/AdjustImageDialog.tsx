import { useState, useEffect, useRef, useCallback } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { importService } from '../../services/importService';
import { projectService } from '../../services/projectService';
import { useNotificationStore } from '../../stores/notificationStore';
import { wrapBackendError } from '../../i18n/errors';
import { useProjectStore } from '../../stores/projectStore';
import { useAppStore } from '../../stores/appStore';
import { NumberStepper } from '../shared/NumberStepper';
import { Toggle } from '../shared/Toggle';
import { effectiveLineIntervalMm, effectiveDpi } from '../../types/rasterSettings';
import { mmToDisplay, displayToMm, roundDisplayLength, lengthStep, lengthUnitLabel, labelWithUnit } from '../../utils/lengthUnits';
import type { RasterAdjustments, RasterMode, RasterSettings } from '../../types/project';

interface AdjustImageDialogProps {
  objectId: string;
  onClose: () => void;
}

const DEFAULT_ADJUSTMENTS: RasterAdjustments = {
  brightness: 0, contrast: 0, gamma: 1, invert: false, threshold: 128,
  saturation: 1, sharpen: 0, edge_enhance: false,
  enhance_radius: 0, enhance_amount: 0, enhance_denoise: 0,
};

/** Beam Bench slider and numeric-stepper control for image settings. */
function SliderInput({ label, value, onChange, min, max, step, testId }: {
  label: string; value: number; onChange: (v: number) => void;
  min: number; max: number; step: number;
  testId?: string;
}) {
  const inputCls = 'w-[60px] px-1 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text text-right focus:outline-none focus:border-bb-accent';
  return (
    <div className="flex items-center gap-2 text-xs">
      <span className="text-bb-text-muted w-24 shrink-0 text-right">{label}:</span>
      <input
        type="range" min={min} max={max} step={step} value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="flex-1 h-1 accent-bb-accent"
      />
      <NumberStepper
        value={value} min={min} max={max} step={step}
        onChange={(e) => onChange(Number(e.target.value))}
        className={inputCls}
        data-testid={testId}
      />
    </div>
  );
}

export function AdjustImageDialog({ objectId, onClose }: AdjustImageDialogProps) {
  const { t } = useTranslation();
  const displayUnit = useAppStore((s) => s.settings?.display_unit) ?? 'mm';
  const object = useProjectStore((s) => s.project?.objects.find((o) => o.id === objectId) ?? null);
  const layer = useProjectStore((s) => {
    const obj = s.project?.objects.find((o) => o.id === objectId);
    if (!obj) return null;
    return s.project?.layers.find((l) => l.id === obj.layer_id) ?? null;
  });

  const assetKey = object?.data.type === 'raster_image' ? (object.data as { asset_key: string }).asset_key : null;
  const adj = object?.data.type === 'raster_image'
    ? (object.data as { adjustments?: RasterAdjustments }).adjustments ?? DEFAULT_ADJUSTMENTS
    : DEFAULT_ADJUSTMENTS;

  useEffect(() => {
    if (!object || assetKey === null) {
      onClose();
    }
  }, [assetKey, object, onClose]);

  // --- Image settings state ---
  const [brightness, setBrightness] = useState(adj.brightness);
  const [contrast, setContrast] = useState(adj.contrast);
  const [gamma, setGamma] = useState(adj.gamma);
  const [invertAdjust, setInvertAdjust] = useState(adj.invert);
  const [threshold, setThreshold] = useState(adj.threshold);
  const [saturation, setSaturation] = useState(adj.saturation);
  const [sharpen, setSharpen] = useState(adj.sharpen ?? 0);
  const [edgeEnhance, setEdgeEnhance] = useState(adj.edge_enhance ?? false);
  const [enhanceRadius, setEnhanceRadius] = useState(adj.enhance_radius ?? 0);
  const [enhanceAmount, setEnhanceAmount] = useState(adj.enhance_amount ?? 0);
  const [enhanceDenoise, setEnhanceDenoise] = useState(adj.enhance_denoise ?? 0);

  // --- Layer settings state (only image-relevant, not cut/speed/power) ---
  const rs = layer?.entries[0]?.raster_settings ?? null;
  const [mode, setMode] = useState<RasterMode>(rs?.mode ?? 'grayscale');
  const [negative, setNegative] = useState(rs?.invert ?? false);
  const [lineInterval, setLineInterval] = useState(effectiveLineIntervalMm(rs));
  const [halftoneCpi, setHalftoneCpi] = useState(rs?.halftone_cells_per_inch ?? 10);
  const [halftoneAngle, setHalftoneAngle] = useState(rs?.halftone_angle_deg ?? 0);
  const [newsprintAngle, setNewsprintAngle] = useState(rs?.newsprint_angle_deg ?? 45);
  const [newsprintFreq, setNewsprintFreq] = useState(rs?.newsprint_frequency ?? 10);

  // --- Preset state ---
  const [presets, setPresets] = useState<Array<{ name: string; adjustments: RasterAdjustments }>>([]);
  const [selectedPreset, setSelectedPreset] = useState('');
  const [showSaveInput, setShowSaveInput] = useState(false);
  const [savePresetName, setSavePresetName] = useState('');
  const loadPresets = useCallback(async () => {
    const next = await importService.getImagePresets();
    setPresets(Array.isArray(next) ? next : []);
  }, []);
  useEffect(() => { void loadPresets(); }, [loadPresets]);

  // --- Preview state ---
  const [sourceBlobUrl, setSourceBlobUrl] = useState<string | null>(null);
  const [previewData, setPreviewData] = useState<{ png_base64: string; width: number; height: number } | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [invertDisplay, setInvertDisplay] = useState(false);
  const requestIdRef = useRef(0);

  // --- Drag/resize state ---
  const [dialogPos, setDialogPos] = useState<{ x: number; y: number } | null>(null);
  const [dialogSize, setDialogSize] = useState<{ w: number; h: number } | null>(null);
  const headerDragRef = useRef<{ startX: number; startY: number; origX: number; origY: number } | null>(null);
  const resizeDragRef = useRef<{ startX: number; startY: number; origW: number; origH: number } | null>(null);

  // --- Zoom/pan state for previews ---
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const panDragRef = useRef<{ startX: number; startY: number; origPanX: number; origPanY: number } | null>(null);

  useEffect(() => {
    if (!assetKey) return;
    useProjectStore.getState().loadAssetData(assetKey).then(setSourceBlobUrl);
  }, [assetKey]);

  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [onClose]);

  const currentAdjustments = (): RasterAdjustments => ({
    ...adj,
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
  });

  // Debounced preview
  const dpi = lineInterval > 0 ? Math.round(25.4 / lineInterval) : effectiveDpi(rs);
  useEffect(() => {
    const myId = ++requestIdRef.current;
    const timer = setTimeout(() => {
      setPreviewLoading(true);
      importService.adjustImagePreview({
        objectId, brightness, contrast, gamma,
        invert: invertAdjust, threshold, saturation,
        sharpen, edgeEnhance,
        enhanceRadius, enhanceAmount, enhanceDenoise,
        mode, dpi, negative, passThrough: rs?.pass_through ?? false,
        halftoneCellsPerInch: halftoneCpi, halftoneAngleDeg: halftoneAngle,
        newsprintAngleDeg: newsprintAngle, newsprintFrequency: newsprintFreq,
      }).then((data) => {
        if (myId !== requestIdRef.current) return;
        setPreviewData(data);
        setPreviewLoading(false);
      }).catch(() => {
        if (myId !== requestIdRef.current) return;
        setPreviewData(null);
        setPreviewLoading(false);
      });
    }, 300);
    return () => clearTimeout(timer);
  }, [objectId, brightness, contrast, gamma, invertAdjust, threshold, saturation,
      sharpen, edgeEnhance, enhanceRadius, enhanceAmount, enhanceDenoise,
      mode, dpi, negative, halftoneCpi, halftoneAngle, newsprintAngle, newsprintFreq, rs?.pass_through]);

  const handleResetAll = () => {
    // Object adjustments
    setBrightness(0); setContrast(0); setGamma(1.0);
    setInvertAdjust(false); setThreshold(128); setSaturation(1);
    setSharpen(0); setEdgeEnhance(false);
    setEnhanceRadius(0); setEnhanceAmount(0); setEnhanceDenoise(0);
    // Layer-side raster settings
    setMode('grayscale');
    setNegative(false);
    setLineInterval(0.1);
    setHalftoneCpi(10);
    setHalftoneAngle(0);
    setNewsprintAngle(45);
    setNewsprintFreq(10);
  };

  const handleAutoAdjust = async () => {
    try {
      const auto = await importService.autoAdjustImage(objectId);
      setBrightness(Math.round(auto.brightness * 100) / 100);
      setContrast(Math.round(auto.contrast * 100) / 100);
      setGamma(Math.round(auto.gamma * 100) / 100);
      setSharpen(Math.round(auto.sharpen * 100) / 100);
      setSelectedPreset('');
    } catch (error) {
      useNotificationStore.getState().push(wrapBackendError(String(error)), 'error');
    }
  };

  const handlePresetSave = async () => {
    const presetName = savePresetName.trim();
    if (!presetName) return;
    try {
      await importService.saveImagePreset(presetName, currentAdjustments());
      await loadPresets();
      setSelectedPreset(presetName);
      setShowSaveInput(false);
      setSavePresetName('');
    } catch (error) {
      useNotificationStore.getState().push(wrapBackendError(String(error)), 'error');
    }
  };

  const handlePresetDelete = async () => {
    if (!selectedPreset) return;
    try {
      await importService.deleteImagePreset(selectedPreset);
      await loadPresets();
      setSelectedPreset('');
    } catch (error) {
      useNotificationStore.getState().push(wrapBackendError(String(error)), 'error');
    }
  };

  const handleOk = async () => {
    // commit object adjustments and layer raster settings atomically
    // so the dialog produces a single undo step and never a partial apply.
    const obj = useProjectStore.getState().project?.objects.find((o) => o.id === objectId);
    if (!obj || obj.data.type !== 'raster_image' || !layer) {
      onClose();
      return;
    }
    const adjustments = currentAdjustments();
    const canonicalLi = lineInterval > 0 ? lineInterval : 0.1;
    const rasterSettings: RasterSettings = {
      dpi: Math.max(1, Math.round(25.4 / canonicalLi)),
      mode, scan_angle: rs?.scan_angle ?? 0, bidirectional: rs?.bidirectional ?? true,
      overscan_mm: rs?.overscan_mm ?? 0, passes: rs?.passes ?? 1,
      line_interval_mm: canonicalLi, crosshatch: false, flood_fill: false,
      angle_passes: rs?.angle_passes ?? 1, angle_increment_deg: rs?.angle_increment_deg ?? 90,
      pass_through: rs?.pass_through ?? false,
      halftone_cells_per_inch: halftoneCpi, halftone_angle_deg: halftoneAngle,
      newsprint_angle_deg: newsprintAngle, newsprint_frequency: newsprintFreq,
      invert: negative, dot_width_correction_mm: rs?.dot_width_correction_mm ?? 0,
      ramp_length_mm: rs?.ramp_length_mm ?? 0,
    };
    try {
      await projectService.applyAdjustImageDialog(objectId, adjustments, layer.id, rasterSettings);
      await useProjectStore.getState().loadProject({ invalidatePreview: true });
    } catch (e) {
      useNotificationStore.getState().push(wrapBackendError(String(e)), 'error');
      // Do not close on failure — user can retry or cancel explicitly.
      return;
    }
    onClose();
  };

  const handleWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    const factor = e.deltaY < 0 ? 1.15 : 1 / 1.15;
    setZoom((z) => Math.max(0.5, Math.min(10, z * factor)));
  }, []);

  const handlePreviewMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) return;
    panDragRef.current = { startX: e.clientX, startY: e.clientY, origPanX: pan.x, origPanY: pan.y };
  }, [pan]);

  const handlePreviewMouseMove = useCallback((e: React.MouseEvent) => {
    if (!panDragRef.current) return;
    const dx = e.clientX - panDragRef.current.startX;
    const dy = e.clientY - panDragRef.current.startY;
    setPan({ x: panDragRef.current.origPanX + dx, y: panDragRef.current.origPanY + dy });
  }, []);

  const handlePreviewMouseUp = useCallback(() => { panDragRef.current = null; }, []);

  const sourceWidth = object?.data.type === 'raster_image' ? (object.data as { original_width_px: number }).original_width_px : 0;
  const sourceHeight = object?.data.type === 'raster_image' ? (object.data as { original_height_px: number }).original_height_px : 0;

  const previewImgStyle: React.CSSProperties = {
    transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
    transformOrigin: 'center center',
    cursor: panDragRef.current ? 'grabbing' : 'grab',
  };

  const inputCls = 'w-[70px] px-1.5 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text text-right focus:outline-none focus:border-bb-accent';
  const rasterModeOptions: { value: RasterMode; label: string }[] = [
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

  return createPortal(
    <div
      className="fixed inset-0 bg-black/50 z-50"
      style={{ display: 'flex', alignItems: dialogPos ? 'flex-start' : 'center', justifyContent: dialogPos ? 'flex-start' : 'center' }}
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
      onMouseMove={(e) => {
        if (headerDragRef.current) {
          setDialogPos({
            x: headerDragRef.current.origX + e.clientX - headerDragRef.current.startX,
            y: headerDragRef.current.origY + e.clientY - headerDragRef.current.startY,
          });
        }
        if (resizeDragRef.current) {
          setDialogSize({
            w: Math.max(700, resizeDragRef.current.origW + e.clientX - resizeDragRef.current.startX),
            h: Math.max(400, resizeDragRef.current.origH + e.clientY - resizeDragRef.current.startY),
          });
        }
      }}
      onMouseUp={() => { headerDragRef.current = null; resizeDragRef.current = null; }}
    >
      <div
        className="bg-bb-panel border border-bb-border rounded-lg shadow-xl flex flex-col relative"
        style={{
          ...(dialogPos ? { position: 'absolute', left: dialogPos.x, top: dialogPos.y } : {}),
          ...(dialogSize ? { width: dialogSize.w, height: dialogSize.h } : { width: 920, maxHeight: '95vh' }),
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Drag bar */}
        <div
          className="flex items-center justify-center h-6 cursor-grab active:cursor-grabbing select-none shrink-0 rounded-t-lg hover:bg-bb-bg/50"
          onMouseDown={(e) => {
            const rect = e.currentTarget.closest('[class*="bg-bb-panel"]')?.getBoundingClientRect();
            if (!rect) return;
            headerDragRef.current = { startX: e.clientX, startY: e.clientY, origX: dialogPos?.x ?? rect.left, origY: dialogPos?.y ?? rect.top };
            if (!dialogPos) setDialogPos({ x: rect.left, y: rect.top });
            if (!dialogSize) setDialogSize({ w: rect.width, h: rect.height });
          }}
          title={t('dialog.adjust_image.drag_to_move')}
        >
          <div className="w-10 h-1 rounded-full bg-bb-text-muted/30" />
        </div>

        <div className="flex flex-col gap-3 p-5 pt-0 overflow-auto flex-1">
          <h2 className="text-sm font-semibold text-bb-text">{t('dialog.adjust_image.title')}</h2>

          {/* Dual previews — zoomable and pannable */}
          <div className="flex gap-4" onMouseMove={handlePreviewMouseMove} onMouseUp={handlePreviewMouseUp}>
            {/* Original */}
            <div className="flex-1 flex flex-col items-center gap-1">
              <div
                className="border border-bb-border rounded overflow-hidden bg-black/20 flex items-center justify-center"
                style={{ width: '100%', height: 280 }}
                onWheel={handleWheel}
                onMouseDown={handlePreviewMouseDown}
              >
                {sourceBlobUrl ? (
                  <img src={sourceBlobUrl} alt={t('dialog.adjust_image.original')} className="max-w-full max-h-full object-contain pointer-events-none" style={previewImgStyle} />
                ) : (
                  <span className="text-xs text-bb-text-muted">{t('dialog.adjust_image.loading')}</span>
                )}
              </div>
              <span className="text-[10px] text-bb-text-dim">
                {sourceWidth > 0 ? `${sourceWidth} \u00D7 ${sourceHeight}` : ''}
              </span>
            </div>

            {/* Processed */}
            <div className="flex-1 flex flex-col items-center gap-1">
              <div
                className="border border-bb-border rounded overflow-hidden bg-black/20 flex items-center justify-center relative"
                style={{ width: '100%', height: 280 }}
                onWheel={handleWheel}
                onMouseDown={handlePreviewMouseDown}
              >
                {previewData ? (
                  <img
                    src={`data:image/png;base64,${previewData.png_base64}`}
                    alt={t('dialog.adjust_image.processed')}
                    className="max-w-full max-h-full object-contain pointer-events-none"
                    style={{ ...previewImgStyle, ...(invertDisplay ? { filter: 'invert(1)' } : {}) }}
                  />
                ) : previewLoading ? (
                  <span className="text-xs text-bb-text-muted animate-pulse">{t('dialog.adjust_image.processing')}</span>
                ) : (
                  <span className="text-xs text-bb-text-muted">{t('dialog.adjust_image.no_preview')}</span>
                )}
                {previewLoading && previewData && (
                  <div className="absolute inset-0 flex items-center justify-center bg-black/20">
                    <span className="text-xs text-bb-text-muted animate-pulse">{t('dialog.adjust_image.updating')}</span>
                  </div>
                )}
              </div>
              <div className="flex items-center gap-3">
                <span className="text-[10px] text-bb-text-dim">
                  {previewData ? `${previewData.width} \u00D7 ${previewData.height}` : ''}
                </span>
                <Toggle label={t('dialog.adjust_image.invert_display')} checked={invertDisplay} onChange={setInvertDisplay} />
              </div>
            </div>
          </div>

          {/* Middle bar: Reset All + Presets */}
          <div className="flex items-center gap-2 text-xs">
            <button onClick={handleResetAll} className="px-3 py-1 rounded border border-bb-border text-bb-text-muted hover:bg-bb-hover">{t('dialog.adjust_image.reset_all')}</button>
            <button data-testid="adjust-auto" onClick={() => void handleAutoAdjust()} className="px-3 py-1 rounded border border-bb-border text-bb-text-muted hover:bg-bb-hover">{t('dialog.adjust_image.auto')}</button>
            <div className="flex-1" />
            <span className="text-bb-text-muted">{t('dialog.adjust_image.presets')}</span>
            <select data-testid="adjust-preset-select" value={selectedPreset} onChange={(e) => {
              const p = presets.find((pr) => pr.name === e.target.value);
              if (p) {
                setBrightness(p.adjustments.brightness); setContrast(p.adjustments.contrast);
                setGamma(p.adjustments.gamma); setEnhanceRadius(p.adjustments.enhance_radius ?? 0);
                setInvertAdjust(p.adjustments.invert); setThreshold(p.adjustments.threshold);
                setSaturation(p.adjustments.saturation); setSharpen(p.adjustments.sharpen ?? 0);
                setEdgeEnhance(p.adjustments.edge_enhance ?? false);
                setEnhanceAmount(p.adjustments.enhance_amount ?? 0); setEnhanceDenoise(p.adjustments.enhance_denoise ?? 0);
                setSelectedPreset(e.target.value);
              }
            }} className="w-32 px-1 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text">
              <option value="">{t('dialog.adjust_image.select_preset')}</option>
              {presets.map((p) => <option key={p.name} value={p.name}>{p.name}</option>)}
            </select>
            {showSaveInput ? (
              <div className="flex items-center gap-1">
                <input type="text" value={savePresetName} onChange={(e) => setSavePresetName(e.target.value)}
                  onKeyDown={async (e) => {
                    if (e.key === 'Enter' && savePresetName.trim()) {
                      await handlePresetSave();
                    } else if (e.key === 'Escape') { setShowSaveInput(false); setSavePresetName(''); }
                  }}
                  placeholder={t('dialog.adjust_image.preset_name')} autoFocus className="w-24 px-1 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text focus:outline-none focus:border-bb-accent" />
                <button data-testid="adjust-save-preset-confirm" disabled={!savePresetName.trim()} onClick={() => void handlePresetSave()} className="px-1.5 py-0.5 rounded border border-bb-border text-bb-text-muted hover:bg-bb-hover text-xs disabled:opacity-40">{t('common.ok')}</button>
                <button onClick={() => { setShowSaveInput(false); setSavePresetName(''); }} className="px-1.5 py-0.5 rounded border border-bb-border text-bb-text-muted hover:bg-bb-hover text-xs">{t('common.cancel')}</button>
              </div>
            ) : (
              <button onClick={() => setShowSaveInput(true)} className="px-1.5 py-0.5 rounded border border-bb-border text-bb-text-muted hover:bg-bb-hover text-xs">{t('common.save')}</button>
            )}
            <button disabled={!selectedPreset} onClick={() => void handlePresetDelete()} className="px-1.5 py-0.5 rounded border border-bb-border text-bb-text-muted hover:bg-bb-hover text-xs disabled:opacity-40">{t('dialog.adjust_image.delete')}</button>
          </div>

          {/* Two-column settings */}
          <div className="flex gap-6">
            {/* Layer settings relevant to image output. */}
            <div className="w-[280px] shrink-0">
              <h3 className="text-[10px] font-semibold text-bb-accent uppercase tracking-wider mb-2">{t('dialog.adjust_image.layer_settings')}</h3>
              <div className="grid grid-cols-[1fr_auto] gap-x-3 gap-y-1.5 items-center text-xs">
                <span className="text-bb-text-muted">{t('dialog.adjust_image.image_mode')}</span>
                <select value={mode} onChange={(e) => setMode(e.target.value as RasterMode)}
                  className="w-[120px] px-1 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text focus:outline-none focus:border-bb-accent">
                  {rasterModeOptions.map((opt) => <option key={opt.value} value={opt.value}>{opt.label}</option>)}
                </select>

                <span className="text-bb-text-muted">{t('dialog.adjust_image.negative_image')}</span>
                <Toggle checked={negative} onChange={setNegative} />

                {(mode === 'halftone') && (
                  <>
                    <span className="text-bb-text-muted">{t('dialog.adjust_image.cells_per_inch')}</span>
                    <NumberStepper value={halftoneCpi} onChange={(e) => setHalftoneCpi(Number(e.target.value))} min={1} max={100} step={1} className={inputCls} />
                    <span className="text-bb-text-muted">{t('dialog.adjust_image.halftone_angle')}</span>
                    <NumberStepper value={halftoneAngle} onChange={(e) => setHalftoneAngle(Number(e.target.value))} min={0} max={360} step={0.5} className={inputCls} />
                  </>
                )}
                {(mode === 'newsprint') && (
                  <>
                    <span className="text-bb-text-muted">{t('dialog.adjust_image.newsprint_angle')}</span>
                    <NumberStepper value={newsprintAngle} onChange={(e) => setNewsprintAngle(Number(e.target.value))} min={0} max={360} step={1} className={inputCls} />
                    <span className="text-bb-text-muted">{t('dialog.adjust_image.frequency')}</span>
                    <NumberStepper value={newsprintFreq} onChange={(e) => setNewsprintFreq(Number(e.target.value))} min={1} max={100} step={1} className={inputCls} />
                  </>
                )}

                <span className="text-bb-text-muted">{labelWithUnit(t('dialog.adjust_image.line_interval'), lengthUnitLabel(displayUnit))}</span>
                <NumberStepper value={roundDisplayLength(mmToDisplay(lineInterval, displayUnit), displayUnit)} onChange={(e) => setLineInterval(displayToMm(Number(e.target.value), displayUnit))} min={mmToDisplay(0.01, displayUnit)} max={mmToDisplay(10, displayUnit)} step={lengthStep(displayUnit, 0.001, 0.001)} className={inputCls} />

                <span className="text-bb-text-muted">{t('dialog.adjust_image.dpi')}</span>
                <NumberStepper value={dpi} onChange={(e) => { const d = Math.max(1, Number(e.target.value)); setLineInterval(parseFloat((25.4 / d).toFixed(4))); }} min={1} max={2540} step={1} className={inputCls} />
              </div>
            </div>

            {/* Image adjustment sliders. */}
            <div className="flex-1">
              <h3 className="text-[10px] font-semibold text-bb-accent uppercase tracking-wider mb-2">{t('dialog.adjust_image.image_settings')}</h3>
              <div className="flex flex-col gap-2">
                <SliderInput label={t('dialog.adjust_image.contrast')} value={contrast} onChange={setContrast} min={-1} max={1} step={0.01} testId="adjust-contrast" />
                <SliderInput label={t('dialog.adjust_image.brightness')} value={brightness} onChange={setBrightness} min={-1} max={1} step={0.01} testId="adjust-brightness" />
                <SliderInput label={t('dialog.adjust_image.gamma')} value={gamma} onChange={setGamma} min={0.1} max={3.0} step={0.01} testId="adjust-gamma" />
                {(mode === 'threshold' || rs?.pass_through) && (
                  <SliderInput label={t('dialog.adjust_image.threshold')} value={threshold} onChange={setThreshold} min={0} max={255} step={1} testId="adjust-threshold" />
                )}
                {!rs?.pass_through && (
                  <>
                    <SliderInput label={t('dialog.adjust_image.saturation')} value={saturation} onChange={setSaturation} min={0} max={2} step={0.01} testId="adjust-saturation" />
                    <SliderInput label={t('dialog.adjust_image.sharpen')} value={sharpen} onChange={setSharpen} min={0} max={2} step={0.01} testId="adjust-sharpen" />
                    <div className="flex items-center justify-between gap-2 text-xs">
                      <span className="text-bb-text-muted">{t('dialog.adjust_image.invert')}</span>
                      <Toggle checked={invertAdjust} onChange={setInvertAdjust} />
                    </div>
                    <div className="flex items-center justify-between gap-2 text-xs">
                      <span className="text-bb-text-muted">{t('dialog.adjust_image.edge_enhance')}</span>
                      <Toggle checked={edgeEnhance} onChange={setEdgeEnhance} />
                    </div>
                  </>
                )}
                <SliderInput label={t('dialog.adjust_image.enhance_radius')} value={enhanceRadius} onChange={setEnhanceRadius} min={0} max={10} step={0.1} testId="adjust-enhance-radius" />
                <SliderInput label={t('dialog.adjust_image.enhance_amount')} value={enhanceAmount} onChange={setEnhanceAmount} min={0} max={3} step={0.1} testId="adjust-enhance-amount" />
                <SliderInput label={t('dialog.adjust_image.enhance_denoise')} value={enhanceDenoise} onChange={setEnhanceDenoise} min={0} max={3} step={0.1} testId="adjust-enhance-denoise" />
              </div>
            </div>
          </div>

          {/* Footer */}
          <div className="flex justify-end gap-2 pt-2 border-t border-bb-border">
            <button onClick={onClose} className="px-3 py-1 text-xs font-medium rounded bg-bb-bg hover:bg-bb-hover text-bb-text">{t('common.cancel')}</button>
            <button onClick={() => void handleOk()} className="px-4 py-1.5 text-xs font-medium rounded bg-bb-accent hover:bg-bb-accent-hover text-bb-on-accent">{t('common.ok')}</button>
          </div>
        </div>

        {/* Resize handle */}
        <div className="absolute bottom-0 right-0 w-5 h-5 cursor-nwse-resize group"
          onMouseDown={(e) => {
            e.stopPropagation();
            const rect = e.currentTarget.closest('[class*="bg-bb-panel"]')?.getBoundingClientRect();
            if (!rect) return;
            resizeDragRef.current = { startX: e.clientX, startY: e.clientY, origW: dialogSize?.w ?? rect.width, origH: dialogSize?.h ?? rect.height };
            if (!dialogPos) setDialogPos({ x: rect.left, y: rect.top });
            if (!dialogSize) setDialogSize({ w: rect.width, h: rect.height });
          }}>
          <svg viewBox="0 0 16 16" className="w-full h-full text-bb-text-muted/60 group-hover:text-bb-text-muted">
            <path d="M14 6L6 14M14 10L10 14M14 14L14 14" stroke="currentColor" strokeWidth="1.5" fill="none" strokeLinecap="round" />
          </svg>
        </div>
      </div>
    </div>,
    document.body,
  );
}
