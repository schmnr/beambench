use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use beambench_core::MacroDefinition;
use beambench_service::ServiceContext;
use beambench_service::persist;

use crate::response::ApiError;

fn bad_request(error: impl Into<String>) -> ApiError {
    (
        axum::http::StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": error.into() })),
    )
}

fn internal_error(error: impl Into<String>) -> ApiError {
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": error.into() })),
    )
}

pub fn router() -> Router<Arc<ServiceContext>> {
    Router::new()
        .route("/", get(get_macros).post(save_macro))
        .route("/{id}", axum::routing::delete(delete_macro))
        .route("/{id}/run", post(run_macro))
        // expose import/export so the HTTP surface has parity with the Tauri command layer.
        .route("/export", get(export_macros))
        .route("/import", post(import_macros))
}

/// Get all macros.
async fn get_macros(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let macros = ctx.get_macros().map_err(bad_request)?;
    Ok(Json(serde_json::json!({
        "macros": macros,
        "count": macros.len(),
    })))
}

/// Save or update a macro.
async fn save_macro(
    State(ctx): State<Arc<ServiceContext>>,
    Json(macro_def): Json<MacroDefinition>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), ApiError> {
    ctx.save_macro(macro_def.clone()).map_err(bad_request)?;
    let macros = ctx.get_macros().map_err(bad_request)?;
    persist::save_macros(&macros)
        .map_err(|e| internal_error(format!("Failed to persist macros: {e}")))?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({
            "success": true,
            "macro_id": macro_def.id.to_string(),
        })),
    ))
}

/// Delete a macro by ID.
async fn delete_macro(
    State(ctx): State<Arc<ServiceContext>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid =
        uuid::Uuid::parse_str(&id).map_err(|e| bad_request(format!("Invalid macro ID: {e}")))?;
    ctx.delete_macro(uuid).map_err(bad_request)?;
    let macros = ctx.get_macros().map_err(bad_request)?;
    persist::save_macros(&macros)
        .map_err(|e| internal_error(format!("Failed to persist macros: {e}")))?;
    Ok(Json(serde_json::json!({
        "success": true,
    })))
}

/// Run a macro by ID (executes all commands in sequence).
async fn run_macro(
    State(ctx): State<Arc<ServiceContext>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = uuid::Uuid::parse_str(&id).map_err(|e| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!("Invalid macro ID: {e}")})),
        )
    })?;

    let macros = ctx.get_macros().map_err(bad_request)?;
    let macro_def = macros
        .iter()
        .find(|m| m.id == uuid)
        .ok_or_else(|| bad_request(format!("Macro {uuid} not found")))?;

    // Execute each command in the macro
    for command in &macro_def.commands {
        ctx.send_gcode_line(command).map_err(bad_request)?;
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "commands_sent": macro_def.commands.len(),
    })))
}

/// export all macros as JSON (mirrors the Tauri `export_macros`
/// command but writes to the HTTP response instead of the local filesystem).
async fn export_macros(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<Vec<MacroDefinition>>, ApiError> {
    let macros = ctx.get_macros().map_err(bad_request)?;
    Ok(Json(macros))
}

/// import a JSON array of macros. Merges with existing
/// (regenerating IDs) and persists atomically — mirrors the Tauri
/// `import_macros` command but accepts JSON body instead of reading from a
/// filesystem path.
async fn import_macros(
    State(ctx): State<Arc<ServiceContext>>,
    Json(imported): Json<Vec<MacroDefinition>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let existing = ctx.get_macros().map_err(bad_request)?;
    let mut merged = existing;
    merged.extend(imported.into_iter().map(|mut macro_def| {
        macro_def.id = uuid::Uuid::new_v4();
        macro_def
    }));
    persist::save_macros(&merged)
        .map_err(|e| internal_error(format!("Failed to persist macros: {e}")))?;
    ctx.replace_macros(merged.clone()).map_err(bad_request)?;
    Ok(Json(serde_json::json!({
        "success": true,
        "count": merged.len(),
        "macros": merged,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use tower::ServiceExt;

    use crate::test_support::PersistTestGuard;

    #[tokio::test]
    async fn get_macros_returns_empty_list() {
        let _persist = PersistTestGuard::new();
        let ctx = Arc::new(ServiceContext::with_settings(Default::default()));
        let router = crate::routes::build_router(ctx.clone());

        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/macros")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        use http_body_util::BodyExt;
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["count"], 0);
        assert!(json["macros"].is_array());
    }

    #[tokio::test]
    async fn save_macro_creates_new_macro() {
        let _persist = PersistTestGuard::new();
        let ctx = Arc::new(ServiceContext::with_settings(Default::default()));
        let router = crate::routes::build_router(ctx.clone());

        let macro_def = MacroDefinition {
            id: uuid::Uuid::new_v4(),
            name: "Test Macro".to_string(),
            description: "Test description".to_string(),
            commands: vec!["G0 X10".to_string()],
            hotkey: None,
            show_in_toolbar: false,
        };

        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/macros")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::to_string(&macro_def).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn run_macro_fails_without_session() {
        let _persist = PersistTestGuard::new();
        let ctx = Arc::new(ServiceContext::with_settings(Default::default()));
        let router = crate::routes::build_router(ctx.clone());

        let macro_def = MacroDefinition {
            id: uuid::Uuid::new_v4(),
            name: "Test Macro".to_string(),
            description: "Test".to_string(),
            commands: vec!["$H".to_string()],
            hotkey: None,
            show_in_toolbar: false,
        };
        ctx.save_macro(macro_def.clone()).unwrap();

        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri(&format!("/api/v1/macros/{}/run", macro_def.id))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
