//! Transient quality-test job pipeline (Material Test, Focus Test, Interval Test).
//!
//! Builds a synthetic in-memory `Project` from the active project context plus a per-tool request,
//! runs it through the planner with `QualityTestOrdering::RowMajor` so segments emit in append
//! order, and produces preview/G-code outputs without ever writing to `ServiceContext.plan_cache`,
//! mutating the active project, or pushing to the undo stack.
//!
//! ## Layer count tradeoff
//!
//! The plan describes "exactly two transient layers" (`Test Samples` + `Test Labels`). In practice
//! the planner emits one cut entry's settings per layer, so per-sample variation in speed, power,
//! Z, or interval requires **one layer per sample** — the synthetic project ends up with `N + 1`
//! layers (N samples + 1 labels). Conceptually they remain two groups; physically they are split
//! to give each sample its own settings without per-object overrides for speed/power/etc.

use beambench_common::StartFromMode;
use beambench_common::geometry::{Bounds, Point2D};
use beambench_common::machine::JobProgress;
use beambench_common::{AnchorPoint, ColorTag};
use beambench_core::ProjectOptimization;
use beambench_core::{
    CutEntry, CutEntryId, FocusTestSettings, FocusTestZMode, IntervalTestSettings, Layer, LayerRef,
    MachineProfile, MaterialTestAxis, MaterialTestAxisParam, MaterialTestSettings, ObjectData,
    ObjectId, OperationType, Project, ProjectObject, QualityTestError, QualityTestRequest,
    QualityTestWarning, RasterSettings, TextAlignment, TextAlignmentV, TextLayoutMode,
    VectorSettings, Workspace,
};
use beambench_grbl::{GcodeConfig, generate_gcode};
use beambench_planner::{
    ExecutionPlan, OptimizationRuntime, PlanSegment, PlannerInput, anchor_segments_to_target,
    build_frame_plan, build_plan_with_input, calculate_plan_bounds,
};
use beambench_preview::{PreviewData, distill_preview};
use beambench_streamer::JobController;

use crate::context::ServiceContext;
use crate::error::{ServiceError, ServiceResult};
use crate::ops::output;
use crate::runtime::{
    ActiveJobHandle, MachineSessionHandle, MarlinRuntimeJob, SmoothiewareRuntimeJob,
};

const SAMPLES_LAYER_PREFIX: &str = "QT/Samples";
const LABELS_LAYER_NAME: &str = "QT/Labels";
const SAMPLE_COLOR: &str = "#000000";
const LABEL_COLOR: &str = "#666666";
const LABEL_FONT_SIZE_MM: f64 = 3.0;
const LABEL_FONT_FAMILY: &str = "Liberation Sans";
const LABEL_GAP_MM: f64 = 1.5;

#[derive(Debug, Clone)]
pub struct TransientPreviewResponse {
    pub preview: PreviewData,
    pub warnings: Vec<QualityTestWarning>,
}

#[derive(Debug, Clone)]
pub struct TransientExportResponse {
    pub path: std::path::PathBuf,
    pub warnings: Vec<QualityTestWarning>,
}

#[derive(Debug, Clone)]
pub struct CanvasMaterialTestResponse {
    pub project: Project,
    pub warnings: Vec<QualityTestWarning>,
    pub created_object_ids: Vec<ObjectId>,
    pub created_layer_ids: Vec<LayerRef>,
}

/// Snapshot of the active project context needed to build a synthetic project.
///
/// Mirrors every project field the transient planner/streamer reads downstream — workspace,
/// origin/start-from transform stack, optimization (for `finish_position` and friends), material
/// height, and the seed cut entry. Anything missing here causes the synthetic job to diverge from
/// where the real job would land or how it would terminate.
pub struct ProjectContext {
    workspace: Workspace,
    job_origin: AnchorPoint,
    user_origin: Option<(f64, f64)>,
    start_from: StartFromMode,
    optimization: ProjectOptimization,
    material_height_mm: Option<f64>,
    seed_entry: CutEntry,
}

fn material_test_internal_seed() -> CutEntry {
    CutEntry::new(OperationType::Fill)
}

fn focus_test_internal_seed(settings: &FocusTestSettings) -> CutEntry {
    let mut entry = CutEntry::new(OperationType::Line);
    entry.speed_mm_min = settings.speed_mm_min.max(1.0);
    entry.power_percent = settings.power_percent.clamp(0.0, 100.0);
    entry.vector_settings = Some(VectorSettings::default());
    entry
}

fn interval_test_internal_seed(settings: &IntervalTestSettings) -> CutEntry {
    let mut entry = CutEntry::new(OperationType::Fill);
    entry.speed_mm_min = settings.speed_mm_min.max(1.0);
    entry.power_percent = settings.power_percent.clamp(0.0, 100.0);
    entry.raster_settings = Some(RasterSettings::default());
    entry
}

fn internal_seed_for_request(request: &QualityTestRequest) -> CutEntry {
    match request {
        QualityTestRequest::Material(_) => material_test_internal_seed(),
        QualityTestRequest::Focus(settings) => focus_test_internal_seed(settings),
        QualityTestRequest::Interval(settings) => interval_test_internal_seed(settings),
    }
}

fn profile_workspace(profile: &MachineProfile) -> Workspace {
    Workspace {
        bed_width_mm: profile.bed_width_mm,
        bed_height_mm: profile.bed_height_mm,
        origin: profile.origin,
    }
}

fn snapshot_context(
    ctx: &ServiceContext,
    request: &QualityTestRequest,
    profile: &MachineProfile,
) -> ServiceResult<ProjectContext> {
    let project_guard = ctx
        .project
        .lock()
        .map_err(|e| ServiceError::internal(format!("Failed to lock project: {e}")))?;
    if let Some(project) = project_guard.as_ref() {
        return Ok(ProjectContext {
            workspace: project.workspace.clone(),
            job_origin: project.job_origin,
            user_origin: project.user_origin,
            start_from: project.start_from,
            optimization: project.optimization.clone(),
            material_height_mm: project.material_height_mm,
            seed_entry: internal_seed_for_request(request),
        });
    }

    Ok(ProjectContext {
        workspace: profile_workspace(profile),
        job_origin: AnchorPoint::TopLeft,
        user_origin: None,
        start_from: StartFromMode::AbsoluteCoords,
        optimization: ProjectOptimization::default(),
        material_height_mm: None,
        seed_entry: internal_seed_for_request(request),
    })
}

fn active_machine_profile(ctx: &ServiceContext) -> ServiceResult<MachineProfile> {
    let settings = ctx
        .settings
        .lock()
        .map_err(|e| ServiceError::internal(format!("Failed to lock settings: {e}")))?;
    settings
        .active_profile_id
        .and_then(|id| settings.machine_profiles.iter().find(|p| p.id == id))
        .cloned()
        .ok_or_else(|| ServiceError::invalid_state("No active machine profile selected"))
}

/// Build a synthetic `Project` for the given request without touching active state.
pub fn build_synthetic_project(
    request: &QualityTestRequest,
    proj_ctx: &ProjectContext,
    machine_profile: &MachineProfile,
) -> (Project, Vec<QualityTestWarning>) {
    let mut warnings: Vec<QualityTestWarning> = Vec::new();

    let mut synthetic = Project::new("__quality_test_transient__");
    synthetic.workspace = proj_ctx.workspace.clone();
    synthetic.job_origin = proj_ctx.job_origin;
    synthetic.user_origin = proj_ctx.user_origin;
    // Copy the active project's transform stack and output policy so transient Frame/Start/export
    // land where the real job would land and finish where the user expects. The planner still
    // bypasses ordering passes via `QualityTestOrdering::RowMajor`; we only need the non-ordering
    // fields (finish_position, custom finish coords, start point, etc.).
    synthetic.start_from = proj_ctx.start_from;
    synthetic.optimization = proj_ctx.optimization.clone();
    synthetic.material_height_mm = proj_ctx.material_height_mm;
    synthetic.machine_profile_id = Some(machine_profile.id);
    synthetic.machine_profile_snapshot = Some(machine_profile.snapshot());

    let (sample_layers, sample_objects, label_objects) = match request {
        QualityTestRequest::Material(s) => generate_material_test(s, &proj_ctx.seed_entry),
        QualityTestRequest::Focus(s) => generate_focus_test(s, &proj_ctx.seed_entry),
        QualityTestRequest::Interval(s) => generate_interval_test(s, &proj_ctx.seed_entry),
    };

    // `perforated_labels` is a Focus Test annotation knob — when on, the dedicated label layer
    // uses perforation so labels stay readable without changing the configured label power.
    let mut layers = sample_layers;
    let mut objects: Vec<ProjectObject> = sample_objects;
    if !label_objects.is_empty() {
        let perforated_labels = matches!(
            request,
            QualityTestRequest::Focus(s) if s.perforated_labels
        );
        let label_entry = match request {
            QualityTestRequest::Material(s) => material_text_entry(&s.text_entry),
            QualityTestRequest::Focus(s) => {
                let focus_seed = focus_test_internal_seed(s);
                focus_label_entry(&focus_seed, perforated_labels)
            }
            _ => interval_label_entry(&proj_ctx.seed_entry),
        };
        let labels_layer_id = LayerRef::new();
        let labels_layer = Layer {
            id: labels_layer_id,
            name: LABELS_LAYER_NAME.to_string(),
            enabled: true,
            order_index: (layers.len() as u32) + 1,
            color_tag: ColorTag(LABEL_COLOR.to_string()),
            visible: true,
            is_tool_layer: false,
            entries: vec![label_entry],
        };
        layers.push(labels_layer);
        objects.extend(label_objects.into_iter().map(|mut obj| {
            obj.layer_id = labels_layer_id;
            obj
        }));
    }

    // Pre-flight bounds check using the union bbox of all sample geometry.
    if let Some(bbox) = union_bounds(&objects) {
        let bbox_w = bbox.max.x - bbox.min.x;
        let bbox_h = bbox.max.y - bbox.min.y;
        if bbox_w > machine_profile.bed_width_mm || bbox_h > machine_profile.bed_height_mm {
            warnings.push(QualityTestWarning::BoundsExceeded {
                bbox_w_mm: bbox_w,
                bbox_h_mm: bbox_h,
                bed_w_mm: machine_profile.bed_width_mm,
                bed_h_mm: machine_profile.bed_height_mm,
            });
        }
    }

    synthetic.layers = layers;
    synthetic.objects = objects;
    (synthetic, warnings)
}

fn focus_label_entry(seed: &CutEntry, perforated: bool) -> CutEntry {
    let mut e = CutEntry::new(OperationType::Line);
    e.speed_mm_min = seed.speed_mm_min;
    e.air_assist = false;
    e.z_offset_mm = 0.0;
    e.power_percent = seed.power_percent.clamp(0.0, 100.0);
    let mut vs = VectorSettings::default();
    if perforated {
        vs.perforation_enabled = true;
    }
    e.vector_settings = Some(vs);
    e
}

fn interval_label_entry(seed: &CutEntry) -> CutEntry {
    let mut e = CutEntry::new(OperationType::Line);
    e.speed_mm_min = seed.speed_mm_min;
    e.air_assist = false;
    e.z_offset_mm = 0.0;
    e.power_percent = (seed.power_percent * 0.5).clamp(5.0, 100.0);
    e.vector_settings = Some(VectorSettings::default());
    e
}

