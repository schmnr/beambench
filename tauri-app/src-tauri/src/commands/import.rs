use std::sync::Arc;
use std::time::Instant;

use base64::Engine;
use beambench_core::{Asset, AssetId, AssetMediaType, ObjectData, Project, ProjectObject};
use beambench_service::ServiceContext;
use beambench_service::ops::imports;
use tauri::State;
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;

use super::project::parse_id;

fn detect_asset_media_type(file_path: &str, bytes: &[u8]) -> Result<AssetMediaType, String> {
    if let Some(ext) = std::path::Path::new(file_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(AssetMediaType::from_extension)
    {
        return Ok(ext);
    }

    match image::guess_format(bytes).map_err(|e| format!("Failed to detect image format: {e}"))? {
        image::ImageFormat::Png => Ok(AssetMediaType::Png),
        image::ImageFormat::Jpeg => Ok(AssetMediaType::Jpeg),
        image::ImageFormat::Bmp => Ok(AssetMediaType::Bmp),
        image::ImageFormat::Gif => Ok(AssetMediaType::Gif),
        image::ImageFormat::Tiff => Ok(AssetMediaType::Tiff),
        image::ImageFormat::WebP => Ok(AssetMediaType::Webp),
        image::ImageFormat::Tga => Ok(AssetMediaType::Tga),
        other => Err(format!("Unsupported image format: {other:?}")),
    }
}

fn absolute_source_path(file_path: &str) -> Option<String> {
    std::fs::canonicalize(file_path)
        .ok()
        .and_then(|path| path.to_str().map(ToOwned::to_owned))
}

fn effective_raster_asset_key(project: &Project, object: &ProjectObject) -> Option<String> {
    let data = project
        .resolve_clone(object)
        .map(|resolved| resolved.data)
        .unwrap_or_else(|| object.data.clone());
    match data {
        ObjectData::RasterImage { asset_key, .. } => Some(asset_key),
        _ => None,
    }
}

fn project_references_asset_key(project: &Project, asset_key: &str) -> bool {
    project
        .objects
        .iter()
        .any(|object| effective_raster_asset_key(project, object).as_deref() == Some(asset_key))
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    let trimmed = text.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    trimmed.starts_with("<svg") || (trimmed.starts_with("<?xml") && trimmed.contains("<svg"))
}

fn clipboard_media_type(
    filename: &str,
    media_type: &str,
    bytes: &[u8],
) -> Result<&'static str, String> {
    if looks_like_svg(bytes) {
        return Ok("image/svg+xml");
    }

    match media_type.trim().to_ascii_lowercase().as_str() {
        "image/svg+xml" => return Ok("image/svg+xml"),
        "image/png" => return Ok("image/png"),
        "image/jpg" | "image/jpeg" => return Ok("image/jpeg"),
        "image/gif" => return Ok("image/gif"),
        "image/bmp" => return Ok("image/bmp"),
        "image/webp" => return Ok("image/webp"),
        "image/tiff" | "image/tif" => return Ok("image/tiff"),
        "image/tga" | "image/x-tga" => return Ok("image/x-tga"),
        _ => {}
    }

    match detect_asset_media_type(filename, bytes)? {
        AssetMediaType::Png => Ok("image/png"),
        AssetMediaType::Jpeg => Ok("image/jpeg"),
        AssetMediaType::Bmp => Ok("image/bmp"),
        AssetMediaType::Gif => Ok("image/gif"),
        AssetMediaType::Tiff => Ok("image/tiff"),
        AssetMediaType::Webp => Ok("image/webp"),
        AssetMediaType::Tga => Ok("image/x-tga"),
        AssetMediaType::Svg => Ok("image/svg+xml"),
        other => Err(format!("Unsupported clipboard artwork format: {other:?}")),
    }
}

