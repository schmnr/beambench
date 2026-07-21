use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use beambench_common::path::{PathCommand, VecPath};
use beambench_common::{
    Bounds, ColorTag, PALETTE_COLORS, Point2D, Transform2D, canonical_palette_color_tag,
};
use beambench_core::variable_text::{VariableTextConfig, advance_sequence_value};
use beambench_core::vector::convert::object_to_world_vecpath_resolved;
use beambench_core::vector::text_to_path::{
    intrinsic_text_bounds, refresh_text_object_cache, refresh_text_object_cache_with_guide,
};
use beambench_core::vector::transform::bake_transform;
use beambench_core::{
    CutEntry, CutEntryId, CutEntryPatch, CutEntryTemplate, Layer, LayerBatchToggle, LayerId,
    MachineProfile, ObjectData, ObjectId, OperationType, Project, ProjectObject, RasterSettings,
    TextAlignment, TextAlignmentV, TextLayoutMode, Workspace, alignment,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::context::ServiceContext;
use crate::error::{ServiceError, ServiceResult};
use crate::events;

#[derive(Debug, Clone)]
pub struct AddLayerInput {
    pub name: String,
    pub operation: OperationType,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateLayerInput {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub visible: Option<bool>,
    pub color_tag: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AddObjectInput {
    pub name: String,
    pub layer_id: LayerId,
    pub object_data: ObjectData,
    pub bounds: Bounds,
}

#[derive(Debug, Clone)]
pub struct AddObjectLayerInput {
    pub name: String,
    pub color_tag: Option<String>,
    /// Operation for the new layer's first (and only) cut entry. The entry's
    /// `CutEntryId` is minted server-side by `Layer::new_single_entry`.
    pub operation: OperationType,
    /// Optional field-level overrides applied to the minted entry. Use this
    /// instead of constructing a `CutEntry` on the caller side, so clients
    /// never have to invent a `CutEntryId`.
    pub entry_patch: Option<CutEntryPatch>,
}

#[derive(Debug, Clone)]
pub struct AddObjectAtomicInput {
    pub name: String,
    pub layer_id: LayerId,
    pub object_data: ObjectData,
    pub bounds: Bounds,
    pub create_layer: Option<AddObjectLayerInput>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddObjectAtomicResult {
    pub object: ProjectObject,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_layer: Option<Layer>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateObjectInput {
    pub name: Option<String>,
    pub visible: Option<bool>,
    pub locked: Option<bool>,
    pub layer_id: Option<LayerId>,
    pub transform: Option<Transform2D>,
    pub bounds: Option<Bounds>,
    pub lock_aspect_ratio: Option<bool>,
    pub power_scale: Option<f64>,
    pub priority: Option<i32>,
}

/// Per-copy input for batch generation: resolved text + full variable text config.
#[derive(Debug, Clone)]
pub struct BatchCopyInput {
    pub resolved_text: String,
    pub variable_text_config: VariableTextConfig,
}

/// Result of batch generation: copies + updated original with advanced state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchResult {
    pub copies: Vec<ProjectObject>,
    pub updated_original: ProjectObject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoveTogetherAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SameSizeAxis {
    Width,
    Height,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockOptions {
    pub move_as_group: bool,
    pub lock_inner_objects: bool,
    pub padding_mm: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResizeSlotsOptions {
    pub current_thickness_mm: f64,
    pub new_thickness_mm: f64,
    pub tolerance_mm: f64,
}

#[derive(Debug, Clone)]
struct ArrangementUnit {
    root_id: ObjectId,
    member_ids: Vec<ObjectId>,
    bounds: Bounds,
}

#[derive(Debug, Clone)]
struct DockCluster {
    root_ids: Vec<ObjectId>,
    member_ids: Vec<ObjectId>,
    bounds: Bounds,
}

fn lock_err(name: &str, e: impl std::fmt::Display) -> ServiceError {
    ServiceError::internal(format!("Failed to lock {name}: {e}"))
}

fn invalidate_plan(ctx: &ServiceContext) -> ServiceResult<()> {
    let mut cache_guard = ctx
        .plan_cache
        .lock()
        .map_err(|e| lock_err("plan_cache", e))?;
    *cache_guard = None;
    Ok(())
}

/// Look up the guide VecPath for a text object's guide_path_id, if applicable.
fn lookup_guide_path_for_text(
    data: &ObjectData,
    project: &Project,
) -> Option<beambench_common::path::VecPath> {
    if let ObjectData::Text {
        guide_path_id: Some(guide_id),
        layout_mode,
        on_path,
        ..
    } = data
    {
        let effective = if *on_path && *layout_mode == TextLayoutMode::Straight {
            TextLayoutMode::Path
        } else {
            *layout_mode
        };
        if effective == TextLayoutMode::Path {
            if let Some(guide_obj) = project.find_object(*guide_id) {
                return object_to_world_vecpath_resolved(guide_obj, project);
            }
        }
    }
    None
}

/// Refresh text objects that depend on `guide_id` as their guide path,
/// including text objects whose guide is a VirtualClone of `guide_id`.
/// Called when the guide path object is edited (geometry, bounds, transform change).
fn refresh_dependent_text_objects(project: &mut Project, guide_id: ObjectId) {
    // Build set of IDs that resolve to guide_id's geometry:
    // guide_id itself + any VirtualClones whose source chain leads to it.
    let mut affected_ids = std::collections::HashSet::new();
    affected_ids.insert(guide_id);
    loop {
        let new_clones: Vec<ObjectId> = project
            .objects
            .iter()
            .filter_map(|o| {
                if let ObjectData::VirtualClone { source_id } = &o.data {
                    if affected_ids.contains(source_id) && !affected_ids.contains(&o.id) {
                        return Some(o.id);
                    }
                }
                None
            })
            .collect();
        if new_clones.is_empty() {
            break;
        }
        affected_ids.extend(new_clones);
    }

    // For each affected guide ID, find text objects referencing it and refresh them.
    for &aid in &affected_ids {
        let guide_vecpath = project
            .find_object(aid)
            .and_then(|obj| object_to_world_vecpath_resolved(obj, project));
        let dependent_ids: Vec<ObjectId> = project
            .objects
            .iter()
            .filter_map(|obj| {
                if let ObjectData::Text {
                    guide_path_id: Some(gid),
                    ..
                } = &obj.data
                {
                    if *gid == aid {
                        return Some(obj.id);
                    }
                }
                None
            })
            .collect();
        for text_id in dependent_ids {
            if let Some(obj) = project.find_object_mut(text_id) {
                let mapped_bounds = refresh_text_object_cache_with_guide(
                    &mut obj.data,
                    &obj.bounds,
                    guide_vecpath.as_ref(),
                );
                if let Some(bounds) = mapped_bounds {
                    obj.bounds = bounds;
                    obj.transform = Transform2D::identity();
                }
            }
        }
    }
}

/// Rebuild text bounds so the alignment anchor stays fixed when width/height change.
/// For example, center-aligned text keeps its center in place; right-aligned keeps its right edge.
fn rebuild_text_bounds_aligned(old: &Bounds, new_w: f64, new_h: f64, data: &ObjectData) -> Bounds {
    let (al, alv) = match data {
        ObjectData::Text {
            alignment,
            alignment_v,
            ..
        } => (*alignment, *alignment_v),
        _ => return Bounds::new(old.min, Point2D::new(old.min.x + new_w, old.min.y + new_h)),
    };
    // Compute the anchor position from the old bounds
    let ax = match al {
        TextAlignment::Left => old.min.x,
        TextAlignment::Center => (old.min.x + old.max.x) / 2.0,
        TextAlignment::Right => old.max.x,
    };
    let ay = match alv {
        TextAlignmentV::Top => old.min.y,
        TextAlignmentV::Middle => (old.min.y + old.max.y) / 2.0,
        TextAlignmentV::Bottom => old.max.y,
    };
    // Compute the new min from the anchor
    let mx = match al {
        TextAlignment::Left => ax,
        TextAlignment::Center => ax - new_w / 2.0,
        TextAlignment::Right => ax - new_w,
    };
    let my = match alv {
        TextAlignmentV::Top => ay,
        TextAlignmentV::Middle => ay - new_h / 2.0,
        TextAlignmentV::Bottom => ay - new_h,
    };
    Bounds::new(Point2D::new(mx, my), Point2D::new(mx + new_w, my + new_h))
}

/// Adopt generated text geometry without baking placement into the object.
/// The existing affine transform remains untouched and the persisted text
/// alignment determines which pre-refit bounds anchor stays fixed.
fn apply_mapped_text_bounds(
    object: &mut ProjectObject,
    placement_bounds: &Bounds,
    mapped_bounds: &Bounds,
) {
    object.bounds = rebuild_text_bounds_aligned(
        placement_bounds,
        mapped_bounds.width().max(0.0),
        mapped_bounds.height().max(0.0),
        &object.data,
    );
}

pub(crate) fn refresh_project_text_caches(project: &mut Project) {
    // First pass: collect guide VecPaths for text objects that reference guide_path_id.
    // We need to read guide geometry before mutating, so collect into a map.
    let mut guide_paths: std::collections::HashMap<ObjectId, beambench_common::path::VecPath> =
        std::collections::HashMap::new();
    for obj in &project.objects {
        if let ObjectData::Text {
            guide_path_id: Some(guide_id),
            layout_mode,
            on_path,
            ..
        } = &obj.data
        {
            let effective = if *on_path && *layout_mode == TextLayoutMode::Straight {
                TextLayoutMode::Path
            } else {
                *layout_mode
            };
            if effective == TextLayoutMode::Path {
                if let Some(guide_obj) = project.find_object(*guide_id) {
                    if let Some(vecpath) = object_to_world_vecpath_resolved(guide_obj, project) {
                        guide_paths.insert(obj.id, vecpath);
                    }
                }
            }
        }
    }
    let project_snapshot = project.clone();
    // Second pass: refresh caches, passing guide path where available.
    for obj in &mut project.objects {
        let mapped_bounds = if let Some(guide) = guide_paths.get(&obj.id) {
            refresh_text_object_cache_with_project_context(&project_snapshot, obj, Some(guide))
        } else {
            refresh_text_object_cache_with_project_context(&project_snapshot, obj, None)
        };
        if let Some(mb) = mapped_bounds {
            // Preserve stored position, update size from geometry
            let width = mb.width();
            let height = mb.height();
            obj.bounds = Bounds::new(
                obj.bounds.min,
                Point2D::new(obj.bounds.min.x + width, obj.bounds.min.y + height),
            );
            obj.transform = Transform2D::identity();
        }
    }
}

fn refresh_text_object_cache_with_project_context(
    project: &Project,
    object: &mut ProjectObject,
    guide_path: Option<&beambench_common::path::VecPath>,
) -> Option<Bounds> {
    let config = match &object.data {
        ObjectData::Text {
            variable_text: Some(config),
            ..
        } => Some(config.clone()),
        _ => None,
    };
    if let Some(config) = config {
        let object_snapshot = object.clone();
        let resolved =
            beambench_core::resolve_text_in_project(project, &object_snapshot, &config, 0);
        if let ObjectData::Text {
            content,
            variable_text,
            ..
        } = &mut object.data
        {
            let original_content = std::mem::replace(content, resolved);
            let original_variable_text = variable_text.take();
            let mapped_bounds =
                refresh_text_object_cache_with_guide(&mut object.data, &object.bounds, guide_path);
            if let ObjectData::Text {
                content,
                variable_text,
                ..
            } = &mut object.data
            {
                *content = original_content;
                *variable_text = original_variable_text;
            }
            mapped_bounds
        } else {
            None
        }
    } else if let Some(guide_path) = guide_path {
        refresh_text_object_cache_with_guide(&mut object.data, &object.bounds, Some(guide_path))
    } else {
        refresh_text_object_cache(&mut object.data, &object.bounds)
    }
}

fn translate_bounds(bounds: Bounds, dx: f64, dy: f64) -> Bounds {
    Bounds::new(
        Point2D::new(bounds.min.x + dx, bounds.min.y + dy),
        Point2D::new(bounds.max.x + dx, bounds.max.y + dy),
    )
}

fn bounds_contains(outer: &Bounds, inner: &Bounds) -> bool {
    outer.min.x <= inner.min.x
        && outer.min.y <= inner.min.y
        && outer.max.x >= inner.max.x
        && outer.max.y >= inner.max.y
}

fn spans_overlap(a_min: f64, a_max: f64, b_min: f64, b_max: f64) -> bool {
    a_max > b_min && b_max > a_min
}

fn layer_is_tool(project: &Project, layer_id: LayerId) -> bool {
    project
        .find_layer(layer_id)
        .is_some_and(|layer| layer.is_tool_layer)
}

fn is_ruler_guide_object(object: &ProjectObject) -> bool {
    matches!(
        &object.data,
        ObjectData::VectorPath {
            ruler_guide_axis: Some(_),
            ..
        }
    )
}

fn is_arrangement_excluded_object(project: &Project, object: &ProjectObject) -> bool {
    layer_is_tool(project, object.layer_id) || is_ruler_guide_object(object)
}

fn workspace_from_machine_profile(profile: &MachineProfile) -> Workspace {
    Workspace {
        bed_width_mm: profile.bed_width_mm,
        bed_height_mm: profile.bed_height_mm,
        origin: profile.origin,
    }
}

fn new_project_workspace_from_machine_profile(profile: &MachineProfile) -> Workspace {
    Workspace {
        bed_width_mm: profile.bed_width_mm,
        bed_height_mm: profile.bed_height_mm,
        origin: profile.preferred_default_origin.unwrap_or(profile.origin),
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

fn normalize_arrangement_roots(project: &Project, object_ids: &[ObjectId]) -> Vec<ObjectId> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for object_id in object_ids {
        let Some(object) = project.find_object(*object_id) else {
            continue;
        };
        if is_arrangement_excluded_object(project, object) {
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

fn arrangement_unit_bounds(project: &Project, member_ids: &[ObjectId]) -> Option<Bounds> {
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

fn build_arrangement_units(
    project: &Project,
    object_ids: &[ObjectId],
) -> ServiceResult<Vec<ArrangementUnit>> {
    normalize_arrangement_roots(project, object_ids)
        .into_iter()
        .map(|root_id| {
            let mut member_ids = Vec::new();
            collect_group_members(project, root_id, &mut member_ids);
            let bounds = arrangement_unit_bounds(project, &member_ids)
                .ok_or_else(|| ServiceError::invalid_input("Group has no positionable members"))?;
            Ok(ArrangementUnit {
                root_id,
                member_ids,
                bounds,
            })
        })
        .collect()
}

fn arrangement_unit_is_locked(project: &Project, unit: &ArrangementUnit) -> bool {
    unit.member_ids.iter().any(|object_id| {
        project
            .find_object(*object_id)
            .is_some_and(|object| object.locked)
    })
}

fn move_entries_for_units(
    project: &mut Project,
    units: &[ArrangementUnit],
    offsets_by_root: &HashMap<ObjectId, Point2D>,
) -> ServiceResult<Vec<(ObjectId, Bounds)>> {
    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    for unit in units {
        let offset = offsets_by_root
            .get(&unit.root_id)
            .copied()
            .unwrap_or(Point2D::new(0.0, 0.0));
        if offset.x.abs() < 1e-9 && offset.y.abs() < 1e-9 {
            continue;
        }
        for object_id in &unit.member_ids {
            if !seen.insert(*object_id) {
                continue;
            }
            project
                .ensure_resolved(*object_id)
                .map_err(ServiceError::invalid_state)?;
            let object = project
                .find_object(*object_id)
                .ok_or_else(|| ServiceError::not_found("Object not found"))?;
            entries.push((
                *object_id,
                translate_bounds(object.bounds, offset.x, offset.y),
            ));
        }
    }
    Ok(entries)
}

fn updated_objects_for_entries(
    project: &Project,
    entries: &[(ObjectId, Bounds)],
) -> ServiceResult<Vec<ProjectObject>> {
    let mut seen = HashSet::new();
    let mut updated = Vec::new();
    for (object_id, _) in entries {
        if !seen.insert(*object_id) {
            continue;
        }
        updated.push(
            project
                .find_object(*object_id)
                .ok_or_else(|| ServiceError::internal("object not found after mutation"))?
                .clone(),
        );
    }
    Ok(updated)
}

fn bounds_center(bounds: Bounds) -> Point2D {
    Point2D::new(
        (bounds.min.x + bounds.max.x) / 2.0,
        (bounds.min.y + bounds.max.y) / 2.0,
    )
}

fn reflect_point_across_line(point: Point2D, line_start: Point2D, line_end: Point2D) -> Point2D {
    let dx = line_end.x - line_start.x;
    let dy = line_end.y - line_start.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq <= 1e-12 {
        return point;
    }
    let t = ((point.x - line_start.x) * dx + (point.y - line_start.y) * dy) / len_sq;
    let proj = Point2D::new(line_start.x + t * dx, line_start.y + t * dy);
    Point2D::new(2.0 * proj.x - point.x, 2.0 * proj.y - point.y)
}

fn reflection_transform_for_line(line_start: Point2D, line_end: Point2D) -> Transform2D {
    let angle = (line_end.y - line_start.y).atan2(line_end.x - line_start.x);
    let doubled = 2.0 * angle;
    Transform2D {
        a: doubled.cos(),
        b: doubled.sin(),
        c: doubled.sin(),
        d: -doubled.cos(),
        tx: 0.0,
        ty: 0.0,
    }
}

fn point_lies_on_line(point: Point2D, line_start: Point2D, line_end: Point2D) -> bool {
    let dx = line_end.x - line_start.x;
    let dy = line_end.y - line_start.y;
    let length = (dx * dx + dy * dy).sqrt();
    if length <= 1e-9 {
        return false;
    }
    let cross = ((point.x - line_start.x) * dy - (point.y - line_start.y) * dx).abs();
    cross <= 1e-9_f64.max(length * 1e-6)
}

fn extract_mirror_axis_points(
    project: &Project,
    axis_object_id: ObjectId,
) -> ServiceResult<(Point2D, Point2D)> {
    let object = project
        .find_object(axis_object_id)
        .ok_or_else(|| ServiceError::not_found("Mirror axis object not found"))?;
    if is_ruler_guide_object(object) {
        return Err(ServiceError::invalid_input(
            "Mirror axis must be a separate straight line object",
        ));
    }
    let world = object_to_world_vecpath_resolved(object, project)
        .ok_or_else(|| ServiceError::invalid_input("Mirror axis object has no vector geometry"))?;
    if world.subpaths.len() != 1 {
        return Err(ServiceError::invalid_input(
            "Mirror axis must be a single open straight segment",
        ));
    }
    let subpath = &world.subpaths[0];
    if subpath.closed || subpath.commands.len() != 2 {
        return Err(ServiceError::invalid_input(
            "Mirror axis must be a single open straight segment",
        ));
    }
    let start = match subpath.commands[0] {
        PathCommand::MoveTo { x, y } => Point2D::new(x, y),
        _ => {
            return Err(ServiceError::invalid_input(
                "Mirror axis must be a single open straight segment",
            ));
        }
    };
    let end = match subpath.commands[1] {
        PathCommand::LineTo { x, y } => Point2D::new(x, y),
        PathCommand::CubicTo {
            c1x,
            c1y,
            c2x,
            c2y,
            x,
            y,
        } => {
            let end = Point2D::new(x, y);
            if !point_lies_on_line(Point2D::new(c1x, c1y), start, end)
                || !point_lies_on_line(Point2D::new(c2x, c2y), start, end)
            {
                return Err(ServiceError::invalid_input(
                    "Mirror axis must be a single open straight segment",
                ));
            }
            end
        }
        _ => {
            return Err(ServiceError::invalid_input(
                "Mirror axis must be a single open straight segment",
            ));
        }
    };
    if start.distance_to(&end) <= 1e-9 {
        return Err(ServiceError::invalid_input(
            "Mirror axis must have non-zero length",
        ));
    }
    Ok((start, end))
}

fn duplicate_subtree(
    project: &mut Project,
    object_id: ObjectId,
    created: &mut Vec<ProjectObject>,
) -> ServiceResult<ObjectId> {
    let original = project
        .find_object(object_id)
        .cloned()
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    let mut duplicated = original.clone();
    duplicated.id = ObjectId::new();
    duplicated.name = format!("{} copy", original.name);
    if let ObjectData::Group { children } = &original.data {
        let mut duplicated_children = Vec::with_capacity(children.len());
        for child_id in children {
            duplicated_children.push(duplicate_subtree(project, *child_id, created)?);
        }
        duplicated.data = ObjectData::Group {
            children: duplicated_children,
        };
    }
    let added = project.add_object(duplicated).clone();
    created.push(added.clone());
    Ok(added.id)
}

fn build_same_size_entries(
    project: &mut Project,
    object_ids: &[ObjectId],
    anchor_object_id: ObjectId,
    axis: SameSizeAxis,
    preserve_aspect: bool,
) -> ServiceResult<Vec<(ObjectId, Bounds)>> {
    let units = build_arrangement_units(project, object_ids)?;
    if units.len() < 2 {
        return Err(ServiceError::invalid_input(
            "Make Same Size requires at least two movable objects",
        ));
    }
    let anchor = units
        .iter()
        .find(|unit| unit.root_id == anchor_object_id)
        .ok_or_else(|| {
            ServiceError::invalid_input("Anchor object is not in the movable selection")
        })?;
    let target_width = anchor.bounds.width();
    let target_height = anchor.bounds.height();
    let mut entries = Vec::new();
    for unit in units {
        if unit.root_id == anchor_object_id {
            continue;
        }
        let locked_aspect = project
            .find_object(unit.root_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?
            .lock_aspect_ratio;
        let unit_width = unit.bounds.width();
        let unit_height = unit.bounds.height();
        if unit_width.abs() <= 1e-9 || unit_height.abs() <= 1e-9 {
            continue;
        }
        let locked_aspect = preserve_aspect || locked_aspect;
        let (sx, sy) = match axis {
            SameSizeAxis::Width => {
                let sx = target_width / unit_width;
                let sy = if locked_aspect { sx } else { 1.0 };
                (sx, sy)
            }
            SameSizeAxis::Height => {
                let sy = target_height / unit_height;
                let sx = if locked_aspect { sy } else { 1.0 };
                (sx, sy)
            }
        };
        if (sx - 1.0).abs() <= 1e-9 && (sy - 1.0).abs() <= 1e-9 {
            continue;
        }
        let center = bounds_center(unit.bounds);
        for member_id in unit.member_ids {
            project
                .ensure_resolved(member_id)
                .map_err(ServiceError::invalid_state)?;
            let object = project
                .find_object(member_id)
                .ok_or_else(|| ServiceError::not_found("Object not found"))?;
            if matches!(object.data, ObjectData::Group { .. }) {
                continue;
            }
            let object_center = bounds_center(object.bounds);
            let offset_x = object_center.x - center.x;
            let offset_y = object_center.y - center.y;
            let next_center = Point2D::new(center.x + offset_x * sx, center.y + offset_y * sy);
            let next_width = object.bounds.width() * sx;
            let next_height = object.bounds.height() * sy;
            entries.push((
                member_id,
                Bounds::new(
                    Point2D::new(
                        next_center.x - next_width / 2.0,
                        next_center.y - next_height / 2.0,
                    ),
                    Point2D::new(
                        next_center.x + next_width / 2.0,
                        next_center.y + next_height / 2.0,
                    ),
                ),
            ));
        }
    }
    Ok(entries)
}

const SLOT_ELIGIBILITY_TOLERANCE_MM: f64 = 0.1;
const SLOT_SQUARE_TOLERANCE_MM: f64 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy)]
struct SlotRect {
    center: Point2D,
    width: f64,
    height: f64,
    narrow_axis: SlotAxis,
}

fn axis_aligned_rect_from_subpath(subpath: &beambench_common::path::SubPath) -> Option<SlotRect> {
    if !subpath.closed {
        return None;
    }
    let mut points = Vec::new();
    for command in &subpath.commands {
        match *command {
            PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => {
                points.push(Point2D::new(x, y));
            }
            PathCommand::Close => {}
            _ => return None,
        }
    }
    if points.len() != 4 {
        return None;
    }
    let mut unique_x = points.iter().map(|point| point.x).collect::<Vec<_>>();
    unique_x.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    unique_x.dedup_by(|a, b| (*a - *b).abs() <= 1e-9);
    let mut unique_y = points.iter().map(|point| point.y).collect::<Vec<_>>();
    unique_y.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    unique_y.dedup_by(|a, b| (*a - *b).abs() <= 1e-9);
    if unique_x.len() != 2 || unique_y.len() != 2 {
        return None;
    }
    let min_x = unique_x[0];
    let max_x = unique_x[1];
    let min_y = unique_y[0];
    let max_y = unique_y[1];
    let width = max_x - min_x;
    let height = max_y - min_y;
    if width <= 1e-9 || height <= 1e-9 {
        return None;
    }
    let narrow_axis = if (width - height).abs() <= SLOT_SQUARE_TOLERANCE_MM {
        return None;
    } else if width < height {
        SlotAxis::Horizontal
    } else {
        SlotAxis::Vertical
    };
    Some(SlotRect {
        center: Point2D::new((min_x + max_x) / 2.0, (min_y + max_y) / 2.0),
        width,
        height,
        narrow_axis,
    })
}

fn slot_matches_thickness(rect: SlotRect, current_thickness_mm: f64) -> bool {
    let slot_width = match rect.narrow_axis {
        SlotAxis::Horizontal => rect.width,
        SlotAxis::Vertical => rect.height,
    };
    (slot_width - current_thickness_mm).abs() <= SLOT_ELIGIBILITY_TOLERANCE_MM
}

fn slot_rect_to_subpath(rect: SlotRect) -> beambench_common::path::SubPath {
    let half_w = rect.width / 2.0;
    let half_h = rect.height / 2.0;
    let min_x = rect.center.x - half_w;
    let max_x = rect.center.x + half_w;
    let min_y = rect.center.y - half_h;
    let max_y = rect.center.y + half_h;
    beambench_common::path::SubPath {
        commands: vec![
            PathCommand::MoveTo { x: min_x, y: min_y },
            PathCommand::LineTo { x: max_x, y: min_y },
            PathCommand::LineTo { x: max_x, y: max_y },
            PathCommand::LineTo { x: min_x, y: max_y },
            PathCommand::Close,
        ],
        closed: true,
    }
}

fn resize_slot_rect(rect: SlotRect, target_slot_width: f64) -> SlotRect {
    match rect.narrow_axis {
        SlotAxis::Horizontal => SlotRect {
            width: target_slot_width,
            ..rect
        },
        SlotAxis::Vertical => SlotRect {
            height: target_slot_width,
            ..rect
        },
    }
}

fn normalize_world_vecpath(path: VecPath) -> Option<(VecPath, Bounds)> {
    let bounds = path.visual_bounds().or_else(|| path.bounds())?;
    let normalized = bake_transform(&path, &Transform2D::translate(-bounds.min.x, -bounds.min.y));
    Some((normalized, bounds))
}

fn dock_axis_overlap(bounds: &Bounds, blocker: &Bounds, direction: DockDirection) -> bool {
    match direction {
        DockDirection::Left | DockDirection::Right => {
            spans_overlap(bounds.min.y, bounds.max.y, blocker.min.y, blocker.max.y)
        }
        DockDirection::Up | DockDirection::Down => {
            spans_overlap(bounds.min.x, bounds.max.x, blocker.min.x, blocker.max.x)
        }
    }
}

fn cluster_contains(a: &DockCluster, b: &DockCluster) -> bool {
    bounds_contains(&a.bounds, &b.bounds)
}

fn combine_clusters(clusters: Vec<DockCluster>) -> DockCluster {
    let mut root_ids = Vec::new();
    let mut member_ids = Vec::new();
    let mut bounds: Option<Bounds> = None;
    for cluster in clusters {
        root_ids.extend(cluster.root_ids);
        member_ids.extend(cluster.member_ids);
        bounds = Some(match bounds {
            Some(current) => current.union(&cluster.bounds),
            None => cluster.bounds,
        });
    }
    DockCluster {
        root_ids,
        member_ids,
        bounds: bounds.expect("combine_clusters requires at least one cluster"),
    }
}

fn build_dock_clusters(units: &[ArrangementUnit], options: &DockOptions) -> Vec<DockCluster> {
    let mut clusters: Vec<DockCluster> = units
        .iter()
        .map(|unit| DockCluster {
            root_ids: vec![unit.root_id],
            member_ids: unit.member_ids.clone(),
            bounds: unit.bounds,
        })
        .collect();

    if options.lock_inner_objects {
        // O(N^3) worst-case bubble-merge is acceptable for typical Arrange selections
        // (tens, not hundreds, of units) and keeps the transitive containment rule simple.
        let mut changed = true;
        while changed {
            changed = false;
            'outer: for i in 0..clusters.len() {
                for j in (i + 1)..clusters.len() {
                    if cluster_contains(&clusters[i], &clusters[j])
                        || cluster_contains(&clusters[j], &clusters[i])
                    {
                        let right = clusters.remove(j);
                        let left = clusters.remove(i);
                        clusters.push(combine_clusters(vec![left, right]));
                        changed = true;
                        break 'outer;
                    }
                }
            }
        }
    }

    if options.move_as_group && !clusters.is_empty() {
        return vec![combine_clusters(clusters)];
    }

    clusters
}

fn dock_cluster_sort_value(cluster: &DockCluster, direction: DockDirection) -> f64 {
    match direction {
        DockDirection::Left => cluster.bounds.min.x,
        DockDirection::Right => -cluster.bounds.max.x,
        DockDirection::Up => cluster.bounds.min.y,
        DockDirection::Down => -cluster.bounds.max.y,
    }
}

fn blocker_object_bounds(project: &Project, object_id: ObjectId) -> Option<Bounds> {
    let object = project.find_object(object_id)?;
    if !object.visible || is_arrangement_excluded_object(project, object) {
        return None;
    }
    let layer = project.find_layer(object.layer_id)?;
    if !layer.visible {
        return None;
    }
    Some(object.bounds)
}

fn compute_dock_target(
    bounds: Bounds,
    blockers: &[Bounds],
    direction: DockDirection,
    workspace: &Bounds,
    padding_mm: f64,
) -> Point2D {
    let padding = padding_mm.max(0.0);
    match direction {
        DockDirection::Left => {
            let mut target_min = workspace.min.x + padding;
            for blocker in blockers {
                if blocker.max.x <= bounds.min.x + 1e-9
                    && dock_axis_overlap(&bounds, blocker, direction)
                {
                    target_min = target_min.max(blocker.max.x + padding);
                }
            }
            Point2D::new(target_min - bounds.min.x, 0.0)
        }
        DockDirection::Right => {
            let mut target_max = workspace.max.x - padding;
            for blocker in blockers {
                if blocker.min.x >= bounds.max.x - 1e-9
                    && dock_axis_overlap(&bounds, blocker, direction)
                {
                    target_max = target_max.min(blocker.min.x - padding);
                }
            }
            Point2D::new(target_max - bounds.max.x, 0.0)
        }
        DockDirection::Up => {
            let mut target_min = workspace.min.y + padding;
            for blocker in blockers {
                if blocker.max.y <= bounds.min.y + 1e-9
                    && dock_axis_overlap(&bounds, blocker, direction)
                {
                    target_min = target_min.max(blocker.max.y + padding);
                }
            }
            Point2D::new(0.0, target_min - bounds.min.y)
        }
        DockDirection::Down => {
            let mut target_max = workspace.max.y - padding;
            for blocker in blockers {
                if blocker.min.y >= bounds.max.y - 1e-9
                    && dock_axis_overlap(&bounds, blocker, direction)
                {
                    target_max = target_max.min(blocker.min.y - padding);
                }
            }
            Point2D::new(0.0, target_max - bounds.max.y)
        }
    }
}

pub fn create_project(ctx: &ServiceContext, name: &str) -> ServiceResult<Project> {
    let active_profile = {
        let settings = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
        match settings.active_profile_id {
            Some(id) => settings
                .machine_profiles
                .iter()
                .find(|p| p.id == id)
                .cloned(),
            None => None,
        }
    };

    let mut project = Project::new(name);
    if let Some(profile) = active_profile {
        project.workspace = new_project_workspace_from_machine_profile(&profile);
        project.machine_profile_id = Some(profile.id);
        project.machine_profile_snapshot = Some(profile.snapshot());
    }
    refresh_project_text_caches(&mut project);

    {
        let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
        *project_guard = Some(project.clone());
    }
    {
        let mut path_guard = ctx
            .project_path
            .lock()
            .map_err(|e| lock_err("project_path", e))?;
        *path_guard = None;
    }
    invalidate_plan(ctx)?;

    ctx.clear_project_history()
        .map_err(ServiceError::internal)?;
    ctx.emit_event(
        "project.created",
        json!({
            "project": events::project_summary(&project, None),
        }),
    );
    Ok(project)
}

pub fn get_project(ctx: &ServiceContext) -> ServiceResult<Option<Project>> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    if let Some(project) = project_guard.as_mut() {
        refresh_project_text_caches(project);
    }
    Ok(project_guard.clone())
}

pub fn require_project(ctx: &ServiceContext) -> ServiceResult<Project> {
    let project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    project_guard
        .clone()
        .ok_or_else(|| ServiceError::not_found("No project open"))
}

pub fn close_project(ctx: &ServiceContext) -> ServiceResult<()> {
    let closed_project = {
        let project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
        project_guard.clone()
    };
    let path = current_project_path(ctx)?;
    {
        let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
        *project_guard = None;
    }
    {
        let mut path_guard = ctx
            .project_path
            .lock()
            .map_err(|e| lock_err("project_path", e))?;
        *path_guard = None;
    }
    invalidate_plan(ctx)?;
    ctx.clear_project_history()
        .map_err(ServiceError::internal)?;
    if let Some(project) = closed_project {
        ctx.emit_event(
            "project.closed",
            json!({
                "project": events::project_summary(&project, path.as_deref()),
            }),
        );
    }
    Ok(())
}

pub fn replace_project(ctx: &ServiceContext, project: Project) -> ServiceResult<()> {
    {
        let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
        let asset_data = guard
            .as_ref()
            .map(|p| p.asset_data.clone())
            .unwrap_or_default();
        let mut restored = project;
        restored.asset_data = asset_data;
        refresh_project_text_caches(&mut restored);
        restored.dirty = true;
        *guard = Some(restored);
    }
    invalidate_plan(ctx)?;
    Ok(())
}

pub fn replace_project_document(ctx: &ServiceContext, project: Project) -> ServiceResult<()> {
    replace_project(ctx, project)?;
    {
        let mut path_guard = ctx
            .project_path
            .lock()
            .map_err(|e| lock_err("project_path", e))?;
        *path_guard = None;
    }
    ctx.clear_project_history()
        .map_err(ServiceError::internal)?;
    let project = require_project(ctx)?;
    ctx.emit_event(
        "project.replaced",
        json!({
            "project": events::project_summary(&project, None),
        }),
    );
    Ok(())
}

pub fn bind_active_machine_profile(ctx: &ServiceContext) -> ServiceResult<Project> {
    let (profile_id, snapshot, workspace) = {
        let settings = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
        let profile_id = settings
            .active_profile_id
            .ok_or_else(|| ServiceError::not_found("No active machine profile"))?;
        let profile = settings
            .machine_profiles
            .iter()
            .find(|p| p.id == profile_id)
            .ok_or_else(|| ServiceError::not_found("Active machine profile not found"))?;
        (
            profile.id,
            profile.snapshot(),
            workspace_from_machine_profile(profile),
        )
    };

    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    project.machine_profile_id = Some(profile_id);
    project.machine_profile_snapshot = Some(snapshot);
    project.workspace = workspace;
    project.dirty = true;
    let result = project.clone();
    invalidate_plan(ctx)?;
    drop(project_guard);
    ctx.emit_event(
        "project.object.updated",
        json!({
            "project_id": result.metadata.project_id,
            "machine_profile_id": profile_id,
        }),
    );
    Ok(result)
}

pub fn current_project_path(ctx: &ServiceContext) -> ServiceResult<Option<PathBuf>> {
    let guard = ctx
        .project_path
        .lock()
        .map_err(|e| lock_err("project_path", e))?;
    Ok(guard.clone())
}

pub fn undo_project(ctx: &ServiceContext) -> ServiceResult<Project> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let current = project_guard
        .as_ref()
        .ok_or_else(|| ServiceError::not_found("No project open"))?
        .clone();
    let restored = {
        let mut history = ctx.history.lock().map_err(|e| lock_err("history", e))?;
        history
            .undo(&current)
            .ok_or_else(|| ServiceError::invalid_state("Nothing to undo"))?
    };
    *project_guard = Some(restored.clone());
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.undo",
        json!({
            "project": events::project_summary(&restored, current_project_path(ctx)?.as_deref()),
        }),
    );
    Ok(restored)
}

pub fn redo_project(ctx: &ServiceContext) -> ServiceResult<Project> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let current = project_guard
        .as_ref()
        .ok_or_else(|| ServiceError::not_found("No project open"))?
        .clone();
    let restored = {
        let mut history = ctx.history.lock().map_err(|e| lock_err("history", e))?;
        history
            .redo(&current)
            .ok_or_else(|| ServiceError::invalid_state("Nothing to redo"))?
    };
    *project_guard = Some(restored.clone());
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.redo",
        json!({
            "project": events::project_summary(&restored, current_project_path(ctx)?.as_deref()),
        }),
    );
    Ok(restored)
}

pub fn get_layers(ctx: &ServiceContext) -> ServiceResult<Vec<Layer>> {
    Ok(require_project(ctx)?.layers)
}

pub fn add_layer(ctx: &ServiceContext, input: AddLayerInput) -> ServiceResult<Layer> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    let standard_count = PALETTE_COLORS.iter().filter(|p| !p.is_tool_layer).count();
    let palette_idx = project.layers.len() % standard_count;
    let mut layer = Layer::new_single_entry(input.name, input.operation);
    layer.color_tag = ColorTag(PALETTE_COLORS[palette_idx].hex.to_string());
    if layer.primary_entry().operation == OperationType::Tool {
        layer.canonicalize_tool_layer();
    }
    let added = project.add_layer(layer).clone();
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.layer.added",
        json!({
            "layer": events::layer_summary(&added),
        }),
    );
    Ok(added)
}

/// If new_rs.pass_through is true AND (it just became true OR dpi changed),
/// Resolve the native pixel dimensions for an object, transparently
/// following a `VirtualClone` chain to the underlying `RasterImage`
/// source. Returns `None` for any object whose effective data is not
/// a raster image.
fn resolve_raster_native_px(project: &Project, object_id: ObjectId) -> Option<(u32, u32)> {
    let obj = project.find_object(object_id)?;
    match &obj.data {
        ObjectData::RasterImage {
            original_width_px,
            original_height_px,
            ..
        } => Some((*original_width_px, *original_height_px)),
        ObjectData::VirtualClone { source_id } => resolve_raster_native_px(project, *source_id),
        _ => None,
    }
}

fn resize_passthrough_targets(
    project: &mut Project,
    targets: Vec<(ObjectId, (u32, u32))>,
    dpi: u32,
) {
    for (id, (orig_w, orig_h)) in targets {
        if let Some(obj) = project.find_object_mut(id) {
            let w = orig_w as f64 / dpi as f64 * 25.4;
            let h = orig_h as f64 / dpi as f64 * 25.4;
            let cx = (obj.bounds.min.x + obj.bounds.max.x) / 2.0;
            let cy = (obj.bounds.min.y + obj.bounds.max.y) / 2.0;
            obj.bounds = Bounds::new(
                Point2D::new(cx - w / 2.0, cy - h / 2.0),
                Point2D::new(cx + w / 2.0, cy + h / 2.0),
            );
        }
    }
}

pub fn sync_passthrough_layer_objects(project: &mut Project, layer_id: LayerId) {
    let Some(rs) = project
        .find_layer(layer_id)
        .and_then(|l| l.primary_entry().raster_settings.as_ref())
    else {
        return;
    };
    if !rs.pass_through {
        return;
    }

    let dpi = rs.effective_dpi();
    let targets: Vec<(ObjectId, (u32, u32))> = project
        .objects
        .iter()
        .filter(|o| o.layer_id == layer_id)
        .filter_map(|o| resolve_raster_native_px(project, o.id).map(|px| (o.id, px)))
        .collect();

    resize_passthrough_targets(project, targets, dpi);
}

/// resize all RasterImage objects on this layer to native-DPI size.
/// Must be called while project is still mutably held and AFTER raster_settings
/// have been written to the layer.
///
/// Also resizes `VirtualClone` objects whose source is a raster — the
/// planner resolves clones back to the source's pixel data at plan
/// time while keeping the clone's own bounds, so a stale clone bound
/// would print at the wrong physical size.
pub fn sync_passthrough_bounds(
    project: &mut Project,
    layer_id: LayerId,
    old_rs: Option<&RasterSettings>,
    new_rs: &RasterSettings,
) {
    let was_pt = old_rs.is_some_and(|r| r.pass_through);
    let old_dpi = old_rs.map_or(0, |r| r.effective_dpi());
    let new_dpi = new_rs.effective_dpi();
    let needs_sync = new_rs.pass_through && (!was_pt || new_dpi != old_dpi);
    if !needs_sync {
        return;
    }

    // Collect (object_id, native_px) pairs up front so the mutable
    // loop below doesn't alias the read-only `resolve_raster_native_px`
    // walk.
    let targets: Vec<(ObjectId, (u32, u32))> = project
        .objects
        .iter()
        .filter(|o| o.layer_id == layer_id)
        .filter_map(|o| resolve_raster_native_px(project, o.id).map(|px| (o.id, px)))
        .collect();

    resize_passthrough_targets(project, targets, new_dpi);
}

/// If the target layer has pass_through enabled, resize a single
/// raster-backed object to its native-DPI physical size. Handles
/// both concrete `RasterImage` objects and `VirtualClone`s that
/// resolve to a raster source. Call after adding or importing an
/// image onto a layer that may already have pass_through = true.
pub fn sync_passthrough_single_object(project: &mut Project, object_id: ObjectId) {
    let Some(layer_id) = project.find_object(object_id).map(|o| o.layer_id) else {
        return;
    };
    let Some(rs) = project
        .find_layer(layer_id)
        .and_then(|l| l.primary_entry().raster_settings.as_ref())
    else {
        return;
    };
    if !rs.pass_through {
        return;
    }
    let dpi = rs.effective_dpi();
    let Some((orig_w, orig_h)) = resolve_raster_native_px(project, object_id) else {
        return;
    };
    resize_passthrough_targets(project, vec![(object_id, (orig_w, orig_h))], dpi);
}

pub fn update_layer(
    ctx: &ServiceContext,
    layer_id: LayerId,
    input: UpdateLayerInput,
) -> ServiceResult<Layer> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    let existing = project
        .find_layer(layer_id)
        .cloned()
        .ok_or_else(|| ServiceError::not_found("Layer not found"))?;

    let mut candidate = existing.clone();
    if let Some(name) = input.name {
        candidate.name = name;
    }
    if let Some(enabled) = input.enabled {
        candidate.enabled = enabled;
    }
    if let Some(visible) = input.visible {
        candidate.visible = visible;
    }
    if let Some(ref color) = input.color_tag {
        candidate.color_tag = ColorTag(canonical_palette_color_tag(color).to_string());
        candidate.is_tool_layer = beambench_common::is_tool_color(&candidate.color_tag.0);
        if candidate.is_tool_layer {
            candidate.canonicalize_tool_layer();
        } else if existing.is_tool_layer
            || candidate.primary_entry().operation == OperationType::Tool
        {
            candidate.entries = vec![CutEntry::new(OperationType::Line)];
        }
    }
    if candidate == existing {
        return Ok(existing);
    }

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    *project
        .find_layer_mut(layer_id)
        .ok_or_else(|| ServiceError::not_found("Layer not found"))? = candidate;
    project.dirty = true;
    let updated = project
        .find_layer(layer_id)
        .ok_or_else(|| ServiceError::internal("layer not found after mutation"))?
        .clone();
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.layer.updated",
        json!({
            "layer": events::layer_summary(&updated),
        }),
    );
    Ok(updated)
}

pub fn remove_layer(ctx: &ServiceContext, layer_id: LayerId) -> ServiceResult<()> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    let layer = if let Some(layer) = project.find_layer(layer_id).cloned() {
        layer
    } else {
        return Err(ServiceError::not_found("Layer not found"));
    };
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    if !project.remove_layer(layer_id) {
        return Err(ServiceError::not_found("Layer not found"));
    }
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.layer.removed",
        json!({
            "layer": events::layer_summary(&layer),
        }),
    );
    Ok(())
}

