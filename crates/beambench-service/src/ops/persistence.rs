use std::path::{Path, PathBuf};

use beambench_common::PALETTE_COLORS;
use beambench_core::{AssetId, Layer, OperationType, Project};
use beambench_project::{
    RecoveryInfo, check_recovery, discard_recovery, load_project, load_recovery, save_project,
    save_recovery,
};
use serde_json::json;

use crate::context::ServiceContext;
use crate::error::{ServiceError, ServiceResult};
use crate::events;
use crate::ops::project;
use crate::persist::persist_settings_to_disk;

/// Split any layer holding mixed raster/vector content into two
/// sibling layers, one per content type. Called on project load so
/// legacy projects self-heal to match the "raster and vector content
/// live on separate layers" invariant enforced by all write paths.
///
/// Direction 1: a non-image layer containing raster objects → create
///   a sibling image layer (same color_tag, name suffixed " (Image)"),
///   re-home the rasters.
/// Direction 2: an image layer containing non-raster objects → create
///   a sibling Line layer (same color_tag, name suffixed " (Line)"),
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
        let (layer_is_image, layer_color, layer_name, layer_speed, layer_power): (
            bool,
            _,
            String,
            f64,
            f64,
        ) = {
            let Some(l) = project.layers.iter().find(|l| l.id == layer_id) else {
                continue;
            };
            if l.is_tool_layer || beambench_common::is_tool_color(&l.color_tag.0) {
                continue;
            }
            let entry = l.primary_entry();
            (
                entry.operation == OperationType::Image,
                l.color_tag.clone(),
                l.name.clone(),
                entry.speed_mm_min,
                entry.power_percent,
            )
        };

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
            let mut new_layer = Layer::new(format!("{base} (Line)"), OperationType::Line);
            new_layer.color_tag = layer_color.clone();
            new_layer.primary_entry_mut().speed_mm_min = layer_speed;
            new_layer.primary_entry_mut().power_percent = layer_power;
            let new_id = new_layer.id;
            project.layers.push(new_layer);
            for obj in project.objects.iter_mut() {
                if vector_ids.contains(&obj.id) {
                    obj.layer_id = new_id;
                }
            }
            warnings.push(format!(
                "Migrated {} non-raster object(s) off image layer '{}' into new sibling '{base} (Line)'",
                vector_ids.len(),
                layer_name,
            ));
        } else if !layer_is_image && !raster_ids.is_empty() {
            // Non-image layer with rasters → create a sibling Image layer.
            let base = crate::validation::strip_mode_suffix(&layer_name);
            let mut new_layer = Layer::new(format!("{base} (Image)"), OperationType::Image);
            new_layer.color_tag = layer_color.clone();
            new_layer.primary_entry_mut().speed_mm_min = layer_speed;
            new_layer.primary_entry_mut().power_percent = layer_power;
            let new_id = new_layer.id;
            project.layers.push(new_layer);
            for obj in project.objects.iter_mut() {
                if raster_ids.contains(&obj.id) {
                    obj.layer_id = new_id;
                }
            }
            warnings.push(format!(
                "Migrated {} raster object(s) off layer '{}' into new sibling '{base} (Image)'",
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

pub fn save_project_to_path(ctx: &ServiceContext, save_path: &Path) -> ServiceResult<String> {
    let project_id_str;
    {
        let project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
        let project = project_guard
            .as_ref()
            .ok_or_else(|| ServiceError::not_found("No project open"))?;

        project_id_str = project.metadata.project_id.to_string();
        save_project(project, save_path)
            .map_err(|e| ServiceError::persistence(format!("Save failed: {e}")))?;
    }

    {
        let mut path_guard = ctx
            .project_path
            .lock()
            .map_err(|e| lock_err("project_path", e))?;
        *path_guard = Some(save_path.to_path_buf());
    }

    {
        let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
        if let Some(p) = project_guard.as_mut() {
            p.dirty = false;
        }
    }

    if let Ok(dir) = recovery_dir() {
        let recovery_path = dir.join(format!("{project_id_str}.lzrproj.recovery"));
        let _ = discard_recovery(&recovery_path);
    }

    {
        let mut settings_guard = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
        let name = save_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unknown".to_string());
        settings_guard.push_recent_file(&save_path.to_string_lossy(), &name);
    }

    persist_settings_to_disk(ctx);
    let project = project::require_project(ctx)?;
    ctx.emit_event(
        "project.saved",
        json!({
            "project": events::project_summary(&project, Some(save_path)),
        }),
    );
    Ok(save_path.to_string_lossy().to_string())
}

pub fn save_project_current_path(ctx: &ServiceContext) -> ServiceResult<String> {
    {
        let project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
        if project_guard.is_none() {
            return Err(ServiceError::not_found("No project open"));
        }
    }
    let Some(path) = project::current_project_path(ctx)? else {
        return Err(ServiceError::invalid_state(
            "No save path set (project has never been saved)",
        ));
    };
    save_project_to_path(ctx, &path)
}

pub fn open_project_from_path(ctx: &ServiceContext, file_path: &str) -> ServiceResult<Project> {
    let open_path = PathBuf::from(file_path);
    let mut project = load_project(&open_path)
        .map_err(|e| ServiceError::persistence(format!("Failed to open project: {e}")))?;

    // Normalize legacy tool-color siblings first, then split non-tool mixed raster/vector layers.
    let mut migration_warnings = migrate_tool_layers(&mut project);
    migration_warnings.extend(migrate_mixed_layers(&mut project));
    for w in &migration_warnings {
        eprintln!("[migrate_mixed_layers] {w}");
    }

    {
        let mut path_guard = ctx
            .project_path
            .lock()
            .map_err(|e| lock_err("project_path", e))?;
        *path_guard = Some(open_path.clone());
    }
    {
        let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
        *project_guard = Some(project.clone());
    }
    {
        let mut cache_guard = ctx
            .plan_cache
            .lock()
            .map_err(|e| lock_err("plan_cache", e))?;
        *cache_guard = None;
    }

    ctx.clear_project_history()
        .map_err(ServiceError::internal)?;

    {
        let mut settings_guard = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
        let name = open_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unknown".to_string());
        settings_guard.push_recent_file(&open_path.to_string_lossy(), &name);
    }

    persist_settings_to_disk(ctx);
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
    std::fs::create_dir_all(dir)?;

    let project = {
        let project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
        project_guard
            .as_ref()
            .ok_or_else(|| ServiceError::not_found("No project open"))?
            .clone()
    };

    // save_recovery takes the recovery *directory* and derives the file name
    // itself — passing a file path here would bury the archive inside a
    // directory named like a file, where check_recovery never finds it.
    let recovery_path = save_recovery(&project, dir)
        .map_err(|e| ServiceError::persistence(format!("Autosave failed: {e}")))?;
    ctx.emit_event(
        "project.autosaved",
        json!({
            "project": events::project_summary(&project, None),
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
    for w in &migration_warnings {
        eprintln!("[migrate_mixed_layers] {w}");
    }
    {
        let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
        *project_guard = Some(project.clone());
    }
    {
        let mut path_guard = ctx
            .project_path
            .lock()
            .map_err(|e| lock_err("project_path", e))?;
        *path_guard = None;
    }
    {
        let mut cache_guard = ctx
            .plan_cache
            .lock()
            .map_err(|e| lock_err("plan_cache", e))?;
        *cache_guard = None;
    }
    ctx.clear_project_history()
        .map_err(ServiceError::internal)?;
    discard_recovery(&path)
        .map_err(|e| ServiceError::persistence(format!("Failed to discard recovery: {e}")))?;
    ctx.emit_event(
        "project.recovery.restored",
        json!({
            "project": events::project_summary(&project, None),
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
    fn restore_recovery_discards_source_file() {
        let ctx = ServiceContext::new();
        let dir = tempdir().unwrap();
        let project = Project::new("Recovery Test");
        let recovery_path = beambench_project::save_recovery(&project, dir.path()).unwrap();

        let restored = restore_recovery_file(&ctx, &recovery_path.to_string_lossy()).unwrap();
        assert_eq!(restored.metadata.project_name, "Recovery Test");
        assert!(!recovery_path.exists());
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
