use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use beambench_common::RasterAdjustments;
use beambench_core::settings::{ExportSettings, ImagePreset, PanelLayout};
use beambench_core::{
    AppSettings, AppStatus, BuildInfo, CursorSize, IconSize, SpeedTimeUnit, UiTheme,
};
use beambench_service::ServiceContext;
use beambench_service::ops::app::{self, UpdateAppSettingsInput};
use beambench_service::ops::preferences::{
    export_preferences_to_path, import_preferences_from_path,
    reset_preferences_to_defaults as reset_preferences_to_defaults_op,
};
use beambench_service::persist;
use tauri::{State, WebviewWindow};
use tauri_plugin_opener::OpenerExt;

async fn run_settings_update<T, F>(svc: Arc<ServiceContext>, operation: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&ServiceContext) -> beambench_service::ServiceResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || operation(&svc).map_err(String::from))
        .await
        .map_err(|e| format!("Settings update worker failed: {e}"))?
}

#[tauri::command]
pub fn get_app_status(svc: State<'_, Arc<ServiceContext>>) -> Result<AppStatus, String> {
    app::get_app_status(&svc).map_err(Into::into)
}

/// Called by the frontend after the user resolves the unsaved-changes prompt
/// (saved or chose to discard). Sets the confirmation flag and closes the
/// window from the backend: the browser `window.close()` is unreliable in a
/// Tauri webview, which left the window open on the first Don't Save and
/// forced a second close click. The close re-fires CloseRequested, which now
/// sees the confirmation flag and proceeds.
#[tauri::command]
pub fn confirm_window_close(
    window: WebviewWindow,
    state: State<'_, crate::state::CloseConfirmed>,
) -> Result<(), String> {
    state
        .inner()
        .0
        .store(true, std::sync::atomic::Ordering::Release);
    window.close().map_err(|e| e.to_string())
}

/// Request a normal window close from the backend. This fires Tauri's
/// CloseRequested event, so unsaved-change handling stays centralized there.
#[tauri::command]
pub fn request_window_close(window: WebviewWindow) -> Result<(), String> {
    window.close().map_err(|e| e.to_string())
}

/// Keep the native title bar in sync with the project shown by this webview.
/// A backend command avoids granting the frontend broad window-mutation
/// permissions just to update one bounded string.
#[tauri::command]
pub fn set_window_title(window: WebviewWindow, title: String) -> Result<(), String> {
    let title = title.replace(['\r', '\n'], " ");
    let title = title.trim();
    if title.is_empty() {
        return Err("Window title cannot be empty".to_string());
    }
    if title.chars().count() > 512 {
        return Err("Window title is too long".to_string());
    }
    window.set_title(title).map_err(|e| e.to_string())
}

/// Called by the frontend once React has mounted. Until this fires, the
/// webview may have failed to boot (an old system WebKit that cannot run the
/// bundled JS); the startup watchdog and the native Quit fallback key off
/// this flag.
#[tauri::command]
pub fn mark_frontend_ready(state: State<'_, crate::state::FrontendReady>) -> Result<(), String> {
    state
        .inner()
        .0
        .store(true, std::sync::atomic::Ordering::Release);
    tracing::info!("Frontend ready");
    Ok(())
}

#[tauri::command]
pub fn get_build_info() -> Result<BuildInfo, String> {
    Ok(app::get_build_info())
}

/// Why the in-app updater cannot swap the app bundle on this machine, if
/// anything. The updater plugin extracts the new app into the system temp
/// folder and renames the installed bundle into a temp backup; both steps
/// fail when the bundle is quarantine-translocated, on a read-only volume
/// (running from the mounted DMG), or on a different volume than the temp
/// folder, where rename returns EXDEV and users see the raw
/// "Cross-device link (os error 18)". Returns a code the frontend maps to
/// a plain-language remedy, or None when an update can proceed.
#[tauri::command]
pub fn get_update_environment_blocker() -> Result<Option<String>, String> {
    #[cfg(target_os = "macos")]
    {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        Ok(macos_update_environment_blocker(&exe))
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Windows updates run a signed installer and the Linux AppImage
        // path already picks a temp dir on the target volume.
        Ok(None)
    }
}

