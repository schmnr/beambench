use std::path::{Path, PathBuf};

use beambench_common::PALETTE_COLORS;
use beambench_core::{AssetId, Layer, OperationType, Project};
use beambench_project::{
    RecoveryInfo, check_recovery, discard_recovery, load_project, load_recovery,
    load_recovery_source, save_project, save_recovery, save_recovery_source,
};
use serde_json::json;

use crate::context::ServiceContext;
use crate::error::{ServiceError, ServiceResult};
use crate::events;
use crate::persist::persist_settings_to_disk;

fn project_name_from_path(path: &Path) -> Option<String> {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().trim().to_string())
        .filter(|name| !name.is_empty())
}

fn unique_migrated_layer_name(project: &Project, base: &str, operation: OperationType) -> String {
    let mode = match operation {
        OperationType::Image => "Image",
        _ => "Line",
    };
    let preferred = format!("{base} ({mode})");
    if !project.layers.iter().any(|layer| layer.name == preferred) {
        return preferred;
    }
    for suffix in 2.. {
        let candidate = format!("{preferred} {suffix}");
        if !project.layers.iter().any(|layer| layer.name == candidate) {
            return candidate;
        }
    }
    unreachable!()
}

/// Build the sibling layer for migrated content. It keeps the source layer's
/// on/off, visibility and output state, so loading a project never makes
/// disabled artwork burn.
fn migrated_sibling_layer(source: &Layer, name: &str, operation: OperationType) -> Layer {
    let mut layer = Layer::new(name, operation);
    layer.color_tag = source.color_tag.clone();
    layer.enabled = source.enabled;
    layer.visible = source.visible;
    layer.fill_opacity = source.fill_opacity;
    let from = source.primary_entry();
    let entry = layer.primary_entry_mut();
    entry.speed_mm_min = from.speed_mm_min;
    entry.power_percent = from.power_percent;
    entry.power_min_percent = from.power_min_percent;
    entry.air_assist = from.air_assist;
    entry.z_offset_mm = from.z_offset_mm;
    entry.gcode_prefix = from.gcode_prefix.clone();
    entry.gcode_suffix = from.gcode_suffix.clone();
    entry.output_enabled = from.output_enabled;
    layer
}

/// Split any layer holding mixed raster/vector content into two
/// sibling layers, one per content type. Called on project load so
/// legacy projects self-heal to match the "raster and vector content
/// live on separate layers" invariant enforced by all write paths.
///
/// Direction 1: a non-image layer containing raster objects → create
///   a sibling image layer (same color_tag and clean family name),
///   re-home the rasters.
/// Direction 2: an image layer containing non-raster objects → create
///   a sibling Line layer (same color_tag and clean family name),
///   re-home the vectors.
///
/// Idempotent: running twice on the same project is a no-op (after the
/// first pass there are no mixed layers left).
fn migrate_mixed_layers(project: &mut Project) -> Vec<String> {
    let mut warnings: Vec<String> = Vec::new();

    // Snapshot layer ids so we can iterate without borrow conflicts
    // while we mutate `project.layers` and `project.objects`.
    let layer_ids: Vec<_> = project.layers.iter().map(|l| l.id).collect();

    for layer_id in layer_ids {
        // Re-fetch the layer each iteration in case earlier passes
        // added siblings that we also need to examine. (They're
        // appended to the end of the vec, so they'll be visited by
        // the outer pass? No — layer_ids was snapshotted. Fresh
        // layers created here skip the scan, which is correct: they
        // hold only the migrated content and don't need splitting.)
        let source = {
            let Some(l) = project.layers.iter().find(|l| l.id == layer_id) else {
                continue;
            };
            if l.is_tool_layer || beambench_common::is_tool_color(&l.color_tag.0) {
                continue;
            }
            l.clone()
        };
        let layer_is_image = source.primary_entry().operation == OperationType::Image;
        let layer_name = source.name.clone();

        // Collect object ids on this layer bucketed by raster vs
        // vector. Uses `effective_is_raster` so a VirtualClone
        // pointing at a RasterImage source is classified as raster —
        // otherwise migration would re-home raster clones onto a
        // non-image layer that the planner will still treat as
        // raster at build time.
        let mut raster_ids: Vec<_> = Vec::new();
        let mut vector_ids: Vec<_> = Vec::new();
        for obj in &project.objects {
            if obj.layer_id != layer_id {
                continue;
            }
            if crate::validation::effective_is_raster(&obj.data, project) {
                raster_ids.push(obj.id);
            } else {
                vector_ids.push(obj.id);
            }
        }

        if layer_is_image && !vector_ids.is_empty() {
            // Image layer with vectors → create a sibling Line layer.
            let base = crate::validation::strip_mode_suffix(&layer_name);
            let sibling_name = unique_migrated_layer_name(project, base, OperationType::Line);
            let new_layer = migrated_sibling_layer(&source, &sibling_name, OperationType::Line);
            let new_id = new_layer.id;
            project.layers.push(new_layer);
            for obj in project.objects.iter_mut() {
                if vector_ids.contains(&obj.id) {
                    obj.layer_id = new_id;
                }
            }
            warnings.push(format!(
                "Migrated {} non-raster object(s) off image layer '{}' into new sibling '{sibling_name}'",
                vector_ids.len(),
                layer_name,
            ));
        } else if !layer_is_image && !raster_ids.is_empty() {
            // Non-image layer with rasters → create a sibling Image layer.
            let base = crate::validation::strip_mode_suffix(&layer_name);
            let sibling_name = unique_migrated_layer_name(project, base, OperationType::Image);
            let new_layer = migrated_sibling_layer(&source, &sibling_name, OperationType::Image);
            let new_id = new_layer.id;
            project.layers.push(new_layer);
            for obj in project.objects.iter_mut() {
                if raster_ids.contains(&obj.id) {
                    obj.layer_id = new_id;
                }
            }
            warnings.push(format!(
                "Migrated {} raster object(s) off layer '{}' into new sibling '{sibling_name}'",
                raster_ids.len(),
                layer_name,
            ));
        }
    }

    warnings
}

