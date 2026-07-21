use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use beambench_common::{Bounds, Id, Transform2D};
use beambench_core::{CutEntryPatch, LayerPatch, ObjectData, OperationType, Project};
use beambench_service::ServiceContext;
use beambench_service::ops::{persistence, project};
use serde::Deserialize;
use uuid::Uuid;

use crate::response::{ApiError, map_service_error};

pub fn router() -> Router<Arc<ServiceContext>> {
    Router::new()
        .route("/current", get(get_current))
        .route("/", post(create_project))
        .route("/open", post(open_project))
        .route("/save", post(save_project))
        .route("/save-as", post(save_project_as))
        .route("/close", post(close_project))
        // Internal document-sync/recovery helper. Prefer the typed mutation
        // routes above and below for the public API surface.
        .route("/replace", post(replace_project))
        .route("/undo", post(undo_project))
        .route("/redo", post(redo_project))
        .route("/recovery", get(list_recovery))
        .route("/recovery/restore", post(restore_recovery))
        .route("/recovery/discard", post(discard_recovery))
        .route("/layers", get(get_layers).post(add_layer))
        .route("/layers/{id}", patch(update_layer).delete(remove_layer))
        .route("/layers/{id}/entries", post(add_cut_entry))
        .route("/layers/{id}/entries/reorder", post(reorder_cut_entry))
        .route(
            "/layers/{id}/entries/{entry_id}",
            patch(update_cut_entry).delete(remove_cut_entry),
        )
        .route("/layers/reorder", post(reorder_layer))
        .route("/objects", get(get_objects).post(add_object))
        .route("/objects/atomic", post(add_object_atomic))
        .route("/objects/{id}", patch(update_object).delete(remove_object))
        .route("/objects/{id}/data", patch(update_object_data))
        .route("/objects/{id}/duplicate", post(duplicate_object))
        .route("/objects/align", post(align_objects))
        .route("/objects/distribute", post(distribute_objects))
}

fn parse_typed_id<T>(raw: &str) -> Result<Id<T>, ApiError> {
    let uuid = Uuid::parse_str(raw).map_err(|e| {
        map_service_error(beambench_service::ServiceError::invalid_input(format!(
            "Invalid ID: {e}"
        )))
    })?;
    Ok(Id::from_uuid(uuid))
}

async fn get_current(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let project = project::get_project(&ctx).map_err(map_service_error)?;
    let project = project.ok_or_else(|| {
        map_service_error(beambench_service::ServiceError::not_found(
            "No project open",
        ))
    })?;
    Ok(Json(serde_json::to_value(project).unwrap_or_default()))
}

#[derive(Deserialize)]
struct CreateBody {
    name: String,
}

async fn create_project(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<CreateBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let new_project = project::create_project(&ctx, &body.name).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(new_project).unwrap_or_default()))
}

#[derive(Deserialize)]
struct OpenBody {
    path: String,
}

async fn open_project(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<OpenBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let loaded =
        persistence::open_project_from_path(&ctx, &body.path).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(loaded).unwrap_or_default()))
}

async fn save_project(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let path = persistence::save_project_current_path(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({ "saved": true, "path": path })))
}

#[derive(Deserialize)]
struct SaveAsBody {
    path: String,
}

async fn save_project_as(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<SaveAsBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let path = persistence::save_project_to_path(&ctx, std::path::Path::new(&body.path))
        .map_err(map_service_error)?;
    Ok(Json(serde_json::json!({ "saved": true, "path": path })))
}

async fn close_project(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    project::close_project(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({ "closed": true })))
}

async fn replace_project(
    State(ctx): State<Arc<ServiceContext>>,
    Json(project_body): Json<Project>,
) -> Result<Json<serde_json::Value>, ApiError> {
    project::replace_project_document(&ctx, project_body).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({ "replaced": true })))
}

async fn undo_project(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let restored = project::undo_project(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(restored).unwrap_or_default()))
}

async fn redo_project(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let restored = project::redo_project(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(restored).unwrap_or_default()))
}

async fn list_recovery(
    State(_ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let recovery = persistence::check_recovery_files().map_err(map_service_error)?;
    Ok(Json(serde_json::json!({
        "recovery_files": recovery,
        "count": recovery.len(),
    })))
}

#[derive(Deserialize)]
struct RecoveryBody {
    path: String,
}

async fn restore_recovery(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<RecoveryBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let project =
        persistence::restore_recovery_file(&ctx, &body.path).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(project).unwrap_or_default()))
}