pub fn add_cut_entry(
    ctx: &ServiceContext,
    layer_id: LayerId,
    after_entry_id: Option<CutEntryId>,
) -> ServiceResult<CutEntry> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    let layer = project
        .find_layer(layer_id)
        .ok_or_else(|| ServiceError::not_found("Layer not found"))?;
    if layer.is_tool_layer {
        return Err(ServiceError::invalid_input(
            "Tool layers do not support cut settings",
        ));
    }
    if layer.entries.len() >= 11 {
        return Err(ServiceError::invalid_input(
            "A layer can have at most 11 sub-layers",
        ));
    }
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    let entry = project
        .add_cut_entry(layer_id, after_entry_id)
        .ok_or_else(|| ServiceError::not_found("Cut entry insertion point not found"))?;
    let layer = project
        .find_layer(layer_id)
        .ok_or_else(|| ServiceError::not_found("Layer not found"))?
        .clone();
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.layer.updated",
        json!({ "layer": events::layer_summary(&layer) }),
    );
    Ok(entry)
}

/// Decide each layer's new flag value under a `LayerBatchToggle` mode.
fn apply_batch_toggle(current: bool, layer_id: LayerId, mode: &LayerBatchToggle) -> bool {
    match mode {
        LayerBatchToggle::AllOn => true,
        LayerBatchToggle::AllOff => false,
        LayerBatchToggle::Invert => !current,
        LayerBatchToggle::OnlyThisOn { keep } => layer_id == *keep,
    }
}

/// Generic atomic batch toggle for either `enabled` or `visible`. Pulls/pushes via the supplied
/// closures so a single template handles both flag families. One undo snapshot, one cache
/// invalidation, one event; no-op short-circuit when no layer changes.
fn apply_layer_batch_toggle(
    ctx: &ServiceContext,
    mode: LayerBatchToggle,
    event_name: &'static str,
    get: fn(&Layer) -> bool,
    set: fn(&mut Layer, bool),
) -> ServiceResult<Vec<Layer>> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    // Validate `OnlyThisOn { keep }` references a real layer. Without this guard, a stale row
    // action or direct IPC call with a bogus id would silently turn EVERY layer off/hidden
    // instead of erroring out — the row action's intent ("disable all but THIS") becomes
    // "disable all" once `keep` matches no layer, which is not what the user asked for.
    if let LayerBatchToggle::OnlyThisOn { keep } = &mode {
        if !project.layers.iter().any(|l| l.id == *keep) {
            return Err(ServiceError::not_found("Layer not found"));
        }
    }

    // Decide and short-circuit before snapshotting undo.
    let mut any_changed = false;
    let plan: Vec<(LayerId, bool, bool)> = project
        .layers
        .iter()
        .map(|l| {
            let current = get(l);
            let next = apply_batch_toggle(current, l.id, &mode);
            if current != next {
                any_changed = true;
            }
            (l.id, current, next)
        })
        .collect();
    if !any_changed {
        return Ok(project.layers.clone());
    }

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    for (layer_id, _, next) in plan {
        if let Some(l) = project.find_layer_mut(layer_id) {
            set(l, next);
        }
    }
    project.dirty = true;
    let layers = project.layers.clone();
    drop(project_guard);
    invalidate_plan(ctx)?;
    let layer_ids: Vec<String> = layers.iter().map(|l| l.id.to_string()).collect();
    ctx.emit_event(event_name, json!({ "layer_ids": layer_ids }));
    Ok(layers)
}

/// M4: re-stamp `order_index` for every layer per `Layer::cut_strength` (stable, ascending).
///
/// Atomic: one undo snapshot, one cache invalidation, one event. No-op short-circuit when the
/// computed order matches the current order (idempotent on repeat invocation).
pub fn sort_layers_cut_last(ctx: &ServiceContext) -> ServiceResult<Vec<Layer>> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    if project.layers.is_empty() {
        return Ok(Vec::new());
    }

    // Build (cut_strength, original_index, layer_id) tuples and stable-sort by strength.
    let mut indexed: Vec<(f64, usize, LayerId)> = project
        .layers
        .iter()
        .enumerate()
        .map(|(i, l)| (l.cut_strength(), i, l.id))
        .collect();
    indexed.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.cmp(&b.1))
    });

    // No-op short-circuit: compare against the actual `project.layers` Vec order, which is what
    // the frontend renders. (Comparing only against `order_index` ordering would miss the case
    // where a previous run restamped order_index but didn't reorder the Vec — the second call
    // would then claim "already sorted" while the rendered list still showed the old order.)
    let target_id_order: Vec<LayerId> = indexed.iter().map(|t| t.2).collect();
    let current_vec_order: Vec<LayerId> = project.layers.iter().map(|l| l.id).collect();
    if target_id_order == current_vec_order {
        return Ok(project.layers.clone());
    }

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;

    // Reorder the Vec to match target order, AND re-stamp order_index. The Vec is the source of
    // truth for display order; order_index is its serialized mirror. Both must agree after this
    // call so subsequent no-op checks and frontend renders see a consistent ordering.
    let mut by_id: std::collections::HashMap<LayerId, Layer> =
        project.layers.drain(..).map(|l| (l.id, l)).collect();
    for (new_idx, layer_id) in target_id_order.iter().enumerate() {
        if let Some(mut l) = by_id.remove(layer_id) {
            l.order_index = new_idx as u32;
            project.layers.push(l);
        }
    }
    project.dirty = true;
    let layers = project.layers.clone();
    drop(project_guard);
    invalidate_plan(ctx)?;
    let layer_ids: Vec<String> = target_id_order.iter().map(|id| id.to_string()).collect();
    ctx.emit_event(
        "project.layers.reordered",
        json!({ "layer_ids": layer_ids }),
    );
    Ok(layers)
}

/// M4: batch toggle for `Layer.enabled` (Output column).
pub fn set_all_layers_enabled(
    ctx: &ServiceContext,
    mode: LayerBatchToggle,
) -> ServiceResult<Vec<Layer>> {
    apply_layer_batch_toggle(
        ctx,
        mode,
        "project.layers.updated",
        |l| l.enabled,
        |l, v| l.enabled = v,
    )
}

/// M4: batch toggle for `Layer.visible` (Show column).
pub fn set_all_layers_visible(
    ctx: &ServiceContext,
    mode: LayerBatchToggle,
) -> ServiceResult<Vec<Layer>> {
    apply_layer_batch_toggle(
        ctx,
        mode,
        "project.layers.updated",
        |l| l.visible,
        |l, v| l.visible = v,
    )
}

/// M4: reset a single cut entry to built-in defaults for its operation.
///
/// Preserves the entry's `id` so undo/render references stay stable. No-op short-circuit when
/// the entry is already at defaults — avoids undo/cache churn matching `set_optimization`.
pub fn reset_cut_entry_to_defaults(
    ctx: &ServiceContext,
    layer_id: LayerId,
    entry_id: CutEntryId,
) -> ServiceResult<CutEntry> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    let layer = project
        .find_layer(layer_id)
        .ok_or_else(|| ServiceError::not_found("Layer not found"))?;
    if layer.is_tool_layer {
        return Err(ServiceError::invalid_input(
            "Tool layers do not support cut settings",
        ));
    }
    let existing = layer
        .entries
        .iter()
        .find(|e| e.id == entry_id)
        .ok_or_else(|| ServiceError::not_found("Cut entry not found"))?
        .clone();
    let mut defaults = CutEntry::defaults_for(existing.operation);
    defaults.id = existing.id;

    if defaults == existing {
        return Ok(existing);
    }

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    let layer = project
        .find_layer_mut(layer_id)
        .ok_or_else(|| ServiceError::not_found("Layer not found"))?;
    let entry = layer
        .entries
        .iter_mut()
        .find(|e| e.id == entry_id)
        .ok_or_else(|| ServiceError::not_found("Cut entry not found"))?;
    *entry = defaults.clone();
    project.dirty = true;
    let layer_clone = project
        .find_layer(layer_id)
        .expect("layer existed above")
        .clone();
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.layer.updated",
        json!({ "layer": events::layer_summary(&layer_clone) }),
    );
    Ok(defaults)
}

/// M4: replace a layer's `entries[]` stack from a clipboard template.
///
/// Atomic: one undo snapshot, one cache invalidation, one event. No-op short-circuit when the
/// new stack equals the existing stack except for entry IDs (so re-pasting the same clipboard
/// onto the same layer doesn't churn undo history).
pub fn paste_layer_entries(
    ctx: &ServiceContext,
    layer_id: LayerId,
    templates: Vec<CutEntryTemplate>,
) -> ServiceResult<Layer> {
    if templates.is_empty() {
        return Err(ServiceError::invalid_input(
            "Cannot paste an empty clipboard — a layer must have at least one entry",
        ));
    }
    if templates.len() > 11 {
        return Err(ServiceError::invalid_input(
            "A layer can have at most 11 sub-layers",
        ));
    }
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    // No-op short-circuit: compare templates against current entries' content (id-stripped).
    let current_templates: Vec<CutEntryTemplate> = {
        let layer = project
            .find_layer(layer_id)
            .ok_or_else(|| ServiceError::not_found("Layer not found"))?;
        if layer.is_tool_layer {
            return Err(ServiceError::invalid_input(
                "Tool layers do not support cut settings",
            ));
        }
        layer
            .entries
            .iter()
            .map(CutEntryTemplate::from_entry)
            .collect()
    };
    if current_templates == templates {
        let layer = project
            .find_layer(layer_id)
            .expect("layer existed above")
            .clone();
        return Ok(layer);
    }

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    project
        .replace_layer_entries(layer_id, templates)
        .ok_or_else(|| ServiceError::not_found("Layer not found"))?;
    let layer = project
        .find_layer(layer_id)
        .ok_or_else(|| ServiceError::not_found("Layer not found"))?
        .clone();
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.layer.updated",
        json!({ "layer": events::layer_summary(&layer) }),
    );
    Ok(layer)
}

