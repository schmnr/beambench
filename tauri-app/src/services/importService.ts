import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import type { GcodeLine, ProjectObject, RasterAdjustments, RasterMode } from '../types/project';
import { measureAsyncPerf } from './perfMarks';
import i18n from '../i18n';

export interface TraceBoundaryPx {
  x: number;
  y: number;
  width: number;
  height: number;
}

export const importService = {
  async importSvgFile(filePath: string, layerId: string): Promise<ProjectObject[]> {
    return invoke<ProjectObject[]>('import_svg_file', { filePath, layerId });
  },

  async importImageFile(filePath: string, layerId: string): Promise<ProjectObject> {
    return invoke<ProjectObject>('import_image_file', { filePath, layerId });
  },

  async importClipboardArtwork(params: {
    dataBase64: string;
    filename: string;
    mediaType: string;
    layerId: string;
    dropX?: number;
    dropY?: number;
  }): Promise<ProjectObject[]> {
    return invoke<ProjectObject[]>('import_clipboard_artwork', params);
  },

  async pickFiles(): Promise<string[]> {
    const selected = await open({
      title: i18n.t('file_dialogs.import_files_title'),
      multiple: true,
      directory: false,
      filters: [
        { name: i18n.t('file_dialogs.filter_supported_files'), extensions: ['svg', 'png', 'jpg', 'jpeg', 'bmp', 'gif', 'tif', 'tiff', 'webp', 'tga', 'dxf', 'ai', 'pdf', 'eps', 'lbrn', 'lbrn2'] },
        { name: 'Lbrn Projects', extensions: ['lbrn', 'lbrn2'] },
        { name: i18n.t('file_dialogs.filter_svg_files'), extensions: ['svg'] },
        { name: i18n.t('file_dialogs.filter_image_files'), extensions: ['png', 'jpg', 'jpeg', 'bmp', 'gif', 'tif', 'tiff', 'webp', 'tga'] },
        { name: i18n.t('file_dialogs.filter_dxf_files'), extensions: ['dxf'] },
        { name: i18n.t('file_dialogs.filter_ai_files'), extensions: ['ai'] },
        { name: i18n.t('file_dialogs.filter_pdf_files'), extensions: ['pdf'] },
        { name: i18n.t('file_dialogs.filter_eps_files'), extensions: ['eps'] },
      ],
    });
    if (selected === null) return [];
    return Array.isArray(selected) ? selected : [selected];
  },

  async importFilePaths(filePaths: string[], layerId: string): Promise<ProjectObject[]> {
    return invoke<ProjectObject[]>('import_files', { filePaths, layerId });
  },

  /** Import files by content (HTML5 drag-drop: the webview has no OS paths). */
  async importFileData(
    files: { filename: string; dataBase64: string }[],
    layerId: string,
  ): Promise<ProjectObject[]> {
    return invoke<ProjectObject[]>('import_file_data', { files, layerId });
  },

  async pickAndImportFiles(layerId: string): Promise<ProjectObject[]> {
    const filePaths = await this.pickFiles();
    if (filePaths.length === 0) return [];
    return this.importFilePaths(filePaths, layerId);
  },

  async importDxfFile(filePath: string, layerId: string): Promise<ProjectObject[]> {
    return invoke<ProjectObject[]>('import_dxf_file', { filePath, layerId });
  },

  async importAiFile(filePath: string, layerId: string): Promise<ProjectObject[]> {
    return invoke<ProjectObject[]>('import_ai_file', { filePath, layerId });
  },

  async importPdfFile(filePath: string, layerId: string): Promise<ProjectObject[]> {
    return invoke<ProjectObject[]>('import_pdf_file', { filePath, layerId });
  },

  async importEpsFile(filePath: string, layerId: string): Promise<ProjectObject[]> {
    return invoke<ProjectObject[]>('import_eps_file', { filePath, layerId });
  },

  async importGcodeFile(filePath: string): Promise<GcodeLine[]> {
    return invoke<GcodeLine[]>('import_gcode_file', { filePath });
  },

  async traceImagePreview(
    objectId: string, threshold: number, cutoff: number, turdsize: number,
    alphamax: number, opttolerance: number, traceAlpha: boolean, sketchTrace: boolean, requestId: number,
    boundary: TraceBoundaryPx | null = null,
  ): Promise<{ paths: string[]; source_width: number; source_height: number }> {
    return measureAsyncPerf(
      'trace_image_preview',
      () => invoke<{ paths: string[]; source_width: number; source_height: number }>(
        'trace_image_preview', { objectId, threshold, cutoff, turdsize, alphamax, opttolerance, traceAlpha, sketchTrace, requestId, boundary },
      ),
    );
  },

  async traceImage(
    objectId: string, threshold = 128, cutoff = 0, turdsize = 2,
    alphamax = 1.0, opttolerance = 0.2, traceAlpha = false, sketchTrace = false, deleteSource = false,
    boundary: TraceBoundaryPx | null = null,
  ): Promise<ProjectObject[]> {
    return measureAsyncPerf(
      'trace_image',
      () => invoke<ProjectObject[]>(
        'trace_image', { objectId, threshold, cutoff, turdsize, alphamax, opttolerance, traceAlpha, sketchTrace, deleteSource, boundary },
      ),
    );
  },

  async refreshImage(objectId: string): Promise<ProjectObject> {
    return invoke<ProjectObject>('refresh_image', { objectId });
  },

  async adjustImagePreview(params: {
    objectId: string;
    brightness: number; contrast: number; gamma: number;
    invert: boolean; threshold: number; saturation: number;
    sharpen: number; edgeEnhance: boolean;
    enhanceRadius: number; enhanceAmount: number; enhanceDenoise: number;
    mode: RasterMode; dpi: number; negative: boolean; passThrough: boolean;
    halftoneCellsPerInch: number; halftoneAngleDeg: number;
    newsprintAngleDeg: number; newsprintFrequency: number;
  }): Promise<{ png_base64: string; width: number; height: number }> {
    return invoke<{ png_base64: string; width: number; height: number }>('adjust_image_preview', params);
  },

  async autoAdjustImage(objectId: string): Promise<{ brightness: number; contrast: number; gamma: number; sharpen: number }> {
    return invoke<{ brightness: number; contrast: number; gamma: number; sharpen: number }>('auto_adjust_image', { objectId });
  },

  async replaceImage(objectId: string, filePath?: string): Promise<ProjectObject | null> {
    let nextPath = filePath;
    if (!nextPath) {
      const selected = await open({
        title: i18n.t('file_dialogs.replace_image_title'),
        multiple: false,
        directory: false,
        filters: [
          { name: i18n.t('file_dialogs.filter_image_files'), extensions: ['png', 'jpg', 'jpeg', 'bmp', 'gif', 'tif', 'tiff', 'webp', 'tga'] },
        ],
      });
      if (selected === null) {
        return null;
      }
      if (Array.isArray(selected)) {
        throw new Error('Expected a single image path');
      }
      nextPath = selected;
    }
    return invoke<ProjectObject>('replace_image', { objectId, filePath: nextPath });
  },

  async replaceImageToFit(objectId: string, filePath?: string): Promise<ProjectObject | null> {
    let nextPath = filePath;
    if (!nextPath) {
      const selected = await open({
        title: i18n.t('file_dialogs.replace_image_to_fit_title'),
        multiple: false,
        directory: false,
        filters: [
          { name: i18n.t('file_dialogs.filter_image_files'), extensions: ['png', 'jpg', 'jpeg', 'bmp', 'gif', 'tif', 'tiff', 'webp', 'tga'] },
        ],
      });
      if (selected === null) {
        return null;
      }
      if (Array.isArray(selected)) {
        throw new Error('Expected a single image path');
      }
      nextPath = selected;
    }
    return invoke<ProjectObject>('replace_image_to_fit', { objectId, filePath: nextPath });
  },

  async getImagePresets(): Promise<Array<{ name: string; adjustments: RasterAdjustments }>> {
    return invoke('get_image_presets');
  },

  async saveImagePreset(name: string, adjustments: RasterAdjustments): Promise<void> {
    return invoke('save_image_preset', { name, adjustments });
  },

  async deleteImagePreset(name: string): Promise<void> {
    return invoke('delete_image_preset', { name });
  },
};
