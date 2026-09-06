use std::io::{Cursor, Read, Seek, Write};
use std::path::Path;

use chrono::{DateTime, Utc};
use thiserror::Error;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

use beambench_common::path::VecPath;
use beambench_core::asset::AssetId;
use beambench_core::object::ObjectData;
use beambench_core::project::Project;

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("validation error: {0}")]
    Validation(String),
}

/// Save a project to a `.lzrproj` zip archive.
/// Writes to a temp file first, then atomically renames to the target path.
pub fn save_project(project: &Project, path: &Path) -> Result<(), PersistenceError> {
    let parent = path.parent().unwrap_or(Path::new("."));
    let temp = tempfile::NamedTempFile::new_in(parent)?;
    let bytes = save_project_to_bytes(project)?;

    {
        let mut file = temp.as_file();
        file.write_all(&bytes)?;
        file.sync_all()?;
    }

    // Atomic rename
    temp.persist(path)
        .map_err(|e| PersistenceError::Io(e.error))?;
    Ok(())
}

/// Serialize a project to `.lzrproj` archive bytes without touching the filesystem.
pub fn save_project_to_bytes(project: &Project) -> Result<Vec<u8>, PersistenceError> {
    let cursor = Cursor::new(Vec::new());
    let cursor = write_project_archive(project, cursor)?;
    Ok(cursor.into_inner())
}

fn write_project_archive<W: Write + Seek>(
    project: &Project,
    writer: W,
) -> Result<W, PersistenceError> {
    let mut zip = zip::ZipWriter::new(writer);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    // Write manifest.json (lightweight metadata)
    let manifest = serde_json::to_string_pretty(&project.metadata)?;
    zip.start_file("manifest.json", options)?;
    zip.write_all(manifest.as_bytes())?;

    // Write project.json (full project minus asset_data which is serde(skip))
    let project_json = serde_json::to_string_pretty(&project.document_value()?)?;
    // Never overwrite a file with an archive this loader would refuse to open.
    let limits = LoadLimits::default();
    let mut expanded_size = (manifest.len() + project_json.len()) as u64;
    if project_json.len() as u64 > limits.json_bytes
        || manifest.len() as u64 > limits.json_bytes
        || project.objects.len() > limits.objects
        || project.assets.len() > limits.assets
    {
        return Err(PersistenceError::Validation(
            "Project exceeds supported archive limits".into(),
        ));
    }
    for asset in &project.assets {
        let data = project.asset_data.get(&asset.id).ok_or_else(|| {
            PersistenceError::Validation(format!("Missing bytes for asset {}", asset.id))
        })?;
        expanded_size = expanded_size.saturating_add(data.len() as u64);
        if data.len() as u64 > limits.asset_bytes || expanded_size > limits.total_bytes {
            return Err(PersistenceError::Validation(
                "Project is too large to save. Split it into smaller files.".into(),
            ));
        }
    }
    zip.start_file("project.json", options)?;
    zip.write_all(project_json.as_bytes())?;

    // Write asset files
    for asset in &project.assets {
        if let Some(data) = project.asset_data.get(&asset.id) {
            let entry_name = format!("assets/asset-{}.{}", asset.id, asset.media_type.extension());
            zip.start_file(entry_name, options)?;
            zip.write_all(data)?;
        }
    }

    Ok(zip.finish()?)
}

/// Load a project from a `.lzrproj` zip archive.
pub fn load_project(path: &Path) -> Result<Project, PersistenceError> {
    load_project_with_limits(path, LoadLimits::default())
}

#[derive(Clone, Copy)]
struct LoadLimits {
    json_bytes: u64,
    asset_bytes: u64,
    total_bytes: u64,
    objects: usize,
    assets: usize,
}

impl Default for LoadLimits {
    fn default() -> Self {
        Self {
            json_bytes: 64 * 1024 * 1024,
            asset_bytes: 256 * 1024 * 1024,
            total_bytes: 512 * 1024 * 1024,
            objects: 250_000,
            assets: 10_000,
        }
    }
}

