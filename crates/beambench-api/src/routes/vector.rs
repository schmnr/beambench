use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use beambench_common::Id;
use beambench_service::ServiceContext;
use beambench_service::ops::vector;
use serde::Deserialize;
use uuid::Uuid;

use crate::response::{ApiError, map_service_error};

pub fn router() -> Router<Arc<ServiceContext>> {
    Router::new()
        .route("/convert-to-path", post(convert_to_path))
        .route("/boolean/union", post(boolean_union))
        .route("/boolean/subtract", post(boolean_subtract))
        .route("/boolean/intersection", post(boolean_intersection))
        .route("/boolean/exclude", post(boolean_exclude))
        .route("/boolean/weld", post(boolean_weld))
        .route("/group", post(group_objects))
        .route("/ungroup", post(ungroup_objects))
        .route("/{id}/editable-path", get(get_editable_path))
        .route("/{id}/nodes/update", post(update_node))
        .route("/{id}/nodes/delete", post(delete_node))
        .route("/{id}/nodes/insert", post(insert_node))
        .route("/{id}/scale-to-bounds", post(scale_to_bounds))
        .route("/normalize", post(normalize_for_planner))
}

fn parse_object_id(raw: &str) -> Result<beambench_core::ObjectId, ApiError> {
    let uuid = Uuid::parse_str(raw).map_err(|e| {
        map_service_error(beambench_service::ServiceError::invalid_input(format!(
            "Invalid object ID: {e}"
        )))
    })?;
    Ok(Id::from_uuid(uuid))
}

#[derive(Debug, Deserialize)]
struct SingleObjectBody {
    object_id: String,
}

#[derive(Debug, Deserialize)]
struct BooleanBody {
    object_id_a: String,
    object_id_b: String,
}

#[derive(Debug, Deserialize)]
struct GroupBody {
    object_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateNodeBody {
    subpath_idx: usize,
    command_idx: usize,
    x: f64,
    y: f64,
    handle_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeleteNodeBody {
    subpath_idx: usize,
    command_idx: usize,
}

#[derive(Debug, Deserialize)]
struct InsertNodeBody {
    subpath_idx: usize,
    command_idx: usize,
    t: f64,
}

#[derive(Debug, Deserialize)]
struct ScaleBody {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

#[derive(Debug, Deserialize)]
struct NormalizeBody {
    object_ids: Vec<String>,
}

async fn convert_to_path(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<SingleObjectBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let object = vector::convert_to_path(
        &ctx,
        vector::ConvertToPathInput {
            object_id: parse_object_id(&body.object_id)?,
        },
    )
    .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(object).unwrap_or_default()))
}

async fn boolean_union(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<BooleanBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let object = vector::boolean_union(
        &ctx,
        vector::BooleanOpInput {
            object_id_a: parse_object_id(&body.object_id_a)?,
            object_id_b: parse_object_id(&body.object_id_b)?,
        },
    )
    .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(object).unwrap_or_default()))
}

async fn boolean_subtract(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<BooleanBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let object = vector::boolean_subtract(
        &ctx,
        vector::BooleanOpInput {
            object_id_a: parse_object_id(&body.object_id_a)?,
            object_id_b: parse_object_id(&body.object_id_b)?,
        },
    )
    .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(object).unwrap_or_default()))
}

async fn boolean_intersection(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<BooleanBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let object = vector::boolean_intersection(
        &ctx,
        vector::BooleanOpInput {
            object_id_a: parse_object_id(&body.object_id_a)?,
            object_id_b: parse_object_id(&body.object_id_b)?,
        },
    )
    .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(object).unwrap_or_default()))
}

async fn boolean_exclude(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<BooleanBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let object = vector::boolean_exclude(
        &ctx,
        vector::BooleanOpInput {
            object_id_a: parse_object_id(&body.object_id_a)?,
            object_id_b: parse_object_id(&body.object_id_b)?,
        },
    )
    .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(object).unwrap_or_default()))
}

async fn boolean_weld(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<GroupBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let object_ids = body
        .object_ids
        .iter()
        .map(|id| parse_object_id(id))
        .collect::<Result<Vec<_>, _>>()?;
    let object = vector::boolean_weld(&ctx, vector::BooleanWeldInput { object_ids })
        .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(object).unwrap_or_default()))
}

async fn group_objects(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<GroupBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let object_ids = body
        .object_ids
        .iter()
        .map(|id| parse_object_id(id))
        .collect::<Result<Vec<_>, _>>()?;
    let object = vector::group_objects(&ctx, vector::GroupObjectsInput { object_ids })
        .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(object).unwrap_or_default()))
}

