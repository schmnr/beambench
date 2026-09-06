//! Shared Material, Focus and Interval Test workflows.
//!
//! Preview/Export/Frame/Start route through a transient pipeline that never touches the active
//! project or undo stack. `quality_test_create_material_on_canvas` is the explicit project
//! materialization path for users who want editable generated artwork.

use std::path::PathBuf;
use std::sync::Arc;

use crate::ops::quality_test;
use crate::{ServiceContext, ServiceError, ServiceErrorCode};
use beambench_common::machine::JobProgress;
use beambench_core::{
    LayerId, MaterialTestRecipe, ObjectId, Project, QualityTestError, QualityTestRequest,
    QualityTestWarning,
};
use beambench_preview::PreviewData;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityTestPreviewResponse {
    pub preview: PreviewData,
    pub warnings: Vec<QualityTestWarning>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityTestExportResponse {
    pub path: PathBuf,
    pub warnings: Vec<QualityTestWarning>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityTestCanvasResponse {
    pub project: Project,
    pub warnings: Vec<QualityTestWarning>,
    pub created_object_ids: Vec<ObjectId>,
    pub created_layer_ids: Vec<LayerId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialTestRecipeFile {
    pub version: u32,
    pub recipes: Vec<MaterialTestRecipe>,
}

const MATERIAL_TEST_RECIPE_FILE_VERSION: u32 = 1;

fn service_error_to_quality_test_error(error: ServiceError) -> QualityTestError {
    if error.code == ServiceErrorCode::InvalidState
        && error.message == "No active machine profile selected"
    {
        QualityTestError::NoActiveMachineProfile
    } else if error.code == ServiceErrorCode::InvalidState
        && error.message == "absolute Z mode requires Project.material_height_mm to be set"
    {
        QualityTestError::MaterialHeightRequired
    } else {
        QualityTestError::Internal {
            message: error.to_string(),
        }
    }
}

pub fn quality_test_preview(
    svc: &Arc<ServiceContext>,
    request: QualityTestRequest,
) -> Result<QualityTestPreviewResponse, QualityTestError> {
    let resp = quality_test::quality_test_preview(svc, &request)
        .map_err(service_error_to_quality_test_error)?;
    Ok(QualityTestPreviewResponse {
        preview: resp.preview,
        warnings: resp.warnings,
    })
}

pub fn quality_test_export_gcode(
    svc: &Arc<ServiceContext>,
    request: QualityTestRequest,
    path: String,
) -> Result<QualityTestExportResponse, QualityTestError> {
    let target = PathBuf::from(&path);
    let resp = quality_test::quality_test_export_gcode(svc, &request, &target)
        .map_err(service_error_to_quality_test_error)?;
    Ok(QualityTestExportResponse {
        path: resp.path,
        warnings: resp.warnings,
    })
}

pub fn quality_test_frame(
    svc: &Arc<ServiceContext>,
    request: QualityTestRequest,
) -> Result<JobProgress, QualityTestError> {
    quality_test::quality_test_frame(svc, &request)
}

pub fn quality_test_start(
    svc: &Arc<ServiceContext>,
    request: QualityTestRequest,
) -> Result<JobProgress, QualityTestError> {
    quality_test::quality_test_start(svc, &request)
}

pub fn quality_test_create_material_on_canvas(
    svc: &Arc<ServiceContext>,
    request: QualityTestRequest,
) -> Result<QualityTestCanvasResponse, QualityTestError> {
    let QualityTestRequest::Material(settings) = request else {
        return Err(QualityTestError::Internal {
            message: "Create on Canvas is only supported for Material Test.".to_string(),
        });
    };
    let resp = quality_test::quality_test_create_material_on_canvas(svc, &settings)?;
    Ok(QualityTestCanvasResponse {
        project: resp.project,
        warnings: resp.warnings,
        created_object_ids: resp.created_object_ids,
        created_layer_ids: resp.created_layer_ids,
    })
}

pub fn export_material_test_recipes(
    path: String,
    recipes: Vec<MaterialTestRecipe>,
) -> Result<(), String> {
    let payload = MaterialTestRecipeFile {
        version: MATERIAL_TEST_RECIPE_FILE_VERSION,
        recipes,
    };
    let json =
        serde_json::to_string_pretty(&payload).map_err(|e| format!("Serialize failed: {e}"))?;
    std::fs::write(PathBuf::from(path), json).map_err(|e| format!("Write failed: {e}"))
}

pub fn import_material_test_recipes(path: String) -> Result<Vec<MaterialTestRecipe>, String> {
    let data =
        std::fs::read_to_string(PathBuf::from(path)).map_err(|e| format!("Read failed: {e}"))?;
    let payload: MaterialTestRecipeFile =
        serde_json::from_str(&data).map_err(|e| format!("Parse failed: {e}"))?;
    if payload.version > MATERIAL_TEST_RECIPE_FILE_VERSION {
        return Err(format!(
            "Unsupported Material Test recipe file version: {}",
            payload.version
        ));
    }
    Ok(payload.recipes)
}

super::define_commands! {
svc;
quality_test_preview { request : QualityTestRequest } => quality_test_preview (svc , request);
quality_test_export_gcode { request : QualityTestRequest , path : String } => quality_test_export_gcode (svc , request , path);
quality_test_frame { request : QualityTestRequest } => quality_test_frame (svc , request);
quality_test_start { request : QualityTestRequest } => quality_test_start (svc , request);
quality_test_create_material_on_canvas { request : QualityTestRequest } => quality_test_create_material_on_canvas (svc , request);
export_material_test_recipes { path : String , recipes : Vec < MaterialTestRecipe > } => export_material_test_recipes (path , recipes);
import_material_test_recipes { path : String } => import_material_test_recipes (path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_error_mapping_preserves_quality_test_error_shape() {
        assert_eq!(
            service_error_to_quality_test_error(ServiceError::invalid_state(
                "No active machine profile selected"
            )),
            QualityTestError::NoActiveMachineProfile
        );
        assert_eq!(
            service_error_to_quality_test_error(ServiceError::internal("planner failed")),
            QualityTestError::Internal {
                message: "planner failed".to_string()
            }
        );
    }
}