fn normalize_color_tag(hex: &str) -> String {
    let h = hex.to_lowercase();
    if h.len() == 9 && h.starts_with('#') {
        h[..7].to_string()
    } else {
        h
    }
}

/// Destructively normalize legacy tool-color sibling layers into one canonical T1/T2 layer.
fn migrate_tool_layers(project: &mut Project) -> Vec<String> {
    let mut warnings = Vec::new();
    let mut changed = false;

    for tool_color in PALETTE_COLORS.iter().filter(|p| p.is_tool_layer) {
        let target = normalize_color_tag(tool_color.hex);
        let matching_indices: Vec<usize> = project
            .layers
            .iter()
            .enumerate()
            .filter(|(_, layer)| normalize_color_tag(&layer.color_tag.0) == target)
            .map(|(idx, _)| idx)
            .collect();

        let Some(&canonical_index) = matching_indices.first() else {
            continue;
        };
        let canonical_id = project.layers[canonical_index].id;
        let duplicate_ids: Vec<_> = matching_indices
            .iter()
            .skip(1)
            .map(|idx| project.layers[*idx].id)
            .collect();

        for object in &mut project.objects {
            if duplicate_ids.contains(&object.layer_id) {
                object.layer_id = canonical_id;
            }
        }

        let canonical = &mut project.layers[canonical_index];
        let canonical_name = tool_color.name.replace("Tool ", "T");
        let already_canonical = duplicate_ids.is_empty()
            && canonical.name == canonical_name
            && canonical.color_tag.0 == tool_color.hex
            && canonical.is_tool_layer
            && canonical.entries.len() == 1
            && canonical.primary_entry().operation == OperationType::Tool
            && !canonical.primary_entry().output_enabled;
        if !already_canonical {
            canonical.name = canonical_name;
            canonical.color_tag.0 = tool_color.hex.to_string();
            canonical.canonicalize_tool_layer();
            changed = true;
        }
        let canonical_name = canonical.name.clone();

        if !duplicate_ids.is_empty() {
            project
                .layers
                .retain(|layer| !duplicate_ids.contains(&layer.id));
            for (idx, layer) in project.layers.iter_mut().enumerate() {
                layer.order_index = idx as u32;
            }
            warnings.push(format!(
                "Migrated {} duplicate {} tool layer(s) into canonical {}",
                duplicate_ids.len(),
                tool_color.name,
                canonical_name
            ));
        }
    }

    if changed {
        project.dirty = true;
    }
    warnings
}

fn lock_err(name: &str, e: impl std::fmt::Display) -> ServiceError {
    ServiceError::internal(format!("Failed to lock {name}: {e}"))
}

fn recovery_dir() -> ServiceResult<PathBuf> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| ServiceError::persistence("Could not determine config directory"))?;
    Ok(config_dir.join("beam-bench").join("recovery"))
}

