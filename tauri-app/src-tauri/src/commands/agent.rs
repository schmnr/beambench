use std::sync::Arc;

use beambench_service::ServiceContext;
use beambench_service::agent::{self, AgentSelectionSyncInput};
use tauri::State;

#[tauri::command]
pub fn agent_sync_selection(
    svc: State<'_, Arc<ServiceContext>>,
    selected_object_ids: Vec<String>,
    selected_layer_id: Option<String>,
    project_id: Option<String>,
    frontend_updated_at_ms: f64,
) -> Result<(), String> {
    agent::sync_selection(
        &svc,
        AgentSelectionSyncInput {
            selected_object_ids,
            selected_layer_id,
            project_id,
            frontend_updated_at_ms,
        },
    )
    .map_err(|e| e.to_string())
}
