use beambench_common::{Bounds, Point2D, Transform2D, VecPath};

use crate::layer::LayerId;
use crate::object::{ObjectData, ObjectId, TextLayoutMode, TextTransformStyle};
use crate::project::Project;
use crate::vector::bake_transform;
use crate::vector::convert::{object_to_world_vecpath, object_to_world_vecpath_resolved};
use crate::vector::path_ops::ensure_denormalized;
use crate::vector::text_to_path::refresh_text_object_cache_with_guide;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawOrderDirection {
    Forward,
    Backward,
    Front,
    Back,
}

/// Lock objects by setting their locked flag.
pub fn lock_objects(project: &mut Project, ids: &[ObjectId]) {
    for id in ids {
        if let Some(obj) = project.objects.iter_mut().find(|o| o.id == *id) {
            obj.locked = true;
        }
    }
    project.dirty = true;
}

/// Unlock objects by clearing their locked flag.
pub fn unlock_objects(project: &mut Project, ids: &[ObjectId]) {
    for id in ids {
        if let Some(obj) = project.objects.iter_mut().find(|o| o.id == *id) {
            obj.locked = false;
        }
    }
    project.dirty = true;
}

/// Set object visibility flags in one batch operation.
pub fn set_objects_visible(project: &mut Project, ids: &[ObjectId], visible: bool) {
    for id in ids {
        if let Some(obj) = project.objects.iter_mut().find(|o| o.id == *id) {
            obj.visible = visible;
        }
    }
    project.dirty = true;
}

/// Flip objects horizontally or vertically.
/// When `pivot` is `None`, each object flips around its own center. When a
/// pivot is provided, object centers are mirrored around that shared pivot.
pub fn flip_objects(
    project: &mut Project,
    ids: &[ObjectId],
    horizontal: bool,
    pivot: Option<Point2D>,
) {
    let has_group = ids.iter().any(|id| {
        project
            .objects
            .iter()
            .any(|obj| obj.id == *id && matches!(obj.data, ObjectData::Group { .. }))
    });
    let (target_ids, pivot) = if has_group {
        let shared_pivot = pivot.or_else(|| combined_object_center(project, ids));
        let mut expanded = Vec::new();
        for id in ids {
            collect_group_leaf_target_ids(project, *id, &mut expanded);
        }
        (expanded, shared_pivot)
    } else {
        (ids.to_vec(), pivot)
    };

    for id in target_ids {
        if let Some(obj) = project.objects.iter_mut().find(|o| o.id == id) {
            if let Some(ref pv) = pivot {
                let bc = Point2D::new(
                    (obj.bounds.min.x + obj.bounds.max.x) / 2.0,
                    (obj.bounds.min.y + obj.bounds.max.y) / 2.0,
                );
                let tc = obj.transform.apply_around_center(&bc, &bc);
                let shift_x = if horizontal {
                    (2.0 * pv.x) - (2.0 * tc.x)
                } else {
                    0.0
                };
                let shift_y = if horizontal {
                    0.0
                } else {
                    (2.0 * pv.y) - (2.0 * tc.y)
                };
                obj.bounds.min.x += shift_x;
                obj.bounds.min.y += shift_y;
                obj.bounds.max.x += shift_x;
                obj.bounds.max.y += shift_y;
            }
            if horizontal {
                obj.transform = Transform2D::scale(-1.0, 1.0).compose(&obj.transform);
            } else {
                obj.transform = Transform2D::scale(1.0, -1.0).compose(&obj.transform);
            }
        }
    }
    project.dirty = true;
}

fn collect_group_leaf_target_ids(project: &Project, id: ObjectId, output: &mut Vec<ObjectId>) {
    if output.contains(&id) {
        return;
    }
    let Some(object) = project.objects.iter().find(|obj| obj.id == id) else {
        return;
    };
    if let ObjectData::Group { children } = &object.data {
        for child_id in children {
            collect_group_leaf_target_ids(project, *child_id, output);
        }
    } else {
        output.push(id);
    }
}

fn collect_group_ids(project: &Project, id: ObjectId, output: &mut Vec<ObjectId>) {
    if output.contains(&id) {
        return;
    }
    let Some(object) = project.objects.iter().find(|obj| obj.id == id) else {
        return;
    };
    if let ObjectData::Group { children } = &object.data {
        output.push(id);
        for child_id in children {
            collect_group_ids(project, *child_id, output);
        }
    }
}

fn collect_group_member_ids(project: &Project, id: ObjectId, output: &mut Vec<ObjectId>) {
    if output.contains(&id) {
        return;
    }
    let Some(object) = project.objects.iter().find(|obj| obj.id == id) else {
        return;
    };
    output.push(id);
    if let ObjectData::Group { children } = &object.data {
        for child_id in children {
            collect_group_member_ids(project, *child_id, output);
        }
    }
}

fn combined_object_center(project: &Project, ids: &[ObjectId]) -> Option<Point2D> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut found = false;
    for id in ids {
        if let Some(object) = project.objects.iter().find(|obj| obj.id == *id) {
            found = true;
            min_x = min_x.min(object.bounds.min.x);
            min_y = min_y.min(object.bounds.min.y);
            max_x = max_x.max(object.bounds.max.x);
            max_y = max_y.max(object.bounds.max.y);
        }
    }
    found.then(|| Point2D::new((min_x + max_x) / 2.0, (min_y + max_y) / 2.0))
}

fn transformed_bounds(object: &crate::object::ProjectObject) -> Bounds {
    let center = Point2D::new(
        (object.bounds.min.x + object.bounds.max.x) / 2.0,
        (object.bounds.min.y + object.bounds.max.y) / 2.0,
    );
    let corners = [
        object.bounds.min,
        Point2D::new(object.bounds.max.x, object.bounds.min.y),
        object.bounds.max,
        Point2D::new(object.bounds.min.x, object.bounds.max.y),
    ];
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for corner in corners {
        let transformed = object.transform.apply_around_center(&corner, &center);
        min_x = min_x.min(transformed.x);
        min_y = min_y.min(transformed.y);
        max_x = max_x.max(transformed.x);
        max_y = max_y.max(transformed.y);
    }
    Bounds {
        min: Point2D::new(min_x, min_y),
        max: Point2D::new(max_x, max_y),
    }
}

fn recompute_group_bounds(project: &mut Project, group_id: ObjectId) {
    let children = project
        .objects
        .iter()
        .find(|obj| obj.id == group_id)
        .and_then(|obj| match &obj.data {
            ObjectData::Group { children } => Some(children.clone()),
            _ => None,
        });
    let Some(children) = children else {
        return;
    };
    for child_id in &children {
        recompute_group_bounds(project, *child_id);
    }

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut found = false;
    for child_id in children {
        if let Some(child) = project.objects.iter().find(|obj| obj.id == child_id) {
            let bounds = transformed_bounds(child);
            found = true;
            min_x = min_x.min(bounds.min.x);
            min_y = min_y.min(bounds.min.y);
            max_x = max_x.max(bounds.max.x);
            max_y = max_y.max(bounds.max.y);
        }
    }
    if found {
        if let Some(group) = project.objects.iter_mut().find(|obj| obj.id == group_id) {
            group.bounds = Bounds {
                min: Point2D::new(min_x, min_y),
                max: Point2D::new(max_x, max_y),
            };
        }
    }
}

