use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use beambench_core::MaterialPreset;
use beambench_service::ServiceContext;
use beambench_service::ops::{planning, project as project_ops};
use beambench_service::persist;
use serde::Deserialize;

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
        .route("/", get(get_material_presets).post(save_material_preset))
        .route("/{id}", axum::routing::delete(delete_material_preset))
        .route("/{id}/apply", post(apply_material_preset))
        // expose import/export so the HTTP surface has parity with the Tauri command layer.
        .route("/export", get(export_material_presets))
        .route("/import", post(import_material_presets))
}

/// Get all material presets.
async fn get_material_presets(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let presets = ctx.get_materials().map_err(bad_request)?;
    Ok(Json(serde_json::json!({
        "presets": presets,
        "count": presets.len(),
    })))
}

/// Save or update a material preset.
async fn save_material_preset(
    State(ctx): State<Arc<ServiceContext>>,
    Json(preset): Json<MaterialPreset>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), ApiError> {
    ctx.save_material(preset.clone()).map_err(bad_request)?;
    let presets = ctx.get_materials().map_err(bad_request)?;
    persist::save_material_presets(&presets)
        .map_err(|e| internal_error(format!("Failed to persist materials: {e}")))?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({
            "success": true,
            "preset_id": preset.id.to_string(),
        })),
    ))
}

/// Delete a material preset by ID.
async fn delete_material_preset(
    State(ctx): State<Arc<ServiceContext>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid =
        uuid::Uuid::parse_str(&id).map_err(|e| bad_request(format!("Invalid preset ID: {e}")))?;
    ctx.delete_material(uuid).map_err(bad_request)?;
    let presets = ctx.get_materials().map_err(bad_request)?;
    persist::save_material_presets(&presets)
        .map_err(|e| internal_error(format!("Failed to persist materials: {e}")))?;
    Ok(Json(serde_json::json!({
        "success": true,
    })))
}

#[derive(Deserialize)]
struct ApplyMaterialBody {
    layer_id: String,
}

/// Apply a material preset to a layer in the current project.
async fn apply_material_preset(
    State(ctx): State<Arc<ServiceContext>>,
    Path(id): Path<String>,
    Json(body): Json<ApplyMaterialBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let preset_id =
        uuid::Uuid::parse_str(&id).map_err(|e| bad_request(format!("Invalid preset ID: {e}")))?;
    let layer_uuid = uuid::Uuid::parse_str(&body.layer_id)
        .map_err(|e| bad_request(format!("Invalid layer ID: {e}")))?;
    let layer_id = beambench_common::Id::from_uuid(layer_uuid);
    let response = {
        let mut guard = ctx
            .project
            .lock()
            .map_err(|e| internal_error(format!("Failed to lock project: {e}")))?;
        let project = guard
            .as_mut()
            .ok_or_else(|| bad_request("No project is open"))?;

        ctx.push_project_undo_snapshot(project)
            .map_err(internal_error)?;
        let old_rs = project
            .find_layer(layer_id)
            .and_then(|layer| layer.primary_entry().raster_settings.clone());
        let response = ctx
            .apply_material(preset_id, layer_id, project)
            .map_err(bad_request)?;

        if let Some(new_rs) = project
            .find_layer(layer_id)
            .and_then(|layer| layer.primary_entry().raster_settings.clone())
        {
            project_ops::sync_passthrough_bounds(project, layer_id, old_rs.as_ref(), &new_rs);
        }
        project.dirty = true;
        response
    };
    planning::invalidate_plan_cache(&ctx).map_err(|e| internal_error(format!("{e}")))?;
    ctx.emit_event(
        "project.layer.updated",
        serde_json::json!({
            "layer_id": body.layer_id,
        }),
    );

    Ok(Json(serde_json::to_value(response).unwrap_or_default()))
}

