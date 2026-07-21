use std::path::PathBuf;
use std::sync::Arc;

use beambench_common::feedback::{
    ConnectionDiagnosticsSnapshot, DiagnosticBundleV1, FeedbackReportInput, SavedReport,
    SubmitFeedbackResponse,
};
use beambench_service::ServiceContext;
use beambench_service::ops::feedback;
use tauri::State;
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
pub fn get_connection_diagnostics(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<ConnectionDiagnosticsSnapshot, String> {
    feedback::get_connection_diagnostics(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn preview_feedback_report(
    svc: State<'_, Arc<ServiceContext>>,
    input: FeedbackReportInput,
) -> Result<DiagnosticBundleV1, String> {
    feedback::preview_feedback_report(&svc, input).map_err(Into::into)
}

#[tauri::command]
pub fn save_feedback_report(
    svc: State<'_, Arc<ServiceContext>>,
    input: FeedbackReportInput,
    path: String,
) -> Result<SavedReport, String> {
    feedback::save_feedback_report(&svc, input, &PathBuf::from(path)).map_err(Into::into)
}

#[tauri::command]
pub async fn submit_feedback_report(
    svc: State<'_, Arc<ServiceContext>>,
    input: FeedbackReportInput,
) -> Result<SubmitFeedbackResponse, String> {
    let transport = feedback::build_submit_request_for_transport(&svc, input.clone())
        .map_err(|e| e.to_string())?;
    let endpoint = std::env::var("BEAMBENCH_FEEDBACK_ENDPOINT")
        .unwrap_or_else(|_| "https://beambench.com/api/feedback/report".to_owned());
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to initialize feedback client: {e}"))?;
    let response = client
        .post(&endpoint)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(transport.body)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "Submission timed out. Save the report to a file and try again later.".to_owned()
            } else {
                format!("Failed to submit report to {endpoint}: {e}")
            }
        })?;

    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(feedback_error_message(status, &text));
    }

    let result = response
        .json::<SubmitFeedbackResponse>()
        .await
        .map_err(|e| format!("Invalid feedback response: {e}"))?;
    feedback::record_feedback_submission(&input, result.report_id.clone())
        .map_err(|e| e.to_string())?;
    feedback::delete_included_panic_files(&svc, &transport.bundle.recent_panics);
    Ok(result)
}

#[tauri::command]
pub fn reveal_feedback_report(app: tauri::AppHandle, path: String) -> Result<(), String> {
    app.opener()
        .reveal_item_in_dir(PathBuf::from(path))
        .map_err(|e| format!("Failed to reveal report: {e}"))
}

fn feedback_error_message(status: reqwest::StatusCode, text: &str) -> String {
    let parsed = serde_json::from_str::<serde_json::Value>(text).ok();
    if let Some(message) = parsed
        .as_ref()
        .and_then(|value| value.get("message"))
        .and_then(serde_json::Value::as_str)
    {
        return message.to_owned();
    }

    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return "Too many reports recently. Save the report to a file and try again later."
            .to_owned();
    }
    if status == reqwest::StatusCode::PAYLOAD_TOO_LARGE {
        return "Feedback report is too large. Save the report to a file instead.".to_owned();
    }
    if status.is_client_error() {
        return format!("Report was rejected ({status}).");
    }
    "Beam Bench could not receive the report right now. Save the report to a file and try again later.".to_owned()
}
