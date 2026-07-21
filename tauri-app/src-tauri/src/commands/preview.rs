use std::sync::Arc;
use std::time::Instant;

use beambench_preview::PreviewData;
use beambench_service::ServiceContext;
use beambench_service::ops::planning::{self, SessionJobOptions};
use tauri::State;

/// Generate a lightweight preview from the current project's execution plan.
/// Uses the cached plan if valid, otherwise generates a fresh one.
#[tauri::command]
pub async fn generate_preview(
    svc: State<'_, Arc<ServiceContext>>,
    job_options: Option<SessionJobOptions>,
) -> Result<PreviewData, String> {
    let svc = svc.inner().clone();
    let job_options = job_options.unwrap_or_default();
    let started_at = Instant::now();
    let preview = tokio::task::spawn_blocking(move || {
        planning::generate_preview_with_options(&svc, &job_options)
    })
    .await
    .map_err(|e| format!("Preview task failed: {e}"))?
    .map_err(|e| e.to_string())?;
    tracing::info!(target: "perf", operation = "generate_preview", duration_ms = started_at.elapsed().as_millis());
    Ok(preview)
}