pub fn remove_cut_entry(
    ctx: &ServiceContext,
    layer_id: LayerId,
    entry_id: CutEntryId,
) -> ServiceResult<()> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    let layer = project
        .find_layer(layer_id)
        .ok_or_else(|| ServiceError::not_found("Layer not found"))?;
    if layer.is_tool_layer {
        return Err(ServiceError::invalid_input(
            "Tool layers do not support cut settings",
        ));
    }
    if layer.entries.len() <= 1 {
        return Err(ServiceError::invalid_input(
            "A layer must keep at least one sub-layer",
        ));
    }
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    if !project.remove_cut_entry(layer_id, entry_id) {
        return Err(ServiceError::not_found("Cut entry not found"));
    }
    let layer = project
        .find_layer(layer_id)
        .ok_or_else(|| ServiceError::not_found("Layer not found"))?
        .clone();
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.layer.updated",
        json!({ "layer": events::layer_summary(&layer) }),
    );
    Ok(())
}

pub fn reorder_cut_entry(
    ctx: &ServiceContext,
    layer_id: LayerId,
    entry_id: CutEntryId,
    new_index: usize,
) -> ServiceResult<Layer> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    let layer = project
        .find_layer(layer_id)
        .ok_or_else(|| ServiceError::not_found("Layer not found"))?;
    if layer.is_tool_layer {
        return Err(ServiceError::invalid_input(
            "Tool layers do not support cut settings",
        ));
    }
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    if !project.reorder_cut_entry(layer_id, entry_id, new_index) {
        return Err(ServiceError::not_found("Cut entry not found"));
    }
    let layer = project
        .find_layer(layer_id)
        .ok_or_else(|| ServiceError::not_found("Layer not found"))?
        .clone();
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.layer.updated",
        json!({ "layer": events::layer_summary(&layer) }),
    );
    Ok(layer)
}

pub fn update_cut_entry(
    ctx: &ServiceContext,
    layer_id: LayerId,
    entry_id: CutEntryId,
    patch: CutEntryPatch,
) -> ServiceResult<CutEntry> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    let existing = project
        .find_layer(layer_id)
        .and_then(|layer| layer.entries.iter().find(|entry| entry.id == entry_id))
        .cloned()
        .ok_or_else(|| ServiceError::not_found("Cut entry not found"))?;
    if project
        .find_layer(layer_id)
        .is_some_and(|layer| layer.is_tool_layer)
    {
        return Err(ServiceError::invalid_input(
            "Tool layers do not support cut settings",
        ));
    }
    let mut candidate = existing.clone();
    if !candidate.apply_patch(&patch) {
        return Ok(existing);
    }
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    let changed = project
        .update_cut_entry(layer_id, entry_id, &patch)
        .ok_or_else(|| ServiceError::not_found("Cut entry not found"))?;
    if !changed {
        return Ok(existing);
    }
    let layer = project
        .find_layer(layer_id)
        .ok_or_else(|| ServiceError::not_found("Layer not found"))?
        .clone();
    let updated = layer
        .entries
        .iter()
        .find(|entry| entry.id == entry_id)
        .cloned()
        .ok_or_else(|| ServiceError::not_found("Cut entry not found"))?;
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.layer.updated",
        json!({ "layer": events::layer_summary(&layer) }),
    );
    Ok(updated)
}

pub fn reorder_layer(
    ctx: &ServiceContext,
    layer_id: LayerId,
    new_index: usize,
) -> ServiceResult<Vec<Layer>> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    if project.find_layer(layer_id).is_none() {
        return Err(ServiceError::not_found("Layer not found"));
    }
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    if !project.reorder_layer(layer_id, new_index) {
        return Err(ServiceError::not_found("Layer not found"));
    }
    let layers = project.layers.clone();
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.layer.reordered",
        json!({
            "layer_id": layer_id,
            "new_index": new_index,
            "layer_ids": layers.iter().map(|layer| layer.id).collect::<Vec<_>>(),
        }),
    );
    Ok(layers)
}

pub fn get_objects(ctx: &ServiceContext) -> ServiceResult<Vec<ProjectObject>> {
    Ok(require_project(ctx)?.objects)
}

pub fn add_object(ctx: &ServiceContext, input: AddObjectInput) -> ServiceResult<ProjectObject> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    // Resolve layer — use requested layer if it exists, otherwise fall back to first layer or auto-create
    let layer_id = if project.find_layer(input.layer_id).is_some() {
        input.layer_id
    } else if project.layers.is_empty() {
        project.ensure_default_layer()
    } else {
        // Requested layer doesn't exist (may have been cleaned up); fall back to first layer
        project.layers[0].id
    };
    // Enforce the raster/vector layer-content invariant before mutating.
    // Frontend callers should have already resolved to a matching layer
    // type; this is the belt-and-suspenders check for API/CLI paths.
    {
        let dest_layer = project
            .find_layer(layer_id)
            .ok_or_else(|| ServiceError::not_found("Layer not found"))?;
        crate::validation::check_layer_content_invariant(&input.object_data, dest_layer, project)?;
    }
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    let mut obj = ProjectObject::new(input.name, layer_id, input.bounds, input.object_data);
    let project_snapshot = project.clone();
    refresh_text_object_cache_with_project_context(&project_snapshot, &mut obj, None);
    let added_id = project.add_object(obj).id;
    // If target layer has pass-through enabled, resize raster images to native-DPI size
    sync_passthrough_single_object(project, added_id);
    let added = project
        .find_object(added_id)
        .ok_or_else(|| ServiceError::internal("Object not found after add"))?
        .clone();
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.object.added",
        json!({
            "object": events::object_summary(&added),
        }),
    );
    Ok(added)
}

pub fn add_object_atomic(
    ctx: &ServiceContext,
    input: AddObjectAtomicInput,
) -> ServiceResult<AddObjectAtomicResult> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    let mut created_layer: Option<Layer> = None;
    let layer_id = if let Some(spec) = input.create_layer.clone() {
        ctx.push_project_undo_snapshot(project)
            .map_err(ServiceError::internal)?;
        let mut layer = Layer::new_single_entry(spec.name, spec.operation);
        if let Some(color_tag) = spec.color_tag {
            layer.color_tag = ColorTag(canonical_palette_color_tag(&color_tag).to_string());
            layer.is_tool_layer = beambench_common::is_tool_color(&layer.color_tag.0);
        }
        if layer.is_tool_layer || layer.primary_entry().operation == OperationType::Tool {
            layer.canonicalize_tool_layer();
        } else if let Some(patch) = spec.entry_patch {
            layer.entries[0].apply_patch(&patch);
        }
        let created = project.add_layer(layer).clone();
        let created_id = created.id;
        created_layer = Some(created);
        created_id
    } else if project.find_layer(input.layer_id).is_some() {
        input.layer_id
    } else if project.layers.is_empty() {
        project.ensure_default_layer()
    } else {
        project.layers[0].id
    };

    {
        let dest_layer = project
            .find_layer(layer_id)
            .ok_or_else(|| ServiceError::not_found("Layer not found"))?;
        crate::validation::check_layer_content_invariant(&input.object_data, dest_layer, project)?;
    }

    if created_layer.is_none() {
        ctx.push_project_undo_snapshot(project)
            .map_err(ServiceError::internal)?;
    }

    let mut obj = ProjectObject::new(input.name, layer_id, input.bounds, input.object_data);
    let project_snapshot = project.clone();
    refresh_text_object_cache_with_project_context(&project_snapshot, &mut obj, None);
    let added_id = project.add_object(obj).id;
    sync_passthrough_single_object(project, added_id);
    let added = project
        .find_object(added_id)
        .ok_or_else(|| ServiceError::internal("Object not found after add"))?
        .clone();
    drop(project_guard);
    invalidate_plan(ctx)?;
    if let Some(layer) = &created_layer {
        ctx.emit_event(
            "project.layer.added",
            json!({
                "layer": events::layer_summary(layer),
            }),
        );
    }
    ctx.emit_event(
        "project.object.added",
        json!({
            "object": events::object_summary(&added),
        }),
    );
    Ok(AddObjectAtomicResult {
        object: added,
        created_layer,
    })
}

pub fn update_object(
    ctx: &ServiceContext,
    object_id: ObjectId,
    input: UpdateObjectInput,
) -> ServiceResult<ProjectObject> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    validate_update_object_patch_in_project(project, object_id, &input, None)?;
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    let updated = apply_update_object_patch_in_project(project, object_id, input, None)?;
    project.dirty = true;
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.object.updated",
        json!({
            "object": events::object_summary(&updated),
        }),
    );
    Ok(updated)
}

fn validate_update_object_patch_in_project(
    project: &Project,
    object_id: ObjectId,
    input: &UpdateObjectInput,
    data: Option<&ObjectData>,
) -> ServiceResult<()> {
    let current = project
        .find_object(object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    let layer_id = input.layer_id.unwrap_or(current.layer_id);
    let layer = project
        .find_layer(layer_id)
        .ok_or_else(|| ServiceError::not_found("Layer not found"))?;
    let object_data = data.unwrap_or(&current.data);
    crate::validation::check_layer_content_invariant(object_data, layer, project)?;
    Ok(())
}

fn apply_update_object_patch_in_project(
    project: &mut Project,
    object_id: ObjectId,
    input: UpdateObjectInput,
    data: Option<ObjectData>,
) -> ServiceResult<ProjectObject> {
    validate_update_object_patch_in_project(project, object_id, &input, data.as_ref())?;

    let current_data = project
        .find_object(object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?
        .data
        .clone();
    let guide = lookup_guide_path_for_text(data.as_ref().unwrap_or(&current_data), project);
    let project_snapshot = project.clone();

    {
        let obj = project
            .find_object_mut(object_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?;
        if let Some(name) = input.name {
            obj.name = name;
        }
        if let Some(visible) = input.visible {
            obj.visible = visible;
        }
        if let Some(locked) = input.locked {
            obj.locked = locked;
        }
        if let Some(layer_id) = input.layer_id {
            obj.layer_id = layer_id;
        }
        if let Some(transform) = input.transform {
            obj.transform = transform;
        }
        if let Some(lock_aspect_ratio) = input.lock_aspect_ratio {
            obj.lock_aspect_ratio = lock_aspect_ratio;
        }
        if let Some(power_scale) = input.power_scale {
            obj.power_scale = power_scale;
        }
        if let Some(data) = data {
            let placement_bounds = obj.bounds;
            if let Some(intrinsic_bounds) = intrinsic_text_bounds(&data) {
                let is_straight = match &data {
                    ObjectData::Text {
                        layout_mode,
                        on_path,
                        ..
                    } => {
                        let effective = if *on_path && *layout_mode == TextLayoutMode::Straight {
                            TextLayoutMode::Path
                        } else {
                            *layout_mode
                        };
                        effective == TextLayoutMode::Straight
                    }
                    _ => false,
                };
                let width = intrinsic_bounds.width().max(0.0);
                let height = intrinsic_bounds.height().max(0.0);
                if is_straight {
                    obj.bounds = rebuild_text_bounds_aligned(&obj.bounds, width, height, &data);
                } else {
                    obj.bounds = Bounds::new(
                        obj.bounds.min,
                        Point2D::new(obj.bounds.min.x + width, obj.bounds.min.y + height),
                    );
                }
            }
            obj.data = data;
            let mapped_bounds = refresh_text_object_cache_with_project_context(
                &project_snapshot,
                obj,
                guide.as_ref(),
            );
            if let Some(mb) = mapped_bounds {
                apply_mapped_text_bounds(obj, &placement_bounds, &mb);
            }
        }
        if let Some(bounds) = input.bounds {
            // Scale dimensional text properties proportionally to the bounds change.
            // Only for straight text (not path/bend) — those geometries are determined
            // by the guide curve or arc, not the bounding box.
            if let ObjectData::Text {
                font_size_mm,
                h_spacing,
                v_spacing,
                max_width,
                layout_mode,
                on_path,
                ..
            } = &mut obj.data
            {
                let is_straight = *layout_mode == TextLayoutMode::Straight && !*on_path;
                if is_straight {
                    let old_w = obj.bounds.width();
                    let old_h = obj.bounds.height();
                    let new_w = bounds.width();
                    let new_h = bounds.height();
                    // Vertical properties scale by height ratio
                    if old_h > 1e-6 {
                        let sy = new_h / old_h;
                        *font_size_mm *= sy;
                        *v_spacing *= sy;
                    }
                    // Horizontal properties scale by width ratio
                    if old_w > 1e-6 {
                        let sx = new_w / old_w;
                        *h_spacing *= sx;
                        if let Some(mw) = max_width {
                            *mw *= sx;
                        }
                    }
                }
            }
            obj.bounds = bounds;
            // Re-resolve text cache so straight text reflows within new box dimensions.
            // For path/bend text, geometry is determined by the guide/arc (not bounds), so
            // we refresh the cache but preserve the user-positioned bounds — otherwise a
            // simple drag would snap the object to the intrinsic geometry origin.
            refresh_text_object_cache_with_project_context(&project_snapshot, obj, guide.as_ref());
        }
        if let Some(priority) = input.priority {
            obj.priority = priority;
        }
    }

    refresh_dependent_text_objects(project, object_id);
    sync_passthrough_single_object(project, object_id);
    let updated = project
        .find_object(object_id)
        .ok_or_else(|| ServiceError::internal("object not found after mutation"))?
        .clone();
    Ok(updated)
}

pub(crate) fn update_object_in_project(
    project: &mut Project,
    object_id: ObjectId,
    input: UpdateObjectInput,
) -> ServiceResult<ProjectObject> {
    apply_update_object_patch_in_project(project, object_id, input, None)
}

pub(crate) fn update_object_patch_in_project(
    project: &mut Project,
    object_id: ObjectId,
    input: UpdateObjectInput,
    data: Option<ObjectData>,
) -> ServiceResult<ProjectObject> {
    apply_update_object_patch_in_project(project, object_id, input, data)
}

pub fn resize_shape_object(
    ctx: &ServiceContext,
    object_id: ObjectId,
    bounds: Bounds,
) -> ServiceResult<ProjectObject> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    let current = project
        .find_object(object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    if !matches!(
        current.data,
        ObjectData::Shape { .. } | ObjectData::Polygon { .. } | ObjectData::Star { .. }
    ) {
        return Err(ServiceError::invalid_input(
            "Object does not support atomic shape resizing",
        ));
    }

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;

    let obj = project
        .find_object_mut(object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    obj.bounds = bounds;
    match &mut obj.data {
        ObjectData::Shape { width, height, .. } => {
            *width = bounds.width();
            *height = bounds.height();
        }
        ObjectData::Polygon { radius, .. } => {
            *radius = bounds.width().min(bounds.height()) / 2.0;
        }
        ObjectData::Star { .. } => {}
        _ => unreachable!("validated supported resize object type"),
    }

    refresh_dependent_text_objects(project, object_id);
    project.dirty = true;
    let updated = project
        .find_object(object_id)
        .ok_or_else(|| ServiceError::internal("object not found after mutation"))?
        .clone();
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.object.updated",
        json!({
            "object": events::object_summary(&updated),
        }),
    );
    Ok(updated)
}

pub fn update_object_data(
    ctx: &ServiceContext,
    object_id: ObjectId,
    data: ObjectData,
) -> ServiceResult<ProjectObject> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    validate_update_object_patch_in_project(
        project,
        object_id,
        &UpdateObjectInput::default(),
        Some(&data),
    )?;
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    let updated = apply_update_object_patch_in_project(
        project,
        object_id,
        UpdateObjectInput::default(),
        Some(data),
    )?;
    project.dirty = true;
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.object.data_updated",
        json!({
            "object": events::object_summary(&updated),
        }),
    );
    Ok(updated)
}

/// atomic apply for the Adjust Image dialog.
///
/// Combines the object-side raster adjustments update and the layer-side
/// raster settings update into a single mutation that pushes ONE undo
/// snapshot and either fully succeeds or is rejected before any change
/// lands. Prevents fractured undo history and partial-apply on failure.
pub fn apply_adjust_image_dialog(
    ctx: &ServiceContext,
    object_id: ObjectId,
    adjustments: Option<beambench_common::RasterAdjustments>,
    layer_id: LayerId,
    raster_settings: RasterSettings,
) -> ServiceResult<(ProjectObject, Layer)> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    // Validate BEFORE pushing undo so a rejected request doesn't waste a snapshot.
    let obj = project
        .find_object(object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    if !matches!(obj.data, ObjectData::RasterImage { .. }) {
        return Err(ServiceError::invalid_input(
            "apply_adjust_image_dialog requires a RasterImage object",
        ));
    }
    if project.find_layer(layer_id).is_none() {
        return Err(ServiceError::not_found("Layer not found"));
    }

    // Single undo snapshot covers both mutations.
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;

    // 1. Apply object-side adjustments.
    if let Some(obj) = project.find_object_mut(object_id) {
        if let ObjectData::RasterImage {
            adjustments: ref mut adj_slot,
            ..
        } = obj.data
        {
            *adj_slot = adjustments;
        }
    }

    // 2. Apply layer-side raster settings.
    if let Some(layer) = project.layers.iter_mut().find(|l| l.id == layer_id) {
        layer.primary_entry_mut().raster_settings = Some(raster_settings);
    }

    project.dirty = true;

    let updated_obj = project
        .find_object(object_id)
        .ok_or_else(|| ServiceError::internal("object not found after mutation"))?
        .clone();
    let updated_layer = project
        .find_layer(layer_id)
        .ok_or_else(|| ServiceError::internal("layer not found after mutation"))?
        .clone();
    drop(project_guard);

    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.object.data_updated",
        json!({
            "object": events::object_summary(&updated_obj),
        }),
    );
    ctx.emit_event(
        "project.layer.updated",
        json!({
            "layer_id": updated_layer.id.to_string(),
        }),
    );

    Ok((updated_obj, updated_layer))
}

pub fn set_text_guide_path(
    ctx: &ServiceContext,
    text_id: ObjectId,
    guide_path_id: Option<ObjectId>,
) -> ServiceResult<ProjectObject> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    // Validate text object exists and is Text variant
    let text_obj = project
        .find_object(text_id)
        .ok_or_else(|| ServiceError::not_found("Text object not found"))?;
    if !matches!(text_obj.data, ObjectData::Text { .. }) {
        return Err(ServiceError::invalid_input("Object is not a text object"));
    }
    // Validate guide path object exists and has usable geometry (if setting, not clearing)
    let guide_vecpath = if let Some(gid) = guide_path_id {
        let guide_obj = project
            .find_object(gid)
            .ok_or_else(|| ServiceError::not_found("Guide path object not found"))?;
        let vp = object_to_world_vecpath_resolved(guide_obj, project).ok_or_else(|| {
            ServiceError::invalid_input(
                "Selected object has no vector geometry that can serve as a guide path",
            )
        })?;
        Some(vp)
    } else {
        None
    };
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    let project_snapshot = project.clone();
    let obj = project
        .find_object_mut(text_id)
        .ok_or_else(|| ServiceError::not_found("Text object not found"))?;
    if let ObjectData::Text {
        guide_path_id: gid,
        layout_mode,
        on_path,
        ..
    } = &mut obj.data
    {
        *gid = guide_path_id;
        if guide_path_id.is_some() {
            *layout_mode = TextLayoutMode::Path;
            *on_path = true;
        } else {
            *layout_mode = TextLayoutMode::Straight;
            *on_path = false;
        }
    }
    let mapped_bounds = refresh_text_object_cache_with_project_context(
        &project_snapshot,
        obj,
        guide_vecpath.as_ref(),
    );
    if let Some(mb) = mapped_bounds {
        obj.bounds = mb;
        obj.transform = Transform2D::identity();
    }
    project.dirty = true;
    let updated = project
        .find_object(text_id)
        .ok_or_else(|| ServiceError::internal("object not found after mutation"))?
        .clone();
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.object.data_updated",
        json!({
            "object": events::object_summary(&updated),
        }),
    );
    Ok(updated)
}

/// Auto-unlink any text objects whose guide_path_id matches one of the removed IDs.
fn unlink_guide_path_references(project: &mut Project, removed_ids: &[ObjectId]) {
    for obj in &mut project.objects {
        if let ObjectData::Text {
            guide_path_id,
            layout_mode,
            on_path,
            ..
        } = &mut obj.data
        {
            if let Some(gid) = guide_path_id {
                if removed_ids.contains(gid) {
                    *guide_path_id = None;
                    *layout_mode = TextLayoutMode::Straight;
                    *on_path = false;
                }
            }
        }
    }
}

pub fn remove_object(ctx: &ServiceContext, object_id: ObjectId) -> ServiceResult<()> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    let object = if let Some(object) = project.find_object(object_id).cloned() {
        object
    } else {
        return Err(ServiceError::not_found("Object not found"));
    };
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    if !project.remove_object(object_id) {
        return Err(ServiceError::not_found("Object not found"));
    }
    unlink_guide_path_references(project, &[object_id]);
    project.clean_empty_layers();
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.object.removed",
        json!({
            "object": events::object_summary(&object),
        }),
    );
    Ok(())
}

pub fn remove_objects(ctx: &ServiceContext, object_ids: &[ObjectId]) -> ServiceResult<usize> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    // Verify at least one object exists
    if !object_ids
        .iter()
        .any(|id| project.find_object(*id).is_some())
    {
        return Err(ServiceError::not_found("No matching objects found"));
    }
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    let removed = project.remove_objects(object_ids);
    unlink_guide_path_references(project, object_ids);
    project.clean_empty_layers();
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.objects.removed",
        json!({
            "count": removed,
            "object_ids": object_ids,
        }),
    );
    Ok(removed)
}

pub fn nudge_objects(
    ctx: &ServiceContext,
    object_ids: &[ObjectId],
    dx: f64,
    dy: f64,
) -> ServiceResult<()> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    if !object_ids
        .iter()
        .any(|id| project.find_object(*id).is_some())
    {
        return Err(ServiceError::not_found("No matching objects found"));
    }
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    beambench_core::operations::nudge_objects(project, object_ids, dx, dy);
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.objects.nudged",
        json!({
            "dx": dx,
            "dy": dy,
            "object_ids": object_ids,
        }),
    );
    Ok(())
}

pub fn duplicate_object(ctx: &ServiceContext, object_id: ObjectId) -> ServiceResult<ProjectObject> {
    duplicate_object_impl(ctx, object_id, false)
}

pub fn duplicate_objects(
    ctx: &ServiceContext,
    object_ids: &[ObjectId],
) -> ServiceResult<Vec<ProjectObject>> {
    duplicate_objects_impl(ctx, object_ids, false)
}

pub fn duplicate_object_in_place(
    ctx: &ServiceContext,
    object_id: ObjectId,
) -> ServiceResult<ProjectObject> {
    duplicate_object_impl(ctx, object_id, true)
}

pub fn duplicate_objects_in_place(
    ctx: &ServiceContext,
    object_ids: &[ObjectId],
) -> ServiceResult<Vec<ProjectObject>> {
    duplicate_objects_impl(ctx, object_ids, true)
}

pub fn paste_objects(
    ctx: &ServiceContext,
    objects: &[ProjectObject],
    in_place: bool,
) -> ServiceResult<Vec<ProjectObject>> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    if objects.is_empty() {
        return Ok(Vec::new());
    }

    let id_map: HashMap<ObjectId, ObjectId> = objects
        .iter()
        .map(|object| (object.id, ObjectId::new()))
        .collect();
    let offset = if in_place { 0.0 } else { 5.0 };

    let mut prepared = Vec::with_capacity(objects.len());
    for template in objects {
        let mut object = template.clone();
        object.id = id_map
            .get(&template.id)
            .copied()
            .unwrap_or_else(ObjectId::new);
        object.name = format!("{} copy", template.name);
        object.bounds = Bounds::new(
            Point2D::new(
                template.bounds.min.x + offset,
                template.bounds.min.y + offset,
            ),
            Point2D::new(
                template.bounds.max.x + offset,
                template.bounds.max.y + offset,
            ),
        );
        remap_object_refs(&mut object.data, &id_map);
        prepared.push(object);
    }
    let pending_data: HashMap<ObjectId, ObjectData> = prepared
        .iter()
        .map(|object| (object.id, object.data.clone()))
        .collect();

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    let mut added = Vec::with_capacity(prepared.len());
    for mut object in prepared {
        let target = paste_routing_target(&object.data, project, &pending_data, 0);
        object.layer_id = resolve_paste_layer(project, object.layer_id, target)?;
        added.push(project.add_object(object).clone());
    }
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.objects.pasted",
        json!({
            "count": added.len(),
            "object_ids": added.iter().map(|object| object.id).collect::<Vec<_>>(),
        }),
    );
    Ok(added)
}

fn resolve_paste_layer(
    project: &mut Project,
    requested: LayerId,
    target: crate::validation::RoutingTarget,
) -> ServiceResult<LayerId> {
    let base_layer_id = if project.find_layer(requested).is_some() {
        requested
    } else if let Some(first) = project.layers.first() {
        first.id
    } else {
        let operation = match target {
            crate::validation::RoutingTarget::NeedsImage => OperationType::Image,
            crate::validation::RoutingTarget::NeedsNonImage => OperationType::Line,
        };
        let name = match target {
            crate::validation::RoutingTarget::NeedsImage => "Image",
            crate::validation::RoutingTarget::NeedsNonImage => "Line",
        };
        let mut layer = Layer::new(name, operation);
        layer.color_tag = ColorTag(
            PALETTE_COLORS
                .iter()
                .find(|entry| !entry.is_tool_layer)
                .map(|entry| entry.hex)
                .unwrap_or("#000000")
                .to_string(),
        );
        return Ok(project.add_layer(layer).id);
    };
    crate::validation::resolve_layer_for_object(project, base_layer_id, target)
        .map(|(layer_id, _)| layer_id)
}

fn paste_routing_target(
    data: &ObjectData,
    project: &Project,
    pending_data: &HashMap<ObjectId, ObjectData>,
    depth: usize,
) -> crate::validation::RoutingTarget {
    if depth > 10 {
        return crate::validation::RoutingTarget::NeedsNonImage;
    }
    match data {
        ObjectData::RasterImage { .. } => crate::validation::RoutingTarget::NeedsImage,
        ObjectData::VirtualClone { source_id } => pending_data
            .get(source_id)
            .map(|source| paste_routing_target(source, project, pending_data, depth + 1))
            .unwrap_or_else(|| crate::validation::RoutingTarget::from_data(data, project)),
        _ => crate::validation::RoutingTarget::NeedsNonImage,
    }
}

