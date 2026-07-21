use beambench_common::AppEvent;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Emit a typed application event to all frontend listeners.
pub fn emit_app_event<T: Serialize + Clone>(
    app: &AppHandle,
    event_type: &str,
    payload: T,
) -> Result<(), String> {
    let event = AppEvent::new(event_type, payload);
    app.emit("app-event", &event)
        .map_err(|e| format!("Failed to emit event: {}", e))
}
