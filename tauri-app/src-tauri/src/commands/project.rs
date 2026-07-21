use std::collections::HashSet;
use std::sync::Arc;

use beambench_common::path::VecPath;
use beambench_common::{
    AnchorPoint, Bounds, Id, Point2D, StartFromMode, Transform2D, TransformLocks,
};
use beambench_core::{
    CutEntry, CutEntryId, CutEntryPatch, CutEntryTemplate, Layer, LayerBatchToggle, LayerPatch,
    ObjectData, ObjectId, OperationType, Project, ProjectObject, ProjectOptimization,
    ProjectOptimizationPatch, RasterSettings, object::LayerRef,
    vector::convert::object_to_world_vecpath_resolved,
};
use beambench_service::ops::{planning, project as project_ops};
use beambench_service::{ServiceContext, UndoState};
use tauri::State;
use uuid::Uuid;

/// Parse a UUID string into a typed `Id<T>`.
pub fn parse_id<T>(id_str: &str) -> Result<Id<T>, String> {
    let uuid = Uuid::parse_str(id_str).map_err(|e| format!("Invalid ID '{}': {}", id_str, e))?;
    Ok(Id::from_uuid(uuid))
}

#[tauri::command]
pub fn create_project(
    svc: State<'_, Arc<ServiceContext>>,
    name: String,
) -> Result<Project, String> {
    project_ops::create_project(&svc, &name).map_err(Into::into)
}

