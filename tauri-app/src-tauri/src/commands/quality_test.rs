use beambench_common::machine::JobProgress;
use beambench_core::{MaterialTestRecipe, QualityTestError, QualityTestRequest};
use beambench_service::ServiceContext;
pub use beambench_service::ops::workflows::quality_test::*;
use std::sync::Arc;
use tauri::State;
#[tauri::command]
pub fn quality_test_preview(
    svc: State<'_, Arc<ServiceContext>>,
    request: QualityTestRequest,
) -> Result<QualityTestPreviewResponse, QualityTestError> {
    beambench_service::ops::workflows::quality_test::quality_test_preview(&svc, request)
}
#[tauri::command]
pub fn quality_test_export_gcode(
    svc: State<'_, Arc<ServiceContext>>,
    request: QualityTestRequest,
    path: String,
) -> Result<QualityTestExportResponse, QualityTestError> {
    beambench_service::ops::workflows::quality_test::quality_test_export_gcode(&svc, request, path)
}
#[tauri::command]
pub fn quality_test_frame(
    svc: State<'_, Arc<ServiceContext>>,
    request: QualityTestRequest,
) -> Result<JobProgress, QualityTestError> {
    beambench_service::ops::workflows::quality_test::quality_test_frame(&svc, request)
}
#[tauri::command]
pub fn quality_test_start(
    svc: State<'_, Arc<ServiceContext>>,
    request: QualityTestRequest,
) -> Result<JobProgress, QualityTestError> {
    beambench_service::ops::workflows::quality_test::quality_test_start(&svc, request)
}
#[tauri::command]
pub fn quality_test_create_material_on_canvas(
    svc: State<'_, Arc<ServiceContext>>,
    request: QualityTestRequest,
) -> Result<QualityTestCanvasResponse, QualityTestError> {
    beambench_service::ops::workflows::quality_test::quality_test_create_material_on_canvas(
        &svc, request,
    )
}
#[tauri::command]
pub fn export_material_test_recipes(
    path: String,
    recipes: Vec<MaterialTestRecipe>,
) -> Result<(), String> {
    beambench_service::ops::workflows::quality_test::export_material_test_recipes(path, recipes)
}
#[tauri::command]
pub fn import_material_test_recipes(path: String) -> Result<Vec<MaterialTestRecipe>, String> {
    beambench_service::ops::workflows::quality_test::import_material_test_recipes(path)
}
