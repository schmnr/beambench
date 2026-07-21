use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine;
use beambench_core::vector::text_to_path::resolve_text_in_box;
use beambench_core::{
    ART_LIBRARY_FORMAT_VERSION, ART_LIBRARY_SNAPSHOT_MEDIA_TYPE, ArtLibraryItem,
    ArtLibraryItemKind, ArtLibrarySelectionSnapshot, ArtLibrarySnapshotAsset,
    ArtLibraryTextSourceMetadata, Asset, AssetId, AssetMediaType, Layer, ObjectData, ObjectId,
    OperationType, ProjectObject,
};
use beambench_service::ServiceContext;
use beambench_service::ops::{imports, planning};
use beambench_service::{RoutingTarget, resolve_layer_for_object};
use chrono::Utc;
use sha2::{Digest, Sha256};
use tauri::State;
use uuid::Uuid;

use super::project::parse_id;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddArtLibraryItemResult {
    pub item: ArtLibraryItem,
    pub duplicate: bool,
}

#[tauri::command]
pub fn get_art_libraries(
    ctx: State<'_, Arc<ServiceContext>>,
) -> Result<beambench_service::persist::ArtLibraryLoadState, String> {
    ctx.get_art_libraries()
}

#[tauri::command]
pub fn create_art_library(
    ctx: State<'_, Arc<ServiceContext>>,
    path: String,
    name: String,
) -> Result<beambench_service::persist::LoadedArtLibrary, String> {
    ctx.create_art_library(PathBuf::from(path), &name)
}

#[tauri::command]
pub fn load_art_library(
    ctx: State<'_, Arc<ServiceContext>>,
    path: String,
) -> Result<beambench_service::persist::LoadedArtLibrary, String> {
    ctx.load_art_library(PathBuf::from(path))
}

#[tauri::command]
pub fn unload_art_library(
    ctx: State<'_, Arc<ServiceContext>>,
    library_id: String,
) -> Result<(), String> {
    ctx.unload_art_library(parse_uuid(&library_id)?)
}

#[tauri::command]
pub fn save_art_library_as(
    ctx: State<'_, Arc<ServiceContext>>,
    library_id: String,
    path: String,
) -> Result<beambench_service::persist::LoadedArtLibrary, String> {
    ctx.save_art_library_as(parse_uuid(&library_id)?, PathBuf::from(path))
}

#[tauri::command]
pub fn rename_art_library(
    ctx: State<'_, Arc<ServiceContext>>,
    library_id: String,
    name: String,
) -> Result<beambench_service::persist::LoadedArtLibrary, String> {
    ctx.rename_art_library(parse_uuid(&library_id)?, &name)
}

#[tauri::command]
pub fn delete_art_library(
    ctx: State<'_, Arc<ServiceContext>>,
    library_id: String,
) -> Result<(), String> {
    ctx.delete_art_library(parse_uuid(&library_id)?)
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
    add_external_file_to_art_library(
        &ctx,
        parse_uuid(&library_id)?,
        name,
        category,
        tags,
        &file_path,
    )
}

fn add_external_file_to_art_library(
    ctx: &ServiceContext,
    library_id: Uuid,
    name: String,
    category: String,
    tags: Vec<String>,
    file_path: &str,
) -> Result<AddArtLibraryItemResult, String> {
    let data = std::fs::read(file_path).map_err(|e| format!("Failed to read file: {e}"))?;
    let media_type = guess_media_type(file_path);
    let encoded = base64::engine::general_purpose::STANDARD.encode(&data);

    let source_filename = Path::new(file_path)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let item = ArtLibraryItem {
        id: Uuid::new_v4(),
        kind: ArtLibraryItemKind::ExternalFile,
        name,
        category,
        tags,
        source_filename,
        media_type,
        data: encoded,
        thumbnail: None,
        created_at: Utc::now().to_rfc3339(),
    };

    let (item, duplicate) = ctx.add_art_library_item_dedupe_external(library_id, item)?;
    Ok(AddArtLibraryItemResult { item, duplicate })
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
    let parsed_ids = object_ids
        .iter()
        .map(|id| parse_id(id))
        .collect::<Result<Vec<_>, _>>()?;
    let snapshot = build_selection_snapshot(&ctx, &parsed_ids)?;
    let snapshot_json =
        serde_json::to_vec(&snapshot).map_err(|e| format!("Failed to encode snapshot: {e}"))?;
    let item = ArtLibraryItem {
        id: Uuid::new_v4(),
        kind: ArtLibraryItemKind::SelectionSnapshot,
        name,
        category,
        tags,
        source_filename: "Selection".to_string(),
        media_type: ART_LIBRARY_SNAPSHOT_MEDIA_TYPE.to_string(),
        data: base64::engine::general_purpose::STANDARD.encode(snapshot_json),
        thumbnail: None,
        created_at: Utc::now().to_rfc3339(),
    };
    ctx.add_art_library_item(parse_uuid(&library_id)?, item.clone())?;
    Ok(item)
}

