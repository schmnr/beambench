use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;

use beambench_core::ObjectId;
use beambench_core::settings::RecentFile;
use beambench_service::ServiceContext;
use beambench_service::ops::nesting::{self, NestError, NestOptions, NestResult};
use beambench_service::ops::planning::{self, SessionJobOptions};
use beambench_service::persist;
use tauri::{State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use super::project::parse_id;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintDocumentResponse {
    title: String,
    svg: String,
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtworkExportPickerFormat {
    label: String,
    extension: String,
}

#[tauri::command]
pub fn pick_artwork_export_path(
    app: tauri::AppHandle,
    title: String,
    default_path: String,
    formats: Vec<ArtworkExportPickerFormat>,
    selected_extension: String,
) -> Result<Option<String>, String> {
    #[cfg(target_os = "macos")]
    {
        return pick_artwork_export_path_macos(
            app,
            title,
            default_path,
            formats,
            selected_extension,
        );
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, title, default_path, formats, selected_extension);
        Err("Native artwork export picker is only available on macOS".to_string())
    }
}

#[cfg(target_os = "macos")]
fn pick_artwork_export_path_macos(
    app: tauri::AppHandle,
    title: String,
    default_path: String,
    formats: Vec<ArtworkExportPickerFormat>,
    selected_extension: String,
) -> Result<Option<String>, String> {
    if let Some(mtm) = objc2::MainThreadMarker::new() {
        return show_artwork_export_panel(
            &title,
            &default_path,
            &formats,
            &selected_extension,
            mtm,
        );
    }

    let (tx, rx) = mpsc::channel();
    app.run_on_main_thread(move || {
        let result = objc2::MainThreadMarker::new()
            .ok_or_else(|| "Failed to open export dialog on the main thread".to_string())
            .and_then(|mtm| {
                show_artwork_export_panel(&title, &default_path, &formats, &selected_extension, mtm)
            });
        let _ = tx.send(result);
    })
    .map_err(|e| format!("Failed to schedule export dialog: {e}"))?;

    rx.recv()
        .map_err(|e| format!("Failed to receive export dialog result: {e}"))?
}

#[cfg(target_os = "macos")]
fn show_artwork_export_panel(
    title: &str,
    default_path: &str,
    formats: &[ArtworkExportPickerFormat],
    selected_extension: &str,
    mtm: objc2::MainThreadMarker,
) -> Result<Option<String>, String> {
    use objc2_app_kit::{NSModalResponseOK, NSSavePanel};
    use objc2_foundation::{NSString, NSURL};

    let panel = NSSavePanel::savePanel(mtm);
    let title = NSString::from_str(title);
    let save_as = NSString::from_str("Save As:");
    panel.setTitle(Some(&title));
    panel.setNameFieldLabel(Some(&save_as));
    panel.setCanCreateDirectories(true);
    panel.setExtensionHidden(false);
    panel.setShowsTagField(true);
    panel.setAllowsOtherFileTypes(false);

    let default = Path::new(default_path);
    if let Some(directory) = default.parent().filter(|path| !path.as_os_str().is_empty()) {
        if let Some(directory) = directory.to_str() {
            let url = NSURL::fileURLWithPath_isDirectory(&NSString::from_str(directory), true);
            panel.setDirectoryURL(Some(&url));
        }
    }
    if let Some(file_name) = default.file_name().and_then(|name| name.to_str()) {
        panel.setNameFieldStringValue(&NSString::from_str(&visible_export_file_stem(
            file_name, formats,
        )));
    }

    let format_popup = build_export_format_popup(formats, selected_extension, mtm);
    panel.setAccessoryView(Some(format_popup.accessory.as_ref()));

    let response = panel.runModal();
    if response != NSModalResponseOK {
        return Ok(None);
    }

    let url = panel
        .URL()
        .ok_or_else(|| "Export dialog did not return a path".to_string())?;
    let path = url
        .path()
        .ok_or_else(|| "Export dialog returned an invalid path".to_string())?;
    let selected_index = format_popup.popup.indexOfSelectedItem();
    let selected_extension = formats
        .get(usize::try_from(selected_index).unwrap_or(0))
        .map(|format| format.extension.as_str())
        .unwrap_or(selected_extension);
    Ok(Some(path_with_selected_extension(
        &path.to_string(),
        selected_extension,
    )))
}

#[cfg(target_os = "macos")]
struct ExportFormatPopup {
    accessory: objc2::rc::Retained<objc2_app_kit::NSView>,
    popup: objc2::rc::Retained<objc2_app_kit::NSPopUpButton>,
}

#[cfg(target_os = "macos")]
fn ns_rect(x: f64, y: f64, width: f64, height: f64) -> objc2_foundation::NSRect {
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
}

#[cfg(target_os = "macos")]
fn build_export_format_popup(
    formats: &[ArtworkExportPickerFormat],
    selected_extension: &str,
    mtm: objc2::MainThreadMarker,
) -> ExportFormatPopup {
    use objc2::MainThreadOnly;
    use objc2_app_kit::{NSPopUpButton, NSView};
    use objc2_foundation::NSString;

    let accessory = NSView::initWithFrame(NSView::alloc(mtm), ns_rect(0.0, 0.0, 370.0, 32.0));
    let popup = NSPopUpButton::initWithFrame_pullsDown(
        NSPopUpButton::alloc(mtm),
        ns_rect(0.0, 0.0, 330.0, 28.0),
        false,
    );

    for format in formats {
        popup.addItemWithTitle(&NSString::from_str(&format.label));
    }

    if let Some(index) = formats
        .iter()
        .position(|format| format.extension.eq_ignore_ascii_case(selected_extension))
    {
        popup.selectItemAtIndex(index as isize);
    }

    accessory.addSubview(&popup);
    ExportFormatPopup { accessory, popup }
}

#[cfg(target_os = "macos")]
fn path_with_selected_extension(path: &str, extension: &str) -> String {
    let mut path = PathBuf::from(path);
    path.set_extension(extension);
    path.to_string_lossy().into_owned()
}

#[cfg(target_os = "macos")]
fn visible_export_file_stem(file_name: &str, formats: &[ArtworkExportPickerFormat]) -> String {
    let mut stem = file_name.to_string();
    loop {
        let path = Path::new(&stem);
        let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
            return stem;
        };
        if !formats
            .iter()
            .any(|format| format.extension.eq_ignore_ascii_case(extension))
        {
            return stem;
        }
        let Some(next_stem) = path.file_stem().and_then(|next_stem| next_stem.to_str()) else {
            return stem;
        };
        stem = next_stem.to_string();
    }
}