/// Remove the open project's autosave after an intentional application close.
///
/// Crash recovery files must survive abnormal termination, but a close that
/// reaches Tauri's accepted `CloseRequested` path is deliberate — including
/// when the user chose Don't Save. Leaving that autosave behind would make the
/// next launch incorrectly offer to restore work the user explicitly discarded.
pub fn discard_current_project_recovery(ctx: &ServiceContext) -> ServiceResult<bool> {
    let dir = recovery_dir()?;
    discard_current_project_recovery_from_dir(ctx, &dir)
}

fn discard_current_project_recovery_from_dir(
    ctx: &ServiceContext,
    dir: &Path,
) -> ServiceResult<bool> {
    let project_id = {
        let project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
        let Some(project) = project_guard.as_ref() else {
            return Ok(false);
        };
        project.metadata.project_id
    };
    let recovery_path = dir.join(format!("{project_id}.lzrproj.recovery"));
    let existed = recovery_path.exists();
    discard_recovery(&recovery_path).map_err(|e| {
        ServiceError::persistence(format!("Failed to discard recovery on clean shutdown: {e}"))
    })?;
    Ok(existed)
}

pub fn save_project_to_path(ctx: &ServiceContext, save_path: &Path) -> ServiceResult<String> {
    save_project_to_path_impl(ctx, Some(save_path))
}

pub fn save_project_current_path(ctx: &ServiceContext) -> ServiceResult<String> {
    save_project_to_path_impl(ctx, None)
}