#[tauri::command]
pub fn rename_art_library_item(
    ctx: State<'_, Arc<ServiceContext>>,
    library_id: String,
    item_id: String,
    name: String,
) -> Result<beambench_service::persist::LoadedArtLibrary, String> {
    ctx.rename_art_library_item(parse_uuid(&library_id)?, parse_uuid(&item_id)?, &name)
}

#[tauri::command]
pub fn remove_art_library_item(
    ctx: State<'_, Arc<ServiceContext>>,
    library_id: String,
    item_id: String,
) -> Result<beambench_service::persist::LoadedArtLibrary, String> {
    ctx.remove_art_library_item(parse_uuid(&library_id)?, parse_uuid(&item_id)?)
}

#[tauri::command]
pub fn commit_art_library_thumbnail(
    ctx: State<'_, Arc<ServiceContext>>,
    library_id: String,
    item_id: String,
    thumbnail: Option<String>,
) -> Result<beambench_service::persist::LoadedArtLibrary, String> {
    ctx.commit_art_library_thumbnail(parse_uuid(&library_id)?, parse_uuid(&item_id)?, thumbnail)
}

#[tauri::command]
pub fn move_art_library_item(
    ctx: State<'_, Arc<ServiceContext>>,
    source_library_id: String,
    item_id: String,
    target_library_id: String,
    remove_source: bool,
) -> Result<(), String> {
    ctx.copy_art_library_item(
        parse_uuid(&source_library_id)?,
        parse_uuid(&item_id)?,
        parse_uuid(&target_library_id)?,
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
    let library_id = parse_uuid(&library_id)?;
    let item_id = parse_uuid(&item_id)?;
    let layer_id = layer_id.as_deref().map(parse_id).transpose()?;
    let item = ctx.art_library_item(library_id, item_id)?;
    let drop_position = match (drop_x, drop_y) {
        (Some(x), Some(y)) => Some((x, y)),
        (None, None) => None,
        _ => return Err("dropX and dropY must be provided together".to_string()),
    };

    match item.kind {
        ArtLibraryItemKind::ExternalFile => {
            let requested_layer_id =
                resolve_insert_base_layer(&ctx, layer_id, base_operation_for_external_item(&item))?;
            insert_external_file_art_library_item(&ctx, requested_layer_id, &item, drop_position)
        }
        ArtLibraryItemKind::SelectionSnapshot => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&item.data)
                .map_err(|e| format!("Failed to decode snapshot item: {e}"))?;
            let snapshot = serde_json::from_slice::<ArtLibrarySelectionSnapshot>(&bytes)
                .map_err(|e| format!("Failed to parse snapshot item: {e}"))?;
            let requested_layer_id =
                resolve_insert_base_layer(&ctx, layer_id, base_operation_for_snapshot(&snapshot))?;
            insert_selection_snapshot(&ctx, requested_layer_id, snapshot, drop_position)
        }
    }
}

fn insert_external_file_art_library_item(
    ctx: &ServiceContext,
    requested_layer_id: beambench_core::LayerId,
    item: &ArtLibraryItem,
    drop_position: Option<(f64, f64)>,
) -> Result<Vec<ProjectObject>, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&item.data)
        .map_err(|e| format!("Failed to decode art library item: {e}"))?;
    let created = imports::import_art_library_item(
        ctx,
        requested_layer_id,
        &item.name,
        &item.source_filename,
        &item.media_type,
        bytes,
    )
    .map_err(|e| e.to_string())?;
    // `import_art_library_item()` releases the project lock and invalidates the
    // plan cache before returning. We reacquire the lock here only to apply the
    // final drop placement to the imported object ids.
    apply_drop_position_after_import(ctx, created, drop_position)
}

fn base_operation_for_external_item(item: &ArtLibraryItem) -> OperationType {
    match item.media_type.as_str() {
        "image/png" | "image/jpeg" | "image/gif" | "image/bmp" | "image/webp" | "image/tiff"
        | "image/x-tga" => OperationType::Image,
        _ => OperationType::Line,
    }
}

fn base_operation_for_snapshot(snapshot: &ArtLibrarySelectionSnapshot) -> OperationType {
    if snapshot
        .objects
        .iter()
        .all(|object| matches!(object.data, ObjectData::RasterImage { .. }))
    {
        OperationType::Image
    } else {
        OperationType::Line
    }
}

fn resolve_insert_base_layer(
    ctx: &ServiceContext,
    requested_layer_id: Option<beambench_core::LayerId>,
    default_operation: OperationType,
) -> Result<beambench_core::LayerId, String> {
    let mut project_guard = ctx
        .project
        .lock()
        .map_err(|e| format!("Failed to lock project: {e}"))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| "No project open".to_string())?;

    if let Some(layer_id) = requested_layer_id {
        if project.find_layer(layer_id).is_some() {
            return Ok(layer_id);
        }
    }

    if let Some(layer) = project.layers.first() {
        return Ok(layer.id);
    }

    let layer_id = match default_operation {
        OperationType::Image => {
            project
                .add_layer(Layer::new_single_entry("Image", OperationType::Image))
                .id
        }
        _ => project.ensure_default_layer(),
    };
    Ok(layer_id)
}