fn remap_object_refs(data: &mut ObjectData, id_map: &HashMap<ObjectId, ObjectId>) {
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

fn duplicate_object_impl(
    ctx: &ServiceContext,
    object_id: ObjectId,
    in_place: bool,
) -> ServiceResult<ProjectObject> {
    let mut added = duplicate_objects_impl(ctx, &[object_id], in_place)?;
    if added.is_empty() {
        return Err(ServiceError::invalid_state(
            "Duplicate did not create an object",
        ));
    }
    let added = added
        .drain(..1)
        .next()
        .ok_or_else(|| ServiceError::invalid_state("Duplicate did not create an object"))?;
    Ok(added)
}

fn collect_object_copy_templates(
    project: &Project,
    object_ids: &[ObjectId],
) -> ServiceResult<Vec<ProjectObject>> {
    let mut seen = HashSet::new();
    let mut templates = Vec::new();
    for object_id in object_ids {
        collect_object_copy_template(project, *object_id, &mut seen, &mut templates)?;
    }
    Ok(templates)
}

fn collect_object_copy_template(
    project: &Project,
    object_id: ObjectId,
    seen: &mut HashSet<ObjectId>,
    templates: &mut Vec<ProjectObject>,
) -> ServiceResult<()> {
    if seen.contains(&object_id) {
        return Ok(());
    }
    let object = project
        .find_object(object_id)
        .cloned()
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    seen.insert(object_id);
    let children = match &object.data {
        ObjectData::Group { children } => children.clone(),
        _ => Vec::new(),
    };
    templates.push(object);
    for child_id in children {
        collect_object_copy_template(project, child_id, seen, templates)?;
    }
    Ok(())
}

fn duplicate_objects_impl(
    ctx: &ServiceContext,
    object_ids: &[ObjectId],
    in_place: bool,
) -> ServiceResult<Vec<ProjectObject>> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    if object_ids.is_empty() {
        return Ok(Vec::new());
    }
    let originals = collect_object_copy_templates(project, object_ids)?;
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    let id_map: HashMap<ObjectId, ObjectId> = originals
        .iter()
        .map(|object| (object.id, ObjectId::new()))
        .collect();
    let source_ids: Vec<ObjectId> = originals.iter().map(|object| object.id).collect();
    let offset = if in_place { 0.0 } else { 5.0 };
    let mut added = Vec::with_capacity(originals.len());
    for original in originals {
        let mut dup = original.clone();
        dup.id = id_map
            .get(&original.id)
            .copied()
            .unwrap_or_else(ObjectId::new);
        dup.name = format!("{} copy", original.name);
        dup.bounds = Bounds::new(
            Point2D::new(
                original.bounds.min.x + offset,
                original.bounds.min.y + offset,
            ),
            Point2D::new(
                original.bounds.max.x + offset,
                original.bounds.max.y + offset,
            ),
        );
        remap_object_refs(&mut dup.data, &id_map);
        added.push(project.add_object(dup).clone());
    }
    drop(project_guard);
    invalidate_plan(ctx)?;
    for (source_object_id, object) in source_ids.iter().zip(added.iter()) {
        ctx.emit_event(
            "project.object.duplicated",
            json!({
                "source_object_id": source_object_id,
                "object": events::object_summary(object),
            }),
        );
    }
    Ok(added)
}

pub fn align_objects(
    ctx: &ServiceContext,
    object_ids: Vec<ObjectId>,
    alignment_type: &str,
    anchor_object_id: Option<ObjectId>,
) -> ServiceResult<Vec<ProjectObject>> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    let units = build_arrangement_units(project, &object_ids)?;
    if units.len() < 2 {
        return Ok(Vec::new());
    }
    let mode = match alignment_type {
        "left" => alignment::AnchorAlignment::Left,
        "right" => alignment::AnchorAlignment::Right,
        "top" => alignment::AnchorAlignment::Top,
        "bottom" => alignment::AnchorAlignment::Bottom,
        "centers_xy" => alignment::AnchorAlignment::CentersXy,
        "centers_v" => alignment::AnchorAlignment::CentersV,
        "centers_h" => alignment::AnchorAlignment::CentersH,
        other => {
            return Err(ServiceError::invalid_input(format!(
                "Invalid alignment type: {other}"
            )));
        }
    };

    let locked_indices: Vec<usize> = units
        .iter()
        .enumerate()
        .filter_map(|(idx, unit)| arrangement_unit_is_locked(project, unit).then_some(idx))
        .collect();
    let movable: Vec<bool> = units
        .iter()
        .map(|unit| !arrangement_unit_is_locked(project, unit))
        .collect();
    if !movable.iter().any(|can_move| *can_move) {
        return Ok(Vec::new());
    }
    let anchor_index = if locked_indices.len() == 1 {
        locked_indices[0]
    } else {
        let anchor_root = anchor_object_id
            .map(|id| top_level_group_for_object(project, id))
            .unwrap_or_else(|| units.last().map(|unit| unit.root_id).unwrap());
        let preferred = units
            .iter()
            .position(|unit| unit.root_id == anchor_root)
            .unwrap_or(units.len() - 1);
        if locked_indices.len() > 1 && !movable[preferred] {
            movable
                .iter()
                .enumerate()
                .rev()
                .find_map(|(idx, can_move)| can_move.then_some(idx))
                .unwrap_or(preferred)
        } else {
            preferred
        }
    };
    let bounds_list: Vec<Bounds> = units.iter().map(|unit| unit.bounds).collect();
    let offsets = alignment::align_to_anchor(&bounds_list, mode, anchor_index, &movable);
    let offsets_by_root: HashMap<ObjectId, Point2D> = units
        .iter()
        .zip(offsets.iter())
        .map(|(unit, offset)| {
            let offset = if arrangement_unit_is_locked(project, unit) {
                Point2D::new(0.0, 0.0)
            } else {
                *offset
            };
            (unit.root_id, offset)
        })
        .collect();
    let has_change = offsets_by_root
        .values()
        .any(|offset| offset.x.abs() > 1e-9 || offset.y.abs() > 1e-9);
    if !has_change {
        return Ok(Vec::new());
    }
    let entries = move_entries_for_units(project, &units, &offsets_by_root)?;
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    beambench_core::operations::update_object_bounds_batch(project, &entries);
    project.dirty = true;
    let updated = updated_objects_for_entries(project, &entries)?;
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.object.aligned",
        json!({
            "alignment_type": alignment_type,
            "anchor_object_id": anchor_object_id,
            "object_ids": object_ids,
        }),
    );
    Ok(updated)
}

/// Atomic batch: create offset duplicates with resolved text, all in one undo
/// step. The original object is preserved unchanged so the template can be
/// re-used for subsequent batch runs.
///
/// Bounds are intentionally preserved from the original — text objects use
/// bounds as a fixed container box (like a text frame), not a tight fit.
/// The renderer aligns text within the box using alignment settings.
pub fn generate_variable_text_batch(
    ctx: &ServiceContext,
    object_id: ObjectId,
    copies: Vec<BatchCopyInput>,
    offset_step: f64,
) -> ServiceResult<BatchResult> {
    if copies.is_empty() {
        return Err(ServiceError::invalid_input("No copies provided"));
    }
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    let original = project
        .find_object(object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?
        .clone();
    match &original.data {
        ObjectData::Text { .. } => {}
        _ => return Err(ServiceError::invalid_input("Object is not a text object")),
    }

    // Single undo snapshot before all mutations
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;

    let mut result_ids = Vec::new();
    let copy_count = copies.len();

    // Create duplicates for all copies, each with resolved text + per-copy config.
    for (i, copy) in copies.iter().enumerate() {
        let dx = (i as f64 + 1.0) * offset_step;
        let dy = (i as f64 + 1.0) * offset_step;
        let mut dup = ProjectObject::new(
            format!("{} copy", original.name),
            original.layer_id,
            Bounds::new(
                Point2D::new(original.bounds.min.x + dx, original.bounds.min.y + dy),
                Point2D::new(original.bounds.max.x + dx, original.bounds.max.y + dy),
            ),
            original.data.clone(),
        );
        dup.transform = original.transform;
        dup.visible = original.visible;
        if let ObjectData::Text {
            ref mut content,
            ref mut variable_text,
            ..
        } = dup.data
        {
            *content = copy.resolved_text.clone();
            *variable_text = Some(copy.variable_text_config.clone());
        }
        let project_snapshot = project.clone();
        refresh_text_object_cache_with_project_context(&project_snapshot, &mut dup, None);
        let added = project.add_object(dup);
        result_ids.push(added.id);
    }

    // Advance (or initialize) the original object's variable_text source state.
    // This ensures the first batch run on a fresh object also persists the config
    // in the same undo step — no separate updateObjectData needed on the frontend.
    if let Some(orig) = project.find_object_mut(object_id) {
        if let ObjectData::Text {
            ref mut variable_text,
            ..
        } = orig.data
        {
            let config = variable_text.get_or_insert_with(|| {
                // Initialize from the first copy's config (template + base source)
                copies[0].variable_text_config.clone()
            });
            let delta = config.source.advance_by * copy_count as i64;
            config.source.current = advance_sequence_value(
                config.source.current,
                config.source.start,
                config.source.end,
                delta,
            );
        }
    }

    let updated_original = project
        .find_object(object_id)
        .cloned()
        .ok_or_else(|| ServiceError::not_found("Original not found after update"))?;

    project.dirty = true;
    let result_objects: Vec<ProjectObject> = result_ids
        .iter()
        .filter_map(|id| project.find_object(*id).cloned())
        .collect();
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.batch.generated",
        json!({
            "source_object_id": object_id,
            "count": result_objects.len(),
        }),
    );
    Ok(BatchResult {
        copies: result_objects,
        updated_original,
    })
}

pub fn advance_auto_variable_text(ctx: &ServiceContext) -> ServiceResult<Vec<ProjectObject>> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    let object_ids: Vec<ObjectId> = project
        .objects
        .iter()
        .filter_map(|object| match &object.data {
            ObjectData::Text {
                variable_text: Some(config),
                ..
            } if config.source.auto_advance => Some(object.id),
            _ => None,
        })
        .collect();

    if object_ids.is_empty() {
        return Ok(Vec::new());
    }

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;

    for object in &mut project.objects {
        if !object_ids.contains(&object.id) {
            continue;
        }
        if let ObjectData::Text {
            variable_text: Some(config),
            ..
        } = &mut object.data
        {
            config.source.current = advance_sequence_value(
                config.source.current,
                config.source.start,
                config.source.end,
                config.source.advance_by,
            );
        }
    }

    refresh_project_text_caches(project);
    project.dirty = true;
    let updated = object_ids
        .iter()
        .filter_map(|id| project.find_object(*id).cloned())
        .collect::<Vec<_>>();
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.variable_text.advanced",
        json!({
            "object_ids": object_ids,
        }),
    );
    Ok(updated)
}

pub fn distribute_objects(
    ctx: &ServiceContext,
    object_ids: Vec<ObjectId>,
    direction: &str,
) -> ServiceResult<Vec<ProjectObject>> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    let units = build_arrangement_units(project, &object_ids)?;
    if units.len() < 3 {
        return Err(ServiceError::invalid_input(
            "Distribution requires at least three movable objects",
        ));
    }
    let bounds_list: Vec<Bounds> = units.iter().map(|unit| unit.bounds).collect();
    let offsets = match direction {
        "h_centered" => alignment::distribute_h_centered(&bounds_list),
        "v_centered" => alignment::distribute_v_centered(&bounds_list),
        "h_spaced" => alignment::distribute_h_spaced(&bounds_list),
        "v_spaced" => alignment::distribute_v_spaced(&bounds_list),
        other => {
            return Err(ServiceError::invalid_input(format!(
                "Invalid distribution direction: {other}"
            )));
        }
    };
    let offsets_by_root: HashMap<ObjectId, Point2D> = units
        .iter()
        .zip(offsets.iter())
        .map(|(unit, offset)| {
            let offset = if arrangement_unit_is_locked(project, unit) {
                Point2D::new(0.0, 0.0)
            } else {
                *offset
            };
            (unit.root_id, offset)
        })
        .collect();
    let has_change = offsets_by_root
        .values()
        .any(|offset| offset.x.abs() > 1e-9 || offset.y.abs() > 1e-9);
    if !has_change {
        return Ok(Vec::new());
    }
    let entries = move_entries_for_units(project, &units, &offsets_by_root)?;
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    beambench_core::operations::update_object_bounds_batch(project, &entries);
    project.dirty = true;
    let updated = updated_objects_for_entries(project, &entries)?;
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.object.distributed",
        json!({
            "direction": direction,
            "object_ids": object_ids,
        }),
    );
    Ok(updated)
}

pub fn move_objects_together(
    ctx: &ServiceContext,
    object_ids: Vec<ObjectId>,
    axis: MoveTogetherAxis,
    anchor_object_id: ObjectId,
) -> ServiceResult<Vec<ProjectObject>> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    let units = build_arrangement_units(project, &object_ids)?;
    if units.len() < 2 {
        return Err(ServiceError::invalid_input(
            "Move Together requires at least two movable objects",
        ));
    }
    let anchor_index = units
        .iter()
        .position(|unit| unit.root_id == anchor_object_id)
        .ok_or_else(|| {
            ServiceError::invalid_input("Anchor object is not in the movable selection")
        })?;
    let bounds_list: Vec<Bounds> = units.iter().map(|unit| unit.bounds).collect();
    let offsets = match axis {
        MoveTogetherAxis::Horizontal => alignment::move_together_h(&bounds_list, anchor_index),
        MoveTogetherAxis::Vertical => alignment::move_together_v(&bounds_list, anchor_index),
    };
    let offsets_by_root: HashMap<ObjectId, Point2D> = units
        .iter()
        .zip(offsets.iter())
        .map(|(unit, offset)| (unit.root_id, *offset))
        .collect();
    let has_change = offsets_by_root
        .values()
        .any(|offset| offset.x.abs() > 1e-9 || offset.y.abs() > 1e-9);
    if !has_change {
        return Ok(Vec::new());
    }
    let entries = move_entries_for_units(project, &units, &offsets_by_root)?;
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    beambench_core::operations::update_object_bounds_batch(project, &entries);
    project.dirty = true;
    let updated = updated_objects_for_entries(project, &entries)?;
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.objects.moved_together",
        json!({
            "axis": axis,
            "anchor_object_id": anchor_object_id,
            "object_ids": object_ids,
        }),
    );
    Ok(updated)
}

pub fn mirror_across_line(
    ctx: &ServiceContext,
    object_ids: Vec<ObjectId>,
    axis_object_id: ObjectId,
) -> ServiceResult<Vec<ProjectObject>> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    let (line_start, line_end) = extract_mirror_axis_points(project, axis_object_id)?;
    let source_ids: Vec<ObjectId> = object_ids
        .into_iter()
        .filter(|object_id| *object_id != axis_object_id)
        .collect();
    let source_roots = normalize_arrangement_roots(project, &source_ids);
    if source_roots.is_empty() {
        return Err(ServiceError::invalid_input(
            "Mirror Across Line requires at least one source object",
        ));
    }

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;

    let reflection = reflection_transform_for_line(line_start, line_end);
    let mut created = Vec::new();
    let mut root_duplicates = Vec::new();
    let mut duplicate_member_ids = Vec::new();
    for root_id in source_roots {
        let duplicated_root_id = duplicate_subtree(project, root_id, &mut created)?;
        root_duplicates.push(duplicated_root_id);
        collect_group_members(project, duplicated_root_id, &mut duplicate_member_ids);
    }

    for duplicate_id in &duplicate_member_ids {
        let object = project
            .find_object_mut(*duplicate_id)
            .ok_or_else(|| ServiceError::internal("Mirrored duplicate not found"))?;
        let center = bounds_center(object.bounds);
        let reflected_center = reflect_point_across_line(center, line_start, line_end);
        let delta = Point2D::new(reflected_center.x - center.x, reflected_center.y - center.y);
        object.bounds = translate_bounds(object.bounds, delta.x, delta.y);
        object.transform = reflection.compose(&object.transform);
    }
    project.dirty = true;
    let created_ids: Vec<ObjectId> = created.iter().map(|object| object.id).collect();
    let created = created_ids
        .iter()
        .map(|object_id| {
            project
                .find_object(*object_id)
                .cloned()
                .ok_or_else(|| ServiceError::internal("Mirrored duplicate not found"))
        })
        .collect::<ServiceResult<Vec<_>>>()?;
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.objects.mirrored_across_line",
        json!({
            "axis_object_id": axis_object_id,
            "duplicated_root_ids": root_duplicates,
        }),
    );
    Ok(created)
}

pub fn make_same_size(
    ctx: &ServiceContext,
    object_ids: Vec<ObjectId>,
    anchor_object_id: ObjectId,
    axis: SameSizeAxis,
    preserve_aspect: bool,
) -> ServiceResult<Vec<ProjectObject>> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    let entries = build_same_size_entries(
        project,
        &object_ids,
        anchor_object_id,
        axis,
        preserve_aspect,
    )?;
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    beambench_core::operations::update_object_bounds_batch(project, &entries);
    project.dirty = true;
    let updated = updated_objects_for_entries(project, &entries)?;
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.objects.same_size",
        json!({
            "axis": axis,
            "anchor_object_id": anchor_object_id,
            "object_ids": object_ids,
            "preserve_aspect": preserve_aspect,
        }),
    );
    Ok(updated)
}

