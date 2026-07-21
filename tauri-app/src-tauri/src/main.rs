#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod events;
mod logging;
mod native_menu;
mod panic_reports;
mod state;
mod theme;

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use beambench_service::ServiceContext;
use state::{ApiRuntime, CloseConfirmed, FrontendReady};
use tauri::{Emitter, Manager};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const SINGLE_INSTANCE_LOCK_ADDR: &str = "127.0.0.1:47683";
const DESIGN_RENDER_TEMP_PREFIX: &str = "beambench-design-render-";
const DESIGN_RENDER_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

fn design_render_cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|dir| dir.join("beam-bench").join("design-renders"))
}

fn is_design_render_artifact(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    file_name.starts_with(DESIGN_RENDER_TEMP_PREFIX)
        && matches!(extension.to_ascii_lowercase().as_str(), "svg" | "png")
}

fn cleanup_old_design_render_artifacts() {
    let Some(cache_dir) = design_render_cache_dir() else {
        return;
    };
    if !cache_dir.exists() {
        return;
    }
    let now = SystemTime::now();
    let mut deleted = 0usize;
    let entries = match std::fs::read_dir(&cache_dir) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!(error = %error, "Failed to read design render cache");
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_design_render_artifact(&path) {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        if now.duration_since(modified).unwrap_or_default() < DESIGN_RENDER_MAX_AGE {
            continue;
        }
        match std::fs::remove_file(&path) {
            Ok(()) => deleted += 1,
            Err(error) => {
                tracing::warn!(path = %path.display(), error = %error, "Failed to delete design render artifact");
            }
        }
    }
    if deleted > 0 {
        tracing::info!(deleted, "Cleaned old design render artifacts");
    }
}

#[cfg(target_os = "linux")]
fn set_env_if_unset(key: &str, value: &str) {
    if std::env::var_os(key).is_none() {
        unsafe { std::env::set_var(key, value) };
    }
}

#[cfg(target_os = "linux")]
fn linux_session_is_wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE")
            .is_ok_and(|session| session.eq_ignore_ascii_case("wayland"))
}