/// Export the current execution plan as a G-code file.
/// Validates the cached plan against the current project, regenerating if stale.
#[tauri::command]
pub fn export_gcode(
    app: tauri::AppHandle,
    svc: State<'_, Arc<ServiceContext>>,
    path: Option<String>,
    job_options: Option<SessionJobOptions>,
) -> Result<String, String> {
    // Open save dialog
    let path = if let Some(path) = path {
        PathBuf::from(path)
    } else {
        let file_path = app
            .dialog()
            .file()
            .add_filter("G-code", &["gcode", "nc", "ngc"])
            .set_file_name("output.gcode")
            .set_title("Export G-code")
            .blocking_save_file();

        let Some(fp) = file_path else {
            return Err("Export cancelled".to_string());
        };

        fp.into_path()
            .map_err(|e| format!("Invalid file path: {e}"))?
    };
    planning::export_gcode_to_path_with_options(&svc, &path, &job_options.unwrap_or_default())
        .map_err(Into::into)
}

// ---------------------------------------------------------------------------
// Export and recent-file commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn export_svg(
    svc: State<'_, Arc<ServiceContext>>,
    path: String,
    selection_only: bool,
    selected_ids: Vec<String>,
) -> Result<String, String> {
    let parsed_ids: Vec<ObjectId> = selected_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_ref().ok_or("No project open")?;
    let svg_content = beambench_core::export_svg(project, selection_only, &parsed_ids);
    std::fs::write(&path, &svg_content).map_err(|e| format!("Failed to write SVG: {e}"))?;
    Ok(path)
}

#[tauri::command]
pub async fn nest_selected(
    svc: State<'_, Arc<ServiceContext>>,
    selected_ids: Vec<String>,
    options: NestOptions,
) -> Result<NestResult, NestError> {
    if selected_ids.is_empty() {
        return Err(NestError::new("no_parts", "Select objects before nesting"));
    }
    let parsed_ids: Vec<ObjectId> = selected_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| NestError::new("unsupported_geometry", error))?;
    let svc = svc.inner().clone();
    tokio::task::spawn_blocking(move || {
        nesting::nest_selected(&svc, parsed_ids, options).map_err(NestError::from)
    })
    .await
    .map_err(|error| NestError::new("engine_error", format!("Nesting task failed: {error}")))?
}

#[tauri::command]
pub fn export_dxf(
    svc: State<'_, Arc<ServiceContext>>,
    path: String,
    selection_only: bool,
    selected_ids: Vec<String>,
) -> Result<String, String> {
    let parsed_ids: Vec<ObjectId> = selected_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_ref().ok_or("No project open")?;
    let dxf_content = beambench_core::export_dxf(project, selection_only, &parsed_ids);
    std::fs::write(&path, &dxf_content).map_err(|e| format!("Failed to write DXF: {e}"))?;
    Ok(path)
}

