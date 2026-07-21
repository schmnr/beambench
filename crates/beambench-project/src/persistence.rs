use std::io::{Cursor, Read, Seek, Write};
use std::path::Path;

use chrono::{DateTime, Utc};
use thiserror::Error;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

use beambench_core::asset::AssetId;
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
    let project_json = serde_json::to_string_pretty(project)?;
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
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    // Verify required entries exist
    if archive.by_name("manifest.json").is_err() {
        return Err(PersistenceError::Validation(
            "missing manifest.json in archive".into(),
        ));
    }

    // Read project.json
    let mut project_json = String::new();
    {
        let mut entry = archive
            .by_name("project.json")
            .map_err(|_| PersistenceError::Validation("missing project.json in archive".into()))?;
        entry.read_to_string(&mut project_json)?;
    }

    let mut project: Project = serde_json::from_str(&project_json)?;

    // Read asset files
    for asset in &project.assets {
        let entry_name = format!("assets/asset-{}.{}", asset.id, asset.media_type.extension());
        match archive.by_name(&entry_name) {
            Ok(mut entry) => {
                let mut data = Vec::new();
                entry.read_to_end(&mut data)?;
                project.asset_data.insert(asset.id, data);
            }
            Err(_) => {
                return Err(PersistenceError::Validation(format!(
                    "missing asset file: {entry_name}"
                )));
            }
        }
    }

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
    use beambench_core::object::{ObjectData, ProjectObject};
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
}