async fn ungroup_objects(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<SingleObjectBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let children = vector::ungroup_objects(&ctx, parse_object_id(&body.object_id)?)
        .map_err(map_service_error)?;
    let children: Vec<String> = children.into_iter().map(|id| id.to_string()).collect();
    Ok(Json(serde_json::json!({
        "children": children,
        "count": children.len(),
    })))
}

async fn get_editable_path(
    State(ctx): State<Arc<ServiceContext>>,
    Path(object_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let path =
        vector::get_editable_path(&ctx, parse_object_id(&object_id)?).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(path).unwrap_or_default()))
}

async fn update_node(
    State(ctx): State<Arc<ServiceContext>>,
    Path(object_id): Path<String>,
    Json(body): Json<UpdateNodeBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let object = vector::update_node(
        &ctx,
        vector::UpdateNodeInput {
            object_id: parse_object_id(&object_id)?,
            subpath_idx: body.subpath_idx,
            command_idx: body.command_idx,
            x: body.x,
            y: body.y,
            handle_type: body.handle_type,
        },
    )
    .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(object).unwrap_or_default()))
}

async fn delete_node(
    State(ctx): State<Arc<ServiceContext>>,
    Path(object_id): Path<String>,
    Json(body): Json<DeleteNodeBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let object = vector::delete_node(
        &ctx,
        vector::DeleteNodeInput {
            object_id: parse_object_id(&object_id)?,
            subpath_idx: body.subpath_idx,
            command_idx: body.command_idx,
        },
    )
    .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(object).unwrap_or_default()))
}

async fn insert_node(
    State(ctx): State<Arc<ServiceContext>>,
    Path(object_id): Path<String>,
    Json(body): Json<InsertNodeBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let object = vector::insert_node(
        &ctx,
        vector::InsertNodeInput {
            object_id: parse_object_id(&object_id)?,
            subpath_idx: body.subpath_idx,
            command_idx: body.command_idx,
            t: body.t,
        },
    )
    .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(object).unwrap_or_default()))
}

async fn scale_to_bounds(
    State(ctx): State<Arc<ServiceContext>>,
    Path(object_id): Path<String>,
    Json(body): Json<ScaleBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let object = vector::scale_path_to_bounds(
        &ctx,
        vector::ScalePathToBoundsInput {
            object_id: parse_object_id(&object_id)?,
            new_min_x: body.min_x,
            new_min_y: body.min_y,
            new_max_x: body.max_x,
            new_max_y: body.max_y,
        },
    )
    .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(object).unwrap_or_default()))
}

async fn normalize_for_planner(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<NormalizeBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let object_ids = body
        .object_ids
        .iter()
        .map(|id| parse_object_id(id))
        .collect::<Result<Vec<_>, _>>()?;
    let normalized =
        vector::normalize_for_planner(&ctx, vector::NormalizeForPlannerInput { object_ids })
            .map_err(map_service_error)?;
    Ok(Json(serde_json::json!({
        "vectors": normalized,
        "count": normalized.len(),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use beambench_common::{Bounds, Point2D};
    use beambench_core::{ObjectData, ProjectObject, ShapeKind};
    use tower::ServiceExt;

    async fn collect_body(response: axum::http::Response<axum::body::Body>) -> Vec<u8> {
        use http_body_util::BodyExt;
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec()
    }

    fn seed_project() -> (Arc<ServiceContext>, String, String) {
        let ctx = Arc::new(ServiceContext::new());
        let mut project = beambench_core::Project::new("Vector");
        let layer_id = project.ensure_default_layer();
        let object = ProjectObject::new(
            "Shape",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        let shape_id = object.id.to_string();
        project.add_object(object);
        let vector = ProjectObject::new(
            "Path",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L10 0 L10 10 Z".to_string(),
                closed: true,
                ruler_guide_axis: None,
            },
        );
        let vector_id = vector.id.to_string();
        project.add_object(vector);
        *ctx.project.lock().unwrap() = Some(project);
        (ctx, shape_id, vector_id)
    }

    #[tokio::test]
    async fn editable_path_rejects_non_vector_object() {
        let (ctx, shape_id, _vector_id) = seed_project();
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/api/v1/vector/{shape_id}/editable-path"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn group_and_undo_round_trip_through_api() {
        let (ctx, shape_id, vector_id) = seed_project();
        let app = crate::routes::build_router(ctx.clone());

        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/vector/group")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(format!(
                        r#"{{"object_ids":["{shape_id}","{vector_id}"]}}"#
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/projects/undo")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["objects"].as_array().unwrap().len(), 2);
    }
}
