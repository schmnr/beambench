use std::path::PathBuf;
use std::sync::Arc;

use beambench_core::{AssetId, Project};
use beambench_project::RecoveryInfo;
use beambench_service::ServiceContext;
use beambench_service::ops::persistence as persistence_ops;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

use super::project::parse_id;

#[tauri::command]
pub fn save_project_cmd(
    app: tauri::AppHandle,
    svc: State<'_, Arc<ServiceContext>>,
    path: Option<String>,
) -> Result<String, String> {
    let save_path = if let Some(p) = path {
        PathBuf::from(p)
    } else {
        match persistence_ops::save_project_current_path(&svc) {
            Ok(path) => return Ok(path),
            Err(_) => {
                return save_project_as_cmd(app, svc);
            }
        }
    };

    persistence_ops::save_project_to_path(&svc, &save_path).map_err(Into::into)
}

#[tauri::command]
pub fn save_project_as_cmd(
    app: tauri::AppHandle,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<String, String> {
    let file_path = app
        .dialog()
        .file()
        .add_filter("Beam Bench Project", &["lzrproj"])
        .set_file_name("project.lzrproj")
        .set_title("Save Project As")
        .blocking_save_file();

    let Some(fp) = file_path else {
        return Err("Save cancelled".to_string());
    };

    let save_path = fp
        .into_path()
        .map_err(|e| format!("Invalid file path: {e}"))?;

    persistence_ops::save_project_to_path(&svc, &save_path).map_err(Into::into)
}

#[tauri::command]
pub fn open_project_cmd(
    app: tauri::AppHandle,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<Project, String> {
    let file_path = app
        .dialog()
        .file()
        .add_filter("Beam Bench Project", &["lzrproj"])
        .set_title("Open Project")
        .blocking_pick_file();

    let Some(fp) = file_path else {
        return Err("Open cancelled".to_string());
    };

    let open_path = fp
        .into_path()
        .map_err(|e| format!("Invalid file path: {e}"))?;

    persistence_ops::open_project_from_path(&svc, &open_path.to_string_lossy()).map_err(Into::into)
}

#[tauri::command]
pub fn open_project_from_path(
    svc: State<'_, Arc<ServiceContext>>,
    file_path: String,
) -> Result<Project, String> {
    persistence_ops::open_project_from_path(&svc, &file_path).map_err(Into::into)
}

#[tauri::command]
pub fn get_asset_data(
    svc: State<'_, Arc<ServiceContext>>,
    asset_id: String,
) -> Result<Vec<u8>, String> {
    let id: AssetId = parse_id(&asset_id)?;
    persistence_ops::get_asset_data(&svc, id).map_err(Into::into)
}

#[tauri::command]
pub fn autosave_project(svc: State<'_, Arc<ServiceContext>>) -> Result<String, String> {
    persistence_ops::autosave_project(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn check_recovery_files() -> Result<Vec<RecoveryInfo>, String> {
    persistence_ops::check_recovery_files().map_err(Into::into)
}

#[tauri::command]
pub fn restore_recovery(
    recovery_path: String,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<Project, String> {
    persistence_ops::restore_recovery_file(&svc, &recovery_path).map_err(Into::into)
}

#[tauri::command]
pub fn discard_recovery_file(
    recovery_path: String,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<(), String> {
    persistence_ops::discard_recovery_file(&svc, &recovery_path).map_err(Into::into)
}
