use std::sync::Arc;

use crate::context::ServiceContext;
use crate::ops::project::{BatchCopyInput, BatchResult};
use beambench_core::ObjectData;
use beambench_core::variable_text::{self, MergeFieldInfo, VariableTextConfig};
use serde::Serialize;

use super::project::parse_id;

/// Wrapper for CSV data returned to the frontend.
#[derive(Debug, Serialize)]
pub struct CsvData {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Parse merge fields from text content.
pub fn parse_merge_fields(text: String) -> Vec<MergeFieldInfo> {
    variable_text::parse_merge_fields(&text)
}

/// Load CSV file and return parsed rows + headers.
pub fn load_csv_file(path: String) -> Result<CsvData, String> {
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("Failed to read CSV file: {e}"))?;
    let rows = variable_text::parse_csv(&content)?;
    if rows.is_empty() {
        return Err("CSV file is empty".to_string());
    }
    let headers = rows[0].clone();
    if headers.iter().any(|header| header.trim().is_empty())
        || headers
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != headers.len()
    {
        return Err("CSV headers must be nonempty and unique".to_string());
    }
    if rows.len() < 2 {
        return Err("CSV must contain at least one data row".to_string());
    }
    let data_rows = rows[1..].to_vec();
    Ok(CsvData {
        headers,
        rows: data_rows,
    })
}

fn validate_config(config: &VariableTextConfig) -> Result<(), String> {
    if !(1..=1000).contains(&config.source.total_copies) {
        return Err("Choose between 1 and 1000 copies".into());
    }
    if [
        config.source.current,
        config.source.start,
        config.source.end,
        config.source.advance_by,
        config.offset.unwrap_or(0),
    ]
    .iter()
    .any(|value| value.unsigned_abs() > 1_000_000_000)
    {
        return Err("Sequence values must be between -1000000000 and 1000000000".into());
    }
    if matches!(config.mode, Some(variable_text::VariableTextMode::MergeCsv))
        && config.source.csv_data.len() < 2
    {
        return Err("Load a CSV with data rows first".into());
    }
    Ok(())
}

/// Resolve a text template for a specific row.
/// `row` is the preview row index — sets `source.current` for CSV
/// resolution and is also passed as the copy index for `{Serial:params}`.
pub fn resolve_variable_text(
    svc: &Arc<ServiceContext>,
    object_id: String,
    config: VariableTextConfig,
    row: usize,
) -> Result<String, String> {
    validate_config(&config)?;
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
pub fn generate_batch_preview(
    svc: &Arc<ServiceContext>,
    object_id: String,
    config: VariableTextConfig,
) -> Result<Vec<String>, String> {
    validate_config(&config)?;
    let oid = parse_id(&object_id)?;
    let project_guard = svc.project.lock().map_err(|e| e.to_string())?;
    let project = project_guard
        .as_ref()
        .ok_or_else(|| "No project open".to_string())?;
    let object = project
        .find_object(oid)
        .ok_or_else(|| "Object not found".to_string())?;
    let count = config.source.total_copies as usize;
    let base_current = config.source.current;
    let mut results = Vec::with_capacity(count);
    let mut iter_config = config.clone();
    for i in 0..count {
        iter_config.source.current = beambench_core::wrap_sequence_value(
            base_current + i as i64 * config.source.advance_by,
            config.source.start,
            config.source.end,
        );
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
/// in a single undo step. Copies retain fixed editable text. The original's
/// source configuration is retained and its sequence advanced.
pub fn generate_variable_text_batch(
    svc: &Arc<ServiceContext>,
    config: VariableTextConfig,
    object_id: String,
    offset_step: f64,
    offset_step_y: Option<f64>,
) -> Result<BatchResult, String> {
    validate_config(&config)?;
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

    let count = config.source.total_copies as usize;
    let base_current = config.source.current;
    let mut copies = Vec::with_capacity(count);
    let mut copy_config = config.clone();
    // Only the first copy's settings are used to update the reusable source.
    // Generated copies are frozen, so do not retain N copies of a large CSV.
    let mut frozen_config = config.clone();
    frozen_config.source.csv_data.clear();
    let project_guard = svc.project.lock().map_err(|e| e.to_string())?;
    let project = project_guard
        .as_ref()
        .ok_or_else(|| "No project open".to_string())?;
    let object = project
        .find_object(oid)
        .ok_or_else(|| "Object not found".to_string())?;
    for i in 0..count {
        copy_config.source.current = beambench_core::wrap_sequence_value(
            base_current + i as i64 * config.source.advance_by,
            config.source.start,
            config.source.end,
        );
        let resolved = variable_text::resolve_text_in_project(project, object, &copy_config, i);
        copies.push(BatchCopyInput {
            resolved_text: resolved,
            variable_text_config: if i == 0 {
                config.clone()
            } else {
                frozen_config.clone()
            },
        });
    }
    drop(project_guard);

    crate::ops::project::generate_variable_text_batch(
        svc,
        oid,
        copies,
        offset_step,
        offset_step_y.unwrap_or(offset_step),
    )
    .map_err(|e| e.to_string())
}

super::define_commands! {
svc;
parse_merge_fields { text : String } => Ok :: < _ , String > (parse_merge_fields (text));
load_csv_file { path : String } => load_csv_file (path);
resolve_variable_text { object_id : String , config : VariableTextConfig , row : usize } => resolve_variable_text (svc , object_id , config , row);
generate_batch_preview { object_id : String , config : VariableTextConfig } => generate_batch_preview (svc , object_id , config);
generate_variable_text_batch { config : VariableTextConfig , object_id : String , offset_step : f64 , offset_step_y : Option<f64> } => generate_variable_text_batch (svc , config , object_id , offset_step , offset_step_y);
}
