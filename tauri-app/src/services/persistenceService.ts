import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';
import i18n from '../i18n';
import { appService } from './appService';
import { exportCanvasScreenshot, type CanvasScreenshotFormat } from './canvasScreenshotExportService';
import { isMacPlatform } from '../utils/platform';
import type { ArtworkExportFormat, ExportSettings } from '../types/commands';
import type { Project } from '../types/project';

const ARTWORK_EXPORTERS: Record<ArtworkExportFormat, {
  filterName: string;
  extension: ArtworkExportFormat;
}> = {
  svg: { filterName: 'SVG Files (*.svg)', extension: 'svg' },
  dxf: { filterName: 'AutoCAD DXF Files (*.dxf)', extension: 'dxf' },
  pdf: { filterName: 'PDF Files (*.pdf)', extension: 'pdf' },
  eps: { filterName: 'EPS Files (*.eps)', extension: 'eps' },
  ai: { filterName: 'Illustrator Files (*.ai)', extension: 'ai' },
  png: { filterName: 'PNG file (*.png)', extension: 'png' },
  jpg: { filterName: 'JPG file (*.jpg)', extension: 'jpg' },
  bmp: { filterName: 'BMP file (*.bmp)', extension: 'bmp' },
};

const BACKEND_ARTWORK_EXPORT_COMMANDS: Record<Exclude<ArtworkExportFormat, CanvasScreenshotFormat>, string> = {
  svg: 'export_svg',
  dxf: 'export_dxf',
  pdf: 'export_pdf',
  eps: 'export_eps',
  ai: 'export_ai',
};

const ARTWORK_EXPORT_PICKER_ORDER: ArtworkExportFormat[] = ['ai', 'dxf', 'svg', 'png', 'jpg', 'bmp'];

const DEFAULT_EXPORT_SETTINGS: ExportSettings = {
  last_directory: null,
  last_format: 'svg',
  filename_stem: null,
};

export interface ExportArtworkOptions {
  selectionOnly?: boolean;
  selectedIds?: string[];
  defaultName?: string | null;
}

function normalizeExportSettings(settings?: Partial<ExportSettings> | null): ExportSettings {
  const lastFormat = settings?.last_format && isUnifiedArtworkExportFormat(settings.last_format)
    ? settings.last_format
    : DEFAULT_EXPORT_SETTINGS.last_format;
  return {
    last_directory: settings?.last_directory ?? DEFAULT_EXPORT_SETTINGS.last_directory,
    last_format: lastFormat,
    filename_stem: settings?.filename_stem ?? DEFAULT_EXPORT_SETTINGS.filename_stem,
  };
}

function isUnifiedArtworkExportFormat(format: string): format is ArtworkExportFormat {
  return (ARTWORK_EXPORT_PICKER_ORDER as string[]).includes(format);
}

function isCanvasScreenshotFormat(format: ArtworkExportFormat): format is CanvasScreenshotFormat {
  return format === 'png' || format === 'jpg' || format === 'bmp';
}

