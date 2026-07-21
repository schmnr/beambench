//! Core application logic for Beam Bench.
//! Owns project model, app state, and business rules.

pub mod alignment;
pub mod app_state;
pub mod array_ops;
pub mod art_library;
pub mod asset;
pub mod barcode_gen;
pub mod diagnostics;
pub mod export_bitmap;
pub mod export_dxf;
pub mod export_eps;
pub mod export_pdf;
pub mod export_print;
pub mod export_svg;
pub mod import;
pub mod import_dxf;
pub mod import_gcode;
pub mod import_lbrn;
pub mod import_pdf;
pub mod layer;
pub mod machine_profile;
pub mod macros;
pub mod material;
pub mod object;
pub mod operations;
pub mod optimization;
pub mod potrace;
pub mod project;
pub mod quality_test;
pub mod settings;
pub mod text_path;
pub mod trace;
pub mod variable_text;
pub mod vector;
pub mod workspace;

pub use app_state::{AppState, AppStatus, BuildInfo};
pub use array_ops::{
    CircularArrayConfig, GridArrayConfig, GridArraySizingMode, SpacingMode, circular_array,
    copy_along_path, fit_grid_array_counts, grid_array, grid_array_in_project,
    grid_array_layout_bounds, rubber_band_outline,
};
pub use art_library::{
    ART_LIBRARY_FORMAT_VERSION, ART_LIBRARY_SNAPSHOT_MEDIA_TYPE, ArtLibraryDocument,
    ArtLibraryItem, ArtLibraryItemKind, ArtLibrarySelectionSnapshot, ArtLibrarySnapshotAsset,
    ArtLibraryTextSourceMetadata,
};
pub use asset::{Asset, AssetId, AssetMediaType};
pub use barcode_gen::generate_barcode;
pub use export_bitmap::save_processed_bitmap;
pub use export_dxf::export_dxf;
pub use export_eps::{export_ai, export_eps};
pub use export_pdf::export_pdf;
pub use export_print::{
    PrintDocument, PrintMode, render_print_document, render_print_document_with_selection,
    render_print_png,
};
pub use export_svg::export_svg;
pub use import::{ImportError, import_image, import_svg};
pub use import_dxf::{DxfEntity, parse_dxf};
pub use import_gcode::{GcodeLine, import_gcode_as_vecpaths, parse_gcode};
pub use import_lbrn::{LbrnCutEntry, LbrnDocument, LbrnLayer, LbrnShape, parse_lbrn_project};
pub use import_pdf::{
    PdfPaintMode, PdfPaintedPath, PdfRgbColor, parse_eps_paths, parse_pdf_painted_paths,
    parse_pdf_paths,
};
pub use layer::{
    CutEntry, CutEntryId, CutEntryPatch, CutEntryTemplate, Layer, LayerBatchToggle, LayerId,
    LayerPatch, OperationType, RasterSettings, VectorSettings,
};
pub use machine_profile::{
    MachineProfile, MachineProfileId, MachineProfileSnapshot, RuidaTableAxis, ScanningOffsetEntry,
    TransferMode,
};
pub use macros::MacroDefinition;
pub use material::{CutSettings, MaterialPreset};
pub use object::{
    ImageMaskPolarity, ImageMaskRef, LayerRef, ObjectData, ObjectId, ProjectObject, ShapeKind,
    TextAlignment, TextAlignmentV, TextCirclePlacement, TextFontSource, TextLayoutMode,
    TextTransformStyle,
};
pub use operations::{
    flip_objects, lock_objects, move_objects_to_position, push_draw_order, reassign_layer,
    rotate_objects, select_all_in_layer, select_open_shapes, set_layer_air_assist,
    set_layer_visible, set_objects_visible, unlock_objects,
};
pub use optimization::{
    DirectionOrder, FinishPosition, OptimizationOrderKey, ProjectOptimization,
    ProjectOptimizationPatch,
};
pub use potrace::types::TurnPolicy;
pub use project::{Project, ProjectMetadata};
pub use quality_test::{
    FocusTestSettings, FocusTestZMode, IntervalTestSettings, MaterialTestAxis,
    MaterialTestAxisParam, MaterialTestRecipe, MaterialTestSettings, QualityTestError,
    QualityTestOrdering, QualityTestRequest, QualityTestSettings, QualityTestWarning,
};
pub use settings::{
    AppSettings, CursorSize, DisplayUnit, IconSize, ImagePreset, SavedPosition, SpeedTimeUnit,
    UiTheme,
};
pub use text_path::{apply_path_to_text, apply_path_to_text_with_options};
pub use trace::{TraceConfig, trace_image, trace_image_preview_fast};
pub use variable_text::{
    MergeField, MergeFieldInfo, VariableTextConfig, VariableTextMode, VariableTextSource,
    advance_sequence_value, parse_csv, parse_merge_fields, resolve_text, resolve_text_in_project,
    wrap_sequence_value,
};
pub use workspace::{Workspace, WorkspaceOrigin};
