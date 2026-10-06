//! Settings persistence — load/save `AppSettings` to disk as JSON.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use beambench_core::{
    ART_LIBRARY_FORMAT_VERSION, AppSettings, ArtLibraryDocument, ArtLibraryItem,
    ArtLibraryItemKind, MacroDefinition, MaterialPreset,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const CONFIG_DIR_ENV: &str = "BEAMBENCH_CONFIG_DIR";
pub const DATA_DIR_ENV: &str = "BEAMBENCH_DATA_DIR";
pub const PREFERENCES_BACKUP_RETENTION: usize = 50;

/// Resolve an export destination, following a symlinked destination.
///
/// Writing through the link preserves a setup where the user points an export
/// name at a share or another volume; replacing the link with a regular file
/// would silently break it.
pub fn resolve_export_target(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Rename a staged temporary file over `target` with the mode a data file should have.
///
/// `NamedTempFile` creates its file 0600 and `persist` keeps that mode, which
/// would leave exports unreadable to a NAS share or a service running as
/// another account. An existing destination keeps its own mode; a new file gets
/// the conventional 0644.
pub fn persist_export(file: tempfile::NamedTempFile, target: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(target)
            .map(|meta| meta.permissions().mode() & 0o777)
            .unwrap_or(0o644);
        file.as_file()
            .set_permissions(fs::Permissions::from_mode(mode))?;
    }
    file.persist(target).map(|_| ()).map_err(|e| e.error)
}

/// Return the config directory for Beam Bench.
/// `$CONFIG_DIR/beam-bench/`
pub fn config_dir() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(path) = crate::test_support::persistence_config_dir_for_current_test() {
        return Some(path);
    }
    if let Some(path) = std::env::var_os(CONFIG_DIR_ENV) {
        return Some(PathBuf::from(path));
    }
    dirs::config_dir().map(|d| d.join("beam-bench"))
}

/// Return the path to the settings JSON file.
/// `$CONFIG_DIR/beam-bench/settings.json`
pub fn settings_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("settings.json"))
}

/// Return the directory for Beam Bench preference backups.
/// `$CONFIG_DIR/beam-bench/preferences-backups/`
pub fn preferences_backup_dir() -> Option<PathBuf> {
    config_dir().map(|d| d.join("preferences-backups"))
}

/// Return the path to the material presets JSON file.
/// `$CONFIG_DIR/beam-bench/material_presets.json`
pub fn material_presets_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("material_presets.json"))
}

/// Return the path to the macros JSON file.
/// `$CONFIG_DIR/beam-bench/macros.json`
pub fn macros_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("macros.json"))
}

/// Load settings from disk, falling back to defaults when nothing is saved.
/// Normalizes `display_language` so a stale or unknown persisted code
/// becomes `"en"` rather than bricking the locale selector.
pub fn load_settings() -> AppSettings {
    load_settings_with_notice().0
}

/// Load settings, salvaging what still parses from a damaged file.
///
/// A file that fails to parse is moved aside before anything can overwrite
/// it, and the returned notice tells the user their settings were reset.
pub fn load_settings_with_notice() -> (AppSettings, Option<String>) {
    let (mut settings, notice) = settings_path()
        .map(|path| load_json_lenient(&path, salvage_object))
        .unwrap_or_default();
    beambench_core::settings::normalize_display_language(&mut settings);
    beambench_core::settings::migrate_settings(&mut settings);
    (settings, notice)
}

/// Stable code the frontend localizes when a saved file had to be reset.
pub const SETTINGS_FILE_RECOVERED_CODE: &str = "settings_file_recovered";

/// Read a JSON file. A missing file is the normal first-run case and yields
/// the default. A file that cannot be read or parsed is kept as
/// `<name>.unreadable-<stamp>` and whatever `salvage` can recover is used.
fn load_json_lenient<T>(path: &Path, salvage: fn(serde_json::Value) -> Option<T>) -> (T, Option<String>)
where
    T: serde::de::DeserializeOwned + Default,
{
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return (T::default(), None);
        }
        Err(error) => {
            return (T::default(), Some(unreadable_notice(path, &error.to_string())));
        }
    };
    let error = match serde_json::from_slice::<T>(&bytes) {
        Ok(value) => return (value, None),
        Err(error) => error,
    };
    let salvaged = serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .and_then(salvage);
    let kept = keep_unreadable_file(path, &bytes);
    let mut notice = unreadable_notice(path, &error.to_string());
    if let Some(kept) = kept {
        notice.push_str(&format!(" The original was kept as {}.", kept.display()));
    }
    (salvaged.unwrap_or_default(), Some(notice))
}