fn read_bounded(mut reader: impl Read, limit: u64) -> Result<Vec<u8>, PersistenceError> {
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(PersistenceError::Validation(format!(
            "Project archive exceeds the expanded size limit of {limit} bytes. Split the project into smaller files."
        )));
    }
    Ok(bytes)
}

fn load_project_with_limits(path: &Path, limits: LoadLimits) -> Result<Project, PersistenceError> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    if archive.len() > limits.assets + 2 {
        return Err(PersistenceError::Validation(
            "Project archive has too many entries".into(),
        ));
    }
    let mut declared_total = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let limit = if entry.name().ends_with(".json") {
            limits.json_bytes
        } else {
            limits.asset_bytes
        };
        declared_total = declared_total.saturating_add(entry.size());
        if entry.size() > limit || declared_total > limits.total_bytes {
            return Err(PersistenceError::Validation(
                "Project archive is too large when expanded. Split it into smaller files.".into(),
            ));
        }
    }

    // Verify required entries exist
    if archive.by_name("manifest.json").is_err() {
        return Err(PersistenceError::Validation(
            "missing manifest.json in archive".into(),
        ));
    }

    // Read project.json
    let project_json = {
        let entry = archive
            .by_name("project.json")
            .map_err(|_| PersistenceError::Validation("missing project.json in archive".into()))?;
        read_bounded(entry, limits.json_bytes.min(limits.total_bytes))?
    };

    let mut project: Project = serde_json::from_slice(&project_json)?;
    if project.objects.len() > limits.objects || project.assets.len() > limits.assets {
        return Err(PersistenceError::Validation(
            "Project has too many objects or assets".into(),
        ));
    }
    let mut remaining = limits.total_bytes.saturating_sub(project_json.len() as u64);

    // Read asset files
    for asset in &project.assets {
        let entry_name = format!("assets/asset-{}.{}", asset.id, asset.media_type.extension());
        match archive.by_name(&entry_name) {
            Ok(entry) => {
                let data = read_bounded(entry, limits.asset_bytes.min(remaining))?;
                remaining -= data.len() as u64;
                project
                    .asset_data
                    .insert(asset.id, std::sync::Arc::new(data));
            }
            Err(_) => {
                return Err(PersistenceError::Validation(format!(
                    "missing asset file: {entry_name}"
                )));
            }
        }
    }

    // Remove isolated nodes left by older node-edit operations before they can
    // affect selection, bounds mapping, or laser planning.
    prune_orphan_vector_subpaths(&mut project);

    // Validate: every RasterImage asset_key resolves to an asset
    for obj in &project.objects {
        if let beambench_core::object::ObjectData::RasterImage { asset_key, .. } = &obj.data {
            let parsed: Result<uuid::Uuid, _> = asset_key.parse();
            if let Ok(uuid) = parsed {
                let id = AssetId::from_uuid(uuid);
                if project.find_asset(id).is_none() {
                    return Err(PersistenceError::Validation(format!(
                        "object '{}' references missing asset '{}'",
                        obj.name, asset_key
                    )));
                }
            }
        }
    }

    project.dirty = false;
    Ok(project)
}