fn build_selection_snapshot(
    ctx: &ServiceContext,
    selected_ids: &[ObjectId],
) -> Result<ArtLibrarySelectionSnapshot, String> {
    let project_guard = ctx
        .project
        .lock()
        .map_err(|e| format!("Failed to lock project: {e}"))?;
    let project = project_guard
        .as_ref()
        .ok_or_else(|| "No project open".to_string())?;

    let expanded_ids = expand_snapshot_selection(project, selected_ids)?;
    let mut asset_hashes = HashSet::new();
    let mut assets = Vec::new();
    let mut source_text_metadata = Vec::new();
    let mut objects = Vec::new();

    for object_id in expanded_ids {
        let object = project
            .find_object(object_id)
            .ok_or_else(|| format!("Object '{object_id}' not found"))?;
        let flattened = flatten_snapshot_object(
            project,
            object,
            &mut source_text_metadata,
            &mut asset_hashes,
            &mut assets,
        )?;
        objects.push(flattened);
    }

    let mut layer_ids = HashSet::new();
    for object in &objects {
        layer_ids.insert(object.layer_id);
    }
    let layer_templates = project
        .layers
        .iter()
        .filter(|layer| layer_ids.contains(&layer.id))
        .cloned()
        .collect::<Vec<_>>();

    Ok(ArtLibrarySelectionSnapshot {
        format_version: ART_LIBRARY_FORMAT_VERSION.to_string(),
        objects,
        layer_templates,
        assets,
        source_text_metadata,
    })
}

fn expand_snapshot_selection(
    project: &beambench_core::Project,
    selected_ids: &[ObjectId],
) -> Result<Vec<ObjectId>, String> {
    let mut wanted = HashSet::new();
    for &id in selected_ids {
        collect_snapshot_object_ids(project, id, &mut wanted)?;
    }
    Ok(project
        .objects
        .iter()
        .filter(|object| wanted.contains(&object.id))
        .map(|object| object.id)
        .collect())
}

fn collect_snapshot_object_ids(
    project: &beambench_core::Project,
    object_id: ObjectId,
    wanted: &mut HashSet<ObjectId>,
) -> Result<(), String> {
    if !wanted.insert(object_id) {
        return Ok(());
    }
    let object = project
        .find_object(object_id)
        .ok_or_else(|| format!("Object '{object_id}' not found"))?;
    if let ObjectData::Group { children } = &object.data {
        for child in children {
            collect_snapshot_object_ids(project, *child, wanted)?;
        }
    }
    Ok(())
}

fn flatten_snapshot_object(
    project: &beambench_core::Project,
    object: &ProjectObject,
    source_text_metadata: &mut Vec<ArtLibraryTextSourceMetadata>,
    asset_hashes: &mut HashSet<String>,
    assets: &mut Vec<ArtLibrarySnapshotAsset>,
) -> Result<ProjectObject, String> {
    let mut flattened = project
        .resolve_clone(object)
        .unwrap_or_else(|| object.clone());
    match &flattened.data {
        ObjectData::Text {
            content,
            font_family,
            font_size_mm,
            alignment,
            alignment_v,
            bold,
            italic,
            upper_case,
            h_spacing,
            v_spacing,
            resolved_path_data,
            ..
        } => {
            source_text_metadata.push(ArtLibraryTextSourceMetadata {
                object_id: flattened.id,
                content: content.clone(),
                font_family: font_family.clone(),
                font_size_mm: *font_size_mm,
                bold: *bold,
                italic: *italic,
                alignment: *alignment,
                alignment_v: *alignment_v,
                upper_case: *upper_case,
            });
            let path_data = resolved_path_data.clone().or_else(|| {
                resolve_text_in_box(
                    content,
                    font_family,
                    *font_size_mm,
                    *bold,
                    *italic,
                    *alignment,
                    *alignment_v,
                    *upper_case,
                    *h_spacing,
                    *v_spacing,
                    flattened.bounds.width(),
                    flattened.bounds.height(),
                )
                .map(|resolved| resolved.path.to_svg_d())
            });
            let Some(path_data) = path_data else {
                return Err(format!(
                    "Could not flatten text object '{}' into geometry",
                    flattened.name
                ));
            };
            flattened.data = ObjectData::VectorPath {
                closed: path_data.contains('Z') || path_data.contains('z'),
                path_data,
                ruler_guide_axis: None,
            };
        }
        ObjectData::RasterImage {
            asset_key,
            original_width_px: _,
            original_height_px: _,
            adjustments: _,
            ..
        } => {
            let asset_id = Uuid::parse_str(asset_key)
                .map_err(|e| format!("Invalid raster asset key '{asset_key}': {e}"))?;
            let asset = project
                .find_asset(AssetId::from_uuid(asset_id))
                .ok_or_else(|| format!("Raster asset '{asset_key}' not found"))?;
            let bytes = project
                .get_asset_data(asset.id)
                .ok_or_else(|| format!("Raster bytes missing for asset '{}'", asset.id))?;
            let hash = hash_bytes(bytes);
            if asset_hashes.insert(hash.clone()) {
                assets.push(ArtLibrarySnapshotAsset {
                    hash: hash.clone(),
                    media_type: asset_media_type_to_mime(asset.media_type).to_string(),
                    data: base64::engine::general_purpose::STANDARD.encode(bytes),
                });
            }
            if let ObjectData::RasterImage { asset_key, .. } = &mut flattened.data {
                *asset_key = hash;
            }
        }
        _ => {}
    }
    Ok(flattened)
}