function sanitizeFilenameStem(stem: string | null | undefined): string {
  const fallback = 'output';
  let trimmed = (stem ?? '').trim();
  if (!trimmed) return fallback;
  while (true) {
    const extension = getExtension(trimmed);
    if (!extension || !ARTWORK_EXPORTERS[extension as ArtworkExportFormat]) break;
    trimmed = getStem(trimmed);
  }
  return trimmed.replace(/[\\/:*?"<>|]+/g, '-');
}

function getDirectory(path: string): string | null {
  const index = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  if (index < 0) return null;
  return path.slice(0, index) || null;
}

function getBasename(path: string): string {
  const index = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return index < 0 ? path : path.slice(index + 1);
}

function getStem(path: string): string {
  const basename = getBasename(path);
  const index = basename.lastIndexOf('.');
  return index <= 0 ? basename : basename.slice(0, index);
}

function getExtension(path: string): string | null {
  const basename = getBasename(path);
  const index = basename.lastIndexOf('.');
  if (index < 0 || index === basename.length - 1) return null;
  return basename.slice(index + 1).toLowerCase();
}

function appendExtension(path: string, format: ArtworkExportFormat): string {
  return `${path}.${ARTWORK_EXPORTERS[format].extension}`;
}

function buildDefaultExportPath(settings: ExportSettings, defaultName?: string | null): string {
  const stem = sanitizeFilenameStem(settings.filename_stem ?? defaultName ?? 'output');
  const filename = `${stem}.${ARTWORK_EXPORTERS[settings.last_format].extension}`;
  return settings.last_directory ? `${settings.last_directory}/${filename}` : filename;
}

function artworkFilters(lastFormat: ArtworkExportFormat) {
  const orderedFormats = [
    lastFormat,
    ...ARTWORK_EXPORT_PICKER_ORDER.filter((format) => format !== lastFormat),
  ];
  return orderedFormats.map((format) => ({
    name: ARTWORK_EXPORTERS[format].filterName,
    extensions: [ARTWORK_EXPORTERS[format].extension],
  }));
}

function artworkPickerFormats() {
  return ARTWORK_EXPORT_PICKER_ORDER.map((format) => ({
    label: ARTWORK_EXPORTERS[format].filterName,
    extension: ARTWORK_EXPORTERS[format].extension,
  }));
}

function exportDialogTitle(selectionOnly: boolean): string {
  return selectionOnly ? 'Export selected vectors to file' : 'Export vectors to file';
}

function isTauriRuntime(): boolean {
  return typeof window !== 'undefined'
    && (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ !== undefined;
}

async function pickArtworkExportPath(
  title: string,
  defaultPath: string,
  lastFormat: ArtworkExportFormat,
): Promise<string | null> {
  if (isMacPlatform() && isTauriRuntime()) {
    return invoke<string | null>('pick_artwork_export_path', {
      title,
      defaultPath,
      formats: artworkPickerFormats(),
      selectedExtension: lastFormat,
    });
  }

  return save({
    title,
    defaultPath,
    filters: artworkFilters(lastFormat),
  });
}

function inferArtworkFormat(path: string, fallback: ArtworkExportFormat): { path: string; format: ArtworkExportFormat } {
  const extension = getExtension(path);
  if (!extension) {
    return { path: appendExtension(path, fallback), format: fallback };
  }
  if (!ARTWORK_EXPORTERS[extension as ArtworkExportFormat]) {
    throw new Error(`Unsupported export file type ".${extension}"`);
  }
  return { path, format: extension as ArtworkExportFormat };
}

async function pickProjectSavePath(): Promise<string> {
  const selected = await save({
    title: i18n.t('file_dialogs.save_project_title'),
    defaultPath: 'project.lzrproj',
    filters: [{
      name: i18n.t('file_dialogs.filter_project'),
      extensions: ['lzrproj'],
    }],
  });

  if (selected === null) {
    throw new Error('Save cancelled');
  }

  return selected;
}

export const persistenceService = {
  async saveProject(path?: string): Promise<string> {
    if (!path) {
      const selected = await pickProjectSavePath();
      return invoke<string>('save_project_cmd', { path: selected });
    }
    return invoke<string>('save_project_cmd', { path: path ?? null });
  },

  async saveProjectAs(): Promise<string> {
    const selected = await pickProjectSavePath();
    return invoke<string>('save_project_cmd', { path: selected });
  },

  async openProject(): Promise<{ project: Project; path: string }> {
    const selected = await open({
      title: i18n.t('file_dialogs.open_project_title'),
      multiple: false,
      directory: false,
      filters: [{
        name: i18n.t('file_dialogs.filter_project'),
        extensions: ['lzrproj'],
      }],
    });

    if (selected === null) {
      throw new Error('Open cancelled');
    }

    if (Array.isArray(selected)) {
      throw new Error('Expected a single project path');
    }

    const project = await invoke<Project>('open_project_from_path', { filePath: selected });
    return { project, path: selected };
  },

  async openProjectFromPath(filePath: string): Promise<Project> {
    return invoke<Project>('open_project_from_path', { filePath });
  },

  async getAssetData(assetId: string): Promise<number[]> {
    return invoke<number[]>('get_asset_data', { assetId });
  },

  async autosave(): Promise<string> {
    return invoke<string>('autosave_project');
  },

  async checkRecovery(): Promise<RecoveryInfo[]> {
    return invoke<RecoveryInfo[]>('check_recovery_files');
  },

  async restoreRecovery(recoveryPath: string): Promise<Project> {
    return invoke<Project>('restore_recovery', { recoveryPath });
  },

  async discardRecovery(recoveryPath: string): Promise<void> {
    return invoke<void>('discard_recovery_file', { recoveryPath });
  },

  async exportSvg(selectionOnly = false, selectedIds: string[] = []): Promise<string> {
    const selected = await save({
      title: i18n.t('file_dialogs.export_svg_title'),
      defaultPath: 'output.svg',
      filters: [{ name: i18n.t('file_dialogs.filter_svg'), extensions: ['svg'] }],
    });
    if (selected === null) { throw new Error('Export cancelled'); }
    return invoke<string>('export_svg', { path: selected, selectionOnly, selectedIds });
  },

  async exportDxf(selectionOnly = false, selectedIds: string[] = []): Promise<string> {
    const selected = await save({
      title: i18n.t('file_dialogs.export_dxf_title'),
      defaultPath: 'output.dxf',
      filters: [{ name: i18n.t('file_dialogs.filter_dxf'), extensions: ['dxf'] }],
    });
    if (selected === null) { throw new Error('Export cancelled'); }
    return invoke<string>('export_dxf', { path: selected, selectionOnly, selectedIds });
  },

  async exportPdf(selectionOnly = false, selectedIds: string[] = []): Promise<string> {
    const selected = await save({
      title: i18n.t('file_dialogs.export_pdf_title'),
      defaultPath: 'output.pdf',
      filters: [{ name: i18n.t('file_dialogs.filter_pdf'), extensions: ['pdf'] }],
    });
    if (selected === null) { throw new Error('Export cancelled'); }
    return invoke<string>('export_pdf', { path: selected, selectionOnly, selectedIds });
  },

  async exportEps(selectionOnly = false, selectedIds: string[] = []): Promise<string> {
    const selected = await save({
      title: i18n.t('file_dialogs.export_eps_title'),
      defaultPath: 'output.eps',
      filters: [{ name: i18n.t('file_dialogs.filter_eps'), extensions: ['eps'] }],
    });
    if (selected === null) { throw new Error('Export cancelled'); }
    return invoke<string>('export_eps', { path: selected, selectionOnly, selectedIds });
  },

  async exportAi(selectionOnly = false, selectedIds: string[] = []): Promise<string> {
    const selected = await save({
      title: i18n.t('file_dialogs.export_ai_title'),
      defaultPath: 'output.ai',
      filters: [{ name: i18n.t('file_dialogs.filter_adobe_illustrator'), extensions: ['ai'] }],
    });
    if (selected === null) { throw new Error('Export cancelled'); }
    return invoke<string>('export_ai', { path: selected, selectionOnly, selectedIds });
  },

  async exportArtwork(options: ExportArtworkOptions = {}): Promise<string> {
    let settings = DEFAULT_EXPORT_SETTINGS;
    try {
      settings = normalizeExportSettings((await appService.getSettings()).export_settings);
    } catch {
      settings = DEFAULT_EXPORT_SETTINGS;
    }

    const selectionOnly = options.selectionOnly ?? false;
    const selectedIds = options.selectedIds ?? [];
    const selected = await pickArtworkExportPath(
      exportDialogTitle(selectionOnly),
      buildDefaultExportPath(settings, options.defaultName),
      settings.last_format,
    );

    if (selected === null) { throw new Error('Export cancelled'); }

    const { path, format } = inferArtworkFormat(selected, settings.last_format);
    const result = isCanvasScreenshotFormat(format)
      ? await exportCanvasScreenshot(path, format)
      : await invoke<string>(BACKEND_ARTWORK_EXPORT_COMMANDS[format], {
        path,
        selectionOnly,
        selectedIds,
      });

    await appService.updateSettings({
      export_settings: {
        last_directory: getDirectory(path),
        last_format: format,
        filename_stem: sanitizeFilenameStem(getStem(path)),
      },
    });

    return result;
  },

  async saveProcessedBitmap(objectId: string): Promise<string> {
    const selected = await save({
      title: i18n.t('file_dialogs.save_processed_bitmap_title'),
      defaultPath: 'processed.png',
      filters: [{ name: i18n.t('file_dialogs.filter_png_image'), extensions: ['png'] }],
    });
    if (selected === null) { throw new Error('Export cancelled'); }
    return invoke<string>('save_processed_bitmap', { objectId, path: selected });
  },

  async getRecentFiles(): Promise<{ path: string; name: string; opened_at: string }[]> {
    return invoke<{ path: string; name: string; opened_at: string }[]>('get_recent_files');
  },

  async clearRecentFiles(): Promise<void> {
    return invoke<void>('clear_recent_files');
  },
};

export interface RecoveryInfo {
  path: string;
  project_name: string;
  saved_at: string;
}