#[tauri::command]
pub fn export_pdf(
    svc: State<'_, Arc<ServiceContext>>,
    path: String,
    selection_only: bool,
    selected_ids: Vec<String>,
) -> Result<String, String> {
    let parsed_ids: Vec<ObjectId> = selected_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_ref().ok_or("No project open")?;
    let pdf_bytes = beambench_core::export_pdf(project, selection_only, &parsed_ids);
    std::fs::write(&path, &pdf_bytes).map_err(|e| format!("Failed to write PDF: {e}"))?;
    Ok(path)
}

#[tauri::command]
pub fn export_eps(
    svc: State<'_, Arc<ServiceContext>>,
    path: String,
    selection_only: bool,
    selected_ids: Vec<String>,
) -> Result<String, String> {
    let parsed_ids: Vec<ObjectId> = selected_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_ref().ok_or("No project open")?;
    let eps_content = beambench_core::export_eps(project, selection_only, &parsed_ids);
    std::fs::write(&path, &eps_content).map_err(|e| format!("Failed to write EPS: {e}"))?;
    Ok(path)
}

#[tauri::command]
pub fn export_ai(
    svc: State<'_, Arc<ServiceContext>>,
    path: String,
    selection_only: bool,
    selected_ids: Vec<String>,
) -> Result<String, String> {
    let parsed_ids: Vec<ObjectId> = selected_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_ref().ok_or("No project open")?;
    let ai_content = beambench_core::export_ai(project, selection_only, &parsed_ids);
    std::fs::write(&path, &ai_content).map_err(|e| format!("Failed to write AI: {e}"))?;
    Ok(path)
}

#[tauri::command]
pub fn render_print_document(
    svc: State<'_, Arc<ServiceContext>>,
    mode: String,
) -> Result<PrintDocumentResponse, String> {
    let mode = match mode.as_str() {
        "black" => beambench_core::PrintMode::Black,
        "color" => beambench_core::PrintMode::Color,
        _ => return Err(format!("Unsupported print mode: {mode}")),
    };
    let guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_ref().ok_or("No project open")?;
    let document = beambench_core::render_print_document(project, mode)?;
    Ok(PrintDocumentResponse {
        title: document.title,
        svg: document.svg,
    })
}

#[tauri::command]
pub fn print_current_webview(window: WebviewWindow) -> Result<(), String> {
    window
        .print()
        .map_err(|e| format!("Failed to open print dialog: {e}"))
}

#[tauri::command]
pub fn write_export_bytes(path: String, bytes: Vec<u8>) -> Result<String, String> {
    std::fs::write(&path, bytes).map_err(|e| format!("Failed to write export bytes: {e}"))?;
    Ok(path)
}

#[tauri::command]
pub fn save_processed_bitmap(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    path: String,
) -> Result<String, String> {
    let object_id = parse_id(&object_id)?;
    let path_buf = PathBuf::from(&path);
    let guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_ref().ok_or("No project open")?;
    beambench_core::save_processed_bitmap(project, object_id, &path_buf)?;
    Ok(path)
}

#[tauri::command]
pub fn get_recent_files(svc: State<'_, Arc<ServiceContext>>) -> Result<Vec<RecentFile>, String> {
    let settings = svc.settings.lock().map_err(|e| format!("lock: {e}"))?;
    Ok(settings.get_recent_files().to_vec())
}

#[tauri::command]
pub fn clear_recent_files(svc: State<'_, Arc<ServiceContext>>) -> Result<(), String> {
    {
        let mut settings = svc.settings.lock().map_err(|e| format!("lock: {e}"))?;
        settings.clear_recent_files();
    }
    persist::persist_settings_to_disk(&svc);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_export_bytes_writes_exact_bytes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("screenshot.bin");
        let result = write_export_bytes(path.to_string_lossy().into_owned(), vec![0, 1, 2, 255])
            .expect("write bytes");

        assert_eq!(result, path.to_string_lossy().as_ref());
        assert_eq!(std::fs::read(path).expect("read bytes"), vec![0, 1, 2, 255]);
    }

    #[test]
    fn write_export_bytes_surfaces_io_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("missing").join("screenshot.bin");
        let err = write_export_bytes(path.to_string_lossy().into_owned(), vec![1, 2, 3])
            .expect_err("missing parent should fail");

        assert!(err.contains("Failed to write export bytes"));
    }
}
