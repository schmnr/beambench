use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use beambench_service::{
    ServiceContext, ServiceError,
    ops::{machine, nesting, planning, workflows},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::response::{ApiError, confirmation_required, map_service_error};

pub fn router() -> Router<Arc<ServiceContext>> {
    Router::new()
        .route("/schema", get(schema))
        .route("/nest", post(nest))
        .route("/{group}", post(execute))
}

async fn schema() -> Json<Value> {
    Json(workflows::schema())
}

fn invalid(message: impl Into<String>) -> ApiError {
    map_service_error(ServiceError::invalid_input(message))
}

async fn execute(
    State(ctx): State<Arc<ServiceContext>>,
    Path(group): Path<String>,
    Json(mut body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let fields = body
        .as_object_mut()
        .ok_or_else(|| invalid("Expected an operation object"))?;
    let motion = fields
        .remove("confirm_motion")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let laser = fields
        .remove("confirm_laser_on")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let operation = fields
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if group == "quality_test"
        && matches!(
            operation.as_str(),
            "quality_test_frame" | "quality_test_start"
        )
    {
        let mut missing = Vec::new();
        if !motion {
            missing.push("confirm_motion");
        }
        if operation == "quality_test_start" && !laser {
            missing.push("confirm_laser_on");
        }
        if !missing.is_empty() {
            return Err(confirmation_required(&missing));
        }
    }
    // Validate paths in the supported native file operations before executing them.
    for key in ["path", "file_path"] {
        if let Some(path) = fields.get(key).and_then(Value::as_str) {
            crate::response::validate_output_path(path)?;
        }
    }
    let runtime = tokio::runtime::Handle::current();
    let tick_ctx = ctx.clone();
    let start_tick = group == "quality_test"
        && matches!(
            operation.as_str(),
            "quality_test_start" | "quality_test_frame"
        );
    let response = tokio::task::spawn_blocking(move || {
        let _transaction = ctx.design_transaction_lock.try_lock().map_err(|_| {
            map_service_error(ServiceError::busy("Another design operation is running"))
        })?;
        let before = ctx
            .project
            .lock()
            .map_err(|e| map_service_error(ServiceError::internal(e.to_string())))?
            .clone();
        let command_error = |e: serde_json::Error| invalid(e.to_string());
        let result = match group.as_str() {
            "project" => runtime.block_on(
                serde_json::from_value::<workflows::project::Command>(body)
                    .map_err(command_error)?
                    .execute(&ctx),
            ),
            "vector" => runtime.block_on(
                serde_json::from_value::<workflows::vector::Command>(body)
                    .map_err(command_error)?
                    .execute(&ctx),
            ),
            "variable_text" => runtime.block_on(
                serde_json::from_value::<workflows::variable_text::Command>(body)
                    .map_err(command_error)?
                    .execute(&ctx),
            ),
            "art_library" => runtime.block_on(
                serde_json::from_value::<workflows::art_library::Command>(body)
                    .map_err(command_error)?
                    .execute(&ctx),
            ),
            "quality_test" => runtime.block_on(
                serde_json::from_value::<workflows::quality_test::Command>(body)
                    .map_err(command_error)?
                    .execute(&ctx),
            ),
            _ => return Err(invalid("Unknown workflow group")),
        }
        .map_err(|e| {
            let mut error = ServiceError::invalid_input(e.message);
            error.details = Some(e.details);
            map_service_error(error)
        })?;
        let changed = *ctx
            .project
            .lock()
            .map_err(|e| map_service_error(ServiceError::internal(e.to_string())))?
            != before;
        if changed {
            planning::invalidate_plan_cache(&ctx).map_err(map_service_error)?;
            ctx.emit_event("project.workflow.applied", json!({"operation": operation}));
        }
        if group == "art_library" && operation != "get_art_libraries" {
            ctx.emit_event(
                "art_library.workflow.applied",
                json!({"operation": operation}),
            );
        }
        Ok(Json(result))
    })
    .await
    .map_err(|e| map_service_error(ServiceError::internal(e.to_string())))??;
    if start_tick {
        machine::spawn_job_tick_loop(tick_ctx);
    }
    Ok(response)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NestBody {
    selected_ids: Vec<beambench_core::ObjectId>,
    options: nesting::NestOptions,
}

async fn nest(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<NestBody>,
) -> Result<Json<Value>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let _transaction = ctx.design_transaction_lock.try_lock().map_err(|_| {
            map_service_error(ServiceError::busy("Another design operation is running"))
        })?;
        let result = nesting::nest_selected(&ctx, body.selected_ids, body.options)
            .map_err(map_service_error)?;
        ctx.emit_event("project.workflow.applied", json!({"operation": "nest"}));
        Ok(Json(json!(result)))
    })
    .await
    .map_err(|e| map_service_error(ServiceError::internal(e.to_string())))?
}
