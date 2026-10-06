use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use beambench_common::Bounds;
use beambench_common::ColorTag;
use beambench_common::PALETTE_COLORS;
use beambench_common::Point2D;
use beambench_common::RasterAdjustments;
use beambench_common::geometry::Transform2D;
use beambench_common::path::{PathCommand, VecPath};
use beambench_core::trace::trace_image_preview_fast_cancellable;
use beambench_core::vector::text_to_path::intrinsic_text_bounds;
use beambench_core::vector::transform::bake_transform;
use beambench_core::{
    AssetId, LayerId, LbrnCutEntry, LbrnDocument, LbrnShape, ObjectData, ObjectId, ProjectObject,
    ShapeKind, TextAlignment, TextAlignmentV, TextCirclePlacement, TextLayoutMode,
    TextTransformStyle, TraceConfig, import_image, import_svg, parse_dxf_with_report,
    parse_eps_paths, parse_lbrn_project, parse_pdf_artwork, trace_image,
};
use beambench_core::{
    CutEntry, Layer, OperationType, PdfImage, PdfPaintMode, PdfPaintedPath, PdfRgbColor,
    RasterSettings, VectorSettings,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::context::ServiceContext;
use crate::error::{ServiceError, ServiceResult};
use crate::events;
use crate::ops::planning;
use crate::validation::{RoutingTarget, resolve_layer_for_object};

/// Info-level record of an import auto-routing decision. Emitted once
/// per distinct routing so the frontend can surface a toast per
/// rerouted asset.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct RoutingNotice {
    reason: &'static str,
    from_layer_id: LayerId,
    to_layer_id: LayerId,
    to_layer_name: String,
}

fn routing_reason(target: RoutingTarget) -> &'static str {
    match target {
        RoutingTarget::NeedsImage => "raster_to_image_layer",
        RoutingTarget::NeedsNonImage => "vector_to_non_image_layer",
    }
}

/// Wrap `resolve_layer_for_object` to return a `RoutingNotice` when a
/// rerouting actually happened, so call sites can emit import events.
fn route_with_notice(
    project: &mut beambench_core::Project,
    requested: LayerId,
    target: RoutingTarget,
) -> ServiceResult<(LayerId, Option<RoutingNotice>)> {
    let (resolved, was_rerouted) = resolve_layer_for_object(project, requested, target)?;
    if !was_rerouted {
        return Ok((resolved, None));
    }
    let layer_name = project
        .find_layer(resolved)
        .map(|l| l.name.clone())
        .unwrap_or_default();
    let notice = RoutingNotice {
        reason: routing_reason(target),
        from_layer_id: requested,
        to_layer_id: resolved,
        to_layer_name: layer_name,
    };
    Ok((resolved, Some(notice)))
}

fn first_standard_color() -> &'static str {
    PALETTE_COLORS
        .iter()
        .find(|p| !p.is_tool_layer)
        .map(|p| p.hex)
        .unwrap_or("#000000")
}

fn route_import_with_notice(
    project: &mut beambench_core::Project,
    requested: LayerId,
    target: RoutingTarget,
    allow_tool_imports: bool,
) -> ServiceResult<(LayerId, Option<RoutingNotice>)> {
    let requested_layer = project
        .find_layer(requested)
        .cloned()
        .ok_or_else(|| ServiceError::not_found("Layer not found"))?;

    if !requested_layer.is_tool_layer {
        return route_with_notice(project, requested, target);
    }

    if allow_tool_imports {
        return Ok((requested, None));
    }

    let operation = match target {
        RoutingTarget::NeedsImage => OperationType::Image,
        RoutingTarget::NeedsNonImage => OperationType::Line,
    };

    if let Some(layer) = project
        .layers
        .iter()
        .filter(|layer| !layer.is_tool_layer)
        .find(|layer| target.layer_matches(layer))
    {
        let notice = RoutingNotice {
            reason: routing_reason(target),
            from_layer_id: requested,
            to_layer_id: layer.id,
            to_layer_name: layer.name.clone(),
        };
        return Ok((layer.id, Some(notice)));
    }

    let mut layer = Layer::new(
        match target {
            RoutingTarget::NeedsImage => "Image",
            RoutingTarget::NeedsNonImage => "Line",
        },
        operation,
    );
    layer.color_tag = ColorTag(first_standard_color().to_string());
    let created_id = layer.id;
    let created_name = layer.name.clone();
    project.layers.push(layer);
    for (idx, layer) in project.layers.iter_mut().enumerate() {
        layer.order_index = idx as u32;
    }

    let notice = RoutingNotice {
        reason: routing_reason(target),
        from_layer_id: requested,
        to_layer_id: created_id,
        to_layer_name: created_name,
    };
    Ok((created_id, Some(notice)))
}

fn emit_routing_notices(ctx: &ServiceContext, notices: &[RoutingNotice]) {
    // Deduplicate: emit at most one notice per unique (reason, from, to)
    let mut seen = Vec::new();
    for notice in notices {
        if seen.iter().any(|s: &&RoutingNotice| **s == *notice) {
            continue;
        }
        seen.push(notice);
        ctx.emit_event(
            "project.import.auto_routed",
            json!({
                "reason": notice.reason,
                "from_layer_id": notice.from_layer_id,
                "to_layer_id": notice.to_layer_id,
                "to_layer_name": notice.to_layer_name,
            }),
        );
    }
}

#[derive(Debug, Clone)]
pub struct ImportSvgInput {
    pub file_path: String,
    pub layer_id: LayerId,
}

#[derive(Debug, Clone)]
pub struct ImportImageInput {
    pub file_path: String,
    pub layer_id: LayerId,
}

#[derive(Debug, Clone)]
pub struct ImportFilesInput {
    pub file_paths: Vec<String>,
    pub layer_id: LayerId,
    pub create_layer: Option<super::project::AddObjectLayerInput>,
}

/// One dropped/pasted file delivered by content rather than path. The
/// webview cannot expose OS paths for HTML5 file drops, so the frontend
/// ships the bytes base64-encoded with the original filename for
/// extension dispatch.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportFileData {
    pub filename: String,
    pub data_base64: String,
}

#[derive(Debug, Clone)]
pub struct ImportFilesDataInput {
    pub files: Vec<ImportFileData>,
    pub layer_id: LayerId,
    pub create_layer: Option<super::project::AddObjectLayerInput>,
}

#[derive(Debug, Clone, Copy)]
pub enum VectorImportFormat {
    Dxf,
    Pdf,
    Ai,
    Eps,
}

#[derive(Debug, Clone)]
pub struct ImportVectorFileInput {
    pub file_path: String,
    pub layer_id: LayerId,
    pub format: VectorImportFormat,
}

#[derive(Debug, Clone)]
pub struct ImportGcodeInput {
    pub file_path: String,
    pub layer_id: LayerId,
}