#[tauri::command]
pub fn get_project(svc: State<'_, Arc<ServiceContext>>) -> Result<Option<Project>, String> {
    project_ops::get_project(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn close_project(svc: State<'_, Arc<ServiceContext>>) -> Result<(), String> {
    project_ops::close_project(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn get_undo_state(svc: State<'_, Arc<ServiceContext>>) -> Result<UndoState, String> {
    svc.undo_state()
}

#[tauri::command]
pub fn undo_project(svc: State<'_, Arc<ServiceContext>>) -> Result<Project, String> {
    project_ops::undo_project(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn redo_project(svc: State<'_, Arc<ServiceContext>>) -> Result<Project, String> {
    project_ops::redo_project(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn get_project_layers(svc: State<'_, Arc<ServiceContext>>) -> Result<Vec<Layer>, String> {
    project_ops::get_layers(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn add_layer(
    svc: State<'_, Arc<ServiceContext>>,
    name: String,
    operation: String,
) -> Result<Layer, String> {
    let op = parse_operation(&operation)?;
    project_ops::add_layer(
        &svc,
        project_ops::AddLayerInput {
            name,
            operation: op,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn update_layer(
    svc: State<'_, Arc<ServiceContext>>,
    layer_id: String,
    patch: LayerPatch,
) -> Result<Layer, String> {
    let id = parse_id(&layer_id)?;
    project_ops::update_layer(
        &svc,
        id,
        project_ops::UpdateLayerInput {
            name: patch.name,
            enabled: patch.enabled,
            visible: patch.visible,
            color_tag: patch.color_tag,
            ..Default::default()
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn remove_layer(svc: State<'_, Arc<ServiceContext>>, layer_id: String) -> Result<(), String> {
    let id = parse_id(&layer_id)?;
    project_ops::remove_layer(&svc, id).map_err(Into::into)
}

#[tauri::command]
pub fn reorder_layer(
    svc: State<'_, Arc<ServiceContext>>,
    layer_id: String,
    new_index: usize,
) -> Result<Vec<Layer>, String> {
    let id = parse_id(&layer_id)?;
    project_ops::reorder_layer(&svc, id, new_index).map_err(Into::into)
}

#[tauri::command]
pub fn get_project_objects(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<Vec<ProjectObject>, String> {
    project_ops::get_objects(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn add_object(
    svc: State<'_, Arc<ServiceContext>>,
    name: String,
    layer_id: String,
    object_data: ObjectData,
    bounds: Bounds,
) -> Result<ProjectObject, String> {
    let lid = parse_id(&layer_id)?;
    project_ops::add_object(
        &svc,
        project_ops::AddObjectInput {
            name,
            layer_id: lid,
            object_data,
            bounds,
        },
    )
    .map_err(Into::into)
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
    let lid = parse_id(&layer_id)?;
    // Name + operation are the minimum to mint a sibling layer; both must be
    // present together. The client never sends a pre-minted CutEntry or
    // CutEntryId — the backend owns id generation via `Layer::new_single_entry`.
    let create_layer = match (create_layer_name, create_layer_operation) {
        (Some(name), Some(operation)) => Some(project_ops::AddObjectLayerInput {
            name,
            color_tag: create_layer_color_tag,
            operation,
            entry_patch: create_layer_entry_patch,
        }),
        (None, None) => {
            if create_layer_entry_patch.is_some() || create_layer_color_tag.is_some() {
                return Err("create_layer_entry_patch / create_layer_color_tag require \
                     create_layer_name and create_layer_operation"
                    .into());
            }
            None
        }
        _ => {
            return Err(
                "create_layer_name and create_layer_operation must be provided together".into(),
            );
        }
    };
    project_ops::add_object_atomic(
        &svc,
        project_ops::AddObjectAtomicInput {
            name,
            layer_id: lid,
            object_data,
            bounds,
            create_layer,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn add_cut_entry(
    svc: State<'_, Arc<ServiceContext>>,
    layer_id: String,
    after_entry_id: Option<String>,
) -> Result<CutEntry, String> {
    let layer_id = parse_id(&layer_id)?;
    let after_entry_id = match after_entry_id {
        Some(id) => Some(parse_id::<beambench_common::markers::CutEntryMarker>(&id)?),
        None => None,
    };
    project_ops::add_cut_entry(&svc, layer_id, after_entry_id).map_err(Into::into)
}

/// M4: re-stamp every layer's order_index per the cut-strength heuristic. Atomic.
#[tauri::command]
pub fn sort_layers_cut_last(svc: State<'_, Arc<ServiceContext>>) -> Result<Vec<Layer>, String> {
    project_ops::sort_layers_cut_last(&svc).map_err(Into::into)
}

/// M4: batch toggle Layer.enabled (Output column) — Enable/Disable all, Invert, OnlyThisOn.
#[tauri::command]
pub fn set_all_layers_enabled(
    svc: State<'_, Arc<ServiceContext>>,
    mode: LayerBatchToggle,
) -> Result<Vec<Layer>, String> {
    project_ops::set_all_layers_enabled(&svc, mode).map_err(Into::into)
}

/// M4: batch toggle Layer.visible (Show column).
#[tauri::command]
pub fn set_all_layers_visible(
    svc: State<'_, Arc<ServiceContext>>,
    mode: LayerBatchToggle,
) -> Result<Vec<Layer>, String> {
    project_ops::set_all_layers_visible(&svc, mode).map_err(Into::into)
}

/// M4: reset a single cut entry to built-in defaults for its operation. Preserves entry id.
#[tauri::command]
pub fn reset_cut_entry_to_defaults(
    svc: State<'_, Arc<ServiceContext>>,
    layer_id: String,
    entry_id: String,
) -> Result<CutEntry, String> {
    let layer_id = parse_id(&layer_id)?;
    let entry_id: CutEntryId = parse_id(&entry_id)?;
    project_ops::reset_cut_entry_to_defaults(&svc, layer_id, entry_id).map_err(Into::into)
}

/// M4: replace a layer's `entries[]` from a clipboard template (Copy/Paste settings).
#[tauri::command]
pub fn paste_layer_entries(
    svc: State<'_, Arc<ServiceContext>>,
    layer_id: String,
    entries: Vec<CutEntryTemplate>,
) -> Result<Layer, String> {
    let layer_id = parse_id(&layer_id)?;
    project_ops::paste_layer_entries(&svc, layer_id, entries).map_err(Into::into)
}

#[tauri::command]
pub fn remove_cut_entry(
    svc: State<'_, Arc<ServiceContext>>,
    layer_id: String,
    entry_id: String,
) -> Result<(), String> {
    let layer_id = parse_id(&layer_id)?;
    let entry_id: CutEntryId = parse_id(&entry_id)?;
    project_ops::remove_cut_entry(&svc, layer_id, entry_id).map_err(Into::into)
}

#[tauri::command]
pub fn reorder_cut_entry(
    svc: State<'_, Arc<ServiceContext>>,
    layer_id: String,
    entry_id: String,
    new_index: usize,
) -> Result<Layer, String> {
    let layer_id = parse_id(&layer_id)?;
    let entry_id: CutEntryId = parse_id(&entry_id)?;
    project_ops::reorder_cut_entry(&svc, layer_id, entry_id, new_index).map_err(Into::into)
}

#[tauri::command]
pub fn update_cut_entry(
    svc: State<'_, Arc<ServiceContext>>,
    layer_id: String,
    entry_id: String,
    patch: CutEntryPatch,
) -> Result<CutEntry, String> {
    let layer_id = parse_id(&layer_id)?;
    let entry_id: CutEntryId = parse_id(&entry_id)?;
    project_ops::update_cut_entry(&svc, layer_id, entry_id, patch).map_err(Into::into)
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
    power_scale: Option<f64>,
    priority: Option<i32>,
) -> Result<ProjectObject, String> {
    let oid: ObjectId = parse_id(&object_id)?;
    let parsed_layer_id = match layer_id {
        Some(lid_str) => Some(parse_id(&lid_str)?),
        None => None,
    };
    project_ops::update_object(
        &svc,
        oid,
        project_ops::UpdateObjectInput {
            name,
            visible,
            locked,
            layer_id: parsed_layer_id,
            transform,
            bounds,
            lock_aspect_ratio,
            power_scale,
            priority,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn update_object_data(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    data: ObjectData,
) -> Result<ProjectObject, String> {
    let oid: ObjectId = parse_id(&object_id)?;
    project_ops::update_object_data(&svc, oid, data).map_err(Into::into)
}

#[tauri::command]
pub fn advance_auto_variable_text(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<Vec<ProjectObject>, String> {
    project_ops::advance_auto_variable_text(&svc).map_err(Into::into)
}

/// atomic apply for the Adjust Image dialog.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ApplyAdjustImageResult {
    pub object: ProjectObject,
    pub layer: Layer,
}

#[tauri::command]
pub fn apply_adjust_image_dialog(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    adjustments: Option<beambench_common::RasterAdjustments>,
    layer_id: String,
    raster_settings: RasterSettings,
) -> Result<ApplyAdjustImageResult, String> {
    let oid: ObjectId = parse_id(&object_id)?;
    let lid = parse_id(&layer_id)?;
    let (object, layer) =
        project_ops::apply_adjust_image_dialog(&svc, oid, adjustments, lid, raster_settings)
            .map_err(|e| String::from(e))?;
    Ok(ApplyAdjustImageResult { object, layer })
}

#[tauri::command]
pub fn resize_shape_object(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    bounds: Bounds,
) -> Result<ProjectObject, String> {
    let oid: ObjectId = parse_id(&object_id)?;
    project_ops::resize_shape_object(&svc, oid, bounds).map_err(Into::into)
}

#[tauri::command]
pub fn remove_object(svc: State<'_, Arc<ServiceContext>>, object_id: String) -> Result<(), String> {
    let oid: ObjectId = parse_id(&object_id)?;
    project_ops::remove_object(&svc, oid).map_err(Into::into)
}

#[tauri::command]
pub fn remove_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<usize, String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    project_ops::remove_objects(&svc, &parsed_ids).map_err(Into::into)
}

#[tauri::command]
pub fn set_text_guide_path(
    svc: State<'_, Arc<ServiceContext>>,
    text_id: String,
    guide_path_id: Option<String>,
) -> Result<ProjectObject, String> {
    let tid: ObjectId = parse_id(&text_id)?;
    let gid: Option<ObjectId> = guide_path_id.map(|s| parse_id(&s)).transpose()?;
    project_ops::set_text_guide_path(&svc, tid, gid).map_err(Into::into)
}

#[tauri::command]
pub fn nudge_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    dx: f64,
    dy: f64,
) -> Result<(), String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    project_ops::nudge_objects(&svc, &parsed_ids, dx, dy).map_err(Into::into)
}

#[tauri::command]
pub fn duplicate_object(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<ProjectObject, String> {
    let oid: ObjectId = parse_id(&object_id)?;
    project_ops::duplicate_object(&svc, oid).map_err(Into::into)
}

#[tauri::command]
pub fn duplicate_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<Vec<ProjectObject>, String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    project_ops::duplicate_objects(&svc, &parsed_ids).map_err(Into::into)
}

#[tauri::command]
pub fn duplicate_object_in_place(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<ProjectObject, String> {
    let oid: ObjectId = parse_id(&object_id)?;
    project_ops::duplicate_object_in_place(&svc, oid).map_err(Into::into)
}

#[tauri::command]
pub fn duplicate_objects_in_place(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<Vec<ProjectObject>, String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    project_ops::duplicate_objects_in_place(&svc, &parsed_ids).map_err(Into::into)
}

#[tauri::command]
pub fn paste_objects(
    svc: State<'_, Arc<ServiceContext>>,
    objects: Vec<ProjectObject>,
    in_place: bool,
) -> Result<Vec<ProjectObject>, String> {
    project_ops::paste_objects(&svc, &objects, in_place).map_err(Into::into)
}

#[tauri::command]
pub fn align_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    alignment_type: String,
    anchor_object_id: Option<String>,
) -> Result<Vec<ProjectObject>, String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let parsed_anchor = anchor_object_id.as_deref().map(parse_id).transpose()?;
    project_ops::align_objects(&svc, parsed_ids, &alignment_type, parsed_anchor).map_err(Into::into)
}

#[tauri::command]
pub fn distribute_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    direction: String,
) -> Result<Vec<ProjectObject>, String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    project_ops::distribute_objects(&svc, parsed_ids, &direction).map_err(Into::into)
}

#[tauri::command]
pub fn move_objects_together(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    axis: project_ops::MoveTogetherAxis,
    anchor_object_id: String,
) -> Result<Vec<ProjectObject>, String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let anchor_object_id = parse_id(&anchor_object_id)?;
    project_ops::move_objects_together(&svc, parsed_ids, axis, anchor_object_id).map_err(Into::into)
}

#[tauri::command]
pub fn mirror_across_line(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    axis_object_id: String,
) -> Result<Vec<ProjectObject>, String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let axis_object_id = parse_id(&axis_object_id)?;
    project_ops::mirror_across_line(&svc, parsed_ids, axis_object_id).map_err(Into::into)
}

#[tauri::command]
pub fn make_same_size(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    anchor_object_id: String,
    axis: project_ops::SameSizeAxis,
    preserve_aspect: bool,
) -> Result<Vec<ProjectObject>, String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let anchor_object_id = parse_id(&anchor_object_id)?;
    project_ops::make_same_size(&svc, parsed_ids, anchor_object_id, axis, preserve_aspect)
        .map_err(Into::into)
}

#[tauri::command]
pub fn dock_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    direction: project_ops::DockDirection,
    options: project_ops::DockOptions,
) -> Result<Vec<ProjectObject>, String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    project_ops::dock_objects(&svc, parsed_ids, direction, options).map_err(Into::into)
}

#[tauri::command]
pub fn resize_slots(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    options: project_ops::ResizeSlotsOptions,
) -> Result<Vec<ProjectObject>, String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    project_ops::resize_slots(&svc, parsed_ids, options).map_err(Into::into)
}

#[tauri::command]
pub fn bind_machine_profile(svc: State<'_, Arc<ServiceContext>>) -> Result<Project, String> {
    project_ops::bind_active_machine_profile(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn replace_project(
    svc: State<'_, Arc<ServiceContext>>,
    project: Project,
) -> Result<(), String> {
    project_ops::replace_project(&svc, project).map_err(String::from)?;
    svc.clear_project_history()?;
    Ok(())
}

fn parse_operation(s: &str) -> Result<OperationType, String> {
    match s {
        "image" => Ok(OperationType::Image),
        "line" => Ok(OperationType::Line),
        "fill" => Ok(OperationType::Fill),
        "score" => Ok(OperationType::Score),
        "cut" => Ok(OperationType::Cut),
        "offset_fill" => Ok(OperationType::OffsetFill),
        "tool" => Ok(OperationType::Tool),
        _ => Err(format!("Invalid operation type: {}", s)),
    }
}

struct AutoJoinPlan {
    source_ids: Vec<ObjectId>,
    requested_layer: LayerRef,
    source_name: String,
    joined_paths: Vec<VecPath>,
}

fn vecpath_to_project_object(name: String, layer_id: LayerRef, path: VecPath) -> ProjectObject {
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

fn replace_object_with_path(
    project: &mut Project,
    object_id: ObjectId,
    path: VecPath,
) -> Result<ProjectObject, String> {
    let bounds = path
        .visual_bounds()
        .unwrap_or_else(|| Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(0.0, 0.0)));
    let closed = path.subpaths.iter().any(|sp| sp.closed);
    let obj = project
        .find_object_mut(object_id)
        .ok_or("Object not found")?;
    let current_layer = obj.layer_id;
    obj.data = ObjectData::VectorPath {
        path_data: path.to_svg_d(),
        closed,
        ruler_guide_axis: None,
    };
    obj.bounds = bounds;
    obj.transform = Transform2D::identity();
    obj.tabs.clear();
    obj.start_point_edits.clear();
    let (dest_layer, _rerouted) = beambench_service::resolve_layer_for_object(
        project,
        current_layer,
        beambench_service::RoutingTarget::NeedsNonImage,
    )
    .map_err(|e| e.to_string())?;
    if dest_layer != current_layer {
        if let Some(o) = project.find_object_mut(object_id) {
            o.layer_id = dest_layer;
        }
    }
    Ok(project
        .find_object(object_id)
        .ok_or("Object not found")?
        .clone())
}

fn plan_auto_join_shapes(
    project: &Project,
    object_ids: &[ObjectId],
    tolerance: f64,
) -> Result<Option<AutoJoinPlan>, String> {
    let mut source_ids = Vec::new();
    let mut vec_paths = Vec::new();
    let mut input_svgs = Vec::new();
    let mut requested_layer = None;
    let mut source_name = None;

    for &id in object_ids {
        let Some(obj) = project.find_object(id) else {
            continue;
        };
        let Some(world_path) = object_to_world_vecpath_resolved(obj, project) else {
            continue;
        };
        if requested_layer.is_none() {
            requested_layer = Some(obj.layer_id);
        }
        if source_name.is_none() {
            source_name = Some(obj.name.clone());
        }
        input_svgs.push(world_path.to_svg_d());
        vec_paths.push(world_path);
        source_ids.push(id);
    }

    if vec_paths.is_empty() {
        return Ok(None);
    }

    let joined_paths = beambench_core::vector::path_ops::auto_join_paths(&vec_paths, tolerance);
    let output_svgs: Vec<String> = joined_paths.iter().map(VecPath::to_svg_d).collect();
    if output_svgs == input_svgs {
        return Ok(None);
    }

    Ok(Some(AutoJoinPlan {
        source_ids,
        requested_layer: requested_layer.ok_or("No vector-capable objects selected")?,
        source_name: source_name.unwrap_or_else(|| "Auto-Join".to_string()),
        joined_paths,
    }))
}

fn apply_auto_join_plan(
    project: &mut Project,
    plan: AutoJoinPlan,
) -> Result<Vec<ProjectObject>, String> {
    let (layer_id, _rerouted) = beambench_service::resolve_layer_for_object(
        project,
        plan.requested_layer,
        beambench_service::RoutingTarget::NeedsNonImage,
    )
    .map_err(|e| e.to_string())?;

    for id in plan.source_ids {
        project.remove_object(id);
    }

    let total = plan.joined_paths.len();
    let mut created = Vec::with_capacity(total);
    for (idx, path) in plan.joined_paths.into_iter().enumerate() {
        let name = if total == 1 {
            format!("{} Auto-Joined", plan.source_name)
        } else {
            format!("{} Auto-Joined {}", plan.source_name, idx + 1)
        };
        let obj = vecpath_to_project_object(name, layer_id, path);
        created.push(project.add_object(obj).clone());
    }
    project.dirty = true;
    Ok(created)
}

fn plan_optimize_shapes(
    project: &Project,
    object_ids: &[ObjectId],
    tolerance: f64,
) -> Vec<(ObjectId, VecPath)> {
    let mut changes = Vec::new();
    for &id in object_ids {
        let Some(obj) = project.find_object(id) else {
            continue;
        };
        let Some(world_path) = object_to_world_vecpath_resolved(obj, project) else {
            continue;
        };
        let optimized = beambench_core::vector::path_ops::optimize_path(&world_path, tolerance);
        if optimized.to_svg_d() != world_path.to_svg_d() {
            changes.push((id, optimized));
        }
    }
    changes
}

fn apply_optimize_plan(
    project: &mut Project,
    changes: Vec<(ObjectId, VecPath)>,
) -> Result<Vec<ProjectObject>, String> {
    let mut updated = Vec::with_capacity(changes.len());
    for (id, path) in changes {
        updated.push(replace_object_with_path(project, id, path)?);
    }
    if !updated.is_empty() {
        project.dirty = true;
    }
    Ok(updated)
}

// ---------------------------------------------------------------------------
// Project and layer commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn set_layer_visible(
    svc: State<'_, Arc<ServiceContext>>,
    layer_id: String,
    visible: bool,
) -> Result<bool, String> {
    let lid = parse_id(&layer_id)?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    svc.push_project_undo_snapshot(project)?;
    Ok(beambench_core::operations::set_layer_visible(
        project, lid, visible,
    ))
}

#[tauri::command]
pub fn set_layer_air_assist(
    svc: State<'_, Arc<ServiceContext>>,
    layer_id: String,
    air_assist: bool,
) -> Result<bool, String> {
    let lid = parse_id(&layer_id)?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    svc.push_project_undo_snapshot(project)?;
    Ok(beambench_core::operations::set_layer_air_assist(
        project, lid, air_assist,
    ))
}

#[tauri::command]
pub fn select_all_in_layer(
    svc: State<'_, Arc<ServiceContext>>,
    layer_id: String,
) -> Result<Vec<String>, String> {
    let lid = parse_id(&layer_id)?;
    let guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_ref().ok_or("No project open")?;
    let ids = beambench_core::operations::select_all_in_layer(project, lid);
    Ok(ids.into_iter().map(|id| id.to_string()).collect())
}

#[tauri::command]
pub fn push_draw_order(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    direction: String,
) -> Result<(), String> {
    let direction = match direction.as_str() {
        "forward" => beambench_core::operations::DrawOrderDirection::Forward,
        "backward" => beambench_core::operations::DrawOrderDirection::Backward,
        "front" => beambench_core::operations::DrawOrderDirection::Front,
        "back" => beambench_core::operations::DrawOrderDirection::Back,
        _ => return Err(format!("Unsupported draw order direction: {direction}")),
    };
    let oid: ObjectId = parse_id(&object_id)?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    svc.push_project_undo_snapshot(project)?;
    beambench_core::operations::push_draw_order(project, oid, direction);
    Ok(())
}

#[tauri::command]
pub fn lock_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<(), String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    svc.push_project_undo_snapshot(project)?;
    beambench_core::operations::lock_objects(project, &parsed_ids);
    Ok(())
}

#[tauri::command]
pub fn unlock_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<(), String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    svc.push_project_undo_snapshot(project)?;
    beambench_core::operations::unlock_objects(project, &parsed_ids);
    Ok(())
}

#[tauri::command]
pub fn flip_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    horizontal: bool,
    pivot_x: Option<f64>,
    pivot_y: Option<f64>,
) -> Result<(), String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    svc.push_project_undo_snapshot(project)?;
    let pivot = match (pivot_x, pivot_y) {
        (Some(x), Some(y)) => Some(Point2D::new(x, y)),
        _ => None,
    };
    beambench_core::operations::flip_objects(project, &parsed_ids, horizontal, pivot);
    Ok(())
}

#[tauri::command]
pub fn rotate_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    degrees: f64,
    pivot_x: Option<f64>,
    pivot_y: Option<f64>,
) -> Result<(), String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let pivot = match (pivot_x, pivot_y) {
        (Some(x), Some(y)) => Some(beambench_common::Point2D::new(x, y)),
        _ => None,
    };
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    svc.push_project_undo_snapshot(project)?;
    beambench_core::operations::rotate_objects(project, &parsed_ids, degrees, pivot);
    Ok(())
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
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let active_id: ObjectId = parse_id(&active_object_id)?;
    let pivot = match (pivot_x, pivot_y) {
        (Some(x), Some(y)) => Some(beambench_common::Point2D::new(x, y)),
        _ => None,
    };
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;

    let mut next_project = project.clone();
    beambench_core::operations::rotate_objects(&mut next_project, &parsed_ids, degrees, pivot);
    let baked =
        beambench_core::operations::bake_object_transform_to_path(&mut next_project, active_id)
            .ok_or_else(|| "Active object cannot be baked to path".to_string())?;

    svc.push_project_undo_snapshot(project)?;
    *project = next_project;
    Ok(baked)
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
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let pivot = match (pivot_x, pivot_y) {
        (Some(x), Some(y)) => Some(beambench_common::Point2D::new(x, y)),
        _ => None,
    };
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    svc.push_project_undo_snapshot(project)?;
    beambench_core::operations::shear_objects(project, &parsed_ids, shear_x, shear_y, pivot);
    Ok(())
}

#[derive(serde::Deserialize)]
pub struct BoundsEntry {
    pub id: String,
    pub bounds: Bounds,
}

/// Batch-update object bounds. Vector path objects are refitted atomically within
/// the same undo snapshot.
#[tauri::command]
pub fn update_object_bounds_batch(
    svc: State<'_, Arc<ServiceContext>>,
    entries: Vec<BoundsEntry>,
) -> Result<(), String> {
    let parsed: Vec<(ObjectId, Bounds)> = entries
        .iter()
        .map(|e| {
            let oid: ObjectId = parse_id(&e.id)?;
            Ok((oid, e.bounds))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    svc.push_project_undo_snapshot(project)?;
    beambench_core::operations::update_object_bounds_batch(project, &parsed);
    Ok(())
}

#[tauri::command]
pub fn move_objects_to(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    x: f64,
    y: f64,
) -> Result<(), String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    svc.push_project_undo_snapshot(project)?;
    beambench_core::operations::move_objects_to_position(project, &parsed_ids, x, y);
    Ok(())
}

#[tauri::command]
pub fn set_start_from(
    svc: State<'_, Arc<ServiceContext>>,
    mode: StartFromMode,
) -> Result<(), String> {
    set_start_from_inner(&svc, mode)
}

fn set_start_from_inner(svc: &ServiceContext, mode: StartFromMode) -> Result<(), String> {
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    if project.start_from == mode {
        return Ok(());
    }
    svc.push_project_undo_snapshot(project)?;
    project.start_from = mode;
    project.dirty = true;
    Ok(())
}

#[tauri::command]
pub fn set_job_origin(
    svc: State<'_, Arc<ServiceContext>>,
    anchor: AnchorPoint,
) -> Result<(), String> {
    set_job_origin_inner(&svc, anchor)
}

fn set_job_origin_inner(svc: &ServiceContext, anchor: AnchorPoint) -> Result<(), String> {
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    if project.job_origin == anchor {
        return Ok(());
    }
    svc.push_project_undo_snapshot(project)?;
    project.job_origin = anchor;
    project.dirty = true;
    Ok(())
}

#[tauri::command]
pub fn set_user_origin(svc: State<'_, Arc<ServiceContext>>, x: f64, y: f64) -> Result<(), String> {
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    svc.push_project_undo_snapshot(project)?;
    project.user_origin = Some((x, y));
    project.dirty = true;
    Ok(())
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
    set_optimization_inner(&svc, patch)
}

fn set_optimization_inner(
    svc: &ServiceContext,
    patch: ProjectOptimizationPatch,
) -> Result<ProjectOptimization, String> {
    let (changed, optimization) = {
        let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
        let project = guard.as_mut().ok_or("No project open")?;
        // Peek at the patch outcome before snapshotting undo so a no-op
        // call doesn't push a useless history entry. `apply_patch`
        // returns true iff any field's value actually changed.
        let mut probe = project.optimization.clone();
        if !probe.apply_patch(&patch) {
            return Ok(project.optimization.clone());
        }
        svc.push_project_undo_snapshot(project)?;
        project.optimization = probe;
        project.dirty = true;
        (true, project.optimization.clone())
    };
    if changed {
        // Plan cache depends on `project.optimization` via
        // `PlannerInput`, so any flag toggle must invalidate.
        planning::invalidate_plan_cache(svc).map_err(|e| format!("{e}"))?;
    }
    Ok(optimization)
}

#[tauri::command]
pub fn update_project_notes(
    svc: State<'_, Arc<ServiceContext>>,
    notes: String,
) -> Result<(), String> {
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    svc.push_project_undo_snapshot(project)?;
    project.notes = notes;
    project.dirty = true;
    Ok(())
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
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    if project.material_height_mm == value {
        return Ok(());
    }
    svc.push_project_undo_snapshot(project)?;
    project.material_height_mm = value;
    project.dirty = true;
    Ok(())
}

#[tauri::command]
pub fn set_transform_locks(
    svc: State<'_, Arc<ServiceContext>>,
    locks: TransformLocks,
) -> Result<(), String> {
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    svc.push_project_undo_snapshot(project)?;
    project.transform_locks = locks;
    project.dirty = true;
    Ok(())
}

#[tauri::command]
pub fn set_objects_visible(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    visible: bool,
) -> Result<(), String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    svc.push_project_undo_snapshot(project)?;
    beambench_core::operations::set_objects_visible(project, &parsed_ids, visible);
    Ok(())
}

#[tauri::command]
pub fn reassign_layer(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    target_layer_id: String,
) -> Result<(), String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let tlid = parse_id(&target_layer_id)?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;

    // Validate each object against the target layer's content-type
    // invariant before committing. This ensures reassignment goes
    // through the same validation path as add_object / update_object
    // / import, preventing family-routing mismatches or mixed-content
    // state from direct palette reassigns.
    let dest_layer = project
        .layers
        .iter()
        .find(|l| l.id == tlid)
        .ok_or("Target layer not found")?;
    for &oid in &parsed_ids {
        let obj = project.find_object(oid).ok_or("Object not found")?;
        beambench_service::check_layer_content_invariant(&obj.data, dest_layer, project)
            .map_err(|e| e.to_string())?;
    }

    svc.push_project_undo_snapshot(project)?;
    beambench_core::operations::reassign_layer(project, &parsed_ids, tlid);
    Ok(())
}

#[tauri::command]
pub fn select_open_shapes(svc: State<'_, Arc<ServiceContext>>) -> Result<Vec<String>, String> {
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    let ids = select_open_shape_ids(project, None);
    flatten_selected_group_children(&svc, project, &ids)?;
    let result = ids.into_iter().map(|id| id.to_string()).collect();
    drop(guard);
    planning::invalidate_plan_cache(&svc).map_err(String::from)?;
    Ok(result)
}

#[tauri::command]
pub fn select_open_shapes_set_to_fill(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<Vec<String>, String> {
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    let ids = select_open_shape_ids(project, Some(true));
    flatten_selected_group_children(&svc, project, &ids)?;
    let result = ids.into_iter().map(|id| id.to_string()).collect();
    drop(guard);
    planning::invalidate_plan_cache(&svc).map_err(String::from)?;
    Ok(result)
}

#[tauri::command]
pub fn select_contained_shapes(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<Vec<String>, String> {
    let object_id = parse_id(&object_id)?;
    let guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_ref().ok_or("No project open")?;
    let ids = select_contained_shape_ids(project, object_id)?;
    Ok(ids.into_iter().map(|id| id.to_string()).collect())
}

fn select_contained_shape_ids(
    project: &Project,
    object_id: ObjectId,
) -> Result<Vec<ObjectId>, String> {
    let reference = project.find_object(object_id).ok_or("Object not found")?;
    if !is_closed_vector_compatible_reference(project, reference) {
        return Err(
            "Select Contained Shapes requires one closed vector-compatible reference".to_string(),
        );
    }
    let grouped_ids = grouped_child_ids(project);
    Ok(project
        .objects
        .iter()
        .filter(|object| object.id == object_id || !grouped_ids.contains(&object.id))
        .filter(|object| {
            object.id == object_id || contains_bounds(&reference.bounds, &object.bounds)
        })
        .map(|object| object.id)
        .collect())
}

fn is_closed_vector_compatible_reference(project: &Project, object: &ProjectObject) -> bool {
    let resolved = project.resolve_clone(object);
    let effective = resolved.as_ref().unwrap_or(object);
    match &effective.data {
        ObjectData::VectorPath { closed, .. } => *closed,
        ObjectData::Shape { .. }
        | ObjectData::Text { .. }
        | ObjectData::Polygon { .. }
        | ObjectData::Star { .. } => true,
        _ => false,
    }
}

#[tauri::command]
pub fn select_shapes_smaller_than_selected(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<Vec<String>, String> {
    let reference_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|id| parse_id(id))
        .collect::<Result<Vec<_>, _>>()?;
    select_shapes_smaller_than_selected_inner(&svc, &reference_ids)
}

fn select_shapes_smaller_than_selected_inner(
    svc: &ServiceContext,
    reference_ids: &[ObjectId],
) -> Result<Vec<String>, String> {
    if reference_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    let threshold = reference_ids
        .iter()
        .filter_map(|id| project.find_object(*id))
        .map(|object| bounds_area(&object.bounds))
        .fold(0.0, f64::max);
    let mut ids: Vec<ObjectId> = reference_ids.to_vec();
    ids.extend(
        project
            .objects
            .iter()
            .filter(|object| !reference_ids.contains(&object.id))
            .filter(|object| !matches!(object.data, ObjectData::Group { .. }))
            .filter(|object| bounds_area(&object.bounds) < threshold)
            .map(|object| object.id),
    );
    flatten_selected_group_children(&svc, project, &ids)?;
    let result = ids.into_iter().map(|id| id.to_string()).collect();
    drop(guard);
    planning::invalidate_plan_cache(&svc).map_err(String::from)?;
    Ok(result)
}

fn select_open_shape_ids(project: &Project, fill_only: Option<bool>) -> Vec<ObjectId> {
    project
        .objects
        .iter()
        .filter(|obj| matches!(&obj.data, ObjectData::VectorPath { closed, .. } if !closed))
        .filter(|obj| {
            if fill_only != Some(true) {
                return true;
            }
            project
                .find_layer(obj.layer_id)
                .and_then(|layer| layer.entries.first())
                .is_some_and(|entry| {
                    matches!(
                        entry.operation,
                        OperationType::Fill | OperationType::OffsetFill
                    )
                })
        })
        .map(|obj| obj.id)
        .collect()
}

fn flatten_selected_group_children(
    svc: &ServiceContext,
    project: &mut Project,
    ids: &[ObjectId],
) -> Result<(), String> {
    let selected: HashSet<ObjectId> = ids.iter().copied().collect();
    let changed = project.objects.iter().any(|object| match &object.data {
        ObjectData::Group { children } => children.iter().any(|child| selected.contains(child)),
        _ => false,
    });
    if !changed {
        return Ok(());
    }
    svc.push_project_undo_snapshot(project)?;
    for object in &mut project.objects {
        let ObjectData::Group { children } = &mut object.data else {
            continue;
        };
        children.retain(|child| !selected.contains(child));
    }
    project.objects.retain(
        |object| !matches!(&object.data, ObjectData::Group { children } if children.is_empty()),
    );
    project.dirty = true;
    Ok(())
}

fn grouped_child_ids(project: &Project) -> HashSet<ObjectId> {
    project
        .objects
        .iter()
        .filter_map(|object| match &object.data {
            ObjectData::Group { children } => Some(children),
            _ => None,
        })
        .flatten()
        .copied()
        .collect()
}

fn contains_bounds(reference: &Bounds, candidate: &Bounds) -> bool {
    candidate.min.x >= reference.min.x
        && candidate.max.x <= reference.max.x
        && candidate.min.y >= reference.min.y
        && candidate.max.y <= reference.max.y
}

fn bounds_area(bounds: &Bounds) -> f64 {
    (bounds.max.x - bounds.min.x).abs() * (bounds.max.y - bounds.min.y).abs()
}

#[tauri::command]
pub fn delete_duplicates(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<Vec<String>, String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    delete_duplicates_inner(&svc, &parsed_ids)
}

#[tauri::command]
pub fn count_duplicates(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<usize, String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_ref().ok_or("No project open")?;
    Ok(duplicate_ids_for_scope(project, &parsed_ids).len())
}

fn duplicate_ids_for_scope(project: &Project, parsed_ids: &[ObjectId]) -> HashSet<ObjectId> {
    let selected: HashSet<ObjectId> = parsed_ids.iter().copied().collect();
    let selection_scope = parsed_ids.len() >= 2;

    // Scope Delete Duplicates to a meaningful multi-selection,
    // otherwise to the whole project. Locked objects can act as duplicate
    // references but are never deleted.
    let mut subset: Vec<_> = project
        .objects
        .iter()
        .filter(|o| !selection_scope || selected.contains(&o.id))
        .cloned()
        .collect();
    subset.sort_by_key(|object| !object.locked);

    beambench_core::vector::path_ops::find_duplicates(&subset)
        .into_iter()
        .filter(|id| {
            project
                .find_object(*id)
                .is_some_and(|object| !object.locked && (!selection_scope || selected.contains(id)))
        })
        .collect()
}

fn delete_duplicates_inner(
    svc: &ServiceContext,
    parsed_ids: &[ObjectId],
) -> Result<Vec<String>, String> {
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    let selection_scope = parsed_ids.len() >= 2;
    let dup_ids = duplicate_ids_for_scope(project, parsed_ids);
    if dup_ids.is_empty() {
        return Ok(parsed_ids.iter().map(|id| id.to_string()).collect());
    }

    svc.push_project_undo_snapshot(project)?;
    let dup_vec: Vec<ObjectId> = dup_ids.iter().copied().collect();
    project.remove_objects(&dup_vec);
    project.clean_empty_layers();
    let remaining = if parsed_ids.is_empty() || !selection_scope {
        Vec::new()
    } else {
        parsed_ids
            .iter()
            .filter(|id| !dup_ids.contains(*id))
            .map(|id| id.to_string())
            .collect()
    };
    drop(guard);
    planning::invalidate_plan_cache(svc).map_err(String::from)?;
    Ok(remaining)
}

#[tauri::command]
pub fn auto_join_shapes(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    tolerance: f64,
) -> Result<Vec<ProjectObject>, String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    let Some(plan) = plan_auto_join_shapes(project, &parsed_ids, tolerance)? else {
        return Ok(vec![]);
    };
    svc.push_project_undo_snapshot(project)?;
    let updated = apply_auto_join_plan(project, plan)?;
    drop(guard);
    planning::invalidate_plan_cache(&svc).map_err(String::from)?;
    Ok(updated)
}

#[tauri::command]
pub fn optimize_shapes(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    tolerance: f64,
) -> Result<Vec<ProjectObject>, String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    let changes = plan_optimize_shapes(project, &parsed_ids, tolerance);
    if changes.is_empty() {
        return Ok(vec![]);
    }
    svc.push_project_undo_snapshot(project)?;
    let updated = apply_optimize_plan(project, changes)?;
    drop(guard);
    planning::invalidate_plan_cache(&svc).map_err(String::from)?;
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::{
        apply_auto_join_plan, apply_optimize_plan, delete_duplicates_inner,
        duplicate_ids_for_scope, flatten_selected_group_children, plan_auto_join_shapes,
        plan_optimize_shapes, select_contained_shape_ids, select_open_shape_ids,
        select_shapes_smaller_than_selected_inner, set_job_origin_inner, set_optimization_inner,
        set_start_from_inner,
    };
    use beambench_common::{AnchorPoint, Bounds, Point2D, StartFromMode};
    use beambench_core::{
        Layer, ObjectData, OperationType, Project, ProjectObject, ProjectOptimizationPatch,
    };
    use beambench_service::ServiceContext;

    fn add_vector_path(
        project: &mut Project,
        layer_id: beambench_core::object::LayerRef,
        name: &str,
        path_data: &str,
    ) -> ProjectObject {
        project
            .add_object(ProjectObject::new(
                name,
                layer_id,
                Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
                ObjectData::VectorPath {
                    path_data: path_data.to_string(),
                    closed: path_data.ends_with('Z'),
                    ruler_guide_axis: None,
                },
            ))
            .clone()
    }

    #[test]
    fn select_open_shapes_set_to_fill_includes_fill_modes_and_flattens_grouped_matches() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("test");
        let fill_layer = Layer::new("Fill", OperationType::Fill);
        let fill_layer_id = fill_layer.id;
        let offset_fill_layer = Layer::new("Offset Fill", OperationType::OffsetFill);
        let offset_fill_layer_id = offset_fill_layer.id;
        let line_layer = Layer::new("Line", OperationType::Line);
        let line_layer_id = line_layer.id;
        project.add_layer(fill_layer);
        project.add_layer(offset_fill_layer);
        project.add_layer(line_layer);

        let fill_open = add_vector_path(&mut project, fill_layer_id, "fill", "M0 0 L1 1");
        let offset_fill_open = add_vector_path(
            &mut project,
            offset_fill_layer_id,
            "offset fill",
            "M2 2 L3 3",
        );
        let line_open = add_vector_path(&mut project, line_layer_id, "line", "M4 4 L5 5");
        project.add_object(ProjectObject::new(
            "group",
            fill_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::Group {
                children: vec![fill_open.id],
            },
        ));

        let ids = select_open_shape_ids(&project, Some(true));
        assert!(ids.contains(&fill_open.id));
        assert!(ids.contains(&offset_fill_open.id));
        assert!(!ids.contains(&line_open.id));

        flatten_selected_group_children(&ctx, &mut project, &ids).unwrap();

        assert!(ctx.undo_state().unwrap().can_undo);
        assert!(project.objects.iter().all(|object| match &object.data {
            ObjectData::Group { children } => !children.contains(&fill_open.id),
            _ => true,
        }));
        assert!(project.dirty);
    }

    #[test]
    fn select_open_shapes_includes_all_open_paths_and_flattens_grouped_matches() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("test");
        let fill_layer = Layer::new("Fill", OperationType::Fill);
        let fill_layer_id = fill_layer.id;
        let line_layer = Layer::new("Line", OperationType::Line);
        let line_layer_id = line_layer.id;
        project.add_layer(fill_layer);
        project.add_layer(line_layer);

        let fill_open = add_vector_path(&mut project, fill_layer_id, "fill", "M0 0 L1 1");
        let line_open = add_vector_path(&mut project, line_layer_id, "line", "M4 4 L5 5");
        let closed = add_vector_path(&mut project, line_layer_id, "closed", "M0 0 L1 0 Z");
        project.add_object(ProjectObject::new(
            "group",
            fill_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::Group {
                children: vec![line_open.id],
            },
        ));

        let ids = select_open_shape_ids(&project, None);
        assert!(ids.contains(&fill_open.id));
        assert!(ids.contains(&line_open.id));
        assert!(!ids.contains(&closed.id));

        flatten_selected_group_children(&ctx, &mut project, &ids).unwrap();

        assert!(ctx.undo_state().unwrap().can_undo);
        assert!(project.objects.iter().all(|object| match &object.data {
            ObjectData::Group { children } => !children.contains(&line_open.id),
            _ => true,
        }));
    }

    #[test]
    fn select_contained_shapes_skips_contained_group_children() {
        let mut project = Project::new("test");
        let layer_id = project.ensure_default_layer();
        let reference = project
            .add_object(ProjectObject::new(
                "reference",
                layer_id,
                Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 100.0)),
                ObjectData::VectorPath {
                    path_data: "M0 0 L100 0 L100 100 L0 100 Z".to_string(),
                    closed: true,
                    ruler_guide_axis: None,
                },
            ))
            .clone();
        let contained_top_level = project
            .add_object(ProjectObject::new(
                "inside",
                layer_id,
                Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(20.0, 20.0)),
                ObjectData::Shape {
                    kind: beambench_core::object::ShapeKind::Rectangle,
                    width: 10.0,
                    height: 10.0,
                    corner_radius: 0.0,
                },
            ))
            .clone();
        let contained_group_child = project
            .add_object(ProjectObject::new(
                "group child",
                layer_id,
                Bounds::new(Point2D::new(30.0, 30.0), Point2D::new(40.0, 40.0)),
                ObjectData::Shape {
                    kind: beambench_core::object::ShapeKind::Rectangle,
                    width: 10.0,
                    height: 10.0,
                    corner_radius: 0.0,
                },
            ))
            .clone();
        project.add_object(ProjectObject::new(
            "group",
            layer_id,
            Bounds::new(Point2D::new(30.0, 30.0), Point2D::new(40.0, 40.0)),
            ObjectData::Group {
                children: vec![contained_group_child.id],
            },
        ));

        let ids = select_contained_shape_ids(&project, reference.id).unwrap();

        assert!(ids.contains(&reference.id));
        assert!(ids.contains(&contained_top_level.id));
        assert!(!ids.contains(&contained_group_child.id));
    }

    #[test]
    fn select_contained_shapes_rejects_open_vector_reference() {
        let mut project = Project::new("test");
        let layer_id = project.ensure_default_layer();
        let reference = add_vector_path(&mut project, layer_id, "open", "M0 0 L100 0");

        let err = select_contained_shape_ids(&project, reference.id).unwrap_err();

        assert!(err.contains("closed vector-compatible reference"));
    }

    #[test]
    fn select_shapes_smaller_than_selected_flattens_matching_group_children_only() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("test");
        let layer_id = project.ensure_default_layer();
        let reference = project
            .add_object(ProjectObject::new(
                "reference",
                layer_id,
                Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 100.0)),
                ObjectData::Shape {
                    kind: beambench_core::object::ShapeKind::Rectangle,
                    width: 100.0,
                    height: 100.0,
                    corner_radius: 0.0,
                },
            ))
            .clone();
        let small_child = project
            .add_object(ProjectObject::new(
                "small child",
                layer_id,
                Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
                ObjectData::Shape {
                    kind: beambench_core::object::ShapeKind::Rectangle,
                    width: 10.0,
                    height: 10.0,
                    corner_radius: 0.0,
                },
            ))
            .clone();
        let larger_sibling = project
            .add_object(ProjectObject::new(
                "larger sibling",
                layer_id,
                Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(200.0, 200.0)),
                ObjectData::Shape {
                    kind: beambench_core::object::ShapeKind::Rectangle,
                    width: 200.0,
                    height: 200.0,
                    corner_radius: 0.0,
                },
            ))
            .clone();
        let group = project
            .add_object(ProjectObject::new(
                "group",
                layer_id,
                Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(200.0, 200.0)),
                ObjectData::Group {
                    children: vec![small_child.id, larger_sibling.id],
                },
            ))
            .clone();
        *ctx.project.lock().unwrap() = Some(project);

        let ids = select_shapes_smaller_than_selected_inner(&ctx, &[reference.id]).unwrap();

        assert!(ids.contains(&reference.id.to_string()));
        assert!(ids.contains(&small_child.id.to_string()));
        assert!(!ids.contains(&larger_sibling.id.to_string()));
        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        let group = project.find_object(group.id).unwrap();
        let ObjectData::Group { children } = &group.data else {
            panic!("expected remaining group");
        };
        assert!(!children.contains(&small_child.id));
        assert!(children.contains(&larger_sibling.id));
    }

    #[test]
    fn delete_duplicates_scopes_to_selection_or_all_unlocked_objects() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("test");
        let layer_id = project.ensure_default_layer();
        let original = add_vector_path(&mut project, layer_id, "original", "M0 0 L10 0");
        let selected_duplicate = add_vector_path(&mut project, layer_id, "duplicate", "M0 0 L10 0");
        let unselected_duplicate =
            add_vector_path(&mut project, layer_id, "duplicate 2", "M0 0 L10 0");
        *ctx.project.lock().unwrap() = Some(project);

        let selected_remaining =
            delete_duplicates_inner(&ctx, &[original.id, selected_duplicate.id]).unwrap();
        assert_eq!(selected_remaining, vec![original.id.to_string()]);
        {
            let guard = ctx.project.lock().unwrap();
            let project = guard.as_ref().unwrap();
            assert!(project.find_object(original.id).is_some());
            assert!(project.find_object(selected_duplicate.id).is_none());
            assert!(project.find_object(unselected_duplicate.id).is_some());
        }

        let all_remaining = delete_duplicates_inner(&ctx, &[]).unwrap();
        assert!(all_remaining.is_empty());
        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        assert!(project.find_object(original.id).is_some());
        assert!(project.find_object(unselected_duplicate.id).is_none());
    }

    #[test]
    fn delete_duplicates_single_selection_scans_all_objects() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("test");
        let layer_id = project.ensure_default_layer();
        let original = add_vector_path(&mut project, layer_id, "original", "M0 0 L10 0");
        let selected_duplicate = add_vector_path(&mut project, layer_id, "duplicate", "M0 0 L10 0");
        *ctx.project.lock().unwrap() = Some(project);

        let remaining = delete_duplicates_inner(&ctx, &[selected_duplicate.id]).unwrap();

        assert!(remaining.is_empty());
        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        assert!(project.find_object(original.id).is_some());
        assert!(project.find_object(selected_duplicate.id).is_none());
    }

    #[test]
    fn duplicate_count_matches_delete_scope() {
        let mut project = Project::new("test");
        let layer_id = project.ensure_default_layer();
        let original = add_vector_path(&mut project, layer_id, "original", "M0 0 L10 0");
        let selected_duplicate = add_vector_path(&mut project, layer_id, "duplicate", "M0 0 L10 0");
        let unselected_duplicate =
            add_vector_path(&mut project, layer_id, "duplicate 2", "M0 0 L10 0");

        assert_eq!(
            duplicate_ids_for_scope(&project, &[original.id, selected_duplicate.id]).len(),
            1
        );
        assert_eq!(
            duplicate_ids_for_scope(&project, &[selected_duplicate.id]).len(),
            2
        );
        assert_eq!(duplicate_ids_for_scope(&project, &[]).len(), 2);
        assert!(project.find_object(unselected_duplicate.id).is_some());
    }

    #[test]
    fn delete_duplicates_uses_locked_objects_as_references_without_deleting_them() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("test");
        let layer_id = project.ensure_default_layer();
        let locked_original = add_vector_path(&mut project, layer_id, "locked", "M0 0 L10 0");
        project.find_object_mut(locked_original.id).unwrap().locked = true;
        let duplicate = add_vector_path(&mut project, layer_id, "duplicate", "M0 0 L10 0");
        *ctx.project.lock().unwrap() = Some(project);

        delete_duplicates_inner(&ctx, &[]).unwrap();

        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        assert!(project.find_object(locked_original.id).is_some());
        assert!(project.find_object(duplicate.id).is_none());
    }

    /// Exercises the same palette-sync mutation pattern to verify
    /// that recoloring a layer to a tool color auto-syncs `is_tool_layer`.
    #[test]
    fn layer_color_tag_syncs_is_tool_layer() {
        let mut project = Project::new("test");
        let layer = Layer::new("Lines", OperationType::Line);
        let layer_id = layer.id;
        project.add_layer(layer);

        let layer = project.find_layer_mut(layer_id).unwrap();
        assert!(!layer.is_tool_layer);

        // Simulate setting color_tag to T1
        let color_tag = Some("#DA0B3F".to_string());
        if let Some(ref c) = color_tag {
            layer.color_tag = beambench_common::ColorTag(c.clone());
            layer.is_tool_layer = beambench_common::is_tool_color(c);
        }
        assert!(
            layer.is_tool_layer,
            "T1 color should auto-set is_tool_layer"
        );

        // Simulate recoloring back to a standard color
        let color_tag = Some("#FF0000".to_string());
        if let Some(ref c) = color_tag {
            layer.color_tag = beambench_common::ColorTag(c.clone());
            layer.is_tool_layer = beambench_common::is_tool_color(c);
        }
        assert!(
            !layer.is_tool_layer,
            "standard color should clear is_tool_layer"
        );
    }

    /// Verify that an explicit `is_tool_layer` parameter overrides the
    /// palette-derived value.
    #[test]
    fn explicit_is_tool_layer_overrides_palette() {
        let mut project = Project::new("test");
        let layer = Layer::new("Custom", OperationType::Line);
        let layer_id = layer.id;
        project.add_layer(layer);

        let layer = project.find_layer_mut(layer_id).unwrap();

        // Set color_tag to standard color (auto-sync sets false)
        let color_tag = Some("#FF0000".to_string());
        if let Some(ref c) = color_tag {
            layer.color_tag = beambench_common::ColorTag(c.clone());
            layer.is_tool_layer = beambench_common::is_tool_color(c);
        }
        assert!(!layer.is_tool_layer);

        // Then explicit is_tool_layer=true overrides
        let is_tool_layer = Some(true);
        if let Some(it) = is_tool_layer {
            layer.is_tool_layer = it;
        }
        assert!(
            layer.is_tool_layer,
            "explicit param should override palette lookup"
        );
    }

    #[test]
    fn set_start_from_noops_when_value_is_unchanged() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("test");
        project.start_from = StartFromMode::AbsoluteCoords;
        *ctx.project.lock().unwrap() = Some(project);

        set_start_from_inner(&ctx, StartFromMode::AbsoluteCoords).unwrap();

        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn set_job_origin_noops_when_value_is_unchanged() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("test");
        project.job_origin = AnchorPoint::TopLeft;
        *ctx.project.lock().unwrap() = Some(project);

        set_job_origin_inner(&ctx, AnchorPoint::TopLeft).unwrap();

        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn set_optimization_applies_patch_and_marks_dirty() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("test");
        project.dirty = false;
        *ctx.project.lock().unwrap() = Some(project);

        let merged = set_optimization_inner(
            &ctx,
            ProjectOptimizationPatch {
                inner_first: Some(true),
                ..Default::default()
            },
        )
        .unwrap();

        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        assert!(merged.inner_first);
        assert!(project.optimization.inner_first);
        assert!(project.dirty);
    }

    #[test]
    fn set_optimization_noop_patch_does_not_push_undo_or_mark_dirty() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("test");
        project.dirty = false;
        *ctx.project.lock().unwrap() = Some(project);

        // Patch whose value is already the current field — no change.
        let merged = set_optimization_inner(
            &ctx,
            ProjectOptimizationPatch {
                enabled: Some(true), // default is already true
                ..Default::default()
            },
        )
        .unwrap();

        assert!(merged.enabled);
        assert!(!ctx.undo_state().unwrap().can_undo);
        let guard = ctx.project.lock().unwrap();
        assert!(!guard.as_ref().unwrap().dirty);
    }

    #[test]
    fn set_optimization_partial_patch_preserves_sibling_fields() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("test");
        project.optimization.inner_first = true;
        project.optimization.reduce_travel = true;
        *ctx.project.lock().unwrap() = Some(project);

        let merged = set_optimization_inner(
            &ctx,
            ProjectOptimizationPatch {
                inner_first: Some(false),
                ..Default::default()
            },
        )
        .unwrap();

        let guard = ctx.project.lock().unwrap();
        let opt = &guard.as_ref().unwrap().optimization;
        assert!(!merged.inner_first);
        assert!(merged.reduce_travel);
        assert!(!opt.inner_first);
        // Sibling flag was not in the patch — should be untouched.
        assert!(opt.reduce_travel);
    }

    #[test]
    fn set_optimization_invalidates_plan_cache_on_change() {
        use beambench_planner::ExecutionPlan;
        let ctx = ServiceContext::new();
        let mut project = Project::new("test");
        project.dirty = false;
        *ctx.project.lock().unwrap() = Some(project);
        // Seed a fake cached plan so we can observe invalidation.
        *ctx.plan_cache.lock().unwrap() = Some(ExecutionPlan {
            id: uuid::Uuid::new_v4(),
            project_id: uuid::Uuid::new_v4(),
            revision_hash: "sentinel".to_string(),
            created_at: chrono::Utc::now(),
            bounds: beambench_common::geometry::Bounds::new(
                beambench_common::geometry::Point2D::new(0.0, 0.0),
                beambench_common::geometry::Point2D::new(1.0, 1.0),
            ),
            total_distance_mm: 0.0,
            estimated_duration_secs: 0.0,
            segments: vec![],
            layer_order: vec![],
            warnings: vec![],
            failed_entries: vec![],
        });

        set_optimization_inner(
            &ctx,
            ProjectOptimizationPatch {
                reduce_travel: Some(true),
                ..Default::default()
            },
        )
        .unwrap();

        assert!(ctx.plan_cache.lock().unwrap().is_none());
    }

    #[test]
    fn set_optimization_noop_does_not_invalidate_plan_cache() {
        use beambench_planner::ExecutionPlan;
        let ctx = ServiceContext::new();
        let project = Project::new("test");
        *ctx.project.lock().unwrap() = Some(project);
        let sentinel_hash = "cached_plan_sentinel".to_string();
        *ctx.plan_cache.lock().unwrap() = Some(ExecutionPlan {
            id: uuid::Uuid::new_v4(),
            project_id: uuid::Uuid::new_v4(),
            revision_hash: sentinel_hash.clone(),
            created_at: chrono::Utc::now(),
            bounds: beambench_common::geometry::Bounds::new(
                beambench_common::geometry::Point2D::new(0.0, 0.0),
                beambench_common::geometry::Point2D::new(1.0, 1.0),
            ),
            total_distance_mm: 0.0,
            estimated_duration_secs: 0.0,
            segments: vec![],
            layer_order: vec![],
            warnings: vec![],
            failed_entries: vec![],
        });

        set_optimization_inner(
            &ctx,
            ProjectOptimizationPatch {
                enabled: Some(true), // already true by default
                ..Default::default()
            },
        )
        .unwrap();

        let cached = ctx.plan_cache.lock().unwrap();
        assert_eq!(
            cached.as_ref().map(|p| p.revision_hash.clone()),
            Some(sentinel_hash),
            "noop patch must not invalidate the plan cache"
        );
    }

    #[test]
    fn set_optimization_returns_error_when_no_project_open() {
        let ctx = ServiceContext::new();
        let result = set_optimization_inner(
            &ctx,
            ProjectOptimizationPatch {
                inner_first: Some(true),
                ..Default::default()
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn auto_join_shapes_plan_and_apply_mutates_project_geometry() {
        let mut project = Project::new("test");
        let layer = Layer::new("Line", OperationType::Line);
        let layer_id = layer.id;
        project.add_layer(layer);
        let a = add_vector_path(&mut project, layer_id, "A", "M0 0 L10 0");
        let b = add_vector_path(&mut project, layer_id, "B", "M10 0 L20 0");
        if let Some(obj) = project.find_object_mut(b.id) {
            obj.bounds = Bounds::new(Point2D::new(10.0, 0.0), Point2D::new(20.0, 10.0));
        }

        let plan = plan_auto_join_shapes(&project, &[a.id, b.id], 0.1)
            .unwrap()
            .expect("expected auto-join plan");
        let created = apply_auto_join_plan(&mut project, plan).unwrap();

        assert_eq!(created.len(), 1);
        assert_eq!(project.objects.len(), 1);
        match &created[0].data {
            ObjectData::VectorPath { path_data, .. } => {
                assert_eq!(path_data, "M0 0 L10 0 M10 0 L20 0");
            }
            other => panic!("expected vector path, got {other:?}"),
        }
    }

    #[test]
    fn optimize_shapes_plan_and_apply_updates_selected_object_in_place() {
        let mut project = Project::new("test");
        let layer = Layer::new("Line", OperationType::Line);
        let layer_id = layer.id;
        project.add_layer(layer);
        let obj = add_vector_path(&mut project, layer_id, "Redundant", "M0 0 L5 0 L10 0");

        let changes = plan_optimize_shapes(&project, &[obj.id], 0.1);
        assert_eq!(changes.len(), 1);

        let updated = apply_optimize_plan(&mut project, changes).unwrap();

        assert_eq!(updated.len(), 1);
        let stored = project
            .find_object(obj.id)
            .expect("optimized object should remain");
        match &stored.data {
            ObjectData::VectorPath { path_data, .. } => {
                assert_eq!(path_data, "M0 0 L10 0");
            }
            other => panic!("expected vector path, got {other:?}"),
        }
    }
}