#[cfg(target_os = "macos")]
fn macos_update_environment_blocker(exe: &std::path::Path) -> Option<String> {
    // Climb from the executable to the .app bundle root. Dev builds run
    // straight from target/debug with no bundle: nothing to block.
    let bundle = exe
        .ancestors()
        .find(|p| p.extension().is_some_and(|ext| ext == "app"))?;

    // Gatekeeper runs quarantined apps from a randomized read-only mount.
    if bundle.to_string_lossy().contains("/AppTranslocation/") {
        return Some("not_in_applications".to_string());
    }

    let parent = bundle.parent()?;

    // Read-only volume probe (running from the mounted DMG). A plain
    // PermissionDenied is NOT a blocker: that is the normal /Applications
    // case for non-admin users, which the updater handles with an admin
    // prompt.
    let probe = parent.join(".beambench-update-probe");
    match std::fs::write(&probe, b"probe") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
        }
        Err(e) => {
            const EROFS: i32 = 30; // macOS: filesystem mounted read-only
            if e.raw_os_error() == Some(EROFS) {
                return Some("not_in_applications".to_string());
            }
        }
    }

    // Different volume than the temp folder: the updater's rename-based
    // swap cannot cross devices.
    use std::os::unix::fs::MetadataExt;
    let bundle_dev = std::fs::metadata(bundle).ok()?.dev();
    let tmp_dev = std::fs::metadata(std::env::temp_dir()).ok()?.dev();
    if bundle_dev != tmp_dev {
        return Some("other_disk".to_string());
    }

    None
}

#[tauri::command]
pub fn open_external_url(app: tauri::AppHandle, url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://") || url.starts_with("mailto:")) {
        return Err(format!("Refusing to open non-http(s)/mailto URL: {url}"));
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| format!("Failed to open URL: {e}"))
}

