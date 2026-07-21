use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::sync::Arc;

use beambench_common::geometry::Transform2D;
use beambench_common::path::{PathCommand, Polyline, VecPath};
use beambench_common::{BarcodeOptions, BarcodeType, Bounds, Point2D};
use beambench_core::Project;
use beambench_core::object::LayerRef;
use beambench_core::vector::convert::{
    object_local_point_to_world, object_to_world_vecpath, object_to_world_vecpath_resolved,
    star_anchor_points, star_display_points,
};
use beambench_core::vector::node_edit::EditablePath;
use beambench_core::vector::normalize::NormalizedVector;
use beambench_core::vector::path_ops::{self as path_ops_core, PathVertex};
use beambench_core::{
    Asset, AssetId, AssetMediaType, CircularArrayConfig, GridArrayConfig, GridArraySizingMode,
    ImageMaskPolarity, ImageMaskRef, Layer, ObjectData, ObjectId, OperationType, ProjectObject,
    SpacingMode,
};
use beambench_service::ServiceContext;
use beambench_service::ops::{planning, vector};
use image::{DynamicImage, GrayImage, Luma, RgbaImage};
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use super::project::parse_id;

#[tauri::command]
pub fn convert_to_path(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<ProjectObject, String> {
    vector::convert_to_path(
        &svc,
        vector::ConvertToPathInput {
            object_id: parse_id(&object_id)?,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn boolean_union(
    svc: State<'_, Arc<ServiceContext>>,
    object_id_a: String,
    object_id_b: String,
) -> Result<ProjectObject, String> {
    vector::boolean_union(
        &svc,
        vector::BooleanOpInput {
            object_id_a: parse_id(&object_id_a)?,
            object_id_b: parse_id(&object_id_b)?,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn boolean_subtract(
    svc: State<'_, Arc<ServiceContext>>,
    object_id_a: String,
    object_id_b: String,
) -> Result<ProjectObject, String> {
    vector::boolean_subtract(
        &svc,
        vector::BooleanOpInput {
            object_id_a: parse_id(&object_id_a)?,
            object_id_b: parse_id(&object_id_b)?,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn boolean_exclude(
    svc: State<'_, Arc<ServiceContext>>,
    object_id_a: String,
    object_id_b: String,
) -> Result<ProjectObject, String> {
    vector::boolean_exclude(
        &svc,
        vector::BooleanOpInput {
            object_id_a: parse_id(&object_id_a)?,
            object_id_b: parse_id(&object_id_b)?,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn group_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<ProjectObject, String> {
    let object_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|id| parse_id(id))
        .collect::<Result<Vec<_>, _>>()?;
    vector::group_objects(&svc, vector::GroupObjectsInput { object_ids }).map_err(Into::into)
}

#[tauri::command]
pub fn auto_group_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<Vec<ProjectObject>, String> {
    let object_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|id| parse_id(id))
        .collect::<Result<Vec<_>, _>>()?;
    vector::auto_group_objects(&svc, vector::AutoGroupObjectsInput { object_ids })
        .map_err(Into::into)
}

#[tauri::command]
pub fn ungroup_objects(
    svc: State<'_, Arc<ServiceContext>>,
    group_id: String,
) -> Result<Vec<String>, String> {
    let children = vector::ungroup_objects(&svc, parse_id(&group_id)?).map_err(String::from)?;
    Ok(children.into_iter().map(|id| id.to_string()).collect())
}

#[tauri::command]
pub fn get_editable_path(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<Vec<EditablePath>, String> {
    vector::get_editable_path(&svc, parse_id(&object_id)?).map_err(Into::into)
}

#[tauri::command]
pub fn update_node(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_idx: usize,
    command_idx: usize,
    x: f64,
    y: f64,
    handle_type: Option<String>,
) -> Result<ProjectObject, String> {
    vector::update_node(
        &svc,
        vector::UpdateNodeInput {
            object_id: parse_id(&object_id)?,
            subpath_idx,
            command_idx,
            x,
            y,
            handle_type,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn update_nodes_batch(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    updates: Vec<serde_json::Value>,
) -> Result<ProjectObject, String> {
    let parsed_updates = updates
        .into_iter()
        .map(|value| {
            let node_id = value
                .get("node_id")
                .ok_or_else(|| "Missing node_id".to_string())?;
            Ok(vector::BatchNodeUpdate {
                node_id: serde_json::from_value(node_id.clone()).map_err(|e| e.to_string())?,
                x: value
                    .get("x")
                    .and_then(serde_json::Value::as_f64)
                    .ok_or_else(|| "Missing x".to_string())?,
                y: value
                    .get("y")
                    .and_then(serde_json::Value::as_f64)
                    .ok_or_else(|| "Missing y".to_string())?,
                handle_type: value
                    .get("handle_type")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    vector::update_nodes_batch(
        &svc,
        vector::UpdateNodesBatchInput {
            object_id: parse_id(&object_id)?,
            updates: parsed_updates,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn set_node_type(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_idx: usize,
    command_idx: usize,
    node_type: String,
) -> Result<ProjectObject, String> {
    vector::set_node_type(
        &svc,
        vector::SetNodeTypeInput {
            object_id: parse_id(&object_id)?,
            subpath_idx,
            command_idx,
            node_type,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn delete_node(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_idx: usize,
    command_idx: usize,
) -> Result<ProjectObject, String> {
    vector::delete_node(
        &svc,
        vector::DeleteNodeInput {
            object_id: parse_id(&object_id)?,
            subpath_idx,
            command_idx,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn delete_nodes(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    node_ids: Vec<serde_json::Value>,
) -> Result<ProjectObject, String> {
    let node_ids = node_ids
        .into_iter()
        .map(|value| serde_json::from_value(value).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    vector::delete_nodes(
        &svc,
        vector::DeleteNodesInput {
            object_id: parse_id(&object_id)?,
            node_ids,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn insert_node(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_idx: usize,
    command_idx: usize,
    t: f64,
) -> Result<ProjectObject, String> {
    vector::insert_node(
        &svc,
        vector::InsertNodeInput {
            object_id: parse_id(&object_id)?,
            subpath_idx,
            command_idx,
            t,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn convert_segment_to_line(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_idx: usize,
    command_idx: usize,
) -> Result<ProjectObject, String> {
    vector::convert_segment_to_line(
        &svc,
        vector::SegmentOpInput {
            object_id: parse_id(&object_id)?,
            subpath_idx,
            command_idx,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn convert_segment_to_curve(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_idx: usize,
    command_idx: usize,
) -> Result<ProjectObject, String> {
    vector::convert_segment_to_curve(
        &svc,
        vector::SegmentOpInput {
            object_id: parse_id(&object_id)?,
            subpath_idx,
            command_idx,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn align_segment_to_angle(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_idx: usize,
    command_idx: usize,
) -> Result<ProjectObject, String> {
    vector::align_segment_to_angle(
        &svc,
        vector::SegmentOpInput {
            object_id: parse_id(&object_id)?,
            subpath_idx,
            command_idx,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn trim_segment_to_intersection(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_idx: usize,
    command_idx: usize,
    click_x: f64,
    click_y: f64,
) -> Result<ProjectObject, String> {
    vector::trim_segment_to_intersection(
        &svc,
        vector::SegmentOpInput {
            object_id: parse_id(&object_id)?,
            subpath_idx,
            command_idx,
        },
        click_x,
        click_y,
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn extend_endpoint_to_intersection(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_idx: usize,
    command_idx: usize,
) -> Result<ProjectObject, String> {
    vector::extend_endpoint_to_intersection(
        &svc,
        vector::ExtendEndpointInput {
            object_id: parse_id(&object_id)?,
            node_id: beambench_core::vector::node_edit::NodeId {
                subpath_idx,
                command_idx,
            },
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn join_subpaths(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    src_node_id: serde_json::Value,
    dst_node_id: serde_json::Value,
) -> Result<ProjectObject, String> {
    vector::join_subpaths(
        &svc,
        vector::JoinSubpathsInput {
            object_id: parse_id(&object_id)?,
            src_node_id: serde_json::from_value(src_node_id).map_err(|e| e.to_string())?,
            dst_node_id: serde_json::from_value(dst_node_id).map_err(|e| e.to_string())?,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn delete_segment_cmd(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_idx: usize,
    command_idx: usize,
) -> Result<ProjectObject, String> {
    vector::delete_segment(
        &svc,
        vector::SegmentOpInput {
            object_id: parse_id(&object_id)?,
            subpath_idx,
            command_idx,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn break_path_at_node(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_idx: usize,
    command_idx: usize,
) -> Result<ProjectObject, String> {
    vector::break_path_at_node(
        &svc,
        vector::SegmentOpInput {
            object_id: parse_id(&object_id)?,
            subpath_idx,
            command_idx,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn toggle_path_closed(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_idx: usize,
) -> Result<ProjectObject, String> {
    vector::toggle_path_closed(
        &svc,
        vector::SubpathOpInput {
            object_id: parse_id(&object_id)?,
            subpath_idx,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn scale_path_to_bounds(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    new_min_x: f64,
    new_min_y: f64,
    new_max_x: f64,
    new_max_y: f64,
) -> Result<ProjectObject, String> {
    vector::scale_path_to_bounds(
        &svc,
        vector::ScalePathToBoundsInput {
            object_id: parse_id(&object_id)?,
            new_min_x,
            new_min_y,
            new_max_x,
            new_max_y,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn mesh_deform_selection(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    source_bounds: Bounds,
    handles: Vec<Point2D>,
    grid_size: usize,
    perspective: bool,
) -> Result<Vec<ProjectObject>, String> {
    let object_ids = object_ids
        .iter()
        .map(|id| parse_id(id))
        .collect::<Result<Vec<_>, _>>()?;
    vector::mesh_deform_selection(
        &svc,
        vector::MeshDeformSelectionInput {
            object_ids,
            source_bounds,
            handles,
            grid_size,
            perspective,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn normalize_for_planner(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<Vec<NormalizedVector>, String> {
    let object_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|id| parse_id(id))
        .collect::<Result<Vec<_>, _>>()?;
    vector::normalize_for_planner(&svc, vector::NormalizeForPlannerInput { object_ids })
        .map_err(Into::into)
}

// ---------------------------------------------------------------------------
// Vector commands
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BooleanAssistantSourcePreview {
    id: String,
    name: String,
    bounds: Bounds,
    path_data: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BooleanAssistantPreview {
    operation: String,
    result: ProjectObject,
    sources: Vec<BooleanAssistantSourcePreview>,
}

/// Helper: extract a VecPath from a project object by ID.
/// Handles VirtualClone resolution transparently (transient, no mutation).
fn get_object_vecpath(
    svc: &ServiceContext,
    object_id: ObjectId,
) -> Result<(ProjectObject, VecPath), String> {
    let guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_ref().ok_or("No project open")?;
    let obj = project
        .objects
        .iter()
        .find(|o| o.id == object_id)
        .ok_or("Object not found")?;
    let resolved = project.resolve_clone(obj);
    let effective = resolved.as_ref().unwrap_or(obj);
    let vp = beambench_core::vector::convert::object_to_vecpath(&effective.data)
        .ok_or("Object is not a vector type")?;
    Ok((obj.clone(), vp))
}

fn to_world_vecpath(obj: &ProjectObject) -> Result<VecPath, String> {
    object_to_world_vecpath(obj).ok_or_else(|| "Object is not a vector type".to_string())
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

fn routed_vector_result_layer(
    project: &mut beambench_core::Project,
    requested_layer: LayerRef,
) -> Result<LayerRef, String> {
    beambench_service::resolve_layer_for_object(
        project,
        requested_layer,
        beambench_service::RoutingTarget::NeedsNonImage,
    )
    .map(|(layer_id, _)| layer_id)
    .map_err(|e| e.to_string())
}

fn add_routed_vecpath_result(
    project: &mut beambench_core::Project,
    name: String,
    requested_layer: LayerRef,
    path: VecPath,
) -> Result<ProjectObject, String> {
    let layer_id = routed_vector_result_layer(project, requested_layer)?;
    let object = vecpath_to_project_object(name, layer_id, path);
    Ok(project.add_object(object).clone())
}

fn boolean_assistant_result(
    operation: &str,
    paths: &[VecPath],
) -> Result<(String, VecPath), String> {
    match operation {
        "union" => {
            let a = paths.first().ok_or("Boolean Union requires two objects")?;
            let b = paths.get(1).ok_or("Boolean Union requires two objects")?;
            Ok((
                "Union".to_string(),
                beambench_core::vector::boolean::path_union(a, b),
            ))
        }
        "subtract" => {
            let a = paths
                .first()
                .ok_or("Boolean Subtract requires two objects")?;
            let b = paths
                .get(1)
                .ok_or("Boolean Subtract requires two objects")?;
            Ok((
                "Subtract".to_string(),
                beambench_core::vector::boolean::path_subtract(a, b),
            ))
        }
        "intersection" | "intersect" => {
            let a = paths
                .first()
                .ok_or("Boolean Intersection requires two objects")?;
            let b = paths
                .get(1)
                .ok_or("Boolean Intersection requires two objects")?;
            Ok((
                "Intersection".to_string(),
                beambench_core::vector::boolean::path_intersection(a, b),
            ))
        }
        "exclude" | "xor" => {
            let a = paths
                .first()
                .ok_or("Boolean Exclude requires two objects")?;
            let b = paths.get(1).ok_or("Boolean Exclude requires two objects")?;
            Ok((
                "Exclude".to_string(),
                beambench_core::vector::boolean::path_exclude(a, b),
            ))
        }
        "weld" => {
            if paths.len() < 2 {
                return Err("Weld requires at least two objects".to_string());
            }
            Ok((
                "Weld".to_string(),
                beambench_core::vector::boolean::weld_shapes(paths),
            ))
        }
        _ => Err(format!(
            "Unsupported boolean assistant operation: {operation}"
        )),
    }
}

#[tauri::command]
pub fn boolean_assistant_preview(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    operation: String,
) -> Result<BooleanAssistantPreview, String> {
    if object_ids.len() < 2 {
        return Err("Boolean Assistant requires at least two objects".to_string());
    }

    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|id| parse_id(id))
        .collect::<Result<Vec<_>, _>>()?;

    let guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_ref().ok_or("No project open")?;
    let mut paths = Vec::with_capacity(parsed_ids.len());
    let mut sources = Vec::with_capacity(parsed_ids.len());
    let mut first_layer = None;

    for id in parsed_ids {
        let obj = project.find_object(id).ok_or("Object not found")?;
        let path = vector::boolean_operand_path_for_preview(project, id).map_err(String::from)?;
        if first_layer.is_none() {
            first_layer = Some(obj.layer_id);
        }
        let bounds = path
            .visual_bounds()
            .unwrap_or_else(|| Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(0.0, 0.0)));
        sources.push(BooleanAssistantSourcePreview {
            id: obj.id.to_string(),
            name: obj.name.clone(),
            bounds,
            path_data: path.to_svg_d(),
        });
        paths.push(path);
    }

    let (name, result_path) = boolean_assistant_result(&operation, &paths)?;
    let result =
        vecpath_to_project_object(name, first_layer.ok_or("No objects provided")?, result_path);

    Ok(BooleanAssistantPreview {
        operation,
        result,
        sources,
    })
}

fn replace_object_with_path(
    project: &mut beambench_core::Project,
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
    // The object just became a vector — if it's sitting on an image
    // layer (because it used to be a RasterImage there), reroute it
    // to a matching non-image sibling so the layer-content invariant
    // holds.
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

fn sharp_star_local_uniform_world_scale(
    obj: &ProjectObject,
    points: u32,
    ratio: f64,
    dual_radius: bool,
    ratio2: Option<f64>,
) -> f64 {
    let anchors = star_anchor_points(points, ratio, dual_radius, ratio2);
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for pt in &anchors {
        min_x = min_x.min(pt.x);
        min_y = min_y.min(pt.y);
        max_x = max_x.max(pt.x);
        max_y = max_y.max(pt.y);
    }
    let old_w = (max_x - min_x).max(1e-9);
    let old_h = (max_y - min_y).max(1e-9);
    let map_sx = obj.bounds.width() / old_w;
    let map_sy = obj.bounds.height() / old_h;
    let ax = obj.transform.a * map_sx;
    let ay = obj.transform.b * map_sx;
    let bx = obj.transform.c * map_sy;
    let by = obj.transform.d * map_sy;
    let scale_x = (ax * ax + ay * ay).sqrt();
    let scale_y = (bx * bx + by * by).sqrt();
    scale_x.min(scale_y).max(1e-9)
}

fn star_local_radius_from_mm(
    obj: &ProjectObject,
    points: u32,
    ratio: f64,
    dual_radius: bool,
    ratio2: Option<f64>,
    radius_mm: f64,
) -> f64 {
    radius_mm / sharp_star_local_uniform_world_scale(obj, points, ratio, dual_radius, ratio2)
}

fn canonical_star_anchor_count(points: u32, dual_radius: bool) -> usize {
    let base_points = points.max(3) as usize;
    if dual_radius {
        4 * base_points
    } else {
        2 * base_points
    }
}

fn draw_line(img: &mut GrayImage, mut x0: i32, mut y0: i32, x1: i32, y1: i32) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        if x0 >= 0 && y0 >= 0 && (x0 as u32) < img.width() && (y0 as u32) < img.height() {
            img.put_pixel(x0 as u32, y0 as u32, Luma([0]));
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

fn rasterize_vecpath_to_png(
    path: &VecPath,
    dpi: f64,
) -> Result<(Vec<u8>, u32, u32, Bounds), String> {
    let bounds = path.bounds().ok_or("Vector object has no bounds")?;
    let width_mm = (bounds.max.x - bounds.min.x).abs().max(0.1);
    let height_mm = (bounds.max.y - bounds.min.y).abs().max(0.1);
    let px_per_mm = (dpi.max(25.4)) / 25.4;
    let width_px = (width_mm * px_per_mm).ceil().max(1.0) as u32 + 2;
    let height_px = (height_mm * px_per_mm).ceil().max(1.0) as u32 + 2;
    let mut image = GrayImage::from_pixel(width_px, height_px, Luma([255]));
    let polylines = beambench_core::vector::flatten::flatten_vecpath(
        path,
        beambench_core::vector::flatten::DEFAULT_TOLERANCE_MM,
    );

    for polyline in polylines {
        for segment in polyline.points.windows(2) {
            let a = &segment[0];
            let b = &segment[1];
            let x0 = ((a.x - bounds.min.x) * px_per_mm).round() as i32 + 1;
            let y0 = ((a.y - bounds.min.y) * px_per_mm).round() as i32 + 1;
            let x1 = ((b.x - bounds.min.x) * px_per_mm).round() as i32 + 1;
            let y1 = ((b.y - bounds.min.y) * px_per_mm).round() as i32 + 1;
            draw_line(&mut image, x0, y0, x1, y1);
        }
        if polyline.closed && polyline.points.len() > 1 {
            let a = polyline.points.last().unwrap();
            let b = &polyline.points[0];
            let x0 = ((a.x - bounds.min.x) * px_per_mm).round() as i32 + 1;
            let y0 = ((a.y - bounds.min.y) * px_per_mm).round() as i32 + 1;
            let x1 = ((b.x - bounds.min.x) * px_per_mm).round() as i32 + 1;
            let y1 = ((b.y - bounds.min.y) * px_per_mm).round() as i32 + 1;
            draw_line(&mut image, x0, y0, x1, y1);
        }
    }

    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageLuma8(image)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|e| format!("Failed to encode bitmap: {e}"))?;
    Ok((bytes.into_inner(), width_px, height_px, bounds))
}

#[tauri::command]
pub fn boolean_intersection(
    svc: State<'_, Arc<ServiceContext>>,
    object_id_a: String,
    object_id_b: String,
) -> Result<ProjectObject, String> {
    boolean_intersection_inner(&svc, parse_id(&object_id_a)?, parse_id(&object_id_b)?)
}

fn boolean_intersection_inner(
    svc: &ServiceContext,
    id_a: ObjectId,
    id_b: ObjectId,
) -> Result<ProjectObject, String> {
    vector::boolean_intersection(
        svc,
        vector::BooleanOpInput {
            object_id_a: id_a,
            object_id_b: id_b,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn boolean_weld(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<ProjectObject, String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    boolean_weld_inner(&svc, parsed_ids)
}

fn boolean_weld_inner(
    svc: &ServiceContext,
    parsed_ids: Vec<ObjectId>,
) -> Result<ProjectObject, String> {
    vector::boolean_weld(
        svc,
        vector::BooleanWeldInput {
            object_ids: parsed_ids,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn offset_shapes(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    distance: f64,
    direction: String,
    corner_style: Option<String>,
    delete_original: Option<bool>,
) -> Result<Vec<ProjectObject>, String> {
    let oids: Vec<ObjectId> = object_ids
        .iter()
        .map(|id| parse_id(id))
        .collect::<Result<Vec<_>, _>>()?;
    let dir = match direction.as_str() {
        "inward" => beambench_core::vector::offset::OffsetDirection::Inward,
        "outward" => beambench_core::vector::offset::OffsetDirection::Outward,
        "both" => beambench_core::vector::offset::OffsetDirection::Both,
        _ => return Err(format!("Invalid direction: {direction}")),
    };
    let corner = match corner_style.as_deref() {
        Some("round") => beambench_core::vector::offset::CornerStyle::Round,
        Some("bevel") => beambench_core::vector::offset::CornerStyle::Bevel,
        _ => beambench_core::vector::offset::CornerStyle::Miter,
    };

    let result = offset_shapes_inner(
        &svc,
        &oids,
        distance,
        dir,
        corner,
        delete_original.unwrap_or(false),
    )?;

    planning::invalidate_plan_cache(&svc).map_err(String::from)?;
    Ok(result)
}

/// Core logic for offset_shapes, separated from the Tauri command wrapper
/// so it can be tested without `State<>`.
///
/// Architecture: **split-before-weld**.
/// 1. Partition all collected SubPaths into closed vs open.
/// 2. Closed paths → `weld_shapes()` → `buffer_closed_path()` (Clipper2).
/// 3. Open paths → `offset_path()` individually (old vertex algorithm).
/// 4. `weld_shapes()` is **never** called on open geometry.
fn offset_shapes_inner(
    svc: &ServiceContext,
    oids: &[ObjectId],
    distance: f64,
    dir: beambench_core::vector::offset::OffsetDirection,
    corner: beambench_core::vector::offset::CornerStyle,
    delete_original: bool,
) -> Result<Vec<ProjectObject>, String> {
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;

    let split = collect_and_split(project, oids)?;
    if split.open_subpaths.is_empty() && split.closed_subpaths.is_empty() {
        return Ok(vec![]);
    }

    // Derive name/layer from the first contributing (non-group) source object.
    let (first_name, first_layer) = split
        .source_ids
        .iter()
        .filter_map(|id| {
            let obj = project.find_object(*id)?;
            if matches!(obj.data, ObjectData::Group { .. }) {
                None
            } else {
                Some((obj.name.clone(), obj.layer_id))
            }
        })
        .next()
        .unwrap_or_else(|| (String::new(), LayerRef::new()));
    let result_layer = routed_vector_result_layer(project, first_layer)?;

    let results = compute_offset_from_split(
        &split.open_subpaths,
        &split.closed_subpaths,
        distance,
        dir,
        corner,
    );

    // Single undo snapshot for the entire batch.
    svc.push_project_undo_snapshot(project)
        .map_err(|e| e.to_string())?;

    let mut created = Vec::new();

    for (idx, path) in results.into_iter().enumerate() {
        if is_degenerate_offset_result(&path) {
            continue;
        }
        let name = if first_name.is_empty() {
            if idx == 0 {
                "Offset".to_string()
            } else {
                format!("Offset {}", idx + 1)
            }
        } else {
            let suffix = if idx == 0 {
                " Offset".to_string()
            } else {
                format!(" Offset {}", idx + 1)
            };
            format!("{first_name}{suffix}")
        };
        created.push(add_routed_vecpath_result(
            project,
            name,
            result_layer,
            path,
        )?);
    }

    // Delete all source objects when at least one valid offset was produced.
    if delete_original && !created.is_empty() {
        for sid in &split.source_ids {
            project.remove_object(*sid);
        }
    }

    Ok(created)
}

/// Result of gathering a selection's vector geometry and partitioning subpaths
/// by closed/open. Shared by `offset_shapes_inner` (apply) and
/// `preview_offset_shapes_inner` (non-mutating preview) so the two can never
/// disagree about what geometry an offset operates on.
struct OffsetSplit {
    open_subpaths: Vec<beambench_common::path::SubPath>,
    closed_subpaths: Vec<beambench_common::path::SubPath>,
    source_ids: Vec<ObjectId>,
}

/// Cheap first stage: collect world-space vector paths from the selection
/// (expanding groups and resolving virtual clones via `collect_vector_paths`)
/// and split their subpaths into open vs closed. Does not mutate the project.
fn collect_and_split(project: &Project, oids: &[ObjectId]) -> Result<OffsetSplit, String> {
    use beambench_common::path::SubPath;

    let mut seen: HashSet<ObjectId> = HashSet::new();
    let mut all_paths: Vec<VecPath> = Vec::new();
    let mut source_ids: Vec<ObjectId> = Vec::new();

    for &oid in oids {
        if project.find_object(oid).is_none() {
            return Err("Object not found".to_string());
        }
        collect_vector_paths(project, oid, &mut seen, &mut all_paths, &mut source_ids);
    }

    let mut open_subpaths: Vec<SubPath> = Vec::new();
    let mut closed_subpaths: Vec<SubPath> = Vec::new();
    for vp in &all_paths {
        for sp in &vp.subpaths {
            if sp.closed {
                closed_subpaths.push(sp.clone());
            } else {
                open_subpaths.push(sp.clone());
            }
        }
    }

    Ok(OffsetSplit {
        open_subpaths,
        closed_subpaths,
        source_ids,
    })
}

/// Heavy second stage: offset the split geometry. Closed paths are welded then
/// buffered with Clipper2; open paths are offset individually with the vertex
/// algorithm (no union). Callers that only want open-path results pass an empty
/// closed slice so the Clipper2 buffer is never computed.
fn compute_offset_from_split(
    open_subpaths: &[beambench_common::path::SubPath],
    closed_subpaths: &[beambench_common::path::SubPath],
    distance: f64,
    dir: beambench_core::vector::offset::OffsetDirection,
    corner: beambench_core::vector::offset::CornerStyle,
) -> Vec<VecPath> {
    let mut results: Vec<VecPath> = Vec::new();

    // Closed paths: weld then buffer with Clipper2.
    // Each closed subpath must be a separate VecPath element so weld_shapes
    // actually boolean-unions them (with one element it returns unchanged).
    if !closed_subpaths.is_empty() {
        let closed_vps: Vec<VecPath> = closed_subpaths
            .iter()
            .map(|sp| VecPath {
                subpaths: vec![sp.clone()],
            })
            .collect();
        let welded = beambench_core::vector::boolean::weld_shapes(&closed_vps);
        results.extend(beambench_core::vector::buffer::buffer_closed_path(
            &welded, distance, dir, corner, 2.0,
        ));
    }

    // Open paths: offset individually with the old vertex algorithm (no union)
    for sp in open_subpaths {
        let open_vp = VecPath {
            subpaths: vec![sp.clone()],
        };
        results.extend(beambench_core::vector::offset::offset_path(
            &open_vp, distance, dir, corner,
        ));
    }

    results
}

/// Shared by the apply and preview paths so the preview ghost can never
/// disagree with what Apply keeps: an offset result is degenerate when it is
/// empty or point-sized. Valid open lines whose bounds are zero in one
/// dimension pass.
fn is_degenerate_offset_result(path: &VecPath) -> bool {
    if path.is_empty() {
        return true;
    }
    match path.bounds() {
        Some(b) => b.width() < 1e-6 && b.height() < 1e-6,
        None => true,
    }
}

/// One flattened offset-preview result: a world-space polyline the frontend
/// renders as a dashed ghost.
#[derive(Serialize)]
pub struct OffsetPreviewPath {
    pub points: Vec<Point2D>,
    pub closed: bool,
}

/// Non-mutating offset preview. `source_all_open` reports whether the collected
/// geometry was entirely open paths (the only case that yields a preview).
#[derive(Serialize)]
pub struct OffsetPreview {
    pub paths: Vec<OffsetPreviewPath>,
    pub source_all_open: bool,
}

#[tauri::command]
pub fn preview_offset_shapes(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    distance: f64,
    direction: String,
    corner_style: Option<String>,
) -> Result<OffsetPreview, String> {
    let oids: Vec<ObjectId> = object_ids
        .iter()
        .map(|id| parse_id(id))
        .collect::<Result<Vec<_>, _>>()?;
    let dir = match direction.as_str() {
        "inward" => beambench_core::vector::offset::OffsetDirection::Inward,
        "outward" => beambench_core::vector::offset::OffsetDirection::Outward,
        "both" => beambench_core::vector::offset::OffsetDirection::Both,
        _ => return Err(format!("Invalid direction: {direction}")),
    };
    let corner = match corner_style.as_deref() {
        Some("round") => beambench_core::vector::offset::CornerStyle::Round,
        Some("bevel") => beambench_core::vector::offset::CornerStyle::Bevel,
        _ => beambench_core::vector::offset::CornerStyle::Miter,
    };

    preview_offset_shapes_inner(&svc, &oids, distance, dir, corner)
}

/// Core logic for `preview_offset_shapes`, separated from the Tauri command
/// wrapper so it can be tested without `State<>`. Reads the project, never
/// mutates it, and skips the Clipper2 closed buffer entirely for closed/mixed
/// selections (preview is scoped to all-open selections).
fn preview_offset_shapes_inner(
    svc: &ServiceContext,
    oids: &[ObjectId],
    distance: f64,
    dir: beambench_core::vector::offset::OffsetDirection,
    corner: beambench_core::vector::offset::CornerStyle,
) -> Result<OffsetPreview, String> {
    let guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_ref().ok_or("No project open")?;

    let split = collect_and_split(project, oids)?;

    // Scope: only all-open selections get a preview. Bail before the heavy
    // stage for closed/mixed/empty selections so Clipper2 never runs here.
    if !split.closed_subpaths.is_empty() || split.open_subpaths.is_empty() {
        return Ok(OffsetPreview {
            paths: vec![],
            source_all_open: false,
        });
    }

    let results = compute_offset_from_split(&split.open_subpaths, &[], distance, dir, corner);

    let mut paths = Vec::new();
    for path in &results {
        if is_degenerate_offset_result(path) {
            continue;
        }
        for poly in beambench_core::vector::flatten::flatten_vecpath(
            path,
            beambench_core::vector::flatten::DEFAULT_TOLERANCE_MM,
        ) {
            if poly.points.len() < 2 {
                continue;
            }
            paths.push(OffsetPreviewPath {
                points: poly.points,
                closed: poly.closed,
            });
        }
    }

    Ok(OffsetPreview {
        paths,
        source_all_open: true,
    })
}

/// Recursively collect vector paths from an object, expanding groups.
/// `seen` deduplicates when a group and its child are both in the selection.
/// Only objects that actually contribute a vector path (or groups containing
/// such objects) are added to `source_ids` for deletion — non-vector leaves
/// (raster, text, barcode) are left untouched.
fn collect_vector_paths(
    project: &Project,
    oid: ObjectId,
    seen: &mut HashSet<ObjectId>,
    all_paths: &mut Vec<VecPath>,
    source_ids: &mut Vec<ObjectId>,
) {
    if !seen.insert(oid) {
        return; // Already processed
    }
    let Some(obj) = project.find_object(oid) else {
        return;
    };

    match &obj.data {
        ObjectData::Group { children } => {
            let before = source_ids.len();
            for &child_id in children {
                collect_vector_paths(project, child_id, seen, all_paths, source_ids);
            }
            // Only mark the group for deletion if at least one child contributed
            if source_ids.len() > before {
                source_ids.push(oid);
            }
        }
        _ => {
            // Resolve VirtualClone to its source geometry (transient, no mutation)
            let resolved = project.resolve_clone(obj);
            let effective = resolved.as_ref().unwrap_or(obj);
            if let Some(vp) = object_to_world_vecpath(effective) {
                all_paths.push(vp);
                source_ids.push(oid);
            }
            // Non-vector objects are NOT added to source_ids — they aren't
            // part of the offset and must not be deleted.
        }
    }
}

#[tauri::command]
pub fn close_path(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<ProjectObject, String> {
    close_path_inner(&svc, parse_id(&object_id)?)
}

fn close_path_inner(svc: &ServiceContext, oid: ObjectId) -> Result<ProjectObject, String> {
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    let source = project.find_object(oid).ok_or("Object not found")?.clone();
    let path = object_to_world_vecpath_resolved(&source, project)
        .ok_or_else(|| "Object is not a vector type".to_string())?;
    let source_svg = path.to_svg_d();
    let result_svg = beambench_core::vector::path_ops::close_paths_with_tolerance(
        std::slice::from_ref(&source_svg),
        0.5,
    )
    .into_iter()
    .next()
    .unwrap_or_else(|| source_svg.clone());
    if result_svg == source_svg {
        return Ok(source);
    }

    svc.push_project_undo_snapshot(project)
        .map_err(|e| e.to_string())?;
    project.ensure_resolved(oid).map_err(|e| e.to_string())?;
    path_ops_core::ensure_denormalized(project.find_object_mut(oid).ok_or("Object not found")?);

    let result = VecPath::parse_svg_d(&result_svg);
    let updated = replace_object_with_path(project, oid, result)?;
    drop(guard);
    planning::invalidate_plan_cache(svc).map_err(String::from)?;
    Ok(updated)
}

#[tauri::command]
pub fn close_paths_with_tolerance(
    _svc: State<'_, Arc<ServiceContext>>,
    paths: Vec<String>,
    tolerance: f64,
) -> Result<Vec<String>, String> {
    Ok(beambench_core::vector::path_ops::close_paths_with_tolerance(&paths, tolerance))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseSelectedPathsWithToleranceResult {
    open_shapes_found: usize,
    shapes_closed: usize,
    remaining_open: usize,
    object_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CloseToleranceMode {
    MoveEndsTogether,
    JoinWithLine,
}

impl CloseToleranceMode {
    fn parse(mode: &str) -> Result<Self, String> {
        match mode {
            "move_ends_together" => Ok(Self::MoveEndsTogether),
            "join_with_line" => Ok(Self::JoinWithLine),
            other => Err(format!("Unsupported close tolerance mode: {other}")),
        }
    }
}

#[tauri::command]
pub fn close_selected_paths_with_tolerance(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    tolerance: f64,
    mode: String,
) -> Result<CloseSelectedPathsWithToleranceResult, String> {
    close_selected_paths_with_tolerance_inner(&svc, object_ids, tolerance, mode)
}

#[tauri::command]
pub fn count_open_paths_with_tolerance(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    tolerance: f64,
    mode: String,
) -> Result<CloseSelectedPathsWithToleranceResult, String> {
    let mode = CloseToleranceMode::parse(&mode)?;
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|id| parse_id(id))
        .collect::<Result<Vec<_>, _>>()?;
    let guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_ref().ok_or("No project open")?;
    let summary = plan_close_selected_paths_with_tolerance(project, &parsed_ids, tolerance, mode)?;
    Ok(CloseSelectedPathsWithToleranceResult {
        open_shapes_found: summary.open_shapes_found,
        shapes_closed: summary.shapes_closed,
        remaining_open: summary.remaining_open,
        object_ids,
    })
}

fn close_selected_paths_with_tolerance_inner(
    svc: &ServiceContext,
    object_ids: Vec<String>,
    tolerance: f64,
    mode: String,
) -> Result<CloseSelectedPathsWithToleranceResult, String> {
    let mode = CloseToleranceMode::parse(&mode)?;
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|id| parse_id(id))
        .collect::<Result<Vec<_>, _>>()?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    let summary = plan_close_selected_paths_with_tolerance(project, &parsed_ids, tolerance, mode)?;
    let open_shapes_found = summary.open_shapes_found;
    let shapes_closed = summary.shapes_closed;
    let remaining_open = summary.remaining_open;
    let mutation_count = summary.plans.len();
    if mutation_count > 0 {
        svc.push_project_undo_snapshot(project)
            .map_err(|e| e.to_string())?;
        for (oid, result) in summary.plans {
            project.ensure_resolved(oid).map_err(|e| e.to_string())?;
            path_ops_core::ensure_denormalized(
                project.find_object_mut(oid).ok_or("Object not found")?,
            );
            replace_object_with_path(project, oid, result)?;
        }
        project.dirty = true;
    }

    drop(guard);
    if mutation_count > 0 {
        planning::invalidate_plan_cache(svc).map_err(String::from)?;
    }
    Ok(CloseSelectedPathsWithToleranceResult {
        open_shapes_found,
        shapes_closed,
        remaining_open,
        object_ids,
    })
}

struct CloseSelectedPathsPlan {
    open_shapes_found: usize,
    shapes_closed: usize,
    remaining_open: usize,
    plans: Vec<(ObjectId, VecPath)>,
}

fn plan_close_selected_paths_with_tolerance(
    project: &Project,
    parsed_ids: &[ObjectId],
    tolerance: f64,
    mode: CloseToleranceMode,
) -> Result<CloseSelectedPathsPlan, String> {
    let mut plans = Vec::new();
    let mut open_shapes_found = 0;
    let mut shapes_closed = 0;
    let mut remaining_open = 0;
    for oid in parsed_ids {
        let source = project.find_object(*oid).ok_or("Object not found")?.clone();
        let Some(path) = object_to_world_vecpath_resolved(&source, project) else {
            continue;
        };
        let source_svg = path.to_svg_d();
        let source_open = path.subpaths.iter().any(|sp| !sp.closed);
        if source_open {
            open_shapes_found += 1;
        }
        let result_svg = close_svg_path_with_tolerance_mode(&source_svg, tolerance, mode);
        if !source_open {
            continue;
        }
        if result_svg == source_svg {
            remaining_open += 1;
            continue;
        }
        let result_path = VecPath::parse_svg_d(&result_svg);
        if result_path.subpaths.iter().any(|sp| !sp.closed) {
            remaining_open += 1;
        } else {
            shapes_closed += 1;
        }
        plans.push((*oid, result_path));
    }
    Ok(CloseSelectedPathsPlan {
        open_shapes_found,
        shapes_closed,
        remaining_open,
        plans,
    })
}

fn close_svg_path_with_tolerance_mode(
    source_svg: &str,
    tolerance: f64,
    mode: CloseToleranceMode,
) -> String {
    match mode {
        CloseToleranceMode::JoinWithLine => {
            let paths = [source_svg.to_string()];
            beambench_core::vector::path_ops::close_paths_with_tolerance(&paths, tolerance)
                .into_iter()
                .next()
                .unwrap_or_else(|| source_svg.to_string())
        }
        CloseToleranceMode::MoveEndsTogether => {
            move_endpoints_together_and_close(source_svg, tolerance)
        }
    }
}

fn move_endpoints_together_and_close(source_svg: &str, tolerance: f64) -> String {
    let mut path = VecPath::parse_svg_d(source_svg);
    for subpath in &mut path.subpaths {
        if subpath.closed || subpath.commands.len() < 2 {
            continue;
        }
        let Some(first) = subpath.commands.first().and_then(command_endpoint) else {
            continue;
        };
        let Some(last) = subpath.commands.last().and_then(command_endpoint) else {
            continue;
        };
        let distance = ((last.x - first.x).powi(2) + (last.y - first.y).powi(2)).sqrt();
        if distance > tolerance {
            continue;
        }

        let midpoint = Point2D::new((first.x + last.x) / 2.0, (first.y + last.y) / 2.0);
        if let Some(first_command) = subpath.commands.first_mut() {
            set_command_endpoint(first_command, midpoint);
        }
        if let Some(last_command) = subpath.commands.last_mut() {
            set_command_endpoint(last_command, midpoint);
        }
        subpath.commands.push(PathCommand::Close);
        subpath.closed = true;
    }
    path.to_svg_d()
}

fn command_endpoint(command: &PathCommand) -> Option<Point2D> {
    match command {
        PathCommand::MoveTo { x, y }
        | PathCommand::LineTo { x, y }
        | PathCommand::QuadTo { x, y, .. }
        | PathCommand::CubicTo { x, y, .. } => Some(Point2D::new(*x, *y)),
        PathCommand::Close => None,
    }
}

fn set_command_endpoint(command: &mut PathCommand, point: Point2D) {
    match command {
        PathCommand::MoveTo { x, y }
        | PathCommand::LineTo { x, y }
        | PathCommand::QuadTo { x, y, .. }
        | PathCommand::CubicTo { x, y, .. } => {
            *x = point.x;
            *y = point.y;
        }
        PathCommand::Close => {}
    }
}

#[tauri::command]
pub fn break_apart(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<Vec<ProjectObject>, String> {
    break_apart_inner(&svc, parse_id(&object_id)?)
}

fn break_apart_inner(svc: &ServiceContext, oid: ObjectId) -> Result<Vec<ProjectObject>, String> {
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    let source = project.find_object(oid).ok_or("Object not found")?.clone();
    let path = object_to_world_vecpath_resolved(&source, project)
        .ok_or_else(|| "Object is not a vector type".to_string())?;
    let parts = beambench_core::vector::path_ops::break_apart(&path.to_svg_d());
    if parts.len() <= 1 {
        return Ok(vec![]);
    }

    svc.push_project_undo_snapshot(project)
        .map_err(|e| e.to_string())?;
    project.ensure_resolved(oid).map_err(|e| e.to_string())?;

    project.remove_object(oid);
    let total = parts.len();
    let mut created = Vec::with_capacity(total);
    for (idx, part) in parts.into_iter().enumerate() {
        created.push(add_routed_vecpath_result(
            project,
            if total <= 1 {
                format!("{} Part", source.name)
            } else {
                format!("{} Part {}", source.name, idx + 1)
            },
            source.layer_id,
            VecPath::parse_svg_d(&part),
        )?);
    }
    drop(guard);
    planning::invalidate_plan_cache(svc).map_err(String::from)?;
    Ok(created)
}

#[tauri::command]
pub fn set_start_point(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    x: f64,
    y: f64,
    mode: Option<String>,
) -> Result<ProjectObject, String> {
    use beambench_core::object::StartPointEdit;

    let oid: ObjectId = parse_id(&object_id)?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;

    let mode = mode.unwrap_or_else(|| "set".to_string());

    // Use clone's bounds/transform for correct world vertex positions
    let obj = project.find_object(oid).ok_or("Object not found")?;
    let world_vp = object_to_world_vecpath_resolved(obj, project)
        .ok_or_else(|| "Object is not a vector type".to_string())?;
    let world_verts = path_ops_core::get_path_vertices(&world_vp);

    // Find nearest closed-subpath vertex to click point
    let mut nearest: Option<(usize, usize, f64)> = None;
    for v in &world_verts {
        if !v.subpath_closed {
            continue;
        }
        let dx = v.x - x;
        let dy = v.y - y;
        let dist_sq = dx * dx + dy * dy;
        if nearest.is_none() || dist_sq < nearest.unwrap().2 {
            nearest = Some((v.subpath_index, v.vertex_index, dist_sq));
        }
    }
    let Some((sp_idx, v_idx, _)) = nearest else {
        // No closed subpath vertex found — return object unchanged (no undo entry)
        let obj = project.find_object(oid).ok_or("Object not found")?;
        return Ok(obj.clone());
    };

    // Re-read effective object for no-op checks (VirtualClones see source's start_point_edits)
    let obj = project.find_object(oid).ok_or("Object not found")?;
    let resolved2 = project.resolve_clone(obj);
    let effective2 = resolved2.as_ref().unwrap_or(obj);

    // Early no-op checks before committing to undo
    if mode == "reset" {
        let has_entry = effective2
            .start_point_edits
            .iter()
            .any(|e| e.subpath_index == sp_idx);
        if !has_entry {
            return Ok(obj.clone()); // No custom start for this subpath — no undo entry
        }
    }
    if mode == "set" && v_idx == 0 {
        let has_entry = effective2
            .start_point_edits
            .iter()
            .any(|e| e.subpath_index == sp_idx);
        if !has_entry {
            // Clicking the current start vertex on a default path — nothing to change
            return Ok(obj.clone());
        }
    }
    // "set" on already-custom subpath at vertex 0 without reverse — no rotation, no change
    if mode == "set" && v_idx == 0 {
        let obj = project.find_object(oid).ok_or("Object not found")?;
        return Ok(obj.clone());
    }

    // We will mutate — push undo snapshot, then resolve clone if needed
    svc.push_project_undo_snapshot(project)
        .map_err(|e| e.to_string())?;
    project.ensure_resolved(oid).map_err(|e| e.to_string())?;

    let obj = project.find_object(oid).ok_or("Object not found")?;
    let is_vector_path = matches!(obj.data, ObjectData::VectorPath { .. });

    // Get a working VecPath representing the current state of the path.
    // For VectorPath: parse from path_data (start_point_edits are already baked in).
    // For non-VectorPath: generate from primitive via object_to_world_vecpath
    //   (which now applies any existing start_point_edits lazily).
    let mut vp = if is_vector_path {
        let path_data = match &obj.data {
            ObjectData::VectorPath { path_data, .. } => path_data.clone(),
            _ => unreachable!(),
        };
        VecPath::parse_svg_d(&path_data)
    } else {
        to_world_vecpath(obj)?
    };

    let obj = project.find_object_mut(oid).ok_or("Object not found")?;

    match mode.as_str() {
        "reset" => {
            // Find entry for this subpath (guaranteed to exist by check above)
            let entry_idx = obj
                .start_point_edits
                .iter()
                .position(|e| e.subpath_index == sp_idx)
                .expect("entry existence verified above");

            // For VectorPath: undo rotation/reversal on path_data to restore original.
            // For non-VectorPath: the original primitive data is unchanged — just
            // remove the metadata entry and the lazy application disappears.
            if is_vector_path {
                let entry = obj.start_point_edits[entry_idx].clone();
                let v = entry.v_display;
                let mut idx = entry.original_start_current_idx;
                if entry.reversed {
                    vp = path_ops_core::reverse_subpath_at(&vp, entry.subpath_index);
                    if idx > 0 {
                        idx = v - idx;
                    }
                }
                vp = path_ops_core::rotate_subpath_start(&vp, entry.subpath_index, idx);
                if entry.normalized && entry.subpath_index < vp.subpaths.len() {
                    vp.subpaths[entry.subpath_index] = path_ops_core::denormalize_closed_subpath(
                        &vp.subpaths[entry.subpath_index],
                    );
                }
                if let ObjectData::VectorPath {
                    ref mut path_data, ..
                } = obj.data
                {
                    *path_data = vp.to_svg_d();
                }
            }
            obj.start_point_edits.remove(entry_idx);
            project.dirty = true;
        }
        _ => {
            // "set" or "set_and_reverse"
            let do_reverse = mode == "set_and_reverse";

            // Normalize if no entry yet for this subpath
            let has_entry = obj
                .start_point_edits
                .iter()
                .any(|e| e.subpath_index == sp_idx);
            if !has_entry {
                let (norm_sp, was_modified) =
                    path_ops_core::normalize_closed_subpath(&vp.subpaths[sp_idx]);
                let v_display = norm_sp
                    .commands
                    .iter()
                    .filter(|c| !matches!(c, beambench_common::path::PathCommand::Close))
                    .count()
                    .saturating_sub(1);
                vp.subpaths[sp_idx] = norm_sp;
                obj.start_point_edits.push(StartPointEdit {
                    subpath_index: sp_idx,
                    original_start_current_idx: 0,
                    reversed: false,
                    v_display,
                    normalized: was_modified,
                });
            }

            // Get mutable entry
            let entry = obj
                .start_point_edits
                .iter_mut()
                .find(|e| e.subpath_index == sp_idx)
                .unwrap();
            let v = entry.v_display;

            // Rotate
            if v_idx > 0 {
                entry.original_start_current_idx =
                    (entry.original_start_current_idx + v - v_idx) % v;
                vp = path_ops_core::rotate_subpath_start(&vp, sp_idx, v_idx);
            }

            // Reverse if requested
            if do_reverse {
                vp = path_ops_core::reverse_subpath_at(&vp, sp_idx);
                let idx = entry.original_start_current_idx;
                if idx > 0 {
                    entry.original_start_current_idx = v - idx;
                }
                entry.reversed = !entry.reversed;
            }

            // Write modified path_data only for VectorPath objects.
            // Non-VectorPath objects keep their original ObjectData;
            // the start_point_edits metadata is applied lazily in
            // object_to_world_vecpath.
            if is_vector_path {
                if let ObjectData::VectorPath {
                    ref mut path_data, ..
                } = obj.data
                {
                    *path_data = vp.to_svg_d();
                }
            }
            project.dirty = true;
        }
    }

    let result = project.find_object(oid).ok_or("Object not found")?.clone();
    drop(guard);
    planning::invalidate_plan_cache(&svc).map_err(String::from)?;
    Ok(result)
}

#[tauri::command]
pub fn get_path_vertices(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<Vec<PathVertex>, String> {
    let oid: ObjectId = parse_id(&object_id)?;
    let guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_ref().ok_or("No project open")?;
    let obj = project.find_object(oid).ok_or("Object not found")?;
    // Use clone's bounds/transform (not source's) for correct world positions
    let vp = object_to_world_vecpath_resolved(obj, project)
        .ok_or_else(|| "Object is not a vector type".to_string())?;
    Ok(path_ops_core::get_path_vertices(&vp))
}

#[tauri::command]
pub fn apply_radius(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    radius_mm: f64,
) -> Result<ProjectObject, String> {
    let oid: ObjectId = parse_id(&object_id)?;
    // Near-zero radius is a no-op — matches core apply_radius threshold
    if radius_mm.abs() < 1e-12 {
        let guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
        let project = guard.as_ref().ok_or("No project open")?;
        let obj = project.find_object(oid).ok_or("Object not found")?;
        return Ok(obj.clone());
    }
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;

    let obj = project.find_object(oid).ok_or("Object not found")?;
    let resolved = project.resolve_clone(obj);
    let effective = resolved.as_ref().unwrap_or(obj);

    if let ObjectData::Star {
        points,
        bulge,
        ratio,
        dual_radius,
        ratio2,
        corner_radius,
        corner_radii,
        ..
    } = &effective.data
    {
        if bulge.abs() > 1e-9 {
            return Err("Radius tool does not support bulged stars yet".into());
        }
        if radius_mm < 0.0 {
            return Err("Negative radius is not supported for stars yet".into());
        }
        let local_radius =
            star_local_radius_from_mm(effective, *points, *ratio, *dual_radius, *ratio2, radius_mm);
        if (local_radius - *corner_radius).abs() < 1e-9 && corner_radii.is_empty() {
            return Ok(obj.clone());
        }
        svc.push_project_undo_snapshot(project)
            .map_err(|e| e.to_string())?;
        project.ensure_resolved(oid).map_err(|e| e.to_string())?;
        let target = project.find_object_mut(oid).ok_or("Object not found")?;
        if let ObjectData::Star {
            corner_radius,
            corner_radii,
            ..
        } = &mut target.data
        {
            *corner_radius = local_radius.max(0.0);
            corner_radii.clear();
        }
        let updated = target.clone();
        drop(guard);
        planning::invalidate_plan_cache(&svc).map_err(String::from)?;
        return Ok(updated);
    }

    // Compute the result read-only first to detect geometric no-ops
    let vp = to_world_vecpath(effective)?;
    let result = beambench_core::vector::path_ops::apply_radius(&vp, radius_mm);
    // If the path is unchanged, return without touching undo or metadata
    if result.to_svg_d() == vp.to_svg_d() {
        let obj = project.find_object(oid).ok_or("Object not found")?;
        return Ok(obj.clone());
    }

    svc.push_project_undo_snapshot(project)
        .map_err(|e| e.to_string())?;
    project.ensure_resolved(oid).map_err(|e| e.to_string())?;
    path_ops_core::ensure_denormalized(project.find_object_mut(oid).ok_or("Object not found")?);
    // Re-compute on the now-resolved/denormalized object
    let source = project.find_object(oid).ok_or("Object not found")?.clone();
    let vp = to_world_vecpath(&source)?;
    let result = beambench_core::vector::path_ops::apply_radius(&vp, radius_mm);
    let updated = replace_object_with_path(project, oid, result)?;
    drop(guard);
    planning::invalidate_plan_cache(&svc).map_err(String::from)?;
    Ok(updated)
}

#[tauri::command]
pub fn get_fillet_candidates(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<Vec<beambench_core::vector::path_ops::FilletCandidate>, String> {
    let oid: ObjectId = parse_id(&object_id)?;
    let guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_ref().ok_or("No project open")?;
    let obj = project.find_object(oid).ok_or("Object not found")?;
    // For clones: resolve data from source but use clone's bounds for world positions
    let resolved = project.resolve_clone(obj);
    let display_obj = resolved.as_ref().unwrap_or(obj);
    if let ObjectData::Star {
        points,
        bulge,
        ratio,
        dual_radius,
        ratio2,
        corner_radius,
        corner_radii,
    } = &display_obj.data
    {
        if bulge.abs() > 1e-9 {
            return Err("Radius tool does not support bulged stars yet".into());
        }
        let world_points = star_display_points(
            *points,
            *ratio,
            *dual_radius,
            *ratio2,
            *corner_radius,
            corner_radii,
        )
        .into_iter()
        .filter_map(|pt| object_local_point_to_world(display_obj, pt))
        .collect::<Vec<_>>();
        let effective_radii = if corner_radii.len() == world_points.len() {
            corner_radii.clone()
        } else {
            vec![*corner_radius; world_points.len()]
        };
        return Ok(world_points
            .into_iter()
            .enumerate()
            .map(
                |(vertex_index, pt)| beambench_core::vector::path_ops::FilletCandidate {
                    subpath_index: 0,
                    vertex_index,
                    x: pt.x,
                    y: pt.y,
                    already_filleted: effective_radii.get(vertex_index).copied().unwrap_or(0.0)
                        > 1e-9,
                },
            )
            .collect());
    }
    // Use clone's bounds for world-space positions
    let vp = object_to_world_vecpath_resolved(obj, project)
        .ok_or_else(|| "Object is not a vector type".to_string())?;
    Ok(beambench_core::vector::path_ops::get_fillet_candidates(&vp))
}

#[tauri::command]
pub fn apply_corner_radius(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_index: usize,
    vertex_index: usize,
    radius_mm: f64,
) -> Result<ProjectObject, String> {
    apply_corner_radius_inner(
        &svc,
        parse_id(&object_id)?,
        subpath_index,
        vertex_index,
        radius_mm,
    )
}

fn apply_corner_radius_inner(
    svc: &ServiceContext,
    oid: ObjectId,
    subpath_index: usize,
    vertex_index: usize,
    radius_mm: f64,
) -> Result<ProjectObject, String> {
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;

    let obj = project.find_object(oid).ok_or("Object not found")?;
    let resolved = project.resolve_clone(obj);
    let effective = resolved.as_ref().unwrap_or(obj);

    if let ObjectData::Star {
        points,
        bulge,
        ratio,
        dual_radius,
        ratio2,
        corner_radius,
        corner_radii,
        ..
    } = &effective.data
    {
        if bulge.abs() > 1e-9 {
            return Err("Radius tool does not support bulged stars yet".into());
        }
        if subpath_index != 0 {
            return Err("Star only has one contour".into());
        }
        let anchor_count = canonical_star_anchor_count(*points, *dual_radius);
        if vertex_index >= anchor_count {
            return Err("Star corner index out of range".into());
        }
        if radius_mm < 0.0 {
            return Err("Negative radius is not supported for stars yet".into());
        }
        let local_radius =
            star_local_radius_from_mm(effective, *points, *ratio, *dual_radius, *ratio2, radius_mm)
                .max(0.0);
        let mut next_radii = if corner_radii.len() == anchor_count {
            corner_radii.clone()
        } else {
            vec![*corner_radius; anchor_count]
        };
        if (next_radii[vertex_index] - local_radius).abs() < 1e-9 {
            // Same radius clicked again → toggle off (unfillet)
            next_radii[vertex_index] = 0.0;
        } else {
            next_radii[vertex_index] = local_radius;
        }

        svc.push_project_undo_snapshot(project)
            .map_err(|e| e.to_string())?;
        project.ensure_resolved(oid).map_err(|e| e.to_string())?;
        let target = project.find_object_mut(oid).ok_or("Object not found")?;
        if let ObjectData::Star { corner_radii, .. } = &mut target.data {
            *corner_radii = next_radii;
        }
        let updated = target.clone();
        drop(guard);
        planning::invalidate_plan_cache(svc).map_err(String::from)?;
        return Ok(updated);
    }

    // Read-only check for no-op
    let vp = to_world_vecpath(effective)?;
    let result = beambench_core::vector::path_ops::apply_radius_at_corner(
        &vp,
        subpath_index,
        vertex_index,
        radius_mm,
    );
    if result.to_svg_d() == vp.to_svg_d() {
        let obj = project.find_object(oid).ok_or("Object not found")?;
        return Ok(obj.clone());
    }

    svc.push_project_undo_snapshot(project)
        .map_err(|e| e.to_string())?;
    project.ensure_resolved(oid).map_err(|e| e.to_string())?;
    path_ops_core::ensure_denormalized(project.find_object_mut(oid).ok_or("Object not found")?);
    let source = project.find_object(oid).ok_or("Object not found")?.clone();
    let vp = to_world_vecpath(&source)?;
    let result = beambench_core::vector::path_ops::apply_radius_at_corner(
        &vp,
        subpath_index,
        vertex_index,
        radius_mm,
    );
    let updated = replace_object_with_path(project, oid, result)?;
    drop(guard);
    planning::invalidate_plan_cache(svc).map_err(String::from)?;
    Ok(updated)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArrayResult {
    pub created_ids: Vec<String>,
    pub group_id: Option<String>,
}

fn center_of_bounds(bounds: Bounds) -> Point2D {
    Point2D::new(
        (bounds.min.x + bounds.max.x) / 2.0,
        (bounds.min.y + bounds.max.y) / 2.0,
    )
}

fn bounds_from_center(center: Point2D, width: f64, height: f64) -> Bounds {
    Bounds::new(
        Point2D::new(center.x - width / 2.0, center.y - height / 2.0),
        Point2D::new(center.x + width / 2.0, center.y + height / 2.0),
    )
}

fn axis_scale(original: f64, transformed: f64) -> f64 {
    if original.abs() <= 1e-9 {
        1.0
    } else {
        transformed / original
    }
}

fn transform_vector(transform: Transform2D, point: Point2D) -> Point2D {
    Point2D::new(
        transform.a * point.x + transform.c * point.y,
        transform.b * point.x + transform.d * point.y,
    )
}

fn root_delta_transform(
    source_root: &ProjectObject,
    transformed_root: &ProjectObject,
) -> Transform2D {
    source_root
        .transform
        .inverse()
        .map(|inverse| transformed_root.transform.compose(&inverse))
        .unwrap_or(transformed_root.transform)
}

fn transform_member_geometry(
    source_root: &ProjectObject,
    transformed_root: &ProjectObject,
    member: &ProjectObject,
) -> (Bounds, Transform2D) {
    let root_center = center_of_bounds(source_root.bounds);
    let transformed_center = center_of_bounds(transformed_root.bounds);
    let member_center = center_of_bounds(member.bounds);
    let sx = axis_scale(source_root.bounds.width(), transformed_root.bounds.width());
    let sy = axis_scale(
        source_root.bounds.height(),
        transformed_root.bounds.height(),
    );
    let delta = root_delta_transform(source_root, transformed_root);
    let scaled_offset = Point2D::new(
        (member_center.x - root_center.x) * sx,
        (member_center.y - root_center.y) * sy,
    );
    let transformed_offset = transform_vector(delta, scaled_offset);
    let next_center = Point2D::new(
        transformed_center.x + transformed_offset.x,
        transformed_center.y + transformed_offset.y,
    );
    let next_bounds = bounds_from_center(
        next_center,
        member.bounds.width() * sx.abs(),
        member.bounds.height() * sy.abs(),
    );
    (next_bounds, delta.compose(&member.transform))
}

fn remap_array_object_refs(data: &mut ObjectData, id_map: &HashMap<ObjectId, ObjectId>) {
    match data {
        ObjectData::RasterImage { masks, .. } => {
            for mask in masks {
                if let Some(mapped) = id_map.get(&mask.object_id) {
                    mask.object_id = *mapped;
                }
            }
        }
        ObjectData::Text { guide_path_id, .. } => {
            if let Some(id) = guide_path_id {
                if let Some(mapped) = id_map.get(id) {
                    *id = *mapped;
                }
            }
        }
        ObjectData::Group { children } => {
            for child_id in children {
                if let Some(mapped) = id_map.get(child_id) {
                    *child_id = *mapped;
                }
            }
        }
        ObjectData::VirtualClone { source_id } => {
            if let Some(mapped) = id_map.get(source_id) {
                *source_id = *mapped;
            }
        }
        _ => {}
    }
}

fn collect_array_copy_template(
    project: &Project,
    object_id: ObjectId,
    seen: &mut HashSet<ObjectId>,
    templates: &mut Vec<ProjectObject>,
) -> Result<(), String> {
    if !seen.insert(object_id) {
        return Ok(());
    }
    let object = project
        .find_object(object_id)
        .cloned()
        .ok_or("Object not found")?;
    let children = match &object.data {
        ObjectData::Group { children } => children.clone(),
        _ => Vec::new(),
    };
    templates.push(object);
    for child_id in children {
        collect_array_copy_template(project, child_id, seen, templates)?;
    }
    Ok(())
}

fn collect_array_copy_templates(
    project: &Project,
    root_id: ObjectId,
) -> Result<Vec<ProjectObject>, String> {
    let mut seen = HashSet::new();
    let mut templates = Vec::new();
    collect_array_copy_template(project, root_id, &mut seen, &mut templates)?;
    Ok(templates)
}

fn recompute_copy_group_bounds(copies: &mut [ProjectObject]) {
    let index_by_id: HashMap<ObjectId, usize> = copies
        .iter()
        .enumerate()
        .map(|(idx, object)| (object.id, idx))
        .collect();
    for idx in (0..copies.len()).rev() {
        let children = match &copies[idx].data {
            ObjectData::Group { children } => children.clone(),
            _ => continue,
        };
        let mut bounds: Option<Bounds> = None;
        for child_id in children {
            if let Some(child_idx) = index_by_id.get(&child_id) {
                bounds = Some(match bounds {
                    Some(current) => current.union(&copies[*child_idx].bounds),
                    None => copies[*child_idx].bounds,
                });
            }
        }
        if let Some(next_bounds) = bounds {
            copies[idx].bounds = next_bounds;
            copies[idx].transform = Transform2D::identity();
        }
    }
}

fn recompute_project_group_bounds(project: &mut Project, group_id: ObjectId) {
    let children = project
        .find_object(group_id)
        .and_then(|object| match &object.data {
            ObjectData::Group { children } => Some(children.clone()),
            _ => None,
        });
    let Some(children) = children else {
        return;
    };
    for child_id in &children {
        recompute_project_group_bounds(project, *child_id);
    }
    let mut bounds: Option<Bounds> = None;
    for child_id in children {
        if let Some(child) = project.find_object(child_id) {
            bounds = Some(match bounds {
                Some(current) => current.union(&child.bounds),
                None => child.bounds,
            });
        }
    }
    if let Some(next_bounds) = bounds {
        if let Some(group) = project.find_object_mut(group_id) {
            group.bounds = next_bounds;
            group.transform = Transform2D::identity();
        }
    }
}

fn materialize_array_copy(
    project: &Project,
    source_root: &ProjectObject,
    transformed_root: ProjectObject,
    create_virtual: bool,
) -> Result<(Vec<ProjectObject>, ObjectId), String> {
    if !matches!(source_root.data, ObjectData::Group { .. }) {
        let mut copy = transformed_root;
        if create_virtual {
            copy.data = ObjectData::VirtualClone {
                source_id: source_root.id,
            };
        }
        return Ok((vec![copy.clone()], copy.id));
    }

    let templates = collect_array_copy_templates(project, source_root.id)?;
    let id_map: HashMap<ObjectId, ObjectId> = templates
        .iter()
        .map(|template| {
            let id = if template.id == source_root.id {
                transformed_root.id
            } else {
                ObjectId::new()
            };
            (template.id, id)
        })
        .collect();
    let mut copies = Vec::with_capacity(templates.len());
    for template in templates {
        let mut copy = template.clone();
        copy.id = id_map
            .get(&template.id)
            .copied()
            .unwrap_or_else(ObjectId::new);
        let (bounds, transform) =
            transform_member_geometry(source_root, &transformed_root, &template);
        copy.bounds = bounds;
        copy.transform = transform;
        remap_array_object_refs(&mut copy.data, &id_map);
        if create_virtual && !matches!(template.data, ObjectData::Group { .. }) {
            copy.data = ObjectData::VirtualClone {
                source_id: template.id,
            };
        }
        if matches!(copy.data, ObjectData::Group { .. }) {
            copy.transform = Transform2D::identity();
        }
        copies.push(copy);
    }
    recompute_copy_group_bounds(&mut copies);
    Ok((copies, transformed_root.id))
}

fn materialize_array_copies(
    project: &Project,
    sources: &[ProjectObject],
    transformed_roots: Vec<ProjectObject>,
    create_virtual: bool,
) -> Result<(Vec<ProjectObject>, Vec<ObjectId>), String> {
    if sources.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let mut copies = Vec::new();
    let mut root_ids = Vec::new();
    for (idx, transformed_root) in transformed_roots.into_iter().enumerate() {
        let source = &sources[idx % sources.len()];
        let (mut source_copies, root_id) =
            materialize_array_copy(project, source, transformed_root, create_virtual)?;
        root_ids.push(root_id);
        copies.append(&mut source_copies);
    }
    Ok((copies, root_ids))
}

fn apply_transformed_source_to_existing(
    project: &mut Project,
    source_root: &ProjectObject,
    transformed_root: &ProjectObject,
) -> Result<ObjectId, String> {
    let templates = collect_array_copy_templates(project, source_root.id)?;
    let mut group_ids = Vec::new();
    let mut updates = Vec::with_capacity(templates.len());
    for template in templates {
        let (bounds, transform) =
            transform_member_geometry(source_root, transformed_root, &template);
        if matches!(template.data, ObjectData::Group { .. }) {
            group_ids.push(template.id);
        }
        updates.push((template.id, bounds, transform));
    }
    for (object_id, bounds, transform) in updates {
        if let Some(object) = project.find_object_mut(object_id) {
            object.bounds = bounds;
            object.transform = if matches!(object.data, ObjectData::Group { .. }) {
                Transform2D::identity()
            } else {
                transform
            };
        }
    }
    for group_id in group_ids.into_iter().rev() {
        recompute_project_group_bounds(project, group_id);
    }
    Ok(source_root.id)
}

#[tauri::command]
pub fn grid_array(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    rows: u32,
    cols: u32,
    sizing_mode_x: Option<String>,
    sizing_mode_y: Option<String>,
    total_width_mm: Option<f64>,
    total_height_mm: Option<f64>,
    h_spacing_mm: f64,
    v_spacing_mm: f64,
    #[allow(unused_variables)] spacing_mode: Option<String>,
    mirror_alternate_cols: Option<bool>,
    mirror_alternate_rows: Option<bool>,
    x_col_shift_mm: Option<f64>,
    y_row_shift_mm: Option<f64>,
    half_shift: Option<bool>,
    reverse_h: Option<bool>,
    reverse_v: Option<bool>,
    random_orientation: Option<bool>,
    random_seed: Option<u64>,
    group_results: Option<bool>,
    create_virtual: Option<bool>,
    auto_increment_text: Option<bool>,
    text_increment: Option<i64>,
) -> Result<ArrayResult, String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|id| parse_id(id))
        .collect::<Result<Vec<_>, _>>()?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    let sources: Vec<ProjectObject> = parsed_ids
        .iter()
        .map(|id| project.find_object(*id).cloned().ok_or("Object not found"))
        .collect::<Result<Vec<_>, _>>()?;

    let spacing_mode_enum = match spacing_mode.as_deref() {
        Some("edgeToEdge") => SpacingMode::EdgeToEdge,
        _ => SpacingMode::CenterToCenter,
    };
    let sizing_mode_x_enum = match sizing_mode_x.as_deref() {
        Some("total") => GridArraySizingMode::Total,
        _ => GridArraySizingMode::Count,
    };
    let sizing_mode_y_enum = match sizing_mode_y.as_deref() {
        Some("total") => GridArraySizingMode::Total,
        _ => GridArraySizingMode::Count,
    };

    let use_virtual = create_virtual.unwrap_or(false);
    let mut config = GridArrayConfig {
        rows,
        cols,
        sizing_mode_x: sizing_mode_x_enum,
        sizing_mode_y: sizing_mode_y_enum,
        total_width_mm,
        total_height_mm,
        h_spacing_mm,
        v_spacing_mm,
        spacing_mode: spacing_mode_enum,
        mirror_alternate_cols: mirror_alternate_cols.unwrap_or(false),
        mirror_alternate_rows: mirror_alternate_rows.unwrap_or(false),
        x_col_shift_mm: x_col_shift_mm.unwrap_or(0.0),
        y_row_shift_mm: y_row_shift_mm.unwrap_or(0.0),
        half_shift: half_shift.unwrap_or(false),
        reverse_h: reverse_h.unwrap_or(false),
        reverse_v: reverse_v.unwrap_or(false),
        random_orientation: random_orientation.unwrap_or(false),
        random_seed: random_seed.unwrap_or(0),
        group_results: group_results.unwrap_or(false),
        create_virtual: use_virtual,
        // Virtual clones inherit geometry from source, so auto-increment is meaningless
        auto_increment_text: if use_virtual {
            false
        } else {
            auto_increment_text.unwrap_or(false)
        },
        text_increment: text_increment.unwrap_or(1),
    };
    let (fitted_rows, fitted_cols) = beambench_core::fit_grid_array_counts(&sources, &config, 100);
    config.rows = fitted_rows;
    config.cols = fitted_cols;

    let transformed_roots = beambench_core::grid_array_in_project(project, &sources, &config);
    let (copies, root_ids) =
        materialize_array_copies(project, &sources, transformed_roots, config.create_virtual)?;

    // `grid_array_in_project` and `materialize_array_copies` only read the
    // project, so push the undo snapshot after they succeed — a failed
    // materialize (dangling group child refs) must not leave a phantom
    // no-op undo entry. The first project mutation is the add loop below.
    svc.push_project_undo_snapshot(project)
        .map_err(|e| e.to_string())?;

    // Layer invariant: rasters stay on image layers, non-rasters stay
    // off image layers. Virtual clones resolve to their source's data
    // at plan time, so the clone's host layer must match the SOURCE's
    // effective content type. Using `effective_is_raster` (rather
    // than a literal RasterImage match) also handles clone-of-clone
    // chains where the indirect source is a raster.
    let mut added_root_ids = HashSet::new();
    let mut created_ids = Vec::with_capacity(root_ids.len());
    for mut copy in copies {
        if matches!(copy.data, ObjectData::VirtualClone { .. }) {
            let target = beambench_service::RoutingTarget::from_data(&copy.data, project);
            let (dest, _) =
                beambench_service::resolve_layer_for_object(project, copy.layer_id, target)
                    .map_err(|e| e.to_string())?;
            copy.layer_id = dest;
        }
        let added = project.add_object(copy).clone();
        if root_ids.contains(&added.id) && added_root_ids.insert(added.id) {
            created_ids.push(added.id.to_string());
        }
    }

    // Group results if requested
    let group_id = if config.group_results && created_ids.len() > 1 {
        let child_ids: Vec<ObjectId> = created_ids
            .iter()
            .map(|s| parse_id(s))
            .collect::<Result<Vec<_>, _>>()?;
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for id in &child_ids {
            if let Some(obj) = project.find_object(*id) {
                min_x = min_x.min(obj.bounds.min.x);
                min_y = min_y.min(obj.bounds.min.y);
                max_x = max_x.max(obj.bounds.max.x);
                max_y = max_y.max(obj.bounds.max.y);
            }
        }
        let group_bounds = Bounds::new(Point2D::new(min_x, min_y), Point2D::new(max_x, max_y));
        // Group is non-raster, so route to a non-image sibling if
        // sources live on an image layer.
        let (layer_id, _) = beambench_service::resolve_layer_for_object(
            project,
            sources[0].layer_id,
            beambench_service::RoutingTarget::NeedsNonImage,
        )
        .map_err(|e| e.to_string())?;
        let group = ProjectObject::new(
            "Array Group",
            layer_id,
            group_bounds,
            ObjectData::Group {
                children: child_ids,
            },
        );
        let gid = group.id.to_string();
        project.add_object(group);
        Some(gid)
    } else {
        None
    };

    project.dirty = true;
    drop(guard);
    planning::invalidate_plan_cache(&svc).map_err(String::from)?;
    Ok(ArrayResult {
        created_ids,
        group_id,
    })
}

#[tauri::command]
pub fn circular_array(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    count: u32,
    radius_mm: f64,
    rotate_copies: Option<bool>,
    center_x: Option<f64>,
    center_y: Option<f64>,
    center_object_id: Option<String>,
    start_angle_deg: Option<f64>,
    end_angle_deg: Option<f64>,
    group_results: Option<bool>,
    create_virtual: Option<bool>,
    auto_increment_text: Option<bool>,
    text_increment: Option<i64>,
) -> Result<ArrayResult, String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|id| parse_id(id))
        .collect::<Result<Vec<_>, _>>()?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;

    // If center_object_id is set, use that object's center as pivot and exclude it from sources
    let center_oid = center_object_id
        .as_ref()
        .map(|id| parse_id(id))
        .transpose()?;
    let (explicit_cx, explicit_cy) = if let Some(coid) = center_oid {
        let cobj = project.find_object(coid).ok_or("Center object not found")?;
        let cx = (cobj.bounds.min.x + cobj.bounds.max.x) / 2.0;
        let cy = (cobj.bounds.min.y + cobj.bounds.max.y) / 2.0;
        (Some(cx), Some(cy))
    } else {
        (center_x, center_y)
    };

    let sources: Vec<ProjectObject> = parsed_ids
        .iter()
        .filter(|id| center_oid.is_none_or(|coid| **id != coid))
        .map(|id| project.find_object(*id).cloned().ok_or("Object not found"))
        .collect::<Result<Vec<_>, _>>()?;

    if sources.is_empty() {
        return Err("No source objects for circular array".into());
    }

    let use_virtual = create_virtual.unwrap_or(false);
    let config = CircularArrayConfig {
        count,
        radius_mm,
        rotate_copies: rotate_copies.unwrap_or(true),
        center_x: explicit_cx,
        center_y: explicit_cy,
        start_angle_deg: start_angle_deg.unwrap_or(0.0),
        end_angle_deg: end_angle_deg.unwrap_or(360.0),
        group_results: group_results.unwrap_or(false),
        create_virtual: use_virtual,
        // Virtual clones inherit geometry from source, so auto-increment is meaningless
        auto_increment_text: if use_virtual {
            false
        } else {
            auto_increment_text.unwrap_or(false)
        },
        text_increment: text_increment.unwrap_or(1),
    };

    let all_positions = beambench_core::circular_array(&sources, &config);

    // The push below must stay ahead of `apply_transformed_source_to_existing`
    // (the first project mutation), which runs before `materialize_array_copies`
    // — so the snapshot cannot move after materialize. Instead, hoist the only
    // fallible step shared by both helpers (`collect_array_copy_templates`,
    // which errors on dangling group child refs) as an up-front validation so
    // a doomed command never pushes a phantom no-op undo entry.
    for source in &sources {
        collect_array_copy_templates(project, source.id)?;
    }

    svc.push_project_undo_snapshot(project)
        .map_err(|e| e.to_string())?;

    // First sources.len() items are position-0 (original IDs) — update originals in-place.
    // Remaining items are new copies at positions 1..count.
    let src_count = sources.len();
    let (position_zero, new_copies) = if all_positions.len() > src_count {
        all_positions.split_at(src_count)
    } else {
        (all_positions.as_slice(), [].as_slice())
    };

    // Update originals in-place to position 0 on the circle. Groups need the
    // same unit transform applied to their children, not just to the shell.
    let mut position_zero_root_ids = Vec::with_capacity(position_zero.len());
    for (source, updated) in sources.iter().zip(position_zero.iter()) {
        position_zero_root_ids.push(apply_transformed_source_to_existing(
            project, source, updated,
        )?);
    }

    let (new_copies, new_root_ids) = materialize_array_copies(
        project,
        &sources,
        new_copies.to_vec(),
        config.create_virtual,
    )?;

    // Include original IDs first, then new copies — all are part of the array.
    // Virtual clones resolve to their source's data at plan time, so
    // the clone's host layer must match the SOURCE's effective
    // content type. `effective_is_raster` (via
    // `RoutingTarget::from_data`) follows VirtualClone chains, so
    // clone-of-clone-of-raster still lands on an image layer.
    let mut created_ids = Vec::with_capacity(src_count + new_root_ids.len());
    for root_id in position_zero_root_ids {
        created_ids.push(root_id.to_string());
    }
    let mut added_root_ids = HashSet::new();
    for mut copy in new_copies {
        if matches!(copy.data, ObjectData::VirtualClone { .. }) {
            let target = beambench_service::RoutingTarget::from_data(&copy.data, project);
            let (dest, _) =
                beambench_service::resolve_layer_for_object(project, copy.layer_id, target)
                    .map_err(|e| e.to_string())?;
            copy.layer_id = dest;
        }
        let added = project.add_object(copy).clone();
        if new_root_ids.contains(&added.id) && added_root_ids.insert(added.id) {
            created_ids.push(added.id.to_string());
        }
    }

    // Group results if requested
    let group_id = if config.group_results && created_ids.len() > 1 {
        let child_ids: Vec<ObjectId> = created_ids
            .iter()
            .map(|s| parse_id(s))
            .collect::<Result<Vec<_>, _>>()?;
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for id in &child_ids {
            if let Some(obj) = project.find_object(*id) {
                min_x = min_x.min(obj.bounds.min.x);
                min_y = min_y.min(obj.bounds.min.y);
                max_x = max_x.max(obj.bounds.max.x);
                max_y = max_y.max(obj.bounds.max.y);
            }
        }
        let group_bounds = Bounds::new(Point2D::new(min_x, min_y), Point2D::new(max_x, max_y));
        // Group is non-raster, so route to a non-image sibling if
        // sources live on an image layer.
        let (layer_id, _) = beambench_service::resolve_layer_for_object(
            project,
            sources[0].layer_id,
            beambench_service::RoutingTarget::NeedsNonImage,
        )
        .map_err(|e| e.to_string())?;
        let group = ProjectObject::new(
            "Array Group",
            layer_id,
            group_bounds,
            ObjectData::Group {
                children: child_ids,
            },
        );
        let gid = group.id.to_string();
        project.add_object(group);
        Some(gid)
    } else {
        None
    };

    project.dirty = true;
    drop(guard);
    planning::invalidate_plan_cache(&svc).map_err(String::from)?;
    Ok(ArrayResult {
        created_ids,
        group_id,
    })
}

#[tauri::command]
pub fn copy_along_path(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    path_object_id: String,
    count: u32,
    rotate: bool,
    scale_copies: Option<bool>,
    final_scale_percent: Option<f64>,
) -> Result<Vec<ProjectObject>, String> {
    let oid: ObjectId = parse_id(&object_id)?;
    let path_oid: ObjectId = parse_id(&path_object_id)?;
    copy_along_path_batch_inner(
        &svc,
        &[oid],
        path_oid,
        count,
        rotate,
        scale_copies.unwrap_or(false),
        final_scale_percent.unwrap_or(100.0),
    )
}

fn copy_along_path_batch_inner(
    svc: &ServiceContext,
    object_ids: &[ObjectId],
    path_oid: ObjectId,
    count: u32,
    rotate: bool,
    scale_copies: bool,
    final_scale_percent: f64,
) -> Result<Vec<ProjectObject>, String> {
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    if object_ids.is_empty() {
        return Err("No source objects provided".to_string());
    }
    project
        .ensure_resolved(path_oid)
        .map_err(|e| e.to_string())?;
    let path_obj = project
        .find_object(path_oid)
        .ok_or("Path object not found")?
        .clone();
    let guide = to_world_vecpath(&path_obj)?;
    let mut sources = Vec::with_capacity(object_ids.len());
    for object_id in object_ids {
        project
            .ensure_resolved(*object_id)
            .map_err(|e| e.to_string())?;
        let source = project
            .find_object(*object_id)
            .ok_or("Source object not found")?
            .clone();
        sources.push(source);
    }
    let guide_length = beambench_core::vector::flatten::flatten_vecpath(
        &guide,
        beambench_core::vector::flatten::DEFAULT_TOLERANCE_MM,
    )
    .into_iter()
    .map(|poly| {
        let mut length = poly
            .points
            .windows(2)
            .map(|segment| segment[0].distance_to(&segment[1]))
            .sum::<f64>();
        if poly.closed && poly.points.len() > 1 {
            length += poly.points.last().unwrap().distance_to(&poly.points[0]);
        }
        length
    })
    .sum::<f64>();
    let spacing_mm = if count == 0 {
        guide_length.max(1.0)
    } else {
        (guide_length / count as f64).max(0.1)
    };
    svc.push_project_undo_snapshot(project)
        .map_err(|e| e.to_string())?;
    let mut created = Vec::new();
    for source in &sources {
        let transformed_roots = beambench_core::copy_along_path(
            source,
            &guide,
            spacing_mm,
            rotate,
            scale_copies,
            final_scale_percent,
        );
        let (copies, _root_ids) = materialize_array_copies(
            project,
            std::slice::from_ref(source),
            transformed_roots,
            false,
        )?;
        created.reserve(copies.len());
        for copy in copies {
            created.push(project.add_object(copy).clone());
        }
    }
    drop(guard);
    planning::invalidate_plan_cache(&svc).map_err(String::from)?;
    Ok(created)
}

#[tauri::command]
pub fn copy_along_path_batch(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    path_object_id: String,
    count: u32,
    rotate: bool,
    scale_copies: Option<bool>,
    final_scale_percent: Option<f64>,
) -> Result<Vec<ProjectObject>, String> {
    let parsed_ids = object_ids
        .iter()
        .map(|id| parse_id(id))
        .collect::<Result<Vec<ObjectId>, _>>()?;
    let path_oid: ObjectId = parse_id(&path_object_id)?;
    copy_along_path_batch_inner(
        &svc,
        &parsed_ids,
        path_oid,
        count,
        rotate,
        scale_copies.unwrap_or(false),
        final_scale_percent.unwrap_or(100.0),
    )
}

#[tauri::command]
pub fn rubber_band_outline(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<ProjectObject, String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    // Resolve VirtualClones to their source geometry (transient)
    let objects: Vec<_> = project
        .objects
        .iter()
        .filter(|o| parsed_ids.contains(&o.id))
        .map(|o| project.resolve_clone(o).unwrap_or_else(|| o.clone()))
        .collect();
    let result = beambench_core::rubber_band_outline(&objects);
    // rubber_band_outline produces a VectorPath — if the selection
    // started with a raster on an image layer, the naive
    // `objects.first().layer_id` would drop the new vector onto that
    // image layer and violate the layer-content invariant. Route it
    // to a matching non-image sibling instead.
    let requested_layer = objects.first().ok_or("No objects provided")?.layer_id;
    svc.push_project_undo_snapshot(project)
        .map_err(|e| e.to_string())?;
    let (layer_id, _rerouted) = beambench_service::resolve_layer_for_object(
        project,
        requested_layer,
        beambench_service::RoutingTarget::NeedsNonImage,
    )
    .map_err(|e| e.to_string())?;
    let created = vecpath_to_project_object("Rubber Band Outline".to_string(), layer_id, result);
    let created = project.add_object(created).clone();
    drop(guard);
    planning::invalidate_plan_cache(&svc).map_err(String::from)?;
    Ok(created)
}

#[tauri::command]
pub fn apply_path_to_text(
    svc: State<'_, Arc<ServiceContext>>,
    text_object_id: String,
    path_object_id: String,
) -> Result<ProjectObject, String> {
    let text_oid: ObjectId = parse_id(&text_object_id)?;
    let path_oid: ObjectId = parse_id(&path_object_id)?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    svc.push_project_undo_snapshot(project)
        .map_err(|e| e.to_string())?;
    project
        .ensure_resolved(text_oid)
        .map_err(|e| e.to_string())?;
    project
        .ensure_resolved(path_oid)
        .map_err(|e| e.to_string())?;
    let text_obj = project
        .find_object(text_oid)
        .ok_or("Text object not found")?
        .clone();
    let (text, font_family, font_size_mm, bold, italic) = match &text_obj.data {
        ObjectData::Text {
            content,
            font_family,
            font_size_mm,
            bold,
            italic,
            ..
        } => (
            content.clone(),
            font_family.clone(),
            *font_size_mm,
            *bold,
            *italic,
        ),
        _ => return Err("Object is not a text object".to_string()),
    };
    let path_obj = project
        .find_object(path_oid)
        .ok_or("Path object not found")?
        .clone();
    let vp = to_world_vecpath(&path_obj)?;
    let result = beambench_core::apply_path_to_text_with_options(
        &text,
        &font_family,
        font_size_mm,
        bold,
        italic,
        &vp,
    );
    project.remove_object(text_oid);
    let created = vecpath_to_project_object(text_obj.name.clone(), text_obj.layer_id, result);
    let created = project.add_object(created).clone();
    drop(guard);
    planning::invalidate_plan_cache(&svc).map_err(String::from)?;
    Ok(created)
}

fn polyline_has_area(poly: &Polyline) -> bool {
    poly.closed
        && poly.points.len() >= 3
        && beambench_core::vector::offset::signed_area(&poly.points).abs() > 1e-9
}

fn point_in_polyline(point: Point2D, poly: &Polyline) -> bool {
    let pts = &poly.points;
    let n = pts.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (pts[i].x, pts[i].y);
        let (xj, yj) = (pts[j].x, pts[j].y);
        if ((yi > point.y) != (yj > point.y))
            && (point.x < (xj - xi) * (point.y - yi) / (yj - yi) + xi)
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn point_in_any_polyline(point: Point2D, polys: &[Polyline]) -> bool {
    let mut winding = 0;
    for poly in polys {
        if !polyline_has_area(poly) {
            continue;
        }
        if point_in_polyline(point, poly) {
            winding += if beambench_core::vector::offset::signed_area(&poly.points) >= 0.0 {
                1
            } else {
                -1
            };
        }
    }
    winding != 0
}

fn encode_rgba_png(image: RgbaImage) -> Result<Vec<u8>, String> {
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|e| format!("Failed to encode cropped image: {e}"))?;
    Ok(bytes.into_inner())
}

fn crop_raster_image_to_mask(
    image_obj: &ProjectObject,
    image_bytes: &[u8],
    mask_path: &VecPath,
) -> Result<(Vec<u8>, u32, u32, Bounds), String> {
    let decoded = image::load_from_memory(image_bytes)
        .map_err(|e| format!("Failed to decode source image: {e}"))?
        .to_rgba8();
    let (width_px, height_px) = decoded.dimensions();
    if width_px == 0 || height_px == 0 {
        return Err("Source image has no pixels".to_string());
    }

    let polylines = beambench_core::vector::flatten::flatten_vecpath(
        mask_path,
        beambench_core::vector::flatten::DEFAULT_TOLERANCE_MM,
    );
    let has_mask_area = polylines.iter().any(polyline_has_area);
    if !has_mask_area {
        return Err("Mask object has no usable area".to_string());
    }

    let mut masked = decoded.clone();
    let x_step = image_obj.bounds.width() / width_px as f64;
    let y_step = image_obj.bounds.height() / height_px as f64;
    let center = Point2D::new(
        (image_obj.bounds.min.x + image_obj.bounds.max.x) / 2.0,
        (image_obj.bounds.min.y + image_obj.bounds.max.y) / 2.0,
    );
    let mut min_x = width_px;
    let mut min_y = height_px;
    let mut max_x = 0;
    let mut max_y = 0;
    let mut visible = false;

    for y in 0..height_px {
        for x in 0..width_px {
            let local = Point2D::new(
                image_obj.bounds.min.x + (x as f64 + 0.5) * x_step,
                image_obj.bounds.min.y + (y as f64 + 0.5) * y_step,
            );
            let world = if image_obj.transform.is_identity() {
                local
            } else {
                image_obj.transform.apply_around_center(&local, &center)
            };
            if point_in_any_polyline(world, &polylines) {
                if masked.get_pixel(x, y)[3] > 0 {
                    visible = true;
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            } else {
                masked.get_pixel_mut(x, y)[3] = 0;
            }
        }
    }

    if !visible {
        return Err("Crop mask does not overlap any visible image pixels".to_string());
    }

    let crop_width = max_x - min_x + 1;
    let crop_height = max_y - min_y + 1;
    let cropped =
        image::imageops::crop_imm(&masked, min_x, min_y, crop_width, crop_height).to_image();
    let new_bounds = Bounds::new(
        Point2D::new(
            image_obj.bounds.min.x + min_x as f64 * x_step,
            image_obj.bounds.min.y + min_y as f64 * y_step,
        ),
        Point2D::new(
            image_obj.bounds.min.x + (max_x + 1) as f64 * x_step,
            image_obj.bounds.min.y + (max_y + 1) as f64 * y_step,
        ),
    );

    Ok((
        encode_rgba_png(cropped)?,
        crop_width,
        crop_height,
        new_bounds,
    ))
}

#[tauri::command]
pub fn crop_image(
    svc: State<'_, Arc<ServiceContext>>,
    image_object_id: String,
    mask_object_id: String,
) -> Result<ProjectObject, String> {
    let img_oid: ObjectId = parse_id(&image_object_id)?;
    let mask_oid: ObjectId = parse_id(&mask_object_id)?;
    if img_oid == mask_oid {
        return Err("Image cannot crop itself".to_string());
    }

    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    project
        .ensure_resolved(img_oid)
        .map_err(|e| e.to_string())?;
    project
        .ensure_resolved(mask_oid)
        .map_err(|e| e.to_string())?;
    validate_closed_mask_object(project, mask_oid)?;

    let image_obj = project
        .find_object(img_oid)
        .ok_or("Image object not found")?
        .clone();
    let mask_obj = project
        .find_object(mask_oid)
        .ok_or("Mask object not found")?
        .clone();
    let (asset_key, adjustments) = match &image_obj.data {
        ObjectData::RasterImage {
            asset_key,
            adjustments,
            ..
        } => (asset_key.clone(), adjustments.clone()),
        _ => return Err("Target object is not a raster image".to_string()),
    };
    let asset_id = AssetId::from_uuid(
        Uuid::parse_str(&asset_key).map_err(|e| format!("Invalid image asset reference: {e}"))?,
    );
    let image_bytes = project
        .asset_data
        .get(&asset_id)
        .cloned()
        .ok_or("Image asset data not found")?;
    let mask_path = object_to_world_vecpath_resolved(&mask_obj, project)
        .ok_or("Mask object is not a vector type")?;
    let (cropped_bytes, width_px, height_px, cropped_bounds) =
        crop_raster_image_to_mask(&image_obj, &image_bytes, &mask_path)?;

    svc.push_project_undo_snapshot(project)
        .map_err(|e| e.to_string())?;
    let asset = Asset::new(
        format!("{} Crop.png", image_obj.name),
        AssetMediaType::Png,
        cropped_bytes.len() as u64,
        Some(width_px),
        Some(height_px),
    );
    let new_asset_id = asset.id;
    project.add_asset(asset, cropped_bytes);
    {
        let object = project
            .find_object_mut(img_oid)
            .ok_or("Image object not found")?;
        object.data = ObjectData::RasterImage {
            asset_key: new_asset_id.to_string(),
            original_width_px: width_px,
            original_height_px: height_px,
            adjustments,
            masks: Vec::new(),
        };
        object.bounds = cropped_bounds;
        object.tabs.clear();
        object.start_point_edits.clear();
    }
    project.remove_object(mask_oid);
    let (dest_layer, _rerouted) = beambench_service::resolve_layer_for_object(
        project,
        image_obj.layer_id,
        beambench_service::RoutingTarget::NeedsImage,
    )
    .map_err(|e| e.to_string())?;
    if dest_layer != image_obj.layer_id {
        if let Some(object) = project.find_object_mut(img_oid) {
            object.layer_id = dest_layer;
        }
    }
    let updated = project
        .find_object(img_oid)
        .ok_or("Image object not found")?
        .clone();
    drop(guard);
    planning::invalidate_plan_cache(&svc).map_err(String::from)?;
    Ok(updated)
}

#[tauri::command]
pub fn apply_mask_to_image(
    svc: State<'_, Arc<ServiceContext>>,
    image_object_id: String,
    mask_object_id: String,
) -> Result<ProjectObject, String> {
    let img_oid: ObjectId = parse_id(&image_object_id)?;
    let mask_oid: ObjectId = parse_id(&mask_object_id)?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    svc.push_project_undo_snapshot(project)
        .map_err(|e| e.to_string())?;
    project
        .ensure_resolved(img_oid)
        .map_err(|e| e.to_string())?;
    project
        .ensure_resolved(mask_oid)
        .map_err(|e| e.to_string())?;
    let img_obj = project
        .find_object(img_oid)
        .ok_or("Image object not found")?
        .clone();
    let mask_obj = project
        .find_object(mask_oid)
        .ok_or("Mask object not found")?
        .clone();
    let mask_vp = to_world_vecpath(&mask_obj)?;
    let result = beambench_core::vector::boolean::apply_mask(&img_obj.bounds, &mask_vp);
    let mut updated = replace_object_with_path(project, img_oid, result)?;
    // Belt-and-suspenders: apply_mask_to_image converts a raster image
    // into vector content in-place. `replace_object_with_path` already
    // reroutes the new vector object off any image layer, but make it
    // visible here too so the invariant is obvious at the command
    // level, and keep the object's layer_id in the returned payload
    // in sync with the rerouted destination.
    if matches!(updated.data, ObjectData::VectorPath { .. }) {
        let (dest_layer, _rerouted) = beambench_service::resolve_layer_for_object(
            project,
            updated.layer_id,
            beambench_service::RoutingTarget::NeedsNonImage,
        )
        .map_err(|e| e.to_string())?;
        if dest_layer != updated.layer_id {
            if let Some(o) = project.find_object_mut(img_oid) {
                o.layer_id = dest_layer;
            }
            updated.layer_id = dest_layer;
        }
    }
    drop(guard);
    planning::invalidate_plan_cache(&svc).map_err(String::from)?;
    Ok(updated)
}

fn validate_closed_mask_object(project: &Project, mask_oid: ObjectId) -> Result<(), String> {
    let mask_obj = project
        .find_object(mask_oid)
        .ok_or("Mask object not found")?;
    let mask_vp = object_to_world_vecpath_resolved(mask_obj, project)
        .ok_or("Mask object is not a vector type")?;
    let has_closed_area = mask_vp
        .subpaths
        .iter()
        .any(|sp| sp.closed && sp.commands.len() >= 4);
    if !has_closed_area {
        return Err("Mask object must be a closed vector shape".to_string());
    }
    if mask_vp
        .visual_bounds()
        .or_else(|| mask_vp.bounds())
        .is_none_or(|b| b.width().abs() <= 1e-9 || b.height().abs() <= 1e-9)
    {
        return Err("Mask object has no usable area".to_string());
    }
    Ok(())
}

fn assign_image_mask_refs(
    project: &mut Project,
    image_oid: ObjectId,
    mask_oids: Vec<ObjectId>,
    polarity: ImageMaskPolarity,
) -> Result<ProjectObject, String> {
    let image = project
        .find_object_mut(image_oid)
        .ok_or("Image object not found")?;
    let ObjectData::RasterImage { masks, .. } = &mut image.data else {
        return Err("Target object is not a raster image".to_string());
    };
    for oid in mask_oids {
        if !masks.iter().any(|mask| mask.object_id == oid) {
            masks.push(ImageMaskRef {
                object_id: oid,
                polarity,
            });
        }
    }
    Ok(image.clone())
}

fn update_image_mask_polarity(
    project: &mut Project,
    image_oid: ObjectId,
    mask_oid: ObjectId,
    polarity: ImageMaskPolarity,
) -> Result<ProjectObject, String> {
    let image = project
        .find_object_mut(image_oid)
        .ok_or("Image object not found")?;
    let ObjectData::RasterImage { masks, .. } = &mut image.data else {
        return Err("Target object is not a raster image".to_string());
    };
    let mask = masks
        .iter_mut()
        .find(|mask| mask.object_id == mask_oid)
        .ok_or("Mask reference not found")?;
    mask.polarity = polarity;
    Ok(image.clone())
}

fn remove_image_mask_refs(
    project: &mut Project,
    image_oid: ObjectId,
    mask_oid: Option<ObjectId>,
) -> Result<ProjectObject, String> {
    let image = project
        .find_object_mut(image_oid)
        .ok_or("Image object not found")?;
    let ObjectData::RasterImage { masks, .. } = &mut image.data else {
        return Err("Target object is not a raster image".to_string());
    };
    match mask_oid {
        Some(oid) => masks.retain(|mask| mask.object_id != oid),
        None => masks.clear(),
    }
    Ok(image.clone())
}

#[tauri::command]
pub fn assign_image_mask(
    svc: State<'_, Arc<ServiceContext>>,
    image_object_id: String,
    mask_object_ids: Vec<String>,
    polarity: ImageMaskPolarity,
) -> Result<ProjectObject, String> {
    let image_oid: ObjectId = parse_id(&image_object_id)?;
    let mut mask_oids = Vec::new();
    let mut seen = HashSet::new();
    for id in mask_object_ids {
        let oid: ObjectId = parse_id(&id)?;
        if oid == image_oid {
            return Err("Image cannot mask itself".to_string());
        }
        if seen.insert(oid) {
            mask_oids.push(oid);
        }
    }
    if mask_oids.is_empty() {
        return Err("Select at least one mask object".to_string());
    }

    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    let image_obj = project
        .find_object(image_oid)
        .ok_or("Image object not found")?;
    if !matches!(image_obj.data, ObjectData::RasterImage { .. }) {
        return Err("Target object is not a raster image".to_string());
    }
    for oid in &mask_oids {
        validate_closed_mask_object(project, *oid)?;
    }

    svc.push_project_undo_snapshot(project)
        .map_err(|e| e.to_string())?;
    let updated = assign_image_mask_refs(project, image_oid, mask_oids, polarity)?;
    project.dirty = true;
    drop(guard);
    planning::invalidate_plan_cache(&svc).map_err(String::from)?;
    Ok(updated)
}

#[tauri::command]
pub fn set_image_mask_polarity(
    svc: State<'_, Arc<ServiceContext>>,
    image_object_id: String,
    mask_object_id: String,
    polarity: ImageMaskPolarity,
) -> Result<ProjectObject, String> {
    let image_oid: ObjectId = parse_id(&image_object_id)?;
    let mask_oid: ObjectId = parse_id(&mask_object_id)?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    {
        let image = project
            .find_object(image_oid)
            .ok_or("Image object not found")?;
        let ObjectData::RasterImage { masks, .. } = &image.data else {
            return Err("Target object is not a raster image".to_string());
        };
        if !masks.iter().any(|mask| mask.object_id == mask_oid) {
            return Err("Mask reference not found".to_string());
        }
    }
    svc.push_project_undo_snapshot(project)
        .map_err(|e| e.to_string())?;
    let updated = update_image_mask_polarity(project, image_oid, mask_oid, polarity)?;
    project.dirty = true;
    drop(guard);
    planning::invalidate_plan_cache(&svc).map_err(String::from)?;
    Ok(updated)
}

#[tauri::command]
pub fn remove_image_mask(
    svc: State<'_, Arc<ServiceContext>>,
    image_object_id: String,
    mask_object_id: Option<String>,
) -> Result<ProjectObject, String> {
    let image_oid: ObjectId = parse_id(&image_object_id)?;
    let mask_oid = mask_object_id.as_deref().map(parse_id).transpose()?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    {
        let image = project
            .find_object(image_oid)
            .ok_or("Image object not found")?;
        if !matches!(image.data, ObjectData::RasterImage { .. }) {
            return Err("Target object is not a raster image".to_string());
        }
    }
    svc.push_project_undo_snapshot(project)
        .map_err(|e| e.to_string())?;
    let updated = remove_image_mask_refs(project, image_oid, mask_oid)?;
    project.dirty = true;
    drop(guard);
    planning::invalidate_plan_cache(&svc).map_err(String::from)?;
    Ok(updated)
}

#[tauri::command]
pub fn convert_to_bitmap(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    dpi: f64,
) -> Result<ProjectObject, String> {
    let oid: ObjectId = parse_id(&object_id)?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    svc.push_project_undo_snapshot(project)
        .map_err(|e| e.to_string())?;
    project.ensure_resolved(oid).map_err(|e| e.to_string())?;
    let source = project.find_object(oid).ok_or("Object not found")?.clone();
    let vp = to_world_vecpath(&source)?;
    let (bytes, width_px, height_px, bounds) = rasterize_vecpath_to_png(&vp, dpi)?;
    let asset = Asset::new(
        format!("{}.png", source.name),
        AssetMediaType::Png,
        bytes.len() as u64,
        Some(width_px),
        Some(height_px),
    );
    let asset_id = asset.id;
    project.add_asset(asset, bytes);
    let current_layer = {
        let object = project.find_object_mut(oid).ok_or("Object not found")?;
        object.data = ObjectData::RasterImage {
            asset_key: asset_id.to_string(),
            original_width_px: width_px,
            original_height_px: height_px,
            adjustments: None,
            masks: Vec::new(),
        };
        object.bounds = bounds;
        object.transform = Transform2D::identity();
        object.start_point_edits.clear();
        object.tabs.clear();
        object.layer_id
    };
    // The object just became a raster — if it's on a non-image layer
    // (because it used to be a vector there), reroute it to an image
    // sibling so the layer-content invariant holds.
    let (dest_layer, _rerouted) = beambench_service::resolve_layer_for_object(
        project,
        current_layer,
        beambench_service::RoutingTarget::NeedsImage,
    )
    .map_err(|e| e.to_string())?;
    if dest_layer != current_layer {
        if let Some(o) = project.find_object_mut(oid) {
            o.layer_id = dest_layer;
        }
    }
    beambench_service::ops::project::sync_passthrough_single_object(project, oid);
    let updated = project.find_object(oid).ok_or("Object not found")?.clone();
    drop(guard);
    planning::invalidate_plan_cache(&svc).map_err(String::from)?;
    Ok(updated)
}

#[tauri::command]
pub fn add_tabs(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    count: u32,
    width_mm: f64,
) -> Result<ProjectObject, String> {
    let oid: ObjectId = parse_id(&object_id)?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;

    // Guard: auto-distribute tabs converts to VectorPath — require it already be one
    let obj = project.find_object(oid).ok_or("Object not found")?;
    let resolved = project.resolve_clone(obj);
    let effective = resolved.as_ref().unwrap_or(obj);
    if !matches!(effective.data, ObjectData::VectorPath { .. }) {
        return Err("Convert to path first".into());
    }

    svc.push_project_undo_snapshot(project)
        .map_err(|e| e.to_string())?;
    project.ensure_resolved(oid).map_err(|e| e.to_string())?;
    path_ops_core::ensure_denormalized(project.find_object_mut(oid).ok_or("Object not found")?);
    let source = project.find_object(oid).ok_or("Object not found")?.clone();
    let vp = to_world_vecpath(&source)?;
    let result = beambench_core::vector::tabs::add_tabs(&vp, count, width_mm);
    let updated = replace_object_with_path(project, oid, result)?;
    drop(guard);
    planning::invalidate_plan_cache(&svc).map_err(String::from)?;
    Ok(updated)
}

#[tauri::command]
pub fn trim_shape(
    svc: State<'_, Arc<ServiceContext>>,
    click_x: f64,
    click_y: f64,
    edge_threshold_mm: f64,
    heal: Option<bool>,
) -> Result<vector::TrimShapeResult, String> {
    vector::trim_at_intersection(
        &svc,
        click_x,
        click_y,
        edge_threshold_mm,
        heal.unwrap_or(true),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn preview_trim_segment(
    svc: State<'_, Arc<ServiceContext>>,
    click_x: f64,
    click_y: f64,
    edge_threshold_mm: f64,
) -> Result<Option<vector::TrimPreview>, String> {
    vector::preview_trim(&svc, click_x, click_y, edge_threshold_mm).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn close_and_join(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    tolerance: Option<f64>,
) -> Result<CloseAndJoinResult, String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let (object, fully_closed) = vector::close_and_join(&svc, parsed_ids, tolerance.unwrap_or(0.1))
        .map_err(|e| e.to_string())?;
    Ok(CloseAndJoinResult {
        object,
        fully_closed,
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseAndJoinResult {
    pub object: ProjectObject,
    pub fully_closed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CutShapesApplyResult {
    pub created_object_ids: Vec<String>,
    pub inside_group_id: Option<String>,
    pub outside_group_id: Option<String>,
    pub cutter_object_id: String,
}

fn ensure_tool_layer(project: &mut Project) -> LayerRef {
    if let Some(layer) = project.layers.iter().find(|layer| layer.is_tool_layer) {
        return layer.id;
    }

    let mut layer = Layer::new("Tool", OperationType::Tool);
    layer.canonicalize_tool_layer();
    project.add_layer(layer).id
}

fn copy_display_fields(source: &ProjectObject, target: &mut ProjectObject) {
    target.visible = source.visible;
    target.locked = source.locked;
    target.transform = source.transform;
    target.z_index = source.z_index;
    target.lock_aspect_ratio = source.lock_aspect_ratio;
    target.power_scale = source.power_scale;
    target.priority = source.priority;
    target.tabs = source.tabs.clone();
    target.start_point_edits = source.start_point_edits.clone();
}

fn bounds_for_object_ids(project: &Project, ids: &[ObjectId]) -> Option<Bounds> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for id in ids {
        let object = project.find_object(*id)?;
        min_x = min_x.min(object.bounds.min.x);
        min_y = min_y.min(object.bounds.min.y);
        max_x = max_x.max(object.bounds.max.x);
        max_y = max_y.max(object.bounds.max.y);
    }
    if min_x.is_finite() && min_y.is_finite() && max_x.is_finite() && max_y.is_finite() {
        Some(Bounds::new(
            Point2D::new(min_x, min_y),
            Point2D::new(max_x, max_y),
        ))
    } else {
        None
    }
}

fn add_cut_shapes_group(
    project: &mut Project,
    name: &str,
    child_ids: &[ObjectId],
) -> Result<Option<ObjectId>, String> {
    if child_ids.is_empty() {
        return Ok(None);
    }
    let layer_id = project
        .find_object(child_ids[0])
        .ok_or("Cut Shapes result object not found")?
        .layer_id;
    let bounds =
        bounds_for_object_ids(project, child_ids).ok_or("Cut Shapes group has no bounds")?;
    let group = ProjectObject::new(
        name,
        layer_id,
        bounds,
        ObjectData::Group {
            children: child_ids.to_vec(),
        },
    );
    Ok(Some(project.add_object(group).id))
}

fn add_cut_image_copy(
    project: &mut Project,
    source: &ProjectObject,
    name_suffix: &str,
    mask_id: ObjectId,
    polarity: ImageMaskPolarity,
) -> Result<ProjectObject, String> {
    let ObjectData::RasterImage {
        asset_key,
        original_width_px,
        original_height_px,
        adjustments,
        ..
    } = &source.data
    else {
        return Err("Cut Shapes image source is not a raster image".to_string());
    };
    let mut object = ProjectObject::new(
        format!("{} {name_suffix}", source.name),
        source.layer_id,
        source.bounds,
        ObjectData::RasterImage {
            asset_key: asset_key.clone(),
            original_width_px: *original_width_px,
            original_height_px: *original_height_px,
            adjustments: adjustments.clone(),
            masks: vec![ImageMaskRef {
                object_id: mask_id,
                polarity,
            }],
        },
    );
    copy_display_fields(source, &mut object);
    Ok(project.add_object(object).clone())
}

fn add_cut_tool_mask_copy(
    project: &mut Project,
    cutter_path: &VecPath,
    name: &str,
) -> ProjectObject {
    let tool_layer_id = ensure_tool_layer(project);
    let object = vecpath_to_project_object(name.to_string(), tool_layer_id, cutter_path.clone());
    project.add_object(object).clone()
}

#[tauri::command]
pub fn cut_shapes_apply(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<CutShapesApplyResult, String> {
    if object_ids.len() < 2 {
        return Err("Cut Shapes requires at least two selected objects".to_string());
    }
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    cut_shapes_apply_inner(&svc, parsed_ids)
}

fn cut_shapes_apply_inner(
    svc: &ServiceContext,
    parsed_ids: Vec<ObjectId>,
) -> Result<CutShapesApplyResult, String> {
    let cutter_id = *parsed_ids
        .last()
        .ok_or("Cut Shapes requires a cutter object")?;

    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    for id in &parsed_ids {
        project.ensure_resolved(*id).map_err(|e| e.to_string())?;
    }
    let cutter = project
        .find_object(cutter_id)
        .ok_or("Cutter object not found")?
        .clone();
    if matches!(cutter.data, ObjectData::Group { .. }) {
        return Err("Cut Shapes cutter must be an ungrouped closed shape".to_string());
    }
    let cutter_path = object_to_world_vecpath_resolved(&cutter, project)
        .ok_or("Cut Shapes cutter must be a vector-compatible object")?;
    let cutter_is_closed = cutter_path
        .subpaths
        .iter()
        .any(|sp| sp.closed && sp.commands.len() >= 4);
    if !cutter_is_closed {
        return Err("Cut Shapes cutter must be a closed vector shape".to_string());
    }

    let subjects = parsed_ids[..parsed_ids.len() - 1]
        .iter()
        .map(|id| {
            project
                .find_object(*id)
                .cloned()
                .ok_or_else(|| "Selected object not found".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Validate every subject (and precompute vector subject paths) BEFORE
    // pushing the undo snapshot or mutating the project. A mid-loop
    // rejection used to leave orphaned mask/image copies in the project and
    // a phantom no-op undo entry. `None` marks a raster subject.
    let mut subject_paths: Vec<Option<VecPath>> = Vec::with_capacity(subjects.len());
    for subject in &subjects {
        if subject.locked {
            return Err(format!("{} is locked", subject.name));
        }
        match &subject.data {
            ObjectData::RasterImage { .. } => subject_paths.push(None),
            ObjectData::Group { .. } => {
                return Err("Cut Shapes subjects must be ungrouped objects".to_string());
            }
            _ => {
                let subject_path = object_to_world_vecpath_resolved(subject, project)
                    .ok_or_else(|| format!("{} is not vector-compatible", subject.name))?;
                subject_paths.push(Some(subject_path));
            }
        }
    }

    svc.push_project_undo_snapshot(project)
        .map_err(|e| e.to_string())?;

    let mut created_ids = Vec::new();
    let mut inside_ids = Vec::new();
    let mut outside_ids = Vec::new();
    for (subject, subject_path) in subjects.iter().zip(subject_paths) {
        match subject_path {
            None => {
                let inside_mask = add_cut_tool_mask_copy(
                    project,
                    &cutter_path,
                    &format!("{} Cutter Inside", subject.name),
                );
                let outside_mask = add_cut_tool_mask_copy(
                    project,
                    &cutter_path,
                    &format!("{} Cutter Outside", subject.name),
                );
                created_ids.push(inside_mask.id);
                created_ids.push(outside_mask.id);
                let inside = add_cut_image_copy(
                    project,
                    subject,
                    "Inside",
                    inside_mask.id,
                    ImageMaskPolarity::KeepInside,
                )?;
                let outside = add_cut_image_copy(
                    project,
                    subject,
                    "Outside",
                    outside_mask.id,
                    ImageMaskPolarity::KeepOutside,
                )?;
                inside_ids.push(inside.id);
                outside_ids.push(outside.id);
                created_ids.push(inside.id);
                created_ids.push(outside.id);
            }
            Some(subject_path) => {
                let inside =
                    beambench_core::vector::boolean::path_intersection(&subject_path, &cutter_path);
                if !inside.is_empty() {
                    let object = add_routed_vecpath_result(
                        project,
                        format!("{} Inside", subject.name),
                        subject.layer_id,
                        inside,
                    )?;
                    inside_ids.push(object.id);
                    created_ids.push(object.id);
                }
                let outside =
                    beambench_core::vector::boolean::path_subtract(&subject_path, &cutter_path);
                if !outside.is_empty() {
                    let object = add_routed_vecpath_result(
                        project,
                        format!("{} Outside", subject.name),
                        subject.layer_id,
                        outside,
                    )?;
                    outside_ids.push(object.id);
                    created_ids.push(object.id);
                }
            }
        }
    }

    let inside_group_id = add_cut_shapes_group(project, "Cut Shapes Inside", &inside_ids)?;
    let outside_group_id = add_cut_shapes_group(project, "Cut Shapes Outside", &outside_ids)?;
    if let Some(id) = inside_group_id {
        created_ids.push(id);
    }
    if let Some(id) = outside_group_id {
        created_ids.push(id);
    }

    project.remove_objects(&parsed_ids);
    project.dirty = true;
    let result = CutShapesApplyResult {
        created_object_ids: created_ids.iter().map(ToString::to_string).collect(),
        inside_group_id: inside_group_id.map(|id| id.to_string()),
        outside_group_id: outside_group_id.map(|id| id.to_string()),
        cutter_object_id: cutter_id.to_string(),
    };
    drop(guard);
    planning::invalidate_plan_cache(svc).map_err(String::from)?;
    Ok(result)
}

#[tauri::command]
pub fn cut_shapes(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<Vec<String>, String> {
    let parsed_ids: Vec<ObjectId> = object_ids
        .iter()
        .map(|s| parse_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let mut vec_paths = Vec::new();
    for id in &parsed_ids {
        let (_, vp) = get_object_vecpath(&svc, *id)?;
        vec_paths.push(vp);
    }
    let results = beambench_core::vector::boolean::cut_shapes(&vec_paths);
    Ok(results.iter().map(|p| p.to_svg_d()).collect())
}

#[tauri::command]
pub fn generate_barcode(
    _svc: State<'_, Arc<ServiceContext>>,
    barcode_type: BarcodeType,
    data: String,
    width: f64,
    height: f64,
    options: Option<BarcodeOptions>,
) -> Result<String, String> {
    let options = options.unwrap_or_default();
    let result = beambench_core::barcode_gen::generate_barcode_with_options(
        barcode_type,
        &data,
        width,
        height,
        &options,
    )?;
    Ok(result.to_svg_d())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TabMarkerDto {
    pub subpath_index: usize,
    pub position: f64,
    pub world_x: f64,
    pub world_y: f64,
}

#[tauri::command]
pub fn place_tab(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    world_x: f64,
    world_y: f64,
) -> Result<ProjectObject, String> {
    place_tab_inner(&svc, parse_id(&object_id)?, world_x, world_y)
}

fn place_tab_inner(
    svc: &ServiceContext,
    oid: ObjectId,
    world_x: f64,
    world_y: f64,
) -> Result<ProjectObject, String> {
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    let source = project.find_object(oid).ok_or("Object not found")?.clone();
    let vp = object_to_world_vecpath_resolved(&source, project)
        .ok_or_else(|| "Object is not a vector type".to_string())?;
    let click = Point2D::new(world_x, world_y);
    let (anchor, dist) = beambench_core::vector::tabs::project_point_to_tab_anchor(&vp, click)
        .ok_or("No closed subpath found")?;
    // Reject clicks that are too far from the path edge (5mm max)
    if dist > 5.0 {
        return Err("Click is too far from the path edge".to_string());
    }

    // Reject duplicate or near-duplicate anchors (within 1% of perimeter on same subpath)
    let dup_tolerance = 0.01;
    let has_duplicate = source.tabs.iter().any(|existing| {
        existing.subpath_index == anchor.subpath_index && {
            let diff = (existing.position - anchor.position).abs();
            let circular_dist = diff.min(1.0 - diff);
            circular_dist < dup_tolerance
        }
    });
    if has_duplicate {
        return Err("A tab already exists at this position".to_string());
    }

    svc.push_project_undo_snapshot(project)
        .map_err(|e| e.to_string())?;
    project.ensure_resolved(oid).map_err(|e| e.to_string())?;

    let obj = project.find_object_mut(oid).ok_or("Object not found")?;
    obj.tabs.push(anchor);
    obj.tabs.sort_by(|a, b| {
        a.subpath_index
            .cmp(&b.subpath_index)
            .then(a.position.partial_cmp(&b.position).unwrap())
    });
    let result = obj.clone();
    project.dirty = true;
    drop(guard);
    planning::invalidate_plan_cache(&svc).map_err(String::from)?;
    Ok(result)
}

#[tauri::command]
pub fn remove_tab(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    world_x: f64,
    world_y: f64,
) -> Result<ProjectObject, String> {
    remove_tab_inner(&svc, parse_id(&object_id)?, world_x, world_y)
}

fn remove_tab_inner(
    svc: &ServiceContext,
    oid: ObjectId,
    world_x: f64,
    world_y: f64,
) -> Result<ProjectObject, String> {
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    let source = project.find_object(oid).ok_or("Object not found")?.clone();
    if source.tabs.is_empty() {
        return Ok(source);
    }
    let vp = object_to_world_vecpath_resolved(&source, project)
        .ok_or_else(|| "Object is not a vector type".to_string())?;

    // Resolve all tab positions to world space, then find closest to click
    let resolved = beambench_core::vector::tabs::resolve_tab_positions(&vp, &source.tabs);
    if resolved.is_empty() {
        return Ok(source);
    }
    let click = Point2D::new(world_x, world_y);
    let mut best_resolved_idx = 0;
    let mut best_dist = f64::INFINITY;
    for (i, marker) in resolved.iter().enumerate() {
        let d = click.distance_to(&Point2D::new(marker.world_x, marker.world_y));
        if d < best_dist {
            best_dist = d;
            best_resolved_idx = i;
        }
    }
    // Reject clicks that are too far from any existing marker (5mm max)
    if best_dist > 5.0 {
        return Err("Click is too far from any tab marker".to_string());
    }
    // Use the original tabs index, not the filtered resolved-list index
    let original_tab_idx = resolved[best_resolved_idx].original_index;

    svc.push_project_undo_snapshot(project)
        .map_err(|e| e.to_string())?;
    project.ensure_resolved(oid).map_err(|e| e.to_string())?;

    let obj = project.find_object_mut(oid).ok_or("Object not found")?;
    if original_tab_idx < obj.tabs.len() {
        obj.tabs.remove(original_tab_idx);
    }
    let result = obj.clone();
    project.dirty = true;
    drop(guard);
    planning::invalidate_plan_cache(&svc).map_err(String::from)?;
    Ok(result)
}

#[tauri::command]
pub fn resolve_tab_markers(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<Vec<TabMarkerDto>, String> {
    let oid: ObjectId = parse_id(&object_id)?;
    let guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_ref().ok_or("No project open")?;
    let obj = project.find_object(oid).ok_or("Object not found")?;
    if obj.tabs.is_empty() {
        return Ok(Vec::new());
    }
    // Resolve VirtualClone to its source geometry (transient, no mutation)
    let resolved = project.resolve_clone(obj);
    let effective = resolved.as_ref().unwrap_or(obj);
    let vp = to_world_vecpath(effective)?;
    let resolved = beambench_core::vector::tabs::resolve_tab_positions(&vp, &obj.tabs);
    Ok(resolved
        .into_iter()
        .map(|m| TabMarkerDto {
            subpath_index: m.subpath_index,
            position: m.position,
            world_x: m.world_x,
            world_y: m.world_y,
        })
        .collect())
}

#[tauri::command]
pub fn unlink_virtual_clone(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<ProjectObject, String> {
    let oid: ObjectId = parse_id(&object_id)?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    svc.push_project_undo_snapshot(project)
        .map_err(|e| e.to_string())?;
    project
        .resolve_clone_in_place(oid)
        .map_err(|e| e.to_string())?;
    let resolved = project
        .find_object(oid)
        .ok_or("Object not found after unlink")?
        .clone();
    project.dirty = true;
    drop(guard);
    planning::invalidate_plan_cache(&svc).map_err(String::from)?;
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_core::object::ShapeKind;

    /// Helper: create a project with a default layer and return it + the layer id.
    fn test_project() -> (Project, LayerRef) {
        let mut project = Project::new("test");
        let layer_id = project.ensure_default_layer();
        (project, layer_id)
    }

    /// Helper: add a rectangle shape object at given position/size.
    fn add_rect(
        project: &mut Project,
        layer_id: LayerRef,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    ) -> ObjectId {
        let obj = ProjectObject::new(
            "rect",
            layer_id,
            Bounds::new(Point2D::new(x, y), Point2D::new(x + w, y + h)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: w,
                height: h,
                corner_radius: 0.0,
            },
        );
        let id = obj.id;
        project.add_object(obj);
        id
    }

    fn add_line(
        project: &mut Project,
        layer_id: LayerRef,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
    ) -> ObjectId {
        let min = Point2D::new(x0.min(x1), y0.min(y1));
        let max = Point2D::new(x0.max(x1), y0.max(y1));
        let obj = ProjectObject::new(
            "line",
            layer_id,
            Bounds::new(min, max),
            ObjectData::VectorPath {
                path_data: format!("M {x0} {y0} L {x1} {y1}"),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let id = obj.id;
        project.add_object(obj);
        id
    }

    #[test]
    fn apply_corner_radius_converts_rectangle_to_path_and_rounds_only_selected_corner() {
        let (mut project, layer_id) = test_project();
        let rect_id = add_rect(&mut project, layer_id, 0.0, 0.0, 40.0, 20.0);
        let svc = svc_with_project(project);

        let updated = apply_corner_radius_inner(&svc, rect_id, 0, 0, 12.7).unwrap();
        match &updated.data {
            ObjectData::VectorPath { .. } => {}
            other => panic!(
                "Expected rectangle to convert to vector path after first fillet, got {other:?}"
            ),
        }

        let guard = svc.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        let stored = project.find_object(rect_id).unwrap();
        assert!(matches!(stored.data, ObjectData::VectorPath { .. }));
        let vp = to_world_vecpath(stored).unwrap();
        let candidates = path_ops_core::get_fillet_candidates(&vp);
        assert_eq!(candidates.len(), 4);
        assert_eq!(
            candidates
                .iter()
                .filter(|candidate| candidate.already_filleted)
                .count(),
            1
        );
    }

    #[test]
    fn apply_corner_radius_can_unfillet_single_rectangle_corner() {
        let (mut project, layer_id) = test_project();
        let rect_id = add_rect(&mut project, layer_id, 0.0, 0.0, 40.0, 20.0);
        let svc = svc_with_project(project);

        let _ = apply_corner_radius_inner(&svc, rect_id, 0, 0, 6.35).unwrap();
        let filleted = {
            let guard = svc.project.lock().unwrap();
            let project = guard.as_ref().unwrap();
            project.find_object(rect_id).unwrap().clone()
        };
        let filleted_path = to_world_vecpath(&filleted).unwrap();
        let filleted_corner = path_ops_core::get_fillet_candidates(&filleted_path)
            .into_iter()
            .find(|candidate| candidate.already_filleted)
            .expect("expected one filleted rectangle corner");

        let updated = apply_corner_radius_inner(
            &svc,
            rect_id,
            filleted_corner.subpath_index,
            filleted_corner.vertex_index,
            0.0,
        )
        .unwrap();

        match &updated.data {
            ObjectData::VectorPath { .. } => {
                let vp = to_world_vecpath(&updated).unwrap();
                let candidates = path_ops_core::get_fillet_candidates(&vp);
                assert_eq!(
                    candidates
                        .iter()
                        .filter(|candidate| candidate.already_filleted)
                        .count(),
                    0
                );
            }
            other => panic!("Expected rectangle vector path after unfillet, got {other:?}"),
        }
    }

    #[test]
    fn assign_image_mask_refs_dedupes_and_preserves_existing_polarity() {
        let (mut project, layer_id) = test_project();
        let image_id = add_raster(&mut project, layer_id);
        let mask_a = add_rect(&mut project, layer_id, 0.0, 0.0, 10.0, 10.0);
        let mask_b = add_rect(&mut project, layer_id, 20.0, 0.0, 10.0, 10.0);

        assign_image_mask_refs(
            &mut project,
            image_id,
            vec![mask_a],
            ImageMaskPolarity::KeepOutside,
        )
        .unwrap();
        let updated = assign_image_mask_refs(
            &mut project,
            image_id,
            vec![mask_a, mask_a, mask_b],
            ImageMaskPolarity::KeepInside,
        )
        .unwrap();

        let ObjectData::RasterImage { masks, .. } = updated.data else {
            panic!("Expected raster image");
        };
        assert_eq!(masks.len(), 2);
        assert_eq!(
            masks
                .iter()
                .find(|mask| mask.object_id == mask_a)
                .unwrap()
                .polarity,
            ImageMaskPolarity::KeepOutside
        );
        assert_eq!(
            masks
                .iter()
                .find(|mask| mask.object_id == mask_b)
                .unwrap()
                .polarity,
            ImageMaskPolarity::KeepInside
        );
    }

    #[test]
    fn set_image_mask_polarity_targets_one_ref_and_remove_can_clear_one_or_all() {
        let (mut project, layer_id) = test_project();
        let image_id = add_raster(&mut project, layer_id);
        let mask_a = add_rect(&mut project, layer_id, 0.0, 0.0, 10.0, 10.0);
        let mask_b = add_rect(&mut project, layer_id, 20.0, 0.0, 10.0, 10.0);

        assign_image_mask_refs(
            &mut project,
            image_id,
            vec![mask_a, mask_b],
            ImageMaskPolarity::KeepInside,
        )
        .unwrap();
        let updated = update_image_mask_polarity(
            &mut project,
            image_id,
            mask_b,
            ImageMaskPolarity::KeepOutside,
        )
        .unwrap();
        let ObjectData::RasterImage { masks, .. } = updated.data else {
            panic!("Expected raster image");
        };
        assert_eq!(
            masks
                .iter()
                .find(|mask| mask.object_id == mask_a)
                .unwrap()
                .polarity,
            ImageMaskPolarity::KeepInside
        );
        assert_eq!(
            masks
                .iter()
                .find(|mask| mask.object_id == mask_b)
                .unwrap()
                .polarity,
            ImageMaskPolarity::KeepOutside
        );

        let updated = remove_image_mask_refs(&mut project, image_id, Some(mask_a)).unwrap();
        let ObjectData::RasterImage { masks, .. } = updated.data else {
            panic!("Expected raster image");
        };
        assert_eq!(masks.len(), 1);
        assert_eq!(masks[0].object_id, mask_b);

        let updated = remove_image_mask_refs(&mut project, image_id, None).unwrap();
        let ObjectData::RasterImage { masks, .. } = updated.data else {
            panic!("Expected raster image");
        };
        assert!(masks.is_empty());
    }

    /// Helper: add a raster image object.
    fn add_raster(project: &mut Project, layer_id: LayerRef) -> ObjectId {
        let obj = ProjectObject::new(
            "raster",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(50.0, 50.0)),
            ObjectData::RasterImage {
                asset_key: "test_asset".to_string(),
                original_width_px: 100,
                original_height_px: 100,
                adjustments: None,
                masks: Vec::new(),
            },
        );
        let id = obj.id;
        project.add_object(obj);
        id
    }

    /// Helper: run grid_array command logic (excluding Tauri State wrapper).
    fn run_grid_array(
        svc: &ServiceContext,
        object_ids: Vec<String>,
        rows: u32,
        cols: u32,
        sizing_mode_x: GridArraySizingMode,
        sizing_mode_y: GridArraySizingMode,
        total_width_mm: Option<f64>,
        total_height_mm: Option<f64>,
        h_spacing_mm: f64,
        v_spacing_mm: f64,
    ) -> Result<ArrayResult, String> {
        let parsed_ids: Vec<ObjectId> = object_ids
            .iter()
            .map(|id| parse_id(id))
            .collect::<Result<Vec<_>, _>>()?;
        let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
        let project = guard.as_mut().ok_or("No project open")?;
        let sources: Vec<ProjectObject> = parsed_ids
            .iter()
            .map(|id| project.find_object(*id).cloned().ok_or("Object not found"))
            .collect::<Result<Vec<_>, _>>()?;

        let mut config = GridArrayConfig {
            rows,
            cols,
            sizing_mode_x,
            sizing_mode_y,
            total_width_mm,
            total_height_mm,
            h_spacing_mm,
            v_spacing_mm,
            spacing_mode: SpacingMode::CenterToCenter,
            mirror_alternate_cols: false,
            mirror_alternate_rows: false,
            x_col_shift_mm: 0.0,
            y_row_shift_mm: 0.0,
            half_shift: false,
            reverse_h: false,
            reverse_v: false,
            random_orientation: false,
            random_seed: 0,
            group_results: false,
            create_virtual: false,
            auto_increment_text: false,
            text_increment: 1,
        };
        let (fitted_rows, fitted_cols) =
            beambench_core::fit_grid_array_counts(&sources, &config, 100);
        config.rows = fitted_rows;
        config.cols = fitted_cols;

        let transformed_roots = beambench_core::grid_array_in_project(project, &sources, &config);
        svc.push_project_undo_snapshot(project)
            .map_err(|e| e.to_string())?;
        let (copies, root_ids) =
            materialize_array_copies(project, &sources, transformed_roots, config.create_virtual)?;
        let mut added_root_ids = HashSet::new();
        let mut created_ids = Vec::with_capacity(root_ids.len());
        for copy in copies {
            let added = project.add_object(copy);
            if root_ids.contains(&added.id) && added_root_ids.insert(added.id) {
                created_ids.push(added.id.to_string());
            }
        }
        drop(guard);
        planning::invalidate_plan_cache(svc).map_err(String::from)?;
        Ok(ArrayResult {
            created_ids,
            group_id: None,
        })
    }

    /// Helper: run circular_array command logic (excluding Tauri State wrapper).
    fn run_circular_array(
        svc: &ServiceContext,
        object_ids: Vec<String>,
        count: u32,
        radius_mm: f64,
        center_object_id: Option<String>,
        create_virtual: bool,
    ) -> Result<ArrayResult, String> {
        let parsed_ids: Vec<ObjectId> = object_ids
            .iter()
            .map(|id| parse_id(id))
            .collect::<Result<Vec<_>, _>>()?;
        let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
        let project = guard.as_mut().ok_or("No project open")?;

        let center_oid = center_object_id
            .as_ref()
            .map(|id| parse_id(id))
            .transpose()?;
        let (explicit_cx, explicit_cy) = if let Some(coid) = center_oid {
            let cobj = project.find_object(coid).ok_or("Center object not found")?;
            let cx = (cobj.bounds.min.x + cobj.bounds.max.x) / 2.0;
            let cy = (cobj.bounds.min.y + cobj.bounds.max.y) / 2.0;
            (Some(cx), Some(cy))
        } else {
            (None, None)
        };

        let sources: Vec<ProjectObject> = parsed_ids
            .iter()
            .filter(|id| center_oid.is_none_or(|coid| **id != coid))
            .map(|id| project.find_object(*id).cloned().ok_or("Object not found"))
            .collect::<Result<Vec<_>, _>>()?;

        if sources.is_empty() {
            return Err("No source objects for circular array".into());
        }

        let config = beambench_core::array_ops::CircularArrayConfig {
            count,
            radius_mm,
            rotate_copies: true,
            center_x: explicit_cx,
            center_y: explicit_cy,
            start_angle_deg: 0.0,
            end_angle_deg: 360.0,
            group_results: false,
            create_virtual,
            auto_increment_text: false,
            text_increment: 1,
        };

        let all_positions = beambench_core::circular_array(&sources, &config);
        let src_count = sources.len();
        let (position_zero, new_copies) = if all_positions.len() > src_count {
            all_positions.split_at(src_count)
        } else {
            (all_positions.as_slice(), [].as_slice())
        };

        let mut position_zero_root_ids = Vec::with_capacity(position_zero.len());
        for (source, updated) in sources.iter().zip(position_zero.iter()) {
            position_zero_root_ids.push(apply_transformed_source_to_existing(
                project, source, updated,
            )?);
        }

        let (new_copies, new_root_ids) =
            materialize_array_copies(project, &sources, new_copies.to_vec(), create_virtual)?;

        let mut created_ids = Vec::with_capacity(src_count + new_copies.len());
        for root_id in position_zero_root_ids {
            created_ids.push(root_id.to_string());
        }
        let mut added_root_ids = HashSet::new();
        for copy in new_copies {
            let added = project.add_object(copy).clone();
            if new_root_ids.contains(&added.id) && added_root_ids.insert(added.id) {
                created_ids.push(added.id.to_string());
            }
        }

        project.dirty = true;
        Ok(ArrayResult {
            created_ids,
            group_id: None,
        })
    }

    /// Helper: add a group containing the given children.
    fn add_group(project: &mut Project, layer_id: LayerRef, children: Vec<ObjectId>) -> ObjectId {
        let obj = ProjectObject::new(
            "group",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 100.0)),
            ObjectData::Group { children },
        );
        let id = obj.id;
        project.add_object(obj);
        id
    }

    fn add_group_with_bounds(
        project: &mut Project,
        layer_id: LayerRef,
        bounds: Bounds,
        children: Vec<ObjectId>,
    ) -> ObjectId {
        let obj = ProjectObject::new("group", layer_id, bounds, ObjectData::Group { children });
        let id = obj.id;
        project.add_object(obj);
        id
    }

    // ── collect_vector_paths tests ─────────────────────────────

    #[test]
    fn collect_skips_non_vector_objects() {
        let (mut project, layer_id) = test_project();
        let rect_id = add_rect(&mut project, layer_id, 0.0, 0.0, 10.0, 10.0);
        let raster_id = add_raster(&mut project, layer_id);

        let mut seen = HashSet::new();
        let mut all_paths = Vec::new();
        let mut source_ids = Vec::new();

        collect_vector_paths(
            &project,
            rect_id,
            &mut seen,
            &mut all_paths,
            &mut source_ids,
        );
        collect_vector_paths(
            &project,
            raster_id,
            &mut seen,
            &mut all_paths,
            &mut source_ids,
        );

        // Only the rectangle should produce a path and be in source_ids
        assert_eq!(
            all_paths.len(),
            1,
            "Only vector objects should produce paths"
        );
        assert_eq!(
            source_ids.len(),
            1,
            "Non-vector objects must not be in source_ids"
        );
        assert_eq!(source_ids[0], rect_id);
    }

    #[test]
    fn collect_expands_group_into_welded_paths() {
        let (mut project, layer_id) = test_project();
        let r1 = add_rect(&mut project, layer_id, 0.0, 0.0, 10.0, 10.0);
        let r2 = add_rect(&mut project, layer_id, 5.0, 0.0, 10.0, 10.0);
        let group_id = add_group(&mut project, layer_id, vec![r1, r2]);

        let mut seen = HashSet::new();
        let mut all_paths = Vec::new();
        let mut source_ids = Vec::new();

        collect_vector_paths(
            &project,
            group_id,
            &mut seen,
            &mut all_paths,
            &mut source_ids,
        );

        // Both children should contribute paths
        assert_eq!(
            all_paths.len(),
            2,
            "Both group children should produce paths"
        );
        // source_ids should contain both children + the group itself
        assert_eq!(source_ids.len(), 3);
        assert!(source_ids.contains(&r1));
        assert!(source_ids.contains(&r2));
        assert!(source_ids.contains(&group_id));
    }

    #[test]
    fn collect_deduplicates_group_and_child() {
        let (mut project, layer_id) = test_project();
        let r1 = add_rect(&mut project, layer_id, 0.0, 0.0, 10.0, 10.0);
        let r2 = add_rect(&mut project, layer_id, 20.0, 0.0, 10.0, 10.0);
        let group_id = add_group(&mut project, layer_id, vec![r1, r2]);

        let mut seen = HashSet::new();
        let mut all_paths = Vec::new();
        let mut source_ids = Vec::new();

        // Select both the group AND one of its children — child should be deduped
        collect_vector_paths(
            &project,
            group_id,
            &mut seen,
            &mut all_paths,
            &mut source_ids,
        );
        collect_vector_paths(&project, r1, &mut seen, &mut all_paths, &mut source_ids);

        // r1 was already visited via the group — should not appear twice
        assert_eq!(all_paths.len(), 2, "Dedup should prevent duplicate paths");
        assert_eq!(
            source_ids.iter().filter(|&&id| id == r1).count(),
            1,
            "r1 should appear exactly once in source_ids"
        );
    }

    #[test]
    fn collect_mixed_group_preserves_non_vector_children() {
        let (mut project, layer_id) = test_project();
        let rect_id = add_rect(&mut project, layer_id, 0.0, 0.0, 10.0, 10.0);
        let raster_id = add_raster(&mut project, layer_id);
        let group_id = add_group(&mut project, layer_id, vec![rect_id, raster_id]);

        let mut seen = HashSet::new();
        let mut all_paths = Vec::new();
        let mut source_ids = Vec::new();

        collect_vector_paths(
            &project,
            group_id,
            &mut seen,
            &mut all_paths,
            &mut source_ids,
        );

        // Only the rect child contributes a path
        assert_eq!(all_paths.len(), 1);
        // source_ids should have rect + group, but NOT the raster
        assert!(
            source_ids.contains(&rect_id),
            "Vector child should be in source_ids"
        );
        assert!(
            source_ids.contains(&group_id),
            "Group with vector children should be in source_ids"
        );
        assert!(
            !source_ids.contains(&raster_id),
            "Raster child must NOT be in source_ids"
        );
    }

    #[test]
    fn collect_all_non_vector_group_not_in_source_ids() {
        let (mut project, layer_id) = test_project();
        let raster_id = add_raster(&mut project, layer_id);
        let group_id = add_group(&mut project, layer_id, vec![raster_id]);

        let mut seen = HashSet::new();
        let mut all_paths = Vec::new();
        let mut source_ids = Vec::new();

        collect_vector_paths(
            &project,
            group_id,
            &mut seen,
            &mut all_paths,
            &mut source_ids,
        );

        // No vector children → group should not be in source_ids either
        assert!(all_paths.is_empty());
        assert!(
            source_ids.is_empty(),
            "Group with no vector children must not be in source_ids"
        );
    }

    // ── command-level (offset_shapes_inner) tests ──────────────

    use beambench_core::vector::offset::{CornerStyle, OffsetDirection};

    /// Helper: create a ServiceContext with a project already loaded.
    fn svc_with_project(project: Project) -> ServiceContext {
        let svc = ServiceContext::new();
        *svc.project.lock().unwrap() = Some(project);
        svc
    }

    #[test]
    fn offset_group_produces_one_welded_result() {
        let (mut project, layer_id) = test_project();
        // Two overlapping rects in a group
        let r1 = add_rect(&mut project, layer_id, 0.0, 0.0, 10.0, 10.0);
        let r2 = add_rect(&mut project, layer_id, 5.0, 0.0, 10.0, 10.0);
        let group_id = add_group(&mut project, layer_id, vec![r1, r2]);
        let svc = svc_with_project(project);

        let result = offset_shapes_inner(
            &svc,
            &[group_id],
            2.0,
            OffsetDirection::Outward,
            CornerStyle::Miter,
            false,
        )
        .unwrap();

        // Welded overlapping rects → single offset result, not two
        assert_eq!(
            result.len(),
            1,
            "Group offset should produce one welded result"
        );
        let bounds = result[0].bounds;
        // Merged rect is 15×10, outward by 2 → ~19×14
        assert!(
            bounds.width() > 15.0,
            "Offset should be larger than merged source"
        );
    }

    #[test]
    fn offset_mixed_selection_uses_vector_metadata() {
        let (mut project, layer_id) = test_project();
        // Raster first, then a rect — name/layer should come from the rect
        let raster_id = add_raster(&mut project, layer_id);
        let rect_id = add_rect(&mut project, layer_id, 0.0, 0.0, 10.0, 10.0);

        // Give the rect a distinct name to verify metadata source
        project.find_object_mut(rect_id).unwrap().name = "MyRect".to_string();

        let svc = svc_with_project(project);

        let result = offset_shapes_inner(
            &svc,
            &[raster_id, rect_id],
            1.0,
            OffsetDirection::Outward,
            CornerStyle::Miter,
            false,
        )
        .unwrap();

        assert_eq!(result.len(), 1);
        // Name should come from the rect, not the raster
        assert!(
            result[0].name.contains("MyRect"),
            "Result name '{}' should derive from the vector object, not the raster",
            result[0].name
        );
        assert_eq!(result[0].layer_id, layer_id);
    }

    #[test]
    fn offset_open_two_point_line_creates_parallel_line_object() {
        let (mut project, layer_id) = test_project();
        let line_id = add_line(&mut project, layer_id, 0.0, 0.0, 10.0, 0.0);
        let svc = svc_with_project(project);

        let result = offset_shapes_inner(
            &svc,
            &[line_id],
            2.0,
            OffsetDirection::Outward,
            CornerStyle::Miter,
            false,
        )
        .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].bounds.min.x, 0.0);
        assert_eq!(result[0].bounds.max.x, 10.0);
        assert_eq!(result[0].bounds.min.y, -2.0);
        assert_eq!(result[0].bounds.max.y, -2.0);
        match &result[0].data {
            ObjectData::VectorPath {
                path_data, closed, ..
            } => {
                assert!(!closed);
                let path = VecPath::parse_svg_d(path_data);
                assert_eq!(path.subpaths.len(), 1);
                assert_eq!(path.subpaths[0].commands.len(), 2);
            }
            other => panic!("expected vector path offset result, got {other:?}"),
        }
    }

    #[test]
    fn preview_offset_open_line_is_non_mutating() {
        let (mut project, layer_id) = test_project();
        let line_id = add_line(&mut project, layer_id, 0.0, 0.0, 10.0, 0.0);
        let svc = svc_with_project(project);

        let before = svc.project.lock().unwrap().as_ref().unwrap().objects.len();

        let preview = preview_offset_shapes_inner(
            &svc,
            &[line_id],
            2.0,
            OffsetDirection::Outward,
            CornerStyle::Miter,
        )
        .unwrap();

        assert!(preview.source_all_open);
        assert!(!preview.paths.is_empty());
        let first = &preview.paths[0];
        assert!(first.points.len() >= 2);
        // Outward offset of a horizontal line lands a parallel line at y = -2.
        assert!(first.points.iter().all(|p| (p.y + 2.0).abs() < 1e-6));

        // Preview must not touch the project.
        let after = svc.project.lock().unwrap().as_ref().unwrap().objects.len();
        assert_eq!(before, after, "preview must not mutate the project");
    }

    #[test]
    fn preview_offset_closed_selection_skips_compute() {
        let (mut project, layer_id) = test_project();
        let rect_id = add_rect(&mut project, layer_id, 0.0, 0.0, 10.0, 10.0);
        let svc = svc_with_project(project);

        let preview = preview_offset_shapes_inner(
            &svc,
            &[rect_id],
            2.0,
            OffsetDirection::Outward,
            CornerStyle::Miter,
        )
        .unwrap();

        assert!(!preview.source_all_open);
        assert!(
            preview.paths.is_empty(),
            "closed selection must not produce a preview"
        );
    }

    #[test]
    fn preview_offset_mixed_selection_skips_compute() {
        let (mut project, layer_id) = test_project();
        let line_id = add_line(&mut project, layer_id, 0.0, 0.0, 10.0, 0.0);
        let rect_id = add_rect(&mut project, layer_id, 0.0, 0.0, 10.0, 10.0);
        let svc = svc_with_project(project);

        let preview = preview_offset_shapes_inner(
            &svc,
            &[line_id, rect_id],
            2.0,
            OffsetDirection::Outward,
            CornerStyle::Miter,
        )
        .unwrap();

        // A closed member disqualifies the whole selection from preview.
        assert!(!preview.source_all_open);
        assert!(preview.paths.is_empty());
    }

    #[test]
    fn preview_offset_grouped_line_is_open() {
        let (mut project, layer_id) = test_project();
        // A line nested in a group must still be detected as open, because the
        // backend collector expands groups (a shallow frontend check would not).
        let line_id = add_line(&mut project, layer_id, 0.0, 0.0, 10.0, 0.0);
        let group_id = add_group(&mut project, layer_id, vec![line_id]);
        let svc = svc_with_project(project);

        let preview = preview_offset_shapes_inner(
            &svc,
            &[group_id],
            2.0,
            OffsetDirection::Outward,
            CornerStyle::Miter,
        )
        .unwrap();

        assert!(preview.source_all_open);
        assert!(!preview.paths.is_empty());
    }

    #[test]
    fn boolean_intersection_reroutes_result_off_image_layer() {
        let mut project = Project::new("Bool Intersection Route");
        let mut image_layer =
            beambench_core::Layer::new("C10 (Image)", beambench_core::OperationType::Image);
        image_layer.color_tag = beambench_common::ColorTag("#ffaa00".into());
        let image_layer_id = image_layer.id;
        let mut line_layer =
            beambench_core::Layer::new("C10 (Line)", beambench_core::OperationType::Line);
        line_layer.color_tag = image_layer.color_tag.clone();
        let line_layer_id = line_layer.id;
        project.add_layer(image_layer);
        project.add_layer(line_layer);
        let id_a = add_rect(&mut project, image_layer_id, 0.0, 0.0, 20.0, 20.0);
        let id_b = add_rect(&mut project, image_layer_id, 10.0, 0.0, 20.0, 20.0);
        let svc = svc_with_project(project);

        let result = boolean_intersection_inner(&svc, id_a, id_b).unwrap();

        assert_eq!(result.layer_id, line_layer_id);
    }

    #[test]
    fn boolean_weld_reroutes_result_off_image_layer() {
        let mut project = Project::new("Bool Weld Route");
        let mut image_layer =
            beambench_core::Layer::new("C11 (Image)", beambench_core::OperationType::Image);
        image_layer.color_tag = beambench_common::ColorTag("#aaff00".into());
        let image_layer_id = image_layer.id;
        let mut line_layer =
            beambench_core::Layer::new("C11 (Line)", beambench_core::OperationType::Line);
        line_layer.color_tag = image_layer.color_tag.clone();
        let line_layer_id = line_layer.id;
        project.add_layer(image_layer);
        project.add_layer(line_layer);
        let id_a = add_rect(&mut project, image_layer_id, 0.0, 0.0, 20.0, 20.0);
        let id_b = add_rect(&mut project, image_layer_id, 10.0, 0.0, 20.0, 20.0);
        let svc = svc_with_project(project);

        let result = boolean_weld_inner(&svc, vec![id_a, id_b]).unwrap();

        assert_eq!(result.layer_id, line_layer_id);
    }

    #[test]
    fn boolean_assistant_preview_dispatches_exclude() {
        let a = VecPath::parse_svg_d("M0 0 L20 0 L20 20 L0 20 Z");
        let b = VecPath::parse_svg_d("M10 0 L30 0 L30 20 L10 20 Z");

        let (name, result) = boolean_assistant_result("exclude", &[a, b]).unwrap();

        assert_eq!(name, "Exclude");
        assert!(
            !result.subpaths.is_empty(),
            "exclude preview dispatch should produce XOR geometry"
        );
    }

    #[test]
    fn boolean_assistant_preview_flattens_closed_group_operand() {
        let (mut project, layer_id) = test_project();
        let id_a = add_rect(&mut project, layer_id, 0.0, 0.0, 10.0, 10.0);
        let id_b = add_rect(&mut project, layer_id, 20.0, 0.0, 10.0, 10.0);
        let group = ProjectObject::new(
            "group",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(30.0, 10.0)),
            ObjectData::Group {
                children: vec![id_a, id_b],
            },
        );
        let group_id = group.id;
        project.add_object(group);

        let path = vector::boolean_operand_path_for_preview(&project, group_id).unwrap();

        assert_eq!(path.subpaths.len(), 2);
        assert!(path.subpaths.iter().all(|sp| sp.closed));
    }

    fn group_children_for_test(project: &Project, group_id: ObjectId) -> Vec<ObjectId> {
        let group = project.find_object(group_id).unwrap();
        match &group.data {
            ObjectData::Group { children } => children.clone(),
            other => panic!("expected group, got {other:?}"),
        }
    }

    fn combined_vector_path_for_test(project: &Project, object_ids: &[ObjectId]) -> VecPath {
        let mut combined = VecPath { subpaths: vec![] };
        for object_id in object_ids {
            let object = project.find_object(*object_id).unwrap();
            match &object.data {
                ObjectData::VectorPath { path_data, .. } => {
                    combined
                        .subpaths
                        .extend(VecPath::parse_svg_d(path_data).subpaths);
                }
                other => panic!("expected vector path result, got {other:?}"),
            }
        }
        combined
    }

    fn point_is_filled_evenodd_for_test(path: &VecPath, x: f64, y: f64) -> bool {
        let mut filled = false;
        for polyline in beambench_core::vector::flatten::flatten_vecpath(
            path,
            beambench_core::vector::flatten::DEFAULT_TOLERANCE_MM,
        )
        .into_iter()
        .filter(|polyline| polyline.points.len() >= 3)
        {
            let mut inside = false;
            let points = &polyline.points;
            let mut j = points.len() - 1;
            for i in 0..points.len() {
                let pi = &points[i];
                let pj = &points[j];
                if (pi.y > y) != (pj.y > y) {
                    let x_at_y = (pj.x - pi.x) * (y - pi.y) / (pj.y - pi.y) + pi.x;
                    if x < x_at_y {
                        inside = !inside;
                    }
                }
                j = i;
            }
            if inside {
                filled = !filled;
            }
        }
        filled
    }

    #[test]
    fn cut_shapes_apply_preserves_donut_subject_and_cutter_holes() {
        let (mut project, layer_id) = test_project();
        let subject_id = add_vecpath(
            &mut project,
            layer_id,
            "Donut Subject",
            "M0 0 L40 0 L40 40 L0 40 Z M12 12 L28 12 L28 28 L12 28 Z",
        );
        let cutter_id = add_vecpath(
            &mut project,
            layer_id,
            "Donut Cutter",
            "M5 5 L35 5 L35 35 L5 35 Z M16 16 L24 16 L24 24 L16 24 Z",
        );
        let svc = svc_with_project(project);

        let result = cut_shapes_apply_inner(&svc, vec![subject_id, cutter_id]).unwrap();

        let inside_group_id = parse_id(
            result
                .inside_group_id
                .as_ref()
                .expect("expected inside group"),
        )
        .unwrap();
        let outside_group_id = parse_id(
            result
                .outside_group_id
                .as_ref()
                .expect("expected outside group"),
        )
        .unwrap();
        let guard = svc.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        assert!(project.find_object(subject_id).is_none());
        assert!(project.find_object(cutter_id).is_none());

        let inside_children = group_children_for_test(project, inside_group_id);
        let outside_children = group_children_for_test(project, outside_group_id);
        assert!(!inside_children.is_empty());
        assert!(!outside_children.is_empty());
        let inside_path = combined_vector_path_for_test(project, &inside_children);
        let outside_path = combined_vector_path_for_test(project, &outside_children);

        assert!(point_is_filled_evenodd_for_test(&inside_path, 8.0, 8.0));
        assert!(!point_is_filled_evenodd_for_test(&inside_path, 20.0, 20.0));
        assert!(point_is_filled_evenodd_for_test(&outside_path, 2.0, 2.0));
        assert!(!point_is_filled_evenodd_for_test(&outside_path, 20.0, 20.0));
    }

    #[test]
    fn cut_shapes_apply_locked_second_subject_leaves_project_and_undo_untouched() {
        let (mut project, layer_id) = test_project();
        let subject_a = add_vecpath(
            &mut project,
            layer_id,
            "Subject A",
            "M0 0 L10 0 L10 10 L0 10 Z",
        );
        let subject_b = add_vecpath(
            &mut project,
            layer_id,
            "Subject B",
            "M20 0 L30 0 L30 10 L20 10 Z",
        );
        project.find_object_mut(subject_b).unwrap().locked = true;
        let cutter_id = add_vecpath(
            &mut project,
            layer_id,
            "Cutter",
            "M5 -5 L25 -5 L25 5 L5 5 Z",
        );
        let initial_count = project.objects.len();
        let svc = svc_with_project(project);

        let result = cut_shapes_apply_inner(&svc, vec![subject_a, subject_b, cutter_id]);

        assert_eq!(result.unwrap_err(), "Subject B is locked");
        let guard = svc.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        // No orphaned mask/image/result copies: object count unchanged.
        assert_eq!(project.objects.len(), initial_count);
        assert!(project.find_object(subject_a).is_some());
        assert!(project.find_object(subject_b).is_some());
        assert!(project.find_object(cutter_id).is_some());
        drop(guard);
        // No phantom undo entry.
        assert!(!svc.undo_state().unwrap().can_undo);
    }

    #[test]
    fn cut_shapes_apply_locked_first_subject_skips_phantom_undo_entry() {
        let (mut project, layer_id) = test_project();
        let subject_id = add_vecpath(
            &mut project,
            layer_id,
            "Subject A",
            "M0 0 L10 0 L10 10 L0 10 Z",
        );
        project.find_object_mut(subject_id).unwrap().locked = true;
        let cutter_id = add_vecpath(
            &mut project,
            layer_id,
            "Cutter",
            "M5 -5 L25 -5 L25 5 L5 5 Z",
        );
        let initial_count = project.objects.len();
        let svc = svc_with_project(project);

        let result = cut_shapes_apply_inner(&svc, vec![subject_id, cutter_id]);

        assert_eq!(result.unwrap_err(), "Subject A is locked");
        assert_eq!(
            svc.project.lock().unwrap().as_ref().unwrap().objects.len(),
            initial_count
        );
        assert!(!svc.undo_state().unwrap().can_undo);
    }

    #[test]
    fn copy_along_path_batch_undo_reverts_all_created_copies_in_one_step() {
        let (mut project, layer_id) = test_project();
        let source_a = add_rect(&mut project, layer_id, 0.0, 0.0, 5.0, 5.0);
        let source_b = add_rect(&mut project, layer_id, 10.0, 0.0, 5.0, 5.0);
        let guide = add_rect(&mut project, layer_id, 0.0, 20.0, 100.0, 5.0);
        let initial_count = project.objects.len();
        let svc = svc_with_project(project);

        let created =
            copy_along_path_batch_inner(&svc, &[source_a, source_b], guide, 4, true, false, 100.0)
                .unwrap();

        assert!(!created.is_empty());
        assert!(svc.undo_state().unwrap().can_undo);
        let after_count = svc.project.lock().unwrap().as_ref().unwrap().objects.len();
        assert!(after_count > initial_count);

        let undone = beambench_service::ops::project::undo_project(&svc).unwrap();
        assert_eq!(undone.objects.len(), initial_count);
    }

    #[test]
    fn copy_along_path_batch_validates_all_sources_before_mutating() {
        let (mut project, layer_id) = test_project();
        let source_a = add_rect(&mut project, layer_id, 0.0, 0.0, 5.0, 5.0);
        let guide = add_rect(&mut project, layer_id, 0.0, 20.0, 100.0, 5.0);
        let initial_count = project.objects.len();
        let svc = svc_with_project(project);
        let missing = ObjectId::new();

        let result =
            copy_along_path_batch_inner(&svc, &[source_a, missing], guide, 4, true, false, 100.0);

        assert!(result.is_err());
        assert_eq!(
            svc.project.lock().unwrap().as_ref().unwrap().objects.len(),
            initial_count
        );
        assert!(!svc.undo_state().unwrap().can_undo);
    }

    #[test]
    fn copy_along_path_batch_count_translates_to_exact_number_of_copies() {
        let (mut project, layer_id) = test_project();
        let source = add_rect(&mut project, layer_id, 0.0, 0.0, 5.0, 5.0);
        let guide = add_vecpath(&mut project, layer_id, "guide", "M0 0 L50 0");
        let svc = svc_with_project(project);

        let created =
            copy_along_path_batch_inner(&svc, &[source], guide, 6, true, false, 100.0).unwrap();

        assert_eq!(created.len(), 6);
    }

    #[test]
    fn offset_delete_original_preserves_non_vector_objects() {
        let (mut project, layer_id) = test_project();
        let raster_id = add_raster(&mut project, layer_id);
        let rect_id = add_rect(&mut project, layer_id, 0.0, 0.0, 10.0, 10.0);

        let svc = svc_with_project(project);

        let result = offset_shapes_inner(
            &svc,
            &[raster_id, rect_id],
            1.0,
            OffsetDirection::Outward,
            CornerStyle::Miter,
            true, // delete_original
        )
        .unwrap();

        assert_eq!(result.len(), 1, "Should produce one offset result");

        // Verify: rect should be deleted, raster should survive
        let guard = svc.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        assert!(
            project.find_object(raster_id).is_some(),
            "Raster object must survive delete_original"
        );
        assert!(
            project.find_object(rect_id).is_none(),
            "Vector source should be deleted"
        );
    }

    #[test]
    fn offset_shapes_reroute_results_off_image_layer() {
        let mut project = Project::new("Offset Route");
        let mut image_layer =
            beambench_core::Layer::new("C12 (Image)", beambench_core::OperationType::Image);
        image_layer.color_tag = beambench_common::ColorTag("#00aaff".into());
        let image_layer_id = image_layer.id;
        let mut line_layer =
            beambench_core::Layer::new("C12 (Line)", beambench_core::OperationType::Line);
        line_layer.color_tag = image_layer.color_tag.clone();
        let line_layer_id = line_layer.id;
        project.add_layer(image_layer);
        project.add_layer(line_layer);
        let rect_id = add_rect(&mut project, image_layer_id, 0.0, 0.0, 10.0, 10.0);
        let svc = svc_with_project(project);

        let result = offset_shapes_inner(
            &svc,
            &[rect_id],
            1.0,
            OffsetDirection::Outward,
            CornerStyle::Miter,
            false,
        )
        .unwrap();

        assert!(!result.is_empty());
        assert!(result.iter().all(|obj| obj.layer_id == line_layer_id));
    }

    #[test]
    fn offset_delete_original_removes_group_and_vector_children() {
        let (mut project, layer_id) = test_project();
        let r1 = add_rect(&mut project, layer_id, 0.0, 0.0, 10.0, 10.0);
        let r2 = add_rect(&mut project, layer_id, 5.0, 0.0, 10.0, 10.0);
        let group_id = add_group(&mut project, layer_id, vec![r1, r2]);

        let svc = svc_with_project(project);

        let result = offset_shapes_inner(
            &svc,
            &[group_id],
            1.0,
            OffsetDirection::Outward,
            CornerStyle::Miter,
            true,
        )
        .unwrap();

        assert!(!result.is_empty());

        let guard = svc.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        assert!(
            project.find_object(group_id).is_none(),
            "Group should be deleted"
        );
        assert!(
            project.find_object(r1).is_none(),
            "Child r1 should be deleted"
        );
        assert!(
            project.find_object(r2).is_none(),
            "Child r2 should be deleted"
        );
    }

    /// Helper: add a VectorPath object (open or closed) from an SVG d-string.
    fn add_vecpath(project: &mut Project, layer_id: LayerRef, name: &str, svg_d: &str) -> ObjectId {
        let path = VecPath::parse_svg_d(svg_d);
        let bounds = path
            .visual_bounds()
            .unwrap_or_else(|| Bounds::new(Point2D::zero(), Point2D::zero()));
        let closed = path.subpaths.iter().any(|sp| sp.closed);
        let obj = ProjectObject::new(
            name,
            layer_id,
            bounds,
            ObjectData::VectorPath {
                path_data: svg_d.to_string(),
                closed,
                ruler_guide_axis: None,
            },
        );
        let id = obj.id;
        project.add_object(obj);
        id
    }

    // ── Clipper2 buffer integration tests ────────────────────────

    #[test]
    fn offset_welded_closed_shapes_spike_free() {
        use beambench_core::vector::flatten::{DEFAULT_TOLERANCE_MM, flatten_vecpath};

        let (mut project, layer_id) = test_project();
        // Overlapping ellipse + rect — classic weld spike scenario
        let ellipse_id = {
            let obj = ProjectObject::new(
                "ellipse",
                layer_id,
                Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(60.0, 40.0)),
                ObjectData::Shape {
                    kind: ShapeKind::Ellipse,
                    width: 60.0,
                    height: 40.0,
                    corner_radius: 0.0,
                },
            );
            let id = obj.id;
            project.add_object(obj);
            id
        };
        let rect_id = add_vecpath(
            &mut project,
            layer_id,
            "rect",
            "M30 10 L95 10 L95 30 L30 30 Z",
        );
        let svc = svc_with_project(project);

        let result = offset_shapes_inner(
            &svc,
            &[ellipse_id, rect_id],
            2.0,
            OffsetDirection::Outward,
            CornerStyle::Miter,
            false,
        )
        .unwrap();

        assert!(
            !result.is_empty(),
            "Should produce at least one offset result"
        );

        // Verify no tiny spike segments in the result
        for created in &result {
            let vp = VecPath::parse_svg_d(match &created.data {
                ObjectData::VectorPath { path_data, .. } => path_data,
                _ => panic!("Expected VectorPath"),
            });
            for pl in flatten_vecpath(&vp, DEFAULT_TOLERANCE_MM) {
                let pts = &pl.points;
                for i in 0..pts.len() {
                    let j = if i + 1 < pts.len() {
                        i + 1
                    } else if pl.closed {
                        0
                    } else {
                        continue;
                    };
                    let d = pts[i].distance_to(&pts[j]);
                    assert!(
                        d > 0.15,
                        "Spike detected: edge length {d:.4}mm between vertices {i} and {j}"
                    );
                }
            }
        }
    }

    #[test]
    fn offset_mixed_closed_open_produces_both() {
        let (mut project, layer_id) = test_project();
        // Closed rect at x=0..10
        let closed_id = add_vecpath(
            &mut project,
            layer_id,
            "closed",
            "M0 0 L10 0 L10 10 L0 10 Z",
        );
        // Open polyline at x=30..40
        let open_id = add_vecpath(&mut project, layer_id, "open", "M30 0 L40 0 L40 10");
        let svc = svc_with_project(project);

        let result = offset_shapes_inner(
            &svc,
            &[closed_id, open_id],
            2.0,
            OffsetDirection::Outward,
            CornerStyle::Miter,
            false,
        )
        .unwrap();

        // Should have results from both closed (Clipper2) and open (vertex) paths
        assert!(
            result.len() >= 2,
            "Mixed closed+open should produce at least 2 results, got {}",
            result.len()
        );

        // Verify results span both x-regions
        let all_bounds: Vec<Bounds> = result.iter().map(|o| o.bounds).collect();
        let has_left = all_bounds.iter().any(|b| b.min.x < 15.0);
        let has_right = all_bounds.iter().any(|b| b.max.x > 25.0);
        assert!(
            has_left,
            "Should have result in the closed-path region (left)"
        );
        assert!(
            has_right,
            "Should have result in the open-path region (right)"
        );
    }

    #[test]
    fn offset_open_paths_not_welded() {
        let (mut project, layer_id) = test_project();
        // Two disjoint open polylines — they should NOT be unioned
        let open1 = add_vecpath(&mut project, layer_id, "open1", "M0 0 L10 0 L10 10");
        let open2 = add_vecpath(&mut project, layer_id, "open2", "M30 0 L40 0 L40 10");
        let svc = svc_with_project(project);

        let result = offset_shapes_inner(
            &svc,
            &[open1, open2],
            1.0,
            OffsetDirection::Outward,
            CornerStyle::Miter,
            false,
        )
        .unwrap();

        // Two separate open paths should produce two separate results (not merged)
        assert!(
            result.len() >= 2,
            "Two disjoint open paths should stay separate, got {} results",
            result.len()
        );
    }

    /// Overlapping closed shapes should be boolean-unioned (welded) before
    /// buffering, producing a single merged contour — not separate buffered rings.
    #[test]
    fn offset_overlapping_closed_produces_merged_contour() {
        let (mut project, layer_id) = test_project();
        // Two overlapping 10×10 rects: x=0..10 and x=5..15 → merged = 15×10
        let r1 = add_vecpath(&mut project, layer_id, "r1", "M0 0 L10 0 L10 10 L0 10 Z");
        let r2 = add_vecpath(&mut project, layer_id, "r2", "M5 0 L15 0 L15 10 L5 10 Z");
        let svc = svc_with_project(project);

        let result = offset_shapes_inner(
            &svc,
            &[r1, r2],
            2.0,
            OffsetDirection::Outward,
            CornerStyle::Miter,
            false,
        )
        .unwrap();

        // Should produce exactly 1 offset result (merged), not 2 separate
        assert_eq!(
            result.len(),
            1,
            "Overlapping closed shapes should produce 1 merged result, got {}",
            result.len()
        );

        // Merged rect is 15×10; outward by 2 → ~19×14
        let bounds = result[0].bounds;
        assert!(
            (bounds.width() - 19.0).abs() < 2.0,
            "Expected ~19mm width for merged offset, got {:.2}",
            bounds.width()
        );
        assert!(
            (bounds.height() - 14.0).abs() < 2.0,
            "Expected ~14mm height for merged offset, got {:.2}",
            bounds.height()
        );
    }

    #[test]
    fn break_apart_reroutes_results_off_image_layer() {
        let mut project = Project::new("Break Route");
        let mut image_layer =
            beambench_core::Layer::new("C13 (Image)", beambench_core::OperationType::Image);
        image_layer.color_tag = beambench_common::ColorTag("#aa00ff".into());
        let image_layer_id = image_layer.id;
        let mut line_layer =
            beambench_core::Layer::new("C13 (Line)", beambench_core::OperationType::Line);
        line_layer.color_tag = image_layer.color_tag.clone();
        let line_layer_id = line_layer.id;
        project.add_layer(image_layer);
        project.add_layer(line_layer);
        let object_id = add_vecpath(
            &mut project,
            image_layer_id,
            "multi",
            "M0 0 L10 0 M20 0 L30 0",
        );
        let svc = svc_with_project(project);

        let result = break_apart_inner(&svc, object_id).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|obj| obj.layer_id == line_layer_id));
    }

    #[test]
    fn break_apart_unsplittable_path_skips_undo() {
        let (mut project, layer_id) = test_project();
        let object_id = add_vecpath(&mut project, layer_id, "single", "M0 0 L10 0");
        let initial_count = project.objects.len();
        let svc = svc_with_project(project);

        let result = break_apart_inner(&svc, object_id).unwrap();

        assert!(result.is_empty());
        assert_eq!(
            svc.project.lock().unwrap().as_ref().unwrap().objects.len(),
            initial_count
        );
        assert!(!svc.undo_state().unwrap().can_undo);
    }

    #[test]
    fn close_path_already_closed_skips_undo() {
        let (mut project, layer_id) = test_project();
        let object_id = add_vecpath(
            &mut project,
            layer_id,
            "closed",
            "M0 0 L10 0 L10 10 L0 10 Z",
        );
        let svc = svc_with_project(project);

        let result = close_path_inner(&svc, object_id).unwrap();

        match result.data {
            ObjectData::VectorPath { closed, .. } => assert!(closed),
            other => panic!("expected vector path, got {other:?}"),
        }
        assert!(!svc.undo_state().unwrap().can_undo);
    }

    #[test]
    fn close_path_out_of_range_open_path_skips_undo() {
        let (mut project, layer_id) = test_project();
        let object_id = add_vecpath(&mut project, layer_id, "open", "M0 0 L10 0 L10 10");
        let svc = svc_with_project(project);

        let result = close_path_inner(&svc, object_id).unwrap();

        match result.data {
            ObjectData::VectorPath { closed, .. } => assert!(!closed),
            other => panic!("expected vector path, got {other:?}"),
        }
        assert!(!svc.undo_state().unwrap().can_undo);
    }

    #[test]
    fn place_tab_far_click_skips_undo() {
        let (mut project, layer_id) = test_project();
        let object_id = add_vecpath(
            &mut project,
            layer_id,
            "closed",
            "M0 0 L10 0 L10 10 L0 10 Z",
        );
        let svc = svc_with_project(project);

        let err = place_tab_inner(&svc, object_id, 100.0, 100.0).unwrap_err();

        assert!(err.contains("too far"));
        let guard = svc.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        assert!(project.find_object(object_id).unwrap().tabs.is_empty());
        assert!(!svc.undo_state().unwrap().can_undo);
    }

    #[test]
    fn place_tab_duplicate_skips_undo() {
        let (mut project, layer_id) = test_project();
        let object_id = add_vecpath(
            &mut project,
            layer_id,
            "closed",
            "M0 0 L10 0 L10 10 L0 10 Z",
        );
        let click = Point2D::new(5.0, 0.0);
        let source = project.find_object(object_id).unwrap().clone();
        let vp = object_to_world_vecpath(&source).unwrap();
        let (anchor, _) =
            beambench_core::vector::tabs::project_point_to_tab_anchor(&vp, click).unwrap();
        project
            .find_object_mut(object_id)
            .unwrap()
            .tabs
            .push(anchor);
        let svc = svc_with_project(project);

        let err = place_tab_inner(&svc, object_id, click.x, click.y).unwrap_err();

        assert!(err.contains("already exists"));
        assert_eq!(
            svc.project
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .find_object(object_id)
                .unwrap()
                .tabs
                .len(),
            1
        );
        assert!(!svc.undo_state().unwrap().can_undo);
    }

    #[test]
    fn remove_tab_without_match_skips_undo() {
        let (mut project, layer_id) = test_project();
        let object_id = add_vecpath(
            &mut project,
            layer_id,
            "closed",
            "M0 0 L10 0 L10 10 L0 10 Z",
        );
        let click = Point2D::new(5.0, 0.0);
        let source = project.find_object(object_id).unwrap().clone();
        let vp = object_to_world_vecpath(&source).unwrap();
        let (anchor, _) =
            beambench_core::vector::tabs::project_point_to_tab_anchor(&vp, click).unwrap();
        project
            .find_object_mut(object_id)
            .unwrap()
            .tabs
            .push(anchor);
        let svc = svc_with_project(project);

        let err = remove_tab_inner(&svc, object_id, 100.0, 100.0).unwrap_err();

        assert!(err.contains("too far"));
        assert_eq!(
            svc.project
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .find_object(object_id)
                .unwrap()
                .tabs
                .len(),
            1
        );
        assert!(!svc.undo_state().unwrap().can_undo);
    }

    #[test]
    fn offset_mixed_group_delete_preserves_raster_child() {
        let (mut project, layer_id) = test_project();
        let rect_id = add_rect(&mut project, layer_id, 0.0, 0.0, 10.0, 10.0);
        let raster_id = add_raster(&mut project, layer_id);
        let group_id = add_group(&mut project, layer_id, vec![rect_id, raster_id]);

        let svc = svc_with_project(project);

        let result = offset_shapes_inner(
            &svc,
            &[group_id],
            1.0,
            OffsetDirection::Outward,
            CornerStyle::Miter,
            true,
        )
        .unwrap();

        assert!(!result.is_empty());

        let guard = svc.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        // Group and vector child deleted; raster child survives
        assert!(
            project.find_object(group_id).is_none(),
            "Group should be deleted"
        );
        assert!(
            project.find_object(rect_id).is_none(),
            "Vector child should be deleted"
        );
        assert!(
            project.find_object(raster_id).is_some(),
            "Raster child in mixed group must survive delete_original"
        );
    }

    // ── circular_array command-level tests ──────────────

    #[test]
    fn circular_array_center_object_excluded_from_sources() {
        let (mut project, layer_id) = test_project();
        let center_id = add_rect(&mut project, layer_id, 50.0, 50.0, 10.0, 10.0);
        let source_id = add_rect(&mut project, layer_id, 0.0, 0.0, 10.0, 10.0);
        let svc = svc_with_project(project);

        let result = run_circular_array(
            &svc,
            vec![source_id.to_string(), center_id.to_string()],
            4,
            30.0,
            Some(center_id.to_string()),
            false,
        )
        .unwrap();

        // 4 positions of 1 source (center excluded) = 4 IDs (1 original moved + 3 new)
        assert_eq!(result.created_ids.len(), 4);

        // The center object should NOT be in created_ids
        assert!(
            !result.created_ids.contains(&center_id.to_string()),
            "Center object must not appear in created_ids"
        );

        // The original source should appear (position 0)
        assert!(
            result.created_ids.contains(&source_id.to_string()),
            "Source object should be in created_ids at position 0"
        );
    }

    #[test]
    fn circular_array_center_object_single_selection_errors() {
        let (mut project, layer_id) = test_project();
        let obj_id = add_rect(&mut project, layer_id, 0.0, 0.0, 10.0, 10.0);
        let svc = svc_with_project(project);

        // Using the only object as center leaves no sources
        let result = run_circular_array(
            &svc,
            vec![obj_id.to_string()],
            4,
            30.0,
            Some(obj_id.to_string()),
            false,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No source objects"));
    }

    #[test]
    fn circular_array_virtual_wraps_copies_as_virtual_clone() {
        let (mut project, layer_id) = test_project();
        let source_id = add_rect(&mut project, layer_id, 0.0, 0.0, 10.0, 10.0);
        let svc = svc_with_project(project);

        let result = run_circular_array(
            &svc,
            vec![source_id.to_string()],
            3,
            30.0,
            None,
            true, // create_virtual
        )
        .unwrap();

        // 3 positions: 1 original (moved) + 2 new virtual clones = 3 IDs
        assert_eq!(result.created_ids.len(), 3);

        let guard = svc.project.lock().unwrap();
        let project = guard.as_ref().unwrap();

        // Position 0 (original) should keep its original data type (Shape)
        let orig = project.find_object(source_id).unwrap();
        assert!(
            !matches!(orig.data, ObjectData::VirtualClone { .. }),
            "Original at position 0 should NOT be a VirtualClone"
        );

        // Positions 1+ should be VirtualClone referencing the source
        for id_str in &result.created_ids[1..] {
            let oid = parse_id(id_str).unwrap();
            let obj = project.find_object(oid).unwrap();
            match &obj.data {
                ObjectData::VirtualClone { source_id: sid } => {
                    assert_eq!(
                        *sid, source_id,
                        "VirtualClone should reference the original source"
                    );
                }
                _ => panic!(
                    "Copy at position 1+ should be VirtualClone, got {:?}",
                    obj.data
                ),
            }
        }
    }

    #[test]
    fn grid_array_copies_group_subtree_and_returns_group_roots() {
        let (mut project, layer_id) = test_project();
        let child_id = add_rect(&mut project, layer_id, 0.0, 0.0, 10.0, 10.0);
        let group_id = add_group_with_bounds(
            &mut project,
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            vec![child_id],
        );
        let svc = svc_with_project(project);

        let result = run_grid_array(
            &svc,
            vec![group_id.to_string()],
            1,
            2,
            GridArraySizingMode::Count,
            GridArraySizingMode::Count,
            None,
            None,
            5.0,
            5.0,
        )
        .unwrap();

        assert_eq!(result.created_ids.len(), 1);
        let copied_group_id = parse_id(&result.created_ids[0]).unwrap();
        let guard = svc.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        let copied_group = project.find_object(copied_group_id).unwrap();
        let ObjectData::Group { children } = &copied_group.data else {
            panic!("grid array should return a copied group root");
        };
        assert_eq!(children.len(), 1);
        assert_ne!(children[0], child_id);
        let copied_child = project.find_object(children[0]).unwrap();
        assert!((copied_child.bounds.min.x - 5.0).abs() < 1e-9);
        assert!((copied_child.bounds.min.y - 0.0).abs() < 1e-9);
    }

    #[test]
    fn circular_array_moves_original_group_members_and_copies_subtree() {
        let (mut project, layer_id) = test_project();
        let child_id = add_rect(&mut project, layer_id, 0.0, 0.0, 10.0, 10.0);
        let group_id = add_group_with_bounds(
            &mut project,
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            vec![child_id],
        );
        let svc = svc_with_project(project);

        let result =
            run_circular_array(&svc, vec![group_id.to_string()], 2, 30.0, None, false).unwrap();

        assert_eq!(result.created_ids.len(), 2);
        assert_eq!(result.created_ids[0], group_id.to_string());
        let copied_group_id = parse_id(&result.created_ids[1]).unwrap();
        let guard = svc.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        let original_child = project.find_object(child_id).unwrap();
        assert!((original_child.bounds.min.x - 30.0).abs() < 1e-9);
        let copied_group = project.find_object(copied_group_id).unwrap();
        let ObjectData::Group { children } = &copied_group.data else {
            panic!("circular array should copy the selected group subtree");
        };
        assert_eq!(children.len(), 1);
        assert_ne!(children[0], child_id);
        assert!(project.find_object(children[0]).is_some());
    }

    #[test]
    fn copy_along_path_copies_group_subtree() {
        let (mut project, layer_id) = test_project();
        let child_id = add_rect(&mut project, layer_id, 0.0, 0.0, 10.0, 10.0);
        let group_id = add_group_with_bounds(
            &mut project,
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            vec![child_id],
        );
        let guide_id = add_vecpath(&mut project, layer_id, "guide", "M0 0 L100 0");
        let svc = svc_with_project(project);

        let created =
            copy_along_path_batch_inner(&svc, &[group_id], guide_id, 2, false, false, 100.0)
                .unwrap();

        let copied_group = created
            .iter()
            .find(|object| matches!(object.data, ObjectData::Group { .. }))
            .expect("copy along path should create a copied group");
        let ObjectData::Group { children } = &copied_group.data else {
            unreachable!();
        };
        assert_eq!(children.len(), 1);
        assert_ne!(children[0], child_id);
        assert!(created.iter().any(|object| object.id == children[0]));
    }

    #[test]
    fn grid_array_total_width_fits_counts_before_creation() {
        let (mut project, layer_id) = test_project();
        let source_id = add_rect(&mut project, layer_id, 0.0, 0.0, 5.0, 5.0);
        let svc = svc_with_project(project);

        let result = run_grid_array(
            &svc,
            vec![source_id.to_string()],
            2,
            10,
            GridArraySizingMode::Total,
            GridArraySizingMode::Count,
            Some(15.0),
            None,
            5.0,
            5.0,
        )
        .unwrap();

        assert_eq!(result.created_ids.len(), 5);
    }

    #[test]
    fn close_tolerance_join_with_line_preserves_existing_endpoints() {
        let result = close_svg_path_with_tolerance_mode(
            "M0 0 L10 0 L0.2 0",
            0.5,
            CloseToleranceMode::JoinWithLine,
        );
        let path = VecPath::parse_svg_d(&result);
        let commands = &path.subpaths[0].commands;

        assert!(path.subpaths[0].closed);
        assert!(matches!(
            commands[0],
            PathCommand::MoveTo { x: 0.0, y: 0.0 }
        ));
        assert!(matches!(
            commands[2],
            PathCommand::LineTo { x, y } if (x - 0.2).abs() < 1e-9 && y == 0.0
        ));
        assert!(matches!(commands.last(), Some(PathCommand::Close)));
    }

    #[test]
    fn close_tolerance_move_ends_together_moves_near_endpoints_to_midpoint() {
        let result = close_svg_path_with_tolerance_mode(
            "M0 0 L10 0 L0.2 0",
            0.5,
            CloseToleranceMode::MoveEndsTogether,
        );
        let path = VecPath::parse_svg_d(&result);
        let commands = &path.subpaths[0].commands;

        assert!(path.subpaths[0].closed);
        assert!(matches!(
            commands[0],
            PathCommand::MoveTo { x, y } if (x - 0.1).abs() < 1e-9 && y == 0.0
        ));
        assert!(matches!(
            commands[2],
            PathCommand::LineTo { x, y } if (x - 0.1).abs() < 1e-9 && y == 0.0
        ));
        assert!(matches!(commands.last(), Some(PathCommand::Close)));
    }

    #[test]
    fn close_tolerance_count_plan_is_read_only_and_uses_selected_open_counts() {
        let (mut project, layer_id) = test_project();
        let near = add_vecpath(&mut project, layer_id, "near", "M0 0 L10 0 L0.2 0");
        let far = add_vecpath(&mut project, layer_id, "far", "M0 0 L10 0 L10 10");
        let closed = add_vecpath(&mut project, layer_id, "closed", "M0 0 L10 0 Z");

        let summary = plan_close_selected_paths_with_tolerance(
            &project,
            &[near, far, closed],
            0.5,
            CloseToleranceMode::JoinWithLine,
        )
        .unwrap();

        assert_eq!(summary.open_shapes_found, 2);
        assert_eq!(summary.shapes_closed, 1);
        assert_eq!(summary.remaining_open, 1);
        assert_eq!(summary.plans.len(), 1);
        assert!(project.find_object(near).is_some());
        assert!(project.find_object(far).is_some());
        assert!(project.find_object(closed).is_some());
    }
}