fn prune_orphan_vector_subpaths(project: &mut Project) {
    for object in &mut project.objects {
        let ObjectData::VectorPath {
            path_data, closed, ..
        } = &mut object.data
        else {
            continue;
        };

        let mut path = VecPath::parse_svg_d(path_data);
        let previous_path_bounds = path.visual_bounds();
        let mut next_subpath_index = 0;
        let subpath_index_map: Vec<Option<usize>> = path
            .subpaths
            .iter()
            .map(|subpath| {
                if subpath.has_drawable_segment() {
                    let mapped = next_subpath_index;
                    next_subpath_index += 1;
                    Some(mapped)
                } else {
                    None
                }
            })
            .collect();
        if path.prune_orphan_subpaths() == 0 {
            continue;
        }

        *path_data = path.to_svg_d();
        *closed = path.subpaths.iter().any(|subpath| subpath.closed);
        object.bounds = match (previous_path_bounds, path.visual_bounds()) {
            (Some(previous), Some(cleaned)) => {
                // Preserve any scale/placement already applied through object.bounds
                // while removing the orphan's contribution to those bounds.
                let previous_width = previous.max.x - previous.min.x;
                let previous_height = previous.max.y - previous.min.y;
                let object_width = object.bounds.max.x - object.bounds.min.x;
                let object_height = object.bounds.max.y - object.bounds.min.y;
                let scale_x = if previous_width.abs() > f64::EPSILON {
                    object_width / previous_width
                } else {
                    1.0
                };
                let scale_y = if previous_height.abs() > f64::EPSILON {
                    object_height / previous_height
                } else {
                    1.0
                };
                let map_x = |x: f64| object.bounds.min.x + (x - previous.min.x) * scale_x;
                let map_y = |y: f64| object.bounds.min.y + (y - previous.min.y) * scale_y;
                beambench_common::Bounds::new(
                    beambench_common::Point2D::new(map_x(cleaned.min.x), map_y(cleaned.min.y)),
                    beambench_common::Point2D::new(map_x(cleaned.max.x), map_y(cleaned.max.y)),
                )
            }
            _ => beambench_common::Bounds::new(object.bounds.min, object.bounds.min),
        };
        object.tabs.retain_mut(|tab| {
            let Some(Some(mapped_index)) = subpath_index_map.get(tab.subpath_index) else {
                return false;
            };
            tab.subpath_index = *mapped_index;
            true
        });
        object.start_point_edits.retain_mut(|edit| {
            let Some(Some(mapped_index)) = subpath_index_map.get(edit.subpath_index) else {
                return false;
            };
            edit.subpath_index = *mapped_index;
            true
        });
    }
}

/// Information about a recoverable project.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RecoveryInfo {
    /// Path to the recovery file.
    pub path: String,
    /// Project name from the recovered project.
    pub project_name: String,
    /// Timestamp when the recovery file was saved (ISO 8601).
    pub saved_at: String,
}

/// Save a recovery copy of the project to the given directory.
/// Recovery files are named `<project_id>.lzrproj.recovery`.
pub fn save_recovery(
    project: &Project,
    recovery_dir: &Path,
) -> Result<std::path::PathBuf, PersistenceError> {
    std::fs::create_dir_all(recovery_dir)?;
    let filename = format!("{}.lzrproj.recovery", project.metadata.project_id);
    let path = recovery_dir.join(filename);
    save_project(project, &path)?;
    Ok(path)
}

/// List all recovery files in the given directory.
pub fn check_recovery(recovery_dir: &Path) -> Result<Vec<RecoveryInfo>, PersistenceError> {
    let mut results = Vec::new();

    if !recovery_dir.exists() {
        return Ok(results);
    }

    for entry in std::fs::read_dir(recovery_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("recovery") {
            // Try to load just the manifest to get the project name
            match load_project(&path) {
                Ok(project) => {
                    let metadata = std::fs::metadata(&path)?;
                    let saved_at = metadata
                        .modified()
                        .ok()
                        .map(|timestamp| DateTime::<Utc>::from(timestamp).to_rfc3339())
                        .unwrap_or_default();

                    results.push(RecoveryInfo {
                        path: path.to_string_lossy().to_string(),
                        project_name: project.metadata.project_name.clone(),
                        saved_at,
                    });
                }
                Err(_) => continue, // Skip corrupt recovery files
            }
        }
    }

    Ok(results)
}

/// Load a project from a recovery file (same format as regular project).
pub fn load_recovery(path: &Path) -> Result<Project, PersistenceError> {
    load_project(path)
}