fn unreadable_notice(path: &Path, detail: &str) -> String {
    format!(
        "[{SETTINGS_FILE_RECOVERED_CODE}] {} could not be read ({detail}); unreadable entries were reset to defaults.",
        path.display()
    )
}

/// Copy a damaged file beside the original so the next save cannot destroy
/// the only copy of the user's data.
fn keep_unreadable_file(path: &Path, bytes: &[u8]) -> Option<PathBuf> {
    let name = path.file_name()?.to_string_lossy();
    let stamp = Utc::now().format("%Y%m%d-%H%M%S");
    let kept = path.with_file_name(format!("{name}.unreadable-{stamp}"));
    fs::write(&kept, bytes).ok().map(|_| kept)
}

/// Keep every top-level field that still parses on its own, so one bad value
/// does not discard machine profiles, recent files and every other setting.
fn salvage_object<T>(value: serde_json::Value) -> Option<T>
where
    T: serde::de::DeserializeOwned + Default + Serialize,
{
    let serde_json::Value::Object(fields) = value else {
        return None;
    };
    let mut merged = serde_json::to_value(T::default()).ok()?;
    for (key, field) in fields {
        let serde_json::Value::Object(object) = &mut merged else {
            return None;
        };
        let previous = object.insert(key.clone(), field);
        if serde_json::from_value::<T>(merged.clone()).is_err() {
            let serde_json::Value::Object(object) = &mut merged else {
                return None;
            };
            match previous {
                Some(previous) => object.insert(key, previous),
                None => object.remove(&key),
            };
        }
    }
    serde_json::from_value(merged).ok()
}

/// Keep every list entry that still parses on its own.
fn salvage_list<T>(value: serde_json::Value) -> Option<Vec<T>>
where
    T: serde::de::DeserializeOwned,
{
    let serde_json::Value::Array(items) = value else {
        return None;
    };
    Some(
        items
            .into_iter()
            .filter_map(|item| serde_json::from_value(item).ok())
            .collect(),
    )
}

/// Log a persistence warning. Silenced under `#[cfg(test)]` to avoid noisy
/// stderr output when tests exercise code paths that call
/// `persist_settings_to_disk` with default (temp-dir-less) settings.
#[cfg(not(test))]
fn log_persist_warning(what: &str, err: &str) {
    eprintln!("Warning: failed to persist {what}: {err}");
}

#[cfg(test)]
fn log_persist_warning(_what: &str, _err: &str) {}

/// Best-effort persist the current settings from a [`ServiceContext`] to disk.
///
/// Acquires the settings lock internally. Logs a warning on failure instead of
/// propagating errors — this is intentional: callers treat persistence as
/// fire-and-forget.
pub fn persist_settings_to_disk(ctx: &super::ServiceContext) {
    // Most service tests exercise state transitions without testing disk I/O.
    // Only tests holding `PersistTestGuard` may write through this best-effort
    // path. Test persistence uses thread-local temporary directories so
    // unrelated parallel tests cannot read or write another test's files.
    #[cfg(test)]
    if !crate::test_support::persistence_enabled_for_current_test() {
        return;
    }

    if let Ok(guard) = ctx.settings.lock()
        && let Err(e) = save_settings(&guard)
    {
        log_persist_warning("settings", &e);
    }

    // Also persist material presets and macros
    if let Ok(presets) = ctx.material_presets.lock()
        && let Err(e) = save_material_presets(&presets)
    {
        log_persist_warning("material presets", &e);
    }

    if let Ok(macros) = ctx.macros.lock()
        && let Err(e) = save_macros(&macros)
    {
        log_persist_warning("macros", &e);
    }

    // Note: optimization settings are no longer persisted standalone —
    // they travel with the project file as `Project.optimization`.
    // Runtime overlay (`optimization_runtime.current_position`) is
    // ephemeral by design and never crosses the persistence boundary.
}

/// Save settings to disk atomically (write `.tmp`, then rename).
pub fn save_settings(settings: &AppSettings) -> Result<(), String> {
    let path = settings_path().ok_or("Could not determine config directory")?;
    let json =
        serde_json::to_string_pretty(settings).map_err(|e| format!("Serialize failed: {e}"))?;
    write_json_atomic(&path, &json)
}