fn save_project_to_path_impl(
    ctx: &ServiceContext,
    requested_path: Option<&Path>,
) -> ServiceResult<String> {
    let _edit_guard = ctx.lock_project_edits();
    // Edits roll back their own panics, so the project is intact even if a
    // panic poisoned its lock. Saving must still work.
    ctx.project.clear_poison();
    // Document and destination are one transaction. All document replacements
    // acquire these locks in the same order: project, then project_path.
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let mut path_guard = ctx
        .project_path
        .lock()
        .map_err(|e| lock_err("project_path", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    let save_path = requested_path
        .map(Path::to_path_buf)
        .or_else(|| path_guard.clone())
        .ok_or_else(|| {
            ServiceError::invalid_state("No save path set (project has never been saved)")
        })?;
    let previous_metadata = project.metadata.clone();
    if let Some(name) = project_name_from_path(&save_path) {
        project.metadata.project_name = name;
    }
    project.metadata.stamp_for_save();
    if let Err(error) = save_project(project, &save_path) {
        project.metadata = previous_metadata;
        return Err(ServiceError::persistence(format!("Save failed: {error}")));
    }
    *path_guard = Some(save_path.clone());
    project.dirty = false;
    ctx.project_save_count
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let saved_project = project.clone();
    if let Ok(dir) = recovery_dir() {
        let _ = discard_recovery(
            &dir.join(format!("{}.lzrproj.recovery", project.metadata.project_id)),
        );
    }
    drop(path_guard);
    drop(project_guard);

    {
        let mut settings = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
        settings.push_recent_file(
            &save_path.to_string_lossy(),
            &saved_project.metadata.project_name,
        );
    }
    persist_settings_to_disk(ctx);
    ctx.emit_event(
        "project.saved",
        json!({
            "project": events::project_summary(&saved_project, Some(&save_path)),
        }),
    );
    Ok(save_path.to_string_lossy().to_string())
}

/// Load and migrate a project without changing app settings or session state.
pub fn load_project_from_path(file_path: &str) -> ServiceResult<Project> {
    let (project, migration_warnings) = load_project_with_migrations(file_path)?;
    for w in &migration_warnings {
        eprintln!("[migrate_mixed_layers] {w}");
    }
    Ok(project)
}

/// Tell the user once that opening a project reorganized its layers.
fn queue_migration_notice(ctx: &ServiceContext, migration_warnings: &[String]) {
    if migration_warnings.is_empty() {
        return;
    }
    for w in migration_warnings {
        tracing::info!("project migration: {w}");
    }
    if let Ok(mut notices) = ctx.pending_notices.lock() {
        notices.push(format!(
            "[{LAYERS_MIGRATED_CODE}] {}",
            migration_warnings.join("; ")
        ));
    }
}

/// Stable code the frontend localizes for a project from a newer app version.
pub const NEWER_APP_PROJECT_CODE: &str = "project_from_newer_app";

/// Stable code the frontend localizes when opening split or merged layers.
pub const LAYERS_MIGRATED_CODE: &str = "layers_migrated_on_open";

fn load_project_with_migrations(file_path: &str) -> ServiceResult<(Project, Vec<String>)> {
    let open_path = PathBuf::from(file_path);
    let mut project = load_project(&open_path)
        .map_err(|e| ServiceError::persistence(format!("Failed to open project: {e}")))?;
    if let Some(name) = project_name_from_path(&open_path) {
        project.metadata.project_name = name;
    }

    // Normalize legacy tool-color siblings first, then split non-tool mixed raster/vector layers.
    let mut migration_warnings = migrate_tool_layers(&mut project);
    migration_warnings.extend(migrate_mixed_layers(&mut project));
    Ok((project, migration_warnings))
}

pub fn open_project_from_path(ctx: &ServiceContext, file_path: &str) -> ServiceResult<Project> {
    let open_path = PathBuf::from(file_path);
    let (project, migration_warnings) = match load_project_with_migrations(file_path) {
        Ok(loaded) => loaded,
        Err(error)
            if std::fs::metadata(&open_path)
                .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            // Usually a Recent Projects entry for a file that was moved or
            // deleted. Drop the stale entry so it is not offered again.
            let removed = ctx
                .settings
                .lock()
                .map_err(|e| lock_err("settings", e))?
                .remove_recent_file(file_path);
            if removed {
                persist_settings_to_disk(ctx);
            }
            tracing::debug!("project open failed for a missing file: {error}");
            return Err(ServiceError::not_found(format!(
                "[project_file_missing] The project file was moved or deleted: {file_path}"
            )));
        }
        Err(error) => return Err(error),
    };
    {
        let _edit_guard = ctx.lock_project_edits();
        let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
        let mut path_guard = ctx
            .project_path
            .lock()
            .map_err(|e| lock_err("project_path", e))?;
        *project_guard = Some(project.clone());
        *path_guard = Some(open_path.clone());
        ctx.clear_project_history()
            .map_err(ServiceError::internal)?;
    }
    {
        let mut cache_guard = ctx
            .plan_cache
            .lock()
            .map_err(|e| lock_err("plan_cache", e))?;
        *cache_guard = None;
    }

    {
        let mut settings_guard = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
        let name = project_name_from_path(&open_path).unwrap_or_else(|| "Unknown".to_string());
        settings_guard.push_recent_file(&open_path.to_string_lossy(), &name);
    }

    persist_settings_to_disk(ctx);
    queue_migration_notice(ctx, &migration_warnings);
    if project.metadata.saved_by_newer_app()
        && let Ok(mut notices) = ctx.pending_notices.lock()
    {
        notices.push(format!(
            "[{NEWER_APP_PROJECT_CODE}] This project was saved by Beam Bench {}. Saving it here may drop settings this version does not support.",
            project.metadata.app_version
        ));
    }
    ctx.emit_event(
        "project.opened",
        json!({
            "project": events::project_summary(&project, Some(&open_path)),
        }),
    );
    Ok(project)
}

pub fn get_asset_data(ctx: &ServiceContext, asset_id: AssetId) -> ServiceResult<Vec<u8>> {
    let project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_ref()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    project
        .get_asset_data(asset_id)
        .map(|d| d.to_vec())
        .ok_or_else(|| ServiceError::not_found("Asset data not found"))
}

pub fn autosave_project(ctx: &ServiceContext) -> ServiceResult<String> {
    let dir = recovery_dir()?;
    autosave_project_to_dir(ctx, &dir)
}

fn autosave_project_to_dir(ctx: &ServiceContext, dir: &Path) -> ServiceResult<String> {
    // Edits roll back their own panics, so the project is intact even if a
    // panic poisoned its lock. Saving must still work.
    ctx.project.clear_poison();
    std::fs::create_dir_all(dir)?;

    // Copy the project and write the archive outside the lock, so edits and
    // the window are not held up while a large project is compressed.
    let (project, saves_before, source_path) = {
        let _edit_guard = ctx.lock_project_edits();
        let guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
        let saves = ctx
            .project_save_count
            .load(std::sync::atomic::Ordering::SeqCst);
        (
            guard
                .clone()
                .ok_or_else(|| ServiceError::not_found("No project open"))?,
            saves,
            ctx.project_path
                .lock()
                .map_err(|e| lock_err("project_path", e))?
                .clone(),
        )
    };

    // save_recovery takes the recovery *directory* and derives the file name
    // itself — passing a file path here would bury the archive inside a
    // directory named like a file, where check_recovery never finds it.
    let recovery_path = save_recovery(&project, dir)
        .map_err(|e| ServiceError::persistence(format!("Autosave failed: {e}")))?;
    save_recovery_source(&recovery_path, source_path.as_deref())
        .map_err(|e| ServiceError::persistence(format!("Autosave failed: {e}")))?;
    // A save that finished while this copy was being written already removed
    // the recovery file; do not leave an older copy behind.
    if ctx
        .project_save_count
        .load(std::sync::atomic::Ordering::SeqCst)
        != saves_before
    {
        let _ = discard_recovery(&recovery_path);
    }
    let summary = events::project_summary(&project, None);
    ctx.emit_event(
        "project.autosaved",
        json!({
            "project": summary,
            "recovery_path": recovery_path.to_string_lossy().to_string(),
        }),
    );
    Ok(recovery_path.to_string_lossy().to_string())
}

pub fn check_recovery_files() -> ServiceResult<Vec<RecoveryInfo>> {
    let dir = recovery_dir()?;
    std::fs::create_dir_all(&dir)?;
    check_recovery(&dir)
        .map_err(|e| ServiceError::persistence(format!("Recovery check failed: {e}")))
}

pub fn restore_recovery_file(ctx: &ServiceContext, recovery_path: &str) -> ServiceResult<Project> {
    let path = PathBuf::from(recovery_path);
    let mut project = load_recovery(&path)
        .map_err(|e| ServiceError::persistence(format!("Failed to restore recovery: {e}")))?;

    // Normalize legacy tool-color siblings first, then split non-tool mixed raster/vector layers.
    let mut migration_warnings = migrate_tool_layers(&mut project);
    migration_warnings.extend(migrate_mixed_layers(&mut project));
    queue_migration_notice(ctx, &migration_warnings);
    project.dirty = true;
    // Save goes back to the file the work came from, if it still exists.
    let source_path = load_recovery_source(&path).filter(|source| source.is_file());
    {
        let _edit_guard = ctx.lock_project_edits();
        let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
        let mut path_guard = ctx
            .project_path
            .lock()
            .map_err(|e| lock_err("project_path", e))?;
        *project_guard = Some(project.clone());
        *path_guard = source_path.clone();
        ctx.clear_project_history()
            .map_err(ServiceError::internal)?;
    }
    {
        let mut cache_guard = ctx
            .plan_cache
            .lock()
            .map_err(|e| lock_err("plan_cache", e))?;
        *cache_guard = None;
    }
    // Keep the recovery archive: it is the only durable copy until the user
    // saves or the next autosave replaces it. Saving or a clean shutdown
    // removes it.
    ctx.emit_event(
        "project.recovery.restored",
        json!({
            "project": events::project_summary(&project, source_path.as_deref()),
            "recovery_path": recovery_path,
        }),
    );
    Ok(project)
}

pub fn discard_recovery_file(ctx: &ServiceContext, recovery_path: &str) -> ServiceResult<()> {
    let path = PathBuf::from(recovery_path);
    discard_recovery(&path)
        .map_err(|e| ServiceError::persistence(format!("Failed to discard recovery: {e}")))?;
    ctx.emit_event(
        "project.recovery.discarded",
        json!({
            "recovery_path": recovery_path,
        }),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::{Bounds, ColorTag, Point2D};
    use beambench_core::{ObjectData, ProjectObject};
    use tempfile::tempdir;

    #[test]
    fn opening_a_missing_project_drops_it_from_recent_projects() {
        let ctx = ServiceContext::new();
        let dir = tempdir().unwrap();
        let missing = dir.path().join("moved.lzrproj");
        let missing = missing.to_string_lossy().into_owned();
        let kept = dir
            .path()
            .join("kept.lzrproj")
            .to_string_lossy()
            .into_owned();
        {
            let mut settings = ctx.settings.lock().unwrap();
            settings.push_recent_file(&kept, "kept");
            settings.push_recent_file(&missing, "moved");
        }

        let error = open_project_from_path(&ctx, &missing).unwrap_err();
        assert!(
            error.message.starts_with("[project_file_missing]"),
            "{error}"
        );
        let recent: Vec<_> = ctx
            .settings
            .lock()
            .unwrap()
            .get_recent_files()
            .iter()
            .map(|file| file.path.clone())
            .collect();
        assert_eq!(recent, vec![kept]);
        assert!(ctx.project.lock().unwrap().is_none());
    }

    #[test]
    fn opening_an_unreadable_project_keeps_it_in_recent_projects() {
        let ctx = ServiceContext::new();
        let dir = tempdir().unwrap();
        let corrupt = dir.path().join("corrupt.lzrproj");
        std::fs::write(&corrupt, b"not a project").unwrap();
        let corrupt = corrupt.to_string_lossy().into_owned();
        ctx.settings
            .lock()
            .unwrap()
            .push_recent_file(&corrupt, "corrupt");

        let error = open_project_from_path(&ctx, &corrupt).unwrap_err();
        assert!(!error.message.contains("[project_file_missing]"), "{error}");
        assert_eq!(ctx.settings.lock().unwrap().get_recent_files().len(), 1);
    }

    #[test]
    fn restore_recovery_keeps_archive_and_original_save_path() {
        let ctx = ServiceContext::new();
        let dir = tempdir().unwrap();
        let original = dir.path().join("original.lzrproj");
        let project = Project::new("Recovery Test");
        beambench_project::save_project(&project, &original).unwrap();
        *ctx.project.lock().unwrap() = Some(project);
        *ctx.project_path.lock().unwrap() = Some(original.clone());
        let recovery_path = PathBuf::from(autosave_project_to_dir(&ctx, dir.path()).unwrap());

        let restarted = ServiceContext::new();
        let restored = restore_recovery_file(&restarted, &recovery_path.to_string_lossy()).unwrap();

        assert_eq!(restored.metadata.project_name, "Recovery Test");
        assert!(restored.dirty);
        // A second crash before the next save must still find the work.
        assert_eq!(check_recovery(dir.path()).unwrap().len(), 1);
        assert_eq!(*restarted.project_path.lock().unwrap(), Some(original));

        discard_recovery_file(&restarted, &recovery_path.to_string_lossy()).unwrap();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn restore_recovery_of_never_saved_project_has_no_save_path() {
        let ctx = ServiceContext::new();
        let dir = tempdir().unwrap();
        let project = Project::new("Recovery Test");
        let recovery_path = beambench_project::save_recovery(&project, dir.path()).unwrap();

        restore_recovery_file(&ctx, &recovery_path.to_string_lossy()).unwrap();
        assert!(ctx.project_path.lock().unwrap().is_none());
        assert!(recovery_path.exists());
    }

    #[test]
    fn autosave_writes_recovery_file_that_check_recovery_finds() {
        let ctx = ServiceContext::new();
        *ctx.project.lock().unwrap() = Some(Project::new("Autosave Test"));
        let dir = tempdir().unwrap();

        let recovery_path = autosave_project_to_dir(&ctx, dir.path()).unwrap();

        let path = PathBuf::from(&recovery_path);
        assert!(path.is_file(), "autosave must produce a file, not a dir");

        let found = check_recovery(dir.path()).unwrap();
        assert_eq!(found.len(), 1, "recovery scan must find the autosave");
        assert_eq!(found[0].project_name, "Autosave Test");
        assert_eq!(found[0].path, recovery_path);
    }

    #[test]
    fn intentional_close_discards_current_project_recovery() {
        let ctx = ServiceContext::new();
        let project = Project::new("Discarded on Close");
        let dir = tempdir().unwrap();
        let recovery_path = beambench_project::save_recovery(&project, dir.path()).unwrap();
        *ctx.project.lock().unwrap() = Some(project);

        let discarded = discard_current_project_recovery_from_dir(&ctx, dir.path()).unwrap();

        assert!(discarded);
        assert!(!recovery_path.exists());
    }

    #[test]
    fn save_and_load_project_round_trips_multi_entry_layers() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("sub-layers.bb");

        let mut project = Project::new("Round Trip");
        project.layers.clear();
        let mut layer = Layer::new("Line", OperationType::Line);
        layer.entries[0].speed_mm_min = 1200.0;
        layer.entries[0].power_percent = 30.0;

        let mut image_entry = beambench_core::CutEntry::new(OperationType::Image);
        image_entry.output_enabled = false;
        image_entry.speed_mm_min = 900.0;
        layer.entries.push(image_entry.clone());

        let mut fill_entry = beambench_core::CutEntry::new(OperationType::Fill);
        fill_entry.speed_mm_min = 700.0;
        fill_entry.power_percent = 65.0;
        layer.entries.push(fill_entry.clone());

        let layer_id = layer.id;
        project.add_layer(layer);
        project.add_object(ProjectObject::new(
            "Rect",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::Shape {
                kind: beambench_core::ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));

        save_project(&project, &path).unwrap();
        let restored = load_project(&path).unwrap();

        let restored_layer = restored
            .find_layer(layer_id)
            .expect("layer should round-trip");
        assert_eq!(restored_layer.entries.len(), 3);
        assert_eq!(restored_layer.entries[0].speed_mm_min, 1200.0);
        assert_eq!(restored_layer.entries[1].id, image_entry.id);
        assert!(!restored_layer.entries[1].output_enabled);
        assert_eq!(restored_layer.entries[2].id, fill_entry.id);
        assert_eq!(restored_layer.entries[2].power_percent, 65.0);
    }

    /// Legacy projects may carry a VirtualClone of a raster on a
    /// non-image layer. migrate_mixed_layers must recognize the clone
    /// as effectively-raster content and re-home it to a sibling
    /// image layer, not leave it on the non-image layer where the
    /// planner will still emit it as raster.
    #[test]
    fn migrate_mixed_layers_re_homes_virtual_raster_clones() {
        let mut project = Project::new("Legacy");
        let image_layer = Layer::new("Image", OperationType::Image);
        let line_layer = Layer::new("Line", OperationType::Line);
        let image_layer_id = image_layer.id;
        let line_layer_id = line_layer.id;
        project.layers.push(image_layer);
        project.layers.push(line_layer);

        let raster = ProjectObject::new(
            "Raster",
            image_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::RasterImage {
                asset_key: "k".into(),
                original_width_px: 10,
                original_height_px: 10,
                adjustments: None,
                masks: Vec::new(),
            },
        );
        let raster_id = raster.id;
        project.add_object(raster);

        // A VirtualClone of the raster placed on the Line layer — the
        // exact invalid state the invariant should catch.
        let clone = ProjectObject::new(
            "Clone",
            line_layer_id,
            Bounds::new(Point2D::new(20.0, 0.0), Point2D::new(30.0, 10.0)),
            ObjectData::VirtualClone {
                source_id: raster_id,
            },
        );
        let clone_id = clone.id;
        project.add_object(clone);

        let warnings = migrate_mixed_layers(&mut project);
        assert!(
            !warnings.is_empty(),
            "expected a migration warning for the raster clone"
        );

        // Find the migrated clone and verify it's on an image layer.
        let clone_obj = project
            .find_object(clone_id)
            .expect("clone should still exist");
        let dest_layer = project
            .find_layer(clone_obj.layer_id)
            .expect("clone destination layer should exist");
        assert_eq!(
            dest_layer.primary_entry().operation,
            OperationType::Image,
            "raster clone must land on an image layer after migration"
        );
        // Original Line layer keeps no mixed content now.
        let line_still_has_clone = project
            .objects
            .iter()
            .any(|o| o.layer_id == line_layer_id && o.id == clone_id);
        assert!(
            !line_still_has_clone,
            "raster clone should be removed from the Line layer"
        );
        let mut names = project
            .layers
            .iter()
            .map(|layer| layer.name.as_str())
            .collect::<Vec<_>>();
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            project.layers.len(),
            "mixed-layer migration must not create duplicate stored names"
        );
    }

    #[test]
    fn migrate_mixed_layers_keeps_disabled_layers_off() {
        let mut project = Project::new("Legacy");
        let mut line_layer = Layer::new("Line", OperationType::Line);
        line_layer.enabled = false;
        line_layer.visible = false;
        line_layer.primary_entry_mut().output_enabled = false;
        line_layer.primary_entry_mut().power_percent = 37.0;
        let line_layer_id = line_layer.id;
        project.layers.push(line_layer);
        let raster = ProjectObject::new(
            "Raster",
            line_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::RasterImage {
                asset_key: "k".into(),
                original_width_px: 10,
                original_height_px: 10,
                adjustments: None,
                masks: Vec::new(),
            },
        );
        let raster_id = raster.id;
        project.add_object(raster);

        migrate_mixed_layers(&mut project);

        let moved = project.find_object(raster_id).unwrap();
        let sibling = project.find_layer(moved.layer_id).unwrap();
        assert_ne!(sibling.id, line_layer_id);
        assert!(!sibling.enabled);
        assert!(!sibling.visible);
        assert!(!sibling.primary_entry().output_enabled);
        assert_eq!(sibling.primary_entry().power_percent, 37.0);
    }

    #[test]
    fn migrate_tool_layers_merges_image_sibling_and_discards_cut_settings() {
        let mut project = Project::new("Tool Migration");
        project.layers.clear();

        let mut tool_line = Layer::new("T1 line", OperationType::Cut);
        tool_line.color_tag = ColorTag("#DA0B3F".to_string());
        tool_line.entries[0].speed_mm_min = 8000.0;
        tool_line.entries[0].power_percent = 90.0;
        let tool_line_id = tool_line.id;

        let mut tool_image = Layer::new("T1 image", OperationType::Image);
        tool_image.color_tag = ColorTag("#da0b3f".to_string());
        let tool_image_id = tool_image.id;

        project.add_layer(tool_line);
        project.add_layer(tool_image);

        let image_obj = ProjectObject::new(
            "Tool image",
            tool_image_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::RasterImage {
                asset_key: "asset-1".to_string(),
                original_width_px: 10,
                original_height_px: 10,
                adjustments: None,
                masks: Vec::new(),
            },
        );
        let image_obj_id = image_obj.id;
        project.add_object(image_obj);

        let warnings = migrate_tool_layers(&mut project);

        assert!(!warnings.is_empty());
        assert!(project.find_layer(tool_image_id).is_none());
        let canonical = project.find_layer(tool_line_id).unwrap();
        assert!(canonical.is_tool_layer);
        assert_eq!(canonical.entries.len(), 1);
        assert_eq!(canonical.primary_entry().operation, OperationType::Tool);
        assert_eq!(canonical.primary_entry().speed_mm_min, 0.0);
        assert!(!canonical.primary_entry().output_enabled);
        assert_eq!(
            project.find_object(image_obj_id).unwrap().layer_id,
            tool_line_id
        );
    }

    #[test]
    fn migrate_tool_layers_leaves_already_canonical_layer_unchanged() {
        let mut project = Project::new("Canonical Tool");
        project.layers.clear();
        project.dirty = false;

        let mut tool_layer = Layer::new("T1", OperationType::Tool);
        tool_layer.color_tag = ColorTag("#DA0B3F".to_string());
        tool_layer.canonicalize_tool_layer();
        tool_layer.name = "T1".to_string();
        let layer_before = tool_layer.clone();
        project.add_layer(tool_layer);
        project.dirty = false;

        let warnings = migrate_tool_layers(&mut project);

        assert!(warnings.is_empty());
        assert!(!project.dirty);
        assert_eq!(project.layers.len(), 1);
        assert_eq!(project.layers[0], layer_before);
    }

    #[test]
    fn migrate_tool_layers_merges_three_tool_color_siblings() {
        let mut project = Project::new("Three Tool Siblings");
        project.layers.clear();

        let mut tool_line = Layer::new("T1 line", OperationType::Line);
        tool_line.color_tag = ColorTag("#DA0B3F".to_string());
        let canonical_id = tool_line.id;

        let mut tool_cut = Layer::new("T1 cut", OperationType::Cut);
        tool_cut.color_tag = ColorTag("#da0b3f".to_string());
        let cut_id = tool_cut.id;

        let mut tool_image = Layer::new("T1 image", OperationType::Image);
        tool_image.color_tag = ColorTag("#DA0B3FFF".to_string());
        let image_id = tool_image.id;

        project.add_layer(tool_line);
        project.add_layer(tool_cut);
        project.add_layer(tool_image);

        let cut_obj = ProjectObject::new(
            "Tool cut",
            cut_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(5.0, 5.0)),
            ObjectData::Shape {
                kind: beambench_core::ShapeKind::Rectangle,
                width: 5.0,
                height: 5.0,
                corner_radius: 0.0,
            },
        );
        let cut_obj_id = cut_obj.id;
        let image_obj = ProjectObject::new(
            "Tool image",
            image_id,
            Bounds::new(Point2D::new(10.0, 0.0), Point2D::new(20.0, 10.0)),
            ObjectData::RasterImage {
                asset_key: "asset-1".to_string(),
                original_width_px: 10,
                original_height_px: 10,
                adjustments: None,
                masks: Vec::new(),
            },
        );
        let image_obj_id = image_obj.id;
        project.add_object(cut_obj);
        project.add_object(image_obj);

        let warnings = migrate_tool_layers(&mut project);

        assert_eq!(warnings.len(), 1);
        assert_eq!(project.layers.len(), 1);
        assert!(project.find_layer(cut_id).is_none());
        assert!(project.find_layer(image_id).is_none());
        let canonical = project.find_layer(canonical_id).unwrap();
        assert!(canonical.is_tool_layer);
        assert_eq!(canonical.primary_entry().operation, OperationType::Tool);
        assert_eq!(
            project.find_object(cut_obj_id).unwrap().layer_id,
            canonical_id
        );
        assert_eq!(
            project.find_object(image_obj_id).unwrap().layer_id,
            canonical_id
        );
        assert!(project.dirty);
    }
}