fn main() {
    // WebKitGTK can fail before the frontend boots on several Linux graphics
    // stacks: NVIDIA proprietary drivers, Wayland/EGL regressions, hybrid GPUs,
    // and AppImages mixing bundled WebKit with newer host Mesa. Set these before
    // Tauri/WebKit starts, while still respecting explicit user overrides.
    #[cfg(target_os = "linux")]
    {
        set_env_if_unset("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        if linux_session_is_wayland() {
            set_env_if_unset("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        }
    }

    panic_reports::install_panic_hook();
    let ctx = Arc::new(ServiceContext::new());
    panic_reports::load_startup_panics_into_context(&ctx);
    let api_runtime = ApiRuntime::default();

    let env_filter =
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());

    let fmt_layer = tracing_subscriber::fmt::layer();
    let buffer_layer = logging::BufferLayer::new(Arc::clone(&ctx));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .with(buffer_layer)
        .init();

    tracing::info!("Beam Bench starting");
    let single_instance_lock = match TcpListener::bind(SINGLE_INSTANCE_LOCK_ADDR) {
        Ok(listener) => listener,
        Err(err) => {
            tracing::warn!(
                addr = SINGLE_INSTANCE_LOCK_ADDR,
                error = %err,
                "Another Beam Bench instance is already running"
            );
            eprintln!("Beam Bench is already running.");
            return;
        }
    };

    let startup_ui_theme = match ctx.settings.lock() {
        Ok(settings) => settings.ui_theme,
        Err(error) => {
            tracing::warn!(error = %error, "Failed to read startup UI theme");
            beambench_core::UiTheme::Dark
        }
    };
    let mut tauri_context = tauri::generate_context!();
    for window_config in &mut tauri_context.config_mut().app.windows {
        theme::configure_window(window_config, startup_ui_theme);
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(ctx.clone())
        .manage(api_runtime)
        .manage(single_instance_lock)
        .manage(native_menu::NativeMenuRegistry::default())
        .manage(CloseConfirmed::default())
        .manage(FrontendReady::default())
        .on_window_event({
            let ctx = ctx.clone();
            move |window, event| {
                if let tauri::WindowEvent::ThemeChanged(system_theme) = event {
                    let follows_system = ctx
                        .settings
                        .lock()
                        .map(|settings| settings.ui_theme == beambench_core::UiTheme::System)
                        .unwrap_or(false);
                    if follows_system {
                        theme::apply_system_background(window.app_handle(), *system_theme);
                    }
                }
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    // swap(false) consumes the confirmation: each close
                    // attempt needs its own Save / Don't Save decision, so a
                    // confirmed close of one window can't bypass the prompt
                    // for a later dirty close of another.
                    let close_confirmed = window
                        .state::<CloseConfirmed>()
                        .inner()
                        .0
                        .swap(false, std::sync::atomic::Ordering::AcqRel);
                    let project_dirty = ctx
                        .project
                        .lock()
                        .ok()
                        .and_then(|guard| guard.as_ref().map(|project| project.dirty))
                        .unwrap_or(false);
                    if project_dirty && !close_confirmed {
                        // Hold the window open and let the frontend show the
                        // Save / Don't Save / Cancel prompt. It re-closes via
                        // confirm_window_close once the user decides.
                        api.prevent_close();
                        let _ = events::emit_app_event(
                            window.app_handle(),
                            "app.close_requested",
                            serde_json::json!({}),
                        );
                        return;
                    }
                    let deleted = ctx.cleanup_tracked_camera_frame_files();
                    if deleted > 0 {
                        tracing::info!(deleted, "Cleaned tracked camera frames");
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            // App
            commands::app::get_app_status,
            commands::app::get_build_info,
            commands::app::get_update_environment_blocker,
            commands::app::mark_frontend_ready,
            commands::app::confirm_window_close,
            commands::app::request_window_close,
            commands::app::open_external_url,
            commands::app::get_app_settings,
            commands::app::open_new_window,
            commands::app::update_app_settings,
            commands::feedback::get_connection_diagnostics,
            commands::feedback::preview_feedback_report,
            commands::feedback::save_feedback_report,
            commands::feedback::submit_feedback_report,
            commands::feedback::reveal_feedback_report,
            commands::app::export_preferences,
            commands::app::import_preferences,
            commands::app::reset_preferences,
            commands::app::open_preferences_folder,
            commands::app::get_system_fonts,
            commands::app::update_display_settings,
            commands::app::get_image_presets,
            commands::app::save_image_preset,
            commands::app::delete_image_preset,
            commands::agent::agent_sync_selection,
            native_menu::update_native_menu_state,
            native_menu::rebuild_native_menu,
            // Camera
            commands::camera::list_camera_devices,
            commands::camera::select_camera_device,
            commands::camera::get_camera_calibration,
            commands::camera::solve_camera_calibration,
            commands::camera::save_camera_calibration,
            commands::camera::reset_camera_calibration,
            commands::camera::capture_camera_frame,
            commands::camera::register_camera_agent_bridge,
            commands::camera::unregister_camera_agent_bridge,
            commands::camera::save_camera_frame_bytes,
            commands::camera::get_camera_alignment,
            commands::camera::solve_camera_alignment,
            commands::camera::update_camera_alignment,
            commands::camera::reset_camera_alignment,
            commands::camera::get_camera_overlay_state,
            commands::camera::get_camera_agent_state,
            commands::camera::update_camera_overlay_display,
            commands::camera::fit_camera_overlay_to_bed,
            commands::camera::discard_camera_overlay_draft,
            commands::camera::save_camera_overlay_alignment,
            commands::camera::commit_camera_overlay_transform,
            commands::camera::complete_camera_capture_request,
            commands::camera::complete_camera_overlay_render_request,
            // Project (existing)
            commands::project::create_project,
            commands::project::get_project,
            commands::project::close_project,
            commands::project::get_undo_state,
            commands::project::undo_project,
            commands::project::redo_project,
            commands::project::get_project_layers,
            commands::project::add_layer,
            commands::project::update_layer,
            commands::project::add_cut_entry,
            commands::project::paste_layer_entries,
            commands::project::reset_cut_entry_to_defaults,
            commands::project::set_all_layers_enabled,
            commands::project::set_all_layers_visible,
            commands::project::sort_layers_cut_last,
            commands::project::remove_cut_entry,
            commands::project::reorder_cut_entry,
            commands::project::update_cut_entry,
            commands::project::remove_layer,
            commands::project::reorder_layer,
            commands::project::get_project_objects,
            commands::project::add_object,
            commands::project::add_object_atomic,
            commands::project::update_object,
            commands::project::update_object_data,
            commands::project::advance_auto_variable_text,
            commands::project::apply_adjust_image_dialog,
            commands::project::resize_shape_object,
            commands::project::remove_object,
            commands::project::remove_objects,
            commands::project::set_text_guide_path,
            commands::project::nudge_objects,
            commands::project::duplicate_object,
            commands::project::duplicate_objects,
            commands::project::duplicate_object_in_place,
            commands::project::duplicate_objects_in_place,
            commands::project::paste_objects,
            commands::project::align_objects,
            commands::project::distribute_objects,
            commands::project::move_objects_together,
            commands::project::mirror_across_line,
            commands::project::make_same_size,
            commands::project::dock_objects,
            commands::project::resize_slots,
            commands::project::bind_machine_profile,
            commands::project::replace_project,
            // Project
            commands::project::set_layer_visible,
            commands::project::set_layer_air_assist,
            commands::project::select_all_in_layer,
            commands::project::push_draw_order,
            commands::project::lock_objects,
            commands::project::unlock_objects,
            commands::project::flip_objects,
            commands::project::rotate_objects,
            commands::project::rotate_objects_and_bake_active_path,
            commands::project::shear_objects,
            commands::project::update_object_bounds_batch,
            commands::project::move_objects_to,
            commands::project::set_start_from,
            commands::project::set_job_origin,
            commands::project::set_user_origin,
            commands::project::set_optimization,
            commands::project::update_project_notes,
            commands::project::set_transform_locks,
            commands::project::set_objects_visible,
            commands::project::reassign_layer,
            commands::project::select_open_shapes,
            commands::project::select_open_shapes_set_to_fill,
            commands::project::select_contained_shapes,
            commands::project::select_shapes_smaller_than_selected,
            commands::project::count_duplicates,
            commands::project::delete_duplicates,
            commands::project::auto_join_shapes,
            commands::project::optimize_shapes,
            // Import (existing)
            commands::import::import_svg_file,
            commands::import::import_image_file,
            commands::import::read_clipboard_artwork,
            commands::import::import_clipboard_artwork,
            commands::import::import_files,
            commands::import::import_file_data,
            commands::import::pick_and_import_files,
            // Import
            commands::import::import_dxf_file,
            commands::import::import_pdf_file,
            commands::import::import_ai_file,
            commands::import::import_eps_file,
            commands::import::import_gcode_file,
            commands::import::trace_image_preview,
            commands::import::trace_image,
            commands::import::refresh_image,
            commands::import::replace_image,
            commands::import::replace_image_to_fit,
            commands::import::adjust_image_preview,
            commands::import::auto_adjust_image,
            commands::import::render_dither_sample,
            // Persistence
            commands::persistence::save_project_cmd,
            commands::persistence::save_project_as_cmd,
            commands::persistence::open_project_cmd,
            commands::persistence::open_project_from_path,
            commands::persistence::get_asset_data,
            commands::persistence::autosave_project,
            commands::persistence::check_recovery_files,
            commands::persistence::restore_recovery,
            commands::persistence::discard_recovery_file,
            commands::export::save_processed_bitmap,
            commands::export::nest_selected,
            // Vector (existing)
            commands::vector::convert_to_path,
            commands::vector::boolean_union,
            commands::vector::boolean_subtract,
            commands::vector::boolean_exclude,
            commands::vector::boolean_assistant_preview,
            commands::vector::group_objects,
            commands::vector::auto_group_objects,
            commands::vector::ungroup_objects,
            commands::vector::get_editable_path,
            commands::vector::update_node,
            commands::vector::update_nodes_batch,
            commands::vector::set_node_type,
            commands::vector::delete_node,
            commands::vector::delete_nodes,
            commands::vector::insert_node,
            commands::vector::convert_segment_to_line,
            commands::vector::convert_segment_to_curve,
            commands::vector::align_segment_to_angle,
            commands::vector::trim_segment_to_intersection,
            commands::vector::extend_endpoint_to_intersection,
            commands::vector::join_subpaths,
            commands::vector::delete_segment_cmd,
            commands::vector::break_path_at_node,
            commands::vector::toggle_path_closed,
            commands::vector::scale_path_to_bounds,
            commands::vector::mesh_deform_selection,
            commands::vector::normalize_for_planner,
            // Vector
            commands::vector::boolean_intersection,
            commands::vector::boolean_weld,
            commands::vector::offset_shapes,
            commands::vector::preview_offset_shapes,
            commands::vector::close_path,
            commands::vector::close_paths_with_tolerance,
            commands::vector::close_selected_paths_with_tolerance,
            commands::vector::count_open_paths_with_tolerance,
            commands::vector::break_apart,
            commands::vector::set_start_point,
            commands::vector::get_path_vertices,
            commands::vector::apply_radius,
            commands::vector::get_fillet_candidates,
            commands::vector::apply_corner_radius,
            commands::vector::grid_array,
            commands::vector::circular_array,
            commands::vector::copy_along_path,
            commands::vector::copy_along_path_batch,
            commands::vector::unlink_virtual_clone,
            commands::vector::rubber_band_outline,
            commands::vector::apply_path_to_text,
            commands::vector::crop_image,
            commands::vector::apply_mask_to_image,
            commands::vector::assign_image_mask,
            commands::vector::set_image_mask_polarity,
            commands::vector::remove_image_mask,
            commands::vector::convert_to_bitmap,
            commands::vector::add_tabs,
            commands::vector::place_tab,
            commands::vector::remove_tab,
            commands::vector::resolve_tab_markers,
            commands::vector::trim_shape,
            commands::vector::preview_trim_segment,
            commands::vector::close_and_join,
            commands::vector::cut_shapes_apply,
            commands::vector::cut_shapes,
            commands::vector::generate_barcode,
            // Planner
            commands::planner::generate_plan,
            commands::planner::get_plan_stats,
            commands::planner::cancel_planning,
            // Machine (existing)
            commands::machine::list_serial_ports,
            commands::machine::connect_machine,
            commands::machine::connect_machine_candidate,
            commands::machine::begin_controller_connection,
            commands::machine::begin_network_controller_connection,
            commands::machine::list_lihuiyu_usb_devices,
            commands::machine::begin_usb_controller_connection,
            commands::machine::continue_controller_connection,
            commands::machine::disconnect_machine,
            commands::machine::get_machine_status,
            commands::machine::get_machine_runtime_state,
            commands::machine::get_machine_coordinates_valid,
            commands::machine::get_session_state,
            commands::machine::machine_home,
            commands::machine::machine_unlock,
            commands::machine::machine_jog,
            commands::machine::machine_jog_cancel,
            commands::machine::run_preflight_check,
            commands::machine::start_job,
            commands::machine::get_job_progress,
            commands::machine::pause_job,
            commands::machine::resume_job,
            commands::machine::cancel_job,
            commands::machine::get_machine_profiles,
            commands::machine::save_machine_profile,
            commands::machine::export_machine_profile,
            commands::machine::import_machine_profile,
            commands::machine::delete_machine_profile,
            commands::machine::get_machine_profile_presets,
            commands::machine::suggest_machine_profile_preset,
            commands::machine::get_machine_profile_preset_diff,
            commands::machine::apply_machine_profile_preset,
            commands::machine::set_active_profile,
            commands::machine::start_machine_discovery,
            commands::machine::get_machine_discovery_state,
            commands::machine::cancel_machine_discovery,
            commands::machine::bootstrap_machine_profile,
            commands::machine::frame_job,
            commands::machine::set_feed_override,
            commands::machine::set_spindle_override,
            commands::machine::reset_all_overrides,
            commands::machine::emergency_stop,
            commands::machine::set_work_origin,
            commands::machine::reset_work_origin,
            // Machine
            commands::machine::send_gcode_line,
            commands::machine::get_console_log,
            commands::machine::clear_console_log,
            commands::machine::get_material_presets,
            commands::machine::save_material_preset,
            commands::machine::delete_material_preset,
            commands::machine::apply_material_preset,
            commands::machine::apply_material_preset_to_seed,
            commands::machine::export_material_presets,
            commands::machine::import_material_presets,
            commands::machine::get_macros,
            commands::machine::save_macro,
            commands::machine::delete_macro,
            commands::machine::run_macro,
            commands::machine::export_macros,
            commands::machine::import_macros,
            commands::machine::get_controller_info,
            commands::machine::move_laser_to,
            commands::machine::move_laser_to_machine,
            commands::machine::laser_fire_start,
            commands::machine::laser_fire_keepalive,
            commands::machine::laser_fire_stop,
            commands::machine::get_grbl_settings,
            commands::machine::set_grbl_setting,
            commands::machine::get_saved_positions,
            commands::machine::save_position,
            commands::machine::delete_saved_position,
            // Preview
            commands::preview::generate_preview,
            // Quality Tests (M3)
            commands::quality_test::quality_test_preview,
            commands::quality_test::quality_test_export_gcode,
            commands::quality_test::quality_test_frame,
            commands::quality_test::quality_test_start,
            commands::quality_test::quality_test_create_material_on_canvas,
            commands::quality_test::export_material_test_recipes,
            commands::quality_test::import_material_test_recipes,
            commands::project::set_material_height,
            // Export (existing)
            commands::export::export_gcode,
            // Export
            commands::export::export_svg,
            commands::export::export_dxf,
            commands::export::export_pdf,
            commands::export::export_eps,
            commands::export::export_ai,
            commands::export::render_print_document,
            commands::export::print_current_webview,
            commands::export::pick_artwork_export_path,
            commands::export::write_export_bytes,
            commands::export::get_recent_files,
            commands::export::clear_recent_files,
            // Variable Text
            commands::variable_text::parse_merge_fields,
            commands::variable_text::load_csv_file,
            commands::variable_text::resolve_variable_text,
            commands::variable_text::generate_batch_preview,
            commands::variable_text::generate_variable_text_batch,
            // Art Library
            commands::art_library::get_art_libraries,
            commands::art_library::create_art_library,
            commands::art_library::load_art_library,
            commands::art_library::unload_art_library,
            commands::art_library::save_art_library_as,
            commands::art_library::rename_art_library,
            commands::art_library::delete_art_library,
            commands::art_library::add_art_library_item,
            commands::art_library::add_selection_to_art_library,
            commands::art_library::rename_art_library_item,
            commands::art_library::remove_art_library_item,
            commands::art_library::commit_art_library_thumbnail,
            commands::art_library::move_art_library_item,
            commands::art_library::insert_art_library_item_to_project,
        ])
        .setup(|app| {
            native_menu::install(app)?;
            let ctx = app.state::<Arc<ServiceContext>>();
            let api_runtime = app.state::<ApiRuntime>().inner().clone();
            ctx.set_settings_applier({
                let ctx = ctx.inner().clone();
                let app_handle = app.handle().clone();
                move |settings| {
                    // Keep the existing fail-closed API runtime behavior. Native
                    // appearance is applied only after API synchronization
                    // succeeds, and its own failures are logged as non-fatal.
                    api_runtime.sync_from_settings(ctx.clone(), settings)?;
                    theme::apply_to_app(&app_handle, settings.ui_theme);
                    Ok(())
                }
            })?;
            let settings = ctx
                .settings
                .lock()
                .map_err(|e| format!("Failed to lock settings: {e}"))?
                .clone();
            if let Err(error) = ctx.apply_settings_side_effects(&settings) {
                tracing::warn!(error = %error, "Failed to apply startup settings side effects");
                ctx.push_error(format!("API startup warning: {error}"));
            }
            cleanup_old_design_render_artifacts();
            let deleted_camera_frames = beambench_service::ops::camera::cleanup_stale_frame_files();
            if deleted_camera_frames > 0 {
                tracing::info!(
                    deleted = deleted_camera_frames,
                    "Cleaned stale camera frames"
                );
            }

            let mut service_events = ctx.events.subscribe();
            let service_event_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    match service_events.recv().await {
                        Ok(message) => match serde_json::from_str::<serde_json::Value>(&message) {
                            Ok(event) => {
                                if service_event_handle.emit("app-event", &event).is_err() {
                                    tracing::debug!("No frontend listeners for service event");
                                }
                            }
                            Err(error) => {
                                tracing::warn!(error = %error, "Failed to parse service event");
                            }
                        },
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(dropped)) => {
                            tracing::warn!(dropped, "Desktop service event bridge lagged");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });

            // Emit a startup event to verify the event bridge works end-to-end.
            // The frontend listens for this and logs it to the console.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Small delay so the frontend has time to mount and subscribe
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let _ = events::emit_app_event(
                    &handle,
                    "app.started",
                    serde_json::json!({ "message": "Beam Bench backend ready" }),
                );
            });

            // Webview-boot watchdog. If the system WebKit is too old to run
            // the bundled JS, the window stays dark and every menu item is
            // dead (menu events emit into the webview). Reported on macOS
            // 10.14. The dialog is native, so it works without the webview.
            // English-only by necessity: the i18n layer lives in the dead
            // frontend.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(20)).await;
                let ready = handle
                    .state::<FrontendReady>()
                    .0
                    .load(std::sync::atomic::Ordering::Acquire);
                if ready {
                    return;
                }
                tracing::error!(
                    "Frontend never signalled ready; the webview likely failed to boot"
                );
                use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
                handle
                    .dialog()
                    .message(
                        "Beam Bench opened, but its interface failed to load.\n\n\
                         This usually means the operating system or graphics stack could \
                         not start the embedded webview. On Linux, this can be caused by \
                         WebKitGTK, Wayland, EGL, or AppImage library compatibility.\n\n\
                         Please send the terminal output from launching Beam Bench in \
                         the Beam Bench Facebook group: facebook.com/groups/beambench",
                    )
                    .title("Beam Bench Could Not Start")
                    .kind(MessageDialogKind::Warning)
                    .show(|_| {});
            });
            Ok(())
        })
        .run(tauri_context)
        .expect("error while running Beam Bench");
}