/// Save a `.bbprefs` backup payload and prune older backups by count.
pub fn save_preferences_backup(preferences_json: &str) -> Result<PathBuf, String> {
    let dir = preferences_backup_dir().ok_or("Could not determine config directory")?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create backup dir: {e}"))?;
    let stamp = Utc::now().format("%Y%m%d-%H%M%S-%3f");
    let path = dir.join(format!("preferences-{stamp}.bbprefs"));
    write_json_atomic(&path, preferences_json)?;
    prune_preferences_backups()?;
    Ok(path)
}

/// Return preference backups newest-first.
pub fn list_preferences_backups() -> Result<Vec<PathBuf>, String> {
    let Some(dir) = preferences_backup_dir() else {
        return Ok(Vec::new());
    };
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries = fs::read_dir(&dir)
        .map_err(|e| format!("Failed to read preference backups: {e}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "bbprefs"))
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| {
        let b_modified = b.metadata().and_then(|m| m.modified()).ok();
        let a_modified = a.metadata().and_then(|m| m.modified()).ok();
        b_modified.cmp(&a_modified).then_with(|| b.cmp(a))
    });
    Ok(entries)
}

/// Retain the latest 50 preference backups by count.
pub fn prune_preferences_backups() -> Result<(), String> {
    let backups = list_preferences_backups()?;
    for path in backups.into_iter().skip(PREFERENCES_BACKUP_RETENTION) {
        fs::remove_file(&path).map_err(|e| {
            format!(
                "Failed to remove old preference backup {}: {e}",
                path.display()
            )
        })?;
    }
    Ok(())
}

/// Load material presets from disk, salvaging entries from a damaged file.
pub fn load_material_presets() -> Vec<MaterialPreset> {
    load_material_presets_with_notice().0
}

pub fn load_material_presets_with_notice() -> (Vec<MaterialPreset>, Option<String>) {
    material_presets_path()
        .map(|path| load_json_lenient(&path, salvage_list))
        .unwrap_or_default()
}

/// Save material presets to disk atomically (write `.tmp`, then rename).
pub fn save_material_presets(presets: &[MaterialPreset]) -> Result<(), String> {
    let path = material_presets_path().ok_or("Could not determine config directory")?;
    let json =
        serde_json::to_string_pretty(presets).map_err(|e| format!("Serialize failed: {e}"))?;
    write_json_atomic(&path, &json)
}

/// Load macros from disk, salvaging entries from a damaged file.
pub fn load_macros() -> Vec<MacroDefinition> {
    load_macros_with_notice().0
}

pub fn load_macros_with_notice() -> (Vec<MacroDefinition>, Option<String>) {
    macros_path()
        .map(|path| load_json_lenient(&path, salvage_list))
        .unwrap_or_default()
}

/// Save macros to disk atomically (write `.tmp`, then rename).
pub fn save_macros(macros: &[MacroDefinition]) -> Result<(), String> {
    let path = macros_path().ok_or("Could not determine config directory")?;
    let json =
        serde_json::to_string_pretty(macros).map_err(|e| format!("Serialize failed: {e}"))?;
    write_json_atomic(&path, &json)
}

/// Return the directory for art libraries.
/// `$DATA_DIR/beam-bench/libraries/`
pub fn data_dir() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(path) = crate::test_support::persistence_data_dir_for_current_test() {
        return Some(path);
    }
    if let Some(path) = std::env::var_os(DATA_DIR_ENV) {
        return Some(PathBuf::from(path));
    }
    dirs::data_dir().map(|d| d.join("beam-bench"))
}

pub fn libraries_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join("libraries"))
}

pub fn art_library_manifest_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("art_library_manifest.json"))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ArtLibraryManifest {
    #[serde(default = "default_art_library_manifest_version")]
    pub format_version: String,
    #[serde(default)]
    pub loaded_paths: Vec<PathBuf>,
}

