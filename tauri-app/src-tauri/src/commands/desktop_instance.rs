use std::fs::{self, File, TryLockError};
use std::io::{self, Error};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use beambench_service::persist;
use tauri::{Manager, State};

/// Holding the file open reserves the storage slot until process exit, including crashes.
/// Never unlink a lock file: another process might already have opened its inode.
pub struct DesktopInstance {
    pub number: u32,
    _slot: File,
    update_gate: PathBuf,
    _secondary_guard: Option<File>,
    install_guard: Mutex<Option<File>>,
}

fn open_lock(path: &Path) -> io::Result<File> {
    File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
}

impl DesktopInstance {
    fn reserve(root: &Path) -> io::Result<Self> {
        let locks = root.join("instances");
        fs::create_dir_all(&locks)?;
        let update_gate = locks.join("update.lock");
        let gate = open_lock(&update_gate)?;
        // A relaunch during installation waits until the previous process exits.
        gate.lock_shared()?;
        for number in 1..=u32::MAX {
            let file = open_lock(&locks.join(format!("{number}.lock")))?;
            match file.try_lock() {
                Ok(()) => {
                    return Ok(Self {
                        number,
                        _slot: file,
                        update_gate,
                        _secondary_guard: (number > 1).then_some(gate),
                        install_guard: Mutex::default(),
                    });
                }
                Err(TryLockError::WouldBlock) => continue,
                Err(TryLockError::Error(error)) => return Err(error),
            }
        }
        Err(Error::other("No available Beam Bench instance"))
    }

    pub fn start() -> io::Result<Self> {
        let root = persist::config_dir().ok_or_else(|| Error::other("Missing config directory"))?;
        let instance = Self::reserve(&root)?;
        beambench_common::desktop_instance::initialize(instance.number);
        let settings_path =
            persist::settings_path().ok_or_else(|| Error::other("Missing settings path"))?;
        if instance.number > 1 && !settings_path.try_exists()? {
            // Copy the last committed preferences once. Do not run recovery/migrations
            // on another running desktop's files or automatically expose a second API.
            let mut settings: beambench_core::AppSettings =
                match fs::read(root.join("settings.json")) {
                    Ok(json) => serde_json::from_slice(&json).unwrap_or_default(),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => Default::default(),
                    Err(error) => return Err(error),
                };
            settings.api_enabled = false;
            settings.api_localhost_only = true;
            settings.active_profile_id = None;
            settings.recent_files.clear();
            persist::save_settings(&settings).map_err(Error::other)?;
        }
        Ok(instance)
    }

    fn reserve_update(&self) -> Result<(), String> {
        if self.number != 1 {
            return Err(
                "Install updates from the first Beam Bench window after closing the others.".into(),
            );
        }
        let mut guard = self.install_guard.lock().map_err(|e| e.to_string())?;
        let gate = open_lock(&self.update_gate).map_err(|e| e.to_string())?;
        gate.try_lock().map_err(|e| match e {
            TryLockError::WouldBlock => {
                "Close the other Beam Bench windows before installing the update.".into()
            }
            TryLockError::Error(error) => error.to_string(),
        })?;
        *guard = Some(gate);
        Ok(())
    }
}

#[tauri::command]
pub fn begin_instance_update(instance: State<'_, DesktopInstance>) -> Result<(), String> {
    instance.reserve_update()
}

#[tauri::command]
pub fn end_instance_update(instance: State<'_, DesktopInstance>) -> Result<(), String> {
    instance
        .install_guard
        .lock()
        .map_err(|e| e.to_string())?
        .take();
    Ok(())
}

#[tauri::command]
pub fn open_laser_window(app: tauri::AppHandle) -> Result<(), String> {
    let exe = tauri::process::current_binary(&app.env()).map_err(|e| e.to_string())?;
    let mut child = std::process::Command::new(exe)
        .spawn()
        .map_err(|e| e.to_string())?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secondary_starts_with_profiles_but_owns_its_settings_and_recovery() {
        // The instance number is fixed at startup. Keep the parent test process
        // unmodified so unrelated tests can keep using primary paths.
        const CHILD: &str = "BEAMBENCH_INSTANCE_STORAGE_TEST";
        if std::env::var_os(CHILD).is_none() {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "commands::desktop_instance::tests::secondary_starts_with_profiles_but_owns_its_settings_and_recovery", "--nocapture"])
                .env(CHILD, "1")
                .output().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            return;
        }
        let root = persist::config_dir().unwrap();
        let data = persist::data_dir().unwrap();
        let _first = DesktopInstance::reserve(&root).unwrap();
        let profile = beambench_core::MachineProfile::default();
        let original = beambench_core::AppSettings {
            active_profile_id: Some(profile.id),
            machine_profiles: vec![profile.clone()],
            api_enabled: true,
            ..Default::default()
        };
        persist::save_settings(&original).unwrap();
        let primary_bytes = fs::read(root.join("settings.json")).unwrap();
        let _second = DesktopInstance::start().unwrap();
        assert_eq!(persist::config_dir().unwrap(), root.join("instances/2"));
        assert_eq!(persist::data_dir().unwrap(), data.join("instances/2"));
        let settings = persist::load_settings();
        assert_eq!(settings.machine_profiles[0].id, profile.id);
        assert!(settings.active_profile_id.is_none());
        assert!(!settings.api_enabled);
        let ctx = beambench_service::ServiceContext::new();
        beambench_service::ops::project::create_project(&ctx, "second").unwrap();
        beambench_service::ops::persistence::autosave_project(&ctx).unwrap();
        assert!(root.join("instances/2/recovery").is_dir());
        assert!(!root.join("recovery").exists());
        assert_eq!(fs::read(root.join("settings.json")).unwrap(), primary_bytes);
    }

    #[test]
    fn windows_own_distinct_slots_and_reuse_only_released_slots() {
        let dir = tempfile::tempdir().unwrap();
        let open = || DesktopInstance::reserve(dir.path()).unwrap();
        let first = open();
        let second = open();
        let third = open();
        let fourth = open();
        assert_eq!(
            [first.number, second.number, third.number, fourth.number],
            [1, 2, 3, 4]
        );
        assert!(first.reserve_update().is_err());
        assert!(second.reserve_update().is_err());
        drop(second);
        let replacement = open();
        assert_eq!(replacement.number, 2);
        drop((replacement, third, fourth));
        first.reserve_update().unwrap();
        assert!(matches!(
            open_lock(&first.update_gate).unwrap().try_lock_shared(),
            Err(TryLockError::WouldBlock)
        ));
    }
}