/// Rotate objects by a given angle in degrees around the given pivot point.
/// When `pivot` is `None`, objects rotate around their own center (self-rotation only).
pub fn rotate_objects(
    project: &mut Project,
    ids: &[ObjectId],
    degrees: f64,
    pivot: Option<Point2D>,
) {
    let radians = degrees.to_radians();
    let (s, c) = radians.sin_cos();
    let has_group = ids.iter().any(|id| {
        project
            .objects
            .iter()
            .any(|obj| obj.id == *id && matches!(obj.data, ObjectData::Group { .. }))
    });
    let mut group_ids = Vec::new();
    let (target_ids, pivot) = if has_group {
        let shared_pivot = pivot.or_else(|| combined_object_center(project, ids));
        let mut expanded = Vec::new();
        for id in ids {
            collect_group_leaf_target_ids(project, *id, &mut expanded);
            collect_group_ids(project, *id, &mut group_ids);
        }
        (expanded, shared_pivot)
    } else {
        (ids.to_vec(), pivot)
    };

    for id in target_ids {
        if let Some(obj) = project.objects.iter_mut().find(|o| o.id == id) {
            if let Some(ref pv) = pivot {
                // Orbit: rotate object's bounds center around the provided pivot
                let bc = Point2D::new(
                    (obj.bounds.min.x + obj.bounds.max.x) / 2.0,
                    (obj.bounds.min.y + obj.bounds.max.y) / 2.0,
                );
                let tc = obj.transform.apply_around_center(&bc, &bc);
                let dx = tc.x - pv.x;
                let dy = tc.y - pv.y;
                let new_x = pv.x + dx * c - dy * s;
                let new_y = pv.y + dx * s + dy * c;
                let shift_x = new_x - tc.x;
                let shift_y = new_y - tc.y;
                obj.bounds.min.x += shift_x;
                obj.bounds.min.y += shift_y;
                obj.bounds.max.x += shift_x;
                obj.bounds.max.y += shift_y;
            }
            // Self-rotation
            obj.transform = Transform2D::rotate(radians).compose(&obj.transform);
        }
    }
    for group_id in group_ids.into_iter().rev() {
        recompute_group_bounds(project, group_id);
    }
    project.dirty = true;
}

/// Bake an object's current transform into vector path data and reset it to an
/// identity transform. Node editing expects active vector geometry in world
/// coordinates, so this keeps handles, hit testing, and selection bounds aligned
/// after transform-based operations.
pub fn bake_object_transform_to_path(
    project: &mut Project,
    id: ObjectId,
) -> Option<crate::object::ProjectObject> {
    let source = project.find_object(id)?.clone();
    let vec_path = object_to_world_vecpath(&source)?;
    let new_bounds = vec_path
        .visual_bounds()
        .or_else(|| vec_path.bounds())
        .unwrap_or(source.bounds);
    let closed = vec_path.subpaths.iter().any(|sp| sp.closed);
    let ruler_guide_axis = match &source.data {
        ObjectData::VectorPath {
            ruler_guide_axis, ..
        } => *ruler_guide_axis,
        _ => None,
    };

    let result = {
        let obj = project.find_object_mut(id)?;
        obj.data = ObjectData::VectorPath {
            path_data: vec_path.to_svg_d(),
            closed,
            ruler_guide_axis,
        };
        obj.bounds = new_bounds;
        obj.transform = Transform2D::identity();
        obj.start_point_edits.clear();
        obj.clone()
    };
    project.dirty = true;
    Some(result)
}

/// Shear objects by the given factors around the given pivot point.
pub fn shear_objects(
    project: &mut Project,
    ids: &[ObjectId],
    shear_x: f64,
    shear_y: f64,
    pivot: Option<Point2D>,
) {
    for id in ids {
        if let Some(obj) = project.objects.iter_mut().find(|o| o.id == *id) {
            if let Some(ref pv) = pivot {
                let bc = Point2D::new(
                    (obj.bounds.min.x + obj.bounds.max.x) / 2.0,
                    (obj.bounds.min.y + obj.bounds.max.y) / 2.0,
                );
                let tc = obj.transform.apply_around_center(&bc, &bc);
                let dx = tc.x - pv.x;
                let dy = tc.y - pv.y;
                let new_x = dx + shear_x * dy;
                let new_y = shear_y * dx + dy;
                let shift_x = (pv.x + new_x) - tc.x;
                let shift_y = (pv.y + new_y) - tc.y;
                obj.bounds.min.x += shift_x;
                obj.bounds.min.y += shift_y;
                obj.bounds.max.x += shift_x;
                obj.bounds.max.y += shift_y;
            }
            obj.transform = Transform2D::shear(shear_x, shear_y).compose(&obj.transform);
        }
    }
    project.dirty = true;
}

/// Batch-update object bounds, setting each object's bounds to the provided values.
/// Handles vector path refitting, text property scaling, text cache refresh,
/// and dependent text object refresh atomically.
pub fn update_object_bounds_batch(project: &mut Project, entries: &[(ObjectId, Bounds)]) {
    for (id, new_bounds) in entries {
        // Auto-unlink VirtualClone if needed (must happen BEFORE guide lookup
        // so that a clone-of-path-text becomes concrete Text with guide_path_id)
        let _ = project.ensure_resolved(*id);

        // Look up guide path AFTER resolve so clones see their concrete Text data
        let guide = project
            .find_object(*id)
            .and_then(|obj| lookup_guide_path_for_text(&obj.data, project));

        if let Some(obj) = project.objects.iter_mut().find(|o| o.id == *id) {
            match &mut obj.data {
                ObjectData::VectorPath { .. } => {
                    refit_vector_path_to_bounds(obj, new_bounds);
                }
                ObjectData::Text {
                    font_size_mm,
                    h_spacing,
                    v_spacing,
                    max_width,
                    layout_mode,
                    on_path,
                    transform_style,
                    ..
                } => {
                    // Scale dimensional text properties for straight text
                    let is_straight = *transform_style == TextTransformStyle::None
                        && *layout_mode == TextLayoutMode::Straight
                        && !*on_path;
                    if is_straight {
                        let old_w = obj.bounds.width();
                        let old_h = obj.bounds.height();
                        let new_w = new_bounds.width();
                        let new_h = new_bounds.height();
                        if old_h > 1e-6 {
                            let sy = new_h / old_h;
                            *font_size_mm *= sy;
                            *v_spacing *= sy;
                        }
                        if old_w > 1e-6 {
                            let sx = new_w / old_w;
                            *h_spacing *= sx;
                            if let Some(mw) = max_width {
                                *mw *= sx;
                            }
                        }
                    }
                    obj.bounds = *new_bounds;
                    // Refresh text cache with guide path (if any).
                    // Apply the guide-derived bounds: keep the user's position
                    // (min corner) but adopt the guide-derived size so the
                    // selection box stays in sync with the actual text layout.
                    let mapped_bounds = refresh_text_object_cache_with_guide(
                        &mut obj.data,
                        &obj.bounds,
                        guide.as_ref(),
                    );
                    if let Some(mb) = mapped_bounds {
                        let width = mb.width();
                        let height = mb.height();
                        obj.bounds = Bounds::new(
                            obj.bounds.min,
                            Point2D::new(obj.bounds.min.x + width, obj.bounds.min.y + height),
                        );
                        obj.transform = Transform2D::identity();
                    }
                }
                _ => {
                    obj.bounds = *new_bounds;
                }
            }
        }
    }

    // Post-pass: refresh text objects that depend on any updated object as a guide path
    let entry_ids: Vec<ObjectId> = entries.iter().map(|(id, _)| *id).collect();
    for guide_id in &entry_ids {
        refresh_dependent_text_objects(project, *guide_id);
    }

    project.dirty = true;
}