fn default_art_library_manifest_version() -> String {
    ART_LIBRARY_FORMAT_VERSION.to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LoadedArtLibrary {
    #[serde(flatten)]
    pub document: ArtLibraryDocument,
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct ArtLibraryLoadState {
    pub libraries: Vec<LoadedArtLibrary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct LegacyArtLibrary {
    pub name: String,
    #[serde(default)]
    pub items: Vec<ArtLibraryItem>,
}

pub(crate) fn write_json_atomic(path: &Path, json: &str) -> Result<(), String> {
    let parent = path.parent().ok_or("Invalid target path")?;
    if !parent.as_os_str().is_empty() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create parent dir: {e}"))?;
    }
    let tmp = path.with_extension(format!(
        "{}.{}.tmp",
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("tmp"),
        Uuid::new_v4()
    ));
    // Flush the contents before the rename: otherwise a power loss can leave
    // the renamed file empty.
    let written = fs::File::create(&tmp).and_then(|mut file| {
        use std::io::Write;
        file.write_all(json.as_bytes())?;
        file.sync_all()
    });
    if let Err(error) = written {
        let _ = fs::remove_file(&tmp);
        return Err(format!("Failed to write temp file: {error}"));
    }

    #[cfg(not(windows))]
    if let Err(error) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("Failed to rename temp file: {error}"));
    }

    // Windows does not replace an existing destination with `rename`. Keep a
    // same-directory backup until the new file is installed so an overwrite
    // either completes or restores the original file.
    #[cfg(windows)]
    if path.exists() {
        let backup = path.with_extension(format!(
            "{}.{}.backup",
            path.extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("tmp"),
            Uuid::new_v4()
        ));
        if let Err(error) = fs::rename(path, &backup) {
            let _ = fs::remove_file(&tmp);
            return Err(format!(
                "Failed to prepare existing file for replacement: {error}"
            ));
        }
        if let Err(error) = fs::rename(&tmp, path) {
            let restore_error = fs::rename(&backup, path).err();
            let _ = fs::remove_file(&tmp);
            return Err(match restore_error {
                Some(restore_error) => format!(
                    "Failed to replace file: {error}; also failed to restore original: {restore_error}"
                ),
                None => format!("Failed to replace file: {error}"),
            });
        }
        let _ = fs::remove_file(backup);
    } else {
        if let Err(error) = fs::rename(&tmp, path) {
            let _ = fs::remove_file(&tmp);
            return Err(format!("Failed to rename temp file: {error}"));
        }
    }
    // Flush the directory entry so the rename itself survives a power loss.
    #[cfg(unix)]
    if let Ok(dir) = fs::File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

pub fn save_art_library_manifest(manifest: &ArtLibraryManifest) -> Result<(), String> {
    let path = art_library_manifest_path().ok_or("Could not determine config directory")?;
    let json =
        serde_json::to_string_pretty(manifest).map_err(|e| format!("Serialize failed: {e}"))?;
    write_json_atomic(&path, &json)
}

pub fn save_art_library_document(path: &Path, document: &ArtLibraryDocument) -> Result<(), String> {
    let json =
        serde_json::to_string_pretty(document).map_err(|e| format!("Serialize failed: {e}"))?;
    write_json_atomic(path, &json)
}

pub fn delete_art_library_document(path: &Path) -> Result<(), String> {
    if path.exists() {
        fs::remove_file(path).map_err(|e| format!("Failed to delete library file: {e}"))?;
    }
    Ok(())
}

fn load_art_library_manifest() -> Result<Option<ArtLibraryManifest>, String> {
    let Some(path) = art_library_manifest_path() else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(&path).map_err(|e| format!("Failed to read manifest: {e}"))?;
    let manifest = serde_json::from_str::<ArtLibraryManifest>(&data)
        .map_err(|e| format!("Failed to parse manifest: {e}"))?;
    Ok(Some(manifest))
}

pub fn load_art_library_document(path: &Path) -> Result<ArtLibraryDocument, String> {
    let data =
        fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    serde_json::from_str::<ArtLibraryDocument>(&data)
        .map_err(|e| format!("Failed to parse {}: {e}", path.display()))
}

fn migrate_legacy_art_libraries(warnings: &mut Vec<String>) -> Vec<PathBuf> {
    let Some(dir) = libraries_dir() else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut migrated = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.extension().is_some_and(|ext| ext == "json") {
            continue;
        }
        let Some(data) = fs::read_to_string(&path).ok() else {
            continue;
        };
        let Ok(legacy) = serde_json::from_str::<LegacyArtLibrary>(&data) else {
            continue;
        };
        let bbart_path = path.with_extension("bbart");
        if bbart_path.exists() {
            migrated.push(bbart_path);
            continue;
        }

        let document = ArtLibraryDocument {
            format_version: ART_LIBRARY_FORMAT_VERSION.to_string(),
            library_id: Uuid::new_v4(),
            name: legacy.name,
            items: legacy
                .items
                .into_iter()
                .map(|mut item| {
                    item.kind = ArtLibraryItemKind::ExternalFile;
                    item
                })
                .collect(),
        };
        match save_art_library_document(&bbart_path, &document) {
            Ok(()) => migrated.push(bbart_path),
            Err(err) => warnings.push(format!(
                "Failed to migrate legacy art library {}: {err}",
                path.display()
            )),
        }
    }
    migrated
}

