use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use beambench_common::{Bounds, Point2D, Transform2D};
use beambench_core::vector::convert::object_to_vecpath;
use beambench_core::vector::flatten::{DEFAULT_TOLERANCE_MM, flatten_vecpath};
use beambench_core::vector::transform::bake_transform;
use beambench_core::{ObjectData, ObjectId, Project, ProjectObject};
use serde::{Deserialize, Serialize};
use u_nesting_d2::{
    Boundary2D as UNestingBoundary2D, Config as UNestingConfig, Geometry2D as UNestingGeometry2D,
    Nester2D as UNester2D, Solver as UNestingSolver, Strategy as UNestingStrategy,
};

use super::planning;
use crate::{ServiceContext, ServiceError, ServiceResult};

const MAX_NEST_PARTS: usize = 50;
const MAX_FLATTENED_POINTS_PER_PART: usize = 5_000;
const EPS: f64 = 1e-7;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NestOptions {
    pub padding_mm: f64,
    pub allow_rotation: bool,
    pub allow_mirror: bool,
    pub lock_inner_objects: bool,
    pub time_limit_ms: u64,
    pub rotation_step_deg: f64,
}

impl Default for NestOptions {
    fn default() -> Self {
        Self {
            padding_mm: 0.0,
            allow_rotation: true,
            allow_mirror: false,
            lock_inner_objects: false,
            time_limit_ms: 15_000,
            rotation_step_deg: 15.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NestResult {
    pub target_container_id: ObjectId,
    pub placed_object_ids: Vec<ObjectId>,
    pub unplaced_object_ids: Vec<ObjectId>,
    pub utilization: f64,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NestError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unplaced_object_ids: Vec<ObjectId>,
}

impl NestError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            unplaced_object_ids: Vec::new(),
        }
    }

    pub fn with_unplaced(mut self, unplaced_object_ids: Vec<ObjectId>) -> Self {
        self.unplaced_object_ids = unplaced_object_ids;
        self
    }
}

impl From<ServiceError> for NestError {
    fn from(value: ServiceError) -> Self {
        if let Some(details) = &value.details {
            let code = details
                .get("nestCode")
                .and_then(|code| code.as_str())
                .map(str::to_string);
            let unplaced_object_ids = details
                .get("unplacedObjectIds")
                .cloned()
                .and_then(|value| serde_json::from_value::<Vec<ObjectId>>(value).ok())
                .unwrap_or_default();
            if let Some(code) = code {
                return NestError {
                    code,
                    message: value.message,
                    unplaced_object_ids,
                };
            }
        }

        NestError::new(infer_nest_error_code(&value), value.message)
    }
}

#[derive(Debug, Clone)]
struct ArrangementUnit {
    root_id: ObjectId,
    member_ids: Vec<ObjectId>,
}

#[derive(Debug, Clone)]
struct Ring {
    points: Vec<Point2D>,
}

#[derive(Debug, Clone)]
struct ContainerRegion {
    rings: Vec<Ring>,
    bounds: Bounds,
    area: f64,
}

#[derive(Debug, Clone)]
struct NestPart {
    root_ids: Vec<ObjectId>,
    member_ids: Vec<ObjectId>,
    rings: Vec<Ring>,
    bounds: Bounds,
    pivot: Point2D,
    area: f64,
    point_count: usize,
}

#[derive(Debug, Clone)]
struct PlacedPart {
    rings: Vec<Ring>,
    bounds: Bounds,
    rotation_deg: f64,
    mirrored: bool,
}

#[derive(Debug, Clone)]
struct Placement {
    root_ids: Vec<ObjectId>,
    member_ids: Vec<ObjectId>,
    offset: Point2D,
    rotation_deg: f64,
    pivot: Point2D,
    mirrored: bool,
}

