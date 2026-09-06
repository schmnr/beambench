use std::sync::Arc;
use beambench_core::{ArtLibraryItem, ProjectObject};
use beambench_service::ServiceContext;
use tauri::State;
pub use beambench_service::ops::workflows::art_library::*;
#[tauri::command]
pub fn get_art_libraries(
    ctx: State<'_, Arc<ServiceContext>>,
) -> Result<beambench_service::persist::ArtLibraryLoadState, String> {
    beambench_service::ops::workflows::art_library::get_art_libraries(&ctx)
}
#[tauri::command]
pub fn create_art_library(
    ctx: State<'_, Arc<ServiceContext>>,
    path: String,
    name: String,
) -> Result<beambench_service::persist::LoadedArtLibrary, String> {
    beambench_service::ops::workflows::art_library::create_art_library(&ctx, path, name)
}
#[tauri::command]
pub fn load_art_library(
    ctx: State<'_, Arc<ServiceContext>>,
    path: String,
) -> Result<beambench_service::persist::LoadedArtLibrary, String> {
    beambench_service::ops::workflows::art_library::load_art_library(&ctx, path)
}
#[tauri::command]
pub fn unload_art_library(
    ctx: State<'_, Arc<ServiceContext>>,
    library_id: String,
) -> Result<(), String> {
    beambench_service::ops::workflows::art_library::unload_art_library(&ctx, library_id)
}
#[tauri::command]
pub fn save_art_library_as(
    ctx: State<'_, Arc<ServiceContext>>,
    library_id: String,
    path: String,
) -> Result<beambench_service::persist::LoadedArtLibrary, String> {
    beambench_service::ops::workflows::art_library::save_art_library_as(
        &ctx,
        library_id,
        path,
    )
}
#[tauri::command]
pub fn rename_art_library(
    ctx: State<'_, Arc<ServiceContext>>,
    library_id: String,
    name: String,
) -> Result<beambench_service::persist::LoadedArtLibrary, String> {
    beambench_service::ops::workflows::art_library::rename_art_library(
        &ctx,
        library_id,
        name,
    )
}
#[tauri::command]
pub fn delete_art_library(
    ctx: State<'_, Arc<ServiceContext>>,
    library_id: String,
) -> Result<(), String> {
    beambench_service::ops::workflows::art_library::delete_art_library(&ctx, library_id)
}
#[tauri::command]
pub fn add_art_library_item(
    ctx: State<'_, Arc<ServiceContext>>,
    library_id: String,
    name: String,
    category: String,
    tags: Vec<String>,
    file_path: String,
) -> Result<AddArtLibraryItemResult, String> {
    beambench_service::ops::workflows::art_library::add_art_library_item(
        &ctx,
        library_id,
        name,
        category,
        tags,
        file_path,
    )
}
#[tauri::command]
pub fn add_selection_to_art_library(
    ctx: State<'_, Arc<ServiceContext>>,
    library_id: String,
    object_ids: Vec<String>,
    name: String,
    category: String,
    tags: Vec<String>,
) -> Result<ArtLibraryItem, String> {
    beambench_service::ops::workflows::art_library::add_selection_to_art_library(
        &ctx,
        library_id,
        object_ids,
        name,
        category,
        tags,
    )
}
#[tauri::command]
pub fn rename_art_library_item(
    ctx: State<'_, Arc<ServiceContext>>,
    library_id: String,
    item_id: String,
    name: String,
) -> Result<beambench_service::persist::LoadedArtLibrary, String> {
    beambench_service::ops::workflows::art_library::rename_art_library_item(
        &ctx,
        library_id,
        item_id,
        name,
    )
}
#[tauri::command]
pub fn remove_art_library_item(
    ctx: State<'_, Arc<ServiceContext>>,
    library_id: String,
    item_id: String,
) -> Result<beambench_service::persist::LoadedArtLibrary, String> {
    beambench_service::ops::workflows::art_library::remove_art_library_item(
        &ctx,
        library_id,
        item_id,
    )
}
#[tauri::command]
pub fn commit_art_library_thumbnail(
    ctx: State<'_, Arc<ServiceContext>>,
    library_id: String,
    item_id: String,
    thumbnail: Option<String>,
) -> Result<beambench_service::persist::LoadedArtLibrary, String> {
    beambench_service::ops::workflows::art_library::commit_art_library_thumbnail(
        &ctx,
        library_id,
        item_id,
        thumbnail,
    )
}
#[tauri::command]
pub fn move_art_library_item(
    ctx: State<'_, Arc<ServiceContext>>,
    source_library_id: String,
    item_id: String,
    target_library_id: String,
    remove_source: bool,
) -> Result<(), String> {
    beambench_service::ops::workflows::art_library::move_art_library_item(
        &ctx,
        source_library_id,
        item_id,
        target_library_id,
        remove_source,
    )
}
#[tauri::command]
pub fn insert_art_library_item_to_project(
    ctx: State<'_, Arc<ServiceContext>>,
    library_id: String,
    item_id: String,
    layer_id: Option<String>,
    drop_x: Option<f64>,
    drop_y: Option<f64>,
) -> Result<Vec<ProjectObject>, String> {
    beambench_service::ops::workflows::art_library::insert_art_library_item_to_project(
        &ctx,
        library_id,
        item_id,
        layer_id,
        drop_x,
        drop_y,
    )
}