/// Compute bounding box union over all `ProjectObject` bounds.
fn union_bounds(objects: &[ProjectObject]) -> Option<Bounds> {
    let mut iter = objects.iter();
    let first = iter.next()?;
    let mut min = first.bounds.min;
    let mut max = first.bounds.max;
    for obj in iter {
        if obj.bounds.min.x < min.x {
            min.x = obj.bounds.min.x;
        }
        if obj.bounds.min.y < min.y {
            min.y = obj.bounds.min.y;
        }
        if obj.bounds.max.x > max.x {
            max.x = obj.bounds.max.x;
        }
        if obj.bounds.max.y > max.y {
            max.y = obj.bounds.max.y;
        }
    }
    Some(Bounds::new(min, max))
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

// ---------- Material Test (speed × power grid) ----------

#[derive(Debug, Clone)]
struct MaterialCell {
    row: u32,
    col: u32,
    grid_index: u32,
    x_value: f64,
    y_value: f64,
    entry: CutEntry,
}

fn generate_material_test(
    settings: &MaterialTestSettings,
    _seed: &CutEntry,
) -> (Vec<Layer>, Vec<ProjectObject>, Vec<ProjectObject>) {
    let mut sample_layers = Vec::new();
    let mut sample_objects = Vec::new();
    let mut label_objects = Vec::new();

    let cw = settings.cell_w_mm.max(1.0);
    let ch = settings.cell_h_mm.max(1.0);
    let gap = settings.cell_spacing_mm.max(0.0);
    let x_axis = normalize_material_axis(&settings.x_axis, &settings.sample_entry);
    let y_axis = normalize_material_axis(&settings.y_axis, &settings.sample_entry);
    let cols = x_axis.count.clamp(1, 10);
    let rows = y_axis.count.clamp(1, 10);
    let grid_w = cols as f64 * cw + (cols.saturating_sub(1)) as f64 * gap;
    let grid_h = rows as f64 * ch + (rows.saturating_sub(1)) as f64 * gap;
    let grid_x = if settings.enable_text { 24.0 } else { 0.0 };
    let grid_y = if settings.enable_text {
        LABEL_FONT_SIZE_MM * 2.0 + LABEL_GAP_MM * 4.0
    } else {
        0.0
    };
    let mut cells = Vec::new();

    for row in 0..rows {
        let y_value = material_axis_value(&y_axis, row);
        let row_y = grid_y + (row as f64) * (ch + gap);
        if settings.enable_text {
            label_objects.push(make_text_label(
                grid_x - LABEL_GAP_MM,
                row_y + ch * 0.5,
                &format_material_axis_label(y_axis.param, y_value),
                TextAlignment::Right,
                TextAlignmentV::Middle,
            ));
        }

        for col in 0..cols {
            let x_value = material_axis_value(&x_axis, col);
            let col_x = grid_x + (col as f64) * (cw + gap);

            if settings.enable_text && row == 0 {
                label_objects.push(make_text_label(
                    col_x + cw * 0.5,
                    grid_y - LABEL_GAP_MM,
                    &format_material_axis_label(x_axis.param, x_value),
                    TextAlignment::Center,
                    TextAlignmentV::Bottom,
                ));
            }

            let mut entry = normalize_material_sample_entry(settings.sample_entry.clone());
            apply_material_axis_value(&mut entry, y_axis.param, y_value);
            apply_material_axis_value(&mut entry, x_axis.param, x_value);
            let grid_index = row * cols + col;
            cells.push(MaterialCell {
                row,
                col,
                grid_index,
                x_value,
                y_value,
                entry,
            });
        }
    }

    if settings.enable_text {
        label_objects.push(make_text_label(
            grid_x + grid_w * 0.5,
            LABEL_GAP_MM + LABEL_FONT_SIZE_MM,
            &format!(
                "X {} / Y {}",
                material_axis_name(x_axis.param),
                material_axis_name(y_axis.param)
            ),
            TextAlignment::Center,
            TextAlignmentV::Bottom,
        ));
    }

    cells.sort_by(material_cell_order);
    for (order, cell) in cells.into_iter().enumerate() {
        let col_x = grid_x + (cell.col as f64) * (cw + gap);
        let row_y = grid_y + (cell.row as f64) * (ch + gap);
        let layer_id = LayerRef::new();
        let object = if material_operation_uses_filled_rect(cell.entry.operation) {
            make_filled_rect(layer_id, col_x, row_y, cw, ch, cell.grid_index as i32)
        } else {
            make_outline_rect(layer_id, col_x, row_y, cw, ch, cell.grid_index as i32)
        };
        sample_layers.push(Layer {
            id: layer_id,
            name: format!(
                "{}/r{}c{}-{}-{}",
                SAMPLES_LAYER_PREFIX,
                cell.row,
                cell.col,
                format_material_axis_label(x_axis.param, cell.x_value),
                format_material_axis_label(y_axis.param, cell.y_value)
            ),
            enabled: true,
            order_index: order as u32,
            color_tag: ColorTag(SAMPLE_COLOR.to_string()),
            visible: true,
            is_tool_layer: false,
            entries: vec![cell.entry],
        });
        sample_objects.push(object);
    }

    if settings.enable_border {
        let layer_id = LayerRef::new();
        sample_layers.push(Layer {
            id: layer_id,
            name: "QT/Border".to_string(),
            enabled: true,
            order_index: sample_layers.len() as u32,
            color_tag: ColorTag(LABEL_COLOR.to_string()),
            visible: true,
            is_tool_layer: false,
            entries: vec![material_border_entry(&settings.border_entry)],
        });
        sample_objects.push(make_outline_rect(
            layer_id,
            grid_x,
            grid_y,
            grid_w,
            grid_h,
            i32::MAX - 1,
        ));
    }

    (sample_layers, sample_objects, label_objects)
}

fn normalize_material_axis(axis: &MaterialTestAxis, sample_entry: &CutEntry) -> MaterialTestAxis {
    let mut axis = axis.clone();
    axis.count = axis.count.clamp(1, 10);
    if axis.param == MaterialTestAxisParam::Interval
        && !sample_entry.operation.uses_raster_settings()
    {
        axis.param = MaterialTestAxisParam::Speed;
    }
    axis
}

fn material_axis_value(axis: &MaterialTestAxis, index: u32) -> f64 {
    if axis.count > 1 {
        let t = index as f64 / (axis.count - 1) as f64;
        lerp(axis.min, axis.max, t)
    } else {
        lerp(axis.min, axis.max, 0.5)
    }
}

fn normalize_material_sample_entry(mut entry: CutEntry) -> CutEntry {
    if entry.operation == OperationType::Image {
        entry.operation = OperationType::Fill;
    }
    if entry.operation.is_tool() {
        entry.operation = OperationType::Fill;
    }
    if entry.operation.uses_raster_settings() && entry.raster_settings.is_none() {
        entry.raster_settings = Some(RasterSettings::default());
    }
    if !entry.operation.uses_raster_settings() {
        entry.raster_settings = None;
    }
    if entry.operation.uses_vector_settings() && entry.vector_settings.is_none() {
        entry.vector_settings = Some(VectorSettings::default());
    }
    if !entry.operation.uses_vector_settings() {
        entry.vector_settings = None;
    }
    entry.air_assist = false;
    entry.output_enabled = true;
    entry
}

fn apply_material_axis_value(entry: &mut CutEntry, param: MaterialTestAxisParam, value: f64) {
    match param {
        MaterialTestAxisParam::Speed => {
            entry.speed_mm_min = value.max(1.0);
        }
        MaterialTestAxisParam::Power => {
            entry.power_percent = value.clamp(0.0, 100.0);
        }
        MaterialTestAxisParam::Interval => {
            let interval = value.max(0.01);
            let rs = entry
                .raster_settings
                .get_or_insert_with(RasterSettings::default);
            rs.line_interval_mm = interval;
            rs.dpi = (25.4 / interval).round().max(1.0) as u32;
        }
        MaterialTestAxisParam::Passes => {
            let passes = value.round().max(1.0) as u32;
            if entry.operation.uses_raster_settings() {
                let rs = entry
                    .raster_settings
                    .get_or_insert_with(RasterSettings::default);
                rs.passes = passes;
            }
            if entry.operation.uses_vector_settings() {
                let vs = entry
                    .vector_settings
                    .get_or_insert_with(VectorSettings::default);
                vs.passes = passes;
            }
        }
    }
}

fn material_operation_uses_filled_rect(operation: OperationType) -> bool {
    matches!(
        operation,
        OperationType::Fill | OperationType::Image | OperationType::OffsetFill
    )
}

fn material_entry_interval(entry: &CutEntry) -> f64 {
    entry
        .raster_settings
        .as_ref()
        .map(|rs| rs.effective_line_interval_mm())
        .unwrap_or(f64::INFINITY)
}

/// Safest samples run first: fastest speed, lowest power, widest interval, fewest passes, then
/// stable grid order. In tuple terms this is `(-speed, power, interval, passes, grid_index)`.
fn material_cell_order(a: &MaterialCell, b: &MaterialCell) -> std::cmp::Ordering {
    b.entry
        .speed_mm_min
        .total_cmp(&a.entry.speed_mm_min)
        .then_with(|| a.entry.power_percent.total_cmp(&b.entry.power_percent))
        .then_with(|| {
            material_entry_interval(&a.entry).total_cmp(&material_entry_interval(&b.entry))
        })
        .then_with(|| {
            a.entry
                .passes_for_operation()
                .cmp(&b.entry.passes_for_operation())
        })
        .then_with(|| a.grid_index.cmp(&b.grid_index))
}

fn material_axis_name(param: MaterialTestAxisParam) -> &'static str {
    match param {
        MaterialTestAxisParam::Speed => "Speed",
        MaterialTestAxisParam::Power => "Power",
        MaterialTestAxisParam::Interval => "Interval",
        MaterialTestAxisParam::Passes => "Passes",
    }
}

fn format_material_axis_label(param: MaterialTestAxisParam, value: f64) -> String {
    match param {
        MaterialTestAxisParam::Speed => format!("S{value:.0}"),
        MaterialTestAxisParam::Power => format!("P{value:.0}"),
        MaterialTestAxisParam::Interval => format!("I{value:.3}"),
        MaterialTestAxisParam::Passes => format!("Pass{:.0}", value.round().max(1.0)),
    }
}

fn material_text_entry(seed: &CutEntry) -> CutEntry {
    let mut entry = seed.clone();
    entry.operation = OperationType::Line;
    entry.raster_settings = None;
    if entry.vector_settings.is_none() {
        entry.vector_settings = Some(VectorSettings::default());
    }
    entry.air_assist = false;
    entry.output_enabled = true;
    entry
}

fn material_border_entry(seed: &CutEntry) -> CutEntry {
    material_text_entry(seed)
}

// ---------- Focus Test (Z sweep) ----------