pub fn nest_selected(
    ctx: &ServiceContext,
    object_ids: Vec<ObjectId>,
    options: NestOptions,
) -> ServiceResult<NestResult> {
    if object_ids.is_empty() {
        return Err(ServiceError::invalid_input(
            "Select a container and at least one object to nest",
        ));
    }

    let started = Instant::now();
    let deadline = if options.time_limit_ms == 0 {
        None
    } else {
        Some(started + Duration::from_millis(options.time_limit_ms))
    };
    let padding = options.padding_mm.max(0.0);

    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    let selected_roots = normalize_arrangement_roots(project, &object_ids);
    let container = find_largest_container(project, &selected_roots)?;
    let part_roots: Vec<ObjectId> = selected_roots
        .into_iter()
        .filter(|id| *id != container.object_id)
        .collect();
    if part_roots.is_empty() {
        return Err(ServiceError::invalid_input(
            "Select at least one object to nest plus a container",
        ));
    }
    if part_roots.len() > MAX_NEST_PARTS {
        return Err(ServiceError::invalid_input(format!(
            "Nest Selected supports up to {MAX_NEST_PARTS} parts"
        )));
    }

    let units = build_units_for_roots(project, &part_roots)?;
    let mut parts = build_nest_parts(project, &units)?;
    if options.lock_inner_objects {
        parts = merge_inner_parts(parts);
    }
    validate_parts_account_for_roots(&part_roots, &parts)?;
    if parts.is_empty() {
        return Err(ServiceError::invalid_input(
            "Select at least one object to nest plus a container",
        ));
    }
    for part in &parts {
        if part.point_count > MAX_FLATTENED_POINTS_PER_PART {
            return Err(ServiceError::invalid_input(format!(
                "Part '{}' exceeds the {MAX_FLATTENED_POINTS_PER_PART} point nesting limit",
                part.root_ids
                    .first()
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            )));
        }
    }
    validate_invertible_transforms(project, &parts)?;

    let (layout, next_project) = compute_valid_layout(
        project,
        &container.region,
        &part_roots,
        &parts,
        padding,
        options.allow_rotation,
        options.allow_mirror,
        options.rotation_step_deg,
        deadline,
    )?;
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    *project = next_project;

    project.dirty = true;
    let placed_ids: Vec<ObjectId> = layout
        .placements
        .iter()
        .flat_map(|placement| placement.root_ids.iter().copied())
        .collect();
    let unplaced_ids: Vec<ObjectId> = layout
        .unplaced
        .iter()
        .flat_map(|part| part.root_ids.iter().copied())
        .collect();
    let utilization = layout.placed_area / container.region.area.max(EPS);
    drop(project_guard);
    planning::invalidate_plan_cache(ctx)?;

    Ok(NestResult {
        target_container_id: container.object_id,
        placed_object_ids: placed_ids,
        unplaced_object_ids: unplaced_ids,
        utilization,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

struct ContainerCandidate {
    object_id: ObjectId,
    region: ContainerRegion,
}

struct LayoutResult {
    placements: Vec<Placement>,
    unplaced: Vec<NestPart>,
    placed_area: f64,
}

fn lock_err(name: &str, e: impl std::fmt::Display) -> ServiceError {
    ServiceError::internal(format!("Failed to lock {name}: {e}"))
}

fn nest_service_error_with_unplaced(
    code: &'static str,
    message: impl Into<String>,
    unplaced_object_ids: Vec<ObjectId>,
) -> ServiceError {
    ServiceError::invalid_input(message).with_details(serde_json::json!({
        "nestCode": code,
        "unplacedObjectIds": unplaced_object_ids,
    }))
}

fn infer_nest_error_code(error: &ServiceError) -> &'static str {
    let message = error.message.to_ascii_lowercase();
    if message.contains("cancel") {
        "cancelled"
    } else if message.contains("timeout") || message.contains("time limit") {
        "timeout"
    } else if message.contains("up to")
        || message.contains("point nesting limit")
        || message.contains("exceeds")
    {
        "cap_exceeded"
    } else if message.contains("container") && message.contains("closed") {
        "no_container"
    } else if message.contains("select at least one object")
        || message.contains("select a container")
        || message.contains("before nesting")
    {
        "no_parts"
    } else if message.contains("too small") || message.contains("could not fit") {
        "unplaced"
    } else if matches!(error.code, crate::error::ServiceErrorCode::InvalidInput) {
        "unsupported_geometry"
    } else {
        "engine_error"
    }
}

fn find_parent_group(project: &Project, child_id: ObjectId) -> Option<ObjectId> {
    project
        .objects
        .iter()
        .find_map(|candidate| match &candidate.data {
            ObjectData::Group { children } if children.contains(&child_id) => Some(candidate.id),
            _ => None,
        })
}

fn top_level_group_for_object(project: &Project, object_id: ObjectId) -> ObjectId {
    let mut current = object_id;
    while let Some(parent) = find_parent_group(project, current) {
        current = parent;
    }
    current
}

fn is_visible_on_visible_layer(project: &Project, object: &ProjectObject) -> bool {
    object.visible
        && project
            .find_layer(object.layer_id)
            .is_some_and(|layer| layer.visible)
}

fn is_ruler_guide(object: &ProjectObject) -> bool {
    matches!(
        object.data,
        ObjectData::VectorPath {
            ruler_guide_axis: Some(_),
            ..
        }
    )
}

fn normalize_arrangement_roots(project: &Project, object_ids: &[ObjectId]) -> Vec<ObjectId> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for object_id in object_ids {
        let Some(object) = project.find_object(*object_id) else {
            continue;
        };
        if is_ruler_guide(object) || !is_visible_on_visible_layer(project, object) {
            continue;
        }
        let promoted = top_level_group_for_object(project, object.id);
        if seen.insert(promoted) {
            normalized.push(promoted);
        }
    }
    normalized
}

fn collect_group_members(project: &Project, object_id: ObjectId, members: &mut Vec<ObjectId>) {
    members.push(object_id);
    if let Some(object) = project.find_object(object_id) {
        if let ObjectData::Group { children } = &object.data {
            for child in children {
                collect_group_members(project, *child, members);
            }
        }
    }
}

fn member_bounds(project: &Project, member_ids: &[ObjectId]) -> Option<Bounds> {
    let mut acc: Option<Bounds> = None;
    for object_id in member_ids {
        let object = project.find_object(*object_id)?;
        if matches!(object.data, ObjectData::Group { .. }) {
            continue;
        }
        acc = Some(match acc {
            Some(current) => current.union(&object.bounds),
            None => object.bounds,
        });
    }
    acc
}

fn member_visual_bounds(project: &Project, member_ids: &[ObjectId]) -> Option<Bounds> {
    let mut acc: Option<Bounds> = None;
    for object_id in member_ids {
        let object = project.find_object(*object_id)?;
        if matches!(object.data, ObjectData::Group { .. }) {
            continue;
        }
        let rings = object_rings(project, object).ok()?;
        let bounds = rings_bounds(&rings)?;
        acc = Some(match acc {
            Some(current) => current.union(&bounds),
            None => bounds,
        });
    }
    acc
}

fn build_units_for_roots(
    project: &Project,
    root_ids: &[ObjectId],
) -> ServiceResult<Vec<ArrangementUnit>> {
    let mut units = Vec::new();
    for root_id in root_ids {
        let Some(root) = project.find_object(*root_id) else {
            continue;
        };
        if root.locked || is_ruler_guide(root) || !is_visible_on_visible_layer(project, root) {
            return Err(ServiceError::invalid_input(
                "Selected nesting parts must be visible and unlocked",
            ));
        }
        let mut member_ids = Vec::new();
        collect_group_members(project, *root_id, &mut member_ids);
        if member_bounds(project, &member_ids).is_none() {
            return Err(ServiceError::invalid_input("Group has no nestable members"));
        }
        units.push(ArrangementUnit {
            root_id: *root_id,
            member_ids,
        });
    }
    Ok(units)
}

fn find_largest_container(
    project: &Project,
    root_ids: &[ObjectId],
) -> ServiceResult<ContainerCandidate> {
    let mut best: Option<ContainerCandidate> = None;
    for root_id in root_ids {
        let Some(object) = project.find_object(*root_id) else {
            continue;
        };
        if !is_container_object(project, object) {
            continue;
        }
        let rings = object_rings(project, object)?;
        let Some(bounds) = rings_bounds(&rings) else {
            continue;
        };
        let area = region_area(&rings);
        if area <= EPS {
            continue;
        }
        if best
            .as_ref()
            .is_none_or(|candidate| area > candidate.region.area)
        {
            best = Some(ContainerCandidate {
                object_id: object.id,
                region: ContainerRegion {
                    rings,
                    bounds,
                    area,
                },
            });
        }
    }
    best.ok_or_else(|| {
        ServiceError::invalid_input("Select a closed vector-compatible container for nesting")
    })
}

fn is_container_object(project: &Project, object: &ProjectObject) -> bool {
    if object.locked || is_ruler_guide(object) || !is_visible_on_visible_layer(project, object) {
        return false;
    }
    match &object.data {
        ObjectData::VectorPath { closed, .. } => *closed,
        ObjectData::Shape { .. } | ObjectData::Polygon { .. } | ObjectData::Star { .. } => true,
        _ => false,
    }
}

fn build_nest_parts(project: &Project, units: &[ArrangementUnit]) -> ServiceResult<Vec<NestPart>> {
    let mut parts = Vec::new();
    for unit in units {
        let mut rings = Vec::new();
        for member_id in &unit.member_ids {
            let object = project
                .find_object(*member_id)
                .ok_or_else(|| ServiceError::not_found("Object not found"))?;
            if matches!(object.data, ObjectData::Group { .. }) {
                continue;
            }
            if object.locked
                || is_ruler_guide(object)
                || !is_visible_on_visible_layer(project, object)
            {
                return Err(ServiceError::invalid_input(
                    "Selected nesting parts must be visible and unlocked",
                ));
            }
            if matches!(object.data, ObjectData::RasterImage { .. }) {
                return Err(ServiceError::invalid_input(
                    "Raster images are not supported by Nest Selected",
                ));
            }
            rings.extend(object_rings(project, object)?);
        }
        if rings.is_empty() {
            return Err(ServiceError::invalid_input(
                "Selected objects must resolve to closed vector outlines",
            ));
        }
        let area: f64 = rings.iter().map(ring_area_abs).sum();
        let point_count = rings.iter().map(|ring| ring.points.len()).sum();
        let bounds = rings_bounds(&rings)
            .ok_or_else(|| ServiceError::invalid_input("Selected part has no bounds"))?;
        parts.push(NestPart {
            root_ids: vec![unit.root_id],
            member_ids: unit.member_ids.clone(),
            rings,
            bounds,
            pivot: bounds_center(&bounds),
            area,
            point_count,
        });
    }
    Ok(parts)
}

fn validate_parts_account_for_roots(
    expected_roots: &[ObjectId],
    parts: &[NestPart],
) -> ServiceResult<()> {
    let expected: HashSet<ObjectId> = expected_roots.iter().copied().collect();
    let actual: HashSet<ObjectId> = parts
        .iter()
        .flat_map(|part| part.root_ids.iter().copied())
        .collect();
    if actual != expected {
        return Err(ServiceError::invalid_input(
            "Nest Selected could not account for all selected objects; no changes were applied",
        ));
    }
    Ok(())
}

fn validate_layout_accounts_for_roots(
    expected_roots: &[ObjectId],
    layout: &LayoutResult,
) -> ServiceResult<()> {
    let expected: HashSet<ObjectId> = expected_roots.iter().copied().collect();
    let actual: HashSet<ObjectId> = layout
        .placements
        .iter()
        .flat_map(|placement| placement.root_ids.iter().copied())
        .chain(
            layout
                .unplaced
                .iter()
                .flat_map(|part| part.root_ids.iter().copied()),
        )
        .collect();
    if actual != expected {
        return Err(ServiceError::invalid_input(
            "Nest Selected could not account for all selected objects; no changes were applied",
        ));
    }
    Ok(())
}

fn object_rings(project: &Project, object: &ProjectObject) -> ServiceResult<Vec<Ring>> {
    let path = object_to_rendered_world_vecpath_resolved(object, project).ok_or_else(|| {
        ServiceError::invalid_input("Selected object cannot be converted to vector geometry")
    })?;
    let rings: Vec<Ring> = flatten_vecpath(&path, DEFAULT_TOLERANCE_MM)
        .into_iter()
        .filter(|polyline| polyline.closed && polyline.points.len() >= 3)
        .map(|polyline| Ring {
            points: polyline.points,
        })
        .collect();
    if rings.is_empty() {
        return Err(ServiceError::invalid_input(
            "Selected objects must resolve to closed vector outlines",
        ));
    }
    Ok(rings)
}

fn object_to_rendered_world_vecpath_resolved(
    object: &ProjectObject,
    project: &Project,
) -> Option<beambench_common::path::VecPath> {
    let resolved;
    let effective = if matches!(object.data, ObjectData::VirtualClone { .. }) {
        resolved = project.resolve_clone(object)?;
        &resolved
    } else {
        object
    };
    let path = object_to_vecpath(&effective.data)?;
    let intrinsic = path.visual_bounds().or_else(|| path.bounds())?;
    let old_w = intrinsic.max.x - intrinsic.min.x;
    let old_h = intrinsic.max.y - intrinsic.min.y;
    let new_w = effective.bounds.max.x - effective.bounds.min.x;
    let new_h = effective.bounds.max.y - effective.bounds.min.y;
    let sx = if old_w > 0.0 { new_w / old_w } else { 1.0 };
    let sy = if old_h > 0.0 { new_h / old_h } else { 1.0 };
    let tx = effective.bounds.min.x - intrinsic.min.x * sx;
    let ty = effective.bounds.min.y - intrinsic.min.y * sy;
    let mapped = bake_transform(
        &path,
        &Transform2D {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            tx,
            ty,
        },
    );
    let center = bounds_center(&effective.bounds);
    let rendered_transform = Transform2D::translate(center.x, center.y)
        .compose(&effective.transform)
        .compose(&Transform2D::translate(-center.x, -center.y));
    Some(bake_transform(&mapped, &rendered_transform))
}

fn merge_inner_parts(parts: Vec<NestPart>) -> Vec<NestPart> {
    let mut consumed = vec![false; parts.len()];
    let mut children_by_parent: HashMap<usize, Vec<usize>> = HashMap::new();
    for child_idx in 0..parts.len() {
        let child = &parts[child_idx];
        let mut parent: Option<(usize, f64)> = None;
        for (parent_idx, candidate) in parts.iter().enumerate() {
            if parent_idx == child_idx {
                continue;
            }
            if bounds_contains(candidate.bounds, child.bounds)
                && rings_inside_rings(&child.rings, &candidate.rings)
            {
                let area = candidate.area;
                if parent.is_none_or(|(_, best_area)| area < best_area) {
                    parent = Some((parent_idx, area));
                }
            }
        }
        if let Some((parent_idx, _)) = parent {
            consumed[child_idx] = true;
            children_by_parent
                .entry(parent_idx)
                .or_default()
                .push(child_idx);
        }
    }

    let mut merged = Vec::new();
    for (idx, part) in parts.iter().enumerate() {
        if consumed[idx] {
            continue;
        }
        let mut next = part.clone();
        if let Some(children) = children_by_parent.get(&idx) {
            for child_idx in children {
                let child = &parts[*child_idx];
                next.root_ids.extend(child.root_ids.iter().copied());
                next.member_ids.extend(child.member_ids.iter().copied());
                next.rings.extend(child.rings.clone());
                next.bounds = next.bounds.union(&child.bounds);
                next.area += child.area;
                next.point_count += child.point_count;
            }
            dedup_ids(&mut next.root_ids);
            dedup_ids(&mut next.member_ids);
            next.pivot = bounds_center(&next.bounds);
        }
        merged.push(next);
    }
    merged
}

fn compute_valid_layout(
    project: &Project,
    container: &ContainerRegion,
    part_roots: &[ObjectId],
    parts: &[NestPart],
    padding: f64,
    allow_rotation: bool,
    allow_mirror: bool,
    rotation_step_deg: f64,
    deadline: Option<Instant>,
) -> ServiceResult<(LayoutResult, Project)> {
    if let Some(layout) = compute_layout_with_u_nesting(
        container,
        parts,
        padding,
        allow_rotation,
        allow_mirror,
        rotation_step_deg,
        deadline,
    )? {
        match validate_layout_candidate(project, container, part_roots, layout, padding) {
            Ok(validated) => return Ok(validated),
            Err(err) => {
                tracing::warn!(
                    "U-Nesting layout failed Beam Bench validation and will fall back to deterministic nesting: {}",
                    err.message
                );
            }
        }
    }

    let layout = compute_layout_legacy(
        container,
        parts,
        padding,
        allow_rotation,
        allow_mirror,
        rotation_step_deg,
        deadline,
    )?;
    validate_layout_candidate(project, container, part_roots, layout, padding)
}

fn validate_layout_candidate(
    project: &Project,
    container: &ContainerRegion,
    part_roots: &[ObjectId],
    layout: LayoutResult,
    padding: f64,
) -> ServiceResult<(LayoutResult, Project)> {
    validate_layout_accounts_for_roots(part_roots, &layout)?;
    if layout.placements.is_empty() {
        let unplaced_ids = layout
            .unplaced
            .iter()
            .flat_map(|part| part.root_ids.iter().copied())
            .collect();
        return Err(nest_service_error_with_unplaced(
            "unplaced",
            "Target container is too small for the selected objects",
            unplaced_ids,
        ));
    }
    if !layout.unplaced.is_empty() {
        let unplaced_ids: Vec<ObjectId> = layout
            .unplaced
            .iter()
            .flat_map(|part| part.root_ids.iter().copied())
            .collect();
        let unplaced_count: usize = layout.unplaced.iter().map(|part| part.root_ids.len()).sum();
        return Err(nest_service_error_with_unplaced(
            "unplaced",
            format!(
                "Nesting could not fit {unplaced_count} object(s) inside the selected container"
            ),
            unplaced_ids,
        ));
    }

    let mut next_project = project.clone();
    apply_placements(&mut next_project, &layout.placements)?;
    validate_applied_layout(&next_project, container, &layout.placements, padding)?;
    Ok((layout, next_project))
}

fn compute_layout_legacy(
    container: &ContainerRegion,
    parts: &[NestPart],
    padding: f64,
    allow_rotation: bool,
    allow_mirror: bool,
    rotation_step_deg: f64,
    deadline: Option<Instant>,
) -> ServiceResult<LayoutResult> {
    let container_bounds = container.bounds;
    let mut order: Vec<usize> = (0..parts.len()).collect();
    order.sort_by(|a, b| {
        parts[*b].area.total_cmp(&parts[*a].area).then_with(|| {
            parts[*b]
                .bounds
                .height()
                .total_cmp(&parts[*a].bounds.height())
        })
    });

    let mut placed = Vec::<PlacedPart>::new();
    let mut placements = Vec::<Placement>::new();
    let mut unplaced = Vec::<NestPart>::new();
    let mut placed_area = 0.0;

    for part_idx in order {
        check_deadline(deadline)?;
        let part = &parts[part_idx];
        if let Some((placement, placed_part)) = find_part_placement(
            container,
            container_bounds,
            part,
            &placed,
            padding,
            allow_rotation,
            allow_mirror,
            rotation_step_deg,
            deadline,
        )? {
            placed_area += part.area;
            placements.push(placement);
            placed.push(placed_part);
        } else {
            unplaced.push(part.clone());
        }
    }

    Ok(LayoutResult {
        placements,
        unplaced,
        placed_area,
    })
}

struct EngineShape {
    exterior: Vec<(f64, f64)>,
    holes: Vec<Vec<(f64, f64)>>,
}

fn compute_layout_with_u_nesting(
    container: &ContainerRegion,
    parts: &[NestPart],
    padding: f64,
    allow_rotation: bool,
    allow_mirror: bool,
    rotation_step_deg: f64,
    deadline: Option<Instant>,
) -> ServiceResult<Option<LayoutResult>> {
    let Some(container_shape) = rings_to_engine_shape(&container.rings, container.bounds.min)
    else {
        return Ok(None);
    };

    let mut geometries = Vec::with_capacity(parts.len());
    for (idx, part) in parts.iter().enumerate() {
        let Some(shape) = rings_to_engine_shape(&part.rings, part.pivot) else {
            return Ok(None);
        };
        let mut geometry =
            UNestingGeometry2D::new(engine_part_id(idx)).with_polygon(shape.exterior);
        for hole in shape.holes {
            geometry = geometry.with_hole(hole);
        }
        if allow_rotation {
            geometry = geometry.with_rotations_deg(rotation_candidates(rotation_step_deg));
        } else {
            geometry = geometry.with_rotations_deg(vec![0.0]);
        }
        geometry = geometry.with_flip(allow_mirror);
        geometries.push(geometry);
    }

    let mut boundary = UNestingBoundary2D::new(container_shape.exterior);
    for hole in container_shape.holes {
        boundary = boundary.with_hole(hole);
    }

    let time_limit_ms = remaining_time_limit_ms(deadline);
    let engine_clearance = padding.max(DEFAULT_TOLERANCE_MM);
    let config = UNestingConfig::new()
        .with_strategy(UNestingStrategy::NfpGuided)
        .with_spacing(engine_clearance)
        .with_margin(engine_clearance)
        .with_time_limit(time_limit_ms);
    let nester = UNester2D::new(config);
    let result = match nester.solve(&geometries, &boundary) {
        Ok(result) => result,
        Err(err) => {
            tracing::warn!(
                "U-Nesting solve failed and will fall back to deterministic nesting: {err}"
            );
            return Ok(None);
        }
    };
    // The user-facing time limit is a hard ceiling. If U-Nesting overruns its
    // own budget, do not start the deterministic fallback and freeze longer.
    check_deadline(deadline)?;

    let mut placed_part_indices = HashSet::new();
    let mut placed_parts = Vec::<PlacedPart>::new();
    let mut placements = Vec::new();
    let mut placed_area = 0.0;
    for placement in result.placements {
        let Some(idx) = parse_engine_part_id(&placement.geometry_id) else {
            continue;
        };
        let Some(part) = parts.get(idx) else {
            continue;
        };
        if !placed_part_indices.insert(idx) {
            continue;
        }
        let x = placement.position.first().copied().unwrap_or(0.0);
        let y = placement.position.get(1).copied().unwrap_or(0.0);
        let rotation_deg = placement
            .rotation
            .first()
            .copied()
            .unwrap_or(0.0)
            .to_degrees();
        let mirrored = placement.mirrored;
        let offset = Point2D::new(
            container.bounds.min.x + x - part.pivot.x,
            container.bounds.min.y + y - part.pivot.y,
        );
        let rings = transform_rings(&part.rings, part.pivot, rotation_deg, mirrored, offset);
        let Some(bounds) = rings_bounds(&rings) else {
            return Ok(None);
        };
        if !bounds_within(bounds, container.bounds, padding)
            || !rings_inside_container(&rings, container, padding)
            || placed_parts
                .iter()
                .any(|other| rings_collide(&rings, bounds, &other.rings, other.bounds, padding))
        {
            return Ok(None);
        }
        let placement = Placement {
            root_ids: part.root_ids.clone(),
            member_ids: part.member_ids.clone(),
            offset,
            rotation_deg,
            pivot: part.pivot,
            mirrored,
        };
        placements.push(placement);
        placed_parts.push(PlacedPart {
            rings,
            bounds,
            rotation_deg,
            mirrored,
        });
        placed_area += part.area;
    }

    let mut unplaced = Vec::new();
    for (idx, part) in parts.iter().enumerate() {
        if !placed_part_indices.contains(&idx) {
            unplaced.push(part.clone());
        }
    }
    if !unplaced.is_empty() {
        return Ok(None);
    }

    Ok(Some(LayoutResult {
        placements,
        unplaced,
        placed_area,
    }))
}

fn engine_part_id(idx: usize) -> String {
    format!("part-{idx}")
}

fn parse_engine_part_id(id: &str) -> Option<usize> {
    id.strip_prefix("part-")?.parse().ok()
}

fn remaining_time_limit_ms(deadline: Option<Instant>) -> u64 {
    let Some(deadline) = deadline else {
        // U-Nesting Config documents 0 as unlimited, which matches Beam Bench's
        // time_limit_ms = 0 semantics.
        return 0;
    };
    deadline
        .checked_duration_since(Instant::now())
        .map(|duration| duration.as_millis().clamp(1, u64::MAX as u128) as u64)
        .unwrap_or(1)
}

fn rotation_candidates(rotation_step_deg: f64) -> Vec<f64> {
    let step = if rotation_step_deg.is_finite() && rotation_step_deg > EPS {
        rotation_step_deg.clamp(1.0, 90.0)
    } else {
        15.0
    };
    let count = (360.0 / step).round().clamp(1.0, 360.0) as usize;
    (0..count)
        .map(|idx| {
            let angle = idx as f64 * step;
            if angle >= 360.0 { angle % 360.0 } else { angle }
        })
        .collect()
}

fn rings_to_engine_shape(rings: &[Ring], origin: Point2D) -> Option<EngineShape> {
    let mut exteriors = Vec::<usize>::new();
    for (idx, ring) in rings.iter().enumerate() {
        let sample = ring.points.first().copied()?;
        let depth = rings
            .iter()
            .enumerate()
            .filter(|(other_idx, other)| *other_idx != idx && point_in_ring_strict(sample, other))
            .count();
        if depth % 2 == 0 {
            exteriors.push(idx);
        }
    }
    if exteriors.is_empty() {
        return None;
    }
    if exteriors.len() > 1 {
        // U-Nesting's 2D geometry API accepts one exterior with holes. For
        // multiple disjoint exterior rings, pass a conservative hull to keep the
        // engine path available. Beam Bench still validates the original rings
        // before applying, so hull-only placements cannot escape correctness,
        // but part holes and multi-region container gaps are intentionally not
        // exposed to the engine in this fallback representation.
        let hull = convex_hull_ring(
            exteriors
                .iter()
                .flat_map(|idx| rings[*idx].points.iter().copied()),
        )?;
        let exterior = normalize_engine_ring(&hull, origin, false)?;
        return Some(EngineShape {
            exterior,
            holes: Vec::new(),
        });
    }
    let exterior_idx = exteriors[0];
    let exterior = normalize_engine_ring(&rings[exterior_idx], origin, false)?;
    let mut holes = Vec::new();
    for (idx, ring) in rings.iter().enumerate() {
        if idx == exterior_idx {
            continue;
        }
        let sample = ring.points.first().copied()?;
        if point_in_ring_strict(sample, &rings[exterior_idx]) {
            holes.push(normalize_engine_ring(ring, origin, true)?);
        } else {
            return None;
        }
    }
    Some(EngineShape { exterior, holes })
}

fn convex_hull_ring(points: impl IntoIterator<Item = Point2D>) -> Option<Ring> {
    let mut points: Vec<Point2D> = points.into_iter().collect();
    if points.len() < 3 {
        return None;
    }
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then_with(|| a.y.total_cmp(&b.y)));
    points.dedup_by(|a, b| (a.x - b.x).abs() <= EPS && (a.y - b.y).abs() <= EPS);
    if points.len() < 3 {
        return None;
    }

    let mut lower = Vec::new();
    for point in &points {
        while lower.len() >= 2
            && cross(lower[lower.len() - 2], lower[lower.len() - 1], *point) <= EPS
        {
            lower.pop();
        }
        lower.push(*point);
    }

    let mut upper = Vec::new();
    for point in points.iter().rev() {
        while upper.len() >= 2
            && cross(upper[upper.len() - 2], upper[upper.len() - 1], *point) <= EPS
        {
            upper.pop();
        }
        upper.push(*point);
    }

    lower.pop();
    upper.pop();
    lower.extend(upper);
    if lower.len() < 3 {
        return None;
    }
    Some(Ring { points: lower })
}