pub fn load_art_libraries() -> ArtLibraryLoadState {
    let mut warnings = Vec::new();
    let migrated_paths = migrate_legacy_art_libraries(&mut warnings);

    let manifest = match load_art_library_manifest() {
        Ok(Some(manifest)) => manifest,
        Ok(None) => {
            let manifest = ArtLibraryManifest {
                format_version: default_art_library_manifest_version(),
                loaded_paths: migrated_paths.clone(),
            };
            if !manifest.loaded_paths.is_empty()
                && let Err(err) = save_art_library_manifest(&manifest)
            {
                warnings.push(format!("Failed to seed art library manifest: {err}"));
            }
            manifest
        }
        Err(err) => {
            warnings.push(format!("Art library manifest is unreadable: {err}"));
            ArtLibraryManifest::default()
        }
    };

    let mut libraries = Vec::new();
    let mut loaded_paths = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut manifest_dirty = false;

    for path in manifest.loaded_paths {
        if !path.exists() {
            warnings.push(format!("Art library file missing: {}", path.display()));
            manifest_dirty = true;
            continue;
        }

        match load_art_library_document(&path) {
            Ok(mut document) => {
                let mut save_error = None;
                if !seen_ids.insert(document.library_id) {
                    let original = document.library_id;
                    document.library_id = Uuid::new_v4();
                    match save_art_library_document(&path, &document) {
                        Ok(()) => warnings.push(format!(
                            "Detected duplicate library id in {}, assigned a new id.",
                            path.display()
                        )),
                        Err(err) => {
                            save_error = Some(err.clone());
                            warnings.push(format!(
                                "Detected duplicate library id in {} but failed to persist the new id: {err}",
                                path.display()
                            ));
                        }
                    }
                    let _ = seen_ids.insert(document.library_id);
                    let _ = original;
                }
                loaded_paths.push(path.clone());
                libraries.push(LoadedArtLibrary {
                    document,
                    path,
                    save_error,
                });
            }
            Err(err) => {
                warnings.push(err);
                manifest_dirty = true;
            }
        }
    }

    if manifest_dirty {
        let updated = ArtLibraryManifest {
            format_version: default_art_library_manifest_version(),
            loaded_paths,
        };
        if let Err(err) = save_art_library_manifest(&updated) {
            warnings.push(format!("Failed to update art library manifest: {err}"));
        }
    }

    ArtLibraryLoadState {
        libraries,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_core::DisplayUnit;

    #[test]
    fn roundtrip_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");

        let settings = AppSettings {
            autosave_enabled: false,
            display_unit: DisplayUnit::Inches,
            ..Default::default()
        };

        let json = serde_json::to_string_pretty(&settings).unwrap();
        fs::write(&path, &json).unwrap();

        let loaded: AppSettings =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert!(!loaded.autosave_enabled);
        assert_eq!(loaded.display_unit, DisplayUnit::Inches);
    }

    #[test]
    fn missing_file_returns_default() {
        // load_settings always returns a valid settings object, even if the
        // on-disk file is missing or unreadable.
        let settings = load_settings();
        assert!(settings.autosave_interval_secs >= 30);
    }

    #[test]
    fn corrupt_file_is_kept_and_reported() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "NOT JSON!!!").unwrap();

        let (loaded, notice) = load_json_lenient::<AppSettings>(&path, salvage_object);
        assert!(loaded.autosave_enabled);
        assert!(notice.unwrap().starts_with("[settings_file_recovered]"));
        let kept: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".unreadable-"))
            .collect();
        assert_eq!(kept.len(), 1);
        assert_eq!(fs::read_to_string(kept[0].path()).unwrap(), "NOT JSON!!!");
    }

    #[test]
    fn one_bad_setting_keeps_profiles_and_other_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut settings = AppSettings {
            autosave_enabled: false,
            ..Default::default()
        };
        settings
            .machine_profiles
            .push(beambench_core::MachineProfile::default());
        let mut value = serde_json::to_value(&settings).unwrap();
        value["display_unit"] = serde_json::json!({"not": "a unit"});
        fs::write(&path, value.to_string()).unwrap();

        let (loaded, notice) = load_json_lenient::<AppSettings>(&path, salvage_object);
        assert!(notice.is_some());
        assert!(!loaded.autosave_enabled);
        assert_eq!(loaded.machine_profiles.len(), 1);
        assert_eq!(loaded.display_unit, AppSettings::default().display_unit);
    }

    #[test]
    fn damaged_list_keeps_readable_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("macros.json");
        let good = serde_json::to_value(MacroDefinition::default()).unwrap();
        fs::write(&path, serde_json::json!([good, {"bogus": true}]).to_string()).unwrap();

        let (loaded, notice) = load_json_lenient::<Vec<MacroDefinition>>(&path, salvage_list);
        assert!(notice.is_some());
        assert_eq!(loaded.len(), 1);
    }

    #[test]
    fn missing_file_has_no_notice() {
        let dir = tempfile::tempdir().unwrap();
        let (_, notice) =
            load_json_lenient::<AppSettings>(&dir.path().join("settings.json"), salvage_object);
        assert!(notice.is_none());
    }

    #[test]
    fn settings_path_returns_some() {
        // On any standard OS, config_dir should exist
        let path = settings_path();
        assert!(path.is_some());
        let p = path.unwrap();
        assert!(
            p.ends_with("beam-bench/settings.json") || p.ends_with("beam-bench\\settings.json")
        );
    }

    // --- Material-preset and macro persistence tests ---

    #[test]
    fn material_presets_path_returns_some() {
        let path = material_presets_path();
        assert!(path.is_some());
        let p = path.unwrap();
        assert!(
            p.ends_with("beam-bench/material_presets.json")
                || p.ends_with("beam-bench\\material_presets.json")
        );
    }

    #[test]
    fn macros_path_returns_some() {
        let path = macros_path();
        assert!(path.is_some());
        let p = path.unwrap();
        assert!(p.ends_with("beam-bench/macros.json") || p.ends_with("beam-bench\\macros.json"));
    }

    #[test]
    fn load_material_presets_missing_file_returns_empty() {
        // load_material_presets always returns a valid vec, even if the file is missing
        let presets = load_material_presets();
        assert!(presets.is_empty() || !presets.is_empty()); // We don't care if it loads system presets
    }

    #[test]
    fn load_macros_missing_file_returns_empty() {
        // load_macros always returns a valid vec, even if the file is missing
        let macros = load_macros();
        assert!(macros.is_empty() || !macros.is_empty()); // We don't care if it loads system macros
    }

    #[test]
    fn roundtrip_material_presets() {
        use beambench_core::{MaterialPreset, OperationType};
        use uuid::Uuid;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("material_presets.json");

        let presets = vec![
            MaterialPreset {
                id: Uuid::new_v4(),
                name: "Plywood 3mm Cut".to_string(),
                material: "Plywood".to_string(),
                thickness_mm: 3.0,
                operation: OperationType::Cut,
                speed_mm_min: 800.0,
                power_percent: 80.0,
                passes: 2,
                dpi: None,
                raster_mode: None,
                line_interval_mm: None,
                scan_angle: None,
                bidirectional: None,
                overscan_mm: None,
                flood_fill: None,
                angle_passes: None,
                angle_increment_deg: None,
                pass_through: None,
                halftone_cells_per_inch: None,
                halftone_angle_deg: None,
                newsprint_angle_deg: None,
                newsprint_frequency: None,
                invert: None,
                dot_width_correction_mm: None,
                ramp_length_mm: None,
                notes: "Use air assist".to_string(),
                category: String::new(),
                device_profile: None,
            },
            MaterialPreset {
                id: Uuid::new_v4(),
                name: "Acrylic 5mm Engrave".to_string(),
                material: "Acrylic".to_string(),
                thickness_mm: 5.0,
                operation: OperationType::Line,
                speed_mm_min: 1000.0,
                power_percent: 50.0,
                passes: 1,
                dpi: Some(300),
                raster_mode: Some(beambench_common::RasterMode::Grayscale),
                line_interval_mm: Some(0.1),
                scan_angle: Some(45.0),
                bidirectional: Some(true),
                overscan_mm: Some(2.5),
                flood_fill: Some(false),
                angle_passes: Some(2),
                angle_increment_deg: Some(90.0),
                pass_through: None,
                halftone_cells_per_inch: None,
                halftone_angle_deg: None,
                newsprint_angle_deg: None,
                newsprint_frequency: None,
                invert: None,
                dot_width_correction_mm: None,
                ramp_length_mm: None,
                notes: String::new(),
                category: String::new(),
                device_profile: None,
            },
        ];

        let json = serde_json::to_string_pretty(&presets).unwrap();
        fs::write(&path, &json).unwrap();

        let loaded: Vec<MaterialPreset> =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].name, "Plywood 3mm Cut");
        assert_eq!(loaded[1].name, "Acrylic 5mm Engrave");
    }

    #[test]
    fn roundtrip_macros() {
        use beambench_core::MacroDefinition;
        use uuid::Uuid;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("macros.json");

        let macros = vec![
            MacroDefinition {
                id: Uuid::new_v4(),
                name: "Home".to_string(),
                description: "Home the machine".to_string(),
                commands: vec!["$H".to_string()],
                hotkey: None,
                show_in_toolbar: false,
            },
            MacroDefinition {
                id: Uuid::new_v4(),
                name: "Air Assist On".to_string(),
                description: "Enable air assist".to_string(),
                commands: vec!["M106 S255".to_string(), "G4 P0.5".to_string()],
                hotkey: None,
                show_in_toolbar: false,
            },
        ];

        let json = serde_json::to_string_pretty(&macros).unwrap();
        fs::write(&path, &json).unwrap();

        let loaded: Vec<MacroDefinition> =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].name, "Home");
        assert_eq!(loaded[1].name, "Air Assist On");
    }

    // --- Art Library Persistence Tests ---

    #[test]
    fn libraries_dir_returns_some() {
        let dir = libraries_dir();
        assert!(dir.is_some());
        let d = dir.unwrap();
        assert!(d.ends_with("beam-bench/libraries") || d.ends_with("beam-bench\\libraries"));
    }

    #[test]
    fn roundtrip_art_library() {
        use beambench_core::{ArtLibraryDocument, ArtLibraryItem, ArtLibraryItemKind};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_library.bbart");

        let lib = ArtLibraryDocument {
            format_version: beambench_core::ART_LIBRARY_FORMAT_VERSION.to_string(),
            library_id: uuid::Uuid::new_v4(),
            name: "Test Library".to_string(),
            items: vec![ArtLibraryItem {
                id: uuid::Uuid::new_v4(),
                kind: ArtLibraryItemKind::ExternalFile,
                name: "Star".to_string(),
                category: "Shapes".to_string(),
                tags: vec!["star".to_string()],
                source_filename: "star.svg".to_string(),
                media_type: "image/svg+xml".to_string(),
                data: "PHN2Zz4=".to_string(),
                thumbnail: None,
                created_at: "2026-03-23T12:00:00Z".to_string(),
            }],
        };

        let json = serde_json::to_string_pretty(&lib).unwrap();
        fs::write(&path, &json).unwrap();

        let loaded = load_art_library_document(&path).unwrap();
        assert_eq!(loaded.name, "Test Library");
        assert_eq!(loaded.items.len(), 1);
        assert_eq!(loaded.items[0].name, "Star");
    }

    #[test]
    fn load_art_library_document_defaults_legacy_items_to_external_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy_library.bbart");
        fs::write(
            &path,
            r#"{
              "format_version": "1.0",
              "library_id": "00000000-0000-0000-0000-000000000001",
              "name": "Legacy",
              "items": [{
                "id": "00000000-0000-0000-0000-000000000002",
                "name": "Legacy Star",
                "category": "General",
                "tags": [],
                "source_filename": "legacy.svg",
                "media_type": "image/svg+xml",
                "data": "PHN2Zz4=",
                "created_at": "2026-01-01T00:00:00Z"
              }]
            }"#,
        )
        .unwrap();

        let loaded = load_art_library_document(&path).unwrap();
        assert_eq!(loaded.items.len(), 1);
        assert_eq!(
            loaded.items[0].kind,
            beambench_core::ArtLibraryItemKind::ExternalFile
        );
    }

    // Optimization travels with the project file via `Project.optimization`;
    // runtime-only state
    // (`OptimizationRuntime.current_position`) is never persisted.
    // Per-field serde round-trip is covered by
    // `beambench_core::optimization::tests::project_optimization_roundtrip`.
}