fn insert_selection_snapshot(
    ctx: &ServiceContext,
    requested_layer_id: beambench_core::LayerId,
    snapshot: ArtLibrarySelectionSnapshot,
    drop_position: Option<(f64, f64)>,
) -> Result<Vec<ProjectObject>, String> {
    let mut project_guard = ctx
        .project
        .lock()
        .map_err(|e| format!("Failed to lock project: {e}"))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| "No project open".to_string())?;

    if project.find_layer(requested_layer_id).is_none() {
        return Err("Layer not found".to_string());
    }

    ctx.push_project_undo_snapshot(project)?;

    let mut asset_key_map = HashMap::new();
    for asset in &snapshot.assets {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&asset.data)
            .map_err(|e| format!("Failed to decode snapshot asset: {e}"))?;
        let media_type = mime_to_asset_media_type(&asset.media_type).ok_or_else(|| {
            format!(
                "Unsupported snapshot asset media type '{}'",
                asset.media_type
            )
        })?;
        let (width_px, height_px) = image::load_from_memory(&bytes)
            .map(|decoded| (Some(decoded.width()), Some(decoded.height())))
            .unwrap_or((None, None));
        let asset_record = Asset::new(
            format!(
                "snapshot-{}.{}",
                &asset.hash[..8.min(asset.hash.len())],
                media_type.extension()
            ),
            media_type,
            bytes.len() as u64,
            width_px,
            height_px,
        );
        let asset_id = asset_record.id;
        project.add_asset(asset_record, bytes);
        asset_key_map.insert(asset.hash.clone(), asset_id.to_string());
    }

    let mut id_map = HashMap::new();
    for object in &snapshot.objects {
        id_map.insert(object.id, beambench_core::ObjectId::new());
    }

    let mut created = Vec::new();
    let mut created_ids = Vec::new();

    for object in &snapshot.objects {
        let mut copy = object.clone();
        copy.id = *id_map
            .get(&object.id)
            .ok_or_else(|| format!("Missing remapped id for '{}'", object.id))?;
        copy.layer_id = resolve_snapshot_layer(project, requested_layer_id, &copy.data)?;
        match &mut copy.data {
            ObjectData::Group { children } => {
                for child in children.iter_mut() {
                    if let Some(mapped) = id_map.get(child) {
                        *child = *mapped;
                    }
                }
            }
            ObjectData::RasterImage { asset_key, .. } => {
                if let Some(mapped) = asset_key_map.get(asset_key) {
                    *asset_key = mapped.clone();
                }
            }
            _ => {}
        }
        created_ids.push(copy.id);
        created.push(project.add_object(copy).clone());
    }

    apply_drop_position(project, &created_ids, &mut created, drop_position);

    drop(project_guard);
    planning::invalidate_plan_cache(ctx).map_err(|e| e.to_string())?;
    Ok(created)
}

fn apply_drop_position_after_import(
    ctx: &ServiceContext,
    mut created: Vec<ProjectObject>,
    drop_position: Option<(f64, f64)>,
) -> Result<Vec<ProjectObject>, String> {
    let created_ids: Vec<_> = created.iter().map(|object| object.id).collect();
    if created_ids.is_empty() || drop_position.is_none() {
        return Ok(created);
    }

    let mut project_guard = ctx
        .project
        .lock()
        .map_err(|e| format!("Failed to lock project: {e}"))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| "No project open".to_string())?;
    // The import path already invalidated the Rust plan cache. This helper only
    // finalizes the returned object positions before the frontend refreshes the
    // preview from the updated project state.
    apply_drop_position(project, &created_ids, &mut created, drop_position);
    Ok(created)
}