fn generate_focus_test(
    settings: &FocusTestSettings,
    seed: &CutEntry,
) -> (Vec<Layer>, Vec<ProjectObject>, Vec<ProjectObject>) {
    let mut sample_layers = Vec::new();
    let mut sample_objects = Vec::new();
    let mut label_objects = Vec::new();

    let intervals = settings.intervals.max(1);
    let line_count = intervals + 1;
    let line_len = settings.line_length_mm.max(1.0);
    let row_h = settings.step_spacing_mm.max(1.0);
    let top_margin = LABEL_FONT_SIZE_MM + LABEL_GAP_MM;

    for i in 0..line_count {
        let t = i as f64 / intervals as f64;
        let z_step = lerp(settings.z_min_mm, settings.z_max_mm, t);
        let y = top_margin + (i as f64) * row_h;

        // One layer per Z step. Sample lines burn at the Focus Test's configured speed/power;
        // `perforated_labels` affects only the dedicated label-layer entry built later.
        let mut entry = seed.clone();
        entry.id = CutEntryId::new();
        // Store the tested offset only; absolute material-height base is applied in G-code output.
        entry.z_offset_mm = z_step;
        entry.operation = OperationType::Line;
        entry.speed_mm_min = settings.speed_mm_min;
        entry.power_percent = settings.power_percent.clamp(0.0, 100.0);
        entry.power_min_percent = entry.power_percent;
        entry.vector_settings = Some(VectorSettings::default());
        entry.raster_settings = None;

        let layer_id = LayerRef::new();
        sample_layers.push(Layer {
            id: layer_id,
            name: format!("{}/z{:.3}", SAMPLES_LAYER_PREFIX, z_step),
            enabled: true,
            order_index: i,
            color_tag: ColorTag(SAMPLE_COLOR.to_string()),
            visible: true,
            is_tool_layer: false,
            entries: vec![entry],
        });
        sample_objects.push(make_horizontal_line(layer_id, 0.0, y, line_len, i as i32));
        label_objects.push(make_text_label(
            line_len + LABEL_GAP_MM,
            y,
            &format!("Z{:+.2}", z_step),
            TextAlignment::Left,
            TextAlignmentV::Middle,
        ));
    }

    (sample_layers, sample_objects, label_objects)
}

// ---------- Interval Test (line interval sweep) ----------

fn generate_interval_test(
    settings: &IntervalTestSettings,
    seed: &CutEntry,
) -> (Vec<Layer>, Vec<ProjectObject>, Vec<ProjectObject>) {
    let mut sample_layers = Vec::new();
    let mut sample_objects = Vec::new();
    let mut label_objects = Vec::new();

    let steps = settings.steps.max(2);
    let cw = settings.cell_w_mm.max(1.0);
    let ch = settings.cell_h_mm.max(1.0);
    let gap = settings.cell_spacing_mm.max(0.0);
    let grid_y = LABEL_FONT_SIZE_MM + LABEL_GAP_MM;

    for i in 0..steps {
        let t = i as f64 / (steps - 1) as f64;
        let interval = lerp(settings.interval_min_mm, settings.interval_max_mm, t);
        let x = (i as f64) * (cw + gap);
        let y = grid_y;

        let mut entry = seed.clone();
        entry.id = CutEntryId::new();
        entry.operation = OperationType::Fill;
        entry.speed_mm_min = settings.speed_mm_min;
        entry.power_percent = settings.power_percent.clamp(0.0, 100.0);
        entry.power_min_percent = entry.power_percent;
        let mut rs = entry.raster_settings.unwrap_or_default();
        rs.line_interval_mm = interval;
        rs.dpi = (25.4 / interval).round().max(1.0) as u32;
        entry.raster_settings = Some(rs);
        entry.vector_settings = None;

        let layer_id = LayerRef::new();
        sample_layers.push(Layer {
            id: layer_id,
            name: format!("{}/i{:.3}", SAMPLES_LAYER_PREFIX, interval),
            enabled: true,
            order_index: i,
            color_tag: ColorTag(SAMPLE_COLOR.to_string()),
            visible: true,
            is_tool_layer: false,
            entries: vec![entry],
        });
        sample_objects.push(make_filled_rect(layer_id, x, y, cw, ch, i as i32));
        label_objects.push(make_text_label(
            x + cw * 0.5,
            y - LABEL_GAP_MM,
            &format!("{:.2}", interval),
            TextAlignment::Center,
            TextAlignmentV::Bottom,
        ));
    }

    (sample_layers, sample_objects, label_objects)
}

// ---------- Object construction helpers ----------

fn make_filled_rect(
    layer_id: LayerRef,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    z_index: i32,
) -> ProjectObject {
    let path_data = format!(
        "M{x} {y} L{x_end} {y} L{x_end} {y_end} L{x} {y_end} Z",
        x = x,
        y = y,
        x_end = x + w,
        y_end = y + h,
    );
    let mut obj = ProjectObject::new(
        format!("qt_sample_{}", z_index),
        layer_id,
        Bounds::new(Point2D::new(x, y), Point2D::new(x + w, y + h)),
        ObjectData::VectorPath {
            path_data,
            closed: true,
            ruler_guide_axis: None,
        },
    );
    obj.z_index = z_index;
    obj
}

fn make_outline_rect(
    layer_id: LayerRef,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    z_index: i32,
) -> ProjectObject {
    make_filled_rect(layer_id, x, y, w, h, z_index)
}

fn make_horizontal_line(
    layer_id: LayerRef,
    x: f64,
    y: f64,
    len: f64,
    z_index: i32,
) -> ProjectObject {
    let path_data = format!("M{x} {y} L{x_end} {y}", x = x, y = y, x_end = x + len);
    let mut obj = ProjectObject::new(
        format!("qt_sample_{}", z_index),
        layer_id,
        Bounds::new(Point2D::new(x, y), Point2D::new(x + len, y)),
        ObjectData::VectorPath {
            path_data,
            closed: false,
            ruler_guide_axis: None,
        },
    );
    obj.z_index = z_index;
    obj
}

