/**
 * Live dither sample preview — renders a horizontal gradient strip
 * (white→black) processed through the selected raster mode.
 *
 * Calls the `render_dither_sample` Tauri command whenever the mode
 * changes (debounced 200ms). Results are cached by mode string so
 * switching back to a previously-seen mode is instant.
 */

import { useEffect, useRef, useState, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import type { RasterMode } from '../../types/project';
import { RASTER_MODE_HELP_KEYS } from './rasterModeHelp';

interface DitherSamplePreviewProps {
  mode: RasterMode;
}

export function DitherSamplePreview({ mode }: DitherSamplePreviewProps) {
  const { t } = useTranslation();
  const [blobUrl, setBlobUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const cacheRef = useRef<Map<string, string>>(new Map());
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const requestIdRef = useRef(0);

  useEffect(() => {
    requestIdRef.current += 1;
    const requestId = requestIdRef.current;

    // Check cache first
    const cached = cacheRef.current.get(mode);
    if (cached) {
      setError(null);
      setLoading(false);
      setBlobUrl(cached);
      return;
    }

    setBlobUrl(null);
    setError(null);
    setLoading(true);

    // Debounce — avoid hammering the backend while the user clicks
    // through modes quickly.
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(async () => {
      try {
        const pngBytes: number[] = await invoke('render_dither_sample', { mode });
        const arr = new Uint8Array(pngBytes);
        const blob = new Blob([arr], { type: 'image/png' });
        const url = URL.createObjectURL(blob);

        if (requestId !== requestIdRef.current) {
          URL.revokeObjectURL(url);
          return;
        }

        // Cache for instant switching later
        cacheRef.current.set(mode, url);
        setBlobUrl(url);
        setError(null);
      } catch (e) {
        if (requestId !== requestIdRef.current) {
          return;
        }
        console.warn('Dither sample failed:', e);
        setBlobUrl(null);
        setError(t('panels.dither_sample.preview_unavailable'));
      } finally {
        if (requestId === requestIdRef.current) {
          setLoading(false);
        }
      }
    }, 150);

    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [mode, t]);

  // Revoke all blob URLs on unmount
  useEffect(() => {
    const cache = cacheRef.current;
    return () => {
      for (const url of cache.values()) {
        URL.revokeObjectURL(url);
      }
      cache.clear();
    };
  }, []);

  const helpKey = useMemo(() => RASTER_MODE_HELP_KEYS[mode] ?? '', [mode]);
  const helpText = helpKey ? t(helpKey) : '';

  return (
    <div className="flex flex-col gap-1.5">
      <div className="text-xs font-medium text-bb-accent uppercase tracking-wider">
        {t('panels.dither_sample.title')}
      </div>
      <div
        className="w-full h-16 rounded border border-bb-border bg-white overflow-hidden flex items-center justify-center"
      >
        {loading && !blobUrl && (
          <span className="text-[10px] text-bb-text-muted">{t('common.loading')}</span>
        )}
        {!loading && error && !blobUrl && (
          <span className="text-[10px] text-bb-error-fg">{error}</span>
        )}
        {blobUrl && (
          <img
            src={blobUrl}
            alt={t('panels.dither_sample.alt', { mode })}
            className="w-full h-full object-contain"
            style={{ imageRendering: 'pixelated' }}
          />
        )}
      </div>
      <p className="text-[10px] text-bb-text-muted leading-tight min-h-[2.5rem]">
        {helpText}
      </p>
    </div>
  );
}