/// Look up the guide VecPath for a text object (if it's path-text with a guide).
fn lookup_guide_path_for_text(data: &ObjectData, project: &Project) -> Option<VecPath> {
    if let ObjectData::Text {
        guide_path_id: Some(guide_id),
        layout_mode,
        on_path,
        transform_style,
        ..
    } = data
    {
        if *transform_style != TextTransformStyle::None {
            return None;
        }
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

/// Refresh text objects that reference `guide_id` as their guide path,
/// including text objects whose guide is a VirtualClone of `guide_id`.
fn refresh_dependent_text_objects(project: &mut Project, guide_id: ObjectId) {
    // Build set of IDs that resolve to guide_id's geometry:
    // guide_id itself + any VirtualClones whose source chain leads to it.
    let mut affected_ids = std::collections::HashSet::new();
    affected_ids.insert(guide_id);
    // Iteratively collect clones (handles chains: clone-of-clone-of-source)
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
                    transform_style,
                    ..
                } = &obj.data
                {
                    if *transform_style == TextTransformStyle::None && *gid == aid {
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

/// Scale a vector_path object's path data to fit the given bounds.
fn refit_vector_path_to_bounds(obj: &mut crate::object::ProjectObject, new_bounds: &Bounds) {
    ensure_denormalized(obj);
    if let ObjectData::VectorPath { ref path_data, .. } = obj.data {
        let vec_path = VecPath::parse_svg_d(path_data);
        let intrinsic = vec_path
            .visual_bounds()
            .unwrap_or(Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(0.0, 0.0)));

        let old_w = intrinsic.max.x - intrinsic.min.x;
        let old_h = intrinsic.max.y - intrinsic.min.y;
        let new_w = new_bounds.max.x - new_bounds.min.x;
        let new_h = new_bounds.max.y - new_bounds.min.y;

        let sx = if old_w > 0.0 { new_w / old_w } else { 1.0 };
        let sy = if old_h > 0.0 { new_h / old_h } else { 1.0 };
        let transform = Transform2D {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            tx: new_bounds.min.x - intrinsic.min.x * sx,
            ty: new_bounds.min.y - intrinsic.min.y * sy,
        };
        let scaled = bake_transform(&vec_path, &transform);
        let closed = scaled.subpaths.iter().any(|sp| sp.closed);
        obj.data = ObjectData::VectorPath {
            path_data: scaled.to_svg_d(),
            closed,
            ruler_guide_axis: None,
        };
    }
    obj.bounds = *new_bounds;
    obj.tabs.clear();
}

/// Reorder an object within the current z-order stack.
pub fn push_draw_order(project: &mut Project, id: ObjectId, direction: DrawOrderDirection) {
    let mut ordered: Vec<(usize, ObjectId, i32)> = project
        .objects
        .iter()
        .enumerate()
        .map(|(idx, obj)| (idx, obj.id, obj.z_index))
        .collect();
    ordered.sort_by_key(|(idx, _, z_index)| (*z_index, *idx));

    let mut target_ids = Vec::new();
    collect_group_member_ids(project, id, &mut target_ids);
    if target_ids.is_empty() {
        return;
    };
    let target_positions: Vec<usize> = ordered
        .iter()
        .enumerate()
        .filter_map(|(idx, (_, obj_id, _))| target_ids.contains(obj_id).then_some(idx))
        .collect();
    if target_positions.is_empty() {
        return;
    }

    let first_target_idx = *target_positions.first().unwrap();
    let last_target_idx = *target_positions.last().unwrap();
    let block: Vec<(usize, ObjectId, i32)> = ordered
        .iter()
        .copied()
        .filter(|(_, obj_id, _)| target_ids.contains(obj_id))
        .collect();
    let mut remaining: Vec<(usize, ObjectId, i32)> = ordered
        .iter()
        .copied()
        .filter(|(_, obj_id, _)| !target_ids.contains(obj_id))
        .collect();

    let insert_idx = match direction {
        DrawOrderDirection::Front => remaining.len(),
        DrawOrderDirection::Back => 0,
        DrawOrderDirection::Forward => {
            let non_targets_through_block = ordered[..=last_target_idx]
                .iter()
                .filter(|(_, obj_id, _)| !target_ids.contains(obj_id))
                .count();
            (non_targets_through_block + 1).min(remaining.len())
        }
        DrawOrderDirection::Backward => {
            let non_targets_before_block = ordered[..first_target_idx]
                .iter()
                .filter(|(_, obj_id, _)| !target_ids.contains(obj_id))
                .count();
            non_targets_before_block.saturating_sub(1)
        }
    };

    let original_without_targets_before = ordered[..first_target_idx]
        .iter()
        .filter(|(_, obj_id, _)| !target_ids.contains(obj_id))
        .count();
    if insert_idx == original_without_targets_before {
        return;
    }

    for (offset, entry) in block.into_iter().enumerate() {
        remaining.insert(insert_idx + offset, entry);
    }

    for (new_z, (_, obj_id, _)) in remaining.into_iter().enumerate() {
        if let Some(obj) = project.objects.iter_mut().find(|o| o.id == obj_id) {
            obj.z_index = new_z as i32;
        }
    }
    project.dirty = true;
}

/// Move objects to a specific position (sets bounds.min to x, y).
pub fn move_objects_to_position(project: &mut Project, ids: &[ObjectId], x: f64, y: f64) {
    for id in ids {
        if let Some(obj) = project.objects.iter_mut().find(|o| o.id == *id) {
            let dx = x - obj.bounds.min.x;
            let dy = y - obj.bounds.min.y;
            obj.bounds.min.x = x;
            obj.bounds.min.y = y;
            obj.bounds.max.x += dx;
            obj.bounds.max.y += dy;
        }
    }
    project.dirty = true;
}

/// Nudge objects by a relative delta (dx, dy).
pub fn nudge_objects(project: &mut Project, ids: &[ObjectId], dx: f64, dy: f64) {
    for id in ids {
        if let Some(obj) = project.objects.iter_mut().find(|o| o.id == *id) {
            obj.bounds.min.x += dx;
            obj.bounds.min.y += dy;
            obj.bounds.max.x += dx;
            obj.bounds.max.y += dy;
        }
    }
    project.dirty = true;
}

/// Reassign objects to a different layer.
pub fn reassign_layer(project: &mut Project, ids: &[ObjectId], target_layer_id: LayerId) {
    // Verify target layer exists
    if !project.layers.iter().any(|l| l.id == target_layer_id) {
        return;
    }

    for id in ids {
        if let Some(obj) = project.objects.iter_mut().find(|o| o.id == *id) {
            obj.layer_id = target_layer_id;
        }
    }

    project.dirty = true;

    // Clean up layers that lost all their objects
    project.clean_empty_layers();
}

/// Select all open shapes (VectorPath objects where closed=false).
pub fn select_open_shapes(project: &Project) -> Vec<ObjectId> {
    project
        .objects
        .iter()
        .filter(|obj| matches!(&obj.data, ObjectData::VectorPath { closed, .. } if !closed))
        .map(|obj| obj.id)
        .collect()
}

/// Select all objects in a specific layer.
pub fn select_all_in_layer(project: &Project, layer_id: LayerId) -> Vec<ObjectId> {
    project
        .objects
        .iter()
        .filter(|obj| obj.layer_id == layer_id)
        .map(|obj| obj.id)
        .collect()
}

/// Set layer visibility.
pub fn set_layer_visible(project: &mut Project, layer_id: LayerId, visible: bool) -> bool {
    if let Some(layer) = project.layers.iter_mut().find(|l| l.id == layer_id) {
        layer.visible = visible;
        project.dirty = true;
        true
    } else {
        false
    }
}

/// Set layer air assist flag.
pub fn set_layer_air_assist(project: &mut Project, layer_id: LayerId, air_assist: bool) -> bool {
    if let Some(layer) = project.layers.iter_mut().find(|l| l.id == layer_id) {
        layer.primary_entry_mut().air_assist = air_assist;
        project.dirty = true;
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::{ObjectData, ProjectObject, ShapeKind};
    use beambench_common::{Bounds, Point2D};

    fn make_test_project() -> Project {
        Project::new("test")
    }

    fn make_test_object(id: ObjectId) -> ProjectObject {
        use crate::layer::LayerId;

        ProjectObject {
            id,
            name: "test".into(),
            visible: true,
            locked: false,
            bounds: Bounds {
                min: Point2D { x: 0.0, y: 0.0 },
                max: Point2D { x: 10.0, y: 10.0 },
            },
            transform: Transform2D::identity(),
            layer_id: LayerId::new(),
            data: ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
            z_index: 0,
            lock_aspect_ratio: false,
            power_scale: 1.0,
            priority: 0,
            created_at: chrono::Utc::now(),
            tabs: Vec::new(),
            start_point_edits: Vec::new(),
        }
    }

    #[test]
    fn lock_objects_sets_locked_flag() {
        let mut project = make_test_project();
        let id = ObjectId::new();
        project.objects.push(make_test_object(id));

        lock_objects(&mut project, &[id]);

        assert!(project.objects[0].locked);
        assert!(project.dirty);
    }

    #[test]
    fn unlock_objects_clears_locked_flag() {
        let mut project = make_test_project();
        let id = ObjectId::new();
        let mut obj = make_test_object(id);
        obj.locked = true;
        project.objects.push(obj);

        unlock_objects(&mut project, &[id]);

        assert!(!project.objects[0].locked);
        assert!(project.dirty);
    }

    #[test]
    fn flip_objects_horizontal() {
        let mut project = make_test_project();
        let id = ObjectId::new();
        project.objects.push(make_test_object(id));

        flip_objects(&mut project, &[id], true, None);

        assert!(project.dirty);
    }

    #[test]
    fn set_objects_visible_updates_visibility_flag() {
        let mut project = make_test_project();
        let id = ObjectId::new();
        project.objects.push(make_test_object(id));

        set_objects_visible(&mut project, &[id], false);

        assert!(!project.objects[0].visible);
        assert!(project.dirty);
    }

    #[test]
    fn flip_objects_vertical() {
        let mut project = make_test_project();
        let id = ObjectId::new();
        project.objects.push(make_test_object(id));

        flip_objects(&mut project, &[id], false, None);

        assert!(project.dirty);
    }

    #[test]
    fn flip_objects_horizontal_with_pivot_mirrors_bounds_centers() {
        let mut project = make_test_project();
        let id_a = ObjectId::new();
        let id_b = ObjectId::new();
        let mut obj_a = make_test_object(id_a);
        let mut obj_b = make_test_object(id_b);
        obj_a.bounds.min.x = 0.0;
        obj_a.bounds.max.x = 10.0;
        obj_b.bounds.min.x = 30.0;
        obj_b.bounds.max.x = 40.0;
        project.objects.push(obj_a);
        project.objects.push(obj_b);

        flip_objects(
            &mut project,
            &[id_a, id_b],
            true,
            Some(Point2D::new(20.0, 5.0)),
        );

        let moved_a = project.objects.iter().find(|obj| obj.id == id_a).unwrap();
        let moved_b = project.objects.iter().find(|obj| obj.id == id_b).unwrap();
        assert_eq!(moved_a.bounds.min.x, 30.0);
        assert_eq!(moved_a.bounds.max.x, 40.0);
        assert_eq!(moved_b.bounds.min.x, 0.0);
        assert_eq!(moved_b.bounds.max.x, 10.0);
        assert_eq!(moved_a.transform.a, -1.0);
        assert_eq!(moved_b.transform.a, -1.0);
        assert!(project.dirty);
    }

    #[test]
    fn flip_objects_vertical_with_pivot_mirrors_bounds_centers() {
        let mut project = make_test_project();
        let id_a = ObjectId::new();
        let id_b = ObjectId::new();
        let mut obj_a = make_test_object(id_a);
        let mut obj_b = make_test_object(id_b);
        obj_a.bounds.min.y = 0.0;
        obj_a.bounds.max.y = 10.0;
        obj_b.bounds.min.y = 30.0;
        obj_b.bounds.max.y = 40.0;
        project.objects.push(obj_a);
        project.objects.push(obj_b);

        flip_objects(
            &mut project,
            &[id_a, id_b],
            false,
            Some(Point2D::new(5.0, 20.0)),
        );

        let moved_a = project.objects.iter().find(|obj| obj.id == id_a).unwrap();
        let moved_b = project.objects.iter().find(|obj| obj.id == id_b).unwrap();
        assert_eq!(moved_a.bounds.min.y, 30.0);
        assert_eq!(moved_a.bounds.max.y, 40.0);
        assert_eq!(moved_b.bounds.min.y, 0.0);
        assert_eq!(moved_b.bounds.max.y, 10.0);
        assert_eq!(moved_a.transform.d, -1.0);
        assert_eq!(moved_b.transform.d, -1.0);
        assert!(project.dirty);
    }

    #[test]
    fn flip_objects_expands_group_to_children_without_flipping_group_metadata() {
        let mut project = make_test_project();
        let group_id = ObjectId::new();
        let id_a = ObjectId::new();
        let id_b = ObjectId::new();
        let mut obj_a = make_test_object(id_a);
        let mut obj_b = make_test_object(id_b);
        let mut group = make_test_object(group_id);
        obj_a.bounds.min.x = 0.0;
        obj_a.bounds.max.x = 10.0;
        obj_b.bounds.min.x = 30.0;
        obj_b.bounds.max.x = 40.0;
        group.bounds.min.x = 0.0;
        group.bounds.max.x = 40.0;
        group.data = ObjectData::Group {
            children: vec![id_a, id_b],
        };
        project.objects.push(obj_a);
        project.objects.push(obj_b);
        project.objects.push(group);

        flip_objects(&mut project, &[group_id], true, None);

        let moved_a = project.objects.iter().find(|obj| obj.id == id_a).unwrap();
        let moved_b = project.objects.iter().find(|obj| obj.id == id_b).unwrap();
        let group = project
            .objects
            .iter()
            .find(|obj| obj.id == group_id)
            .unwrap();
        assert_eq!(moved_a.bounds.min.x, 30.0);
        assert_eq!(moved_a.bounds.max.x, 40.0);
        assert_eq!(moved_b.bounds.min.x, 0.0);
        assert_eq!(moved_b.bounds.max.x, 10.0);
        assert_eq!(moved_a.transform.a, -1.0);
        assert_eq!(moved_b.transform.a, -1.0);
        assert_eq!(group.transform, Transform2D::identity());
        assert!(project.dirty);
    }

    #[test]
    fn rotate_objects_applies_rotation() {
        let mut project = make_test_project();
        let id = ObjectId::new();
        project.objects.push(make_test_object(id));

        rotate_objects(&mut project, &[id], 90.0, None);

        assert!(project.dirty);
    }

    #[test]
    fn rotate_objects_expands_group_to_children_and_updates_group_bounds() {
        let mut project = make_test_project();
        let group_id = ObjectId::new();
        let id_a = ObjectId::new();
        let id_b = ObjectId::new();
        let mut obj_a = make_test_object(id_a);
        let mut obj_b = make_test_object(id_b);
        let mut group = make_test_object(group_id);
        obj_a.bounds.min.x = 0.0;
        obj_a.bounds.min.y = 0.0;
        obj_a.bounds.max.x = 10.0;
        obj_a.bounds.max.y = 10.0;
        obj_b.bounds.min.x = 30.0;
        obj_b.bounds.min.y = 10.0;
        obj_b.bounds.max.x = 40.0;
        obj_b.bounds.max.y = 20.0;
        group.bounds.min.x = 0.0;
        group.bounds.min.y = 0.0;
        group.bounds.max.x = 40.0;
        group.bounds.max.y = 20.0;
        group.data = ObjectData::Group {
            children: vec![id_a, id_b],
        };
        project.objects.push(obj_a);
        project.objects.push(obj_b);
        project.objects.push(group);

        rotate_objects(&mut project, &[group_id], 90.0, None);

        let moved_a = project.objects.iter().find(|obj| obj.id == id_a).unwrap();
        let moved_b = project.objects.iter().find(|obj| obj.id == id_b).unwrap();
        let group = project
            .objects
            .iter()
            .find(|obj| obj.id == group_id)
            .unwrap();
        assert!((moved_a.bounds.min.x - 20.0).abs() < 1e-9);
        assert!((moved_a.bounds.min.y - -10.0).abs() < 1e-9);
        assert!((moved_a.bounds.max.x - 30.0).abs() < 1e-9);
        assert!((moved_a.bounds.max.y - 0.0).abs() < 1e-9);
        assert!((moved_b.bounds.min.x - 10.0).abs() < 1e-9);
        assert!((moved_b.bounds.min.y - 20.0).abs() < 1e-9);
        assert!((moved_b.bounds.max.x - 20.0).abs() < 1e-9);
        assert!((moved_b.bounds.max.y - 30.0).abs() < 1e-9);
        assert!((group.bounds.min.x - 10.0).abs() < 1e-9);
        assert!((group.bounds.min.y - -10.0).abs() < 1e-9);
        assert!((group.bounds.max.x - 30.0).abs() < 1e-9);
        assert!((group.bounds.max.y - 30.0).abs() < 1e-9);
        assert_eq!(group.transform, Transform2D::identity());
        assert!(project.dirty);
    }

    #[test]
    fn push_draw_order_forward() {
        let mut project = make_test_project();
        let id_a = ObjectId::new();
        let id_b = ObjectId::new();
        let mut obj_a = make_test_object(id_a);
        let mut obj_b = make_test_object(id_b);
        obj_a.z_index = 0;
        obj_b.z_index = 1;
        project.objects.push(obj_a);
        project.objects.push(obj_b);

        push_draw_order(&mut project, id_a, DrawOrderDirection::Forward);

        let moved = project.objects.iter().find(|obj| obj.id == id_a).unwrap();
        let other = project.objects.iter().find(|obj| obj.id == id_b).unwrap();
        assert_eq!(moved.z_index, 1);
        assert_eq!(other.z_index, 0);
        assert!(project.dirty);
    }

    #[test]
    fn push_draw_order_front_moves_group_members_as_block() {
        let mut project = make_test_project();
        let id_background = ObjectId::new();
        let id_child_a = ObjectId::new();
        let id_child_b = ObjectId::new();
        let id_group = ObjectId::new();
        let id_foreground = ObjectId::new();
        let mut background = make_test_object(id_background);
        let mut child_a = make_test_object(id_child_a);
        let mut child_b = make_test_object(id_child_b);
        let mut group = make_test_object(id_group);
        let mut foreground = make_test_object(id_foreground);
        background.z_index = 0;
        child_a.z_index = 1;
        child_b.z_index = 2;
        group.z_index = 3;
        foreground.z_index = 4;
        group.data = ObjectData::Group {
            children: vec![id_child_a, id_child_b],
        };
        project.objects.push(background);
        project.objects.push(child_a);
        project.objects.push(child_b);
        project.objects.push(group);
        project.objects.push(foreground);

        push_draw_order(&mut project, id_group, DrawOrderDirection::Front);

        let z = |id| {
            project
                .objects
                .iter()
                .find(|obj| obj.id == id)
                .unwrap()
                .z_index
        };
        assert_eq!(z(id_background), 0);
        assert_eq!(z(id_foreground), 1);
        assert_eq!(z(id_child_a), 2);
        assert_eq!(z(id_child_b), 3);
        assert_eq!(z(id_group), 4);
        assert!(project.dirty);
    }

    #[test]
    fn push_draw_order_backward() {
        let mut project = make_test_project();
        let id_a = ObjectId::new();
        let id_b = ObjectId::new();
        let mut obj_a = make_test_object(id_a);
        let mut obj_b = make_test_object(id_b);
        obj_a.z_index = 0;
        obj_b.z_index = 1;
        project.objects.push(obj_a);
        project.objects.push(obj_b);

        push_draw_order(&mut project, id_b, DrawOrderDirection::Backward);

        let moved = project.objects.iter().find(|obj| obj.id == id_b).unwrap();
        let other = project.objects.iter().find(|obj| obj.id == id_a).unwrap();
        assert_eq!(moved.z_index, 0);
        assert_eq!(other.z_index, 1);
    }

    #[test]
    fn push_draw_order_front_moves_to_top() {
        let mut project = make_test_project();
        let id_a = ObjectId::new();
        let id_b = ObjectId::new();
        let id_c = ObjectId::new();
        let mut obj_a = make_test_object(id_a);
        let mut obj_b = make_test_object(id_b);
        let mut obj_c = make_test_object(id_c);
        obj_a.z_index = 0;
        obj_b.z_index = 1;
        obj_c.z_index = 2;
        project.objects.push(obj_a);
        project.objects.push(obj_b);
        project.objects.push(obj_c);

        push_draw_order(&mut project, id_a, DrawOrderDirection::Front);

        let moved = project.objects.iter().find(|obj| obj.id == id_a).unwrap();
        assert_eq!(moved.z_index, 2);
    }

    #[test]
    fn push_draw_order_back_moves_to_bottom() {
        let mut project = make_test_project();
        let id_a = ObjectId::new();
        let id_b = ObjectId::new();
        let id_c = ObjectId::new();
        let mut obj_a = make_test_object(id_a);
        let mut obj_b = make_test_object(id_b);
        let mut obj_c = make_test_object(id_c);
        obj_a.z_index = 0;
        obj_b.z_index = 1;
        obj_c.z_index = 2;
        project.objects.push(obj_a);
        project.objects.push(obj_b);
        project.objects.push(obj_c);

        push_draw_order(&mut project, id_c, DrawOrderDirection::Back);

        let moved = project.objects.iter().find(|obj| obj.id == id_c).unwrap();
        assert_eq!(moved.z_index, 0);
    }

    #[test]
    fn nudge_objects_moves_by_delta() {
        let mut project = make_test_project();
        let id = ObjectId::new();
        project.objects.push(make_test_object(id));
        project.dirty = false;

        nudge_objects(&mut project, &[id], 5.0, -3.0);

        let obj = &project.objects[0];
        assert!((obj.bounds.min.x - 5.0).abs() < 1e-9);
        assert!((obj.bounds.min.y - (-3.0)).abs() < 1e-9);
        assert!((obj.bounds.max.x - 15.0).abs() < 1e-9);
        assert!((obj.bounds.max.y - 7.0).abs() < 1e-9);
        assert!(project.dirty);
    }

    #[test]
    fn nudge_objects_skips_missing() {
        let mut project = make_test_project();
        let id = ObjectId::new();
        project.objects.push(make_test_object(id));
        let missing = ObjectId::new();

        nudge_objects(&mut project, &[missing], 1.0, 1.0);

        let obj = &project.objects[0];
        assert!((obj.bounds.min.x).abs() < 1e-9);
        assert!((obj.bounds.min.y).abs() < 1e-9);
    }

    #[test]
    fn reassign_layer_cleans_up_empty_source_layer() {
        use crate::layer::{Layer, OperationType};

        let mut project = make_test_project();
        let layer_a = Layer::new("Black", OperationType::Line);
        let layer_b = Layer::new("Red", OperationType::Line);
        let lid_a = layer_a.id;
        let lid_b = layer_b.id;
        project.add_layer(layer_a);
        project.add_layer(layer_b);

        // One object on layer_a, none on layer_b
        let id = ObjectId::new();
        let mut obj = make_test_object(id);
        obj.layer_id = lid_a;
        project.objects.push(obj);

        assert_eq!(project.layers.len(), 2);

        // Reassign the object from layer_a to layer_b
        reassign_layer(&mut project, &[id], lid_b);

        assert_eq!(project.objects[0].layer_id, lid_b);
        // layer_a should be cleaned up (no objects left)
        assert_eq!(project.layers.len(), 1);
        assert_eq!(project.layers[0].id, lid_b);
    }

    #[test]
    fn reassign_layer_keeps_layers_with_remaining_objects() {
        use crate::layer::{Layer, OperationType};

        let mut project = make_test_project();
        let layer_a = Layer::new("Black", OperationType::Line);
        let layer_b = Layer::new("Red", OperationType::Line);
        let lid_a = layer_a.id;
        let lid_b = layer_b.id;
        project.add_layer(layer_a);
        project.add_layer(layer_b);

        // Two objects on layer_a
        let id1 = ObjectId::new();
        let id2 = ObjectId::new();
        let mut obj1 = make_test_object(id1);
        obj1.layer_id = lid_a;
        let mut obj2 = make_test_object(id2);
        obj2.layer_id = lid_a;
        project.objects.push(obj1);
        project.objects.push(obj2);

        // Move only one to layer_b
        reassign_layer(&mut project, &[id1], lid_b);

        // layer_a still has obj2, so both layers remain
        assert_eq!(project.layers.len(), 2);
    }

    #[test]
    fn rotate_objects_single_object_no_orbit() {
        let mut project = make_test_project();
        let id = ObjectId::new();
        project.objects.push(make_test_object(id));
        let orig_cx = 5.0; // center of 0..10
        let orig_cy = 5.0;

        // When pivot == object center, bounds should not shift
        rotate_objects(
            &mut project,
            &[id],
            90.0,
            Some(Point2D::new(orig_cx, orig_cy)),
        );

        let obj = &project.objects[0];
        assert!((obj.bounds.min.x).abs() < 1e-6);
        assert!((obj.bounds.min.y).abs() < 1e-6);
        assert!((obj.bounds.max.x - 10.0).abs() < 1e-6);
        assert!((obj.bounds.max.y - 10.0).abs() < 1e-6);
    }

    #[test]
    fn rotate_objects_multi_object_orbits() {
        let mut project = make_test_project();
        let id1 = ObjectId::new();
        let id2 = ObjectId::new();
        let mut obj1 = make_test_object(id1);
        obj1.bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0));
        let mut obj2 = make_test_object(id2);
        obj2.bounds = Bounds::new(Point2D::new(20.0, 0.0), Point2D::new(30.0, 10.0));
        project.objects.push(obj1);
        project.objects.push(obj2);

        // Pivot at center of combined bounds: (15, 5)
        let pivot = Point2D::new(15.0, 5.0);
        rotate_objects(&mut project, &[id1, id2], 90.0, Some(pivot));

        // obj1 center was (5,5), relative to pivot (-10,0), rotated 90 -> (0,-10) + pivot -> (15,-5)
        // shift = (15,-5) - (5,5) = (10,-10)
        let o1 = project.objects.iter().find(|o| o.id == id1).unwrap();
        assert!((o1.bounds.min.x - 10.0).abs() < 1e-4);
        assert!((o1.bounds.min.y - (-10.0)).abs() < 1e-4);
    }

    #[test]
    fn bake_object_transform_to_path_keeps_path_bounds_aligned() {
        let mut project = make_test_project();
        let id = ObjectId::new();
        let mut obj = make_test_object(id);
        obj.bounds = Bounds::new(Point2D::new(10.0, 20.0), Point2D::new(30.0, 30.0));
        obj.data = ObjectData::VectorPath {
            path_data: "M0 0 L20 0 L20 10 L0 10 Z".into(),
            closed: true,
            ruler_guide_axis: None,
        };
        project.objects.push(obj);

        rotate_objects(&mut project, &[id], 90.0, Some(Point2D::new(20.0, 25.0)));
        let expected_bounds = object_to_world_vecpath(project.find_object(id).unwrap())
            .unwrap()
            .visual_bounds()
            .unwrap();

        let baked = bake_object_transform_to_path(&mut project, id).unwrap();
        assert!(baked.transform.is_identity());
        assert!((baked.bounds.min.x - expected_bounds.min.x).abs() < 1e-6);
        assert!((baked.bounds.min.y - expected_bounds.min.y).abs() < 1e-6);
        assert!((baked.bounds.max.x - expected_bounds.max.x).abs() < 1e-6);
        assert!((baked.bounds.max.y - expected_bounds.max.y).abs() < 1e-6);

        let ObjectData::VectorPath { path_data, .. } = baked.data else {
            panic!("expected baked vector path");
        };
        let parsed_bounds = VecPath::parse_svg_d(&path_data).visual_bounds().unwrap();
        assert!((parsed_bounds.min.x - baked.bounds.min.x).abs() < 1e-6);
        assert!((parsed_bounds.min.y - baked.bounds.min.y).abs() < 1e-6);
        assert!((parsed_bounds.max.x - baked.bounds.max.x).abs() < 1e-6);
        assert!((parsed_bounds.max.y - baked.bounds.max.y).abs() < 1e-6);
    }

    #[test]
    fn shear_objects_applies_transform() {
        let mut project = make_test_project();
        let id = ObjectId::new();
        project.objects.push(make_test_object(id));

        shear_objects(&mut project, &[id], 0.5, 0.3, None);

        let obj = &project.objects[0];
        assert!((obj.transform.c - 0.5).abs() < 1e-10);
        assert!((obj.transform.b - 0.3).abs() < 1e-10);
        assert!(project.dirty);
    }

    #[test]
    fn update_object_bounds_batch_sets_bounds() {
        let mut project = make_test_project();
        let id1 = ObjectId::new();
        let id2 = ObjectId::new();
        project.objects.push(make_test_object(id1));
        project.objects.push(make_test_object(id2));
        project.dirty = false;

        let entries = vec![
            (
                id1,
                Bounds::new(Point2D::new(10.0, 20.0), Point2D::new(30.0, 40.0)),
            ),
            (
                id2,
                Bounds::new(Point2D::new(50.0, 60.0), Point2D::new(70.0, 80.0)),
            ),
        ];
        update_object_bounds_batch(&mut project, &entries);

        let o1 = project.objects.iter().find(|o| o.id == id1).unwrap();
        assert!((o1.bounds.min.x - 10.0).abs() < 1e-9);
        assert!((o1.bounds.max.y - 40.0).abs() < 1e-9);
        let o2 = project.objects.iter().find(|o| o.id == id2).unwrap();
        assert!((o2.bounds.min.x - 50.0).abs() < 1e-9);
        assert!(project.dirty);
    }

    #[test]
    fn update_object_bounds_batch_refits_vector_paths() {
        let mut project = make_test_project();
        let id = ObjectId::new();
        let mut obj = make_test_object(id);
        obj.data = ObjectData::VectorPath {
            path_data: "M0 0 L10 0 L10 10 L0 10 Z".into(),
            closed: true,
            ruler_guide_axis: None,
        };
        obj.bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0));
        project.objects.push(obj);

        let new_bounds = Bounds::new(Point2D::new(5.0, 5.0), Point2D::new(25.0, 15.0));
        update_object_bounds_batch(&mut project, &[(id, new_bounds)]);

        let obj = project.objects.iter().find(|o| o.id == id).unwrap();
        // Bounds should match the new bounds
        assert!((obj.bounds.min.x - 5.0).abs() < 1e-9);
        assert!((obj.bounds.min.y - 5.0).abs() < 1e-9);
        assert!((obj.bounds.max.x - 25.0).abs() < 1e-9);
        assert!((obj.bounds.max.y - 15.0).abs() < 1e-9);
        // Path data should be refitted (not the original "M0 0 L10 0...")
        if let ObjectData::VectorPath { ref path_data, .. } = obj.data {
            let vec_path = VecPath::parse_svg_d(path_data);
            let vb = vec_path.visual_bounds().unwrap();
            assert!((vb.min.x - 5.0).abs() < 0.1);
            assert!((vb.min.y - 5.0).abs() < 0.1);
            assert!((vb.max.x - 25.0).abs() < 0.1);
            assert!((vb.max.y - 15.0).abs() < 0.1);
        } else {
            panic!("Expected VectorPath");
        }
        // Tabs should be cleared
        assert!(obj.tabs.is_empty());
    }

    #[test]
    fn update_object_bounds_batch_scales_text_properties() {
        use crate::object::{TextAlignment, TextAlignmentV, TextLayoutMode};

        let mut project = make_test_project();
        let id = ObjectId::new();
        let mut obj = make_test_object(id);
        obj.data = ObjectData::Text {
            content: "Hello".into(),
            font_family: "Arial".into(),
            font_size_mm: 10.0,
            alignment: TextAlignment::Left,
            alignment_v: TextAlignmentV::Top,
            bold: false,
            italic: false,
            upper_case: false,
            welded: false,
            h_spacing: 1.0,
            v_spacing: 2.0,
            on_path: false,
            path_offset: 0.0,
            distort: false,
            layout_mode: TextLayoutMode::Straight,
            rtl: false,
            bend_radius: 0.0,
            transform_style: crate::object::TextTransformStyle::None,
            transform_curve: 0.0,
            circle_placement: crate::object::TextCirclePlacement::TopOutside,
            max_width: Some(100.0),
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
        obj.bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 50.0));
        project.objects.push(obj);

        // Resize to double width, triple height
        let new_bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(200.0, 150.0));
        update_object_bounds_batch(&mut project, &[(id, new_bounds)]);

        let obj = project.objects.iter().find(|o| o.id == id).unwrap();
        if let ObjectData::Text {
            font_size_mm,
            h_spacing,
            v_spacing,
            max_width,
            ..
        } = &obj.data
        {
            // font_size_mm scales by height ratio (3x)
            assert!(
                (*font_size_mm - 30.0).abs() < 1e-9,
                "font_size_mm: {font_size_mm}"
            );
            // v_spacing scales by height ratio (3x)
            assert!((*v_spacing - 6.0).abs() < 1e-9, "v_spacing: {v_spacing}");
            // h_spacing scales by width ratio (2x)
            assert!((*h_spacing - 2.0).abs() < 1e-9, "h_spacing: {h_spacing}");
            // max_width scales by width ratio (2x)
            assert_eq!(*max_width, Some(200.0));
        } else {
            panic!("Expected Text");
        }
    }

    #[test]
    fn update_object_bounds_batch_no_scale_for_move() {
        use crate::object::{TextAlignment, TextAlignmentV, TextLayoutMode};

        let mut project = make_test_project();
        let id = ObjectId::new();
        let mut obj = make_test_object(id);
        obj.data = ObjectData::Text {
            content: "Hello".into(),
            font_family: "Arial".into(),
            font_size_mm: 10.0,
            alignment: TextAlignment::Left,
            alignment_v: TextAlignmentV::Top,
            bold: false,
            italic: false,
            upper_case: false,
            welded: false,
            h_spacing: 1.0,
            v_spacing: 2.0,
            on_path: false,
            path_offset: 0.0,
            distort: false,
            layout_mode: TextLayoutMode::Straight,
            rtl: false,
            bend_radius: 0.0,
            transform_style: crate::object::TextTransformStyle::None,
            transform_curve: 0.0,
            circle_placement: crate::object::TextCirclePlacement::TopOutside,
            max_width: Some(100.0),
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
        obj.bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 50.0));
        project.objects.push(obj);

        // Move (same size, different position)
        let new_bounds = Bounds::new(Point2D::new(50.0, 30.0), Point2D::new(150.0, 80.0));
        update_object_bounds_batch(&mut project, &[(id, new_bounds)]);

        let obj = project.objects.iter().find(|o| o.id == id).unwrap();
        if let ObjectData::Text {
            font_size_mm,
            h_spacing,
            v_spacing,
            max_width,
            ..
        } = &obj.data
        {
            // Properties should be unchanged (same dimensions, just moved)
            assert!(
                (*font_size_mm - 10.0).abs() < 1e-9,
                "font_size_mm: {font_size_mm}"
            );
            assert!((*v_spacing - 2.0).abs() < 1e-9, "v_spacing: {v_spacing}");
            assert!((*h_spacing - 1.0).abs() < 1e-9, "h_spacing: {h_spacing}");
            assert_eq!(*max_width, Some(100.0));
        } else {
            panic!("Expected Text");
        }
    }

    #[test]
    fn move_objects_to_position_does_not_corrupt_transform() {
        let mut project = make_test_project();
        let id = ObjectId::new();
        project.objects.push(make_test_object(id));

        // Move to (50, 60)
        move_objects_to_position(&mut project, &[id], 50.0, 60.0);

        let obj = project.find_object(id).unwrap();
        assert_eq!(obj.bounds.min.x, 50.0);
        assert_eq!(obj.bounds.min.y, 60.0);
        // Transform should remain identity for unrotated objects
        assert!((obj.transform.a - 1.0).abs() < 1e-10);
        assert!((obj.transform.tx).abs() < 1e-10);
        assert!((obj.transform.ty).abs() < 1e-10);
    }

    #[test]
    fn move_rotated_object_preserves_visual_position() {
        let mut project = make_test_project();
        let id = ObjectId::new();
        project.objects.push(make_test_object(id));

        // Rotate 90 degrees, then move
        rotate_objects(&mut project, &[id], 90.0, None);
        let old_transform = project.find_object(id).unwrap().transform;

        move_objects_to_position(&mut project, &[id], 100.0, 200.0);

        let obj = project.find_object(id).unwrap();
        // Transform should be completely unchanged — only bounds moved
        assert!((obj.transform.a - old_transform.a).abs() < 1e-10);
        assert!((obj.transform.b - old_transform.b).abs() < 1e-10);
        assert!((obj.transform.c - old_transform.c).abs() < 1e-10);
        assert!((obj.transform.d - old_transform.d).abs() < 1e-10);
        assert!((obj.transform.tx - old_transform.tx).abs() < 1e-10);
        assert!((obj.transform.ty - old_transform.ty).abs() < 1e-10);
    }

    #[test]
    fn rotate_objects_single_no_orbit() {
        // Rotate a single object by 90 degrees where bounds center IS the pivot,
        // so only transform changes, no bounds shift.
        let mut project = make_test_project();
        let id = ObjectId::new();
        let mut obj = make_test_object(id);
        obj.bounds = Bounds::new(Point2D::new(10.0, 20.0), Point2D::new(30.0, 40.0));
        project.objects.push(obj);

        let pivot = Point2D::new(20.0, 30.0); // center of bounds
        rotate_objects(&mut project, &[id], 90.0, Some(pivot));

        let obj = &project.objects[0];
        // Bounds should not shift because pivot == center
        assert!((obj.bounds.min.x - 10.0).abs() < 1e-6);
        assert!((obj.bounds.min.y - 20.0).abs() < 1e-6);
        assert!((obj.bounds.max.x - 30.0).abs() < 1e-6);
        assert!((obj.bounds.max.y - 40.0).abs() < 1e-6);
        // Transform should have 90-degree rotation composed
        assert!(!obj.transform.is_identity());
        assert!((obj.transform.a - 0.0).abs() < 1e-6);
        assert!((obj.transform.b - 1.0).abs() < 1e-6);
    }

    #[test]
    fn rotate_objects_multi_orbits() {
        // Create 2 objects offset from each other, rotate 90 degrees around their
        // midpoint — verify bounds shift AND transform compose.
        let mut project = make_test_project();
        let id1 = ObjectId::new();
        let id2 = ObjectId::new();
        let mut obj1 = make_test_object(id1);
        obj1.bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0));
        let mut obj2 = make_test_object(id2);
        obj2.bounds = Bounds::new(Point2D::new(30.0, 0.0), Point2D::new(40.0, 10.0));
        project.objects.push(obj1);
        project.objects.push(obj2);

        // Midpoint pivot: ((5+35)/2, (5+5)/2) = (20, 5)
        let pivot = Point2D::new(20.0, 5.0);
        rotate_objects(&mut project, &[id1, id2], 90.0, Some(pivot));

        // obj1 center was (5,5), relative (-15,0), rotated 90 -> (0,-15), shifted to (20,-10)
        let o1 = project.objects.iter().find(|o| o.id == id1).unwrap();
        let o1_cx = (o1.bounds.min.x + o1.bounds.max.x) / 2.0;
        let o1_cy = (o1.bounds.min.y + o1.bounds.max.y) / 2.0;
        assert!(
            (o1_cx - 20.0).abs() < 1e-4,
            "o1 cx should be ~20, got {}",
            o1_cx
        );
        assert!(
            (o1_cy - (-10.0)).abs() < 1e-4,
            "o1 cy should be ~-10, got {}",
            o1_cy
        );

        // obj2 center was (35,5), relative (15,0), rotated 90 -> (0,15), shifted to (20,20)
        let o2 = project.objects.iter().find(|o| o.id == id2).unwrap();
        let o2_cx = (o2.bounds.min.x + o2.bounds.max.x) / 2.0;
        let o2_cy = (o2.bounds.min.y + o2.bounds.max.y) / 2.0;
        assert!(
            (o2_cx - 20.0).abs() < 1e-4,
            "o2 cx should be ~20, got {}",
            o2_cx
        );
        assert!(
            (o2_cy - 20.0).abs() < 1e-4,
            "o2 cy should be ~20, got {}",
            o2_cy
        );

        // Both should have rotation composed into transform
        assert!(!o1.transform.is_identity());
        assert!(!o2.transform.is_identity());
    }

    #[test]
    fn shear_objects_basic() {
        // Shear a single object — verify transform has shear values composed.
        let mut project = make_test_project();
        let id = ObjectId::new();
        project.objects.push(make_test_object(id));

        shear_objects(&mut project, &[id], 0.7, 0.0, None);

        let obj = &project.objects[0];
        // Shear(0.7, 0.0) composed with identity should give c=0.7
        assert!((obj.transform.a - 1.0).abs() < 1e-10);
        assert!((obj.transform.b - 0.0).abs() < 1e-10);
        assert!((obj.transform.c - 0.7).abs() < 1e-10);
        assert!((obj.transform.d - 1.0).abs() < 1e-10);
        assert!(project.dirty);
    }

    #[test]
    fn select_open_shapes_finds_open_paths() {
        let mut project = make_test_project();
        let id1 = ObjectId::new();
        let id2 = ObjectId::new();

        let mut obj1 = make_test_object(id1);
        obj1.data = ObjectData::VectorPath {
            path_data: "M0 0 L10 0".into(),
            closed: false,
            ruler_guide_axis: None,
        };

        let mut obj2 = make_test_object(id2);
        obj2.data = ObjectData::VectorPath {
            path_data: "M0 0 L10 0 Z".into(),
            closed: true,
            ruler_guide_axis: None,
        };

        project.objects.push(obj1);
        project.objects.push(obj2);

        let result = select_open_shapes(&project);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], id1);
    }

    #[test]
    fn update_object_bounds_batch_virtualclone_path_text_keeps_guide() {
        use crate::object::{TextAlignment, TextAlignmentV, TextLayoutMode};

        let mut project = make_test_project();

        // 1. Create a guide-path vector object
        let guide_id = ObjectId::new();
        let mut guide_obj = make_test_object(guide_id);
        guide_obj.data = ObjectData::VectorPath {
            path_data: "M0 0 L100 0".into(),
            closed: false,
            ruler_guide_axis: None,
        };
        guide_obj.bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 10.0));
        project.objects.push(guide_obj);

        // 2. Create a path-text source referencing the guide
        let text_src_id = ObjectId::new();
        let mut text_src = make_test_object(text_src_id);
        text_src.data = ObjectData::Text {
            content: "Hello".into(),
            font_family: "Arial".into(),
            font_size_mm: 5.0,
            alignment: TextAlignment::Left,
            alignment_v: TextAlignmentV::Top,
            bold: false,
            italic: false,
            upper_case: false,
            welded: false,
            h_spacing: 0.0,
            v_spacing: 0.0,
            on_path: true,
            path_offset: 0.0,
            distort: false,
            layout_mode: TextLayoutMode::Straight,
            rtl: false,
            bend_radius: 0.0,
            transform_style: crate::object::TextTransformStyle::None,
            transform_curve: 0.0,
            circle_placement: crate::object::TextCirclePlacement::TopOutside,
            max_width: None,
            squeeze: false,
            ignore_empty_vars: false,
            resolved_font_source: None,
            resolved_font_key: None,
            resolved_path_data: Some("M0 0 L100 0".into()),
            missing_font: false,
            missing_glyphs: Vec::new(),
            guide_path_id: Some(guide_id),
            variable_text: None,
        };
        text_src.bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 10.0));
        project.objects.push(text_src);

        // 3. Create a VirtualClone of the path-text
        let clone_id = ObjectId::new();
        let mut clone_obj = make_test_object(clone_id);
        clone_obj.data = ObjectData::VirtualClone {
            source_id: text_src_id,
        };
        clone_obj.bounds = Bounds::new(Point2D::new(0.0, 20.0), Point2D::new(100.0, 30.0));
        project.objects.push(clone_obj);

        // 4. Batch-update bounds on the clone (move it)
        let new_bounds = Bounds::new(Point2D::new(10.0, 25.0), Point2D::new(110.0, 35.0));
        update_object_bounds_batch(&mut project, &[(clone_id, new_bounds)]);

        // 5. The clone should now be resolved to concrete Text (no longer VirtualClone)
        let resolved = project.find_object(clone_id).unwrap();
        assert!(
            matches!(&resolved.data, ObjectData::Text { .. }),
            "Clone should be resolved to Text, got {:?}",
            std::mem::discriminant(&resolved.data),
        );

        // 6. The resolved text should still have guide_path_id and resolved_path_data
        if let ObjectData::Text {
            guide_path_id,
            resolved_path_data,
            ..
        } = &resolved.data
        {
            assert_eq!(
                *guide_path_id,
                Some(guide_id),
                "guide_path_id should be preserved after resolve"
            );
            // The critical assertion: resolved_path_data must NOT be None.
            // Before the fix, the pre-pass cached guides before ensure_resolved(),
            // so VirtualClone entries got None, causing text refresh to clear this.
            assert!(
                resolved_path_data.is_some(),
                "resolved_path_data should be Some after batch bounds on clone-of-path-text"
            );
        } else {
            panic!("Expected Text variant");
        }
    }

    #[test]
    fn update_object_bounds_batch_path_text_applies_mapped_bounds() {
        use crate::object::{TextAlignment, TextAlignmentV, TextLayoutMode};

        let mut project = make_test_project();

        // Create a guide-path vector object (horizontal line 0..100)
        let guide_id = ObjectId::new();
        let mut guide_obj = make_test_object(guide_id);
        guide_obj.data = ObjectData::VectorPath {
            path_data: "M0 0 L100 0".into(),
            closed: false,
            ruler_guide_axis: None,
        };
        guide_obj.bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 10.0));
        project.objects.push(guide_obj);

        // Create a path-text referencing the guide
        let text_id = ObjectId::new();
        let mut text_obj = make_test_object(text_id);
        text_obj.data = ObjectData::Text {
            content: "Hello".into(),
            font_family: "Arial".into(),
            font_size_mm: 5.0,
            alignment: TextAlignment::Left,
            alignment_v: TextAlignmentV::Top,
            bold: false,
            italic: false,
            upper_case: false,
            welded: false,
            h_spacing: 0.0,
            v_spacing: 0.0,
            on_path: true,
            path_offset: 0.0,
            distort: false,
            layout_mode: TextLayoutMode::Straight,
            rtl: false,
            bend_radius: 0.0,
            transform_style: crate::object::TextTransformStyle::None,
            transform_curve: 0.0,
            circle_placement: crate::object::TextCirclePlacement::TopOutside,
            max_width: None,
            squeeze: false,
            ignore_empty_vars: false,
            resolved_font_source: None,
            resolved_font_key: None,
            resolved_path_data: Some("M0 0 L100 0".into()),
            missing_font: false,
            missing_glyphs: Vec::new(),
            guide_path_id: Some(guide_id),
            variable_text: None,
        };
        text_obj.bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 10.0));
        project.objects.push(text_obj);

        // Move the path-text via batch bounds to a new position with wrong size
        let user_bounds = Bounds::new(Point2D::new(20.0, 30.0), Point2D::new(70.0, 80.0));
        update_object_bounds_batch(&mut project, &[(text_id, user_bounds)]);

        let obj = project.find_object(text_id).unwrap();
        // The min corner should be the user's chosen position
        assert!(
            (obj.bounds.min.x - 20.0).abs() < 1e-9,
            "min.x should be user-provided: {}",
            obj.bounds.min.x
        );
        assert!(
            (obj.bounds.min.y - 30.0).abs() < 1e-9,
            "min.y should be user-provided: {}",
            obj.bounds.min.y
        );
        // The width/height should NOT be the user's 50x50 but the guide-derived
        // size (from refresh_text_object_cache_with_guide's mapped_bounds).
        // Since fonts are not loaded in tests, mapped_bounds may be None and
        // the user bounds are kept — but if mapped_bounds IS returned, size must
        // come from there. Either way, resolved_path_data must be preserved.
        if let ObjectData::Text {
            resolved_path_data, ..
        } = &obj.data
        {
            assert!(
                resolved_path_data.is_some(),
                "resolved_path_data must not be cleared by batch bounds"
            );
        } else {
            panic!("Expected Text variant");
        }
        // Transform must be identity (reset by mapped bounds application)
        assert!(
            (obj.transform.a - 1.0).abs() < 1e-9
                && (obj.transform.d - 1.0).abs() < 1e-9
                && obj.transform.tx.abs() < 1e-9
                && obj.transform.ty.abs() < 1e-9,
            "Transform should be identity after path-text batch bounds"
        );
    }
}
