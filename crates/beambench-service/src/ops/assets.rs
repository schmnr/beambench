use std::path::Path;

use beambench_core::{Asset, AssetId, AssetMediaType};

use crate::context::ServiceContext;
use crate::error::{ServiceError, ServiceResult};

fn lock_err(name: &str, e: impl std::fmt::Display) -> ServiceError {
    ServiceError::internal(format!("Failed to lock {name}: {e}"))
}

pub fn list_assets(ctx: &ServiceContext) -> ServiceResult<Vec<Asset>> {
    let project = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project
        .as_ref()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    Ok(project.assets.clone())
}

pub fn get_asset(ctx: &ServiceContext, asset_id: AssetId) -> ServiceResult<(Asset, Vec<u8>)> {
    let project = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project
        .as_ref()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    let asset = project
        .find_asset(asset_id)
        .cloned()
        .ok_or_else(|| ServiceError::not_found("Asset not found"))?;
    let data = project
        .get_asset_data(asset_id)
        .map(|d| d.to_vec())
        .ok_or_else(|| ServiceError::not_found("Asset data not found"))?;
    Ok((asset, data))
}

pub fn import_asset_from_path(
    ctx: &ServiceContext,
    file_path: &Path,
) -> ServiceResult<(Asset, usize)> {
    {
        let project = ctx.project.lock().map_err(|e| lock_err("project", e))?;
        if project.is_none() {
            return Err(ServiceError::not_found("No project open"));
        }
    }

    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let media_type = AssetMediaType::from_extension(ext)
        .ok_or_else(|| ServiceError::invalid_input(format!("Unsupported file type: {ext}")))?;

    let data = std::fs::read(file_path)
        .map_err(|e| ServiceError::persistence(format!("Failed to read file: {e}")))?;

    let filename = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let byte_size = data.len() as u64;
    let asset = Asset::new(&filename, media_type, byte_size, None, None);

    let mut project = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    project.add_asset(asset.clone(), data);
    project.dirty = true;
    Ok((asset, byte_size as usize))
}