/// export all material presets as JSON (mirrors the Tauri
/// `export_material_presets` command but writes to the HTTP response instead
/// of the local filesystem).
async fn export_material_presets(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<Vec<MaterialPreset>>, ApiError> {
    let presets = ctx.get_materials().map_err(bad_request)?;
    Ok(Json(presets))
}

/// import a JSON array of material presets. Merges with existing
/// (regenerating IDs) and persists atomically — mirrors the Tauri
/// `import_material_presets` command but accepts JSON body instead of reading
/// from a filesystem path.
async fn import_material_presets(
    State(ctx): State<Arc<ServiceContext>>,
    Json(imported): Json<Vec<MaterialPreset>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let existing = ctx.get_materials().map_err(bad_request)?;
    let mut merged = existing;
    merged.extend(imported.into_iter().map(|mut preset| {
        preset.id = uuid::Uuid::new_v4();
        preset
    }));
    persist::save_material_presets(&merged)
        .map_err(|e| internal_error(format!("Failed to persist materials: {e}")))?;
    ctx.replace_materials(merged.clone()).map_err(bad_request)?;
    Ok(Json(serde_json::json!({
        "success": true,
        "count": merged.len(),
        "presets": merged,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use tower::ServiceExt;

    use crate::test_support::PersistTestGuard;

    #[tokio::test]
    async fn get_material_presets_returns_empty_list() {
        let _persist = PersistTestGuard::new();
        let ctx = Arc::new(ServiceContext::with_settings(Default::default()));
        let router = crate::routes::build_router(ctx.clone());

        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/materials")
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
        assert!(json["presets"].is_array());
    }

    #[tokio::test]
    async fn save_material_preset_creates_new_preset() {
        let _persist = PersistTestGuard::new();
        let ctx = Arc::new(ServiceContext::with_settings(Default::default()));
        let router = crate::routes::build_router(ctx.clone());

        let preset = MaterialPreset {
            id: uuid::Uuid::new_v4(),
            name: "Test Material".to_string(),
            material: "Wood".to_string(),
            thickness_mm: 3.0,
            operation: beambench_core::OperationType::Cut,
            speed_mm_min: 500.0,
            power_percent: 80.0,
            passes: 1,
            dpi: None,
            raster_mode: None,
            line_interval_mm: None,
            scan_angle: None,
            bidirectional: None,
            overscan_mm: None,
            flood_fill: None,
            angle_passes: None,
            angle_increment_deg: None,
            pass_through: None,
            halftone_cells_per_inch: None,
            halftone_angle_deg: None,
            newsprint_angle_deg: None,
            newsprint_frequency: None,
            invert: None,
            dot_width_correction_mm: None,
            ramp_length_mm: None,
            notes: String::new(),
            category: String::new(),
            device_profile: None,
        };

        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/materials")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::to_string(&preset).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn apply_material_fails_without_project() {
        let _persist = PersistTestGuard::new();
        let ctx = Arc::new(ServiceContext::with_settings(Default::default()));
        let router = crate::routes::build_router(ctx.clone());

        let preset_id = uuid::Uuid::new_v4();
        let layer_id = uuid::Uuid::new_v4();

        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri(&format!("/api/v1/materials/{preset_id}/apply"))
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::json!({"layer_id": layer_id.to_string()}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    // HTTP export/import parity with the Tauri command layer.
    #[tokio::test]
    async fn import_then_export_material_presets_roundtrips() {
        let _persist = PersistTestGuard::new();
        let ctx = Arc::new(ServiceContext::with_settings(Default::default()));
        let router = crate::routes::build_router(ctx.clone());

        let preset = MaterialPreset {
            id: uuid::Uuid::new_v4(),
            name: "Imported Plywood".to_string(),
            material: "Wood".to_string(),
            thickness_mm: 6.0,
            operation: beambench_core::OperationType::Cut,
            speed_mm_min: 400.0,
            power_percent: 90.0,
            passes: 2,
            dpi: None,
            raster_mode: None,
            line_interval_mm: None,
            scan_angle: None,
            bidirectional: None,
            overscan_mm: None,
            flood_fill: None,
            angle_passes: None,
            angle_increment_deg: None,
            pass_through: None,
            halftone_cells_per_inch: None,
            halftone_angle_deg: None,
            newsprint_angle_deg: None,
            newsprint_frequency: None,
            invert: None,
            dot_width_correction_mm: None,
            ramp_length_mm: None,
            notes: String::new(),
            category: String::new(),
            device_profile: None,
        };

        // Import a one-element array via the new /import endpoint.
        let import_response = router
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/materials/import")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::to_string(&vec![preset.clone()]).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(import_response.status(), StatusCode::OK);

        // Export should return a JSON array containing the imported preset
        // (with a regenerated ID, since import always regenerates).
        let export_response = router
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/materials/export")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(export_response.status(), StatusCode::OK);
        use http_body_util::BodyExt;
        let body = export_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let exported: Vec<MaterialPreset> = serde_json::from_slice(&body).unwrap();
        assert_eq!(exported.len(), 1);
        assert_eq!(exported[0].name, "Imported Plywood");
        // The ID should have been regenerated during import, so it must differ
        // from the original preset.id.
        assert_ne!(exported[0].id, preset.id);
    }
}