fn cross(a: Point2D, b: Point2D, c: Point2D) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn normalize_engine_ring(ring: &Ring, origin: Point2D, hole: bool) -> Option<Vec<(f64, f64)>> {
    if ring.points.len() < 3 {
        return None;
    }
    let mut points: Vec<(f64, f64)> = ring
        .points
        .iter()
        .map(|point| (point.x - origin.x, point.y - origin.y))
        .collect();
    let area = signed_area(&ring.points);
    if (!hole && area < 0.0) || (hole && area > 0.0) {
        points.reverse();
    }
    Some(points)
}

fn find_part_placement(
    container: &ContainerRegion,
    container_bounds: Bounds,
    part: &NestPart,
    placed: &[PlacedPart],
    padding: f64,
    allow_rotation: bool,
    allow_mirror: bool,
    rotation_step_deg: f64,
    deadline: Option<Instant>,
) -> ServiceResult<Option<(Placement, PlacedPart)>> {
    let rotations = if allow_rotation {
        rotation_candidates(rotation_step_deg)
    } else {
        vec![0.0]
    };
    let mirrors: &[bool] = if allow_mirror {
        &[false, true]
    } else {
        &[false]
    };
    let mut best: Option<(Placement, PlacedPart)> = None;
    for mirrored in mirrors {
        for rotation_deg in &rotations {
            check_deadline(deadline)?;
            let rotated = transform_rings(
                &part.rings,
                part.pivot,
                *rotation_deg,
                *mirrored,
                Point2D::zero(),
            );
            let Some(rotated_bounds) = rings_bounds(&rotated) else {
                continue;
            };
            let candidates = candidate_mins(container_bounds, rotated_bounds, placed, padding);
            for target_min in candidates {
                check_deadline(deadline)?;
                let dx = target_min.x - rotated_bounds.min.x;
                let dy = target_min.y - rotated_bounds.min.y;
                let rings = translate_rings(&rotated, dx, dy);
                let Some(bounds) = rings_bounds(&rings) else {
                    continue;
                };
                if !bounds_within(bounds, container_bounds, padding) {
                    continue;
                }
                if !rings_inside_container(&rings, container, padding) {
                    continue;
                }
                if placed
                    .iter()
                    .any(|other| rings_collide(&rings, bounds, &other.rings, other.bounds, padding))
                {
                    continue;
                }

                let placement = Placement {
                    root_ids: part.root_ids.clone(),
                    member_ids: part.member_ids.clone(),
                    offset: Point2D::new(dx, dy),
                    rotation_deg: *rotation_deg,
                    pivot: part.pivot,
                    mirrored: *mirrored,
                };
                let placed_part = PlacedPart {
                    rings,
                    bounds,
                    rotation_deg: *rotation_deg,
                    mirrored: *mirrored,
                };
                let is_better = best.as_ref().is_none_or(|(_, current)| {
                    bounds.min.y < current.bounds.min.y - EPS
                        || ((bounds.min.y - current.bounds.min.y).abs() <= EPS
                            && bounds.min.x < current.bounds.min.x - EPS)
                        || ((bounds.min.y - current.bounds.min.y).abs() <= EPS
                            && (bounds.min.x - current.bounds.min.x).abs() <= EPS
                            && !*mirrored
                            && current.mirrored)
                        || ((bounds.min.y - current.bounds.min.y).abs() <= EPS
                            && (bounds.min.x - current.bounds.min.x).abs() <= EPS
                            && *mirrored == current.mirrored
                            && rotation_deg.abs() < current.rotation_deg.abs())
                });
                if is_better {
                    best = Some((placement, placed_part));
                }
            }
        }
    }
    Ok(best)
}

fn candidate_mins(
    container_bounds: Bounds,
    part_bounds: Bounds,
    placed: &[PlacedPart],
    padding: f64,
) -> Vec<Point2D> {
    let mut xs = vec![container_bounds.min.x + padding];
    let mut ys = vec![container_bounds.min.y + padding];
    for other in placed {
        xs.push(other.bounds.min.x);
        xs.push(other.bounds.max.x + padding);
        ys.push(other.bounds.min.y);
        ys.push(other.bounds.max.y + padding);
    }
    let step = ((part_bounds.width().min(part_bounds.height())) / 4.0).clamp(1.0, 10.0);
    let max_x = container_bounds.max.x - part_bounds.width() - padding;
    let max_y = container_bounds.max.y - part_bounds.height() - padding;
    let mut y = container_bounds.min.y + padding;
    while y <= max_y + EPS {
        ys.push(y);
        y += step;
    }
    let mut x = container_bounds.min.x + padding;
    while x <= max_x + EPS {
        xs.push(x);
        x += step;
    }
    xs.sort_by(f64::total_cmp);
    ys.sort_by(f64::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() <= EPS);
    ys.dedup_by(|a, b| (*a - *b).abs() <= EPS);

    let mut result = Vec::new();
    for y in ys {
        for x in &xs {
            if *x <= max_x + EPS && y <= max_y + EPS {
                result.push(Point2D::new(*x, y));
            }
        }
    }
    result
}

fn check_deadline(deadline: Option<Instant>) -> ServiceResult<()> {
    if deadline.is_some_and(|instant| Instant::now() >= instant) {
        return Err(ServiceError::invalid_input(
            "Nesting timed out before applying changes",
        ));
    }
    Ok(())
}

fn transform_rings(
    rings: &[Ring],
    pivot: Point2D,
    rotation_deg: f64,
    mirrored: bool,
    offset: Point2D,
) -> Vec<Ring> {
    rings
        .iter()
        .map(|ring| Ring {
            points: ring
                .points
                .iter()
                .map(|point| mirror_point(*point, pivot, mirrored))
                .map(|point| rotate_point(point, pivot, rotation_deg))
                .map(|point| Point2D::new(point.x + offset.x, point.y + offset.y))
                .collect(),
        })
        .collect()
}

fn mirror_point(point: Point2D, pivot: Point2D, mirrored: bool) -> Point2D {
    if mirrored {
        Point2D::new((2.0 * pivot.x) - point.x, point.y)
    } else {
        point
    }
}

fn translate_rings(rings: &[Ring], dx: f64, dy: f64) -> Vec<Ring> {
    rings
        .iter()
        .map(|ring| Ring {
            points: ring
                .points
                .iter()
                .map(|point| Point2D::new(point.x + dx, point.y + dy))
                .collect(),
        })
        .collect()
}

fn rotate_point(point: Point2D, pivot: Point2D, rotation_deg: f64) -> Point2D {
    if rotation_deg.abs() <= EPS {
        return point;
    }
    let radians = rotation_deg.to_radians();
    let (s, c) = radians.sin_cos();
    let dx = point.x - pivot.x;
    let dy = point.y - pivot.y;
    Point2D::new(pivot.x + dx * c - dy * s, pivot.y + dx * s + dy * c)
}

fn apply_placements(project: &mut Project, placements: &[Placement]) -> ServiceResult<()> {
    for placement in placements {
        apply_placement_to_members(project, placement)?;
    }
    Ok(())
}

fn placement_world_transform(placement: &Placement) -> ServiceResult<Transform2D> {
    let mirror_about_pivot = Transform2D::translate(placement.pivot.x, placement.pivot.y)
        .compose(&if placement.mirrored {
            Transform2D::scale(-1.0, 1.0)
        } else {
            Transform2D::identity()
        })
        .compose(&Transform2D::translate(
            -placement.pivot.x,
            -placement.pivot.y,
        ));
    let rotate_about_pivot = Transform2D::translate(placement.pivot.x, placement.pivot.y)
        .compose(&Transform2D::rotate(placement.rotation_deg.to_radians()))
        .compose(&Transform2D::translate(
            -placement.pivot.x,
            -placement.pivot.y,
        ));
    Ok(
        Transform2D::translate(placement.offset.x, placement.offset.y)
            .compose(&rotate_about_pivot)
            .compose(&mirror_about_pivot),
    )
}