async fn discard_recovery(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<RecoveryBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    persistence::discard_recovery_file(&ctx, &body.path).map_err(map_service_error)?;
    Ok(Json(
        serde_json::json!({ "discarded": true, "path": body.path }),
    ))
}

async fn get_layers(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let layers = project::get_layers(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({
        "layers": layers,
        "count": layers.len(),
    })))
}

#[derive(Deserialize)]
struct AddLayerBody {
    name: String,
    operation: String,
}

async fn add_layer(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<AddLayerBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let operation = match body.operation.as_str() {
        "image" => beambench_core::OperationType::Image,
        "line" => beambench_core::OperationType::Line,
        "fill" => beambench_core::OperationType::Fill,
        "score" => beambench_core::OperationType::Score,
        "cut" => beambench_core::OperationType::Cut,
        "tool" => beambench_core::OperationType::Tool,
        other => {
            return Err(map_service_error(
                beambench_service::ServiceError::invalid_input(format!(
                    "Invalid operation type: {other}"
                )),
            ));
        }
    };
    let layer = project::add_layer(
        &ctx,
        project::AddLayerInput {
            name: body.name,
            operation,
        },
    )
    .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(layer).unwrap_or_default()))
}

#[derive(Deserialize)]
struct UpdateLayerBody(LayerPatch);

async fn update_layer(
    State(ctx): State<Arc<ServiceContext>>,
    Path(layer_id): Path<String>,
    Json(body): Json<UpdateLayerBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_typed_id(&layer_id)?;
    let updated = project::update_layer(
        &ctx,
        id,
        project::UpdateLayerInput {
            name: body.0.name,
            enabled: body.0.enabled,
            visible: body.0.visible,
            color_tag: body.0.color_tag,
            ..Default::default()
        },
    )
    .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(updated).unwrap_or_default()))
}

async fn remove_layer(
    State(ctx): State<Arc<ServiceContext>>,
    Path(layer_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_typed_id(&layer_id)?;
    project::remove_layer(&ctx, id).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({ "removed": true })))
}

#[derive(Deserialize)]
struct AddCutEntryBody {
    after_entry_id: Option<String>,
}

async fn add_cut_entry(
    State(ctx): State<Arc<ServiceContext>>,
    Path(layer_id): Path<String>,
    Json(body): Json<AddCutEntryBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let layer_id = parse_typed_id(&layer_id)?;
    let after_entry_id = body
        .after_entry_id
        .as_deref()
        .map(parse_typed_id)
        .transpose()?;
    let entry =
        project::add_cut_entry(&ctx, layer_id, after_entry_id).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(entry).unwrap_or_default()))
}

async fn remove_cut_entry(
    State(ctx): State<Arc<ServiceContext>>,
    Path((layer_id, entry_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let layer_id = parse_typed_id(&layer_id)?;
    let entry_id = parse_typed_id(&entry_id)?;
    project::remove_cut_entry(&ctx, layer_id, entry_id).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({ "removed": true })))
}

#[derive(Deserialize)]
struct ReorderCutEntryBody {
    entry_id: String,
    new_index: usize,
}

async fn reorder_cut_entry(
    State(ctx): State<Arc<ServiceContext>>,
    Path(layer_id): Path<String>,
    Json(body): Json<ReorderCutEntryBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let layer_id = parse_typed_id(&layer_id)?;
    let entry_id = parse_typed_id(&body.entry_id)?;
    let layer = project::reorder_cut_entry(&ctx, layer_id, entry_id, body.new_index)
        .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(layer).unwrap_or_default()))
}

async fn update_cut_entry(
    State(ctx): State<Arc<ServiceContext>>,
    Path((layer_id, entry_id)): Path<(String, String)>,
    Json(patch): Json<CutEntryPatch>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let layer_id = parse_typed_id(&layer_id)?;
    let entry_id = parse_typed_id(&entry_id)?;
    let entry =
        project::update_cut_entry(&ctx, layer_id, entry_id, patch).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(entry).unwrap_or_default()))
}

#[derive(Deserialize)]
struct ReorderLayerBody {
    layer_id: String,
    new_index: usize,
}

async fn reorder_layer(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<ReorderLayerBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_typed_id(&body.layer_id)?;
    let layers = project::reorder_layer(&ctx, id, body.new_index).map_err(map_service_error)?;
    Ok(Json(
        serde_json::json!({ "layers": layers, "count": layers.len() }),
    ))
}