fn apply_drop_position(
    project: &mut beambench_core::Project,
    created_ids: &[beambench_core::ObjectId],
    created: &mut Vec<ProjectObject>,
    drop_position: Option<(f64, f64)>,
) {
    let Some((x, y)) = drop_position else {
        return;
    };
    let Some(min_x) = created
        .iter()
        .map(|object| object.bounds.min.x)
        .reduce(f64::min)
    else {
        return;
    };
    let Some(min_y) = created
        .iter()
        .map(|object| object.bounds.min.y)
        .reduce(f64::min)
    else {
        return;
    };

    // The drop point anchors the combined inserted artwork by its min-bounds
    // corner, preserving the relative offsets between created objects. This
    // applies equally to snapshots and external-file inserts.
    beambench_core::operations::nudge_objects(project, created_ids, x - min_x, y - min_y);
    *created = created_ids
        .iter()
        .filter_map(|id| project.find_object(*id).cloned())
        .collect();
}

fn resolve_snapshot_layer(
    project: &mut beambench_core::Project,
    requested_layer_id: beambench_core::LayerId,
    data: &ObjectData,
) -> Result<beambench_core::LayerId, String> {
    let target = match data {
        ObjectData::RasterImage { .. } => RoutingTarget::NeedsImage,
        _ => RoutingTarget::NeedsNonImage,
    };
    resolve_layer_for_object(project, requested_layer_id, target)
        .map(|(layer_id, _)| layer_id)
        .map_err(|e| e.to_string())
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn parse_uuid(raw: &str) -> Result<Uuid, String> {
    Uuid::parse_str(raw).map_err(|e| format!("Invalid UUID '{raw}': {e}"))
}

fn asset_media_type_to_mime(media_type: AssetMediaType) -> &'static str {
    match media_type {
        AssetMediaType::Png => "image/png",
        AssetMediaType::Jpeg => "image/jpeg",
        AssetMediaType::Bmp => "image/bmp",
        AssetMediaType::Gif => "image/gif",
        AssetMediaType::Tiff => "image/tiff",
        AssetMediaType::Webp => "image/webp",
        AssetMediaType::Tga => "image/x-tga",
        AssetMediaType::Svg => "image/svg+xml",
        AssetMediaType::Dxf => "application/dxf",
        AssetMediaType::Ai => "application/ai",
        AssetMediaType::Pdf => "application/pdf",
        AssetMediaType::Eps => "application/postscript",
    }
}

fn mime_to_asset_media_type(media_type: &str) -> Option<AssetMediaType> {
    match media_type {
        "image/png" => Some(AssetMediaType::Png),
        "image/jpeg" => Some(AssetMediaType::Jpeg),
        "image/bmp" => Some(AssetMediaType::Bmp),
        "image/gif" => Some(AssetMediaType::Gif),
        "image/tiff" => Some(AssetMediaType::Tiff),
        "image/webp" => Some(AssetMediaType::Webp),
        "image/x-tga" => Some(AssetMediaType::Tga),
        _ => None,
    }
}