fn apply_placement_to_members(project: &mut Project, placement: &Placement) -> ServiceResult<()> {
    let transform = placement_world_transform(placement)?;
    let linear_transform =
        Transform2D::rotate(placement.rotation_deg.to_radians()).compose(&if placement.mirrored {
            Transform2D::scale(-1.0, 1.0)
        } else {
            Transform2D::identity()
        });
    let mut seen = HashSet::new();
    let mut group_ids = Vec::new();
    for object_id in &placement.member_ids {
        if !seen.insert(*object_id) {
            continue;
        }
        let object = project
            .find_object_mut(*object_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?;
        if matches!(object.data, ObjectData::Group { .. }) {
            group_ids.push(*object_id);
            continue;
        }
        let center = bounds_center(&object.bounds);
        let next_center = transform.apply(&center);
        let dx = next_center.x - center.x;
        let dy = next_center.y - center.y;
        object.bounds.min.x += dx;
        object.bounds.min.y += dy;
        object.bounds.max.x += dx;
        object.bounds.max.y += dy;
        object.transform = linear_transform.compose(&object.transform);
    }

    for group_id in group_ids.into_iter().rev() {
        let mut members = Vec::new();
        collect_group_members(project, group_id, &mut members);
        if let Some(bounds) = member_visual_bounds(project, &members) {
            let group = project
                .find_object_mut(group_id)
                .ok_or_else(|| ServiceError::not_found("Object not found"))?;
            group.bounds = bounds;
        }
    }
    project.dirty = true;
    Ok(())
}

fn validate_applied_layout(
    project: &Project,
    container: &ContainerRegion,
    placements: &[Placement],
    padding: f64,
) -> ServiceResult<()> {
    let mut placed = Vec::<(ObjectId, Vec<Ring>, Bounds)>::new();
    for placement in placements {
        let rings = member_rings(project, &placement.member_ids)?;
        let bounds = rings_bounds(&rings)
            .ok_or_else(|| ServiceError::internal("Nested part lost its bounds"))?;
        let root_id = placement.root_ids.first().copied().ok_or_else(|| {
            ServiceError::internal("Nested placement was missing its root object")
        })?;
        if !bounds_within(bounds, container.bounds, padding)
            || !rings_inside_container(&rings, container, padding)
        {
            return Err(ServiceError::invalid_input(
                "Nesting result would place objects outside the selected container; no changes were applied",
            ));
        }
        for (other_id, other_rings, other_bounds) in &placed {
            if rings_collide(&rings, bounds, other_rings, *other_bounds, padding) {
                return Err(ServiceError::invalid_input(format!(
                    "Nesting result would overlap objects {root_id} and {other_id}; no changes were applied"
                )));
            }
        }
        placed.push((root_id, rings, bounds));
    }
    Ok(())
}

fn member_rings(project: &Project, member_ids: &[ObjectId]) -> ServiceResult<Vec<Ring>> {
    let mut seen = HashSet::new();
    let mut rings = Vec::new();
    for object_id in member_ids {
        if !seen.insert(*object_id) {
            continue;
        }
        let object = project
            .find_object(*object_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?;
        if matches!(object.data, ObjectData::Group { .. }) {
            continue;
        }
        rings.extend(object_rings(project, object)?);
    }
    Ok(rings)
}

fn validate_invertible_transforms(project: &Project, parts: &[NestPart]) -> ServiceResult<()> {
    let mut seen = HashSet::new();
    for part in parts {
        for object_id in &part.member_ids {
            if !seen.insert(*object_id) {
                continue;
            }
            let object = project
                .find_object(*object_id)
                .ok_or_else(|| ServiceError::not_found("Object not found"))?;
            if matches!(object.data, ObjectData::Group { .. }) {
                continue;
            }
            if object.transform.determinant().abs() <= EPS {
                return Err(ServiceError::invalid_input(
                    "Nest Selected cannot place objects with non-invertible transforms",
                ));
            }
        }
    }
    Ok(())
}

fn ring_area_abs(ring: &Ring) -> f64 {
    signed_area(&ring.points).abs()
}

fn region_area(rings: &[Ring]) -> f64 {
    let mut area = 0.0;
    for (idx, ring) in rings.iter().enumerate() {
        let Some(sample) = ring.points.first().copied() else {
            continue;
        };
        let depth = rings
            .iter()
            .enumerate()
            .filter(|(other_idx, other)| *other_idx != idx && point_in_ring_strict(sample, other))
            .count();
        if depth % 2 == 0 {
            area += ring_area_abs(ring);
        } else {
            area -= ring_area_abs(ring);
        }
    }
    area.max(0.0)
}

fn signed_area(points: &[Point2D]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }
    let mut area = 0.0;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        area += a.x * b.y - b.x * a.y;
    }
    area / 2.0
}

fn rings_bounds(rings: &[Ring]) -> Option<Bounds> {
    let mut acc: Option<Bounds> = None;
    for ring in rings {
        let bounds = ring_bounds(ring)?;
        acc = Some(match acc {
            Some(current) => current.union(&bounds),
            None => bounds,
        });
    }
    acc
}

fn ring_bounds(ring: &Ring) -> Option<Bounds> {
    let first = ring.points.first()?;
    let mut min_x = first.x;
    let mut min_y = first.y;
    let mut max_x = first.x;
    let mut max_y = first.y;
    for point in &ring.points {
        min_x = min_x.min(point.x);
        min_y = min_y.min(point.y);
        max_x = max_x.max(point.x);
        max_y = max_y.max(point.y);
    }
    Some(Bounds::new(
        Point2D::new(min_x, min_y),
        Point2D::new(max_x, max_y),
    ))
}

fn bounds_center(bounds: &Bounds) -> Point2D {
    Point2D::new(
        (bounds.min.x + bounds.max.x) / 2.0,
        (bounds.min.y + bounds.max.y) / 2.0,
    )
}

fn bounds_contains(outer: Bounds, inner: Bounds) -> bool {
    inner.min.x >= outer.min.x - EPS
        && inner.min.y >= outer.min.y - EPS
        && inner.max.x <= outer.max.x + EPS
        && inner.max.y <= outer.max.y + EPS
}

fn bounds_within(inner: Bounds, outer: Bounds, padding: f64) -> bool {
    inner.min.x >= outer.min.x + padding - EPS
        && inner.min.y >= outer.min.y + padding - EPS
        && inner.max.x <= outer.max.x - padding + EPS
        && inner.max.y <= outer.max.y - padding + EPS
}

fn bounds_overlap(a: Bounds, b: Bounds, padding: f64) -> bool {
    a.min.x <= b.max.x + padding + EPS
        && a.max.x + padding + EPS >= b.min.x
        && a.min.y <= b.max.y + padding + EPS
        && a.max.y + padding + EPS >= b.min.y
}

fn rings_inside_rings(inner: &[Ring], outer: &[Ring]) -> bool {
    inner.iter().all(|ring| {
        ring.points
            .iter()
            .all(|point| point_in_region(*point, outer))
    })
}

fn rings_inside_container(rings: &[Ring], container: &ContainerRegion, padding: f64) -> bool {
    for ring in rings {
        for point in &ring.points {
            if !point_in_region(*point, &container.rings) {
                return false;
            }
            if padding > EPS && point_to_rings_distance(*point, &container.rings) < padding - EPS {
                return false;
            }
        }
        if !segments_clear_container(ring, &container.rings, padding) {
            return false;
        }
    }
    for boundary in &container.rings {
        for point in &boundary.points {
            if rings
                .iter()
                .any(|ring| point_in_ring_strict(*point, ring) && !point_on_ring(*point, ring))
            {
                return false;
            }
        }
    }
    true
}

fn segments_clear_container(ring: &Ring, container_rings: &[Ring], padding: f64) -> bool {
    for i in 0..ring.points.len() {
        let a = ring.points[i];
        let b = ring.points[(i + 1) % ring.points.len()];
        if container_rings
            .iter()
            .any(|container| segment_intersects_ring(a, b, container))
        {
            return false;
        }
        if padding > EPS && segment_to_rings_distance(a, b, container_rings) < padding - EPS {
            return false;
        }
    }
    true
}

fn rings_collide(
    a_rings: &[Ring],
    a_bounds: Bounds,
    b_rings: &[Ring],
    b_bounds: Bounds,
    padding: f64,
) -> bool {
    if !bounds_overlap(a_bounds, b_bounds, padding) {
        return false;
    }
    for a in a_rings {
        for b in b_rings {
            if rings_overlap(a, b, padding) {
                return true;
            }
        }
    }
    false
}

fn rings_overlap(a: &Ring, b: &Ring, padding: f64) -> bool {
    for i in 0..a.points.len() {
        let a1 = a.points[i];
        let a2 = a.points[(i + 1) % a.points.len()];
        for j in 0..b.points.len() {
            let b1 = b.points[j];
            let b2 = b.points[(j + 1) % b.points.len()];
            if segments_properly_intersect(a1, a2, b1, b2) {
                return true;
            }
            if padding > EPS && segment_distance(a1, a2, b1, b2) < padding - EPS {
                return true;
            }
        }
    }
    a.points.iter().any(|point| point_in_ring_strict(*point, b))
        || b.points.iter().any(|point| point_in_ring_strict(*point, a))
}

fn segment_intersects_ring(a: Point2D, b: Point2D, ring: &Ring) -> bool {
    for i in 0..ring.points.len() {
        let c = ring.points[i];
        let d = ring.points[(i + 1) % ring.points.len()];
        if segments_properly_intersect(a, b, c, d) {
            return true;
        }
    }
    false
}

fn point_in_region(point: Point2D, rings: &[Ring]) -> bool {
    if rings.iter().any(|ring| point_on_ring(point, ring)) {
        return true;
    }
    rings
        .iter()
        .filter(|ring| point_in_ring_strict(point, ring))
        .count()
        % 2
        == 1
}

