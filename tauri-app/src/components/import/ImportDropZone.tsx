import { useState, useCallback, useEffect, type DragEvent, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { getCurrentWebviewWindow, type DragDropEvent } from '@tauri-apps/api/webviewWindow';
import { useProjectStore } from '../../stores/projectStore';
import { useNotificationStore } from '../../stores/notificationStore';
import i18n from '../../i18n';
import { useArtLibraryStore } from '../../stores/artLibraryStore';
import { isArtLibraryDragDataTransfer } from '../shared/artLibraryDragData';
import { blobToBase64 } from '../../utils/systemClipboard';

interface ImportDropZoneProps {
  children: ReactNode;
}

const SUPPORTED_EXTS = new Set([
  'svg', 'png', 'jpg', 'jpeg', 'bmp', 'gif', 'tif', 'tiff', 'webp', 'tga',
  'dxf', 'ai', 'pdf', 'eps', 'lbrn', 'lbrn2',
]);

function getSupportedPaths(paths: string[]): string[] {
  return paths.filter((path) => {
    const ext = path.split('.').pop()?.toLowerCase() ?? '';
    return SUPPORTED_EXTS.has(ext);
  });
}

export function ImportDropZone({ children }: ImportDropZoneProps) {
  const { t } = useTranslation();
  const project = useProjectStore((s) => s.project);
  const [isDragging, setIsDragging] = useState(false);

  const isOsFileDrag = useCallback((e: DragEvent) => (
    e.dataTransfer?.types?.includes('Files') ?? false
  ), []);

  const hasActiveArtLibraryDrag = useCallback((e: DragEvent) => (
    useArtLibraryStore.getState().dragState !== null || isArtLibraryDragDataTransfer(e.dataTransfer)
  ), []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;

    const handleWindowDragDrop = async (event: { payload: DragDropEvent }) => {
      if (disposed) return;

      switch (event.payload.type) {
        case 'enter': {
          if (!project) {
            setIsDragging(false);
            return;
          }
          setIsDragging(getSupportedPaths(event.payload.paths ?? []).length > 0);
          return;
        }
        case 'over':
          return;
        case 'leave':
          setIsDragging(false);
          return;
        case 'drop': {
          setIsDragging(false);
          if (!project) return;
          const filePaths = getSupportedPaths(event.payload.paths ?? []);
          if (filePaths.length === 0) return;
          try {
            await useProjectStore.getState().importFilePaths(filePaths);
          } catch (err) {
            useNotificationStore.getState().push(i18n.t('notifications.import_failed', { detail: String(err) }), 'error');
          }
        }
      }
    };

    void getCurrentWebviewWindow()
      .onDragDropEvent(handleWindowDragDrop)
      .then((fn) => {
        if (disposed) {
          fn();
          return;
        }
        unlisten = fn;
      })
      .catch(() => {
        // HTML5 drop fallback below remains available in non-Tauri contexts.
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [project]);

  const handleDragEnter = useCallback(
    (e: DragEvent) => {
      if (hasActiveArtLibraryDrag(e) || !isOsFileDrag(e)) return;
      e.preventDefault();
      e.stopPropagation();
      if (project) setIsDragging(true);
    },
    [hasActiveArtLibraryDrag, isOsFileDrag, project],
  );

  const handleDragOver = useCallback((e: DragEvent) => {
    if (hasActiveArtLibraryDrag(e) || !isOsFileDrag(e)) return;
    e.preventDefault();
    e.stopPropagation();
  }, [hasActiveArtLibraryDrag, isOsFileDrag]);

  const handleDragLeave = useCallback((e: DragEvent) => {
    if (hasActiveArtLibraryDrag(e) || !isOsFileDrag(e)) return;
    e.preventDefault();
    e.stopPropagation();
    // Only hide when leaving the drop zone itself (not child elements)
    if (e.currentTarget === e.target) {
      setIsDragging(false);
    }
  }, [hasActiveArtLibraryDrag, isOsFileDrag]);

  const handleDrop = useCallback(
    async (e: DragEvent) => {
      if (hasActiveArtLibraryDrag(e) || !isOsFileDrag(e)) return;
      e.preventDefault();
      e.stopPropagation();
      setIsDragging(false);

      if (!project) return;

      // The webview never exposes OS paths for dropped files (and with
      // native drag-drop disabled for in-app HTML5 drags, the Tauri
      // path-based event doesn't fire) — so ship the file CONTENTS to the
      // backend instead.
      const files = e.dataTransfer?.files;
      if (!files || files.length === 0) return;

      const supported = Array.from(files).filter((file) => {
        const ext = file.name.split('.').pop()?.toLowerCase() ?? '';
        return SUPPORTED_EXTS.has(ext);
      });
      if (supported.length === 0) return;

      try {
        const payload = await Promise.all(
          supported.map(async (file) => ({
            filename: file.name,
            dataBase64: await blobToBase64(file),
          })),
        );
        await useProjectStore.getState().importFileData(payload);
      } catch (err) {
        useNotificationStore.getState().push(i18n.t('notifications.import_failed', { detail: String(err) }), 'error');
      }
    },
    [hasActiveArtLibraryDrag, isOsFileDrag, project],
  );

  return (
    <div
      className="relative w-full h-full"
      onDragEnter={handleDragEnter}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      {children}
      {isDragging && (
        <div className="absolute inset-0 bg-bb-accent/10 border-2 border-dashed border-bb-accent rounded flex items-center justify-center z-40 pointer-events-none">
          <span className="text-bb-accent text-lg font-semibold bg-bb-panel/90 px-4 py-2 rounded">
            {t('import.drop_to_import')}
          </span>
        </div>
      )}
    </div>
  );
}