fn make_text_label(
    x: f64,
    y: f64,
    content: &str,
    h_align: TextAlignment,
    v_align: TextAlignmentV,
) -> ProjectObject {
    // Rough bounds: half a line of text wide per character, plus small height.
    let w = (content.len() as f64) * LABEL_FONT_SIZE_MM * 0.6;
    let h = LABEL_FONT_SIZE_MM;
    let (anchor_x, anchor_y) = (x, y);
    let bounds_min = Point2D::new(anchor_x - w * 0.5, anchor_y - h * 0.5);
    let bounds_max = Point2D::new(anchor_x + w * 0.5, anchor_y + h * 0.5);
    let mut obj = ProjectObject::new(
        format!("qt_label_{}_{}", anchor_x as i32, anchor_y as i32),
        LayerRef::new(),
        Bounds::new(bounds_min, bounds_max),
        ObjectData::Text {
            content: content.to_string(),
            font_family: LABEL_FONT_FAMILY.to_string(),
            font_size_mm: LABEL_FONT_SIZE_MM,
            alignment: h_align,
            alignment_v: v_align,
            bold: false,
            italic: false,
            upper_case: false,
            welded: false,
            h_spacing: 0.0,
            v_spacing: 0.0,
            on_path: false,
            path_offset: 0.0,
            distort: false,
            layout_mode: TextLayoutMode::default(),
            rtl: false,
            bend_radius: 0.0,
            transform_style: beambench_core::TextTransformStyle::None,
            transform_curve: 0.0,
            circle_placement: beambench_core::TextCirclePlacement::TopOutside,
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
    obj.z_index = i32::MAX; // Labels sort last.
    obj
}

// ---------- Plan / preview / G-code building ----------

fn build_transient_plan(
    synthetic: &Project,
    profile: &MachineProfile,
) -> ServiceResult<ExecutionPlan> {
    let calibration = output::build_planner_calibration(profile);
    let input = PlannerInput::new(
        synthetic.optimization.clone(),
        OptimizationRuntime::default(),
        calibration,
    )
    .with_row_major_ordering();
    build_plan_with_input(synthetic, &input)
        .map_err(|e| ServiceError::invalid_state(format!("Quality-test plan build failed: {e}")))
}

/// Produce a transient preview without touching plan cache or project state.
pub fn quality_test_preview(
    ctx: &ServiceContext,
    request: &QualityTestRequest,
) -> ServiceResult<TransientPreviewResponse> {
    let profile = active_machine_profile(ctx)?;
    let proj_ctx = snapshot_context(ctx, request, &profile)?;
    gate_focus_material_height_for_context(request, &proj_ctx)?;
    let (synthetic, warnings) = build_synthetic_project(request, &proj_ctx, &profile);
    let plan = build_placed_transient_plan(&synthetic, &profile, request, false)?;
    let preview = distill_preview(&plan);
    Ok(TransientPreviewResponse { preview, warnings })
}

/// Generate G-code for a transient quality-test job and write it to disk.
pub fn quality_test_export_gcode(
    ctx: &ServiceContext,
    request: &QualityTestRequest,
    path: &std::path::Path,
) -> ServiceResult<TransientExportResponse> {
    let profile = active_machine_profile(ctx)?;
    let proj_ctx = snapshot_context(ctx, request, &profile)?;
    gate_focus_material_height_for_context(request, &proj_ctx)?;
    let (synthetic, warnings) = build_synthetic_project(request, &proj_ctx, &profile);
    let plan = build_placed_transient_plan(&synthetic, &profile, request, false)?;
    let config = quality_test_gcode_config(&synthetic, &profile, request);
    let lines = generate_gcode(&plan, &config)
        .map_err(|e| ServiceError::invalid_state(format!("G-code generation failed: {e}")))?;
    std::fs::write(path, lines.join("\n"))
        .map_err(|e| ServiceError::internal(format!("Failed to write G-code: {e}")))?;
    Ok(TransientExportResponse {
        path: path.to_path_buf(),
        warnings,
    })
}

fn quality_test_gcode_config(
    synthetic: &Project,
    profile: &MachineProfile,
    request: &QualityTestRequest,
) -> GcodeConfig {
    let mut config = output::build_gcode_config(&synthetic.optimization, profile);
    output::apply_project_gcode_metadata(&mut config, synthetic);
    if let QualityTestRequest::Focus(settings) = request {
        config.z_mode = settings.mode;
    }
    config
}

fn quality_test_frame_gcode_config(
    synthetic: &Project,
    profile: &MachineProfile,
    request: &QualityTestRequest,
) -> GcodeConfig {
    let mut config = quality_test_gcode_config(synthetic, profile, request);
    config.z_moves_enabled = false;
    config.z_offset_cut_entry_ids.clear();
    config
}

fn gate_focus_material_height_for_context(
    request: &QualityTestRequest,
    proj_ctx: &ProjectContext,
) -> ServiceResult<()> {
    if matches!(
        request,
        QualityTestRequest::Focus(FocusTestSettings {
            mode: FocusTestZMode::AbsoluteWorkCoord,
            ..
        })
    ) && proj_ctx.material_height_mm.is_none()
    {
        return Err(ServiceError::invalid_state(
            "absolute Z mode requires Project.material_height_mm to be set",
        ));
    }
    Ok(())
}

/// Materialize a generated Material Test as real project layers and objects.
///
/// Preview/Frame/Start remain transient so tests do not modify the project.
/// This canvas action is a Beam Bench enhancement for users who want to inspect, move, edit,
/// save, or run the generated grid like ordinary artwork.
pub fn quality_test_create_material_on_canvas(
    ctx: &ServiceContext,
    settings: &MaterialTestSettings,
) -> Result<CanvasMaterialTestResponse, QualityTestError> {
    let request = QualityTestRequest::Material(settings.clone());
    let profile =
        active_machine_profile(ctx).map_err(|_| QualityTestError::NoActiveMachineProfile)?;
    let proj_ctx =
        snapshot_context(ctx, &request, &profile).map_err(|e| QualityTestError::Internal {
            message: e.to_string(),
        })?;
    let (mut synthetic, warnings) = build_synthetic_project(&request, &proj_ctx, &profile);
    place_synthetic_project_on_canvas(&mut synthetic, &request);
    if union_bounds(&synthetic.objects).is_none() {
        return Err(no_geometry_error());
    }
    append_synthetic_project_to_canvas(ctx, synthetic, warnings)
}

/// Apply the synthetic project's job-origin and start-from transforms to a freshly-built plan.
/// Mirrors `machine::frame_job`'s transform sequence so quality-test motion lands where the real
/// engraving will burn on the bed. The plan arrives in machine space (the builder applies the
/// workspace transform internally but skips anchoring because the transient runtime carries no
/// live position), so the anchor translation happens here with the real position.
fn apply_project_transforms(plan: &mut ExecutionPlan, synthetic: &Project, ctx: &ServiceContext) {
    let current_pos = ctx
        .optimization_runtime
        .lock()
        .ok()
        .and_then(|g| g.current_position);
    if synthetic.start_from == StartFromMode::UserOrigin && synthetic.user_origin.is_none() {
        // No offset
    } else if synthetic.start_from == StartFromMode::CurrentPosition && current_pos.is_none() {
        // No offset
    } else if synthetic.start_from != StartFromMode::AbsoluteCoords {
        let y_flipped =
            synthetic.workspace.origin != beambench_core::workspace::WorkspaceOrigin::TopLeft;
        let content_bounds = plan.bounds;
        let target = if synthetic.start_from == StartFromMode::CurrentPosition {
            current_pos.unwrap_or((0.0, 0.0))
        } else {
            synthetic.user_origin.unwrap_or((0.0, 0.0))
        };
        anchor_segments_to_target(
            &mut plan.segments,
            synthetic.job_origin,
            &content_bounds,
            y_flipped,
            target,
        );
    }
    plan.bounds = calculate_plan_bounds(&plan.segments);
}

fn material_center(request: &QualityTestRequest) -> Option<(f64, f64)> {
    match request {
        QualityTestRequest::Material(settings) if settings.absolute_center_enabled => {
            Some((settings.x_center_mm, settings.y_center_mm))
        }
        _ => None,
    }
}

fn bounds_exceeded_error(bounds: &Bounds, profile: &MachineProfile) -> Option<QualityTestError> {
    let bbox_w = bounds.width();
    let bbox_h = bounds.height();
    let eps = 1e-6;
    if bbox_w > profile.bed_width_mm + eps
        || bbox_h > profile.bed_height_mm + eps
        || bounds.min.x < -eps
        || bounds.min.y < -eps
        || bounds.max.x > profile.bed_width_mm + eps
        || bounds.max.y > profile.bed_height_mm + eps
    {
        Some(QualityTestError::BoundsExceeded {
            bbox_w_mm: bbox_w,
            bbox_h_mm: bbox_h,
            bed_w_mm: profile.bed_width_mm,
            bed_h_mm: profile.bed_height_mm,
        })
    } else {
        None
    }
}

fn no_geometry_error() -> QualityTestError {
    QualityTestError::Internal {
        message: "No geometry generated".to_string(),
    }
}

fn translate_bounds(bounds: Bounds, dx: f64, dy: f64) -> Bounds {
    Bounds::new(
        Point2D::new(bounds.min.x + dx, bounds.min.y + dy),
        Point2D::new(bounds.max.x + dx, bounds.max.y + dy),
    )
}

fn translate_object(object: &mut ProjectObject, dx: f64, dy: f64) {
    object.bounds = translate_bounds(object.bounds, dx, dy);
}

fn translate_project_objects(objects: &mut [ProjectObject], dx: f64, dy: f64) {
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    for object in objects {
        translate_object(object, dx, dy);
    }
}

fn place_synthetic_project_on_canvas(synthetic: &mut Project, request: &QualityTestRequest) {
    if let Some((x, y)) = material_center(request) {
        if let Some(bounds) = union_bounds(&synthetic.objects) {
            let center_x = bounds.min.x + bounds.width() * 0.5;
            let center_y = bounds.min.y + bounds.height() * 0.5;
            translate_project_objects(&mut synthetic.objects, x - center_x, y - center_y);
        }
    }
}

fn invalidate_plan_for_quality_test(ctx: &ServiceContext) -> Result<(), QualityTestError> {
    let mut cache_guard = ctx
        .plan_cache
        .lock()
        .map_err(|e| QualityTestError::Internal {
            message: format!("Failed to lock plan_cache: {e}"),
        })?;
    *cache_guard = None;
    Ok(())
}

fn append_synthetic_project_to_canvas(
    ctx: &ServiceContext,
    mut synthetic: Project,
    warnings: Vec<QualityTestWarning>,
) -> Result<CanvasMaterialTestResponse, QualityTestError> {
    let created_object_ids = synthetic
        .objects
        .iter()
        .map(|object| object.id)
        .collect::<Vec<_>>();
    let created_layer_ids = synthetic
        .layers
        .iter()
        .map(|layer| layer.id)
        .collect::<Vec<_>>();
    let mut project_guard = ctx.project.lock().map_err(|e| QualityTestError::Internal {
        message: format!("Failed to lock project: {e}"),
    })?;

    let project = if let Some(project) = project_guard.as_mut() {
        ctx.push_project_undo_snapshot(project)
            .map_err(|message| QualityTestError::Internal { message })?;
        let layer_offset = project.layers.len() as u32;
        for (index, layer) in synthetic.layers.iter_mut().enumerate() {
            layer.order_index = layer_offset + index as u32;
        }
        project.layers.extend(synthetic.layers);
        project.objects.extend(synthetic.objects);
        crate::ops::project::refresh_project_text_caches(project);
        project.dirty = true;
        project.clone()
    } else {
        let mut project = Project::new("Material Test");
        project.workspace = synthetic.workspace;
        project.machine_profile_id = synthetic.machine_profile_id;
        project.machine_profile_snapshot = synthetic.machine_profile_snapshot;
        project.layers = synthetic.layers;
        project.objects = synthetic.objects;
        project.start_from = StartFromMode::AbsoluteCoords;
        project.job_origin = AnchorPoint::TopLeft;
        project.user_origin = None;
        crate::ops::project::refresh_project_text_caches(&mut project);
        project.dirty = true;
        *project_guard = Some(project.clone());
        drop(project_guard);
        {
            let mut path_guard =
                ctx.project_path
                    .lock()
                    .map_err(|e| QualityTestError::Internal {
                        message: format!("Failed to lock project_path: {e}"),
                    })?;
            *path_guard = None;
        }
        ctx.clear_project_history()
            .map_err(|message| QualityTestError::Internal { message })?;
        ctx.emit_event(
            "project.created",
            serde_json::json!({
                "project": crate::events::project_summary(&project, None),
            }),
        );
        invalidate_plan_for_quality_test(ctx)?;
        return Ok(CanvasMaterialTestResponse {
            project,
            warnings,
            created_object_ids,
            created_layer_ids,
        });
    };
    drop(project_guard);
    invalidate_plan_for_quality_test(ctx)?;
    ctx.emit_event(
        "project.quality_test.created",
        serde_json::json!({
            "project_id": project.metadata.project_id,
            "object_ids": created_object_ids,
            "layer_ids": created_layer_ids,
        }),
    );
    Ok(CanvasMaterialTestResponse {
        project,
        warnings,
        created_object_ids,
        created_layer_ids,
    })
}

fn neutral_placement_project(synthetic: &Project) -> Project {
    let mut project = synthetic.clone();
    project.job_origin = AnchorPoint::TopLeft;
    project.user_origin = None;
    project.start_from = StartFromMode::AbsoluteCoords;
    project
}

fn build_placed_transient_plan(
    synthetic: &Project,
    profile: &MachineProfile,
    request: &QualityTestRequest,
    apply_project_during_planning: bool,
) -> ServiceResult<ExecutionPlan> {
    let neutralize_project_placement =
        material_center(request).is_some() || !apply_project_during_planning;
    let neutral_project;
    let plan_project = if neutralize_project_placement {
        neutral_project = neutral_placement_project(synthetic);
        &neutral_project
    } else {
        synthetic
    };
    let mut plan = build_transient_plan(plan_project, profile)?;
    if let Some((x, y)) = material_center(request) {
        translate_plan_to_center(&mut plan, x, y);
    }
    Ok(plan)
}

fn place_quality_test_frame_plan(
    plan: &mut ExecutionPlan,
    synthetic: &Project,
    ctx: &ServiceContext,
    request: &QualityTestRequest,
) {
    if let Some((x, y)) = material_center(request) {
        translate_plan_to_center(plan, x, y);
    } else {
        apply_project_transforms(plan, synthetic, ctx);
    }
}

fn translate_plan_to_center(plan: &mut ExecutionPlan, x: f64, y: f64) {
    let bounds = calculate_plan_bounds(&plan.segments);
    let center_x = bounds.min.x + bounds.width() * 0.5;
    let center_y = bounds.min.y + bounds.height() * 0.5;
    translate_plan(plan, x - center_x, y - center_y);
}

fn translate_plan(plan: &mut ExecutionPlan, dx: f64, dy: f64) {
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    for segment in plan.segments.iter_mut() {
        translate_segment(segment, dx, dy);
    }
    plan.bounds = calculate_plan_bounds(&plan.segments);
}

fn translate_point(point: &mut Point2D, dx: f64, dy: f64) {
    point.x += dx;
    point.y += dy;
}

fn translate_segment(segment: &mut PlanSegment, dx: f64, dy: f64) {
    match segment {
        PlanSegment::Travel { start, end } => {
            translate_point(start, dx, dy);
            translate_point(end, dx, dy);
        }
        PlanSegment::Vector { polyline, .. } => {
            for point in polyline.iter_mut() {
                translate_point(point, dx, dy);
            }
        }
        PlanSegment::Raster {
            scanlines,
            scan_origin,
            outlines,
            scan_axis,
            ..
        } => {
            translate_point(scan_origin, dx, dy);
            for outline in outlines.iter_mut() {
                for point in outline.points.iter_mut() {
                    translate_point(point, dx, dy);
                }
            }
            match scan_axis {
                beambench_planner::ScanAxis::Horizontal => {
                    for scanline in scanlines.iter_mut() {
                        scanline.y_mm += dy;
                        for run in scanline.runs.iter_mut() {
                            run.start_x_mm += dx;
                            run.end_x_mm += dx;
                        }
                    }
                }
                beambench_planner::ScanAxis::Vertical => {
                    for scanline in scanlines.iter_mut() {
                        scanline.y_mm += dx;
                        for run in scanline.runs.iter_mut() {
                            run.start_x_mm += dy;
                            run.end_x_mm += dy;
                        }
                    }
                }
            }
        }
        PlanSegment::Frame { path, .. } => {
            for point in path.iter_mut() {
                translate_point(point, dx, dy);
            }
        }
        PlanSegment::OffsetFill { .. } => {
            unreachable!("OffsetFill placeholder segments must be expanded before placement")
        }
    }
}

/// Frame the union bounding box of a generated quality-test job through the active session.
/// Builds a transient frame plan, applies the synthetic project's transforms, and streams it.
pub fn quality_test_frame(
    ctx: &ServiceContext,
    request: &QualityTestRequest,
) -> Result<JobProgress, QualityTestError> {
    let mut job_lock = ctx
        .job
        .lock()
        .map_err(|_| QualityTestError::JobInProgress)?;
    if job_lock.is_some() {
        return Err(QualityTestError::JobInProgress);
    }
    gate_focus_test(ctx, request, true)?;
    let profile =
        active_machine_profile(ctx).map_err(|_| QualityTestError::NoActiveMachineProfile)?;
    let proj_ctx =
        snapshot_context(ctx, request, &profile).map_err(|e| QualityTestError::Internal {
            message: e.to_string(),
        })?;
    let (synthetic, _warnings) = build_synthetic_project(request, &proj_ctx, &profile);
    let bbox = union_bounds(&synthetic.objects).ok_or_else(no_geometry_error)?;
    if let Some(error) = bounds_exceeded_error(&bbox, &profile) {
        return Err(error);
    }

    // Build a rectangular frame plan around the union bbox at low power, then transform.
    let mut frame_plan = build_frame_plan(&bbox, 1.0, 3000.0);
    place_quality_test_frame_plan(&mut frame_plan, &synthetic, ctx, request);
    if let Some(error) = bounds_exceeded_error(&frame_plan.bounds, &profile) {
        return Err(error);
    }
    let gcode_config = quality_test_frame_gcode_config(&synthetic, &profile, request);
    stream_job(
        ctx,
        &mut job_lock,
        frame_plan,
        gcode_config,
        &synthetic,
        &profile,
        "job.frame.started",
    )
    .map_err(|e| QualityTestError::Internal {
        message: e.to_string(),
    })
}

/// Start a transient quality-test job through the active session. Builds the synthetic project,
/// runs it through the planner with row-major ordering, and streams the resulting plan.
pub fn quality_test_start(
    ctx: &ServiceContext,
    request: &QualityTestRequest,
) -> Result<JobProgress, QualityTestError> {
    let mut job_lock = ctx
        .job
        .lock()
        .map_err(|_| QualityTestError::JobInProgress)?;
    if job_lock.is_some() {
        return Err(QualityTestError::JobInProgress);
    }
    gate_focus_test(ctx, request, true)?;
    let profile =
        active_machine_profile(ctx).map_err(|_| QualityTestError::NoActiveMachineProfile)?;
    let proj_ctx =
        snapshot_context(ctx, request, &profile).map_err(|e| QualityTestError::Internal {
            message: e.to_string(),
        })?;
    let (synthetic, _warnings) = build_synthetic_project(request, &proj_ctx, &profile);
    let bbox = union_bounds(&synthetic.objects).ok_or_else(no_geometry_error)?;
    if let Some(error) = bounds_exceeded_error(&bbox, &profile) {
        return Err(error);
    }
    let plan = build_placed_transient_plan(&synthetic, &profile, request, true).map_err(|e| {
        QualityTestError::Internal {
            message: e.to_string(),
        }
    })?;
    if let Some(error) = bounds_exceeded_error(&plan.bounds, &profile) {
        return Err(error);
    }
    let gcode_config = quality_test_gcode_config(&synthetic, &profile, request);
    stream_job(
        ctx,
        &mut job_lock,
        plan,
        gcode_config,
        &synthetic,
        &profile,
        "job.started",
    )
    .map_err(|e| QualityTestError::Internal {
        message: e.to_string(),
    })
}

/// Shared helper that hands a plan to the active session and registers it as the current job.
/// Mirrors the controller dispatch in `machine::start_job`.
fn stream_job(
    ctx: &ServiceContext,
    job_slot: &mut Option<ActiveJobHandle>,
    mut plan: ExecutionPlan,
    gcode_config: GcodeConfig,
    project: &Project,
    profile: &MachineProfile,
    event_name: &str,
) -> ServiceResult<JobProgress> {
    if job_slot.is_some() {
        return Err(ServiceError::conflict(
            "A job is already active. Cancel or wait for it to finish.",
        ));
    }
    let mut session_lock = ctx
        .session
        .lock()
        .map_err(|e| ServiceError::internal(format!("Failed to lock session: {e}")))?;
    let session = session_lock
        .as_mut()
        .ok_or_else(|| ServiceError::invalid_state("Not connected"))?;
    let action = if event_name == "job.frame.started" {
        "Quality-test frame"
    } else {
        "Quality-test start"
    };
    crate::ops::machine::require_idle_ready_for_motion(session, action)?;
    let job = match session {
        MachineSessionHandle::Grbl(session) => {
            let mut job = JobController::prepare(&plan, &gcode_config)
                .map_err(|e| ServiceError::machine(format!("Job prepare failed: {e}")))?;
            job.start(session)
                .map_err(|e| ServiceError::machine(format!("Job start failed: {e}")))?;
            ActiveJobHandle::Grbl(job)
        }
        MachineSessionHandle::Marlin(session) => {
            let driver = session.driver();
            let commands =
                crate::ops::machine::acknowledged_gcode_commands(driver, &plan, gcode_config)
                    .map_err(ServiceError::machine)?;
            ActiveJobHandle::Marlin(MarlinRuntimeJob::start(commands, session).map_err(
                |error| ServiceError::machine(format!("{driver:?} job start failed: {error}")),
            )?)
        }
        MachineSessionHandle::Smoothieware(session) => {
            let commands =
                crate::ops::machine::smoothieware_gcode_commands(session, &plan, gcode_config)
                    .map_err(ServiceError::machine)?;
            ActiveJobHandle::Smoothieware(
                SmoothiewareRuntimeJob::start(commands, session).map_err(|error| {
                    ServiceError::machine(format!("Smoothieware job start failed: {error}"))
                })?,
            )
        }
        MachineSessionHandle::Ruida(session) => {
            if event_name == "job.frame.started" {
                for segment in &mut plan.segments {
                    match segment {
                        PlanSegment::Vector { power_percent, .. }
                        | PlanSegment::Frame { power_percent, .. } => *power_percent = 0.0,
                        PlanSegment::Raster {
                            power_max_percent,
                            power_min_percent,
                            ..
                        } => {
                            *power_max_percent = 0.0;
                            *power_min_percent = 0.0;
                        }
                        PlanSegment::Travel { .. } | PlanSegment::OffsetFill { .. } => {}
                    }
                }
            }
            let compiled =
                crate::ops::machine::compile_ruida_execution_plan(&plan, project, profile)
                    .map_err(ServiceError::machine)?;
            ActiveJobHandle::Ruida(
                session
                    .start_job(&compiled, event_name == "job.frame.started")
                    .map_err(|error| {
                        ServiceError::machine(format!("Ruida quality-test start failed: {error}"))
                    })?,
            )
        }
        MachineSessionHandle::Lihuiyu(session) => {
            if event_name == "job.frame.started" {
                for segment in &mut plan.segments {
                    match segment {
                        PlanSegment::Vector { power_percent, .. }
                        | PlanSegment::Frame { power_percent, .. } => *power_percent = 0.0,
                        PlanSegment::Raster {
                            power_max_percent,
                            power_min_percent,
                            ..
                        } => {
                            *power_max_percent = 0.0;
                            *power_min_percent = 0.0;
                        }
                        PlanSegment::Travel { .. } | PlanSegment::OffsetFill { .. } => {}
                    }
                }
            }
            let compiled =
                crate::ops::machine::compile_lihuiyu_execution_plan(&plan, project, profile)
                    .map_err(ServiceError::machine)?;
            ActiveJobHandle::Lihuiyu(
                session
                    .start_job(&compiled, event_name == "job.frame.started")
                    .map_err(|error| {
                        ServiceError::machine(format!("Lihuiyu quality-test start failed: {error}"))
                    })?,
            )
        }
        MachineSessionHandle::Dsp(session) => ActiveJobHandle::Dsp(session.start_job(&plan)),
        MachineSessionHandle::Galvo(session) => ActiveJobHandle::Galvo(session.start_job(&plan)),
    };
    let progress = job.progress();
    *job_slot = Some(job);
    ctx.emit_event(
        event_name,
        serde_json::to_value(&progress).unwrap_or_default(),
    );
    Ok(progress)
}

fn gate_focus_test(
    ctx: &ServiceContext,
    request: &QualityTestRequest,
    require_grbl_session: bool,
) -> Result<(), QualityTestError> {
    if let QualityTestRequest::Focus(focus) = request {
        let profile =
            active_machine_profile(ctx).map_err(|_| QualityTestError::NoActiveMachineProfile)?;
        if !profile.supports_z_moves {
            return Err(QualityTestError::ZSupportRequired);
        }
        if require_grbl_session {
            let session_lock = ctx.session.lock().map_err(|_| QualityTestError::Internal {
                message: "Failed to lock session".to_string(),
            })?;
            if matches!(
                session_lock.as_ref(),
                Some(
                    MachineSessionHandle::Ruida(_)
                        | MachineSessionHandle::Dsp(_)
                        | MachineSessionHandle::Galvo(_)
                )
            ) {
                return Err(QualityTestError::UnsupportedZBackend);
            }
        }
        if focus.mode == FocusTestZMode::AbsoluteWorkCoord {
            let proj_ctx = snapshot_context(ctx, request, &profile)
                .map_err(|_| QualityTestError::MaterialHeightRequired)?;
            if proj_ctx.material_height_mm.is_none() {
                return Err(QualityTestError::MaterialHeightRequired);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ServiceErrorCode;
    use beambench_common::machine::{MachineRunState, SessionState};
    use beambench_core::AppSettings;
    use beambench_grbl::GrblSession;
    use beambench_serial::MockSerialTransport;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_seed() -> CutEntry {
        let mut e = CutEntry::new(OperationType::Line);
        e.speed_mm_min = 1500.0;
        e.power_percent = 60.0;
        e
    }

    fn settings_with_active_profile(profile: MachineProfile) -> AppSettings {
        let mut settings = AppSettings::default();
        settings.active_profile_id = Some(profile.id);
        settings.machine_profiles.push(profile);
        settings
    }

    fn temp_gcode_path(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("{name}-{nonce}.gcode"))
    }

    fn assert_bounds_center(bounds: Bounds, x: f64, y: f64) {
        let center_x = bounds.min.x + bounds.width() * 0.5;
        let center_y = bounds.min.y + bounds.height() * 0.5;
        assert!(
            (center_x - x).abs() < 0.001,
            "center x {center_x} should equal {x}"
        );
        assert!(
            (center_y - y).abs() < 0.001,
            "center y {center_y} should equal {y}"
        );
    }

    #[test]
    fn translate_segment_preserves_vertical_raster_transposed_axes() {
        use beambench_planner::{
            DirectionMode, PowerMode, ScanAxis, ScanDirection, ScanRun, Scanline,
        };

        let mut segment = PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 1.0,
                    end_x_mm: 3.0,
                    power_values: vec![255],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Unidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 1000.0,
            layer_id: "layer".to_owned(),
            cut_entry_id: "entry".to_owned(),
            scan_angle_deg: 90.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: Vec::new(),
            scan_axis: ScanAxis::Vertical,
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        };

        translate_segment(&mut segment, 5.0, 7.0);

        let PlanSegment::Raster {
            scanlines,
            scan_origin,
            ..
        } = segment
        else {
            panic!("expected raster segment");
        };
        assert_eq!(scan_origin, Point2D::new(5.0, 7.0));
        assert_eq!(scanlines[0].y_mm, 15.0);
        assert_eq!(scanlines[0].runs[0].start_x_mm, 8.0);
        assert_eq!(scanlines[0].runs[0].end_x_mm, 10.0);
    }

    fn material_settings() -> MaterialTestSettings {
        MaterialTestSettings::default()
    }

    fn focus_settings() -> FocusTestSettings {
        FocusTestSettings::default()
    }

    fn interval_settings() -> IntervalTestSettings {
        IntervalTestSettings::default()
    }

    fn ready_grbl_quality_test_context_with_run_state(
        status_report: &str,
    ) -> (ServiceContext, QualityTestRequest) {
        let profile = MachineProfile::default();
        let ctx = ServiceContext::with_settings(settings_with_active_profile(profile));
        let mut transport = MockSerialTransport::new("quality-test-ready-non-idle");
        transport.enqueue_response("Grbl 1.1h");
        transport.enqueue_response(status_report);
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().expect("connect mock GRBL session");
        session.poll().expect("read banner and status");
        session.mark_ready().expect("mark mock session ready");
        assert_eq!(session.session_state(), SessionState::Ready);
        assert_eq!(session.last_status().run_state, MachineRunState::Jog);
        session.clear_console_log();
        *ctx.session.lock().expect("session lock") =
            Some(MachineSessionHandle::Grbl(session.into()));
        (
            ctx,
            QualityTestRequest::Material(MaterialTestSettings::default()),
        )
    }

    #[test]
    fn frame_and_start_reject_ready_non_idle_machine_without_sending_commands() {
        type LiveAction =
            fn(&ServiceContext, &QualityTestRequest) -> Result<JobProgress, QualityTestError>;
        let actions: [(&str, LiveAction); 2] =
            [("frame", quality_test_frame), ("start", quality_test_start)];

        for (name, action) in actions {
            let (ctx, request) = ready_grbl_quality_test_context_with_run_state(
                "<Jog|MPos:10.000,20.000,0.000|WPos:10.000,20.000,0.000|FS:0,0>",
            );

            let error = match action(&ctx, &request) {
                Ok(_) => panic!("{name} should reject a non-idle machine"),
                Err(error) => error,
            };
            let message = match error {
                QualityTestError::Internal { message } => message,
                other => panic!("{name} returned the wrong error: {other}"),
            };
            assert!(
                message.contains("requires an idle machine"),
                "unexpected {name} error: {message}"
            );
            assert!(ctx.job.lock().expect("job lock").is_none());
            let session_guard = ctx.session.lock().expect("session lock");
            let Some(MachineSessionHandle::Grbl(session)) = session_guard.as_ref() else {
                panic!("expected GRBL session");
            };
            assert_eq!(session.session_state(), SessionState::Ready);
            assert!(
                session.get_console_log(100).is_empty(),
                "{name} sent controller commands before rejecting the non-idle state"
            );
        }
    }

    #[test]
    fn material_test_internal_seed_matches_fill_defaults() {
        let seed = material_test_internal_seed();
        let mut expected = CutEntry::new(OperationType::Fill);
        expected.id = seed.id;
        assert_eq!(seed, expected);
        assert_eq!(seed.raster_settings, Some(RasterSettings::default()));
        assert_eq!(seed.vector_settings, None);
        assert!(!seed.air_assist);
        assert_eq!(seed.z_offset_mm, 0.0);
        assert!(seed.gcode_prefix.is_empty());
        assert!(seed.gcode_suffix.is_empty());
        assert!(seed.output_enabled);
    }

    #[test]
    fn no_active_profile_is_a_hard_error_for_all_entry_points() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let req = QualityTestRequest::Material(material_settings());

        let preview_err = quality_test_preview(&ctx, &req).expect_err("preview should fail");
        assert_eq!(preview_err.code, ServiceErrorCode::InvalidState);
        assert_eq!(preview_err.message, "No active machine profile selected");

        let path = temp_gcode_path("bb-quality-test-no-profile");
        let export_err =
            quality_test_export_gcode(&ctx, &req, &path).expect_err("export should fail");
        assert_eq!(export_err.code, ServiceErrorCode::InvalidState);
        assert_eq!(export_err.message, "No active machine profile selected");

        assert!(matches!(
            quality_test_frame(&ctx, &req),
            Err(QualityTestError::NoActiveMachineProfile)
        ));
        assert!(matches!(
            quality_test_start(&ctx, &req),
            Err(QualityTestError::NoActiveMachineProfile)
        ));
    }

    #[test]
    fn blank_project_with_no_layers_generates_material_preview_and_gcode() {
        let profile = MachineProfile::default();
        let ctx = ServiceContext::with_settings(settings_with_active_profile(profile));
        *ctx.project.lock().expect("project lock") = Some(Project::new("blank"));
        let req = QualityTestRequest::Material(material_settings());

        let preview = quality_test_preview(&ctx, &req).expect("preview");
        assert!(preview.preview.stats.segment_count > 0);
        assert!(!preview.preview.layers.is_empty());

        let path = temp_gcode_path("bb-quality-test-blank-project");
        let export = quality_test_export_gcode(&ctx, &req, &path).expect("export");
        assert_eq!(export.path, path);
        let gcode = std::fs::read_to_string(&export.path).expect("gcode");
        assert!(!gcode.trim().is_empty());
        let _ = std::fs::remove_file(export.path);
    }

    #[test]
    fn blank_project_with_no_layers_generates_focus_preview_when_material_height_set() {
        let profile = MachineProfile::default();
        let ctx = ServiceContext::with_settings(settings_with_active_profile(profile));
        let mut project = Project::new("blank focus");
        project.material_height_mm = Some(3.0);
        *ctx.project.lock().expect("project lock") = Some(project);
        let req = QualityTestRequest::Focus(focus_settings());

        let preview = quality_test_preview(&ctx, &req).expect("preview");

        assert!(preview.preview.stats.segment_count > 0);
        assert!(!preview.preview.layers.is_empty());
    }

    #[test]
    fn blank_project_with_no_layers_generates_interval_preview() {
        let profile = MachineProfile::default();
        let ctx = ServiceContext::with_settings(settings_with_active_profile(profile));
        *ctx.project.lock().expect("project lock") = Some(Project::new("blank interval"));
        let req = QualityTestRequest::Interval(interval_settings());

        let preview = quality_test_preview(&ctx, &req).expect("preview");

        assert!(preview.preview.stats.segment_count > 0);
        assert!(!preview.preview.layers.is_empty());
    }

    #[test]
    fn focus_relative_preview_works_without_open_project() {
        let profile = MachineProfile::default();
        let ctx = ServiceContext::with_settings(settings_with_active_profile(profile));
        let mut settings = focus_settings();
        settings.mode = FocusTestZMode::RelativeTemporary;
        let req = QualityTestRequest::Focus(settings);

        let preview = quality_test_preview(&ctx, &req).expect("preview");

        assert!(preview.preview.stats.segment_count > 0);
        assert!(!preview.preview.layers.is_empty());
    }

    #[test]
    fn focus_absolute_preview_requires_material_height() {
        let profile = MachineProfile::default();
        let ctx = ServiceContext::with_settings(settings_with_active_profile(profile));
        let req = QualityTestRequest::Focus(focus_settings());

        let err = quality_test_preview(&ctx, &req).expect_err("preview should fail");

        assert_eq!(err.code, ServiceErrorCode::InvalidState);
        assert_eq!(
            err.message,
            "absolute Z mode requires Project.material_height_mm to be set"
        );
    }

    #[test]
    fn no_project_uses_active_profile_synthetic_context() {
        let mut profile = MachineProfile::default();
        profile.bed_width_mm = 410.0;
        profile.bed_height_mm = 290.0;
        let ctx = ServiceContext::with_settings(settings_with_active_profile(profile.clone()));
        let req = QualityTestRequest::Material(material_settings());

        let proj_ctx = snapshot_context(&ctx, &req, &profile).expect("synthetic context");
        assert_eq!(proj_ctx.workspace.bed_width_mm, 410.0);
        assert_eq!(proj_ctx.workspace.bed_height_mm, 290.0);
        assert_eq!(proj_ctx.start_from, StartFromMode::AbsoluteCoords);
        assert_eq!(proj_ctx.job_origin, AnchorPoint::TopLeft);
        assert_eq!(proj_ctx.user_origin, None);
        assert_eq!(proj_ctx.material_height_mm, None);
        assert_eq!(proj_ctx.seed_entry.operation, OperationType::Fill);

        let preview = quality_test_preview(&ctx, &req).expect("preview");
        assert!(preview.preview.stats.segment_count > 0);
    }

    #[test]
    fn material_test_layer_count_matches_grid() {
        let s = material_settings();
        let (layers, samples, _labels) = generate_material_test(&s, &make_seed());
        let cells = (s.x_axis.count * s.y_axis.count) as usize;
        assert_eq!(layers.len(), cells + 1);
        assert_eq!(samples.len(), cells + 1);
    }

    #[test]
    fn material_test_per_sample_speed_and_power_are_set() {
        let s = material_settings();
        let (layers, _, _) = generate_material_test(&s, &make_seed());
        let sample_entries = layers
            .iter()
            .filter(|l| l.name.starts_with(SAMPLES_LAYER_PREFIX))
            .map(|l| &l.entries[0])
            .collect::<Vec<_>>();
        assert!(
            sample_entries
                .iter()
                .any(|entry| entry.speed_mm_min == s.y_axis.min
                    && entry.power_percent == s.x_axis.min)
        );
        assert!(
            sample_entries
                .iter()
                .any(|entry| entry.speed_mm_min == s.y_axis.max
                    && entry.power_percent == s.x_axis.max)
        );
    }

    #[test]
    fn material_axes_mutate_operation_specific_fields() {
        let mut raster = CutEntry::new(OperationType::Fill);
        apply_material_axis_value(&mut raster, MaterialTestAxisParam::Speed, 2222.0);
        apply_material_axis_value(&mut raster, MaterialTestAxisParam::Power, 77.0);
        apply_material_axis_value(&mut raster, MaterialTestAxisParam::Interval, 0.125);
        apply_material_axis_value(&mut raster, MaterialTestAxisParam::Passes, 3.0);
        assert_eq!(raster.speed_mm_min, 2222.0);
        assert_eq!(raster.power_percent, 77.0);
        assert_eq!(
            raster.raster_settings.as_ref().map(|r| r.line_interval_mm),
            Some(0.125)
        );
        assert_eq!(raster.raster_settings.as_ref().map(|r| r.dpi), Some(203));
        assert_eq!(raster.raster_settings.as_ref().map(|r| r.passes), Some(3));
        assert_eq!(raster.vector_settings.as_ref().map(|v| v.passes), None);

        let mut vector = CutEntry::new(OperationType::Line);
        apply_material_axis_value(&mut vector, MaterialTestAxisParam::Passes, 4.0);
        assert_eq!(vector.vector_settings.as_ref().map(|v| v.passes), Some(4));
        assert_eq!(vector.raster_settings, None);
    }

    #[test]
    fn interval_axis_is_normalized_for_vector_operations() {
        let mut s = material_settings();
        s.enable_text = false;
        s.enable_border = false;
        s.sample_entry = CutEntry::new(OperationType::Line);
        s.x_axis = MaterialTestAxis {
            param: MaterialTestAxisParam::Interval,
            count: 2,
            min: 1000.0,
            max: 2000.0,
        };
        s.y_axis = MaterialTestAxis {
            param: MaterialTestAxisParam::Power,
            count: 1,
            min: 20.0,
            max: 20.0,
        };

        let (layers, _, _) = generate_material_test(&s, &make_seed());
        let entries = layers.iter().map(|l| &l.entries[0]).collect::<Vec<_>>();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.operation == OperationType::Line));
        assert!(entries.iter().all(|e| e.raster_settings.is_none()));
        assert!(entries.iter().any(|e| e.speed_mm_min == 1000.0));
        assert!(entries.iter().any(|e| e.speed_mm_min == 2000.0));
    }

    #[test]
    fn material_test_safe_first_ordering_matches_speed_power_tuple() {
        let mut s = material_settings();
        s.enable_text = false;
        s.enable_border = false;
        s.x_axis = MaterialTestAxis {
            param: MaterialTestAxisParam::Speed,
            count: 2,
            min: 1000.0,
            max: 2000.0,
        };
        s.y_axis = MaterialTestAxis {
            param: MaterialTestAxisParam::Power,
            count: 2,
            min: 10.0,
            max: 20.0,
        };

        let (layers, _, _) = generate_material_test(&s, &make_seed());
        let order = layers
            .iter()
            .map(|l| (l.entries[0].speed_mm_min, l.entries[0].power_percent))
            .collect::<Vec<_>>();
        assert_eq!(
            order,
            vec![
                (2000.0, 10.0),
                (2000.0, 20.0),
                (1000.0, 10.0),
                (1000.0, 20.0)
            ]
        );
    }

    #[test]
    fn center_placement_is_shared_for_preview_export_frame_and_start_paths() {
        let mut settings = material_settings();
        settings.absolute_center_enabled = true;
        settings.x_center_mm = 75.0;
        settings.y_center_mm = 45.0;
        settings.enable_text = false;
        settings.enable_border = false;
        let request = QualityTestRequest::Material(settings);
        let profile = MachineProfile::default();
        let ctx = ServiceContext::with_settings(settings_with_active_profile(profile.clone()));
        let proj_ctx = ProjectContext {
            workspace: Workspace::default(),
            job_origin: AnchorPoint::BottomRight,
            user_origin: Some((999.0, 999.0)),
            start_from: StartFromMode::UserOrigin,
            optimization: ProjectOptimization::default(),
            material_height_mm: None,
            seed_entry: make_seed(),
        };
        let (synthetic, _warnings) = build_synthetic_project(&request, &proj_ctx, &profile);

        let preview_or_export_plan =
            build_placed_transient_plan(&synthetic, &profile, &request, false).expect("plan");
        assert_bounds_center(preview_or_export_plan.bounds, 75.0, 45.0);

        let start_plan =
            build_placed_transient_plan(&synthetic, &profile, &request, true).expect("plan");
        assert_bounds_center(start_plan.bounds, 75.0, 45.0);

        let raw_bbox = union_bounds(&synthetic.objects).expect("bbox");
        let mut frame_plan = build_frame_plan(&raw_bbox, 1.0, 3000.0);
        place_quality_test_frame_plan(&mut frame_plan, &synthetic, &ctx, &request);
        assert_bounds_center(frame_plan.bounds, 75.0, 45.0);
    }

    #[test]
    fn absolute_coords_quality_tests_ignore_saved_job_origin() {
        let settings = material_settings();
        let request = QualityTestRequest::Material(settings);
        let profile = MachineProfile::default();
        let mut proj_ctx = ProjectContext {
            workspace: Workspace::default(),
            job_origin: AnchorPoint::TopLeft,
            user_origin: None,
            start_from: StartFromMode::AbsoluteCoords,
            optimization: ProjectOptimization::default(),
            material_height_mm: None,
            seed_entry: make_seed(),
        };
        let (top_left_project, _warnings) = build_synthetic_project(&request, &proj_ctx, &profile);
        proj_ctx.job_origin = AnchorPoint::BottomRight;
        let (bottom_right_project, _warnings) =
            build_synthetic_project(&request, &proj_ctx, &profile);

        let top_left_plan =
            build_placed_transient_plan(&top_left_project, &profile, &request, true)
                .expect("top-left plan");
        let bottom_right_plan =
            build_placed_transient_plan(&bottom_right_project, &profile, &request, true)
                .expect("bottom-right plan");

        assert_eq!(bottom_right_plan.bounds, top_left_plan.bounds);
        assert_eq!(
            bottom_right_plan.segments.len(),
            top_left_plan.segments.len(),
            "Absolute Coords should ignore saved Job Origin anchors without changing geometry"
        );

        let ctx = ServiceContext::with_settings(settings_with_active_profile(profile));
        let top_bbox = union_bounds(&top_left_project.objects).expect("top-left bbox");
        let mut top_frame = build_frame_plan(&top_bbox, 1.0, 3000.0);
        place_quality_test_frame_plan(&mut top_frame, &top_left_project, &ctx, &request);
        let bottom_bbox = union_bounds(&bottom_right_project.objects).expect("bottom-right bbox");
        let mut bottom_frame = build_frame_plan(&bottom_bbox, 1.0, 3000.0);
        place_quality_test_frame_plan(&mut bottom_frame, &bottom_right_project, &ctx, &request);
        assert_eq!(bottom_frame.bounds, top_frame.bounds);
    }

    #[test]
    fn center_mode_affects_service_preview_and_export_geometry() {
        let mut settings = material_settings();
        settings.absolute_center_enabled = true;
        settings.x_center_mm = 120.0;
        settings.y_center_mm = 80.0;
        settings.enable_text = false;
        settings.enable_border = false;
        settings.sample_entry = CutEntry::new(OperationType::Line);
        settings.x_axis.count = 1;
        settings.y_axis.count = 1;
        let req = QualityTestRequest::Material(settings);
        let profile = MachineProfile::default();
        let ctx = ServiceContext::with_settings(settings_with_active_profile(profile));

        let preview = quality_test_preview(&ctx, &req).expect("preview");
        assert_bounds_center(preview.preview.bounds, 120.0, 80.0);

        let path = temp_gcode_path("bb-quality-test-center-export");
        let export = quality_test_export_gcode(&ctx, &req, &path).expect("export");
        let gcode = std::fs::read_to_string(&export.path).expect("gcode");
        assert!(
            gcode.contains("X115") && gcode.contains("X125"),
            "expected centered export geometry in G-code"
        );
        let _ = std::fs::remove_file(export.path);
    }

    #[test]
    fn frame_and_start_reject_centered_geometry_outside_bed() {
        let mut settings = material_settings();
        settings.absolute_center_enabled = true;
        settings.x_center_mm = -1000.0;
        settings.y_center_mm = -1000.0;
        settings.enable_text = false;
        settings.enable_border = false;
        settings.sample_entry = CutEntry::new(OperationType::Line);
        settings.x_axis.count = 1;
        settings.y_axis.count = 1;
        let req = QualityTestRequest::Material(settings);
        let profile = MachineProfile::default();
        let ctx = ServiceContext::with_settings(settings_with_active_profile(profile));

        assert!(matches!(
            quality_test_frame(&ctx, &req),
            Err(QualityTestError::BoundsExceeded { .. })
        ));
        assert!(matches!(
            quality_test_start(&ctx, &req),
            Err(QualityTestError::BoundsExceeded { .. })
        ));
    }

    #[test]
    fn create_material_test_on_canvas_creates_project_when_none_is_open() {
        let profile = MachineProfile::default();
        let ctx = ServiceContext::with_settings(settings_with_active_profile(profile));
        let mut settings = material_settings();
        settings.enable_text = false;
        settings.enable_border = false;
        settings.x_axis.count = 1;
        settings.y_axis.count = 1;

        let resp = quality_test_create_material_on_canvas(&ctx, &settings).expect("create");

        assert_eq!(resp.project.metadata.project_name, "Material Test");
        assert_eq!(resp.created_object_ids.len(), 1);
        assert_eq!(resp.created_layer_ids.len(), 1);
        assert_eq!(resp.project.objects.len(), 1);
        assert_eq!(resp.project.layers.len(), 1);
        assert!(
            ctx.project
                .lock()
                .expect("project lock")
                .as_ref()
                .is_some_and(|project| project.metadata.project_name == "Material Test")
        );
        assert!(!ctx.undo_state().expect("undo state").can_undo);
    }

    #[test]
    fn create_material_test_on_canvas_appends_to_existing_project_with_single_undo() {
        let profile = MachineProfile::default();
        let ctx = ServiceContext::with_settings(settings_with_active_profile(profile));
        *ctx.project.lock().expect("project lock") = Some(Project::new("Existing"));
        let mut settings = material_settings();
        settings.enable_text = false;
        settings.enable_border = false;
        settings.x_axis.count = 1;
        settings.y_axis.count = 1;

        let resp = quality_test_create_material_on_canvas(&ctx, &settings).expect("create");

        assert_eq!(resp.project.metadata.project_name, "Existing");
        assert_eq!(resp.created_object_ids.len(), 1);
        assert_eq!(resp.project.objects.len(), 1);
        assert_eq!(resp.project.layers.len(), 1);
        assert!(ctx.undo_state().expect("undo state").can_undo);

        let restored = crate::ops::project::undo_project(&ctx).expect("undo");
        assert!(restored.objects.is_empty());
        assert!(restored.layers.is_empty());
        assert!(!ctx.undo_state().expect("undo state").can_undo);
    }

    #[test]
    fn create_material_test_on_canvas_honors_absolute_center_fields() {
        let profile = MachineProfile::default();
        let ctx = ServiceContext::with_settings(settings_with_active_profile(profile));
        *ctx.project.lock().expect("project lock") = Some(Project::new("Existing"));
        let mut settings = material_settings();
        settings.absolute_center_enabled = true;
        settings.x_center_mm = 90.0;
        settings.y_center_mm = 70.0;
        settings.enable_text = false;
        settings.enable_border = false;
        settings.x_axis.count = 1;
        settings.y_axis.count = 1;

        let resp = quality_test_create_material_on_canvas(&ctx, &settings).expect("create");
        let bounds = union_bounds(&resp.project.objects).expect("bounds");

        assert_bounds_center(bounds, 90.0, 70.0);
    }

    #[test]
    fn centered_offset_fill_material_test_is_preexpanded_before_translation() {
        let mut settings = material_settings();
        settings.absolute_center_enabled = true;
        settings.x_center_mm = 60.0;
        settings.y_center_mm = 40.0;
        settings.enable_text = false;
        settings.enable_border = false;
        settings.sample_entry = CutEntry::new(OperationType::OffsetFill);
        settings.x_axis.count = 1;
        settings.y_axis.count = 1;
        let request = QualityTestRequest::Material(settings);
        let profile = MachineProfile::default();
        let proj_ctx = ProjectContext {
            workspace: Workspace::default(),
            job_origin: AnchorPoint::TopLeft,
            user_origin: None,
            start_from: StartFromMode::AbsoluteCoords,
            optimization: ProjectOptimization::default(),
            material_height_mm: None,
            seed_entry: make_seed(),
        };
        let (synthetic, _warnings) = build_synthetic_project(&request, &proj_ctx, &profile);

        let plan = build_placed_transient_plan(&synthetic, &profile, &request, true).expect("plan");
        assert!(
            plan.segments
                .iter()
                .all(|segment| !matches!(segment, PlanSegment::OffsetFill { .. })),
            "OffsetFill Material Test geometry must be expanded before center placement"
        );
        assert_bounds_center(plan.bounds, 60.0, 40.0);
    }

    #[test]
    fn focus_test_intervals_generate_line_count_plus_one() {
        let s = focus_settings();
        let (layers, samples, labels) = generate_focus_test(&s, &make_seed());
        let expected = s.intervals as usize + 1;
        assert_eq!(layers.len(), expected);
        assert_eq!(samples.len(), expected);
        assert_eq!(labels.len(), expected);
    }

    #[test]
    fn legacy_focus_steps_alias_generates_intervals_plus_one_lines() {
        let s: FocusTestSettings = serde_json::from_str(r#"{"steps":9}"#).unwrap();
        let (layers, samples, labels) = generate_focus_test(&s, &make_seed());
        assert_eq!(s.intervals, 9);
        assert_eq!(layers.len(), 10);
        assert_eq!(samples.len(), 10);
        assert_eq!(labels.len(), 10);
    }

    #[test]
    fn focus_test_intervals_floor_at_one() {
        let mut s = focus_settings();
        s.intervals = 0;
        let (layers, samples, labels) = generate_focus_test(&s, &make_seed());
        assert_eq!(layers.len(), 2);
        assert_eq!(samples.len(), 2);
        assert_eq!(labels.len(), 2);
    }

    #[test]
    fn focus_test_absolute_z_stores_tested_offset() {
        let mut s = focus_settings();
        s.mode = FocusTestZMode::AbsoluteWorkCoord;
        s.z_min_mm = 0.0;
        s.z_max_mm = 0.0;
        s.intervals = 1;
        let (layers, _, _) = generate_focus_test(&s, &make_seed());
        for l in layers {
            assert_eq!(l.entries[0].z_offset_mm, 0.0);
        }
    }

    #[test]
    fn focus_test_relative_z_emits_delta_only() {
        let mut s = focus_settings();
        s.mode = FocusTestZMode::RelativeTemporary;
        s.z_min_mm = 1.0;
        s.z_max_mm = 1.0;
        s.intervals = 1;
        let (layers, _, _) = generate_focus_test(&s, &make_seed());
        for l in layers {
            assert_eq!(l.entries[0].z_offset_mm, 1.0);
        }
    }

    #[test]
    fn focus_test_absolute_export_emits_z_moves_with_profile_feed() {
        let mut profile = MachineProfile::default();
        profile.supports_z_moves = true;
        profile.z_move_feed_mm_min = 450.0;
        let ctx = ServiceContext::with_settings(settings_with_active_profile(profile));
        let mut project = Project::new("focus gcode");
        project.material_height_mm = Some(7.5);
        *ctx.project.lock().expect("project lock") = Some(project);
        let mut settings = focus_settings();
        settings.intervals = 1;
        settings.z_min_mm = 0.0;
        settings.z_max_mm = 1.0;
        let path = temp_gcode_path("bb-quality-test-focus-absolute-z");

        let export = quality_test_export_gcode(&ctx, &QualityTestRequest::Focus(settings), &path)
            .expect("export");
        let gcode = std::fs::read_to_string(&export.path).expect("gcode");
        let _ = std::fs::remove_file(export.path);

        assert!(gcode.contains("G1 Z7.500 F450"));
        assert!(gcode.contains("G1 Z8.500 F450"));
    }

    #[test]
    fn focus_frame_gcode_is_xy_only_even_when_z_is_supported() {
        let mut profile = MachineProfile::default();
        profile.supports_z_moves = true;
        profile.z_move_feed_mm_min = 450.0;
        let ctx = ServiceContext::with_settings(settings_with_active_profile(profile.clone()));
        let mut project = Project::new("focus frame");
        project.material_height_mm = Some(7.5);
        *ctx.project.lock().expect("project lock") = Some(project);

        let mut settings = focus_settings();
        settings.intervals = 1;
        settings.z_min_mm = 0.0;
        settings.z_max_mm = 1.0;
        let request = QualityTestRequest::Focus(settings);
        let proj_ctx = snapshot_context(&ctx, &request, &profile).expect("context");
        let (synthetic, _warnings) = build_synthetic_project(&request, &proj_ctx, &profile);
        let bbox = union_bounds(&synthetic.objects).expect("bounds");
        let mut frame_plan = build_frame_plan(&bbox, 1.0, 3000.0);
        place_quality_test_frame_plan(&mut frame_plan, &synthetic, &ctx, &request);
        let config = quality_test_frame_gcode_config(&synthetic, &profile, &request);
        let gcode = beambench_grbl::generate_gcode(&frame_plan, &config).expect("gcode");

        assert!(!config.z_moves_enabled);
        assert!(!gcode.iter().any(|line| line.starts_with("G1 Z")));
    }

    #[test]
    fn focus_test_relative_equal_min_max_export_emits_no_z_moves() {
        let mut profile = MachineProfile::default();
        profile.supports_z_moves = true;
        let ctx = ServiceContext::with_settings(settings_with_active_profile(profile));
        let mut settings = focus_settings();
        settings.mode = FocusTestZMode::RelativeTemporary;
        settings.intervals = 1;
        settings.z_min_mm = 0.0;
        settings.z_max_mm = 0.0;
        let path = temp_gcode_path("bb-quality-test-focus-relative-zero-z");

        let export = quality_test_export_gcode(&ctx, &QualityTestRequest::Focus(settings), &path)
            .expect("export");
        let gcode = std::fs::read_to_string(&export.path).expect("gcode");
        let _ = std::fs::remove_file(export.path);

        assert!(!gcode.lines().any(|line| line.starts_with("G1 Z")));
    }

    #[test]
    fn interval_test_intervals_strictly_increasing() {
        let s = interval_settings();
        let (layers, _, _) = generate_interval_test(&s, &make_seed());
        let mut prev: Option<f64> = None;
        for l in layers {
            let interval = l.entries[0]
                .raster_settings
                .as_ref()
                .unwrap()
                .line_interval_mm;
            if let Some(p) = prev {
                assert!(interval > p, "intervals must be strictly increasing");
            }
            prev = Some(interval);
        }
    }

    #[test]
    fn interval_test_uses_configured_speed_and_power() {
        let mut s = interval_settings();
        s.speed_mm_min = 2222.0;
        s.power_percent = 37.0;
        let seed = interval_test_internal_seed(&s);
        let (layers, _, _) = generate_interval_test(&s, &seed);
        for l in layers {
            assert_eq!(l.entries[0].speed_mm_min, 2222.0);
            assert_eq!(l.entries[0].power_percent, 37.0);
        }
    }

    #[test]
    fn synthetic_project_includes_labels_layer() {
        let s = focus_settings();
        let proj_ctx = ProjectContext {
            workspace: Workspace::default(),
            job_origin: AnchorPoint::default(),
            user_origin: None,
            start_from: StartFromMode::default(),
            optimization: ProjectOptimization::default(),
            material_height_mm: Some(0.0),
            seed_entry: make_seed(),
        };
        let (proj, _w) = build_synthetic_project(
            &QualityTestRequest::Focus(s.clone()),
            &proj_ctx,
            &MachineProfile::default(),
        );
        assert!(proj.layers.iter().any(|l| l.name == LABELS_LAYER_NAME));
    }

    #[test]
    fn bounds_check_emits_warning_when_exceeded() {
        let mut s = material_settings();
        s.cell_w_mm = 1000.0;
        s.cell_h_mm = 1000.0;
        let proj_ctx = ProjectContext {
            workspace: Workspace::default(),
            job_origin: AnchorPoint::default(),
            user_origin: None,
            start_from: StartFromMode::default(),
            optimization: ProjectOptimization::default(),
            material_height_mm: None,
            seed_entry: make_seed(),
        };
        let (_proj, warnings) = build_synthetic_project(
            &QualityTestRequest::Material(s),
            &proj_ctx,
            &MachineProfile::default(),
        );
        assert!(matches!(
            warnings
                .iter()
                .find(|w| matches!(w, QualityTestWarning::BoundsExceeded { .. })),
            Some(_)
        ));
    }

    #[test]
    fn focus_gate_rejects_when_z_unsupported() {
        let mut settings = AppSettings::default();
        let profile = MachineProfile::default();
        settings.active_profile_id = Some(profile.id);
        settings.machine_profiles.push(profile);
        let ctx = ServiceContext::with_settings(settings);
        let req = QualityTestRequest::Focus(focus_settings());
        assert!(matches!(
            gate_focus_test(&ctx, &req, false),
            Err(QualityTestError::ZSupportRequired)
        ));
    }

    #[test]
    fn focus_test_perforated_labels_targets_labels_not_samples() {
        let mut s = focus_settings();
        s.perforated_labels = true;
        // Sample lines must keep perforation OFF — the toggle is for label glyphs only.
        let (sample_layers, _, _) = generate_focus_test(&s, &make_seed());
        for l in sample_layers {
            let vs = l.entries[0]
                .vector_settings
                .as_ref()
                .expect("vector settings");
            assert!(
                !vs.perforation_enabled,
                "sample lines must not enable perforation just because perforated_labels is on"
            );
        }
        // Label layer entry must enable perforation when the flag is on.
        let proj_ctx = ProjectContext {
            workspace: Workspace::default(),
            job_origin: AnchorPoint::default(),
            user_origin: None,
            start_from: StartFromMode::default(),
            optimization: ProjectOptimization::default(),
            material_height_mm: Some(0.0),
            seed_entry: make_seed(),
        };
        let (proj, _w) = build_synthetic_project(
            &QualityTestRequest::Focus(s.clone()),
            &proj_ctx,
            &MachineProfile::default(),
        );
        let label_layer = proj
            .layers
            .iter()
            .find(|l| l.name == LABELS_LAYER_NAME)
            .expect("labels layer present");
        let vs = label_layer.entries[0]
            .vector_settings
            .as_ref()
            .expect("label vector settings");
        assert!(
            vs.perforation_enabled,
            "label entry must enable perforation when perforated_labels is on"
        );
        assert_eq!(label_layer.entries[0].power_percent, s.power_percent);
        assert_eq!(label_layer.entries[0].speed_mm_min, s.speed_mm_min);
    }

    #[test]
    fn focus_test_default_labels_have_no_perforation() {
        let s = focus_settings();
        assert!(!s.perforated_labels, "default must be off");
        let proj_ctx = ProjectContext {
            workspace: Workspace::default(),
            job_origin: AnchorPoint::default(),
            user_origin: None,
            start_from: StartFromMode::default(),
            optimization: ProjectOptimization::default(),
            material_height_mm: Some(0.0),
            seed_entry: make_seed(),
        };
        let (proj, _w) = build_synthetic_project(
            &QualityTestRequest::Focus(s),
            &proj_ctx,
            &MachineProfile::default(),
        );
        let label_layer = proj
            .layers
            .iter()
            .find(|l| l.name == LABELS_LAYER_NAME)
            .expect("labels layer present");
        let vs = label_layer.entries[0]
            .vector_settings
            .as_ref()
            .expect("label vector settings");
        assert!(!vs.perforation_enabled, "default labels must not perforate");
    }

    #[test]
    fn material_test_uses_fill_operation() {
        let s = material_settings();
        let (layers, _, _) = generate_material_test(&s, &make_seed());
        for l in layers
            .iter()
            .filter(|l| l.name.starts_with(SAMPLES_LAYER_PREFIX))
        {
            assert_eq!(l.entries[0].operation, OperationType::Fill);
        }
    }

    #[test]
    fn synthetic_project_does_not_alias_caller_seed() {
        let mut original = make_seed();
        original.power_percent = 42.0;
        let proj_ctx = ProjectContext {
            workspace: Workspace::default(),
            job_origin: AnchorPoint::default(),
            user_origin: None,
            start_from: StartFromMode::default(),
            optimization: ProjectOptimization::default(),
            material_height_mm: None,
            seed_entry: original.clone(),
        };
        let (_proj, _w) = build_synthetic_project(
            &QualityTestRequest::Material(material_settings()),
            &proj_ctx,
            &MachineProfile::default(),
        );
        // The caller-supplied seed must remain untouched after generation.
        assert_eq!(proj_ctx.seed_entry.power_percent, 42.0);
    }

    #[test]
    fn synthetic_project_inherits_start_from_and_optimization() {
        use beambench_core::FinishPosition;
        let proj_ctx = ProjectContext {
            workspace: Workspace::default(),
            job_origin: AnchorPoint::Center,
            user_origin: Some((42.0, 7.5)),
            start_from: StartFromMode::UserOrigin,
            optimization: ProjectOptimization {
                finish_position: FinishPosition::CustomXY,
                finish_x: Some(123.0),
                finish_y: Some(45.0),
                ..Default::default()
            },
            material_height_mm: None,
            seed_entry: make_seed(),
        };
        let (proj, _w) = build_synthetic_project(
            &QualityTestRequest::Material(material_settings()),
            &proj_ctx,
            &MachineProfile::default(),
        );
        // Without these copies, transient Frame/Start drift away from the real job's anchor and
        // exported G-code finishes at the planner default instead of the user's chosen position.
        assert_eq!(proj.start_from, StartFromMode::UserOrigin);
        assert_eq!(proj.user_origin, Some((42.0, 7.5)));
        assert_eq!(proj.job_origin, AnchorPoint::Center);
        assert_eq!(proj.optimization.finish_position, FinishPosition::CustomXY);
        assert_eq!(proj.optimization.finish_x, Some(123.0));
        assert_eq!(proj.optimization.finish_y, Some(45.0));
    }
}
