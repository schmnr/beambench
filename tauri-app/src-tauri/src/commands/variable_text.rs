use beambench_core::variable_text::{MergeFieldInfo, VariableTextConfig};
use beambench_service::context::ServiceContext;
use beambench_service::ops::project::BatchResult;
pub use beambench_service::ops::workflows::variable_text::*;
use std::sync::Arc;
use tauri::State;
/// Parse merge fields from text content.
#[tauri::command]
pub fn parse_merge_fields(text: String) -> Vec<MergeFieldInfo> {
    beambench_service::ops::workflows::variable_text::parse_merge_fields(text)
}
/// Load CSV file and return parsed rows + headers.
#[tauri::command]
pub fn load_csv_file(path: String) -> Result<CsvData, String> {
    beambench_service::ops::workflows::variable_text::load_csv_file(path)
}
/// Resolve a text template for a specific row.
/// `row` is the preview row index — sets `source.current` for CSV
/// resolution and is also passed as the copy index for `{Serial:params}`.
#[tauri::command]
pub fn resolve_variable_text(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    config: VariableTextConfig,
    row: usize,
) -> Result<String, String> {
    beambench_service::ops::workflows::variable_text::resolve_variable_text(
        &svc, object_id, config, row,
    )
}
/// Generate batch: resolve text for all rows/copies.
/// Uses `total_copies` as the output count. When CSV data is present,
/// row indices cycle through available data rows via modulo.
#[tauri::command]
pub fn generate_batch_preview(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    config: VariableTextConfig,
) -> Result<Vec<String>, String> {
    beambench_service::ops::workflows::variable_text::generate_batch_preview(
        &svc, object_id, config,
    )
}
/// Atomic batch generation: resolve text, create offset duplicates — all
/// in a single undo step. Each copy gets its own `variable_text` config with
/// the correct serial/row state. The original's source state is advanced.
#[tauri::command]
pub fn generate_variable_text_batch(
    svc: State<'_, Arc<ServiceContext>>,
    config: VariableTextConfig,
    object_id: String,
    offset_step: f64,
) -> Result<BatchResult, String> {
    beambench_service::ops::workflows::variable_text::generate_variable_text_batch(
        &svc,
        config,
        object_id,
        offset_step,
    )
}
