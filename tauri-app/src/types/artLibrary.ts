import type { Layer, ProjectObject, TextAlignment, TextAlignmentV } from './project';

export type ArtLibraryItemKind = 'external_file' | 'selection_snapshot';

export interface ArtLibraryItem {
  id: string;
  kind: ArtLibraryItemKind;
  name: string;
  category: string;
  tags: string[];
  source_filename: string;
  media_type: string;
  data: string;
  thumbnail?: string;
  created_at: string;
}

export interface ArtLibraryAddItemResult {
  item: ArtLibraryItem;
  duplicate: boolean;
}

export interface ArtLibraryTextSourceMetadata {
  object_id: string;
  content: string;
  font_family: string;
  font_size_mm: number;
  bold: boolean;
  italic: boolean;
  alignment: TextAlignment;
  alignment_v: TextAlignmentV;
  upper_case: boolean;
}

export interface ArtLibrarySnapshotAsset {
  hash: string;
  media_type: string;
  data: string;
}

export interface ArtLibrarySelectionSnapshot {
  format_version: string;
  objects: ProjectObject[];
  layer_templates: Layer[];
  assets: ArtLibrarySnapshotAsset[];
  source_text_metadata: ArtLibraryTextSourceMetadata[];
}

export interface LoadedArtLibrary {
  format_version: string;
  library_id: string;
  name: string;
  items: ArtLibraryItem[];
  path: string;
  save_error?: string;
}

export interface ArtLibraryLoadState {
  libraries: LoadedArtLibrary[];
  warnings: string[];
}