#[tauri::command]
pub fn get_app_settings(svc: State<'_, Arc<ServiceContext>>) -> Result<AppSettings, String> {
    app::get_app_settings(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn take_pending_notices(svc: State<'_, Arc<ServiceContext>>) -> Result<Vec<String>, String> {
    app::take_pending_notices(&svc).map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn update_app_settings(
    svc: State<'_, Arc<ServiceContext>>,
    display_unit: Option<String>,
    speed_time_unit: Option<SpeedTimeUnit>,
    autosave_enabled: Option<bool>,
    autosave_interval_secs: Option<u32>,
    api_enabled: Option<bool>,
    api_port: Option<u16>,
    api_localhost_only: Option<bool>,
    ui_theme: Option<UiTheme>,
    dark_mode: Option<bool>,
    antialiasing: Option<bool>,
    artwork_display_mode: Option<beambench_core::ArtworkDisplayMode>,
    transform_anchor: Option<beambench_common::AnchorPoint>,
    filled_rendering: Option<bool>,
    reduce_motion: Option<bool>,
    show_palette_labels: Option<bool>,
    cursor_size: Option<CursorSize>,
    toolbar_icon_size: Option<IconSize>,
    click_tolerance_px: Option<f64>,
    snap_threshold_px: Option<f64>,
    grid_spacing_mm: Option<f64>,
    nudge_step_mm: Option<f64>,
    nudge_step_fine_mm: Option<f64>,
    nudge_step_coarse_mm: Option<f64>,
    scroll_zoom: Option<bool>,
    debug_log_enabled: Option<bool>,
    panel_layout: Option<PanelLayout>,
    last_radius_mm: Option<f64>,
    export_settings: Option<ExportSettings>,
    allow_importing_to_tool_layers: Option<bool>,
    check_for_updates_on_startup: Option<bool>,
    update_snoozed_until: Option<String>,
    skipped_update_version: Option<String>,
    custom_hotkeys: Option<HashMap<String, String>>,
    display_language: Option<String>,
) -> Result<AppSettings, String> {
    let input = UpdateAppSettingsInput {
        display_unit,
        speed_time_unit,
        autosave_enabled,
        autosave_interval_secs,
        api_enabled,
        api_port,
        api_localhost_only,
        ui_theme,
        dark_mode,
        antialiasing,
        artwork_display_mode,
        transform_anchor,
        filled_rendering,
        reduce_motion,
        show_palette_labels,
        cursor_size,
        toolbar_icon_size,
        click_tolerance_px,
        snap_threshold_px,
        grid_spacing_mm,
        nudge_step_mm,
        nudge_step_fine_mm,
        nudge_step_coarse_mm,
        scroll_zoom,
        debug_log_enabled,
        panel_layout,
        last_radius_mm,
        export_settings,
        allow_importing_to_tool_layers,
        check_for_updates_on_startup,
        update_snoozed_until,
        skipped_update_version,
        custom_hotkeys,
        display_language,
    };
    run_settings_update(svc.inner().clone(), move |svc| {
        app::update_app_settings(svc, input)
    })
    .await
}

#[tauri::command]
pub fn export_preferences(
    svc: State<'_, Arc<ServiceContext>>,
    path: String,
) -> Result<String, String> {
    export_preferences_to_path(&svc, &PathBuf::from(path)).map_err(Into::into)
}

#[tauri::command]
pub async fn import_preferences(
    svc: State<'_, Arc<ServiceContext>>,
    path: String,
) -> Result<AppSettings, String> {
    run_settings_update(svc.inner().clone(), move |svc| {
        import_preferences_from_path(svc, &PathBuf::from(path))
    })
    .await
}

#[tauri::command]
pub async fn reset_preferences(svc: State<'_, Arc<ServiceContext>>) -> Result<AppSettings, String> {
    run_settings_update(svc.inner().clone(), reset_preferences_to_defaults_op).await
}

#[tauri::command]
pub fn open_preferences_folder(app: tauri::AppHandle) -> Result<(), String> {
    let dir = persist::config_dir().ok_or("Could not determine config directory")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create config directory: {e}"))?;
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| format!("Failed to open preferences folder: {e}"))
}

// ---------------------------------------------------------------------------
// System fonts and display settings
// ---------------------------------------------------------------------------

/// Return a hardcoded list of common font families.
/// A future version may use platform-specific APIs for live enumeration.
#[tauri::command]
pub fn get_system_fonts() -> Result<Vec<String>, String> {
    Ok(beambench_core::vector::text_to_path::available_font_families())
}

/// Update display-related settings fields.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn update_display_settings(
    svc: State<'_, Arc<ServiceContext>>,
    ui_theme: Option<UiTheme>,
    dark_mode: Option<bool>,
    antialiasing: Option<bool>,
    artwork_display_mode: Option<beambench_core::ArtworkDisplayMode>,
    filled_rendering: Option<bool>,
    reduce_motion: Option<bool>,
    show_palette_labels: Option<bool>,
    cursor_size: Option<CursorSize>,
    toolbar_icon_size: Option<IconSize>,
    click_tolerance_px: Option<f64>,
    snap_threshold_px: Option<f64>,
    scroll_zoom: Option<bool>,
    debug_log_enabled: Option<bool>,
) -> Result<AppSettings, String> {
    let input = UpdateAppSettingsInput {
        ui_theme,
        dark_mode,
        antialiasing,
        artwork_display_mode,
        filled_rendering,
        reduce_motion,
        show_palette_labels,
        cursor_size,
        toolbar_icon_size,
        click_tolerance_px,
        snap_threshold_px,
        scroll_zoom,
        debug_log_enabled,
        ..UpdateAppSettingsInput::default()
    };
    run_settings_update(svc.inner().clone(), move |svc| {
        app::update_app_settings(svc, input)
    })
    .await
}

// ---------------------------------------------------------------------------
// Image Adjustment Presets
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_image_presets(svc: State<'_, Arc<ServiceContext>>) -> Result<Vec<ImagePreset>, String> {
    let settings = svc.settings.lock().map_err(|e| format!("{e}"))?;
    Ok(settings.image_presets.clone())
}

#[tauri::command]
pub fn save_image_preset(
    svc: State<'_, Arc<ServiceContext>>,
    name: String,
    adjustments: RasterAdjustments,
) -> Result<(), String> {
    {
        let mut settings = svc.settings.lock().map_err(|e| format!("{e}"))?;
        settings.image_presets.retain(|p| p.name != name);
        settings
            .image_presets
            .push(ImagePreset { name, adjustments });
    }
    persist::persist_settings_to_disk(&svc);
    Ok(())
}

#[tauri::command]
pub fn delete_image_preset(
    svc: State<'_, Arc<ServiceContext>>,
    name: String,
) -> Result<(), String> {
    {
        let mut settings = svc.settings.lock().map_err(|e| format!("{e}"))?;
        settings.image_presets.retain(|p| p.name != name);
    }
    persist::persist_settings_to_disk(&svc);
    Ok(())
}

#[cfg(all(test, target_os = "macos"))]
mod update_environment_tests {
    use super::macos_update_environment_blocker;

    fn make_bundle(root: &std::path::Path, rel: &str) -> std::path::PathBuf {
        let exe = root.join(rel);
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"stub").unwrap();
        exe
    }

    #[test]
    fn dev_build_without_bundle_is_not_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let exe = make_bundle(dir.path(), "target/debug/beambench-tauri");
        assert_eq!(macos_update_environment_blocker(&exe), None);
    }

    #[test]
    fn translocated_bundle_is_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let exe = make_bundle(
            dir.path(),
            "AppTranslocation/9D4F/d/Beam Bench.app/Contents/MacOS/beambench-tauri",
        );
        assert_eq!(
            macos_update_environment_blocker(&exe),
            Some("not_in_applications".to_string())
        );
    }

    #[test]
    fn writable_bundle_on_temp_volume_is_not_blocked() {
        // tempdir lives on the same volume as std::env::temp_dir(), so a
        // normal writable bundle there must produce no blocker.
        let dir = tempfile::tempdir().unwrap();
        let exe = make_bundle(dir.path(), "Beam Bench.app/Contents/MacOS/beambench-tauri");
        assert_eq!(macos_update_environment_blocker(&exe), None);
    }
}