#[derive(Debug, Clone)]
pub struct TraceImageInput {
    pub object_id: ObjectId,
    pub threshold: u8,
    pub cutoff: u8,
    pub turdsize: u32,
    pub alphamax: f64,
    pub opttolerance: f64,
    pub trace_alpha: bool,
    pub sketch_trace: bool,
    pub delete_source: bool,
    pub preview_request_id: Option<u64>,
    pub boundary: Option<TraceBoundaryPx>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TraceBoundaryPx {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NormalizedTraceBoundaryPx {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

enum PendingImport {
    Svg {
        bytes: Vec<u8>,
    },
    Image {
        filename: String,
        bytes: Vec<u8>,
        source_path: Option<String>,
    },
    Vector {
        name_prefix: String,
        paths: Vec<ImportedVectorPath>,
        images: Vec<PdfImage>,
        warnings: Vec<String>,
        /// Source Y axis points up (DXF, EPS, AI). Flipped against the bed so
        /// the source's bottom-left corner lands on the bed's bottom-left.
        y_up: bool,
    },
    Lbrn {
        document: LbrnDocument,
    },
}

/// Vector geometry plus the exact source color, when the source format
/// exposes one. Uncolored formats keep `color_tag` as `None` and retain the
/// caller's normal layer-routing behavior.
#[derive(Debug, Clone)]
struct ImportedVectorPath {
    path: VecPath,
    color_tag: Option<String>,
}

#[derive(Debug, Clone)]
struct ParsedVectorImport {
    paths: Vec<ImportedVectorPath>,
    images: Vec<PdfImage>,
    warnings: Vec<String>,
    y_up: bool,
}

fn pending_vector_import(name_prefix: &str, parsed: ParsedVectorImport) -> PendingImport {
    PendingImport::Vector {
        name_prefix: name_prefix.to_string(),
        paths: parsed.paths,
        images: parsed.images,
        warnings: parsed.warnings,
        y_up: parsed.y_up,
    }
}

impl ImportedVectorPath {
    fn uncolored(path: VecPath) -> Self {
        Self {
            path,
            color_tag: None,
        }
    }

    fn from_pdf(path: PdfPaintedPath) -> Self {
        let color = match path.paint_mode {
            PdfPaintMode::Stroke => path.stroke_color,
            PdfPaintMode::Fill => path.fill_color,
            // Prefer the outline color when both are present, matching the
            // conservative SVG routing contract for laser operations.
            PdfPaintMode::FillStroke => path.stroke_color.or(path.fill_color),
            PdfPaintMode::Unspecified => None,
        };
        Self {
            path: path.path,
            color_tag: color.map(pdf_rgb_color_tag),
        }
    }
}

const MAX_AUTO_COLOR_LAYERS: usize = 16;

fn pdf_rgb_color_tag(color: PdfRgbColor) -> String {
    format!("#{:02X}{:02X}{:02X}", color.r, color.g, color.b)
}

fn lock_err(name: &str, e: impl std::fmt::Display) -> ServiceError {
    ServiceError::internal(format!("Failed to lock {name}: {e}"))
}

#[derive(Clone)]
struct TraceSourceSnapshot {
    object_id: ObjectId,
    source_layer_id: LayerId,
    source_layer_name: String,
    source_name: String,
    bounds: Bounds,
    asset_key: String,
    width_px: u32,
    height_px: u32,
    image_data: Arc<Vec<u8>>,
}

fn load_trace_source_snapshot(
    ctx: &ServiceContext,
    object_id: ObjectId,
) -> ServiceResult<TraceSourceSnapshot> {
    let project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_ref()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    let source = project
        .find_object(object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    let source_layer = project
        .find_layer(source.layer_id)
        .ok_or_else(|| ServiceError::not_found("Layer not found"))?;

    let (asset_key, width_px, height_px) = match &source.data {
        ObjectData::RasterImage {
            asset_key,
            original_width_px,
            original_height_px,
            ..
        } => (asset_key.clone(), *original_width_px, *original_height_px),
        _ => return Err(ServiceError::invalid_input("Object is not a raster image")),
    };

    let asset_id = AssetId::from_uuid(
        Uuid::parse_str(&asset_key)
            .map_err(|_| ServiceError::invalid_input("Invalid image asset reference"))?,
    );
    let image_data = project
        .asset_data
        .get(&asset_id)
        .cloned()
        .ok_or_else(|| ServiceError::not_found("Image asset data not found"))?;

    Ok(TraceSourceSnapshot {
        object_id,
        source_layer_id: source.layer_id,
        source_layer_name: source_layer.name.clone(),
        source_name: source.name.clone(),
        bounds: source.bounds,
        asset_key,
        width_px,
        height_px,
        image_data,
    })
}

fn decode_trace_grayscale(
    ctx: &ServiceContext,
    asset_key: &str,
    image_data: &[u8],
    trace_alpha: bool,
) -> ServiceResult<Arc<image::GrayImage>> {
    let cache_key = crate::context::TracePreviewSourceKey {
        asset_key: asset_key.to_string(),
        trace_alpha,
    };
    {
        let mut cache = ctx
            .trace_preview_source_cache
            .lock()
            .map_err(|e| lock_err("trace_preview_source_cache", e))?;
        if let Some(image) = cache.get(&cache_key) {
            return Ok(image);
        }
    }

    let decoded = image::load_from_memory(image_data)
        .map_err(|e| ServiceError::invalid_input(format!("Failed to decode image: {e}")))?;
    let grayscale = if trace_alpha {
        let rgba = decoded.to_rgba8();
        let (w, h) = rgba.dimensions();
        let mut gray = image::GrayImage::new(w, h);
        for py in 0..h {
            for px in 0..w {
                gray.put_pixel(px, py, image::Luma([rgba.get_pixel(px, py)[3]]));
            }
        }
        gray
    } else {
        decoded.to_luma8()
    };
    let grayscale = Arc::new(grayscale);

    let mut cache = ctx
        .trace_preview_source_cache
        .lock()
        .map_err(|e| lock_err("trace_preview_source_cache", e))?;
    cache.put(cache_key, grayscale.clone());
    Ok(grayscale)
}

fn build_trace_config(input: &TraceImageInput) -> TraceConfig {
    TraceConfig {
        threshold: input.threshold,
        cutoff: input.cutoff,
        turdsize: input.turdsize,
        alphamax: input.alphamax,
        opttolerance: input.opttolerance,
        invert: input.trace_alpha,
        sketch_trace: input.sketch_trace,
        ..Default::default()
    }
}

fn build_preview_trace_config(input: &TraceImageInput) -> TraceConfig {
    let mut config = build_trace_config(input);
    config.supersample_factor = 1;
    config.optimizer_run_limit = 2;
    config
}

fn ensure_trace_preview_request_current(
    ctx: &ServiceContext,
    request_id: Option<u64>,
) -> ServiceResult<()> {
    let Some(request_id) = request_id else {
        return Ok(());
    };
    if ctx.latest_trace_preview_request_id.load(Ordering::Acquire) == request_id {
        return Ok(());
    }
    Err(ServiceError::stale_revision(
        "Trace preview request superseded by a newer request",
    ))
}

#[cfg(test)]
type TracePreviewCancelCheckHook =
    Arc<dyn Fn(&ServiceContext, Option<u64>) + Send + Sync + 'static>;

#[cfg(test)]
static TRACE_PREVIEW_CANCEL_CHECK_HOOK: std::sync::OnceLock<
    std::sync::Mutex<Option<TracePreviewCancelCheckHook>>,
> = std::sync::OnceLock::new();

#[cfg(test)]
pub(super) fn set_trace_preview_cancel_check_hook(hook: Option<TracePreviewCancelCheckHook>) {
    let hook_cell = TRACE_PREVIEW_CANCEL_CHECK_HOOK.get_or_init(|| std::sync::Mutex::new(None));
    if let Ok(mut slot) = hook_cell.lock() {
        *slot = hook;
    }
}

#[cfg(test)]
fn run_trace_preview_cancel_check_hook(ctx: &ServiceContext, request_id: Option<u64>) {
    let hook = TRACE_PREVIEW_CANCEL_CHECK_HOOK
        .get()
        .and_then(|cell| cell.lock().ok().and_then(|slot| slot.clone()));
    if let Some(hook) = hook {
        hook(ctx, request_id);
    }
}

fn trace_preview_should_cancel(ctx: &ServiceContext, request_id: Option<u64>) -> bool {
    #[cfg(test)]
    run_trace_preview_cancel_check_hook(ctx, request_id);
    ensure_trace_preview_request_current(ctx, request_id).is_err()
}

fn clamp_trace_preview_image(
    grayscale: &image::GrayImage,
    max_side_px: u32,
) -> (image::GrayImage, u32, u32) {
    let (width, height) = grayscale.dimensions();
    if width == 0 || height == 0 {
        return (grayscale.clone(), width, height);
    }

    let longest_side = width.max(height);
    if longest_side <= max_side_px {
        return (grayscale.clone(), width, height);
    }

    let scale = max_side_px as f64 / longest_side as f64;
    let target_width = ((width as f64 * scale).round() as u32).max(1);
    let target_height = ((height as f64 * scale).round() as u32).max(1);
    (
        image::imageops::resize(
            grayscale,
            target_width,
            target_height,
            image::imageops::FilterType::Triangle,
        ),
        target_width,
        target_height,
    )
}

fn normalize_trace_boundary(
    boundary: Option<TraceBoundaryPx>,
    source_width: u32,
    source_height: u32,
) -> ServiceResult<Option<NormalizedTraceBoundaryPx>> {
    let Some(boundary) = boundary else {
        return Ok(None);
    };
    if source_width == 0 || source_height == 0 {
        return Err(ServiceError::invalid_input(
            "Trace boundary requires a non-empty source image",
        ));
    }
    if !boundary.x.is_finite()
        || !boundary.y.is_finite()
        || !boundary.width.is_finite()
        || !boundary.height.is_finite()
    {
        return Err(ServiceError::invalid_input(
            "Trace boundary contains invalid coordinates",
        ));
    }

    let x2 = boundary.x + boundary.width;
    let y2 = boundary.y + boundary.height;
    let min_x = boundary.x.min(x2).floor().max(0.0);
    let min_y = boundary.y.min(y2).floor().max(0.0);
    let max_x = boundary.x.max(x2).ceil().min(source_width as f64);
    let max_y = boundary.y.max(y2).ceil().min(source_height as f64);

    if max_x <= min_x || max_y <= min_y {
        return Err(ServiceError::invalid_input(
            "Trace boundary is outside the image or has zero area",
        ));
    }

    let x = min_x as u32;
    let y = min_y as u32;
    let width = (max_x as u32).saturating_sub(x);
    let height = (max_y as u32).saturating_sub(y);
    if width == 0 || height == 0 {
        return Err(ServiceError::invalid_input(
            "Trace boundary is outside the image or has zero area",
        ));
    }
    if x == 0 && y == 0 && width == source_width && height == source_height {
        return Ok(None);
    }

    Ok(Some(NormalizedTraceBoundaryPx {
        x,
        y,
        width,
        height,
    }))
}

fn trace_image_for_boundary<'a>(
    grayscale: &'a image::GrayImage,
    boundary: Option<NormalizedTraceBoundaryPx>,
) -> (Cow<'a, image::GrayImage>, f64, f64) {
    match boundary {
        Some(boundary) => (
            Cow::Owned(
                image::imageops::crop_imm(
                    grayscale,
                    boundary.x,
                    boundary.y,
                    boundary.width,
                    boundary.height,
                )
                .to_image(),
            ),
            boundary.x as f64,
            boundary.y as f64,
        ),
        None => (Cow::Borrowed(grayscale), 0.0, 0.0),
    }
}

fn read_file_bytes(path: &str) -> ServiceResult<Vec<u8>> {
    std::fs::read(path).map_err(|e| ServiceError::persistence(format!("Failed to read file: {e}")))
}

fn absolute_source_path(path: &str) -> Option<String> {
    std::fs::canonicalize(path)
        .ok()
        .and_then(|path| path.to_str().map(ToOwned::to_owned))
}

fn prepare_pending_imports(file_paths: Vec<String>) -> ServiceResult<Vec<PendingImport>> {
    let mut pending = Vec::new();
    for file_path in file_paths {
        let bytes = read_file_bytes(&file_path)?;
        let source_path = absolute_source_path(&file_path);
        pending.push(prepare_pending_import(&file_path, bytes, source_path)?);
    }
    Ok(pending)
}

/// Largest vector document (SVG, DXF, EPS, AI, LightBurn) parsed in memory.
const VECTOR_IMPORT_BYTE_LIMIT: usize = 64 * 1024 * 1024;
/// Most objects one import may add. Matches what a saved project can reopen.
const IMPORT_OBJECT_LIMIT: usize = 250_000;

fn check_vector_import_size(bytes: &[u8]) -> ServiceResult<()> {
    if bytes.len() > VECTOR_IMPORT_BYTE_LIMIT {
        return Err(ServiceError::invalid_input(format!(
            "This file is larger than the {} MB limit for vector imports. Simplify it or split it into smaller files.",
            VECTOR_IMPORT_BYTE_LIMIT / (1024 * 1024)
        )));
    }
    Ok(())
}

/// Build a single pending import from in-memory content. `filename` may be a
/// full path or a bare name; only its extension and stem are used.
/// `source_path` is the on-disk origin when known (path-based imports only) —
/// content-based imports (drops, pastes) pass `None`.
fn prepare_pending_import(
    filename: &str,
    bytes: Vec<u8>,
    source_path: Option<String>,
) -> ServiceResult<PendingImport> {
    let path = PathBuf::from(filename);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    // DXF, EPS and AI are checked by the shared vector parser.
    if matches!(ext.as_str(), "svg" | "lbrn" | "lbrn2") {
        check_vector_import_size(&bytes)?;
    }

    match ext.as_str() {
        "svg" => Ok(PendingImport::Svg { bytes }),
        "png" | "jpg" | "jpeg" | "bmp" | "gif" | "tif" | "tiff" | "webp" | "tga" => {
            Ok(PendingImport::Image {
                filename: path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("image")
                    .to_string(),
                bytes,
                source_path,
            })
        }
        "dxf" | "pdf" | "ai" | "eps" => {
            let format = match ext.as_str() {
                "dxf" => VectorImportFormat::Dxf,
                "pdf" => VectorImportFormat::Pdf,
                "ai" => VectorImportFormat::Ai,
                "eps" => VectorImportFormat::Eps,
                _ => unreachable!(),
            };
            let name_prefix = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .filter(|stem| !stem.is_empty())
                .unwrap_or("Import")
                .to_string();
            let parsed = parse_vector_bytes(&bytes, format)?;
            Ok(pending_vector_import(&name_prefix, parsed))
        }
        "lbrn" | "lbrn2" => Ok(PendingImport::Lbrn {
            document: parse_lbrn_project(&bytes).map_err(|error| {
                ServiceError::invalid_input(format!("Lbrn import failed: {error}"))
            })?,
        }),
        _ => Err(ServiceError::invalid_input(format!(
            "Unsupported file type: .{ext}"
        ))),
    }
}

// Lbrn's layer indices use a fixed palette whose order differs from
// Beam Bench's native palette (notably C01 is blue and C02 is red). Keep the
// source index and its exact display color together during project migration.
const LBRN_PALETTE_COLORS: [&str; 32] = [
    "#000000", "#0000FF", "#FF0000", "#00E000", "#D0D000", "#FF8000", "#00E0E0", "#FF00FF",
    "#B4B4B4", "#0000A0", "#A00000", "#00A000", "#A0A000", "#C08000", "#00A0FF", "#A000A0",
    "#808080", "#7D87B9", "#BB7784", "#4A6FE3", "#D33F6A", "#8CD78C", "#F0B98D", "#F6C4E1",
    "#FA9ED4", "#500A78", "#B45A00", "#004754", "#86FA88", "#FFDB66", "#F36926", "#0C96D9",
];

fn lbrn_palette_color(index: u32) -> ColorTag {
    LBRN_PALETTE_COLORS
        .get(index as usize)
        .map(|color| ColorTag((*color).to_string()))
        .unwrap_or_else(|| ColorTag(first_standard_color().to_string()))
}

fn lbrn_layer_name(index: u32) -> String {
    match index {
        30 => "T1".to_string(),
        31 => "T2".to_string(),
        _ => format!("C{index:02}"),
    }
}

fn unique_imported_layer_name(project: &beambench_core::Project, preferred: &str) -> String {
    if !project.layers.iter().any(|layer| layer.name == preferred) {
        return preferred.to_string();
    }
    let base = format!("{preferred} (Lbrn)");
    if !project.layers.iter().any(|layer| layer.name == base) {
        return base;
    }
    for suffix in 2.. {
        let candidate = format!("{base} {suffix}");
        if !project.layers.iter().any(|layer| layer.name == candidate) {
            return candidate;
        }
    }
    unreachable!()
}

fn lbrn_cut_entry(source: &LbrnCutEntry) -> CutEntry {
    let mut entry = CutEntry::new(source.operation);
    entry.speed_mm_min = source.speed_mm_min.max(0.0);
    entry.power_percent = source.power_percent.clamp(0.0, 100.0);
    entry.power_min_percent = source.power_min_percent.clamp(0.0, 100.0);
    entry.air_assist = source.air_assist;
    entry.output_enabled = source.output_enabled;

    if source.operation.uses_raster_settings() {
        let settings = entry
            .raster_settings
            .get_or_insert_with(RasterSettings::default);
        settings.passes = source.passes.max(1);
        if let Some(interval) = source.line_interval_mm.filter(|value| *value > 0.0) {
            settings.line_interval_mm = interval;
            settings.dpi = (25.4 / interval).round().clamp(1.0, u32::MAX as f64) as u32;
        }
        if let Some(angle) = source.scan_angle_deg {
            settings.scan_angle = angle;
        }
        settings.crosshatch = source.crosshatch;
        if let Some(mode) = source.raster_mode {
            settings.mode = mode;
        }
    }
    if source.operation.uses_vector_settings() {
        let settings = entry
            .vector_settings
            .get_or_insert_with(VectorSettings::default);
        settings.passes = source.passes.max(1);
    }
    entry
}

fn install_lbrn_layer(
    project: &mut beambench_core::Project,
    source: &beambench_core::LbrnLayer,
) -> LayerId {
    let color = lbrn_palette_color(source.index);
    let reusable = project.layers.iter().position(|layer| {
        layer.color_tag == color
            && layer.is_tool_layer == source.is_tool_layer
            && !project
                .objects
                .iter()
                .any(|object| object.layer_id == layer.id)
    });

    let mut imported = if source.is_tool_layer {
        let mut layer = Layer::new(&source.name, OperationType::Tool);
        layer.canonicalize_tool_layer();
        layer
    } else {
        let entries = source
            .entries
            .iter()
            .map(lbrn_cut_entry)
            .collect::<Vec<_>>();
        let primary = entries
            .first()
            .map(|entry| entry.operation)
            .unwrap_or(OperationType::Line);
        let mut layer = Layer::new(&source.name, primary);
        layer.entries = if entries.is_empty() {
            vec![CutEntry::new(primary)]
        } else {
            entries
        };
        layer
    };
    imported.color_tag = color;

    if let Some(position) = reusable {
        let id = project.layers[position].id;
        imported.id = id;
        imported.order_index = project.layers[position].order_index;
        project.layers[position] = imported;
        project.dirty = true;
        id
    } else {
        imported.name = unique_imported_layer_name(project, &imported.name);
        let id = imported.id;
        project.add_layer(imported);
        id
    }
}

fn collect_lbrn_layer_requirements(
    shape: &LbrnShape,
    requirements: &mut HashMap<u32, OperationType>,
) {
    match shape {
        LbrnShape::Rectangle { layer_index, .. }
        | LbrnShape::Ellipse { layer_index, .. }
        | LbrnShape::Path { layer_index, .. }
        | LbrnShape::Text { layer_index, .. } => {
            requirements
                .entry(*layer_index)
                .or_insert(OperationType::Line);
        }
        LbrnShape::Bitmap { layer_index, .. } => {
            requirements.insert(*layer_index, OperationType::Image);
        }
        LbrnShape::Group { children } => {
            for child in children {
                collect_lbrn_layer_requirements(child, requirements);
            }
        }
    }
}

fn linear_object_transform(transform: Transform2D) -> Transform2D {
    Transform2D {
        tx: 0.0,
        ty: 0.0,
        ..transform
    }
}

/// Convert LightBurn's Y-up project coordinates into Beam Bench's Y-down
/// canvas coordinates. Root mirror flags describe axis mirroring in the
/// LightBurn project and are applied before the coordinate-system conversion.
fn lbrn_world_to_canvas_transform(
    project: &beambench_core::Project,
    mirror_x: bool,
    mirror_y: bool,
) -> Transform2D {
    Transform2D {
        a: if mirror_x { -1.0 } else { 1.0 },
        b: 0.0,
        c: 0.0,
        // LightBurn geometry is Y-up. A mirrored LightBurn Y axis already
        // points down, so it does not need the usual canvas-axis inversion.
        d: if mirror_y { 1.0 } else { -1.0 },
        tx: if mirror_x {
            project.workspace.bed_width_mm
        } else {
            0.0
        },
        ty: if mirror_y {
            0.0
        } else {
            project.workspace.bed_height_mm
        },
    }
}

/// Rectangles, ellipses, and text are reconstructed from native Beam
/// Bench primitives whose local coordinates are already Y-down. Conjugating
/// the source transform with a local Y flip converts rotation/shear without
/// turning text upside down. Bitmaps use LightBurn's row coordinates directly.
fn lbrn_semantic_object_transform(
    world_to_canvas: Transform2D,
    source: Transform2D,
) -> Transform2D {
    world_to_canvas
        .compose(&source)
        .compose(&Transform2D::scale(1.0, -1.0))
}

fn transform_lbrn_path(mut path: VecPath, transform: Transform2D) -> VecPath {
    let transform_point = |x: &mut f64, y: &mut f64| {
        let point = transform.apply(&Point2D::new(*x, *y));
        *x = point.x;
        *y = point.y;
    };

    for subpath in &mut path.subpaths {
        for command in &mut subpath.commands {
            match command {
                PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => {
                    transform_point(x, y);
                }
                PathCommand::QuadTo { cx, cy, x, y } => {
                    transform_point(cx, cy);
                    transform_point(x, y);
                }
                PathCommand::CubicTo {
                    c1x,
                    c1y,
                    c2x,
                    c2y,
                    x,
                    y,
                } => {
                    transform_point(c1x, c1y);
                    transform_point(c2x, c2y);
                    transform_point(x, y);
                }
                PathCommand::Close => {}
            }
        }
    }
    path
}

fn centered_bounds(transform: Transform2D, width: f64, height: f64) -> Bounds {
    let half_w = width.max(0.0) / 2.0;
    let half_h = height.max(0.0) / 2.0;
    Bounds::new(
        Point2D::new(transform.tx - half_w, transform.ty - half_h),
        Point2D::new(transform.tx + half_w, transform.ty + half_h),
    )
}

fn lbrn_text_alignment(value: i32) -> TextAlignment {
    match value {
        1 => TextAlignment::Center,
        2 => TextAlignment::Right,
        _ => TextAlignment::Left,
    }
}

fn lbrn_text_alignment_v(value: i32) -> TextAlignmentV {
    match value {
        1 => TextAlignmentV::Middle,
        2 => TextAlignmentV::Bottom,
        _ => TextAlignmentV::Top,
    }
}

fn text_bounds(
    transform: Transform2D,
    content: &str,
    height: f64,
    horizontal_alignment: i32,
    vertical_alignment: i32,
) -> Bounds {
    let width = (height * content.chars().count() as f64 * 0.6).max(height);
    let min_x = match horizontal_alignment {
        1 => transform.tx - width / 2.0,
        2 => transform.tx - width,
        _ => transform.tx,
    };
    let min_y = match vertical_alignment {
        1 => transform.ty - height / 2.0,
        2 => transform.ty - height,
        _ => transform.ty,
    };
    Bounds::new(
        Point2D::new(min_x, min_y),
        Point2D::new(min_x + width, min_y + height),
    )
}

fn import_lbrn_shape(
    project: &mut beambench_core::Project,
    shape: LbrnShape,
    layer_ids: &HashMap<u32, LayerId>,
    world_to_canvas: Transform2D,
) -> ServiceResult<Vec<ProjectObject>> {
    let mut created = Vec::new();
    match shape {
        LbrnShape::Rectangle {
            layer_index,
            transform,
            width_mm,
            height_mm,
            corner_radius_mm,
        } => {
            let transform = lbrn_semantic_object_transform(world_to_canvas, transform);
            let layer_id = *layer_ids
                .get(&layer_index)
                .ok_or_else(|| ServiceError::internal("Missing imported Lbrn layer"))?;
            let mut object = ProjectObject::new(
                "Lbrn Rectangle",
                layer_id,
                centered_bounds(transform, width_mm, height_mm),
                ObjectData::Shape {
                    kind: ShapeKind::Rectangle,
                    width: width_mm,
                    height: height_mm,
                    corner_radius: corner_radius_mm,
                },
            );
            object.transform = linear_object_transform(transform);
            created.push(project.add_object(object).clone());
        }
        LbrnShape::Ellipse {
            layer_index,
            transform,
            radius_x_mm,
            radius_y_mm,
        } => {
            let transform = lbrn_semantic_object_transform(world_to_canvas, transform);
            let layer_id = *layer_ids
                .get(&layer_index)
                .ok_or_else(|| ServiceError::internal("Missing imported Lbrn layer"))?;
            let width = radius_x_mm * 2.0;
            let height = radius_y_mm * 2.0;
            let mut object = ProjectObject::new(
                "Lbrn Ellipse",
                layer_id,
                centered_bounds(transform, width, height),
                ObjectData::Shape {
                    kind: ShapeKind::Ellipse,
                    width,
                    height,
                    corner_radius: 0.0,
                },
            );
            object.transform = linear_object_transform(transform);
            created.push(project.add_object(object).clone());
        }
        LbrnShape::Path { layer_index, path } => {
            let path = transform_lbrn_path(path, world_to_canvas);
            let layer_id = *layer_ids
                .get(&layer_index)
                .ok_or_else(|| ServiceError::internal("Missing imported Lbrn layer"))?;
            created.push(
                project
                    .add_object(vecpath_to_project_object(
                        layer_id,
                        "Lbrn Path".to_string(),
                        path,
                    ))
                    .clone(),
            );
        }
        LbrnShape::Text {
            layer_index,
            transform,
            content,
            font_family,
            font_height_mm,
            horizontal_alignment,
            vertical_alignment,
            bold,
            italic,
            welded,
            letter_spacing_mm,
            line_spacing_mm,
        } => {
            let transform = lbrn_semantic_object_transform(world_to_canvas, transform);
            let layer_id = *layer_ids
                .get(&layer_index)
                .ok_or_else(|| ServiceError::internal("Missing imported Lbrn layer"))?;
            let name = format!("Text: {}", content.chars().take(40).collect::<String>());
            let mut object = ProjectObject::new(
                name,
                layer_id,
                text_bounds(
                    transform,
                    &content,
                    font_height_mm,
                    horizontal_alignment,
                    vertical_alignment,
                ),
                ObjectData::Text {
                    content,
                    font_family,
                    font_size_mm: font_height_mm,
                    alignment: lbrn_text_alignment(horizontal_alignment),
                    alignment_v: lbrn_text_alignment_v(vertical_alignment),
                    bold,
                    italic,
                    upper_case: false,
                    welded,
                    h_spacing: letter_spacing_mm,
                    v_spacing: line_spacing_mm,
                    on_path: false,
                    path_offset: 0.0,
                    distort: false,
                    layout_mode: TextLayoutMode::Straight,
                    rtl: false,
                    bend_radius: 0.0,
                    transform_style: TextTransformStyle::None,
                    transform_curve: 0.0,
                    circle_placement: TextCirclePlacement::TopOutside,
                    max_width: None,
                    squeeze: false,
                    ignore_empty_vars: false,
                    resolved_font_source: None,
                    resolved_font_key: None,
                    resolved_path_data: None,
                    missing_font: false,
                    missing_glyphs: Vec::new(),
                    guide_path_id: None,
                    variable_text: None,
                },
            );
            object.transform = linear_object_transform(transform);
            created.push(project.add_object(object).clone());
        }
        LbrnShape::Bitmap {
            layer_index,
            power_scale,
            transform,
            width_mm,
            height_mm,
            filename,
            data,
            adjustments,
        } => {
            // LightBurn maps embedded bitmap row zero to local negative Y.
            // Applying the semantic-shape Y correction here inverts its pixels.
            let transform = world_to_canvas.compose(&transform);
            let layer_id = *layer_ids
                .get(&layer_index)
                .ok_or_else(|| ServiceError::internal("Missing imported Lbrn layer"))?;
            let object_id =
                import_image(&data, &filename, None, project, layer_id).map_err(|error| {
                    ServiceError::invalid_input(format!("Lbrn bitmap import failed: {error}"))
                })?;
            let object = project
                .find_object_mut(object_id)
                .ok_or_else(|| ServiceError::internal("Imported Lbrn bitmap is missing"))?;
            object.power_scale = power_scale;
            object.bounds = centered_bounds(transform, width_mm, height_mm);
            object.transform = linear_object_transform(transform);
            if let ObjectData::RasterImage {
                adjustments: object_adjustments,
                ..
            } = &mut object.data
            {
                *object_adjustments =
                    (adjustments != RasterAdjustments::default()).then_some(adjustments);
            }
            created.push(object.clone());
        }
        LbrnShape::Group { children } => {
            let mut child_ids = Vec::new();
            for child in children {
                let imported = import_lbrn_shape(project, child, layer_ids, world_to_canvas)?;
                child_ids.extend(imported.iter().map(|object| object.id));
                created.extend(imported);
            }
            if child_ids.len() >= 2 {
                let group = super::vector::group_objects_in_project(
                    project,
                    super::vector::GroupObjectsInput {
                        object_ids: child_ids,
                    },
                )?;
                created.push(group);
            }
        }
    }
    Ok(created)
}

fn import_lbrn_document(
    project: &mut beambench_core::Project,
    document: LbrnDocument,
) -> ServiceResult<(Vec<ProjectObject>, Vec<String>)> {
    let world_to_canvas =
        lbrn_world_to_canvas_transform(project, document.mirror_x, document.mirror_y);
    let mut layer_ids = HashMap::new();
    for source in &document.layers {
        layer_ids.insert(source.index, install_lbrn_layer(project, source));
    }

    let mut requirements = HashMap::new();
    for shape in &document.shapes {
        collect_lbrn_layer_requirements(shape, &mut requirements);
    }
    let mut missing = requirements
        .into_iter()
        .filter(|(index, _)| !layer_ids.contains_key(index))
        .collect::<Vec<_>>();
    missing.sort_by_key(|(index, _)| *index);
    for (index, operation) in missing {
        let source = beambench_core::LbrnLayer {
            index,
            name: lbrn_layer_name(index),
            priority: index as i32,
            entries: vec![LbrnCutEntry {
                operation,
                speed_mm_min: 1000.0,
                power_percent: 50.0,
                power_min_percent: 0.0,
                passes: 1,
                air_assist: false,
                output_enabled: true,
                line_interval_mm: None,
                scan_angle_deg: None,
                crosshatch: false,
                raster_mode: None,
            }],
            is_tool_layer: index >= 30,
        };
        layer_ids.insert(index, install_lbrn_layer(project, &source));
    }

    if project.material_height_mm.is_none() {
        project.material_height_mm = document.material_height_mm;
    }
    if !document.notes.trim().is_empty() {
        if project.notes.trim().is_empty() {
            project.notes = document.notes.clone();
        } else {
            project.notes.push_str("\n\nLbrn import:\n");
            project.notes.push_str(&document.notes);
        }
    }

    let mut created = Vec::new();
    for shape in document.shapes {
        created.extend(import_lbrn_shape(
            project,
            shape,
            &layer_ids,
            world_to_canvas,
        )?);
    }
    project.dirty = true;
    Ok((created, document.warnings))
}

fn import_pending(
    ctx: &ServiceContext,
    layer_id: LayerId,
    pending: Vec<PendingImport>,
    create_layer: Option<super::project::AddObjectLayerInput>,
) -> ServiceResult<Vec<ProjectObject>> {
    let file_count = pending.len();
    if pending.is_empty() {
        return Ok(Vec::new());
    }
    let imports_document = pending
        .iter()
        .any(|item| matches!(item, PendingImport::Lbrn { .. }));

    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let original = project_guard
        .as_ref()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    // Stage the entire batch before changing the live project or undo history.
    // Asset bytes are Arc-backed and stay shared with the original project.
    let mut working = original.clone();
    let project = &mut working;
    let layer_id = if let Some(spec) = create_layer {
        let mut layer = Layer::new_single_entry(spec.name, spec.operation);
        if let Some(color_tag) = spec.color_tag {
            layer.color_tag =
                ColorTag(beambench_common::canonical_palette_color_tag(&color_tag).to_string());
            layer.is_tool_layer = beambench_common::is_tool_color(&layer.color_tag.0);
        }
        if layer.is_tool_layer || layer.primary_entry().operation == OperationType::Tool {
            layer.canonicalize_tool_layer();
        } else if let Some(patch) = spec.entry_patch {
            layer.entries[0].apply_patch(&patch);
        }
        project.add_layer(layer).id
    } else {
        layer_id
    };
    if project.find_layer(layer_id).is_none() {
        return Err(ServiceError::not_found("Layer not found"));
    }

    let allow_tool_imports = ctx
        .settings
        .lock()
        .map_err(|e| lock_err("settings", e))?
        .allow_importing_to_tool_layers;

    let mut imported_objects = Vec::new();
    let mut routing_notices: Vec<RoutingNotice> = Vec::new();
    let mut import_warnings: Vec<String> = Vec::new();
    for item in pending {
        // Per-item auto-routing: images must land on an image layer;
        // vectors/SVG must land on a non-image layer. Resolved once
        // per item so a batch can split across multiple layers.
        let effective_layer_id = match &item {
            PendingImport::Lbrn { .. } => layer_id,
            PendingImport::Image { .. } => {
                let (resolved, notice) = route_import_with_notice(
                    project,
                    layer_id,
                    RoutingTarget::NeedsImage,
                    allow_tool_imports,
                )?;
                if let Some(notice) = notice {
                    routing_notices.push(notice);
                }
                resolved
            }
            PendingImport::Vector { paths, .. } if paths.is_empty() => layer_id,
            PendingImport::Svg { .. } | PendingImport::Vector { .. } => {
                let (resolved, notice) = route_import_with_notice(
                    project,
                    layer_id,
                    RoutingTarget::NeedsNonImage,
                    allow_tool_imports,
                )?;
                if let Some(notice) = notice {
                    routing_notices.push(notice);
                }
                resolved
            }
        };

        match item {
            PendingImport::Svg { bytes } => {
                let ids = import_svg(&bytes, project, effective_layer_id)
                    .map_err(|e| ServiceError::invalid_input(format!("SVG import failed: {e}")))?;

                let raster_ids = ids
                    .iter()
                    .copied()
                    .filter(|id| {
                        project.find_object(*id).is_some_and(|object| {
                            matches!(object.data, ObjectData::RasterImage { .. })
                        })
                    })
                    .collect::<Vec<_>>();
                if !raster_ids.is_empty() {
                    let (image_layer_id, notice) = route_import_with_notice(
                        project,
                        layer_id,
                        RoutingTarget::NeedsImage,
                        allow_tool_imports,
                    )?;
                    if let Some(notice) = notice {
                        routing_notices.push(notice);
                    }
                    for raster_id in raster_ids {
                        if let Some(object) = project.find_object_mut(raster_id) {
                            object.layer_id = image_layer_id;
                        }
                    }
                }
                imported_objects.extend(
                    ids.iter()
                        .filter_map(|id| project.find_object(*id).cloned())
                        .collect::<Vec<_>>(),
                );
            }
            PendingImport::Image {
                filename,
                bytes,
                source_path,
            } => {
                let obj_id =
                    import_image(&bytes, &filename, source_path, project, effective_layer_id)
                        .map_err(|e| {
                            ServiceError::invalid_input(format!("Image import failed: {e}"))
                        })?;
                // If target layer has pass-through enabled, resize to native-DPI size
                super::project::sync_passthrough_single_object(project, obj_id);
                let obj = project
                    .find_object(obj_id)
                    .ok_or_else(|| ServiceError::internal("Failed to find imported object"))?
                    .clone();
                imported_objects.push(obj);
            }
            PendingImport::Vector {
                name_prefix,
                mut paths,
                images,
                warnings,
                y_up,
            } => {
                if y_up {
                    let flip = y_up_to_canvas(project.workspace.bed_height_mm);
                    for imported in &mut paths {
                        imported.path = bake_transform(&imported.path, &flip);
                    }
                }
                imported_objects.extend(add_imported_vector_paths(
                    project,
                    effective_layer_id,
                    &name_prefix,
                    paths,
                )?);
                if !images.is_empty() {
                    let (image_layer_id, notice) = route_import_with_notice(
                        project,
                        layer_id,
                        RoutingTarget::NeedsImage,
                        allow_tool_imports,
                    )?;
                    if let Some(notice) = notice {
                        routing_notices.push(notice);
                    }
                    for (index, image) in images.into_iter().enumerate() {
                        let name = format!("{name_prefix} image {}", index + 1);
                        let id = import_image(&image.png, &name, None, project, image_layer_id)
                            .map_err(|e| {
                                ServiceError::invalid_input(format!("PDF image import failed: {e}"))
                            })?;
                        let mask_object = if let Some(clip) = image.clip {
                            let mask_layer = if let Some(layer) = project
                                .layers
                                .iter()
                                .find(|l| l.is_tool_layer && l.name == "PDF clipping masks")
                            {
                                layer.id
                            } else {
                                let mut layer =
                                    Layer::new("PDF clipping masks", OperationType::Tool);
                                layer.canonicalize_tool_layer();
                                layer.visible = false;
                                project.add_layer(layer).id
                            };
                            let mask = vecpath_to_project_object(
                                mask_layer,
                                format!("{name} clipping"),
                                clip,
                            );
                            Some(project.add_object(mask).clone())
                        } else {
                            None
                        };
                        let object = project
                            .find_object_mut(id)
                            .ok_or_else(|| ServiceError::internal("Missing imported PDF image"))?;
                        let t = image.transform;
                        let width = t.a.hypot(t.b);
                        let height = t.c.hypot(t.d);
                        let center = t.apply(&Point2D::new(0.5, 0.5));
                        object.bounds = Bounds::new(
                            Point2D::new(center.x - width / 2.0, center.y - height / 2.0),
                            Point2D::new(center.x + width / 2.0, center.y + height / 2.0),
                        );
                        object.transform = Transform2D {
                            a: t.a / width,
                            b: t.b / width,
                            c: t.c / height,
                            d: t.d / height,
                            tx: 0.0,
                            ty: 0.0,
                        };
                        if let Some(mask) = &mask_object
                            && let ObjectData::RasterImage { masks, .. } = &mut object.data
                        {
                            masks.push(beambench_core::ImageMaskRef {
                                object_id: mask.id,
                                polarity: beambench_core::ImageMaskPolarity::KeepInside,
                            });
                        }
                        imported_objects.push(object.clone());
                        if let Some(mask) = mask_object {
                            let mask_id = mask.id;
                            imported_objects.push(mask);
                            let group = super::vector::group_objects_in_project(
                                project,
                                super::vector::GroupObjectsInput {
                                    object_ids: vec![id, mask_id],
                                },
                            )?;
                            imported_objects.push(group);
                        }
                    }
                }
                import_warnings.extend(warnings);
            }
            PendingImport::Lbrn { document } => {
                let (objects, warnings) = import_lbrn_document(project, document)?;
                imported_objects.extend(objects);
                import_warnings.extend(warnings);
            }
        }
    }

    if imported_objects.is_empty() && !imports_document {
        return Ok(imported_objects);
    }

    // Refresh text caches for any imported text objects (SVG text import
    // creates objects with resolved_path_data: None).
    super::project::refresh_project_text_caches(project);

    // Resize text object bounds from actual resolved geometry — the importer
    // uses a heuristic estimate (content.len() * 0.6) that doesn't match real
    // glyph metrics.
    for obj in &mut project.objects {
        if let Some(intrinsic) = intrinsic_text_bounds(&obj.data) {
            let w = intrinsic.width().max(0.0);
            let h = intrinsic.height().max(0.0);
            obj.bounds = Bounds::new(
                obj.bounds.min,
                Point2D::new(obj.bounds.min.x + w, obj.bounds.min.y + h),
            );
        }
    }

    // Collect the union of distinct layer ids actually used by the
    // imported objects. When a mixed batch auto-routes to multiple
    // sibling layers, this will contain multiple entries and the
    // frontend should prefer these over the caller's requested layer.
    let resolved_layer_ids: Vec<LayerId> = {
        let mut seen: Vec<LayerId> = Vec::new();
        for obj in &imported_objects {
            if !seen.contains(&obj.layer_id) {
                seen.push(obj.layer_id);
            }
        }
        seen
    };

    // Imported content keeps its true file dimensions (never silently
    // scaled), so detect a result larger than the workspace and surface it
    // to the user as a warning.
    let oversize_payload = {
        let bed_w = project.workspace.bed_width_mm;
        let bed_h = project.workspace.bed_height_mm;
        let mut union: Option<Bounds> = None;
        for obj in &imported_objects {
            union = Some(match union {
                None => obj.bounds,
                Some(b) => Bounds::new(
                    Point2D::new(b.min.x.min(obj.bounds.min.x), b.min.y.min(obj.bounds.min.y)),
                    Point2D::new(b.max.x.max(obj.bounds.max.x), b.max.y.max(obj.bounds.max.y)),
                ),
            });
        }
        union
            .filter(|b| b.width() > bed_w + 0.01 || b.height() > bed_h + 0.01)
            .map(|b| {
                json!({
                    "width_mm": b.width(),
                    "height_mm": b.height(),
                    "bed_width_mm": bed_w,
                    "bed_height_mm": bed_h,
                })
            })
    };

    ctx.push_project_undo_snapshot(original)
        .map_err(ServiceError::internal)?;
    *project_guard = Some(working);
    drop(project_guard);
    planning::invalidate_plan_cache(ctx)?;
    emit_routing_notices(ctx, &routing_notices);
    ctx.emit_event(
        "project.import.completed",
        json!({
            "layer_id": layer_id,
            "resolved_layer_ids": resolved_layer_ids,
            "file_count": file_count,
            "object_ids": imported_objects.iter().map(|obj| obj.id).collect::<Vec<_>>(),
            "objects": imported_objects.iter().map(events::object_summary).collect::<Vec<_>>(),
            "warnings": import_warnings,
        }),
    );
    if let Some(payload) = oversize_payload {
        ctx.emit_event("project.import.oversized", payload);
    }
    Ok(imported_objects)
}

fn vector_object_name(base_name: &str, index: usize, total: usize) -> String {
    if total <= 1 {
        base_name.to_string()
    } else {
        format!("{base_name} {}", index + 1)
    }
}

/// Normalize exact RGB tags for comparison without snapping arbitrary source
/// colors to Beam Bench's standard palette. Alpha, when present, does not
/// participate in layer matching because PDF colors are opaque RGB values.
fn normalize_import_color_tag(color_tag: &str) -> Option<String> {
    let color_tag = color_tag.trim();
    let rgb = match color_tag.len() {
        7 if color_tag.starts_with('#') => color_tag,
        9 if color_tag.starts_with('#') => &color_tag[..7],
        _ => return None,
    };
    rgb[1..]
        .chars()
        .all(|ch| ch.is_ascii_hexdigit())
        .then(|| rgb.to_ascii_uppercase())
}

fn supports_colored_vector_import(layer: &Layer) -> bool {
    !layer.is_tool_layer && RoutingTarget::NeedsNonImage.layer_matches(layer)
}

/// Reuse an existing ordinary vector layer with the same exact RGB tag, or
/// create a safe Line layer. Source colors never imply Cut, Score, or Engrave;
/// an existing matching layer is the only way to inherit configured behavior.
fn resolve_import_color_layer(
    project: &mut beambench_core::Project,
    color_tag: &str,
) -> ServiceResult<LayerId> {
    let normalized = normalize_import_color_tag(color_tag)
        .ok_or_else(|| ServiceError::invalid_input("Invalid imported vector color"))?;

    if let Some(layer) = project.layers.iter().find(|layer| {
        supports_colored_vector_import(layer)
            && normalize_import_color_tag(&layer.color_tag.0).as_deref()
                == Some(normalized.as_str())
    }) {
        return Ok(layer.id);
    }

    let mut layer = Layer::new(format!("Imported {normalized}"), OperationType::Line);
    layer.color_tag = ColorTag(normalized);
    let layer_id = layer.id;
    project.add_layer(layer);
    Ok(layer_id)
}

fn should_auto_route_import_colors(paths: &[ImportedVectorPath]) -> bool {
    let mut colors = HashSet::new();
    for path in paths {
        if let Some(color) = path
            .color_tag
            .as_deref()
            .and_then(normalize_import_color_tag)
        {
            colors.insert(color);
            if colors.len() > MAX_AUTO_COLOR_LAYERS {
                return false;
            }
        }
    }
    !colors.is_empty()
}

fn add_imported_vector_paths(
    project: &mut beambench_core::Project,
    effective_layer_id: LayerId,
    name_prefix: &str,
    paths: Vec<ImportedVectorPath>,
) -> ServiceResult<Vec<ProjectObject>> {
    let route_colors = should_auto_route_import_colors(&paths);
    let total = paths.len();
    let mut imported_objects = Vec::with_capacity(total);
    for (idx, imported_path) in paths.into_iter().enumerate() {
        let destination_layer_id = if route_colors {
            match imported_path.color_tag.as_deref() {
                Some(color_tag) => resolve_import_color_layer(project, color_tag)?,
                None => effective_layer_id,
            }
        } else {
            effective_layer_id
        };
        let obj = vecpath_to_project_object(
            destination_layer_id,
            vector_object_name(name_prefix, idx, total),
            imported_path.path,
        );
        imported_objects.push(project.add_object(obj).clone());
    }
    Ok(imported_objects)
}

fn vecpath_to_project_object(layer_id: LayerId, name: String, path: VecPath) -> ProjectObject {
    let bounds = path
        .visual_bounds()
        .unwrap_or_else(|| Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(0.0, 0.0)));
    let closed = path.subpaths.iter().any(|sp| sp.closed);
    ProjectObject::new(
        name,
        layer_id,
        bounds,
        ObjectData::VectorPath {
            path_data: path.to_svg_d(),
            closed,
            ruler_guide_axis: None,
        },
    )
}

fn import_vector_paths(
    ctx: &ServiceContext,
    layer_id: LayerId,
    file_count: usize,
    name_prefix: &str,
    paths: Vec<ImportedVectorPath>,
    warnings: Vec<String>,
) -> ServiceResult<Vec<ProjectObject>> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    if project.find_layer(layer_id).is_none() {
        return Err(ServiceError::not_found("Layer not found"));
    }

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;

    // Vector paths always land on a non-image layer — auto-route if
    // the caller target is Image.
    let allow_tool_imports = ctx
        .settings
        .lock()
        .map_err(|e| lock_err("settings", e))?
        .allow_importing_to_tool_layers;
    let (effective_layer_id, notice) = route_import_with_notice(
        project,
        layer_id,
        RoutingTarget::NeedsNonImage,
        allow_tool_imports,
    )?;
    let routing_notices: Vec<RoutingNotice> = notice.into_iter().collect();

    let imported_objects =
        add_imported_vector_paths(project, effective_layer_id, name_prefix, paths)?;

    let resolved_layer_ids: Vec<LayerId> = {
        let mut seen: Vec<LayerId> = Vec::new();
        for obj in &imported_objects {
            if !seen.contains(&obj.layer_id) {
                seen.push(obj.layer_id);
            }
        }
        seen
    };

    drop(project_guard);
    planning::invalidate_plan_cache(ctx)?;
    emit_routing_notices(ctx, &routing_notices);
    ctx.emit_event(
        "project.import.completed",
        json!({
            "layer_id": layer_id,
            "resolved_layer_ids": resolved_layer_ids,
            "file_count": file_count,
            "object_ids": imported_objects.iter().map(|obj| obj.id).collect::<Vec<_>>(),
            "objects": imported_objects.iter().map(events::object_summary).collect::<Vec<_>>(),
            "warnings": warnings,
        }),
    );
    Ok(imported_objects)
}

fn parse_vector_file(
    file_path: &str,
    format: VectorImportFormat,
) -> ServiceResult<ParsedVectorImport> {
    let bytes = read_file_bytes(file_path)?;
    parse_vector_bytes(&bytes, format)
}

fn parse_vector_bytes(
    bytes: &[u8],
    format: VectorImportFormat,
) -> ServiceResult<ParsedVectorImport> {
    // The PDF reader enforces its own input and expansion limits.
    if !matches!(format, VectorImportFormat::Pdf) {
        check_vector_import_size(bytes)?;
    }
    let parsed = parse_vector_bytes_unchecked(bytes, format)?;
    if parsed.paths.len() > IMPORT_OBJECT_LIMIT {
        return Err(ServiceError::invalid_input(format!(
            "This file has {} separate shapes, more than the {IMPORT_OBJECT_LIMIT} one import can add. Join or simplify the shapes, then import again.",
            parsed.paths.len()
        )));
    }
    Ok(parsed)
}

fn parse_vector_bytes_unchecked(
    bytes: &[u8],
    format: VectorImportFormat,
) -> ServiceResult<ParsedVectorImport> {
    match format {
        VectorImportFormat::Dxf => {
            let content = String::from_utf8_lossy(bytes);
            let report = parse_dxf_with_report(&content)
                .map_err(|e| ServiceError::invalid_input(format!("DXF import failed: {e}")))?;
            if report.entities.is_empty() {
                let message = match dxf_skipped_entities_summary(&report.skipped_entities) {
                    Some(summary) => format!(
                        "DXF import found no usable 2D vector geometry. Unsupported or malformed entities: {summary}."
                    ),
                    None => "DXF import found no usable 2D vector geometry.".to_string(),
                };
                return Err(ServiceError::invalid_input(message));
            }
            let warnings = dxf_skipped_entities_warning(&report.skipped_entities)
                .into_iter()
                .collect();
            Ok(ParsedVectorImport {
                paths: report
                    .entities
                    .into_iter()
                    .map(|entity| ImportedVectorPath::uncolored(entity.path))
                    .collect(),
                images: Vec::new(),
                warnings,
                y_up: true,
            })
        }
        VectorImportFormat::Pdf => parse_pdf_vector_import(bytes),
        VectorImportFormat::Ai
            if bytes[..bytes.len().min(1024)]
                .windows(5)
                .any(|w| w == b"%PDF-") =>
        {
            parse_pdf_vector_import(bytes)
        }
        VectorImportFormat::Ai => parse_eps_paths(bytes)
            .map(|paths| ParsedVectorImport {
                paths: paths
                    .into_iter()
                    .map(ImportedVectorPath::uncolored)
                    .collect(),
                images: Vec::new(),
                warnings: Vec::new(),
                y_up: true,
            })
            .map_err(|e| ServiceError::invalid_input(format!("AI import failed: {e}"))),
        VectorImportFormat::Eps => parse_eps_paths(bytes)
            .map(|paths| ParsedVectorImport {
                paths: paths
                    .into_iter()
                    .map(ImportedVectorPath::uncolored)
                    .collect(),
                images: Vec::new(),
                warnings: Vec::new(),
                y_up: true,
            })
            .map_err(|e| ServiceError::invalid_input(format!("EPS import failed: {e}"))),
    }
}

/// Map Y-up source coordinates onto the Y-down canvas, anchored at the bed's
/// bottom-left corner. Design exports use the same mapping, so they round-trip.
fn y_up_to_canvas(bed_height_mm: f64) -> Transform2D {
    Transform2D {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: -1.0,
        tx: 0.0,
        ty: bed_height_mm,
    }
}

fn parse_pdf_vector_import(bytes: &[u8]) -> ServiceResult<ParsedVectorImport> {
    let artwork = parse_pdf_artwork(bytes)
        .map_err(|e| ServiceError::invalid_input(format!("PDF import failed: {e}")))?;
    let canvas = artwork.page_to_canvas;
    Ok(ParsedVectorImport {
        paths: artwork
            .paths
            .into_iter()
            .map(|mut path| {
                path.path = bake_transform(&path.path, &canvas);
                ImportedVectorPath::from_pdf(path)
            })
            .collect(),
        images: artwork
            .images
            .into_iter()
            .map(|mut image| {
                image.transform = canvas.compose(&image.transform);
                image.clip = image.clip.map(|clip| bake_transform(&clip, &canvas));
                image
            })
            .collect(),
        warnings: Vec::new(),
        // The PDF parser already maps its page to the Y-down canvas.
        y_up: false,
    })
}

fn dxf_skipped_entities_warning(
    skipped: &std::collections::BTreeMap<String, usize>,
) -> Option<String> {
    let summary = dxf_skipped_entities_summary(skipped)?;
    Some(format!(
        "DXF import skipped unsupported or malformed entities: {summary}."
    ))
}

fn dxf_skipped_entities_summary(
    skipped: &std::collections::BTreeMap<String, usize>,
) -> Option<String> {
    (!skipped.is_empty()).then(|| {
        skipped
            .iter()
            .map(|(entity_type, count)| format!("{count} {entity_type}"))
            .collect::<Vec<_>>()
            .join(", ")
    })
}

pub fn import_svg_from_path(
    ctx: &ServiceContext,
    input: ImportSvgInput,
) -> ServiceResult<Vec<ProjectObject>> {
    import_pending(
        ctx,
        input.layer_id,
        vec![PendingImport::Svg {
            bytes: read_file_bytes(&input.file_path)?,
        }],
        None,
    )
}

pub fn import_image_from_path(
    ctx: &ServiceContext,
    input: ImportImageInput,
) -> ServiceResult<ProjectObject> {
    let filename = Path::new(&input.file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("image")
        .to_string();
    let mut objects = import_pending(
        ctx,
        input.layer_id,
        vec![PendingImport::Image {
            filename,
            bytes: read_file_bytes(&input.file_path)?,
            source_path: absolute_source_path(&input.file_path),
        }],
        None,
    )?;
    objects
        .pop()
        .ok_or_else(|| ServiceError::internal("Missing imported image object"))
}

pub fn import_image_from_bytes(
    ctx: &ServiceContext,
    filename: String,
    bytes: Vec<u8>,
    layer_id: LayerId,
) -> ServiceResult<ProjectObject> {
    let mut objects = import_pending(
        ctx,
        layer_id,
        vec![PendingImport::Image {
            filename,
            bytes,
            source_path: None,
        }],
        None,
    )?;
    objects
        .pop()
        .ok_or_else(|| ServiceError::internal("Missing imported image object"))
}

pub fn import_files_from_data(
    ctx: &ServiceContext,
    input: ImportFilesDataInput,
) -> ServiceResult<Vec<ProjectObject>> {
    use base64::Engine;
    let mut pending = Vec::with_capacity(input.files.len());
    for file in input.files {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(file.data_base64.trim())
            .map_err(|e| {
                ServiceError::invalid_input(format!("Invalid data for {}: {e}", file.filename))
            })?;
        pending.push(prepare_pending_import(&file.filename, bytes, None)?);
    }
    import_pending(ctx, input.layer_id, pending, input.create_layer)
}

pub fn import_files_from_paths(
    ctx: &ServiceContext,
    input: ImportFilesInput,
) -> ServiceResult<Vec<ProjectObject>> {
    import_pending(
        ctx,
        input.layer_id,
        prepare_pending_imports(input.file_paths)?,
        input.create_layer,
    )
}

pub fn import_art_library_item(
    ctx: &ServiceContext,
    layer_id: LayerId,
    item_name: &str,
    source_filename: &str,
    media_type: &str,
    bytes: Vec<u8>,
) -> ServiceResult<Vec<ProjectObject>> {
    if media_type == "image/svg+xml" {
        check_vector_import_size(&bytes)?;
    }
    let pending = match media_type {
        "image/svg+xml" => PendingImport::Svg { bytes },
        "application/dxf" => pending_vector_import(
            item_name,
            parse_vector_bytes(&bytes, VectorImportFormat::Dxf)?,
        ),
        "application/pdf" => pending_vector_import(
            item_name,
            parse_vector_bytes(&bytes, VectorImportFormat::Pdf)?,
        ),
        "application/ai" => pending_vector_import(
            item_name,
            parse_vector_bytes(&bytes, VectorImportFormat::Ai)?,
        ),
        "application/postscript" => pending_vector_import(
            item_name,
            parse_vector_bytes(&bytes, VectorImportFormat::Eps)?,
        ),
        "image/png" | "image/jpeg" | "image/gif" | "image/bmp" | "image/webp" | "image/tiff"
        | "image/x-tga" => PendingImport::Image {
            filename: source_filename.to_string(),
            bytes,
            source_path: None,
        },
        other => {
            return Err(ServiceError::invalid_input(format!(
                "Unsupported art library media type: {other}"
            )));
        }
    };

    import_pending(ctx, layer_id, vec![pending], None)
}

pub fn import_vector_file_from_path(
    ctx: &ServiceContext,
    input: ImportVectorFileInput,
) -> ServiceResult<Vec<ProjectObject>> {
    let path = Path::new(&input.file_path);
    let name_prefix = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or(match input.format {
            VectorImportFormat::Dxf => "DXF Import",
            VectorImportFormat::Pdf => "PDF Import",
            VectorImportFormat::Ai => "AI Import",
            VectorImportFormat::Eps => "EPS Import",
        });
    let parsed = parse_vector_file(&input.file_path, input.format)?;
    import_pending(
        ctx,
        input.layer_id,
        vec![pending_vector_import(name_prefix, parsed)],
        None,
    )
}

pub fn import_gcode_from_path(
    ctx: &ServiceContext,
    input: ImportGcodeInput,
) -> ServiceResult<Vec<ProjectObject>> {
    let path = Path::new(&input.file_path);
    let name_prefix = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("GCode Import");
    let content = std::fs::read_to_string(&input.file_path)
        .map_err(|e| ServiceError::persistence(format!("Failed to read G-code: {e}")))?;
    let paths: Vec<_> = beambench_core::import_gcode::import_gcode_checked(&content)
        .map_err(ServiceError::invalid_input)?
        .into_iter()
        .map(ImportedVectorPath::uncolored)
        .collect();
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    import_vector_paths(ctx, input.layer_id, 1, name_prefix, paths, Vec::new())
}

pub fn trace_raster_image(
    ctx: &ServiceContext,
    input: TraceImageInput,
) -> ServiceResult<Vec<ProjectObject>> {
    let source = load_trace_source_snapshot(ctx, input.object_id)?;
    let grayscale = decode_trace_grayscale(
        ctx,
        &source.asset_key,
        &source.image_data,
        input.trace_alpha,
    )?;
    let config = build_trace_config(&input);
    let boundary = normalize_trace_boundary(input.boundary, source.width_px, source.height_px)?;
    let (trace_source, boundary_x, boundary_y) =
        trace_image_for_boundary(grayscale.as_ref(), boundary);

    let sx = if source.width_px == 0 {
        1.0
    } else {
        source.bounds.width() / source.width_px as f64
    };
    let sy = if source.height_px == 0 {
        1.0
    } else {
        source.bounds.height() / source.height_px as f64
    };
    let pixel_to_world = Transform2D::translate(source.bounds.min.x, source.bounds.min.y)
        .compose(&Transform2D::scale(sx, sy));
    let boundary_offset = Transform2D::translate(boundary_x, boundary_y);
    let transform = pixel_to_world.compose(&boundary_offset);
    let traced_paths = trace_image(trace_source.as_ref(), &config)
        .into_iter()
        .map(|path| bake_transform(&path, &transform))
        .collect::<Vec<_>>();

    // Guard: if tracing produced no paths, return early without mutating
    // the project. This prevents delete_source from removing the raster
    // with nothing to show for it, and avoids pushing an empty undo step.
    if traced_paths.is_empty() {
        return Ok(vec![]);
    }

    let source_id = source.object_id;
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;

    // Trace output is vector content, so it must NOT land on the
    // source raster's image layer. Route to a non-image sibling layer
    // (existing or newly created with matching color_tag).
    let layer_count_before_route = project.layers.len();
    let (dest_layer_id, trace_notice) = route_with_notice(
        project,
        source.source_layer_id,
        RoutingTarget::NeedsNonImage,
    )?;
    if project.layers.len() > layer_count_before_route
        && let Some(dest_layer) = project.find_layer_mut(dest_layer_id)
    {
        dest_layer.name =
            crate::validation::strip_mode_suffix(&source.source_layer_name).to_string();
    }

    let total = traced_paths.len();
    let mut imported_objects = Vec::with_capacity(total);
    for (idx, path) in traced_paths.into_iter().enumerate() {
        let obj = vecpath_to_project_object(
            dest_layer_id,
            vector_object_name(&format!("{} Trace", source.source_name), idx, total),
            path,
        );
        let obj = project.add_object(obj).clone();
        imported_objects.push(obj);
    }

    // Delete source raster if requested (part of same undo step)
    if input.delete_source {
        project.remove_object(source_id);
        let source_layer_is_empty = !project
            .objects
            .iter()
            .any(|obj| obj.layer_id == source.source_layer_id);
        if source_layer_is_empty {
            project.remove_layer(source.source_layer_id);
        }
        if let Ok(mut cache) = ctx.trace_preview_source_cache.lock() {
            cache.remove_asset(&source.asset_key);
        }
    }

    drop(project_guard);
    planning::invalidate_plan_cache(ctx)?;
    if let Some(notice) = trace_notice {
        emit_routing_notices(ctx, std::slice::from_ref(&notice));
    }
    ctx.emit_event(
        "project.import.completed",
        json!({
            "layer_id": dest_layer_id,
            "resolved_layer_ids": vec![dest_layer_id],
            "file_count": 1,
            "source_object_id": input.object_id,
            "object_ids": imported_objects.iter().map(|obj| obj.id).collect::<Vec<_>>(),
            "objects": imported_objects.iter().map(events::object_summary).collect::<Vec<_>>(),
        }),
    );
    Ok(imported_objects)
}

/// Preview output for trace image — paths in image-pixel space, no project mutation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracePreviewOutput {
    pub paths: Vec<String>,
    pub source_width: u32,
    pub source_height: u32,
}

/// Generate a trace preview without modifying the project.
/// Returns paths in image-pixel coordinates (not world-space).
pub fn trace_image_preview(
    ctx: &ServiceContext,
    input: TraceImageInput,
) -> ServiceResult<TracePreviewOutput> {
    const TRACE_PREVIEW_MAX_SIDE_PX: u32 = 1024;

    ensure_trace_preview_request_current(ctx, input.preview_request_id)?;
    let source = load_trace_source_snapshot(ctx, input.object_id)?;
    ensure_trace_preview_request_current(ctx, input.preview_request_id)?;
    let grayscale = decode_trace_grayscale(
        ctx,
        &source.asset_key,
        &source.image_data,
        input.trace_alpha,
    )?;
    ensure_trace_preview_request_current(ctx, input.preview_request_id)?;
    let boundary = normalize_trace_boundary(input.boundary, source.width_px, source.height_px)?;
    let (trace_source, boundary_x, boundary_y) =
        trace_image_for_boundary(grayscale.as_ref(), boundary);
    let (preview_image, preview_width, preview_height) =
        clamp_trace_preview_image(trace_source.as_ref(), TRACE_PREVIEW_MAX_SIDE_PX);
    let config = build_preview_trace_config(&input);

    let mut traced_paths = trace_image_preview_fast_cancellable(&preview_image, &config, || {
        trace_preview_should_cancel(ctx, input.preview_request_id)
    })
    .ok_or_else(|| {
        ServiceError::stale_revision("Trace preview request superseded by a newer request")
    })?;
    ensure_trace_preview_request_current(ctx, input.preview_request_id)?;
    let trace_width = trace_source.width();
    let trace_height = trace_source.height();
    if preview_width != trace_width
        || preview_height != trace_height
        || boundary_x != 0.0
        || boundary_y != 0.0
    {
        let scale_x = if preview_width == 0 {
            1.0
        } else {
            trace_width as f64 / preview_width as f64
        };
        let scale_y = if preview_height == 0 {
            1.0
        } else {
            trace_height as f64 / preview_height as f64
        };
        let transform = Transform2D::translate(boundary_x, boundary_y)
            .compose(&Transform2D::scale(scale_x, scale_y));
        traced_paths = traced_paths
            .into_iter()
            .map(|path| bake_transform(&path, &transform))
            .collect();
    }
    let paths = traced_paths.iter().map(|p| p.to_svg_d()).collect();

    Ok(TracePreviewOutput {
        paths,
        source_width: source.width_px,
        source_height: source.height_px,
    })
}

// ---------------------------------------------------------------------------
// Adjust Image Preview
// ---------------------------------------------------------------------------

/// Input for adjust image preview — contains both per-object adjustments
/// and layer settings that affect the processed output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdjustImagePreviewInput {
    pub object_id: ObjectId,
    /// Frontend-owned monotonic id. `None` keeps non-interactive callers compatible.
    pub preview_request_id: Option<u64>,
    /// Render a bounded crop at the job's planned resolution instead of a
    /// downscaled whole-image preview.
    #[serde(default)]
    pub pixel_sample: bool,
    // Per-object adjustments
    pub brightness: f64,
    pub contrast: f64,
    pub gamma: f64,
    pub invert: bool,
    pub threshold: u8,
    pub saturation: f64,
    pub sharpen: f64,
    pub edge_enhance: bool,
    pub enhance_radius: f64,
    pub enhance_amount: f64,
    pub enhance_denoise: f64,
    // Layer settings
    pub mode: String,
    pub dpi: u32,
    pub negative: bool,
    pub pass_through: bool,
    pub halftone_cells_per_inch: u32,
    pub halftone_angle_deg: f64,
    pub newsprint_angle_deg: f64,
    pub newsprint_frequency: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdjustImagePreviewOutput {
    pub png_base64: String,
    pub width: u32,
    pub height: u32,
}

fn ensure_adjust_preview_request_current(
    ctx: &ServiceContext,
    request_id: Option<u64>,
) -> ServiceResult<()> {
    let Some(request_id) = request_id else {
        return Ok(());
    };
    if ctx.latest_adjust_preview_request_id.load(Ordering::Acquire) == request_id {
        return Ok(());
    }
    Err(ServiceError::stale_revision(
        "Image adjustment preview superseded by a newer request",
    ))
}

/// Suggested slider values from `auto_adjust_image`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoAdjustOutput {
    pub brightness: f64,
    pub contrast: f64,
    pub gamma: f64,
    pub sharpen: f64,
}