async fn get_objects(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let objects = project::get_objects(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({
        "objects": objects,
        "count": objects.len(),
    })))
}

#[derive(Deserialize)]
struct AddObjectBody {
    name: String,
    layer_id: String,
    object_data: ObjectData,
    bounds: Bounds,
}

#[derive(Deserialize)]
struct AddObjectLayerBody {
    name: String,
    color_tag: Option<String>,
    operation: OperationType,
    #[serde(default)]
    entry_patch: Option<CutEntryPatch>,
}

#[derive(Deserialize)]
struct AddObjectAtomicBody {
    name: String,
    layer_id: String,
    object_data: ObjectData,
    bounds: Bounds,
    create_layer: Option<AddObjectLayerBody>,
}

async fn add_object(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<AddObjectBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let layer_id = parse_typed_id(&body.layer_id)?;
    let object = project::add_object(
        &ctx,
        project::AddObjectInput {
            name: body.name,
            layer_id,
            object_data: body.object_data,
            bounds: body.bounds,
        },
    )
    .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(object).unwrap_or_default()))
}

async fn add_object_atomic(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<AddObjectAtomicBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let layer_id = parse_typed_id(&body.layer_id)?;
    let create_layer = body.create_layer.map(|layer| project::AddObjectLayerInput {
        name: layer.name,
        color_tag: layer.color_tag,
        operation: layer.operation,
        entry_patch: layer.entry_patch,
    });
    let result = project::add_object_atomic(
        &ctx,
        project::AddObjectAtomicInput {
            name: body.name,
            layer_id,
            object_data: body.object_data,
            bounds: body.bounds,
            create_layer,
        },
    )
    .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(result).unwrap_or_default()))
}

#[derive(Deserialize)]
struct UpdateObjectBody {
    name: Option<String>,
    visible: Option<bool>,
    locked: Option<bool>,
    layer_id: Option<String>,
    transform: Option<Transform2D>,
    bounds: Option<Bounds>,
    lock_aspect_ratio: Option<bool>,
    power_scale: Option<f64>,
    priority: Option<i32>,
}

async fn update_object(
    State(ctx): State<Arc<ServiceContext>>,
    Path(object_id): Path<String>,
    Json(body): Json<UpdateObjectBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let object_id = parse_typed_id(&object_id)?;
    let layer_id = body.layer_id.as_deref().map(parse_typed_id).transpose()?;
    let object = project::update_object(
        &ctx,
        object_id,
        project::UpdateObjectInput {
            name: body.name,
            visible: body.visible,
            locked: body.locked,
            layer_id,
            transform: body.transform,
            bounds: body.bounds,
            lock_aspect_ratio: body.lock_aspect_ratio,
            power_scale: body.power_scale,
            priority: body.priority,
        },
    )
    .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(object).unwrap_or_default()))
}

async fn update_object_data(
    State(ctx): State<Arc<ServiceContext>>,
    Path(object_id): Path<String>,
    Json(data): Json<ObjectData>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let object_id = parse_typed_id(&object_id)?;
    let object = project::update_object_data(&ctx, object_id, data).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(object).unwrap_or_default()))
}

async fn remove_object(
    State(ctx): State<Arc<ServiceContext>>,
    Path(object_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let object_id = parse_typed_id(&object_id)?;
    project::remove_object(&ctx, object_id).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({ "removed": true })))
}

async fn duplicate_object(
    State(ctx): State<Arc<ServiceContext>>,
    Path(object_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let object_id = parse_typed_id(&object_id)?;
    let object = project::duplicate_object(&ctx, object_id).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(object).unwrap_or_default()))
}

#[derive(Deserialize)]
struct AlignObjectsBody {
    object_ids: Vec<String>,
    alignment_type: String,
    anchor_object_id: Option<String>,
}