/// Delete a recovery file.
pub fn discard_recovery(path: &Path) -> Result<(), PersistenceError> {
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::{Bounds, Point2D};
    use beambench_core::asset::{Asset, AssetMediaType};
    use beambench_core::object::{ObjectData, ProjectObject, StartPointEdit, TabAnchor};
    use tempfile::TempDir;

    fn test_project_with_asset() -> Project {
        let mut project = Project::new("Persistence Test");
        let layer_id = project.ensure_default_layer();

        // Add an asset
        let asset = Asset::new("photo.png", AssetMediaType::Png, 4, Some(1), Some(1));
        let asset_id = asset.id;
        project.add_asset(asset, vec![0x89, 0x50, 0x4E, 0x47]);

        // Add a raster image object referencing the asset
        project.add_object(ProjectObject::new(
            "test-image",
            layer_id,
            Bounds::new(Point2D::zero(), Point2D::new(100.0, 100.0)),
            ObjectData::RasterImage {
                asset_key: asset_id.to_string(),
                original_width_px: 1,
                original_height_px: 1,
                adjustments: None,
                masks: Vec::new(),
            },
        ));

        project
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.lzrproj");

        let original = test_project_with_asset();
        save_project(&original, &path).unwrap();

        let loaded = load_project(&path).unwrap();

        assert_eq!(loaded.metadata, original.metadata);
        assert_eq!(loaded.workspace, original.workspace);
        assert_eq!(loaded.layers, original.layers);
        assert_eq!(loaded.objects, original.objects);
        assert_eq!(loaded.assets, original.assets);
        assert!(!loaded.dirty);
    }

    #[test]
    fn load_prunes_orphan_vector_nodes_and_repairs_bounds() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("orphan-node.lzrproj");
        let mut project = Project::new("Orphan Node");
        let layer_id = project.ensure_default_layer();
        let mut object = ProjectObject::new(
            "legacy-path",
            layer_id,
            Bounds::new(Point2D::new(10.0, 20.0), Point2D::new(1000.0, 1000.0)),
            ObjectData::VectorPath {
                path_data: "M1000 1000 Z M10 20 L30 40".to_string(),
                closed: true,
                ruler_guide_axis: None,
            },
        );
        object.tabs = vec![
            TabAnchor {
                subpath_index: 0,
                position: 0.25,
            },
            TabAnchor {
                subpath_index: 1,
                position: 0.5,
            },
        ];
        object.start_point_edits = vec![
            StartPointEdit {
                subpath_index: 0,
                original_start_current_idx: 0,
                reversed: false,
                v_display: 1,
                normalized: false,
            },
            StartPointEdit {
                subpath_index: 1,
                original_start_current_idx: 0,
                reversed: false,
                v_display: 2,
                normalized: false,
            },
        ];
        project.add_object(object);
        save_project(&project, &path).unwrap();

        let loaded = load_project(&path).unwrap();
        let object = &loaded.objects[0];
        let ObjectData::VectorPath {
            path_data, closed, ..
        } = &object.data
        else {
            panic!("expected vector path");
        };
        assert_eq!(path_data, "M10 20 L30 40");
        assert!(!closed);
        assert_eq!(object.bounds.min, Point2D::new(10.0, 20.0));
        assert_eq!(object.bounds.max, Point2D::new(30.0, 40.0));
        assert_eq!(object.tabs.len(), 1);
        assert_eq!(object.tabs[0].subpath_index, 0);
        assert_eq!(object.start_point_edits.len(), 1);
        assert_eq!(object.start_point_edits[0].subpath_index, 0);
        assert!(!loaded.dirty);
    }

    #[test]
    fn save_and_load_preserves_asset_data() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.lzrproj");

        let original = test_project_with_asset();
        let asset_id = original.assets[0].id;
        let original_data = original.get_asset_data(asset_id).unwrap().to_vec();

        save_project(&original, &path).unwrap();
        let loaded = load_project(&path).unwrap();

        let loaded_data = loaded.get_asset_data(asset_id).unwrap();
        assert_eq!(loaded_data, &original_data);
    }

    #[test]
    fn save_project_to_bytes_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bytes.lzrproj");

        let original = test_project_with_asset();
        let bytes = save_project_to_bytes(&original).unwrap();
        assert!(!bytes.is_empty());

        std::fs::write(&path, bytes).unwrap();
        let loaded = load_project(&path).unwrap();

        assert_eq!(loaded.metadata, original.metadata);
        assert_eq!(loaded.objects, original.objects);
        assert_eq!(
            loaded.get_asset_data(original.assets[0].id).unwrap(),
            original.get_asset_data(original.assets[0].id).unwrap()
        );
    }

    #[test]
    fn save_project_path_callers_still_write_valid_archive() {
        let dir = TempDir::new().unwrap();
        let manual_save_path = dir.path().join("manual-save.lzrproj");
        let autosave_path = dir.path().join("autosave.lzrproj.recovery");
        let library_snapshot_path = dir.path().join("library-snapshot.lzrproj");

        let original = test_project_with_asset();
        save_project(&original, &manual_save_path).unwrap();
        save_project(&original, &library_snapshot_path).unwrap();
        save_recovery(&original, dir.path()).unwrap();

        let recovery_name = format!("{}.lzrproj.recovery", original.metadata.project_id);
        let actual_recovery_path = dir.path().join(recovery_name);
        std::fs::copy(&actual_recovery_path, &autosave_path).unwrap();

        for path in [&manual_save_path, &autosave_path, &library_snapshot_path] {
            let loaded = load_project(path).unwrap();
            assert_eq!(loaded.metadata, original.metadata);
            assert_eq!(loaded.assets, original.assets);
        }
    }

    #[test]
    fn save_creates_valid_zip_structure() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.lzrproj");

        let project = test_project_with_asset();
        save_project(&project, &path).unwrap();

        // Verify zip contents
        let file = std::fs::File::open(&path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();

        assert!(archive.by_name("manifest.json").is_ok());
        assert!(archive.by_name("project.json").is_ok());

        // Check that there's at least one asset entry
        let asset_name = format!(
            "assets/asset-{}.{}",
            project.assets[0].id,
            project.assets[0].media_type.extension()
        );
        assert!(archive.by_name(&asset_name).is_ok());
    }

    #[test]
    fn load_nonexistent_file_returns_error() {
        let result = load_project(Path::new("/nonexistent/test.lzrproj"));
        assert!(result.is_err());
    }

    #[test]
    fn load_invalid_zip_returns_error() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bad.lzrproj");
        std::fs::write(&path, b"not a zip file").unwrap();

        let result = load_project(&path);
        assert!(result.is_err());
    }

    #[test]
    fn load_zip_without_project_json_returns_validation_error() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("incomplete.lzrproj");

        // Create a zip with only manifest.json
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        zip.start_file("manifest.json", options).unwrap();
        zip.write_all(b"{}").unwrap();
        zip.finish().unwrap();

        let result = load_project(&path);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("project.json"), "Error was: {err}");
    }

    #[test]
    fn roundtrip_empty_project() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("empty.lzrproj");

        let original = Project::new("Empty");
        save_project(&original, &path).unwrap();
        let loaded = load_project(&path).unwrap();

        assert_eq!(loaded.metadata, original.metadata);
        assert!(loaded.layers.is_empty());
        assert!(loaded.objects.is_empty());
        assert!(loaded.assets.is_empty());
    }

    #[test]
    fn save_and_load_with_multiple_assets() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("multi.lzrproj");

        let mut project = Project::new("Multi Asset");
        let layer_id = project.ensure_default_layer();

        let asset1 = Asset::new("a.png", AssetMediaType::Png, 3, Some(10), Some(10));
        let asset2 = Asset::new("b.jpg", AssetMediaType::Jpeg, 4, Some(20), Some(20));
        let id1 = asset1.id;
        let id2 = asset2.id;
        project.add_asset(asset1, vec![1, 2, 3]);
        project.add_asset(asset2, vec![4, 5, 6, 7]);

        project.add_object(ProjectObject::new(
            "img1",
            layer_id,
            Bounds::new(Point2D::zero(), Point2D::new(10.0, 10.0)),
            ObjectData::RasterImage {
                asset_key: id1.to_string(),
                original_width_px: 10,
                original_height_px: 10,
                adjustments: None,
                masks: Vec::new(),
            },
        ));
        project.add_object(ProjectObject::new(
            "img2",
            layer_id,
            Bounds::new(Point2D::zero(), Point2D::new(20.0, 20.0)),
            ObjectData::RasterImage {
                asset_key: id2.to_string(),
                original_width_px: 20,
                original_height_px: 20,
                adjustments: None,
                masks: Vec::new(),
            },
        ));

        save_project(&project, &path).unwrap();
        let loaded = load_project(&path).unwrap();

        assert_eq!(loaded.assets.len(), 2);
        assert_eq!(loaded.get_asset_data(id1).unwrap(), &[1, 2, 3]);
        assert_eq!(loaded.get_asset_data(id2).unwrap(), &[4, 5, 6, 7]);
    }

    #[test]
    fn save_and_load_recovery_roundtrip() {
        let dir = TempDir::new().unwrap();
        let recovery_dir = dir.path().join("recovery");

        let original = test_project_with_asset();
        let path = save_recovery(&original, &recovery_dir).unwrap();

        assert!(path.exists());
        assert!(path.to_string_lossy().contains(".recovery"));

        let loaded = load_recovery(&path).unwrap();
        assert_eq!(loaded.metadata, original.metadata);
        assert_eq!(loaded.layers, original.layers);
        assert_eq!(loaded.objects, original.objects);
    }

    #[test]
    fn check_recovery_lists_files() {
        let dir = TempDir::new().unwrap();
        let recovery_dir = dir.path().join("recovery");

        let project = test_project_with_asset();
        save_recovery(&project, &recovery_dir).unwrap();

        let results = check_recovery(&recovery_dir).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].project_name, "Persistence Test");
        assert!(DateTime::parse_from_rfc3339(&results[0].saved_at).is_ok());
    }

    #[test]
    fn check_recovery_empty_dir() {
        let dir = TempDir::new().unwrap();
        let recovery_dir = dir.path().join("nonexistent");
        let results = check_recovery(&recovery_dir).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn discard_recovery_removes_file() {
        let dir = TempDir::new().unwrap();
        let recovery_dir = dir.path().join("recovery");

        let project = test_project_with_asset();
        let path = save_recovery(&project, &recovery_dir).unwrap();
        assert!(path.exists());

        discard_recovery(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn discard_recovery_nonexistent_is_ok() {
        discard_recovery(Path::new("/nonexistent/file.recovery")).unwrap();
    }
    #[test]
    fn archive_expansion_and_collection_limits_are_enforced() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("limits.lzrproj");
        let project = test_project_with_asset();
        save_project(&project, &path).unwrap();
        for limits in [
            LoadLimits {
                json_bytes: 1,
                ..LoadLimits::default()
            },
            LoadLimits {
                asset_bytes: 1,
                ..LoadLimits::default()
            },
            LoadLimits {
                total_bytes: 1,
                ..LoadLimits::default()
            },
            LoadLimits {
                objects: 0,
                ..LoadLimits::default()
            },
            LoadLimits {
                assets: 0,
                ..LoadLimits::default()
            },
        ] {
            assert!(matches!(
                load_project_with_limits(&path, limits),
                Err(PersistenceError::Validation(_))
            ));
        }
        assert!(load_project(&path).is_ok());
        assert!(read_bounded(Cursor::new(vec![0; 11]), 10).is_err());
        assert_eq!(
            read_bounded(Cursor::new(vec![0; 10]), 10).unwrap().len(),
            10
        );
    }

    #[test]
    fn archive_omits_runtime_dirty_and_rejects_missing_asset_bytes() {
        let mut project = test_project_with_asset();
        project.dirty = true;
        let bytes = save_project_to_bytes(&project).unwrap();
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut json = String::new();
        zip.by_name("project.json")
            .unwrap()
            .read_to_string(&mut json)
            .unwrap();
        assert!(
            serde_json::from_str::<serde_json::Value>(&json)
                .unwrap()
                .get("dirty")
                .is_none()
        );
        project.asset_data.clear();
        assert!(matches!(
            save_project_to_bytes(&project),
            Err(PersistenceError::Validation(_))
        ));
    }
}