/// Compute suggested brightness/contrast/gamma/sharpen values for a raster
/// object from its image statistics. Read-only — the dialog applies the
/// returned values to its sliders, nothing is committed here.
pub fn auto_adjust_image(
    ctx: &ServiceContext,
    object_id: ObjectId,
) -> ServiceResult<AutoAdjustOutput> {
    let project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_ref()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    let source = project
        .find_object(object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;

    let asset_key = match &source.data {
        ObjectData::RasterImage { asset_key, .. } => asset_key.clone(),
        _ => return Err(ServiceError::invalid_input("Object is not a raster image")),
    };

    let asset_id = AssetId::from_uuid(
        Uuid::parse_str(&asset_key)
            .map_err(|_| ServiceError::invalid_input("Invalid image asset reference"))?,
    );
    let image_data = project
        .asset_data
        .get(&asset_id)
        .cloned()
        .ok_or_else(|| ServiceError::not_found("Image asset data not found"))?;

    drop(project_guard);

    let decoded = image::load_from_memory(&image_data)
        .map_err(|e| ServiceError::invalid_input(format!("Failed to decode image: {e}")))?;
    let auto = beambench_raster::adjust::suggest_auto_adjustments(&decoded.to_luma8());

    Ok(AutoAdjustOutput {
        brightness: auto.brightness,
        contrast: auto.contrast,
        gamma: auto.gamma,
        sharpen: auto.sharpen,
    })
}

/// Process a raster image with given adjustments and layer settings,
/// returning PNG bytes (base64) for live preview. Read-only — does NOT
/// modify the project.
pub fn adjust_image_preview(
    ctx: &ServiceContext,
    input: AdjustImagePreviewInput,
) -> ServiceResult<AdjustImagePreviewOutput> {
    use base64::Engine;
    use beambench_common::{RasterAdjustments, RasterMode};
    use beambench_raster::RasterProcessingParams;
    use image::{ImageEncoder, codecs::png::PngEncoder};

    ensure_adjust_preview_request_current(ctx, input.preview_request_id)?;

    // 1. Lock project read-only, find source raster object
    let project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_ref()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    let source = project
        .find_object(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;

    let asset_key = match &source.data {
        ObjectData::RasterImage { asset_key, .. } => asset_key.clone(),
        _ => return Err(ServiceError::invalid_input("Object is not a raster image")),
    };

    let bounds = source.bounds;

    // 2. Load image bytes from asset store
    let asset_id = AssetId::from_uuid(
        Uuid::parse_str(&asset_key)
            .map_err(|_| ServiceError::invalid_input("Invalid image asset reference"))?,
    );
    let image_data = project
        .asset_data
        .get(&asset_id)
        .cloned()
        .ok_or_else(|| ServiceError::not_found("Image asset data not found"))?;

    drop(project_guard);
    ensure_adjust_preview_request_current(ctx, input.preview_request_id)?;

    // 3. Build RasterProcessingParams from input overrides
    let adjustments = RasterAdjustments {
        brightness: input.brightness,
        contrast: input.contrast,
        gamma: input.gamma,
        invert: input.invert,
        threshold: input.threshold,
        saturation: input.saturation,
        sharpen: input.sharpen,
        edge_enhance: input.edge_enhance,
        enhance_radius: input.enhance_radius,
        enhance_amount: input.enhance_amount,
        enhance_denoise: input.enhance_denoise,
    };

    let mode: RasterMode = serde_json::from_value(serde_json::Value::String(input.mode.clone()))
        .unwrap_or(RasterMode::FloydSteinberg);

    // Whole-image previews are capped for responsiveness. The optional pixel
    // sample instead crops the source before processing so it can use the
    // planned DPI without allocating the full job-sized bitmap.
    let (bw, bh) = (bounds.width(), bounds.height());
    let full_w = (bw / 25.4 * input.dpi as f64).round().max(1.0) as u32;
    let full_h = (bh / 25.4 * input.dpi as f64).round().max(1.0) as u32;
    let max_preview_px = 500.0;
    let mut preview_source = image_data.as_ref().clone();
    let mut preview_bounds = (bw, bh);
    let preview_dpi = if input.pixel_sample {
        const SAMPLE_EDGE_PX: u32 = 512;
        let decoded = image::load_from_memory(&preview_source)
            .map_err(|e| ServiceError::invalid_input(format!("Failed to decode image: {e}")))?;
        let planned_w = if input.pass_through {
            decoded.width().max(1)
        } else {
            full_w
        };
        let planned_h = if input.pass_through {
            decoded.height().max(1)
        } else {
            full_h
        };
        let sample_w = planned_w.min(SAMPLE_EDGE_PX);
        let sample_h = planned_h.min(SAMPLE_EDGE_PX);

        if sample_w < planned_w || sample_h < planned_h {
            let source_w = decoded.width().max(1);
            let source_h = decoded.height().max(1);
            let crop_w =
                ((u64::from(source_w) * u64::from(sample_w)).div_ceil(u64::from(planned_w)) as u32)
                    .clamp(1, source_w);
            let crop_h =
                ((u64::from(source_h) * u64::from(sample_h)).div_ceil(u64::from(planned_h)) as u32)
                    .clamp(1, source_h);
            let crop_x = (source_w - crop_w) / 2;
            let crop_y = (source_h - crop_h) / 2;
            let cropped = decoded.crop_imm(crop_x, crop_y, crop_w, crop_h);
            let mut cropped_png = Vec::new();
            cropped
                .write_to(
                    &mut std::io::Cursor::new(&mut cropped_png),
                    image::ImageFormat::Png,
                )
                .map_err(|e| ServiceError::internal(format!("Sample crop encode failed: {e}")))?;
            preview_source = cropped_png;
            preview_bounds = (
                bw * f64::from(sample_w) / f64::from(planned_w),
                bh * f64::from(sample_h) / f64::from(planned_h),
            );
        }
        input.dpi
    } else if f64::from(full_w) > max_preview_px || f64::from(full_h) > max_preview_px {
        let scale_down = max_preview_px / f64::from(full_w.max(full_h));
        ((input.dpi as f64 * scale_down).round() as u32).max(1)
    } else {
        input.dpi
    };

    let params = RasterProcessingParams {
        source_bytes: preview_source,
        bounds_mm: preview_bounds,
        dpi: preview_dpi,
        mode,
        adjustments,
        pass_through: input.pass_through,
        halftone_cells_per_inch: input.halftone_cells_per_inch,
        halftone_angle_deg: input.halftone_angle_deg,
        newsprint_angle_deg: input.newsprint_angle_deg,
        newsprint_frequency: input.newsprint_frequency,
        invert: input.negative,
    };

    // 4. Process the raster with two-tier caching:
    //    - preview_cache: full processed result (separate from planner cache
    //      to avoid evicting planner entries with transient slider settings)
    //    - scaled_image_cache: staged decode+scale (reused across slider drags)
    let cache_key = params.cache_key();
    let processed_arc = if let Some(cached) = ctx.preview_cache.get(&cache_key) {
        cached
    } else {
        let result =
            beambench_raster::process_raster_with_cache(params, Some(&ctx.scaled_image_cache))
                .map_err(|e| ServiceError::internal(e.to_string()))?;
        ensure_adjust_preview_request_current(ctx, input.preview_request_id)?;
        let arc = std::sync::Arc::new(result);
        ctx.preview_cache.insert(cache_key, arc.clone());
        arc
    };
    ensure_adjust_preview_request_current(ctx, input.preview_request_id)?;
    let processed = &*processed_arc;

    let pw = processed.width_px;
    let ph = processed.height_px;

    // 5. Convert to displayable grayscale pixels
    let gray_pixels: Vec<u8> = match processed.format {
        beambench_raster::types::RasterPixelFormat::Binary => {
            let row_bytes = (pw as usize).div_ceil(8);
            let mut pixels = Vec::with_capacity((pw * ph) as usize);
            for y in 0..ph {
                for x in 0..pw {
                    let byte_idx = y as usize * row_bytes + (x as usize / 8);
                    let bit_idx = 7 - (x as usize % 8);
                    let is_burn = byte_idx < processed.data.len()
                        && (processed.data[byte_idx] & (1 << bit_idx)) == 0;
                    pixels.push(if is_burn { 0 } else { 255 });
                }
            }
            pixels
        }
        beambench_raster::types::RasterPixelFormat::Grayscale8 => processed.data.clone(),
    };
    ensure_adjust_preview_request_current(ctx, input.preview_request_id)?;

    // 6. PNG-encode
    let mut png_buf = Vec::new();
    PngEncoder::new(std::io::Cursor::new(&mut png_buf))
        .write_image(&gray_pixels, pw, ph, image::ExtendedColorType::L8)
        .map_err(|e| ServiceError::internal(format!("PNG encode failed: {e}")))?;
    ensure_adjust_preview_request_current(ctx, input.preview_request_id)?;

    // 7. Base64 encode and return
    let png_base64 = base64::engine::general_purpose::STANDARD.encode(&png_buf);

    Ok(AdjustImagePreviewOutput {
        png_base64,
        width: pw,
        height: ph,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::{Bounds, ColorTag, Id, Point2D};
    use beambench_core::{
        ObjectData, OperationType, Project, ProjectObject, ShapeKind, export_pdf,
    };
    use beambench_planner::{ExecutionPlan, build_plan};
    use tempfile::tempdir;

    fn sample_svg() -> &'static [u8] {
        br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect x="0" y="0" width="20" height="10"/></svg>"#
    }

    fn sample_svg_with_embedded_png() -> Vec<u8> {
        use base64::Engine;

        let mut png = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(std::io::Cursor::new(&mut png));
        image::ImageEncoder::write_image(
            encoder,
            &[0, 64, 128, 255],
            2,
            2,
            image::ExtendedColorType::L8,
        )
        .unwrap();
        let encoded = base64::engine::general_purpose::STANDARD.encode(png);
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50">
                <rect x="5" y="5" width="20" height="20" fill="none" stroke="black"/>
                <image x="40" y="10" width="30" height="30" href="data:image/png;base64,{encoded}"/>
            </svg>"#
        )
        .into_bytes()
    }

    fn sample_lbrn_project() -> Vec<u8> {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<LBRN_PROJECT_ROOT AppVersion="1.6.03" FormatVersion="1" MaterialHeight="3">
  <CutSetting type="Cut"><index Value="1"/><name Value="C01"/><minPower Value="5"/><maxPower Value="20"/><speed Value="100"/><priority Value="0"/></CutSetting>
  <CutSetting type="Cut"><index Value="2"/><name Value="C02"/><maxPower Value="35"/><speed Value="50"/><priority Value="1"/></CutSetting>
  <Shape Type="Group"><XForm>1 0 0 1 10 20</XForm><Children>
    <Shape Type="Rect" CutIndex="1" W="40" H="20" Cr="2"><XForm>1 0 0 1 30 40</XForm></Shape>
    <Shape Type="Ellipse" CutIndex="2" Rx="10" Ry="5"><XForm>1 0 0 1 80 40</XForm></Shape>
  </Children></Shape>
  <Shape Type="Text" CutIndex="1" Font="Arial,-1,100,5,50,0,0,0,0,0" Str="Beam Bench" H="12" LS="0" LnS="0" Ah="1" Av="1" Weld="1"><XForm>1 0 0 1 100 120</XForm></Shape>
  <Notes ShowOnLoad="0" Notes="Imported notes"/>
</LBRN_PROJECT_ROOT>"#
            .replace(
                "LBRN_PROJECT_ROOT",
                concat!("Light", "BurnProject"),
            )
            .into_bytes()
    }

    fn sample_plan(project: &beambench_core::Project) -> ExecutionPlan {
        let mut project = project.clone();
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "Rect",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));
        build_plan(&project).unwrap()
    }

    fn sample_pdf_bytes() -> Vec<u8> {
        let mut project = Project::new("PDF Import");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "Rect",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(20.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 20.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));
        export_pdf(&project, false, &[]).unwrap()
    }

    fn sample_colored_pdf_bytes() -> Vec<u8> {
        let mut project = Project::new("Colored PDF Import");
        project.layers.clear();

        let mut red = Layer::new("Red", OperationType::Line);
        red.color_tag = ColorTag("#FF0000".to_string());
        let red_id = red.id;
        project.add_layer(red);

        let mut blue = Layer::new("Blue", OperationType::Line);
        blue.color_tag = ColorTag("#0000FF".to_string());
        let blue_id = blue.id;
        project.add_layer(blue);

        for (name, layer_id, x) in [("Red path", red_id, 0.0), ("Blue path", blue_id, 30.0)] {
            project.add_object(ProjectObject::new(
                name,
                layer_id,
                Bounds::new(Point2D::new(x, 0.0), Point2D::new(x + 20.0, 10.0)),
                ObjectData::Shape {
                    kind: ShapeKind::Rectangle,
                    width: 20.0,
                    height: 10.0,
                    corner_radius: 0.0,
                },
            ));
        }

        export_pdf(&project, false, &[]).unwrap()
    }

    fn colored_pdf_import_context() -> (ServiceContext, LayerId, LayerId) {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Colored PDF Target");
        project.layers.clear();

        let mut requested = Layer::new("Default Line", OperationType::Line);
        requested.color_tag = ColorTag("#000000".to_string());
        let requested_id = requested.id;
        project.add_layer(requested);

        let mut configured_red = Layer::new("Configured Red Cut", OperationType::Cut);
        configured_red.color_tag = ColorTag("#ff0000ff".to_string());
        configured_red.primary_entry_mut().speed_mm_min = 777.0;
        configured_red.primary_entry_mut().power_percent = 42.0;
        let configured_red_id = configured_red.id;
        project.add_layer(configured_red);

        let plan = sample_plan(&project);
        *ctx.project.lock().unwrap() = Some(project);
        *ctx.plan_cache.lock().unwrap() = Some((plan).into());
        (ctx, requested_id, configured_red_id)
    }

    fn sample_postscript_bytes() -> &'static [u8] {
        b"%!PS-Adobe-3.0\n10 20 moveto\n30 40 lineto\nclosepath\nstroke\n"
    }

    fn art_library_import_context() -> (ServiceContext, LayerId) {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Art Library Import");
        let layer_id = project.ensure_default_layer();
        let plan = sample_plan(&project);
        *ctx.project.lock().unwrap() = Some(project);
        *ctx.plan_cache.lock().unwrap() = Some((plan).into());
        (ctx, layer_id)
    }

    fn assert_art_library_vector_import_completed(
        ctx: &ServiceContext,
        layer_id: LayerId,
        objects: &[ProjectObject],
    ) {
        assert!(!objects.is_empty());
        assert!(objects.iter().all(|object| object.layer_id == layer_id));
        assert!(
            objects
                .iter()
                .all(|object| matches!(&object.data, ObjectData::VectorPath { .. }))
        );
        assert!(ctx.undo_state().unwrap().can_undo);
        assert!(ctx.plan_cache.lock().unwrap().is_none());
    }

    #[test]
    fn route_import_rehomes_tool_layer_imports_when_preference_is_false() {
        let mut project = beambench_core::Project::new("Tool Import");
        project.layers.clear();

        let mut tool_layer = Layer::new("T1", OperationType::Tool);
        tool_layer.color_tag = ColorTag("#DA0B3F".to_string());
        tool_layer.canonicalize_tool_layer();
        let tool_id = tool_layer.id;

        let mut line_layer = Layer::new("Line", OperationType::Line);
        line_layer.color_tag = ColorTag("#000000".to_string());
        let line_id = line_layer.id;

        project.add_layer(tool_layer);
        project.add_layer(line_layer);

        let (resolved, notice) =
            route_import_with_notice(&mut project, tool_id, RoutingTarget::NeedsNonImage, false)
                .unwrap();

        assert_eq!(resolved, line_id);
        assert!(notice.is_some());
    }

    #[test]
    fn route_import_keeps_tool_layer_imports_when_preference_is_true() {
        let mut project = beambench_core::Project::new("Tool Import");
        project.layers.clear();

        let mut tool_layer = Layer::new("T1", OperationType::Tool);
        tool_layer.color_tag = ColorTag("#DA0B3F".to_string());
        tool_layer.canonicalize_tool_layer();
        let tool_id = tool_layer.id;
        project.add_layer(tool_layer);

        let (resolved, notice) =
            route_import_with_notice(&mut project, tool_id, RoutingTarget::NeedsImage, true)
                .unwrap();

        assert_eq!(resolved, tool_id);
        assert!(notice.is_none());
        assert_eq!(project.layers.len(), 1);
    }

    #[test]
    fn import_requires_valid_layer() {
        let ctx = ServiceContext::new();
        *ctx.project.lock().unwrap() = Some(beambench_core::Project::new("Import"));
        let dir = tempdir().unwrap();
        let svg_path = dir.path().join("demo.svg");
        std::fs::write(&svg_path, sample_svg()).unwrap();

        let err = import_svg_from_path(
            &ctx,
            ImportSvgInput {
                file_path: svg_path.to_string_lossy().to_string(),
                layer_id: Id::new(),
            },
        )
        .unwrap_err();
        assert_eq!(err.code, crate::error::ServiceErrorCode::NotFound);
    }

    #[test]
    fn mixed_svg_routes_embedded_raster_to_image_layer() {
        let ctx = ServiceContext::new();
        let mut project = beambench_core::Project::new("Mixed SVG Import");
        let line_layer_id = project.ensure_default_layer();
        *ctx.project.lock().unwrap() = Some(project);

        let dir = tempdir().unwrap();
        let svg_path = dir.path().join("mixed.svg");
        std::fs::write(&svg_path, sample_svg_with_embedded_png()).unwrap();

        let objects = import_svg_from_path(
            &ctx,
            ImportSvgInput {
                file_path: svg_path.to_string_lossy().to_string(),
                layer_id: line_layer_id,
            },
        )
        .unwrap();

        assert_eq!(objects.len(), 2);
        let vector = objects
            .iter()
            .find(|object| matches!(object.data, ObjectData::VectorPath { .. }))
            .expect("vector portion");
        assert_eq!(vector.layer_id, line_layer_id);

        let raster = objects
            .iter()
            .find(|object| matches!(object.data, ObjectData::RasterImage { .. }))
            .expect("embedded raster portion");
        assert_ne!(raster.layer_id, line_layer_id);

        let project_guard = ctx.project.lock().unwrap();
        let imported_project = project_guard.as_ref().unwrap();
        assert_eq!(imported_project.assets.len(), 1);
        let raster_layer = imported_project.find_layer(raster.layer_id).unwrap();
        assert_eq!(raster_layer.primary_entry().operation, OperationType::Image);
    }

    #[test]
    fn multi_file_import_uses_single_undo_snapshot_and_invalidates_plan() {
        let ctx = ServiceContext::new();
        let mut project = beambench_core::Project::new("Import");
        let layer_id = project.ensure_default_layer();
        let plan = sample_plan(&project);
        *ctx.project.lock().unwrap() = Some(project);
        *ctx.plan_cache.lock().unwrap() = Some((plan).into());

        let dir = tempdir().unwrap();
        let svg_path = dir.path().join("demo.svg");
        let svg_path_two = dir.path().join("demo-two.svg");
        std::fs::write(&svg_path, sample_svg()).unwrap();
        std::fs::write(&svg_path_two, sample_svg()).unwrap();

        let objects = import_files_from_paths(
            &ctx,
            ImportFilesInput {
                create_layer: None,
                file_paths: vec![
                    svg_path.to_string_lossy().to_string(),
                    svg_path_two.to_string_lossy().to_string(),
                ],
                layer_id,
            },
        )
        .unwrap();

        assert_eq!(objects.len(), 2);
        assert!(ctx.undo_state().unwrap().can_undo);
        assert!(ctx.plan_cache.lock().unwrap().is_none());
    }

    fn new_import_layer(operation: OperationType) -> super::super::project::AddObjectLayerInput {
        super::super::project::AddObjectLayerInput {
            name: "Imported".into(),
            operation,
            color_tag: Some("#FF0000".into()),
            entry_patch: Some(beambench_core::CutEntryPatch {
                speed_mm_min: Some(777.0),
                power_percent: Some(42.0),
                air_assist: Some(true),
                ..Default::default()
            }),
        }
    }

    #[test]
    fn failed_batch_preserves_project_assets_history_and_plan_for_paths_and_data() {
        use base64::Engine;
        for by_path in [true, false] {
            for fail_early in [true, false] {
                let (ctx, layer_id) = art_library_import_context();
                // Leave an existing redo entry to catch accidental history changes.
                import_pending(
                    &ctx,
                    layer_id,
                    vec![PendingImport::Svg {
                        bytes: sample_svg().to_vec(),
                    }],
                    None,
                )
                .unwrap();
                super::super::project::undo_project(&ctx).unwrap();
                let before = ctx.project.lock().unwrap().as_ref().unwrap().clone();
                let plan = std::sync::Arc::new(sample_plan(&before));
                *ctx.plan_cache.lock().unwrap() = Some(plan.clone());
                let history = ctx.undo_state().unwrap();
                assert!(history.can_redo);
                let mixed_svg = sample_svg_with_embedded_png();
                // PDF parsing fails before staging; bad PNG fails after SVG
                // paths, embedded image bytes, and sibling layers were staged.
                let bad_name = if fail_early { "bad.pdf" } else { "bad.png" };
                let files = [
                    ("mixed.svg", mixed_svg.as_slice()),
                    (bad_name, b"invalid".as_slice()),
                ];
                for _ in 0..3 {
                    let result = if by_path {
                        let dir = tempdir().unwrap();
                        let paths = files
                            .iter()
                            .map(|(name, bytes)| {
                                let path = dir.path().join(name);
                                std::fs::write(&path, bytes).unwrap();
                                path.to_string_lossy().to_string()
                            })
                            .collect();
                        import_files_from_paths(
                            &ctx,
                            ImportFilesInput {
                                file_paths: paths,
                                layer_id,
                                create_layer: Some(new_import_layer(OperationType::Image)),
                            },
                        )
                    } else {
                        import_files_from_data(
                            &ctx,
                            ImportFilesDataInput {
                                files: files
                                    .iter()
                                    .map(|(name, bytes)| ImportFileData {
                                        filename: (*name).into(),
                                        data_base64: base64::engine::general_purpose::STANDARD
                                            .encode(bytes),
                                    })
                                    .collect(),
                                layer_id,
                                create_layer: Some(new_import_layer(OperationType::Image)),
                            },
                        )
                    };
                    assert!(result.is_err());
                    assert_eq!(ctx.project.lock().unwrap().as_ref().unwrap(), &before);
                    assert_eq!(ctx.undo_state().unwrap().can_undo, history.can_undo);
                    assert_eq!(ctx.undo_state().unwrap().can_redo, history.can_redo);
                    assert!(std::sync::Arc::ptr_eq(
                        ctx.plan_cache.lock().unwrap().as_ref().unwrap(),
                        &plan
                    ));
                }
                // The earlier successful edit can still be redone after failures.
                assert_eq!(
                    super::super::project::redo_project(&ctx)
                        .unwrap()
                        .objects
                        .len(),
                    1
                );
            }
        }
    }

    #[test]
    fn mixed_pdf_entry_points_preserve_size_pixels_layers_planning_and_undo() {
        use base64::Engine;
        use beambench_core::WorkspaceOrigin;
        use beambench_planner::PlanSegment;
        const MM: f64 = 25.4 / 72.0;
        let bytes = include_bytes!("../../../beambench-core/tests/fixtures/pdf/mixed-modern.pdf");
        for origin in [WorkspaceOrigin::TopLeft, WorkspaceOrigin::BottomLeft] {
            for entry_point in 0..3 {
                let ctx = ServiceContext::new();
                let mut project = Project::new("Mixed PDF");
                project.workspace.origin = origin;
                project.workspace.bed_height_mm = 100.0;
                project.workspace.bed_width_mm = 100.0;
                let layer_id = project.ensure_default_layer();
                let before = project.clone();
                *ctx.project.lock().unwrap() = Some(project);
                let directory = tempdir().unwrap();
                let file = directory.path().join("mixed.pdf");
                std::fs::write(&file, bytes).unwrap();
                let objects = match entry_point {
                    0 => import_vector_file_from_path(
                        &ctx,
                        ImportVectorFileInput {
                            file_path: file.to_string_lossy().into_owned(),
                            layer_id,
                            format: VectorImportFormat::Pdf,
                        },
                    ),
                    1 => import_files_from_data(
                        &ctx,
                        ImportFilesDataInput {
                            files: vec![ImportFileData {
                                filename: "mixed.ai".into(),
                                data_base64: base64::engine::general_purpose::STANDARD
                                    .encode(bytes),
                            }],
                            layer_id,
                            create_layer: None,
                        },
                    ),
                    _ => import_art_library_item(
                        &ctx,
                        layer_id,
                        "Mixed PDF",
                        "mixed.pdf",
                        "application/pdf",
                        bytes.to_vec(),
                    ),
                }
                .unwrap();
                let after = ctx.project.lock().unwrap().as_ref().unwrap().clone();
                let images: Vec<_> = objects
                    .iter()
                    .filter(|o| matches!(o.data, ObjectData::RasterImage { .. }))
                    .collect();
                assert_eq!(images.len(), 3);
                assert_eq!(after.assets.len(), 3);
                for image in &images {
                    assert_eq!(
                        after
                            .find_layer(image.layer_id)
                            .unwrap()
                            .primary_entry()
                            .operation,
                        OperationType::Image
                    );
                }
                assert!((images[0].bounds.width() - 32.0 * MM).abs() < 1e-6);
                assert!((images[0].bounds.height() - 24.0 * MM).abs() < 1e-6);
                assert!((images[0].bounds.min.x - 58.0 * MM).abs() < 1e-6);
                assert!((images[0].bounds.min.y - 10.0 * MM).abs() < 1e-6);
                // Independently derive the source pixel centers from the PDF
                // fixture matrices, then probe the actual machine scanlines.
                // Threshold makes geometry probes deterministic even where
                // resizing the tiny source blends adjacent black/white pixels.
                // Error-diffusion modes can legitimately leave a probe unburned.
                let mut planning_project = after.clone();
                for layer in &mut planning_project.layers {
                    if let Some(settings) = layer.primary_entry_mut().raster_settings.as_mut() {
                        settings.mode = beambench_common::RasterMode::Threshold;
                    }
                }
                let plan = build_plan(&planning_project).unwrap();
                assert!(plan.failed_entries.is_empty());
                let rows: Vec<_> = plan
                    .segments
                    .iter()
                    .filter_map(|s| match s {
                        PlanSegment::Raster { scanlines, .. } => Some(scanlines),
                        _ => None,
                    })
                    .flatten()
                    .collect();
                assert!(!rows.is_empty());
                for (a, c, d, tx, ty) in [
                    (32.0, 0.0, 24.0, 58.0, 66.0),
                    (-20.0, 8.0, 18.0, 76.0, 38.0),
                ] {
                    for y in 0..3 {
                        for x in 0..4 {
                            let u = (x as f64 + 0.5) / 4.0;
                            let v = 1.0 - (y as f64 + 0.5) / 3.0;
                            let wx = (a * u + c * v + tx) * MM;
                            let canvas_y = (100.0 - (d * v + ty)) * MM;
                            let wy = if origin == WorkspaceOrigin::BottomLeft {
                                100.0 - canvas_y
                            } else {
                                canvas_y
                            };
                            let burns = rows.iter().any(|row| {
                                (row.y_mm - wy).abs() < 0.11
                                    && row
                                        .runs
                                        .iter()
                                        .any(|run| wx > run.start_x_mm && wx < run.end_x_mm)
                            });
                            assert_eq!(
                                burns,
                                x < 3 - y,
                                "origin={origin:?} entry={entry_point} transform={a},{c},{d} pixel={x},{y} machine={wx},{wy}"
                            );
                        }
                    }
                }
                let ObjectData::RasterImage { masks, .. } = &images[2].data else {
                    unreachable!()
                };
                assert_eq!(masks.len(), 1);
                let mask = after.find_object(masks[0].object_id).unwrap();
                assert!(after.find_layer(mask.layer_id).unwrap().is_tool_layer);
                assert!(objects.iter().any(|o| matches!(&o.data,ObjectData::Group {children} if children.contains(&images[2].id) && children.contains(&mask.id))));
                for (x, y, expected) in [
                    (9.0, 47.0, true),
                    (9.0, 49.0, false),
                    (7.0, 47.0, false),
                    (13.0, 43.0, false),
                    (11.0, 43.0, true),
                    (9.0, 40.0, true),
                    (9.0, 38.5, false),
                    (19.0, 47.0, true),
                    (22.0, 47.0, false),
                ] {
                    let wx = x * MM;
                    let canvas_y = (100.0 - y) * MM;
                    let wy = if origin == WorkspaceOrigin::BottomLeft {
                        100.0 - canvas_y
                    } else {
                        canvas_y
                    };
                    let burns = rows.iter().any(|row| {
                        (row.y_mm - wy).abs() < 0.06
                            && row
                                .runs
                                .iter()
                                .any(|run| wx > run.start_x_mm && wx < run.end_x_mm)
                    });
                    assert_eq!(
                        burns, expected,
                        "subpixel clip origin={origin:?} entry={entry_point} PDF={x},{y}"
                    );
                }
                let mut expected_undo = before;
                expected_undo.dirty = true;
                assert_eq!(
                    super::super::project::undo_project(&ctx).unwrap(),
                    expected_undo
                );
                assert!(!ctx.undo_state().unwrap().can_undo);
                assert_eq!(super::super::project::redo_project(&ctx).unwrap(), after);
            }
        }
    }

    #[test]
    fn image_only_pdf_does_not_create_an_empty_vector_layer() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Image-only PDF");
        project.layers.clear();
        let layer_id = project
            .add_layer(Layer::new("Image", OperationType::Image))
            .id;
        *ctx.project.lock().unwrap() = Some(project);
        let bytes = include_bytes!("../../../beambench-core/tests/fixtures/pdf/mixed-classic.pdf");
        let mut parsed = parse_pdf_vector_import(bytes).unwrap();
        parsed.paths.clear();
        parsed.images.truncate(1);
        let objects = import_pending(
            &ctx,
            layer_id,
            vec![pending_vector_import("image", parsed)],
            None,
        )
        .unwrap();
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0].layer_id, layer_id);
        assert_eq!(
            ctx.project.lock().unwrap().as_ref().unwrap().layers.len(),
            1
        );
    }

    #[test]
    fn mixed_pdf_image_failure_rolls_back_vectors_assets_and_layers() {
        let ctx = ServiceContext::new();
        let mut before = Project::new("PDF rollback");
        let layer_id = before.ensure_default_layer();
        *ctx.project.lock().unwrap() = Some(before.clone());
        let bytes = include_bytes!("../../../beambench-core/tests/fixtures/pdf/mixed-classic.pdf");
        let mut parsed = parse_pdf_vector_import(bytes).unwrap();
        // Fail after at least one vector and image have been staged.
        parsed.images[1].png = vec![0, 1, 2];
        assert!(
            import_pending(
                &ctx,
                layer_id,
                vec![pending_vector_import("mixed", parsed)],
                None
            )
            .is_err()
        );
        assert_eq!(ctx.project.lock().unwrap().as_ref().unwrap(), &before);
        assert!(!ctx.undo_state().unwrap().can_undo);
    }
    #[test]
    fn new_import_layer_and_mixed_artwork_commit_as_one_undoable_edit() {
        use base64::Engine;
        let ctx = ServiceContext::new();
        let mut before = Project::new("Empty import target");
        before.layers.clear();
        *ctx.project.lock().unwrap() = Some(before.clone());
        let imported = import_files_from_data(
            &ctx,
            ImportFilesDataInput {
                files: vec![ImportFileData {
                    filename: "mixed.svg".into(),
                    data_base64: base64::engine::general_purpose::STANDARD
                        .encode(sample_svg_with_embedded_png()),
                }],
                layer_id: Id::from_uuid(uuid::Uuid::nil()),
                create_layer: Some(new_import_layer(OperationType::Image)),
            },
        )
        .unwrap();
        assert_eq!(imported.len(), 2);
        let after = ctx.project.lock().unwrap().as_ref().unwrap().clone();
        assert_eq!(after.layers.len(), 2);
        assert_eq!(after.assets.len(), 1);
        let image_layer = after
            .layers
            .iter()
            .find(|layer| layer.primary_entry().operation == OperationType::Image)
            .unwrap();
        assert_eq!(image_layer.color_tag.0, "#FF0000");
        assert_eq!(image_layer.primary_entry().speed_mm_min, 777.0);
        assert_eq!(image_layer.primary_entry().power_percent, 42.0);
        assert!(image_layer.primary_entry().air_assist);
        // Undo marks restored projects dirty until they are saved again.
        let mut expected_undo = before;
        expected_undo.dirty = true;
        assert_eq!(
            super::super::project::undo_project(&ctx).unwrap(),
            expected_undo
        );
        assert!(!ctx.undo_state().unwrap().can_undo);
        assert_eq!(super::super::project::redo_project(&ctx).unwrap(), after);
    }

    #[test]
    fn empty_import_does_not_create_a_layer_or_undo_entry() {
        let ctx = ServiceContext::new();
        let before = Project::new("Empty import");
        *ctx.project.lock().unwrap() = Some(before.clone());
        let imported = import_pending(
            &ctx,
            Id::from_uuid(uuid::Uuid::nil()),
            vec![PendingImport::Svg {
                bytes: br#"<svg xmlns="http://www.w3.org/2000/svg"/>"#.to_vec(),
            }],
            Some(new_import_layer(OperationType::Line)),
        )
        .unwrap();
        assert!(imported.is_empty());
        assert_eq!(ctx.project.lock().unwrap().as_ref().unwrap(), &before);
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn multi_file_import_accepts_tif_images() {
        let ctx = ServiceContext::new();
        let mut project = beambench_core::Project::new("Import TIFF");
        let layer_id = project.ensure_default_layer();
        *ctx.project.lock().unwrap() = Some(project);

        let dir = tempdir().unwrap();
        let tif_path = dir.path().join("demo.tif");
        let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
        image.save(&tif_path).unwrap();

        let objects = import_files_from_paths(
            &ctx,
            ImportFilesInput {
                create_layer: None,
                file_paths: vec![tif_path.to_string_lossy().to_string()],
                layer_id,
            },
        )
        .unwrap();

        assert_eq!(objects.len(), 1);
        assert!(matches!(objects[0].data, ObjectData::RasterImage { .. }));
    }

    #[test]
    fn import_files_from_data_imports_svg_and_image_by_content() {
        use base64::Engine;
        let ctx = ServiceContext::new();
        let mut project = beambench_core::Project::new("Drop Import");
        let layer_id = project.ensure_default_layer();
        *ctx.project.lock().unwrap() = Some(project);

        // Encode a real 2x2 grayscale PNG in memory.
        let mut png_bytes: Vec<u8> = Vec::new();
        let img = image::GrayImage::from_pixel(2, 2, image::Luma([0u8]));
        image::DynamicImage::ImageLuma8(img)
            .write_to(
                &mut std::io::Cursor::new(&mut png_bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        let b64 = base64::engine::general_purpose::STANDARD;

        let objects = import_files_from_data(
            &ctx,
            ImportFilesDataInput {
                create_layer: None,
                files: vec![
                    ImportFileData {
                        filename: "shape.svg".to_string(),
                        data_base64: b64.encode(sample_svg()),
                    },
                    ImportFileData {
                        filename: "photo.png".to_string(),
                        data_base64: b64.encode(&png_bytes),
                    },
                ],
                layer_id,
            },
        )
        .unwrap();

        assert_eq!(objects.len(), 2, "one vector + one image object");
        assert!(
            objects
                .iter()
                .any(|o| matches!(o.data, ObjectData::RasterImage { .. })),
            "PNG should import as a raster image"
        );
    }

    #[test]
    fn import_files_from_data_imports_lbrn_layers_groups_and_text() {
        use base64::Engine;
        let ctx = ServiceContext::new();
        let mut project = beambench_core::Project::new("Lbrn Import");
        let layer_id = project.ensure_default_layer();
        *ctx.project.lock().unwrap() = Some(project);

        let objects = import_files_from_data(
            &ctx,
            ImportFilesDataInput {
                create_layer: None,
                files: vec![ImportFileData {
                    filename: "fixture.lbrn2".to_string(),
                    data_base64: base64::engine::general_purpose::STANDARD
                        .encode(sample_lbrn_project()),
                }],
                layer_id,
            },
        )
        .unwrap();

        assert_eq!(
            objects.len(),
            4,
            "two children, their group, and editable text"
        );
        assert!(
            objects
                .iter()
                .any(|object| matches!(object.data, ObjectData::Group { .. }))
        );
        assert!(
            objects
                .iter()
                .any(|object| matches!(object.data, ObjectData::Text { .. }))
        );

        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        let c01 = project
            .layers
            .iter()
            .find(|layer| layer.name == "C01")
            .unwrap();
        assert_eq!(c01.color_tag.0, "#0000FF");
        assert_eq!(c01.primary_entry().speed_mm_min, 6000.0);
        assert_eq!(c01.primary_entry().power_percent, 20.0);
        assert_eq!(c01.primary_entry().power_min_percent, 5.0);
        assert_eq!(project.material_height_mm, Some(3.0));
        assert_eq!(project.notes, "Imported notes");
        assert!(ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn lbrn_image_power_scale_survives_import_planning_and_gcode_export() {
        use base64::Engine;
        use beambench_planner::PlanSegment;
        use std::collections::BTreeSet;

        let mut png = Vec::new();
        image::ImageEncoder::write_image(
            image::codecs::png::PngEncoder::new(&mut png),
            &[0; 16],
            4,
            4,
            image::ExtendedColorType::L8,
        )
        .unwrap();
        let data = base64::engine::general_purpose::STANDARD.encode(png);

        // MGL columns, then native LightBurn 1.6.03 reference cases with
        // nonzero minimum power. In grayscale mode, 50% scales 20..80 to 20..50.
        for (mode, min, max, scales, expected) in [
            (
                "jarvis",
                0,
                100,
                vec![10, 24, 38, 52, 66, 80],
                vec![(0, 100), (0, 240), (0, 380), (0, 520), (0, 660), (0, 800)],
            ),
            (
                "threshold",
                20,
                80,
                vec![0, 50, 100],
                vec![(0, 0), (100, 400), (200, 800)],
            ),
            (
                // A zeroed power scale means "do not fire" in both modes. Only
                // compressing the range would leave a grayscale image burning
                // flat at the 20% layer minimum over its whole area.
                "grayscale",
                20,
                80,
                vec![0, 50, 100],
                vec![(0, 0), (200, 500), (200, 800)],
            ),
        ] {
            for (angle, transform) in [
                (0, "1 0 0 1"),
                (45, "1 0 0 1"),
                (90, "1 0 0 1"),
                (0, "0 1 -1 0"),
            ] {
                let ctx = ServiceContext::with_settings(Default::default());
                let mut project = Project::new("Image power scale regression");
                project.workspace.bed_width_mm = 200.0;
                project.workspace.bed_height_mm = 200.0;
                let layer_id = project.ensure_default_layer();
                *ctx.project.lock().unwrap() = Some(project);
                let shapes: String = scales.iter().enumerate().map(|(i, scale)| format!(
                    r#"<Shape Type="Bitmap" CutIndex="1" PowerScale="{scale}" W="4" H="4" Data="{data}"><XForm>{transform} {} 20</XForm></Shape>"#,
                    20 + 10 * i,
                )).collect();
                let xml = format!(
                    r#"<LightBurnProject FormatVersion="0" MirrorX="False" MirrorY="False">
                      <CutSetting_Img type="Image"><index Value="1"/><name Value="Calibration"/>
                        <minPower Value="{min}"/><maxPower Value="{max}"/><speed Value="20"/>
                        <interval Value="0.25"/><angle Value="{angle}"/><ditherMode Value="{mode}"/>
                      </CutSetting_Img>{shapes}</LightBurnProject>"#
                );
                let objects = import_files_from_data(
                    &ctx,
                    ImportFilesDataInput {
                        create_layer: None,
                        files: vec![ImportFileData {
                            filename: "calibration.lbrn".into(),
                            data_base64: base64::engine::general_purpose::STANDARD.encode(xml),
                        }],
                        layer_id,
                    },
                )
                .unwrap();
                assert_eq!(objects.len(), scales.len());
                for (object, scale) in objects.iter().zip(&scales) {
                    assert_eq!(object.power_scale, f64::from(*scale) / 100.0);
                }
                let (plan, mut spool) =
                    super::super::planning::prepare_gcode_export(&ctx, &Default::default())
                        .unwrap();
                let mut powers: Vec<_> = plan
                    .segments
                    .iter()
                    .filter_map(|segment| {
                        let PlanSegment::Raster {
                            power_min_percent,
                            power_max_percent,
                            ..
                        } = segment
                        else {
                            return None;
                        };
                        Some((
                            (power_min_percent * 10.0).round() as u32,
                            (power_max_percent * 10.0).round() as u32,
                        ))
                    })
                    .collect();
                powers.sort();
                assert_eq!(powers, expected, "{mode}, angle {angle}, {transform}");
                let mut emitted = BTreeSet::from([0]);
                while let Some(line) = spool.next_line().unwrap() {
                    for word in line.split_whitespace() {
                        if let Some(value) =
                            word.strip_prefix('S').and_then(|s| s.parse::<u32>().ok())
                        {
                            emitted.insert(value);
                        }
                    }
                }
                let wanted: BTreeSet<_> = std::iter::once(0)
                    .chain(expected.iter().map(|(_, max)| *max))
                    .collect();
                assert_eq!(emitted, wanted, "{mode}, angle {angle}, {transform}");
            }
        }
    }

    #[test]
    fn lbrn_smooth_circle_preserves_project_contours_for_line_and_fill() {
        use base64::Engine;

        let legacy = include_str!("../../../beambench-core/tests/fixtures/lbrn/smooth-circle.lbrn");
        let compact =
            include_str!("../../../beambench-core/tests/fixtures/lbrn/smooth-circle.lbrn2");
        for (cut_type, operation) in [("Cut", OperationType::Line), ("Scan", OperationType::Fill)] {
            let import = |xml: &str, filename: &str| {
                let ctx = ServiceContext::new();
                let mut project = Project::new("Smooth curve import");
                let layer_id = project.ensure_default_layer();
                *ctx.project.lock().unwrap() = Some(project);
                let xml = xml.replace(
                    "<Shape",
                    &format!(
                        "<CutSetting type=\"{cut_type}\"><index Value=\"0\"/></CutSetting><Shape"
                    ),
                );
                let objects = import_files_from_data(
                    &ctx,
                    ImportFilesDataInput {
                        create_layer: None,
                        files: vec![ImportFileData {
                            filename: filename.to_string(),
                            data_base64: base64::engine::general_purpose::STANDARD.encode(xml),
                        }],
                        layer_id,
                    },
                )
                .unwrap();
                assert_eq!(objects.len(), 1);
                let object = objects.into_iter().next().unwrap();
                let project = ctx.project.lock().unwrap();
                let layer = project
                    .as_ref()
                    .unwrap()
                    .layers
                    .iter()
                    .find(|layer| layer.id == object.layer_id)
                    .unwrap();
                assert_eq!(layer.primary_entry().operation, operation);
                assert!(matches!(
                    &object.data,
                    ObjectData::VectorPath { closed: true, .. }
                ));
                object
            };
            let legacy = import(legacy, "circle.lbrn");
            let compact = import(compact, "circle.lbrn2");
            assert_eq!(compact.data, legacy.data, "{cut_type}");
            assert_eq!(compact.bounds, legacy.bounds, "{cut_type}");
        }
    }

    #[test]
    fn lbrn_import_maps_bottom_left_coordinates_and_inherited_t2_layer() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<LBRN_PROJECT_ROOT AppVersion="1.6.03" FormatVersion="1" MirrorX="False" MirrorY="False">
  <CutSetting type="Cut"><index Value="1"/><name Value="C01"/></CutSetting>
  <CutSetting type="Tool"><index Value="31"/><name Value="T2"/></CutSetting>
  <Shape Type="Path" CutIndex="1"><XForm>1 0 0 1 0 0</XForm>
    <VertList>V0 0V10 0V0 20</VertList><PrimList>LineClosed</PrimList>
  </Shape>
  <Shape Type="Group" CutIndex="31"><XForm>1 0 0 1 0 0</XForm><Children>
    <Shape Type="Rect" W="10" H="10"><XForm>1 0 0 1 5 5</XForm></Shape>
    <Shape Type="Rect" W="10" H="10"><XForm>1 0 0 1 20 10</XForm></Shape>
  </Children></Shape>
</LBRN_PROJECT_ROOT>"#
            .replace("LBRN_PROJECT_ROOT", concat!("Light", "BurnProject"));
        let document = parse_lbrn_project(xml.as_bytes()).unwrap();
        let mut project = Project::new("Bottom-left Lbrn import");
        project.layers.clear();
        project.workspace.bed_width_mm = 100.0;
        project.workspace.bed_height_mm = 80.0;

        let (objects, warnings) = import_lbrn_document(&mut project, document).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(objects.len(), 4, "path, two rectangles, and their group");
        assert_eq!(project.layers.len(), 2);

        let c01 = project
            .layers
            .iter()
            .find(|layer| layer.name == "C01")
            .unwrap();
        let t2 = project
            .layers
            .iter()
            .find(|layer| layer.name == "T2")
            .unwrap();
        assert!(t2.is_tool_layer);

        let path = project
            .objects
            .iter()
            .find(|object| matches!(object.data, ObjectData::VectorPath { .. }))
            .unwrap();
        assert_eq!(path.layer_id, c01.id);
        assert_eq!(path.bounds.min, Point2D::new(0.0, 60.0));
        assert_eq!(path.bounds.max, Point2D::new(10.0, 80.0));
        let ObjectData::VectorPath { path_data, .. } = &path.data else {
            unreachable!()
        };
        assert_eq!(path_data, "M0 80 L10 80 L0 60Z");

        let t2_rectangles = project
            .objects
            .iter()
            .filter(|object| {
                object.layer_id == t2.id && matches!(object.data, ObjectData::Shape { .. })
            })
            .collect::<Vec<_>>();
        assert_eq!(t2_rectangles.len(), 2);
        assert!(t2_rectangles.iter().any(|object| {
            object.bounds.min == Point2D::new(0.0, 70.0)
                && object.bounds.max == Point2D::new(10.0, 80.0)
        }));
    }

    #[test]
    fn lbrn_asymmetric_bitmap_orientation_survives_import_and_planning() {
        use base64::Engine;
        use beambench_core::WorkspaceOrigin;
        use beambench_planner::PlanSegment;

        // Staircase: each row differs, and its white right edge distinguishes
        // horizontal reflection. Cell centers stay clear of resampling edges.
        let pixels = [0, 255, 255, 255, 0, 0, 255, 255, 0, 0, 0, 255];
        let mut png = Vec::new();
        image::ImageEncoder::write_image(
            image::codecs::png::PngEncoder::new(&mut png),
            &pixels,
            4,
            3,
            image::ExtendedColorType::L8,
        )
        .unwrap();
        let data = base64::engine::general_purpose::STANDARD.encode(png);
        for origin in [WorkspaceOrigin::TopLeft, WorkspaceOrigin::BottomLeft] {
            for mirror_x in [false, true] {
                for mirror_y in [false, true] {
                    for (a, b, c, d) in [
                        (1.0, 0.0, 0.0, 1.0),
                        (0.0, 1.0, -1.0, 0.0),
                        (-1.0, 0.0, 0.0, -1.0),
                        (0.0, -1.0, 1.0, 0.0),
                        (-1.0, 0.0, 0.0, 1.0),
                        (1.0, 0.0, 0.0, -1.0),
                        (0.8, 0.6, -0.6, 0.8),
                        (1.0, 0.0, 0.3, 1.0),
                    ] {
                        for version in [0, 1] {
                            // Parent translation exercises nested group composition.
                            let xml = format!(
                                r#"<LightBurnProject FormatVersion="{version}" MirrorX="{mirror_x}" MirrorY="{mirror_y}">
                              <CutSetting_Img type="Image"><index Value="1"/><maxPower Value="30"/><speed Value="20"/><interval Value="1"/><ditherMode Value="threshold"/></CutSetting_Img>
                              <Shape Type="Group"><XForm>1 0 0 1 10 20</XForm><Children>
                                <Shape Type="Bitmap" CutIndex="1" W="40" H="30" Data="{data}"><XForm>{a} {b} {c} {d} 90 80</XForm></Shape>
                              </Children></Shape>
                            </LightBurnProject>"#
                            );
                            let mut project = Project::new("Orientation matrix");
                            project.workspace.origin = origin;
                            project.workspace.bed_width_mm = 200.0;
                            project.workspace.bed_height_mm = 200.0;
                            import_lbrn_document(
                                &mut project,
                                parse_lbrn_project(xml.as_bytes()).unwrap(),
                            )
                            .unwrap();
                            let plan = build_plan(&project).unwrap();
                            assert!(plan.failed_entries.is_empty());
                            let rows: Vec<_> = plan
                                .segments
                                .iter()
                                .filter_map(|segment| {
                                    if let PlanSegment::Raster { scanlines, .. } = segment {
                                        Some(scanlines)
                                    } else {
                                        None
                                    }
                                })
                                .flatten()
                                .collect();
                            assert!(!rows.is_empty());
                            for (i, pixel) in pixels.iter().enumerate() {
                                let x = (i % 4) as f64 * 10.0 - 15.0;
                                let y = (i / 4) as f64 * 10.0 - 10.0;
                                let mut wx = a * x + c * y + 100.0;
                                let mut wy = b * x + d * y + 100.0;
                                if mirror_x {
                                    wx = 200.0 - wx;
                                }
                                if !mirror_y {
                                    wy = 200.0 - wy;
                                }
                                if origin == WorkspaceOrigin::BottomLeft {
                                    wy = 200.0 - wy;
                                }
                                let row = rows
                                    .iter()
                                    .min_by(|left, right| {
                                        (left.y_mm - wy).abs().total_cmp(&(right.y_mm - wy).abs())
                                    })
                                    .unwrap();
                                let burns = (row.y_mm - wy).abs() < 1.1
                                    && row
                                        .runs
                                        .iter()
                                        .any(|run| wx > run.start_x_mm && wx < run.end_x_mm);
                                assert_eq!(
                                    burns,
                                    *pixel == 0,
                                    "origin={origin:?} root={mirror_x}/{mirror_y} matrix={a},{b},{c},{d} version={version} pixel={i} point={wx},{wy}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn lbrn_world_transform_respects_root_mirror_flags() {
        let mut project = Project::new("Mirrored Lbrn import");
        project.workspace.bed_width_mm = 100.0;
        project.workspace.bed_height_mm = 80.0;

        let bottom_left = lbrn_world_to_canvas_transform(&project, false, false);
        assert_eq!(
            bottom_left.apply(&Point2D::new(10.0, 20.0)),
            Point2D::new(10.0, 60.0)
        );

        let mirrored_both = lbrn_world_to_canvas_transform(&project, true, true);
        assert_eq!(
            mirrored_both.apply(&Point2D::new(10.0, 20.0)),
            Point2D::new(90.0, 20.0)
        );

        let semantic = lbrn_semantic_object_transform(bottom_left, Transform2D::identity());
        assert_eq!(semantic, Transform2D::translate(0.0, 80.0));
    }

    #[test]
    fn import_files_from_data_rejects_bad_base64() {
        let ctx = ServiceContext::new();
        let mut project = beambench_core::Project::new("Drop Import");
        let layer_id = project.ensure_default_layer();
        *ctx.project.lock().unwrap() = Some(project);

        let err = import_files_from_data(
            &ctx,
            ImportFilesDataInput {
                create_layer: None,
                files: vec![ImportFileData {
                    filename: "shape.svg".to_string(),
                    data_base64: "not!!valid@@base64".to_string(),
                }],
                layer_id,
            },
        )
        .unwrap_err();
        assert!(err.message.contains("shape.svg"));
    }

    #[test]
    fn import_emits_single_completed_event() {
        let ctx = ServiceContext::new();
        let mut project = beambench_core::Project::new("Import");
        let layer_id = project.ensure_default_layer();
        *ctx.project.lock().unwrap() = Some(project);
        let mut rx = ctx.events.subscribe();

        let dir = tempdir().unwrap();
        let svg_path = dir.path().join("demo.svg");
        std::fs::write(&svg_path, sample_svg()).unwrap();

        let objects = import_svg_from_path(
            &ctx,
            ImportSvgInput {
                file_path: svg_path.to_string_lossy().to_string(),
                layer_id,
            },
        )
        .unwrap();

        assert_eq!(objects.len(), 1);
        let msg = rx.try_recv().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(parsed["type"], "project.import.completed");
        assert_eq!(parsed["payload"]["file_count"], 1);
    }

    #[test]
    fn import_larger_than_workspace_emits_oversized_event() {
        use base64::Engine;
        let ctx = ServiceContext::new();
        let mut project = beambench_core::Project::new("Oversized");
        let layer_id = project.ensure_default_layer();
        let bed_w = project.workspace.bed_width_mm;
        *ctx.project.lock().unwrap() = Some(project);
        let mut rx = ctx.events.subscribe();

        // 4000px ≈ 1058mm: far larger than any default bed.
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="4000" height="4000"><rect x="0" y="0" width="4000" height="4000"/></svg>"#;
        let objects = import_files_from_data(
            &ctx,
            ImportFilesDataInput {
                create_layer: None,
                files: vec![ImportFileData {
                    filename: "big.svg".to_string(),
                    data_base64: base64::engine::general_purpose::STANDARD.encode(svg),
                }],
                layer_id,
            },
        )
        .unwrap();
        assert_eq!(objects.len(), 1);
        let expected_mm = 4000.0 * 25.4 / 96.0;
        assert!(
            (objects[0].bounds.width() - expected_mm).abs() < 0.1,
            "import must preserve the file's true dimensions"
        );

        let mut saw_oversized = false;
        while let Ok(msg) = rx.try_recv() {
            let parsed: serde_json::Value = serde_json::from_str(&msg).unwrap();
            if parsed["type"] == "project.import.oversized" {
                saw_oversized = true;
                assert!(parsed["payload"]["width_mm"].as_f64().unwrap() > bed_w);
                assert_eq!(parsed["payload"]["bed_width_mm"].as_f64().unwrap(), bed_w);
            }
        }
        assert!(
            saw_oversized,
            "an import larger than the workspace must emit project.import.oversized"
        );
    }

    #[test]
    fn imported_svg_text_has_resolved_path_data() {
        let ctx = ServiceContext::new();
        let mut project = beambench_core::Project::new("Import Text");
        let layer_id = project.ensure_default_layer();
        *ctx.project.lock().unwrap() = Some(project);

        let dir = tempdir().unwrap();
        let svg_path = dir.path().join("text.svg");
        std::fs::write(
            &svg_path,
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50">
                <text x="10" y="30" font-family="sans-serif" font-size="16">Hello World</text>
            </svg>"#,
        )
        .unwrap();

        let objects = import_svg_from_path(
            &ctx,
            ImportSvgInput {
                file_path: svg_path.to_string_lossy().to_string(),
                layer_id,
            },
        )
        .unwrap();

        assert!(!objects.is_empty(), "should import at least one object");

        // Read the text object from the project (after refresh_project_text_caches ran)
        let project_guard = ctx.project.lock().unwrap();
        let p = project_guard.as_ref().unwrap();
        let text_obj = p
            .objects
            .iter()
            .find(|obj| matches!(obj.data, ObjectData::Text { .. }));
        assert!(
            text_obj.is_some(),
            "should have at least one text object in project"
        );
        let text_obj = text_obj.unwrap();
        match &text_obj.data {
            ObjectData::Text {
                resolved_path_data,
                resolved_font_source,
                ..
            } => {
                assert!(
                    resolved_path_data.is_some(),
                    "imported text should have resolved_path_data after cache refresh"
                );
                assert!(
                    resolved_font_source.is_some(),
                    "imported text should have resolved_font_source after cache refresh"
                );
            }
            _ => panic!("expected text"),
        }

        // Bounds should be resized from actual glyph geometry, not the
        // heuristic estimate (content.len() * 0.6 * font_size).
        let bounds = text_obj.bounds;
        assert!(bounds.width() > 0.0, "text bounds width should be positive");
        assert!(
            bounds.height() > 0.0,
            "text bounds height should be positive"
        );
        // The import heuristic for "Hello World" (11 chars) at font_size=16
        // would give width ≈ 11 * 0.6 * 16 * scale ≈ ~105 * scale.
        // Actual glyph metrics will differ — just verify the bounds are
        // consistent with the resolved path data.
        if let ObjectData::Text {
            resolved_path_data: Some(ref pd),
            ..
        } = text_obj.data
        {
            let path = beambench_common::path::VecPath::parse_svg_d(pd);
            if let Some(path_bounds) = path.bounds() {
                // Bounds width should approximate the path width (within font_size tolerance)
                let diff = (bounds.width() - path_bounds.width()).abs();
                assert!(
                    diff < 5.0,
                    "bounds width should match resolved path width: bounds={:.1} path={:.1}",
                    bounds.width(),
                    path_bounds.width()
                );
            }
        }
    }

    #[test]
    fn import_dxf_uses_single_undo_snapshot_and_invalidates_plan() {
        let ctx = ServiceContext::new();
        let mut project = beambench_core::Project::new("Import");
        let layer_id = project.ensure_default_layer();
        let plan = sample_plan(&project);
        *ctx.project.lock().unwrap() = Some(project);
        *ctx.plan_cache.lock().unwrap() = Some((plan).into());

        let dir = tempdir().unwrap();
        let dxf_path = dir.path().join("demo.dxf");
        std::fs::write(
            &dxf_path,
            "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0\n20\n0\n11\n10\n21\n10\n0\nENDSEC\n0\nEOF\n",
        )
        .unwrap();

        let objects = import_vector_file_from_path(
            &ctx,
            ImportVectorFileInput {
                file_path: dxf_path.to_string_lossy().to_string(),
                layer_id,
                format: VectorImportFormat::Dxf,
            },
        )
        .unwrap();

        assert_eq!(objects.len(), 1);
        assert!(ctx.undo_state().unwrap().can_undo);
        assert!(ctx.plan_cache.lock().unwrap().is_none());
    }

    #[test]
    fn import_dxf_rejects_files_with_no_usable_geometry() {
        let parsed = parse_vector_bytes(
            b"0\nSECTION\n2\nENTITIES\n0\nTEXT\n10\n0\n20\n0\n1\nHello\n0\nENDSEC\n",
            VectorImportFormat::Dxf,
        );
        let error = parsed.expect_err("text-only DXF should not silently import nothing");
        let message = error.to_string();
        assert!(message.contains("no usable 2D vector geometry"));
        assert!(message.contains("1 TEXT"));
    }

    #[test]
    fn import_dxf_surfaces_a_partial_import_warning() {
        let ctx = ServiceContext::new();
        let mut project = beambench_core::Project::new("Partial DXF");
        let layer_id = project.ensure_default_layer();
        *ctx.project.lock().unwrap() = Some(project);
        let mut rx = ctx.events.subscribe();

        let dir = tempdir().unwrap();
        let dxf_path = dir.path().join("partial.dxf");
        std::fs::write(
            &dxf_path,
            "0\nSECTION\n2\nENTITIES\n0\nTEXT\n10\n1\n20\n2\n1\nHello\n0\nLINE\n10\n0\n20\n0\n11\n10\n21\n0\n0\nENDSEC\n",
        )
        .unwrap();

        let objects = import_vector_file_from_path(
            &ctx,
            ImportVectorFileInput {
                file_path: dxf_path.to_string_lossy().to_string(),
                layer_id,
                format: VectorImportFormat::Dxf,
            },
        )
        .unwrap();
        assert_eq!(objects.len(), 1);

        let mut completed = None;
        while let Ok(message) = rx.try_recv() {
            let event: serde_json::Value = serde_json::from_str(&message).unwrap();
            if event["type"] == "project.import.completed" {
                completed = Some(event);
            }
        }
        let warnings = &completed.expect("completed import event")["payload"]["warnings"];
        assert_eq!(warnings.as_array().map(Vec::len), Some(1));
        assert!(warnings[0].as_str().unwrap().contains("1 TEXT"));
    }

    #[test]
    fn pdf_and_ai_import_opaque_graphics_state_with_color_routing_and_undo() {
        let bytes =
            include_bytes!("../../../beambench-core/tests/fixtures/pdf/opaque-graphics-state.pdf");
        for (extension, format) in [
            ("pdf", VectorImportFormat::Pdf),
            ("ai", VectorImportFormat::Ai),
        ] {
            let (ctx, layer_id, configured_red_id) = colored_pdf_import_context();
            let before = ctx.project.lock().unwrap().as_ref().unwrap().clone();
            *ctx.plan_cache.lock().unwrap() = Some((sample_plan(&before)).into());
            let dir = tempdir().unwrap();
            let path = dir.path().join(format!("opaque-state.{extension}"));
            std::fs::write(&path, bytes).unwrap();

            let objects = import_vector_file_from_path(
                &ctx,
                ImportVectorFileInput {
                    file_path: path.to_string_lossy().to_string(),
                    layer_id,
                    format,
                },
            )
            .unwrap();

            assert_eq!(objects.len(), 2, "{extension}");
            assert_eq!(objects[0].layer_id, configured_red_id);
            let guard = ctx.project.lock().unwrap();
            let project = guard.as_ref().unwrap();
            assert_eq!(
                project.find_layer(objects[1].layer_id).unwrap().color_tag.0,
                "#0000FF"
            );
            assert!((objects[0].bounds.width() - 25.4).abs() < 1e-6);
            drop(guard);
            assert!(ctx.plan_cache.lock().unwrap().is_none());
            assert!(ctx.undo_state().unwrap().can_undo);
        }
    }

    #[test]
    fn import_pdf_uses_single_undo_snapshot_and_invalidates_plan() {
        let ctx = ServiceContext::new();
        let mut project = beambench_core::Project::new("Import");
        let layer_id = project.ensure_default_layer();
        let plan = sample_plan(&project);
        *ctx.project.lock().unwrap() = Some(project);
        *ctx.plan_cache.lock().unwrap() = Some((plan).into());

        let dir = tempdir().unwrap();
        let pdf_path = dir.path().join("demo.pdf");
        std::fs::write(&pdf_path, sample_pdf_bytes()).unwrap();

        let objects = import_vector_file_from_path(
            &ctx,
            ImportVectorFileInput {
                file_path: pdf_path.to_string_lossy().to_string(),
                layer_id,
                format: VectorImportFormat::Pdf,
            },
        )
        .unwrap();

        assert!(!objects.is_empty());
        assert!(ctx.undo_state().unwrap().can_undo);
        assert!(ctx.plan_cache.lock().unwrap().is_none());
    }

    #[test]
    fn pdf_color_metadata_uses_painted_color_with_stroke_preferred_for_fill_stroke() {
        let path = VecPath::parse_svg_d("M0 0 L10 10");
        let stroke = PdfRgbColor { r: 255, g: 0, b: 0 };
        let fill = PdfRgbColor { r: 0, g: 0, b: 255 };

        for (paint_mode, expected) in [
            (PdfPaintMode::Stroke, Some("#FF0000")),
            (PdfPaintMode::Fill, Some("#0000FF")),
            (PdfPaintMode::FillStroke, Some("#FF0000")),
            (PdfPaintMode::Unspecified, None),
        ] {
            let imported = ImportedVectorPath::from_pdf(PdfPaintedPath {
                path: path.clone(),
                stroke_color: Some(stroke),
                fill_color: Some(fill),
                paint_mode,
            });
            assert_eq!(imported.color_tag.as_deref(), expected);
        }
    }

    #[test]
    fn colored_pdf_reuses_configured_layer_and_creates_safe_line_layer() {
        let (ctx, requested_id, configured_red_id) = colored_pdf_import_context();
        let bytes = sample_colored_pdf_bytes();
        let expected_paths: Vec<_> = parse_pdf_vector_import(&bytes)
            .unwrap()
            .paths
            .into_iter()
            .map(|path| path.path.to_svg_d())
            .collect();
        let mut rx = ctx.events.subscribe();

        let dir = tempdir().unwrap();
        let pdf_path = dir.path().join("red-blue.pdf");
        std::fs::write(&pdf_path, bytes).unwrap();
        let objects = import_files_from_paths(
            &ctx,
            ImportFilesInput {
                create_layer: None,
                file_paths: vec![pdf_path.to_string_lossy().to_string()],
                layer_id: requested_id,
            },
        )
        .unwrap();

        assert_eq!(objects.len(), 2);
        let project_guard = ctx.project.lock().unwrap();
        let project = project_guard.as_ref().unwrap();
        let blue_layer = project
            .layers
            .iter()
            .find(|layer| layer.color_tag.0 == "#0000FF")
            .expect("blue source color should create a layer");
        assert_eq!(blue_layer.primary_entry().operation, OperationType::Line);
        assert!(!blue_layer.is_tool_layer);
        assert!(blue_layer.name.contains("#0000FF"));

        assert_eq!(objects[0].layer_id, configured_red_id);
        assert_eq!(objects[1].layer_id, blue_layer.id);
        assert_eq!(
            project
                .find_layer(configured_red_id)
                .unwrap()
                .primary_entry()
                .operation,
            OperationType::Cut,
            "matching an existing color must preserve its configured operation"
        );
        for (object, expected_path_data) in objects.iter().zip(&expected_paths) {
            let ObjectData::VectorPath { path_data, .. } = &object.data else {
                panic!("expected vector path")
            };
            assert_eq!(path_data, expected_path_data);
            assert_eq!(project.find_object(object.id), Some(object));
        }
        let blue_id = blue_layer.id;
        assert_eq!(project.layers.len(), 3);
        drop(project_guard);

        assert!(ctx.undo_state().unwrap().can_undo);
        assert!(ctx.plan_cache.lock().unwrap().is_none());

        let mut completed = None;
        while let Ok(message) = rx.try_recv() {
            let event: serde_json::Value = serde_json::from_str(&message).unwrap();
            if event["type"] == "project.import.completed" {
                completed = Some(event);
            }
        }
        let completed = completed.expect("completed import event");
        assert_eq!(
            completed["payload"]["resolved_layer_ids"],
            json!([configured_red_id, blue_id])
        );

        let restored = crate::ops::project::undo_project(&ctx).unwrap();
        assert!(restored.objects.is_empty());
        assert_eq!(restored.layers.len(), 2);
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn colored_pdf_drag_drop_preserves_matching_operations_and_source_colors() {
        use base64::Engine;

        let (ctx, requested_id, configured_red_id) = colored_pdf_import_context();
        let objects = import_files_from_data(
            &ctx,
            ImportFilesDataInput {
                create_layer: None,
                files: vec![ImportFileData {
                    filename: "red-blue.pdf".to_string(),
                    data_base64: base64::engine::general_purpose::STANDARD
                        .encode(sample_colored_pdf_bytes()),
                }],
                layer_id: requested_id,
            },
        )
        .unwrap();

        assert_eq!(objects.len(), 2);
        assert_eq!(objects[0].layer_id, configured_red_id);
        let project_guard = ctx.project.lock().unwrap();
        let project = project_guard.as_ref().unwrap();
        assert_eq!(
            project
                .find_layer(configured_red_id)
                .unwrap()
                .primary_entry()
                .operation,
            OperationType::Cut
        );
        let blue_layer = project
            .layers
            .iter()
            .find(|layer| layer.color_tag.0 == "#0000FF")
            .expect("dragged blue path should retain its source color");
        assert_eq!(objects[1].layer_id, blue_layer.id);
        assert_eq!(blue_layer.primary_entry().operation, OperationType::Line);
        assert!(!blue_layer.is_tool_layer);
        drop(project_guard);
        assert!(ctx.undo_state().unwrap().can_undo);
        assert!(ctx.plan_cache.lock().unwrap().is_none());
    }

    #[test]
    fn colored_pdf_art_library_import_uses_color_layer_routing() {
        let (ctx, requested_id, configured_red_id) = colored_pdf_import_context();
        let objects = import_art_library_item(
            &ctx,
            requested_id,
            "Library Colors",
            "library-colors.pdf",
            "application/pdf",
            sample_colored_pdf_bytes(),
        )
        .unwrap();

        assert_eq!(objects.len(), 2);
        assert_eq!(objects[0].layer_id, configured_red_id);
        let project_guard = ctx.project.lock().unwrap();
        let project = project_guard.as_ref().unwrap();
        let blue_layer = project
            .layers
            .iter()
            .find(|layer| layer.color_tag.0 == "#0000FF")
            .expect("blue source color should create a layer");
        assert_eq!(objects[1].layer_id, blue_layer.id);
        assert_eq!(blue_layer.primary_entry().operation, OperationType::Line);
        drop(project_guard);
        assert!(ctx.undo_state().unwrap().can_undo);
        assert!(ctx.plan_cache.lock().unwrap().is_none());
    }

    #[test]
    fn vector_import_with_more_than_sixteen_colors_falls_back_to_requested_layer() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Color cap");
        let requested_id = project.ensure_default_layer();
        *ctx.project.lock().unwrap() = Some(project);

        let paths = (0..=MAX_AUTO_COLOR_LAYERS)
            .map(|index| ImportedVectorPath {
                path: VecPath::parse_svg_d(&format!("M{index} 0 L{index} 10")),
                color_tag: Some(format!("#{index:06X}")),
            })
            .collect();
        let objects =
            import_vector_paths(&ctx, requested_id, 1, "Many Colors", paths, Vec::new()).unwrap();

        assert_eq!(objects.len(), MAX_AUTO_COLOR_LAYERS + 1);
        assert!(objects.iter().all(|object| object.layer_id == requested_id));
        assert_eq!(
            ctx.project.lock().unwrap().as_ref().unwrap().layers.len(),
            1
        );
    }

    #[test]
    fn import_pdf_compatible_ai_preserves_color_layer_routing_and_invalidates_plan() {
        let (ctx, layer_id, configured_red_id) = colored_pdf_import_context();

        let dir = tempdir().unwrap();
        let ai_path = dir.path().join("demo.ai");
        std::fs::write(&ai_path, sample_colored_pdf_bytes()).unwrap();

        let objects = import_vector_file_from_path(
            &ctx,
            ImportVectorFileInput {
                file_path: ai_path.to_string_lossy().to_string(),
                layer_id,
                format: VectorImportFormat::Ai,
            },
        )
        .unwrap();

        assert_eq!(objects.len(), 2);
        assert_eq!(objects[0].layer_id, configured_red_id);
        let project_guard = ctx.project.lock().unwrap();
        let project = project_guard.as_ref().unwrap();
        let blue_layer = project
            .layers
            .iter()
            .find(|layer| layer.color_tag.0 == "#0000FF")
            .expect("PDF-compatible AI should preserve its blue source color");
        assert_eq!(objects[1].layer_id, blue_layer.id);
        drop(project_guard);
        assert!(ctx.undo_state().unwrap().can_undo);
        assert!(ctx.plan_cache.lock().unwrap().is_none());
    }

    #[test]
    fn import_ai_uses_postscript_parser_path_and_invalidates_plan() {
        let ctx = ServiceContext::new();
        let mut project = beambench_core::Project::new("Import");
        let layer_id = project.ensure_default_layer();
        let plan = sample_plan(&project);
        *ctx.project.lock().unwrap() = Some(project);
        *ctx.plan_cache.lock().unwrap() = Some((plan).into());

        let dir = tempdir().unwrap();
        let ai_path = dir.path().join("demo.ai");
        std::fs::write(
            &ai_path,
            b"%!PS-Adobe-3.0\n%%AI5_FileFormat 3\n10 20 moveto\n30 40 lineto\nclosepath\nstroke\n",
        )
        .unwrap();

        let objects = import_vector_file_from_path(
            &ctx,
            ImportVectorFileInput {
                file_path: ai_path.to_string_lossy().to_string(),
                layer_id,
                format: VectorImportFormat::Ai,
            },
        )
        .unwrap();

        assert!(!objects.is_empty());
        assert!(ctx.undo_state().unwrap().can_undo);
        assert!(ctx.plan_cache.lock().unwrap().is_none());
    }

    #[test]
    fn import_art_library_pdf_uses_normal_vector_import_path() {
        let (ctx, layer_id) = art_library_import_context();

        let objects = import_art_library_item(
            &ctx,
            layer_id,
            "Library PDF",
            "library.pdf",
            "application/pdf",
            sample_pdf_bytes(),
        )
        .unwrap();

        assert_art_library_vector_import_completed(&ctx, layer_id, &objects);
    }

    #[test]
    fn import_art_library_ai_sniffs_pdf_and_postscript_payloads() {
        for (source_filename, bytes) in [
            ("pdf-compatible.ai", sample_pdf_bytes()),
            ("postscript.ai", sample_postscript_bytes().to_vec()),
        ] {
            let (ctx, layer_id) = art_library_import_context();

            let objects = import_art_library_item(
                &ctx,
                layer_id,
                "Library AI",
                source_filename,
                "application/ai",
                bytes,
            )
            .unwrap();

            assert_art_library_vector_import_completed(&ctx, layer_id, &objects);
        }
    }

    #[test]
    fn oversized_vector_files_are_refused_before_parsing() {
        let huge = vec![b' '; VECTOR_IMPORT_BYTE_LIMIT + 1];
        for name in ["big.svg", "big.dxf", "big.eps", "big.lbrn2"] {
            let error = match prepare_pending_import(name, huge.clone(), None) {
                Err(error) => error,
                Ok(_) => panic!("{name} was accepted"),
            };
            assert!(error.message.contains("MB limit"), "{name}: {error}");
        }
    }

    #[test]
    fn y_up_design_exports_round_trip_without_mirroring() {
        let mut source = Project::new("Round trip");
        source.workspace.bed_height_mm = 100.0;
        let layer_id = source.ensure_default_layer();
        // Open path: a short stroke at the top, a long one going down.
        source.add_object(ProjectObject::new(
            "Hook",
            layer_id,
            Bounds::new(Point2D::new(10.0, 20.0), Point2D::new(30.0, 40.0)),
            ObjectData::VectorPath {
                path_data: "M30 20 L10 20 L10 40".into(),
                closed: false,
                ruler_guide_axis: None,
            },
        ));
        let exports = [
            (
                "round.dxf",
                "application/dxf",
                beambench_core::export_dxf(&source, false, &[]).into_bytes(),
            ),
            ("round.pdf", "application/pdf", export_pdf(&source, false, &[]).unwrap()),
            (
                "round.eps",
                "application/postscript",
                beambench_core::export_eps(&source, false, &[]).unwrap().into_bytes(),
            ),
        ];
        for (filename, media_type, bytes) in exports {
            let (ctx, layer_id) = art_library_import_context();
            ctx.project.lock().unwrap().as_mut().unwrap().workspace.bed_height_mm = 100.0;
            let objects =
                import_art_library_item(&ctx, layer_id, "Round", filename, media_type, bytes)
                    .unwrap();
            // DXF lines import as separate objects; check them together.
            let vectors: Vec<_> = objects
                .iter()
                .filter(|object| matches!(object.data, ObjectData::VectorPath { .. }))
                .collect();
            assert!(!vectors.is_empty(), "{filename}: no vector imported");
            let min_y = vectors.iter().map(|o| o.bounds.min.y).fold(f64::MAX, f64::min);
            let max_y = vectors.iter().map(|o| o.bounds.max.y).fold(f64::MIN, f64::max);
            assert!(
                (min_y - 20.0).abs() < 0.05 && (max_y - 40.0).abs() < 0.05,
                "{filename}: y {min_y}..{max_y}"
            );
            // The short horizontal stroke stays at the top, not mirrored down.
            let points: Vec<_> = vectors
                .iter()
                .flat_map(|object| {
                    let ObjectData::VectorPath { path_data, .. } = &object.data else {
                        unreachable!()
                    };
                    beambench_common::path::VecPath::parse_svg_d(path_data).subpaths
                })
                .flat_map(|subpath| subpath.commands)
                .filter_map(|command| match command {
                    beambench_common::path::PathCommand::MoveTo { x, y }
                    | beambench_common::path::PathCommand::LineTo { x, y } => Some((x, y)),
                    _ => None,
                })
                .collect();
            assert!(
                points
                    .iter()
                    .any(|(x, y)| (x - 30.0).abs() < 0.05 && (y - 20.0).abs() < 0.05),
                "{filename}: {points:?}"
            );
        }
    }

    #[test]
    fn import_art_library_eps_uses_normal_vector_import_path() {
        let (ctx, layer_id) = art_library_import_context();

        let objects = import_art_library_item(
            &ctx,
            layer_id,
            "Library EPS",
            "library.eps",
            "application/postscript",
            sample_postscript_bytes().to_vec(),
        )
        .unwrap();

        assert_art_library_vector_import_completed(&ctx, layer_id, &objects);
    }

    #[test]
    fn import_gcode_creates_open_vector_objects_and_invalidates_plan() {
        let ctx = ServiceContext::new();
        let mut project = beambench_core::Project::new("Import");
        let layer_id = project.ensure_default_layer();
        let plan = sample_plan(&project);
        *ctx.project.lock().unwrap() = Some(project);
        *ctx.plan_cache.lock().unwrap() = Some((plan).into());

        let dir = tempdir().unwrap();
        let gcode_path = dir.path().join("demo.gcode");
        std::fs::write(
            &gcode_path,
            "G90\nG0 X0 Y0\nM4 S500\nG1 X10 Y0\nG1 X10 Y10\nM5\n",
        )
        .unwrap();

        let objects = import_gcode_from_path(
            &ctx,
            ImportGcodeInput {
                file_path: gcode_path.to_string_lossy().to_string(),
                layer_id,
            },
        )
        .unwrap();

        assert_eq!(objects.len(), 1);
        match &objects[0].data {
            ObjectData::VectorPath {
                path_data, closed, ..
            } => {
                assert_eq!(path_data, "M0 0 L10 0 L10 10");
                assert!(!closed);
            }
            other => panic!("expected vector path import, got {other:?}"),
        }
        assert_eq!(objects[0].layer_id, layer_id);
        assert!(ctx.undo_state().unwrap().can_undo);
        assert!(ctx.plan_cache.lock().unwrap().is_none());
    }

    #[test]
    fn trace_preview_returns_pixel_space_paths_without_modifying_project() {
        use beambench_core::{Asset, AssetMediaType};

        let ctx = ServiceContext::new();
        let mut project = Project::new("Preview");
        let layer_id = project.ensure_default_layer();

        // Create a 64x64 image with a solid black rectangle for tracing
        let img = image::GrayImage::from_fn(64, 64, |x, y| {
            if x >= 16 && x < 48 && y >= 16 && y < 48 {
                image::Luma([0u8]) // black center block
            } else {
                image::Luma([255u8]) // white border
            }
        });
        let mut png_bytes = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(std::io::Cursor::new(&mut png_bytes));
        image::ImageEncoder::write_image(
            encoder,
            img.as_raw(),
            64,
            64,
            image::ExtendedColorType::L8,
        )
        .unwrap();

        let asset = Asset::new(
            "test.png",
            AssetMediaType::Png,
            png_bytes.len() as u64,
            Some(64),
            Some(64),
        );
        let asset_id = asset.id;
        project.add_asset(asset, png_bytes);

        let obj = ProjectObject::new(
            "TestImg",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(40.0, 40.0)),
            ObjectData::RasterImage {
                asset_key: asset_id.to_string(),
                original_width_px: 64,
                original_height_px: 64,
                adjustments: None,
                masks: Vec::new(),
            },
        );
        let obj_id = obj.id;
        project.add_object(obj);
        *ctx.project.lock().unwrap() = Some(project);

        let result = trace_image_preview(
            &ctx,
            TraceImageInput {
                object_id: obj_id,
                threshold: 128,
                cutoff: 0,
                turdsize: 2,
                alphamax: 1.0,
                opttolerance: 0.2,
                trace_alpha: false,
                sketch_trace: false,
                delete_source: false,
                preview_request_id: None,
                boundary: None,
            },
        )
        .unwrap();

        // Should return pixel dimensions
        assert_eq!(result.source_width, 64);
        assert_eq!(result.source_height, 64);
        // Should have at least one traced path
        assert!(!result.paths.is_empty());
        // Project should be unmodified — no undo snapshot, no new objects
        assert!(!ctx.undo_state().unwrap().can_undo);
        let guard = ctx.project.lock().unwrap();
        let p = guard.as_ref().unwrap();
        // Only the original raster object should exist (no traced vector objects)
        assert_eq!(p.objects.len(), 1);
        assert_eq!(p.objects[0].id, obj_id);
    }
}