fn guess_media_type(path: &str) -> String {
    let lower = path.to_lowercase();
    if lower.ends_with(".svg") {
        "image/svg+xml".to_string()
    } else if lower.ends_with(".png") {
        "image/png".to_string()
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg".to_string()
    } else if lower.ends_with(".gif") {
        "image/gif".to_string()
    } else if lower.ends_with(".bmp") {
        "image/bmp".to_string()
    } else if lower.ends_with(".webp") {
        "image/webp".to_string()
    } else if lower.ends_with(".tif") || lower.ends_with(".tiff") {
        "image/tiff".to_string()
    } else if lower.ends_with(".tga") {
        "image/x-tga".to_string()
    } else if lower.ends_with(".dxf") {
        "application/dxf".to_string()
    } else if lower.ends_with(".pdf") {
        "application/pdf".to_string()
    } else if lower.ends_with(".ai") {
        "application/ai".to_string()
    } else if lower.ends_with(".eps") {
        "application/postscript".to_string()
    } else {
        "application/octet-stream".to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use base64::Engine;
    use beambench_common::{Bounds, Point2D};
    use beambench_core::{
        AppSettings, ArtLibraryItem, ArtLibraryItemKind, Asset, AssetMediaType, Layer, ObjectData,
        OperationType, Project, ProjectObject, TextAlignment, TextAlignmentV,
    };
    use beambench_service::ServiceContext;
    use tempfile::tempdir;

    use super::{
        add_external_file_to_art_library, build_selection_snapshot, guess_media_type,
        insert_external_file_art_library_item, insert_selection_snapshot,
        resolve_insert_base_layer,
    };

    fn png_bytes() -> Vec<u8> {
        base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/aMcAAAAASUVORK5CYII=")
            .unwrap()
    }

    fn seeded_context_with_project(project: Project) -> ServiceContext {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        *ctx.project.lock().unwrap() = Some(project);
        ctx
    }

    fn make_text_object(layer_id: beambench_core::LayerRef) -> ProjectObject {
        ProjectObject::new(
            "Label",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 5.0)),
            ObjectData::Text {
                content: "Hello".to_string(),
                font_family: "Arial".to_string(),
                font_size_mm: 5.0,
                alignment: TextAlignment::Left,
                alignment_v: TextAlignmentV::Top,
                bold: false,
                italic: false,
                upper_case: false,
                welded: false,
                h_spacing: 0.0,
                v_spacing: 0.0,
                on_path: false,
                path_offset: 0.0,
                distort: false,
                layout_mode: beambench_core::TextLayoutMode::Straight,
                rtl: false,
                bend_radius: 0.0,
                transform_style: beambench_core::TextTransformStyle::None,
                transform_curve: 0.0,
                circle_placement: beambench_core::TextCirclePlacement::TopOutside,
                max_width: None,
                squeeze: false,
                ignore_empty_vars: false,
                resolved_font_source: None,
                resolved_font_key: None,
                resolved_path_data: Some("M0 0 L10 0 L10 5 L0 5 Z".to_string()),
                missing_font: false,
                missing_glyphs: Vec::new(),
                guide_path_id: None,
                variable_text: None,
            },
        )
    }

    #[test]
    fn guess_media_type_supports_tiff_extensions() {
        assert_eq!(guess_media_type("artwork.tif"), "image/tiff");
        assert_eq!(guess_media_type("artwork.TIFF"), "image/tiff");
    }

    #[test]
    fn guess_media_type_supports_tga_extension() {
        assert_eq!(guess_media_type("artwork.tga"), "image/x-tga");
    }

    #[test]
    fn selection_snapshot_flattens_clone_text_and_dedupes_raster_assets() {
        let mut project = Project::new("Snapshot Source");
        let line_layer = project.ensure_default_layer();
        let mut image_layer = Layer::new_single_entry("Image", OperationType::Image);
        image_layer.order_index = 1;
        let image_layer_id = project.add_layer(image_layer).id;

        let bytes = png_bytes();
        let asset = Asset::new(
            "shared.png",
            AssetMediaType::Png,
            bytes.len() as u64,
            Some(1),
            Some(1),
        );
        let asset_id = asset.id;
        project.add_asset(asset, bytes);

        let raster_one = project
            .add_object(ProjectObject::new(
                "Raster 1",
                image_layer_id,
                Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(5.0, 5.0)),
                ObjectData::RasterImage {
                    asset_key: asset_id.to_string(),
                    original_width_px: 1,
                    original_height_px: 1,
                    adjustments: None,
                    masks: Vec::new(),
                },
            ))
            .clone();
        let raster_two = project
            .add_object(ProjectObject::new(
                "Raster 2",
                image_layer_id,
                Bounds::new(Point2D::new(6.0, 0.0), Point2D::new(11.0, 5.0)),
                ObjectData::RasterImage {
                    asset_key: asset_id.to_string(),
                    original_width_px: 1,
                    original_height_px: 1,
                    adjustments: None,
                    masks: Vec::new(),
                },
            ))
            .clone();
        let group = project
            .add_object(ProjectObject::new(
                "Group",
                line_layer,
                Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(11.0, 5.0)),
                ObjectData::Group {
                    children: vec![raster_one.id, raster_two.id],
                },
            ))
            .clone();
        let text = project.add_object(make_text_object(line_layer)).clone();
        let clone = project
            .add_object(ProjectObject::new(
                "Clone",
                line_layer,
                Bounds::new(Point2D::new(12.0, 0.0), Point2D::new(22.0, 5.0)),
                ObjectData::VirtualClone { source_id: text.id },
            ))
            .clone();

        let ctx = seeded_context_with_project(project);
        let snapshot = build_selection_snapshot(&ctx, &[group.id, clone.id]).unwrap();

        assert_eq!(
            snapshot.assets.len(),
            1,
            "shared raster asset should be deduped by content hash"
        );
        assert_eq!(snapshot.source_text_metadata.len(), 1);
        assert_eq!(snapshot.source_text_metadata[0].content, "Hello");
        assert_eq!(snapshot.source_text_metadata[0].object_id, clone.id);

        let cloned_text = snapshot
            .objects
            .iter()
            .find(|object| object.id == clone.id)
            .expect("clone should be part of the snapshot");
        assert!(matches!(cloned_text.data, ObjectData::VectorPath { .. }));

        let mut raster_asset_keys = snapshot
            .objects
            .iter()
            .filter_map(|object| match &object.data {
                ObjectData::RasterImage { asset_key, .. } => Some(asset_key.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        raster_asset_keys.sort();
        raster_asset_keys.dedup();
        assert_eq!(raster_asset_keys, vec![snapshot.assets[0].hash.clone()]);
    }

    #[test]
    fn insert_selection_snapshot_remaps_groups_and_restores_raster_assets() {
        let mut source = Project::new("Snapshot Source");
        let line_layer = source.ensure_default_layer();
        let mut image_layer = Layer::new_single_entry("Image", OperationType::Image);
        image_layer.order_index = 1;
        let image_layer_id = source.add_layer(image_layer).id;

        let bytes = png_bytes();
        let asset = Asset::new(
            "shared.png",
            AssetMediaType::Png,
            bytes.len() as u64,
            Some(1),
            Some(1),
        );
        let asset_id = asset.id;
        source.add_asset(asset, bytes);

        let raster_one = source
            .add_object(ProjectObject::new(
                "Raster 1",
                image_layer_id,
                Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(5.0, 5.0)),
                ObjectData::RasterImage {
                    asset_key: asset_id.to_string(),
                    original_width_px: 1,
                    original_height_px: 1,
                    adjustments: None,
                    masks: Vec::new(),
                },
            ))
            .clone();
        let raster_two = source
            .add_object(ProjectObject::new(
                "Raster 2",
                image_layer_id,
                Bounds::new(Point2D::new(6.0, 0.0), Point2D::new(11.0, 5.0)),
                ObjectData::RasterImage {
                    asset_key: asset_id.to_string(),
                    original_width_px: 1,
                    original_height_px: 1,
                    adjustments: None,
                    masks: Vec::new(),
                },
            ))
            .clone();
        let group = source
            .add_object(ProjectObject::new(
                "Group",
                line_layer,
                Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(11.0, 5.0)),
                ObjectData::Group {
                    children: vec![raster_one.id, raster_two.id],
                },
            ))
            .clone();
        let text = source.add_object(make_text_object(line_layer)).clone();
        let clone = source
            .add_object(ProjectObject::new(
                "Clone",
                line_layer,
                Bounds::new(Point2D::new(12.0, 0.0), Point2D::new(22.0, 5.0)),
                ObjectData::VirtualClone { source_id: text.id },
            ))
            .clone();

        let source_ctx = seeded_context_with_project(source);
        let snapshot = build_selection_snapshot(&source_ctx, &[group.id, clone.id]).unwrap();

        let mut destination = Project::new("Snapshot Dest");
        let requested_layer = destination.ensure_default_layer();
        destination.add_layer(Layer::new_single_entry("Image", OperationType::Image));

        let ctx = seeded_context_with_project(destination);
        let created = insert_selection_snapshot(&ctx, requested_layer, snapshot, None).unwrap();

        let project_guard = ctx.project.lock().unwrap();
        let project = project_guard.as_ref().unwrap();
        assert_eq!(
            project.assets.len(),
            1,
            "deduped snapshot assets should restore once"
        );

        let created_ids = created
            .iter()
            .map(|object| object.id)
            .collect::<std::collections::HashSet<_>>();
        let group_object = created
            .iter()
            .find(|object| matches!(object.data, ObjectData::Group { .. }))
            .expect("group should be restored");
        let ObjectData::Group { children } = &group_object.data else {
            unreachable!();
        };
        assert_eq!(children.len(), 2);
        assert!(children.iter().all(|child| created_ids.contains(child)));

        let raster_keys = created
            .iter()
            .filter_map(|object| match &object.data {
                ObjectData::RasterImage { asset_key, .. } => Some(asset_key.clone()),
                _ => None,
            })
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(
            raster_keys.len(),
            1,
            "restored rasters should share the recreated asset"
        );
        let raster_key = raster_keys.into_iter().next().unwrap();
        assert!(uuid::Uuid::parse_str(&raster_key).is_ok());

        let inserted_text = created
            .iter()
            .find(|object| object.name == "Clone")
            .expect("flattened clone should be restored");
        assert!(matches!(inserted_text.data, ObjectData::VectorPath { .. }));
    }

    #[test]
    fn insert_selection_snapshot_anchors_combined_min_bounds_at_drop_point() {
        let mut destination = Project::new("Snapshot Dest");
        let requested_layer = destination.ensure_default_layer();
        let ctx = seeded_context_with_project(destination);

        let object_a = ProjectObject::new(
            "A",
            requested_layer,
            Bounds::new(Point2D::new(10.0, 20.0), Point2D::new(20.0, 30.0)),
            ObjectData::VectorPath {
                path_data: "M10 20 L20 20 L20 30 L10 30 Z".to_string(),
                closed: true,
                ruler_guide_axis: None,
            },
        );
        let object_b = ProjectObject::new(
            "B",
            requested_layer,
            Bounds::new(Point2D::new(25.0, 35.0), Point2D::new(30.0, 40.0)),
            ObjectData::VectorPath {
                path_data: "M25 35 L30 35 L30 40 L25 40 Z".to_string(),
                closed: true,
                ruler_guide_axis: None,
            },
        );
        let snapshot = beambench_core::ArtLibrarySelectionSnapshot {
            format_version: beambench_core::ART_LIBRARY_FORMAT_VERSION.to_string(),
            objects: vec![object_a, object_b],
            layer_templates: vec![],
            assets: vec![],
            source_text_metadata: vec![],
        };

        let created =
            insert_selection_snapshot(&ctx, requested_layer, snapshot, Some((100.0, 200.0)))
                .unwrap();

        let bounds = created.iter().fold(
            Bounds::new(
                Point2D::new(f64::INFINITY, f64::INFINITY),
                Point2D::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
            ),
            |acc, object| {
                Bounds::new(
                    Point2D::new(
                        acc.min.x.min(object.bounds.min.x),
                        acc.min.y.min(object.bounds.min.y),
                    ),
                    Point2D::new(
                        acc.max.x.max(object.bounds.max.x),
                        acc.max.y.max(object.bounds.max.y),
                    ),
                )
            },
        );

        assert_eq!(bounds.min, Point2D::new(100.0, 200.0));
        let moved_a = created.iter().find(|object| object.name == "A").unwrap();
        let moved_b = created.iter().find(|object| object.name == "B").unwrap();
        assert_eq!(moved_a.bounds.min, Point2D::new(100.0, 200.0));
        assert_eq!(moved_b.bounds.min, Point2D::new(115.0, 215.0));
    }

    #[test]
    fn resolve_insert_base_layer_uses_first_existing_layer_when_none_requested() {
        let mut project = Project::new("Has Layer");
        let first = project.ensure_default_layer();
        let ctx = seeded_context_with_project(project);

        let resolved = resolve_insert_base_layer(&ctx, None, OperationType::Line).unwrap();

        assert_eq!(resolved, first);
    }

    #[test]
    fn resolve_insert_base_layer_creates_image_layer_for_empty_raster_project() {
        let project = Project::new("Empty Raster");
        let ctx = seeded_context_with_project(project);

        let resolved = resolve_insert_base_layer(&ctx, None, OperationType::Image).unwrap();

        let project_guard = ctx.project.lock().unwrap();
        let project = project_guard.as_ref().unwrap();
        let layer = project
            .find_layer(resolved)
            .expect("created image layer should exist");
        assert_eq!(layer.primary_entry().operation, OperationType::Image);
    }

    #[test]
    fn resolve_insert_base_layer_creates_default_line_layer_for_empty_vector_project() {
        let project = Project::new("Empty Vector");
        let ctx = seeded_context_with_project(project);

        let resolved = resolve_insert_base_layer(&ctx, None, OperationType::Line).unwrap();

        let project_guard = ctx.project.lock().unwrap();
        let project = project_guard.as_ref().unwrap();
        let layer = project
            .find_layer(resolved)
            .expect("default line layer should exist");
        assert_eq!(layer.primary_entry().operation, OperationType::Line);
    }

    #[test]
    fn insert_external_file_art_library_item_anchors_min_bounds_at_drop_point() {
        let mut project = Project::new("Art Library Dest");
        let requested_layer = project.ensure_default_layer();
        let ctx = seeded_context_with_project(project);
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="5" viewBox="0 0 10 5"><rect x="0" y="0" width="10" height="5"/></svg>"#;
        let item = ArtLibraryItem {
            id: uuid::Uuid::new_v4(),
            kind: ArtLibraryItemKind::ExternalFile,
            name: "Star".to_string(),
            category: "General".to_string(),
            tags: vec![],
            source_filename: "star.svg".to_string(),
            media_type: "image/svg+xml".to_string(),
            data: base64::engine::general_purpose::STANDARD.encode(svg),
            thumbnail: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        };

        let created =
            insert_external_file_art_library_item(&ctx, requested_layer, &item, Some((42.0, 24.0)))
                .unwrap();

        assert_eq!(created.len(), 1);
        assert_eq!(created[0].bounds.min, Point2D::new(42.0, 24.0));
    }

    #[test]
    fn add_external_file_to_art_library_returns_existing_item_for_duplicate_bytes() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let dir = tempdir().unwrap();
        let library_path = dir.path().join("Shapes.bbart");
        let file_path = dir.path().join("star.png");
        fs::write(&file_path, png_bytes()).unwrap();

        let library = ctx
            .create_art_library(library_path.clone(), "Shapes")
            .expect("library should be created");
        let library_id = library.document.library_id;

        let first = add_external_file_to_art_library(
            &ctx,
            library_id,
            "Star".to_string(),
            "General".to_string(),
            vec![],
            file_path.to_str().unwrap(),
        )
        .expect("first import should succeed");
        let second = add_external_file_to_art_library(
            &ctx,
            library_id,
            "Star Copy".to_string(),
            "General".to_string(),
            vec![],
            file_path.to_str().unwrap(),
        )
        .expect("duplicate import should resolve to existing item");

        assert!(!first.duplicate);
        assert!(second.duplicate);
        assert_eq!(second.item.id, first.item.id);

        let libraries = ctx.get_art_libraries().unwrap();
        let loaded = libraries
            .libraries
            .into_iter()
            .find(|entry| entry.document.library_id == library_id)
            .expect("library should remain loaded");
        assert_eq!(loaded.document.items.len(), 1);
    }
}