#[cfg(test)]
mod settings_worker_tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn blocking_settings_work_leaves_the_calling_runtime_responsive() {
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let ctx = Arc::new(ServiceContext::with_settings(AppSettings::default()));
        let worker = tokio::spawn(run_settings_update(ctx, move |_| {
            started_tx.send(()).unwrap();
            release_rx
                .recv_timeout(std::time::Duration::from_secs(2))
                .map_err(|e| beambench_service::ServiceError::internal(e.to_string()))?;
            Ok(())
        }));
        started_rx.await.unwrap();
        // This continuation can run while the blocking settings operation waits.
        tokio::task::yield_now().await;
        release_tx.send(()).unwrap();
        worker.await.unwrap().unwrap();
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn conflicting_api_restart_keeps_the_calling_runtime_responsive() {
        const TEST: &str = "commands::app::settings_worker_tests::conflicting_api_restart_keeps_the_calling_runtime_responsive";
        if std::env::var("BEAMBENCH_SETTINGS_WORKER_CHILD").as_deref() != Ok("1") {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", TEST, "--nocapture"])
                .env("BEAMBENCH_SETTINGS_WORKER_CHILD", "1")
                .env("BEAMBENCH_API_TOKEN", "a".repeat(40))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let occupied = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
        let settings = AppSettings {
            api_enabled: true,
            api_port: occupied.local_addr().unwrap().port(),
            ..Default::default()
        };
        let ctx = Arc::new(ServiceContext::with_settings(settings.clone()));
        let api = crate::state::ApiRuntime::default();
        api.sync_from_settings(ctx.clone(), &settings).unwrap();
        let worker_api = api.clone();
        let network = AppSettings {
            api_localhost_only: false,
            ..settings.clone()
        };
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let (started_tx, started_rx) = tokio::sync::oneshot::channel();
                let start = std::time::Instant::now();
                let worker_ctx = ctx.clone();
                let updating = tokio::spawn(run_settings_update(ctx.clone(), move |_| {
                    started_tx.send(()).unwrap();
                    worker_api
                        .sync_from_settings(worker_ctx, &network)
                        .map_err(beambench_service::ServiceError::internal)
                }));
                started_rx.await.unwrap();
                tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                assert!(start.elapsed() < std::time::Duration::from_millis(500));
                assert!(
                    !updating.is_finished(),
                    "The conflicting bind must still be retrying"
                );
                assert!(updating.await.unwrap().is_err());
            });
        // Recovery still serves the original port.
        assert!(std::net::TcpStream::connect(("127.0.0.1", settings.api_port)).is_ok());
        api.sync_from_settings(
            ctx,
            &AppSettings {
                api_enabled: false,
                ..settings
            },
        )
        .unwrap();
    }
}
