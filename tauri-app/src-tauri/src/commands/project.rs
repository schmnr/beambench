use std::sync::Arc;
use beambench_common::{AnchorPoint, Bounds, StartFromMode, Transform2D, TransformLocks};
use beambench_core::{
    CutEntry, CutEntryPatch, CutEntryTemplate, Layer, LayerBatchToggle, LayerPatch,
    ObjectData, OperationType, Project, ProjectObject, ProjectOptimization,
    ProjectOptimizationPatch, RasterSettings,
};
use beambench_service::ops::project as project_ops;
use beambench_service::{ServiceContext, UndoState};
use tauri::State;
pub use beambench_service::ops::workflows::project::*;
#[tauri::command]
pub fn create_project(
    svc: State<'_, Arc<ServiceContext>>,
    name: String,
) -> Result<Project, String> {
    beambench_service::ops::workflows::project::create_project(&svc, name)
}
#[tauri::command]
pub fn get_project(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<Option<Project>, String> {
    beambench_service::ops::workflows::project::get_project(&svc)
}
#[tauri::command]
pub fn close_project(svc: State<'_, Arc<ServiceContext>>) -> Result<(), String> {
    beambench_service::ops::workflows::project::close_project(&svc)
}
#[tauri::command]
pub fn get_undo_state(svc: State<'_, Arc<ServiceContext>>) -> Result<UndoState, String> {
    beambench_service::ops::workflows::project::get_undo_state(&svc)
}
#[tauri::command]
pub fn undo_project(svc: State<'_, Arc<ServiceContext>>) -> Result<Project, String> {
    beambench_service::ops::workflows::project::undo_project(&svc)
}
#[tauri::command]
pub fn redo_project(svc: State<'_, Arc<ServiceContext>>) -> Result<Project, String> {
    beambench_service::ops::workflows::project::redo_project(&svc)
}
#[tauri::command]
pub fn get_project_layers(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<Vec<Layer>, String> {
    beambench_service::ops::workflows::project::get_project_layers(&svc)
}
#[tauri::command]
pub fn add_layer(
    svc: State<'_, Arc<ServiceContext>>,
    name: String,
    operation: String,
) -> Result<Layer, String> {
    beambench_service::ops::workflows::project::add_layer(&svc, name, operation)
}
#[tauri::command]
pub fn update_layer(
    svc: State<'_, Arc<ServiceContext>>,
    layer_id: String,
    patch: LayerPatch,
) -> Result<Layer, String> {
    beambench_service::ops::workflows::project::update_layer(&svc, layer_id, patch)
}
#[tauri::command]
pub fn remove_layer(
    svc: State<'_, Arc<ServiceContext>>,
    layer_id: String,
) -> Result<(), String> {
    beambench_service::ops::workflows::project::remove_layer(&svc, layer_id)
}
#[tauri::command]
pub fn reorder_layer(
    svc: State<'_, Arc<ServiceContext>>,
    layer_id: String,
    new_index: usize,
) -> Result<Vec<Layer>, String> {
    beambench_service::ops::workflows::project::reorder_layer(&svc, layer_id, new_index)
}
#[tauri::command]
pub fn get_project_objects(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<Vec<ProjectObject>, String> {
    beambench_service::ops::workflows::project::get_project_objects(&svc)
}
#[tauri::command]
pub fn add_object(
    svc: State<'_, Arc<ServiceContext>>,
    name: String,
    layer_id: String,
    object_data: ObjectData,
    bounds: Bounds,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::project::add_object(
        &svc,
        name,
        layer_id,
        object_data,
        bounds,
    )
}
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn add_object_atomic(
    svc: State<'_, Arc<ServiceContext>>,
    name: String,
    layer_id: String,
    object_data: ObjectData,
    bounds: Bounds,
    create_layer_name: Option<String>,
    create_layer_color_tag: Option<String>,
    create_layer_operation: Option<OperationType>,
    create_layer_entry_patch: Option<CutEntryPatch>,
) -> Result<project_ops::AddObjectAtomicResult, String> {
    beambench_service::ops::workflows::project::add_object_atomic(
        &svc,
        name,
        layer_id,
        object_data,
        bounds,
        create_layer_name,
        create_layer_color_tag,
        create_layer_operation,
        create_layer_entry_patch,
    )
}
#[tauri::command]
pub fn add_cut_entry(
    svc: State<'_, Arc<ServiceContext>>,
    layer_id: String,
    after_entry_id: Option<String>,
) -> Result<CutEntry, String> {
    beambench_service::ops::workflows::project::add_cut_entry(
        &svc,
        layer_id,
        after_entry_id,
    )
}
/// M4: re-stamp every layer's order_index per the cut-strength heuristic. Atomic.
#[tauri::command]
pub fn sort_layers_cut_last(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<Vec<Layer>, String> {
    beambench_service::ops::workflows::project::sort_layers_cut_last(&svc)
}
/// M4: batch toggle Layer.enabled (Output column) — Enable/Disable all, Invert, OnlyThisOn.
#[tauri::command]
pub fn set_all_layers_enabled(
    svc: State<'_, Arc<ServiceContext>>,
    mode: LayerBatchToggle,
) -> Result<Vec<Layer>, String> {
    beambench_service::ops::workflows::project::set_all_layers_enabled(&svc, mode)
}
/// M4: batch toggle Layer.visible (Show column).
#[tauri::command]
pub fn set_all_layers_visible(
    svc: State<'_, Arc<ServiceContext>>,
    mode: LayerBatchToggle,
) -> Result<Vec<Layer>, String> {
    beambench_service::ops::workflows::project::set_all_layers_visible(&svc, mode)
}
/// M4: reset a single cut entry to built-in defaults for its operation. Preserves entry id.
#[tauri::command]
pub fn reset_cut_entry_to_defaults(
    svc: State<'_, Arc<ServiceContext>>,
    layer_id: String,
    entry_id: String,
) -> Result<CutEntry, String> {
    beambench_service::ops::workflows::project::reset_cut_entry_to_defaults(
        &svc,
        layer_id,
        entry_id,
    )
}
/// M4: replace a layer's `entries[]` from a clipboard template (Copy/Paste settings).
#[tauri::command]
pub fn paste_layer_entries(
    svc: State<'_, Arc<ServiceContext>>,
    layer_id: String,
    entries: Vec<CutEntryTemplate>,
) -> Result<Layer, String> {
    beambench_service::ops::workflows::project::paste_layer_entries(
        &svc,
        layer_id,
        entries,
    )
}
#[tauri::command]
pub fn remove_cut_entry(
    svc: State<'_, Arc<ServiceContext>>,
    layer_id: String,
    entry_id: String,
) -> Result<(), String> {
    beambench_service::ops::workflows::project::remove_cut_entry(
        &svc,
        layer_id,
        entry_id,
    )
}
#[tauri::command]
pub fn reorder_cut_entry(
    svc: State<'_, Arc<ServiceContext>>,
    layer_id: String,
    entry_id: String,
    new_index: usize,
) -> Result<Layer, String> {
    beambench_service::ops::workflows::project::reorder_cut_entry(
        &svc,
        layer_id,
        entry_id,
        new_index,
    )
}
#[tauri::command]
pub fn update_cut_entry(
    svc: State<'_, Arc<ServiceContext>>,
    layer_id: String,
    entry_id: String,
    patch: CutEntryPatch,
) -> Result<CutEntry, String> {
    beambench_service::ops::workflows::project::update_cut_entry(
        &svc,
        layer_id,
        entry_id,
        patch,
    )
}
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn update_object(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    name: Option<String>,
    visible: Option<bool>,
    locked: Option<bool>,
    layer_id: Option<String>,
    transform: Option<Transform2D>,
    bounds: Option<Bounds>,
    lock_aspect_ratio: Option<bool>,
    transform_locks: Option<TransformLocks>,
    power_scale: Option<f64>,
    priority: Option<i32>,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::project::update_object(
        &svc,
        object_id,
        name,
        visible,
        locked,
        layer_id,
        transform,
        bounds,
        lock_aspect_ratio,
        transform_locks,
        power_scale,
        priority,
    )
}
#[tauri::command]
pub fn update_object_transform_state(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    transform_locks: Option<TransformLocks>,
    transform_lock_key: Option<String>,
    transform_enabled: Option<bool>,
    lock_aspect_ratio: Option<bool>,
) -> Result<Vec<ProjectObject>, String> {
    beambench_service::ops::workflows::project::update_object_transform_state(
        &svc,
        object_ids,
        transform_locks,
        transform_lock_key,
        transform_enabled,
        lock_aspect_ratio,
    )
}
#[tauri::command]
pub fn update_object_data(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    data: ObjectData,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::project::update_object_data(&svc, object_id, data)
}
#[tauri::command]
pub fn resize_text_area(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    bounds: Bounds,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::project::resize_text_area(&svc, object_id, bounds)
}
#[tauri::command]
pub fn advance_auto_variable_text(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<Vec<ProjectObject>, String> {
    beambench_service::ops::workflows::project::advance_auto_variable_text(&svc)
}
#[tauri::command]
pub fn apply_adjust_image_dialog(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    adjustments: Option<beambench_common::RasterAdjustments>,
    layer_id: String,
    raster_settings: RasterSettings,
) -> Result<ApplyAdjustImageResult, String> {
    beambench_service::ops::workflows::project::apply_adjust_image_dialog(
        &svc,
        object_id,
        adjustments,
        layer_id,
        raster_settings,
    )
}
#[tauri::command]
pub fn resize_shape_object(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    bounds: Bounds,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::project::resize_shape_object(
        &svc,
        object_id,
        bounds,
    )
}
#[tauri::command]
pub fn remove_object(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<(), String> {
    beambench_service::ops::workflows::project::remove_object(&svc, object_id)
}
#[tauri::command]
pub fn remove_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<usize, String> {
    beambench_service::ops::workflows::project::remove_objects(&svc, object_ids)
}
#[tauri::command]
pub fn set_text_guide_path(
    svc: State<'_, Arc<ServiceContext>>,
    text_id: String,
    guide_path_id: Option<String>,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::project::set_text_guide_path(
        &svc,
        text_id,
        guide_path_id,
    )
}
#[tauri::command]
pub fn nudge_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    dx: f64,
    dy: f64,
) -> Result<(), String> {
    beambench_service::ops::workflows::project::nudge_objects(&svc, object_ids, dx, dy)
}
#[tauri::command]
pub fn duplicate_object(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::project::duplicate_object(&svc, object_id)
}
#[tauri::command]
pub fn duplicate_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<Vec<ProjectObject>, String> {
    beambench_service::ops::workflows::project::duplicate_objects(&svc, object_ids)
}
#[tauri::command]
pub fn duplicate_object_in_place(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::project::duplicate_object_in_place(
        &svc,
        object_id,
    )
}
#[tauri::command]
pub fn duplicate_objects_in_place(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<Vec<ProjectObject>, String> {
    beambench_service::ops::workflows::project::duplicate_objects_in_place(
        &svc,
        object_ids,
    )
}
#[tauri::command]
pub fn paste_objects(
    svc: State<'_, Arc<ServiceContext>>,
    objects: Vec<ProjectObject>,
    in_place: bool,
) -> Result<Vec<ProjectObject>, String> {
    beambench_service::ops::workflows::project::paste_objects(&svc, objects, in_place)
}
#[tauri::command]
pub fn align_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    alignment_type: String,
    anchor_object_id: Option<String>,
) -> Result<Vec<ProjectObject>, String> {
    beambench_service::ops::workflows::project::align_objects(
        &svc,
        object_ids,
        alignment_type,
        anchor_object_id,
    )
}
#[tauri::command]
pub fn distribute_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    direction: String,
) -> Result<Vec<ProjectObject>, String> {
    beambench_service::ops::workflows::project::distribute_objects(
        &svc,
        object_ids,
        direction,
    )
}
#[tauri::command]
pub fn move_objects_together(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    axis: project_ops::MoveTogetherAxis,
    anchor_object_id: String,
) -> Result<Vec<ProjectObject>, String> {
    beambench_service::ops::workflows::project::move_objects_together(
        &svc,
        object_ids,
        axis,
        anchor_object_id,
    )
}
#[tauri::command]
pub fn mirror_across_line(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    axis_object_id: String,
) -> Result<Vec<ProjectObject>, String> {
    beambench_service::ops::workflows::project::mirror_across_line(
        &svc,
        object_ids,
        axis_object_id,
    )
}
#[tauri::command]
pub fn make_same_size(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    anchor_object_id: String,
    axis: project_ops::SameSizeAxis,
    preserve_aspect: bool,
) -> Result<Vec<ProjectObject>, String> {
    beambench_service::ops::workflows::project::make_same_size(
        &svc,
        object_ids,
        anchor_object_id,
        axis,
        preserve_aspect,
    )
}
#[tauri::command]
pub fn dock_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    direction: project_ops::DockDirection,
    options: project_ops::DockOptions,
) -> Result<Vec<ProjectObject>, String> {
    beambench_service::ops::workflows::project::dock_objects(
        &svc,
        object_ids,
        direction,
        options,
    )
}
#[tauri::command]
pub fn resize_slots(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    options: project_ops::ResizeSlotsOptions,
) -> Result<Vec<ProjectObject>, String> {
    beambench_service::ops::workflows::project::resize_slots(&svc, object_ids, options)
}
#[tauri::command]
pub fn bind_machine_profile(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<Project, String> {
    beambench_service::ops::workflows::project::bind_machine_profile(&svc)
}
#[tauri::command]
pub fn replace_project(
    svc: State<'_, Arc<ServiceContext>>,
    project: Project,
) -> Result<(), String> {
    beambench_service::ops::workflows::project::replace_project(&svc, project)
}
#[tauri::command]
pub fn set_layer_visible(
    svc: State<'_, Arc<ServiceContext>>,
    layer_id: String,
    visible: bool,
) -> Result<bool, String> {
    beambench_service::ops::workflows::project::set_layer_visible(
        &svc,
        layer_id,
        visible,
    )
}
#[tauri::command]
pub fn set_layer_air_assist(
    svc: State<'_, Arc<ServiceContext>>,
    layer_id: String,
    air_assist: bool,
) -> Result<bool, String> {
    beambench_service::ops::workflows::project::set_layer_air_assist(
        &svc,
        layer_id,
        air_assist,
    )
}
#[tauri::command]
pub fn select_all_in_layer(
    svc: State<'_, Arc<ServiceContext>>,
    layer_id: String,
) -> Result<Vec<String>, String> {
    beambench_service::ops::workflows::project::select_all_in_layer(&svc, layer_id)
}
#[tauri::command]
pub fn push_draw_order(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    direction: String,
) -> Result<(), String> {
    beambench_service::ops::workflows::project::push_draw_order(
        &svc,
        object_id,
        direction,
    )
}
#[tauri::command]
pub fn lock_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<(), String> {
    beambench_service::ops::workflows::project::lock_objects(&svc, object_ids)
}
#[tauri::command]
pub fn unlock_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<(), String> {
    beambench_service::ops::workflows::project::unlock_objects(&svc, object_ids)
}
#[tauri::command]
pub fn flip_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    horizontal: bool,
    pivot_x: Option<f64>,
    pivot_y: Option<f64>,
) -> Result<(), String> {
    beambench_service::ops::workflows::project::flip_objects(
        &svc,
        object_ids,
        horizontal,
        pivot_x,
        pivot_y,
    )
}
#[tauri::command]
pub fn rotate_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    degrees: f64,
    pivot_x: Option<f64>,
    pivot_y: Option<f64>,
) -> Result<(), String> {
    beambench_service::ops::workflows::project::rotate_objects(
        &svc,
        object_ids,
        degrees,
        pivot_x,
        pivot_y,
    )
}
#[tauri::command]
pub fn rotate_objects_and_bake_active_path(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    degrees: f64,
    pivot_x: Option<f64>,
    pivot_y: Option<f64>,
    active_object_id: String,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::project::rotate_objects_and_bake_active_path(
        &svc,
        object_ids,
        degrees,
        pivot_x,
        pivot_y,
        active_object_id,
    )
}
#[tauri::command]
pub fn shear_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    shear_x: f64,
    shear_y: f64,
    pivot_x: Option<f64>,
    pivot_y: Option<f64>,
) -> Result<(), String> {
    beambench_service::ops::workflows::project::shear_objects(
        &svc,
        object_ids,
        shear_x,
        shear_y,
        pivot_x,
        pivot_y,
    )
}
/// Batch-update object bounds. Vector path objects are refitted atomically within
/// the same undo snapshot.
#[tauri::command]
pub fn update_object_bounds_batch(
    svc: State<'_, Arc<ServiceContext>>,
    entries: Vec<BoundsEntry>,
) -> Result<(), String> {
    beambench_service::ops::workflows::project::update_object_bounds_batch(&svc, entries)
}
#[tauri::command]
pub fn move_objects_to(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    x: f64,
    y: f64,
) -> Result<(), String> {
    beambench_service::ops::workflows::project::move_objects_to(&svc, object_ids, x, y)
}
#[tauri::command]
pub fn set_start_from(
    svc: State<'_, Arc<ServiceContext>>,
    mode: StartFromMode,
) -> Result<(), String> {
    beambench_service::ops::workflows::project::set_start_from(&svc, mode)
}
#[tauri::command]
pub fn set_job_origin(
    svc: State<'_, Arc<ServiceContext>>,
    anchor: AnchorPoint,
) -> Result<(), String> {
    beambench_service::ops::workflows::project::set_job_origin(&svc, anchor)
}
#[tauri::command]
pub fn set_user_origin(
    svc: State<'_, Arc<ServiceContext>>,
    x: f64,
    y: f64,
) -> Result<(), String> {
    beambench_service::ops::workflows::project::set_user_origin(&svc, x, y)
}
/// Merge a partial [`ProjectOptimizationPatch`] onto the open project's
/// `Project.optimization` block.
///
/// Patch-shaped (not full-object) to match the frontend's
/// `Partial<OptimizationSettings>` merge contract, so rapid UI edits to
/// different fields don't race to overwrite each other with stale
/// snapshots. `ProjectOptimization::apply_patch` returns `true` only
/// when a field actually changed; when nothing changed we short-circuit
/// the undo snapshot, the dirty-flag bump, and the plan-cache
/// invalidation — same no-op rule as `set_start_from_inner`.
#[tauri::command]
pub fn set_optimization(
    svc: State<'_, Arc<ServiceContext>>,
    patch: ProjectOptimizationPatch,
) -> Result<ProjectOptimization, String> {
    beambench_service::ops::workflows::project::set_optimization(&svc, patch)
}
#[tauri::command]
pub fn update_project_notes(
    svc: State<'_, Arc<ServiceContext>>,
    notes: String,
) -> Result<(), String> {
    beambench_service::ops::workflows::project::update_project_notes(&svc, notes)
}
/// M3: set the project's material thickness used as the absolute-Z reference for Focus Test.
///
/// Patch-shaped, no full-project replacement, no selection/preview/undo trashing. Only pushes an
/// undo snapshot and bumps `dirty` when the value actually changes — matches `set_optimization`'s
/// no-op short-circuit.
#[tauri::command]
pub fn set_material_height(
    svc: State<'_, Arc<ServiceContext>>,
    value: Option<f64>,
) -> Result<(), String> {
    beambench_service::ops::workflows::project::set_material_height(&svc, value)
}
#[tauri::command]
pub fn set_transform_locks(
    svc: State<'_, Arc<ServiceContext>>,
    locks: TransformLocks,
) -> Result<(), String> {
    beambench_service::ops::workflows::project::set_transform_locks(&svc, locks)
}
#[tauri::command]
pub fn set_objects_visible(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    visible: bool,
) -> Result<(), String> {
    beambench_service::ops::workflows::project::set_objects_visible(
        &svc,
        object_ids,
        visible,
    )
}
#[tauri::command]
pub fn reassign_layer(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    target_layer_id: String,
) -> Result<(), String> {
    beambench_service::ops::workflows::project::reassign_layer(
        &svc,
        object_ids,
        target_layer_id,
    )
}
#[tauri::command]
pub fn move_objects_in_outliner(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    target_layer_id: String,
    before_object_id: Option<String>,
) -> Result<(), String> {
    beambench_service::ops::workflows::project::move_objects_in_outliner(
        &svc,
        object_ids,
        target_layer_id,
        before_object_id,
    )
}
#[tauri::command]
pub fn select_open_shapes(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<Vec<String>, String> {
    beambench_service::ops::workflows::project::select_open_shapes(&svc)
}
#[tauri::command]
pub fn select_open_shapes_set_to_fill(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<Vec<String>, String> {
    beambench_service::ops::workflows::project::select_open_shapes_set_to_fill(&svc)
}
#[tauri::command]
pub fn select_contained_shapes(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<Vec<String>, String> {
    beambench_service::ops::workflows::project::select_contained_shapes(&svc, object_id)
}
#[tauri::command]
pub fn select_shapes_smaller_than_selected(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<Vec<String>, String> {
    beambench_service::ops::workflows::project::select_shapes_smaller_than_selected(
        &svc,
        object_ids,
    )
}
#[tauri::command]
pub fn delete_duplicates(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<Vec<String>, String> {
    beambench_service::ops::workflows::project::delete_duplicates(&svc, object_ids)
}
#[tauri::command]
pub fn count_duplicates(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<usize, String> {
    beambench_service::ops::workflows::project::count_duplicates(&svc, object_ids)
}
#[tauri::command]
pub fn auto_join_shapes(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    tolerance: f64,
) -> Result<Vec<ProjectObject>, String> {
    beambench_service::ops::workflows::project::auto_join_shapes(
        &svc,
        object_ids,
        tolerance,
    )
}
#[tauri::command]
pub fn optimize_shapes(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    tolerance: f64,
) -> Result<Vec<ProjectObject>, String> {
    beambench_service::ops::workflows::project::optimize_shapes(
        &svc,
        object_ids,
        tolerance,
    )
}