async fn align_objects(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<AlignObjectsBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ids = body
        .object_ids
        .iter()
        .map(|s| parse_typed_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let objects = project::align_objects(
        &ctx,
        ids,
        &body.alignment_type,
        body.anchor_object_id
            .as_deref()
            .map(parse_typed_id)
            .transpose()?,
    )
    .map_err(map_service_error)?;
    Ok(Json(
        serde_json::json!({ "objects": objects, "count": objects.len() }),
    ))
}

#[derive(Deserialize)]
struct DistributeObjectsBody {
    object_ids: Vec<String>,
    direction: String,
}

async fn distribute_objects(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<DistributeObjectsBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ids = body
        .object_ids
        .iter()
        .map(|s| parse_typed_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let objects =
        project::distribute_objects(&ctx, ids, &body.direction).map_err(map_service_error)?;
    Ok(Json(
        serde_json::json!({ "objects": objects, "count": objects.len() }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use tower::ServiceExt;

    async fn collect_body(response: axum::http::Response<axum::body::Body>) -> Vec<u8> {
        use http_body_util::BodyExt;
        let body = response.into_body();
        let collected = body.collect().await.unwrap();
        collected.to_bytes().to_vec()
    }

    #[tokio::test]
    async fn get_current_no_project_returns_404() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/projects/current")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn create_project_returns_project() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/projects")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"name": "Test Project"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["metadata"]["project_name"], "Test Project");
    }

    #[tokio::test]
    async fn create_then_get_current() {
        let ctx = Arc::new(ServiceContext::new());

        // Create a project directly in context
        let project = Project::new("API Test");
        ctx.project.lock().unwrap().replace(project);

        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/projects/current")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["metadata"]["project_name"], "API Test");
    }

    #[tokio::test]
    async fn save_without_project_returns_404() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/projects/save")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn save_as_without_project_returns_404() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/projects/save-as")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"path":"/tmp/test.lzrproj"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn replace_route_clears_path_and_history() {
        let ctx = Arc::new(ServiceContext::new());
        *ctx.project.lock().unwrap() = Some(Project::new("Original"));
        *ctx.project_path.lock().unwrap() = Some(std::path::PathBuf::from("/tmp/original.lzrproj"));
        ctx.push_project_undo_snapshot(&Project::new("Original"))
            .unwrap();
        let app = crate::routes::build_router(ctx.clone());

        let replacement = serde_json::to_string(&Project::new("Replacement")).unwrap();
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/projects/replace")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(replacement))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(ctx.project_path.lock().unwrap().is_none());
        let undo = ctx.undo_state().unwrap();
        assert!(!undo.can_undo);
        assert!(!undo.can_redo);
    }

    #[tokio::test]
    async fn add_object_with_missing_layer_falls_back_to_first_layer() {
        let ctx = Arc::new(ServiceContext::new());
        let mut project = Project::new("Objects");
        let default_layer_id = project.ensure_default_layer();
        *ctx.project.lock().unwrap() = Some(project);
        let app = crate::routes::build_router(ctx.clone());
        let body = serde_json::json!({
            "name": "Rect",
            "layer_id": uuid::Uuid::new_v4().to_string(),
            "object_data": {
                "type": "shape",
                "kind": "rectangle",
                "width": 10.0,
                "height": 10.0,
                "corner_radius": 0.0,
            },
            "bounds": {
                "min": { "x": 0.0, "y": 0.0 },
                "max": { "x": 10.0, "y": 10.0 },
            },
        });

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/projects/objects")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Missing layer falls back to first available layer instead of 404
        assert_eq!(response.status(), StatusCode::OK);
        // Verify the object was placed on the default layer
        let project_guard = ctx.project.lock().unwrap();
        let project = project_guard.as_ref().unwrap();
        let objects = project.objects_in_layer(default_layer_id);
        assert_eq!(
            objects.len(),
            1,
            "Object should be placed on the fallback layer"
        );
    }

    #[tokio::test]
    async fn layer_mutation_routes_participate_in_undo_redo() {
        let ctx = Arc::new(ServiceContext::new());
        *ctx.project.lock().unwrap() = Some(Project::new("History"));
        let initial_layers = ctx.project.lock().unwrap().as_ref().unwrap().layers.len();
        let app = crate::routes::build_router(ctx.clone());

        let add_response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/projects/layers")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"name":"Cut Layer","operation":"cut"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(add_response.status(), StatusCode::OK);
        assert_eq!(
            ctx.project.lock().unwrap().as_ref().unwrap().layers.len(),
            initial_layers + 1
        );

        let undo_response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/projects/undo")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(undo_response.status(), StatusCode::OK);
        assert_eq!(
            ctx.project.lock().unwrap().as_ref().unwrap().layers.len(),
            initial_layers
        );

        let redo_response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/projects/redo")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(redo_response.status(), StatusCode::OK);
        assert_eq!(
            ctx.project.lock().unwrap().as_ref().unwrap().layers.len(),
            initial_layers + 1
        );
    }
}