pub fn resize_slots(
    ctx: &ServiceContext,
    object_ids: Vec<ObjectId>,
    options: ResizeSlotsOptions,
) -> ServiceResult<Vec<ProjectObject>> {
    if options.current_thickness_mm <= 0.0 {
        return Err(ServiceError::invalid_input(
            "Current thickness must be greater than 0",
        ));
    }
    if options.new_thickness_mm <= 0.0 {
        return Err(ServiceError::invalid_input(
            "New thickness must be greater than 0",
        ));
    }
    if options.tolerance_mm < 0.0 {
        return Err(ServiceError::invalid_input(
            "Tolerance must be greater than or equal to 0",
        ));
    }

    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    let target_slot_width = options.new_thickness_mm + options.tolerance_mm;
    let roots = normalize_arrangement_roots(project, &object_ids);
    if roots.is_empty() {
        return Err(ServiceError::invalid_input(
            "No selectable slot geometry found",
        ));
    }

    let mut changed_ids = Vec::new();
    let mut changed_set = HashSet::new();
    let mut changed_paths: Vec<(ObjectId, ObjectData, Bounds, Transform2D)> = Vec::new();
    let mut changed_bounds: Vec<(ObjectId, Bounds)> = Vec::new();

    for root_id in roots {
        let mut member_ids = Vec::new();
        collect_group_members(project, root_id, &mut member_ids);
        for member_id in member_ids {
            project
                .ensure_resolved(member_id)
                .map_err(ServiceError::invalid_state)?;
            let Some(object) = project.find_object(member_id).cloned() else {
                continue;
            };
            if is_arrangement_excluded_object(project, &object) {
                continue;
            }
            match &object.data {
                ObjectData::Shape {
                    kind: beambench_core::ShapeKind::Rectangle,
                    corner_radius,
                    ..
                } if *corner_radius == 0.0 => {
                    let rect = SlotRect {
                        center: bounds_center(object.bounds),
                        width: object.bounds.width(),
                        height: object.bounds.height(),
                        narrow_axis: if (object.bounds.width() - object.bounds.height()).abs()
                            <= SLOT_SQUARE_TOLERANCE_MM
                        {
                            continue;
                        } else if object.bounds.width() < object.bounds.height() {
                            SlotAxis::Horizontal
                        } else {
                            SlotAxis::Vertical
                        },
                    };
                    if !slot_matches_thickness(rect, options.current_thickness_mm) {
                        continue;
                    }
                    let next = resize_slot_rect(rect, target_slot_width);
                    let next_bounds = Bounds::new(
                        Point2D::new(
                            next.center.x - next.width / 2.0,
                            next.center.y - next.height / 2.0,
                        ),
                        Point2D::new(
                            next.center.x + next.width / 2.0,
                            next.center.y + next.height / 2.0,
                        ),
                    );
                    if (next_bounds.width() - object.bounds.width()).abs() <= 1e-9
                        && (next_bounds.height() - object.bounds.height()).abs() <= 1e-9
                    {
                        continue;
                    }
                    changed_bounds.push((member_id, next_bounds));
                    if changed_set.insert(member_id) {
                        changed_ids.push(member_id);
                    }
                }
                ObjectData::VectorPath {
                    ruler_guide_axis: None,
                    ..
                } => {
                    let Some(world) = object_to_world_vecpath_resolved(&object, project) else {
                        continue;
                    };
                    let mut any_changed = false;
                    let mut next_subpaths = Vec::with_capacity(world.subpaths.len());
                    for subpath in &world.subpaths {
                        if let Some(rect) = axis_aligned_rect_from_subpath(subpath)
                            && slot_matches_thickness(rect, options.current_thickness_mm)
                        {
                            let resized = resize_slot_rect(rect, target_slot_width);
                            let width_changed = (resized.width - rect.width).abs() > 1e-9;
                            let height_changed = (resized.height - rect.height).abs() > 1e-9;
                            if width_changed || height_changed {
                                next_subpaths.push(slot_rect_to_subpath(resized));
                                any_changed = true;
                                continue;
                            }
                        }
                        next_subpaths.push(subpath.clone());
                    }
                    if !any_changed {
                        continue;
                    }
                    let next_world = VecPath {
                        subpaths: next_subpaths,
                    };
                    let Some((normalized, bounds)) = normalize_world_vecpath(next_world) else {
                        continue;
                    };
                    changed_paths.push((
                        member_id,
                        ObjectData::VectorPath {
                            path_data: normalized.to_svg_d(),
                            closed: normalized.subpaths.iter().all(|subpath| subpath.closed),
                            ruler_guide_axis: None,
                        },
                        bounds,
                        Transform2D::identity(),
                    ));
                    if changed_set.insert(member_id) {
                        changed_ids.push(member_id);
                    }
                }
                _ => {}
            }
        }
    }

    if changed_ids.is_empty() {
        return Ok(Vec::new());
    }

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    for (object_id, bounds) in &changed_bounds {
        let object = project
            .find_object_mut(*object_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?;
        object.bounds = *bounds;
        if let ObjectData::Shape { width, height, .. } = &mut object.data {
            *width = bounds.width();
            *height = bounds.height();
        }
    }
    for (object_id, data, bounds, transform) in &changed_paths {
        let object = project
            .find_object_mut(*object_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?;
        object.data = data.clone();
        object.bounds = *bounds;
        object.transform = *transform;
    }
    project.dirty = true;
    let updated = changed_ids
        .iter()
        .map(|object_id| {
            project
                .find_object(*object_id)
                .cloned()
                .ok_or_else(|| ServiceError::internal("Object not found after slot resize"))
        })
        .collect::<ServiceResult<Vec<_>>>()?;
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.objects.slots_resized",
        json!({
            "object_ids": changed_ids,
            "options": options,
        }),
    );
    Ok(updated)
}

pub fn dock_objects(
    ctx: &ServiceContext,
    object_ids: Vec<ObjectId>,
    direction: DockDirection,
    options: DockOptions,
) -> ServiceResult<Vec<ProjectObject>> {
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = project_guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    let units = build_arrangement_units(project, &object_ids)?;
    if units.is_empty() {
        return Err(ServiceError::invalid_input("No movable objects to dock"));
    }
    let selected_member_ids: HashSet<ObjectId> = units
        .iter()
        .flat_map(|unit| unit.member_ids.iter().copied())
        .collect();
    let workspace = Bounds::new(
        Point2D::new(0.0, 0.0),
        Point2D::new(
            project.workspace.bed_width_mm,
            project.workspace.bed_height_mm,
        ),
    );
    let mut blockers = Vec::new();
    blockers.extend(
        project
            .objects
            .iter()
            .filter(|object| !selected_member_ids.contains(&object.id))
            .filter_map(|object| blocker_object_bounds(project, object.id)),
    );

    let mut clusters = build_dock_clusters(&units, &options);
    clusters.sort_by(|a, b| {
        dock_cluster_sort_value(a, direction)
            .partial_cmp(&dock_cluster_sort_value(b, direction))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut entries = Vec::new();
    let mut moved_ids = HashSet::new();
    let mut any_change = false;
    for cluster in &clusters {
        let delta = compute_dock_target(
            cluster.bounds,
            &blockers,
            direction,
            &workspace,
            options.padding_mm,
        );
        if delta.x.abs() > 1e-9 || delta.y.abs() > 1e-9 {
            any_change = true;
        }
        let moved_bounds = translate_bounds(cluster.bounds, delta.x, delta.y);
        if !options.move_as_group {
            blockers.push(moved_bounds);
        }
        for object_id in &cluster.member_ids {
            if !moved_ids.insert(*object_id) {
                continue;
            }
            project
                .ensure_resolved(*object_id)
                .map_err(ServiceError::invalid_state)?;
            let object = project
                .find_object(*object_id)
                .ok_or_else(|| ServiceError::not_found("Object not found"))?;
            entries.push((
                *object_id,
                translate_bounds(object.bounds, delta.x, delta.y),
            ));
        }
    }

    if !any_change || entries.is_empty() {
        return Ok(Vec::new());
    }

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    beambench_core::operations::update_object_bounds_batch(project, &entries);
    project.dirty = true;
    let updated = updated_objects_for_entries(project, &entries)?;
    drop(project_guard);
    invalidate_plan(ctx)?;
    ctx.emit_event(
        "project.objects.docked",
        json!({
            "direction": direction,
            "options": options,
            "object_ids": object_ids,
        }),
    );
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::{Bounds, ColorTag, Point2D};
    use beambench_core::{
        AppSettings, Asset, AssetMediaType, Layer, MachineProfile, ObjectData, OperationType,
        ProjectObject, ShapeKind, TextAlignment, TextAlignmentV, TextLayoutMode, WorkspaceOrigin,
    };

    fn sample_project() -> Project {
        let mut project = Project::new("Test");
        let layer = Layer::new("Layer 1", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);
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
        project
    }

    /// Regression: enabling pass-through on a layer that holds a
    /// raster VirtualClone must resize the clone's bounds to native
    /// DPI, not just concrete RasterImage objects. The planner
    /// resolves clones back to the source's pixel data at plan time
    /// but keeps the clone's own bounds, so stale bounds would
    /// produce wrong physical sizing on the bed.
    #[test]
    fn sync_passthrough_bounds_resizes_virtual_raster_clones() {
        let mut project = Project::new("PT Clone");
        let image_layer = Layer::new("Image", OperationType::Image);
        let image_layer_id = image_layer.id;
        project.add_layer(image_layer);

        // Concrete raster source at an arbitrary 20x20 mm on bed.
        let raster = ProjectObject::new(
            "Src",
            image_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(20.0, 20.0)),
            ObjectData::RasterImage {
                asset_key: "k".into(),
                original_width_px: 200,
                original_height_px: 200,
                adjustments: None,
                masks: Vec::new(),
            },
        );
        let raster_id = raster.id;
        project.add_object(raster);

        // VirtualClone of the raster, positioned elsewhere, at a
        // stale size different from both the source bounds and the
        // pass-through native size.
        let clone = ProjectObject::new(
            "Clone",
            image_layer_id,
            Bounds::new(Point2D::new(50.0, 50.0), Point2D::new(65.0, 65.0)),
            ObjectData::VirtualClone {
                source_id: raster_id,
            },
        );
        let clone_id = clone.id;
        project.add_object(clone);

        // New raster settings: pass_through ON at 254 DPI.
        // Native size: 200 px / 254 DPI * 25.4 mm = 20.0 mm.
        let mut new_rs = RasterSettings::default();
        new_rs.pass_through = true;
        new_rs.dpi = 254;
        new_rs.line_interval_mm = 25.4 / 254.0;

        sync_passthrough_bounds(&mut project, image_layer_id, None, &new_rs);

        // Concrete raster should be resized to native 20x20 mm
        // centered on its previous center (10, 10).
        let src_bounds = project.find_object(raster_id).unwrap().bounds;
        assert!((src_bounds.width() - 20.0).abs() < 1e-6);
        assert!((src_bounds.height() - 20.0).abs() < 1e-6);

        // Clone must ALSO be resized to native 20x20 mm centered on
        // its previous center (57.5, 57.5). Previously this was the
        // broken case — the literal matches!(RasterImage) filter
        // excluded VirtualClone objects.
        let clone_bounds = project.find_object(clone_id).unwrap().bounds;
        assert!(
            (clone_bounds.width() - 20.0).abs() < 1e-6,
            "expected 20mm width, got {}",
            clone_bounds.width()
        );
        assert!(
            (clone_bounds.height() - 20.0).abs() < 1e-6,
            "expected 20mm height, got {}",
            clone_bounds.height()
        );
        let cx = (clone_bounds.min.x + clone_bounds.max.x) / 2.0;
        let cy = (clone_bounds.min.y + clone_bounds.max.y) / 2.0;
        assert!((cx - 57.5).abs() < 1e-6);
        assert!((cy - 57.5).abs() < 1e-6);
    }

    #[test]
    fn sync_passthrough_layer_objects_resizes_clones_after_source_dimensions_change() {
        let mut project = Project::new("PT Replace");
        let mut image_layer = Layer::new("Image", OperationType::Image);
        image_layer.primary_entry_mut().raster_settings = Some(RasterSettings {
            pass_through: true,
            dpi: 254,
            line_interval_mm: 25.4 / 254.0,
            ..RasterSettings::default()
        });
        let image_layer_id = image_layer.id;
        project.add_layer(image_layer);

        let raster = ProjectObject::new(
            "Src",
            image_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(20.0, 20.0)),
            ObjectData::RasterImage {
                asset_key: "k".into(),
                original_width_px: 200,
                original_height_px: 200,
                adjustments: None,
                masks: Vec::new(),
            },
        );
        let raster_id = raster.id;
        project.add_object(raster);

        let clone = ProjectObject::new(
            "Clone",
            image_layer_id,
            Bounds::new(Point2D::new(50.0, 50.0), Point2D::new(70.0, 70.0)),
            ObjectData::VirtualClone {
                source_id: raster_id,
            },
        );
        let clone_id = clone.id;
        project.add_object(clone);

        if let Some(src) = project.find_object_mut(raster_id) {
            src.data = ObjectData::RasterImage {
                asset_key: "k2".into(),
                original_width_px: 400,
                original_height_px: 400,
                adjustments: None,
                masks: Vec::new(),
            };
        }

        sync_passthrough_layer_objects(&mut project, image_layer_id);

        let clone_bounds = project.find_object(clone_id).unwrap().bounds;
        assert!((clone_bounds.width() - 40.0).abs() < 1e-6);
        assert!((clone_bounds.height() - 40.0).abs() < 1e-6);
        let cx = (clone_bounds.min.x + clone_bounds.max.x) / 2.0;
        let cy = (clone_bounds.min.y + clone_bounds.max.y) / 2.0;
        assert!((cx - 60.0).abs() < 1e-6);
        assert!((cy - 60.0).abs() < 1e-6);
    }

    #[test]
    fn replace_project_preserves_existing_asset_data() {
        let ctx = ServiceContext::new();
        let mut project = sample_project();
        let asset = Asset::new("test.png", AssetMediaType::Png, 4, None, None);
        let asset_id = asset.id;
        project.add_asset(asset, vec![1, 2, 3, 4]);
        *ctx.project.lock().unwrap() = Some(project);

        let replacement = sample_project();
        replace_project(&ctx, replacement).unwrap();

        let restored = ctx.project.lock().unwrap().clone().unwrap();
        assert_eq!(restored.get_asset_data(asset_id), Some(&[1, 2, 3, 4][..]));
        assert!(restored.dirty);
    }

    #[test]
    fn bind_active_machine_profile_pushes_undo_snapshot() {
        let ctx = ServiceContext::new();
        let mut project = sample_project();
        project.dirty = false;
        *ctx.project.lock().unwrap() = Some(project.clone());

        {
            let mut settings = ctx.settings.lock().unwrap();
            let profile = settings
                .machine_profiles
                .first()
                .cloned()
                .unwrap_or_default();
            settings.active_profile_id = Some(profile.id);
            if settings.machine_profiles.is_empty() {
                settings.machine_profiles.push(profile);
            }
        }

        bind_active_machine_profile(&ctx).unwrap();
        assert!(ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn bind_active_machine_profile_updates_project_workspace() {
        let ctx = ServiceContext::new();
        let mut project = sample_project();
        project.workspace.bed_width_mm = 400.0;
        project.workspace.bed_height_mm = 400.0;
        *ctx.project.lock().unwrap() = Some(project);

        let profile_id = {
            let mut settings = ctx.settings.lock().unwrap();
            let mut profile = MachineProfile::default();
            profile.bed_width_mm = 600.0;
            profile.bed_height_mm = 300.0;
            profile.origin = WorkspaceOrigin::BottomLeft;
            let profile_id = profile.id;
            settings.machine_profiles.push(profile);
            settings.active_profile_id = Some(profile_id);
            profile_id
        };

        let updated = bind_active_machine_profile(&ctx).unwrap();

        assert_eq!(updated.machine_profile_id, Some(profile_id));
        assert_eq!(updated.workspace.bed_width_mm, 600.0);
        assert_eq!(updated.workspace.bed_height_mm, 300.0);
        assert_eq!(updated.workspace.origin, WorkspaceOrigin::BottomLeft);
        let stored = ctx.project.lock().unwrap().clone().unwrap();
        assert_eq!(stored.workspace, updated.workspace);
    }

    #[test]
    fn replace_project_document_clears_path_and_history() {
        let ctx = ServiceContext::new();
        *ctx.project.lock().unwrap() = Some(sample_project());
        *ctx.project_path.lock().unwrap() = Some(PathBuf::from("/tmp/original.lzrproj"));
        ctx.push_project_undo_snapshot(&sample_project()).unwrap();

        replace_project_document(&ctx, sample_project()).unwrap();

        assert!(ctx.project_path.lock().unwrap().is_none());
        let undo = ctx.undo_state().unwrap();
        assert!(!undo.can_undo);
        assert!(!undo.can_redo);
    }

    #[test]
    fn add_object_atomic_undo_removes_auto_created_layer_and_object_together() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Atomic Add");

        let mut image_layer = Layer::new("C00 Image", OperationType::Image);
        image_layer.color_tag = ColorTag("#000000".into());
        let image_layer_id = image_layer.id;
        project.add_layer(image_layer);
        project.add_object(ProjectObject::new(
            "Image",
            image_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(20.0, 20.0)),
            ObjectData::RasterImage {
                asset_key: "img".into(),
                original_width_px: 100,
                original_height_px: 100,
                adjustments: None,
                masks: Vec::new(),
            },
        ));
        *ctx.project.lock().unwrap() = Some(project);

        let result = add_object_atomic(
            &ctx,
            AddObjectAtomicInput {
                name: "Star".into(),
                layer_id: LayerId::new(),
                object_data: ObjectData::Star {
                    points: 5,
                    bulge: 0.0,
                    ratio: 0.5,
                    dual_radius: false,
                    ratio2: None,
                    corner_radius: 0.0,
                    corner_radii: vec![],
                },
                bounds: Bounds::new(Point2D::new(30.0, 0.0), Point2D::new(40.0, 10.0)),
                create_layer: Some(AddObjectLayerInput {
                    name: "C00 Line".into(),
                    color_tag: Some("#000000".into()),
                    operation: OperationType::Line,
                    entry_patch: Some(CutEntryPatch {
                        speed_mm_min: Some(1000.0),
                        power_percent: Some(50.0),
                        power_min_percent: Some(10.0),
                        ..Default::default()
                    }),
                }),
            },
        )
        .unwrap();

        assert!(
            result.created_layer.is_some(),
            "should create a sibling layer"
        );
        let added_layer_id = result.created_layer.as_ref().unwrap().id;
        let after_add = ctx.project.lock().unwrap().clone().unwrap();
        assert!(after_add.layers.iter().any(|l| l.id == added_layer_id));
        assert!(after_add.objects.iter().any(|o| o.id == result.object.id));

        let restored = undo_project(&ctx).unwrap();
        assert_eq!(
            restored.layers.len(),
            1,
            "undo should remove the auto-created layer"
        );
        assert_eq!(
            restored.objects.len(),
            1,
            "undo should remove the added object"
        );
        assert!(restored.layers.iter().all(|l| l.id != added_layer_id));
        assert!(restored.objects.iter().all(|o| o.id != result.object.id));
    }

    #[test]
    fn add_object_atomic_create_layer_takes_precedence_over_existing_layer_id() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Atomic Add");

        let mut image_layer = Layer::new("C00 Image", OperationType::Image);
        image_layer.color_tag = ColorTag("#000000".into());
        let image_layer_id = image_layer.id;
        project.add_layer(image_layer);
        *ctx.project.lock().unwrap() = Some(project);

        let result = add_object_atomic(
            &ctx,
            AddObjectAtomicInput {
                name: "Rect".into(),
                layer_id: image_layer_id,
                object_data: ObjectData::Shape {
                    kind: ShapeKind::Rectangle,
                    width: 10.0,
                    height: 10.0,
                    corner_radius: 0.0,
                },
                bounds: Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
                create_layer: Some(AddObjectLayerInput {
                    name: "C00 Line".into(),
                    color_tag: Some("#000000".into()),
                    operation: OperationType::Line,
                    entry_patch: Some(CutEntryPatch {
                        speed_mm_min: Some(1000.0),
                        power_percent: Some(50.0),
                        power_min_percent: Some(10.0),
                        ..Default::default()
                    }),
                }),
            },
        )
        .unwrap();

        let created_layer = result.created_layer.expect("should create sibling layer");
        assert_ne!(created_layer.id, image_layer_id);
        assert_eq!(result.object.layer_id, created_layer.id);
    }

    #[test]
    fn add_object_atomic_mints_cut_entry_id_from_operation_alone() {
        // Regression: the canvas rectangle-draw path calls `add_object_atomic`
        // with only `{ name, operation, color_tag }` and no `entry_patch`.
        // The backend must mint a fresh `CutEntry` (and its `CutEntryId`) via
        // `Layer::new_single_entry`. Before this test existed, the Tauri
        // command required callers to ship a full `CutEntry` including a
        // pre-generated `CutEntryId`, and the frontend service synthesized
        // one with `id: ''` that failed UUID parsing at the IPC boundary.
        let ctx = ServiceContext::new();
        *ctx.project.lock().unwrap() = Some(Project::new("Canvas Rect"));

        let result = add_object_atomic(
            &ctx,
            AddObjectAtomicInput {
                name: "Rect".into(),
                layer_id: LayerId::new(),
                object_data: ObjectData::Shape {
                    kind: ShapeKind::Rectangle,
                    width: 10.0,
                    height: 10.0,
                    corner_radius: 0.0,
                },
                bounds: Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
                create_layer: Some(AddObjectLayerInput {
                    name: "Line".into(),
                    color_tag: Some("#000000".into()),
                    operation: OperationType::Line,
                    entry_patch: None,
                }),
            },
        )
        .unwrap();

        let created_layer = result.created_layer.expect("should create layer");
        assert_eq!(created_layer.entries.len(), 1);
        let entry = &created_layer.entries[0];
        assert_eq!(entry.operation, OperationType::Line);
        // Minted id must round-trip through JSON (which was the failing
        // boundary — `Id<CutEntryMarker>` parses in 32-char simple UUID
        // format and the bug serialized the id as `""`, failing deserialize
        // with "invalid length: expected length 32 ... found 0").
        let entry_json = serde_json::to_string(&entry.id).unwrap();
        let restored: beambench_core::CutEntryId =
            serde_json::from_str(&entry_json).expect("minted id must round-trip through JSON");
        assert_eq!(restored, entry.id);
        // Default settings come from `CutEntry::new(OperationType::Line)`.
        assert_eq!(entry.speed_mm_min, 1000.0);
        assert_eq!(entry.power_percent, 50.0);
        assert!(entry.output_enabled);
    }

    #[test]
    fn add_object_atomic_entry_patch_overrides_minted_defaults() {
        let ctx = ServiceContext::new();
        *ctx.project.lock().unwrap() = Some(Project::new("Patch Test"));

        let result = add_object_atomic(
            &ctx,
            AddObjectAtomicInput {
                name: "Rect".into(),
                layer_id: LayerId::new(),
                object_data: ObjectData::Shape {
                    kind: ShapeKind::Rectangle,
                    width: 10.0,
                    height: 10.0,
                    corner_radius: 0.0,
                },
                bounds: Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
                create_layer: Some(AddObjectLayerInput {
                    name: "Line".into(),
                    color_tag: None,
                    operation: OperationType::Line,
                    entry_patch: Some(CutEntryPatch {
                        speed_mm_min: Some(3500.0),
                        power_percent: Some(25.0),
                        ..Default::default()
                    }),
                }),
            },
        )
        .unwrap();

        let created_layer = result.created_layer.expect("should create layer");
        let entry = &created_layer.entries[0];
        assert_eq!(entry.speed_mm_min, 3500.0);
        assert_eq!(entry.power_percent, 25.0);
        // Untouched-by-patch fields keep `CutEntry::new` defaults.
        assert_eq!(entry.power_min_percent, 0.0);
        assert!(entry.output_enabled);
    }

    fn sample_text_project() -> (Project, ObjectId) {
        let mut project = Project::new("Batch Test");
        let layer = Layer::new("Layer 1", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);
        let obj = ProjectObject::new(
            "Label",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(50.0, 20.0)),
            ObjectData::Text {
                content: "SN-{Serial}".to_string(),
                font_family: "Arial".to_string(),
                font_size_mm: 5.0,
                alignment: TextAlignment::Left,
                alignment_v: Default::default(),
                bold: false,
                italic: false,
                upper_case: false,
                welded: false,
                h_spacing: 0.0,
                v_spacing: 0.0,
                on_path: false,
                path_offset: 0.0,
                distort: false,
                layout_mode: Default::default(),
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
        let oid = obj.id;
        project.add_object(obj);
        (project, oid)
    }

    fn make_batch_copy(text: &str) -> BatchCopyInput {
        use beambench_core::variable_text::VariableTextSource;
        BatchCopyInput {
            resolved_text: text.to_string(),
            variable_text_config: VariableTextConfig {
                template: "SN-{Serial}".to_string(),
                mode: None,
                offset: None,
                source: VariableTextSource::default(),
            },
        }
    }

    #[test]
    fn batch_generates_all_objects_with_single_undo() {
        let ctx = ServiceContext::new();
        let (project, obj_id) = sample_text_project();
        *ctx.project.lock().unwrap() = Some(project);

        let copies = vec![
            make_batch_copy("SN-001"),
            make_batch_copy("SN-002"),
            make_batch_copy("SN-003"),
        ];
        let result = generate_variable_text_batch(&ctx, obj_id, copies, 5.0).unwrap();

        assert_eq!(result.copies.len(), 3);
        // All results are new duplicates — original is preserved with template
        assert_ne!(result.copies[0].id, obj_id);
        if let ObjectData::Text { ref content, .. } = result.copies[0].data {
            assert_eq!(content, "SN-001");
        } else {
            panic!("Expected Text");
        }
        assert!((result.copies[0].bounds.min.x - 15.0).abs() < 0.001); // 10 + 1*5
        // Second duplicate
        if let ObjectData::Text { ref content, .. } = result.copies[1].data {
            assert_eq!(content, "SN-002");
        }
        assert!((result.copies[1].bounds.min.x - 20.0).abs() < 0.001); // 10 + 2*5
        // Third duplicate
        if let ObjectData::Text { ref content, .. } = result.copies[2].data {
            assert_eq!(content, "SN-003");
        }
        assert!((result.copies[2].bounds.min.x - 25.0).abs() < 0.001); // 10 + 3*5

        // Updated original is returned
        assert_eq!(result.updated_original.id, obj_id);

        // Only one undo snapshot was pushed
        let undo = ctx.undo_state().unwrap();
        assert!(undo.can_undo);
    }

    #[test]
    fn duplicate_objects_batches_into_single_undo_and_preserves_all_results() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Duplicate Batch");
        let layer = Layer::new("Layer 1", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);
        let source_a = project
            .add_object(ProjectObject::new(
                "Rect",
                layer_id,
                Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
                ObjectData::Shape {
                    kind: ShapeKind::Rectangle,
                    width: 10.0,
                    height: 10.0,
                    corner_radius: 0.0,
                },
            ))
            .id;
        let source_b = project
            .add_object(ProjectObject::new(
                "Ellipse",
                layer_id,
                Bounds::new(Point2D::new(20.0, 0.0), Point2D::new(30.0, 10.0)),
                ObjectData::Shape {
                    kind: ShapeKind::Ellipse,
                    width: 10.0,
                    height: 10.0,
                    corner_radius: 0.0,
                },
            ))
            .id;
        *ctx.project.lock().unwrap() = Some(project.clone());

        let duplicated = duplicate_objects(&ctx, &[source_a, source_b]).unwrap();

        assert_eq!(duplicated.len(), 2);
        assert_eq!(duplicated[0].name, "Rect copy");
        assert_eq!(duplicated[1].name, "Ellipse copy");
        assert_ne!(duplicated[0].id, source_a);
        assert_ne!(duplicated[1].id, source_b);

        let stored = ctx.project.lock().unwrap().clone().unwrap();
        assert_eq!(stored.objects.len(), project.objects.len() + 2);
        assert!(ctx.undo_state().unwrap().can_undo);

        let restored = undo_project(&ctx).unwrap();
        assert_eq!(restored.objects.len(), project.objects.len());
    }

    #[test]
    fn duplicate_objects_copies_group_subtree_and_remaps_children() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Duplicate Group");
        let layer = Layer::new("Layer 1", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);
        let child = project
            .add_object(ProjectObject::new(
                "Child",
                layer_id,
                Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
                ObjectData::Shape {
                    kind: ShapeKind::Rectangle,
                    width: 10.0,
                    height: 10.0,
                    corner_radius: 0.0,
                },
            ))
            .clone();
        let child_id = child.id;
        let group = project
            .add_object(ProjectObject::new(
                "Group",
                layer_id,
                Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
                ObjectData::Group {
                    children: vec![child_id],
                },
            ))
            .clone();
        *ctx.project.lock().unwrap() = Some(project.clone());

        let duplicated = duplicate_objects(&ctx, &[group.id]).unwrap();

        assert_eq!(duplicated.len(), 2);
        assert_eq!(duplicated[0].name, "Group copy");
        assert_eq!(duplicated[1].name, "Child copy");
        let copied_child_id = duplicated[1].id;
        match &duplicated[0].data {
            ObjectData::Group { children } => assert_eq!(children, &vec![copied_child_id]),
            other => panic!("expected group, got {other:?}"),
        }
        assert_ne!(copied_child_id, child_id);
        let stored = ctx.project.lock().unwrap().clone().unwrap();
        assert!(stored.find_object(copied_child_id).is_some());
    }

    #[test]
    fn paste_objects_inserts_snapshot_after_source_was_removed() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Paste Snapshot");
        let layer = Layer::new("Layer 1", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);
        let source = project
            .add_object(ProjectObject::new(
                "Rect",
                layer_id,
                Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
                ObjectData::Shape {
                    kind: ShapeKind::Rectangle,
                    width: 10.0,
                    height: 10.0,
                    corner_radius: 0.0,
                },
            ))
            .clone();
        let source_id = source.id;
        *ctx.project.lock().unwrap() = Some(project);

        remove_objects(&ctx, &[source_id]).unwrap();
        let pasted = paste_objects(&ctx, &[source], false).unwrap();

        assert_eq!(pasted.len(), 1);
        assert_ne!(pasted[0].id, source_id);
        assert_eq!(pasted[0].name, "Rect copy");
        assert!((pasted[0].bounds.min.x - 5.0).abs() < 1e-9);
        assert!((pasted[0].bounds.min.y - 5.0).abs() < 1e-9);
        let stored = ctx.project.lock().unwrap().clone().unwrap();
        assert!(stored.find_object(source_id).is_none());
        assert!(stored.find_object(pasted[0].id).is_some());
        assert_eq!(stored.layers[0].color_tag, ColorTag("#000000".to_string()));
    }

    #[test]
    fn paste_objects_remaps_internal_group_and_clone_references() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Paste References");
        let layer = Layer::new("Layer 1", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);
        let child = ProjectObject::new(
            "Child",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        let child_id = child.id;
        let clone = ProjectObject::new(
            "Clone",
            layer_id,
            Bounds::new(Point2D::new(20.0, 0.0), Point2D::new(30.0, 10.0)),
            ObjectData::VirtualClone {
                source_id: child_id,
            },
        );
        let group = ProjectObject::new(
            "Group",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(30.0, 10.0)),
            ObjectData::Group {
                children: vec![child_id],
            },
        );
        *ctx.project.lock().unwrap() = Some(project);

        let pasted = paste_objects(&ctx, &[child, clone, group], true).unwrap();

        let pasted_child_id = pasted[0].id;
        match &pasted[1].data {
            ObjectData::VirtualClone { source_id } => assert_eq!(*source_id, pasted_child_id),
            other => panic!("expected virtual clone, got {other:?}"),
        }
        match &pasted[2].data {
            ObjectData::Group { children } => assert_eq!(children, &vec![pasted_child_id]),
            other => panic!("expected group, got {other:?}"),
        }
    }

    #[test]
    fn paste_objects_routes_pending_raster_clones_to_image_layers() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Paste Raster Clone");
        let image_layer = Layer::new("Image", OperationType::Image);
        let image_layer_id = image_layer.id;
        project.add_layer(image_layer);
        let raster = ProjectObject::new(
            "Raster",
            image_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::RasterImage {
                asset_key: "asset".to_string(),
                original_width_px: 10,
                original_height_px: 10,
                adjustments: None,
                masks: Vec::new(),
            },
        );
        let raster_id = raster.id;
        let clone = ProjectObject::new(
            "Raster Clone",
            image_layer_id,
            Bounds::new(Point2D::new(20.0, 0.0), Point2D::new(30.0, 10.0)),
            ObjectData::VirtualClone {
                source_id: raster_id,
            },
        );
        *ctx.project.lock().unwrap() = Some(project);

        let pasted = paste_objects(&ctx, &[clone, raster], true).unwrap();

        assert_eq!(pasted[0].layer_id, image_layer_id);
        match &pasted[0].data {
            ObjectData::VirtualClone { source_id } => assert_eq!(*source_id, pasted[1].id),
            other => panic!("expected virtual clone, got {other:?}"),
        }
    }

    #[test]
    fn batch_csv_current_row_wraps_on_overflow() {
        use beambench_core::variable_text::VariableTextSource;

        let ctx = ServiceContext::new();
        let (project, obj_id) = sample_text_project();
        *ctx.project.lock().unwrap() = Some(project);

        // CSV with header + 3 data rows → csv_row_count = 3
        let csv_data = vec![
            vec!["Name".to_string()],
            vec!["Alice".to_string()],
            vec!["Bob".to_string()],
            vec!["Carol".to_string()],
        ];
        let source = VariableTextSource {
            csv_data: csv_data.clone(),
            current: 2, // start near the end so wrapping kicks in
            start: 0,
            end: 2,
            advance_by: 1,
            auto_advance: false,
            ..Default::default()
        };
        // Generate 4 copies — should wrap past 3 CSV rows
        let copies: Vec<BatchCopyInput> = (0..4)
            .map(|i| {
                let row = (2 + i) % 3; // expected wrapping
                let mut s = source.clone();
                s.current = row as i64;
                BatchCopyInput {
                    resolved_text: format!("Copy-{i}"),
                    variable_text_config: VariableTextConfig {
                        template: "{CSV:Name}".to_string(),
                        mode: None,
                        offset: None,
                        source: s,
                    },
                }
            })
            .collect();

        let result = generate_variable_text_batch(&ctx, obj_id, copies, 5.0).unwrap();
        assert_eq!(result.copies.len(), 4);

        // Original's current should be wrapped: (2 + 4) % 3 = 0
        if let ObjectData::Text {
            ref variable_text, ..
        } = result.updated_original.data
        {
            let vt = variable_text.as_ref().expect("variable_text should be set");
            assert_eq!(
                vt.source.current, 0,
                "current should wrap via modulo: (2+4)%3 = 0"
            );
        } else {
            panic!("Expected text object");
        }
    }

    #[test]
    fn get_project_refreshes_text_outline_cache() {
        let ctx = ServiceContext::new();
        let (project, _obj_id) = sample_text_project();
        *ctx.project.lock().unwrap() = Some(project);

        let loaded = get_project(&ctx).unwrap().unwrap();
        let text = loaded
            .objects
            .into_iter()
            .find(|obj| matches!(obj.data, ObjectData::Text { .. }))
            .expect("text object");
        match text.data {
            ObjectData::Text {
                resolved_path_data,
                resolved_font_key,
                ..
            } => {
                assert!(resolved_path_data.is_some());
                assert!(resolved_font_key.is_some());
            }
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn update_object_data_resizes_straight_text_bounds() {
        let ctx = ServiceContext::new();
        let (project, obj_id) = sample_text_project();
        *ctx.project.lock().unwrap() = Some(project);

        let bigger = ObjectData::Text {
            content: "SN-{Serial}".to_string(),
            font_family: "Arial".to_string(),
            font_size_mm: 20.0,
            alignment: TextAlignment::Left,
            alignment_v: Default::default(),
            bold: false,
            italic: false,
            upper_case: false,
            welded: false,
            h_spacing: 0.0,
            v_spacing: 0.0,
            on_path: false,
            path_offset: 0.0,
            distort: false,
            layout_mode: Default::default(),
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
        };

        let updated = update_object_data(&ctx, obj_id, bigger).unwrap();
        // Left-aligned text preserves min.x
        assert_eq!(updated.bounds.min, Point2D::new(10.0, 10.0));
        assert!(updated.bounds.width() > 40.0);
        assert!(updated.bounds.height() > 10.0);
    }

    // the Adjust Image dialog previously committed its two
    // mutations (object adjustments + layer raster settings) as two
    // separate backend calls, each pushing its own undo snapshot. The new
    // atomic op must push a single snapshot and apply both together.
    #[test]
    fn apply_adjust_image_dialog_is_atomic_with_one_undo_snapshot() {
        use beambench_common::{RasterAdjustments, RasterMode};

        let ctx = ServiceContext::new();
        let mut project = Project::new("Adjust");
        let mut layer = Layer::new("Image", OperationType::Image);
        layer.primary_entry_mut().raster_settings = Some(RasterSettings::default());
        let layer_id = layer.id;
        project.add_layer(layer);
        let raster = ProjectObject::new(
            "Raster",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(20.0, 20.0)),
            ObjectData::RasterImage {
                asset_key: "k".into(),
                original_width_px: 200,
                original_height_px: 200,
                adjustments: None,
                masks: Vec::new(),
            },
        );
        let obj_id = raster.id;
        project.add_object(raster);
        project.dirty = false;
        *ctx.project.lock().unwrap() = Some(project);

        assert!(!ctx.undo_state().unwrap().can_undo);

        let new_adjustments = RasterAdjustments {
            brightness: 10.0,
            contrast: 20.0,
            ..RasterAdjustments::default()
        };
        let mut new_settings = RasterSettings::default();
        new_settings.mode = RasterMode::Threshold;
        new_settings.line_interval_mm = 0.2;

        let (updated_obj, updated_layer) = apply_adjust_image_dialog(
            &ctx,
            obj_id,
            Some(new_adjustments.clone()),
            layer_id,
            new_settings.clone(),
        )
        .unwrap();

        // Object adjustments applied.
        match &updated_obj.data {
            ObjectData::RasterImage { adjustments, .. } => {
                assert_eq!(adjustments.as_ref(), Some(&new_adjustments));
            }
            _ => panic!("expected RasterImage"),
        }
        // Layer raster settings applied.
        assert_eq!(
            updated_layer
                .primary_entry()
                .raster_settings
                .as_ref()
                .map(|s| s.mode),
            Some(RasterMode::Threshold)
        );
        // Single undo snapshot covers the combined change.
        assert!(ctx.undo_state().unwrap().can_undo);
        // Project marked dirty.
        assert!(ctx.project.lock().unwrap().as_ref().unwrap().dirty);
    }

    #[test]
    fn apply_adjust_image_dialog_rejects_non_raster_object() {
        let ctx = ServiceContext::new();
        let project = sample_project(); // rectangle shape, not raster
        let obj_id = project.objects[0].id;
        let layer_id = project.layers[0].id;
        *ctx.project.lock().unwrap() = Some(project);

        let result =
            apply_adjust_image_dialog(&ctx, obj_id, None, layer_id, RasterSettings::default());
        assert!(result.is_err());
        // No undo snapshot pushed on validation failure.
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn batch_rejects_non_text_object() {
        let ctx = ServiceContext::new();
        let project = sample_project();
        let obj_id = project.objects[0].id;
        *ctx.project.lock().unwrap() = Some(project);

        let result = generate_variable_text_batch(&ctx, obj_id, vec![make_batch_copy("test")], 5.0);
        assert!(result.is_err());
    }

    #[test]
    fn batch_single_text_creates_duplicate_preserves_original() {
        let ctx = ServiceContext::new();
        let (project, obj_id) = sample_text_project();
        *ctx.project.lock().unwrap() = Some(project);

        let result =
            generate_variable_text_batch(&ctx, obj_id, vec![make_batch_copy("Updated")], 5.0)
                .unwrap();
        assert_eq!(result.copies.len(), 1);
        // Result is a new duplicate, not the original
        assert_ne!(result.copies[0].id, obj_id);
        if let ObjectData::Text { ref content, .. } = result.copies[0].data {
            assert_eq!(content, "Updated");
        }
        // Original still has its template content
        let project_guard = ctx.project.lock().unwrap();
        let project = project_guard.as_ref().unwrap();
        let original = project.find_object(obj_id).unwrap();
        if let ObjectData::Text { ref content, .. } = original.data {
            assert_eq!(content, "SN-{Serial}");
        }
    }

    #[test]
    fn create_project_starts_with_no_layers() {
        let ctx = ServiceContext::new();
        let project = create_project(&ctx, "Fresh").unwrap();
        assert_eq!(
            project.layers.len(),
            0,
            "new project should have no layers until an object is drawn"
        );
    }

    #[test]
    fn create_project_defaults_to_bottom_left_workspace_origin() {
        let ctx = ServiceContext::new();
        let project = create_project(&ctx, "Fresh").unwrap();
        assert_eq!(project.workspace.origin, WorkspaceOrigin::BottomLeft);
    }

    #[test]
    fn create_project_emits_created_event() {
        let ctx = ServiceContext::new();
        let mut rx = ctx.events.subscribe();

        let project = create_project(&ctx, "Event Project").unwrap();

        let msg = rx.try_recv().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(parsed["type"], "project.created");
        assert_eq!(
            parsed["payload"]["project"]["id"],
            serde_json::json!(project.metadata.project_id)
        );
    }

    #[test]
    fn create_project_uses_active_machine_profile_workspace() {
        let mut settings = AppSettings::default();
        let mut profile = MachineProfile::default();
        profile.bed_width_mm = 730.0;
        profile.bed_height_mm = 410.0;
        profile.origin = WorkspaceOrigin::BottomLeft;
        settings.active_profile_id = Some(profile.id);
        settings.machine_profiles.push(profile.clone());
        let ctx = ServiceContext::with_settings(settings);

        let project = create_project(&ctx, "Profile Workspace").unwrap();

        assert_eq!(project.workspace.bed_width_mm, 730.0);
        assert_eq!(project.workspace.bed_height_mm, 410.0);
        assert_eq!(project.workspace.origin, WorkspaceOrigin::BottomLeft);
        assert_eq!(project.machine_profile_id, Some(profile.id));
        assert_eq!(
            project
                .machine_profile_snapshot
                .as_ref()
                .map(|snapshot| snapshot.bed_width_mm),
            Some(730.0)
        );
    }

    #[test]
    fn add_layer_auto_cycles_only_standard_colors() {
        let ctx = ServiceContext::new();
        create_project(&ctx, "Color Test").unwrap();

        // Create 32 layers (more than palette size) and verify none get tool colors
        let tool_hexes: Vec<&str> = PALETTE_COLORS
            .iter()
            .filter(|p| p.is_tool_layer)
            .map(|p| p.hex)
            .collect();
        for i in 0..32 {
            let layer = add_layer(
                &ctx,
                AddLayerInput {
                    name: format!("Layer {}", i),
                    operation: OperationType::Line,
                },
            )
            .unwrap();
            let hex = layer.color_tag.0.to_uppercase();
            assert!(
                !tool_hexes.contains(&hex.as_str()),
                "Layer {} got tool color {}, expected standard color",
                i,
                hex
            );
        }
    }

    #[test]
    fn update_layer_with_tool_color_sets_is_tool_layer() {
        let ctx = ServiceContext::new();
        create_project(&ctx, "Tool Flag Test").unwrap();
        let layer = add_layer(
            &ctx,
            AddLayerInput {
                name: "Lines".to_string(),
                operation: OperationType::Line,
            },
        )
        .unwrap();
        assert!(!layer.is_tool_layer);

        // Update to T1 color
        let updated = update_layer(
            &ctx,
            layer.id,
            UpdateLayerInput {
                color_tag: Some("#DA0B3F".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(updated.is_tool_layer);
    }

    #[test]
    fn update_layer_from_tool_to_standard_clears_is_tool_layer() {
        let ctx = ServiceContext::new();
        create_project(&ctx, "Tool Reset Test").unwrap();
        let layer = add_layer(
            &ctx,
            AddLayerInput {
                name: "Tool".to_string(),
                operation: OperationType::Line,
            },
        )
        .unwrap();

        // Set to tool color
        let tool_layer = update_layer(
            &ctx,
            layer.id,
            UpdateLayerInput {
                color_tag: Some("#DA0B3F".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(tool_layer.is_tool_layer);

        // Switch back to standard color
        let standard_layer = update_layer(
            &ctx,
            layer.id,
            UpdateLayerInput {
                color_tag: Some("#FF0000".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!standard_layer.is_tool_layer);
        assert_eq!(standard_layer.entries.len(), 1);
        assert_eq!(
            standard_layer.primary_entry().operation,
            OperationType::Line
        );
        assert!(standard_layer.primary_entry().output_enabled);
        assert_eq!(standard_layer.primary_entry().speed_mm_min, 1000.0);
        assert_eq!(standard_layer.primary_entry().power_percent, 50.0);
    }

    #[test]
    fn cut_entry_crud_round_trip_and_noop_update_behave_as_expected() {
        let ctx = ServiceContext::new();
        create_project(&ctx, "Cut Entry CRUD").unwrap();
        let layer = add_layer(
            &ctx,
            AddLayerInput {
                name: "Layer".to_string(),
                operation: OperationType::Line,
            },
        )
        .unwrap();
        let primary_id = layer.entries[0].id;

        // Reset the history baseline so the assertions below measure only the
        // cut-entry CRUD mutations, not the layer-scaffolding snapshots.
        ctx.clear_project_history().unwrap();
        assert!(!ctx.undo_state().unwrap().can_undo);

        let added = add_cut_entry(&ctx, layer.id, Some(primary_id)).unwrap();
        let project = ctx.project.lock().unwrap().clone().unwrap();
        let layer_after_add = project.find_layer(layer.id).unwrap();
        assert_eq!(layer_after_add.entries.len(), 2);
        assert_eq!(layer_after_add.entries[1].id, added.id);
        assert!(ctx.undo_state().unwrap().can_undo);

        let noop = update_cut_entry(&ctx, layer.id, added.id, CutEntryPatch::default()).unwrap();
        assert_eq!(noop, added);

        let changed = update_cut_entry(
            &ctx,
            layer.id,
            added.id,
            CutEntryPatch {
                speed_mm_min: Some(3210.0),
                power_percent: Some(67.0),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(changed.speed_mm_min, 3210.0);
        assert_eq!(changed.power_percent, 67.0);

        let reordered = reorder_cut_entry(&ctx, layer.id, added.id, 0).unwrap();
        assert_eq!(reordered.entries[0].id, added.id);

        remove_cut_entry(&ctx, layer.id, primary_id).unwrap();
        let project = ctx.project.lock().unwrap().clone().unwrap();
        let layer_after_remove = project.find_layer(layer.id).unwrap();
        assert_eq!(layer_after_remove.entries.len(), 1);
        assert_eq!(layer_after_remove.entries[0].id, added.id);

        let err = remove_cut_entry(&ctx, layer.id, added.id).unwrap_err();
        assert!(
            err.message.to_lowercase().contains("sub-layer")
                || err.message.to_lowercase().contains("cut entry"),
            "expected min-1 protection error, got {:?}",
            err
        );
    }

    #[test]
    fn set_text_guide_path_links_text_to_vector() {
        let ctx = ServiceContext::new();
        let (mut project, text_id) = sample_text_project();
        // Add a vector path object as guide
        let layer_id = project.layers[0].id;
        let guide_obj = ProjectObject::new(
            "Guide",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 0.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L100 0".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let guide_id = guide_obj.id;
        project.add_object(guide_obj);
        *ctx.project.lock().unwrap() = Some(project);

        let updated = set_text_guide_path(&ctx, text_id, Some(guide_id)).unwrap();
        match &updated.data {
            ObjectData::Text {
                guide_path_id,
                layout_mode,
                on_path,
                resolved_path_data,
                ..
            } => {
                assert_eq!(*guide_path_id, Some(guide_id));
                assert_eq!(*layout_mode, TextLayoutMode::Path);
                assert!(*on_path);
                assert!(
                    resolved_path_data.is_some(),
                    "path text should have resolved geometry"
                );
            }
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn set_text_guide_path_clear_reverts_to_straight() {
        let ctx = ServiceContext::new();
        let (mut project, text_id) = sample_text_project();
        let layer_id = project.layers[0].id;
        let guide_obj = ProjectObject::new(
            "Guide",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 0.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L100 0".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let guide_id = guide_obj.id;
        project.add_object(guide_obj);
        *ctx.project.lock().unwrap() = Some(project);

        // Link then unlink
        set_text_guide_path(&ctx, text_id, Some(guide_id)).unwrap();
        let cleared = set_text_guide_path(&ctx, text_id, None).unwrap();
        match &cleared.data {
            ObjectData::Text {
                guide_path_id,
                layout_mode,
                on_path,
                ..
            } => {
                assert!(guide_path_id.is_none());
                assert_eq!(*layout_mode, TextLayoutMode::Straight);
                assert!(!on_path);
            }
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn set_text_guide_path_rejects_non_text_object() {
        let ctx = ServiceContext::new();
        let project = sample_project();
        let rect_id = project.objects[0].id;
        *ctx.project.lock().unwrap() = Some(project);

        let err = set_text_guide_path(&ctx, rect_id, None);
        assert!(err.is_err());
    }

    #[test]
    fn remove_object_unlinks_guide_path_references() {
        let ctx = ServiceContext::new();
        let (mut project, text_id) = sample_text_project();
        let layer_id = project.layers[0].id;
        let guide_obj = ProjectObject::new(
            "Guide",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 0.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L100 0".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let guide_id = guide_obj.id;
        project.add_object(guide_obj);
        *ctx.project.lock().unwrap() = Some(project);

        // Link text to guide
        set_text_guide_path(&ctx, text_id, Some(guide_id)).unwrap();

        // Delete guide path
        remove_object(&ctx, guide_id).unwrap();

        // Text should be auto-unlinked
        let project_guard = ctx.project.lock().unwrap();
        let project = project_guard.as_ref().unwrap();
        let text = project.find_object(text_id).unwrap();
        match &text.data {
            ObjectData::Text {
                guide_path_id,
                layout_mode,
                on_path,
                ..
            } => {
                assert!(guide_path_id.is_none(), "guide_path_id should be cleared");
                assert_eq!(
                    *layout_mode,
                    TextLayoutMode::Straight,
                    "should revert to Straight"
                );
                assert!(!on_path, "on_path should be false");
            }
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn remove_objects_batch_unlinks_guide_path_references() {
        let ctx = ServiceContext::new();
        let (mut project, text_id) = sample_text_project();
        let layer_id = project.layers[0].id;
        let guide_obj = ProjectObject::new(
            "Guide",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 0.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L100 0".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let guide_id = guide_obj.id;
        project.add_object(guide_obj);
        *ctx.project.lock().unwrap() = Some(project);

        // Link text to guide
        set_text_guide_path(&ctx, text_id, Some(guide_id)).unwrap();

        // Batch-delete guide path
        remove_objects(&ctx, &[guide_id]).unwrap();

        // Text should be auto-unlinked
        let project_guard = ctx.project.lock().unwrap();
        let project = project_guard.as_ref().unwrap();
        let text = project.find_object(text_id).unwrap();
        match &text.data {
            ObjectData::Text { guide_path_id, .. } => {
                assert!(
                    guide_path_id.is_none(),
                    "guide_path_id should be cleared after batch delete"
                );
            }
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn update_object_data_preserves_path_text_geometry() {
        // Regression test: editing content of path-text should not wipe resolved geometry.
        let ctx = ServiceContext::new();
        let (mut project, text_id) = sample_text_project();
        let layer_id = project.layers[0].id;
        let guide_obj = ProjectObject::new(
            "Guide",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 0.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L100 0".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let guide_id = guide_obj.id;
        project.add_object(guide_obj);
        *ctx.project.lock().unwrap() = Some(project);

        // Link text to guide path
        set_text_guide_path(&ctx, text_id, Some(guide_id)).unwrap();

        // Now update the text content (simulating a user edit)
        let project_guard = ctx.project.lock().unwrap();
        let text_obj = project_guard
            .as_ref()
            .unwrap()
            .find_object(text_id)
            .unwrap();
        let mut new_data = text_obj.data.clone();
        if let ObjectData::Text {
            ref mut content, ..
        } = new_data
        {
            *content = "Edited Content".to_string();
        }
        drop(project_guard);

        let updated = update_object_data(&ctx, text_id, new_data).unwrap();
        match &updated.data {
            ObjectData::Text {
                content,
                resolved_path_data,
                guide_path_id,
                ..
            } => {
                assert_eq!(content, "Edited Content");
                assert_eq!(*guide_path_id, Some(guide_id));
                assert!(
                    resolved_path_data.is_some(),
                    "path-text should preserve resolved geometry after content edit"
                );
            }
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn update_object_reflows_dependent_text_when_guide_edited() {
        // Regression test: editing the guide path should reflow linked text objects.
        let ctx = ServiceContext::new();
        let (mut project, text_id) = sample_text_project();
        let layer_id = project.layers[0].id;
        let guide_obj = ProjectObject::new(
            "Guide",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 0.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L100 0".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let guide_id = guide_obj.id;
        project.add_object(guide_obj);
        *ctx.project.lock().unwrap() = Some(project);

        // Link text to guide
        set_text_guide_path(&ctx, text_id, Some(guide_id)).unwrap();

        // Capture resolved geometry before guide edit
        let before = {
            let pg = ctx.project.lock().unwrap();
            let p = pg.as_ref().unwrap();
            let t = p.find_object(text_id).unwrap();
            if let ObjectData::Text {
                resolved_path_data, ..
            } = &t.data
            {
                resolved_path_data.clone()
            } else {
                None
            }
        };
        assert!(
            before.is_some(),
            "text should have geometry before guide edit"
        );

        // Edit the guide path's bounds (simulate moving it)
        let input = UpdateObjectInput {
            bounds: Some(Bounds::new(
                Point2D::new(0.0, 50.0),
                Point2D::new(200.0, 50.0),
            )),
            ..Default::default()
        };
        update_object(&ctx, guide_id, input).unwrap();

        // Check that the dependent text object was reflowed (still has geometry)
        let after = {
            let pg = ctx.project.lock().unwrap();
            let p = pg.as_ref().unwrap();
            let t = p.find_object(text_id).unwrap();
            if let ObjectData::Text {
                resolved_path_data, ..
            } = &t.data
            {
                resolved_path_data.clone()
            } else {
                None
            }
        };
        assert!(
            after.is_some(),
            "text should still have geometry after guide edit"
        );
    }

    #[test]
    fn update_object_data_center_text_preserves_center() {
        // Center/middle aligned text should keep its center fixed when content changes.
        let ctx = ServiceContext::new();
        let mut project = Project::new("Center Test");
        let layer = Layer::new("L1", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);
        // Create a center/middle text object with center at (50, 50)
        let obj = ProjectObject::new(
            "CenterText",
            layer_id,
            Bounds::new(Point2D::new(40.0, 45.0), Point2D::new(60.0, 55.0)),
            ObjectData::Text {
                content: "Hi".to_string(),
                font_family: "Arial".to_string(),
                font_size_mm: 5.0,
                alignment: TextAlignment::Center,
                alignment_v: TextAlignmentV::Middle,
                bold: false,
                italic: false,
                upper_case: false,
                welded: false,
                h_spacing: 0.0,
                v_spacing: 0.0,
                on_path: false,
                path_offset: 0.0,
                distort: false,
                layout_mode: Default::default(),
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
        let obj_id = obj.id;
        project.add_object(obj);
        *ctx.project.lock().unwrap() = Some(project);

        // Update content to something longer — bounds will change size but center should stay
        let new_data = ObjectData::Text {
            content: "Hello World".to_string(),
            font_family: "Arial".to_string(),
            font_size_mm: 5.0,
            alignment: TextAlignment::Center,
            alignment_v: TextAlignmentV::Middle,
            bold: false,
            italic: false,
            upper_case: false,
            welded: false,
            h_spacing: 0.0,
            v_spacing: 0.0,
            on_path: false,
            path_offset: 0.0,
            distort: false,
            layout_mode: Default::default(),
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
        };

        let updated = update_object_data(&ctx, obj_id, new_data).unwrap();
        let center_x = (updated.bounds.min.x + updated.bounds.max.x) / 2.0;
        let center_y = (updated.bounds.min.y + updated.bounds.max.y) / 2.0;
        assert!(
            (center_x - 50.0).abs() < 0.01,
            "center X should remain at 50, got {center_x}"
        );
        assert!(
            (center_y - 50.0).abs() < 0.01,
            "center Y should remain at 50, got {center_y}"
        );
    }

    #[test]
    fn update_object_data_mapped_text_preserves_transform_and_alignment_anchor() {
        let ctx = ServiceContext::new();
        let old_bounds = Bounds::new(Point2D::new(10.0, 20.0), Point2D::new(50.0, 60.0));
        let (mut project, object_id) = make_straight_text_project(10.0, 0.0, 0.0, None, old_bounds);
        let affine = Transform2D {
            a: 0.8,
            b: 0.6,
            c: -0.3,
            d: 1.2,
            tx: 7.0,
            ty: -4.0,
        };
        let object = project.find_object_mut(object_id).unwrap();
        object.transform = affine;
        let ObjectData::Text {
            alignment,
            alignment_v,
            layout_mode,
            bend_radius,
            transform_style,
            transform_curve,
            ..
        } = &mut object.data
        else {
            panic!("expected text")
        };
        *alignment = TextAlignment::Right;
        *alignment_v = TextAlignmentV::Bottom;
        // Keep a mapped legacy mode underneath the new transform to verify
        // transform_style precedence follows the same anchor-preserving refit path.
        *layout_mode = TextLayoutMode::Bend;
        *bend_radius = 35.0;
        *transform_style = beambench_core::TextTransformStyle::Wave;
        *transform_curve = 70.0;

        let mut updated_data = object.data.clone();
        if let ObjectData::Text {
            content,
            font_size_mm,
            ..
        } = &mut updated_data
        {
            *content = "A much longer transformed label".to_string();
            *font_size_mm = 18.0;
        }
        *ctx.project.lock().unwrap() = Some(project);

        let updated = update_object_data(&ctx, object_id, updated_data).unwrap();

        assert_eq!(updated.transform, affine);
        assert!((updated.bounds.max.x - old_bounds.max.x).abs() < 1e-9);
        assert!((updated.bounds.max.y - old_bounds.max.y).abs() < 1e-9);
        assert_ne!(updated.bounds.width(), old_bounds.width());
        assert_ne!(updated.bounds.height(), old_bounds.height());
        assert!(matches!(
            updated.data,
            ObjectData::Text {
                resolved_path_data: Some(_),
                ..
            }
        ));
    }

    // --- Text resize scaling tests ---

    fn make_straight_text_project(
        font_size: f64,
        h_spacing: f64,
        v_spacing: f64,
        max_width: Option<f64>,
        bounds: Bounds,
    ) -> (Project, ObjectId) {
        let mut project = Project::new("Text Scale Test");
        let layer = Layer::new("Layer 1", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);
        let obj = ProjectObject::new(
            "Label",
            layer_id,
            bounds,
            ObjectData::Text {
                content: "Hello World".to_string(),
                font_family: "Arial".to_string(),
                font_size_mm: font_size,
                alignment: TextAlignment::Left,
                alignment_v: TextAlignmentV::Top,
                bold: false,
                italic: false,
                upper_case: false,
                welded: false,
                h_spacing,
                v_spacing,
                on_path: false,
                path_offset: 0.0,
                distort: false,
                layout_mode: TextLayoutMode::Straight,
                rtl: false,
                bend_radius: 0.0,
                transform_style: beambench_core::TextTransformStyle::None,
                transform_curve: 0.0,
                circle_placement: beambench_core::TextCirclePlacement::TopOutside,
                max_width,
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
        let oid = obj.id;
        project.add_object(obj);
        (project, oid)
    }

    #[test]
    fn update_object_bounds_scales_text_proportional() {
        let ctx = ServiceContext::new();
        let bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 20.0));
        let (project, obj_id) = make_straight_text_project(10.0, 2.0, 1.0, Some(50.0), bounds);
        *ctx.project.lock().unwrap() = Some(project);

        let new_bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(200.0, 40.0));
        let input = UpdateObjectInput {
            bounds: Some(new_bounds),
            ..Default::default()
        };
        let updated = update_object(&ctx, obj_id, input).unwrap();

        if let ObjectData::Text {
            font_size_mm,
            h_spacing,
            v_spacing,
            max_width,
            ..
        } = &updated.data
        {
            assert!(
                (font_size_mm - 20.0).abs() < 1e-6,
                "font_size_mm: {font_size_mm}"
            );
            assert!((v_spacing - 2.0).abs() < 1e-6, "v_spacing: {v_spacing}");
            assert!((h_spacing - 4.0).abs() < 1e-6, "h_spacing: {h_spacing}");
            assert!(
                (max_width.unwrap() - 100.0).abs() < 1e-6,
                "max_width: {max_width:?}"
            );
        } else {
            panic!("Expected Text variant");
        }
    }

    #[test]
    fn update_object_applies_power_scale_and_lock_aspect_ratio() {
        let ctx = ServiceContext::new();
        let project = sample_project();
        let obj_id = project.objects[0].id;
        *ctx.project.lock().unwrap() = Some(project);

        let updated = update_object(
            &ctx,
            obj_id,
            UpdateObjectInput {
                lock_aspect_ratio: Some(true),
                power_scale: Some(0.65),
                ..Default::default()
            },
        )
        .unwrap();

        assert!(updated.lock_aspect_ratio);
        assert!((updated.power_scale - 0.65).abs() < 1e-9);
    }

    #[test]
    fn update_object_bounds_width_only() {
        let ctx = ServiceContext::new();
        let bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 20.0));
        let (project, obj_id) = make_straight_text_project(10.0, 2.0, 1.0, Some(50.0), bounds);
        *ctx.project.lock().unwrap() = Some(project);

        // Only width changes: 100→200, height stays 20
        let new_bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(200.0, 20.0));
        let input = UpdateObjectInput {
            bounds: Some(new_bounds),
            ..Default::default()
        };
        let updated = update_object(&ctx, obj_id, input).unwrap();

        if let ObjectData::Text {
            font_size_mm,
            h_spacing,
            v_spacing,
            max_width,
            ..
        } = &updated.data
        {
            // Vertical properties unchanged (sy=1)
            assert!(
                (font_size_mm - 10.0).abs() < 1e-6,
                "font_size_mm should be unchanged: {font_size_mm}"
            );
            assert!(
                (v_spacing - 1.0).abs() < 1e-6,
                "v_spacing should be unchanged: {v_spacing}"
            );
            // Horizontal properties doubled (sx=2)
            assert!((h_spacing - 4.0).abs() < 1e-6, "h_spacing: {h_spacing}");
            assert!(
                (max_width.unwrap() - 100.0).abs() < 1e-6,
                "max_width: {max_width:?}"
            );
        } else {
            panic!("Expected Text variant");
        }
    }

    #[test]
    fn update_object_bounds_height_only() {
        let ctx = ServiceContext::new();
        let bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 20.0));
        let (project, obj_id) = make_straight_text_project(10.0, 2.0, 1.0, Some(50.0), bounds);
        *ctx.project.lock().unwrap() = Some(project);

        // Only height changes: 20→40, width stays 100
        let new_bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 40.0));
        let input = UpdateObjectInput {
            bounds: Some(new_bounds),
            ..Default::default()
        };
        let updated = update_object(&ctx, obj_id, input).unwrap();

        if let ObjectData::Text {
            font_size_mm,
            h_spacing,
            v_spacing,
            max_width,
            ..
        } = &updated.data
        {
            // Vertical properties doubled (sy=2)
            assert!(
                (font_size_mm - 20.0).abs() < 1e-6,
                "font_size_mm: {font_size_mm}"
            );
            assert!((v_spacing - 2.0).abs() < 1e-6, "v_spacing: {v_spacing}");
            // Horizontal properties unchanged (sx=1)
            assert!(
                (h_spacing - 2.0).abs() < 1e-6,
                "h_spacing should be unchanged: {h_spacing}"
            );
            assert!(
                (max_width.unwrap() - 50.0).abs() < 1e-6,
                "max_width should be unchanged: {max_width:?}"
            );
        } else {
            panic!("Expected Text variant");
        }
    }

    #[test]
    fn update_object_bounds_no_scale_for_path_text() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Path Text Test");
        let layer = Layer::new("Layer 1", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);
        let obj = ProjectObject::new(
            "PathLabel",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 20.0)),
            ObjectData::Text {
                content: "Hello".to_string(),
                font_family: "Arial".to_string(),
                font_size_mm: 10.0,
                alignment: TextAlignment::Left,
                alignment_v: TextAlignmentV::Top,
                bold: false,
                italic: false,
                upper_case: false,
                welded: false,
                h_spacing: 2.0,
                v_spacing: 1.0,
                on_path: true,
                path_offset: 5.0,
                distort: false,
                layout_mode: TextLayoutMode::Path,
                rtl: false,
                bend_radius: 0.0,
                transform_style: beambench_core::TextTransformStyle::None,
                transform_curve: 0.0,
                circle_placement: beambench_core::TextCirclePlacement::TopOutside,
                max_width: Some(50.0),
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
        let oid = obj.id;
        project.add_object(obj);
        *ctx.project.lock().unwrap() = Some(project);

        let new_bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(200.0, 40.0));
        let input = UpdateObjectInput {
            bounds: Some(new_bounds),
            ..Default::default()
        };
        let updated = update_object(&ctx, oid, input).unwrap();

        if let ObjectData::Text {
            font_size_mm,
            h_spacing,
            v_spacing,
            max_width,
            ..
        } = &updated.data
        {
            assert!(
                (font_size_mm - 10.0).abs() < 1e-6,
                "font_size_mm should be unchanged: {font_size_mm}"
            );
            assert!(
                (h_spacing - 2.0).abs() < 1e-6,
                "h_spacing should be unchanged: {h_spacing}"
            );
            assert!(
                (v_spacing - 1.0).abs() < 1e-6,
                "v_spacing should be unchanged: {v_spacing}"
            );
            assert!(
                (max_width.unwrap() - 50.0).abs() < 1e-6,
                "max_width should be unchanged: {max_width:?}"
            );
        } else {
            panic!("Expected Text variant");
        }
    }

    #[test]
    fn update_object_bounds_no_scale_for_bend_text() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Bend Text Test");
        let layer = Layer::new("Layer 1", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);
        let obj = ProjectObject::new(
            "BendLabel",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 20.0)),
            ObjectData::Text {
                content: "Curved".to_string(),
                font_family: "Arial".to_string(),
                font_size_mm: 10.0,
                alignment: TextAlignment::Left,
                alignment_v: TextAlignmentV::Top,
                bold: false,
                italic: false,
                upper_case: false,
                welded: false,
                h_spacing: 2.0,
                v_spacing: 1.0,
                on_path: false,
                path_offset: 0.0,
                distort: false,
                layout_mode: TextLayoutMode::Bend,
                rtl: false,
                bend_radius: 50.0,
                transform_style: beambench_core::TextTransformStyle::None,
                transform_curve: 0.0,
                circle_placement: beambench_core::TextCirclePlacement::TopOutside,
                max_width: Some(50.0),
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
        let oid = obj.id;
        project.add_object(obj);
        *ctx.project.lock().unwrap() = Some(project);

        let new_bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(200.0, 40.0));
        let input = UpdateObjectInput {
            bounds: Some(new_bounds),
            ..Default::default()
        };
        let updated = update_object(&ctx, oid, input).unwrap();

        if let ObjectData::Text {
            font_size_mm,
            h_spacing,
            v_spacing,
            max_width,
            bend_radius,
            ..
        } = &updated.data
        {
            assert!(
                (font_size_mm - 10.0).abs() < 1e-6,
                "font_size_mm should be unchanged: {font_size_mm}"
            );
            assert!(
                (h_spacing - 2.0).abs() < 1e-6,
                "h_spacing should be unchanged: {h_spacing}"
            );
            assert!(
                (v_spacing - 1.0).abs() < 1e-6,
                "v_spacing should be unchanged: {v_spacing}"
            );
            assert!(
                (max_width.unwrap() - 50.0).abs() < 1e-6,
                "max_width should be unchanged: {max_width:?}"
            );
            assert!(
                (bend_radius - 50.0).abs() < 1e-6,
                "bend_radius should be unchanged: {bend_radius}"
            );
        } else {
            panic!("Expected Text variant");
        }
    }

    #[test]
    fn update_object_bounds_no_scale_for_non_text() {
        let ctx = ServiceContext::new();
        let project = sample_project();
        let obj_id = project.objects[0].id;
        *ctx.project.lock().unwrap() = Some(project);

        let new_bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(20.0, 20.0));
        let input = UpdateObjectInput {
            bounds: Some(new_bounds),
            ..Default::default()
        };
        let updated = update_object(&ctx, obj_id, input).unwrap();

        // Shape data should be completely unchanged
        if let ObjectData::Shape {
            kind,
            width,
            height,
            corner_radius,
        } = &updated.data
        {
            assert_eq!(*kind, ShapeKind::Rectangle);
            assert!((width - 10.0).abs() < 1e-6);
            assert!((height - 10.0).abs() < 1e-6);
            assert!((corner_radius - 0.0).abs() < 1e-6);
        } else {
            panic!("Expected Shape variant");
        }
    }

    #[test]
    fn resize_shape_object_updates_bounds_and_shape_metadata_atomically() {
        let ctx = ServiceContext::new();
        let project = sample_project();
        let obj_id = project.objects[0].id;
        *ctx.project.lock().unwrap() = Some(project);

        let new_bounds = Bounds::new(Point2D::new(5.0, 6.0), Point2D::new(35.0, 26.0));
        let updated = resize_shape_object(&ctx, obj_id, new_bounds).unwrap();

        assert_eq!(updated.bounds, new_bounds);
        if let ObjectData::Shape { width, height, .. } = &updated.data {
            assert!((width - 30.0).abs() < 1e-6, "width: {width}");
            assert!((height - 20.0).abs() < 1e-6, "height: {height}");
        } else {
            panic!("Expected Shape variant");
        }
    }

    // ============================================================================
    // M4 — Layer/Cut Workflow Polish tests
    // ============================================================================

    use beambench_core::{CutEntryTemplate, LayerBatchToggle};

    /// Build a project with N layers, each given a distinct operation/speed/power so the
    /// cut-strength heuristic produces a non-trivial ordering.
    fn project_with_layers(specs: &[(OperationType, f64, f64, u32)]) -> Project {
        let mut project = Project::new("M4 test");
        for (i, (op, speed, power, passes)) in specs.iter().enumerate() {
            let mut layer = Layer::new(&format!("L{i}"), *op);
            layer.order_index = i as u32;
            let entry = layer.primary_entry_mut();
            entry.speed_mm_min = *speed;
            entry.power_percent = *power;
            match entry.operation {
                OperationType::Image | OperationType::Fill => {
                    if let Some(rs) = entry.raster_settings.as_mut() {
                        rs.passes = *passes;
                    }
                }
                _ => {
                    if let Some(vs) = entry.vector_settings.as_mut() {
                        vs.passes = *passes;
                    }
                }
            }
            project.add_layer(layer);
        }
        project
    }

    fn ctx_with(project: Project) -> std::sync::Arc<ServiceContext> {
        let ctx = ServiceContext::new();
        *ctx.project.lock().unwrap() = Some(project);
        std::sync::Arc::new(ctx)
    }

    fn add_rect(
        project: &mut Project,
        layer_id: LayerId,
        name: &str,
        min_x: f64,
        min_y: f64,
        max_x: f64,
        max_y: f64,
    ) -> ObjectId {
        let object = ProjectObject::new(
            name,
            layer_id,
            Bounds::new(Point2D::new(min_x, min_y), Point2D::new(max_x, max_y)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: max_x - min_x,
                height: max_y - min_y,
                corner_radius: 0.0,
            },
        );
        let id = object.id;
        project.add_object(object);
        id
    }

    fn add_line(
        project: &mut Project,
        layer_id: LayerId,
        name: &str,
        start: Point2D,
        end: Point2D,
    ) -> ObjectId {
        let min_x = start.x.min(end.x);
        let min_y = start.y.min(end.y);
        let max_x = start.x.max(end.x);
        let max_y = start.y.max(end.y);
        let object = ProjectObject::new(
            name,
            layer_id,
            Bounds::new(Point2D::new(min_x, min_y), Point2D::new(max_x, max_y)),
            ObjectData::VectorPath {
                path_data: format!("M {} {} L {} {}", start.x, start.y, end.x, end.y),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let id = object.id;
        project.add_object(object);
        id
    }

    fn add_vector_path(
        project: &mut Project,
        layer_id: LayerId,
        name: &str,
        bounds: Bounds,
        path_data: &str,
        closed: bool,
    ) -> ObjectId {
        let object = ProjectObject::new(
            name,
            layer_id,
            bounds,
            ObjectData::VectorPath {
                path_data: path_data.to_string(),
                closed,
                ruler_guide_axis: None,
            },
        );
        let id = object.id;
        project.add_object(object);
        id
    }

    // ---------- paste_layer_entries ----------

    #[test]
    fn paste_layer_entries_replaces_full_stack_with_fresh_ids_in_one_undo() {
        let mut project = Project::new("paste");
        let src = Layer::new("src", OperationType::Cut);
        let src_id = src.id;
        project.add_layer(src);
        // Add 2 more entries to src so we have a 3-entry stack to paste.
        project.add_cut_entry(src_id, None).unwrap();
        project.add_cut_entry(src_id, None).unwrap();
        let dst = Layer::new("dst", OperationType::Line);
        let dst_id = dst.id;
        project.add_layer(dst);
        let templates: Vec<CutEntryTemplate> = project
            .find_layer(src_id)
            .unwrap()
            .entries
            .iter()
            .map(CutEntryTemplate::from_entry)
            .collect();
        let src_ids: std::collections::HashSet<_> = project
            .find_layer(src_id)
            .unwrap()
            .entries
            .iter()
            .map(|e| e.id)
            .collect();

        let ctx = ctx_with(project);
        assert!(!ctx.undo_state().unwrap().can_undo);

        let result = paste_layer_entries(&ctx, dst_id, templates).unwrap();
        assert_eq!(result.id, dst_id);
        assert_eq!(result.entries.len(), 3);
        // No id aliasing.
        for entry in &result.entries {
            assert!(
                !src_ids.contains(&entry.id),
                "pasted entry id collided with source id"
            );
        }
        // Exactly one undo snapshot — undo once and history is empty.
        assert!(ctx.undo_state().unwrap().can_undo);
        undo_project(&ctx).unwrap();
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn paste_layer_entries_no_op_when_content_matches() {
        let mut project = Project::new("paste-noop");
        let layer = Layer::new("L", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);
        let templates: Vec<CutEntryTemplate> = project
            .find_layer(layer_id)
            .unwrap()
            .entries
            .iter()
            .map(CutEntryTemplate::from_entry)
            .collect();
        let ctx = ctx_with(project);
        let _ = paste_layer_entries(&ctx, layer_id, templates).unwrap();
        // No undo snapshot pushed because the content already matched.
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn paste_layer_entries_preserves_shell_fields() {
        let mut project = Project::new("paste-shell");
        let dst = Layer::new("dst-name", OperationType::Line);
        let dst_id = dst.id;
        project.add_layer(dst);
        // Mutate shell fields *after* add_layer (which assigns order_index = layers.len()).
        {
            let layer = project.find_layer_mut(dst_id).unwrap();
            layer.color_tag = ColorTag("#abcdef".to_string());
            layer.enabled = false;
            layer.visible = false;
        }
        let original_order_index = project.find_layer(dst_id).unwrap().order_index;
        let templates = vec![CutEntryTemplate::from_entry(
            project.find_layer(dst_id).unwrap().primary_entry(),
        )];
        // Mutate the template to force a non-no-op paste.
        let mut templates = templates;
        templates[0].speed_mm_min = 9999.0;
        let ctx = ctx_with(project);
        let result = paste_layer_entries(&ctx, dst_id, templates).unwrap();
        assert_eq!(result.name, "dst-name");
        assert_eq!(result.color_tag.0, "#abcdef");
        assert!(!result.enabled);
        assert!(!result.visible);
        assert_eq!(result.order_index, original_order_index);
        assert!(!result.is_tool_layer);
    }

    #[test]
    fn cut_entry_mutations_reject_tool_layers_consistently() {
        let mut project = Project::new("tool rejects");
        let mut layer = Layer::new("T1", OperationType::Tool);
        layer.canonicalize_tool_layer();
        let layer_id = layer.id;
        let entry_id = layer.primary_entry().id;
        project.add_layer(layer);
        let ctx = ctx_with(project);
        let template = CutEntryTemplate::from_entry(&CutEntry::new(OperationType::Line));

        for result in [
            reset_cut_entry_to_defaults(&ctx, layer_id, entry_id).map(|_| ()),
            paste_layer_entries(&ctx, layer_id, vec![template.clone()]).map(|_| ()),
            reorder_cut_entry(&ctx, layer_id, entry_id, 0).map(|_| ()),
            update_cut_entry(
                &ctx,
                layer_id,
                entry_id,
                CutEntryPatch {
                    speed_mm_min: Some(5000.0),
                    ..Default::default()
                },
            )
            .map(|_| ()),
        ] {
            let err = result.expect_err("tool-layer cut setting mutation should reject");
            assert!(
                err.to_string()
                    .contains("Tool layers do not support cut settings")
            );
        }
    }

    // ---------- reset_cut_entry_to_defaults ----------

    #[test]
    fn reset_cut_entry_to_defaults_restores_defaults_and_preserves_id() {
        let mut project = Project::new("reset");
        let layer = Layer::new("L", OperationType::Fill);
        let layer_id = layer.id;
        project.add_layer(layer);
        let entry_id = project.find_layer(layer_id).unwrap().primary_entry().id;
        // Mutate away from defaults.
        {
            let entry = project
                .find_layer_mut(layer_id)
                .unwrap()
                .primary_entry_mut();
            entry.speed_mm_min = 12.5;
            entry.power_percent = 99.0;
            entry.air_assist = true;
        }
        let ctx = ctx_with(project);
        let restored = reset_cut_entry_to_defaults(&ctx, layer_id, entry_id).unwrap();
        let defaults = CutEntry::defaults_for(OperationType::Fill);
        assert_eq!(restored.id, entry_id, "id must stay stable across reset");
        assert_eq!(restored.speed_mm_min, defaults.speed_mm_min);
        assert_eq!(restored.power_percent, defaults.power_percent);
        assert_eq!(restored.air_assist, defaults.air_assist);
    }

    #[test]
    fn reset_cut_entry_to_defaults_no_op_when_already_at_defaults() {
        let mut project = Project::new("reset-noop");
        let layer = Layer::new("L", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);
        let entry_id = project.find_layer(layer_id).unwrap().primary_entry().id;
        let ctx = ctx_with(project);
        let _ = reset_cut_entry_to_defaults(&ctx, layer_id, entry_id).unwrap();
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn defaults_centralization_layer_new_matches_defaults_for() {
        for op in [
            OperationType::Image,
            OperationType::Line,
            OperationType::Fill,
            OperationType::Score,
            OperationType::Cut,
            OperationType::OffsetFill,
            OperationType::Tool,
        ] {
            let from_layer = Layer::new_single_entry("x", op).primary_entry().clone();
            let from_defaults = CutEntry::defaults_for(op);
            assert_eq!(
                CutEntryTemplate::from_entry(&from_layer),
                CutEntryTemplate::from_entry(&from_defaults),
                "Layer::new_single_entry for {op:?} must equal CutEntry::defaults_for({op:?})"
            );
        }
    }

    // ---------- batch toggles ----------

    #[test]
    fn set_all_layers_enabled_all_off_then_invert_in_one_undo_each() {
        let project = project_with_layers(&[
            (OperationType::Cut, 1000.0, 50.0, 1),
            (OperationType::Fill, 1000.0, 50.0, 1),
            (OperationType::Line, 1000.0, 50.0, 1),
        ]);
        let ctx = ctx_with(project);
        let after_off = set_all_layers_enabled(&ctx, LayerBatchToggle::AllOff).unwrap();
        assert!(after_off.iter().all(|l| !l.enabled));
        undo_project(&ctx).unwrap();
        assert!(
            !ctx.undo_state().unwrap().can_undo,
            "exactly one undo snapshot for AllOff"
        );

        // Re-apply AllOff, then Invert.
        let _ = set_all_layers_enabled(&ctx, LayerBatchToggle::AllOff).unwrap();
        let after_invert = set_all_layers_enabled(&ctx, LayerBatchToggle::Invert).unwrap();
        assert!(after_invert.iter().all(|l| l.enabled));
    }

    #[test]
    fn set_all_layers_enabled_all_on_when_already_on_is_noop() {
        let project = project_with_layers(&[(OperationType::Cut, 1000.0, 50.0, 1)]);
        let ctx = ctx_with(project);
        let _ = set_all_layers_enabled(&ctx, LayerBatchToggle::AllOn).unwrap();
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn only_this_on_disables_every_other_layer_in_one_undo() {
        let project = project_with_layers(&[
            (OperationType::Cut, 1000.0, 50.0, 1),
            (OperationType::Fill, 1000.0, 50.0, 1),
            (OperationType::Line, 1000.0, 50.0, 1),
        ]);
        let keep_id = {
            let p = project.layers[1].id;
            p
        };
        let ctx = ctx_with(project);
        let after =
            set_all_layers_enabled(&ctx, LayerBatchToggle::OnlyThisOn { keep: keep_id }).unwrap();
        for l in &after {
            assert_eq!(l.enabled, l.id == keep_id);
        }
        undo_project(&ctx).unwrap();
        assert!(!ctx.undo_state().unwrap().can_undo);

        // Calling again with the same `keep` is a no-op.
        let _ =
            set_all_layers_enabled(&ctx, LayerBatchToggle::OnlyThisOn { keep: keep_id }).unwrap();
        let _ =
            set_all_layers_enabled(&ctx, LayerBatchToggle::OnlyThisOn { keep: keep_id }).unwrap();
        undo_project(&ctx).unwrap();
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    // ---------- sort_layers_cut_last ----------

    #[test]
    fn sort_layers_cut_last_engraves_first_then_score_then_cut() {
        // L1 Fill (engrave-class, low energy) → first
        // L2 Line (heavy cut: low speed, full power) → last
        // L3 Score (mid)
        // L4 Image (engrave-class, low energy) → near first
        let project = project_with_layers(&[
            (OperationType::Fill, 5000.0, 30.0, 1),
            (OperationType::Line, 100.0, 100.0, 1),
            (OperationType::Score, 1000.0, 40.0, 1),
            (OperationType::Image, 5000.0, 20.0, 1),
        ]);
        let ids: Vec<_> = project
            .layers
            .iter()
            .map(|l| (l.name.clone(), l.id))
            .collect();
        let ctx = ctx_with(project);
        let after = sort_layers_cut_last(&ctx).unwrap();
        // Build the new ordering by id, then assert L2 (heavy Line cut) is last.
        let by_order: Vec<_> = {
            let mut v: Vec<_> = after
                .iter()
                .map(|l| (l.order_index, l.name.clone()))
                .collect();
            v.sort_by_key(|(idx, _)| *idx);
            v.into_iter().map(|(_, name)| name).collect()
        };
        assert_eq!(by_order.last().unwrap(), "L1", "wait, L2 not last");
        // Actually: L0=Fill, L1=Line, L2=Score, L3=Image (project_with_layers uses i for name).
        assert_eq!(
            by_order.last().unwrap(),
            "L1",
            "the heavy Line cut must sort last"
        );
        // Sanity: L0 and L3 (engraves) come before L2 (Score) which comes before L1 (Cut-class).
        let pos_l0 = by_order.iter().position(|n| n == "L0").unwrap();
        let pos_l1 = by_order.iter().position(|n| n == "L1").unwrap();
        let pos_l2 = by_order.iter().position(|n| n == "L2").unwrap();
        let pos_l3 = by_order.iter().position(|n| n == "L3").unwrap();
        assert!(pos_l0 < pos_l2, "Fill before Score");
        assert!(pos_l3 < pos_l2, "Image before Score");
        assert!(pos_l2 < pos_l1, "Score before heavy Line");
        let _ = ids; // silence unused
    }

    #[test]
    fn sort_layers_cut_last_idempotent_after_first_sort() {
        let project = project_with_layers(&[
            (OperationType::Fill, 5000.0, 30.0, 1),
            (OperationType::Line, 100.0, 100.0, 1),
            (OperationType::Score, 1000.0, 40.0, 1),
        ]);
        let ctx = ctx_with(project);
        let _ = sort_layers_cut_last(&ctx).unwrap();
        undo_project(&ctx).unwrap();
        // Now re-sort fresh, then call again — second call must be no-op.
        let _ = sort_layers_cut_last(&ctx).unwrap();
        let undo_before = ctx.undo_state().unwrap().can_undo;
        let _ = sort_layers_cut_last(&ctx).unwrap();
        let undo_after = ctx.undo_state().unwrap().can_undo;
        assert_eq!(
            undo_before, undo_after,
            "second sort must be a no-op (no extra snapshot)"
        );
        // And undoing once must clear the only snapshot.
        undo_project(&ctx).unwrap();
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn sort_layers_cut_last_reorders_the_layers_vec_not_just_order_index() {
        // Regression: previously the op restamped order_index but left project.layers in its
        // original insertion order, so the UI (which renders layers.map(...) in array order)
        // stayed visually unsorted. Frontend should see a fully reordered Vec, AND the no-op
        // short-circuit must compare against Vec order so a second call is a true no-op.
        let project = project_with_layers(&[
            (OperationType::Cut, 100.0, 100.0, 1), // L0 = heavy cut → must end up last
            (OperationType::Fill, 5000.0, 30.0, 1), // L1 = engrave → must end up first
            (OperationType::Score, 1000.0, 40.0, 1), // L2 = mid
        ]);
        let heavy_cut_id = project.layers[0].id;
        let engrave_id = project.layers[1].id;
        let score_id = project.layers[2].id;
        let ctx = ctx_with(project);

        let after = sort_layers_cut_last(&ctx).unwrap();

        // Vec must be in cut-strength order: engrave → score → heavy cut.
        assert_eq!(
            after.iter().map(|l| l.id).collect::<Vec<_>>(),
            vec![engrave_id, score_id, heavy_cut_id],
            "project.layers Vec must be reordered, not just order_index restamped",
        );

        // order_index must mirror the new Vec order.
        for (i, l) in after.iter().enumerate() {
            assert_eq!(
                l.order_index, i as u32,
                "order_index drifted from Vec index"
            );
        }

        // Second call must be a true no-op (no extra undo snapshot).
        let undo_before = ctx.undo_state().unwrap().can_undo;
        let _ = sort_layers_cut_last(&ctx).unwrap();
        let undo_after = ctx.undo_state().unwrap().can_undo;
        assert_eq!(
            undo_before, undo_after,
            "second sort must not push another snapshot"
        );
        // And one undo restores everything (proves first call was exactly one snapshot).
        undo_project(&ctx).unwrap();
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn only_this_on_with_unknown_layer_returns_not_found() {
        // Regression: without keep-id validation, a bogus `keep` matched no layer and silently
        // disabled every layer — a stale row action would nuke the whole project's enabled
        // state with no error to the user.
        let project = project_with_layers(&[
            (OperationType::Cut, 1000.0, 50.0, 1),
            (OperationType::Fill, 1000.0, 50.0, 1),
        ]);
        let bogus_id = LayerId::new();
        let ctx = ctx_with(project);
        let result = set_all_layers_enabled(&ctx, LayerBatchToggle::OnlyThisOn { keep: bogus_id });
        assert!(result.is_err(), "must reject unknown keep id");
        // No layers must have been mutated; no undo snapshot pushed.
        assert!(!ctx.undo_state().unwrap().can_undo);
        let layers_after = ctx.project.lock().unwrap().as_ref().unwrap().layers.clone();
        for l in layers_after {
            assert!(l.enabled, "no layer should have been disabled");
        }

        // Same guard for the visible variant.
        let result = set_all_layers_visible(&ctx, LayerBatchToggle::OnlyThisOn { keep: bogus_id });
        assert!(result.is_err());
    }

    #[test]
    fn cut_strength_low_power_line_sorts_before_heavy_cut() {
        // Edge case from the plan: a Line layer set up as a low-power score-like pass
        // should sort BEFORE a heavy-power Cut. Energy dominates; line-bias only ties.
        let lo_line = Layer::new("lo_line", OperationType::Line);
        let lo_line = {
            let mut l = lo_line;
            l.primary_entry_mut().speed_mm_min = 5000.0;
            l.primary_entry_mut().power_percent = 5.0;
            l
        };
        let hi_cut = Layer::new("hi_cut", OperationType::Cut);
        let hi_cut = {
            let mut l = hi_cut;
            l.primary_entry_mut().speed_mm_min = 100.0;
            l.primary_entry_mut().power_percent = 100.0;
            l
        };
        assert!(
            lo_line.cut_strength() < hi_cut.cut_strength(),
            "lo_line cut_strength {} must be < hi_cut {} so it sorts earlier",
            lo_line.cut_strength(),
            hi_cut.cut_strength()
        );
    }

    #[test]
    fn move_objects_together_keeps_anchor_fixed_and_moves_neighbors() {
        let mut project = Project::new("move-together");
        let layer = Layer::new("L", OperationType::Line);
        let layer_id = layer.id;
        project.add_layer(layer);
        let left_id = add_rect(&mut project, layer_id, "left", 0.0, 0.0, 10.0, 10.0);
        let anchor_id = add_rect(&mut project, layer_id, "anchor", 20.0, 0.0, 30.0, 10.0);
        let right_id = add_rect(&mut project, layer_id, "right", 50.0, 0.0, 60.0, 10.0);
        let ctx = ctx_with(project);

        let updated = move_objects_together(
            &ctx,
            vec![left_id, anchor_id, right_id],
            MoveTogetherAxis::Horizontal,
            anchor_id,
        )
        .unwrap();
        let by_id: HashMap<_, _> = updated
            .into_iter()
            .map(|object| (object.id, object))
            .collect();
        assert_eq!(by_id[&left_id].bounds.max.x, 20.0);
        assert_eq!(by_id[&right_id].bounds.min.x, 30.0);
        let anchor_bounds = ctx
            .project
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .find_object(anchor_id)
            .unwrap()
            .bounds;
        assert_eq!(anchor_bounds.min.x, 20.0);
        assert_eq!(anchor_bounds.max.x, 30.0);
        assert!(ctx.undo_state().unwrap().can_undo);
        undo_project(&ctx).unwrap();
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn move_objects_together_noop_when_objects_already_abut() {
        let mut project = Project::new("move-together-noop");
        let layer = Layer::new("L", OperationType::Line);
        let layer_id = layer.id;
        project.add_layer(layer);
        let left_id = add_rect(&mut project, layer_id, "left", 0.0, 0.0, 10.0, 10.0);
        let anchor_id = add_rect(&mut project, layer_id, "anchor", 10.0, 0.0, 20.0, 10.0);
        let ctx = ctx_with(project);

        let updated = move_objects_together(
            &ctx,
            vec![left_id, anchor_id],
            MoveTogetherAxis::Horizontal,
            anchor_id,
        )
        .unwrap();
        assert!(updated.is_empty());
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn align_objects_uses_requested_anchor_when_no_objects_are_locked() {
        let mut project = Project::new("align-anchor");
        let layer = Layer::new("L", OperationType::Line);
        let layer_id = layer.id;
        project.add_layer(layer);
        let left_id = add_rect(&mut project, layer_id, "left", 0.0, 0.0, 10.0, 10.0);
        let anchor_id = add_rect(&mut project, layer_id, "anchor", 30.0, 20.0, 50.0, 40.0);
        let right_id = add_rect(&mut project, layer_id, "right", 80.0, 60.0, 90.0, 70.0);
        let ctx = ctx_with(project);

        let updated = align_objects(
            &ctx,
            vec![left_id, anchor_id, right_id],
            "centers_v",
            Some(anchor_id),
        )
        .unwrap();
        let by_id: HashMap<_, _> = updated
            .into_iter()
            .map(|object| (object.id, object))
            .collect();
        assert!((bounds_center(by_id[&left_id].bounds).x - 40.0).abs() < 1e-6);
        assert!((bounds_center(by_id[&right_id].bounds).x - 40.0).abs() < 1e-6);

        let project = ctx.project.lock().unwrap();
        let anchor = project.as_ref().unwrap().find_object(anchor_id).unwrap();
        assert!((anchor.bounds.min.x - 30.0).abs() < 1e-6);
        assert!((anchor.bounds.min.y - 20.0).abs() < 1e-6);
    }

    #[test]
    fn align_objects_single_locked_object_becomes_anchor() {
        let mut project = Project::new("align-locked-anchor");
        let layer = Layer::new("L", OperationType::Line);
        let layer_id = layer.id;
        project.add_layer(layer);
        let left_id = add_rect(&mut project, layer_id, "left", 0.0, 0.0, 10.0, 10.0);
        let locked_id = add_rect(&mut project, layer_id, "locked", 30.0, 0.0, 40.0, 10.0);
        let right_id = add_rect(&mut project, layer_id, "right", 80.0, 0.0, 90.0, 10.0);
        project.find_object_mut(locked_id).unwrap().locked = true;
        let ctx = ctx_with(project);

        let updated = align_objects(
            &ctx,
            vec![left_id, locked_id, right_id],
            "left",
            Some(right_id),
        )
        .unwrap();
        let by_id: HashMap<_, _> = updated
            .into_iter()
            .map(|object| (object.id, object))
            .collect();
        assert_eq!(by_id[&left_id].bounds.min.x, 30.0);
        assert_eq!(by_id[&right_id].bounds.min.x, 30.0);

        let project = ctx.project.lock().unwrap();
        let locked = project.as_ref().unwrap().find_object(locked_id).unwrap();
        assert_eq!(locked.bounds.min.x, 30.0);
    }

    #[test]
    fn align_objects_multiple_locked_objects_stay_fixed() {
        let mut project = Project::new("align-multiple-locked");
        let layer = Layer::new("L", OperationType::Line);
        let layer_id = layer.id;
        project.add_layer(layer);
        let locked_left_id = add_rect(&mut project, layer_id, "locked-left", 0.0, 0.0, 10.0, 10.0);
        let anchor_id = add_rect(&mut project, layer_id, "anchor", 30.0, 0.0, 40.0, 10.0);
        let peer_id = add_rect(&mut project, layer_id, "peer", 80.0, 0.0, 90.0, 10.0);
        let locked_right_id = add_rect(
            &mut project,
            layer_id,
            "locked-right",
            120.0,
            0.0,
            130.0,
            10.0,
        );
        project.find_object_mut(locked_left_id).unwrap().locked = true;
        project.find_object_mut(locked_right_id).unwrap().locked = true;
        let ctx = ctx_with(project);

        let updated = align_objects(
            &ctx,
            vec![locked_left_id, anchor_id, peer_id, locked_right_id],
            "right",
            Some(anchor_id),
        )
        .unwrap();
        let by_id: HashMap<_, _> = updated
            .into_iter()
            .map(|object| (object.id, object))
            .collect();
        assert_eq!(by_id[&peer_id].bounds.max.x, 40.0);

        let project = ctx.project.lock().unwrap();
        let project = project.as_ref().unwrap();
        assert_eq!(
            project.find_object(locked_left_id).unwrap().bounds.min.x,
            0.0
        );
        assert_eq!(
            project.find_object(locked_right_id).unwrap().bounds.min.x,
            120.0
        );
    }

    #[test]
    fn mirror_across_line_uses_axis_and_duplicates_only_once() {
        let mut project = Project::new("mirror");
        let layer = Layer::new("L", OperationType::Line);
        let layer_id = layer.id;
        project.add_layer(layer);
        let source_id = add_rect(&mut project, layer_id, "source", 0.0, 0.0, 10.0, 10.0);
        let axis_id = add_line(
            &mut project,
            layer_id,
            "axis",
            Point2D::new(20.0, 0.0),
            Point2D::new(20.0, 40.0),
        );
        let ctx = ctx_with(project);

        let duplicated = mirror_across_line(&ctx, vec![source_id, axis_id], axis_id).unwrap();
        assert_eq!(duplicated.len(), 1);
        let duplicated_bounds = duplicated[0].bounds;
        assert!((duplicated_bounds.min.x - 30.0).abs() < 1e-6);
        assert!((duplicated_bounds.max.x - 40.0).abs() < 1e-6);
        let axis_bounds = ctx
            .project
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .find_object(axis_id)
            .unwrap()
            .bounds;
        assert_eq!(axis_bounds.min.x, 20.0);
        assert_eq!(axis_bounds.max.x, 20.0);
        assert!(ctx.undo_state().unwrap().can_undo);
        undo_project(&ctx).unwrap();
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn mirror_across_line_accepts_straight_cubic_axis() {
        let mut project = Project::new("mirror-cubic-axis");
        let layer = Layer::new("L", OperationType::Line);
        let layer_id = layer.id;
        project.add_layer(layer);
        let source_id = add_rect(&mut project, layer_id, "source", 0.0, 0.0, 10.0, 10.0);
        let axis_id = add_vector_path(
            &mut project,
            layer_id,
            "axis",
            Bounds::new(Point2D::new(20.0, 0.0), Point2D::new(20.0, 40.0)),
            "M 20 0 C 20 0 20 40 20 40",
            false,
        );
        let ctx = ctx_with(project);

        let duplicated = mirror_across_line(&ctx, vec![source_id, axis_id], axis_id).unwrap();

        assert_eq!(duplicated.len(), 1);
        assert!((duplicated[0].bounds.min.x - 30.0).abs() < 1e-6);
        assert!((duplicated[0].bounds.max.x - 40.0).abs() < 1e-6);
    }

    #[test]
    fn mirror_across_line_accepts_tool_layer_axis_without_mirroring_it() {
        let mut project = Project::new("mirror-tool-axis");
        let layer = Layer::new("L", OperationType::Line);
        let layer_id = layer.id;
        project.add_layer(layer);
        let mut tool_layer = Layer::new("T1", OperationType::Line);
        tool_layer.is_tool_layer = true;
        let tool_layer_id = tool_layer.id;
        project.add_layer(tool_layer);
        let source_id = add_rect(&mut project, layer_id, "source", 0.0, 0.0, 10.0, 10.0);
        let axis_id = add_line(
            &mut project,
            tool_layer_id,
            "tool-axis",
            Point2D::new(20.0, 0.0),
            Point2D::new(20.0, 40.0),
        );
        let ctx = ctx_with(project);

        let duplicated = mirror_across_line(&ctx, vec![source_id, axis_id], axis_id).unwrap();

        assert_eq!(duplicated.len(), 1);
        assert_eq!(duplicated[0].layer_id, layer_id);
        let project_guard = ctx.project.lock().unwrap();
        let project = project_guard.as_ref().unwrap();
        assert_eq!(
            project
                .objects
                .iter()
                .filter(|object| object.layer_id == tool_layer_id)
                .count(),
            1
        );
        assert!(project.find_object(axis_id).is_some());
    }

    #[test]
    fn mirror_across_line_reflects_group_members() {
        let mut project = Project::new("mirror-group");
        let layer = Layer::new("L", OperationType::Line);
        let layer_id = layer.id;
        project.add_layer(layer);
        let child_id = add_rect(&mut project, layer_id, "child", 0.0, 0.0, 10.0, 10.0);
        let group = ProjectObject::new(
            "group",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::Group {
                children: vec![child_id],
            },
        );
        let group_id = group.id;
        project.add_object(group);
        let axis_id = add_line(
            &mut project,
            layer_id,
            "axis",
            Point2D::new(20.0, 0.0),
            Point2D::new(20.0, 40.0),
        );
        let ctx = ctx_with(project);

        let duplicated = mirror_across_line(&ctx, vec![group_id, axis_id], axis_id).unwrap();
        assert_eq!(duplicated.len(), 2);

        let duplicated_group = duplicated
            .iter()
            .find(|object| matches!(object.data, ObjectData::Group { .. }))
            .unwrap();
        let duplicated_child_id = match &duplicated_group.data {
            ObjectData::Group { children } => children[0],
            _ => unreachable!(),
        };
        let duplicated_child = ctx
            .project
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .find_object(duplicated_child_id)
            .unwrap()
            .clone();
        assert!((duplicated_child.bounds.min.x - 30.0).abs() < 1e-6);
        assert!((duplicated_child.bounds.max.x - 40.0).abs() < 1e-6);
    }

    #[test]
    fn mirror_across_line_reflects_across_diagonal_axis() {
        let mut project = Project::new("mirror-diagonal");
        let layer = Layer::new("L", OperationType::Line);
        let layer_id = layer.id;
        project.add_layer(layer);
        let source_id = add_rect(&mut project, layer_id, "source", 30.0, 10.0, 40.0, 20.0);
        let axis_id = add_line(
            &mut project,
            layer_id,
            "axis",
            Point2D::new(0.0, 0.0),
            Point2D::new(40.0, 40.0),
        );
        let ctx = ctx_with(project);

        let duplicated = mirror_across_line(&ctx, vec![source_id, axis_id], axis_id).unwrap();
        assert_eq!(duplicated.len(), 1);
        assert!((duplicated[0].bounds.min.x - 10.0).abs() < 1e-6);
        assert!((duplicated[0].bounds.min.y - 30.0).abs() < 1e-6);
        assert!((duplicated[0].bounds.max.x - 20.0).abs() < 1e-6);
        assert!((duplicated[0].bounds.max.y - 40.0).abs() < 1e-6);
    }

    #[test]
    fn mirror_across_line_rejects_invalid_axis_variants() {
        let mut project = Project::new("mirror-invalid");
        let layer = Layer::new("L", OperationType::Line);
        let layer_id = layer.id;
        project.add_layer(layer);
        let source_id = add_rect(&mut project, layer_id, "source", 0.0, 0.0, 10.0, 10.0);
        let closed_id = add_vector_path(
            &mut project,
            layer_id,
            "closed",
            Bounds::new(Point2D::new(20.0, 0.0), Point2D::new(30.0, 10.0)),
            "M 20 0 L 30 0 Z",
            true,
        );
        let curved_id = add_vector_path(
            &mut project,
            layer_id,
            "curve",
            Bounds::new(Point2D::new(20.0, 0.0), Point2D::new(30.0, 10.0)),
            "M 20 0 Q 25 10 30 0",
            false,
        );
        let curved_cubic_id = add_vector_path(
            &mut project,
            layer_id,
            "cubic-curve",
            Bounds::new(Point2D::new(20.0, 0.0), Point2D::new(30.0, 10.0)),
            "M 20 0 C 22 10 28 10 30 0",
            false,
        );
        let multi_id = add_vector_path(
            &mut project,
            layer_id,
            "multi",
            Bounds::new(Point2D::new(20.0, 0.0), Point2D::new(40.0, 10.0)),
            "M 20 0 L 30 0 L 40 0",
            false,
        );
        let zero_id = add_vector_path(
            &mut project,
            layer_id,
            "zero",
            Bounds::new(Point2D::new(20.0, 0.0), Point2D::new(20.0, 0.0)),
            "M 20 0 L 20 0",
            false,
        );
        let ctx = ctx_with(project);

        for axis_id in [closed_id, curved_id, curved_cubic_id, multi_id] {
            let err = mirror_across_line(&ctx, vec![source_id, axis_id], axis_id).unwrap_err();
            assert!(
                err.to_string()
                    .contains("Mirror axis must be a single open straight segment")
            );
        }
        let err = mirror_across_line(&ctx, vec![source_id, zero_id], zero_id).unwrap_err();
        assert!(
            err.to_string()
                .contains("Mirror axis must have non-zero length")
        );
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn make_same_size_width_preserves_peer_center() {
        let mut project = Project::new("same-size");
        let layer = Layer::new("L", OperationType::Line);
        let layer_id = layer.id;
        project.add_layer(layer);
        let peer_id = add_rect(&mut project, layer_id, "peer", 0.0, 0.0, 10.0, 20.0);
        let anchor_id = add_rect(&mut project, layer_id, "anchor", 20.0, 0.0, 50.0, 10.0);
        let ctx = ctx_with(project);

        let updated = make_same_size(
            &ctx,
            vec![peer_id, anchor_id],
            anchor_id,
            SameSizeAxis::Width,
            false,
        )
        .unwrap();
        let peer = updated
            .into_iter()
            .find(|object| object.id == peer_id)
            .unwrap();
        assert!((peer.bounds.width() - 30.0).abs() < 1e-6);
        assert!((bounds_center(peer.bounds).x - 5.0).abs() < 1e-6);
        assert!((bounds_center(peer.bounds).y - 10.0).abs() < 1e-6);
    }

    #[test]
    fn make_same_size_height_with_preserve_aspect_scales_width() {
        let mut project = Project::new("same-size-height");
        let layer = Layer::new("L", OperationType::Line);
        let layer_id = layer.id;
        project.add_layer(layer);
        let peer_id = add_rect(&mut project, layer_id, "peer", 0.0, 0.0, 10.0, 20.0);
        let anchor_id = add_rect(&mut project, layer_id, "anchor", 20.0, 0.0, 40.0, 50.0);
        let ctx = ctx_with(project);

        let updated = make_same_size(
            &ctx,
            vec![peer_id, anchor_id],
            anchor_id,
            SameSizeAxis::Height,
            true,
        )
        .unwrap();
        let peer = updated
            .into_iter()
            .find(|object| object.id == peer_id)
            .unwrap();
        assert!((peer.bounds.height() - 50.0).abs() < 1e-6);
        assert!((peer.bounds.width() - 25.0).abs() < 1e-6);
    }

    #[test]
    fn make_same_size_honors_lock_aspect_ratio_without_shift() {
        let mut project = Project::new("same-size-locked-aspect");
        let layer = Layer::new("L", OperationType::Line);
        let layer_id = layer.id;
        project.add_layer(layer);
        let peer_id = add_rect(&mut project, layer_id, "peer", 0.0, 0.0, 10.0, 20.0);
        project.find_object_mut(peer_id).unwrap().lock_aspect_ratio = true;
        let anchor_id = add_rect(&mut project, layer_id, "anchor", 20.0, 0.0, 50.0, 10.0);
        let ctx = ctx_with(project);

        let updated = make_same_size(
            &ctx,
            vec![peer_id, anchor_id],
            anchor_id,
            SameSizeAxis::Width,
            false,
        )
        .unwrap();
        let peer = updated
            .into_iter()
            .find(|object| object.id == peer_id)
            .unwrap();
        assert!((peer.bounds.width() - 30.0).abs() < 1e-6);
        assert!((peer.bounds.height() - 60.0).abs() < 1e-6);
    }

    #[test]
    fn dock_objects_ignores_tool_layer_guides_and_applies_edge_padding() {
        let mut project = Project::new("dock");
        let layer = Layer::new("L", OperationType::Line);
        let layer_id = layer.id;
        project.add_layer(layer);
        let mut tool_layer = Layer::new("T1", OperationType::Line);
        tool_layer.is_tool_layer = true;
        let tool_layer_id = tool_layer.id;
        project.add_layer(tool_layer);

        let movable_id = add_rect(&mut project, layer_id, "movable", 40.0, 40.0, 50.0, 50.0);
        project.add_object(ProjectObject::new(
            "guide",
            tool_layer_id,
            Bounds::new(Point2D::new(5.0, 0.0), Point2D::new(5.0, 100.0)),
            ObjectData::VectorPath {
                path_data: "M 5 0 L 5 100".into(),
                closed: false,
                ruler_guide_axis: Some(beambench_core::object::GuideAxis::Vertical),
            },
        ));
        let ctx = ctx_with(project);

        let updated = dock_objects(
            &ctx,
            vec![movable_id],
            DockDirection::Left,
            DockOptions {
                move_as_group: false,
                lock_inner_objects: false,
                padding_mm: 3.0,
            },
        )
        .unwrap();
        assert_eq!(updated.len(), 1);
        assert!((updated[0].bounds.min.x - 3.0).abs() < 1e-6);
        assert!((updated[0].bounds.max.x - 13.0).abs() < 1e-6);
    }

    #[test]
    fn resize_slots_resizes_standalone_rectangle_and_noops_when_already_matching() {
        let mut project = Project::new("slots");
        let layer = Layer::new("L", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);
        let rect_id = add_rect(&mut project, layer_id, "slot", 0.0, 0.0, 3.0, 20.0);
        let ctx = ctx_with(project);

        let updated = resize_slots(
            &ctx,
            vec![rect_id],
            ResizeSlotsOptions {
                current_thickness_mm: 3.0,
                new_thickness_mm: 4.0,
                tolerance_mm: 0.2,
            },
        )
        .unwrap();
        assert_eq!(updated.len(), 1);
        assert!((updated[0].bounds.width() - 4.2).abs() < 1e-6);

        undo_project(&ctx).unwrap();
        let noop = resize_slots(
            &ctx,
            vec![rect_id],
            ResizeSlotsOptions {
                current_thickness_mm: 3.0,
                new_thickness_mm: 3.0,
                tolerance_mm: 0.0,
            },
        )
        .unwrap();
        assert!(noop.is_empty());
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn resize_slots_resizes_rectangular_hole_and_preserves_outer_contour() {
        let mut project = Project::new("slots-hole");
        let layer = Layer::new("L", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);
        let path_id = add_vector_path(
            &mut project,
            layer_id,
            "compound",
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(30.0, 30.0)),
            "M 0 0 L 30 0 L 30 30 L 0 30 Z M 10 5 L 13 5 L 13 25 L 10 25 Z",
            true,
        );
        let ctx = ctx_with(project);

        let updated = resize_slots(
            &ctx,
            vec![path_id],
            ResizeSlotsOptions {
                current_thickness_mm: 3.0,
                new_thickness_mm: 4.0,
                tolerance_mm: 0.2,
            },
        )
        .unwrap();
        let object = updated
            .into_iter()
            .find(|object| object.id == path_id)
            .unwrap();
        let ObjectData::VectorPath { path_data, .. } = object.data else {
            panic!("expected vector path");
        };
        let parsed = VecPath::parse_svg_d(&path_data);
        assert_eq!(parsed.subpaths.len(), 2);
        let outer = &parsed.subpaths[0];
        let inner = &parsed.subpaths[1];
        match outer.commands[0] {
            PathCommand::MoveTo { x, y } => {
                assert!((x - 0.0).abs() < 1e-6);
                assert!((y - 0.0).abs() < 1e-6);
            }
            _ => panic!("expected outer move command"),
        }
        assert!(outer.closed);
        match outer.commands[1] {
            PathCommand::LineTo { x, y } => {
                assert!((x - 30.0).abs() < 1e-6);
                assert!((y - 0.0).abs() < 1e-6);
            }
            _ => panic!("expected preserved outer edge"),
        }
        match inner.commands[0] {
            PathCommand::MoveTo { x, y } => {
                assert!((x - 9.4).abs() < 1e-6);
                assert!((y - 5.0).abs() < 1e-6);
            }
            _ => panic!("expected inner move command"),
        }
        assert!(inner.closed);
        match inner.commands[1] {
            PathCommand::LineTo { x, y } => {
                assert!((x - 13.6).abs() < 1e-6);
                assert!((y - 5.0).abs() < 1e-6);
            }
            _ => panic!("expected horizontal inner slot edge"),
        }
    }

    #[test]
    fn resize_slots_skips_square_and_honors_eligibility_tolerance() {
        let mut project = Project::new("slots-filter");
        let layer = Layer::new("L", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);
        let square_id = add_rect(&mut project, layer_id, "square", 0.0, 0.0, 3.0, 3.0);
        let near_id = add_rect(&mut project, layer_id, "near", 10.0, 0.0, 12.95, 20.0);
        let far_id = add_rect(&mut project, layer_id, "far", 20.0, 0.0, 22.8, 20.0);
        let ctx = ctx_with(project);

        let updated = resize_slots(
            &ctx,
            vec![square_id, near_id, far_id],
            ResizeSlotsOptions {
                current_thickness_mm: 3.0,
                new_thickness_mm: 4.0,
                tolerance_mm: 0.2,
            },
        )
        .unwrap();
        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].id, near_id);
        assert!((updated[0].bounds.width() - 4.2).abs() < 1e-6);
    }

    #[test]
    fn move_objects_together_resolves_virtual_clones_before_mutation() {
        let mut project = Project::new("clone-move");
        let layer = Layer::new("L", OperationType::Line);
        let layer_id = layer.id;
        project.add_layer(layer);
        let source_id = add_rect(&mut project, layer_id, "source", 0.0, 0.0, 10.0, 10.0);
        let clone = ProjectObject::new(
            "clone",
            layer_id,
            Bounds::new(Point2D::new(30.0, 0.0), Point2D::new(40.0, 10.0)),
            ObjectData::VirtualClone { source_id },
        );
        let clone_id = clone.id;
        project.add_object(clone);
        let anchor_id = add_rect(&mut project, layer_id, "anchor", 60.0, 0.0, 70.0, 10.0);
        let ctx = ctx_with(project);

        let updated = move_objects_together(
            &ctx,
            vec![clone_id, anchor_id],
            MoveTogetherAxis::Horizontal,
            anchor_id,
        )
        .unwrap();
        let clone = updated.iter().find(|object| object.id == clone_id).unwrap();
        assert_ne!(clone.data, ObjectData::VirtualClone { source_id });
        assert_eq!(clone.bounds.max.x, 60.0);
    }

    #[test]
    fn move_objects_together_rejects_empty_groups() {
        let mut project = Project::new("empty-group");
        let layer = Layer::new("L", OperationType::Line);
        let layer_id = layer.id;
        project.add_layer(layer);
        let empty_group = ProjectObject::new(
            "Group",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(0.0, 0.0)),
            ObjectData::Group { children: vec![] },
        );
        let group_id = empty_group.id;
        project.add_object(empty_group);
        let anchor_id = add_rect(&mut project, layer_id, "anchor", 10.0, 0.0, 20.0, 10.0);
        let ctx = ctx_with(project);

        let result = move_objects_together(
            &ctx,
            vec![group_id, anchor_id],
            MoveTogetherAxis::Horizontal,
            anchor_id,
        );
        assert!(result.is_err());
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn advance_auto_variable_text_advances_only_enabled_objects_with_single_undo() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("auto-advance");
        let layer = Layer::new("Layer 1", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);

        let auto_id = project
            .add_object(ProjectObject::new(
                "Auto",
                layer_id,
                Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(20.0, 5.0)),
                ObjectData::Text {
                    content: "SN-1".to_string(),
                    font_family: "Arial".to_string(),
                    font_size_mm: 5.0,
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
                    resolved_path_data: None,
                    missing_font: false,
                    missing_glyphs: Vec::new(),
                    guide_path_id: None,
                    variable_text: Some(VariableTextConfig {
                        template: "SN-{Serial}".to_string(),
                        mode: Some(beambench_core::variable_text::VariableTextMode::SerialNumber),
                        offset: None,
                        source: beambench_core::variable_text::VariableTextSource {
                            current: 1,
                            start: 1,
                            end: 5,
                            advance_by: 2,
                            auto_advance: true,
                            ..Default::default()
                        },
                    }),
                },
            ))
            .id;

        let manual_id = project
            .add_object(ProjectObject::new(
                "Manual",
                layer_id,
                Bounds::new(Point2D::new(30.0, 0.0), Point2D::new(50.0, 5.0)),
                ObjectData::Text {
                    content: "SN-9".to_string(),
                    font_family: "Arial".to_string(),
                    font_size_mm: 5.0,
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
                    resolved_path_data: None,
                    missing_font: false,
                    missing_glyphs: Vec::new(),
                    guide_path_id: None,
                    variable_text: Some(VariableTextConfig {
                        template: "SN-{Serial}".to_string(),
                        mode: Some(beambench_core::variable_text::VariableTextMode::SerialNumber),
                        offset: None,
                        source: beambench_core::variable_text::VariableTextSource {
                            current: 9,
                            start: 1,
                            end: 20,
                            advance_by: 1,
                            auto_advance: false,
                            ..Default::default()
                        },
                    }),
                },
            ))
            .id;

        *ctx.project.lock().unwrap() = Some(project);

        let updated = advance_auto_variable_text(&ctx).unwrap();
        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].id, auto_id);

        let stored = ctx.project.lock().unwrap().clone().unwrap();
        let auto = stored.find_object(auto_id).unwrap();
        let manual = stored.find_object(manual_id).unwrap();
        if let ObjectData::Text {
            variable_text: Some(config),
            ..
        } = &auto.data
        {
            assert_eq!(config.source.current, 3);
        } else {
            panic!("expected auto text");
        }
        if let ObjectData::Text {
            variable_text: Some(config),
            ..
        } = &manual.data
        {
            assert_eq!(config.source.current, 9);
        } else {
            panic!("expected manual text");
        }
        assert!(ctx.undo_state().unwrap().can_undo);

        undo_project(&ctx).unwrap();
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn advance_auto_variable_text_is_noop_when_no_objects_opt_in() {
        let ctx = ServiceContext::new();
        let (project, _) = sample_text_project();
        *ctx.project.lock().unwrap() = Some(project);

        let updated = advance_auto_variable_text(&ctx).unwrap();
        assert!(updated.is_empty());
        assert!(!ctx.undo_state().unwrap().can_undo);
    }
}
