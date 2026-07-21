export interface MergeFieldInfo {
  start: number;
  end: number;
  field_name: string;
}

export type VariableTextMode =
  | 'normal'
  | 'serial_number'
  | 'date_time'
  | 'merge_csv'
  | 'cut_setting';

export interface VariableTextSource {
  csvPath: string | null;
  csvData: string[][];
  fieldDefaults: Record<string, string>;
  current?: number;
  currentRow?: number;
  start?: number;
  end?: number;
  advanceBy?: number;
  autoAdvance?: boolean;
  totalCopies: number;
}

export interface VariableTextConfig {
  template: string;
  mode?: VariableTextMode | null;
  offset?: number | null;
  source: VariableTextSource;
}

export interface CsvData {
  headers: string[];
  rows: string[][];
}
