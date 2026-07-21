use std::sync::Arc;

use beambench_core::ObjectData;
use beambench_core::variable_text::{self, MergeFieldInfo, VariableTextConfig};
use beambench_service::context::ServiceContext;
use beambench_service::ops::project::{BatchCopyInput, BatchResult};
use serde::Serialize;
use tauri::State;

use super::project::parse_id;

/// Wrapper for CSV data returned to the frontend.
#[derive(Debug, Serialize)]
pub struct CsvData {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Parse merge fields from text content.
#[tauri::command]
pub fn parse_merge_fields(text: String) -> Vec<MergeFieldInfo> {
    variable_text::parse_merge_fields(&text)
}

/// Load CSV file and return parsed rows + headers.
#[tauri::command]
pub fn load_csv_file(path: String) -> Result<CsvData, String> {
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("Failed to read CSV file: {e}"))?;
    let rows = variable_text::parse_csv(&content)?;
    if rows.is_empty() {
        return Err("CSV file is empty".to_string());
    }
    let headers = rows[0].clone();
    let data_rows = rows[1..].to_vec();
    Ok(CsvData {
        headers,
        rows: data_rows,
    })
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
    let oid = parse_id(&object_id)?;
    let project_guard = svc.project.lock().map_err(|e| e.to_string())?;
    let project = project_guard
        .as_ref()
        .ok_or_else(|| "No project open".to_string())?;
    let object = project
        .find_object(oid)
        .ok_or_else(|| "Object not found".to_string())?;
    Ok(variable_text::resolve_text_in_project(
        project, object, &config, row,
    ))
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
    let oid = parse_id(&object_id)?;
    let project_guard = svc.project.lock().map_err(|e| e.to_string())?;
    let project = project_guard
        .as_ref()
        .ok_or_else(|| "No project open".to_string())?;
    let object = project
        .find_object(oid)
        .ok_or_else(|| "Object not found".to_string())?;
    let count = (config.source.total_copies as usize).max(1);
    let base_current = config.source.current;
    let mut results = Vec::with_capacity(count);
    for i in 0..count {
        let mut iter_source = config.source.clone();
        iter_source.current = beambench_core::wrap_sequence_value(
            base_current + i as i64 * config.source.advance_by,
            iter_source.start,
            iter_source.end,
        );
        let mut iter_config = config.clone();
        iter_config.source = iter_source;
        results.push(variable_text::resolve_text_in_project(
            project,
            object,
            &iter_config,
            i,
        ));
    }
    Ok(results)
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
    let oid = parse_id(&object_id)?;
    let project_guard = svc.project.lock().map_err(|e| e.to_string())?;
    let project = project_guard
        .as_ref()
        .ok_or_else(|| "No project open".to_string())?;
    let object = project
        .find_object(oid)
        .ok_or_else(|| "Object not found".to_string())?
        .clone();
    if !matches!(object.data, ObjectData::Text { .. }) {
        return Err("Object is not a text object".to_string());
    }
    drop(project_guard);

    let count = (config.source.total_copies as usize).max(1);
    let base_current = config.source.current;
    let mut copies = Vec::with_capacity(count);
    for i in 0..count {
        let mut copy_source = config.source.clone();
        copy_source.current = beambench_core::wrap_sequence_value(
            base_current + i as i64 * config.source.advance_by,
            copy_source.start,
            copy_source.end,
        );
        let mut copy_config = config.clone();
        copy_config.source = copy_source;
        let project_guard = svc.project.lock().map_err(|e| e.to_string())?;
        let project = project_guard
            .as_ref()
            .ok_or_else(|| "No project open".to_string())?;
        let object = project
            .find_object(oid)
            .ok_or_else(|| "Object not found".to_string())?;
        let resolved = variable_text::resolve_text_in_project(project, object, &copy_config, i);
        drop(project_guard);
        copies.push(BatchCopyInput {
            resolved_text: resolved,
            variable_text_config: copy_config,
        });
    }

    beambench_service::ops::project::generate_variable_text_batch(&svc, oid, copies, offset_step)
        .map_err(|e| e.to_string())
}