fn apply_drop_position_after_import(
    ctx: &ServiceContext,
    mut created: Vec<ProjectObject>,
    drop_position: Option<(f64, f64)>,
) -> Result<Vec<ProjectObject>, String> {
    let Some((x, y)) = drop_position else {
        return Ok(created);
    };
    let Some(min_x) = created
        .iter()
        .map(|object| object.bounds.min.x)
        .reduce(f64::min)
    else {
        return Ok(created);
    };
    let Some(min_y) = created
        .iter()
        .map(|object| object.bounds.min.y)
        .reduce(f64::min)
    else {
        return Ok(created);
    };
    let created_ids = created.iter().map(|object| object.id).collect::<Vec<_>>();
    let mut guard = ctx.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    beambench_core::operations::nudge_objects(project, &created_ids, x - min_x, y - min_y);
    created = created_ids
        .iter()
        .filter_map(|id| project.find_object(*id).cloned())
        .collect();
    Ok(created)
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardArtworkPayload {
    data_base64: String,
    filename: String,
    media_type: String,
}

#[tauri::command]
pub fn import_svg_file(
    svc: State<'_, Arc<ServiceContext>>,
    file_path: String,
    layer_id: String,
) -> Result<Vec<ProjectObject>, String> {
    imports::import_svg_from_path(
        &svc,
        imports::ImportSvgInput {
            file_path,
            layer_id: parse_id(&layer_id)?,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn import_image_file(
    svc: State<'_, Arc<ServiceContext>>,
    file_path: String,
    layer_id: String,
) -> Result<ProjectObject, String> {
    imports::import_image_from_path(
        &svc,
        imports::ImportImageInput {
            file_path,
            layer_id: parse_id(&layer_id)?,
        },
    )
    .map_err(Into::into)
}

/// File URL of a file copied to the macOS pasteboard (e.g. Cmd+C in Finder),
/// if any. A Finder file-copy also carries a rendered ICON image flavor, so
/// the file reference must be checked before the bitmap flavor or pasting a
/// copied image file imports its document icon instead of its contents.
#[cfg(target_os = "macos")]
fn clipboard_file_path() -> Option<std::path::PathBuf> {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeFileURL};
    use objc2_foundation::NSURL;
    unsafe {
        let pasteboard = NSPasteboard::generalPasteboard();
        let url_string = pasteboard.stringForType(NSPasteboardTypeFileURL)?;
        let url = NSURL::URLWithString(&url_string)?;
        let path = url.path()?;
        Some(std::path::PathBuf::from(path.to_string()))
    }
}

#[cfg(not(target_os = "macos"))]
fn clipboard_file_path() -> Option<std::path::PathBuf> {
    None
}

/// Build a clipboard artwork payload from an on-disk file, when the file is
/// a supported artwork type (SVG or raster image).
fn artwork_payload_from_file(path: &std::path::Path) -> Option<ClipboardArtworkPayload> {
    let filename = path.file_name()?.to_str()?.to_string();
    let bytes = std::fs::read(path).ok()?;
    let media_type = clipboard_media_type(&filename, "", &bytes).ok()?;
    Some(ClipboardArtworkPayload {
        data_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
        filename,
        media_type: media_type.to_string(),
    })
}

#[tauri::command]
pub fn read_clipboard_artwork() -> Result<Option<ClipboardArtworkPayload>, String> {
    // A copied FILE wins over every other flavor: import its contents, or
    // nothing at all (never the icon bitmap that rides along with it).
    if let Some(path) = clipboard_file_path() {
        return Ok(artwork_payload_from_file(&path));
    }

    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Failed to open system clipboard: {e}"))?;

    if let Ok(text) = clipboard.get_text() {
        if looks_like_svg(text.as_bytes()) {
            return Ok(Some(ClipboardArtworkPayload {
                data_base64: base64::engine::general_purpose::STANDARD.encode(text.as_bytes()),
                filename: "Clipboard Artwork.svg".to_string(),
                media_type: "image/svg+xml".to_string(),
            }));
        }
        // Windows/Linux file copies often surface as a path string. When the
        // text is exactly an existing artwork file path, import that file.
        let candidate = std::path::Path::new(text.trim());
        if candidate.is_absolute() && candidate.is_file() {
            if let Some(payload) = artwork_payload_from_file(candidate) {
                return Ok(Some(payload));
            }
        }
    }

    if let Ok(image) = clipboard.get_image() {
        let width = u32::try_from(image.width)
            .map_err(|_| "Clipboard image is too wide to import".to_string())?;
        let height = u32::try_from(image.height)
            .map_err(|_| "Clipboard image is too tall to import".to_string())?;
        let mut png = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(std::io::Cursor::new(&mut png));
        image::ImageEncoder::write_image(
            encoder,
            image.bytes.as_ref(),
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("Failed to encode clipboard image: {e}"))?;

        return Ok(Some(ClipboardArtworkPayload {
            data_base64: base64::engine::general_purpose::STANDARD.encode(&png),
            filename: "Clipboard Screenshot.png".to_string(),
            media_type: "image/png".to_string(),
        }));
    }

    Ok(None)
}

#[tauri::command]
pub fn import_clipboard_artwork(
    svc: State<'_, Arc<ServiceContext>>,
    data_base64: String,
    filename: String,
    media_type: String,
    layer_id: String,
    drop_x: Option<f64>,
    drop_y: Option<f64>,
) -> Result<Vec<ProjectObject>, String> {
    let drop_position = match (drop_x, drop_y) {
        (Some(x), Some(y)) => Some((x, y)),
        (None, None) => None,
        _ => return Err("dropX and dropY must be provided together".to_string()),
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_base64.trim())
        .map_err(|e| format!("Failed to decode clipboard artwork: {e}"))?;
    let normalized_media_type = clipboard_media_type(&filename, &media_type, &bytes)?;
    let layer_id = parse_id(&layer_id)?;

    let created = if normalized_media_type == "image/svg+xml" {
        imports::import_art_library_item(
            &svc,
            layer_id,
            "Clipboard SVG",
            &filename,
            normalized_media_type,
            bytes,
        )
        .map_err(String::from)?
    } else {
        vec![
            imports::import_image_from_bytes(&svc, filename, bytes, layer_id)
                .map_err(String::from)?,
        ]
    };

    apply_drop_position_after_import(&svc, created, drop_position)
}

#[tauri::command]
pub fn import_files(
    svc: State<'_, Arc<ServiceContext>>,
    file_paths: Vec<String>,
    layer_id: String,
) -> Result<Vec<ProjectObject>, String> {
    imports::import_files_from_paths(
        &svc,
        imports::ImportFilesInput {
            file_paths,
            layer_id: parse_id(&layer_id)?,
        },
    )
    .map_err(Into::into)
}

/// Import files delivered by content (HTML5 drag-drop), where the webview
/// cannot expose OS file paths.
#[tauri::command]
pub fn import_file_data(
    svc: State<'_, Arc<ServiceContext>>,
    files: Vec<imports::ImportFileData>,
    layer_id: String,
) -> Result<Vec<ProjectObject>, String> {
    imports::import_files_from_data(
        &svc,
        imports::ImportFilesDataInput {
            files,
            layer_id: parse_id(&layer_id)?,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn pick_and_import_files(
    app: tauri::AppHandle,
    svc: State<'_, Arc<ServiceContext>>,
    layer_id: String,
) -> Result<Vec<ProjectObject>, String> {
    let paths = app
        .dialog()
        .file()
        .add_filter(
            "Supported Files",
            &[
                "svg", "png", "jpg", "jpeg", "bmp", "gif", "tif", "tiff", "webp", "tga", "dxf",
                "ai", "pdf", "eps", "lbrn", "lbrn2",
            ],
        )
        .add_filter("Lbrn Projects", &["lbrn", "lbrn2"])
        .add_filter("SVG Files", &["svg"])
        .add_filter(
            "Image Files",
            &[
                "png", "jpg", "jpeg", "bmp", "gif", "tif", "tiff", "webp", "tga",
            ],
        )
        .add_filter("DXF Files", &["dxf"])
        .add_filter("AI Files", &["ai"])
        .add_filter("PDF Files", &["pdf"])
        .add_filter("EPS Files", &["eps"])
        .set_title("Import Files")
        .blocking_pick_files();

    let Some(file_paths) = paths else {
        return Ok(Vec::new());
    };

    let file_paths = file_paths
        .into_iter()
        .map(|file_path| {
            file_path
                .into_path()
                .map(|path| path.to_string_lossy().to_string())
                .map_err(|e| format!("Invalid file path: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    imports::import_files_from_paths(
        &svc,
        imports::ImportFilesInput {
            file_paths,
            layer_id: parse_id(&layer_id)?,
        },
    )
    .map_err(Into::into)
}

// ---------------------------------------------------------------------------
// Import commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn import_dxf_file(
    svc: State<'_, Arc<ServiceContext>>,
    file_path: String,
    layer_id: String,
) -> Result<Vec<ProjectObject>, String> {
    imports::import_vector_file_from_path(
        &svc,
        imports::ImportVectorFileInput {
            file_path,
            layer_id: parse_id(&layer_id)?,
            format: imports::VectorImportFormat::Dxf,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn import_pdf_file(
    svc: State<'_, Arc<ServiceContext>>,
    file_path: String,
    layer_id: String,
) -> Result<Vec<ProjectObject>, String> {
    imports::import_vector_file_from_path(
        &svc,
        imports::ImportVectorFileInput {
            file_path,
            layer_id: parse_id(&layer_id)?,
            format: imports::VectorImportFormat::Pdf,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn import_ai_file(
    svc: State<'_, Arc<ServiceContext>>,
    file_path: String,
    layer_id: String,
) -> Result<Vec<ProjectObject>, String> {
    imports::import_vector_file_from_path(
        &svc,
        imports::ImportVectorFileInput {
            file_path,
            layer_id: parse_id(&layer_id)?,
            format: imports::VectorImportFormat::Ai,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn import_eps_file(
    svc: State<'_, Arc<ServiceContext>>,
    file_path: String,
    layer_id: String,
) -> Result<Vec<ProjectObject>, String> {
    imports::import_vector_file_from_path(
        &svc,
        imports::ImportVectorFileInput {
            file_path,
            layer_id: parse_id(&layer_id)?,
            format: imports::VectorImportFormat::Eps,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn import_gcode_file(
    svc: State<'_, Arc<ServiceContext>>,
    file_path: String,
    layer_id: String,
) -> Result<Vec<ProjectObject>, String> {
    imports::import_gcode_from_path(
        &svc,
        imports::ImportGcodeInput {
            file_path,
            layer_id: parse_id(&layer_id)?,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub async fn trace_image_preview(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    threshold: u8,
    cutoff: u8,
    turdsize: u32,
    alphamax: f64,
    opttolerance: f64,
    trace_alpha: bool,
    sketch_trace: bool,
    request_id: u64,
    boundary: Option<imports::TraceBoundaryPx>,
) -> Result<imports::TracePreviewOutput, String> {
    let svc = svc.inner().clone();
    svc.latest_trace_preview_request_id
        .fetch_max(request_id, std::sync::atomic::Ordering::AcqRel);
    let input = imports::TraceImageInput {
        object_id: parse_id(&object_id)?,
        threshold,
        cutoff,
        turdsize,
        alphamax,
        opttolerance,
        trace_alpha,
        sketch_trace,
        delete_source: false,
        preview_request_id: Some(request_id),
        boundary,
    };
    let started_at = Instant::now();
    let output = tokio::task::spawn_blocking(move || imports::trace_image_preview(&svc, input))
        .await
        .map_err(|e| format!("Trace preview task failed: {e}"))?
        .map_err(|e| e.to_string())?;
    tracing::info!(target: "perf", operation = "trace_image_preview", duration_ms = started_at.elapsed().as_millis());
    Ok(output)
}

#[tauri::command]
pub async fn trace_image(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    threshold: u8,
    cutoff: u8,
    turdsize: u32,
    alphamax: f64,
    opttolerance: f64,
    trace_alpha: bool,
    sketch_trace: bool,
    delete_source: bool,
    boundary: Option<imports::TraceBoundaryPx>,
) -> Result<Vec<ProjectObject>, String> {
    let svc = svc.inner().clone();
    let input = imports::TraceImageInput {
        object_id: parse_id(&object_id)?,
        threshold,
        cutoff,
        turdsize,
        alphamax,
        opttolerance,
        trace_alpha,
        sketch_trace,
        delete_source,
        preview_request_id: None,
        boundary,
    };
    let started_at = Instant::now();
    let objects = tokio::task::spawn_blocking(move || imports::trace_raster_image(&svc, input))
        .await
        .map_err(|e| format!("Trace task failed: {e}"))?
        .map_err(|e| e.to_string())?;
    tracing::info!(target: "perf", operation = "trace_image", duration_ms = started_at.elapsed().as_millis());
    Ok(objects)
}

#[tauri::command]
pub fn refresh_image(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<ProjectObject, String> {
    refresh_image_inner(&svc, parse_id(&object_id)?)
}

fn refresh_image_inner(
    svc: &ServiceContext,
    object_id: beambench_core::ObjectId,
) -> Result<ProjectObject, String> {
    let guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_ref().ok_or("No project open")?;
    let obj = project
        .find_object(object_id)
        .ok_or("Object not found")?
        .clone();
    let asset_key = match &obj.data {
        ObjectData::RasterImage { asset_key, .. } => asset_key,
        _ => return Err("Object is not a raster image".to_string()),
    };
    let asset_id = AssetId::from_uuid(
        Uuid::parse_str(asset_key).map_err(|e| format!("Invalid asset key: {e}"))?,
    );
    let _asset = project
        .find_asset(asset_id)
        .ok_or("Image asset not found")?;
    let source_path = _asset
        .source_path
        .clone()
        .ok_or("Image source path not available")?;
    drop(guard);
    replace_image_inner(&svc, object_id, source_path, false)
}

#[tauri::command]
pub fn replace_image(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    file_path: String,
) -> Result<ProjectObject, String> {
    replace_image_inner(&svc, parse_id(&object_id)?, file_path, false)
}

#[tauri::command]
pub fn replace_image_to_fit(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    file_path: String,
) -> Result<ProjectObject, String> {
    replace_image_inner(&svc, parse_id(&object_id)?, file_path, true)
}

fn replace_image_inner(
    svc: &ServiceContext,
    object_id: beambench_core::ObjectId,
    file_path: String,
    preserve_bounds: bool,
) -> Result<ProjectObject, String> {
    let bytes = std::fs::read(&file_path).map_err(|e| format!("Failed to read image: {e}"))?;
    let decoded = beambench_raster::decode::decode_image_oriented(&bytes)
        .map_err(|e| format!("Failed to decode image: {e}"))?;
    let media_type = detect_asset_media_type(&file_path, &bytes)?;
    let filename = std::path::Path::new(&file_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image")
        .to_string();
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    let existing = project
        .find_object(object_id)
        .ok_or("Object not found")?
        .clone();
    let existing_bounds = existing.bounds;
    let (old_asset_key, old_asset_id, adjustments) = match &existing.data {
        ObjectData::RasterImage {
            asset_key,
            adjustments,
            ..
        } => (
            asset_key.clone(),
            Uuid::parse_str(asset_key).ok().map(AssetId::from_uuid),
            adjustments.clone(),
        ),
        _ => return Err("Object is not a raster image".to_string()),
    };
    svc.push_project_undo_snapshot(project)
        .map_err(|e| e.to_string())?;
    let asset = Asset::new(
        filename,
        media_type,
        bytes.len() as u64,
        Some(decoded.width()),
        Some(decoded.height()),
    )
    .with_source_path(absolute_source_path(&file_path));
    let asset_id = asset.id;
    project.add_asset(asset, bytes);
    let layer_id = {
        let object = project
            .find_object_mut(object_id)
            .ok_or("Object not found")?;
        object.data = ObjectData::RasterImage {
            asset_key: asset_id.to_string(),
            original_width_px: decoded.width(),
            original_height_px: decoded.height(),
            adjustments,
            masks: Vec::new(),
        };
        object.layer_id
    };
    beambench_service::ops::project::sync_passthrough_layer_objects(project, layer_id);
    if preserve_bounds {
        if let Some(object) = project.find_object_mut(object_id) {
            object.bounds = existing_bounds;
        }
    }
    if let Some(old_asset_id) = old_asset_id {
        if !project_references_asset_key(project, &old_asset_key) {
            project.remove_asset(old_asset_id);
        }
    }
    let updated = project
        .find_object(object_id)
        .ok_or("Object not found")?
        .clone();
    drop(guard);
    beambench_service::ops::planning::invalidate_plan_cache(&svc).map_err(String::from)?;
    Ok(updated)
}

#[tauri::command]
pub fn adjust_image_preview(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    brightness: f64,
    contrast: f64,
    gamma: f64,
    invert: bool,
    threshold: u8,
    saturation: f64,
    sharpen: f64,
    edge_enhance: bool,
    enhance_radius: f64,
    enhance_amount: f64,
    enhance_denoise: f64,
    mode: String,
    dpi: u32,
    negative: bool,
    pass_through: bool,
    halftone_cells_per_inch: u32,
    halftone_angle_deg: f64,
    newsprint_angle_deg: f64,
    newsprint_frequency: f64,
) -> Result<imports::AdjustImagePreviewOutput, String> {
    imports::adjust_image_preview(
        &svc,
        imports::AdjustImagePreviewInput {
            object_id: parse_id(&object_id)?,
            brightness,
            contrast,
            gamma,
            invert,
            threshold,
            saturation,
            sharpen,
            edge_enhance,
            enhance_radius,
            enhance_amount,
            enhance_denoise,
            mode,
            dpi,
            negative,
            pass_through,
            halftone_cells_per_inch,
            halftone_angle_deg,
            newsprint_angle_deg,
            newsprint_frequency,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn auto_adjust_image(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<imports::AutoAdjustOutput, String> {
    imports::auto_adjust_image(&svc, parse_id(&object_id)?).map_err(Into::into)
}

/// Generate a small dither sample preview image: a horizontal gradient
/// (white→black) processed through the specified raster mode. Returns
/// the result as PNG bytes that the frontend can display directly.
///
/// The gradient is 256×64 pixels at a nominal 254 DPI, so the
/// round-trip is fast (~10ms) and the result clearly shows the
/// visual difference between dither algorithms.
#[tauri::command]
pub fn render_dither_sample(mode: String) -> Result<Vec<u8>, String> {
    use beambench_common::{RasterAdjustments, RasterMode};
    use beambench_raster::{RasterProcessingParams, process_raster};
    use image::{GrayImage, ImageEncoder, Luma, codecs::png::PngEncoder};
    use std::io::Cursor;

    let raster_mode: RasterMode =
        serde_json::from_str(&format!("\"{mode}\"")).map_err(|e| format!("Invalid mode: {e}"))?;

    // Build a 256×64 horizontal gradient: column 0 = white (255),
    // column 255 = black (0). This is the standard test pattern
    // used in the Dither Sample panel.
    let width = 256u32;
    let height = 64u32;
    let mut gradient = GrayImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let v = 255 - x as u8;
            gradient.put_pixel(x, y, Luma([v]));
        }
    }

    // PNG-encode the gradient so process_raster can decode it.
    let mut gradient_png: Vec<u8> = Vec::new();
    {
        let encoder = PngEncoder::new(Cursor::new(&mut gradient_png));
        encoder
            .write_image(
                gradient.as_raw(),
                width,
                height,
                image::ExtendedColorType::L8,
            )
            .map_err(|e| format!("Failed to encode gradient: {e}"))?;
    }

    // Run through the raster pipeline with the specified mode.
    let params = RasterProcessingParams {
        source_bytes: gradient_png,
        bounds_mm: (25.4, 6.4), // 256px at 254 DPI ≈ 25.6mm × 6.4mm
        dpi: 254,
        mode: raster_mode,
        adjustments: RasterAdjustments::default(),
        pass_through: false,
        halftone_cells_per_inch: 10,
        halftone_angle_deg: 0.0,
        newsprint_angle_deg: 45.0,
        newsprint_frequency: 10.0,
        invert: false,
    };

    let processed = process_raster(params).map_err(|e| format!("Raster processing failed: {e}"))?;

    // Convert the processed result back to a displayable PNG.
    // Binary format → expand packed bits to 8-bit gray for the PNG.
    let out_width = processed.width_px;
    let out_height = processed.height_px;
    let gray_pixels: Vec<u8> = match processed.format {
        beambench_raster::types::RasterPixelFormat::Binary => {
            let mut pixels = Vec::with_capacity((out_width * out_height) as usize);
            for y in 0..out_height {
                for x in 0..out_width {
                    let byte_idx =
                        (y as usize * (out_width as usize).div_ceil(8)) + (x as usize / 8);
                    let bit_idx = 7 - (x as usize % 8);
                    let is_burn = byte_idx < processed.data.len()
                        && (processed.data[byte_idx] & (1 << bit_idx)) == 0;
                    pixels.push(if is_burn { 0 } else { 255 });
                }
            }
            pixels
        }
        beambench_raster::types::RasterPixelFormat::Grayscale8 => {
            // Grayscale mode: invert so 0=white, 255=black (display convention)
            processed.data.iter().map(|&v| 255 - v).collect()
        }
    };

    let mut out_png: Vec<u8> = Vec::new();
    {
        let encoder = PngEncoder::new(Cursor::new(&mut out_png));
        encoder
            .write_image(
                &gray_pixels,
                out_width,
                out_height,
                image::ExtendedColorType::L8,
            )
            .map_err(|e| format!("Failed to encode output: {e}"))?;
    }

    Ok(out_png)
}

#[cfg(test)]
mod tests {
    use super::{
        artwork_payload_from_file, effective_raster_asset_key, refresh_image_inner,
        replace_image_inner,
    };
    use beambench_common::{Bounds, Point2D};
    use beambench_core::{
        Asset, AssetId, AssetMediaType, Layer, ObjectData, OperationType, Project, ProjectObject,
    };
    use beambench_service::ServiceContext;
    use image::{ImageBuffer, Rgba};
    use std::sync::Arc;
    use uuid::Uuid;

    fn write_png(dir: &tempfile::TempDir, name: &str, width: u32, height: u32) -> String {
        let path = dir.path().join(name);
        let image = ImageBuffer::from_pixel(width, height, Rgba([255u8, 0, 0, 255]));
        image.save(&path).unwrap();
        path.to_string_lossy().to_string()
    }

    #[test]
    fn artwork_payload_reads_image_file_contents() {
        let dir = tempfile::tempdir().unwrap();
        let png_path = write_png(&dir, "photo.png", 4, 4);

        let payload = artwork_payload_from_file(std::path::Path::new(&png_path))
            .expect("png file should produce a payload");
        assert_eq!(payload.filename, "photo.png");
        assert_eq!(payload.media_type, "image/png");
        assert!(!payload.data_base64.is_empty());
    }

    #[test]
    fn artwork_payload_rejects_unsupported_files() {
        let dir = tempfile::tempdir().unwrap();
        let txt_path = dir.path().join("notes.txt");
        std::fs::write(&txt_path, b"not artwork").unwrap();

        assert!(
            artwork_payload_from_file(&txt_path).is_none(),
            "a copied non-artwork file must paste nothing, never an icon"
        );
    }

    fn project_with_image(
        source_path: Option<String>,
        pass_through: bool,
    ) -> (Arc<ServiceContext>, beambench_core::ObjectId) {
        let ctx = Arc::new(ServiceContext::new());
        let mut project = Project::new("test");
        let mut layer = Layer::new("Image", OperationType::Image);
        if let Some(entry) = layer.entries.first_mut() {
            if let Some(raster) = entry.raster_settings.as_mut() {
                raster.pass_through = pass_through;
                raster.dpi = 100;
            }
        }
        let layer_id = layer.id;
        project.add_layer(layer);
        let asset = Asset::new(
            "old.png".to_string(),
            AssetMediaType::Png,
            4,
            Some(10),
            Some(10),
        )
        .with_source_path(source_path);
        let asset_id = asset.id;
        project.add_asset(asset, vec![0, 1, 2, 3]);
        let object = project.add_object(ProjectObject::new(
            "image",
            layer_id,
            Bounds::new(Point2D::new(5.0, 6.0), Point2D::new(25.0, 36.0)),
            ObjectData::RasterImage {
                asset_key: asset_id.to_string(),
                original_width_px: 10,
                original_height_px: 10,
                adjustments: None,
                masks: Vec::new(),
            },
        ));
        let object_id = object.id;
        *ctx.project.lock().unwrap() = Some(project);
        (ctx, object_id)
    }

    fn raster_asset_id(project: &Project, object_id: beambench_core::ObjectId) -> AssetId {
        let object = project.find_object(object_id).unwrap();
        let ObjectData::RasterImage { asset_key, .. } = &object.data else {
            panic!("expected raster image");
        };
        AssetId::from_uuid(Uuid::parse_str(asset_key).unwrap())
    }

    #[test]
    fn refresh_image_errors_when_source_path_is_missing() {
        let (ctx, object_id) = project_with_image(None, false);

        let err = refresh_image_inner(&ctx, object_id).unwrap_err();

        assert!(err.contains("Image source path not available"));
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn refresh_image_uses_absolute_source_path_and_rewrites_asset_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_png(&dir, "refresh.png", 12, 14);
        let canonical = std::fs::canonicalize(&path)
            .unwrap()
            .to_string_lossy()
            .to_string();
        let (ctx, object_id) = project_with_image(Some(canonical.clone()), false);
        let old_asset_id = {
            let guard = ctx.project.lock().unwrap();
            raster_asset_id(guard.as_ref().unwrap(), object_id)
        };

        let updated = refresh_image_inner(&ctx, object_id).unwrap();

        let ObjectData::RasterImage { asset_key, .. } = updated.data else {
            panic!("expected raster image");
        };
        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        let asset_id = AssetId::from_uuid(Uuid::parse_str(&asset_key).unwrap());
        let asset = project.find_asset(asset_id).unwrap();
        assert_eq!(asset.source_path.as_deref(), Some(canonical.as_str()));
        assert!(project.find_asset(old_asset_id).is_none());
        assert!(!project.asset_data.contains_key(&old_asset_id));
        assert!(ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn replace_image_removes_unreferenced_old_asset_and_clone_resolves_to_new_asset() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_png(&dir, "replacement.png", 12, 14);
        let (ctx, object_id) = project_with_image(None, false);
        let old_asset_id = {
            let mut guard = ctx.project.lock().unwrap();
            let project = guard.as_mut().unwrap();
            let old_asset_id = raster_asset_id(project, object_id);
            let source = project.find_object(object_id).unwrap().clone();
            project.add_object(ProjectObject::new(
                "image clone",
                source.layer_id,
                source.bounds,
                ObjectData::VirtualClone {
                    source_id: object_id,
                },
            ));
            old_asset_id
        };

        let updated = replace_image_inner(&ctx, object_id, path, false).unwrap();

        let ObjectData::RasterImage { asset_key, .. } = updated.data else {
            panic!("expected raster image");
        };
        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        assert!(project.find_asset(old_asset_id).is_none());
        assert!(!project.asset_data.contains_key(&old_asset_id));
        assert!(
            project
                .objects
                .iter()
                .filter_map(|object| effective_raster_asset_key(project, object))
                .all(|key| key != old_asset_id.to_string())
        );
        assert!(
            project
                .objects
                .iter()
                .filter_map(|object| effective_raster_asset_key(project, object))
                .any(|key| key == asset_key)
        );
    }

    #[test]
    fn replace_image_preserves_old_asset_when_another_raster_still_references_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_png(&dir, "replacement.png", 12, 14);
        let (ctx, object_id) = project_with_image(None, false);
        let old_asset_id = {
            let mut guard = ctx.project.lock().unwrap();
            let project = guard.as_mut().unwrap();
            let old_asset_id = raster_asset_id(project, object_id);
            let source = project.find_object(object_id).unwrap().clone();
            project.add_object(ProjectObject::new(
                "sibling image",
                source.layer_id,
                Bounds::new(Point2D::new(40.0, 6.0), Point2D::new(60.0, 36.0)),
                source.data.clone(),
            ));
            old_asset_id
        };

        replace_image_inner(&ctx, object_id, path, false).unwrap();

        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        assert!(project.find_asset(old_asset_id).is_some());
        assert!(project.asset_data.contains_key(&old_asset_id));
    }

    #[test]
    fn replace_image_to_fit_preserves_bounds_on_pass_through_layer() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_png(&dir, "replacement.png", 80, 20);
        let (ctx, object_id) = project_with_image(None, true);
        let before_bounds = {
            let guard = ctx.project.lock().unwrap();
            guard
                .as_ref()
                .unwrap()
                .find_object(object_id)
                .unwrap()
                .bounds
        };

        let updated = replace_image_inner(&ctx, object_id, path.clone(), true).unwrap();

        assert_eq!(updated.bounds, before_bounds);
        let ObjectData::RasterImage { asset_key, .. } = updated.data else {
            panic!("expected raster image");
        };
        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        let asset_id = AssetId::from_uuid(Uuid::parse_str(&asset_key).unwrap());
        let asset = project.find_asset(asset_id).unwrap();
        let canonical = std::fs::canonicalize(path)
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(asset.source_path.as_deref(), Some(canonical.as_str()));
    }
}
