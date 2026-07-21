import { invoke } from '@tauri-apps/api/core';
import type {
  MergeFieldInfo,
  VariableTextConfig,
  CsvData,
} from '../types/variableText';
import type { ProjectObject } from '../types/project';

export interface BatchResult {
  copies: ProjectObject[];
  updatedOriginal: ProjectObject;
}

export const variableTextService = {
  async parseMergeFields(text: string): Promise<MergeFieldInfo[]> {
    return invoke<MergeFieldInfo[]>('parse_merge_fields', { text });
  },

  async loadCsvFile(path: string): Promise<CsvData> {
    return invoke<CsvData>('load_csv_file', { path });
  },

  async resolveVariableText(
    objectId: string,
    config: VariableTextConfig,
    row: number,
  ): Promise<string> {
    return invoke<string>('resolve_variable_text', { objectId, config, row });
  },

  async generateBatchPreview(
    objectId: string,
    config: VariableTextConfig,
  ): Promise<string[]> {
    return invoke<string[]>('generate_batch_preview', { objectId, config });
  },

  async generateVariableTextBatch(
    config: VariableTextConfig,
    objectId: string,
    offsetStep: number,
  ): Promise<BatchResult> {
    return invoke<BatchResult>('generate_variable_text_batch', { config, objectId, offsetStep });
  },
};