fn point_in_ring_strict(point: Point2D, ring: &Ring) -> bool {
    let mut inside = false;
    let mut j = ring.points.len() - 1;
    for i in 0..ring.points.len() {
        let pi = ring.points[i];
        let pj = ring.points[j];
        if (pi.y > point.y) != (pj.y > point.y) {
            let denom = pj.y - pi.y;
            if denom.abs() > EPS && point.x < (pj.x - pi.x) * (point.y - pi.y) / denom + pi.x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

fn point_to_ring_distance(point: Point2D, ring: &Ring) -> f64 {
    let mut best = f64::INFINITY;
    for i in 0..ring.points.len() {
        best = best.min(point_segment_distance(
            point,
            ring.points[i],
            ring.points[(i + 1) % ring.points.len()],
        ));
    }
    best
}

fn point_to_rings_distance(point: Point2D, rings: &[Ring]) -> f64 {
    rings
        .iter()
        .map(|ring| point_to_ring_distance(point, ring))
        .fold(f64::INFINITY, f64::min)
}

fn segment_to_ring_distance(a: Point2D, b: Point2D, ring: &Ring) -> f64 {
    let mut best = f64::INFINITY;
    for i in 0..ring.points.len() {
        best = best.min(segment_distance(
            a,
            b,
            ring.points[i],
            ring.points[(i + 1) % ring.points.len()],
        ));
    }
    best
}

fn segment_to_rings_distance(a: Point2D, b: Point2D, rings: &[Ring]) -> f64 {
    rings
        .iter()
        .map(|ring| segment_to_ring_distance(a, b, ring))
        .fold(f64::INFINITY, f64::min)
}

fn segment_distance(a: Point2D, b: Point2D, c: Point2D, d: Point2D) -> f64 {
    if segments_properly_intersect(a, b, c, d) {
        return 0.0;
    }
    point_segment_distance(a, c, d)
        .min(point_segment_distance(b, c, d))
        .min(point_segment_distance(c, a, b))
        .min(point_segment_distance(d, a, b))
}

fn point_segment_distance(p: Point2D, a: Point2D, b: Point2D) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq <= EPS {
        return ((p.x - a.x).powi(2) + (p.y - a.y).powi(2)).sqrt();
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq).clamp(0.0, 1.0);
    let proj = Point2D::new(a.x + t * dx, a.y + t * dy);
    ((p.x - proj.x).powi(2) + (p.y - proj.y).powi(2)).sqrt()
}

fn segments_properly_intersect(a: Point2D, b: Point2D, c: Point2D, d: Point2D) -> bool {
    let o1 = orientation(a, b, c);
    let o2 = orientation(a, b, d);
    let o3 = orientation(c, d, a);
    let o4 = orientation(c, d, b);
    o1 * o2 < -EPS && o3 * o4 < -EPS
}

fn point_on_segment(p: Point2D, a: Point2D, b: Point2D) -> bool {
    orientation(a, b, p).abs() <= EPS
        && p.x >= a.x.min(b.x) - EPS
        && p.x <= a.x.max(b.x) + EPS
        && p.y >= a.y.min(b.y) - EPS
        && p.y <= a.y.max(b.y) + EPS
}

fn point_on_ring(point: Point2D, ring: &Ring) -> bool {
    ring.points
        .iter()
        .enumerate()
        .any(|(idx, a)| point_on_segment(point, *a, ring.points[(idx + 1) % ring.points.len()]))
}

fn orientation(a: Point2D, b: Point2D, c: Point2D) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn dedup_ids(ids: &mut Vec<ObjectId>) {
    let mut seen = HashSet::new();
    ids.retain(|id| seen.insert(*id));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ServiceContext;
    use beambench_common::barcode::{BarcodeOptions, BarcodeType};
    use beambench_common::path::VecPath;
    use beambench_common::{Bounds, Point2D, Transform2D};
    use beambench_core::vector::convert::object_to_world_vecpath_resolved;
    use beambench_core::{Project, ShapeKind, TextAlignment, TextAlignmentV, TextLayoutMode};

    fn sample_context_with_project(project: Project) -> ServiceContext {
        let ctx = ServiceContext::new();
        let mut project = project;
        project.dirty = false;
        *ctx.project.lock().unwrap() = Some(project);
        ctx
    }

    fn rect_obj(
        project: &mut Project,
        name: &str,
        min_x: f64,
        min_y: f64,
        width: f64,
        height: f64,
    ) -> ObjectId {
        let layer_id = project.ensure_default_layer();
        let object = ProjectObject::new(
            name,
            layer_id,
            Bounds::new(
                Point2D::new(min_x, min_y),
                Point2D::new(min_x + width, min_y + height),
            ),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width,
                height,
                corner_radius: 0.0,
            },
        );
        let id = object.id;
        project.add_object(object);
        id
    }

    fn builtin_obj(
        project: &mut Project,
        name: &str,
        bounds: Bounds,
        data: ObjectData,
    ) -> ObjectId {
        let layer_id = project.ensure_default_layer();
        let object = ProjectObject::new(name, layer_id, bounds, data);
        let id = object.id;
        project.add_object(object);
        id
    }

    fn vector_path_obj(
        project: &mut Project,
        name: &str,
        path_data: String,
        bounds: Bounds,
        closed: bool,
    ) -> ObjectId {
        let layer_id = project.ensure_default_layer();
        let object = ProjectObject::new(
            name,
            layer_id,
            bounds,
            ObjectData::VectorPath {
                path_data,
                closed,
                ruler_guide_axis: None,
            },
        );
        let id = object.id;
        project.add_object(object);
        id
    }

    fn world_vector_path_obj(project: &mut Project, name: &str, path_data: String) -> ObjectId {
        let bounds = VecPath::parse_svg_d(&path_data)
            .visual_bounds()
            .expect("test path should have bounds");
        vector_path_obj(project, name, path_data, bounds, true)
    }

    fn rect_path(min_x: f64, min_y: f64, width: f64, height: f64) -> String {
        format!(
            "M{min_x} {min_y} L{} {min_y} L{} {} L{min_x} {} Z",
            min_x + width,
            min_x + width,
            min_y + height,
            min_y + height
        )
    }

    fn star_path(cx: f64, cy: f64, outer: f64, inner: f64, start_angle: f64) -> String {
        let mut path = String::new();
        for i in 0..10 {
            let radius = if i % 2 == 0 { outer } else { inner };
            let angle = start_angle + i as f64 * std::f64::consts::PI / 5.0;
            let x = cx + radius * angle.cos();
            let y = cy + radius * angle.sin();
            if i == 0 {
                path.push_str(&format!("M{x} {y}"));
            } else {
                path.push_str(&format!(" L{x} {y}"));
            }
        }
        path.push_str(" Z");
        path
    }

    fn regular_polygon_path(
        cx: f64,
        cy: f64,
        radius: f64,
        sides: usize,
        start_angle: f64,
    ) -> String {
        let mut path = String::new();
        for i in 0..sides {
            let angle = start_angle + i as f64 * std::f64::consts::TAU / sides as f64;
            let x = cx + radius * angle.cos();
            let y = cy + radius * angle.sin();
            if i == 0 {
                path.push_str(&format!("M{x} {y}"));
            } else {
                path.push_str(&format!(" L{x} {y}"));
            }
        }
        path.push_str(" Z");
        path
    }

    fn bounds_overlap_strict(a: Bounds, b: Bounds) -> bool {
        a.min.x < b.max.x - EPS
            && a.max.x > b.min.x + EPS
            && a.min.y < b.max.y - EPS
            && a.max.y > b.min.y + EPS
    }

    fn test_rect_ring(min_x: f64, min_y: f64, width: f64, height: f64) -> Ring {
        Ring {
            points: vec![
                Point2D::new(min_x, min_y),
                Point2D::new(min_x + width, min_y),
                Point2D::new(min_x + width, min_y + height),
                Point2D::new(min_x, min_y + height),
            ],
        }
    }

    fn test_l_ring(min_x: f64, min_y: f64, width: f64, height: f64, notch: f64) -> Ring {
        Ring {
            points: vec![
                Point2D::new(min_x, min_y),
                Point2D::new(min_x + width, min_y),
                Point2D::new(min_x + width, min_y + notch),
                Point2D::new(min_x + notch, min_y + notch),
                Point2D::new(min_x + notch, min_y + height),
                Point2D::new(min_x, min_y + height),
            ],
        }
    }

    fn test_part(min_x: f64, min_y: f64, width: f64, height: f64) -> NestPart {
        let ring = test_rect_ring(min_x, min_y, width, height);
        let bounds = ring_bounds(&ring).unwrap();
        NestPart {
            root_ids: vec![ObjectId::new()],
            member_ids: vec![ObjectId::new()],
            rings: vec![ring],
            bounds,
            pivot: bounds_center(&bounds),
            area: width * height,
            point_count: 4,
        }
    }

    #[test]
    fn u_nesting_adapter_handles_simple_single_region_parts() {
        let container_ring = test_rect_ring(100.0, 100.0, 90.0, 50.0);
        let container = ContainerRegion {
            rings: vec![container_ring],
            bounds: Bounds::new(Point2D::new(100.0, 100.0), Point2D::new(190.0, 150.0)),
            area: 90.0 * 50.0,
        };
        let parts = vec![
            test_part(220.0, 100.0, 20.0, 10.0),
            test_part(250.0, 100.0, 18.0, 12.0),
        ];

        let layout =
            compute_layout_with_u_nesting(&container, &parts, 0.0, false, false, 15.0, None)
                .expect("adapter should not error")
                .expect("simple rectangles should use U-Nesting");

        assert_eq!(layout.placements.len(), 2);
        assert!(layout.unplaced.is_empty());
        for placement in &layout.placements {
            let rings = transform_rings(
                &parts
                    .iter()
                    .find(|part| part.root_ids == placement.root_ids)
                    .unwrap()
                    .rings,
                placement.pivot,
                placement.rotation_deg,
                placement.mirrored,
                placement.offset,
            );
            let bounds = rings_bounds(&rings).unwrap();
            assert!(bounds_within(bounds, container.bounds, 0.0));
            assert!(rings_inside_container(&rings, &container, 0.0));
        }
    }

    #[test]
    fn u_nesting_adapter_builds_conservative_shape_for_multi_exterior_parts() {
        let left = test_rect_ring(0.0, 0.0, 10.0, 10.0);
        let right = test_rect_ring(20.0, 0.0, 10.0, 10.0);

        let shape = rings_to_engine_shape(&[left, right], Point2D::zero())
            .expect("multi-exterior compound part should still reach the engine");

        assert!(shape.holes.is_empty());
        assert!(shape.exterior.iter().any(|point| point.0 <= EPS));
        assert!(shape.exterior.iter().any(|point| point.0 >= 30.0 - EPS));
        assert!(shape.exterior.iter().any(|point| point.1 <= EPS));
        assert!(shape.exterior.iter().any(|point| point.1 >= 10.0 - EPS));
    }

    #[test]
    fn mirrored_legacy_candidate_is_applied_when_only_mirror_fits() {
        let container_ring = test_rect_ring(0.0, 0.0, 10.0, 10.0);
        let container = ContainerRegion {
            rings: vec![container_ring],
            bounds: Bounds::new(Point2D::zero(), Point2D::new(10.0, 10.0)),
            area: 100.0,
        };
        let part_ring = test_l_ring(0.0, 0.0, 10.0, 10.0, 4.0);
        let part_bounds = ring_bounds(&part_ring).unwrap();
        let part = NestPart {
            root_ids: vec![ObjectId::new()],
            member_ids: vec![ObjectId::new()],
            rings: vec![part_ring],
            bounds: part_bounds,
            pivot: bounds_center(&part_bounds),
            area: 64.0,
            point_count: 6,
        };
        let blocker_ring = test_rect_ring(0.0, 5.0, 4.0, 5.0);
        let blocker_bounds = ring_bounds(&blocker_ring).unwrap();
        let blocker = PlacedPart {
            rings: vec![blocker_ring],
            bounds: blocker_bounds,
            rotation_deg: 0.0,
            mirrored: false,
        };

        let without_mirror = find_part_placement(
            &container,
            container.bounds,
            &part,
            std::slice::from_ref(&blocker),
            0.0,
            false,
            false,
            15.0,
            None,
        )
        .unwrap();
        assert!(without_mirror.is_none());

        let with_mirror = find_part_placement(
            &container,
            container.bounds,
            &part,
            &[blocker],
            0.0,
            false,
            true,
            15.0,
            None,
        )
        .unwrap()
        .expect("mirrored L-shape should avoid the blocker");
        assert!(with_mirror.0.mirrored);
    }

    #[test]
    fn mirrored_placement_transform_mirrors_before_rotation() {
        let placement = Placement {
            root_ids: vec![ObjectId::new()],
            member_ids: vec![ObjectId::new()],
            offset: Point2D::zero(),
            rotation_deg: 90.0,
            pivot: Point2D::new(10.0, 5.0),
            mirrored: true,
        };

        let transformed = placement_world_transform(&placement)
            .unwrap()
            .apply(&Point2D::new(8.0, 5.0));

        assert!((transformed.x - 10.0).abs() < EPS, "{transformed:?}");
        assert!((transformed.y - 7.0).abs() < EPS, "{transformed:?}");
    }

    #[test]
    fn mirrored_rotated_placement_applies_to_object_geometry() {
        let mut project = Project::new("nest");
        let part = world_vector_path_obj(
            &mut project,
            "asymmetric",
            "M0 0 L12 0 L12 4 L5 4 L5 10 L0 10 Z".to_string(),
        );
        let before = object_rings(&project, project.find_object(part).unwrap()).unwrap();
        let pivot = Point2D::new(6.0, 5.0);
        let offset = Point2D::new(20.0, 30.0);
        let rotation_deg = 30.0;
        let placement = Placement {
            root_ids: vec![part],
            member_ids: vec![part],
            offset,
            rotation_deg,
            pivot,
            mirrored: true,
        };

        apply_placements(&mut project, &[placement]).unwrap();

        let after = object_rings(&project, project.find_object(part).unwrap()).unwrap();
        assert_eq!(after.len(), before.len());
        for (actual_ring, before_ring) in after.iter().zip(before.iter()) {
            assert_eq!(actual_ring.points.len(), before_ring.points.len());
            for (actual, original) in actual_ring.points.iter().zip(before_ring.points.iter()) {
                let mirrored = Point2D::new((2.0 * pivot.x) - original.x, original.y);
                let rotated = rotate_point(mirrored, pivot, rotation_deg);
                let expected = Point2D::new(rotated.x + offset.x, rotated.y + offset.y);
                assert!(
                    (actual.x - expected.x).abs() < 1e-6 && (actual.y - expected.y).abs() < 1e-6,
                    "actual={actual:?}, expected={expected:?}"
                );
            }
        }
    }

    #[test]
    fn unlimited_deadline_maps_to_u_nesting_zero_time_limit() {
        assert_eq!(remaining_time_limit_ms(None), 0);
    }

    #[test]
    fn rejects_single_container_selection_without_mutation() {
        let mut project = Project::new("nest");
        let container = rect_obj(&mut project, "container", 0.0, 0.0, 100.0, 100.0);
        let ctx = sample_context_with_project(project);

        let err = nest_selected(&ctx, vec![container], NestOptions::default()).unwrap_err();
        assert!(err.message.contains("at least one object"));
        assert!(!ctx.project.lock().unwrap().as_ref().unwrap().dirty);
    }

    #[test]
    fn largest_closed_shape_is_container_and_parts_move_inside_it() {
        let mut project = Project::new("nest");
        let small_container = rect_obj(&mut project, "small", 0.0, 0.0, 20.0, 20.0);
        let large_container = rect_obj(&mut project, "large", 0.0, 0.0, 100.0, 100.0);
        let part = rect_obj(&mut project, "part", 150.0, 0.0, 10.0, 10.0);
        let ctx = sample_context_with_project(project);

        let result = nest_selected(
            &ctx,
            vec![small_container, large_container, part],
            NestOptions {
                allow_rotation: false,
                ..NestOptions::default()
            },
        )
        .unwrap();

        assert_eq!(result.target_container_id, large_container);
        assert!(result.placed_object_ids.contains(&small_container));
        assert!(result.placed_object_ids.contains(&part));
        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        let nested = member_visual_bounds(project, &[part]).unwrap();
        assert!(nested.min.x >= -EPS, "{nested:?}");
        assert!(nested.max.x <= 100.0 + EPS, "{nested:?}");
        assert!(nested.min.y >= -EPS, "{nested:?}");
        assert!(nested.max.y <= 100.0 + EPS, "{nested:?}");
    }

    #[test]
    fn container_holes_are_excluded_from_placement() {
        let mut project = Project::new("nest");
        let frame_path = format!(
            "{} {}",
            rect_path(0.0, 0.0, 100.0, 100.0),
            rect_path(10.0, 10.0, 30.0, 30.0)
        );
        let container = vector_path_obj(
            &mut project,
            "frame",
            frame_path,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 100.0)),
            true,
        );
        let part = rect_obj(&mut project, "part", 150.0, 0.0, 20.0, 20.0);
        let ctx = sample_context_with_project(project);

        let result = nest_selected(
            &ctx,
            vec![container, part],
            NestOptions {
                allow_rotation: false,
                ..NestOptions::default()
            },
        )
        .unwrap();

        assert!(result.placed_object_ids.contains(&part));
        let guard = ctx.project.lock().unwrap();
        let nested = guard.as_ref().unwrap().find_object(part).unwrap();
        let hole = Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(40.0, 40.0));
        assert!(
            !bounds_overlap_strict(nested.bounds, hole),
            "nested part was placed into the container cutout: {:?}",
            nested.bounds
        );
    }

    #[test]
    fn allow_rotation_false_rejects_without_partial_mutation_when_a_part_will_not_fit() {
        let mut locked_project = Project::new("nest");
        let container = rect_obj(&mut locked_project, "container", 0.0, 0.0, 30.0, 60.0);
        let rotation_required = rect_obj(&mut locked_project, "wide", 100.0, 0.0, 40.0, 20.0);
        let fits_without_rotation = rect_obj(&mut locked_project, "small", 150.0, 0.0, 10.0, 10.0);
        let locked_ctx = sample_context_with_project(locked_project);

        let err = nest_selected(
            &locked_ctx,
            vec![container, rotation_required, fits_without_rotation],
            NestOptions {
                allow_rotation: false,
                ..NestOptions::default()
            },
        )
        .unwrap_err();
        assert!(err.message.contains("could not fit 1 object"));
        let guard = locked_ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        assert!(!project.dirty);
        assert_eq!(
            project.find_object(rotation_required).unwrap().bounds.min.x,
            100.0
        );
        assert_eq!(
            project
                .find_object(fits_without_rotation)
                .unwrap()
                .bounds
                .min
                .x,
            150.0
        );
        drop(guard);
        assert!(!locked_ctx.undo_state().unwrap().can_undo);

        let mut rotatable_project = Project::new("nest");
        let container = rect_obj(&mut rotatable_project, "container", 0.0, 0.0, 30.0, 60.0);
        let rotation_required = rect_obj(&mut rotatable_project, "wide", 100.0, 0.0, 40.0, 20.0);
        let fits_without_rotation =
            rect_obj(&mut rotatable_project, "small", 150.0, 0.0, 10.0, 10.0);
        let rotatable_ctx = sample_context_with_project(rotatable_project);

        let rotatable_result = nest_selected(
            &rotatable_ctx,
            vec![container, rotation_required, fits_without_rotation],
            NestOptions::default(),
        )
        .unwrap();
        assert_eq!(rotatable_result.unplaced_object_ids, Vec::<ObjectId>::new());
        assert!(
            rotatable_result
                .placed_object_ids
                .contains(&rotation_required)
        );
        assert!(
            rotatable_result
                .placed_object_ids
                .contains(&fits_without_rotation)
        );
    }

    #[test]
    fn legacy_fallback_uses_configured_non_cardinal_rotation_step() {
        let container = ContainerRegion {
            rings: vec![test_rect_ring(0.0, 0.0, 30.0, 30.0)],
            bounds: Bounds::new(Point2D::zero(), Point2D::new(30.0, 30.0)),
            area: 900.0,
        };
        let part_ring = test_rect_ring(100.0, 0.0, 36.0, 5.0);
        let part_bounds = ring_bounds(&part_ring).unwrap();
        let part = NestPart {
            root_ids: vec![ObjectId::new()],
            member_ids: vec![ObjectId::new()],
            rings: vec![part_ring],
            bounds: part_bounds,
            pivot: bounds_center(&part_bounds),
            area: 180.0,
            point_count: 4,
        };

        let cardinal_only = compute_layout_legacy(
            &container,
            std::slice::from_ref(&part),
            0.0,
            true,
            false,
            90.0,
            None,
        )
        .unwrap();
        assert_eq!(cardinal_only.placements.len(), 0);
        assert_eq!(cardinal_only.unplaced.len(), 1);

        let diagonal_allowed =
            compute_layout_legacy(&container, &[part], 0.0, true, false, 15.0, None).unwrap();
        assert_eq!(diagonal_allowed.placements.len(), 1);
        assert!(diagonal_allowed.unplaced.is_empty());
        let rotation = diagonal_allowed.placements[0]
            .rotation_deg
            .rem_euclid(180.0);
        assert!(
            rotation > 0.0 + EPS && (rotation - 90.0).abs() > EPS,
            "expected a non-cardinal rotation, got {}",
            diagonal_allowed.placements[0].rotation_deg
        );
    }

    #[test]
    fn rotated_non_square_part_visual_bounds_fit_after_apply() {
        let mut project = Project::new("nest");
        let container = rect_obj(&mut project, "container", 0.0, 0.0, 30.0, 60.0);
        let part = rect_obj(&mut project, "wide", 100.0, 0.0, 40.0, 20.0);
        let ctx = sample_context_with_project(project);

        let result = nest_selected(&ctx, vec![container, part], NestOptions::default()).unwrap();

        assert_eq!(result.placed_object_ids, vec![part]);
        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        let part_obj = project.find_object(part).unwrap();
        assert!(
            part_obj.transform.b.abs() > 0.9,
            "part was expected to rotate 90 degrees"
        );
        let visual = member_visual_bounds(project, &[part]).unwrap();
        assert!(visual.min.x >= -EPS, "{visual:?}");
        assert!(visual.min.y >= -EPS, "{visual:?}");
        assert!(visual.max.x <= 30.0 + EPS, "{visual:?}");
        assert!(visual.max.y <= 60.0 + EPS, "{visual:?}");
        assert!((visual.width() - 20.0).abs() < 1e-5, "{visual:?}");
        assert!((visual.height() - 40.0).abs() < 1e-5, "{visual:?}");
    }

    #[test]
    fn group_parts_move_rigidly() {
        let mut project = Project::new("nest");
        let container = rect_obj(&mut project, "container", 0.0, 0.0, 100.0, 100.0);
        let child_a = rect_obj(&mut project, "child-a", 150.0, 0.0, 10.0, 10.0);
        let child_b = rect_obj(&mut project, "child-b", 165.0, 5.0, 10.0, 10.0);
        let layer_id = project.ensure_default_layer();
        let group = ProjectObject::new(
            "group",
            layer_id,
            Bounds::new(Point2D::new(150.0, 0.0), Point2D::new(175.0, 15.0)),
            ObjectData::Group {
                children: vec![child_a, child_b],
            },
        );
        let group_id = group.id;
        project.add_object(group);
        let ctx = sample_context_with_project(project);

        let result = nest_selected(
            &ctx,
            vec![container, group_id],
            NestOptions {
                allow_rotation: false,
                ..NestOptions::default()
            },
        )
        .unwrap();

        assert_eq!(result.placed_object_ids, vec![group_id]);
        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        let a = project.find_object(child_a).unwrap();
        let b = project.find_object(child_b).unwrap();
        assert!((b.bounds.min.x - a.bounds.min.x - 15.0).abs() < EPS);
        assert!((b.bounds.min.y - a.bounds.min.y - 5.0).abs() < EPS);
    }

    #[test]
    fn virtual_clone_parts_resolve_to_source_geometry() {
        let mut project = Project::new("nest");
        let container = rect_obj(&mut project, "container", 0.0, 0.0, 100.0, 100.0);
        let source = rect_obj(&mut project, "source", 150.0, 0.0, 10.0, 10.0);
        let layer_id = project.ensure_default_layer();
        let clone = ProjectObject::new(
            "clone",
            layer_id,
            Bounds::new(Point2D::new(180.0, 0.0), Point2D::new(190.0, 10.0)),
            ObjectData::VirtualClone { source_id: source },
        );
        let clone_id = clone.id;
        project.add_object(clone);
        let ctx = sample_context_with_project(project);

        let result = nest_selected(
            &ctx,
            vec![container, clone_id],
            NestOptions {
                allow_rotation: false,
                ..NestOptions::default()
            },
        )
        .unwrap();

        assert_eq!(result.placed_object_ids, vec![clone_id]);
        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        let nested = member_visual_bounds(project, &[clone_id]).unwrap();
        assert!(nested.max.x <= 100.0 + EPS, "{nested:?}");
        assert!(nested.max.y <= 100.0 + EPS, "{nested:?}");
    }

    #[test]
    fn placed_parts_do_not_overlap_and_respect_padding() {
        let mut project = Project::new("nest");
        let container = rect_obj(&mut project, "container", 0.0, 0.0, 60.0, 30.0);
        let part_a = rect_obj(&mut project, "part-a", 100.0, 0.0, 20.0, 20.0);
        let part_b = rect_obj(&mut project, "part-b", 130.0, 0.0, 20.0, 20.0);
        let ctx = sample_context_with_project(project);

        let result = nest_selected(
            &ctx,
            vec![container, part_a, part_b],
            NestOptions {
                padding_mm: 5.0,
                allow_rotation: false,
                ..NestOptions::default()
            },
        )
        .unwrap();

        assert_eq!(result.unplaced_object_ids, Vec::<ObjectId>::new());
        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        let a = member_visual_bounds(project, &[part_a]).unwrap();
        let b = member_visual_bounds(project, &[part_b]).unwrap();
        assert!(a.min.x >= 5.0 - EPS, "{a:?}");
        assert!(a.min.y >= 5.0 - EPS, "{a:?}");
        assert!(a.max.x <= 55.0 + EPS, "{a:?}");
        assert!(a.max.y <= 25.0 + EPS, "{a:?}");
        assert!(b.min.x >= 5.0 - EPS, "{b:?}");
        assert!(b.min.y >= 5.0 - EPS, "{b:?}");
        assert!(b.max.x <= 55.0 + EPS, "{b:?}");
        assert!(b.max.y <= 25.0 + EPS, "{b:?}");
        assert!(
            (b.min.x - a.max.x).abs() >= 5.0 - EPS
                || (a.min.x - b.max.x).abs() >= 5.0 - EPS
                || (b.min.y - a.max.y).abs() >= 5.0 - EPS
                || (a.min.y - b.max.y).abs() >= 5.0 - EPS,
            "{a:?} {b:?}"
        );
    }

    #[test]
    fn tight_rectangle_does_not_clip_star_outline() {
        let mut project = Project::new("nest");
        let container = rect_obj(&mut project, "container", 0.0, 0.0, 22.0, 22.0);
        let star = vector_path_obj(
            &mut project,
            "star",
            star_path(100.0, 100.0, 10.0, 4.0, -std::f64::consts::FRAC_PI_2),
            Bounds::new(Point2D::new(90.0, 90.0), Point2D::new(110.0, 110.0)),
            true,
        );
        let before_area: f64 = object_rings(&project, project.find_object(star).unwrap())
            .unwrap()
            .iter()
            .map(ring_area_abs)
            .sum();
        let ctx = sample_context_with_project(project);

        let result = nest_selected(
            &ctx,
            vec![container, star],
            NestOptions {
                allow_rotation: false,
                ..NestOptions::default()
            },
        )
        .unwrap();

        assert_eq!(result.placed_object_ids, vec![star]);
        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        let container_region = find_largest_container(project, &[container])
            .unwrap()
            .region;
        let rings = object_rings(project, project.find_object(star).unwrap()).unwrap();
        let bounds = rings_bounds(&rings).unwrap();
        let after_area: f64 = rings.iter().map(ring_area_abs).sum();
        assert!(
            bounds_within(bounds, container_region.bounds, 0.0),
            "{bounds:?}"
        );
        assert!(rings_inside_container(&rings, &container_region, 0.0));
        assert!(
            (after_area - before_area).abs() < 1e-6,
            "star outline area changed, before={before_area}, after={after_area}"
        );
    }

    #[test]
    fn double_star_stress_parts_do_not_overlap_or_spill_out() {
        // Correctness stress fixture. It intentionally raises the test budget
        // above the production default so the assertion is about placement
        // quality, not deadline behavior.
        let mut project = Project::new("nest");
        let container = rect_obj(&mut project, "container", 10.0, 10.0, 175.0, 135.0);
        let mut selected = vec![container];
        for idx in 0..16 {
            let row = idx / 8;
            let col = idx % 8;
            let cx = 230.0 + col as f64 * 28.0;
            let cy = 25.0 + row as f64 * 32.0;
            let start = -std::f64::consts::FRAC_PI_2 + (idx % 4) as f64 * 0.08;
            selected.push(vector_path_obj(
                &mut project,
                &format!("double-star-{idx}-outer"),
                star_path(cx, cy, 10.5, 4.6, start),
                Bounds::new(
                    Point2D::new(cx - 10.5, cy - 10.5),
                    Point2D::new(cx + 10.5, cy + 10.5),
                ),
                true,
            ));
            selected.push(vector_path_obj(
                &mut project,
                &format!("double-star-{idx}-inner"),
                star_path(
                    cx + 2.7,
                    cy + 1.9,
                    4.4,
                    1.9,
                    -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI / 5.0,
                ),
                Bounds::new(
                    Point2D::new(cx - 1.7, cy - 2.5),
                    Point2D::new(cx + 7.1, cy + 6.3),
                ),
                true,
            ));
        }
        let ctx = sample_context_with_project(project);

        let result = nest_selected(
            &ctx,
            selected,
            NestOptions {
                time_limit_ms: 180_000,
                ..NestOptions::default()
            },
        )
        .unwrap();

        assert_eq!(result.placed_object_ids.len(), 32);
        assert!(result.unplaced_object_ids.is_empty());
        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        let container_region = find_largest_container(project, &[container])
            .unwrap()
            .region;
        let mut placed = Vec::new();
        for object_id in result.placed_object_ids {
            let object = project.find_object(object_id).unwrap();
            let rings = object_rings(project, object).unwrap();
            let bounds = rings_bounds(&rings).unwrap();
            assert!(
                bounds_within(bounds, container_region.bounds, 0.0),
                "{object_id} {bounds:?}"
            );
            assert!(
                rings_inside_container(&rings, &container_region, 0.0),
                "{object_id} spilled outside the container"
            );
            placed.push((object_id, rings, bounds));
        }
        for i in 0..placed.len() {
            for j in (i + 1)..placed.len() {
                let (a_id, a_rings, a_bounds) = &placed[i];
                let (b_id, b_rings, b_bounds) = &placed[j];
                assert!(
                    !rings_collide(a_rings, *a_bounds, b_rings, *b_bounds, 0.0),
                    "{a_id} overlapped {b_id}"
                );
            }
        }
    }

    #[test]
    fn mixed_shapes_that_fit_are_all_placed_inside_container() {
        let mut project = Project::new("nest");
        let container = rect_obj(&mut project, "container", 0.0, 0.0, 160.0, 95.0);
        let mut selected = vec![container];
        let shapes = [
            (
                "circle",
                regular_polygon_path(230.0, 20.0, 12.0, 32, 0.0),
                Bounds::new(Point2D::new(218.0, 8.0), Point2D::new(242.0, 32.0)),
            ),
            (
                "hex",
                regular_polygon_path(270.0, 20.0, 13.0, 6, std::f64::consts::PI / 6.0),
                Bounds::new(Point2D::new(257.0, 7.0), Point2D::new(283.0, 33.0)),
            ),
            (
                "triangle",
                regular_polygon_path(310.0, 20.0, 15.0, 3, -std::f64::consts::FRAC_PI_2),
                Bounds::new(Point2D::new(295.0, 5.0), Point2D::new(325.0, 35.0)),
            ),
            (
                "star-large",
                star_path(350.0, 20.0, 14.0, 6.0, -std::f64::consts::FRAC_PI_2),
                Bounds::new(Point2D::new(336.0, 6.0), Point2D::new(364.0, 34.0)),
            ),
            (
                "star-small-a",
                star_path(230.0, 60.0, 9.0, 3.8, -std::f64::consts::FRAC_PI_2),
                Bounds::new(Point2D::new(221.0, 51.0), Point2D::new(239.0, 69.0)),
            ),
            (
                "star-small-b",
                star_path(270.0, 60.0, 9.0, 3.8, -std::f64::consts::FRAC_PI_2),
                Bounds::new(Point2D::new(261.0, 51.0), Point2D::new(279.0, 69.0)),
            ),
            (
                "star-small-c",
                star_path(310.0, 60.0, 9.0, 3.8, -std::f64::consts::FRAC_PI_2),
                Bounds::new(Point2D::new(301.0, 51.0), Point2D::new(319.0, 69.0)),
            ),
            (
                "star-small-d",
                star_path(350.0, 60.0, 9.0, 3.8, -std::f64::consts::FRAC_PI_2),
                Bounds::new(Point2D::new(341.0, 51.0), Point2D::new(359.0, 69.0)),
            ),
        ];
        for (name, path, bounds) in shapes {
            selected.push(vector_path_obj(&mut project, name, path, bounds, true));
        }
        let expected: HashSet<ObjectId> = selected.iter().copied().skip(1).collect();
        let ctx = sample_context_with_project(project);

        let result = nest_selected(
            &ctx,
            selected,
            NestOptions {
                time_limit_ms: 60_000,
                ..NestOptions::default()
            },
        )
        .unwrap();

        assert!(result.unplaced_object_ids.is_empty());
        let placed: HashSet<ObjectId> = result.placed_object_ids.iter().copied().collect();
        assert_eq!(placed, expected);
        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        let container_region = find_largest_container(project, &[container])
            .unwrap()
            .region;
        let mut placed_rings = Vec::new();
        for object_id in result.placed_object_ids {
            let object = project.find_object(object_id).unwrap();
            let rings = object_rings(project, object).unwrap();
            let bounds = rings_bounds(&rings).unwrap();
            assert!(
                rings_inside_container(&rings, &container_region, 0.0),
                "{object_id} spilled outside the container"
            );
            placed_rings.push((object_id, rings, bounds));
        }
        for i in 0..placed_rings.len() {
            for j in (i + 1)..placed_rings.len() {
                let (a_id, a_rings, a_bounds) = &placed_rings[i];
                let (b_id, b_rings, b_bounds) = &placed_rings[j];
                assert!(
                    !rings_collide(a_rings, *a_bounds, b_rings, *b_bounds, 0.0),
                    "{a_id} overlapped {b_id}"
                );
            }
        }
    }

    #[test]
    fn builtin_on_canvas_shapes_all_move_inside_container() {
        let mut project = Project::new("nest");
        let container = rect_obj(&mut project, "container", 180.0, 120.0, 155.0, 95.0);
        let circle = builtin_obj(
            &mut project,
            "circle",
            Bounds::new(Point2D::new(20.0, 20.0), Point2D::new(48.0, 48.0)),
            ObjectData::Shape {
                kind: ShapeKind::Ellipse,
                width: 28.0,
                height: 28.0,
                corner_radius: 0.0,
            },
        );
        let hex = builtin_obj(
            &mut project,
            "hex",
            Bounds::new(Point2D::new(58.0, 18.0), Point2D::new(88.0, 48.0)),
            ObjectData::Polygon {
                sides: 6,
                radius: 15.0,
            },
        );
        let triangle = builtin_obj(
            &mut project,
            "triangle",
            Bounds::new(Point2D::new(100.0, 16.0), Point2D::new(132.0, 48.0)),
            ObjectData::Polygon {
                sides: 3,
                radius: 16.0,
            },
        );
        let star_large = builtin_obj(
            &mut project,
            "star-large",
            Bounds::new(Point2D::new(144.0, 14.0), Point2D::new(178.0, 48.0)),
            ObjectData::Star {
                points: 5,
                bulge: 0.0,
                ratio: 0.45,
                dual_radius: true,
                ratio2: Some(0.72),
                corner_radius: 0.0,
                corner_radii: Vec::new(),
            },
        );
        let star_a = builtin_obj(
            &mut project,
            "star-a",
            Bounds::new(Point2D::new(25.0, 70.0), Point2D::new(45.0, 90.0)),
            ObjectData::Star {
                points: 5,
                bulge: 0.0,
                ratio: 0.45,
                dual_radius: true,
                ratio2: Some(0.72),
                corner_radius: 0.0,
                corner_radii: Vec::new(),
            },
        );
        let star_b = builtin_obj(
            &mut project,
            "star-b",
            Bounds::new(Point2D::new(65.0, 70.0), Point2D::new(85.0, 90.0)),
            ObjectData::Star {
                points: 5,
                bulge: 0.0,
                ratio: 0.45,
                dual_radius: true,
                ratio2: Some(0.72),
                corner_radius: 0.0,
                corner_radii: Vec::new(),
            },
        );
        let selected = vec![container, circle, hex, triangle, star_large, star_a, star_b];
        let expected: HashSet<ObjectId> = selected.iter().copied().skip(1).collect();
        let ctx = sample_context_with_project(project);

        let result = nest_selected(
            &ctx,
            selected,
            NestOptions {
                time_limit_ms: 60_000,
                ..NestOptions::default()
            },
        )
        .unwrap();

        assert!(result.unplaced_object_ids.is_empty());
        assert_eq!(
            result
                .placed_object_ids
                .iter()
                .copied()
                .collect::<HashSet<_>>(),
            expected
        );
        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        let container_region = find_largest_container(project, &[container])
            .unwrap()
            .region;
        let mut placed = Vec::new();
        for object_id in result.placed_object_ids {
            let object = project.find_object(object_id).unwrap();
            let rings = object_rings(project, object).unwrap();
            let bounds = rings_bounds(&rings).unwrap();
            assert!(
                bounds_within(bounds, container_region.bounds, 0.0),
                "{object_id} visual bounds landed outside the container: {bounds:?}"
            );
            assert!(
                rings_inside_container(&rings, &container_region, 0.0),
                "{object_id} landed outside the container with bounds {bounds:?}"
            );
            placed.push((object_id, rings, bounds));
        }
        for i in 0..placed.len() {
            for j in (i + 1)..placed.len() {
                let (a_id, a_rings, a_bounds) = &placed[i];
                let (b_id, b_rings, b_bounds) = &placed[j];
                assert!(
                    !rings_collide(a_rings, *a_bounds, b_rings, *b_bounds, 0.0),
                    "{a_id} overlapped {b_id}"
                );
            }
        }
    }

    #[test]
    fn imported_break_apart_style_paths_all_move_inside_container() {
        let mut project = Project::new("nest");
        let selected = vec![
            world_vector_path_obj(
                &mut project,
                "container",
                "M 180 120 L 335 120 L 335 215 L 180 215 Z".to_string(),
            ),
            world_vector_path_obj(
                &mut project,
                "circle",
                "M 48 34 C 48 41.7319864972 41.7319864972 48 34 48 C 26.2680135028 48 20 41.7319864972 20 34 C 20 26.2680135028 26.2680135028 20 34 20 C 41.7319864972 20 48 26.2680135028 48 34 Z".to_string(),
            ),
            world_vector_path_obj(
                &mut project,
                "hex",
                regular_polygon_path(73.0, 33.0, 15.0, 6, std::f64::consts::PI / 6.0),
            ),
            world_vector_path_obj(
                &mut project,
                "triangle",
                regular_polygon_path(116.0, 32.0, 17.0, 3, -std::f64::consts::FRAC_PI_2),
            ),
            world_vector_path_obj(
                &mut project,
                "star-large",
                star_path(161.0, 31.0, 17.0, 7.0, -std::f64::consts::FRAC_PI_2),
            ),
            world_vector_path_obj(
                &mut project,
                "star-a",
                star_path(35.0, 80.0, 10.0, 4.0, -std::f64::consts::FRAC_PI_2),
            ),
            world_vector_path_obj(
                &mut project,
                "star-b",
                star_path(75.0, 80.0, 10.0, 4.0, -std::f64::consts::FRAC_PI_2),
            ),
            world_vector_path_obj(
                &mut project,
                "star-c",
                star_path(115.0, 80.0, 10.0, 4.0, -std::f64::consts::FRAC_PI_2),
            ),
            world_vector_path_obj(
                &mut project,
                "star-d",
                star_path(155.0, 80.0, 10.0, 4.0, -std::f64::consts::FRAC_PI_2),
            ),
        ];
        let container = selected[0];
        let expected: HashSet<ObjectId> = selected.iter().copied().skip(1).collect();
        let ctx = sample_context_with_project(project);

        let result = nest_selected(
            &ctx,
            selected,
            NestOptions {
                time_limit_ms: 60_000,
                ..NestOptions::default()
            },
        )
        .unwrap();

        assert!(result.unplaced_object_ids.is_empty());
        assert_eq!(
            result
                .placed_object_ids
                .iter()
                .copied()
                .collect::<HashSet<_>>(),
            expected
        );
        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        let container_region = find_largest_container(project, &[container])
            .unwrap()
            .region;
        for object_id in result.placed_object_ids {
            let object = project.find_object(object_id).unwrap();
            let rings = object_rings(project, object).unwrap();
            assert!(
                rings_inside_container(&rings, &container_region, 0.0),
                "{object_id} landed outside the container"
            );
        }
    }

    #[test]
    fn imported_svg_then_break_apart_paths_all_move_inside_container() {
        let mut project = Project::new("nest");
        let layer_id = project.ensure_default_layer();
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="380mm" height="240mm" viewBox="0 0 380 240">
  <path d="M 180 120 L 335 120 L 335 215 L 180 215 Z"/>
  <path d="M 48 34 C 48 41.7319864972 41.7319864972 48 34 48 C 26.2680135028 48 20 41.7319864972 20 34 C 20 26.2680135028 26.2680135028 20 34 20 C 41.7319864972 20 48 26.2680135028 48 34 Z"/>
  <path d="{}"/>
  <path d="{}"/>
  <path d="{}"/>
  <path d="{}"/>
  <path d="{}"/>
  <path d="{}"/>
  <path d="{}"/>
</svg>"#,
            regular_polygon_path(73.0, 33.0, 15.0, 6, std::f64::consts::PI / 6.0),
            regular_polygon_path(116.0, 32.0, 17.0, 3, -std::f64::consts::FRAC_PI_2),
            star_path(161.0, 31.0, 17.0, 7.0, -std::f64::consts::FRAC_PI_2),
            star_path(35.0, 80.0, 10.0, 4.0, -std::f64::consts::FRAC_PI_2),
            star_path(75.0, 80.0, 10.0, 4.0, -std::f64::consts::FRAC_PI_2),
            star_path(115.0, 80.0, 10.0, 4.0, -std::f64::consts::FRAC_PI_2),
            star_path(155.0, 80.0, 10.0, 4.0, -std::f64::consts::FRAC_PI_2),
        );
        let imported =
            beambench_core::import::import_svg(svg.as_bytes(), &mut project, layer_id).unwrap();
        assert_eq!(imported.len(), 1);
        let source = project.find_object(imported[0]).unwrap().clone();
        let path = object_to_world_vecpath_resolved(&source, &project).unwrap();
        let parts = beambench_core::vector::path_ops::break_apart(&path.to_svg_d());
        assert_eq!(parts.len(), 9);
        project.remove_object(source.id);
        let mut selected = Vec::new();
        for (idx, part) in parts.into_iter().enumerate() {
            selected.push(world_vector_path_obj(
                &mut project,
                &format!("imported-part-{idx}"),
                part,
            ));
        }
        let container = selected[0];
        let expected: HashSet<ObjectId> = selected.iter().copied().skip(1).collect();
        let ctx = sample_context_with_project(project);

        let result = nest_selected(
            &ctx,
            selected,
            NestOptions {
                time_limit_ms: 60_000,
                ..NestOptions::default()
            },
        )
        .unwrap();

        assert!(result.unplaced_object_ids.is_empty());
        assert_eq!(
            result
                .placed_object_ids
                .iter()
                .copied()
                .collect::<HashSet<_>>(),
            expected
        );
        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        let container_region = find_largest_container(project, &[container])
            .unwrap()
            .region;
        for object_id in result.placed_object_ids {
            let object = project.find_object(object_id).unwrap();
            let rings = object_rings(project, object).unwrap();
            assert!(
                rings_inside_container(&rings, &container_region, 0.0),
                "{object_id} landed outside the container"
            );
        }
    }

    #[test]
    fn locked_parts_reject_before_mutation() {
        let mut project = Project::new("nest");
        let container = rect_obj(&mut project, "container", 0.0, 0.0, 100.0, 100.0);
        let part = rect_obj(&mut project, "part", 150.0, 0.0, 10.0, 10.0);
        project.find_object_mut(part).unwrap().locked = true;
        let ctx = sample_context_with_project(project);

        let err = nest_selected(&ctx, vec![container, part], NestOptions::default()).unwrap_err();

        assert!(err.message.contains("visible and unlocked"));
        assert!(!ctx.project.lock().unwrap().as_ref().unwrap().dirty);
    }

    #[test]
    fn non_invertible_transform_rejects_before_undo_snapshot() {
        let mut project = Project::new("nest");
        let container = rect_obj(&mut project, "container", 0.0, 0.0, 100.0, 100.0);
        let part = rect_obj(&mut project, "part", 150.0, 0.0, 10.0, 10.0);
        project.find_object_mut(part).unwrap().transform = Transform2D::scale(1.0, 0.0);
        let ctx = sample_context_with_project(project);

        let err = nest_selected(&ctx, vec![container, part], NestOptions::default()).unwrap_err();

        assert!(err.message.contains("non-invertible transforms"));
        assert!(!ctx.project.lock().unwrap().as_ref().unwrap().dirty);
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn rejects_raster_parts_before_mutation() {
        let mut project = Project::new("nest");
        let container = rect_obj(&mut project, "container", 0.0, 0.0, 100.0, 100.0);
        let layer_id = project.ensure_default_layer();
        let raster = ProjectObject::new(
            "raster",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(20.0, 20.0)),
            ObjectData::RasterImage {
                asset_key: "asset".into(),
                original_width_px: 10,
                original_height_px: 10,
                adjustments: None,
                masks: Vec::new(),
            },
        );
        let raster_id = raster.id;
        project.add_object(raster);
        let ctx = sample_context_with_project(project);

        let err =
            nest_selected(&ctx, vec![container, raster_id], NestOptions::default()).unwrap_err();
        assert!(err.message.contains("Raster images"));
        assert!(!ctx.project.lock().unwrap().as_ref().unwrap().dirty);
    }

    #[test]
    fn hidden_parts_reject_before_mutation() {
        let mut project = Project::new("nest");
        let container = rect_obj(&mut project, "container", 0.0, 0.0, 100.0, 100.0);
        let part = rect_obj(&mut project, "part", 150.0, 0.0, 10.0, 10.0);
        project.find_object_mut(part).unwrap().visible = false;
        let ctx = sample_context_with_project(project);

        let err = nest_selected(&ctx, vec![container, part], NestOptions::default()).unwrap_err();

        assert!(err.message.contains("at least one object"));
        assert!(!ctx.project.lock().unwrap().as_ref().unwrap().dirty);
    }

    #[test]
    fn text_and_barcode_parts_resolve_to_vector_geometry() {
        let mut project = Project::new("nest");
        let container = rect_obj(&mut project, "container", 0.0, 0.0, 150.0, 150.0);
        let layer_id = project.ensure_default_layer();
        let text = ProjectObject::new(
            "text",
            layer_id,
            Bounds::new(Point2D::new(200.0, 0.0), Point2D::new(210.0, 10.0)),
            ObjectData::Text {
                content: "A".into(),
                font_family: "Arial".into(),
                font_size_mm: 10.0,
                alignment: TextAlignment::Left,
                alignment_v: TextAlignmentV::Top,
                bold: false,
                italic: false,
                upper_case: false,
                welded: false,
                h_spacing: 0.0,
                v_spacing: 0.0,
                on_path: false,
                path_offset: 0.0,
                distort: false,
                layout_mode: TextLayoutMode::Straight,
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
                resolved_path_data: Some(rect_path(0.0, 0.0, 10.0, 10.0)),
                missing_font: false,
                missing_glyphs: Vec::new(),
                guide_path_id: None,
                variable_text: None,
            },
        );
        let text_id = text.id;
        project.add_object(text);
        let barcode = ProjectObject::new(
            "barcode",
            layer_id,
            Bounds::new(Point2D::new(220.0, 0.0), Point2D::new(240.0, 10.0)),
            ObjectData::Barcode {
                barcode_type: BarcodeType::Code128,
                data: "ABC123".into(),
                width: 20.0,
                height: 10.0,
                options: BarcodeOptions::default(),
            },
        );
        let barcode_id = barcode.id;
        project.add_object(barcode);
        let ctx = sample_context_with_project(project);

        let result = nest_selected(
            &ctx,
            vec![container, text_id, barcode_id],
            NestOptions {
                allow_rotation: false,
                ..NestOptions::default()
            },
        )
        .unwrap();

        assert!(result.placed_object_ids.contains(&text_id));
        assert!(result.placed_object_ids.contains(&barcode_id));
    }

    #[test]
    fn hidden_or_open_paths_are_not_container_candidates() {
        let mut project = Project::new("nest");
        let layer_id = project.ensure_default_layer();
        let open = ProjectObject::new(
            "open",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 0.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L100 0".into(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let open_id = open.id;
        project.add_object(open);
        let raster = ProjectObject::new(
            "raster",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(20.0, 20.0)),
            ObjectData::RasterImage {
                asset_key: "asset".into(),
                original_width_px: 10,
                original_height_px: 10,
                adjustments: None,
                masks: Vec::new(),
            },
        );
        let raster_id = raster.id;
        project.add_object(raster);
        let ctx = sample_context_with_project(project);

        let err =
            nest_selected(&ctx, vec![open_id, raster_id], NestOptions::default()).unwrap_err();
        assert!(err.message.contains("container"));
    }

    #[test]
    fn part_count_cap_rejects_before_mutation() {
        let mut project = Project::new("nest");
        let container = rect_obj(&mut project, "container", 0.0, 0.0, 500.0, 500.0);
        let mut ids = vec![container];
        for i in 0..=MAX_NEST_PARTS {
            ids.push(rect_obj(
                &mut project,
                &format!("part-{i}"),
                600.0 + i as f64 * 2.0,
                0.0,
                1.0,
                1.0,
            ));
        }
        let ctx = sample_context_with_project(project);

        let err = nest_selected(&ctx, ids, NestOptions::default()).unwrap_err();
        assert!(err.message.contains("supports up to"));
        assert!(!ctx.project.lock().unwrap().as_ref().unwrap().dirty);
    }

    #[test]
    fn point_count_cap_rejects_before_mutation() {
        let mut project = Project::new("nest");
        let container = rect_obj(&mut project, "container", 0.0, 0.0, 500.0, 500.0);
        let mut path = String::new();
        let center = Point2D::new(650.0, 50.0);
        let radius = 20.0;
        let dense_point_count = MAX_FLATTENED_POINTS_PER_PART + 200;
        for i in 0..dense_point_count {
            let theta = (i as f64 / dense_point_count as f64) * std::f64::consts::TAU;
            let x = center.x + radius * theta.cos();
            let y = center.y + radius * theta.sin();
            if i == 0 {
                path.push_str(&format!("M{x} {y}"));
            } else {
                path.push_str(&format!(" L{x} {y}"));
            }
        }
        path.push_str(" Z");
        let dense = vector_path_obj(
            &mut project,
            "dense",
            path,
            Bounds::new(Point2D::new(630.0, 30.0), Point2D::new(670.0, 70.0)),
            true,
        );
        let ctx = sample_context_with_project(project);

        let err = nest_selected(
            &ctx,
            vec![container, dense],
            NestOptions {
                time_limit_ms: 60_000,
                ..NestOptions::default()
            },
        )
        .unwrap_err();

        assert!(
            err.message.contains("point nesting limit"),
            "{}",
            err.message
        );
        assert!(!ctx.project.lock().unwrap().as_ref().unwrap().dirty);
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn lock_inner_objects_moves_contained_shape_with_outer_part() {
        let mut project = Project::new("nest");
        let container = rect_obj(&mut project, "container", 0.0, 0.0, 100.0, 100.0);
        let outer = rect_obj(&mut project, "outer", 150.0, 0.0, 20.0, 20.0);
        let inner = rect_obj(&mut project, "inner", 155.0, 5.0, 5.0, 5.0);
        let ctx = sample_context_with_project(project);

        let result = nest_selected(
            &ctx,
            vec![container, outer, inner],
            NestOptions {
                lock_inner_objects: true,
                allow_rotation: false,
                ..NestOptions::default()
            },
        )
        .unwrap();

        assert!(result.placed_object_ids.contains(&outer));
        assert!(result.placed_object_ids.contains(&inner));
        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        let outer_obj = project.find_object(outer).unwrap();
        let inner_obj = project.find_object(inner).unwrap();
        assert!((inner_obj.bounds.min.x - outer_obj.bounds.min.x - 5.0).abs() < 1e-6);
        assert!((inner_obj.bounds.min.y - outer_obj.bounds.min.y - 5.0).abs() < 1e-6);
    }

    #[test]
    fn successful_nest_pushes_one_undo_snapshot() {
        let mut project = Project::new("nest");
        let container = rect_obj(&mut project, "container", 0.0, 0.0, 100.0, 100.0);
        let part = rect_obj(&mut project, "part", 150.0, 0.0, 10.0, 10.0);
        let ctx = sample_context_with_project(project);

        assert!(!ctx.undo_state().unwrap().can_undo);
        nest_selected(
            &ctx,
            vec![container, part],
            NestOptions {
                allow_rotation: false,
                ..NestOptions::default()
            },
        )
        .unwrap();
        assert!(ctx.undo_state().unwrap().can_undo);

        crate::ops::project::undo_project(&ctx).unwrap();
        assert!(!ctx.undo_state().unwrap().can_undo);
        let guard = ctx.project.lock().unwrap();
        let restored = guard.as_ref().unwrap().find_object(part).unwrap();
        assert!((restored.bounds.min.x - 150.0).abs() < EPS);
        assert!((restored.bounds.min.y - 0.0).abs() < EPS);
    }

    #[test]
    fn timeout_rejects_without_mutation() {
        let mut project = Project::new("nest");
        let container = rect_obj(&mut project, "container", 0.0, 0.0, 500.0, 500.0);
        let mut ids = vec![container];
        for i in 0..20 {
            ids.push(rect_obj(
                &mut project,
                &format!("part-{i}"),
                600.0 + i as f64 * 20.0,
                0.0,
                10.0,
                10.0,
            ));
        }
        let ctx = sample_context_with_project(project);

        let err = nest_selected(
            &ctx,
            ids,
            NestOptions {
                time_limit_ms: 1,
                ..NestOptions::default()
            },
        )
        .unwrap_err();

        assert!(err.message.contains("timed out"));
        assert!(!ctx.project.lock().unwrap().as_ref().unwrap().dirty);
    }
}
